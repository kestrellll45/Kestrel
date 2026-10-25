use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use egui::{Color32, CornerRadius, RichText, Stroke, Vec2};
use kestrel_core::auth::{self, DeviceCode, StoredAccount};
use kestrel_core::config::{GamePreset, LauncherConfig};
use kestrel_core::discord::{self, PresenceService};
use kestrel_core::install::InstallState;
use kestrel_core::launch::{GameActivity, GameEvent};
use kestrel_core::paths::Layout;
use kestrel_core::progress::{Progress, SharedProgress};
use kestrel_core::{FORGE_VERSION, LAUNCHER_VERSION, MC_VERSION, runtime, sysmem};

use crate::cosmetics_page::CosmeticsPage;
use crate::logger::LogBuffer;
use crate::presence::{self, Phase};
use crate::qol_page::QolPage;
use crate::updater::Updater;
use crate::worker::{self, Msg, PlayRequest};

pub(crate) const ACCENT: Color32 = Color32::from_rgb(0xE8, 0x83, 0x3A);
const BG: Color32 = Color32::from_rgb(0x14, 0x15, 0x18);
const PANEL: Color32 = Color32::from_rgb(0x1B, 0x1D, 0x21);
pub(crate) const CARD: Color32 = Color32::from_rgb(0x22, 0x25, 0x2A);
pub(crate) const MUTED: Color32 = Color32::from_rgb(0x9A, 0xA0, 0xA8);
pub(crate) const OK: Color32 = Color32::from_rgb(0x5B, 0xC2, 0x7A);
pub(crate) const WARN: Color32 = Color32::from_rgb(0xE3, 0xB3, 0x41);
pub(crate) const ERR: Color32 = Color32::from_rgb(0xE0, 0x5A, 0x5A);
const MAX_GAME_LINES: usize = 5000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Cosmetics,
    Qol,
    Account,
    Settings,
    Performance,
    Logs,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Busy {
    Idle,
    Installing,
    SigningIn,
    Launching,
    Running,
}

enum Status {
    Info(String),
    Ok(String),
    Error(String),
}

pub struct LauncherApp {
    layout: Layout,
    cfg: LauncherConfig,
    cfg_dirty: bool,
    page: Page,
    busy: Busy,
    progress: SharedProgress,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    install: Option<InstallState>,
    install_problems: Vec<String>,
    account: Option<StoredAccount>,
    offline_mode: bool,
    device_code: Option<DeviceCode>,
    status: Option<Status>,
    forge_seen: bool,
    mod_seen: bool,
    game_lines: Vec<String>,
    launcher_log: LogBuffer,
    log_tab_game: bool,
    java_probe: Option<Result<String, String>>,
    total_ram: Option<u64>,
    bundled_mod: Option<PathBuf>,
    cosmetics: CosmeticsPage,
    qol: QolPage,
    updater: Updater,
    /// Discord Rich Presence (background thread; no-op until configured).
    discord: PresenceService,
    launcher_started: u64,
    game_started: Option<u64>,
    game_activity: Option<GameActivity>,
}

