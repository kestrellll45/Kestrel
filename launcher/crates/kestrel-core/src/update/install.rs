//! Replacing the running launcher executable.
//!
//! Windows will not let a running .exe be overwritten or deleted, but it
//! does allow it to be *renamed*. So the swap is:
//!
//! 1. Copy the verified new exe next to the current one (`Kestrel.exe.new`).
//! 2. Rename the running `Kestrel.exe` to `Kestrel.exe.old`.
//! 3. Rename `Kestrel.exe.new` to `Kestrel.exe`.
//! 4. Start the new exe and wait for it to report a healthy start.
//! 5. The new exe deletes `Kestrel.exe.old` once the old process has exited.
//!
//! If step 3 fails, step 2 is undone. If the new exe exits without reporting
//! health, the old exe is renamed back ([`rollback`]). At no point is the only
//! working copy deleted. If the folder is not writable (e.g. Program Files),
//! nothing is changed and the caller falls back to running the verified copy
//! from Kestrel's data directory.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::UpdateError;

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

pub fn backup_path(current_exe: &Path) -> PathBuf {
    sibling(current_exe, ".old")
}

fn staged_path(current_exe: &Path) -> PathBuf {
    sibling(current_exe, ".new")
}

fn failed_path(current_exe: &Path) -> PathBuf {
    sibling(current_exe, ".failed")
}

/// Retry a filesystem operation a few times: antivirus scanners and the
/// Windows search indexer briefly lock freshly written executables.
fn retry<T>(mut f: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut last = None;
    for attempt in 0..8u32 {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(e),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(150 * (attempt as u64 + 1)));
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("retry failed")))
}

/// Whether we can create files in `dir` (the install folder).
pub fn dir_is_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".kestrel-write-test-{}", std::process::id()));
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Swap `new_exe` into `current_exe`'s place. Returns the backup path.
/// On error the original executable is still at `current_exe`.
pub fn swap(current_exe: &Path, new_exe: &Path) -> Result<PathBuf, UpdateError> {
    let dir = current_exe
        .parent()
        .ok_or_else(|| UpdateError::Install("launcher path has no folder".into()))?;
    if !current_exe.is_file() {
        return Err(UpdateError::Install(format!(
            "{} not found",
            current_exe.display()
        )));
    }
    if !dir_is_writable(dir) {
        return Err(UpdateError::NotWritable(dir.to_path_buf()));
    }
    let staged = staged_path(current_exe);
    let backup = backup_path(current_exe);
    let _ = std::fs::remove_file(&staged);
    // A leftover backup from an earlier update may still be locked by an old
    // process; that only matters if we cannot remove it.
    if backup.exists() {
        retry(|| std::fs::remove_file(&backup)).map_err(|e| {
            UpdateError::Install(format!(
                "cannot remove old backup {}: {e}",
                backup.display()
            ))
        })?;
    }

    // 1. Stage next to the target (same volume, so the final rename is atomic).
    retry(|| std::fs::copy(new_exe, &staged)).map_err(|e| {
        let _ = std::fs::remove_file(&staged);
        UpdateError::Install(format!(
            "cannot copy the new version into {}: {e}",
            dir.display()
        ))
    })?;
    if std::fs::metadata(&staged).map(|m| m.len()).ok()
        != std::fs::metadata(new_exe).map(|m| m.len()).ok()
    {
        let _ = std::fs::remove_file(&staged);
        return Err(UpdateError::Install(
            "staged copy is incomplete (disk full or blocked by antivirus?)".into(),
        ));
    }

    // 2. Move the running exe aside.
    if let Err(e) = retry(|| std::fs::rename(current_exe, &backup)) {
        let _ = std::fs::remove_file(&staged);
        return Err(UpdateError::Install(format!(
            "cannot move the current launcher aside: {e}"
        )));
    }

    // 3. Put the new exe in place; undo step 2 on failure.
    if let Err(e) = retry(|| std::fs::rename(&staged, current_exe)) {
        let restore = retry(|| std::fs::rename(&backup, current_exe));
        let _ = std::fs::remove_file(&staged);
        return Err(match restore {
            Ok(()) => UpdateError::Install(format!(
                "cannot install the new version: {e}. The current version was kept."
            )),
            Err(r) => UpdateError::Install(format!(
                "cannot install the new version ({e}) and restoring failed ({r}). Your previous launcher is at {}",
                backup.display()
            )),
        });
    }
    Ok(backup)
}

