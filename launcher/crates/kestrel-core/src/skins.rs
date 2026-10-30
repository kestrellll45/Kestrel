//! Local skin changer, shared by the launcher and the client mod.
//!
//! Everything lives under the client data folder
//! (`<root>/instance/kestrel/skins/`):
//!
//! * `<16 hex>.png` — validated copies of imported skins, named by content
//!   hash (so re-importing the same file never duplicates it);
//! * `skin.json` — the list of imported skins and which one is active.
//!
//! Both the launcher and the mod (`com.kestrel.skin.SkinStore`) read and
//! rewrite `skin.json` atomically; the mod polls it about once a second, so a
//! change made on either side shows up on the other. No entry → the account's
//! own skin is used.
//!
//! The skin is a **local, client-side appearance only**: nothing is uploaded
//! to Mojang/Microsoft, nothing is sent to servers, and other players keep
//! seeing the account's real skin.

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cosmetics::decode_png;
use crate::error::{Error, IoContext, Result};
use crate::hash;
use crate::paths::{Layout, atomic_write};

/// Largest skin file accepted (real skins are a few KB).
pub const MAX_SKIN_BYTES: usize = 512 * 1024;
/// Largest JSON response accepted from the Mojang profile API.
const MAX_JSON_BYTES: u64 = 64 * 1024;
/// Most skins kept in the list.
pub const MAX_SKINS: usize = 64;
/// Longest URL accepted in the "Load from URL" box.
const MAX_URL_LEN: usize = 300;

pub const MODEL_CLASSIC: &str = "classic";
pub const MODEL_SLIM: &str = "slim";

/// One imported skin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinEntry {
    /// File name inside the skins folder: `<16 lowercase hex>.png`.
    pub file: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// `classic` (4 px arms) or `slim` (3 px arms).
    #[serde(default = "default_model")]
    pub model: String,
    /// Where it came from (file name or URL), for display only.
    #[serde(default)]
    pub source: String,
    /// Import time, milliseconds since the Unix epoch.
    #[serde(default)]
    pub added: u64,
}

/// Contents of `skin.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinState {
    #[serde(default = "one")]
    pub version: u32,
    /// File name of the active skin; `None` = use the account skin.
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub skins: Vec<SkinEntry>,
    /// Increases on every write (milliseconds, strictly increasing).
    #[serde(default)]
    pub revision: u64,
    /// "launcher" or "game", for diagnostics.
    #[serde(default)]
    pub updated_by: String,
}

impl Default for SkinState {
    fn default() -> Self {
        SkinState {
            version: 1,
            active: None,
            skins: Vec::new(),
            revision: 0,
            updated_by: String::new(),
        }
    }
}

fn one() -> u32 {
    1
}

fn default_model() -> String {
    MODEL_CLASSIC.into()
}

impl SkinState {
    /// The active entry, if any.
    pub fn active_entry(&self) -> Option<&SkinEntry> {
        let active = self.active.as_deref()?;
        self.skins.iter().find(|s| s.file == active)
    }

    /// Repair anything a hand edit or an older/newer writer could have left:
    /// invalid file names, unknown models, duplicates, a dangling `active`.
    /// `exists` reports whether a skin file is present on disk.
    pub fn normalize(&mut self, exists: impl Fn(&str) -> bool) {
        let mut seen = std::collections::HashSet::new();
        self.skins.retain(|s| {
            is_valid_file_name(&s.file) && exists(&s.file) && seen.insert(s.file.clone())
        });
        self.skins.truncate(MAX_SKINS);
        for s in &mut self.skins {
            s.model = normalize_model(&s.model).to_string();
            s.name = sanitize_name(&s.name);
            if s.name.is_empty() {
                s.name = "Skin".into();
            }
            s.source = s.source.chars().take(MAX_URL_LEN).collect();
        }
        if let Some(a) = &self.active
            && !self.skins.iter().any(|s| &s.file == a)
        {
            self.active = None;
        }
    }
}

/// `classic` unless the value is exactly `slim` (case-insensitive).
pub fn normalize_model(m: &str) -> &'static str {
    if m.trim().eq_ignore_ascii_case(MODEL_SLIM) {
        MODEL_SLIM
    } else {
        MODEL_CLASSIC
    }
}

/// Skin files are always `<16 lowercase hex>.png`, so a name from the
/// shared JSON can never point outside the skins folder.
pub fn is_valid_file_name(name: &str) -> bool {
    match name.strip_suffix(".png") {
        Some(stem) => {
            stem.len() == 16
                && stem
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }
        None => false,
    }
}

/// Display names: printable, at most 32 characters.
pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .take(32)
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn skins_dir(layout: &Layout) -> PathBuf {
    layout.client_data().join("skins")
}

pub fn state_path(layout: &Layout) -> PathBuf {
    skins_dir(layout).join("skin.json")
}

/// Path of a skin file, or `None` for an invalid name.
pub fn skin_path(layout: &Layout, file: &str) -> Option<PathBuf> {
    is_valid_file_name(file).then(|| skins_dir(layout).join(file))
}

