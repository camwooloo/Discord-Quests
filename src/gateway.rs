// Discord Gateway presence: a minimal self-token WebSocket client that keeps a
// connection alive and broadcasts a "Playing <game>" Rich Presence while a game
// quest is being mimicked, so it shows on the user's profile.
//
// Single background thread with a short read timeout, so it can read gateway
// events, send heartbeats on schedule, and apply presence commands without
// blocking. Reconnects on drop and re-applies the desired activity.

use std::net::TcpStream;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

const GATEWAY: &str = "wss://gateway.discord.gg/?v=10&encoding=json";

enum Cmd {
    Set(Value),
    Clear,
}

/// Handle used by the app to drive presence.
pub struct Presence {
    tx: Sender<Cmd>,
}

impl Presence {
    pub fn new(token: String) -> Presence {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || manager(token, rx));
        Presence { tx }
    }
    /// Broadcast "Playing <game>" for a quest game.
    pub fn set(&self, name: &str, app_id: &str) {
        let _ = self.tx.send(Cmd::Set(json!({
            "name": name, "type": 0, "application_id": app_id
        })));
    }
    /// Broadcast an arbitrary activity (custom Rich Presence editor).
    pub fn set_activity(&self, activity: Value) {
        let _ = self.tx.send(Cmd::Set(activity));
    }
    pub fn clear(&self) {
        let _ = self.tx.send(Cmd::Clear);
    }
}

fn identity_properties() -> Value {
    json!({
        "os": "Windows",
        "browser": "Discord Client",
        "release_channel": "stable",
        "client_version": "1.0.9251",
        "os_version": "10.0.26200",
        "system_locale": "en-US",
        "client_build_number": 9_999_999,
        "native_build_number": Value::Null,
        "client_event_source": Value::Null,
    })
}

fn presence_payload(desired: &Option<Value>) -> Value {
    let activities = match desired {
        Some(act) => json!([act]),
        None => json!([]),
    };
    json!({ "op": 3, "d": { "since": 0, "activities": activities, "status": "online", "afk": false } })
}

fn set_read_timeout(ws: &WebSocket<MaybeTlsStream<TcpStream>>, d: Duration) {
    match ws.get_ref() {
        MaybeTlsStream::Plain(s) => {
            let _ = s.set_read_timeout(Some(d));
        }
        MaybeTlsStream::Rustls(s) => {
            let _ = s.get_ref().set_read_timeout(Some(d));
        }
        _ => {}
    }
}

/// Owns the current desired activity across reconnects and drains commands.
fn manager(token: String, rx: Receiver<Cmd>) {
    // Stay disconnected until the app first asks to show a game — no idle session.
    let mut desired: Option<Value> = match rx.recv() {
        Ok(cmd) => apply(cmd),
        Err(_) => return,
    };
    loop {
        while let Ok(cmd) = rx.try_recv() {
            desired = apply(cmd);
        }
        match run_session(&token, &rx, &mut desired) {
            SessionEnd::Shutdown => return,
            SessionEnd::Reconnect => {
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    }
}

fn apply(cmd: Cmd) -> Option<Value> {
    match cmd {
        Cmd::Set(v) => Some(v),
        Cmd::Clear => None,
    }
}

enum SessionEnd {
    Reconnect,
    #[allow(dead_code)]
    Shutdown,
}

fn run_session(
    token: &str,
    rx: &Receiver<Cmd>,
    desired: &mut Option<Value>,
) -> SessionEnd {
    let (mut ws, _resp) = match tungstenite::connect(GATEWAY) {
        Ok(v) => v,
        Err(_) => return SessionEnd::Reconnect,
    };
    set_read_timeout(&ws, Duration::from_millis(250));

    let mut hb_interval = Duration::from_millis(41250);
    let mut last_hb = Instant::now();
    let mut last_seq: Value = Value::Null;
    let mut identified = false;
    let mut ready = false;
    let mut sent: Option<Value> = None;
    // Force a presence send once the session is ready.
    let mut dirty = true;

    loop {
        // Apply queued commands.
        let mut got_cmd = false;
        while let Ok(cmd) = rx.try_recv() {
            *desired = apply(cmd);
            got_cmd = true;
        }
        if got_cmd {
            dirty = true;
        }

        // Send presence once the session is READY and the desired activity changed.
        if ready && dirty && &sent != desired {
            if ws
                .send(Message::text(presence_payload(desired).to_string()))
                .is_err()
            {
                return SessionEnd::Reconnect;
            }
            sent = desired.clone();
            dirty = false;
        }

        // Heartbeat on schedule.
        if identified && last_hb.elapsed() >= hb_interval {
            let hb = json!({ "op": 1, "d": last_seq });
            if ws.send(Message::text(hb.to_string())).is_err() {
                return SessionEnd::Reconnect;
            }
            last_hb = Instant::now();
        }

        match ws.read() {
            Ok(Message::Text(txt)) => {
                let v: Value = match serde_json::from_str(&txt) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if let Some(s) = v.get("s") {
                    if !s.is_null() {
                        last_seq = s.clone();
                    }
                }
                if v["t"].as_str() == Some("READY") {
                    ready = true;
                    dirty = true;
                }
                match v["op"].as_u64().unwrap_or(999) {
                    10 => {
                        // Hello: set heartbeat interval, then identify.
                        if let Some(ms) = v["d"]["heartbeat_interval"].as_u64() {
                            hb_interval = Duration::from_millis(ms);
                        }
                        let identify = json!({
                            "op": 2,
                            "d": {
                                "token": token,
                                "capabilities": 161789,
                                "properties": identity_properties(),
                                "presence": { "status": "online", "since": 0, "activities": [], "afk": false },
                                "compress": false,
                                "client_state": { "guild_versions": {} }
                            }
                        });
                        if ws.send(Message::text(identify.to_string())).is_err() {
                            return SessionEnd::Reconnect;
                        }
                        last_hb = Instant::now();
                        identified = true;
                        dirty = true;
                    }
                    9 => {
                        // Invalid session — reconnect fresh.
                        return SessionEnd::Reconnect;
                    }
                    _ => {}
                }
            }
            Ok(Message::Close(_)) => return SessionEnd::Reconnect,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // Idle read timeout — loop again.
            }
            Err(_) => return SessionEnd::Reconnect,
        }
    }
}
