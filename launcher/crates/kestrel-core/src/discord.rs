//! Discord Rich Presence over Discord's local IPC (RPC version 1).
//!
//! Discord desktop listens on `\\?\pipe\discord-ipc-{0..9}` (Windows) or a
//! `discord-ipc-{0..9}` socket in `$XDG_RUNTIME_DIR`, `$TMPDIR`, `$TMP`,
//! `$TEMP` or `/tmp` (Linux/macOS). A frame is `op: u32 LE`, `len: u32 LE`,
//! then `len` bytes of JSON. The client sends a HANDSHAKE (`{"v":1,
//! "client_id":…}`), waits for the READY dispatch, then sends
//! `SET_ACTIVITY` commands; Discord answers each with a frame carrying the
//! same nonce.
//!
//! Nothing here talks to the network: the only peer is the local Discord
//! app. [`PresenceService`] owns one background thread that connects,
//! reconnects with backoff when Discord is not running, coalesces updates
//! and clears the activity on shutdown. It never spawns processes.

use std::io::{self, Read, Write};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

pub const OP_HANDSHAKE: u32 = 0;
pub const OP_FRAME: u32 = 1;
pub const OP_CLOSE: u32 = 2;
pub const OP_PING: u32 = 3;
pub const OP_PONG: u32 = 4;
/// Largest frame accepted from Discord (READY carries the user object; this
/// is far above any real reply).
const MAX_FRAME: u32 = 64 * 1024;
/// Discord rate-limits presence updates; never send more often than this.
const MIN_UPDATE_INTERVAL: Duration = Duration::from_secs(2);
const RETRY_MIN: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(60);

/// A byte stream to the Discord client (named pipe or Unix socket).
pub trait Transport: Read + Write + Send {}
impl<T: Read + Write + Send> Transport for T {}

/// Opens a connection to Discord. Injected so tests can use a fake server.
pub type Connector = Box<dyn Fn() -> io::Result<Box<dyn Transport>> + Send>;

/// What Discord shows. Kept small and free of anything private: no server
/// addresses, world names or account data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presence {
    /// First line, e.g. "In the main menu".
    pub details: String,
    /// Second line, e.g. "Minecraft 1.8.9".
    pub state: Option<String>,
    /// Unix seconds; Discord shows elapsed time since then.
    pub start_unix: Option<u64>,
    /// Art asset key uploaded to the Discord application (Rich Presence → Art Assets).
    pub large_image: Option<String>,
    pub large_text: Option<String>,
}

/// Validate a Discord application ID (a snowflake: 17–20 decimal digits).
pub fn validate_client_id(id: &str) -> Result<String, String> {
    let id = id.trim();
    if id.is_empty() {
        return Err("no Discord application ID set".into());
    }
    if !(17..=20).contains(&id.len()) || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(
            "a Discord application ID is 17–20 digits (Developer Portal → your app → General Information → Application ID)"
                .into(),
        );
    }
    Ok(id.to_string())
}

pub fn encode_frame(op: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(&op.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// Read one frame, refusing oversized lengths before allocating.
pub fn read_frame(r: &mut dyn Read) -> io::Result<(u32, Value)> {
    let mut head = [0u8; 8];
    r.read_exact(&mut head)?;
    let op = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
    let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
    if len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Discord frame too large ({len} bytes)"),
        ));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body)?;
    let v = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("bad JSON: {e}")))?
    };
    Ok((op, v))
}

/// The `SET_ACTIVITY` command. `None` clears the activity.
pub fn activity_command(p: Option<&Presence>, pid: u32, nonce: &str) -> Value {
    let activity = p.map(|p| {
        let mut a = json!({ "details": p.details, "type": 0, "instance": false });
        if let Some(s) = &p.state {
            a["state"] = json!(s);
        }
        if let Some(t) = p.start_unix {
            a["timestamps"] = json!({ "start": t });
        }
        let mut assets = serde_json::Map::new();
        if let Some(i) = &p.large_image {
            assets.insert("large_image".into(), json!(i));
        }
        if let Some(t) = &p.large_text {
            assets.insert("large_text".into(), json!(t));
        }
        if !assets.is_empty() {
            a["assets"] = Value::Object(assets);
        }
        a
    });
    json!({
        "cmd": "SET_ACTIVITY",
        "args": { "pid": pid, "activity": activity },
        "nonce": nonce,
    })
}

