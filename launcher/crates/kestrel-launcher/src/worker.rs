//! Long-running jobs, executed off the UI thread. Results come back as
//! [`Msg`] over a channel; progress is read from the shared [`Progress`].

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use kestrel_core::auth::{self, Authenticator, DeviceCode, Session, StoredAccount};
use kestrel_core::config::LauncherConfig;
use kestrel_core::install::{self, InstallOptions, InstallState};
use kestrel_core::launch::{self, GameEvent};
use kestrel_core::mojang::Platform;
use kestrel_core::paths::Layout;
use kestrel_core::progress::SharedProgress;
use kestrel_core::runtime;

pub enum Msg {
    InstallFinished(Box<Result<InstallState, String>>),
    DeviceCode(DeviceCode),
    SignedIn(Result<StoredAccount, String>),
    Launched { log_path: PathBuf },
    LaunchFailed(String),
    Game(GameEvent),
    JavaProbe(Result<String, String>),
}

/// The client mod to install: embedded in this exe, or next to it (dev builds).
pub fn bundled_mod(layout: &Layout) -> Option<PathBuf> {
    install::resolve_mod(layout, crate::embedded::EMBEDDED_MOD.as_ref())
}

fn install_opts(layout: &Layout, cfg: &LauncherConfig) -> InstallOptions {
    InstallOptions {
        managed_java: cfg.use_managed_java,
        download_threads: cfg.download_threads,
        mod_jar: bundled_mod(layout),
    }
}

/// Full (re)install with verification of every file.
pub fn repair(
    layout: Layout,
    cfg: LauncherConfig,
    progress: SharedProgress,
    tx: Sender<Msg>,
    ctx: egui::Context,
) {
    let r = install::install(&layout, &install_opts(&layout, &cfg), &progress)
        .map_err(|e| e.to_string());
    if let Err(e) = &r {
        log::error!("installation failed: {e}");
    }
    let _ = tx.send(Msg::InstallFinished(Box::new(r)));
    ctx.request_repaint();
}

pub fn sign_in(
    client_id: String,
    accounts_path: PathBuf,
    progress: SharedProgress,
    tx: Sender<Msg>,
    ctx: egui::Context,
) {
    let r = (|| -> kestrel_core::Result<StoredAccount> {
        let a = Authenticator::new(&client_id)?;
        progress.stage("Requesting sign-in code", 0);
        let code = a.start_device_code()?;
        let _ = tx.send(Msg::DeviceCode(code.clone()));
        ctx.request_repaint();
        progress.stage("Waiting for you to sign in at microsoft.com", 0);
        let (ms, refresh) = a.poll_device_code(&code, &progress)?;
        progress.stage("Signing in to Minecraft", 0);
        let session = a.minecraft_session(&ms)?;
        let acct = StoredAccount {
            name: session.name,
            uuid: session.uuid,
            refresh_token: refresh,
        };
        auth::save_account(&accounts_path, &acct)?;
        progress.stage("Signed in", 0);
        Ok(acct)
    })()
    .map_err(|e| e.to_string());
    if let Err(e) = &r {
        log::error!("sign-in failed: {e}");
    }
    let _ = tx.send(Msg::SignedIn(r));
    ctx.request_repaint();
}

pub fn probe_java(path: PathBuf, tx: Sender<Msg>, ctx: egui::Context) {
    let r = runtime::probe_version(&path).map_err(|e| e.to_string());
    let _ = tx.send(Msg::JavaProbe(r));
    ctx.request_repaint();
}

pub struct PlayRequest {
    pub layout: Layout,
    pub cfg: LauncherConfig,
    /// Signed-in account; `None` means offline mode.
    pub account: Option<StoredAccount>,
}

