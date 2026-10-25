//! Launcher self-update: start-up check, the "update available" window,
//! download/verify/install progress, and the Settings → Updates section.
//!
//! All network and file work runs on a worker thread; the UI only reads
//! messages. A failed check never blocks the launcher.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use egui::{Color32, CornerRadius, RichText, Stroke, Vec2};
use kestrel_core::config::LauncherConfig;
use kestrel_core::paths::Layout;
use kestrel_core::update::{
    self, CheckOutcome, DownloadProgress, Release, SelfCheckValidator, UpdateError, UpdateSource,
    UreqTransport, install,
};
use semver::Version;

use crate::app::{ACCENT, CARD, ERR, MUTED, OK, WARN, card};
use crate::embedded;

/// How long the old launcher waits for the new one to report a healthy start.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Downloading,
    Verifying,
    Installing,
    Restarting,
}

enum Msg {
    Checked {
        manual: bool,
        result: Result<CheckOutcome, String>,
    },
    Progress(DownloadProgress),
    Stage(Stage),
    Finished(Result<Finish, String>),
}

enum Finish {
    /// The new launcher is running; this process should exit.
    Restarted,
    /// Could not replace the exe in place; a verified copy was kept here.
    Fallback { file: PathBuf, reason: String },
}

enum State {
    Idle,
    NotConfigured,
    Checking,
    UpToDate {
        latest: Version,
    },
    CheckFailed(String),
    Available,
    Working {
        stage: Stage,
        progress: DownloadProgress,
    },
    Failed(String),
    Fallback {
        file: PathBuf,
        reason: String,
    },
}

/// What the app must do after a frame of updater UI.
#[derive(Default)]
pub struct Actions {
    pub save_config: bool,
    pub exit: bool,
}

pub struct Updater {
    layout: Layout,
    source: Result<UpdateSource, String>,
    current: Version,
    state: State,
    release: Option<Release>,
    show_prompt: bool,
    cancel: Arc<AtomicBool>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    last_check: Option<Instant>,
    /// Set only after the new launcher confirmed a healthy start.
    restart_confirmed: bool,
    /// Show optional updates automatically ("Automatically check" setting).
    /// Required updates are always checked for and shown.
    prompt_optional: bool,
}

impl Updater {
    pub fn new(layout: Layout, cfg: &LauncherConfig, ctx: &egui::Context) -> Updater {
        let source =
            UpdateSource::from_build(embedded::UPDATE_MANIFEST_URL, embedded::UPDATE_PUBLIC_KEY)
                .map_err(|e| e.to_string());
        let (tx, rx) = channel();
        let mut u = Updater {
            layout,
            state: match &source {
                Ok(_) => State::Idle,
                Err(_) => State::NotConfigured,
            },
            source,
            current: update::current_version(),
            release: None,
            show_prompt: false,
            cancel: Arc::new(AtomicBool::new(false)),
            tx,
            rx,
            last_check: None,
            restart_confirmed: false,
            prompt_optional: cfg.update_auto_check,
        };
        // Always check at start-up: a required update must be found even when
        // optional prompts are turned off (they are then simply not shown).
        if u.source.is_ok() {
            u.check(false, ctx);
        }
        #[cfg(debug_assertions)]
        u.debug_demo();
        u
    }

    /// Debug builds only: `KESTREL_UPDATE_DEMO=prompt|download|failed|fallback`
    /// shows the update window with a fake release, for UI review. Never
    /// downloads or installs anything. Compiled out of release builds.
    #[cfg(debug_assertions)]
    fn debug_demo(&mut self) {
        let Ok(mode) = std::env::var("KESTREL_UPDATE_DEMO") else {
            return;
        };
        self.release = Some(Release {
            version: Version::new(self.current.major, self.current.minor + 1, 0),
            minimum_supported: None,
            notes: "Automatic launcher updates.\n- New kill effects\n- Faster first launch\n- Bug fixes".into(),
            published: String::new(),
            asset: kestrel_core::update::manifest::Asset {
                url: "https://github.com/demo/kestrel/releases/download/v0.2.0/Kestrel.exe".into(),
                sha256: "0".repeat(64),
                size: 22_642_688,
            },
        });
        self.show_prompt = true;
        self.state = match mode.as_str() {
            "download" => State::Working {
                stage: Stage::Downloading,
                progress: DownloadProgress {
                    downloaded: 9_437_184,
                    total: 22_642_688,
                    speed: 4_404_019.0,
                },
            },
            "failed" => {
                State::Failed("downloaded file failed verification: checksum mismatch".into())
            }
            "fallback" => State::Fallback {
                file: PathBuf::from(r"C:\Users\you\AppData\Local\Kestrel\app\Kestrel.exe"),
                reason:
                    r"Windows does not allow Kestrel to change files in C:\Program Files\Kestrel."
                        .into(),
            },
            _ => State::Available,
        };
    }

