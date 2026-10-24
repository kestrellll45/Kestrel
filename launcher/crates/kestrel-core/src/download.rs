//! Verified, retrying, parallel downloads.
//!
//! Every file is written to `<name>.part` and only renamed into place after
//! its size and SHA-1 (when known) are verified, so an interrupted or
//! corrupted download is never mistaken for a complete one. Already-valid
//! files are skipped, which makes re-running an install a safe "resume".

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::error::{Error, IoContext, Result};
use crate::hash;
use crate::progress::Progress;

/// Hosts we accept downloads from. Everything else is rejected so metadata
/// can never point the launcher at arbitrary code.
const ALLOWED_HOSTS: &[&str] = &[
    "piston-meta.mojang.com",
    "piston-data.mojang.com",
    "launchermeta.mojang.com",
    "launcher.mojang.com",
    "libraries.minecraft.net",
    "resources.download.minecraft.net",
    "maven.minecraftforge.net",
    "files.minecraftforge.net",
    "repo1.maven.org",
    "repo.maven.apache.org",
];

/// Maximum size of a single file we will download (protects disk/memory
/// against bogus metadata). The largest legitimate 1.8.9 file is < 100 MB.
const MAX_FILE_SIZE: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Default)]
pub struct Task {
    pub url: String,
    /// Fallback URLs tried in order after `url`.
    pub mirrors: Vec<String>,
    pub path: PathBuf,
    pub sha1: Option<String>,
    /// Additional acceptable SHA-1 values (legacy Forge `checksums`).
    pub alt_sha1: Vec<String>,
    pub size: Option<u64>,
}

impl Task {
    pub fn new(
        url: impl Into<String>,
        path: impl Into<PathBuf>,
        sha1: Option<String>,
        size: Option<u64>,
    ) -> Task {
        Task {
            url: url.into(),
            path: path.into(),
            sha1,
            size,
            ..Default::default()
        }
    }

    fn accepts(&self, actual: &str) -> bool {
        self.sha1
            .iter()
            .chain(self.alt_sha1.iter())
            .any(|h| h.eq_ignore_ascii_case(actual))
    }

    /// Whether the file on disk is already valid for this task.
    pub fn is_satisfied(&self) -> bool {
        if self.sha1.is_none() {
            return hash::file_matches(&self.path, None, self.size);
        }
        if let Some(sz) = self.size {
            match std::fs::metadata(&self.path) {
                Ok(m) if m.len() == sz => {}
                _ => return false,
            }
        }
        matches!(hash::sha1_file(&self.path), Ok(a) if self.accepts(&a))
    }
}

/// Normalise and validate a URL: must be https (http is upgraded) and on the
/// allowlist. Legacy Forge metadata uses `http://files.minecraftforge.net/maven/`.
pub fn validate_url(url: &str) -> Result<String> {
    let url = if let Some(rest) = url.strip_prefix("http://") {
        format!("https://{rest}")
    } else {
        url.to_string()
    };
    let url = url.replace(
        "https://files.minecraftforge.net/maven/",
        "https://maven.minecraftforge.net/",
    );
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| Error::UntrustedUrl(url.clone()))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    // Reject userinfo / explicit ports to keep host matching unambiguous.
    if host.contains('@') || host.contains(':') {
        return Err(Error::UntrustedUrl(url));
    }
    if ALLOWED_HOSTS.iter().any(|h| host.eq_ignore_ascii_case(h)) {
        Ok(url)
    } else {
        Err(Error::UntrustedUrl(url))
    }
}

#[derive(Clone)]
pub struct Downloader {
    agent: ureq::Agent,
    retries: u32,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}