/// Read `skin.json`. Never fails: a missing or malformed file means "no
/// custom skin" (the account skin).
pub fn load_state(layout: &Layout) -> SkinState {
    let dir = skins_dir(layout);
    let mut state = std::fs::read(state_path(layout))
        .ok()
        .and_then(|b| serde_json::from_slice::<SkinState>(&b).ok())
        .unwrap_or_default();
    state.normalize(|f| dir.join(f).is_file());
    state
}

/// Write `skin.json` atomically with a new, strictly increasing revision.
pub fn save_state(layout: &Layout, state: &mut SkinState) -> Result<()> {
    let dir = skins_dir(layout);
    state.normalize(|f| dir.join(f).is_file());
    let previous = std::fs::read(state_path(layout))
        .ok()
        .and_then(|b| serde_json::from_slice::<SkinState>(&b).ok())
        .map(|s| s.revision)
        .unwrap_or(0);
    state.revision = now_ms().max(previous + 1).max(state.revision + 1);
    state.version = 1;
    state.updated_by = "launcher".into();
    let data = serde_json::to_vec_pretty(state).map_err(|e| Error::json("skin.json", e))?;
    atomic_write(&state_path(layout), &data)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---- validation -------------------------------------------------------------

/// A validated skin image.
#[derive(Debug, Clone)]
pub struct ValidSkin {
    pub width: u32,
    pub height: u32,
    /// True for the pre-1.8 64x32 layout (classic arms only).
    pub legacy: bool,
    /// Model guessed from the pixels (`slim` when the 4th arm column is empty).
    pub suggested_model: &'static str,
    /// Decoded RGBA, for previews.
    pub rgba: crate::cosmetics::Rgba,
}

/// Validate skin PNG bytes: size limit, PNG signature, 64x64 or legacy 64x32.
/// The pixels (including transparency) are not modified; the file is stored
/// exactly as given.
pub fn validate_skin(bytes: &[u8]) -> std::result::Result<ValidSkin, String> {
    if bytes.len() > MAX_SKIN_BYTES {
        return Err(format!(
            "file is too large ({} KB, max {} KB)",
            bytes.len() / 1024,
            MAX_SKIN_BYTES / 1024
        ));
    }
    // Check the header before decoding so a huge image is never decoded.
    let (w, h) = png_dimensions(bytes)?;
    if !(w == 64 && (h == 64 || h == 32)) {
        return Err(format!(
            "a Minecraft skin must be 64x64 (or the old 64x32 format); this image is {w}x{h}"
        ));
    }
    let rgba = decode_png(bytes)?;
    if rgba.width != w || rgba.height != h {
        return Err("corrupt PNG (header and image size differ)".into());
    }
    let legacy = h == 32;
    let suggested_model = if !legacy && is_slim(&rgba) {
        MODEL_SLIM
    } else {
        MODEL_CLASSIC
    };
    Ok(ValidSkin {
        width: w,
        height: h,
        legacy,
        suggested_model,
        rgba,
    })
}

/// Width/height from the IHDR chunk.
pub fn png_dimensions(bytes: &[u8]) -> std::result::Result<(u32, u32), String> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err("not a PNG image (only .png skins are supported)".into());
    }
    if &bytes[12..16] != b"IHDR" {
        return Err("corrupt PNG header".into());
    }
    let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    if w == 0 || h == 0 {
        return Err("PNG has invalid dimensions".into());
    }
    Ok((w, h))
}

/// Slim (3 px) arms leave texture columns 54-55 of the right arm's base
/// layer (rows 20-31) unused; classic skins paint them.
fn is_slim(img: &crate::cosmetics::Rgba) -> bool {
    let alpha = |x: u32, y: u32| img.pixels[((y * img.width + x) * 4 + 3) as usize];
    (20..32).all(|y| alpha(54, y) == 0 && alpha(55, y) == 0)
}

// ---- preview ----------------------------------------------------------------

/// Width/height of one view in [`preview`] (a 16x32 front or back view).
pub const VIEW_W: u32 = 16;
pub const VIEW_H: u32 = 32;
/// Gap between the front and back view.
pub const VIEW_GAP: u32 = 4;

