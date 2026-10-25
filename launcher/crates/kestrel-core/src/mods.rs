//! External mod management for the Forge 1.8.9 game directory (QoL page).
//!
//! * Active mods are the `.jar`/`.zip` files Forge loads: `<instance>/mods`
//!   and `<instance>/mods/1.8.9` (FML 1.8.9 scans both, nothing deeper).
//! * Disabling moves a file to `<root>/mods-disabled`, outside the game
//!   directory, so Forge cannot see it; enabling moves it back to `mods`.
//! * Removing moves a file to `<root>/mods-trash/<timestamp>-<name>`;
//!   it can be restored (as a disabled mod) until the user deletes it.
//! * Kestrel's own client mod (`kestrel-*.jar`, written by
//!   [`crate::install::install_mod`] on every Play) is listed as built in and
//!   can't be disabled, removed or replaced here. `kestrel-*.jar` names are
//!   reserved because `install_mod` deletes such files.
//!
//! Nothing is ever overwritten: moves use a hard link + unlink (atomic
//! "create if absent" on NTFS) and fail on any name collision. Jar contents
//! are only read (a few small metadata entries, size-limited), never executed
//! or extracted.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::{Error, IoContext, Result};
use crate::install::is_kestrel_jar;
use crate::paths::Layout;

/// Largest mod file the manager installs.
pub const MAX_MOD_BYTES: u64 = 256 * 1024 * 1024;
/// Largest metadata entry (mcmod.info, MANIFEST.MF) read from a jar.
const MAX_META_BYTES: u64 = 1024 * 1024;
/// Forge's version-specific mods folder for this Minecraft version.
pub const VERSION_SUBDIR: &str = "1.8.9";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModState {
    Enabled,
    Disabled,
}

/// What the manager can tell about a file without running it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compat {
    /// Declares Minecraft 1.8.9 in Forge metadata.
    Supported(String),
    /// Looks like a mod, but nothing says which version it targets.
    Unknown(String),
    /// Made for a different game version or loader.
    Incompatible(String),
    /// Not a readable JAR/ZIP archive.
    Invalid(String),
}

impl Compat {
    pub fn reason(&self) -> &str {
        match self {
            Compat::Supported(r)
            | Compat::Unknown(r)
            | Compat::Incompatible(r)
            | Compat::Invalid(r) => r,
        }
    }
}

/// Metadata read from a mod file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModMeta {
    pub name: Option<String>,
    pub mod_id: Option<String>,
    pub version: Option<String>,
    pub mc_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModEntry {
    pub file_name: String,
    pub path: PathBuf,
    pub state: ModState,
    /// Kestrel's own client mod: managed by the launcher, not removable here.
    pub builtin: bool,
    pub size: u64,
    pub meta: ModMeta,
    pub compat: Compat,
}

impl ModEntry {
    /// Name to show: the mod's declared name, else the file name.
    pub fn display_name(&self) -> &str {
        if self.builtin {
            return "Kestrel client";
        }
        self.meta.name.as_deref().unwrap_or(&self.file_name)
    }
}

#[derive(Debug, Default)]
pub struct ModScan {
    pub mods: Vec<ModEntry>,
    /// Folders that could not be read (shown to the user, never fatal).
    pub errors: Vec<String>,
}

/// List every mod file in the active and disabled folders. Never fails:
/// unreadable folders are reported in [`ModScan::errors`].
pub fn scan(layout: &Layout) -> ModScan {
    let mut out = ModScan::default();
    let dirs = [
        (layout.mods(), ModState::Enabled),
        (layout.mods().join(VERSION_SUBDIR), ModState::Enabled),
        (layout.disabled_mods(), ModState::Disabled),
    ];
    for (dir, state) in dirs {
        // The version subfolder is optional; problems with `mods` itself are
        // reported once, for `mods`.
        if dir.ends_with(VERSION_SUBDIR) && !dir.is_dir() {
            continue;
        }
        let rd = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                out.errors
                    .push(format!("cannot read {}: {e}", dir.display()));
                continue;
            }
        };
        for entry in rd.flatten() {
            let path = entry.path();
            let Ok(md) = entry.metadata() else { continue };
            if !md.is_file() || !is_mod_file(&path) {
                continue;
            }
            let Some(file_name) = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string)
            else {
                out.errors.push(format!(
                    "skipped a file with a non-Unicode name in {}",
                    dir.display()
                ));
                continue;
            };
            let builtin =
                state == ModState::Enabled && dir == layout.mods() && is_kestrel_jar(&path);
            let (meta, compat) = inspect(&path);
            out.mods.push(ModEntry {
                file_name,
                path,
                state,
                builtin,
                size: md.len(),
                meta,
                compat,
            });
        }
    }
    out.mods.sort_by(|a, b| {
        b.builtin.cmp(&a.builtin).then_with(|| {
            a.display_name()
                .to_lowercase()
                .cmp(&b.display_name().to_lowercase())
        })
    });
    out
}

