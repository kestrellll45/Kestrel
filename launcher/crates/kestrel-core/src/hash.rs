use std::fs::File;
use std::io::Read;
use std::path::Path;

use sha1::{Digest, Sha1};

use crate::error::{IoContext, Result};

/// Lower-case hex SHA-1 of a byte slice.
pub fn sha1_bytes(data: &[u8]) -> String {
    let mut h = Sha1::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// Lower-case hex SHA-1 of a file, streamed in 64 KiB chunks.
pub fn sha1_file(path: &Path) -> Result<String> {
    let mut f = File::open(path).at(path)?;
    let mut h = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).at(path)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Returns `true` if the file exists, matches `size` (when given) and
/// matches `sha1` (when given). A file with no expected hash and no expected
/// size is considered valid if it exists and is non-empty.
pub fn file_matches(path: &Path, sha1: Option<&str>, size: Option<u64>) -> bool {
    let meta = match std::fs::metadata(path) {
        Ok(m) if m.is_file() => m,
        _ => return false,
    };
    if let Some(s) = size
        && meta.len() != s
    {
        return false;
    }
    match sha1 {
        Some(expected) => {
            matches!(sha1_file(path), Ok(actual) if actual.eq_ignore_ascii_case(expected))
        }
        None => meta.len() > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector() {
        assert_eq!(
            sha1_bytes(b"abc"),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    fn file_hash_and_match() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(sha1_file(&p).unwrap(), sha1_bytes(b"abc"));
        assert!(file_matches(
            &p,
            Some("A9993E364706816ABA3E25717850C26C9CD0D89D"),
            Some(3)
        ));
        assert!(!file_matches(&p, Some("00"), None));
        assert!(!file_matches(&p, None, Some(4)));
        assert!(!file_matches(&dir.path().join("missing"), None, None));
    }
}