/// Flat front and back views of a skin, side by side (36x32 RGBA), with the
/// second (hat/jacket/sleeve/pants) layer drawn over the base layer. Like the
/// game, the base layer is drawn opaque; legacy 64x32 skins get mirrored left
/// limbs, exactly as Minecraft 1.8 converts them.
pub fn preview(img: &crate::cosmetics::Rgba, slim: bool) -> crate::cosmetics::Rgba {
    let w = VIEW_W * 2 + VIEW_GAP;
    let mut out = crate::cosmetics::Rgba {
        width: w,
        height: VIEW_H,
        pixels: vec![0; (w * VIEW_H * 4) as usize],
    };
    let legacy = img.height == 32;
    let aw = if slim && !legacy { 3 } else { 4 };
    // (src x, src y, width, height, dst x, dst y, mirror, overlay)
    type Part = (u32, u32, u32, u32, u32, u32, bool, bool);
    let mut parts: Vec<Part> = Vec::with_capacity(24);
    // Front view: the player's right side is on the viewer's left.
    parts.extend([
        (8, 8, 8, 8, 4, 0, false, false),
        (20, 20, 8, 12, 4, 8, false, false),
        (44, 20, aw, 12, 4 - aw, 8, false, false),
        (4, 20, 4, 12, 4, 20, false, false),
    ]);
    if legacy {
        parts.extend([
            (44, 20, 4, 12, 12, 8, true, false),
            (4, 20, 4, 12, 8, 20, true, false),
        ]);
    } else {
        parts.extend([
            (36, 52, aw, 12, 12, 8, false, false),
            (20, 52, 4, 12, 8, 20, false, false),
        ]);
    }
    parts.push((40, 8, 8, 8, 4, 0, false, true));
    if !legacy {
        parts.extend([
            (20, 36, 8, 12, 4, 8, false, true),
            (44, 36, aw, 12, 4 - aw, 8, false, true),
            (52, 52, aw, 12, 12, 8, false, true),
            (4, 36, 4, 12, 4, 20, false, true),
            (4, 52, 4, 12, 8, 20, false, true),
        ]);
    }
    // Back view: the player's left side is on the viewer's left.
    let bx = VIEW_W + VIEW_GAP;
    let r_back = 40 + 4 + aw + 4; // right arm back face
    let l_back = 32 + 4 + aw + 4; // left arm back face
    parts.extend([
        (24, 8, 8, 8, bx + 4, 0, false, false),
        (32, 20, 8, 12, bx + 4, 8, false, false),
        (r_back, 20, aw, 12, bx + 12, 8, false, false),
        (12, 20, 4, 12, bx + 8, 20, false, false),
    ]);
    if legacy {
        parts.extend([
            (r_back, 20, 4, 12, bx, 8, true, false),
            (12, 20, 4, 12, bx + 4, 20, true, false),
        ]);
    } else {
        parts.extend([
            (l_back, 52, aw, 12, bx + 4 - aw, 8, false, false),
            (28, 52, 4, 12, bx + 4, 20, false, false),
        ]);
    }
    parts.push((56, 8, 8, 8, bx + 4, 0, false, true));
    if !legacy {
        parts.extend([
            (32, 36, 8, 12, bx + 4, 8, false, true),
            (r_back, 36, aw, 12, bx + 12, 8, false, true),
            (l_back + 16, 52, aw, 12, bx + 4 - aw, 8, false, true),
            (12, 36, 4, 12, bx + 8, 20, false, true),
            (12, 52, 4, 12, bx + 4, 20, false, true),
        ]);
    }
    for (sx, sy, pw, ph, dx, dy, mirror, overlay) in parts {
        for y in 0..ph {
            for x in 0..pw {
                let srcx = if mirror { sx + pw - 1 - x } else { sx + x };
                let si = (((sy + y) * img.width + srcx) * 4) as usize;
                let di = (((dy + y) * w + dx + x) * 4) as usize;
                let src = &img.pixels[si..si + 4];
                if !overlay {
                    out.pixels[di..di + 3].copy_from_slice(&src[..3]);
                    out.pixels[di + 3] = 255;
                } else if src[3] > 0 {
                    let a = src[3] as u32;
                    for (d, &sc) in out.pixels[di..di + 3].iter_mut().zip(&src[..3]) {
                        *d = ((sc as u32 * a + *d as u32 * (255 - a)) / 255) as u8;
                    }
                    out.pixels[di + 3] = out.pixels[di + 3].max(src[3]);
                }
            }
        }
    }
    out
}

// ---- import -----------------------------------------------------------------

/// Copy validated skin bytes into the skins folder and add (or refresh) the
/// entry. Does not change the active skin. Returns the entry.
pub fn import_bytes(
    layout: &Layout,
    state: &mut SkinState,
    bytes: &[u8],
    name: &str,
    source: &str,
    model_hint: Option<&str>,
) -> Result<SkinEntry> {
    let skin = validate_skin(bytes).map_err(Error::Other)?;
    let file = format!("{}.png", &hash::sha1_bytes(bytes)[..16]);
    let dir = skins_dir(layout);
    std::fs::create_dir_all(&dir).at(&dir)?;
    let dest = dir.join(&file);
    if !hash::file_matches(&dest, None, Some(bytes.len() as u64)) {
        atomic_write(&dest, bytes)?;
    }
    let model = if skin.legacy {
        MODEL_CLASSIC
    } else {
        model_hint
            .map(normalize_model)
            .unwrap_or(skin.suggested_model)
    };
    let mut name = sanitize_name(name);
    if name.is_empty() {
        name = "Skin".into();
    }
    let entry = SkinEntry {
        file: file.clone(),
        name,
        model: model.into(),
        source: source.chars().take(MAX_URL_LEN).collect(),
        added: now_ms(),
    };
    state.skins.retain(|s| s.file != file);
    state.skins.insert(0, entry.clone());
    if state.skins.len() > MAX_SKINS {
        state.skins.truncate(MAX_SKINS);
    }
    log::info!(
        "imported skin {file} ({}x{}, {model})",
        skin.width,
        skin.height
    );
    Ok(entry)
}