/// Files FML 1.8.9 treats as mod candidates.
fn is_mod_file(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("jar") || e.eq_ignore_ascii_case("zip"))
}

/// `kestrel-*.jar` (any case) is reserved for the built-in client mod.
pub fn is_reserved_name(file_name: &str) -> bool {
    let l = file_name.to_ascii_lowercase();
    l.starts_with("kestrel-") && l.ends_with(".jar")
}

// ---- inspection ---------------------------------------------------------------

/// Read metadata and judge compatibility. Never panics on hostile input.
pub fn inspect(path: &Path) -> (ModMeta, Compat) {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return (
                ModMeta::default(),
                Compat::Invalid(format!("cannot open the file: {e}")),
            );
        }
    };
    let mut zip = match zip::ZipArchive::new(file) {
        Ok(z) => z,
        Err(e) => {
            return (
                ModMeta::default(),
                Compat::Invalid(format!("not a valid JAR/ZIP archive ({e})")),
            );
        }
    };
    let has = |zip: &zip::ZipArchive<fs::File>, name: &str| zip.index_for_name(name).is_some();

    // Other loaders / game versions: these files are unambiguous.
    let other = [
        (
            "fabric.mod.json",
            "Fabric mod: needs the Fabric loader, Kestrel uses Forge 1.8.9",
        ),
        (
            "quilt.mod.json",
            "Quilt mod: needs the Quilt loader, Kestrel uses Forge 1.8.9",
        ),
        (
            "META-INF/neoforge.mods.toml",
            "NeoForge mod for a much newer Minecraft",
        ),
        (
            "META-INF/mods.toml",
            "Forge mod for Minecraft 1.13 or newer",
        ),
        (
            "litemod.json",
            "LiteLoader mod: needs LiteLoader, which Kestrel does not install",
        ),
    ];
    let has_mcmod = has(&zip, "mcmod.info");
    for (file, why) in other {
        if !has_mcmod && has(&zip, file) {
            return (ModMeta::default(), Compat::Incompatible(why.into()));
        }
    }

    let meta = if has_mcmod {
        read_entry(&mut zip, "mcmod.info")
            .and_then(|b| parse_mcmod_info(&b))
            .unwrap_or_default()
    } else {
        ModMeta::default()
    };
    if has_mcmod {
        let compat = match meta.mc_version.as_deref() {
            Some(v) => judge_mc_version(v),
            None => Compat::Unknown(
                "Forge mod without a Minecraft version in mcmod.info; compatibility can't be verified".into(),
            ),
        };
        return (meta, compat);
    }

    let manifest = read_entry(&mut zip, "META-INF/MANIFEST.MF")
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let coremod = manifest.lines().any(|l| {
        let k = l.split(':').next().unwrap_or("").trim();
        k == "FMLCorePlugin" || k == "TweakClass"
    });
    if coremod {
        return (
            meta,
            Compat::Unknown(
                "Forge core mod/tweaker without mcmod.info; compatibility can't be verified".into(),
            ),
        );
    }
    if has(&zip, "plugin.yml") {
        return (
            meta,
            Compat::Incompatible("Bukkit/Spigot server plugin, not a client mod".into()),
        );
    }
    let has_classes = zip
        .file_names()
        .any(|n| n.is_ok_and(|n| n.ends_with(".class")));
    if !has_classes && has(&zip, "pack.mcmeta") {
        return (
            meta,
            Compat::Incompatible(
                "this is a resource pack; add it in Resource packs instead".into(),
            ),
        );
    }
    if !has_classes {
        return (
            meta,
            Compat::Incompatible("contains no Java code, so it is not a mod".into()),
        );
    }
    (
        meta,
        Compat::Unknown("no Forge metadata (mcmod.info); compatibility can't be verified".into()),
    )
}

/// Read one entry by exact name, refusing anything larger than the limit.
fn read_entry(zip: &mut zip::ZipArchive<fs::File>, name: &str) -> Option<Vec<u8>> {
    let entry = zip.by_name(name).ok()?;
    if entry.size() > MAX_META_BYTES {
        return None;
    }
    let mut buf = Vec::new();
    // `take` also bounds a lying header (decompression bomb).
    entry.take(MAX_META_BYTES + 1).read_to_end(&mut buf).ok()?;
    (buf.len() as u64 <= MAX_META_BYTES).then_some(buf)
}

