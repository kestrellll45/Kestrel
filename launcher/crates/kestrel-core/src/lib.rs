//! Kestrel launcher core: installation, verification, authentication and
//! launch of Minecraft 1.8.9 + Forge + the Kestrel client mod.
//!
//! Everything in here is UI-agnostic and blocking; the GUI runs it on worker
//! threads and observes [`progress::Progress`].

pub mod assets;
pub mod auth;
pub mod config;
pub mod cosmetics;
pub mod discord;
pub mod download;
pub mod error;
pub mod forge;
pub mod hash;
pub mod install;
pub mod launch;
pub mod maven;
pub mod mods;
pub mod mojang;
pub mod natives;
pub mod paths;
pub mod progress;
pub mod redact;
pub mod resourcepacks;
pub mod runtime;
pub mod secure;
pub mod skins;
pub mod sysmem;
pub mod update;

pub use error::{Error, Result};

/// Minecraft version this launcher targets.
pub const MC_VERSION: &str = "1.8.9";
/// Forge build used for the client (the recommended, final 1.8.9 build).
pub const FORGE_VERSION: &str = "1.8.9-11.15.1.2318-1.8.9";
/// Launcher version reported to the game via `minecraft.launcher.version`.
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Brand reported to the game via `minecraft.launcher.brand`.
pub const LAUNCHER_BRAND: &str = "kestrel";