impl LauncherApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        layout: Layout,
        launcher_log: LogBuffer,
        updated: Option<String>,
    ) -> Self {
        apply_theme(&cc.egui_ctx);
        let cfg = LauncherConfig::load(&layout.launcher_config());
        let install = InstallState::load(&layout.install_state());
        let install_problems = install
            .as_ref()
            .map(|s| s.quick_verify())
            .unwrap_or_default();
        let account = auth::load_account(&layout.accounts());
        let (tx, rx) = channel();
        let layout_for_cosmetics = layout.clone();
        // Starts the background update check (never blocks start-up).
        let updater = Updater::new(layout.clone(), &cfg, &cc.egui_ctx);
        let bundled_mod = crate::worker::bundled_mod(&layout);
        log::info!(
            "Kestrel launcher {LAUNCHER_VERSION}; data dir {}; bundled mod {:?}",
            layout.root().display(),
            bundled_mod
        );
        LauncherApp {
            offline_mode: account.is_none() && cfg.msa_client_id.is_empty(),
            layout,
            cfg,
            cfg_dirty: false,
            page: Page::Home,
            busy: Busy::Idle,
            progress: Progress::new(),
            tx,
            rx,
            install,
            install_problems,
            account,
            device_code: None,
            status: updated.map(Status::Ok),
            forge_seen: false,
            mod_seen: false,
            game_lines: Vec::new(),
            launcher_log,
            log_tab_game: true,
            java_probe: None,
            total_ram: sysmem::total_mib(),
            bundled_mod,
            qol: QolPage::new(layout_for_cosmetics.clone()),
            cosmetics: CosmeticsPage::new(layout_for_cosmetics),
            updater,
            discord: PresenceService::start(discord::system_connector()),
            launcher_started: presence::now_unix(),
            game_started: None,
            game_activity: None,
        }
    }

    fn start(&mut self, busy: Busy, f: impl FnOnce(SharedProgress, Sender<Msg>) + Send + 'static) {
        self.save_config();
        self.busy = busy;
        self.status = None;
        self.progress = Progress::new();
        let p = self.progress.clone();
        let tx = self.tx.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("kestrel-worker".into())
            .spawn(move || f(p, tx))
        {
            self.busy = Busy::Idle;
            self.status = Some(Status::Error(format!("could not start worker thread: {e}")));
        }
    }

    fn save_config(&mut self) {
        if !self.cfg_dirty {
            return;
        }
        self.cfg.validate();
        match self.cfg.save(&self.layout.launcher_config()) {
            Ok(()) => self.cfg_dirty = false,
            Err(e) => self.status = Some(Status::Error(format!("could not save settings: {e}"))),
        }
    }

    fn play(&mut self, ctx: &egui::Context) {
        // A signed release marked this version as no longer supported.
        if let Some(v) = self.updater.required_version(&self.cfg) {
            self.status = Some(Status::Error(format!(
                "Kestrel {v} is a required update. Install it before playing."
            )));
            self.updater.open_prompt(ctx);
            return;
        }
        if !self.offline_mode && self.account.is_none() {
            self.page = Page::Account;
            self.status = Some(Status::Error(
                "Sign in with Microsoft, or enable offline mode, before playing.".into(),
            ));
            return;
        }
        self.forge_seen = false;
        self.mod_seen = false;
        self.game_lines.clear();
        self.game_started = Some(presence::now_unix());
        self.game_activity = None;
        let req = PlayRequest {
            layout: self.layout.clone(),
            cfg: self.cfg.clone(),
            account: if self.offline_mode {
                None
            } else {
                self.account.clone()
            },
        };
        let ctx = ctx.clone();
        self.start(Busy::Launching, move |p, tx| worker::play(req, p, tx, ctx));
    }

    fn handle_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::InstallFinished(r) => match *r {
                    Ok(state) => {
                        self.install_problems = state.quick_verify();
                        self.install = Some(state);
                        self.busy = Busy::Idle;
                        self.status = Some(Status::Ok("Installation verified.".into()));
                    }
                    Err(e) => {
                        self.busy = Busy::Idle;
                        self.status = Some(Status::Error(format!("Installation failed: {e}")));
                    }
                },
                Msg::DeviceCode(c) => self.device_code = Some(c),
                Msg::SignedIn(Ok(a)) => {
                    if self.busy == Busy::SigningIn {
                        self.busy = Busy::Idle;
                        self.status = Some(Status::Ok(format!("Signed in as {}.", a.name)));
                        self.offline_mode = false;
                    }
                    self.device_code = None;
                    self.account = Some(a);
                }
                Msg::SignedIn(Err(e)) => {
                    self.busy = Busy::Idle;
                    self.device_code = None;
                    self.status = Some(Status::Error(format!("Sign-in failed: {e}")));
                }
                Msg::Launched { log_path } => {
                    self.busy = Busy::Running;
                    self.install = InstallState::load(&self.layout.install_state());
                    self.install_problems.clear();
                    self.status = Some(Status::Info(format!(
                        "Minecraft is starting… (log: {})",
                        log_path.display()
                    )));
                }
                Msg::LaunchFailed(e) => {
                    self.busy = Busy::Idle;
                    self.game_started = None;
                    self.game_activity = None;
                    self.install = InstallState::load(&self.layout.install_state());
                    self.status = Some(Status::Error(e));
                }
                Msg::Game(GameEvent::Line(l)) => {
                    if self.game_lines.len() >= MAX_GAME_LINES {
                        self.game_lines.drain(..MAX_GAME_LINES / 10);
                    }
                    self.game_lines.push(l);
                }
                Msg::Game(GameEvent::ForgeLoaded) => self.forge_seen = true,
                Msg::Game(GameEvent::Activity(a)) => self.game_activity = Some(a),
                Msg::Game(GameEvent::ModReady) => {
                    self.mod_seen = true;
                    self.status = Some(Status::Ok(
                        "Minecraft is running with the Kestrel client.".into(),
                    ));
                    if self.cfg.close_on_launch {
                        std::process::exit(0);
                    }
                }
                Msg::Game(GameEvent::Exited(code)) => {
                    self.busy = Busy::Idle;
                    self.game_started = None;
                    self.game_activity = None;
                    self.status = Some(match code {
                        Some(0) => Status::Info("Minecraft closed.".into()),
                        c => Status::Error(format!(
                            "Minecraft exited with code {}. See Logs for details{}.",
                            c.map(|c| c.to_string()).unwrap_or_else(|| "unknown".into()),
                            if self.mod_seen {
                                ""
                            } else {
                                " (the client mod never finished loading)"
                            }
                        )),
                    });
                }
                Msg::JavaProbe(r) => self.java_probe = Some(r),
            }
        }
    }

    /// Push the current state to Discord (dropped when unchanged; never blocks).
    fn sync_discord(&mut self) {
        let phase = match self.busy {
            Busy::Launching => Phase::Starting,
            Busy::Running => Phase::Running(self.game_activity),
            Busy::Idle | Busy::Installing | Busy::SigningIn => Phase::Launcher,
        };
        self.discord.set(presence::desired(
            &self.cfg,
            phase,
            self.launcher_started,
            self.game_started,
        ));
    }

    // ---- pages --------------------------------------------------------------

    fn home(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new("KESTREL").size(42.0).strong().color(ACCENT));
        ui.label(
            RichText::new(format!("Minecraft {MC_VERSION} PvP client"))
                .size(15.0)
                .color(MUTED),
        );
        ui.add_space(18.0);

        card(ui, |ui| {
            ui.horizontal(|ui| {
                let (txt, col) = match (&self.install, self.install_problems.is_empty()) {
                    (None, _) => (
                        "Not installed — Play will download and install everything.",
                        WARN,
                    ),
                    (Some(_), false) => (
                        "Installation has problems — Play or Repair will fix it.",
                        WARN,
                    ),
                    (Some(_), true) => ("Installed and ready.", OK),
                };
                dot(ui, col);
                ui.label(txt);
            });
            ui.add_space(6.0);
            egui::Grid::new("info")
                .num_columns(2)
                .spacing([24.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Game").color(MUTED));
                    ui.label(format!("Minecraft {MC_VERSION} + Forge {FORGE_VERSION}"));
                    ui.end_row();
                    ui.label(RichText::new("Java").color(MUTED));
                    ui.label(if self.cfg.use_managed_java {
                        match self.install.as_ref().and_then(|s| s.java_version.clone()) {
                            Some(v) => format!("Managed Java {v}"),
                            None => "Managed Java 8 (installed on first launch)".into(),
                        }
                    } else {
                        format!("Custom: {}", self.cfg.custom_java_path)
                    });
                    ui.end_row();
                    ui.label(RichText::new("Memory").color(MUTED));
                    ui.label(format!("{} MB", self.cfg.max_ram_mib));
                    ui.end_row();
                    ui.label(RichText::new("Account").color(MUTED));
                    ui.label(match (&self.account, self.offline_mode) {
                        (_, true) => format!("Offline: {}", self.cfg.offline_name),
                        (Some(a), false) => a.name.clone(),
                        (None, false) => "Not signed in".into(),
                    });
                    ui.end_row();
                    ui.label(RichText::new("Preset").color(MUTED));
                    ui.label(self.cfg.game_preset.label());
                    ui.end_row();
                    ui.label(RichText::new("Client mod").color(MUTED));
                    ui.label(match (&self.bundled_mod, crate::embedded::EMBEDDED_MOD) {
                        (Some(_), Some(e)) => format!("{} (built into Kestrel.exe)", e.file_name),
                        (Some(p), None) => p
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        (None, _) => "Not bundled — build with scripts\\release.ps1".into(),
                    });
                    ui.end_row();
                });
        });

        ui.add_space(16.0);
        let ctx = ui.ctx().clone();
        let required = self.updater.required_version(&self.cfg);
        if let Some(v) = &required {
            egui::Frame::NONE
                .fill(Color32::from_rgb(0x3A, 0x2A, 0x12))
                .stroke(Stroke::new(1.0, WARN))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Update required").strong().color(WARN));
                    ui.label(format!(
                        "Kestrel {v} is required to play. This version ({LAUNCHER_VERSION}) can no longer start Minecraft. Your settings, accounts, game files, mods and resource packs are kept."
                    ));
                    ui.add_space(4.0);
                    let checking = self.updater.is_checking();
                    if ui
                        .add_enabled(
                            !checking && self.busy == Busy::Idle,
                            egui::Button::new(RichText::new("Update now").strong()),
                        )
                        .clicked()
                    {
                        self.updater.open_prompt(&ctx);
                    }
                    if checking {
                        ui.label(RichText::new("Checking for the update…").color(MUTED));
                    }
                });
            ui.add_space(12.0);
        }
        ui.horizontal(|ui| {
            let busy = self.busy != Busy::Idle;
            let play_blocked = busy || required.is_some();
            let label = match self.busy {
                Busy::Idle if required.is_some() => "UPDATE REQUIRED",
                Busy::Idle => "PLAY",
                Busy::Installing => "INSTALLING…",
                Busy::SigningIn => "SIGNING IN…",
                Busy::Launching => "STARTING…",
                Busy::Running => "RUNNING",
            };
            let btn = egui::Button::new(
                RichText::new(label)
                    .size(22.0)
                    .strong()
                    .color(Color32::BLACK),
            )
            .fill(if play_blocked {
                ACCENT.gamma_multiply(0.5)
            } else {
                ACCENT
            })
            .min_size(Vec2::new(220.0, 56.0))
            .corner_radius(CornerRadius::same(8));
            if ui.add_enabled(!play_blocked, btn).clicked() {
                self.play(&ctx);
            }
            ui.add_space(8.0);
            ui.vertical(|ui| {
                if ui
                    .add_enabled(!busy, egui::Button::new("Repair / verify installation"))
                    .clicked()
                {
                    let (layout, cfg, ctx) = (self.layout.clone(), self.cfg.clone(), ctx.clone());
                    self.start(Busy::Installing, move |p, tx| {
                        worker::repair(layout, cfg, p, tx, ctx)
                    });
                }
                if ui.button("Open game folder").clicked() {
                    open_path(&self.layout.instance());
                }
            });
        });

        if matches!(
            self.busy,
            Busy::Installing | Busy::Launching | Busy::SigningIn
        ) {
            ui.add_space(14.0);
            let (stage, done, total) = self.progress.snapshot();
            ui.label(stage);
            let frac = if total > 0 {
                done as f32 / total as f32
            } else {
                0.0
            };
            let bar = egui::ProgressBar::new(frac).desired_width(460.0);
            ui.add(if total > 0 {
                bar.text(format!("{done} / {total}"))
            } else {
                bar.animate(true)
            });
            if ui.button("Cancel").clicked() {
                self.progress.cancel();
            }
        }
        if self.busy == Busy::Running {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                indicator(ui, "Forge loaded", self.forge_seen);
                indicator(ui, "Kestrel loaded", self.mod_seen);
            });
        }
    }

    fn account_page(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Account");
        card(ui, |ui| {
            match &self.account {
                Some(a) => {
                    ui.label(RichText::new(format!("Signed in as {}", a.name)).size(16.0));
                    ui.label(
                        RichText::new(format!("UUID {}", a.uuid))
                            .color(MUTED)
                            .small(),
                    );
                    if !kestrel_core::secure::is_supported() {
                        ui.label(RichText::new("Secure storage is unavailable on this OS; you will need to sign in again next time.").color(WARN));
                    }
                    if ui
                        .add_enabled(self.busy == Busy::Idle, egui::Button::new("Sign out"))
                        .clicked()
                    {
                        auth::forget_account(&self.layout.accounts());
                        self.account = None;
                    }
                }
                None => {
                    ui.label("Sign in with your Microsoft account to play on online servers such as Hypixel.");
                    ui.label(
                        RichText::new("You sign in on microsoft.com in your browser. Kestrel never sees your password.")
                            .color(MUTED),
                    );
                    ui.add_space(6.0);
                    let client_id = auth::validate_client_id(&self.cfg.msa_client_id);
                    let can = self.busy == Busy::Idle && client_id.is_ok();
                    if ui
                        .add_enabled(can, egui::Button::new("Sign in with Microsoft"))
                        .clicked()
                    {
                        let (id, path, ctx) = (
                            client_id.as_ref().cloned().unwrap_or_default(),
                            self.layout.accounts(),
                            ui.ctx().clone(),
                        );
                        self.start(Busy::SigningIn, move |p, tx| {
                            worker::sign_in(id, path, p, tx, ctx)
                        });
                    }
                    if let Err(e) = &client_id {
                        ui.label(RichText::new(format!("{e}")).color(WARN));
                    }
                }
            }
            if let Some(code) = &self.device_code {
                ui.add_space(10.0);
                ui.label("Open the page below and enter this code:");
                ui.label(
                    RichText::new(&code.user_code)
                        .size(30.0)
                        .monospace()
                        .strong()
                        .color(ACCENT),
                );
                ui.horizontal(|ui| {
                    if ui
                        .button(format!("Open {}", code.verification_uri))
                        .clicked()
                    {
                        ui.ctx()
                            .open_url(egui::OpenUrl::new_tab(&code.verification_uri));
                    }
                    if ui.button("Copy code").clicked() {
                        ui.ctx().copy_text(code.user_code.clone());
                    }
                    if ui.button("Cancel").clicked() {
                        self.progress.cancel();
                    }
                });
            }
        });
        ui.add_space(12.0);
        card(ui, |ui| {
            ui.checkbox(&mut self.offline_mode, "Offline mode");
            ui.label(
                RichText::new("For singleplayer and offline-mode servers only. Online servers (Hypixel, etc.) reject offline sessions.")
                    .color(MUTED),
            );
            ui.horizontal(|ui| {
                ui.label("Offline name");
                if ui
                    .text_edit_singleline(&mut self.cfg.offline_name)
                    .changed()
                {
                    self.cfg_dirty = true;
                }
            });
        });
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Settings");
        let mut dirty = self
            .updater
            .settings_ui(ui, &mut self.cfg, self.busy != Busy::Idle);
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(RichText::new("Memory").strong());
            let upper = self
                .total_ram
                .map(|t| t * 3 / 4)
                .unwrap_or(16384)
                .clamp(1024, 16384);
            dirty |= ui
                .add(
                    egui::Slider::new(&mut self.cfg.max_ram_mib, 512..=upper)
                        .suffix(" MB")
                        .step_by(256.0),
                )
                .changed();
            ui.horizontal_wrapped(|ui| {
                for p in sysmem::presets(self.total_ram) {
                    if ui
                        .selectable_label(self.cfg.max_ram_mib == p.max_mib, p.name)
                        .clicked()
                    {
                        self.cfg.max_ram_mib = p.max_mib;
                        dirty = true;
                    }
                }
            });
            ui.label(
                RichText::new(format!(
                    "System memory: {}. 1.8.9 rarely benefits from more than 3–4 GB; larger heaps can increase GC pauses.",
                    self.total_ram.map(|t| format!("{} MB", t)).unwrap_or_else(|| "unknown".into())
                ))
                .color(MUTED)
                .small(),
            );
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(RichText::new("Java").strong());
            dirty |= ui
                .radio_value(
                    &mut self.cfg.use_managed_java,
                    true,
                    "Managed Java 8 from Mojang (recommended)",
                )
                .changed();
            dirty |= ui
                .radio_value(
                    &mut self.cfg.use_managed_java,
                    false,
                    "Custom Java executable",
                )
                .changed();
            ui.add_enabled_ui(!self.cfg.use_managed_java, |ui| {
                ui.horizontal(|ui| {
                    dirty |= ui
                        .add(
                            egui::TextEdit::singleline(&mut self.cfg.custom_java_path)
                                .desired_width(360.0)
                                .hint_text("C:\\Program Files\\Java\\jre1.8.0_xxx\\bin\\java.exe"),
                        )
                        .changed();
                    if ui.button("Test").clicked() {
                        self.java_probe = None;
                        let (p, tx, ctx) = (
                            PathBuf::from(&self.cfg.custom_java_path),
                            self.tx.clone(),
                            ui.ctx().clone(),
                        );
                        std::thread::spawn(move || worker::probe_java(p, tx, ctx));
                    }
                });
                match &self.java_probe {
                    Some(Ok(v)) if runtime::is_java8(v) => {
                        ui.label(RichText::new(format!("Java {v} — OK")).color(OK));
                    }
                    Some(Ok(v)) => {
                        ui.label(
                            RichText::new(format!("Java {v} — 1.8.9 + Forge needs Java 8"))
                                .color(ERR),
                        );
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(ERR));
                    }
                    None => {}
                }
            });
            ui.add_space(4.0);
            dirty |= ui
                .checkbox(
                    &mut self.cfg.use_default_jvm_args,
                    "Use tuned G1 garbage collector flags",
                )
                .changed();
            ui.horizontal(|ui| {
                ui.label("Extra JVM arguments");
                dirty |= ui
                    .add(
                        egui::TextEdit::singleline(&mut self.cfg.extra_jvm_args)
                            .desired_width(320.0),
                    )
                    .changed();
            });
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(RichText::new("Game window").strong());
            dirty |= ui
                .checkbox(&mut self.cfg.fullscreen, "Start in fullscreen")
                .changed();
            ui.add_enabled_ui(!self.cfg.fullscreen, |ui| {
                ui.horizontal(|ui| {
                    dirty |= ui
                        .add(egui::DragValue::new(&mut self.cfg.window_width).range(640..=7680))
                        .changed();
                    ui.label("×");
                    dirty |= ui
                        .add(egui::DragValue::new(&mut self.cfg.window_height).range(480..=4320))
                        .changed();
                });
            });
            dirty |= ui
                .checkbox(
                    &mut self.cfg.close_on_launch,
                    "Close the launcher once the game has loaded",
                )
                .changed();
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(RichText::new("Advanced").strong());
            ui.horizontal(|ui| {
                ui.label("Microsoft app (client) ID");
                dirty |= ui
                    .add(
                        egui::TextEdit::singleline(&mut self.cfg.msa_client_id)
                            .hint_text("00000000-0000-0000-0000-000000000000")
                            .desired_width(300.0),
                    )
                    .changed();
            });
            if !self.cfg.msa_client_id.trim().is_empty() {
                match auth::validate_client_id(&self.cfg.msa_client_id) {
                    Ok(_) => {
                        ui.label(
                            RichText::new("Signs in with personal Microsoft accounts (login.microsoftonline.com/consumers).")
                                .small()
                                .color(MUTED),
                        );
                    }
                    Err(e) => {
                        ui.label(RichText::new(format!("{e}")).small().color(WARN));
                    }
                }
            }
            ui.add_space(6.0);
            dirty |= ui
                .checkbox(
                    &mut self.cfg.discord_rpc_enabled,
                    "Discord Rich Presence (show Kestrel and your game state in Discord)",
                )
                .changed();
            ui.add_enabled_ui(self.cfg.discord_rpc_enabled, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Discord application ID");
                    dirty |= ui
                        .add(
                            egui::TextEdit::singleline(&mut self.cfg.discord_client_id)
                                .hint_text("e.g. 123456789012345678")
                                .desired_width(220.0),
                        )
                        .changed();
                });
                let (text, color) = match (
                    discord::validate_client_id(&self.cfg.discord_client_id),
                    self.discord.status(),
                ) {
                    (Err(_), _) if self.cfg.discord_client_id.trim().is_empty() => (
                        "Not configured: create an application named \"Kestrel\" at discord.com/developers, \
                         upload an art asset named \"kestrel\", and paste its Application ID here."
                            .to_string(),
                        MUTED,
                    ),
                    (Err(e), _) => (e, WARN),
                    (Ok(_), discord::Status::Connected) => ("Connected to Discord.".into(), OK),
                    (Ok(_), discord::Status::Waiting(m)) => (format!("{m}; retrying in the background."), MUTED),
                    (Ok(_), discord::Status::Error(m)) => (m, ERR),
                    (Ok(_), _) => ("Connecting to Discord…".into(), MUTED),
                };
                ui.label(RichText::new(text).small().color(color));
                ui.label(
                    RichText::new(
                        "Shows only: in the launcher / starting / main menu / singleplayer / multiplayer, with elapsed time. \
                         Never server addresses, world names or account details.",
                    )
                    .small()
                    .color(MUTED),
                );
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Download threads");
                dirty |= ui
                    .add(egui::Slider::new(&mut self.cfg.download_threads, 1..=16))
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Data directory: {}", self.layout.root().display()))
                        .color(MUTED),
                );
                if ui.button("Open").clicked() {
                    open_path(self.layout.root());
                }
            });
        });
        if dirty {
            self.cfg_dirty = true;
        }
    }

    fn performance(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Performance presets");
        ui.label(
            RichText::new(
                "Choosing a preset here asks the client to apply it on next start. Individual settings remain adjustable in-game (Right Shift), \
                 and presets never reset unrelated preferences such as HUD layout or keybinds.",
            )
            .color(MUTED),
        );
        ui.add_space(8.0);
        for p in GamePreset::ALL {
            card(ui, |ui| {
                if ui
                    .radio_value(
                        &mut self.cfg.game_preset,
                        p,
                        RichText::new(p.label()).strong().size(16.0),
                    )
                    .changed()
                {
                    self.cfg_dirty = true;
                }
                ui.label(RichText::new(p.description()).color(MUTED));
            });
            ui.add_space(6.0);
        }
    }

    fn logs(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Logs");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.log_tab_game, true, "Game output");
            ui.selectable_value(&mut self.log_tab_game, false, "Launcher");
            ui.separator();
            if ui.button("Open logs folder").clicked() {
                open_path(&self.layout.logs());
            }
            if ui.button("Copy all").clicked() {
                let text = if self.log_tab_game {
                    self.game_lines.join("\n")
                } else {
                    self.launcher_log.snapshot().join("\n")
                };
                ui.ctx().copy_text(text);
            }
            if ui.button("Clear").clicked() {
                if self.log_tab_game {
                    self.game_lines.clear();
                } else {
                    self.launcher_log.clear();
                }
            }
        });
        ui.add_space(6.0);
        let launcher_lines;
        let lines: &[String] = if self.log_tab_game {
            &self.game_lines
        } else {
            launcher_lines = self.launcher_log.snapshot();
            &launcher_lines
        };
        egui::Frame::NONE
            .fill(Color32::from_rgb(0x0E, 0x0F, 0x11))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(8.0)
            .show(ui, |ui| {
                let row_h = ui.text_style_height(&egui::TextStyle::Monospace);
                egui::ScrollArea::both()
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show_rows(ui, row_h, lines.len(), |ui, range| {
                        for l in &lines[range] {
                            let col = if l.contains("ERROR") || l.contains("Exception") {
                                ERR
                            } else if l.contains("WARN") {
                                WARN
                            } else {
                                Color32::from_gray(200)
                            };
                            ui.label(RichText::new(l).monospace().color(col));
                        }
                    });
            });
    }

    fn about(&mut self, ui: &mut egui::Ui) {
        heading(ui, "About Kestrel");
        card(ui, |ui| {
            egui::Grid::new("about")
                .num_columns(2)
                .spacing([24.0, 4.0])
                .show(ui, |ui| {
                    for (k, v) in [
                        ("Launcher", LAUNCHER_VERSION.to_string()),
                        ("Minecraft", MC_VERSION.to_string()),
                        ("Forge", FORGE_VERSION.to_string()),
                        (
                            "Java runtime",
                            format!(
                                "Mojang {} ({})",
                                runtime::COMPONENT,
                                runtime::platform_key()
                            ),
                        ),
                        ("Data directory", self.layout.root().display().to_string()),
                    ] {
                        ui.label(RichText::new(k).color(MUTED));
                        ui.label(v);
                        ui.end_row();
                    }
                });
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.label(RichText::new("Notes").strong());
            ui.label("• Capes, skies and kill effects are rendered on your client only. Other players do not see them \
                      (except capes the bundled catalog assigns to a specific player, which Kestrel clients draw on that player).");
            ui.label("• Kestrel installs into its own directory and never modifies your normal .minecraft folder.");
            ui.label("• Game files are downloaded from Mojang and Forge servers and verified by SHA-1 where hashes are published.");
            ui.label("• Not affiliated with Mojang or Microsoft.");
        });
    }
}

