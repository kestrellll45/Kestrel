//! Resource packs for the Minecraft 1.8.9 game directory (QoL page).
//!
//! Semantics, verified against the 1.8.9 client bytecode:
//!
//! * Minecraft lists `<instance>/resourcepacks/*.zip` files and folders that
//!   contain `pack.mcmeta`. A pack whose `pack.mcmeta` is missing or invalid
//!   is not shown in game.
//! * `options.txt` stores the selection as `resourcePacks:["a.zip","b"]`,
//!   **lowest priority first**: the in-game menu reverses its on-screen list
//!   (top = highest) before saving, and `FallbackResourceManager` searches
//!   packs from the last to the first, so the last entry wins.
//! * A selected pack whose `pack_format` is not 1 is kept at start-up only if
//!   it is also listed in `incompatibleResourcePacks` (the menu adds it there
//!   when you confirm); otherwise Minecraft drops it with a warning.
//!
//! Packs are untrusted: only `pack.mcmeta` and `pack.png` are read, by exact
//! name, with size limits; nothing is extracted or executed. `options.txt` is
//! edited line by line: only the two resource-pack lines are replaced, every
//! other byte is preserved, and the file is replaced atomically.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::cosmetics::{Rgba, decode_png};
use crate::error::{IoContext, Result};
use crate::paths::{Layout, atomic_write};

/// `pack_format` used by Minecraft 1.8.9.
pub const PACK_FORMAT: i64 = 1;
const MAX_MCMETA_BYTES: u64 = 256 * 1024;
const MAX_ICON_BYTES: u64 = 1024 * 1024;
const MAX_ICON_SIDE: u32 = 512;
const MAX_DESCRIPTION_CHARS: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackKind {
    Zip,
    Folder,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackInfo {
    /// Name Minecraft uses in options.txt: the zip file name (with `.zip`)
    /// or the folder name.
    pub id: String,
    pub kind: PackKind,
    pub path: PathBuf,
    /// Description from pack.mcmeta, formatting codes removed.
    pub description: String,
    pub pack_format: Option<i64>,
    /// `None` when Minecraft can load it; otherwise why it can't.
    pub problem: Option<String>,
    pub has_icon: bool,
}

impl PackInfo {
    pub fn display_name(&self) -> &str {
        match self.kind {
            PackKind::Zip => self.id.strip_suffix(".zip").unwrap_or(&self.id),
            PackKind::Folder => &self.id,
        }
    }
    pub fn is_valid(&self) -> bool {
        self.problem.is_none()
    }
    /// Valid but declares another pack_format (Minecraft asks to confirm).
    pub fn is_other_version(&self) -> bool {
        self.is_valid() && self.pack_format != Some(PACK_FORMAT)
    }
}

#[derive(Debug, Default)]
pub struct PackScan {
    pub packs: Vec<PackInfo>,
    pub errors: Vec<String>,
}

/// List resource packs. Never fails; problems are reported per pack or in
/// [`PackScan::errors`].
pub fn scan(layout: &Layout) -> PackScan {
    let dir = layout.resourcepacks();
    let mut out = PackScan::default();
    let rd = match fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return out,
        Err(e) => {
            out.errors
                .push(format!("cannot read {}: {e}", dir.display()));
            return out;
        }
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(md) = entry.metadata() else { continue };
        let Some(id) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            out.errors
                .push("skipped a pack with a non-Unicode name".into());
            continue;
        };
        if md.is_dir() {
            out.packs.push(inspect_folder(&path, id));
        } else if md.is_file() && id.to_ascii_lowercase().ends_with(".zip") {
            // Minecraft only checks for ".zip" (case-sensitive) — match it.
            if !id.ends_with(".zip") {
                continue;
            }
            out.packs.push(inspect_zip(&path, id));
        }
    }
    out.packs.sort_by_key(|p| p.display_name().to_lowercase());
    out
}

