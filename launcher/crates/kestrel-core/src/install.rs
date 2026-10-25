//! End-to-end installation: vanilla 1.8.9 + Forge + Java + Kestrel mod.
//!
//! The result is an [`InstallState`] persisted to `install-state.json`. A
//! launch only needs this file plus a cheap existence/size check; "Repair"
//! re-runs the installer, which re-hashes every file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::download::{Downloader, Task};
use crate::error::{Error, IoContext, Result};
use crate::mojang::{self, Platform, VersionJson, VersionManifest};
use crate::paths::{Layout, atomic_write};
use crate::progress::Progress;
use crate::{FORGE_VERSION, MC_VERSION, assets, forge, hash, natives, runtime};

/// Bumped when the install procedure changes so old installs are redone.
pub const INSTALL_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: PathBuf,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallState {
    pub schema: u32,
    pub mc_version: String,
    pub forge_version: String,
    pub version_id: String,
    pub main_class: String,
    pub minecraft_arguments: String,
    pub asset_index: String,
    pub classpath: Vec<PathBuf>,
    pub natives_dir: PathBuf,
    /// JVM logging argument with `${path}` already substituted.
    pub logging_arg: Option<String>,
    pub managed_java: Option<PathBuf>,
    pub java_version: Option<String>,
    pub mod_jar: Option<PathBuf>,
    pub mod_sha1: Option<String>,
    /// Files checked by the quick pre-launch verification.
    pub files: Vec<FileRecord>,
    pub installed_at: u64,
}

impl InstallState {
    pub fn load(path: &Path) -> Option<InstallState> {
        let bytes = std::fs::read(path).ok()?;
        match serde_json::from_slice::<InstallState>(&bytes) {
            Ok(s)
                if s.schema == INSTALL_SCHEMA
                    && s.mc_version == MC_VERSION
                    && s.forge_version == FORGE_VERSION =>
            {
                Some(s)
            }
            Ok(_) => None,
            Err(e) => {
                log::warn!("install state unreadable: {e}");
                None
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        atomic_write(
            path,
            &serde_json::to_vec_pretty(self).map_err(|e| Error::json("install state", e))?,
        )
    }

    /// Cheap check: every recorded file exists with the recorded size.
    /// Returns the list of problems (empty = OK).
    pub fn quick_verify(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for f in &self.files {
            match std::fs::metadata(&f.path) {
                Ok(m) if m.len() == f.size => {}
                Ok(m) => problems.push(format!(
                    "{} has size {} (expected {})",
                    f.path.display(),
                    m.len(),
                    f.size
                )),
                Err(_) => problems.push(format!("{} is missing", f.path.display())),
            }
        }
        if !self.natives_dir.is_dir() {
            problems.push(format!(
                "natives directory {} is missing",
                self.natives_dir.display()
            ));
        }
        problems
    }
}

pub struct InstallOptions {
    pub managed_java: bool,
    pub download_threads: usize,
    /// Client mod jar shipped with the launcher.
    pub mod_jar: Option<PathBuf>,
}

/// A client mod jar compiled into the launcher executable.
#[derive(Debug, Clone, Copy)]
pub struct EmbeddedMod {
    /// Jar file name, e.g. `kestrel-0.1.0.jar`.
    pub file_name: &'static str,
    pub bytes: &'static [u8],
}

/// Locate the client mod jar to install, in priority order:
/// 1. `$KESTREL_MOD_JAR` (development override);
/// 2. the jar embedded in the launcher executable — written once to
///    `<root>/cache/client/` so the rest of the install pipeline can work
///    with a file (standalone `Kestrel.exe`);
/// 3. the newest `kestrel-*.jar` in `<exe dir>/client` (unpackaged dev builds).
pub fn resolve_mod(layout: &Layout, embedded: Option<&EmbeddedMod>) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KESTREL_MOD_JAR") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(e) = embedded {
        match materialize_embedded(layout, e) {
            Ok(p) => return Some(p),
            Err(err) => log::error!("cannot unpack embedded client mod: {err}"),
        }
    }
    let dir = std::env::current_exe().ok()?.parent()?.join("client");
    let mut jars: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| is_kestrel_jar(p))
        .collect();
    jars.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    jars.pop()
}

