//! `Kestrel.exe --self-check <report>`: verifies that a packaged executable
//! carries everything it needs, without opening a window. Exit code 0 = OK.

use std::fmt::Write as _;
use std::path::Path;

use kestrel_core::cosmetics::{Kind, cape_catalog, sky_catalog};
use kestrel_core::install;
use kestrel_core::paths::Layout;

pub fn run(layout: &Layout, report_path: Option<&Path>) -> i32 {
    let mut report = String::new();
    let mut ok = true;
    let mut check = |name: &str, r: Result<String, String>| {
        match &r {
            Ok(detail) => writeln!(report, "PASS  {name}: {detail}"),
            Err(e) => writeln!(report, "FAIL  {name}: {e}"),
        }
        .ok();
        ok &= r.is_ok();
    };

    check(
        "version",
        Ok(format!(
            "Kestrel {} for Minecraft {} / Forge {}",
            kestrel_core::LAUNCHER_VERSION,
            kestrel_core::MC_VERSION,
            kestrel_core::FORGE_VERSION
        )),
    );
    check(
        "data directory writable",
        layout
            .ensure()
            .map(|_| layout.root().display().to_string())
            .map_err(|e| e.to_string()),
    );
    match crate::embedded::EMBEDDED_MOD {
        None => check(
            "embedded client mod",
            Err("not embedded (build with scripts\\release.ps1)".into()),
        ),
        Some(e) => {
            check(
                "embedded client mod",
                install::validate_mod_jar(e.bytes)
                    .map(|_| format!("{} ({} KB)", e.file_name, e.bytes.len() / 1024))
                    .map_err(|err| err.to_string()),
            );
            check(
                "client mod unpacks to data directory",
                install::materialize_embedded(layout, &e)
                    .map(|p| p.display().to_string())
                    .map_err(|err| err.to_string()),
            );
        }
    }
    // Informational: a build without an update source still works, it just
    // cannot update itself. A *malformed* configured source is a failure.
    check(
        "update source",
        match (
            crate::embedded::UPDATE_MANIFEST_URL,
            crate::embedded::UPDATE_PUBLIC_KEY,
        ) {
            (None, None) | (Some(""), Some("")) => {
                Ok("not configured (this build cannot self-update)".into())
            }
            (u, k) => kestrel_core::update::UpdateSource::from_build(u, k)
                .map(|s| {
                    format!(
                        "{} (key {})",
                        s.describe(),
                        kestrel_core::update::manifest::key_id(&s.public_key.to_bytes())
                    )
                })
                .map_err(|e| e.to_string()),
        },
    );
    // Test-only update hooks must never ship (release.ps1 enforces this).
    check(
        "update test hooks",
        Ok(if crate::updater::TEST_HOOKS {
            "ENABLED (test build, do not distribute)".into()
        } else {
            "none".into()
        }),
    );
    check(
        "window icon",
        eframe::icon_data::from_png_bytes(crate::embedded::ICON_PNG)
            .map(|i| format!("{}x{}", i.width, i.height))
            .map_err(|e| e.to_string()),
    );
    let capes: Vec<&str> = cape_catalog().iter().map(|c| c.id.as_str()).collect();
    check(
        "cape catalog",
        if capes.is_empty() {
            Err("capes.json is empty or invalid".to_string())
        } else {
            Ok(format!("{} capes", capes.len()))
        },
    );
    let skies: Vec<&str> = sky_catalog().iter().map(|s| s.id.as_str()).collect();
    for (kind, ids) in [(Kind::Cape, capes), (Kind::Sky, skies)] {
        for id in ids {
            check(
                &format!("bundled {kind:?} preview '{id}'"),
                crate::cosmetics_page::load_preview(layout, kind, id)
                    .map(|i| format!("{}x{}", i.width, i.height)),
            );
        }
    }
    writeln!(report, "RESULT: {}", if ok { "OK" } else { "FAILED" }).ok();

    match report_path {
        Some(p) => {
            if let Err(e) = std::fs::write(p, &report) {
                eprintln!("cannot write {}: {e}", p.display());
                return 2;
            }
        }
        None => print!("{report}"),
    }
    if ok { 0 } else { 1 }
}
