//! Signed release manifest (`kestrel-update.json`).
//!
//! The manifest is an envelope around a base64 payload:
//!
//! ```json
//! { "format": "kestrel-update-v1", "key_id": "1a2b3c4d",
//!   "payload": "<base64 of the release JSON>", "signature": "<base64 Ed25519>" }
//! ```
//!
//! The signature covers `SIGNING_CONTEXT || payload bytes` exactly as
//! published, so there is no JSON canonicalisation to get wrong. The launcher
//! verifies it with a public key compiled into the executable; the private
//! key never leaves the publisher's machine. Only after the signature checks
//! out is the payload parsed and validated.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::fetch::validate_url;
use super::{UpdateError, UpdateSource};

pub const FORMAT: &str = "kestrel-update-v1";
/// Domain separation: a signature over a manifest can never be confused with
/// a signature over anything else made with the same key.
pub const SIGNING_CONTEXT: &[u8] = b"kestrel-update-v1\0";
pub const PRODUCT: &str = "kestrel-launcher";
pub const CHANNEL_STABLE: &str = "stable";

/// Upper bound on the manifest file itself.
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
/// Upper bound on a launcher executable (current builds are ~22 MB).
pub const MAX_EXE_BYTES: u64 = 256 * 1024 * 1024;
const MIN_EXE_BYTES: u64 = 64 * 1024;
const MAX_NOTES_CHARS: usize = 8 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub format: String,
    pub key_id: String,
    pub payload: String,
    pub signature: String,
}

/// The Windows launcher download described by a release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// Release information as published (the signed payload).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub schema: u32,
    pub product: String,
    pub channel: String,
    pub version: String,
    /// RFC 3339 publication time (informational).
    #[serde(default)]
    pub published: String,
    /// Launchers older than this should treat the update as required.
    #[serde(default)]
    pub minimum_supported: Option<String>,
    #[serde(default)]
    pub notes: String,
    pub windows_x86_64: Asset,
}

/// A verified and validated release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub minimum_supported: Option<Version>,
    pub notes: String,
    pub published: String,
    pub asset: Asset,
}

impl Release {
    pub fn is_newer_than(&self, current: &Version) -> bool {
        self.version > *current
    }

    /// The running launcher is older than the release's minimum supported version.
    pub fn is_required_for(&self, current: &Version) -> bool {
        self.minimum_supported.as_ref().is_some_and(|m| current < m)
    }
}

/// Short identifier of a public key: first 8 hex digits of its SHA-256.
pub fn key_id(public_key: &[u8; 32]) -> String {
    hex::encode(&Sha256::digest(public_key)[..4])
}

pub fn parse_public_key(text: &str) -> Result<VerifyingKey, UpdateError> {
    let bytes = B64
        .decode(text.trim())
        .map_err(|_| UpdateError::Config("update public key is not valid base64".into()))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| UpdateError::Config("update public key must be 32 bytes".into()))?;
    VerifyingKey::from_bytes(&arr)
        .map_err(|_| UpdateError::Config("update public key is invalid".into()))
}

/// Strict "x.y.z" (optionally with a pre-release tag) without build metadata or a leading "v".
pub fn parse_version(s: &str) -> Result<Version, UpdateError> {
    let v = Version::parse(s.trim())
        .map_err(|e| UpdateError::Manifest(format!("invalid version {s:?}: {e}")))?;
    if !v.build.is_empty() {
        return Err(UpdateError::Manifest(format!(
            "version {s:?} must not carry build metadata"
        )));
    }
    Ok(v)
}

/// Verify the envelope's signature, then parse and validate the payload.
pub fn verify_and_parse(bytes: &[u8], source: &UpdateSource) -> Result<Release, UpdateError> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(UpdateError::Manifest(
            "release manifest is too large".into(),
        ));
    }
    let env: Envelope = serde_json::from_slice(bytes)
        .map_err(|e| UpdateError::Manifest(format!("release manifest is not valid JSON: {e}")))?;
    if env.format != FORMAT {
        return Err(UpdateError::Manifest(format!(
            "unsupported manifest format {:?}",
            env.format
        )));
    }
    let key = &source.public_key;
    let expected_id = key_id(&key.to_bytes());
    if env.key_id != expected_id {
        return Err(UpdateError::Signature(format!(
            "manifest was signed with key {} but this launcher trusts key {expected_id}",
            env.key_id
        )));
    }
    let payload = B64
        .decode(env.payload.as_bytes())
        .map_err(|_| UpdateError::Manifest("manifest payload is not valid base64".into()))?;
    let sig_bytes = B64
        .decode(env.signature.as_bytes())
        .map_err(|_| UpdateError::Signature("signature is not valid base64".into()))?;
    let signature = Signature::from_slice(&sig_bytes)
        .map_err(|_| UpdateError::Signature("malformed signature".into()))?;
    let mut message = Vec::with_capacity(SIGNING_CONTEXT.len() + payload.len());
    message.extend_from_slice(SIGNING_CONTEXT);
    message.extend_from_slice(&payload);
    key.verify_strict(&message, &signature)
        .map_err(|_| UpdateError::Signature("release manifest signature is invalid".into()))?;

    let info: ReleaseInfo = serde_json::from_slice(&payload)
        .map_err(|e| UpdateError::Manifest(format!("release information is malformed: {e}")))?;
    validate(info, source)
}