/// Check that bytes look like the Kestrel Forge mod jar: a readable zip
/// containing `mcmod.info` and the mod's entry class.
pub fn validate_mod_jar(bytes: &[u8]) -> Result<()> {
    let mut zip =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| Error::Archive {
            path: PathBuf::from("<embedded mod>"),
            message: e.to_string(),
        })?;
    for required in ["mcmod.info", "com/kestrel/KestrelMod.class"] {
        if zip.by_name(required).is_err() {
            return Err(Error::Other(format!("mod jar has no {required}")));
        }
    }
    Ok(())
}

/// Write the embedded jar to `<root>/cache/client/<file>` unless an identical
/// copy is already there; older embedded jars in that folder are removed.
pub fn materialize_embedded(layout: &Layout, e: &EmbeddedMod) -> Result<PathBuf> {
    let name = Path::new(e.file_name);
    if !is_kestrel_jar(name) || name.components().count() != 1 {
        return Err(Error::Other(format!(
            "invalid embedded mod name {}",
            e.file_name
        )));
    }
    let dir = layout.cache().join("client");
    std::fs::create_dir_all(&dir).at(&dir)?;
    let dest = dir.join(e.file_name);
    let sha = hash::sha1_bytes(e.bytes);
    if !hash::file_matches(&dest, Some(&sha), Some(e.bytes.len() as u64)) {
        atomic_write(&dest, e.bytes)?;
    }
    for entry in std::fs::read_dir(&dir).at(&dir)?.flatten() {
        let p = entry.path();
        if p != dest && is_kestrel_jar(&p) {
            let _ = std::fs::remove_file(&p);
        }
    }
    Ok(dest)
}

pub(crate) fn is_kestrel_jar(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("kestrel-") && n.ends_with(".jar"))
}

