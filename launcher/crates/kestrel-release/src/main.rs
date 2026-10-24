//! Publisher tool for Kestrel releases. NOT shipped to users.
//!
//! ```text
//! kestrel-release keygen <private-key-file>            create a signing key; prints the public key
//! kestrel-release pubkey <private-key-file>            print the public key for an existing private key
//! kestrel-release sign --key <file> --exe <Kestrel.exe> --version <x.y.z>
//!                      --url <download url> --notes <notes.md> --out <kestrel-update.json>
//!                      [--minimum <x.y.z>] [--channel stable]
//! kestrel-release verify --manifest <file> --pubkey <base64> [--exe <file>]
//! ```
//!
//! The private key file holds the 32-byte Ed25519 seed as base64. Keep it out
//! of the repository and backed up: losing it means existing launchers can no
//! longer accept updates (they would need one manual reinstall).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::SigningKey;
use kestrel_core::update::manifest::{self, Asset, ReleaseInfo};
use kestrel_core::update::{UpdateSource, fetch};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("keygen") => keygen(args.get(1).map(PathBuf::from)),
        Some("pubkey") => args
            .get(1)
            .ok_or_else(|| "usage: pubkey <private-key-file>".to_string())
            .and_then(|p| load_key(Path::new(p)))
            .map(|k| println!("{}", B64.encode(k.verifying_key().to_bytes()))),
        Some("sign") => sign(&args[1..]),
        Some("verify") => verify(&args[1..]),
        _ => Err("usage: kestrel-release keygen|pubkey|sign|verify ... (see source header)".into()),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn opt(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn req(args: &[String], name: &str) -> Result<String, String> {
    opt(args, name).ok_or_else(|| format!("missing {name}"))
}

fn keygen(path: Option<PathBuf>) -> Result<(), String> {
    let path = path.ok_or("usage: keygen <private-key-file>")?;
    if path.exists() {
        return Err(format!(
            "{} already exists; refusing to overwrite a signing key",
            path.display()
        ));
    }
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|e| format!("no secure randomness: {e}"))?;
    let key = SigningKey::from_bytes(&seed);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    write_private(&path, &B64.encode(seed))?;
    println!("{}", B64.encode(key.verifying_key().to_bytes()));
    eprintln!(
        "private key written to {} — keep it secret and back it up",
        path.display()
    );
    Ok(())
}

#[cfg(unix)]
fn write_private(path: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| e.to_string())?;
    f.write_all(text.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn write_private(path: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    // %APPDATA% is per-user on Windows; setup-updates.ps1 also restricts the ACL.
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    f.write_all(text.as_bytes()).map_err(|e| e.to_string())
}

fn load_key(path: &Path) -> Result<SigningKey, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let seed: [u8; 32] = B64
        .decode(text.trim())
        .map_err(|_| "private key file is not base64".to_string())?
        .try_into()
        .map_err(|_| "private key must be 32 bytes".to_string())?;
    Ok(SigningKey::from_bytes(&seed))
}

fn sign(args: &[String]) -> Result<(), String> {
    let key = load_key(Path::new(&req(args, "--key")?))?;
    let exe = PathBuf::from(req(args, "--exe")?);
    let version = req(args, "--version")?;
    manifest::parse_version(&version).map_err(|e| e.to_string())?;
    let url = req(args, "--url")?;
    let notes = match opt(args, "--notes") {
        Some(p) => {
            std::fs::read_to_string(&p).map_err(|e| format!("cannot read notes {p}: {e}"))?
        }
        None => String::new(),
    };
    let out = PathBuf::from(req(args, "--out")?);
    let size = std::fs::metadata(&exe)
        .map_err(|e| format!("{}: {e}", exe.display()))?
        .len();
    let sha256 = fetch::sha256_file(&exe).map_err(|e| e.to_string())?;
    let info = ReleaseInfo {
        schema: 1,
        product: manifest::PRODUCT.into(),
        channel: opt(args, "--channel").unwrap_or_else(|| manifest::CHANNEL_STABLE.into()),
        version,
        published: opt(args, "--published").unwrap_or_default(),
        minimum_supported: opt(args, "--minimum"),
        notes,
        windows_x86_64: Asset { url, sha256, size },
    };
    let envelope = manifest::sign(&info, &key);
    // Self-verify with the derived public key before writing anything.
    let pk = B64.encode(key.verifying_key().to_bytes());
    let manifest_url =
        opt(args, "--manifest-url").unwrap_or_else(|| info.windows_x86_64.url.clone());
    let source = UpdateSource::new(&manifest_url, &pk).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(&envelope).map_err(|e| e.to_string())?;
    manifest::verify_and_parse(&bytes, &source)
        .map_err(|e| format!("generated manifest does not verify: {e}"))?;
    std::fs::write(&out, &bytes).map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    println!(
        "signed {} ({} bytes, sha256 {})",
        out.display(),
        size,
        info.windows_x86_64.sha256
    );
    Ok(())
}

fn verify(args: &[String]) -> Result<(), String> {
    let path = req(args, "--manifest")?;
    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let manifest_url = opt(args, "--manifest-url").unwrap_or_else(|| "https://github.com/".into());
    let source =
        UpdateSource::new(&manifest_url, &req(args, "--pubkey")?).map_err(|e| e.to_string())?;
    let r = manifest::verify_and_parse(&bytes, &source).map_err(|e| e.to_string())?;
    if let Some(exe) = opt(args, "--exe") {
        let h = fetch::sha256_file(Path::new(&exe)).map_err(|e| e.to_string())?;
        if h != r.asset.sha256 {
            return Err(format!(
                "{exe} does not match the manifest (sha256 {h}, manifest {})",
                r.asset.sha256
            ));
        }
    }
    println!(
        "OK: Kestrel {} -> {} ({} bytes)",
        r.version, r.asset.url, r.asset.size
    );
    Ok(())
}