    pub fn check(&mut self, manual: bool, ctx: &egui::Context) {
        let Ok(source) = self.source.clone() else {
            return;
        };
        if matches!(self.state, State::Checking | State::Working { .. }) {
            return;
        }
        self.state = State::Checking;
        self.last_check = Some(Instant::now());
        let (tx, ctx, current) = (self.tx.clone(), ctx.clone(), self.current.clone());
        std::thread::Builder::new()
            .name("kestrel-update-check".into())
            .spawn(move || {
                let result = update::check(transport().as_ref(), &source, &current).map_err(|e| {
                    log::warn!("update check failed: {e}");
                    e.to_string()
                });
                if let Ok(CheckOutcome::UpToDate { latest }) = &result {
                    log::info!("launcher is up to date (latest published: {latest})");
                }
                let _ = tx.send(Msg::Checked { manual, result });
                ctx.request_repaint();
            })
            .ok();
    }

    fn start_update(&mut self, ctx: &egui::Context) {
        let (Ok(source), Some(release)) = (self.source.clone(), self.release.clone()) else {
            return;
        };
        self.cancel = Arc::new(AtomicBool::new(false));
        self.state = State::Working {
            stage: Stage::Downloading,
            progress: DownloadProgress {
                total: release.asset.size,
                ..Default::default()
            },
        };
        let (tx, ctx, layout, cancel) = (
            self.tx.clone(),
            ctx.clone(),
            self.layout.clone(),
            self.cancel.clone(),
        );
        let current = self.current.clone();
        std::thread::Builder::new()
            .name("kestrel-update".into())
            .spawn(move || {
                let r = std::env::current_exe()
                    .map_err(|e| format!("cannot locate Kestrel.exe: {e}"))
                    .and_then(|exe| {
                        let t = transport();
                        let env = UpdateEnv {
                            transport: t.as_ref(),
                            validator: &SelfCheckValidator,
                            current_exe: &exe,
                            health_timeout: HEALTH_TIMEOUT,
                        };
                        run_update(
                            &env, &source, &release, &layout, &current, &cancel, &tx, &ctx,
                        )
                    });
                if let Err(e) = &r {
                    log::error!("update to {} failed: {e}", release.version);
                }
                let _ = tx.send(Msg::Finished(r));
                ctx.request_repaint();
            })
            .ok();
    }

    /// The version Minecraft can't be started without, if this launcher is
    /// older than a signed release's `minimum_supported` (seen in this
    /// session or remembered from an earlier one). Builds without an update
    /// source never block.
    pub fn required_version(&self, cfg: &LauncherConfig) -> Option<Version> {
        if self.source.is_err() {
            return None;
        }
        update::required_minimum(
            &self.current,
            self.release.as_ref(),
            &cfg.update_required_minimum,
        )
    }

    /// Show the update window (Home page "Update now"); checks first when no
    /// release is known yet, e.g. after starting offline.
    pub fn open_prompt(&mut self, ctx: &egui::Context) {
        if self.release.is_some() {
            if !matches!(self.state, State::Working { .. })
                && !matches!(self.state, State::Failed(_) | State::Fallback { .. })
            {
                self.state = State::Available;
            }
            self.show_prompt = true;
        } else {
            self.check(true, ctx);
        }
    }

    /// Whether a check is running (for the Home page).
    pub fn is_checking(&self) -> bool {
        matches!(self.state, State::Checking)
    }