/// Why a `SET_ACTIVITY` did not succeed.
#[derive(Debug)]
pub enum ActivityError {
    /// Discord answered with an error; the connection is still usable.
    Rejected(String),
    /// The connection failed; reconnect.
    Io(io::Error),
}

impl std::fmt::Display for ActivityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActivityError::Rejected(m) => write!(f, "Discord rejected the activity: {m}"),
            ActivityError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<io::Error> for ActivityError {
    fn from(e: io::Error) -> Self {
        ActivityError::Io(e)
    }
}

/// A connected, handshaken IPC session.
pub struct Client {
    conn: Box<dyn Transport>,
    nonce: u64,
}

impl Client {
    /// Handshake over an open transport and wait for READY.
    pub fn handshake(mut conn: Box<dyn Transport>, client_id: &str) -> io::Result<Client> {
        let hello = json!({ "v": 1, "client_id": client_id }).to_string();
        conn.write_all(&encode_frame(OP_HANDSHAKE, hello.as_bytes()))?;
        conn.flush()?;
        loop {
            let (op, v) = read_frame(&mut conn)?;
            match op {
                OP_FRAME if v["evt"] == "READY" => return Ok(Client { conn, nonce: 0 }),
                OP_CLOSE => {
                    return Err(io::Error::new(
                        io::ErrorKind::ConnectionRefused,
                        format!(
                            "Discord closed the connection: {} (code {})",
                            v["message"].as_str().unwrap_or("no reason given"),
                            v["code"]
                        ),
                    ));
                }
                OP_PING => {
                    let body = serde_json::to_vec(&v).unwrap_or_default();
                    conn.write_all(&encode_frame(OP_PONG, &body))?;
                }
                _ => {} // ignore anything else before READY
            }
        }
    }

    /// Send SET_ACTIVITY and wait for Discord's reply to it.
    pub fn set_activity(&mut self, p: Option<&Presence>) -> Result<(), ActivityError> {
        self.nonce += 1;
        let nonce = format!("kestrel-{}", self.nonce);
        let cmd = activity_command(p, std::process::id(), &nonce).to_string();
        self.conn
            .write_all(&encode_frame(OP_FRAME, cmd.as_bytes()))?;
        self.conn.flush()?;
        loop {
            let (op, v) = read_frame(&mut self.conn)?;
            match op {
                OP_FRAME if v["nonce"] == nonce.as_str() => {
                    if v["evt"] == "ERROR" {
                        return Err(ActivityError::Rejected(format!(
                            "{} (code {})",
                            v["data"]["message"].as_str().unwrap_or("unknown error"),
                            v["data"]["code"]
                        )));
                    }
                    return Ok(());
                }
                OP_CLOSE => {
                    return Err(ActivityError::Io(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "Discord closed the connection",
                    )));
                }
                OP_PING => {
                    let body = serde_json::to_vec(&v).unwrap_or_default();
                    self.conn.write_all(&encode_frame(OP_PONG, &body))?;
                }
                _ => {} // dispatches for other nonces/events
            }
        }
    }
}

/// Default connector: the platform's Discord IPC endpoints, 0..=9.
pub fn system_connector() -> Connector {
    Box::new(|| {
        let mut last = io::Error::new(io::ErrorKind::NotFound, "Discord is not running");
        for path in ipc_paths() {
            match open_ipc(&path) {
                Ok(c) => return Ok(c),
                Err(e) => last = e,
            }
        }
        Err(last)
    })
}