fn validate(info: ReleaseInfo, source: &UpdateSource) -> Result<Release, UpdateError> {
    let bad = |m: String| Err(UpdateError::Manifest(m));
    if info.schema != 1 {
        return bad(format!("unsupported release schema {}", info.schema));
    }
    if info.product != PRODUCT {
        return bad(format!("release is for {:?}, not {PRODUCT}", info.product));
    }
    if info.channel != source.channel {
        return bad(format!(
            "release is on channel {:?}, expected {:?}",
            info.channel, source.channel
        ));
    }
    let version = parse_version(&info.version)?;
    let minimum_supported = match &info.minimum_supported {
        Some(m) if !m.trim().is_empty() => Some(parse_version(m)?),
        _ => None,
    };
    if minimum_supported.as_ref().is_some_and(|m| *m > version) {
        return bad("minimum supported version is newer than the release itself".into());
    }
    let a = &info.windows_x86_64;
    validate_url(&a.url, &source.allowed_hosts).map_err(UpdateError::Manifest)?;
    if !a.url.to_ascii_lowercase().ends_with(".exe") {
        return bad("download is not a Windows .exe".into());
    }
    if a.sha256.len() != 64 || !a.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return bad("sha256 must be 64 hex digits".into());
    }
    if !(MIN_EXE_BYTES..=MAX_EXE_BYTES).contains(&a.size) {
        return bad(format!("implausible download size {}", a.size));
    }
    let mut notes: String = info
        .notes
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect();
    if notes.chars().count() > MAX_NOTES_CHARS {
        notes = notes.chars().take(MAX_NOTES_CHARS).collect::<String>() + "…";
    }
    Ok(Release {
        version,
        minimum_supported,
        notes,
        published: info.published,
        asset: Asset {
            url: a.url.clone(),
            sha256: a.sha256.to_ascii_lowercase(),
            size: a.size,
        },
    })
}