/// mcmod.info is either a JSON array of mods or `{"modListVersion":2,"modList":[...]}`.
/// Values like `${version}` (unexpanded build placeholders) count as missing.
pub fn parse_mcmod_info(bytes: &[u8]) -> Option<ModMeta> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_start_matches('\u{feff}');
    let v: Value = serde_json::from_str(text).ok()?;
    let list = match &v {
        Value::Array(a) => a.clone(),
        Value::Object(o) => o.get("modList").and_then(Value::as_array).cloned()?,
        _ => return None,
    };
    let first = list.into_iter().find(Value::is_object)?;
    let field = |k: &str| {
        first
            .get(k)
            .and_then(Value::as_str)
            .map(|s| {
                s.trim()
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(80)
                    .collect::<String>()
            })
            .filter(|s| !s.is_empty() && !s.contains("${"))
    };
    Some(ModMeta {
        name: field("name"),
        mod_id: field("modid").map(|s| s.to_lowercase()),
        version: field("version"),
        mc_version: field("mcversion"),
    })
}

/// Judge an mcmod.info `mcversion` value such as `1.8.9`, `[1.8.9]`, `1.8`,
/// `1.7.10` or `1.8.8,1.8.9`.
pub fn judge_mc_version(raw: &str) -> Compat {
    let tokens: Vec<&str> = raw
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == 'x' || c == 'X'))
        .map(|t| t.trim_matches('.'))
        .filter(|t| t.starts_with("1."))
        .collect();
    let ok = tokens.iter().any(|t| {
        let t = t.to_ascii_lowercase();
        t == "1.8.9" || t == "1.8" || t == "1.8.x"
    });
    if ok {
        Compat::Supported(format!("made for Minecraft {raw}"))
    } else if tokens.is_empty() {
        Compat::Unknown(format!(
            "unrecognised Minecraft version \"{raw}\"; compatibility can't be verified"
        ))
    } else {
        Compat::Incompatible(format!("made for Minecraft {raw}, not 1.8.9"))
    }
}

// ---- actions ------------------------------------------------------------------

/// Result of checking a file before installing it.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub source: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub meta: ModMeta,
    pub compat: Compat,
}

impl Candidate {
    /// Can be installed (after an explicit confirmation when `Unknown`).
    pub fn installable(&self) -> bool {
        matches!(self.compat, Compat::Supported(_) | Compat::Unknown(_))
    }
}

/// Validate a file the user picked. Errors are user-facing.
pub fn check_candidate(layout: &Layout, source: &Path) -> Result<Candidate> {
    let md = fs::metadata(source).at(source)?;
    if !md.is_file() {
        return Err(Error::Other(format!("{} is not a file", source.display())));
    }
    let file_name = source
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::Other("the file name is not valid Unicode; rename it first".into()))?
        .to_string();
    validate_file_name(&file_name)?;
    if md.len() > MAX_MOD_BYTES {
        return Err(Error::Other(format!(
            "{file_name} is larger than {} MB; that is not a normal mod",
            MAX_MOD_BYTES / (1024 * 1024)
        )));
    }
    if md.len() == 0 {
        return Err(Error::Other(format!("{file_name} is empty")));
    }
    let (meta, compat) = inspect(source);
    let installed = scan(layout);
    if let Some(m) = installed
        .mods
        .iter()
        .find(|m| m.file_name.eq_ignore_ascii_case(&file_name))
    {
        return Err(Error::Other(format!(
            "a mod file named {file_name} is already installed ({}); remove it first",
            state_word(m)
        )));
    }
    if let Some(id) = &meta.mod_id
        && let Some(m) = installed
            .mods
            .iter()
            .find(|m| m.meta.mod_id.as_ref() == Some(id))
    {
        return Err(Error::Other(format!(
            "{} (mod id \"{id}\") is already installed as {} ({}); remove that copy first",
            meta.name.as_deref().unwrap_or(&file_name),
            m.file_name,
            state_word(m)
        )));
    }
    Ok(Candidate {
        source: source.to_path_buf(),
        file_name,
        size: md.len(),
        meta,
        compat,
    })
}

fn state_word(m: &ModEntry) -> &'static str {
    match (m.builtin, m.state) {
        (true, _) => "built in",
        (_, ModState::Enabled) => "enabled",
        (_, ModState::Disabled) => "disabled",
    }
}

fn validate_file_name(name: &str) -> Result<()> {
    let ext_ok = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("jar") || e.eq_ignore_ascii_case("zip"));
    if !ext_ok {
        return Err(Error::Other(format!(
            "{name} is not a .jar or .zip file; Forge only loads those"
        )));
    }
    if is_reserved_name(name) {
        return Err(Error::Other(format!(
            "{name}: file names starting with \"kestrel-\" are reserved for Kestrel's built-in client mod; rename the file"
        )));
    }
    if name
        .chars()
        .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || name.starts_with('.')
    {
        return Err(Error::Other(format!(
            "{name} has characters that are not allowed in a file name"
        )));
    }
    Ok(())
}