impl Downloader {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_body(Some(Duration::from_secs(60)))
            .user_agent(format!("Kestrel-Launcher/{}", crate::LAUNCHER_VERSION))
            .build();
        Downloader {
            agent: config.into(),
            retries: 3,
        }
    }

    pub fn agent(&self) -> &ureq::Agent {
        &self.agent
    }

    /// Fetch a small text/JSON resource into memory (limit 32 MiB).
    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let url = validate_url(url)?;
        let mut last = None;
        for attempt in 0..=self.retries {
            if attempt > 0 {
                std::thread::sleep(backoff(attempt));
            }
            match self.agent.get(&url).call() {
                Ok(resp) => {
                    match resp
                        .into_body()
                        .into_with_config()
                        .limit(32 * 1024 * 1024)
                        .read_to_vec()
                    {
                        Ok(v) => return Ok(v),
                        Err(e) => last = Some(e.to_string()),
                    }
                }
                Err(ureq::Error::StatusCode(code)) if code == 404 || code == 403 => {
                    return Err(Error::Http {
                        url,
                        message: format!("HTTP {code}"),
                    });
                }
                Err(e) => last = Some(e.to_string()),
            }
        }
        Err(Error::Http {
            url,
            message: last.unwrap_or_default(),
        })
    }

    /// Download a single task, verifying it. Skips the download if the
    /// destination already matches.
    pub fn fetch(&self, task: &Task) -> Result<()> {
        if task.is_satisfied() {
            return Ok(());
        }
        // Validate every candidate up-front so a bad mirror list fails loudly.
        let urls = std::iter::once(&task.url)
            .chain(task.mirrors.iter())
            .map(|u| validate_url(u))
            .collect::<Result<Vec<_>>>()?;
        if let Some(parent) = task.path.parent() {
            std::fs::create_dir_all(parent).at(parent)?;
        }
        let part = part_path(&task.path);
        let mut last_err = None;
        for url in &urls {
            for attempt in 0..=self.retries {
                if attempt > 0 {
                    std::thread::sleep(backoff(attempt));
                }
                match self.fetch_once(url, &part, task) {
                    Ok(()) => {
                        std::fs::rename(&part, &task.path).at(&task.path)?;
                        return Ok(());
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&part);
                        log::warn!("download attempt {} failed for {url}: {e}", attempt + 1);
                        let not_found = matches!(&e, Error::Http { message, .. }
                            if message == "HTTP 404" || message == "HTTP 403");
                        last_err = Some(e);
                        // Missing on this mirror: move on to the next one.
                        if not_found {
                            break;
                        }
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| Error::Other("download failed".into())))
    }

    fn fetch_once(&self, url: &str, part: &Path, task: &Task) -> Result<()> {
        let resp = self.agent.get(url).call().map_err(|e| Error::Http {
            url: url.to_string(),
            message: match e {
                ureq::Error::StatusCode(c) => format!("HTTP {c}"),
                other => other.to_string(),
            },
        })?;
        let limit = task
            .size
            .map(|s| s.saturating_add(1))
            .unwrap_or(MAX_FILE_SIZE);
        let mut reader = resp.into_body().into_with_config().limit(limit).reader();
        let mut out = std::fs::File::create(part).at(part)?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut written: u64 = 0;
        loop {
            let n = reader.read(&mut buf).map_err(|e| Error::Http {
                url: url.to_string(),
                message: e.to_string(),
            })?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).at(part)?;
            written += n as u64;
        }
        out.sync_all().at(part)?;
        drop(out);

        if let Some(expected) = task.size
            && written != expected
        {
            return Err(Error::SizeMismatch {
                path: task.path.clone(),
                expected,
                actual: written,
            });
        }
        if let Some(expected) = &task.sha1 {
            let actual = hash::sha1_file(part)?;
            if !task.accepts(&actual) {
                return Err(Error::HashMismatch {
                    path: task.path.clone(),
                    expected: expected.clone(),
                    actual,
                });
            }
        } else if written == 0 {
            return Err(Error::Http {
                url: url.to_string(),
                message: "empty response".into(),
            });
        }
        Ok(())
    }

    /// Download many tasks on a small fixed pool of worker threads.
    /// Progress is reported in completed-file units. Returns every failure
    /// (not just the first) so the UI can show a complete picture.
    pub fn fetch_all(&self, tasks: &[Task], threads: usize, progress: &Progress) -> Result<()> {
        if tasks.is_empty() {
            return Ok(());
        }
        let next = AtomicUsize::new(0);
        let failures: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let threads = threads.clamp(1, 16).min(tasks.len());
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    loop {
                        if progress.is_cancelled() {
                            return;
                        }
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(task) = tasks.get(i) else { return };
                        if let Err(e) = self.fetch(task)
                            && let Ok(mut f) = failures.lock()
                        {
                            f.push(e.to_string());
                        }
                        progress.add(1);
                    }
                });
            }
        });
        progress.check_cancel()?;
        let failures = failures.into_inner().unwrap_or_default();
        if failures.is_empty() {
            Ok(())
        } else {
            let shown: Vec<_> = failures.iter().take(5).cloned().collect();
            Err(Error::Other(format!(
                "{} download(s) failed:\n{}{}",
                failures.len(),
                shown.join("\n"),
                if failures.len() > 5 { "\n…" } else { "" }
            )))
        }
    }
}

fn part_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(".part");
    PathBuf::from(s)
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500u64 << attempt.min(4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_allowlist() {
        assert!(validate_url("https://libraries.minecraft.net/a/b.jar").is_ok());
        assert_eq!(
            validate_url("http://files.minecraftforge.net/maven/net/x.jar").unwrap(),
            "https://maven.minecraftforge.net/net/x.jar"
        );
        assert!(validate_url("https://evil.example/x.jar").is_err());
        assert!(validate_url("https://libraries.minecraft.net.evil.example/x").is_err());
        assert!(validate_url("https://user@libraries.minecraft.net/x").is_err());
        assert!(validate_url("https://libraries.minecraft.net:8443/x").is_err());
        assert!(validate_url("ftp://libraries.minecraft.net/x").is_err());
        assert!(validate_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn existing_valid_file_is_skipped_without_network() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x");
        std::fs::write(&p, b"abc").unwrap();
        // Unreachable in tests: must not be contacted because the file is valid.
        let mut t = Task::new(
            "https://libraries.minecraft.net/never",
            p,
            Some("00".into()),
            Some(3),
        );
        t.alt_sha1.push(hash::sha1_bytes(b"abc"));
        assert!(t.is_satisfied());
        Downloader::new().fetch(&t).unwrap();
    }

    #[test]
    fn untrusted_url_never_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let t = Task::new("https://example.com/x", dir.path().join("x"), None, None);
        assert!(matches!(
            Downloader::new().fetch(&t),
            Err(Error::UntrustedUrl(_))
        ));
    }
}
