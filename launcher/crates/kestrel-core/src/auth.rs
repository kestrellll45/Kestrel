//! Microsoft account authentication for Minecraft Java Edition.
//!
//! Flow: OAuth 2.0 device code (the user signs in on microsoft.com — the
//! launcher never sees their password) → Xbox Live user token → XSTS token →
//! Minecraft services token → Minecraft profile.
//!
//! Requires an Azure app registration (public client, "Personal Microsoft
//! accounts", device code flow enabled). New app IDs must additionally be
//! approved by Mojang before `login_with_xbox` accepts them; until then step 4
//! fails with HTTP 403 and we report that explicitly.

use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{Error, Result};
use crate::paths::atomic_write;
use crate::progress::Progress;
use crate::{redact, secure};

const DEVICE_CODE_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const SCOPE: &str = "XboxLive.signin offline_access";

/// A ready-to-launch identity.
#[derive(Debug, Clone)]
pub struct Session {
    pub name: String,
    /// UUID without dashes.
    pub uuid: String,
    pub access_token: String,
    /// `msa` for Microsoft accounts, `legacy` for offline.
    pub user_type: &'static str,
}

impl Session {
    /// Offline session for singleplayer / offline-mode servers. The UUID is
    /// the standard `OfflinePlayer:<name>` v3 UUID used by vanilla servers.
    pub fn offline(name: &str) -> Session {
        use md5::{Digest, Md5};
        let mut h: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
        h[6] = (h[6] & 0x0f) | 0x30;
        h[8] = (h[8] & 0x3f) | 0x80;
        Session {
            name: name.to_string(),
            uuid: hex::encode(h),
            access_token: "0".into(),
            user_type: "legacy",
        }
    }
}

/// What is persisted (DPAPI-encrypted) between launches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAccount {
    pub name: String,
    pub uuid: String,
    pub refresh_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
    #[serde(default)]
    pub message: String,
}

