//! Managed Java 8 runtime from Mojang's official `jre-legacy` component —
//! the same runtime the vanilla launcher uses for 1.8.9.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::download::{Downloader, Task};
use crate::error::{Error, IoContext, Result};
use crate::paths::{atomic_write, safe_join};
use crate::progress::Progress;

pub const RUNTIME_INDEX_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
pub const COMPONENT: &str = "jre-legacy";

/// Mojang's platform key for the host.
pub fn platform_key() -> &'static str {
    if cfg!(windows) {
        if cfg!(target_arch = "x86") {
            "windows-x86"
        } else if cfg!(target_arch = "aarch64") {
            "windows-arm64"
        } else {
            "windows-x64"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-os-arm64"
        } else {
            "mac-os"
        }
    } else if cfg!(target_arch = "x86") {
        "linux-i386"
    } else {
        "linux"
    }
}

#[derive(Debug, Deserialize)]
struct RuntimeEntry {
    manifest: ManifestRef,
    version: VersionInfo,
}

#[derive(Debug, Deserialize)]
struct ManifestRef {
    sha1: String,
    size: u64,
    url: String,
}

#[derive(Debug, Deserialize)]
struct VersionInfo {
    name: String,
}

#[derive(Debug, Deserialize)]
struct FileManifest {
    files: BTreeMap<String, FileEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum FileEntry {
    File {
        #[serde(default)]
        executable: bool,
        downloads: FileDownloads,
    },
    Directory,
    Link {
        #[allow(dead_code)]
        target: String,
    },
}

#[derive(Debug, Deserialize)]
struct FileDownloads {
    raw: RawDownload,
}

#[derive(Debug, Deserialize)]
struct RawDownload {
    sha1: String,
    size: u64,
    url: String,
}

/// Pick the `jre-legacy` entry for a platform from Mojang's runtime index.
///
/// Platforms are parsed individually and leniently: the index also contains
/// keys (e.g. `gamecore`) whose layout we do not need, and those must not
/// break parsing of the platform we do need.
fn select_runtime(index_bytes: &[u8], platform: &str) -> Result<RuntimeEntry> {
    let index: BTreeMap<String, serde_json::Value> =
        serde_json::from_slice(index_bytes).map_err(|e| Error::json("java runtime index", e))?;
    let components = index
        .get(platform)
        .ok_or_else(|| Error::Metadata(format!("no Java runtimes listed for {platform}")))?;
    let list = components
        .get(COMPONENT)
        .cloned()
        .ok_or_else(|| Error::Metadata(format!("no {COMPONENT} runtime for {platform}")))?;
    let mut entries: Vec<RuntimeEntry> =
        serde_json::from_value(list).map_err(|e| Error::json("java runtime entry", e))?;
    if entries.is_empty() {
        return Err(Error::Metadata(format!(
            "no {COMPONENT} runtime for {platform}"
        )));
    }
    Ok(entries.swap_remove(0))
}

pub fn runtime_dir(runtime_root: &Path) -> PathBuf {
    runtime_root.join(COMPONENT)
}

/// Path of the Java executable inside the managed runtime.
pub fn java_executable(runtime_root: &Path) -> PathBuf {
    let bin = runtime_dir(runtime_root).join("bin");
    if cfg!(windows) {
        bin.join("java.exe")
    } else {
        bin.join("java")
    }
}

/// Ensure the managed runtime is installed and verified. Returns the java
/// executable path and the runtime version name.
pub fn ensure(
    dl: &Downloader,
    runtime_root: &Path,
    progress: &Progress,
) -> Result<(PathBuf, String)> {
    progress.stage("Resolving Java runtime", 0);
    let entry = select_runtime(&dl.get_bytes(RUNTIME_INDEX_URL)?, platform_key())?;

    let manifest_path = runtime_root.join(format!("{COMPONENT}.manifest.json"));
    dl.fetch(&Task::new(
        entry.manifest.url.clone(),
        manifest_path.clone(),
        Some(entry.manifest.sha1.clone()),
        Some(entry.manifest.size),
    ))?;
    let manifest: FileManifest =
        serde_json::from_slice(&std::fs::read(&manifest_path).at(&manifest_path)?)
            .map_err(|e| Error::json("java runtime manifest", e))?;

    let dir = runtime_dir(runtime_root);
    let mut tasks = Vec::new();
    let mut executables = Vec::new();
    for (rel, f) in &manifest.files {
        let path = safe_join(&dir, rel)
            .ok_or_else(|| Error::Metadata(format!("bad runtime path {rel}")))?;
        match f {
            FileEntry::Directory => std::fs::create_dir_all(&path).at(&path)?,
            FileEntry::File {
                executable,
                downloads,
            } => {
                if *executable {
                    executables.push(path.clone());
                }
                tasks.push(Task::new(
                    downloads.raw.url.clone(),
                    path,
                    Some(downloads.raw.sha1.clone()),
                    Some(downloads.raw.size),
                ));
            }
            // Symlinks only occur in the macOS bundles; Windows has none.
            FileEntry::Link { .. } => {}
        }
    }
    progress.stage(
        format!("Downloading Java {}", entry.version.name),
        tasks.len() as u64,
    );
    dl.fetch_all(&tasks, 8, progress)?;

    #[cfg(unix)]
    for p in &executables {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).at(p)?;
    }
    #[cfg(not(unix))]
    let _ = executables;