fn inspect_zip(path: &Path, id: String) -> PackInfo {
    let mut info = PackInfo {
        id,
        kind: PackKind::Zip,
        path: path.to_path_buf(),
        description: String::new(),
        pack_format: None,
        problem: None,
        has_icon: false,
    };
    let mut zip = match fs::File::open(path)
        .map_err(|e| e.to_string())
        .and_then(|f| zip::ZipArchive::new(f).map_err(|e| e.to_string()))
    {
        Ok(z) => z,
        Err(e) => {
            info.problem = Some(format!("not a valid ZIP archive ({e})"));
            return info;
        }
    };
    info.has_icon = zip.index_for_name("pack.png").is_some();
    match read_zip_entry(&mut zip, "pack.mcmeta", MAX_MCMETA_BYTES) {
        Ok(Some(bytes)) => apply_mcmeta(&mut info, &bytes),
        Ok(None) => {
            info.problem =
                Some("no pack.mcmeta at the top of the zip; Minecraft won't list it".into())
        }
        Err(e) => info.problem = Some(e),
    }
    info
}

fn inspect_folder(path: &Path, id: String) -> PackInfo {
    let mut info = PackInfo {
        id,
        kind: PackKind::Folder,
        path: path.to_path_buf(),
        description: String::new(),
        pack_format: None,
        problem: None,
        has_icon: path.join("pack.png").is_file(),
    };
    match read_file_limited(&path.join("pack.mcmeta"), MAX_MCMETA_BYTES) {
        Ok(Some(bytes)) => apply_mcmeta(&mut info, &bytes),
        Ok(None) => {
            info.problem = Some("folder has no pack.mcmeta; Minecraft won't list it".into())
        }
        Err(e) => info.problem = Some(e),
    }
    info
}

fn apply_mcmeta(info: &mut PackInfo, bytes: &[u8]) {
    match parse_mcmeta(bytes) {
        Ok((format, description)) => {
            info.pack_format = Some(format);
            info.description = description;
        }
        Err(e) => info.problem = Some(e),
    }
}

/// Parse `{"pack":{"pack_format":1,"description":...}}`. The description may
/// be a string or a JSON text component.
pub fn parse_mcmeta(bytes: &[u8]) -> std::result::Result<(i64, String), String> {
    let text = String::from_utf8_lossy(bytes);
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("pack.mcmeta is not valid JSON ({e})"))?;
    let pack = v
        .get("pack")
        .and_then(Value::as_object)
        .ok_or("pack.mcmeta has no \"pack\" section")?;
    let format = pack
        .get("pack_format")
        .and_then(Value::as_i64)
        .ok_or("pack.mcmeta has no numeric pack_format")?;
    let mut description = String::new();
    if let Some(d) = pack.get("description") {
        flatten_text(d, &mut description, 0);
    }
    Ok((format, clean_text(&description)))
}

fn flatten_text(v: &Value, out: &mut String, depth: usize) {
    if depth > 16 || out.len() > 4 * MAX_DESCRIPTION_CHARS {
        return;
    }
    match v {
        Value::String(s) => out.push_str(s),
        Value::Array(a) => a.iter().for_each(|x| flatten_text(x, out, depth + 1)),
        Value::Object(o) => {
            if let Some(t) = o.get("text") {
                flatten_text(t, out, depth + 1);
            }
            if let Some(e) = o.get("extra") {
                flatten_text(e, out, depth + 1);
            }
        }
        Value::Number(n) => out.push_str(&n.to_string()),
        _ => {}
    }
}

/// Remove `§x` formatting codes and control characters; bound the length.
fn clean_text(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else if c == '\n' {
            out.push('\n');
        } else if !c.is_control() {
            out.push(c);
        }
        if out.chars().count() >= MAX_DESCRIPTION_CHARS {
            out.push('…');
            break;
        }
    }
    out.trim().to_string()
}