    /// Returns true when `cfg` changed and must be saved.
    fn poll(&mut self, cfg: &mut LauncherConfig) -> bool {
        let mut save = false;
        let mut remember = |cfg: &mut LauncherConfig, value: String| {
            if cfg.update_required_minimum != value {
                log::info!(
                    "required launcher version: {}",
                    if value.is_empty() { "none" } else { &value }
                );
                cfg.update_required_minimum = value;
                save = true;
            }
        };
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Checked { manual, result } => match result {
                    Ok(CheckOutcome::UpToDate { latest }) => {
                        self.release = None;
                        self.state = State::UpToDate { latest };
                        remember(
                            cfg,
                            update::remembered_minimum_after_check(&self.current, None),
                        );
                    }
                    Ok(CheckOutcome::Available(r)) => {
                        log::info!(
                            "launcher update available: {} -> {}",
                            self.current,
                            r.version
                        );
                        let required = r.is_required_for(&self.current);
                        self.show_prompt = update::should_prompt(
                            &r,
                            &self.current,
                            &cfg.update_dismissed_version,
                            manual,
                        ) && (manual || required || self.prompt_optional);
                        remember(
                            cfg,
                            update::remembered_minimum_after_check(&self.current, Some(&r)),
                        );
                        self.release = Some(*r);
                        self.state = State::Available;
                    }
                    Err(e) => self.state = State::CheckFailed(e),
                },
                Msg::Progress(p) => {
                    if let State::Working { progress, .. } = &mut self.state {
                        *progress = p;
                    }
                }
                Msg::Stage(s) => {
                    if let State::Working { stage, .. } = &mut self.state {
                        *stage = s;
                    }
                }
                Msg::Finished(r) => {
                    if matches!(r, Ok(Finish::Restarted)) {
                        self.restart_confirmed = true;
                    }
                    self.state = match r {
                        Ok(Finish::Restarted) => State::Working {
                            stage: Stage::Restarting,
                            progress: DownloadProgress::default(),
                        },
                        Ok(Finish::Fallback { file, reason }) => State::Fallback { file, reason },
                        Err(e) => State::Failed(e),
                    };
                    self.show_prompt = true;
                }
            }
        }
        save
    }

    /// Update window (modal). Call every frame.
    pub fn prompt_ui(
        &mut self,
        ctx: &egui::Context,
        cfg: &mut LauncherConfig,
        launcher_busy: bool,
    ) -> Actions {
        let mut actions = Actions {
            save_config: self.poll(cfg),
            ..Default::default()
        };
        // Only close once the new version has reported a healthy start; until
        // then this process must stay alive to roll back if it crashes.
        if self.restart_confirmed {
            actions.exit = true;
        }
        if matches!(self.state, State::Checking | State::Working { .. }) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if !self.show_prompt {
            return actions;
        }
        let Some(release) = self.release.clone() else {
            self.show_prompt = false;
            return actions;
        };
        let required = release.is_required_for(&self.current);
        let mut later = false;
        let mut update_now = false;
        let mut close = false;
        let mut start_fallback: Option<PathBuf> = None;

        let modal = egui::Modal::new(egui::Id::unique("kestrel-update"))
            .backdrop_color(Color32::from_black_alpha(160))
            .frame(
                egui::Frame::NONE
                    .fill(CARD)
                    .stroke(Stroke::new(1.0, ACCENT))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(20.0),
            )
            .show(ctx, |ui| {
                ui.set_width(460.0);
                let title = if required { "A required Kestrel update is available" } else { "A Kestrel update is available" };
                ui.label(RichText::new(title).size(20.0).strong().color(ACCENT));
                ui.add_space(8.0);
                egui::Grid::new("upd-versions").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    ui.label(RichText::new("Current version").color(MUTED));
                    ui.label(self.current.to_string());
                    ui.end_row();
                    ui.label(RichText::new("New version").color(MUTED));
                    ui.label(RichText::new(release.version.to_string()).strong());
                    ui.end_row();
                    ui.label(RichText::new("Download").color(MUTED));
                    ui.label(format_size(release.asset.size));
                    ui.end_row();
                });
                if !release.notes.trim().is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("What's new").strong());
                    egui::Frame::NONE
                        .fill(Color32::from_rgb(0x18, 0x1A, 0x1E))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical().max_height(150.0).auto_shrink([false, true]).show(ui, |ui| {
                                ui.label(&release.notes);
                            });
                        });
                }
                ui.add_space(12.0);
                match &self.state {
                    State::Available | State::UpToDate { .. } | State::CheckFailed(_) | State::Idle | State::Checking | State::NotConfigured => {
                        if launcher_busy {
                            ui.label(
                                RichText::new("Close Minecraft and let Kestrel finish what it is doing before updating.")
                                    .color(WARN),
                            );
                        }
                        ui.horizontal(|ui| {
                            let b = egui::Button::new(RichText::new("Update Now").strong().color(Color32::BLACK))
                                .fill(ACCENT)
                                .min_size(Vec2::new(140.0, 34.0));
                            if ui.add_enabled(!launcher_busy, b).clicked() {
                                update_now = true;
                            }
                            if ui.add(egui::Button::new("Later").min_size(Vec2::new(100.0, 34.0))).clicked() {
                                later = true;
                            }
                        });
                        if required {
                            ui.label(
                                RichText::new("This update is required: Minecraft can't be started from this version until it is installed.")
                                    .color(WARN),
                            );
                        }
                        ui.label(
                            RichText::new("Your settings, game files and cosmetics are kept. Kestrel restarts when the update is done.")
                                .small()
                                .color(MUTED),
                        );
                    }
                    State::Working { stage, progress } => {
                        let (label, frac) = match stage {
                            Stage::Downloading => {
                                let f = if progress.total > 0 { progress.downloaded as f32 / progress.total as f32 } else { 0.0 };
                                (format!("Downloading… {:.0}%", f * 100.0), Some(f))
                            }
                            Stage::Verifying => ("Verifying signature, checksum and running the new version's self-test…".into(), None),
                            Stage::Installing => ("Installing…".into(), None),
                            Stage::Restarting => ("Restarting Kestrel…".into(), None),
                        };
                        ui.label(label);
                        let bar = egui::ProgressBar::new(frac.unwrap_or(0.0)).desired_width(460.0);
                        ui.add(if frac.is_some() { bar } else { bar.animate(true) });
                        if *stage == Stage::Downloading {
                            ui.label(
                                RichText::new(format!(
                                    "{} of {}  ·  {}/s",
                                    format_size(progress.downloaded),
                                    format_size(progress.total),
                                    format_size(progress.speed as u64)
                                ))
                                .color(MUTED),
                            );
                            if ui.button("Cancel").clicked() {
                                self.cancel.store(true, Ordering::Relaxed);
                            }
                        }
                    }
                    State::Failed(e) => {
                        ui.label(RichText::new("The update could not be installed.").color(ERR).strong());
                        ui.label(RichText::new(e).color(ERR));
                        ui.label(RichText::new("Your current version is still installed and working.").color(MUTED));
                        ui.horizontal(|ui| {
                            if ui.add_enabled(!launcher_busy, egui::Button::new("Retry")).clicked() {
                                update_now = true;
                            }
                            if ui.button("Close").clicked() {
                                close = true;
                            }
                        });
                    }
                    State::Fallback { file, reason } => {
                        ui.label(RichText::new("Kestrel can't update itself in its current folder.").color(WARN).strong());
                        ui.label(reason);
                        ui.label(format!(
                            "The verified new version was installed to:\n{}\n\nStart it from there (it keeps updating itself), or move Kestrel.exe to a folder you own, such as your Desktop or Documents.",
                            file.display()
                        ));
                        ui.horizontal(|ui| {
                            if ui.button("Start new version").clicked() {
                                start_fallback = Some(file.clone());
                            }
                            if ui.button("Open folder").clicked() {
                                crate::app::open_path(file.parent().unwrap_or(Path::new(".")));
                            }
                            if ui.button("Close").clicked() {
                                close = true;
                            }
                        });
                    }
                }
            });

        let dismissable = matches!(
            self.state,
            State::Available | State::Failed(_) | State::Fallback { .. }
        );
        if modal.should_close() && dismissable {
            if matches!(self.state, State::Available) {
                later = true;
            } else {
                close = true;
            }
        }
        if later {
            if !required {
                cfg.update_dismissed_version = release.version.to_string();
                actions.save_config = true;
            }
            log::info!("update {} postponed", release.version);
            self.show_prompt = false;
        }
        if close {
            self.show_prompt = false;
            self.state = State::Available;
        }
        if update_now && !launcher_busy {
            self.start_update(ctx);
        }
        if let Some(file) = start_fallback {
            match spawn_new(&file, None) {
                Ok(_) => actions.exit = true,
                Err(e) => {
                    self.state = State::Failed(format!("could not start {}: {e}", file.display()))
                }
            }
        }
        actions
    }

    /// The "Updates" card on the Settings page.
    pub fn settings_ui(
        &mut self,
        ui: &mut egui::Ui,
        cfg: &mut LauncherConfig,
        launcher_busy: bool,
    ) -> bool {
        let mut dirty = false;
        let ctx = ui.ctx().clone();
        card(ui, |ui| {
            ui.label(RichText::new("Updates").strong());
            egui::Grid::new("upd-settings")
                .num_columns(2)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Installed version").color(MUTED));
                    ui.label(self.current.to_string());
                    ui.end_row();
                    ui.label(RichText::new("Update source").color(MUTED));
                    match &self.source {
                        Ok(s) => ui.label(s.describe()),
                        Err(_) => {
                            ui.label(RichText::new("Not configured in this build").color(WARN))
                        }
                    };
                    ui.end_row();
                });
            if self.source.is_err() {
                ui.label(
                    RichText::new("This copy of Kestrel was built without an update source, so it cannot update itself. See documentation/UPDATES.md.")
                        .small()
                        .color(MUTED),
                );
                return;
            }
            if ui
                .checkbox(
                    &mut cfg.update_auto_check,
                    "Show new versions automatically when Kestrel starts",
                )
                .on_hover_text("Required updates are always checked for and shown, because Minecraft can't be started without them.")
                .changed()
            {
                self.prompt_optional = cfg.update_auto_check;
                dirty = true;
            }
            ui.horizontal(|ui| {
                let busy = matches!(self.state, State::Checking | State::Working { .. });
                if ui
                    .add_enabled(!busy, egui::Button::new("Check for updates"))
                    .clicked()
                {
                    self.check(true, &ctx);
                }
                match &self.state {
                    State::Checking => {
                        ui.spinner();
                        ui.label("Checking…");
                    }
                    State::UpToDate { latest } => {
                        ui.label(
                            RichText::new(format!("Kestrel is up to date (latest: {latest})."))
                                .color(OK),
                        );
                    }
                    State::CheckFailed(e) => {
                        ui.label(RichText::new(format!("Check failed: {e}")).color(ERR));
                    }
                    State::Available => {
                        if let Some(r) = &self.release {
                            ui.label(
                                RichText::new(format!("Version {} is available.", r.version))
                                    .color(ACCENT),
                            );
                            if ui.button("Show update").clicked() {
                                self.show_prompt = true;
                            }
                        }
                    }
                    State::Working { .. } => {
                        ui.spinner();
                        ui.label("Updating…");
                        if ui.button("Show progress").clicked() {
                            self.show_prompt = true;
                        }
                    }
                    State::Failed(e) => {
                        ui.label(RichText::new(format!("Last update failed: {e}")).color(ERR));
                        if ui
                            .add_enabled(!launcher_busy, egui::Button::new("Retry update"))
                            .clicked()
                        {
                            self.show_prompt = true;
                            self.start_update(&ctx);
                        }
                    }
                    State::Fallback { .. } => {
                        if ui.button("Show downloaded update").clicked() {
                            self.show_prompt = true;
                        }
                    }
                    State::Idle | State::NotConfigured => {}
                }
            });
            if let Some(t) = self.last_check {
                ui.label(
                    RichText::new(format!("Last checked {} s ago.", t.elapsed().as_secs()))
                        .small()
                        .color(MUTED),
                );
            }
        });
        dirty
    }
}