/// Copy a checked candidate into `mods`. Refuses `Incompatible`/`Invalid`
/// files and never overwrites: the copy goes to a temporary name Forge
/// ignores, is size-checked, then linked into place only if the name is free.
pub fn install(layout: &Layout, candidate: &Candidate) -> Result<PathBuf> {
    if !candidate.installable() {
        return Err(Error::Other(format!(
            "{} can't be installed: {}",
            candidate.file_name,
            candidate.compat.reason()
        )));
    }
    // Re-check: the folders may have changed since the user picked the file.
    let fresh = check_candidate(layout, &candidate.source)?;
    let mods = layout.mods();
    fs::create_dir_all(&mods).at(&mods)?;
    let dest = mods.join(&fresh.file_name);
    let tmp = mods.join(format!(".{}.kestrel-part", fresh.file_name));
    let _ = fs::remove_file(&tmp);
    let copied = fs::copy(&fresh.source, &tmp).at(&tmp);
    let result = copied.and_then(|n| {
        if n != fresh.size {
            return Err(Error::Other(format!(
                "copy of {} is incomplete ({n} of {} bytes)",
                fresh.file_name, fresh.size
            )));
        }
        move_no_clobber(&tmp, &dest)
    });
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map(|()| dest)
}

/// Move an enabled external mod to the disabled folder.
pub fn disable(layout: &Layout, path: &Path) -> Result<PathBuf> {
    let name = managed_name(
        layout,
        path,
        &[layout.mods(), layout.mods().join(VERSION_SUBDIR)],
    )?;
    let dir = layout.disabled_mods();
    fs::create_dir_all(&dir).at(&dir)?;
    let dest = unique_in(&dir, &name);
    move_no_clobber(path, &dest)?;
    Ok(dest)
}

/// Move a disabled mod back into `mods`. Refuses if a file with that name is
/// active, or if the same mod id is already enabled (Forge would crash on
/// the duplicate).
pub fn enable(layout: &Layout, path: &Path) -> Result<PathBuf> {
    let name = managed_name(layout, path, &[layout.disabled_mods()])?;
    let (meta, _) = inspect(path);
    let active = scan(layout);
    let active: Vec<&ModEntry> = active
        .mods
        .iter()
        .filter(|m| m.state == ModState::Enabled)
        .collect();
    if let Some(m) = active
        .iter()
        .find(|m| m.file_name.eq_ignore_ascii_case(&name))
    {
        return Err(Error::Other(format!(
            "{} is already enabled ({}); remove one of the copies first",
            name,
            m.path.display()
        )));
    }
    if let Some(id) = &meta.mod_id
        && let Some(m) = active.iter().find(|m| m.meta.mod_id.as_ref() == Some(id))
    {
        return Err(Error::Other(format!(
            "mod id \"{id}\" is already enabled as {}; Forge can't load the same mod twice",
            m.file_name
        )));
    }
    let mods = layout.mods();
    fs::create_dir_all(&mods).at(&mods)?;
    let dest = mods.join(&name);
    move_no_clobber(path, &dest)?;
    Ok(dest)
}

/// Move an external mod (enabled or disabled) into the trash.
pub fn remove(layout: &Layout, path: &Path) -> Result<PathBuf> {
    let name = managed_name(
        layout,
        path,
        &[
            layout.mods(),
            layout.mods().join(VERSION_SUBDIR),
            layout.disabled_mods(),
        ],
    )?;
    let dir = layout.mod_trash();
    fs::create_dir_all(&dir).at(&dir)?;
    let stamp = chrono_stamp();
    let dest = unique_in(&dir, &format!("{stamp}-{name}"));
    move_no_clobber(path, &dest)?;
    Ok(dest)
}

#[derive(Debug, Clone)]
pub struct TrashEntry {
    pub path: PathBuf,
    /// Original file name (timestamp prefix removed).
    pub original_name: String,
    pub stored_name: String,
}

/// Mods in the trash, newest first.
pub fn list_trash(layout: &Layout) -> Vec<TrashEntry> {
    let Ok(rd) = fs::read_dir(layout.mod_trash()) else {
        return Vec::new();
    };
    let mut out: Vec<TrashEntry> = rd
        .flatten()
        .filter(|e| e.metadata().is_ok_and(|m| m.is_file()))
        .filter_map(|e| {
            let stored = e.file_name().to_str()?.to_string();
            let original = strip_stamp(&stored).to_string();
            Some(TrashEntry {
                path: e.path(),
                original_name: original,
                stored_name: stored,
            })
        })
        .collect();
    out.sort_by(|a, b| b.stored_name.cmp(&a.stored_name));
    out
}

