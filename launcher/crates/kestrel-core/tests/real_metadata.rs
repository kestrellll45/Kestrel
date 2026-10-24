//! Exercises the install/launch pipeline against real Mojang and Forge
//! metadata (copied into tests/fixtures), without network access.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use kestrel_core::auth::Session;
use kestrel_core::config::LauncherConfig;
use kestrel_core::download::validate_url;
use kestrel_core::install::{INSTALL_SCHEMA, InstallState};
use kestrel_core::mojang::{self, Platform, VersionManifest};
use kestrel_core::paths::Layout;
use kestrel_core::{FORGE_VERSION, MC_VERSION, forge, launch};

const VANILLA: &[u8] = include_bytes!("fixtures/vanilla-1.8.9.json");
const FORGE_PROFILE: &[u8] = include_bytes!("fixtures/forge-1.8.9-install_profile.json");
const MANIFEST: &[u8] = include_bytes!("fixtures/version_manifest_v2-subset.json");

fn fake_installer(dir: &Path) -> PathBuf {
    let jar = dir.join("forge-installer.jar");
    let mut w = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
    let o = zip::write::SimpleFileOptions::default();
    w.start_file("install_profile.json", o).unwrap();
    w.write_all(FORGE_PROFILE).unwrap();
    w.start_file("forge-1.8.9-11.15.1.2318-1.8.9-universal.jar", o)
        .unwrap();
    w.write_all(b"universal").unwrap();
    w.finish().unwrap();
    jar
}

#[test]
fn manifest_lists_1_8_9_on_allowed_host() {
    let m: VersionManifest = serde_json::from_slice(MANIFEST).unwrap();
    let e = m.versions.iter().find(|v| v.id == MC_VERSION).unwrap();
    validate_url(&e.url).unwrap();
    assert_eq!(e.sha1.as_deref().map(str::len), Some(40));
}

#[test]
fn vanilla_libraries_resolve_for_windows() {
    let v = mojang::parse_version(VANILLA, "vanilla").unwrap();
    assert_eq!(v.id, "1.8.9");
    assert_eq!(v.asset_index.as_ref().unwrap().id, "1.8");
    assert!(v.logging.as_ref().unwrap().client.is_some());
    let libs =
        mojang::resolve_libraries(&v.libraries, Path::new("/L"), &Platform::windows_x64()).unwrap();
    for l in &libs {
        for u in &l.urls {
            validate_url(u).unwrap_or_else(|e| panic!("{}: {e}", l.key));
        }
        assert!(l.sha1.is_some(), "vanilla library without hash: {}", l.key);
    }
    let natives: Vec<_> = libs
        .iter()
        .filter(|l| l.native)
        .map(|l| l.key.as_str())
        .collect();
    assert!(
        natives
            .iter()
            .any(|k| k.starts_with("org.lwjgl.lwjgl:lwjgl-platform")),
        "{natives:?}"
    );
    assert!(
        natives
            .iter()
            .any(|k| k.starts_with("net.java.jinput:jinput-platform")),
        "{natives:?}"
    );
    assert!(
        natives.iter().any(|k| k.ends_with("natives-windows-64")),
        "{natives:?}"
    );
    // No macOS-only libraries on Windows.
    assert!(!libs.iter().any(|l| l.key.contains("natives-osx")));
}

#[test]
fn forge_profile_merges_and_launches() {
    let dir = tempfile::tempdir().unwrap();
    let libs_root = dir.path().join("libraries");
    let fi = forge::unpack(&fake_installer(dir.path()), MC_VERSION, &libs_root).unwrap();
    assert_eq!(fi.version.id, "1.8.9-forge1.8.9-11.15.1.2318-1.8.9");
    assert!(fi.universal_path.is_file());

    let vanilla = mojang::parse_version(VANILLA, "vanilla").unwrap();
    let merged = mojang::merge(&fi.version, &vanilla);
    assert_eq!(merged.main_class, "net.minecraft.launchwrapper.Launch");
    let args = merged.minecraft_arguments.clone().unwrap();
    assert!(args.contains("--tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker"));

    let libs =
        mojang::resolve_libraries(&merged.libraries, &libs_root, &Platform::windows_x64()).unwrap();
    let mut keys = HashSet::new();
    for l in &libs {
        assert!(keys.insert(l.key.clone()), "duplicate library {}", l.key);
        for u in &l.urls {
            validate_url(u).unwrap_or_else(|e| panic!("{}: {e}", l.key));
        }
    }
    assert!(keys.contains("net.minecraft:launchwrapper"));
    assert!(
        !keys.contains("jline:jline"),
        "clientreq=false must be skipped"
    );
    let scala = libs
        .iter()
        .find(|l| l.key == "org.scala-lang:scala-library")
        .unwrap();
    assert!(scala.sha1.is_some() && !scala.alt_sha1.is_empty());
    assert!(scala.urls.len() >= 2, "expected mirrors: {:?}", scala.urls);
    // Forge's own jar resolves to the extracted universal jar.
    let forge_lib = libs
        .iter()
        .find(|l| l.key == "net.minecraftforge:forge")
        .unwrap();
    assert_eq!(forge_lib.path, fi.universal_path);
    // Shared vanilla/Forge libraries (e.g. guava) keep vanilla's hashes.
    if let Some(g) = libs.iter().find(|l| l.key == "com.google.guava:guava") {
        assert!(g.sha1.is_some());
    }

    let layout = Layout::new(dir.path().join("root"));
    let state = InstallState {
        schema: INSTALL_SCHEMA,
        mc_version: MC_VERSION.into(),
        forge_version: FORGE_VERSION.into(),
        version_id: merged.id.clone(),
        main_class: merged.main_class.clone(),
        minecraft_arguments: args,
        asset_index: merged.asset_index.unwrap().id,
        classpath: libs
            .iter()
            .filter(|l| !l.native)
            .map(|l| l.path.clone())
            .collect(),
        natives_dir: layout.natives(&merged.id),
        logging_arg: None,
        managed_java: None,
        java_version: None,
        mod_jar: None,
        mod_sha1: None,
        files: vec![],
        installed_at: 0,
    };
    let spec = launch::build(
        &state,
        &Session::offline("Kellen"),
        &LauncherConfig::default(),
        Path::new("java"),
        &layout,
        &Platform::windows_x64(),
    )
    .unwrap();
    let a = &spec.args;
    assert!(a.iter().any(|x| x == "--tweakClass"));
    assert!(
        !a.iter().any(|x| x.contains("${")),
        "unsubstituted placeholder in {a:?}"
    );
    assert_eq!(
        a[a.iter().position(|x| x == "--version").unwrap() + 1],
        merged.id
    );
}
