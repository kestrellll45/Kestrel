//! Extraction of LWJGL/JInput/Twitch native libraries.

use std::fs::File;
use std::io;
use std::path::Path;

use crate::error::{Error, IoContext, Result};

/// Extract every non-excluded file entry of `jar` into `dest`. Paths are
/// sanitised (zip-slip safe) and entries larger than 64 MiB are rejected.
pub fn extract(jar: &Path, dest: &Path, exclude: &[String]) -> Result<usize> {
    let f = File::open(jar).at(jar)?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| Error::Archive {
        path: jar.to_path_buf(),
        message: e.to_string(),
    })?;
    std::fs::create_dir_all(dest).at(dest)?;
    let mut count = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| Error::Archive {
            path: jar.to_path_buf(),
            message: e.to_string(),
        })?;
        let Ok(name) = entry.name().map(|n| n.into_owned()) else {
            log::warn!(
                "skipping zip entry with undecodable name in {}",
                jar.display()
            );
            continue;
        };
        if entry.is_dir() || exclude.iter().any(|ex| name.starts_with(ex.as_str())) {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else {
            log::warn!("skipping unsafe zip entry {name} in {}", jar.display());
            continue;
        };
        if entry.size() > 64 * 1024 * 1024 {
            return Err(Error::Archive {
                path: jar.to_path_buf(),
                message: format!("entry {name} is too large"),
            });
        }
        let out_path = dest.join(rel);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).at(parent)?;
        }
        let mut out = File::create(&out_path).at(&out_path)?;
        io::copy(&mut entry, &mut out).at(&out_path)?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_jar(path: &Path, entries: &[(&str, &[u8])]) {
        let f = File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        for (n, d) in entries {
            w.start_file(*n, opts).unwrap();
            w.write_all(d).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn extracts_with_exclude_and_rejects_slip() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("n.jar");
        make_jar(
            &jar,
            &[
                ("lwjgl64.dll", b"dll"),
                ("META-INF/MANIFEST.MF", b"m"),
                ("../evil.dll", b"x"),
            ],
        );
        let out = dir.path().join("out");
        let n = extract(&jar, &out, &["META-INF/".to_string()]).unwrap();
        assert_eq!(n, 1);
        assert_eq!(std::fs::read(out.join("lwjgl64.dll")).unwrap(), b"dll");
        assert!(!out.join("META-INF").exists());
        assert!(!dir.path().join("evil.dll").exists());
    }
}