/// Restore a trashed mod into the disabled folder (enable it afterwards).
pub fn restore(layout: &Layout, path: &Path) -> Result<PathBuf> {
    let stored = managed_name(layout, path, &[layout.mod_trash()])?;
    let dir = layout.disabled_mods();
    fs::create_dir_all(&dir).at(&dir)?;
    let dest = unique_in(&dir, strip_stamp(&stored));
    move_no_clobber(path, &dest)?;
    Ok(dest)
}

/// `path` must be a file directly inside one of `allowed` and must not be the
/// built-in Kestrel mod. Returns its file name.
fn managed_name(layout: &Layout, path: &Path, allowed: &[PathBuf]) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::Other(format!("not a managed mod file: {}", path.display())))?
        .to_string();
    let parent = path.parent().unwrap_or(Path::new(""));
    if !allowed.iter().any(|d| d == parent) {
        return Err(Error::Other(format!(
            "{} is not in a folder the mod manager manages",
            path.display()
        )));
    }
    if parent == layout.mods() && is_kestrel_jar(path) {
        return Err(Error::Other(
            "Kestrel's own client mod is built into Kestrel.exe and reinstalled on every Play; it can't be disabled or removed here".into(),
        ));
    }
    if !path.is_file() {
        return Err(Error::Other(format!(
            "{name} no longer exists; refresh the list"
        )));
    }
    Ok(name)
}

/// First free name in `dir`: `name`, then `name (2).jar`, `name (3).jar`, ...
fn unique_in(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let ext = p.extension().and_then(|s| s.to_str());
    for i in 2..10_000 {
        let candidate = match ext {
            Some(e) => format!("{stem} ({i}).{e}"),
            None => format!("{stem} ({i})"),
        };
        let path = dir.join(candidate);
        if !path.exists() {
            return path;
        }
    }
    dir.join(format!("{stem}-{}", chrono_stamp()))
}

