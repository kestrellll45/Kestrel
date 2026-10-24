//! HTTPS transport for update checks and downloads.
//!
//! Redirects are followed manually so every hop can be checked: HTTPS only,
//! host on the allowlist, no credentials in the URL. Downloads are streamed
//! to a `.part` file with exact-size and SHA-256 checks; nothing is renamed
//! into place unless it is complete and matches the signed manifest.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::UpdateError;
use super::manifest::{Asset, MAX_MANIFEST_BYTES};

const MAX_REDIRECTS: usize = 5;

/// Hosts GitHub Releases serves from (the asset itself and its CDN redirect).
pub const GITHUB_HOSTS: &[&str] = &[
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

pub struct HttpResponse {
    pub status: u16,
    pub location: Option<String>,
    pub content_length: Option<u64>,
    pub body: Box<dyn Read + Send>,
}

/// Minimal HTTP GET abstraction so tests can mock the release service.
pub trait Transport: Send + Sync {
    /// Must NOT follow redirects.
    fn get(&self, url: &str) -> Result<HttpResponse, String>;
}

/// Real transport (ureq + rustls), HTTPS only.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl Default for UreqTransport {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_recv_body(Some(Duration::from_secs(15 * 60)))
            // No identifying data beyond the product version.
            .user_agent(format!("Kestrel-Launcher/{}", crate::LAUNCHER_VERSION))
            .build();
        UreqTransport {
            agent: config.into(),
        }
    }
}

impl Transport for UreqTransport {
    fn get(&self, url: &str) -> Result<HttpResponse, String> {
        let resp = self.agent.get(url).call().map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let header = |n: &str| {
            resp.headers()
                .get(n)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let location = header("location");
        let content_length = header("content-length").and_then(|v| v.parse().ok());
        Ok(HttpResponse {
            status,
            location,
            content_length,
            body: Box::new(resp.into_body().into_reader()),
        })
    }
}

/// Check that `url` is `https://<allowed host>/...` with no userinfo or port.
pub fn validate_url(url: &str, allowed_hosts: &[String]) -> Result<(), String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| format!("update URL must use HTTPS: {url}"))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') || authority.contains(':') || authority.is_empty() {
        return Err(format!("update URL has an unsupported host: {url}"));
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("update URL contains invalid characters".into());
    }
    if allowed_hosts
        .iter()
        .any(|h| authority.eq_ignore_ascii_case(h))
    {
        Ok(())
    } else {
        Err(format!("update host {authority} is not trusted"))
    }
}

/// Resolve a `Location` header against the current URL (absolute or path-absolute only).
fn resolve_location(current: &str, location: &str) -> Result<String, String> {
    if location.starts_with("https://") || location.starts_with("http://") {
        return Ok(location.to_string());
    }
    if location.starts_with('/') && !location.starts_with("//") {
        let rest = current.strip_prefix("https://").ok_or("bad base URL")?;
        let host = rest.split('/').next().unwrap_or("");
        return Ok(format!("https://{host}{location}"));
    }
    Err(format!("unsupported redirect target {location:?}"))
}

/// GET with validated redirects; returns the final 200 response.
pub fn get_following(
    t: &dyn Transport,
    url: &str,
    allowed: &[String],
) -> Result<HttpResponse, UpdateError> {
    let mut url = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        validate_url(&url, allowed).map_err(UpdateError::Network)?;
        let resp = t.get(&url).map_err(UpdateError::Network)?;
        match resp.status {
            200 => return Ok(resp),
            301 | 302 | 303 | 307 | 308 => {
                let loc = resp.location.ok_or_else(|| {
                    UpdateError::Network("redirect without a Location header".into())
                })?;
                url = resolve_location(&url, &loc).map_err(UpdateError::Network)?;
            }
            404 => return Err(UpdateError::NotFound(url)),
            s => return Err(UpdateError::Network(format!("server returned HTTP {s}"))),
        }
    }
    Err(UpdateError::Network("too many redirects".into()))
}