fn read_zip_entry(
    zip: &mut zip::ZipArchive<fs::File>,
    name: &str,
    limit: u64,
) -> std::result::Result<Option<Vec<u8>>, String> {
    let entry = match zip.by_name(name) {
        Ok(e) => e,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(format!("cannot read {name} ({e})")),
    };
    if entry.size() > limit {
        return Err(format!("{name} is too large ({} bytes)", entry.size()));
    }
    let mut buf = Vec::new();
    entry
        .take(limit + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("cannot read {name} ({e})"))?;
    if buf.len() as u64 > limit {
        return Err(format!("{name} is too large"));
    }
    Ok(Some(buf))
}

fn read_file_limited(path: &Path, limit: u64) -> std::result::Result<Option<Vec<u8>>, String> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    match fs::metadata(path) {
        Ok(m) if !m.is_file() => return Ok(None),
        Ok(m) if m.len() > limit => return Err(format!("{name} is too large ({} bytes)", m.len())),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {name} ({e})")),
    }
    let mut buf = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(limit + 1).read_to_end(&mut buf))
        .map_err(|e| format!("cannot read {name} ({e})"))?;
    Ok(Some(buf))
}

/// Decode a pack's `pack.png` for the preview (size-limited).
pub fn load_icon(pack: &PackInfo) -> std::result::Result<Rgba, String> {
    let bytes = match pack.kind {
        PackKind::Zip => {
            let f = fs::File::open(&pack.path).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
            read_zip_entry(&mut zip, "pack.png", MAX_ICON_BYTES)?
        }
        PackKind::Folder => read_file_limited(&pack.path.join("pack.png"), MAX_ICON_BYTES)?,
    }
    .ok_or("no pack.png")?;
    let img = decode_png(&bytes)?;
    if img.width == 0 || img.height == 0 || img.width > MAX_ICON_SIDE || img.height > MAX_ICON_SIDE
    {
        return Err(format!(
            "pack.png is {}x{} (max {MAX_ICON_SIDE})",
            img.width, img.height
        ));
    }
    Ok(img)
}

// ---- options.txt -----------------------------------------------------------------

const KEY_PACKS: &[u8] = b"resourcePacks:";
const KEY_INCOMPATIBLE: &[u8] = b"incompatibleResourcePacks:";

/// The resource pack selection from options.txt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// Enabled packs, **lowest priority first** (options.txt order).
    pub enabled: Vec<String>,
    /// Packs acknowledged as made for another version.
    pub incompatible: Vec<String>,
    /// Set when a line could not be parsed (treated as empty; the line is
    /// only rewritten if you change the selection).
    pub warning: Option<String>,
}

impl Selection {
    /// Enabled packs as Minecraft's menu shows them: highest priority first.
    pub fn by_priority(&self) -> Vec<String> {
        self.enabled.iter().rev().cloned().collect()
    }
}

/// Read the selection. A missing options.txt means nothing is enabled yet.
pub fn read_selection(layout: &Layout) -> Result<Selection> {
    let path = layout.options_txt();
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Selection::default()),
        Err(e) => return Err(crate::error::Error::io(path, e)),
    };
    Ok(parse_selection(&bytes))
}

pub fn parse_selection(bytes: &[u8]) -> Selection {
    let mut sel = Selection::default();
    // Minecraft applies lines in order, so the last occurrence wins.
    for line in split_lines(bytes) {
        let content = strip_eol(line);
        for (key, target) in [(KEY_PACKS, 0), (KEY_INCOMPATIBLE, 1)] {
            if let Some(value) = content.strip_prefix(key) {
                match parse_list(value) {
                    Ok(list) => {
                        if target == 0 {
                            sel.enabled = list;
                        } else {
                            sel.incompatible = list;
                        }
                    }
                    Err(e) => {
                        sel.warning = Some(format!(
                            "options.txt has an unreadable {} line ({e}); it will be replaced if you change packs",
                            String::from_utf8_lossy(&key[..key.len() - 1])
                        ));
                        if target == 0 {
                            sel.enabled.clear();
                        } else {
                            sel.incompatible.clear();
                        }
                    }
                }
            }
        }
    }
    sel
}