impl eframe::App for LauncherApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_messages();
        self.sync_discord();
        let ctx = ui.ctx().clone();
        if self.busy != Busy::Idle {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        egui::Panel::left("nav")
            .exact_size(170.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(PANEL).inner_margin(12.0))
            .show(ui, |ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("KESTREL").size(20.0).strong().color(ACCENT));
                ui.label(
                    RichText::new(format!("v{LAUNCHER_VERSION}"))
                        .small()
                        .color(MUTED),
                );
                ui.add_space(16.0);
                for (p, name) in [
                    (Page::Home, "Home"),
                    (Page::Cosmetics, "Cosmetics"),
                    (Page::Qol, "QoL"),
                    (Page::Account, "Account"),
                    (Page::Settings, "Settings"),
                    (Page::Performance, "Performance"),
                    (Page::Logs, "Logs"),
                    (Page::About, "About"),
                ] {
                    let selected = self.page == p;
                    let text = RichText::new(name).size(15.0).color(if selected {
                        Color32::WHITE
                    } else {
                        MUTED
                    });
                    let b = egui::Button::new(text)
                        .fill(if selected { CARD } else { Color32::TRANSPARENT })
                        .stroke(if selected {
                            Stroke::new(1.0, ACCENT)
                        } else {
                            Stroke::NONE
                        })
                        .min_size(Vec2::new(146.0, 32.0));
                    if ui.add(b).clicked() {
                        if self.page == Page::Settings || self.page == Page::Account {
                            self.save_config();
                        }
                        if p == Page::Cosmetics {
                            self.cosmetics.refresh();
                        }
                        if p == Page::Qol {
                            self.qol.refresh();
                        }
                        self.page = p;
                    }
                    ui.add_space(2.0);
                }
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG).inner_margin(24.0))
            .show(ui, |ui| {
                if let Some(s) = &self.status {
                    let (txt, col) = match s {
                        Status::Info(t) => (t.as_str(), MUTED),
                        Status::Ok(t) => (t.as_str(), OK),
                        Status::Error(t) => (t.as_str(), ERR),
                    };
                    let mut dismiss = false;
                    egui::Frame::NONE
                        .fill(CARD)
                        .stroke(Stroke::new(1.0, col))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(10.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(txt).color(col));
                                if ui.small_button("Dismiss").clicked() {
                                    dismiss = true;
                                }
                            });
                        });
                    if dismiss {
                        self.status = None;
                    }
                    ui.add_space(10.0);
                }
                if self.page == Page::Logs {
                    self.logs(ui);
                    return;
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.page {
                        Page::Home => self.home(ui),
                        Page::Account => self.account_page(ui),
                        Page::Settings => self.settings(ui),
                        Page::Performance => self.performance(ui),
                        Page::Cosmetics => {
                            // Restricted catalog capes are offered only to the
                            // signed-in Microsoft account that owns them.
                            let uuid = self
                                .account
                                .as_ref()
                                .filter(|_| !self.offline_mode)
                                .map(|a| a.uuid.as_str());
                            self.cosmetics.set_account(uuid);
                            self.cosmetics.ui(ui, self.busy == Busy::Running)
                        }
                        Page::Qol => self.qol.ui(
                            ui,
                            self.busy != Busy::Idle && self.busy != Busy::SigningIn,
                            self.busy == Busy::Running,
                        ),
                        Page::About => self.about(ui),
                        Page::Logs => {}
                    });
            });

        let actions = self
            .updater
            .prompt_ui(&ctx, &mut self.cfg, self.busy != Busy::Idle);
        if actions.save_config {
            self.cfg_dirty = true;
            self.save_config();
        }
        if actions.exit {
            // The updated launcher is running; close this one.
            self.save_config();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Debounced autosave: settings are written when you leave the page,
        // start a job, or every couple of seconds while editing.
        if self.cfg_dirty && ctx.input(|i| i.time) % 2.0 < 0.05 {
            self.save_config();
        }
        if self.cfg_dirty {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_config();
        self.progress.cancel();
    }
}

fn apply_theme(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(0x0E, 0x0F, 0x11);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.6);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.7));
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
    });
}

pub(crate) fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(CARD)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width().min(720.0));
            add(ui);
        });
}

pub(crate) fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(24.0).strong());
    ui.add_space(10.0);
}

fn dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}

fn indicator(ui: &mut egui::Ui, label: &str, on: bool) {
    dot(ui, if on { OK } else { MUTED });
    ui.label(RichText::new(label).color(if on { Color32::WHITE } else { MUTED }));
    ui.add_space(12.0);
}

pub(crate) fn open_path(p: &Path) {
    let _ = std::fs::create_dir_all(p);
    #[cfg(windows)]
    let r = std::process::Command::new("explorer").arg(p).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(p).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(p).spawn();
    if let Err(e) = r {
        log::error!("cannot open {}: {e}", p.display());
    }
}
