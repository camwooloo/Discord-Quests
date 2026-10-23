// Streaming badge helpers. Only the Discord client itself can Go Live, so
// Aurora can't stream for the user; instead it
//   * follows the user's own voice state over a read-only gateway session, so
//     the farm page can say which voice channel they're in and whether they're
//     live (and count streamed time), and
//   * runs a separate "Aurora Quests stream" window (`--stream-window`) for
//     them to pick in Go Live. It's its own process, so it keeps running while
//     the farm switches games, and it shows whatever the farm is playing.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::farm::{self, Game};
use crate::platform;

/// Title of the stream window, as the user will see it in Discord's Go Live picker.
pub const WINDOW_TITLE: &str = "Aurora Quests stream";

// ---------------------------------------------------------------------------
// Voice state
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct Guild {
    name: String,
    icon: Option<String>,
    /// Voice + stage channels: id -> (position, name).
    channels: BTreeMap<String, (i64, String)>,
}

#[derive(Default)]
struct Voice {
    ready: bool,
    me: String,
    guilds: BTreeMap<String, Guild>,
    guild_id: Option<String>,
    channel_id: Option<String>,
    streaming_since: Option<Instant>,
}

/// Streamed-time totals (`stream.json`). Kept apart from farm.json because the
/// farm loop holds and rewrites its own copy of that.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Stats {
    total_secs: u64,
}

fn stats_path() -> PathBuf {
    platform::config_dir().join("stream.json")
}

fn load_stats() -> Stats {
    std::fs::read_to_string(stats_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_stats(s: &Stats) {
    let _ = std::fs::create_dir_all(platform::config_dir());
    if let Ok(j) = serde_json::to_string(s) {
        let _ = std::fs::write(stats_path(), j);
    }
}

type Notify = Box<dyn Fn(Value) + Send + Sync>;

static VOICE: OnceLock<Arc<Mutex<Voice>>> = OnceLock::new();
static NOTIFY: OnceLock<Notify> = OnceLock::new();
static WINDOW: Mutex<Option<Child>> = Mutex::new(None);

fn voice() -> &'static Arc<Mutex<Voice>> {
    VOICE.get_or_init(Default::default)
}

fn notify() {
    if let Some(n) = NOTIFY.get() {
        n(snapshot());
    }
}

/// Start following the user's voice state (once); `on_change` gets a fresh
/// `snapshot()` whenever it changes.
pub fn watch(token: impl FnOnce() -> String, on_change: impl Fn(Value) + Send + Sync + 'static) {
    if NOTIFY.set(Box::new(on_change)).is_err() {
        return;
    }
    let token = token();
    if token.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        crate::gateway::listen(&token, |t, d| {
            if handle(t, d) {
                notify();
            }
        })
    });
    // Bank streamed time every minute so a crash loses at most that much.
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(60));
        let mut v = voice().lock().unwrap();
        if v.streaming_since.is_some() {
            bank(&mut v, true);
        }
    });
}

/// Add the current stream's time to the total, restarting the clock if still live.
fn bank(v: &mut Voice, still_live: bool) {
    if let Some(since) = v.streaming_since.take() {
        let mut s = load_stats();
        s.total_secs += since.elapsed().as_secs();
        save_stats(&s);
        if still_live {
            v.streaming_since = Some(Instant::now());
        }
    }
}

fn add_guild(v: &mut Voice, g: &Value) {
    let Some(id) = g["id"].as_str() else { return };
    let prop = |k: &str| g[k].as_str().or(g["properties"][k].as_str()).map(str::to_string);
    let mut guild = Guild {
        name: prop("name").unwrap_or_default(),
        icon: prop("icon"),
        channels: BTreeMap::new(),
    };
    for c in g["channels"].as_array().into_iter().flatten() {
        add_channel(&mut guild, c);
    }
    v.guilds.insert(id.to_string(), guild);
    for vs in g["voice_states"].as_array().into_iter().flatten() {
        own_state(v, Some(id), vs);
    }
}

fn add_channel(g: &mut Guild, c: &Value) {
    // 2 = voice, 13 = stage.
    if matches!(c["type"].as_u64(), Some(2) | Some(13)) {
        if let Some(id) = c["id"].as_str() {
            let name = c["name"].as_str().unwrap_or("voice").to_string();
            g.channels.insert(id.to_string(), (c["position"].as_i64().unwrap_or(0), name));
        }
    }
}

