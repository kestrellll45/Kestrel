//! On-disk layout of a Kestrel installation.
//!
//! Default root is `%LOCALAPPDATA%\Kestrel` on Windows. Nothing here touches
//! the user's regular `.minecraft` directory.

use std::path::{Path, PathBuf};

use crate::error::{IoContext, Result};

#[derive(Debug, Clone)]
pub struct Layout {
    root: PathBuf,
}

impl Layout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Layout { root: root.into() }
    }

    /// Platform default root directory.
    pub fn default_root() -> PathBuf {
        #[cfg(windows)]
        {
            if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
                return PathBuf::from(dir).join("Kestrel");
            }
        }
        #[cfg(not(windows))]
        {
            if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
                return PathBuf::from(dir).join("kestrel");
            }
            if let Some(home) = std::env::var_os("HOME") {
                return PathBuf::from(home).join(".local/share/kestrel");
            }
        }
        PathBuf::from("kestrel-data")
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }
    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions().join(id)
    }
    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }
    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }
    pub fn runtime(&self) -> PathBuf {
        self.root.join("runtime")
    }
    /// The game directory (`--gameDir`): mods, resource packs, options, saves.
    pub fn instance(&self) -> PathBuf {
        self.root.join("instance")
    }
    pub fn mods(&self) -> PathBuf {
        self.instance().join("mods")
    }
    pub fn resourcepacks(&self) -> PathBuf {
        self.instance().join("resourcepacks")
    }
    /// Client mod config + cosmetics live under `<instance>/kestrel`.
    pub fn client_data(&self) -> PathBuf {
        self.instance().join("kestrel")
    }
    pub fn natives(&self, version_id: &str) -> PathBuf {
        self.version_dir(version_id).join("natives")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn launcher_config(&self) -> PathBuf {
        self.root.join("launcher.json")
    }
    pub fn install_state(&self) -> PathBuf {
        self.root.join("install-state.json")
    }
    pub fn accounts(&self) -> PathBuf {
        self.root.join("accounts.dat")
    }
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }
    /// `options.txt` in the game directory (vanilla settings, resource packs).
    pub fn options_txt(&self) -> PathBuf {
        self.instance().join("options.txt")
    }
    /// Mods disabled in the QoL Mod Manager. Outside the game directory, so
    /// Forge never sees them.
    pub fn disabled_mods(&self) -> PathBuf {
        self.root.join("mods-disabled")
    }
    /// Mods removed in the QoL Mod Manager (recoverable until deleted by hand).
    pub fn mod_trash(&self) -> PathBuf {
        self.root.join("mods-trash")
    }

    /// Create every directory the launcher writes into.
    pub fn ensure(&self) -> Result<()> {
        for d in [
            self.versions(),
            self.libraries(),
            self.assets(),
            self.runtime(),
            self.mods(),
            self.resourcepacks(),
            self.client_data(),
            self.logs(),
            self.cache(),
        ] {
            std::fs::create_dir_all(&d).at(&d)?;
        }
        Ok(())
    }
}

/// Join a relative path from untrusted metadata onto `base`, rejecting
/// absolute paths and any `..`/prefix components (path traversal).
pub fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    use std::path::Component;
    let rel_path = Path::new(rel);
    let mut out = base.to_path_buf();
    let mut pushed = false;
    for c in rel_path.components() {
        match c {
            Component::Normal(p) => {
                out.push(p);
                pushed = true;
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    // Backslashes are not separators on Unix, so also reject them explicitly
    // to keep behaviour identical on every platform.
    if !pushed || rel.contains('\\') || rel.contains(':') {
        return None;
    }
    Some(out)
}

/// Write `data` to `path` atomically: write a sibling temp file, then rename.
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).at(parent)?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    std::fs::write(&tmp, data).at(&tmp)?;
    std::fs::rename(&tmp, path).at(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_rejects_traversal() {
        let base = Path::new("/base");
        assert_eq!(
            safe_join(base, "a/b.txt"),
            Some(PathBuf::from("/base/a/b.txt"))
        );
        assert_eq!(safe_join(base, "./a"), Some(PathBuf::from("/base/a")));
        assert_eq!(safe_join(base, "../x"), None);
        assert_eq!(safe_join(base, "a/../../x"), None);
        assert_eq!(safe_join(base, "/etc/passwd"), None);
        assert_eq!(safe_join(base, "a\\..\\x"), None);
        assert_eq!(safe_join(base, "C:/x"), None);
        assert_eq!(safe_join(base, ""), None);
    }

    #[test]
    fn atomic_write_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub/c.json");
        atomic_write(&p, b"1").unwrap();
        atomic_write(&p, b"2").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"2");
        assert!(!dir.path().join("sub/c.json.tmp").exists());
    }
}