#[cfg(windows)]
fn ipc_paths() -> Vec<std::path::PathBuf> {
    (0..10)
        .map(|n| std::path::PathBuf::from(format!(r"\\?\pipe\discord-ipc-{n}")))
        .collect()
}

#[cfg(windows)]
fn open_ipc(path: &std::path::Path) -> io::Result<Box<dyn Transport>> {
    // A named pipe opens like a file; reads and writes are synchronous.
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    Ok(Box::new(f))
}

#[cfg(unix)]
fn ipc_paths() -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .collect();
    dirs.push("/tmp".into());
    let mut out = Vec::new();
    for d in dirs {
        for n in 0..10 {
            out.push(d.join(format!("discord-ipc-{n}")));
        }
    }
    out
}

#[cfg(unix)]
fn open_ipc(path: &std::path::Path) -> io::Result<Box<dyn Transport>> {
    Ok(Box::new(std::os::unix::net::UnixStream::connect(path)?))
}

#[cfg(not(any(windows, unix)))]
fn ipc_paths() -> Vec<std::path::PathBuf> {
    Vec::new()
}

#[cfg(not(any(windows, unix)))]
fn open_ipc(_: &std::path::Path) -> io::Result<Box<dyn Transport>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no Discord IPC on this platform",
    ))
}

// ---- background service -------------------------------------------------------

/// What the settings page shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Disabled,
    NotConfigured,
    Connecting,
    /// Discord is not running (or not reachable); retrying periodically.
    Waiting(String),
    Connected,
    Error(String),
}

/// Desired state, sent from the UI thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Desired {
    /// `None` = Rich Presence off (disconnects and clears).
    pub client_id: Option<String>,
    pub presence: Option<Presence>,
}

enum Cmd {
    Set(Desired),
    Shutdown,
}

/// One background thread that keeps Discord in sync with [`Desired`].
pub struct PresenceService {
    tx: Sender<Cmd>,
    done: Receiver<()>,
    status: Arc<Mutex<Status>>,
    last: Option<Desired>,
}

impl PresenceService {
    pub fn start(connector: Connector) -> PresenceService {
        let (tx, rx) = channel();
        let (done_tx, done) = channel();
        let status = Arc::new(Mutex::new(Status::Disabled));
        let st = status.clone();
        let spawned = std::thread::Builder::new()
            .name("Kestrel-Discord".into())
            .spawn(move || {
                run(rx, connector, st);
                let _ = done_tx.send(());
            });
        if let Err(e) = spawned {
            log::warn!("Discord Rich Presence unavailable: cannot start thread: {e}");
            if let Ok(mut s) = status.lock() {
                *s = Status::Error(format!("cannot start: {e}"));
            }
        }
        PresenceService {
            tx,
            done,
            status,
            last: None,
        }
    }

    /// Update what Discord should show. Cheap; identical updates are dropped.
    pub fn set(&mut self, desired: Desired) {
        if self.last.as_ref() == Some(&desired) {
            return;
        }
        self.last = Some(desired.clone());
        let _ = self.tx.send(Cmd::Set(desired));
    }

    pub fn status(&self) -> Status {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or(Status::Disabled)
    }

    /// Clear the activity and stop the thread, waiting at most `timeout`
    /// (a hung Discord must never keep the launcher from exiting).
    pub fn shutdown(&mut self, timeout: Duration) {
        if self.tx.send(Cmd::Shutdown).is_ok() {
            let _ = self.done.recv_timeout(timeout);
        }
    }
}

impl Drop for PresenceService {
    fn drop(&mut self) {
        self.shutdown(Duration::from_millis(500));
    }
}

fn set_status(status: &Mutex<Status>, s: Status) {
    if let Ok(mut g) = status.lock()
        && *g != s
    {
        *g = s;
    }
}

