//! Asset index handling (`assets/indexes`, `assets/objects`).

use std::collections::HashSet;
use std::path::Path;

use crate::download::Task;
use crate::error::{Error, Result};
use crate::mojang::{AssetIndex, RESOURCES_BASE};

pub fn parse_index(bytes: &[u8]) -> Result<AssetIndex> {
    serde_json::from_slice(bytes).map_err(|e| Error::json("asset index", e))
}

fn valid_hash(h: &str) -> bool {
    h.len() == 40 && h.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Download tasks for every object in the index (deduplicated by hash).
pub fn object_tasks(index: &AssetIndex, assets_root: &Path) -> Result<Vec<Task>> {
    let objects = assets_root.join("objects");
    let mut seen = HashSet::new();
    let mut tasks = Vec::with_capacity(index.objects.len());
    for (name, obj) in &index.objects {
        if !valid_hash(&obj.hash) {
            return Err(Error::Metadata(format!("bad asset hash for {name}")));
        }
        let hash = obj.hash.to_ascii_lowercase();
        if !seen.insert(hash.clone()) {
            continue;
        }
        let prefix = &hash[..2];
        tasks.push(Task::new(
            format!("{RESOURCES_BASE}{prefix}/{hash}"),
            objects.join(prefix).join(&hash),
            Some(hash.clone()),
            Some(obj.size),
        ));
    }
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_tasks() {
        let idx = parse_index(
            br#"{"objects":{
                "a.ogg":{"hash":"0123456789abcdef0123456789abcdef01234567","size":5},
                "b.ogg":{"hash":"0123456789abcdef0123456789abcdef01234567","size":5},
                "c.png":{"hash":"ffffffffffffffffffffffffffffffffffffffff","size":1}}}"#,
        )
        .unwrap();
        let t = object_tasks(&idx, Path::new("/A")).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(
            t[0].url,
            "https://resources.download.minecraft.net/01/0123456789abcdef0123456789abcdef01234567"
        );
        assert!(
            t[0].path
                .ends_with("objects/01/0123456789abcdef0123456789abcdef01234567")
        );
    }

    #[test]
    fn rejects_traversal_hash() {
        let idx = parse_index(br#"{"objects":{"x":{"hash":"../../../../etc/passwd","size":1}}}"#)
            .unwrap();
        assert!(object_tasks(&idx, Path::new("/A")).is_err());
    }
}
