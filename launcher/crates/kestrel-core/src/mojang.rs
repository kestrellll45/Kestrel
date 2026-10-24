//! Mojang version metadata (manifest + per-version JSON) and library resolution.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::download::Task;
use crate::error::{Error, Result};
use crate::maven::Coord;

pub const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const LIBRARIES_BASE: &str = "https://libraries.minecraft.net/";
pub const RESOURCES_BASE: &str = "https://resources.download.minecraft.net/";

#[derive(Debug, Deserialize)]
pub struct VersionManifest {
    pub versions: Vec<ManifestEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestEntry {
    pub id: String,
    pub url: String,
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(default)]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub main_class: String,
    /// Legacy (pre-1.13) argument template.
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub downloads: Option<VersionDownloads>,
    #[serde(default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub logging: Option<Logging>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionDownloads {
    pub client: Artifact,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Logging {
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingClient {
    pub argument: String,
    pub file: LoggingFile,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Artifact {
    #[serde(default)]
    pub path: Option<String>,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default)]
    pub rules: Option<Vec<Rule>>,
    #[serde(default)]
    pub extract: Option<Extract>,
    /// Legacy Forge: base maven URL.
    #[serde(default)]
    pub url: Option<String>,
    /// Legacy Forge: accepted SHA-1 values.
    #[serde(default)]
    pub checksums: Option<Vec<String>>,
    /// Legacy Forge: whether the client needs it (absent = true).
    #[serde(default)]
    pub clientreq: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default)]
    pub os: Option<OsRule>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    // `version` regexes exist in old metadata (OS X 10.5); we treat a
    // version-constrained rule as non-matching, which only affects macOS.
    #[serde(default)]
    pub version: Option<String>,
}

/// The platform we evaluate rules for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub os: &'static str,
    pub arch_bits: &'static str,
    pub arch: &'static str,
}

impl Platform {
    pub fn current() -> Platform {
        let os = if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "osx"
        } else {
            "linux"
        };
        let (arch_bits, arch) = if cfg!(target_pointer_width = "64") {
            (
                "64",
                if cfg!(target_arch = "aarch64") {
                    "arm64"
                } else {
                    "x86_64"
                },
            )
        } else {
            ("32", "x86")
        };
        Platform {
            os,
            arch_bits,
            arch,
        }
    }

    pub fn windows_x64() -> Platform {
        Platform {
            os: "windows",
            arch_bits: "64",
            arch: "x86_64",
        }
    }

    pub fn classpath_separator(&self) -> &'static str {
        if self.os == "windows" { ";" } else { ":" }
    }
}

/// Evaluate a rule list the way the vanilla launcher does: default deny when
/// rules exist, last matching rule wins.
pub fn rules_allow(rules: Option<&[Rule]>, platform: &Platform) -> bool {
    let Some(rules) = rules else { return true };
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for r in rules {
        let matches = match &r.os {
            None => true,
            Some(os) => {
                os.version.is_none()
                    && os.name.as_deref().is_none_or(|n| n == platform.os)
                    && os.arch.as_deref().is_none_or(|a| {
                        a == platform.arch || (a == "x86" && platform.arch_bits == "32")
                    })
            }
        };
        if matches {
            allowed = r.action == RuleAction::Allow;
        }
    }
    allowed
}

/// A resolved library file to put on the classpath or extract as natives.
#[derive(Debug, Clone)]
pub struct ResolvedLib {
    pub key: String,
    pub path: PathBuf,
    /// Candidate URLs in order (first is preferred).
    pub urls: Vec<String>,
    pub sha1: Option<String>,
    /// Extra acceptable hashes (legacy Forge `checksums`).
    pub alt_sha1: Vec<String>,
    pub size: Option<u64>,
    pub native: bool,
    pub extract_exclude: Vec<String>,
}

impl ResolvedLib {
    pub fn task(&self) -> Task {
        Task {
            url: self.urls.first().cloned().unwrap_or_default(),
            mirrors: self.urls.iter().skip(1).cloned().collect(),
            path: self.path.clone(),
            sha1: self.sha1.clone(),
            alt_sha1: self.alt_sha1.clone(),
            size: self.size,
        }
    }
}