/// Ensure installation, authenticate, build the command, start the game and
/// wait for it to exit.
pub fn play(req: PlayRequest, progress: SharedProgress, tx: Sender<Msg>, ctx: egui::Context) {
    let PlayRequest {
        layout,
        cfg,
        account,
    } = req;
    let accounts_path = layout.accounts();

    let prepared = (|| -> kestrel_core::Result<(launch::LaunchSpec, Option<StoredAccount>)> {
        // 1. Installation (fast path when the recorded files are intact).
        let mut state = match InstallState::load(&layout.install_state()) {
            Some(s) if s.quick_verify().is_empty() => s,
            Some(s) => {
                for p in s.quick_verify().iter().take(10) {
                    log::warn!("installation problem: {p}");
                }
                install::install(&layout, &install_opts(&layout, &cfg), &progress)?
            }
            None => install::install(&layout, &install_opts(&layout, &cfg), &progress)?,
        };
        // A launcher update may ship a newer client mod; refresh just that.
        if !mod_up_to_date(&layout, &state) {
            progress.stage("Updating Kestrel client mod", 0);
            let (jar, sha) = install::install_mod(&layout, bundled_mod(&layout).as_deref())?;
            state.mod_jar = jar;
            state.mod_sha1 = sha;
            state.save(&layout.install_state())?;
        }
        progress.check_cancel()?;

        // 2. Session.
        progress.stage("Authenticating", 0);
        let (session, updated_account) = match account {
            None => {
                log::info!("offline launch as {}", cfg.offline_name);
                (Session::offline(&cfg.offline_name), None)
            }
            Some(acct) => {
                let a = Authenticator::new(&cfg.msa_client_id)?;
                let (ms, refresh) = a.refresh(&acct.refresh_token)?;
                let session = a.minecraft_session(&ms)?;
                let updated = StoredAccount {
                    name: session.name.clone(),
                    uuid: session.uuid.clone(),
                    refresh_token: refresh,
                };
                auth::save_account(&accounts_path, &updated)?;
                (session, Some(updated))
            }
        };

        // 3. Java.
        let java = if cfg.use_managed_java {
            match &state.managed_java {
                Some(p) if p.is_file() => p.clone(),
                // Installed earlier with a custom Java, or runtime removed.
                _ => {
                    let dl = kestrel_core::download::Downloader::new();
                    let (p, v) = runtime::ensure(&dl, &layout.runtime(), &progress)?;
                    state.managed_java = Some(p.clone());
                    state.java_version = Some(v);
                    state.save(&layout.install_state())?;
                    p
                }
            }
        } else {
            let p = PathBuf::from(&cfg.custom_java_path);
            let v = runtime::probe_version(&p)?;
            if !runtime::is_java8(&v) {
                return Err(kestrel_core::Error::Launch(format!(
                    "custom Java is version {v}; Minecraft 1.8.9 with Forge requires Java 8. \
                     Switch to the managed runtime in Settings."
                )));
            }
            p
        };

        // 4. Command.
        let spec = launch::build(&state, &session, &cfg, &java, &layout, &Platform::current())?;
        Ok((spec, updated_account))
    })();

    let (spec, updated) = match prepared {
        Ok(x) => x,
        Err(e) => {
            log::error!("launch preparation failed: {e}");
            let _ = tx.send(Msg::LaunchFailed(e.to_string()));
            ctx.request_repaint();
            return;
        }
    };
    if let Some(acct) = updated {
        let _ = tx.send(Msg::SignedIn(Ok(acct)));
    }

    progress.stage("Starting Minecraft", 0);
    let (gtx, grx) = std::sync::mpsc::channel::<GameEvent>();
    let mut game = match launch::spawn(&spec, &layout, gtx) {
        Ok(g) => g,
        Err(e) => {
            log::error!("{e}");
            let _ = tx.send(Msg::LaunchFailed(e.to_string()));
            ctx.request_repaint();
            return;
        }
    };
    let _ = tx.send(Msg::Launched {
        log_path: game.log_path.clone(),
    });
    ctx.request_repaint();

    // Forward game output until both pipes close.
    let fwd_tx = tx.clone();
    let fwd_ctx = ctx.clone();
    let forwarder = std::thread::spawn(move || {
        for ev in grx {
            if fwd_tx.send(Msg::Game(ev)).is_err() {
                break;
            }
            fwd_ctx.request_repaint();
        }
    });
    let code = game.child.wait().ok().and_then(|s| s.code());
    let _ = forwarder.join();
    log::info!("game exited with code {code:?}");
    let _ = tx.send(Msg::Game(GameEvent::Exited(code)));
    ctx.request_repaint();
}

/// Whether the installed client mod still matches the bundled one.
fn mod_up_to_date(layout: &Layout, state: &InstallState) -> bool {
    match (bundled_mod(layout), &state.mod_sha1) {
        (None, _) => true,
        (Some(p), Some(h)) => kestrel_core::hash::sha1_file(&p)
            .map(|a| &a == h)
            .unwrap_or(false),
        (Some(_), None) => false,
    }
}
