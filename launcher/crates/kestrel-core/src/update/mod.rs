//! Self-updater for the Kestrel launcher.
//!
//! * [`manifest`]: signed release metadata (Ed25519) and its validation.
//! * [`fetch`]: HTTPS transport with checked redirects and verified downloads.
//! * [`install`]: replacing the running executable, rollback and cleanup.
//!
//! The update source (manifest URL + trusted public key) is compiled into the
//! launcher by `scripts/release.ps1`; a build without one simply reports that
//! updates are not configured.

pub mod fetch;
pub mod install;
pub mod manifest;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use ed25519_dalek::VerifyingKey;
use semver::Version;

pub use fetch::{DownloadProgress, Transport, UreqTransport};
pub use manifest::Release;

use crate::paths::Layout;

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("updates are not configured: {0}")]
    Config(String),
    #[error("could not reach the update server: {0}")]
    Network(String),
    #[error("no release found at {0}")]
    NotFound(String),
    #[error("invalid release information: {0}")]
    Manifest(String),
    #[error("release signature check failed: {0}")]
    Signature(String),
    #[error("downloaded file failed verification: {0}")]
    Integrity(String),
    #[error("{0}")]
    Io(String),
    #[error("the new version failed its self-test: {0}")]
    Validation(String),
    #[error("{0}")]
    Install(String),
    #[error("Kestrel cannot replace itself in {0} (no write permission)")]
    NotWritable(PathBuf),
    #[error("update cancelled")]
    Cancelled,
}

/// Where updates come from and which key signs them.
#[derive(Debug, Clone)]
pub struct UpdateSource {
    pub manifest_url: String,
    pub public_key: VerifyingKey,
    pub channel: String,
    /// GitHub hosts plus the manifest's own host.
    pub allowed_hosts: Vec<String>,
}

impl UpdateSource {
    pub fn new(manifest_url: &str, public_key_b64: &str) -> Result<UpdateSource, UpdateError> {
        let manifest_url = manifest_url.trim().to_string();
        let public_key = manifest::parse_public_key(public_key_b64)?;
        let host = manifest_url
            .strip_prefix("https://")
            .and_then(|r| r.split(['/', '?', '#']).next())
            .filter(|h| !h.is_empty() && !h.contains('@') && !h.contains(':'))
            .ok_or_else(|| {
                UpdateError::Config(format!(
                    "manifest URL must be https://host/...: {manifest_url}"
                ))
            })?
            .to_ascii_lowercase();
        let mut allowed_hosts: Vec<String> =
            fetch::GITHUB_HOSTS.iter().map(|s| s.to_string()).collect();
        if !allowed_hosts.contains(&host) {
            allowed_hosts.push(host);
        }
        Ok(UpdateSource {
            manifest_url,
            public_key,
            channel: manifest::CHANNEL_STABLE.into(),
            allowed_hosts,
        })
    }

    /// From build-time values (`None`/empty = not configured).
    pub fn from_build(
        manifest_url: Option<&str>,
        public_key: Option<&str>,
    ) -> Result<UpdateSource, UpdateError> {
        match (
            manifest_url.filter(|s| !s.trim().is_empty()),
            public_key.filter(|s| !s.trim().is_empty()),
        ) {
            (Some(u), Some(k)) => UpdateSource::new(u, k),
            _ => Err(UpdateError::Config(
                "this build has no update source (see documentation/UPDATES.md)".into(),
            )),
        }
    }

    /// Human-readable description for the Settings page.
    pub fn describe(&self) -> String {
        // github.com/<owner>/<repo>/releases/... -> github.com/<owner>/<repo>
        let rest = self.manifest_url.trim_start_matches("https://");
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.first() == Some(&"github.com") && parts.len() >= 3 {
            format!("github.com/{}/{}", parts[1], parts[2])
        } else {
            parts.first().copied().unwrap_or("").to_string()
        }
    }
}

pub fn current_version() -> Version {
    // CARGO_PKG_VERSION is always valid semver.
    Version::parse(crate::LAUNCHER_VERSION).unwrap_or_else(|_| Version::new(0, 0, 0))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    UpToDate { latest: Version },
    Available(Box<Release>),
}