/// Resolve libraries for a platform into classpath and native entries.
pub fn resolve_libraries(
    libs: &[Library],
    lib_root: &Path,
    platform: &Platform,
) -> Result<Vec<ResolvedLib>> {
    let mut out = Vec::new();
    for lib in libs {
        if !rules_allow(lib.rules.as_deref(), platform) {
            continue;
        }
        if lib.clientreq == Some(false) {
            continue;
        }
        let coord = Coord::parse(&lib.name)?;

        if let Some(natives) = &lib.natives {
            let Some(classifier) = natives.get(platform.os) else {
                continue;
            };
            let classifier = classifier.replace("${arch}", platform.arch_bits);
            let art = lib
                .downloads
                .as_ref()
                .and_then(|d| d.classifiers.as_ref())
                .and_then(|c| c.get(&classifier));
            let ncoord = coord.with_classifier(&classifier);
            let rel = art
                .and_then(|a| a.path.clone())
                .unwrap_or_else(|| ncoord.path());
            out.push(ResolvedLib {
                key: ncoord.key(),
                path: crate::paths::safe_join(lib_root, &rel)
                    .ok_or_else(|| Error::Metadata(format!("bad library path {rel}")))?,
                urls: vec![
                    art.map(|a| a.url.clone())
                        .unwrap_or_else(|| format!("{LIBRARIES_BASE}{}", ncoord.path())),
                ],
                sha1: art.map(|a| a.sha1.clone()),
                alt_sha1: Vec::new(),
                size: art.map(|a| a.size),
                native: true,
                extract_exclude: lib
                    .extract
                    .as_ref()
                    .map(|e| e.exclude.clone())
                    .unwrap_or_default(),
            });
            // A natives-only entry may also carry a main artifact (newer
            // metadata); 1.8.9's LWJGL natives do not, so fall through only
            // when an artifact is explicitly present.
            if lib
                .downloads
                .as_ref()
                .and_then(|d| d.artifact.as_ref())
                .is_none()
            {
                continue;
            }
        }

        let art = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());
        let rel = art
            .and_then(|a| a.path.clone())
            .unwrap_or_else(|| coord.path());
        let mut urls = Vec::new();
        match art {
            Some(a) if !a.url.is_empty() => urls.push(a.url.clone()),
            _ => {
                if let Some(base) = &lib.url {
                    let base = if base.ends_with('/') {
                        base.clone()
                    } else {
                        format!("{base}/")
                    };
                    urls.push(format!("{base}{}", coord.path()));
                }
                urls.push(format!("{LIBRARIES_BASE}{}", coord.path()));
                urls.push(format!("https://repo1.maven.org/maven2/{}", coord.path()));
            }
        }
        let checksums = lib.checksums.clone().unwrap_or_default();
        out.push(ResolvedLib {
            key: coord.key(),
            path: crate::paths::safe_join(lib_root, &rel)
                .ok_or_else(|| Error::Metadata(format!("bad library path {rel}")))?,
            urls,
            sha1: art
                .map(|a| a.sha1.clone())
                .or_else(|| checksums.first().cloned()),
            alt_sha1: checksums.into_iter().skip(1).collect(),
            size: art.map(|a| a.size),
            native: false,
            extract_exclude: Vec::new(),
        });
    }
    Ok(out)
}

/// Merge a child (Forge) version onto its parent (vanilla). Child libraries
/// take precedence; duplicates by `group:artifact[:classifier]` are dropped.
pub fn merge(child: &VersionJson, parent: &VersionJson) -> VersionJson {
    let mut seen = HashSet::new();
    let mut libraries = Vec::new();
    for lib in child.libraries.iter().chain(parent.libraries.iter()) {
        let key = match Coord::parse(&lib.name) {
            Ok(c) => {
                // natives entries share group:artifact with nothing else; key
                // them by name so both artifact and natives survive.
                if lib.natives.is_some() {
                    format!("{}#natives", c.key())
                } else {
                    c.key()
                }
            }
            Err(_) => lib.name.clone(),
        };
        if seen.insert(key) {
            // Legacy Forge lists some vanilla libraries again without
            // download hashes; when the exact same coordinate exists in the
            // parent, use the parent's (hashed) entry instead.
            let better = (lib.downloads.is_none())
                .then(|| {
                    parent
                        .libraries
                        .iter()
                        .find(|p| p.name == lib.name && p.downloads.is_some())
                })
                .flatten();
            libraries.push(better.unwrap_or(lib).clone());
        }
    }
    VersionJson {
        id: child.id.clone(),
        inherits_from: None,
        main_class: if child.main_class.is_empty() {
            parent.main_class.clone()
        } else {
            child.main_class.clone()
        },
        minecraft_arguments: child
            .minecraft_arguments
            .clone()
            .or_else(|| parent.minecraft_arguments.clone()),
        libraries,
        downloads: child.downloads.clone().or_else(|| parent.downloads.clone()),
        asset_index: child
            .asset_index
            .clone()
            .or_else(|| parent.asset_index.clone()),
        assets: child.assets.clone().or_else(|| parent.assets.clone()),
        logging: child.logging.clone().or_else(|| parent.logging.clone()),
    }
}