pub fn install(
    layout: &Layout,
    opts: &InstallOptions,
    progress: &Progress,
) -> Result<InstallState> {
    layout.ensure()?;
    let dl = Downloader::new();
    let platform = Platform::current();

    // 1. Vanilla version metadata.
    progress.stage("Fetching Minecraft version list", 0);
    let manifest: VersionManifest =
        serde_json::from_slice(&dl.get_bytes(mojang::VERSION_MANIFEST_URL)?)
            .map_err(|e| Error::json("version manifest", e))?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == MC_VERSION)
        .ok_or_else(|| Error::Metadata(format!("{MC_VERSION} not in version manifest")))?;
    let vanilla_json_path = layout
        .version_dir(MC_VERSION)
        .join(format!("{MC_VERSION}.json"));
    dl.fetch(&Task::new(
        entry.url.clone(),
        vanilla_json_path.clone(),
        entry.sha1.clone(),
        None,
    ))?;
    let vanilla = mojang::parse_version(
        &std::fs::read(&vanilla_json_path).at(&vanilla_json_path)?,
        "vanilla version JSON",
    )?;
    progress.check_cancel()?;

    // 2. Forge.
    progress.stage("Installing Forge", 0);
    let forge = forge::install(
        &dl,
        FORGE_VERSION,
        MC_VERSION,
        &layout.cache(),
        &layout.libraries(),
    )?;
    let forge_id = forge.version.id.clone();
    let forge_json_path = layout
        .version_dir(&forge_id)
        .join(format!("{forge_id}.json"));
    atomic_write(
        &forge_json_path,
        &serde_json::to_vec_pretty(&forge.version).map_err(|e| Error::json("forge json", e))?,
    )?;
    let merged: VersionJson = mojang::merge(&forge.version, &vanilla);
    progress.check_cancel()?;

    let mut files = Vec::new();

    // 3. Client jar.
    progress.stage("Downloading Minecraft client", 1);
    let client = merged
        .downloads
        .as_ref()
        .ok_or_else(|| Error::Metadata("no client download".into()))?
        .client
        .clone();
    let client_jar = layout
        .version_dir(MC_VERSION)
        .join(format!("{MC_VERSION}.jar"));
    dl.fetch(&Task::new(
        client.url,
        client_jar.clone(),
        Some(client.sha1),
        Some(client.size),
    ))?;
    progress.add(1);

    // 4. Libraries.
    let libs = mojang::resolve_libraries(&merged.libraries, &layout.libraries(), &platform)?;
    let lib_tasks: Vec<Task> = libs.iter().map(|l| l.task()).collect();
    progress.stage("Downloading libraries", lib_tasks.len() as u64);
    dl.fetch_all(&lib_tasks, opts.download_threads, progress)?;

    // 5. Natives (re-extracted every install so a repair fixes them).
    progress.stage("Extracting natives", 0);
    let natives_dir = layout.natives(&forge_id);
    if natives_dir.exists() {
        std::fs::remove_dir_all(&natives_dir).at(&natives_dir)?;
    }
    std::fs::create_dir_all(&natives_dir).at(&natives_dir)?;
    for l in libs.iter().filter(|l| l.native) {
        natives::extract(&l.path, &natives_dir, &l.extract_exclude)?;
    }

    // 6. Assets.
    progress.stage("Downloading asset index", 0);
    let idx_ref = merged
        .asset_index
        .clone()
        .ok_or_else(|| Error::Metadata("no asset index".into()))?;
    let idx_path = layout
        .assets()
        .join("indexes")
        .join(format!("{}.json", idx_ref.id));
    dl.fetch(&Task::new(
        idx_ref.url.clone(),
        idx_path.clone(),
        Some(idx_ref.sha1.clone()),
        Some(idx_ref.size),
    ))?;
    let index = assets::parse_index(&std::fs::read(&idx_path).at(&idx_path)?)?;
    let asset_tasks = assets::object_tasks(&index, &layout.assets())?;
    progress.stage("Downloading assets", asset_tasks.len() as u64);
    dl.fetch_all(&asset_tasks, opts.download_threads, progress)?;

    // 7. Logging config (Mojang's patched log4j configuration).
    let logging_arg = match merged.logging.as_ref().and_then(|l| l.client.as_ref()) {
        Some(lc) => {
            let p = layout.assets().join("log_configs").join(&lc.file.id);
            if crate::paths::safe_join(&layout.assets().join("log_configs"), &lc.file.id).is_none()
            {
                return Err(Error::Metadata("bad logging config id".into()));
            }
            dl.fetch(&Task::new(
                lc.file.url.clone(),
                p.clone(),
                Some(lc.file.sha1.clone()),
                Some(lc.file.size),
            ))?;
            Some(lc.argument.replace("${path}", &p.to_string_lossy()))
        }
        None => None,
    };

    // 8. Java runtime.
    let (managed_java, java_version) = if opts.managed_java {
        let (p, v) = runtime::ensure(&dl, &layout.runtime(), progress)?;
        (Some(p), Some(v))
    } else {
        (None, None)
    };

    // 9. Client mod.
    progress.stage("Installing Kestrel client mod", 0);
    let (mod_jar, mod_sha1) = install_mod(layout, opts.mod_jar.as_deref())?;

    // Classpath: libraries (non-native) then the client jar, as vanilla does.
    let mut classpath: Vec<PathBuf> = libs
        .iter()
        .filter(|l| !l.native)
        .map(|l| l.path.clone())
        .collect();
    classpath.push(client_jar);
    for p in &classpath {
        let size = std::fs::metadata(p).at(p)?.len();
        files.push(FileRecord {
            path: p.clone(),
            size,
        });
    }

    let state = InstallState {
        schema: INSTALL_SCHEMA,
        mc_version: MC_VERSION.into(),
        forge_version: FORGE_VERSION.into(),
        version_id: forge_id,
        main_class: merged.main_class.clone(),
        minecraft_arguments: merged
            .minecraft_arguments
            .clone()
            .ok_or_else(|| Error::Metadata("no minecraftArguments".into()))?,
        asset_index: idx_ref.id,
        classpath,
        natives_dir,
        logging_arg,
        managed_java,
        java_version,
        mod_jar,
        mod_sha1,
        files,
        installed_at: crate::auth::now_secs(),
    };
    state.save(&layout.install_state())?;
    progress.stage("Installation complete", 0);
    Ok(state)
}

