//! Launcher settings (`launcher.json`). Separate from the in-game client
//! config, which the mod owns (`instance/kestrel/config.json`).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths::atomic_write;
use crate::sysmem;

pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GamePreset {
    Potato,
    #[default]
    Competitive,
    Visual,
}

impl GamePreset {
    pub const ALL: [GamePreset; 3] = [
        GamePreset::Potato,
        GamePreset::Competitive,
        GamePreset::Visual,
    ];

    pub fn id(self) -> &'static str {
        match self {
            GamePreset::Potato => "potato",
            GamePreset::Competitive => "competitive",
            GamePreset::Visual => "visual",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GamePreset::Potato => "Potato",
            GamePreset::Competitive => "Competitive",
            GamePreset::Visual => "Visual",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            GamePreset::Potato => {
                "Very low-end PCs: no custom sky, minimal particles and effects, light HUD."
            }
            GamePreset::Competitive => {
                "PvP: FPS/CPS/keystrokes HUD, simple sky, lightweight kill effects."
            }
            GamePreset::Visual => "Appearance: custom skies, animated capes, richer kill effects.",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherConfig {
    pub config_version: u32,
    /// Max heap (-Xmx) in MiB.
    pub max_ram_mib: u64,
    /// Use the managed Mojang Java 8 runtime (recommended).
    pub use_managed_java: bool,
    /// Custom java executable when `use_managed_java` is false.
    pub custom_java_path: String,
    /// Apply the launcher's tuned GC flags.
    pub use_default_jvm_args: bool,
    /// Extra JVM arguments, whitespace-separated.
    pub extra_jvm_args: String,
    pub window_width: u32,
    pub window_height: u32,
    pub fullscreen: bool,
    /// Azure application (client) ID used for Microsoft sign-in.
    pub msa_client_id: String,
    /// Name used for offline/development launches.
    pub offline_name: String,
    /// Preset requested from the launcher; applied by the mod on next start
    /// only when it differs from what the mod last applied.
    pub game_preset: GamePreset,
    pub close_on_launch: bool,
    /// Parallel download threads.
    pub download_threads: usize,
    /// Check for launcher updates at start-up.
    pub update_auto_check: bool,
    /// Version the user answered "Later" for; not prompted again automatically.
    pub update_dismissed_version: String,
    /// `minimum_supported` of the last signed release that required an update
    /// from this launcher. Play stays blocked while the running version is
    /// older, even offline; cleared once a check shows nothing is required.
    pub update_required_minimum: String,
    /// Show Kestrel/Minecraft activity in Discord (needs `discord_client_id`).
    pub discord_rpc_enabled: bool,
    /// Discord application ID of the "Kestrel" app (Developer Portal). Empty
    /// = Rich Presence not configured.
    pub discord_client_id: String,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        LauncherConfig {
            config_version: CONFIG_VERSION,
            max_ram_mib: sysmem::default_max_mib(sysmem::total_mib()),
            use_managed_java: true,
            custom_java_path: String::new(),
            use_default_jvm_args: true,
            extra_jvm_args: String::new(),
            window_width: 1280,
            window_height: 720,
            fullscreen: false,
            msa_client_id: String::new(),
            offline_name: "Player".into(),
            game_preset: GamePreset::Competitive,
            close_on_launch: false,
            download_threads: 8,
            update_auto_check: true,
            update_dismissed_version: String::new(),
            update_required_minimum: String::new(),
            discord_rpc_enabled: true,
            discord_client_id: String::new(),
        }
    }
}

impl LauncherConfig {
    /// Load from disk. Missing → defaults. Malformed → the bad file is kept
    /// as `launcher.json.bad` and defaults are used, so a corrupt file never
    /// blocks the launcher and is never silently destroyed.
    pub fn load(path: &Path) -> LauncherConfig {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return LauncherConfig::default(),
            Err(e) => {
                log::error!("cannot read {}: {e}; using defaults", path.display());
                return LauncherConfig::default();
            }
        };
        match serde_json::from_slice::<LauncherConfig>(&bytes) {
            Ok(mut c) => {
                c.migrate();
                c.validate();
                c
            }
            Err(e) => {
                let bad = path.with_extension("json.bad");
                log::error!(
                    "malformed {}: {e}; backed up to {}",
                    path.display(),
                    bad.display()
                );
                let _ = std::fs::write(&bad, &bytes);
                LauncherConfig::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let data =
            serde_json::to_vec_pretty(self).map_err(|e| Error::json("launcher config", e))?;
        atomic_write(path, &data)
    }

    fn migrate(&mut self) {
        // v1 is the first schema. Future migrations go here, keyed on the
        // stored version, and must preserve unrelated settings.
        if self.config_version > CONFIG_VERSION {
            log::warn!(
                "launcher.json is from a newer launcher (v{}); unknown fields are ignored",
                self.config_version
            );
        }
        self.config_version = CONFIG_VERSION;
    }

    /// Clamp every value into a safe range.
    pub fn validate(&mut self) {
        self.max_ram_mib = sysmem::clamp_max_mib(self.max_ram_mib, sysmem::total_mib());
        self.window_width = self.window_width.clamp(640, 7680);
        self.window_height = self.window_height.clamp(480, 4320);
        self.download_threads = self.download_threads.clamp(1, 16);
        self.offline_name = sanitize_name(&self.offline_name);
        self.msa_client_id = self.msa_client_id.trim().to_string();
        self.custom_java_path = self.custom_java_path.trim().to_string();
    }

    /// Extra JVM args split on whitespace. Arguments that would override
    /// what the launcher controls (classpath, natives) are dropped.
    pub fn extra_jvm_args(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut it = self.extra_jvm_args.split_whitespace();
        while let Some(a) = it.next() {
            if a == "-cp" || a == "-classpath" {
                log::warn!("ignoring extra JVM argument controlled by the launcher: {a}");
                it.next(); // drop its value too
                continue;
            }
            if a.starts_with("-Djava.library.path")
                || a.starts_with("-Xmx")
                || a.starts_with("-Xms")
            {
                log::warn!("ignoring extra JVM argument controlled by the launcher: {a}");
                continue;
            }
            out.push(a.to_string());
        }
        out
    }
}

/// Minecraft usernames: 3-16 chars of [A-Za-z0-9_].
pub fn sanitize_name(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(16)
        .collect();
    if s.len() < 3 { "Player".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_partial() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("launcher.json");
        assert_eq!(LauncherConfig::load(&p).config_version, CONFIG_VERSION);

        std::fs::write(
            &p,
            br#"{"window_width": 99999, "offline_name": "a b!c_dEfghijklmnopqrstuvwxyz"}"#,
        )
        .unwrap();
        let c = LauncherConfig::load(&p);
        assert_eq!(c.window_width, 7680);
        assert_eq!(c.offline_name, "abc_dEfghijklmno");
        assert!(c.use_managed_java);

        c.save(&p).unwrap();
        let c2 = LauncherConfig::load(&p);
        assert_eq!(c2.offline_name, c.offline_name);
    }

    #[test]
    fn older_config_without_update_fields_keeps_settings() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("launcher.json");
        // A launcher.json written by 0.1.0 (before the updater existed).
        std::fs::write(&p, br#"{"config_version":1,"max_ram_mib":3072,"offline_name":"Kellen","game_preset":"visual"}"#).unwrap();
        let c = LauncherConfig::load(&p);
        assert_eq!(
            c.max_ram_mib,
            sysmem::clamp_max_mib(3072, sysmem::total_mib())
        );
        assert_eq!(c.offline_name, "Kellen");
        assert_eq!(c.game_preset, GamePreset::Visual);
        assert!(c.update_auto_check);
        assert!(c.update_dismissed_version.is_empty());
    }

    #[test]
    fn malformed_is_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("launcher.json");
        std::fs::write(&p, b"{not json").unwrap();
        let c = LauncherConfig::load(&p);
        assert_eq!(c.game_preset, GamePreset::Competitive);
        assert_eq!(
            std::fs::read(dir.path().join("launcher.json.bad")).unwrap(),
            b"{not json"
        );
    }

    #[test]
    fn blocks_controlled_jvm_args() {
        let c = LauncherConfig {
            extra_jvm_args: "-XX:+UseG1GC -Xmx99G -cp x -Dfoo=bar".into(),
            ..Default::default()
        };
        assert_eq!(c.extra_jvm_args(), vec!["-XX:+UseG1GC", "-Dfoo=bar"]);
    }
}