/// Transport for update checks and downloads: HTTPS in every shipped build.
fn transport() -> Box<dyn update::Transport> {
    #[cfg(feature = "e2e-update-test")]
    if let Some(dir) = std::env::var_os("KESTREL_E2E_UPDATE_DIR") {
        log::warn!(
            "TEST BUILD: update files are read from {} instead of the network",
            std::path::Path::new(&dir).display()
        );
        return Box::new(e2e::LocalDirTransport(PathBuf::from(dir)));
    }
    Box::new(UreqTransport::default())
}

/// Whether this build contains the end-to-end update test hooks.
pub const TEST_HOOKS: bool = cfg!(feature = "e2e-update-test");

/// Test builds only: simulate a new version that crashes on its first start
/// after an update, so the old launcher's rollback can be exercised.
pub fn e2e_startup_hook(args: &[String]) {
    #[cfg(feature = "e2e-update-test")]
    if args.iter().any(|a| a == "--post-update")
        && std::env::var_os("KESTREL_E2E_FAIL_AFTER_UPDATE").is_some()
    {
        log::error!("TEST BUILD: simulating a crash after update");
        std::process::exit(3);
    }
    let _ = args;
}

/// End-to-end test transport (compiled only with `e2e-update-test`).
#[cfg(feature = "e2e-update-test")]
mod e2e {
    use super::*;
    use kestrel_core::update::fetch::HttpResponse;