fn default_interval() -> u64 {
    5
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

pub struct Authenticator {
    agent: ureq::Agent,
    client_id: String,
}

impl Authenticator {
    pub fn new(client_id: &str) -> Result<Authenticator> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            return Err(Error::Auth(
                "no Microsoft application (client) ID configured — see documentation/BUILD.md, \"Microsoft sign-in\"".into(),
            ));
        }
        // OAuth errors come back as 4xx with a JSON body we need to read.
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .user_agent(format!("Kestrel-Launcher/{}", crate::LAUNCHER_VERSION))
            .build()
            .into();
        Ok(Authenticator {
            agent,
            client_id: client_id.to_string(),
        })
    }

    pub fn start_device_code(&self) -> Result<DeviceCode> {
        let mut resp = self
            .agent
            .post(DEVICE_CODE_URL)
            .send_form([("client_id", self.client_id.as_str()), ("scope", SCOPE)])
            .map_err(|e| http_err(DEVICE_CODE_URL, e))?;
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| http_err(DEVICE_CODE_URL, e))?;
        if status != 200 {
            return Err(Error::Auth(format!(
                "device code request failed (HTTP {status}): {}",
                oauth_message(&body)
            )));
        }
        serde_json::from_str(&body).map_err(|e| Error::json("device code response", e))
    }

    /// Poll until the user completes sign-in, the code expires, or `progress`
    /// is cancelled. Returns (MS access token, refresh token).
    pub fn poll_device_code(
        &self,
        code: &DeviceCode,
        progress: &Progress,
    ) -> Result<(String, String)> {
        let deadline = Instant::now() + Duration::from_secs(code.expires_in.min(1800));
        let mut interval = code.interval.clamp(1, 60);
        loop {
            // Sleep in short slices so cancellation is responsive.
            for _ in 0..interval * 4 {
                progress.check_cancel()?;
                std::thread::sleep(Duration::from_millis(250));
            }
            if Instant::now() > deadline {
                return Err(Error::Auth("sign-in code expired; please try again".into()));
            }
            let t = self.token_request(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", self.client_id.as_str()),
                ("device_code", code.device_code.as_str()),
            ])?;
            match t.error.as_deref() {
                None => return tokens(t),
                Some("authorization_pending") => {}
                Some("slow_down") => interval += 5,
                Some("authorization_declined") => {
                    return Err(Error::Auth("sign-in was declined".into()));
                }
                Some("expired_token") => {
                    return Err(Error::Auth("sign-in code expired; please try again".into()));
                }
                Some(other) => {
                    return Err(Error::Auth(format!(
                        "{other}: {}",
                        t.error_description.unwrap_or_default()
                    )));
                }
            }
        }
    }

    pub fn refresh(&self, refresh_token: &str) -> Result<(String, String)> {
        let t = self.token_request(&[
            ("grant_type", "refresh_token"),
            ("client_id", self.client_id.as_str()),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ])?;
        if let Some(e) = &t.error {
            return Err(Error::Auth(format!(
                "session expired, please sign in again ({e}: {})",
                t.error_description.clone().unwrap_or_default()
            )));
        }
        tokens(t)
    }

    fn token_request(&self, form: &[(&str, &str)]) -> Result<TokenResponse> {
        let mut resp = self
            .agent
            .post(TOKEN_URL)
            .send_form(form.iter().copied())
            .map_err(|e| http_err(TOKEN_URL, e))?;
        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| http_err(TOKEN_URL, e))?;
        serde_json::from_str(&body).map_err(|e| Error::json("token response", e))
    }

    /// Exchange a Microsoft access token for a Minecraft session.
    pub fn minecraft_session(&self, ms_access_token: &str) -> Result<Session> {
        redact::register(ms_access_token);

        // 1. Xbox Live user token.
        let xbl: serde_json::Value = self.post_json(
            XBL_URL,
            json!({
                "Properties": {"AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={ms_access_token}")},
                "RelyingParty": "http://auth.xboxlive.com",
                "TokenType": "JWT"
            }),
            None,
        )?;
        let xbl_token = str_field(&xbl, &["Token"])?;
        let uhs = str_field(&xbl, &["DisplayClaims", "xui", "0", "uhs"])?;
        redact::register(&xbl_token);

        // 2. XSTS token for Minecraft services.
        let xsts: serde_json::Value = self.post_json(
            XSTS_URL,
            json!({
                "Properties": {"SandboxId": "RETAIL", "UserTokens": [xbl_token]},
                "RelyingParty": "rp://api.minecraftservices.com/",
                "TokenType": "JWT"
            }),
            Some(xsts_error),
        )?;
        let xsts_token = str_field(&xsts, &["Token"])?;
        redact::register(&xsts_token);

        // 3. Minecraft access token.
        let mc: serde_json::Value = self.post_json(
            MC_LOGIN_URL,
            json!({"identityToken": format!("XBL3.0 x={uhs};{xsts_token}")}),
            Some(|status, _| {
                (status == 403).then(|| {
                    "Minecraft services rejected this application ID (HTTP 403). New Azure app IDs must be approved by Mojang before they can sign in to Minecraft — see BUILD.md.".to_string()
                })
            }),
        )?;
        let mc_token = str_field(&mc, &["access_token"])?;
        redact::register(&mc_token);

        // 4. Profile (fails with 404 if the account does not own Java Edition).
        let mut resp = self
            .agent
            .get(MC_PROFILE_URL)
            .header("Authorization", format!("Bearer {mc_token}"))
            .call()
            .map_err(|e| http_err(MC_PROFILE_URL, e))?;
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| http_err(MC_PROFILE_URL, e))?;
        if status == 404 {
            return Err(Error::Auth(
                "this Microsoft account does not own Minecraft: Java Edition".into(),
            ));
        }
        if status != 200 {
            return Err(Error::Auth(format!(
                "profile request failed (HTTP {status})"
            )));
        }
        let profile: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| Error::json("profile", e))?;
        Ok(Session {
            name: str_field(&profile, &["name"])?,
            uuid: str_field(&profile, &["id"])?.replace('-', ""),
            access_token: mc_token,
            user_type: "msa",
        })
    }

    fn post_json(
        &self,
        url: &str,
        body: serde_json::Value,
        explain: Option<fn(u16, &str) -> Option<String>>,
    ) -> Result<serde_json::Value> {
        let mut resp = self
            .agent
            .post(url)
            .header("Accept", "application/json")
            .send_json(body)
            .map_err(|e| http_err(url, e))?;
        let status = resp.status().as_u16();
        let text = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| http_err(url, e))?;
        if status != 200 {
            if let Some(msg) = explain.and_then(|f| f(status, &text)) {
                return Err(Error::Auth(msg));
            }
            return Err(Error::Auth(format!("{url} returned HTTP {status}")));
        }
        serde_json::from_str(&text).map_err(|e| Error::json(url.to_string(), e))
    }
}