/// Import a PNG file chosen by the user.
pub fn import_file(layout: &Layout, state: &mut SkinState, src: &Path) -> Result<SkinEntry> {
    let meta = std::fs::metadata(src).at(src)?;
    if !meta.is_file() {
        return Err(Error::Other(format!("{} is not a file", src.display())));
    }
    if meta.len() > MAX_SKIN_BYTES as u64 {
        return Err(Error::Other(format!(
            "file is too large ({} KB, max {} KB)",
            meta.len() / 1024,
            MAX_SKIN_BYTES / 1024
        )));
    }
    let bytes = std::fs::read(src).at(src)?;
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let source = src
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    import_bytes(layout, state, &bytes, &stem, &source, None)
}

/// Remove an imported skin (the entry and its copy in the skins folder).
/// The user's original file is never touched.
pub fn remove(layout: &Layout, state: &mut SkinState, file: &str) -> Result<()> {
    let Some(path) = skin_path(layout, file) else {
        return Err(Error::Other("invalid skin name".into()));
    };
    state.skins.retain(|s| s.file != file);
    if state.active.as_deref() == Some(file) {
        state.active = None;
    }
    if path.exists() {
        std::fs::remove_file(&path).at(&path)?;
    }
    Ok(())
}

// ---- URLs -------------------------------------------------------------------

/// What a pasted URL points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkinSource {
    /// `namemc.com/profile/<name or uuid>`: the player's current skin,
    /// resolved through Mojang's public profile API.
    Profile(String),
    /// A skin texture on Mojang's texture server (`textures.minecraft.net`).
    Texture(String),
    /// `namemc.com/skin/<id>`: a NameMC skin page. NameMC has no public API
    /// for these and blocks automated requests, so they cannot be loaded.
    NameMcSkin(String),
}