    /// Serves `<dir>/<last URL path segment>`; anything else is "offline".
    /// The URL itself is still validated (HTTPS, allowed host) by the caller,
    /// and the content still has to pass signature and hash checks.
    pub struct LocalDirTransport(pub PathBuf);

    impl update::Transport for LocalDirTransport {
        fn get(&self, url: &str) -> Result<HttpResponse, String> {
            let name = url
                .rsplit('/')
                .next()
                .filter(|n| !n.is_empty() && !n.contains(['\\', ':']) && *n != "..")
                .ok_or("bad test URL")?;
            let path = self.0.join(name);
            let bytes = std::fs::read(&path)
                .map_err(|e| format!("offline (test dir {}): {e}", path.display()))?;
            Ok(HttpResponse {
                status: 200,
                location: None,
                content_length: Some(bytes.len() as u64),
                body: Box::new(std::io::Cursor::new(bytes)),
            })
        }
    }
}

/// Everything `run_update` touches outside its arguments (injectable for tests).
struct UpdateEnv<'a> {
    transport: &'a dyn update::Transport,
    validator: &'a dyn update::Validator,
    current_exe: &'a Path,
    health_timeout: Duration,
}

/// Download → verify → self-test → swap → restart → wait for health.
#[allow(clippy::too_many_arguments)]
fn run_update(
    env: &UpdateEnv<'_>,
    source: &UpdateSource,
    release: &Release,
    layout: &Layout,
    current: &Version,
    cancel: &AtomicBool,
    tx: &Sender<Msg>,
    ctx: &egui::Context,
) -> Result<Finish, String> {
    let file = update::prepare(
        env.transport,
        source,
        release,
        layout,
        env.validator,
        cancel,
        |p| {
            let _ = tx.send(Msg::Progress(p));
            if p.downloaded == p.total {
                let _ = tx.send(Msg::Stage(Stage::Verifying));
            }
            ctx.request_repaint();
        },
    )
    .map_err(|e| e.to_string())?;
    log::info!(
        "update {} downloaded and verified: {}",
        release.version,
        file.display()
    );

    let _ = tx.send(Msg::Stage(Stage::Installing));
    ctx.request_repaint();
    let current_exe = env.current_exe.to_path_buf();
    let backup = match install::swap(&current_exe, &file) {
        Ok(b) => b,
        Err(UpdateError::NotWritable(dir)) => {
            // Keep a stable, self-updatable copy in Kestrel's own data folder
            // (the downloads folder is cleaned on every update).
            let target = fallback_exe(layout);
            install_fallback_copy(&file, &target).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&file);
            log::warn!(
                "install folder {} is not writable; installed {} to {}",
                dir.display(),
                release.version,
                target.display()
            );
            return Ok(Finish::Fallback {
                file: target,
                reason: format!(
                    "Windows does not allow Kestrel to change files in {}.",
                    dir.display()
                ),
            });
        }
        Err(e) => return Err(e.to_string()),
    };
    log::info!(
        "installed {} over {} (backup {})",
        release.version,
        current,
        backup.display()
    );

    // Start the new version and wait for it to report a healthy start.
    let health = update::health_file(layout, &release.version);
    let _ = std::fs::remove_file(&health);
    let _ = tx.send(Msg::Stage(Stage::Restarting));
    ctx.request_repaint();
    let mut child = match spawn_new(&current_exe, Some((&current.to_string(), &health))) {
        Ok(c) => c,
        Err(e) => {
            let restored = install::rollback(&current_exe, &backup);
            return Err(format!(
                "could not start the new version ({e}); {}",
                rollback_note(&restored)
            ));
        }
    };
    let deadline = Instant::now() + env.health_timeout;
    loop {
        if health.exists() {
            let _ = std::fs::remove_file(&health);
            log::info!("new version {} started successfully", release.version);
            return Ok(Finish::Restarted);
        }
        if let Ok(Some(status)) = child.try_wait() {
            log::error!("new version exited during start-up ({status}); rolling back");
            let restored = install::rollback(&current_exe, &backup);
            return Err(format!(
                "the new version closed during start-up ({status}); {}",
                rollback_note(&restored)
            ));
        }
        if Instant::now() > deadline {
            // Still running but slow to open (first start, antivirus scan): keep it.
            log::warn!(
                "new version did not report health within {:?}; assuming it is starting",
                env.health_timeout
            );
            return Ok(Finish::Restarted);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Where an update goes when the launcher's own folder is not writable.
fn fallback_exe(layout: &Layout) -> PathBuf {
    layout.root().join("app").join("Kestrel.exe")
}

/// Put a verified exe at `target`, replacing an earlier fallback copy safely.
fn install_fallback_copy(verified: &Path, target: &Path) -> Result<(), UpdateError> {
    if target.is_file() {
        // Same swap as a normal update (the old copy may be running).
        return install::swap(target, verified).map(|_| ());
    }
    let dir = target
        .parent()
        .ok_or_else(|| UpdateError::Install("bad fallback path".into()))?;
    std::fs::create_dir_all(dir)
        .map_err(|e| UpdateError::Install(format!("cannot create {}: {e}", dir.display())))?;
    let tmp = target.with_extension("exe.new");
    std::fs::copy(verified, &tmp)
        .and_then(|_| std::fs::rename(&tmp, target))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            UpdateError::Install(format!("cannot copy the update to {}: {e}", dir.display()))
        })
}

