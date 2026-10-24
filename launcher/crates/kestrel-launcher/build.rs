//! Build script for the launcher executable.
//!
//! * Embeds the client mod jar when `KESTREL_EMBED_MOD_JAR` points to it
//!   (set by scripts/release.ps1 and scripts/build.ps1), producing a
//!   standalone `Kestrel.exe`. Without it the launcher falls back to
//!   `<exe dir>/client/kestrel-*.jar` (development builds).
//! * On Windows targets, adds the icon and version information.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=KESTREL_EMBED_MOD_JAR");
    println!("cargo:rerun-if-env-changed=KESTREL_UPDATE_MANIFEST_URL");
    println!("cargo:rerun-if-env-changed=KESTREL_UPDATE_PUBLIC_KEY");
    println!("cargo:rerun-if-changed=assets/kestrel.ico");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let generated = out.join("embedded_mod.rs");

    let code = match env::var_os("KESTREL_EMBED_MOD_JAR").filter(|v| !v.is_empty()) {
        Some(path) => {
            let path = PathBuf::from(path);
            println!("cargo:rerun-if-changed={}", path.display());
            let bytes = fs::read(&path).unwrap_or_else(|e| {
                panic!(
                    "KESTREL_EMBED_MOD_JAR={} cannot be read: {e}",
                    path.display()
                )
            });
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            assert!(
                name.starts_with("kestrel-")
                    && name.ends_with(".jar")
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)),
                "embedded mod must be named kestrel-<version>.jar, got {name:?}"
            );
            assert!(
                bytes.starts_with(b"PK"),
                "{} is not a jar (zip) file",
                path.display()
            );
            fs::write(out.join("kestrel-mod.jar"), &bytes).expect("copy embedded jar");
            format!(
                "pub const EMBEDDED_MOD: Option<kestrel_core::install::EmbeddedMod> = \
                 Some(kestrel_core::install::EmbeddedMod {{ file_name: {name:?}, \
                 bytes: include_bytes!(concat!(env!(\"OUT_DIR\"), \"/kestrel-mod.jar\")) }});\n"
            )
        }
        None => {
            "pub const EMBEDDED_MOD: Option<kestrel_core::install::EmbeddedMod> = None;\n".into()
        }
    };
    fs::write(&generated, code).expect("write embedded_mod.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/kestrel.ico");
        if let Err(e) = res.compile() {
            // An icon must never break the build; the exe works without it.
            println!("cargo:warning=could not embed the Windows icon/version info: {e}");
        }
    }
}
