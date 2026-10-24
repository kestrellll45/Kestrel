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
//! file name.
//!
//! Everything here is local: capes, skies and effects are rendered by your
//! own client only. Other players do not see them.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{Error, IoContext, Result};
use crate::hash;
use crate::paths::{Layout, atomic_write};

pub const CUSTOM_PREFIX: &str = "custom:";
pub const CAPE_NONE: &str = "none";
pub const MAX_CAPE_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_SKY_BYTES: u64 = 16 * 1024 * 1024;

/// Bundled capes (id, display name). Must match `CapeManager.BUILTIN`.
pub const BUILTIN_CAPES: &[(&str, &str)] = &[
    ("ember", "Ember"),
    ("glacier", "Glacier"),
    ("midnight", "Midnight"),
    ("verdant", "Verdant"),
    ("aurora", "Aurora (animated)"),
];

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CosmeticSettings {
    pub cosmetics: CapeSettings,
    pub sky: SkySettings,
    #[serde(rename = "killEffects")]
    pub kill_effects: KillEffectSettings,
}

impl Default for CosmeticSettings {
    fn default() -> Self {
        CosmeticSettings {
            cosmetics: CapeSettings {
                capes_enabled: true,
                cape: CAPE_NONE.into(),
                animated_capes: true,
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
    CosmeticSettings {
        cosmetics: CapeSettings {
            capes_enabled: b(cos, "capesEnabled", d.cosmetics.capes_enabled),
            cape: s(cos, "cape", &d.cosmetics.cape),
            animated_capes: b(cos, "animatedCapes", d.cosmetics.animated_capes),
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

// ---- image validation -------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Cape,
    Sky,
}

impl Kind {
    fn max_bytes(self) -> u64 {
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

/// Fully validate image bytes for a cosmetic kind.
pub fn validate_image(kind: Kind, bytes: &[u8]) -> std::result::Result<Rgba, String> {
    if bytes.len() as u64 > kind.max_bytes() {
        return Err(format!(
            "file is too large ({} KB, max {} KB)",
            bytes.len() / 1024,
            kind.max_bytes() / 1024
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

/// Turn a user file name into a safe one: `[A-Za-z0-9_-]`, max 40 chars, `.png`.
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
            format!("{stem}.png")
        } else {
            format!("{stem}-{n}.png")
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
                .filter(|n| n.to_ascii_lowercase().ends_with(".png"))
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
        for (id, _) in BUILTIN_CAPES {
            let b = std::fs::read(base.join(format!("capes/{id}.png"))).unwrap();
            validate_image(Kind::Cape, &b).unwrap();
        }
        for (id, _) in BUILTIN_SKIES {
            let b = std::fs::read(base.join(format!("sky/{id}.png"))).unwrap();
            validate_image(Kind::Sky, &b).unwrap();
        }
    }
}
