//! Cosmetics shared between the launcher and the client mod.
//!
//! # Communication with the mod
//!
//! The mod owns `instance/kestrel/config.json` and rewrites it whenever you
//! change something in-game, so the launcher never writes that file (two
//! writers would overwrite each other). Instead the launcher writes a
//! *request* file, `instance/kestrel/launcher-cosmetics.json`, containing the
//! cosmetic settings plus a monotonically increasing `revision`. The mod
//! applies a request once (at start-up, and within ~1 s while running),
//! records `lastLauncherCosmeticsRevision` in its config, and saves.
//!
//! The launcher shows: the pending request if it has not been applied yet,
//! otherwise what the mod's config says is equipped.
//!
//! # Assets
//!
//! Uploaded images are fully decoded and validated with the same rules the
//! mod enforces, then copied (never referenced in place) into
//! `instance/kestrel/capes` or `instance/kestrel/skies` under a sanitised
//! file name. Capes may be PNG or GIF; skies are PNG.
//!
//! # Cape catalog
//!
//! Bundled capes are listed in the mod's `assets/kestrel/capes.json`, which
//! is compiled into both the mod jar and this crate, so the two can never
//! disagree. A catalog cape with `owners` is restricted to those exact
//! Minecraft UUIDs (usernames are never used); the launcher only offers it
//! to a matching signed-in account, and the mod independently refuses to
//! render it for anyone else.
//!
//! Everything here is local: your capes, skies and effects are rendered by
//! your own client. The only exception is that Kestrel clients draw a
//! restricted catalog cape on the player who owns it.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{Error, IoContext, Result};
use crate::hash;
use crate::paths::{Layout, atomic_write};

pub const CUSTOM_PREFIX: &str = "custom:";
pub const CAPE_NONE: &str = "none";
/// PNG capes (mod: `CapeManager.MAX_FILE_BYTES`).
pub const MAX_CAPE_BYTES: u64 = 2 * 1024 * 1024;
/// GIF capes (mod: `GifDecoder.CAPE_LIMITS`).
pub const MAX_CAPE_GIF_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_SKY_BYTES: u64 = 16 * 1024 * 1024;
/// GIF limits, identical to the mod's `GifDecoder.CAPE_LIMITS`.
const GIF_MAX_SIDE: u32 = 1024;
const GIF_MAX_FRAMES: u32 = 128;
/// Decoded texels kept per cape (mod: `CapeManager.MAX_TEXELS_PER_CAPE`).
const MAX_TEXELS_PER_CAPE: u64 = 12 * 1024 * 1024;
/// A GIF that is not a cape sheet is fitted onto a 512x256 texture
/// (mod: `CapeManager.CUSTOM_GIF_SCALE` = 8).
const FACES_TEXELS: u64 = 512 * 256;

/// The mod's cape catalog, compiled in so the launcher and mod agree.
const CAPE_CATALOG_JSON: &str =
    include_str!("../../../../client/src/main/resources/assets/kestrel/capes.json");

/// A bundled cape from `capes.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct CapeEntry {
    pub id: String,
    pub name: String,
    /// Path under `assets/kestrel/textures/`.
    pub texture: String,
    /// Static PNG used where the animation cannot be (e.g. previews).
    #[serde(default)]
    pub fallback: Option<String>,
    /// Owner UUIDs; `None` = available to everyone. Kept private so callers
    /// go through [`CapeEntry::allows`].
    #[serde(default)]
    owners: Option<Vec<String>>,
}

impl CapeEntry {
    pub fn restricted(&self) -> bool {
        self.owners.is_some()
    }

    /// Whether the account with this UUID may wear the cape. Only the UUID is
    /// compared (normalised: case- and dash-insensitive); `None` (offline or
    /// signed out) gets unrestricted capes only. An empty owner list allows
    /// nobody.
    pub fn allows(&self, uuid: Option<&str>) -> bool {
        let Some(owners) = &self.owners else {
            return true;
        };
        let Some(me) = uuid.and_then(normalize_uuid) else {
            return false;
        };
        owners
            .iter()
            .filter_map(|o| normalize_uuid(o))
            .any(|o| o == me)
    }
}

#[derive(Deserialize)]
struct CatalogFile {
    capes: Vec<CapeEntry>,
}

/// Parse and check a cape catalog (unique ids, valid owner UUIDs).
pub fn parse_catalog(json: &str) -> std::result::Result<Vec<CapeEntry>, String> {
    let file: CatalogFile = serde_json::from_str(json).map_err(|e| format!("capes.json: {e}"))?;
    let mut seen = std::collections::HashSet::new();
    for c in &file.capes {
        if c.id.is_empty() || c.id == CAPE_NONE || c.id.starts_with(CUSTOM_PREFIX) {
            return Err(format!("capes.json: invalid id '{}'", c.id));
        }
        if !seen.insert(c.id.as_str()) {
            return Err(format!("capes.json: duplicate id '{}'", c.id));
        }
        if let Some(owners) = &c.owners
            && let Some(bad) = owners.iter().find(|o| normalize_uuid(o).is_none())
        {
            return Err(format!("capes.json: {}: invalid owner UUID '{bad}'", c.id));
        }
    }
    Ok(file.capes)
}

/// The bundled cape catalog. A broken catalog (caught by tests) yields an
/// empty list rather than a crash.
pub fn cape_catalog() -> &'static [CapeEntry] {
    static CATALOG: std::sync::OnceLock<Vec<CapeEntry>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        parse_catalog(CAPE_CATALOG_JSON).unwrap_or_else(|e| {
            log::error!("{e}");
            Vec::new()
        })
    })
}

/// Canonical Minecraft UUID: 32 lower-case hex digits without dashes.
/// Accepts the dashed 8-4-4-4-12 form or 32 hex digits, any case.
pub fn normalize_uuid(s: &str) -> Option<String> {
    let t = s.trim();
    let hex: String = if t.len() == 36 {
        let b = t.as_bytes();
        if b[8] != b'-' || b[13] != b'-' || b[18] != b'-' || b[23] != b'-' {
            return None;
        }
        t.chars().filter(|&c| c != '-').collect()
    } else {
        t.to_string()
    };
    if hex.len() != 32 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(hex.to_ascii_lowercase())
}