fn run(rx: Receiver<Cmd>, connector: Connector, status: Arc<Mutex<Status>>) {
    let mut desired = Desired {
        client_id: None,
        presence: None,
    };
    let mut client: Option<(Client, String)> = None; // connection + its client id
    let mut sent: Option<Option<Presence>> = None; // what Discord currently shows
    let mut next_connect = Instant::now();
    let mut backoff = RETRY_MIN;
    let mut last_send: Option<Instant> = None;

    loop {
        // Work out how long we may sleep before something needs doing.
        let now = Instant::now();
        let mut wake: Option<Instant> = None;
        if desired.client_id.is_some() && client.is_none() {
            wake = Some(next_connect);
        } else if client.is_some() && sent.as_ref() != Some(&desired.presence) {
            wake = Some(last_send.map_or(now, |t| t + MIN_UPDATE_INTERVAL));
        }
        let cmd = match wake {
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(t) => rx.recv_timeout(t.saturating_duration_since(now)),
        };
        match cmd {
            Ok(Cmd::Set(d)) => {
                if d.client_id != desired.client_id {
                    if let Some((mut c, _)) = client.take() {
                        let _ = c.set_activity(None);
                    }
                    sent = None;
                    next_connect = Instant::now();
                    backoff = RETRY_MIN;
                }
                desired = d;
                if desired.client_id.is_none() {
                    set_status(&status, Status::Disabled);
                }
                continue; // drain further commands before acting
            }
            Ok(Cmd::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                if let Some((mut c, _)) = client.take() {
                    let _ = c.set_activity(None);
                }
                return;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        let Some(id) = desired.client_id.clone() else {
            continue;
        };
        if client.is_none() {
            if Instant::now() < next_connect {
                continue;
            }
            set_status(&status, Status::Connecting);
            match connector().and_then(|t| Client::handshake(t, &id)) {
                Ok(c) => {
                    log::info!("Discord Rich Presence connected");
                    client = Some((c, id));
                    sent = None;
                    backoff = RETRY_MIN;
                    set_status(&status, Status::Connected);
                }
                Err(e) => {
                    let refused = e.kind() == io::ErrorKind::ConnectionRefused;
                    log::debug!("Discord IPC: {e}");
                    set_status(
                        &status,
                        if refused {
                            Status::Error(e.to_string())
                        } else {
                            Status::Waiting("Discord is not running".into())
                        },
                    );
                    next_connect = Instant::now() + backoff;
                    backoff = (backoff * 2).min(RETRY_MAX);
                    continue;
                }
            }
        }
        if sent.as_ref() == Some(&desired.presence) {
            continue;
        }
        if let Some(t) = last_send
            && t.elapsed() < MIN_UPDATE_INTERVAL
        {
            continue;
        }
        if let Some((c, _)) = client.as_mut() {
            last_send = Some(Instant::now());
            match c.set_activity(desired.presence.as_ref()) {
                Ok(()) => {
                    sent = Some(desired.presence.clone());
                    set_status(&status, Status::Connected);
                }
                Err(e @ ActivityError::Rejected(_)) => {
                    // Keep the connection; do not resend the same activity in a loop.
                    log::warn!("{e}");
                    sent = Some(desired.presence.clone());
                    set_status(&status, Status::Error(e.to_string()));
                }
                Err(ActivityError::Io(e)) => {
                    log::info!("Discord connection lost: {e}");
                    client = None;
                    sent = None;
                    next_connect = Instant::now() + RETRY_MIN;
                    set_status(&status, Status::Waiting("Discord connection lost".into()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let f = encode_frame(OP_FRAME, br#"{"a":1}"#);
        assert_eq!(&f[..4], &1u32.to_le_bytes());
        assert_eq!(&f[4..8], &7u32.to_le_bytes());
        let (op, v) = read_frame(&mut &f[..]).unwrap();
        assert_eq!(op, OP_FRAME);
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn oversized_and_truncated_frames_fail() {
        let mut big = encode_frame(OP_FRAME, b"{}");
        big[4..8].copy_from_slice(&(MAX_FRAME + 1).to_le_bytes());
        assert!(read_frame(&mut &big[..]).is_err());
        let f = encode_frame(OP_FRAME, br#"{"a":1}"#);
        assert!(read_frame(&mut &f[..10]).is_err());
        let junk = encode_frame(OP_FRAME, b"not json");
        assert!(read_frame(&mut &junk[..]).is_err());
    }

    #[test]
    fn client_id_validation() {
        assert!(validate_client_id("").is_err());
        assert!(validate_client_id("abc").is_err());
        assert!(validate_client_id("1234").is_err());
        assert!(validate_client_id("123456789012345678901").is_err());
        assert_eq!(
            validate_client_id(" 123456789012345678 ").unwrap(),
            "123456789012345678"
        );
    }

    #[test]
    fn activity_payload_shape() {
        let p = Presence {
            details: "In the main menu".into(),
            state: Some("Minecraft 1.8.9".into()),
            start_unix: Some(1_700_000_000),
            large_image: Some("kestrel".into()),
            large_text: Some("Kestrel 0.4.0".into()),
        };
        let v = activity_command(Some(&p), 42, "n1");
        assert_eq!(v["cmd"], "SET_ACTIVITY");
        assert_eq!(v["nonce"], "n1");
        assert_eq!(v["args"]["pid"], 42);
        let a = &v["args"]["activity"];
        assert_eq!(a["details"], "In the main menu");
        assert_eq!(a["state"], "Minecraft 1.8.9");
        assert_eq!(a["timestamps"]["start"], 1_700_000_000u64);
        assert_eq!(a["assets"]["large_image"], "kestrel");
        assert_eq!(a["type"], 0);
        let cleared = activity_command(None, 42, "n2");
        assert!(cleared["args"]["activity"].is_null());
    }

    #[cfg(unix)]
    mod fake_discord {
        use super::super::*;
        use std::os::unix::net::{UnixListener, UnixStream};
        use std::path::PathBuf;

        /// A minimal Discord IPC server: READY on handshake, acknowledges
        /// SET_ACTIVITY, and records every activity it receives.
        pub fn serve(
            path: PathBuf,
            reject_client: bool,
        ) -> (std::thread::JoinHandle<()>, Receiver<Option<Value>>) {
            let listener = UnixListener::bind(&path).unwrap();
            let (tx, rx) = channel();
            let h = std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut s) = stream else { break };
                    let tx = tx.clone();
                    let (op, hello) = match read_frame(&mut s) {
                        Ok(x) => x,
                        Err(_) => continue,
                    };
                    assert_eq!(op, OP_HANDSHAKE);
                    assert_eq!(hello["v"], 1);
                    if reject_client {
                        let close =
                            json!({"code": 4000, "message": "Invalid Client ID"}).to_string();
                        let _ = s.write_all(&encode_frame(OP_CLOSE, close.as_bytes()));
                        continue;
                    }
                    let ready =
                        json!({"cmd": "DISPATCH", "evt": "READY", "data": {"v": 1}}).to_string();
                    s.write_all(&encode_frame(OP_FRAME, ready.as_bytes()))
                        .unwrap();
                    while let Ok((op, cmd)) = read_frame(&mut s) {
                        if op != OP_FRAME {
                            break;
                        }
                        let activity = cmd["args"]["activity"].clone();
                        let reply = json!({"cmd": "SET_ACTIVITY", "evt": null, "nonce": cmd["nonce"], "data": activity}).to_string();
                        if s.write_all(&encode_frame(OP_FRAME, reply.as_bytes()))
                            .is_err()
                        {
                            break;
                        }
                        let _ = tx.send(if activity.is_null() {
                            None
                        } else {
                            Some(activity)
                        });
                    }
                }
            });
            (h, rx)
        }

        fn connector_for(path: PathBuf) -> Connector {
            Box::new(move || Ok(Box::new(UnixStream::connect(&path)?) as Box<dyn Transport>))
        }

        fn presence(details: &str) -> Presence {
            Presence {
                details: details.into(),
                state: Some("Minecraft 1.8.9".into()),
                start_unix: Some(1),
                large_image: Some("kestrel".into()),
                large_text: None,
            }
        }

        const ID: &str = "123456789012345678";

        #[test]
        fn end_to_end_updates_and_clears_on_shutdown() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("discord-ipc-0");
            let (_h, seen) = serve(path.clone(), false);
            let mut svc = PresenceService::start(connector_for(path));
            svc.set(Desired {
                client_id: Some(ID.into()),
                presence: Some(presence("In the main menu")),
            });
            let first = seen.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
            assert_eq!(first["details"], "In the main menu");
            assert_eq!(svc.status(), Status::Connected);

            // Rapid changes are coalesced; the latest one wins.
            svc.set(Desired {
                client_id: Some(ID.into()),
                presence: Some(presence("Playing singleplayer")),
            });
            svc.set(Desired {
                client_id: Some(ID.into()),
                presence: Some(presence("Playing multiplayer")),
            });
            let next = seen.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
            assert_eq!(next["details"], "Playing multiplayer");

            svc.shutdown(Duration::from_secs(5));
            assert_eq!(
                seen.recv_timeout(Duration::from_secs(5)).unwrap(),
                None,
                "cleared on exit"
            );
        }

        #[test]
        fn disabling_clears_and_disconnects() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("discord-ipc-0");
            let (_h, seen) = serve(path.clone(), false);
            let mut svc = PresenceService::start(connector_for(path));
            svc.set(Desired {
                client_id: Some(ID.into()),
                presence: Some(presence("In the launcher")),
            });
            assert!(seen.recv_timeout(Duration::from_secs(5)).unwrap().is_some());
            svc.set(Desired {
                client_id: None,
                presence: Some(presence("In the launcher")),
            });
            assert_eq!(seen.recv_timeout(Duration::from_secs(5)).unwrap(), None);
            std::thread::sleep(Duration::from_millis(100));
            assert_eq!(svc.status(), Status::Disabled);
        }

        #[test]
        fn discord_not_running_is_reported_and_retried() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("discord-ipc-0");
            let mut svc = PresenceService::start(connector_for(path.clone()));
            svc.set(Desired {
                client_id: Some(ID.into()),
                presence: Some(presence("In the launcher")),
            });
            let deadline = Instant::now() + Duration::from_secs(5);
            while !matches!(svc.status(), Status::Waiting(_)) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(
                matches!(svc.status(), Status::Waiting(_)),
                "{:?}",
                svc.status()
            );
            // Discord starts: the next retry (5 s backoff) connects and sends.
            let (_h, seen) = serve(path, false);
            let got = seen.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
            assert_eq!(got["details"], "In the launcher");
            assert_eq!(svc.status(), Status::Connected);
        }

        #[test]
        fn invalid_client_id_is_an_error_not_a_crash() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("discord-ipc-0");
            let (_h, _seen) = serve(path.clone(), true);
            let mut svc = PresenceService::start(connector_for(path));
            svc.set(Desired {
                client_id: Some(ID.into()),
                presence: Some(presence("x")),
            });
            let deadline = Instant::now() + Duration::from_secs(5);
            while !matches!(svc.status(), Status::Error(_)) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            match svc.status() {
                Status::Error(m) => assert!(m.contains("Invalid Client ID"), "{m}"),
                s => panic!("unexpected status {s:?}"),
            }
        }

        #[test]
        fn shutdown_never_hangs_without_discord() {
            let mut svc = PresenceService::start(Box::new(|| {
                Err(io::Error::new(io::ErrorKind::NotFound, "nope"))
            }));
            svc.set(Desired {
                client_id: Some(ID.into()),
                presence: None,
            });
            let t = Instant::now();
            svc.shutdown(Duration::from_secs(2));
            assert!(t.elapsed() < Duration::from_secs(2));
        }
    }
}