fn xsts_error(status: u16, body: &str) -> Option<String> {
    if status != 401 {
        return None;
    }
    let code = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("XErr").and_then(|x| x.as_u64()));
    Some(match code {
        Some(2148916233) => "this Microsoft account has no Xbox profile; sign in once at minecraft.net to create one".into(),
        Some(2148916235) => "Xbox Live is not available in your country".into(),
        Some(2148916236) | Some(2148916237) => "this account needs adult verification on xbox.com".into(),
        Some(2148916238) => "child accounts must be added to a Microsoft Family by an adult".into(),
        other => format!("Xbox authorization failed (XErr {other:?})"),
    })
}

fn tokens(t: TokenResponse) -> Result<(String, String)> {
    match (t.access_token, t.refresh_token) {
        (Some(a), Some(r)) => {
            redact::register(&a);
            redact::register(&r);
            Ok((a, r))
        }
        _ => Err(Error::Auth("token response missing tokens".into())),
    }
}

fn str_field(v: &serde_json::Value, path: &[&str]) -> Result<String> {
    let mut cur = v;
    for p in path {
        cur = match p.parse::<usize>() {
            Ok(i) => cur.get(i),
            Err(_) => cur.get(*p),
        }
        .ok_or_else(|| Error::Auth(format!("unexpected response: missing {}", path.join("."))))?;
    }
    cur.as_str().map(str::to_string).ok_or_else(|| {
        Error::Auth(format!(
            "unexpected response: {} is not a string",
            path.join(".")
        ))
    })
}

fn oauth_message(body: &str) -> String {
    serde_json::from_str::<TokenResponse>(body)
        .ok()
        .and_then(|t| t.error_description.or(t.error))
        .unwrap_or_else(|| "unexpected response".into())
}

fn http_err(url: &str, e: ureq::Error) -> Error {
    Error::Http {
        url: url.to_string(),
        message: e.to_string(),
    }
}

// ---- persistence -----------------------------------------------------------

/// Save the account encrypted with DPAPI. On platforms without secure
/// storage nothing is written (the user signs in each session).
pub fn save_account(path: &Path, acct: &StoredAccount) -> Result<()> {
    if !secure::is_supported() {
        log::warn!("secure storage unavailable on this platform; account not persisted");
        return Ok(());
    }
    let plain = serde_json::to_vec(acct).map_err(|e| Error::json("account", e))?;
    let cipher = secure::protect(&plain)?;
    atomic_write(path, &cipher)
}

pub fn load_account(path: &Path) -> Option<StoredAccount> {
    let cipher = std::fs::read(path).ok()?;
    match secure::unprotect(&cipher)
        .and_then(|p| serde_json::from_slice(&p).map_err(|e| Error::json("account", e)))
    {
        Ok(a) => {
            let a: StoredAccount = a;
            redact::register(&a.refresh_token);
            Some(a)
        }
        Err(e) => {
            log::warn!("stored account unreadable ({e}); sign in again");
            None
        }
    }
}

pub fn forget_account(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Seconds since the Unix epoch (for logs/UI).
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_vanilla() {
        // Known value: offline UUID for "Notch" is b50ad385-829d-3141-a216-7e7d7539ba7f.
        let s = Session::offline("Notch");
        assert_eq!(s.uuid, "b50ad385829d3141a2167e7d7539ba7f");
        assert_eq!(s.user_type, "legacy");
    }

    #[test]
    fn requires_client_id() {
        assert!(Authenticator::new("  ").is_err());
    }

    #[test]
    fn json_path() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"DisplayClaims":{"xui":[{"uhs":"123"}]}}"#).unwrap();
        assert_eq!(
            str_field(&v, &["DisplayClaims", "xui", "0", "uhs"]).unwrap(),
            "123"
        );
        assert!(str_field(&v, &["nope"]).is_err());
    }

    #[test]
    fn xsts_messages() {
        assert!(
            xsts_error(401, r#"{"XErr":2148916238}"#)
                .unwrap()
                .contains("child")
        );
        assert!(xsts_error(200, "").is_none());
    }
}