/// Bundled skies (id, display name). Must match `SkyManager.BUILTIN`.
pub const BUILTIN_SKIES: &[(&str, &str)] = &[
    ("dusk", "Dusk"),
    ("starry", "Starry Night"),
    ("nebula", "Nebula"),
];

/// Kill effects (id, name, description). Must match `KillEffectType`.
pub const KILL_EFFECTS: &[(&str, &str, &str)] = &[
    (
        "burst",
        "Particle burst",
        "A quick burst of sparks and magic crits.",
    ),
    (
        "ring",
        "Particle ring",
        "An expanding ring that rises around the victim.",
    ),
    (
        "helix",
        "Colour helix",
        "Two coloured dust spirals climbing upward.",
    ),
    (
        "dust",
        "Colour burst",
        "A sphere of dust in your chosen colour.",
    ),
    (
        "explosion",
        "Explosion",
        "Large explosion puffs and smoke (visual only).",
    ),
    (
        "lightning",
        "Lightning",
        "A client-drawn lightning bolt with sparks.",
    ),
    ("hearts", "Hearts", "A fountain of hearts."),
    ("flames", "Flame pillar", "A spiralling column of flames."),
    (
        "arcane",
        "Arcane",
        "Enchanting glyphs swirl into the victim.",
    ),
];

/// Cosmetic settings the launcher manages. Field names follow the mod's
/// JSON (camelCase); defaults mirror `ClientConfig`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapeSettings {
    pub capes_enabled: bool,
    pub cape: String,
    pub animated_capes: bool,
    /// Draw catalog capes on the players they are assigned to (by UUID).
    #[serde(default = "default_true")]
    pub show_assigned_capes: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkySettings {
    pub enabled: bool,
    pub preset: String,
    pub brightness: f32,
    pub rotate: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KillEffectSettings {
    pub enabled: bool,
    pub effect: String,
    pub intensity: f32,
    pub sounds: bool,
    /// Opaque ARGB colour used by the colour effects.
    pub color: u32,
}

/// QoL settings carried by the same launcher request (QoL page).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QolSettings {
    /// Full lightmap brightness in game (`ClientConfig.Qol.fullbright`).
    #[serde(default)]
    pub fullbright: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CosmeticSettings {
    pub cosmetics: CapeSettings,
    pub sky: SkySettings,
    #[serde(rename = "killEffects")]
    pub kill_effects: KillEffectSettings,
    /// Older request files have no `qol` section.
    #[serde(default)]
    pub qol: QolSettings,
}

impl Default for CosmeticSettings {
    fn default() -> Self {
        CosmeticSettings {
            cosmetics: CapeSettings {
                capes_enabled: true,
                cape: CAPE_NONE.into(),
                animated_capes: true,
                show_assigned_capes: true,
            },
            sky: SkySettings {
                enabled: false,
                preset: "dusk".into(),
                brightness: 1.0,
                rotate: true,
            },
            kill_effects: KillEffectSettings {
                enabled: true,
                effect: "burst".into(),
                intensity: 1.0,
                sounds: true,
                color: 0xFFE8_833A,
            },
            qol: QolSettings::default(),
        }
    }
}

impl CosmeticSettings {
    /// Clamp/repair values so the request is always valid for the mod.
    pub fn validate(&mut self) {
        if self.cosmetics.cape.is_empty() {
            self.cosmetics.cape = CAPE_NONE.into();
        }
        if self.sky.preset.is_empty() {
            self.sky.preset = "dusk".into();
        }
        if !KILL_EFFECTS
            .iter()
            .any(|(id, _, _)| *id == self.kill_effects.effect)
        {
            self.kill_effects.effect = "burst".into();
        }
        self.sky.brightness = clamp_f(self.sky.brightness, 0.1, 1.0, 1.0);
        self.kill_effects.intensity = clamp_f(self.kill_effects.intensity, 0.25, 2.0, 1.0);
        self.kill_effects.color |= 0xFF00_0000;
    }
}

fn clamp_f(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_nan() { default } else { v.clamp(lo, hi) }
}

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    revision: u64,
    #[serde(flatten)]
    settings: CosmeticSettings,
}

pub fn client_config_path(layout: &Layout) -> PathBuf {
    layout.client_data().join("config.json")
}

pub fn request_path(layout: &Layout) -> PathBuf {
    layout.client_data().join("launcher-cosmetics.json")
}

pub fn capes_dir(layout: &Layout) -> PathBuf {
    layout.client_data().join("capes")
}

pub fn skies_dir(layout: &Layout) -> PathBuf {
    layout.client_data().join("skies")
}

/// Current cosmetic state and whether a launcher request is still pending.
#[derive(Debug, Clone)]
pub struct CosmeticState {
    pub settings: CosmeticSettings,
    /// True when the launcher's latest request has not been applied by the
    /// mod yet (it applies on next start, or within ~1 s while running).
    pub pending: bool,
}

