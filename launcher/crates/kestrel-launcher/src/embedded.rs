//! Resources compiled into the executable.

include!(concat!(env!("OUT_DIR"), "/embedded_mod.rs"));

/// Window/taskbar icon (the .exe file icon comes from the Windows resource).
pub const ICON_PNG: &[u8] = include_bytes!("../assets/kestrel.png");

/// Update source, compiled in by scripts/release.ps1 from release.json and
/// launcher/update-public-key.txt. Both empty = updates not configured.
pub const UPDATE_MANIFEST_URL: Option<&str> = option_env!("KESTREL_UPDATE_MANIFEST_URL");
pub const UPDATE_PUBLIC_KEY: Option<&str> = option_env!("KESTREL_UPDATE_PUBLIC_KEY");
