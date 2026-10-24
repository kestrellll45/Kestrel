//! Maven coordinate handling (`group:artifact:version[:classifier][@ext]`).

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coord {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    pub ext: String,
}

impl Coord {
    pub fn parse(s: &str) -> Result<Coord> {
        let (main, ext) = match s.split_once('@') {
            Some((m, e)) => (m, e.to_string()),
            None => (s, "jar".to_string()),
        };
        let parts: Vec<&str> = main.split(':').collect();
        let bad = || Error::Metadata(format!("invalid maven coordinate: {s}"));
        if parts.len() < 3 || parts.len() > 4 || parts.iter().any(|p| p.is_empty()) {
            return Err(bad());
        }
        // Each segment becomes a path component; forbid traversal.
        if parts
            .iter()
            .chain(std::iter::once(&ext.as_str()))
            .any(|p| p.contains('/') || p.contains('\\') || *p == ".." || p.starts_with('.'))
        {
            return Err(bad());
        }
        Ok(Coord {
            group: parts[0].to_string(),
            artifact: parts[1].to_string(),
            version: parts[2].to_string(),
            classifier: parts.get(3).map(|c| c.to_string()),
            ext,
        })
    }

    /// `group:artifact` — used to de-duplicate libraries across versions.
    pub fn key(&self) -> String {
        match &self.classifier {
            Some(c) => format!("{}:{}:{}", self.group, self.artifact, c),
            None => format!("{}:{}", self.group, self.artifact),
        }
    }

    /// Relative path using `/` separators, e.g. `org/ow2/asm/asm-all/5.0.3/asm-all-5.0.3.jar`.
    pub fn path(&self) -> String {
        let file = match &self.classifier {
            Some(c) => format!("{}-{}-{}.{}", self.artifact, self.version, c, self.ext),
            None => format!("{}-{}.{}", self.artifact, self.version, self.ext),
        };
        format!(
            "{}/{}/{}/{}",
            self.group.replace('.', "/"),
            self.artifact,
            self.version,
            file
        )
    }

    pub fn with_classifier(&self, classifier: &str) -> Coord {
        Coord {
            classifier: Some(classifier.to_string()),
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_path() {
        let c = Coord::parse("org.ow2.asm:asm-all:5.0.3").unwrap();
        assert_eq!(c.path(), "org/ow2/asm/asm-all/5.0.3/asm-all-5.0.3.jar");
        let n = c.with_classifier("natives-windows");
        assert_eq!(
            n.path(),
            "org/ow2/asm/asm-all/5.0.3/asm-all-5.0.3-natives-windows.jar"
        );
        let z = Coord::parse("a.b:c:1@zip").unwrap();
        assert_eq!(z.path(), "a/b/c/1/c-1.zip");
    }

    #[test]
    fn rejects_bad() {
        assert!(Coord::parse("a:b").is_err());
        assert!(Coord::parse("a:..:1").is_err());
        assert!(Coord::parse("a:b/c:1").is_err());
        assert!(Coord::parse("a::1").is_err());
    }
}