/// Read what is equipped: the pending request if newer than what the mod
/// applied, otherwise the mod's config. Never fails; malformed files fall
/// back to defaults.
pub fn load_state(layout: &Layout) -> CosmeticState {
    let config: Option<Value> = std::fs::read(client_config_path(layout))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let applied_rev = config
        .as_ref()
        .and_then(|c| c.get("lastLauncherCosmeticsRevision"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let request: Option<Request> = std::fs::read(request_path(layout))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    if let Some(r) = request
        && r.revision > applied_rev
    {
        let mut settings = r.settings;
        settings.validate();
        return CosmeticState {
            settings,
            pending: true,
        };
    }
    let mut settings = config
        .as_ref()
        .map(settings_from_config)
        .unwrap_or_default();
    settings.validate();
    CosmeticState {
        settings,
        pending: false,
    }
}

/// Extract cosmetic settings from the mod's config JSON, keeping defaults
/// for anything missing or of the wrong type.
fn settings_from_config(c: &Value) -> CosmeticSettings {
    let d = CosmeticSettings::default();
    let obj = |k: &str| c.get(k).and_then(Value::as_object);
    let b = |o: Option<&Map<String, Value>>, k: &str, def: bool| {
        o.and_then(|o| o.get(k))
            .and_then(Value::as_bool)
            .unwrap_or(def)
    };
    let s = |o: Option<&Map<String, Value>>, k: &str, def: &str| {
        o.and_then(|o| o.get(k))
            .and_then(Value::as_str)
            .unwrap_or(def)
            .to_string()
    };
    let f = |o: Option<&Map<String, Value>>, k: &str, def: f32| {
        o.and_then(|o| o.get(k))
            .and_then(Value::as_f64)
            .map(|v| v as f32)
            .unwrap_or(def)
    };
    let cos = obj("cosmetics");
    let sky = obj("sky");
    let ke = obj("killEffects");
    let qol = obj("qol");
    CosmeticSettings {
        cosmetics: CapeSettings {
            capes_enabled: b(cos, "capesEnabled", d.cosmetics.capes_enabled),
            cape: s(cos, "cape", &d.cosmetics.cape),
            animated_capes: b(cos, "animatedCapes", d.cosmetics.animated_capes),
            show_assigned_capes: b(cos, "showAssignedCapes", d.cosmetics.show_assigned_capes),
        },
        sky: SkySettings {
            enabled: b(sky, "enabled", d.sky.enabled),
            preset: s(sky, "preset", &d.sky.preset),
            brightness: f(sky, "brightness", d.sky.brightness),
            rotate: b(sky, "rotate", d.sky.rotate),
        },
        kill_effects: KillEffectSettings {
            enabled: b(ke, "enabled", d.kill_effects.enabled),
            effect: s(ke, "effect", &d.kill_effects.effect),
            intensity: f(ke, "intensity", d.kill_effects.intensity),
            sounds: b(ke, "sounds", d.kill_effects.sounds),
            // Java ints are signed: 0xFFE8833A is stored as a negative number.
            color: ke
                .and_then(|o| o.get("color"))
                .and_then(Value::as_i64)
                .map(|v| v as u32)
                .unwrap_or(d.kill_effects.color),
        },
        qol: QolSettings {
            fullbright: b(qol, "fullbright", d.qol.fullbright),
        },
    }
}

/// Save settings as a new launcher request (atomic write).
pub fn save_request(layout: &Layout, settings: &CosmeticSettings) -> Result<()> {
    let mut settings = settings.clone();
    settings.validate();
    let previous = std::fs::read(request_path(layout))
        .ok()
        .and_then(|b| serde_json::from_slice::<Request>(&b).ok())
        .map(|r| r.revision)
        .unwrap_or(0);
    // Millisecond timestamp, but always strictly increasing.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let revision = now.max(previous + 1);
    let data = serde_json::to_vec_pretty(&Request { revision, settings })
        .map_err(|e| Error::json("cosmetics request", e))?;
    atomic_write(&request_path(layout), &data)
}

/// Turn Fullbright on or off (QoL page). Re-reads the current state first so
/// cosmetics changed in game since the page was opened are not reverted.
pub fn save_fullbright(layout: &Layout, on: bool) -> Result<CosmeticState> {
    let mut state = load_state(layout);
    state.settings.qol.fullbright = on;
    save_request(layout, &state.settings)?;
    Ok(load_state(layout))
}

// ---- image validation -------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Cape,
    Sky,
}

impl Kind {
    /// Largest file of any accepted format (GIF capes may be larger than PNGs).
    fn max_bytes(self) -> u64 {
        match self {
            Kind::Cape => MAX_CAPE_GIF_BYTES,
            Kind::Sky => MAX_SKY_BYTES,
        }
    }

    fn max_png_bytes(self) -> u64 {
        match self {
            Kind::Cape => MAX_CAPE_BYTES,
            Kind::Sky => MAX_SKY_BYTES,
        }
    }
}

/// Decoded RGBA8 image.
#[derive(Debug, Clone)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Validate dimensions with exactly the mod's rules (`ImageValidation.java`).
/// Returns the number of animation frames for capes (1 for skies).
pub fn validate_dimensions(kind: Kind, w: u32, h: u32) -> std::result::Result<u32, String> {
    match kind {
        Kind::Cape => {
            if !(64..=1024).contains(&w) || !w.is_multiple_of(64) {
                return Err(format!(
                    "cape width must be a multiple of 64 between 64 and 1024 (got {w}x{h})"
                ));
            }
            let frame_h = w / 2;
            if !h.is_multiple_of(frame_h) {
                return Err(format!(
                    "cape height must be a multiple of width/2, e.g. 64x32 or 128x64 (got {w}x{h})"
                ));
            }
            if h > 2048 {
                return Err(format!("cape image is too tall (max 2048 px, got {w}x{h})"));
            }
            let frames = h / frame_h;
            if !(1..=64).contains(&frames) {
                return Err(format!(
                    "animated capes may have at most 64 frames (got {frames})"
                ));
            }
            Ok(frames)
        }
        Kind::Sky => {
            if w * 2 != h * 3 {
                return Err(format!(
                    "sky image must be a 3x2 grid of square faces, i.e. 3:2 (got {w}x{h})"
                ));
            }
            if !(384..=3072).contains(&w) {
                return Err(format!(
                    "sky width must be between 384 and 3072 (got {w}x{h})"
                ));
            }
            Ok(1)
        }
    }
}

/// Decode PNG bytes to RGBA8 with a hard memory limit.
pub fn decode_png(bytes: &[u8]) -> std::result::Result<Rgba, String> {
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err("not a PNG image (only .png files are supported)".into());
    }
    let mut dec = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: 64 * 1024 * 1024,
        },
    );
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().map_err(|e| format!("corrupt PNG: {e}"))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| "PNG is too large".to_string())?;
    let mut buf = vec![0u8; size];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("corrupt PNG: {e}"))?;
    buf.truncate(info.buffer_size());
    let (w, h) = (info.width, info.height);
    let px = (w as usize) * (h as usize);
    let pixels = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[g, a]| [g, g, g, a])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        other => return Err(format!("unsupported PNG colour type {other:?}")),
    };
    if pixels.len() != px * 4 {
        return Err("PNG decoded to an unexpected size".into());
    }
    Ok(Rgba {
        width: w,
        height: h,
        pixels,
    })
}

pub fn is_gif(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}

/// A GIF in the cape sheet layout (2:1, width a multiple of 64, <= 1024);
/// the mod fits any other GIF onto both faces of the cape.
pub fn is_cape_sheet(w: u32, h: u32) -> bool {
    w >= 64 && w.is_multiple_of(64) && w == h * 2 && w <= 1024
}