/// Move without ever replacing an existing file. A hard link fails with
/// `AlreadyExists` if `dst` exists (atomic on NTFS and POSIX), then the source
/// is unlinked. Falls back to a checked rename where hard links are not
/// supported (e.g. FAT32).
pub fn move_no_clobber(src: &Path, dst: &Path) -> Result<()> {
    match fs::hard_link(src, dst) {
        Ok(()) => {
            if let Err(e) = fs::remove_file(src) {
                // Usually: Minecraft has the jar open. Undo, keep the original.
                let _ = fs::remove_file(dst);
                return Err(locked_hint(src, e));
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(Error::Other(format!(
            "{} already exists; nothing was moved",
            dst.display()
        ))),
        Err(_) => {
            if dst.exists() {
                return Err(Error::Other(format!(
                    "{} already exists; nothing was moved",
                    dst.display()
                )));
            }
            fs::rename(src, dst).map_err(|e| locked_hint(src, e))
        }
    }
}

fn locked_hint(path: &Path, e: std::io::Error) -> Error {
    if e.kind() == std::io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(32) {
        Error::Other(format!(
            "{} is in use or not writable (is Minecraft still running?): {e}",
            path.display()
        ))
    } else {
        Error::io(path, e)
    }
}

/// `YYYYMMDD-HHMMSS` (UTC) for trash names; sortable.
fn chrono_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn strip_stamp(stored: &str) -> &str {
    // "20261024-205900-name.jar"
    let b = stored.as_bytes();
    if b.len() > 16
        && b[..8].iter().all(u8::is_ascii_digit)
        && b[8] == b'-'
        && b[9..15].iter().all(u8::is_ascii_digit)
        && b[15] == b'-'
    {
        &stored[16..]
    } else {
        stored
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn layout() -> (tempfile::TempDir, Layout) {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        (dir, layout)
    }

    fn jar(path: &Path, entries: &[(&str, &[u8])]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            w.start_file(*name, o).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }

    fn forge_mod(path: &Path, modid: &str, name: &str, mc: &str) {
        let info = format!(
            r#"[{{"modid":"{modid}","name":"{name}","version":"1.2.3","mcversion":"{mc}"}}]"#
        );
        jar(
            path,
            &[
                ("mcmod.info", info.as_bytes()),
                ("com/example/Mod.class", b"\xCA\xFE\xBA\xBE"),
            ],
        );
    }

    #[test]
    fn detects_forge_metadata_and_compatibility() {
        let (_d, l) = layout();
        forge_mod(&l.mods().join("good.jar"), "goodmod", "Good Mod", "1.8.9");
        forge_mod(&l.mods().join("old.jar"), "oldmod", "Old Mod", "1.7.10");
        jar(
            &l.mods().join(VERSION_SUBDIR).join("v2.jar"),
            &[
                (
                    "mcmod.info",
                    br#"{"modListVersion":2,"modList":[{"modid":"V2","name":"Version Two","version":"${version}","mcversion":"[1.8.9]"}]}"#,
                ),
                ("a/B.class", b"x"),
            ],
        );
        jar(
            &l.mods().join("fabric.jar"),
            &[("fabric.mod.json", b"{}"), ("a/B.class", b"x")],
        );
        jar(
            &l.mods().join("modern.jar"),
            &[("META-INF/mods.toml", b""), ("a/B.class", b"x")],
        );
        jar(&l.mods().join("plain.jar"), &[("a/B.class", b"x")]);
        jar(&l.mods().join("pack.zip"), &[("pack.mcmeta", b"{}")]);
        fs::write(l.mods().join("broken.jar"), b"definitely not a zip").unwrap();
        fs::write(l.mods().join("notes.txt"), b"ignored").unwrap();
        fs::write(l.mods().join("kestrel-0.2.1.jar"), b"builtin").unwrap();

        let s = scan(&l);
        assert!(s.errors.is_empty(), "{:?}", s.errors);
        let get = |f: &str| {
            s.mods
                .iter()
                .find(|m| m.file_name == f)
                .unwrap_or_else(|| panic!("{f}"))
        };
        assert_eq!(s.mods.len(), 9, "txt files are not mods");
        assert!(s.mods[0].builtin, "built-in first");

        let good = get("good.jar");
        assert_eq!(good.display_name(), "Good Mod");
        assert_eq!(good.meta.version.as_deref(), Some("1.2.3"));
        assert_eq!(good.meta.mod_id.as_deref(), Some("goodmod"));
        assert!(matches!(good.compat, Compat::Supported(_)));
        assert!(matches!(get("old.jar").compat, Compat::Incompatible(_)));
        let v2 = get("v2.jar");
        assert_eq!(v2.state, ModState::Enabled);
        assert_eq!(v2.meta.version, None, "placeholder version ignored");
        assert_eq!(v2.meta.mod_id.as_deref(), Some("v2"));
        assert!(matches!(v2.compat, Compat::Supported(_)));
        assert!(matches!(get("fabric.jar").compat, Compat::Incompatible(_)));
        assert!(matches!(get("modern.jar").compat, Compat::Incompatible(_)));
        let plain = get("plain.jar");
        assert!(matches!(plain.compat, Compat::Unknown(_)));
        assert_eq!(
            plain.display_name(),
            "plain.jar",
            "falls back to the file name"
        );
        assert!(get("pack.zip").compat.reason().contains("resource pack"));
        assert!(matches!(get("broken.jar").compat, Compat::Invalid(_)));
        let builtin = get("kestrel-0.2.1.jar");
        assert!(builtin.builtin);
        assert!(
            matches!(builtin.compat, Compat::Invalid(_)),
            "fake bytes still reported honestly"
        );
    }

    #[test]
    fn mc_version_rules() {
        for ok in ["1.8.9", "[1.8.9]", "1.8", "1.8.x", "1.8.8,1.8.9", " 1.8.9 "] {
            assert!(matches!(judge_mc_version(ok), Compat::Supported(_)), "{ok}");
        }
        for bad in ["1.7.10", "1.12.2", "[1.9,1.10]"] {
            assert!(
                matches!(judge_mc_version(bad), Compat::Incompatible(_)),
                "{bad}"
            );
        }
        for unk in ["", "any", "${mcversion}"] {
            assert!(matches!(judge_mc_version(unk), Compat::Unknown(_)), "{unk}");
        }
    }

    #[test]
    fn malformed_and_oversized_metadata_fall_back_to_file_name() {
        let (_d, l) = layout();
        jar(
            &l.mods().join("bad-info.jar"),
            &[("mcmod.info", b"{ nope"), ("a/B.class", b"x")],
        );
        let big = vec![b' '; (MAX_META_BYTES + 10) as usize];
        jar(
            &l.mods().join("big-info.jar"),
            &[("mcmod.info", &big), ("a/B.class", b"x")],
        );
        let s = scan(&l);
        for m in &s.mods {
            assert_eq!(m.display_name(), m.file_name);
            assert!(matches!(m.compat, Compat::Unknown(_)), "{:?}", m.compat);
        }
        assert_eq!(
            parse_mcmod_info(b"\xef\xbb\xbf[{\"name\":\"Bom\"}]")
                .unwrap()
                .name
                .as_deref(),
            Some("Bom")
        );
        assert!(parse_mcmod_info(b"42").is_none());
    }

    #[test]
    fn install_copies_and_rejects_bad_files_and_duplicates() {
        let (d, l) = layout();
        let src_dir = d.path().join("downloads");
        let good = src_dir.join("Good-1.0.jar");
        forge_mod(&good, "goodmod", "Good Mod", "1.8.9");
        let c = check_candidate(&l, &good).unwrap();
        assert!(c.installable());
        let dest = install(&l, &c).unwrap();
        assert_eq!(dest, l.mods().join("Good-1.0.jar"));
        assert_eq!(fs::read(&dest).unwrap(), fs::read(&good).unwrap());
        assert!(good.exists(), "the source is copied, not moved");
        let leftovers: Vec<_> = fs::read_dir(l.mods()).unwrap().flatten().collect();
        assert_eq!(leftovers.len(), 1, "no temporary file left behind");

        // Same file name again (any case) -> refused, nothing overwritten.
        let again = src_dir.join("again").join("good-1.0.JAR");
        forge_mod(&again, "other", "Other", "1.8.9");
        assert!(
            check_candidate(&l, &again)
                .unwrap_err()
                .to_string()
                .contains("already installed")
        );
        // Same mod id under another name -> refused.
        let dup = src_dir.join("Good-2.0.jar");
        forge_mod(&dup, "goodmod", "Good Mod", "1.8.9");
        assert!(
            check_candidate(&l, &dup)
                .unwrap_err()
                .to_string()
                .contains("mod id")
        );

        // Invalid archive / other loader / reserved name / wrong extension.
        let broken = src_dir.join("broken.jar");
        fs::write(&broken, b"nope").unwrap();
        let cb = check_candidate(&l, &broken).unwrap();
        assert!(!cb.installable());
        assert!(install(&l, &cb).is_err());
        assert!(!l.mods().join("broken.jar").exists());
        let fabric = src_dir.join("fab.jar");
        jar(&fabric, &[("fabric.mod.json", b"{}"), ("a/B.class", b"x")]);
        assert!(!check_candidate(&l, &fabric).unwrap().installable());
        let reserved = src_dir.join("Kestrel-evil.jar");
        forge_mod(&reserved, "evil", "Evil", "1.8.9");
        assert!(
            check_candidate(&l, &reserved)
                .unwrap_err()
                .to_string()
                .contains("reserved")
        );
        let txt = src_dir.join("readme.txt");
        fs::write(&txt, b"x").unwrap();
        assert!(check_candidate(&l, &txt).is_err());
        let empty = src_dir.join("empty.jar");
        fs::write(&empty, b"").unwrap();
        assert!(check_candidate(&l, &empty).is_err());

        // Unknown compatibility is installable (the UI asks first).
        let plain = src_dir.join("plain.jar");
        jar(&plain, &[("a/B.class", b"x")]);
        let cp = check_candidate(&l, &plain).unwrap();
        assert!(matches!(cp.compat, Compat::Unknown(_)) && cp.installable());
    }

    #[test]
    fn install_creates_missing_mods_folder() {
        let (d, l) = layout();
        assert!(!l.mods().exists());
        assert!(scan(&l).mods.is_empty() && scan(&l).errors.is_empty());
        let src = d.path().join("m.jar");
        forge_mod(&src, "m", "M", "1.8.9");
        install(&l, &check_candidate(&l, &src).unwrap()).unwrap();
        assert!(l.mods().join("m.jar").is_file());
    }

    #[test]
    fn disable_enable_round_trip_and_collisions() {
        let (_d, l) = layout();
        let p = l.mods().join("good.jar");
        forge_mod(&p, "goodmod", "Good", "1.8.9");
        let bytes = fs::read(&p).unwrap();

        let off = disable(&l, &p).unwrap();
        assert_eq!(off, l.disabled_mods().join("good.jar"));
        assert!(!p.exists());
        let s = scan(&l);
        assert_eq!(s.mods.len(), 1);
        assert_eq!(
            s.mods[0].state,
            ModState::Disabled,
            "state persists on disk"
        );

        // A different active copy with the same mod id blocks enabling.
        let twin = l.mods().join("twin.jar");
        forge_mod(&twin, "goodmod", "Good", "1.8.9");
        assert!(
            enable(&l, &off)
                .unwrap_err()
                .to_string()
                .contains("already enabled")
        );
        fs::remove_file(&twin).unwrap();
        // Same file name active blocks enabling, and nothing is overwritten.
        fs::write(l.mods().join("good.jar"), b"other").unwrap();
        assert!(enable(&l, &off).is_err());
        assert_eq!(fs::read(l.mods().join("good.jar")).unwrap(), b"other");
        fs::remove_file(l.mods().join("good.jar")).unwrap();

        let on = enable(&l, &off).unwrap();
        assert_eq!(on, p);
        assert_eq!(fs::read(&p).unwrap(), bytes);
        assert!(!off.exists());
        // Enabling twice is impossible: the disabled file is gone.
        assert!(enable(&l, &off).is_err());

        // Disabling when the disabled folder already has that name keeps both.
        fs::create_dir_all(l.disabled_mods()).unwrap();
        fs::write(l.disabled_mods().join("good.jar"), b"stale").unwrap();
        let off2 = disable(&l, &p).unwrap();
        assert_eq!(off2, l.disabled_mods().join("good (2).jar"));
        assert_eq!(
            fs::read(l.disabled_mods().join("good.jar")).unwrap(),
            b"stale"
        );
    }

    #[test]
    fn remove_goes_to_trash_and_can_be_restored() {
        let (_d, l) = layout();
        let p = l.mods().join("good.jar");
        forge_mod(&p, "goodmod", "Good", "1.8.9");
        let bytes = fs::read(&p).unwrap();
        let trashed = remove(&l, &p).unwrap();
        assert!(!p.exists());
        assert_eq!(trashed.parent().unwrap(), l.mod_trash());
        assert_eq!(fs::read(&trashed).unwrap(), bytes, "nothing is deleted");
        let t = list_trash(&l);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].original_name, "good.jar");
        let back = restore(&l, &t[0].path).unwrap();
        assert_eq!(back, l.disabled_mods().join("good.jar"));
        assert_eq!(fs::read(&back).unwrap(), bytes);
        assert!(list_trash(&l).is_empty());

        // Disabled mods can be removed too.
        let trashed = remove(&l, &back).unwrap();
        assert!(trashed.starts_with(l.mod_trash()));
    }

    #[test]
    fn builtin_and_foreign_paths_are_protected() {
        let (d, l) = layout();
        let builtin = l.mods().join("kestrel-0.2.1.jar");
        fs::create_dir_all(l.mods()).unwrap();
        fs::write(&builtin, b"x").unwrap();
        for r in [disable(&l, &builtin), remove(&l, &builtin)] {
            assert!(r.unwrap_err().to_string().contains("built into Kestrel"));
        }
        assert!(builtin.exists());
        // Files outside the managed folders (e.g. libraries) are refused.
        let lib = l.libraries().join("org/lwjgl/lwjgl.jar");
        fs::create_dir_all(lib.parent().unwrap()).unwrap();
        fs::write(&lib, b"x").unwrap();
        for r in [disable(&l, &lib), remove(&l, &lib), enable(&l, &lib)] {
            assert!(r.unwrap_err().to_string().contains("not in a folder"));
        }
        let outside = d.path().join("x.jar");
        fs::write(&outside, b"x").unwrap();
        assert!(remove(&l, &outside).is_err());
        assert!(lib.exists() && outside.exists());
        // Missing file -> clear error, no panic.
        assert!(
            disable(&l, &l.mods().join("gone.jar"))
                .unwrap_err()
                .to_string()
                .contains("no longer exists")
        );
    }

    #[test]
    fn unreadable_mods_folder_is_reported_not_fatal() {
        let (_d, l) = layout();
        fs::create_dir_all(l.instance()).unwrap();
        // A *file* where the folder should be makes read_dir fail.
        fs::write(l.mods(), b"not a dir").unwrap();
        let s = scan(&l);
        assert!(s.mods.is_empty());
        assert_eq!(s.errors.len(), 1, "{:?}", s.errors);
    }

    #[test]
    fn move_no_clobber_never_overwrites() {
        let d = tempfile::tempdir().unwrap();
        let (a, b) = (d.path().join("a"), d.path().join("b"));
        fs::write(&a, b"A").unwrap();
        fs::write(&b, b"B").unwrap();
        assert!(move_no_clobber(&a, &b).is_err());
        assert_eq!(fs::read(&a).unwrap(), b"A");
        assert_eq!(fs::read(&b).unwrap(), b"B");
        fs::remove_file(&b).unwrap();
        move_no_clobber(&a, &b).unwrap();
        assert!(!a.exists());
        assert_eq!(fs::read(&b).unwrap(), b"A");
    }

    #[test]
    fn trash_names_round_trip() {
        assert_eq!(strip_stamp("20261024-205900-good.jar"), "good.jar");
        assert_eq!(strip_stamp("good.jar"), "good.jar");
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_385), (2025, 10, 24));
        let s = chrono_stamp();
        assert_eq!(s.len(), 15);
        assert_eq!(strip_stamp(&format!("{s}-x.jar")), "x.jar");
    }
}