fn parse_list(value: &[u8]) -> std::result::Result<Vec<String>, String> {
    // Minecraft writes options.txt in the platform charset; JSON escapes keep
    // our own writes ASCII. Fall back to Latin-1 for non-UTF-8 bytes.
    let text = match std::str::from_utf8(value) {
        Ok(s) => s.to_string(),
        Err(_) => value.iter().map(|&b| b as char).collect(),
    };
    let v: Vec<String> = serde_json::from_str(text.trim()).map_err(|e| e.to_string())?;
    Ok(v)
}

/// Write the selection, preserving every other line of options.txt.
/// `enabled_low_to_high` uses options.txt order (last = highest priority).
/// `incompatibleResourcePacks` is recomputed like Minecraft's menu does:
/// enabled packs whose pack_format is not 1; for packs that are not on disk
/// any previous acknowledgement is kept.
pub fn write_selection(
    layout: &Layout,
    enabled_low_to_high: &[String],
    packs: &[PackInfo],
) -> Result<()> {
    let path = layout.options_txt();
    let old = match fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(crate::error::Error::io(path, e)),
    };
    let previous = parse_selection(&old);
    let mut seen = std::collections::HashSet::new();
    let enabled: Vec<String> = enabled_low_to_high
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .cloned()
        .collect();
    let incompatible: Vec<String> = enabled
        .iter()
        .filter(|id| match packs.iter().find(|p| &p.id == *id) {
            Some(p) => p.pack_format != Some(PACK_FORMAT),
            None => previous.incompatible.contains(id),
        })
        .cloned()
        .collect();
    let data = replace_lines(
        &old,
        &[
            (KEY_PACKS, format_list(&enabled)),
            (KEY_INCOMPATIBLE, format_list(&incompatible)),
        ],
    );
    let parent = layout.instance();
    fs::create_dir_all(&parent).at(&parent)?;
    atomic_write(&path, &data)
}

/// Replace (first occurrence) or append each `key` line; drop later
/// duplicates. Every other line is copied byte for byte.
fn replace_lines(old: &[u8], replacements: &[(&[u8], String)]) -> Vec<u8> {
    let eol: &[u8] = if old.windows(2).any(|w| w == b"\r\n") {
        b"\r\n"
    } else {
        b"\n"
    };
    let mut out = Vec::with_capacity(old.len() + 256);
    let mut done = vec![false; replacements.len()];
    for line in split_lines(old) {
        let content = strip_eol(line);
        let hit = replacements
            .iter()
            .position(|(k, _)| content.starts_with(k));
        match hit {
            Some(i) if done[i] => {} // duplicate key: drop
            Some(i) => {
                done[i] = true;
                out.extend_from_slice(replacements[i].0);
                out.extend_from_slice(replacements[i].1.as_bytes());
                out.extend_from_slice(&line[content.len()..]); // original line ending
                if line.len() == content.len() {
                    out.extend_from_slice(eol);
                }
            }
            None => out.extend_from_slice(line),
        }
    }
    for (i, (k, v)) in replacements.iter().enumerate() {
        if !done[i] {
            if !out.is_empty() && !out.ends_with(b"\n") {
                out.extend_from_slice(eol);
            }
            out.extend_from_slice(k);
            out.extend_from_slice(v.as_bytes());
            out.extend_from_slice(eol);
        }
    }
    out
}

fn split_lines(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes.split_inclusive(|&b| b == b'\n')
}

fn strip_eol(line: &[u8]) -> &[u8] {
    let l = line.strip_suffix(b"\n").unwrap_or(line);
    l.strip_suffix(b"\r").unwrap_or(l)
}