/// Build and sign an envelope (used by the `kestrel-release` tool and tests).
pub fn sign(info: &ReleaseInfo, key: &ed25519_dalek::SigningKey) -> Envelope {
    use ed25519_dalek::Signer as _;
    let payload = serde_json::to_vec_pretty(info).expect("release info serialises");
    let mut message = Vec::with_capacity(SIGNING_CONTEXT.len() + payload.len());
    message.extend_from_slice(SIGNING_CONTEXT);
    message.extend_from_slice(&payload);
    let signature = key.sign(&message);
    Envelope {
        format: FORMAT.into(),
        key_id: key_id(&key.verifying_key().to_bytes()),
        payload: B64.encode(&payload),
        signature: B64.encode(signature.to_bytes()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    pub fn test_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    pub fn source(key: &SigningKey) -> UpdateSource {
        UpdateSource::new(
            "https://github.com/owner/kestrel/releases/latest/download/kestrel-update.json",
            &B64.encode(key.verifying_key().to_bytes()),
        )
        .unwrap()
    }

    pub fn info(version: &str) -> ReleaseInfo {
        ReleaseInfo {
            schema: 1,
            product: PRODUCT.into(),
            channel: CHANNEL_STABLE.into(),
            version: version.into(),
            published: "2026-10-25T12:00:00Z".into(),
            minimum_supported: Some("0.1.0".into()),
            notes: "Fixes and improvements.\n- Faster startup".into(),
            windows_x86_64: Asset {
                url: format!(
                    "https://github.com/owner/kestrel/releases/download/v{version}/Kestrel.exe"
                ),
                sha256: "ab".repeat(32),
                size: 22_000_000,
            },
        }
    }

    fn signed(info: &ReleaseInfo, key: &SigningKey) -> Vec<u8> {
        serde_json::to_vec(&sign(info, key)).unwrap()
    }

    #[test]
    fn version_ordering() {
        let v = |s| parse_version(s).unwrap();
        assert!(v("0.1.1") > v("0.1.0"));
        assert!(v("0.2.0") > v("0.1.9"));
        assert!(v("0.10.0") > v("0.9.0"), "numeric, not lexicographic");
        assert!(v("1.0.0") > v("1.0.0-beta.2"));
        assert!(v("1.0.0-beta.10") > v("1.0.0-beta.2"));
        assert_eq!(v("0.1.0"), v(" 0.1.0 "));
        for bad in ["v0.1.0", "0.1", "1.2.3.4", "", "x.y.z", "0.1.0+build5"] {
            assert!(parse_version(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn valid_manifest_round_trips() {
        let k = test_key(7);
        let r = verify_and_parse(&signed(&info("0.2.0"), &k), &source(&k)).unwrap();
        assert_eq!(r.version, Version::new(0, 2, 0));
        assert!(r.is_newer_than(&Version::new(0, 1, 0)));
        assert!(
            !r.is_newer_than(&Version::new(0, 2, 0)),
            "same version is up to date"
        );
        assert!(
            !r.is_newer_than(&Version::new(0, 3, 0)),
            "never offer a downgrade"
        );
        assert!(!r.is_required_for(&Version::new(0, 1, 0)));
        assert!(r.is_required_for(&parse_version("0.0.9").unwrap()));
    }

    #[test]
    fn rejects_wrong_key_and_tampering() {
        let k = test_key(7);
        let other = test_key(8);
        // Signed by an untrusted key.
        let e = verify_and_parse(&signed(&info("0.2.0"), &other), &source(&k)).unwrap_err();
        assert!(matches!(e, UpdateError::Signature(_)), "{e}");
        // Payload swapped after signing (key id forged to match).
        let mut env = sign(&info("0.2.0"), &k);
        env.payload = B64.encode(serde_json::to_vec(&info("9.9.9")).unwrap());
        let e = verify_and_parse(&serde_json::to_vec(&env).unwrap(), &source(&k)).unwrap_err();
        assert!(matches!(e, UpdateError::Signature(_)), "{e}");
        // Signature bytes corrupted.
        let mut env = sign(&info("0.2.0"), &k);
        env.signature = B64.encode([0u8; 64]);
        assert!(verify_and_parse(&serde_json::to_vec(&env).unwrap(), &source(&k)).is_err());
    }

    #[test]
    fn rejects_malformed_or_missing_metadata() {
        let k = test_key(7);
        let s = source(&k);
        for raw in [
            &b""[..],
            b"{",
            b"null",
            b"[]",
            b"{\"format\":\"kestrel-update-v1\"}",
        ] {
            assert!(verify_and_parse(raw, &s).is_err());
        }
        let mut env = sign(&info("0.2.0"), &k);
        env.format = "kestrel-update-v0".into();
        assert!(verify_and_parse(&serde_json::to_vec(&env).unwrap(), &s).is_err());
        assert!(verify_and_parse(&vec![b' '; MAX_MANIFEST_BYTES + 1], &s).is_err());
    }

    type Mutation = Box<dyn Fn(&mut ReleaseInfo)>;

    #[test]
    fn rejects_invalid_release_fields_even_when_signed() {
        let k = test_key(7);
        let s = source(&k);
        let cases: Vec<(&str, Mutation)> = vec![
            (
                "http url",
                Box::new(|i| {
                    i.windows_x86_64.url = i.windows_x86_64.url.replace("https://", "http://")
                }),
            ),
            (
                "foreign host",
                Box::new(|i| i.windows_x86_64.url = "https://evil.example/Kestrel.exe".into()),
            ),
            (
                "look-alike host",
                Box::new(|i| {
                    i.windows_x86_64.url = "https://github.com.evil.example/Kestrel.exe".into()
                }),
            ),
            (
                "userinfo",
                Box::new(|i| {
                    i.windows_x86_64.url = "https://github.com@evil.example/Kestrel.exe".into()
                }),
            ),
            (
                "not exe",
                Box::new(|i| {
                    i.windows_x86_64.url =
                        "https://github.com/o/r/releases/download/v1/run.bat".into()
                }),
            ),
            (
                "bad hash",
                Box::new(|i| i.windows_x86_64.sha256 = "zz".into()),
            ),
            ("tiny", Box::new(|i| i.windows_x86_64.size = 10)),
            (
                "huge",
                Box::new(|i| i.windows_x86_64.size = MAX_EXE_BYTES + 1),
            ),
            ("bad version", Box::new(|i| i.version = "latest".into())),
            (
                "other product",
                Box::new(|i| i.product = "something-else".into()),
            ),
            ("other channel", Box::new(|i| i.channel = "beta".into())),
            ("schema", Box::new(|i| i.schema = 2)),
            (
                "min > version",
                Box::new(|i| i.minimum_supported = Some("9.0.0".into())),
            ),
        ];
        for (name, mutate) in cases {
            let mut i = info("0.2.0");
            mutate(&mut i);
            assert!(
                verify_and_parse(&signed(&i, &k), &s).is_err(),
                "{name} should be rejected"
            );
        }
    }

    #[test]
    fn notes_are_sanitised() {
        let k = test_key(7);
        let mut i = info("0.2.0");
        i.notes = format!("ok\u{1b}[31m\n{}", "x".repeat(20_000));
        let r = verify_and_parse(&signed(&i, &k), &source(&k)).unwrap();
        assert!(!r.notes.contains('\u{1b}'));
        assert!(r.notes.chars().count() <= MAX_NOTES_CHARS + 1);
    }
}