/// Fetch and verify the manifest and compare with `current`.
pub fn check(
    t: &dyn Transport,
    source: &UpdateSource,
    current: &Version,
) -> Result<CheckOutcome, UpdateError> {
    let bytes = fetch::fetch_manifest(t, &source.manifest_url, &source.allowed_hosts)?;
    let release = manifest::verify_and_parse(&bytes, source)?;
    Ok(if release.is_newer_than(current) {
        CheckOutcome::Available(Box::new(release))
    } else {
        CheckOutcome::UpToDate {
            latest: release.version,
        }
    })
}

/// Whether to show the update prompt automatically. A version the user
/// dismissed with "Later" is not offered again automatically (unless it is
/// required); a manual check always shows it.
pub fn should_prompt(release: &Release, current: &Version, dismissed: &str, manual: bool) -> bool {
    if !release.is_newer_than(current) {
        return false;
    }
    if manual || release.is_required_for(current) {
        return true;
    }
    manifest::parse_version(dismissed)
        .map(|d| d != release.version)
        .unwrap_or(true)
}

/// The version this launcher must be updated to before Minecraft may start,
/// if any. `live` is the newest signed release from this session's check
/// (`None` if there was no successful check); `remembered` is the minimum
/// saved from an earlier check, so going offline cannot bypass a required
/// update. Unparsable remembered values are ignored.
pub fn required_minimum(
    current: &Version,
    live: Option<&Release>,
    remembered: &str,
) -> Option<Version> {
    if let Some(r) = live
        && r.is_required_for(current)
    {
        return r.minimum_supported.clone();
    }
    manifest::parse_version(remembered)
        .ok()
        .filter(|m| current < m)
}

/// New value for the remembered minimum after a successful check:
/// the release's minimum when it requires an update from `current`,
/// otherwise empty (nothing is required any more).
pub fn remembered_minimum_after_check(current: &Version, latest: Option<&Release>) -> String {
    match latest {
        Some(r) if r.is_required_for(current) => r
            .minimum_supported
            .as_ref()
            .map(|m| m.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

pub fn updates_dir(layout: &Layout) -> PathBuf {
    layout.root().join("updates")
}

/// Runs the downloaded exe's own self-test before it replaces anything:
/// `<exe> --self-check <report> --root <scratch>` must exit 0, and its report
/// must name the expected version.
pub trait Validator: Send + Sync {
    fn validate(&self, exe: &Path, expected: &Version, scratch: &Path) -> Result<(), String>;
}

pub struct SelfCheckValidator;

impl Validator for SelfCheckValidator {
    fn validate(&self, exe: &Path, expected: &Version, scratch: &Path) -> Result<(), String> {
        if !install::looks_like_windows_exe(exe) {
            return Err("download is not a Windows executable".into());
        }
        std::fs::create_dir_all(scratch).map_err(|e| e.to_string())?;
        let report = scratch.join("self-check.txt");
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--self-check")
            .arg(&report)
            .arg("--root")
            .arg(scratch.join("data"));
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("cannot start the new version: {e}"))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(200))
                }
                Ok(None) => {
                    let _ = child.kill();
                    return Err("self-test timed out".into());
                }
                Err(e) => return Err(e.to_string()),
            }
        };
        let text = std::fs::read_to_string(&report).unwrap_or_default();
        if !status.success() || !text.contains("RESULT: OK") {
            let first_fail = text
                .lines()
                .find(|l| l.starts_with("FAIL"))
                .unwrap_or("no report");
            return Err(format!("self-test failed ({status}): {first_fail}"));
        }
        if !text.contains(&format!("Kestrel {expected} ")) {
            return Err(format!("the downloaded file is not Kestrel {expected}"));
        }
        Ok(())
    }
}