/// Undo a swap after the new version failed to start.
pub fn rollback(current_exe: &Path, backup: &Path) -> Result<(), UpdateError> {
    if !backup.is_file() {
        return Err(UpdateError::Install(format!(
            "backup {} is missing",
            backup.display()
        )));
    }
    let failed = failed_path(current_exe);
    let _ = std::fs::remove_file(&failed);
    if current_exe.exists() {
        retry(|| std::fs::rename(current_exe, &failed))
            .map_err(|e| UpdateError::Install(format!("cannot remove the failed version: {e}")))?;
    }
    retry(|| std::fs::rename(backup, current_exe)).map_err(|e| {
        UpdateError::Install(format!(
            "cannot restore the previous launcher ({e}); it is at {}",
            backup.display()
        ))
    })?;
    let _ = std::fs::remove_file(&failed);
    Ok(())
}

/// Delete leftovers from a finished update. Called by the new version at
/// start-up; the backup can only be deleted once the old process has exited,
/// so this retries for a while (run it on a background thread).
///
/// `keep` is the health marker this process just wrote: the old launcher may
/// still be waiting for it (it deletes the marker itself once seen), so it
/// must survive this cleanup. A marker left behind is removed next time.
pub fn cleanup_after_update(
    current_exe: &Path,
    updates_dir: &Path,
    keep: Option<&Path>,
    wait: Duration,
) {
    let backup = backup_path(current_exe);
    let deadline = std::time::Instant::now() + wait;
    loop {
        match std::fs::remove_file(&backup) {
            Ok(()) => {
                log::info!("removed previous launcher {}", backup.display());
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(500))
            }
            Err(e) => {
                log::warn!("could not remove {} yet: {e}", backup.display());
                break;
            }
        }
    }
    let _ = std::fs::remove_file(failed_path(current_exe));
    let _ = std::fs::remove_file(staged_path(current_exe));
    clean_updates_dir(updates_dir, keep);
}