/// Parse a pasted URL. Only NameMC profile/skin pages and Mojang texture URLs
/// are accepted; everything else is rejected before any network access.
pub fn parse_source(input: &str) -> std::result::Result<SkinSource, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("paste a NameMC profile link, e.g. https://namemc.com/profile/Notch".into());
    }
    if input.len() > MAX_URL_LEN {
        return Err("that link is too long".into());
    }
    let rest = if let Some(r) = input.strip_prefix("https://") {
        r
    } else if let Some(r) = input.strip_prefix("http://") {
        r
    } else if input.contains("://") {
        return Err("only http(s) links are supported".into());
    } else {
        input
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = rest[..end].to_ascii_lowercase();
    if host.contains('@') || host.contains(':') {
        return Err("unsupported link (user names and ports are not allowed)".into());
    }
    let path = rest[end..].split(['?', '#']).next().unwrap_or("");
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if is_namemc_host(&host) {
        return match segs.as_slice() {
            ["profile", id, ..] => {
                // NameMC disambiguates names with a ".N" suffix (Notch.1).
                let id = match id.rsplit_once('.') {
                    Some((base, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
                        base
                    }
                    _ => id,
                };
                if let Some(uuid) = normalize_uuid(id) {
                    Ok(SkinSource::Profile(uuid))
                } else if is_valid_player_name(id) {
                    Ok(SkinSource::Profile(id.to_string()))
                } else {
                    Err(format!("'{id}' is not a valid Minecraft name or UUID"))
                }
            }
            ["skin", id, ..]
                if !id.is_empty()
                    && id.len() <= 64
                    && id.bytes().all(|b| b.is_ascii_alphanumeric()) =>
            {
                Ok(SkinSource::NameMcSkin(id.to_string()))
            }
            _ => Err(
                "unsupported NameMC link: use a profile link (namemc.com/profile/<name>)".into(),
            ),
        };
    }
    if host == "textures.minecraft.net" {
        return match segs.as_slice() {
            ["texture", hash]
                if !hash.is_empty()
                    && hash.len() <= 64
                    && hash.bytes().all(|b| b.is_ascii_hexdigit()) =>
            {
                Ok(SkinSource::Texture(format!(
                    "https://textures.minecraft.net/texture/{hash}"
                )))
            }
            _ => Err("unsupported textures.minecraft.net link".into()),
        };
    }
    Err(format!(
        "unsupported site '{host}': paste a NameMC profile link (namemc.com/profile/<name>) or a textures.minecraft.net link"
    ))
}

/// namemc.com, www.namemc.com and NameMC's language subdomains (xx.namemc.com).
fn is_namemc_host(host: &str) -> bool {
    match host.strip_suffix("namemc.com") {
        Some("") | Some("www.") => true,
        Some(sub) => {
            let sub = sub.trim_end_matches('.');
            host.ends_with(".namemc.com")
                && sub.len() == 2
                && sub.bytes().all(|b| b.is_ascii_lowercase())
        }
        None => false,
    }
}

pub fn is_valid_player_name(s: &str) -> bool {
    (3..=16).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// 32 hex digits, with or without dashes → lowercase without dashes.
pub fn normalize_uuid(s: &str) -> Option<String> {
    let hex: String = s.chars().filter(|&c| c != '-').collect();
    (hex.len() == 32
        && hex.bytes().all(|b| b.is_ascii_hexdigit())
        && (s.len() == 32 || s.len() == 36))
        .then(|| hex.to_ascii_lowercase())
}

/// A skin downloaded from a URL, not yet imported.
#[derive(Debug, Clone)]
pub struct Downloaded {
    pub bytes: Vec<u8>,
    pub name: String,
    pub model: Option<&'static str>,
}

/// Fetches a URL into memory with a size cap. Only ever called with the
/// three Mojang hosts below.
pub type Fetch<'a> = dyn Fn(&str, u64) -> std::result::Result<Vec<u8>, String> + 'a;

pub const NAMEMC_SKIN_PAGE_HELP: &str = "NameMC skin pages (namemc.com/skin/…) can't be loaded automatically: NameMC has no \
public API for them and blocks automated downloads. Open the page in your browser, click Download, then use \
Import Skin with the saved PNG. NameMC profile links (namemc.com/profile/<name>) work directly.";

/// Resolve a parsed source to skin bytes using `fetch` for the network.
pub fn resolve(source: &SkinSource, fetch: &Fetch) -> std::result::Result<Downloaded, String> {
    match source {
        SkinSource::NameMcSkin(_) => Err(NAMEMC_SKIN_PAGE_HELP.into()),
        SkinSource::Texture(url) => {
            let bytes = fetch(url, MAX_SKIN_BYTES as u64)?;
            let hash = url.rsplit('/').next().unwrap_or("");
            Ok(Downloaded {
                bytes,
                name: format!("texture {}", &hash[..hash.len().min(8)]),
                model: None,
            })
        }
        SkinSource::Profile(id) => {
            let (uuid, name) = match normalize_uuid(id) {
                Some(u) => (u, None),
                None => {
                    let body = fetch(
                        &format!("https://api.mojang.com/users/profiles/minecraft/{id}"),
                        MAX_JSON_BYTES,
                    )
                    .map_err(|e| {
                        if e.contains("404") || e.contains("204") {
                            format!("no Minecraft account is named '{id}'")
                        } else {
                            e
                        }
                    })?;
                    let v: Value = serde_json::from_slice(&body)
                        .map_err(|_| format!("no Minecraft account is named '{id}'"))?;
                    let uuid = v
                        .get("id")
                        .and_then(Value::as_str)
                        .and_then(normalize_uuid)
                        .ok_or_else(|| format!("no Minecraft account is named '{id}'"))?;
                    let name = v.get("name").and_then(Value::as_str).map(str::to_string);
                    (uuid, name)
                }
            };
            let body = fetch(
                &format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}"),
                MAX_JSON_BYTES,
            )?;
            let (url, model, profile_name) = parse_profile_textures(&body)?;
            let bytes = fetch(&url, MAX_SKIN_BYTES as u64)?;
            Ok(Downloaded {
                bytes,
                name: profile_name.or(name).unwrap_or_else(|| id.clone()),
                model: Some(model),
            })
        }
    }
}

/// From a session-server profile response, return the skin URL (https,
/// textures.minecraft.net only), the model and the player name.
pub fn parse_profile_textures(
    body: &[u8],
) -> std::result::Result<(String, &'static str, Option<String>), String> {
    let v: Value =
        serde_json::from_slice(body).map_err(|_| "unexpected reply from Mojang".to_string())?;
    let name = v.get("name").and_then(Value::as_str).map(str::to_string);
    let encoded = v
        .get("properties")
        .and_then(Value::as_array)
        .and_then(|props| {
            props
                .iter()
                .find(|p| p.get("name").and_then(Value::as_str) == Some("textures"))
        })
        .and_then(|p| p.get("value"))
        .and_then(Value::as_str)
        .ok_or_else(|| "this player has no skin information".to_string())?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded.as_bytes())
        .map_err(|_| "unexpected reply from Mojang".to_string())?;
    let t: Value =
        serde_json::from_slice(&decoded).map_err(|_| "unexpected reply from Mojang".to_string())?;
    let skin = t
        .get("textures")
        .and_then(|x| x.get("SKIN"))
        .ok_or_else(|| {
            "this player uses a default skin (Steve/Alex); there is no custom skin to load"
                .to_string()
        })?;
    let url = skin
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "unexpected reply from Mojang".to_string())?;
    let model = skin
        .get("metadata")
        .and_then(|m| m.get("model"))
        .and_then(Value::as_str)
        .map(normalize_model)
        .unwrap_or(MODEL_CLASSIC);
    // Mojang returns http:// texture URLs; the same files are served over https.
    let SkinSource::Texture(url) =
        parse_source(url).map_err(|_| "unexpected skin URL from Mojang".to_string())?
    else {
        return Err("unexpected skin URL from Mojang".into());
    };
    Ok((url, model, name))
}

/// The real network fetcher: https only, the three Mojang hosts only, no
/// redirects, no cookies or credentials, 10 s timeouts and a size cap.
pub fn http_fetch(url: &str, limit: u64) -> std::result::Result<Vec<u8>, String> {
    let host = url
        .strip_prefix("https://")
        .and_then(|r| r.split('/').next())
        .unwrap_or("");
    if !matches!(
        host,
        "api.mojang.com" | "sessionserver.mojang.com" | "textures.minecraft.net"
    ) {
        return Err(format!("refusing to contact {url}"));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .max_redirects(0)
        .user_agent(format!("Kestrel-Launcher/{}", crate::LAUNCHER_VERSION))
        .build()
        .into();
    match agent.get(url).call() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            if status != 200 {
                return Err(format!("HTTP {status} from {host}"));
            }
            resp.into_body()
                .into_with_config()
                .limit(limit)
                .read_to_vec()
                .map_err(|e| format!("download from {host} failed: {e}"))
        }
        Err(ureq::Error::StatusCode(code)) => Err(format!("HTTP {code} from {host}")),
        Err(e) => Err(format!("cannot reach {host}: {e}")),
    }
}