/// Download, verify and self-test a release. Returns the verified file in
/// `<root>/updates/`. Nothing outside that folder is touched.
pub fn prepare(
    t: &dyn Transport,
    source: &UpdateSource,
    release: &Release,
    layout: &Layout,
    validator: &dyn Validator,
    cancel: &AtomicBool,
    progress: impl FnMut(DownloadProgress),
) -> Result<PathBuf, UpdateError> {
    let dir = updates_dir(layout);
    install::clean_updates_dir(&dir, None);
    let name = format!("Kestrel-{}.exe", release.version);
    let file = fetch::download_asset(
        t,
        &release.asset,
        &source.allowed_hosts,
        &dir,
        &name,
        cancel,
        progress,
    )?;
    let scratch = dir.join(format!("selftest-{}", release.version));
    let _ = std::fs::remove_dir_all(&scratch);
    let r = validator.validate(&file, &release.version, &scratch);
    let _ = std::fs::remove_dir_all(&scratch);
    if let Err(e) = r {
        let _ = std::fs::remove_file(&file);
        return Err(UpdateError::Validation(e));
    }
    // Re-hash right before use: the file must not have changed since download.
    let actual = fetch::sha256_file(&file).map_err(|e| UpdateError::Io(e.to_string()))?;
    if actual != release.asset.sha256 {
        let _ = std::fs::remove_file(&file);
        return Err(UpdateError::Integrity("file changed after download".into()));
    }
    Ok(file)
}

/// File the new process creates once its window is up (see the launcher's
/// `--update-health` argument).
pub fn health_file(layout: &Layout, version: &Version) -> PathBuf {
    updates_dir(layout).join(format!("health-{version}"))
}

#[cfg(test)]
mod tests {
    use super::fetch::tests::{MockRoute, MockTransport};
    use super::manifest::tests::{info, source, test_key};
    use super::*;
    use sha2::{Digest, Sha256};

    const MANIFEST: &str =
        "https://github.com/owner/kestrel/releases/latest/download/kestrel-update.json";

    struct OkValidator;
    impl Validator for OkValidator {
        fn validate(&self, _: &Path, _: &Version, _: &Path) -> Result<(), String> {
            Ok(())
        }
    }
    struct BadValidator;
    impl Validator for BadValidator {
        fn validate(&self, _: &Path, _: &Version, _: &Path) -> Result<(), String> {
            Err("FAIL  embedded client mod".into())
        }
    }

    fn publish(
        t: &MockTransport,
        version: &str,
        exe: &[u8],
        key: &ed25519_dalek::SigningKey,
    ) -> Release {
        let mut i = info(version);
        i.windows_x86_64.sha256 = hex::encode(Sha256::digest(exe));
        i.windows_x86_64.size = exe.len() as u64;
        let env = manifest::sign(&i, key);
        // GitHub-style: latest -> tagged asset -> CDN.
        let tagged = format!(
            "https://github.com/owner/kestrel/releases/download/v{version}/kestrel-update.json"
        );
        t.route(MANIFEST, MockRoute::Redirect(tagged.clone()));
        t.route(&tagged, MockRoute::Ok(serde_json::to_vec(&env).unwrap()));
        let cdn = format!("https://release-assets.githubusercontent.com/kestrel/{version}");
        t.route(&i.windows_x86_64.url, MockRoute::Redirect(cdn.clone()));
        t.route(&cdn, MockRoute::Ok(exe.to_vec()));
        manifest::verify_and_parse(&serde_json::to_vec(&env).unwrap(), &source(key)).unwrap()
    }

    fn exe_bytes() -> Vec<u8> {
        (0..100_000u32).map(|i| (i * 7 % 256) as u8).collect()
    }

    #[test]
    fn detects_available_update_and_up_to_date() {
        let k = test_key(3);
        let t = MockTransport::default();
        publish(&t, "0.2.0", &exe_bytes(), &k);
        match check(&t, &source(&k), &Version::new(0, 1, 0)).unwrap() {
            CheckOutcome::Available(r) => assert_eq!(r.version, Version::new(0, 2, 0)),
            o => panic!("{o:?}"),
        }
        assert_eq!(
            check(&t, &source(&k), &Version::new(0, 2, 0)).unwrap(),
            CheckOutcome::UpToDate {
                latest: Version::new(0, 2, 0)
            }
        );
        // A newer local build than the published one is also "up to date".
        assert!(matches!(
            check(&t, &source(&k), &Version::new(0, 3, 0)).unwrap(),
            CheckOutcome::UpToDate { .. }
        ));
    }