pub fn parse_version(bytes: &[u8], context: &str) -> Result<VersionJson> {
    serde_json::from_slice(bytes).map_err(|e| Error::json(context, e))
}

/// Asset index (`assets/indexes/<id>.json`).
#[derive(Debug, Deserialize)]
pub struct AssetIndex {
    pub objects: BTreeMap<String, AssetObject>,
    #[serde(default, rename = "virtual")]
    pub is_virtual: bool,
    #[serde(default)]
    pub map_to_resources: bool,
}

#[derive(Debug, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Representative subset of the real 1.8.9 version JSON structure.
    pub const VANILLA_1_8_9: &str = r#"{
      "id": "1.8.9",
      "mainClass": "net.minecraft.client.main.Main",
      "minecraftArguments": "--username ${auth_player_name} --version ${version_name} --gameDir ${game_directory} --assetsDir ${assets_root} --assetIndex ${assets_index_name} --uuid ${auth_uuid} --accessToken ${auth_access_token} --userProperties ${user_properties} --userType ${user_type}",
      "assets": "1.8",
      "assetIndex": {"id": "1.8", "sha1": "aaaa", "size": 10, "url": "https://launchermeta.mojang.com/v1/packages/aaaa/1.8.json"},
      "downloads": {"client": {"sha1": "cccc", "size": 100, "url": "https://launcher.mojang.com/v1/objects/cccc/client.jar"}},
      "logging": {"client": {"argument": "-Dlog4j.configurationFile=${path}", "file": {"id": "client-1.7.xml", "sha1": "llll", "size": 5, "url": "https://launcher.mojang.com/v1/objects/llll/client-1.7.xml"}, "type": "log4j2-xml"}},
      "libraries": [
        {"name": "com.mojang:netty:1.6",
         "downloads": {"artifact": {"path": "com/mojang/netty/1.6/netty-1.6.jar", "sha1": "n1", "size": 1, "url": "https://libraries.minecraft.net/com/mojang/netty/1.6/netty-1.6.jar"}}},
        {"name": "org.lwjgl.lwjgl:lwjgl:2.9.4-nightly-20150209",
         "rules": [{"action": "allow"}, {"action": "disallow", "os": {"name": "osx"}}],
         "downloads": {"artifact": {"path": "org/lwjgl/lwjgl/lwjgl/2.9.4-nightly-20150209/lwjgl-2.9.4-nightly-20150209.jar", "sha1": "l1", "size": 1, "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/lwjgl/2.9.4-nightly-20150209/lwjgl-2.9.4-nightly-20150209.jar"}}},
        {"name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4-nightly-20150209",
         "natives": {"linux": "natives-linux", "osx": "natives-osx", "windows": "natives-windows"},
         "extract": {"exclude": ["META-INF/"]},
         "rules": [{"action": "allow"}, {"action": "disallow", "os": {"name": "osx"}}],
         "downloads": {"classifiers": {
            "natives-windows": {"path": "org/lwjgl/lwjgl/lwjgl-platform/2.9.4-nightly-20150209/lwjgl-platform-2.9.4-nightly-20150209-natives-windows.jar", "sha1": "w1", "size": 2, "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/lwjgl-platform/2.9.4-nightly-20150209/lwjgl-platform-2.9.4-nightly-20150209-natives-windows.jar"},
            "natives-linux": {"path": "org/lwjgl/lwjgl/lwjgl-platform/2.9.4-nightly-20150209/lwjgl-platform-2.9.4-nightly-20150209-natives-linux.jar", "sha1": "x1", "size": 2, "url": "https://libraries.minecraft.net/x.jar"}}}},
        {"name": "tv.twitch:twitch-platform:6.5",
         "natives": {"linux": "natives-linux", "osx": "natives-osx", "windows": "natives-windows-${arch}"},
         "extract": {"exclude": ["META-INF/"]},
         "rules": [{"action": "allow"}, {"action": "disallow", "os": {"name": "linux"}}],
         "downloads": {"classifiers": {
            "natives-windows-64": {"path": "tv/twitch/twitch-platform/6.5/twitch-platform-6.5-natives-windows-64.jar", "sha1": "t64", "size": 3, "url": "https://libraries.minecraft.net/tv/twitch/twitch-platform/6.5/twitch-platform-6.5-natives-windows-64.jar"}}}},
        {"name": "com.apple:osx-only:1.0",
         "rules": [{"action": "allow", "os": {"name": "osx"}}],
         "downloads": {"artifact": {"path": "com/apple/osx-only/1.0/osx-only-1.0.jar", "sha1": "o", "size": 1, "url": "https://libraries.minecraft.net/o.jar"}}}
      ]
    }"#;

    #[test]
    fn resolves_windows_libraries() {
        let v = parse_version(VANILLA_1_8_9.as_bytes(), "test").unwrap();
        let libs =
            resolve_libraries(&v.libraries, Path::new("/L"), &Platform::windows_x64()).unwrap();
        let keys: Vec<_> = libs.iter().map(|l| (l.key.as_str(), l.native)).collect();
        assert_eq!(
            keys,
            vec![
                ("com.mojang:netty", false),
                ("org.lwjgl.lwjgl:lwjgl", false),
                ("org.lwjgl.lwjgl:lwjgl-platform:natives-windows", true),
                ("tv.twitch:twitch-platform:natives-windows-64", true),
            ]
        );
        let lwjgl_natives = &libs[2];
        assert_eq!(lwjgl_natives.sha1.as_deref(), Some("w1"));
        assert_eq!(lwjgl_natives.extract_exclude, vec!["META-INF/".to_string()]);
    }

    #[test]
    fn rules_semantics() {
        let w = Platform::windows_x64();
        let osx_only = vec![Rule {
            action: RuleAction::Allow,
            os: Some(OsRule {
                name: Some("osx".into()),
                arch: None,
                version: None,
            }),
        }];
        assert!(!rules_allow(Some(&osx_only), &w));
        assert!(rules_allow(None, &w));
        assert!(rules_allow(Some(&[]), &w));
    }

    #[test]
    fn merge_prefers_child() {
        let parent = parse_version(VANILLA_1_8_9.as_bytes(), "p").unwrap();
        let child = VersionJson {
            id: "forge".into(),
            main_class: "net.minecraft.launchwrapper.Launch".into(),
            minecraft_arguments: Some("--tweakClass X".into()),
            libraries: vec![Library {
                name: "com.mojang:netty:9.9".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let m = merge(&child, &parent);
        assert_eq!(m.main_class, "net.minecraft.launchwrapper.Launch");
        assert_eq!(m.minecraft_arguments.as_deref(), Some("--tweakClass X"));
        assert_eq!(m.libraries[0].name, "com.mojang:netty:9.9");
        assert_eq!(
            m.libraries
                .iter()
                .filter(|l| l.name.starts_with("com.mojang:netty"))
                .count(),
            1
        );
        assert!(m.downloads.is_some());
        assert_eq!(m.assets.as_deref(), Some("1.8"));
    }

    #[test]
    fn legacy_forge_library_urls() {
        let lib = Library {
            name: "com.typesafe.akka:akka-actor_2.11:2.3.3".into(),
            url: Some("http://files.minecraftforge.net/maven/".into()),
            checksums: Some(vec!["a".into(), "b".into()]),
            ..Default::default()
        };
        let r = resolve_libraries(&[lib], Path::new("/L"), &Platform::windows_x64()).unwrap();
        assert_eq!(
            r[0].urls[0],
            "http://files.minecraftforge.net/maven/com/typesafe/akka/akka-actor_2.11/2.3.3/akka-actor_2.11-2.3.3.jar"
        );
        assert_eq!(r[0].sha1.as_deref(), Some("a"));
        assert_eq!(r[0].alt_sha1, vec!["b".to_string()]);
        let skip = Library {
            name: "jline:jline:2.13".into(),
            clientreq: Some(false),
            ..Default::default()
        };
        assert!(
            resolve_libraries(&[skip], Path::new("/L"), &Platform::windows_x64())
                .unwrap()
                .is_empty()
        );
    }
}