/// Parse, resolve and import a URL in one step (blocking; run on a worker).
pub fn import_url(layout: &Layout, state: &mut SkinState, input: &str) -> Result<SkinEntry> {
    import_url_with(layout, state, input, &http_fetch)
}

pub fn import_url_with(
    layout: &Layout,
    state: &mut SkinState,
    input: &str,
    fetch: &Fetch,
) -> Result<SkinEntry> {
    let source = parse_source(input).map_err(Error::Other)?;
    let d = resolve(&source, fetch).map_err(Error::Other)?;
    import_bytes(layout, state, &d.bytes, &d.name, input.trim(), d.model)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().unwrap();
            let mut data = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h {
                for x in 0..w {
                    data.extend_from_slice(&f(x, y));
                }
            }
            wr.write_image_data(&data).unwrap();
        }
        out
    }

    fn classic() -> Vec<u8> {
        png(64, 64, |x, y| {
            [x as u8, y as u8, 7, if y < 32 { 255 } else { 0 }]
        })
    }

    fn slim() -> Vec<u8> {
        png(64, 64, |x, y| {
            let empty = (54..56).contains(&x) && (20..32).contains(&y);
            [1, 2, 3, if empty || y >= 32 { 0 } else { 255 }]
        })
    }

    fn layout() -> (tempfile::TempDir, Layout) {
        let d = tempfile::tempdir().unwrap();
        let l = Layout::new(d.path());
        (d, l)
    }

    #[test]
    fn validates_dimensions_and_format() {
        assert!(validate_skin(&classic()).is_ok());
        let legacy = validate_skin(&png(64, 32, |_, _| [9, 9, 9, 255])).unwrap();
        assert!(legacy.legacy);
        assert_eq!(legacy.suggested_model, MODEL_CLASSIC);
        assert!(
            validate_skin(&png(128, 128, |_, _| [0; 4]))
                .unwrap_err()
                .contains("64x64")
        );
        assert!(validate_skin(&png(64, 48, |_, _| [0; 4])).is_err());
        assert!(validate_skin(b"GIF89a....").unwrap_err().contains("PNG"));
        assert!(validate_skin(&[0u8; 10]).is_err());
        let mut big = classic();
        big.resize(MAX_SKIN_BYTES + 1, 0);
        assert!(validate_skin(&big).unwrap_err().contains("too large"));
    }

    #[test]
    fn detects_slim_arms() {
        assert_eq!(validate_skin(&slim()).unwrap().suggested_model, MODEL_SLIM);
        assert_eq!(
            validate_skin(&classic()).unwrap().suggested_model,
            MODEL_CLASSIC
        );
    }

    #[test]
    fn import_dedupes_preserves_bytes_and_persists() {
        let (_d, l) = layout();
        let mut st = load_state(&l);
        assert_eq!(
            st,
            SkinState {
                revision: 0,
                ..SkinState::default()
            }
        );
        let bytes = slim();
        let e = import_bytes(&l, &mut st, &bytes, "My\u{7} skin", "my.png", None).unwrap();
        assert!(is_valid_file_name(&e.file));
        assert_eq!(e.name, "My skin");
        assert_eq!(e.model, MODEL_SLIM);
        // Stored byte-for-byte (transparency untouched).
        assert_eq!(
            std::fs::read(skin_path(&l, &e.file).unwrap()).unwrap(),
            bytes
        );
        let again = import_bytes(&l, &mut st, &bytes, "dup", "dup.png", Some("classic")).unwrap();
        assert_eq!(again.file, e.file);
        assert_eq!(st.skins.len(), 1);
        assert_eq!(st.skins[0].model, MODEL_CLASSIC);

        st.active = Some(e.file.clone());
        save_state(&l, &mut st).unwrap();
        let r1 = st.revision;
        let loaded = load_state(&l);
        assert_eq!(loaded.active.as_deref(), Some(e.file.as_str()));
        assert_eq!(loaded.active_entry().unwrap().model, MODEL_CLASSIC);
        save_state(&l, &mut st).unwrap();
        assert!(st.revision > r1);

        // Restore account skin.
        st.active = None;
        save_state(&l, &mut st).unwrap();
        assert_eq!(load_state(&l).active, None);
        assert_eq!(load_state(&l).skins.len(), 1);

        remove(&l, &mut st, &e.file).unwrap();
        assert!(st.skins.is_empty());
        assert!(!skin_path(&l, &e.file).unwrap().exists());
    }

    #[test]
    fn legacy_skins_are_always_classic() {
        let (_d, l) = layout();
        let mut st = SkinState::default();
        let e = import_bytes(
            &l,
            &mut st,
            &png(64, 32, |_, _| [5, 5, 5, 255]),
            "old",
            "",
            Some("slim"),
        )
        .unwrap();
        assert_eq!(e.model, MODEL_CLASSIC);
    }

    #[test]
    fn rejected_import_writes_nothing() {
        let (_d, l) = layout();
        let mut st = SkinState::default();
        assert!(import_bytes(&l, &mut st, &png(32, 32, |_, _| [0; 4]), "x", "", None).is_err());
        assert!(st.skins.is_empty());
        assert!(
            !skins_dir(&l).exists() || std::fs::read_dir(skins_dir(&l)).unwrap().next().is_none()
        );
    }

    #[test]
    fn normalize_repairs_untrusted_state() {
        let (_d, l) = layout();
        let json = r#"{"active":"../../evil.png","skins":[
            {"file":"../../evil.png","name":"x"},
            {"file":"0123456789abcdef.png","name":"ok","model":"SLIM"},
            {"file":"0123456789abcdef.png","name":"dup"},
            {"file":"fedcba9876543210.png","name":"missing"}],"revision":5}"#;
        std::fs::create_dir_all(skins_dir(&l)).unwrap();
        std::fs::write(state_path(&l), json).unwrap();
        std::fs::write(skins_dir(&l).join("0123456789abcdef.png"), classic()).unwrap();
        let st = load_state(&l);
        assert_eq!(st.skins.len(), 1);
        assert_eq!(st.skins[0].model, MODEL_SLIM);
        assert_eq!(st.active, None);
        std::fs::write(state_path(&l), "not json").unwrap();
        assert_eq!(load_state(&l).skins.len(), 0);
    }

    #[test]
    fn file_names_cannot_escape() {
        assert!(is_valid_file_name("0123456789abcdef.png"));
        for bad in [
            "0123456789ABCDEF.png",
            "../0123456789abcdef.png",
            "0123456789abcde.png",
            "0123456789abcdef.gif",
            "",
        ] {
            assert!(!is_valid_file_name(bad), "{bad}");
        }
    }

    #[test]
    fn parses_supported_urls() {
        use SkinSource::*;
        assert_eq!(
            parse_source("https://namemc.com/profile/Notch").unwrap(),
            Profile("Notch".into())
        );
        assert_eq!(
            parse_source(" namemc.com/profile/Notch.1 ").unwrap(),
            Profile("Notch".into())
        );
        assert_eq!(
            parse_source("https://de.namemc.com/profile/jeb_?x=1#a").unwrap(),
            Profile("jeb_".into())
        );
        assert_eq!(
            parse_source("https://namemc.com/profile/069a79f4-44e9-4726-a5be-fca90e38aaf5")
                .unwrap(),
            Profile("069a79f444e94726a5befca90e38aaf5".into())
        );
        assert_eq!(
            parse_source("https://namemc.com/skin/1b0bb2e4a3d2d3a0").unwrap(),
            NameMcSkin("1b0bb2e4a3d2d3a0".into())
        );
        assert_eq!(
            parse_source("http://textures.minecraft.net/texture/abc123").unwrap(),
            Texture("https://textures.minecraft.net/texture/abc123".into())
        );
    }

    #[test]
    fn rejects_unsupported_urls() {
        for bad in [
            "",
            "https://evil.com/profile/Notch",
            "https://namemc.com.evil.com/profile/Notch",
            "https://evilnamemc.com/profile/Notch",
            "https://user@namemc.com/profile/Notch",
            "https://namemc.com:8443/profile/Notch",
            "ftp://namemc.com/profile/Notch",
            "file:///C:/skin.png",
            "https://namemc.com/profile/a",
            "https://namemc.com/profile/bad%20name",
            "https://namemc.com/server/hypixel.net",
            "https://textures.minecraft.net/texture/../../x",
            "https://abc.namemc.com/profile/Notch",
        ] {
            assert!(parse_source(bad).is_err(), "{bad}");
        }
        assert!(parse_source(&format!("https://namemc.com/profile/{}", "a".repeat(400))).is_err());
    }

    fn profile_json(skin_url: Option<&str>, slim: bool) -> Vec<u8> {
        let textures = match skin_url {
            Some(u) if slim => format!(
                r#"{{"textures":{{"SKIN":{{"url":"{u}","metadata":{{"model":"slim"}}}}}}}}"#
            ),
            Some(u) => format!(r#"{{"textures":{{"SKIN":{{"url":"{u}"}}}}}}"#),
            None => r#"{"textures":{}}"#.into(),
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(textures);
        format!(r#"{{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","properties":[{{"name":"textures","value":"{b64}"}}]}}"#)
            .into_bytes()
    }

    #[test]
    fn resolves_profile_through_mojang_api() {
        let skin = slim();
        let calls = std::cell::RefCell::new(Vec::new());
        let fetch = |url: &str, _limit: u64| -> std::result::Result<Vec<u8>, String> {
            calls.borrow_mut().push(url.to_string());
            match url {
                "https://api.mojang.com/users/profiles/minecraft/notch" => {
                    Ok(br#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch"}"#.to_vec())
                }
                "https://sessionserver.mojang.com/session/minecraft/profile/069a79f444e94726a5befca90e38aaf5" => {
                    Ok(profile_json(
                        Some("http://textures.minecraft.net/texture/deadbeef"),
                        true,
                    ))
                }
                "https://textures.minecraft.net/texture/deadbeef" => Ok(skin.clone()),
                other => Err(format!("unexpected {other}")),
            }
        };
        let d = resolve(&SkinSource::Profile("notch".into()), &fetch).unwrap();
        assert_eq!(d.name, "Notch");
        assert_eq!(d.model, Some(MODEL_SLIM));
        assert_eq!(d.bytes, skin);
        assert_eq!(calls.borrow().len(), 3);

        let (_d, l) = layout();
        let mut st = SkinState::default();
        let e = import_url_with(&l, &mut st, "https://namemc.com/profile/notch", &fetch).unwrap();
        assert_eq!(e.model, MODEL_SLIM);
        assert_eq!(e.name, "Notch");
    }

    #[test]
    fn profile_errors_are_explained() {
        let fetch = |url: &str, _: u64| -> std::result::Result<Vec<u8>, String> {
            if url.contains("api.mojang.com") {
                Err("HTTP 404 from api.mojang.com".into())
            } else {
                Ok(profile_json(None, false))
            }
        };
        assert!(
            resolve(&SkinSource::Profile("nobody_here".into()), &fetch)
                .unwrap_err()
                .contains("no Minecraft account")
        );
        let e = resolve(
            &SkinSource::Profile("069a79f444e94726a5befca90e38aaf5".into()),
            &fetch,
        )
        .unwrap_err();
        assert!(e.contains("default skin"), "{e}");
        let e = resolve(&SkinSource::NameMcSkin("abc".into()), &fetch).unwrap_err();
        assert!(e.contains("Download") && e.contains("Import Skin"), "{e}");
    }

    #[test]
    fn untrusted_texture_urls_from_profile_are_refused() {
        let body = profile_json(Some("https://evil.com/x.png"), false);
        assert!(parse_profile_textures(&body).is_err());
    }

    #[test]
    fn http_fetch_refuses_other_hosts_without_network() {
        assert!(
            http_fetch("https://namemc.com/x", 10)
                .unwrap_err()
                .contains("refusing")
        );
        assert!(
            http_fetch("http://textures.minecraft.net/texture/a", 10)
                .unwrap_err()
                .contains("refusing")
        );
        assert!(
            http_fetch("https://textures.minecraft.net.evil.com/x", 10)
                .unwrap_err()
                .contains("refusing")
        );
    }

    #[test]
    fn preview_layout() {
        // Encode each texel's position in its colour.
        let img = crate::cosmetics::decode_png(&png(64, 64, |x, y| {
            [
                x as u8,
                y as u8,
                0,
                if y >= 32 || (x >= 32 && y < 16) {
                    0
                } else {
                    255
                },
            ]
        }))
        .unwrap();
        let p = preview(&img, false);
        assert_eq!((p.width, p.height), (36, 32));
        let at = |x: u32, y: u32| {
            &p.pixels[((y * p.width + x) * 4) as usize..((y * p.width + x) * 4 + 4) as usize]
        };
        assert_eq!(at(4, 0), &[8, 8, 0, 255]); // head front, top-left
        assert_eq!(at(0, 8), &[44, 20, 0, 255]); // right arm front
        assert_eq!(at(4, 20), &[4, 20, 0, 255]); // right leg front
        assert_eq!(at(24, 0), &[24, 8, 0, 255]); // head back
        assert_eq!(at(32, 8), &[52, 20, 0, 255]); // right arm back (viewer's right)
        assert_eq!(at(2, 30)[3], 0); // empty corner stays transparent
        // Slim: 3 px arms, right arm back face starts at x=51.
        let s = preview(&img, true);
        let sat = |x: u32, y: u32| s.pixels[((y * s.width + x) * 4) as usize];
        assert_eq!(sat(1, 8), 44);
        assert_eq!(s.pixels[((8 * s.width) * 4 + 3) as usize], 0); // column 0 empty
        assert_eq!(sat(32, 8), 51);
        // Legacy: left limbs mirror the right ones.
        let legacy =
            crate::cosmetics::decode_png(&png(64, 32, |x, y| [x as u8, y as u8, 0, 255])).unwrap();
        let l = preview(&legacy, true);
        let lat = |x: u32, y: u32| l.pixels[((y * l.width + x) * 4) as usize];
        assert_eq!(lat(12, 8), 47); // mirrored right arm front (44..48 reversed)
        assert_eq!(lat(0, 8), 44); // slim ignored for legacy
    }

    #[test]
    fn preview_blends_overlay() {
        let img = crate::cosmetics::decode_png(&png(64, 64, |x, y| {
            if (40..48).contains(&x) && (8..16).contains(&y) {
                [255, 0, 0, 128]
            } else {
                [0, 0, 255, 0]
            }
        }))
        .unwrap();
        let p = preview(&img, false);
        let px = &p.pixels[(4 * 4)..(4 * 4 + 4)];
        assert_eq!(px[3], 255); // base made opaque
        // 50% red over the (now opaque) blue base.
        assert_eq!(px, &[128, 0, 127, 255]);
    }
}
