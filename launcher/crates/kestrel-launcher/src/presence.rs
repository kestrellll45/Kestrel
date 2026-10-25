//! Maps the launcher's state to Discord Rich Presence.
//!
//! Only coarse, non-private states are shown: launcher open, Minecraft
//! starting, main menu, singleplayer, multiplayer. Server addresses, world
//! names and account details are never included.

use kestrel_core::discord::{Desired, Presence, validate_client_id};
use kestrel_core::launch::GameActivity;
use kestrel_core::{LAUNCHER_VERSION, config::LauncherConfig};

/// Art asset key the Discord application must have (Rich Presence → Art Assets).
pub const LARGE_IMAGE_KEY: &str = "kestrel";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Launcher open, Minecraft not running.
    Launcher,
    /// Downloading/verifying/starting Minecraft.
    Starting,
    /// Minecraft process running; `None` until the mod reports a state.
    Running(Option<GameActivity>),
}

/// What Discord should show. `launcher_start` / `game_start` are Unix seconds.
pub fn desired(
    cfg: &LauncherConfig,
    phase: Phase,
    launcher_start: u64,
    game_start: Option<u64>,
) -> Desired {
    let client_id = if cfg.discord_rpc_enabled {
        validate_client_id(&cfg.discord_client_id).ok()
    } else {
        None
    };
    let (details, start) = match phase {
        Phase::Launcher => ("In the launcher", launcher_start),
        Phase::Starting => (
            "Starting Minecraft 1.8.9",
            game_start.unwrap_or(launcher_start),
        ),
        Phase::Running(a) => (
            match a {
                None => "Playing Minecraft 1.8.9",
                Some(GameActivity::MainMenu) => "In the main menu",
                Some(GameActivity::Singleplayer) => "Playing singleplayer",
                Some(GameActivity::Multiplayer) => "Playing multiplayer",
            },
            game_start.unwrap_or(launcher_start),
        ),
    };
    Desired {
        client_id,
        presence: Some(Presence {
            details: details.into(),
            state: Some("Minecraft 1.8.9".into()),
            start_unix: Some(start),
            large_image: Some(LARGE_IMAGE_KEY.into()),
            large_text: Some(format!("Kestrel {LAUNCHER_VERSION}")),
        }),
    }
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(enabled: bool, id: &str) -> LauncherConfig {
        LauncherConfig {
            discord_rpc_enabled: enabled,
            discord_client_id: id.into(),
            ..LauncherConfig::default()
        }
    }

    const ID: &str = "123456789012345678";

    #[test]
    fn states_map_to_honest_text() {
        let c = cfg(true, ID);
        let d = |p| desired(&c, p, 100, Some(200)).presence.unwrap();
        assert_eq!(d(Phase::Launcher).details, "In the launcher");
        assert_eq!(d(Phase::Launcher).start_unix, Some(100));
        assert_eq!(d(Phase::Starting).details, "Starting Minecraft 1.8.9");
        assert_eq!(d(Phase::Running(None)).details, "Playing Minecraft 1.8.9");
        assert_eq!(
            d(Phase::Running(Some(GameActivity::MainMenu))).details,
            "In the main menu"
        );
        assert_eq!(
            d(Phase::Running(Some(GameActivity::Singleplayer))).details,
            "Playing singleplayer"
        );
        assert_eq!(
            d(Phase::Running(Some(GameActivity::Multiplayer))).details,
            "Playing multiplayer"
        );
        assert_eq!(
            d(Phase::Running(Some(GameActivity::Multiplayer))).start_unix,
            Some(200)
        );
        assert_eq!(
            desired(&c, Phase::Launcher, 1, None).client_id.as_deref(),
            Some(ID)
        );
    }

    #[test]
    fn disabled_or_unconfigured_means_no_connection() {
        assert_eq!(
            desired(&cfg(false, ID), Phase::Launcher, 1, None).client_id,
            None
        );
        assert_eq!(
            desired(&cfg(true, ""), Phase::Launcher, 1, None).client_id,
            None
        );
        assert_eq!(
            desired(&cfg(true, "not-an-id"), Phase::Launcher, 1, None).client_id,
            None
        );
    }

    #[test]
    fn nothing_private_is_sent() {
        let p = desired(
            &cfg(true, ID),
            Phase::Running(Some(GameActivity::Multiplayer)),
            1,
            Some(2),
        )
        .presence
        .unwrap();
        let all = format!("{p:?}");
        for forbidden in ["hypixel", "127.0.0.1", ".net", "saves", "uuid"] {
            assert!(
                !all.to_lowercase().contains(forbidden),
                "{forbidden} in {all}"
            );
        }
    }
}