/// Apply a voice state if it's the user's own.
fn own_state(v: &mut Voice, guild: Option<&str>, vs: &Value) {
    if v.me.is_empty() || vs["user_id"].as_str() != Some(v.me.as_str()) {
        return;
    }
    let channel = vs["channel_id"].as_str().map(str::to_string);
    let live = channel.is_some() && vs["self_stream"].as_bool().unwrap_or(false);
    v.guild_id = channel.as_ref().and(vs["guild_id"].as_str().or(guild)).map(str::to_string);
    v.channel_id = channel;
    if live && v.streaming_since.is_none() {
        v.streaming_since = Some(Instant::now());
    } else if !live {
        bank(v, false);
    }
}

/// Fold one gateway dispatch into the voice state; true if anything we show changed.
fn handle(t: &str, d: &Value) -> bool {
    let mut v = voice().lock().unwrap();
    match t {
        "READY" => {
            v.me = d["user"]["id"].as_str().unwrap_or("").to_string();
            v.guilds.clear();
            v.guild_id = None;
            v.channel_id = None;
            for g in d["guilds"].as_array().into_iter().flatten() {
                add_guild(&mut v, g);
            }
            v.ready = true;
        }
        "READY_SUPPLEMENTAL" => {
            for g in d["guilds"].as_array().into_iter().flatten() {
                for vs in g["voice_states"].as_array().into_iter().flatten() {
                    own_state(&mut v, g["id"].as_str(), vs);
                }
            }
        }
        "GUILD_CREATE" => add_guild(&mut v, d),
        "GUILD_DELETE" => {
            if let Some(id) = d["id"].as_str() {
                v.guilds.remove(id);
            }
        }
        "CHANNEL_CREATE" | "CHANNEL_UPDATE" | "CHANNEL_DELETE" => {
            let (Some(g), Some(id)) = (d["guild_id"].as_str(), d["id"].as_str()) else {
                return false;
            };
            let Some(guild) = v.guilds.get_mut(g) else { return false };
            guild.channels.remove(id);
            if t != "CHANNEL_DELETE" {
                add_channel(guild, d);
            }
        }
        "VOICE_STATE_UPDATE" => own_state(&mut v, None, d),
        _ => return false,
    }
    true
}

/// Everything the farm page's streaming area shows.
pub fn snapshot() -> Value {
    let open = window_open();
    let v = voice().lock().unwrap();
    let streamed = load_stats().total_secs
        + v.streaming_since.map(|s| s.elapsed().as_secs()).unwrap_or(0);
    let guild = v.guild_id.as_ref().and_then(|g| v.guilds.get(g));
    let channel_name = match (&v.channel_id, guild) {
        (Some(c), Some(g)) => g.channels.get(c).map(|(_, n)| n.clone()),
        _ => None,
    };
    let mut guilds: Vec<Value> = v
        .guilds
        .iter()
        .filter(|(_, g)| !g.channels.is_empty())
        .map(|(id, g)| {
            let mut ch: Vec<_> = g.channels.iter().collect();
            ch.sort_by_key(|(_, (pos, _))| *pos);
            json!({
                "id": id,
                "name": g.name,
                "icon": g.icon,
                "channels": ch.iter().map(|(id, (_, n))| json!({ "id": id, "name": n })).collect::<Vec<_>>(),
            })
        })
        .collect();
    guilds.sort_by_key(|g| g["name"].as_str().unwrap_or("").to_lowercase());
    json!({
        "ready": v.ready,
        "inVoice": v.channel_id.is_some(),
        "guild": v.guild_id,
        "guildName": guild.map(|g| g.name.clone()),
        "channel": v.channel_id,
        "channelName": channel_name,
        "streaming": v.streaming_since.is_some(),
        "streamSecs": streamed,
        "window": open,
        "guilds": guilds,
    })
}

/// Wipe the streamed-time total (and restart the clock if live right now).
pub fn reset_totals() {
    save_stats(&Stats::default());
    let mut v = voice().lock().unwrap();
    if v.streaming_since.is_some() {
        v.streaming_since = Some(Instant::now());
    }
}