    #[test]
    fn offline_or_missing_release_is_an_error_not_a_crash() {
        let k = test_key(3);
        let t = MockTransport::default();
        t.route(MANIFEST, MockRoute::Offline);
        assert!(matches!(
            check(&t, &source(&k), &Version::new(0, 1, 0)),
            Err(UpdateError::Network(_))
        ));
        let t = MockTransport::default(); // nothing published yet
        assert!(matches!(
            check(&t, &source(&k), &Version::new(0, 1, 0)),
            Err(UpdateError::NotFound(_))
        ));
        t.route(
            MANIFEST,
            MockRoute::Ok(b"<html>rate limited</html>".to_vec()),
        );
        assert!(matches!(
            check(&t, &source(&k), &Version::new(0, 1, 0)),
            Err(UpdateError::Manifest(_))
        ));
    }

    #[test]
    fn later_is_remembered_per_version() {
        let k = test_key(3);
        let t = MockTransport::default();
        let r = publish(&t, "0.2.0", &exe_bytes(), &k);
        let cur = Version::new(0, 1, 0);
        assert!(should_prompt(&r, &cur, "", false));
        // User chose Later for 0.2.0.
        assert!(!should_prompt(&r, &cur, "0.2.0", false));
        // A manual check still offers it.
        assert!(should_prompt(&r, &cur, "0.2.0", true));
        // A newer release than the dismissed one is offered again.
        let r3 = publish(&t, "0.3.0", &exe_bytes(), &k);
        assert!(should_prompt(&r3, &cur, "0.2.0", false));
        // Required updates are always offered.
        let mut req = r.clone();
        req.minimum_supported = Some(Version::new(0, 2, 0));
        assert!(should_prompt(&req, &cur, "0.2.0", false));
        // Never offered when already current; garbage dismissal values are ignored.
        assert!(!should_prompt(&r, &Version::new(0, 2, 0), "", true));
        assert!(should_prompt(&r, &cur, "not-a-version", false));
    }

    #[test]
    fn required_update_blocks_older_versions_only() {
        let k = test_key(3);
        let t = MockTransport::default();
        let mut r = publish(&t, "0.3.0", &exe_bytes(), &k);
        r.minimum_supported = Some(Version::new(0, 3, 0));
        let old = Version::new(0, 2, 2);
        let new = Version::new(0, 3, 0);
        let v = |s: &str| Version::parse(s).unwrap();

        // Older launcher: blocked by the live release, and the minimum is remembered.
        assert_eq!(required_minimum(&old, Some(&r), ""), Some(v("0.3.0")));
        let remembered = remembered_minimum_after_check(&old, Some(&r));
        assert_eq!(remembered, "0.3.0");
        // ...still blocked offline (no live check) thanks to the remembered minimum.
        assert_eq!(required_minimum(&old, None, &remembered), Some(v("0.3.0")));

        // Updated launcher: not blocked, nothing remembered -> no update loop.
        assert_eq!(required_minimum(&new, Some(&r), &remembered), None);
        assert_eq!(remembered_minimum_after_check(&new, Some(&r)), "");
        assert_eq!(required_minimum(&new, None, ""), None);

        // Optional releases never block, and a check without a required release
        // clears an old remembered minimum (e.g. a withdrawn release).
        let mut optional = r.clone();
        optional.minimum_supported = None;
        assert_eq!(required_minimum(&old, Some(&optional), ""), None);
        assert_eq!(remembered_minimum_after_check(&old, Some(&optional)), "");
        assert_eq!(remembered_minimum_after_check(&old, None), "");
        // A minimum older than or equal to the running version never blocks.
        r.minimum_supported = Some(Version::new(0, 2, 0));
        assert_eq!(required_minimum(&old, Some(&r), ""), None);
        // Garbage in launcher.json cannot block (or crash) the launcher.
        assert_eq!(required_minimum(&old, None, "not-a-version"), None);
        // Pre-releases compare by semver rules.
        assert_eq!(
            required_minimum(&v("0.3.0-rc.1"), None, "0.3.0"),
            Some(v("0.3.0"))
        );
    }