/// Validate a cape GIF with the mod's rules (`GifDecoder` + `CapeManager`):
/// size, logical screen, every frame's rectangle, frame count, decoded
/// texture budget, and a full decode of every frame. Returns frame 0 on a
/// transparent canvas of the logical screen size.
pub fn validate_cape_gif(bytes: &[u8]) -> std::result::Result<Rgba, String> {
    if bytes.len() as u64 > MAX_CAPE_GIF_BYTES {
        return Err(format!(
            "GIF is too large ({} KB, max {} KB)",
            bytes.len() / 1024,
            MAX_CAPE_GIF_BYTES / 1024
        ));
    }
    if !is_gif(bytes) {
        return Err("not a GIF image".into());
    }
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let frame_limit = u64::from(GIF_MAX_SIDE) * u64::from(GIF_MAX_SIDE) * 4;
    if let Some(n) = std::num::NonZeroU64::new(frame_limit) {
        opts.set_memory_limit(gif::MemoryLimit::Bytes(n));
    }
    let mut dec = opts
        .read_info(Cursor::new(bytes))
        .map_err(|e| format!("corrupt GIF: {e}"))?;
    let (w, h) = (u32::from(dec.width()), u32::from(dec.height()));
    if w == 0 || h == 0 {
        return Err("GIF has invalid dimensions".into());
    }
    if w > GIF_MAX_SIDE || h > GIF_MAX_SIDE {
        return Err(format!(
            "GIF is too large ({w}x{h}, max {GIF_MAX_SIDE} px per side)"
        ));
    }
    let texels_per_frame = if is_cape_sheet(w, h) {
        u64::from(w) * u64::from(h)
    } else {
        FACES_TEXELS
    };
    let mut first: Option<Rgba> = None;
    let mut frames = 0u32;
    let mut buf = Vec::new();
    loop {
        let (fx, fy, fw, fh) = match dec
            .next_frame_info()
            .map_err(|e| format!("corrupt GIF: {e}"))?
        {
            None => break,
            Some(f) => (
                u32::from(f.left),
                u32::from(f.top),
                u32::from(f.width),
                u32::from(f.height),
            ),
        };
        frames += 1;
        if frames > GIF_MAX_FRAMES {
            return Err(format!("GIF has too many frames (max {GIF_MAX_FRAMES})"));
        }
        if fw == 0 || fh == 0 || fw > GIF_MAX_SIDE || fh > GIF_MAX_SIDE {
            return Err(format!("corrupt GIF: frame {} is {fw}x{fh}", frames - 1));
        }
        if u64::from(frames) * texels_per_frame > MAX_TEXELS_PER_CAPE {
            return Err("animated cape is too large (too many frames for its size)".into());
        }
        buf.resize(dec.buffer_size(), 0);
        dec.read_into_buffer(&mut buf)
            .map_err(|e| format!("corrupt GIF: {e}"))?;
        if first.is_none() {
            first = Some(place_frame(w, h, fx, fy, fw, fh, &buf));
        }
    }
    first.ok_or_else(|| "GIF has no frames".to_string())
}

/// Draw an RGBA frame rectangle onto a transparent w x h canvas (clipped).
fn place_frame(w: u32, h: u32, fx: u32, fy: u32, fw: u32, fh: u32, rgba: &[u8]) -> Rgba {
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    for y in 0..fh.min(h.saturating_sub(fy)) {
        for x in 0..fw.min(w.saturating_sub(fx)) {
            let src = ((y * fw + x) * 4) as usize;
            let dst = (((fy + y) * w + (fx + x)) * 4) as usize;
            if rgba.len() >= src + 4 && rgba[src + 3] != 0 {
                pixels[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
            }
        }
    }
    Rgba {
        width: w,
        height: h,
        pixels,
    }
}

/// Fully validate image bytes for a cosmetic kind. For GIF capes this
/// returns the first frame.
pub fn validate_image(kind: Kind, bytes: &[u8]) -> std::result::Result<Rgba, String> {
    if kind == Kind::Cape && is_gif(bytes) {
        return validate_cape_gif(bytes);
    }
    if bytes.len() as u64 > kind.max_png_bytes() {
        return Err(format!(
            "file is too large ({} KB, max {} KB)",
            bytes.len() / 1024,
            kind.max_png_bytes() / 1024
        ));
    }
    // Check dimensions from the header before allocating for pixels.
    if bytes.len() >= 24 && &bytes[12..16] == b"IHDR" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        validate_dimensions(kind, w, h)?;
    }
    let img = decode_png(bytes)?;
    validate_dimensions(kind, img.width, img.height)?;
    Ok(img)
}

/// Turn a user file name into a safe stem: `[A-Za-z0-9_-]`, max 40 chars
/// (the extension is chosen from the validated content).
pub fn sanitize_file_stem(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let mut s: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    s = s.trim_matches('_').chars().take(40).collect();
    if s.is_empty() { "custom".into() } else { s }
}

/// Validate `src` and copy it into the cape/sky folder. Returns the
/// cosmetic id (`custom:<file>`). Re-importing an identical file returns
/// the existing id instead of creating a duplicate.
pub fn import(layout: &Layout, kind: Kind, src: &Path) -> Result<String> {
    let meta = std::fs::metadata(src).at(src)?;
    if !meta.is_file() {
        return Err(Error::Other(format!("{} is not a file", src.display())));
    }
    if meta.len() > kind.max_bytes() {
        return Err(Error::Other(format!(
            "file is too large ({} KB, max {} KB)",
            meta.len() / 1024,
            kind.max_bytes() / 1024
        )));
    }
    let bytes = std::fs::read(src).at(src)?;
    validate_image(kind, &bytes).map_err(Error::Other)?;
    let ext = if kind == Kind::Cape && is_gif(&bytes) {
        "gif"
    } else {
        "png"
    };

    let dir = match kind {
        Kind::Cape => capes_dir(layout),
        Kind::Sky => skies_dir(layout),
    };
    std::fs::create_dir_all(&dir).at(&dir)?;
    let stem = sanitize_file_stem(
        &src.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    );
    let new_hash = hash::sha1_bytes(&bytes);
    for n in 1..1000 {
        let name = if n == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}-{n}.{ext}")
        };
        let dest = dir.join(&name);
        if dest.exists() {
            if hash::file_matches(&dest, Some(&new_hash), Some(bytes.len() as u64)) {
                return Ok(format!("{CUSTOM_PREFIX}{name}"));
            }
            continue;
        }
        atomic_write(&dest, &bytes)?;
        log::info!("imported {kind:?} {} as {name}", src.display());
        return Ok(format!("{CUSTOM_PREFIX}{name}"));
    }
    Err(Error::Other("too many files with this name".into()))
}