// ---------------------------------------------------------------------------
// The stream window
// ---------------------------------------------------------------------------

/// Is the stream window still running? (Forgets it once the user closed it.)
pub fn window_open() -> bool {
    let mut w = WINDOW.lock().unwrap();
    if let Some(c) = w.as_mut() {
        if matches!(c.try_wait(), Ok(None)) {
            return true;
        }
    }
    *w = None;
    false
}

pub fn open_window() -> Result<(), String> {
    if window_open() {
        return Ok(());
    }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let child = Command::new(me)
        .arg("--stream-window")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    *WINDOW.lock().unwrap() = Some(child);
    // Tell the page when the user closes the window themselves.
    std::thread::spawn(|| {
        while window_open() {
            std::thread::sleep(Duration::from_secs(2));
        }
        notify();
    });
    Ok(())
}

pub fn close_window() {
    if let Some(mut c) = WINDOW.lock().unwrap().take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

fn now_path() -> PathBuf {
    platform::config_dir().join("stream-now.json")
}

/// Record what the farm is playing (None = nothing), for the stream window.
pub fn set_now(game: Option<&Game>) {
    let v = match game {
        Some(g) => json!({ "name": g.name, "image": farm::art_url(g) }),
        None => json!({ "name": null, "image": "" }),
    };
    let _ = std::fs::create_dir_all(platform::config_dir());
    let _ = std::fs::write(now_path(), v.to_string());
}

/// Entry point for `--stream-window`: a 720p window for Go Live that follows
/// the farm's current game.
pub fn run_window() {
    use tao::dpi::LogicalSize;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoopBuilder};
    use tao::window::{Icon, WindowBuilder};

    farm::exit_with_parent();
    let event_loop = EventLoopBuilder::<String>::with_user_event().build();
    let icon = crate::assets::logo_rgba().and_then(|(rgba, w, h)| Icon::from_rgba(rgba, w, h).ok());
    let Ok(window) = WindowBuilder::new()
        .with_title(WINDOW_TITLE)
        .with_window_icon(icon)
        .with_inner_size(LogicalSize::new(1280.0, 720.0))
        .with_min_inner_size(LogicalSize::new(640.0, 360.0))
        .build(&event_loop)
    else {
        return;
    };
    let Ok(webview) = wry::WebViewBuilder::new().with_html(window_html()).build(&window) else {
        return;
    };
    let proxy = event_loop.create_proxy();
    std::thread::spawn(move || {
        let mut last = String::new();
        loop {
            let now = std::fs::read_to_string(now_path()).unwrap_or_default();
            if now != last && serde_json::from_str::<Value>(&now).is_ok() {
                if proxy.send_event(format!("setNow({now})")).is_err() {
                    return;
                }
                last = now;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(js) => {
                let _ = webview.evaluate_script(&js);
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

/// The stream scene: the farmed game's art drifting behind Aurora branding,
/// with a running clock so every frame changes and the stream never looks frozen.
fn window_html() -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><style>
html,body{{margin:0;height:100%;overflow:hidden;font-family:"Segoe UI",system-ui,sans-serif;color:#fff;background:#07081a;user-select:none}}
.art{{position:absolute;inset:-6%;background:#141633 center/cover no-repeat;opacity:0;transition:opacity 1.2s;animation:drift 40s ease-in-out infinite alternate}}
.art.on{{opacity:1}}
@keyframes drift{{from{{transform:scale(1.02) translate(-1.5%,-1%)}}to{{transform:scale(1.1) translate(1.5%,1%)}}}}
.veil{{position:absolute;inset:0;background:radial-gradient(120% 90% at 20% 10%,rgba(180,140,255,.28),transparent 60%),
  radial-gradient(90% 80% at 90% 100%,rgba(52,211,153,.22),transparent 60%),linear-gradient(180deg,rgba(7,8,26,.25),rgba(7,8,26,.9) 78%)}}
.top{{position:absolute;top:4.5vh;left:4vw;right:4vw;display:flex;align-items:center;gap:1.6vw}}
.logo{{width:7vh;height:7vh;border-radius:1.8vh}}
.brand{{font-size:4vh;font-weight:800;letter-spacing:.02em}}
.pill{{font-size:2.1vh;font-weight:800;letter-spacing:.12em;text-transform:uppercase;padding:1vh 1.6vh;border-radius:1.4vh;
  background:linear-gradient(135deg,#b48cff,#34d399);color:#0b0616}}
.clock{{margin-left:auto;font-size:3vh;font-weight:700;font-variant-numeric:tabular-nums;padding:1vh 1.8vh;border-radius:1.4vh;background:rgba(7,8,26,.6)}}
.clock i{{display:inline-block;width:1.4vh;height:1.4vh;border-radius:50%;background:#f43f5e;margin-right:1.2vh;animation:p 1.4s infinite}}
@keyframes p{{50%{{opacity:.25}}}}
.foot{{position:absolute;left:4vw;right:4vw;bottom:6vh}}
.k{{font-size:2.4vh;font-weight:800;letter-spacing:.14em;text-transform:uppercase;opacity:.7}}
.g{{font-size:9vh;font-weight:800;line-height:1.05;margin-top:1vh;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}}
.s{{font-size:2.4vh;opacity:.7;margin-top:1.6vh}}
</style></head><body><div class="art" id="a1"></div><div class="art" id="a2"></div><div class="veil"></div>
<div class="top"><img class="logo" src="{logo}"><span class="brand">Aurora Quests</span><span class="pill">Streaming badge</span>
<span class="clock"><i></i><span id="clock">00:00:00</span></span></div>
<div class="foot"><div class="k" id="k">Live with Aurora Quests</div><div class="g" id="g">Aurora Quests</div>
<div class="s">Streamed with Aurora Quests. Keep this window open; it can sit behind your game.</div></div>
<script>
const t0=Date.now(),pad=n=>String(n).padStart(2,'0');
setInterval(()=>{{const s=Math.floor((Date.now()-t0)/1000);document.getElementById('clock').textContent=pad(Math.floor(s/3600))+':'+pad(Math.floor(s/60)%60)+':'+pad(s%60);}},250);
let cur=1,img='';
function setNow(n){{
  document.getElementById('g').textContent=n&&n.name?n.name:'Aurora Quests';
  document.getElementById('k').textContent=n&&n.name?'Now farming':'Live with Aurora Quests';
  const u=n&&n.image&&n.image.startsWith('https://cdn.discordapp.com/')?n.image:'';
  if(u===img)return; img=u;
  const next=document.getElementById(cur===1?'a2':'a1'),prev=document.getElementById(cur===1?'a1':'a2');
  next.style.backgroundImage=u?'url("'+u+'")':'none'; next.classList.add('on'); prev.classList.remove('on'); cur=cur===1?2:1;
}}
</script></body></html>"#,
        logo = crate::assets::logo_uri(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_own_voice_state() {
        let ready = json!({
            "user": { "id": "1" },
            "guilds": [{
                "id": "g", "properties": { "name": "Home", "icon": null },
                "channels": [
                    { "id": "t", "type": 0, "name": "chat" },
                    { "id": "v2", "type": 2, "name": "Lounge", "position": 2 },
                    { "id": "v1", "type": 2, "name": "General", "position": 1 }
                ],
                "voice_states": [{ "user_id": "2", "channel_id": "v1" }]
            }]
        });
        assert!(handle("READY", &ready));
        let s = snapshot();
        assert_eq!(s["inVoice"], false);
        let ch = &s["guilds"][0]["channels"];
        assert_eq!(ch.as_array().unwrap().len(), 2);
        assert_eq!(ch[0]["name"], "General");

        handle("VOICE_STATE_UPDATE", &json!({ "user_id": "1", "guild_id": "g", "channel_id": "v2", "self_stream": true }));
        let s = snapshot();
        assert_eq!(s["channelName"], "Lounge");
        assert_eq!(s["guildName"], "Home");
        assert_eq!(s["streaming"], true);

        // Someone else leaving doesn't move us; us leaving does.
        handle("VOICE_STATE_UPDATE", &json!({ "user_id": "2", "guild_id": "g", "channel_id": null }));
        assert_eq!(snapshot()["inVoice"], true);
        handle("VOICE_STATE_UPDATE", &json!({ "user_id": "1", "guild_id": "g", "channel_id": null }));
        let s = snapshot();
        assert_eq!(s["inVoice"], false);
        assert_eq!(s["streaming"], false);
    }
}