/// Download the (small) manifest into memory.
pub fn fetch_manifest(
    t: &dyn Transport,
    url: &str,
    allowed: &[String],
) -> Result<Vec<u8>, UpdateError> {
    let resp = get_following(t, url, allowed)?;
    let mut buf = Vec::new();
    resp.body
        .take(MAX_MANIFEST_BYTES as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|e| UpdateError::Network(format!("manifest download interrupted: {e}")))?;
    Ok(buf)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    /// Smoothed bytes per second.
    pub speed: f64,
}

/// Stream `asset` into `dir/<file_name>`; returns the verified file path.
/// The partial file is removed on any failure (cancel, interruption, wrong
/// size, checksum mismatch).
pub fn download_asset(
    t: &dyn Transport,
    asset: &Asset,
    allowed: &[String],
    dir: &Path,
    file_name: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(DownloadProgress),
) -> Result<PathBuf, UpdateError> {
    std::fs::create_dir_all(dir)
        .map_err(|e| UpdateError::Io(format!("cannot create {}: {e}", dir.display())))?;
    let dest = dir.join(file_name);
    let part = dir.join(format!("{file_name}.part"));
    let _ = std::fs::remove_file(&part);
    let result = (|| -> Result<(), UpdateError> {
        let resp = get_following(t, &asset.url, allowed)?;
        if let Some(len) = resp.content_length
            && len != asset.size
        {
            return Err(UpdateError::Integrity(format!(
                "server reports {len} bytes but the release says {}",
                asset.size
            )));
        }
        let mut out = File::create(&part)
            .map_err(|e| UpdateError::Io(format!("cannot write {}: {e}", part.display())))?;
        let mut body = resp.body.take(asset.size + 1);
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut p = DownloadProgress {
            total: asset.size,
            ..Default::default()
        };
        let start = Instant::now();
        let mut last_report = Instant::now() - Duration::from_secs(1);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(UpdateError::Cancelled);
            }
            let n = body
                .read(&mut buf)
                .map_err(|e| UpdateError::Network(format!("download interrupted: {e}")))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])
                .map_err(|e| UpdateError::Io(format!("write failed: {e}")))?;
            hasher.update(&buf[..n]);
            p.downloaded += n as u64;
            if p.downloaded > asset.size {
                return Err(UpdateError::Integrity(
                    "download is larger than the release says".into(),
                ));
            }
            if last_report.elapsed() >= Duration::from_millis(100) {
                let secs = start.elapsed().as_secs_f64().max(0.001);
                p.speed = p.downloaded as f64 / secs;
                progress(p);
                last_report = Instant::now();
            }
        }
        out.sync_all()
            .map_err(|e| UpdateError::Io(format!("flush failed: {e}")))?;
        drop(out);
        p.speed = p.downloaded as f64 / start.elapsed().as_secs_f64().max(0.001);
        progress(p);
        if p.downloaded != asset.size {
            return Err(UpdateError::Network(format!(
                "download incomplete ({} of {} bytes)",
                p.downloaded, asset.size
            )));
        }
        let actual = hex::encode(hasher.finalize());
        if actual != asset.sha256 {
            return Err(UpdateError::Integrity(format!(
                "checksum mismatch: expected {}, got {actual}",
                asset.sha256
            )));
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            let _ = std::fs::remove_file(&dest);
            std::fs::rename(&part, &dest)
                .map_err(|e| UpdateError::Io(format!("cannot finalise download: {e}")))?;
            Ok(dest)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

/// SHA-256 of a file, lower-case hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Scripted responses keyed by URL; records every request.
    #[derive(Default)]
    pub struct MockTransport {
        pub routes: Mutex<HashMap<String, MockRoute>>,
        pub requests: Mutex<Vec<String>>,
    }

    #[derive(Clone)]
    pub enum MockRoute {
        Ok(Vec<u8>),
        Redirect(String),
        Status(u16),
        /// Serves this many bytes of the body, then fails.
        Truncated(Vec<u8>, usize),
        /// Connection failure (offline, DNS, TLS).
        Offline,
    }

    impl MockTransport {
        pub fn route(&self, url: &str, r: MockRoute) {
            self.routes.lock().unwrap().insert(url.to_string(), r);
        }
    }

    struct FailAfter(std::io::Cursor<Vec<u8>>, usize);
    impl Read for FailAfter {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            let pos = self.0.position() as usize;
            if pos >= self.1 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "connection reset",
                ));
            }
            let max = (self.1 - pos).min(b.len());
            self.0.read(&mut b[..max])
        }
    }

    impl Transport for MockTransport {
        fn get(&self, url: &str) -> Result<HttpResponse, String> {
            self.requests.lock().unwrap().push(url.to_string());
            let route = self.routes.lock().unwrap().get(url).cloned();
            let ok = |status, location, len, body: Box<dyn Read + Send>| HttpResponse {
                status,
                location,
                content_length: len,
                body,
            };
            match route {
                None | Some(MockRoute::Status(404)) => {
                    Ok(ok(404, None, None, Box::new(std::io::empty())))
                }
                Some(MockRoute::Ok(b)) => Ok(ok(
                    200,
                    None,
                    Some(b.len() as u64),
                    Box::new(std::io::Cursor::new(b)),
                )),
                Some(MockRoute::Redirect(to)) => {
                    Ok(ok(302, Some(to), None, Box::new(std::io::empty())))
                }
                Some(MockRoute::Status(s)) => Ok(ok(s, None, None, Box::new(std::io::empty()))),
                Some(MockRoute::Truncated(b, n)) => {
                    let len = b.len() as u64;
                    Ok(ok(
                        200,
                        None,
                        Some(len),
                        Box::new(FailAfter(std::io::Cursor::new(b), n)),
                    ))
                }
                Some(MockRoute::Offline) => Err("failed to lookup address information".into()),
            }
        }
    }

    pub fn hosts() -> Vec<String> {
        GITHUB_HOSTS.iter().map(|s| s.to_string()).collect()
    }

    fn asset(body: &[u8]) -> Asset {
        Asset {
            url: "https://github.com/o/r/releases/download/v0.2.0/Kestrel.exe".into(),
            sha256: hex::encode(Sha256::digest(body)),
            size: body.len() as u64,
        }
    }

    #[test]
    fn url_validation() {
        let h = hosts();
        assert!(validate_url("https://github.com/o/r/x.exe", &h).is_ok());
        assert!(validate_url("https://GitHub.com/o/r/x.exe", &h).is_ok());
        for bad in [
            "http://github.com/x",
            "https://evil.example/x",
            "https://github.com.evil.example/x",
            "https://github.com@evil.example/x",
            "https://github.com:8443/x",
            "https:///x",
            "https://github.com/a b",
            "file:///C:/x.exe",
            "ftp://github.com/x",
        ] {
            assert!(validate_url(bad, &h).is_err(), "{bad}");
        }
    }

    #[test]
    fn follows_only_safe_redirects() {
        let t = MockTransport::default();
        let a = "https://github.com/o/r/releases/latest/download/m.json";
        let b = "https://github.com/o/r/releases/download/v1/m.json";
        let c = "https://release-assets.githubusercontent.com/abc?sig=1";
        t.route(a, MockRoute::Redirect(b.into()));
        t.route(b, MockRoute::Redirect(c.into()));
        t.route(c, MockRoute::Ok(b"{}".to_vec()));
        assert_eq!(fetch_manifest(&t, a, &hosts()).unwrap(), b"{}");

        t.route(b, MockRoute::Redirect("https://evil.example/m.json".into()));
        assert!(
            fetch_manifest(&t, a, &hosts()).is_err(),
            "redirect to untrusted host"
        );
        t.route(b, MockRoute::Redirect("http://github.com/m.json".into()));
        assert!(
            fetch_manifest(&t, a, &hosts()).is_err(),
            "redirect downgrade to http"
        );
        t.route(b, MockRoute::Redirect(a.into()));
        assert!(
            matches!(fetch_manifest(&t, a, &hosts()), Err(UpdateError::Network(m)) if m.contains("too many"))
        );
        // Relative redirect stays on the same host.
        t.route(b, MockRoute::Redirect("/o/r/final.json".into()));
        t.route(
            "https://github.com/o/r/final.json",
            MockRoute::Ok(b"[]".to_vec()),
        );
        assert_eq!(fetch_manifest(&t, a, &hosts()).unwrap(), b"[]");
    }

    #[test]
    fn offline_and_server_errors() {
        let t = MockTransport::default();
        let u = "https://github.com/o/r/m.json";
        t.route(u, MockRoute::Offline);
        assert!(matches!(
            fetch_manifest(&t, u, &hosts()),
            Err(UpdateError::Network(_))
        ));
        t.route(u, MockRoute::Status(503));
        assert!(matches!(
            fetch_manifest(&t, u, &hosts()),
            Err(UpdateError::Network(_))
        ));
        t.route(u, MockRoute::Status(404));
        assert!(matches!(
            fetch_manifest(&t, u, &hosts()),
            Err(UpdateError::NotFound(_))
        ));
    }

    #[test]
    fn download_verifies_and_reports_progress() {
        let dir = tempfile::tempdir().unwrap();
        let body: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let a = asset(&body);
        let t = MockTransport::default();
        t.route(&a.url, MockRoute::Ok(body.clone()));
        let mut last = DownloadProgress::default();
        let p = download_asset(
            &t,
            &a,
            &hosts(),
            dir.path(),
            "Kestrel-0.2.0.exe",
            &AtomicBool::new(false),
            |x| last = x,
        )
        .unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), body);
        assert_eq!(
            (last.downloaded, last.total),
            (body.len() as u64, body.len() as u64)
        );
        assert!(last.speed > 0.0);
        assert!(!dir.path().join("Kestrel-0.2.0.exe.part").exists());
    }

    #[test]
    fn checksum_mismatch_is_rejected_and_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let body = vec![7u8; 100_000];
        let mut a = asset(&body);
        a.sha256 = "00".repeat(32);
        let t = MockTransport::default();
        t.route(&a.url, MockRoute::Ok(body));
        let e = download_asset(
            &t,
            &a,
            &hosts(),
            dir.path(),
            "k.exe",
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
        assert!(matches!(e, UpdateError::Integrity(_)), "{e}");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "no partial or final file left"
        );
    }

    #[test]
    fn interrupted_and_incomplete_downloads() {
        let dir = tempfile::tempdir().unwrap();
        let body = vec![1u8; 200_000];
        let a = asset(&body);
        let t = MockTransport::default();
        // Connection drops mid-way.
        t.route(&a.url, MockRoute::Truncated(body.clone(), 50_000));
        let e = download_asset(
            &t,
            &a,
            &hosts(),
            dir.path(),
            "k.exe",
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
        assert!(
            matches!(e, UpdateError::Network(ref m) if m.contains("interrupted")),
            "{e}"
        );
        // Server sends fewer bytes than promised (clean EOF).
        let mut short = a.clone();
        short.size = 300_000;
        t.route(&a.url, MockRoute::Ok(body.clone()));
        assert!(
            download_asset(
                &t,
                &short,
                &hosts(),
                dir.path(),
                "k.exe",
                &AtomicBool::new(false),
                |_| {}
            )
            .is_err()
        );
        // Server sends more bytes than promised.
        let mut long = a.clone();
        long.size = 100_000;
        assert!(
            download_asset(
                &t,
                &long,
                &hosts(),
                dir.path(),
                "k.exe",
                &AtomicBool::new(false),
                |_| {}
            )
            .is_err()
        );
        // Cancelled by the user.
        let e = download_asset(
            &t,
            &a,
            &hosts(),
            dir.path(),
            "k.exe",
            &AtomicBool::new(true),
            |_| {},
        )
        .unwrap_err();
        assert!(matches!(e, UpdateError::Cancelled));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
