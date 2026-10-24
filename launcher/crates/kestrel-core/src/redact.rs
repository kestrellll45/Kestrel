//! Remove secrets from text before it is logged or displayed.

use std::sync::{Mutex, OnceLock};

fn secrets() -> &'static Mutex<Vec<String>> {
    static S: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Vec::new()))
}

/// Register a secret (e.g. an access token) so it is masked everywhere.
/// Short values are ignored to avoid masking ordinary words.
pub fn register(secret: &str) {
    if secret.len() < 12 {
        return;
    }
    if let Ok(mut s) = secrets().lock()
        && !s.iter().any(|x| x == secret)
    {
        s.push(secret.to_string());
    }
}

/// Mask registered secrets and anything following `--accessToken`.
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    if let Ok(s) = secrets().lock() {
        for secret in s.iter() {
            if out.contains(secret.as_str()) {
                out = out.replace(secret.as_str(), "<redacted>");
            }
        }
    }
    redact_flag(&out, "--accessToken")
}

fn redact_flag(text: &str, flag: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(flag) {
        let after = i + flag.len();
        result.push_str(&rest[..after]);
        let tail = &rest[after..];
        let ws = tail.len() - tail.trim_start_matches([' ', ',', '=']).len();
        result.push_str(&tail[..ws]);
        let value_len = tail[ws..]
            .find(|c: char| c.is_whitespace() || c == ',' || c == ']')
            .unwrap_or(tail.len() - ws);
        if value_len > 0 {
            result.push_str("<redacted>");
        }
        rest = &tail[ws + value_len..];
    }
    result.push_str(rest);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_tokens() {
        register("eyJhbGciOiJIUzI1NiJ9.secret.value");
        let s = redact("token eyJhbGciOiJIUzI1NiJ9.secret.value here");
        assert_eq!(s, "token <redacted> here");
        assert_eq!(
            redact("a --accessToken abc123 --userType msa"),
            "a --accessToken <redacted> --userType msa"
        );
        assert_eq!(
            redact("[--accessToken, abc, x]"),
            "[--accessToken, <redacted>, x]"
        );
        assert_eq!(redact("no secrets"), "no secrets");
    }
}