/// Copy the bundled mod into `instance/mods`, replacing older Kestrel jars
/// only (other mods the user placed there are left alone).
pub fn install_mod(
    layout: &Layout,
    src: Option<&Path>,
) -> Result<(Option<PathBuf>, Option<String>)> {
    let Some(src) = src else {
        log::warn!("no bundled Kestrel mod found; the game will start without the client mod");
        return Ok((None, None));
    };
    let mods = layout.mods();
    std::fs::create_dir_all(&mods).at(&mods)?;
    let name = src
        .file_name()
        .ok_or_else(|| Error::Other("bad mod path".into()))?
        .to_os_string();
    let dest = mods.join(&name);
    let src_hash = hash::sha1_file(src)?;
    for e in std::fs::read_dir(&mods).at(&mods)?.flatten() {
        let p = e.path();
        if is_kestrel_jar(&p) && p.file_name() != Some(name.as_os_str()) {
            log::info!("removing old client mod {}", p.display());
            std::fs::remove_file(&p).at(&p)?;
        }
    }
    if !hash::file_matches(&dest, Some(&src_hash), None) {
        let data = std::fs::read(src).at(src)?;
        atomic_write(&dest, &data)?;
    }
    Ok((Some(dest), Some(src_hash)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_mod_is_materialized_once() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path().join("root"));
        layout.ensure().unwrap();
        // Simulate an older embedded jar left by a previous launcher version.
        let cdir = layout.cache().join("client");
        std::fs::create_dir_all(&cdir).unwrap();
        std::fs::write(cdir.join("kestrel-0.0.9.jar"), b"old").unwrap();
        let e = EmbeddedMod {
            file_name: "kestrel-0.1.0.jar",
            bytes: b"jar-bytes",
        };
        let p = materialize_embedded(&layout, &e).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"jar-bytes");
        assert!(!cdir.join("kestrel-0.0.9.jar").exists());
        let mtime = std::fs::metadata(&p).unwrap().modified().unwrap();
        // Second call does not rewrite an identical file.
        materialize_embedded(&layout, &e).unwrap();
        assert_eq!(std::fs::metadata(&p).unwrap().modified().unwrap(), mtime);
        // Unsafe names are rejected.
        let bad = EmbeddedMod {
            file_name: "../kestrel-x.jar",
            bytes: b"x",
        };
        assert!(materialize_embedded(&layout, &bad).is_err());
        assert_eq!(resolve_mod(&layout, Some(&e)), Some(p));
    }

    #[test]
    fn mod_install_replaces_only_kestrel_jars() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path().join("root"));
        layout.ensure().unwrap();
        std::fs::write(layout.mods().join("kestrel-0.0.1.jar"), b"old").unwrap();
        std::fs::write(layout.mods().join("othermod.jar"), b"keep").unwrap();
        let src = dir.path().join("kestrel-0.1.0.jar");
        std::fs::write(&src, b"new").unwrap();

        let (dest, sha) = install_mod(&layout, Some(&src)).unwrap();
        assert_eq!(std::fs::read(dest.unwrap()).unwrap(), b"new");
        assert_eq!(sha.unwrap(), hash::sha1_bytes(b"new"));
        assert!(!layout.mods().join("kestrel-0.0.1.jar").exists());
        assert!(layout.mods().join("othermod.jar").exists());
    }

    #[test]
    fn quick_verify_reports_problems() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.jar");
        std::fs::write(&f, b"1234").unwrap();
        let mut s = InstallState {
            schema: INSTALL_SCHEMA,
            mc_version: MC_VERSION.into(),
            forge_version: FORGE_VERSION.into(),
            version_id: "x".into(),
            main_class: "m".into(),
            minecraft_arguments: String::new(),
            asset_index: "1.8".into(),
            classpath: vec![],
            natives_dir: dir.path().to_path_buf(),
            logging_arg: None,
            managed_java: None,
            java_version: None,
            mod_jar: None,
            mod_sha1: None,
            files: vec![FileRecord {
                path: f.clone(),
                size: 4,
            }],
            installed_at: 0,
        };
        assert!(s.quick_verify().is_empty());
        s.files.push(FileRecord {
            path: dir.path().join("missing"),
            size: 1,
        });
        s.files[0].size = 5;
        assert_eq!(s.quick_verify().len(), 2);
        let p = dir.path().join("state.json");
        s.save(&p).unwrap();
        assert!(InstallState::load(&p).is_some());
    }
}