    #[test]
    fn prepare_downloads_verifies_and_preserves_user_data() {
        let k = test_key(3);
        let t = MockTransport::default();
        let exe = exe_bytes();
        let r = publish(&t, "0.2.0", &exe, &k);
        let d = tempfile::tempdir().unwrap();
        let layout = Layout::new(d.path().join("Kestrel"));
        layout.ensure().unwrap();
        // Existing user data that must survive.
        let cfg = layout.launcher_config();
        std::fs::write(&cfg, b"{\"max_ram_mib\":3072}").unwrap();
        std::fs::write(
            layout.client_data().join("config.json"),
            b"{\"cosmetics\":{}}",
        )
        .unwrap();
        std::fs::create_dir_all(layout.client_data().join("capes")).unwrap();
        std::fs::write(layout.client_data().join("capes/mine.png"), b"png").unwrap();
        std::fs::write(layout.accounts(), b"secret").unwrap();

        let file = prepare(
            &t,
            &source(&k),
            &r,
            &layout,
            &OkValidator,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), exe);
        assert!(file.starts_with(updates_dir(&layout)));
        assert_eq!(std::fs::read(&cfg).unwrap(), b"{\"max_ram_mib\":3072}");
        assert!(layout.client_data().join("capes/mine.png").is_file());
        assert!(layout.client_data().join("config.json").is_file());
        assert_eq!(std::fs::read(layout.accounts()).unwrap(), b"secret");

        // Full swap with the prepared file, then cleanup by the "new" version.
        let install_dir = d.path().join("Desktop");
        std::fs::create_dir_all(&install_dir).unwrap();
        let current = install_dir.join("Kestrel.exe");
        std::fs::write(&current, b"old").unwrap();
        let backup = install::swap(&current, &file).unwrap();
        assert_eq!(std::fs::read(&current).unwrap(), exe);
        install::cleanup_after_update(
            &current,
            &updates_dir(&layout),
            None,
            std::time::Duration::from_millis(10),
        );
        assert!(!backup.exists());
        assert!(!file.exists(), "downloaded copy cleaned up");
        assert!(layout.client_data().join("capes/mine.png").is_file());
        assert_eq!(std::fs::read(&cfg).unwrap(), b"{\"max_ram_mib\":3072}");
    }

    #[test]
    fn failed_self_test_or_tampered_download_installs_nothing() {
        let k = test_key(3);
        let t = MockTransport::default();
        let r = publish(&t, "0.2.0", &exe_bytes(), &k);
        let d = tempfile::tempdir().unwrap();
        let layout = Layout::new(d.path().join("Kestrel"));
        let e = prepare(
            &t,
            &source(&k),
            &r,
            &layout,
            &BadValidator,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
        assert!(matches!(e, UpdateError::Validation(_)), "{e}");
        assert!(!updates_dir(&layout).join("Kestrel-0.2.0.exe").exists());

        // Server swaps the binary after the manifest was signed.
        let cdn = "https://release-assets.githubusercontent.com/kestrel/0.2.0";
        let mut evil = exe_bytes();
        evil[0] ^= 0xff;
        t.route(cdn, MockRoute::Ok(evil));
        let e = prepare(
            &t,
            &source(&k),
            &r,
            &layout,
            &OkValidator,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
        assert!(matches!(e, UpdateError::Integrity(_)), "{e}");
    }

    #[test]
    fn source_configuration() {
        let k = test_key(3);
        assert!(UpdateSource::from_build(None, None).is_err());
        assert!(UpdateSource::from_build(Some(""), Some("")).is_err());
        let s = source(&k);
        assert_eq!(s.describe(), "github.com/owner/kestrel");
        use base64::Engine as _;
        let pk = base64::engine::general_purpose::STANDARD.encode(k.verifying_key().to_bytes());
        assert!(UpdateSource::new("http://github.com/x", &pk).is_err());
        assert!(UpdateSource::new(MANIFEST, "not-a-key").is_err());
        let custom = UpdateSource::new("https://updates.example.org/kestrel.json", &pk).unwrap();
        assert!(
            custom
                .allowed_hosts
                .contains(&"updates.example.org".to_string())
        );
        assert_eq!(current_version().to_string(), crate::LAUNCHER_VERSION);
    }
}