/// Custom files currently in the cape or sky folder (sorted).
pub fn list_custom(layout: &Layout, kind: Kind) -> Vec<String> {
    let dir = match kind {
        Kind::Cape => capes_dir(layout),
        Kind::Sky => skies_dir(layout),
    };
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_file())
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .filter(|n| {
                    let n = n.to_ascii_lowercase();
                    n.ends_with(".png") || (kind == Kind::Cape && n.ends_with(".gif"))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Path of a custom cosmetic file, refusing anything that is not a plain
/// file name inside the folder.
pub fn custom_path(layout: &Layout, kind: Kind, id: &str) -> Option<PathBuf> {
    let name = id.strip_prefix(CUSTOM_PREFIX)?;
    if name.is_empty() || name.contains(['/', '\\', ':']) || name.contains("..") {
        return None;
    }
    let dir = match kind {
        Kind::Cape => capes_dir(layout),
        Kind::Sky => skies_dir(layout),
    };
    Some(dir.join(name))
}

/// Delete an uploaded cosmetic.
pub fn remove_custom(layout: &Layout, kind: Kind, id: &str) -> Result<()> {
    let p = custom_path(layout, kind, id)
        .ok_or_else(|| Error::Other(format!("not a custom cosmetic: {id}")))?;
    std::fs::remove_file(&p).at(&p)
}

// ---- previews ---------------------------------------------------------------

/// Outer face of a cape (10x16 texels in 64x32 units) from frame 0.
pub fn cape_preview(img: &Rgba) -> Rgba {
    let s = img.width / 64; // texels per cape unit
    crop(img, s, s, 10 * s, 16 * s)
}

/// Preview for validated cape bytes: the outer face for PNGs and GIF cape
/// sheets; for other GIFs (fitted onto the faces by the mod) the first
/// frame, centre-cropped to the face's 10:16 shape.
pub fn cape_preview_bytes(bytes: &[u8]) -> std::result::Result<Rgba, String> {
    let img = validate_image(Kind::Cape, bytes)?;
    if is_gif(bytes) && !is_cape_sheet(img.width, img.height) {
        let (w, h) = (img.width, img.height);
        let (cw, ch) = if w * 16 > h * 10 {
            ((h * 10 / 16).max(1), h)
        } else {
            (w, (w * 16 / 10).max(1))
        };
        return Ok(crop(&img, (w - cw) / 2, (h - ch) / 2, cw, ch));
    }
    Ok(cape_preview(&img))
}

/// The north/east/south strip of a sky atlas, downscaled to <= `max_w` wide.
pub fn sky_preview(img: &Rgba, max_w: u32) -> Rgba {
    let strip = crop(img, 0, 0, img.width, img.height / 2);
    downscale(&strip, max_w)
}

fn crop(img: &Rgba, x: u32, y: u32, w: u32, h: u32) -> Rgba {
    let w = w.min(img.width.saturating_sub(x));
    let h = h.min(img.height.saturating_sub(y));
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for row in y..y + h {
        let start = ((row * img.width + x) * 4) as usize;
        pixels.extend_from_slice(&img.pixels[start..start + (w * 4) as usize]);
    }
    Rgba {
        width: w,
        height: h,
        pixels,
    }
}

/// Box-filter downscale by an integer factor so the result is <= max_w wide.
fn downscale(img: &Rgba, max_w: u32) -> Rgba {
    let f = img.width.div_ceil(max_w.max(1)).max(1);
    if f == 1 {
        return img.clone();
    }
    let (w, h) = (img.width / f, img.height / f);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0u32; 4];
            for dy in 0..f {
                for dx in 0..f {
                    let i = (((y * f + dy) * img.width + (x * f + dx)) * 4) as usize;
                    for (c, a) in acc.iter_mut().enumerate() {
                        *a += img.pixels[i + c] as u32;
                    }
                }
            }
            pixels.extend(acc.iter().map(|a| (a / (f * f)) as u8));
        }
    }
    Rgba {
        width: w,
        height: h,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().unwrap();
            let data: Vec<u8> = (0..w * h)
                .flat_map(|i| [(i % 256) as u8, 10, 20, 255])
                .collect();
            wr.write_image_data(&data).unwrap();
        }
        out
    }

    fn layout() -> (tempfile::TempDir, Layout) {
        let d = tempfile::tempdir().unwrap();
        let l = Layout::new(d.path().join("root"));
        l.ensure().unwrap();
        (d, l)
    }

    #[test]
    fn dimension_rules_match_mod() {
        assert_eq!(validate_dimensions(Kind::Cape, 64, 32), Ok(1));
        assert_eq!(validate_dimensions(Kind::Cape, 64, 256), Ok(8));
        assert_eq!(validate_dimensions(Kind::Cape, 128, 64), Ok(1));
        assert!(validate_dimensions(Kind::Cape, 63, 32).is_err());
        assert!(validate_dimensions(Kind::Cape, 64, 33).is_err());
        assert!(validate_dimensions(Kind::Cape, 2048, 1024).is_err());
        assert!(validate_dimensions(Kind::Cape, 1024, 4096).is_err());
        assert!(validate_dimensions(Kind::Sky, 1536, 1024).is_ok());
        assert!(validate_dimensions(Kind::Sky, 1024, 1024).is_err());
        assert!(validate_dimensions(Kind::Sky, 6144, 4096).is_err());
    }

    #[test]
    fn import_validates_copies_and_dedupes() {
        let (d, l) = layout();
        let src = d.path().join("My Cool Cape!!.png");
        std::fs::write(&src, png_bytes(64, 32)).unwrap();
        let id = import(&l, Kind::Cape, &src).unwrap();
        assert_eq!(id, "custom:My_Cool_Cape.png");
        assert!(capes_dir(&l).join("My_Cool_Cape.png").is_file());
        // Same content again: same id, no duplicate file.
        assert_eq!(import(&l, Kind::Cape, &src).unwrap(), id);
        // Different content, same name: new file.
        std::fs::write(&src, png_bytes(128, 64)).unwrap();
        assert_eq!(
            import(&l, Kind::Cape, &src).unwrap(),
            "custom:My_Cool_Cape-2.png"
        );
        assert_eq!(list_custom(&l, Kind::Cape).len(), 2);
        // The original can disappear; the copy stays.
        std::fs::remove_file(&src).unwrap();
        assert!(capes_dir(&l).join("My_Cool_Cape.png").is_file());
        remove_custom(&l, Kind::Cape, &id).unwrap();
        assert_eq!(
            list_custom(&l, Kind::Cape),
            vec!["My_Cool_Cape-2.png".to_string()]
        );
    }

    #[test]
    fn import_rejects_bad_files() {
        let (d, l) = layout();
        let bad = d.path().join("x.png");
        std::fs::write(&bad, b"GIF89a not a png at all").unwrap();
        assert!(import(&l, Kind::Cape, &bad).is_err());
        std::fs::write(&bad, png_bytes(100, 50)).unwrap();
        assert!(
            import(&l, Kind::Cape, &bad)
                .unwrap_err()
                .to_string()
                .contains("multiple of 64")
        );
        std::fs::write(&bad, png_bytes(64, 32)).unwrap();
        assert!(import(&l, Kind::Sky, &bad).is_err());
        let mut truncated = png_bytes(64, 32);
        truncated.truncate(truncated.len() - 20);
        std::fs::write(&bad, truncated).unwrap();
        assert!(import(&l, Kind::Cape, &bad).is_err());
        assert!(import(&l, Kind::Cape, &d.path().join("missing.png")).is_err());
        assert!(list_custom(&l, Kind::Cape).is_empty());
    }

    #[test]
    fn path_safety() {
        let (_d, l) = layout();
        assert!(custom_path(&l, Kind::Cape, "custom:../../x.png").is_none());
        assert!(custom_path(&l, Kind::Cape, "custom:a\\b.png").is_none());
        assert!(custom_path(&l, Kind::Cape, "ember").is_none());
        assert!(custom_path(&l, Kind::Sky, "custom:ok.png").is_some());
        assert_eq!(sanitize_file_stem("../../evil name.png"), "evil_name");
        assert_eq!(sanitize_file_stem("....png"), "custom");
    }

    #[test]
    fn state_reads_config_then_pending_request() {
        let (_d, l) = layout();
        // No files: defaults.
        let s = load_state(&l);
        assert!(!s.pending);
        assert_eq!(s.settings, CosmeticSettings::default());

        // Mod config (Java ints are signed).
        std::fs::write(
            client_config_path(&l),
            br#"{"cosmetics":{"cape":"ember","capesEnabled":true},"sky":{"enabled":true,"preset":"starry"},
               "killEffects":{"effect":"lightning","color":-1538246},"lastLauncherCosmeticsRevision":5,"hud":{}}"#,
        )
        .unwrap();
        let s = load_state(&l);
        assert!(!s.pending);
        assert_eq!(s.settings.cosmetics.cape, "ember");
        assert_eq!(s.settings.sky.preset, "starry");
        assert_eq!(s.settings.kill_effects.effect, "lightning");
        assert_eq!(s.settings.kill_effects.color, 0xFFE8_873A);

        // Launcher request newer than applied revision => pending.
        let mut want = s.settings.clone();
        want.cosmetics.cape = "glacier".into();
        save_request(&l, &want).unwrap();
        let s2 = load_state(&l);
        assert!(s2.pending);
        assert_eq!(s2.settings.cosmetics.cape, "glacier");

        // Revisions always increase, even within the same millisecond.
        let r1: Value = serde_json::from_slice(&std::fs::read(request_path(&l)).unwrap()).unwrap();
        save_request(&l, &want).unwrap();
        let r2: Value = serde_json::from_slice(&std::fs::read(request_path(&l)).unwrap()).unwrap();
        assert!(r2["revision"].as_u64() > r1["revision"].as_u64());
        // Request JSON uses the mod's field names.
        assert_eq!(r2["cosmetics"]["capesEnabled"], Value::Bool(true));
        assert!(r2["killEffects"]["effect"].is_string());
    }

    #[test]
    fn malformed_files_fall_back() {
        let (_d, l) = layout();
        std::fs::write(client_config_path(&l), b"{nope").unwrap();
        std::fs::write(request_path(&l), b"[]").unwrap();
        let s = load_state(&l);
        assert!(!s.pending);
        assert_eq!(s.settings, CosmeticSettings::default());
    }

    #[test]
    fn previews() {
        let cape = decode_png(&png_bytes(128, 64)).unwrap();
        let p = cape_preview(&cape);
        assert_eq!((p.width, p.height), (20, 32));
        let sky = decode_png(&png_bytes(1536, 1024)).unwrap();
        let p = sky_preview(&sky, 384);
        assert_eq!((p.width, p.height), (384, 128));
        assert_eq!(p.pixels.len(), 384 * 128 * 4);
    }

    #[test]
    fn bundled_assets_pass_validation() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../client/src/main/resources/assets/kestrel/textures");
        for c in cape_catalog() {
            let b = std::fs::read(base.join(&c.texture)).unwrap();
            validate_image(Kind::Cape, &b).unwrap_or_else(|e| panic!("{}: {e}", c.id));
            if let Some(f) = &c.fallback {
                let b = std::fs::read(base.join(f)).unwrap();
                assert!(!is_gif(&b), "{}: fallback must be a PNG", c.id);
                validate_image(Kind::Cape, &b).unwrap_or_else(|e| panic!("{}: {e}", c.id));
            }
        }
        for (id, _) in BUILTIN_SKIES {
            let b = std::fs::read(base.join(format!("sky/{id}.png"))).unwrap();
            validate_image(Kind::Sky, &b).unwrap();
        }
    }

    #[test]
    fn fullbright_round_trip_and_persistence() {
        let (_d, l) = layout();
        // Nothing on disk yet: off.
        assert!(!load_state(&l).settings.qol.fullbright);

        let st = save_fullbright(&l, true).unwrap();
        assert!(
            st.settings.qol.fullbright && st.pending,
            "pending until the mod applies it"
        );
        let raw: Value = serde_json::from_slice(&std::fs::read(request_path(&l)).unwrap()).unwrap();
        assert_eq!(
            raw["qol"]["fullbright"],
            Value::Bool(true),
            "shape the mod reads"
        );
        // A fresh read (launcher restart) sees the same state.
        assert!(load_state(&l).settings.qol.fullbright);

        // The mod applies the revision and records it in its own config.
        let rev = raw["revision"].as_u64().unwrap();
        let cfg = serde_json::json!({"lastLauncherCosmeticsRevision": rev, "qol": {"fullbright": true},
            "cosmetics": {"cape": "ember"}});
        std::fs::write(client_config_path(&l), serde_json::to_vec(&cfg).unwrap()).unwrap();
        let st = load_state(&l);
        assert!(st.settings.qol.fullbright && !st.pending);

        // Turning it off keeps other settings that changed in game.
        let st = save_fullbright(&l, false).unwrap();
        assert!(!st.settings.qol.fullbright);
        assert_eq!(st.settings.cosmetics.cape, "ember");
    }

    #[test]
    fn fullbright_tolerates_old_or_broken_files() {
        let (_d, l) = layout();
        // Request written by an older launcher (no "qol").
        std::fs::write(
            request_path(&l),
            br#"{"revision":5,"cosmetics":{"capesEnabled":true,"cape":"none","animatedCapes":true},
               "sky":{"enabled":false,"preset":"dusk","brightness":1.0,"rotate":true},
               "killEffects":{"enabled":true,"effect":"burst","intensity":1.0,"sounds":true,"color":4293427002}}"#,
        )
        .unwrap();
        let st = load_state(&l);
        assert!(st.pending && !st.settings.qol.fullbright);
        // Malformed request and config -> defaults, never a panic.
        std::fs::write(request_path(&l), b"{ nope").unwrap();
        std::fs::write(client_config_path(&l), br#"{"qol": {"fullbright": "yes"}}"#).unwrap();
        assert!(!load_state(&l).settings.qol.fullbright);
        assert!(save_fullbright(&l, true).unwrap().settings.qol.fullbright);
    }

    const TETO_OWNER: &str = "ba14933e-a7ad-4816-8696-a788cc1b3c1f";

    #[test]
    fn catalog_keeps_existing_capes_and_restricts_teto() {
        let cat = parse_catalog(CAPE_CATALOG_JSON).unwrap();
        assert_eq!(cat.len(), cape_catalog().len());
        let get = |id: &str| cat.iter().find(|c| c.id == id).unwrap();
        for id in ["ember", "glacier", "midnight", "verdant", "aurora"] {
            assert!(!get(id).restricted(), "{id}");
            assert!(get(id).allows(None), "{id}");
        }
        let teto = get("teto");
        assert!(teto.restricted());
        assert_eq!(teto.texture, "capes/teto.gif");
        assert_eq!(teto.fallback.as_deref(), Some("capes/teto.png"));
        assert!(teto.allows(Some(TETO_OWNER)));
        assert!(teto.allows(Some("BA14933EA7AD48168696A788CC1B3C1F")));
        assert!(teto.allows(Some("ba14933ea7ad48168696a788cc1b3c1f")));
        assert!(!teto.allows(Some("ba14933e-a7ad-4816-8696-a788cc1b3c1e")));
        assert!(!teto.allows(Some("bringing")));
        assert!(!teto.allows(Some("")));
        assert!(!teto.allows(None));
        // An offline session named "bringing" gets the OfflinePlayer UUID, not the owner's.
        let offline = crate::auth::Session::offline("bringing");
        assert!(!teto.allows(Some(&offline.uuid)));
    }

    #[test]
    fn catalog_rejects_bad_owners_and_duplicates() {
        let bad_owner =
            r#"{"capes":[{"id":"x","name":"X","texture":"capes/x.png","owners":["bringing"]}]}"#;
        assert!(parse_catalog(bad_owner).is_err());
        let dup = r#"{"capes":[{"id":"x","name":"X","texture":"a.png"},{"id":"x","name":"Y","texture":"b.png"}]}"#;
        assert!(parse_catalog(dup).is_err());
        let none = r#"{"capes":[{"id":"none","name":"X","texture":"a.png"}]}"#;
        assert!(parse_catalog(none).is_err());
        let empty = r#"{"capes":[{"id":"x","name":"X","texture":"a.png","owners":[]}]}"#;
        let c = parse_catalog(empty).unwrap();
        assert!(c[0].restricted());
        assert!(
            !c[0].allows(Some(TETO_OWNER)),
            "empty owner list allows nobody"
        );
    }

    #[test]
    fn uuid_normalisation() {
        assert_eq!(
            normalize_uuid(TETO_OWNER).as_deref(),
            Some("ba14933ea7ad48168696a788cc1b3c1f")
        );
        assert_eq!(
            normalize_uuid(" BA14933EA7AD48168696A788CC1B3C1F ").as_deref(),
            Some("ba14933ea7ad48168696a788cc1b3c1f")
        );
        assert!(normalize_uuid("ba14933e-a7ad-4816-8696_a788cc1b3c1f").is_none());
        assert!(normalize_uuid("ba14933ea7ad48168696a788cc1b3c1").is_none());
        assert!(normalize_uuid("zz14933ea7ad48168696a788cc1b3c1f").is_none());
        assert!(normalize_uuid("ba14933ea7ad-4816-8696-a788cc1b3c1f-").is_none());
    }

    #[test]
    fn show_assigned_capes_defaults_on_and_round_trips() {
        let (_d, l) = layout();
        assert!(load_state(&l).settings.cosmetics.show_assigned_capes);
        // A request written by an older launcher has no such field.
        std::fs::create_dir_all(l.client_data()).unwrap();
        std::fs::write(
            request_path(&l),
            r#"{"revision":3,"cosmetics":{"capesEnabled":true,"cape":"ember","animatedCapes":true},
               "sky":{"enabled":false,"preset":"dusk","brightness":1.0,"rotate":true},
               "killEffects":{"enabled":true,"effect":"burst","intensity":1.0,"sounds":true,"color":4293427002}}"#,
        )
        .unwrap();
        assert!(load_state(&l).settings.cosmetics.show_assigned_capes);
        let mut s = load_state(&l).settings;
        s.cosmetics.show_assigned_capes = false;
        save_request(&l, &s).unwrap();
        let raw: Value = serde_json::from_slice(&std::fs::read(request_path(&l)).unwrap()).unwrap();
        assert_eq!(raw["cosmetics"]["showAssignedCapes"], Value::Bool(false));
        assert!(!load_state(&l).settings.cosmetics.show_assigned_capes);
        // The mod's config is read too.
        std::fs::write(
            client_config_path(&l),
            r#"{"lastLauncherCosmeticsRevision": 99999999999999, "cosmetics": {"showAssignedCapes": true}}"#,
        )
        .unwrap();
        assert!(load_state(&l).settings.cosmetics.show_assigned_capes);
    }

    /// Encode a GIF with the given frames (rgba, w, h, left, top).
    fn gif_bytes(sw: u16, sh: u16, frames: &[(Vec<u8>, u16, u16, u16, u16)]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = gif::Encoder::new(&mut out, sw, sh, &[]).unwrap();
            for (rgba, w, h, x, y) in frames {
                let mut px = rgba.clone();
                let mut f = gif::Frame::from_rgba_speed(*w, *h, &mut px, 30);
                f.left = *x;
                f.top = *y;
                f.delay = 7;
                enc.write_frame(&f).unwrap();
            }
        }
        out
    }

    fn solid(w: u16, h: u16, rgba: [u8; 4]) -> Vec<u8> {
        (0..u32::from(w) * u32::from(h))
            .flat_map(|_| rgba)
            .collect()
    }

    #[test]
    fn gif_capes_validate_like_the_mod() {
        // Arbitrary picture (fitted onto both faces by the mod).
        let pic = gif_bytes(
            30,
            60,
            &[
                (solid(30, 60, [255, 0, 0, 255]), 30, 60, 0, 0),
                (solid(10, 10, [0, 0, 255, 255]), 10, 10, 5, 5),
            ],
        );
        let first = validate_image(Kind::Cape, &pic).unwrap();
        assert_eq!((first.width, first.height), (30, 60));
        let prev = cape_preview_bytes(&pic).unwrap();
        assert_eq!((prev.width, prev.height), (30, 48)); // centre-cropped to 10:16
        assert_eq!(&prev.pixels[..4], &[255, 0, 0, 255]);

        // A GIF that is already a cape sheet previews its outer face.
        let sheet = gif_bytes(64, 32, &[(solid(64, 32, [0, 255, 0, 255]), 64, 32, 0, 0)]);
        let prev = cape_preview_bytes(&sheet).unwrap();
        assert_eq!((prev.width, prev.height), (10, 16));

        // Oversized logical screen, too many frames, garbage, truncated.
        let big = gif_bytes(1025, 10, &[(solid(1, 1, [1, 1, 1, 255]), 1, 1, 0, 0)]);
        assert!(
            validate_image(Kind::Cape, &big)
                .unwrap_err()
                .contains("too large")
        );
        let many: Vec<_> = (0..129)
            .map(|_| (solid(1, 1, [1, 2, 3, 255]), 1u16, 1u16, 0u16, 0u16))
            .collect();
        assert!(
            validate_image(Kind::Cape, &gif_bytes(1, 1, &many))
                .unwrap_err()
                .contains("too many frames")
        );
        let mut junk = b"GIF89a".to_vec();
        junk.extend_from_slice(&[0xFF; 40]);
        assert!(validate_image(Kind::Cape, &junk).is_err());
        for cut in [10, 20, pic.len() / 2, pic.len() - 2] {
            let _ = validate_image(Kind::Cape, &pic[..cut]); // must not panic
        }
        // GIFs are capes only.
        assert!(validate_image(Kind::Sky, &pic).is_err());
    }

    #[test]
    fn texel_budget_matches_the_mod() {
        // 64 frames of a 512x256 sheet = 8M texels: fine; 1024x512 x 24 frames = 12.6M: too large.
        let ok: Vec<_> = (0..2)
            .map(|_| (solid(1, 1, [9, 9, 9, 255]), 1u16, 1u16, 0u16, 0u16))
            .collect();
        assert!(validate_image(Kind::Cape, &gif_bytes(512, 256, &ok)).is_ok());
        let heavy: Vec<_> = (0..25)
            .map(|_| (solid(1, 1, [9, 9, 9, 255]), 1u16, 1u16, 0u16, 0u16))
            .collect();
        assert!(
            validate_image(Kind::Cape, &gif_bytes(1024, 512, &heavy))
                .unwrap_err()
                .contains("too large")
        );
    }

    #[test]
    fn gif_import_keeps_gif_extension_and_is_listed() {
        let (d, l) = layout();
        let src = d.path().join("teto dance.gif");
        std::fs::write(
            &src,
            gif_bytes(30, 60, &[(solid(30, 60, [255, 0, 0, 255]), 30, 60, 0, 0)]),
        )
        .unwrap();
        let id = import(&l, Kind::Cape, &src).unwrap();
        assert_eq!(id, "custom:teto_dance.gif");
        assert_eq!(import(&l, Kind::Cape, &src).unwrap(), id, "deduplicated");
        assert_eq!(
            list_custom(&l, Kind::Cape),
            vec!["teto_dance.gif".to_string()]
        );
        // A GIF dropped into the skies folder is not offered as a sky.
        std::fs::create_dir_all(skies_dir(&l)).unwrap();
        std::fs::copy(&src, skies_dir(&l).join("x.gif")).unwrap();
        assert!(list_custom(&l, Kind::Sky).is_empty());
        // PNG capes keep the 2 MB limit even though GIFs may be 4 MB.
        let big_png = d.path().join("big.png");
        let mut b = png_bytes(64, 32);
        b.resize(3 * 1024 * 1024, 0);
        std::fs::write(&big_png, b).unwrap();
        assert!(import(&l, Kind::Cape, &big_png).is_err());
    }

    #[test]
    fn bundled_teto_gif_validates() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../client/src/main/resources/assets/kestrel/textures");
        let b = std::fs::read(base.join("capes/teto.gif")).unwrap();
        let first = validate_cape_gif(&b).unwrap();
        assert_eq!((first.width, first.height), (231, 498));
        let prev = cape_preview_bytes(&b).unwrap();
        // 231 wide -> 231 * 16 / 10 = 369 rows, centred.
        assert_eq!((prev.width, prev.height), (231, 369));
    }
}
