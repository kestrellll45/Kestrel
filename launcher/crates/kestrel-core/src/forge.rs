//! Forge 1.8.9 installation (legacy installer format).
//!
//! The 1.8.9 installer jar contains `install_profile.json` with a
//! `versionInfo` block (a version JSON inheriting from vanilla) and the
//! universal jar. No processors run for this generation of Forge, so the
//! installer never has to be executed: we read it as an archive.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::download::{Downloader, Task};
use crate::error::{Error, IoContext, Result};
use crate::maven::Coord;
use crate::mojang::VersionJson;
use crate::paths::{atomic_write, safe_join};

pub fn installer_url(forge_version: &str) -> String {
    format!(
        "https://maven.minecraftforge.net/net/minecraftforge/forge/{v}/forge-{v}-installer.jar",
        v = forge_version
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallProfile {
    install: InstallSection,
    version_info: VersionJson,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallSection {
    path: String,
    file_path: String,
    #[serde(default)]
    minecraft: Option<String>,
}

pub struct ForgeInstall {
    pub version: VersionJson,
    pub universal_path: PathBuf,
}

/// Download (verified against Forge maven's published `.sha1`) and unpack
/// the installer. Idempotent.
pub fn install(
    dl: &Downloader,
    forge_version: &str,
    mc_version: &str,
    cache: &Path,
    libraries: &Path,
) -> Result<ForgeInstall> {
    let url = installer_url(forge_version);
    let expected = String::from_utf8_lossy(&dl.get_bytes(&format!("{url}.sha1"))?)
        .trim()
        .to_ascii_lowercase();
    if expected.len() != 40 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Metadata(format!(
            "bad Forge installer checksum: {expected:?}"
        )));
    }
    let installer = cache.join(format!("forge-{forge_version}-installer.jar"));
    dl.fetch(&Task::new(url, installer.clone(), Some(expected), None))?;
    unpack(&installer, mc_version, libraries)
}

/// Read the install profile and extract the universal jar into `libraries`.
pub fn unpack(installer: &Path, mc_version: &str, libraries: &Path) -> Result<ForgeInstall> {
    let arch_err = |m: String| Error::Archive {
        path: installer.to_path_buf(),
        message: m,
    };
    let f = File::open(installer).at(installer)?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| arch_err(e.to_string()))?;

    let profile: InstallProfile = {
        let mut entry = zip
            .by_name("install_profile.json")
            .map_err(|e| arch_err(format!("install_profile.json: {e}")))?;
        let mut buf = Vec::new();
        entry
            .by_ref()
            .take(4 * 1024 * 1024)
            .read_to_end(&mut buf)
            .at(installer)?;
        serde_json::from_slice(&buf).map_err(|e| Error::json("forge install_profile.json", e))?
    };

    if let Some(mc) = &profile.install.minecraft
        && mc != mc_version
    {
        return Err(Error::Metadata(format!(
            "Forge installer targets Minecraft {mc}, expected {mc_version}"
        )));
    }
    if profile.version_info.main_class.is_empty() {
        return Err(Error::Metadata("Forge versionInfo has no mainClass".into()));
    }

    let coord = Coord::parse(&profile.install.path)?;
    let universal_path = safe_join(libraries, &coord.path())
        .ok_or_else(|| Error::Metadata("bad Forge library path".into()))?;
    if !universal_path.is_file() {
        let mut entry = zip
            .by_name(&profile.install.file_path)
            .map_err(|e| arch_err(format!("{}: {e}", profile.install.file_path)))?;
        let mut buf = Vec::new();
        entry
            .by_ref()
            .take(128 * 1024 * 1024)
            .read_to_end(&mut buf)
            .at(installer)?;
        atomic_write(&universal_path, &buf)?;
    }
    Ok(ForgeInstall {
        version: profile.version_info,
        universal_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn unpacks_legacy_installer() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("installer.jar");
        let mut w = zip::ZipWriter::new(File::create(&jar).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        w.start_file("install_profile.json", o).unwrap();
        w.write_all(
            br#"{"install":{"path":"net.minecraftforge:forge:1.8.9-11.15.1.2318-1.8.9",
              "filePath":"forge-1.8.9-11.15.1.2318-1.8.9-universal.jar","minecraft":"1.8.9"},
              "versionInfo":{"id":"1.8.9-forge1.8.9-11.15.1.2318-1.8.9","inheritsFrom":"1.8.9",
              "mainClass":"net.minecraft.launchwrapper.Launch",
              "minecraftArguments":"--username ${auth_player_name} --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker",
              "libraries":[{"name":"net.minecraftforge:forge:1.8.9-11.15.1.2318-1.8.9","url":"http://files.minecraftforge.net/maven/"},
                           {"name":"net.minecraft:launchwrapper:1.12"}]}}"#,
        )
        .unwrap();
        w.start_file("forge-1.8.9-11.15.1.2318-1.8.9-universal.jar", o)
            .unwrap();
        w.write_all(b"UNIVERSAL").unwrap();
        w.finish().unwrap();

        let libs = dir.path().join("libraries");
        let fi = unpack(&jar, "1.8.9", &libs).unwrap();
        assert_eq!(fi.version.main_class, "net.minecraft.launchwrapper.Launch");
        assert_eq!(fi.version.inherits_from.as_deref(), Some("1.8.9"));
        assert_eq!(std::fs::read(&fi.universal_path).unwrap(), b"UNIVERSAL");
        assert!(fi.universal_path.ends_with(
            "net/minecraftforge/forge/1.8.9-11.15.1.2318-1.8.9/forge-1.8.9-11.15.1.2318-1.8.9.jar"
        ));
        assert!(unpack(&jar, "1.12.2", &libs).is_err());
    }
}