/// Remove downloaded update files, optionally keeping one.
pub fn clean_updates_dir(updates_dir: &Path, keep: Option<&Path>) {
    let Ok(rd) = std::fs::read_dir(updates_dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if Some(p.as_path()) == keep || !p.is_file() {
            continue;
        }
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // Only files this module creates.
        if name.starts_with("Kestrel-") || name.starts_with("health-") || name.ends_with(".part") {
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// Minimal check that a file is a Windows PE executable (`MZ` + `PE\0\0`).
pub fn looks_like_windows_exe(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 64];
    if f.read_exact(&mut head).is_err() || &head[..2] != b"MZ" {
        return false;
    }
    let pe = u32::from_le_bytes([head[60], head[61], head[62], head[63]]) as u64;
    if pe == 0 || pe > 4096 {
        return false;
    }
    let mut sig = [0u8; 4];
    f.seek(SeekFrom::Start(pe)).is_ok() && f.read_exact(&mut sig).is_ok() && &sig == b"PE\0\0"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let install = d.path().join("Desktop");
        std::fs::create_dir_all(&install).unwrap();
        let current = install.join("Kestrel.exe");
        std::fs::write(&current, b"OLD VERSION").unwrap();
        let new = d.path().join("Kestrel-0.2.0.exe");
        std::fs::write(&new, b"NEW VERSION").unwrap();
        (d, current, new)
    }

    #[test]
    fn successful_swap_and_cleanup() {
        let (d, current, new) = setup();
        let backup = swap(&current, &new).unwrap();
        assert_eq!(std::fs::read(&current).unwrap(), b"NEW VERSION");
        assert_eq!(std::fs::read(&backup).unwrap(), b"OLD VERSION");
        assert!(!staged_path(&current).exists());
        let updates = d.path().join("updates");
        std::fs::create_dir_all(&updates).unwrap();
        std::fs::write(updates.join("Kestrel-0.2.0.exe"), b"x").unwrap();
        std::fs::write(updates.join("unrelated.txt"), b"keep").unwrap();
        let marker = updates.join("health-0.2.0");
        std::fs::write(&marker, b"ok").unwrap();
        std::fs::write(updates.join("health-0.1.9"), b"stale").unwrap();
        cleanup_after_update(&current, &updates, Some(&marker), Duration::from_millis(10));
        assert!(!backup.exists());
        assert!(!updates.join("Kestrel-0.2.0.exe").exists());
        assert!(
            marker.exists(),
            "the marker the old launcher waits for is kept"
        );
        assert!(
            !updates.join("health-0.1.9").exists(),
            "stale markers are removed"
        );
        assert!(
            updates.join("unrelated.txt").exists(),
            "only update files are removed"
        );
    }

    #[test]
    fn rollback_restores_previous_version() {
        let (_d, current, new) = setup();
        let backup = swap(&current, &new).unwrap();
        rollback(&current, &backup).unwrap();
        assert_eq!(std::fs::read(&current).unwrap(), b"OLD VERSION");
        assert!(!backup.exists());
        assert!(!failed_path(&current).exists());
    }

    #[test]
    fn failed_staging_keeps_current_version() {
        let (d, current, _new) = setup();
        // New file missing: copy fails, nothing changes.
        let e = swap(&current, &d.path().join("missing.exe")).unwrap_err();
        assert!(matches!(e, UpdateError::Install(_)), "{e}");
        assert_eq!(std::fs::read(&current).unwrap(), b"OLD VERSION");
        assert!(!backup_path(&current).exists());
        assert!(!staged_path(&current).exists());
    }

    #[test]
    fn stale_backup_is_replaced() {
        let (_d, current, new) = setup();
        std::fs::write(backup_path(&current), b"ANCIENT").unwrap();
        let backup = swap(&current, &new).unwrap();
        assert_eq!(std::fs::read(backup).unwrap(), b"OLD VERSION");
    }

    #[cfg(unix)]
    #[test]
    fn read_only_folder_is_reported_not_modified() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, current, new) = setup();
        let dir = current.parent().unwrap().to_path_buf();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        // Root ignores directory permissions; only assert when they apply.
        let enforced = !dir_is_writable(&dir);
        let r = swap(&current, &new);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        if enforced {
            assert!(matches!(r, Err(UpdateError::NotWritable(_))));
            assert_eq!(std::fs::read(&current).unwrap(), b"OLD VERSION");
        }
    }

    /// The core assumption of the swap: Windows lets a *running* exe be
    /// renamed (not overwritten). Uses a copy of PING.EXE as the running exe.
    #[cfg(windows)]
    #[test]
    fn windows_can_swap_a_running_exe() {
        let d = tempfile::tempdir().unwrap();
        let current = d.path().join("Kestrel.exe");
        let sys = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        std::fs::copy(sys.join("System32").join("PING.EXE"), &current).unwrap();
        let mut child = std::process::Command::new(&current)
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            std::fs::write(&current, b"x").is_err(),
            "a running exe cannot be overwritten"
        );
        let new = d.path().join("new.exe");
        std::fs::copy(sys.join("System32").join("PING.EXE"), &new).unwrap();
        let backup = swap(&current, &new).expect("swap while running");
        assert!(current.is_file() && backup.is_file());
        // The backup cannot be removed while the old process runs...
        assert!(std::fs::remove_file(&backup).is_err());
        let _ = child.kill();
        let _ = child.wait();
        // ...but can once it has exited.
        cleanup_after_update(
            &current,
            &d.path().join("updates"),
            None,
            Duration::from_secs(10),
        );
        assert!(!backup.exists());
    }

    #[test]
    fn pe_detection() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.exe");
        let mut pe = vec![0u8; 256];
        pe[0] = b'M';
        pe[1] = b'Z';
        pe[60] = 128;
        pe[128..132].copy_from_slice(b"PE\0\0");
        std::fs::write(&p, &pe).unwrap();
        assert!(looks_like_windows_exe(&p));
        std::fs::write(&p, b"#!/bin/sh\necho hi\n").unwrap();
        assert!(!looks_like_windows_exe(&p));
        pe[128] = b'X';
        std::fs::write(&p, &pe).unwrap();
        assert!(!looks_like_windows_exe(&p));
    }
}