/// JSON array of strings, pure ASCII (non-ASCII as `\uXXXX`) so the bytes are
/// identical in every charset Minecraft might read options.txt with.
pub fn format_list(items: &[String]) -> String {
    let mut s = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push('"');
        for c in item.chars() {
            match c {
                '"' => s.push_str("\\\""),
                '\\' => s.push_str("\\\\"),
                c if (c as u32) < 0x20 || (c as u32) > 0x7e => {
                    let mut buf = [0u16; 2];
                    for u in c.encode_utf16(&mut buf) {
                        s.push_str(&format!("\\u{u:04x}"));
                    }
                }
                c => s.push(c),
            }
        }
        s.push('"');
    }
    s.push(']');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn layout() -> (tempfile::TempDir, Layout) {
        let d = tempfile::tempdir().unwrap();
        let l = Layout::new(d.path());
        (d, l)
    }

    fn zip_pack(path: &Path, entries: &[(&str, &[u8])]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        for (n, d) in entries {
            w.start_file(*n, o).unwrap();
            w.write_all(d).unwrap();
        }
        w.finish().unwrap();
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .unwrap()
            .write_image_data(&vec![200u8; (w * h * 4) as usize])
            .unwrap();
        out
    }

    const META1: &[u8] =
        br#"{"pack":{"pack_format":1,"description":"\u00a7aFaithful \u00a7r32x"}}"#;

    #[test]
    fn detects_zip_and_folder_packs() {
        let (_d, l) = layout();
        let rp = l.resourcepacks();
        zip_pack(
            &rp.join("Faithful.zip"),
            &[("pack.mcmeta", META1), ("pack.png", &png(16, 16))],
        );
        zip_pack(
            &rp.join("Modern.zip"),
            &[("pack.mcmeta", br#"{"pack":{"pack_format":3,"description":{"text":"Made ","extra":[{"text":"for 1.11"}]}}}"#)],
        );
        zip_pack(&rp.join("NoMeta.zip"), &[("assets/minecraft/x.png", b"x")]);
        zip_pack(&rp.join("Nested.zip"), &[("Nested/pack.mcmeta", META1)]);
        zip_pack(&rp.join("BadJson.zip"), &[("pack.mcmeta", b"{ nope")]);
        zip_pack(
            &rp.join("NoFormat.zip"),
            &[("pack.mcmeta", br#"{"pack":{"description":"x"}}"#)],
        );
        fs::write(rp.join("Corrupt.zip"), b"not a zip").unwrap();
        fs::write(rp.join("readme.txt"), b"ignored").unwrap();
        fs::write(rp.join("Upper.ZIP"), b"ignored like Minecraft").unwrap();
        fs::create_dir_all(rp.join("Folder Pack")).unwrap();
        fs::write(rp.join("Folder Pack/pack.mcmeta"), META1).unwrap();
        fs::create_dir_all(rp.join("Empty Folder")).unwrap();

        let s = scan(&l);
        assert!(s.errors.is_empty());
        let get = |id: &str| {
            s.packs
                .iter()
                .find(|p| p.id == id)
                .unwrap_or_else(|| panic!("{id}"))
        };
        assert_eq!(s.packs.len(), 9);

        let f = get("Faithful.zip");
        assert!(f.is_valid() && !f.is_other_version());
        assert_eq!(f.display_name(), "Faithful");
        assert_eq!(f.description, "Faithful 32x", "formatting codes stripped");
        assert!(f.has_icon);
        assert_eq!(load_icon(f).unwrap().width, 16);

        let m = get("Modern.zip");
        assert!(m.is_valid() && m.is_other_version());
        assert_eq!(m.description, "Made for 1.11");
        assert!(!m.has_icon);
        assert!(load_icon(m).is_err(), "missing icon handled");

        for bad in [
            "NoMeta.zip",
            "Nested.zip",
            "BadJson.zip",
            "NoFormat.zip",
            "Corrupt.zip",
            "Empty Folder",
        ] {
            assert!(!get(bad).is_valid(), "{bad} should be invalid");
        }
        let folder = get("Folder Pack");
        assert_eq!(folder.kind, PackKind::Folder);
        assert!(folder.is_valid());
    }

    #[test]
    fn oversized_metadata_and_icons_are_refused() {
        let (_d, l) = layout();
        let rp = l.resourcepacks();
        let mut big = br#"{"pack":{"pack_format":1,"description":""#.to_vec();
        big.extend(std::iter::repeat_n(b'a', (MAX_MCMETA_BYTES + 1) as usize));
        big.extend_from_slice(br#""}}"#);
        zip_pack(&rp.join("Big.zip"), &[("pack.mcmeta", &big)]);
        zip_pack(
            &rp.join("BigIcon.zip"),
            &[("pack.mcmeta", META1), ("pack.png", &png(1024, 1024))],
        );
        zip_pack(
            &rp.join("FakeIcon.zip"),
            &[("pack.mcmeta", META1), ("pack.png", b"GIF89a")],
        );
        let s = scan(&l);
        let get = |id: &str| s.packs.iter().find(|p| p.id == id).unwrap();
        assert!(
            get("Big.zip")
                .problem
                .as_ref()
                .unwrap()
                .contains("too large")
        );
        assert!(load_icon(get("BigIcon.zip")).is_err());
        assert!(load_icon(get("FakeIcon.zip")).is_err());
        // Long descriptions are bounded.
        let long = format!(
            r#"{{"pack":{{"pack_format":1,"description":"{}"}}}}"#,
            "x".repeat(5000)
        );
        assert!(
            parse_mcmeta(long.as_bytes()).unwrap().1.chars().count() <= MAX_DESCRIPTION_CHARS + 1
        );
    }

    #[test]
    fn missing_folder_is_empty_not_an_error() {
        let (_d, l) = layout();
        let s = scan(&l);
        assert!(s.packs.is_empty() && s.errors.is_empty());
        assert_eq!(read_selection(&l).unwrap(), Selection::default());
    }

    fn pack(id: &str, format: i64) -> PackInfo {
        PackInfo {
            id: id.into(),
            kind: PackKind::Zip,
            path: PathBuf::from(id),
            description: String::new(),
            pack_format: Some(format),
            problem: None,
            has_icon: false,
        }
    }

    #[test]
    fn selection_round_trip_preserves_other_settings_and_order() {
        let (_d, l) = layout();
        fs::create_dir_all(l.instance()).unwrap();
        let original = b"invertYMouse:false\r\nmouseSensitivity:0.5\r\nresourcePacks:[\"A.zip\"]\r\nincompatibleResourcePacks:[]\r\nlastServer:mc.hypixel.net\r\nkey_key.attack:-100\r\n";
        fs::write(l.options_txt(), original).unwrap();
        assert_eq!(read_selection(&l).unwrap().enabled, vec!["A.zip"]);

        let packs = vec![pack("A.zip", 1), pack("B.zip", 1), pack("Mod é.zip", 3)];
        // options order: lowest priority first -> "Mod é.zip" wins.
        let order: Vec<String> = ["A.zip", "B.zip", "Mod é.zip"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        write_selection(&l, &order, &packs).unwrap();
        let text = fs::read(l.options_txt()).unwrap();
        let s = String::from_utf8(text.clone()).unwrap();
        assert!(s.is_ascii(), "written as ASCII JSON escapes");
        assert!(
            s.contains("resourcePacks:[\"A.zip\",\"B.zip\",\"Mod \\u00e9.zip\"]\r\n"),
            "{s}"
        );
        assert!(
            s.contains("incompatibleResourcePacks:[\"Mod \\u00e9.zip\"]\r\n"),
            "{s}"
        );
        // Everything else is byte-for-byte identical and in place.
        let expect_rest = "invertYMouse:false\r\nmouseSensitivity:0.5\r\n";
        assert!(s.starts_with(expect_rest));
        assert!(s.ends_with("lastServer:mc.hypixel.net\r\nkey_key.attack:-100\r\n"));
        assert_eq!(s.lines().count(), 6);

        let back = read_selection(&l).unwrap();
        assert_eq!(back.enabled, order);
        assert_eq!(
            back.by_priority()[0],
            "Mod é.zip",
            "last in options.txt = top priority"
        );
        assert_eq!(back.incompatible, vec!["Mod é.zip"]);

        // Disable everything.
        write_selection(&l, &[], &packs).unwrap();
        let s = String::from_utf8(fs::read(l.options_txt()).unwrap()).unwrap();
        assert!(
            s.contains("resourcePacks:[]\r\n") && s.contains("incompatibleResourcePacks:[]\r\n")
        );
        assert!(s.contains("lastServer:mc.hypixel.net"));
    }

    #[test]
    fn creates_options_and_appends_missing_keys() {
        let (_d, l) = layout();
        write_selection(&l, &["A.zip".to_string()], &[pack("A.zip", 1)]).unwrap();
        assert_eq!(
            fs::read_to_string(l.options_txt()).unwrap(),
            "resourcePacks:[\"A.zip\"]\nincompatibleResourcePacks:[]\n"
        );
        // File without trailing newline and without the keys.
        fs::write(l.options_txt(), b"fov:0.0").unwrap();
        write_selection(&l, &[], &[]).unwrap();
        assert_eq!(
            fs::read_to_string(l.options_txt()).unwrap(),
            "fov:0.0\nresourcePacks:[]\nincompatibleResourcePacks:[]\n"
        );
    }

    #[test]
    fn malformed_and_duplicate_lines() {
        let sel = parse_selection(b"resourcePacks:[\"A.zip\"]\nresourcePacks:not json\n");
        assert!(sel.enabled.is_empty());
        assert!(sel.warning.is_some());
        let sel = parse_selection(b"resourcePacks:[\"A.zip\"]\nresourcePacks:[\"B.zip\"]\n");
        assert_eq!(
            sel.enabled,
            vec!["B.zip"],
            "last occurrence wins, like Minecraft"
        );
        // Latin-1 (Windows charset) names are read.
        let sel = parse_selection(b"resourcePacks:[\"Caf\xe9.zip\"]\n");
        assert_eq!(sel.enabled, vec!["Café.zip"]);

        let (_d, l) = layout();
        fs::create_dir_all(l.instance()).unwrap();
        fs::write(
            l.options_txt(),
            b"resourcePacks:[\"A.zip\"]\ngamma:0.5\nresourcePacks:[\"B.zip\"]\n",
        )
        .unwrap();
        write_selection(&l, &["C.zip".into(), "C.zip".into()], &[]).unwrap();
        assert_eq!(
            fs::read_to_string(l.options_txt()).unwrap(),
            "resourcePacks:[\"C.zip\"]\ngamma:0.5\nincompatibleResourcePacks:[]\n",
            "duplicates removed, no duplicate entries written"
        );
    }

    #[test]
    fn missing_packs_keep_their_acknowledgement() {
        let (_d, l) = layout();
        fs::create_dir_all(l.instance()).unwrap();
        fs::write(
            l.options_txt(),
            b"resourcePacks:[\"Gone.zip\",\"A.zip\"]\nincompatibleResourcePacks:[\"Gone.zip\"]\n",
        )
        .unwrap();
        write_selection(
            &l,
            &["Gone.zip".into(), "A.zip".into()],
            &[pack("A.zip", 1)],
        )
        .unwrap();
        let sel = read_selection(&l).unwrap();
        assert_eq!(sel.incompatible, vec!["Gone.zip"]);
    }

    #[test]
    fn format_list_escapes() {
        assert_eq!(format_list(&[]), "[]");
        assert_eq!(
            format_list(&["a\"b\\c".into(), "\u{1F600}".into()]),
            r#"["a\"b\\c","\ud83d\ude00"]"#
        );
        let parsed: Vec<String> =
            serde_json::from_str(&format_list(&["\u{1F600}x".into()])).unwrap();
        assert_eq!(parsed, vec!["\u{1F600}x"]);
    }
}
