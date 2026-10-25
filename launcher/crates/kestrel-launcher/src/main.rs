// Hide the console window in release builds on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod cosmetics_page;
mod embedded;
mod graphics;
mod logger;
mod presence;
mod qol_page;
mod selfcheck;
mod updater;
mod worker;

use std::path::PathBuf;

use kestrel_core::paths::Layout;

/// Data root: `--root <dir>`, else `$KESTREL_HOME`, else the platform default.
fn data_root() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--root"
            && let Some(p) = args.next()
        {
            return PathBuf::from(p);
        }
    }
    std::env::var_os("KESTREL_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(Layout::default_root)
}

fn install_crash_handler(log_dir: PathBuf) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = format!(
            "Kestrel launcher {} crashed\n{info}\n\nbacktrace:\n{}",
            kestrel_core::LAUNCHER_VERSION,
            std::backtrace::Backtrace::force_capture()
        );
        let path = log_dir.join(format!(
            "launcher-crash-{}.txt",
            kestrel_core::auth::now_secs()
        ));
        let _ = std::fs::create_dir_all(&log_dir);
        let _ = std::fs::write(&path, kestrel_core::redact::redact(&report));
        log::error!("{report}");
        default(info);
    }));
}

fn main() -> eframe::Result {
    // `Kestrel.exe --self-check <report.txt> [--root <dir>]`: verify the
    // packaged executable without opening a window (used by release.ps1).
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--self-check") {
        let report = args.get(i + 1).map(PathBuf::from);
        std::process::exit(selfcheck::run(&Layout::new(data_root()), report.as_deref()));
    }

    let layout = Layout::new(data_root());
    let buffer = logger::LogBuffer::default();
    logger::init(&layout.logs(), buffer.clone());
    install_crash_handler(layout.logs());
    updater::e2e_startup_hook(&args);

    let gfx = graphics::choose(&args, layout.root());
    let fallback_allowed = !args.iter().any(|a| a == "--no-fallback");
    log::info!(
        "Kestrel {} starting on {} ({}); renderer {}",
        kestrel_core::LAUNCHER_VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
        gfx.id()
    );

    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Kestrel")
            .with_inner_size([960.0, 620.0])
            .with_min_inner_size([760.0, 520.0])
            .with_icon(std::sync::Arc::new(
                eframe::icon_data::from_png_bytes(embedded::ICON_PNG).unwrap_or_default(),
            )),
        ..Default::default()
    };
    graphics::configure(&mut options, gfx);

    let root = layout.root().to_path_buf();
    let startup_args = args.clone();
    let started = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let started_in_app = started.clone();
    // A renderer that panics during start-up counts as a failure too.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        eframe::run_native(
            "Kestrel",
            options,
            Box::new(move |cc| {
                // Reaching this point means the renderer initialised.
                started_in_app.store(true, std::sync::atomic::Ordering::SeqCst);
                graphics::remember(&root, gfx);
                // Tells a launcher that just updated us that we started fine.
                let updated = updater::on_startup(&layout, &startup_args);
                Ok(Box::new(app::LauncherApp::new(cc, layout, buffer, updated)))
            }),
        )
    }));
    let error = match result {
        Ok(Ok(())) => return Ok(()),
        Ok(Err(e)) => e.to_string(),
        Err(_) => "the renderer crashed during start-up".to_string(),
    };
    if started.load(std::sync::atomic::Ordering::SeqCst) {
        // Failed after the window was up: not a start-up problem.
        graphics::show_fatal(&format!("Kestrel stopped unexpectedly: {error}"));
        return Ok(());
    }
    log::error!("{} renderer failed to start: {error}", gfx.id());
    if fallback_allowed && graphics::relaunch_with(gfx.other(), &args) {
        log::info!("retrying with the {} renderer", gfx.other().id());
        return Ok(());
    }
    graphics::show_fatal(&format!(
        "Kestrel could not open its window: no working graphics renderer was found.\n\n\
         Last error: {error}\n\n\
         Update your graphics driver. In a virtual machine, enable 3D acceleration and install the \
         guest tools. Note that Minecraft itself also needs OpenGL 2.0 or newer.\n\n\
         Log: {}",
        data_root().join("logs").join("launcher.log").display()
    ));
    Ok(())
}