fn rollback_note(r: &Result<(), UpdateError>) -> String {
    match r {
        Ok(()) => "the previous version was restored.".into(),
        Err(e) => format!("restoring the previous version also failed: {e}"),
    }
}

/// Start a launcher executable, forwarding `--root` if this one was given it.
fn spawn_new(
    exe: &Path,
    post_update: Option<(&str, &Path)>,
) -> std::io::Result<std::process::Child> {
    let mut cmd = std::process::Command::new(exe);
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--root")
        && let Some(r) = args.get(i + 1)
    {
        cmd.arg("--root").arg(r);
    }
    if let Some((from, health)) = post_update {
        cmd.arg("--post-update")
            .arg(from)
            .arg("--update-health")
            .arg(health);
    }
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn()
}

/// Start-up hook for the new version: report health and clean up leftovers.
/// Also removes a stale backup left by any earlier update.
pub fn on_startup(layout: &Layout, args: &[String]) -> Option<String> {
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let updated_from = arg("--post-update");
    let mut marker: Option<PathBuf> = None;
    if let Some(h) = arg("--update-health") {
        let h = PathBuf::from(h);
        // Only ever create the marker inside our own updates folder.
        if h.parent() == Some(update::updates_dir(layout).as_path()) {
            let _ = std::fs::write(&h, b"ok");
            marker = Some(h);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let updates = update::updates_dir(layout);
        let wait = if updated_from.is_some() {
            Duration::from_secs(60)
        } else {
            Duration::from_millis(0)
        };
        if updated_from.is_some() || install::backup_path(&exe).exists() {
            std::thread::Builder::new()
                .name("kestrel-update-cleanup".into())
                .spawn(move || {
                    install::cleanup_after_update(&exe, &updates, marker.as_deref(), wait)
                })
                .ok();
        }
    }
    updated_from.map(|from| {
        log::info!("updated from {from} to {}", kestrel_core::LAUNCHER_VERSION);
        format!(
            "Kestrel was updated from {from} to {}.",
            kestrel_core::LAUNCHER_VERSION
        )
    })
}

pub fn format_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1024.0 * 1024.0 {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    } else if b >= 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2 KB");
        assert_eq!(format_size(22_642_688), "21.6 MB");
    }

    #[test]
    fn health_marker_only_inside_updates_dir() {
        let d = tempfile_dir("health");
        let layout = Layout::new(&d);
        std::fs::create_dir_all(update::updates_dir(&layout)).unwrap();
        let good = update::updates_dir(&layout).join("health-0.2.0");
        let evil = d.join("elsewhere");
        let args = |p: &Path| {
            vec![
                "Kestrel.exe".into(),
                "--post-update".into(),
                "0.1.0".into(),
                "--update-health".into(),
                p.display().to_string(),
            ]
        };
        assert!(on_startup(&layout, &args(&good)).unwrap().contains("0.1.0"));
        assert!(good.exists());
        on_startup(&layout, &args(&evil));
        assert!(!evil.exists());
        assert!(on_startup(&layout, &["Kestrel.exe".into()]).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn fallback_copy_installs_and_replaces_safely() {
        let d = tempfile_dir("fallback");
        let layout = Layout::new(d.join("data"));
        let target = fallback_exe(&layout);
        assert!(
            target.starts_with(layout.root()) && !target.starts_with(update::updates_dir(&layout))
        );
        std::fs::create_dir_all(&d).unwrap();
        let v1 = d.join("v1.exe");
        std::fs::write(&v1, b"VERSION ONE").unwrap();
        install_fallback_copy(&v1, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"VERSION ONE");
        assert!(!target.with_extension("exe.new").exists());
        // A later update replaces it with the normal swap (backup kept until cleanup).
        let v2 = d.join("v2.exe");
        std::fs::write(&v2, b"VERSION TWO").unwrap();
        install_fallback_copy(&v2, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"VERSION TWO");
        assert_eq!(
            std::fs::read(install::backup_path(&target)).unwrap(),
            b"VERSION ONE"
        );
        // A missing source changes nothing.
        let e = install_fallback_copy(&d.join("missing.exe"), &target);
        assert!(e.is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"VERSION TWO");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Full orchestration with a mocked release service and a fake "new
    /// launcher" (a shell script) that either reports health or crashes.
    #[cfg(unix)]
    mod orchestration {
        use super::*;
        use base64::Engine as _;
        use ed25519_dalek::SigningKey;
        use kestrel_core::update::fetch::{HttpResponse, Transport};
        use kestrel_core::update::manifest::{self, Asset, ReleaseInfo};
        use sha2::{Digest, Sha256};
        use std::collections::HashMap;
        use std::os::unix::fs::PermissionsExt;

        struct Mock(HashMap<String, Vec<u8>>);
        impl Transport for Mock {
            fn get(&self, url: &str) -> Result<HttpResponse, String> {
                match self.0.get(url) {
                    Some(b) => Ok(HttpResponse {
                        status: 200,
                        location: None,
                        content_length: Some(b.len() as u64),
                        body: Box::new(std::io::Cursor::new(b.clone())),
                    }),
                    None => Err("offline".into()),
                }
            }
        }

        /// Stands in for the real self-test; marks the download executable.
        struct MakeExecutable;
        impl update::Validator for MakeExecutable {
            fn validate(&self, exe: &Path, _: &Version, _: &Path) -> Result<(), String> {
                std::fs::set_permissions(exe, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| e.to_string())
            }
        }

        const MANIFEST: &str =
            "https://github.com/o/kestrel/releases/latest/download/kestrel-update.json";
        const EXE: &str = "https://github.com/o/kestrel/releases/download/v9.0.0/Kestrel.exe";

        fn script(healthy: bool) -> Vec<u8> {
            let mut s = String::from("#!/bin/sh\n");
            if healthy {
                s.push_str("while [ $# -gt 0 ]; do if [ \"$1\" = --update-health ]; then echo ok > \"$2\"; fi; shift; done\nsleep 1\n");
            } else {
                s.push_str("exit 3\n");
            }
            // Pad past the manifest's minimum download size.
            s.push_str(&format!("# {}\n", "x".repeat(70_000)));
            s.into_bytes()
        }

        struct Fixture {
            dir: PathBuf,
            layout: Layout,
            current: PathBuf,
            mock: Mock,
            source: UpdateSource,
            release: Release,
        }

        fn setup(healthy: bool, tag: &str) -> Fixture {
            let dir = tempfile_dir(tag);
            let layout = Layout::new(dir.join("data"));
            layout.ensure().unwrap();
            std::fs::write(layout.launcher_config(), b"{\"max_ram_mib\":2048}").unwrap();
            let install_dir = dir.join("Desktop");
            std::fs::create_dir_all(&install_dir).unwrap();
            let current = install_dir.join("Kestrel.exe");
            std::fs::write(&current, b"OLD LAUNCHER").unwrap();

            let exe = script(healthy);
            let key = SigningKey::from_bytes(&[9; 32]);
            let info = ReleaseInfo {
                schema: 1,
                product: manifest::PRODUCT.into(),
                channel: manifest::CHANNEL_STABLE.into(),
                version: "9.0.0".into(),
                published: String::new(),
                minimum_supported: None,
                notes: "test".into(),
                windows_x86_64: Asset {
                    url: EXE.into(),
                    sha256: hex::encode(Sha256::digest(&exe)),
                    size: exe.len() as u64,
                },
            };
            let env = serde_json::to_vec(&manifest::sign(&info, &key)).unwrap();
            let pk =
                base64::engine::general_purpose::STANDARD.encode(key.verifying_key().to_bytes());
            let source = UpdateSource::new(MANIFEST, &pk).unwrap();
            let release = manifest::verify_and_parse(&env, &source).unwrap();
            let mock = Mock(HashMap::from([
                (MANIFEST.to_string(), env),
                (EXE.to_string(), exe),
            ]));
            Fixture {
                dir,
                layout,
                current,
                mock,
                source,
                release,
            }
        }

        fn run(f: &Fixture, transport: &dyn Transport) -> Result<Finish, String> {
            let env = UpdateEnv {
                transport,
                validator: &MakeExecutable,
                current_exe: &f.current,
                health_timeout: Duration::from_secs(10),
            };
            let (tx, _rx) = channel();
            run_update(
                &env,
                &f.source,
                &f.release,
                &f.layout,
                &Version::new(0, 1, 0),
                &AtomicBool::new(false),
                &tx,
                &egui::Context::default(),
            )
        }

        #[test]
        fn successful_update_restarts_into_new_version() {
            let f = setup(true, "ok");
            let r = run(&f, &f.mock);
            assert!(matches!(r, Ok(Finish::Restarted)), "{:?}", r.err());
            assert!(std::fs::read(&f.current).unwrap().starts_with(b"#!/bin/sh"));
            assert_eq!(
                std::fs::read(install::backup_path(&f.current)).unwrap(),
                b"OLD LAUNCHER"
            );
            assert_eq!(
                std::fs::read(f.layout.launcher_config()).unwrap(),
                b"{\"max_ram_mib\":2048}"
            );
            let _ = std::fs::remove_dir_all(&f.dir);
        }

        #[test]
        fn crashing_new_version_is_rolled_back() {
            let f = setup(false, "crash");
            let e = run(&f, &f.mock).err().expect("must fail");
            assert!(e.contains("restored"), "{e}");
            assert_eq!(
                std::fs::read(&f.current).unwrap(),
                b"OLD LAUNCHER",
                "previous launcher back in place"
            );
            assert!(!install::backup_path(&f.current).exists());
            assert_eq!(
                std::fs::read(f.layout.launcher_config()).unwrap(),
                b"{\"max_ram_mib\":2048}"
            );
            let _ = std::fs::remove_dir_all(&f.dir);
        }

        #[test]
        fn offline_update_leaves_everything_alone() {
            let f = setup(true, "offline");
            assert!(run(&f, &Mock(HashMap::new())).is_err());
            assert_eq!(std::fs::read(&f.current).unwrap(), b"OLD LAUNCHER");
            assert!(!install::backup_path(&f.current).exists());
            let _ = std::fs::remove_dir_all(&f.dir);
        }
    }

    fn tempfile_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kestrel-upd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }
}