    atomic_write(&dir.join(".kestrel-version"), entry.version.name.as_bytes())?;
    Ok((java_executable(runtime_root), entry.version.name.clone()))
}

/// Run `java -version` and return the reported version string
/// (e.g. `1.8.0_51`). Used to validate custom Java paths.
pub fn probe_version(java: &Path) -> Result<String> {
    let mut cmd = Command::new(java);
    cmd.arg("-version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd
        .output()
        .map_err(|e| Error::Launch(format!("cannot run {}: {e}", java.display())))?;
    let text =
        String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
    parse_version_output(&text)
        .ok_or_else(|| Error::Launch(format!("unrecognised `java -version` output: {text}")))
}

pub fn parse_version_output(text: &str) -> Option<String> {
    let start = text.find("version \"")? + "version \"".len();
    let end = text[start..].find('"')? + start;
    Some(text[start..end].to_string())
}

/// 1.8.9 + Forge 11.15 + LaunchWrapper require Java 8.
pub fn is_java8(version: &str) -> bool {
    version.starts_with("1.8.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_java_version() {
        let s = "java version \"1.8.0_51\"\nJava(TM) SE Runtime Environment (build 1.8.0_51-b16)";
        assert_eq!(parse_version_output(s).as_deref(), Some("1.8.0_51"));
        assert!(is_java8("1.8.0_51"));
        assert!(!is_java8("17.0.2"));
        assert_eq!(parse_version_output("garbage"), None);
    }

    #[test]
    fn selects_runtime_from_real_index() {
        let bytes = include_bytes!("../tests/fixtures/java-runtime-all.json");
        let e = select_runtime(bytes, "windows-x64").unwrap();
        assert!(e.version.name.starts_with("8u"), "{}", e.version.name);
        assert_eq!(e.manifest.sha1.len(), 40);
        assert!(
            crate::download::validate_url(&e.manifest.url).is_ok(),
            "{}",
            e.manifest.url
        );
        assert!(select_runtime(bytes, "linux").is_ok());
        assert!(select_runtime(bytes, "nonexistent").is_err());
    }

    #[test]
    fn parses_manifest_shapes() {
        let m: FileManifest = serde_json::from_str(
            r#"{"files":{"bin":{"type":"directory"},
            "bin/java.exe":{"type":"file","executable":true,"downloads":{"raw":{"sha1":"a","size":1,"url":"https://piston-data.mojang.com/x"}}},
            "legal":{"type":"link","target":"../x"}}}"#,
        )
        .unwrap();
        assert_eq!(m.files.len(), 3);
    }
}
