// Badge farming: the Game Time badge (hours played) and the Game Variety badge
// (number of different games played).
//
// Discord's client counts a game as "played" when it sees one of the game's
// known executables running. The list of those executables is Discord's public
// detectable-applications list (`/applications/detectable`), which we cache in
// the config dir. To "play" a game we copy our own binary to a path ending in
// that game's executable name and run it with `--farm-stub`, which shows a
// small card window (Discord ignores windowless processes) until we kill it
// or we exit (it watches its stdin pipe). The local Discord client then
// detects it like the real game. If "Show game on my profile" is on, we also
// broadcast it over the gateway, same as quest play.
//
// One farm at a time. It either cycles through a list of games, N minutes
// each (Game Variety, and every minute also counts toward Game Time), or with
// `minutes == 0` stays on one game until the user switches it.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::gateway::Presence;
use crate::platform;
use crate::quests::CLIENT_UA;

const DETECTABLE_URL: &str = "https://discord.com/api/v9/applications/detectable";
/// Re-download the detectable list after this long (it changes slowly).
const CACHE_MAX_AGE: Duration = Duration::from_secs(3 * 24 * 3600);
/// Upper bound on the detectable list download (it's tens of MB at most).
const MAX_LIST_BYTES: u64 = 128 * 1024 * 1024;

/// Popular games, most popular first. Matched by name against the detectable
/// list, so a game Discord no longer lists is simply skipped.
const POPULAR: &[&str] = &[
    "Fortnite", "VALORANT", "League of Legends", "Minecraft", "Roblox",
    "Counter-Strike 2", "Marvel Rivals", "Apex Legends", "Grand Theft Auto V",
    "Rocket League", "Overwatch 2", "Dota 2", "Tom Clancy's Rainbow Six Siege",
    "Call of Duty", "Call of Duty®", "Genshin Impact", "Destiny 2", "Rust",
    "PUBG: BATTLEGROUNDS", "Dead by Daylight", "Terraria", "Among Us",
    "Stardew Valley", "Team Fortress 2", "Warframe", "World of Warcraft",
    "ELDEN RING", "Lethal Company", "Phasmophobia", "Palworld",
    "Baldur's Gate 3", "HELLDIVERS 2", "HELLDIVERS™ 2", "The Sims 4", "Hearthstone",
    "osu!", "Garry's Mod", "Path of Exile 2", "Path of Exile", "Sea of Thieves",
    "Fall Guys", "Valheim", "Honkai: Star Rail", "Deadlock", "THE FINALS",
    "Escape from Tarkov", "Diablo IV", "Monster Hunter Wilds", "Brawlhalla",
    "Paladins", "SMITE 2", "War Thunder", "World of Tanks", "Black Desert",
    "Lost Ark", "New World", "Final Fantasy XIV Online", "Old School RuneScape",
    "RuneScape", "Satisfactory", "Factorio", "No Man's Sky", "Cyberpunk 2077",
    "Red Dead Redemption 2", "Hogwarts Legacy", "Starfield", "Sons Of The Forest",
    "7 Days to Die", "ARK: Survival Evolved", "DayZ", "Hunt: Showdown 1896",
    "Splitgate", "Halo Infinite", "Battlefield 2042", "EA SPORTS FC 25",
    "Euro Truck Simulator 2", "Geometry Dash", "Cities: Skylines", "Hades II",
    "Hollow Knight", "Celeste", "Don't Starve Together", "Project Zomboid",
];
/// How many popular games the variety mode uses when the user hasn't picked.
const DEFAULT_VARIETY: usize = 25;

#[derive(Clone)]
pub struct Game {
    pub id: String,
    pub name: String,
    /// Relative executable path Discord matches against, for this OS.
    pub exe: String,
    /// Position in POPULAR, if it's one of the popular games.
    pub rank: Option<usize>,
    /// CDN image hashes (`app-icons/<id>/<hash>.png`).
    pub icon: Option<String>,
    pub cover: Option<String>,
}

/// Time farmed on one game, with enough to show it without the full list.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct GameStat {
    pub name: String,
    pub icon: Option<String>,
    pub cover: Option<String>,
    pub secs: u64,
}

/// Persisted farm settings + totals (`farm.json` in the config dir).
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    /// App id of the game "one game" mode runs (empty = first in the list).
    #[serde(alias = "hours_game")]
    pub single_game: String,
    /// The user's own list (app ids, in order).
    #[serde(alias = "variety_custom")]
    pub custom: Vec<String>,
    /// Use `custom` instead of the popular list.
    #[serde(alias = "variety_use_custom")]
    pub use_custom: bool,
    /// Minutes each game runs for; 0 = stay on one game until switched.
    #[serde(alias = "variety_minutes")]
    pub minutes: u64,
    /// All-time seconds farmed.
    pub total_secs: u64,
    /// Distinct app ids the farm has run.
    pub played: Vec<String>,
    /// Take a break after this many minutes of farming (0 = never)…
    pub pause_after_mins: u64,
    /// …for this many minutes, then carry on.
    pub pause_for_mins: u64,
    /// Per-game totals, keyed by app id.
    pub per_game: std::collections::BTreeMap<String, GameStat>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            single_game: String::new(),
            custom: Vec::new(),
            use_custom: false,
            minutes: 15,
            total_secs: 0,
            played: Vec::new(),
            pause_after_mins: 0,
            pause_for_mins: 60,
            per_game: Default::default(),
        }
    }
}

fn config_path() -> PathBuf {
    platform::config_dir().join("farm.json")
}

pub fn load() -> Config {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(c: &Config) {
    let p = config_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(j) = serde_json::to_string_pretty(c) {
        let _ = std::fs::write(&p, j);
    }
}

// ---------------------------------------------------------------------------
// Detectable games list
// ---------------------------------------------------------------------------

fn cache_path() -> PathBuf {
    platform::config_dir().join("detectable.json")
}

fn cache_is_fresh(p: &Path) -> bool {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|age| age < CACHE_MAX_AGE)
        .unwrap_or(false)
}

/// The raw detectable list: fresh cache, else download, else stale cache.
fn detectable_raw() -> Result<String, String> {
    let p = cache_path();
    if cache_is_fresh(&p) {
        if let Ok(s) = std::fs::read_to_string(&p) {
            return Ok(s);
        }
    }
    let fetched = ureq::get(DETECTABLE_URL)
        .set("User-Agent", CLIENT_UA)
        .timeout(Duration::from_secs(30))
        .call()
        .map_err(|e| e.to_string())
        .and_then(|r| {
            // The list is well over ureq's 10 MB `into_string` cap, so read it ourselves.
            use std::io::Read;
            let mut s = String::new();
            r.into_reader()
                .take(MAX_LIST_BYTES)
                .read_to_string(&mut s)
                .map_err(|e| e.to_string())?;
            Ok(s)
        });
    match fetched {
        Ok(s) if s.trim_start().starts_with('[') => {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&p, &s);
            Ok(s)
        }
        Ok(_) => std::fs::read_to_string(&p).map_err(|_| "unexpected response from Discord".into()),
        Err(e) => std::fs::read_to_string(&p).map_err(|_| format!("couldn't download Discord's game list: {e}")),
    }
}

#[cfg(windows)]
const OS_KEYS: &[&str] = &["win32"];
// Linux clients detect native games, and Proton/Wine games by their .exe name.
#[cfg(not(windows))]
const OS_KEYS: &[&str] = &["linux", "win32"];

/// The first non-launcher executable for this OS, as a safe relative path.
/// Prefers the plainest name (fewest folders), which is the game's main exe
/// rather than a regional build like `garenalolth/.../lolex.exe`.
fn pick_exe(app: &Value) -> Option<String> {
    let exes = app["executables"].as_array()?;
    for os in OS_KEYS {
        let best = exes
            .iter()
            .filter(|e| e["os"].as_str() == Some(os) && e["is_launcher"].as_bool() != Some(true))
            .filter_map(|e| e["name"].as_str().and_then(clean_rel_path))
            .min_by_key(|rel| rel.matches('/').count());
        if best.is_some() {
            return best;
        }
    }
    None
}

/// Turn a detectable executable name ("game/bin/game.exe", ">game.exe") into a
/// relative path we can create: no leading markers, no `..`, no odd characters.
fn clean_rel_path(name: &str) -> Option<String> {
    let name = name.trim().trim_start_matches('>');
    let parts: Vec<String> = name
        .split(['/', '\\'])
        .map(|seg| {
            seg.chars()
                .filter(|c| !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control())
                .collect::<String>()
                .trim()
                .to_string()
        })
        .filter(|seg| !seg.is_empty() && seg != "." && seg != "..")
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Every detectable game we can run on this OS, popular ones first.
pub fn games() -> Result<Vec<Game>, String> {
    let raw = detectable_raw()?;
    let list: Vec<Value> = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let ranks: std::collections::HashMap<String, usize> = POPULAR
        .iter()
        .enumerate()
        .map(|(i, n)| (norm(n), i))
        .collect();
    let mut seen_rank = std::collections::HashSet::new();
    let mut out: Vec<Game> = list
        .iter()
        .filter_map(|app| {
            let id = app["id"].as_str()?.to_string();
            let name = app["name"].as_str()?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let exe = pick_exe(app)?;
            let rank = std::iter::once(name.as_str())
                .chain(app["aliases"].as_array().into_iter().flatten().filter_map(Value::as_str))
                .filter_map(|n| ranks.get(&norm(n)).copied())
                .min();
            let hash = |k: &str| app[k].as_str().filter(|h| !h.is_empty()).map(str::to_string);
            Some(Game { id, name, exe, rank, icon: hash("icon_hash"), cover: hash("cover_image_hash") })
        })
        .collect();
    let by_rank = |a: &Game, b: &Game| {
        a.rank
            .unwrap_or(usize::MAX)
            .cmp(&b.rank.unwrap_or(usize::MAX))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    };
    // Several apps can share a popular name (old/regional builds); keep the first.
    out.sort_by(by_rank);
    for g in out.iter_mut() {
        if let Some(r) = g.rank {
            if !seen_rank.insert(r) {
                g.rank = None;
            }
        }
    }
    out.sort_by(by_rank);
    Ok(out)
}

/// Compact list for the UI: `[{id,name,pop}]` (`pop` = popular rank or null).
pub fn games_json(games: &[Game]) -> String {
    let arr: Vec<Value> = games
        .iter()
        .map(|g| json!({ "id": g.id, "name": g.name, "pop": g.rank, "icon": g.icon, "cover": g.cover }))
        .collect();
    Value::Array(arr).to_string()
}

/// The games a farm run uses: the chosen list, or just one game when
/// `minutes == 0`.
pub fn plan(cfg: &Config, all: &[Game]) -> Vec<Game> {
    let list: Vec<Game> = if cfg.use_custom && !cfg.custom.is_empty() {
        cfg.custom
            .iter()
            .filter_map(|id| all.iter().find(|g| &g.id == id).cloned())
            .collect()
    } else {
        all.iter().filter(|g| g.rank.is_some()).take(DEFAULT_VARIETY).cloned().collect()
    };
    if cfg.minutes == 0 {
        let chosen = all.iter().find(|g| g.id == cfg.single_game).cloned();
        return chosen.or_else(|| list.into_iter().next()).into_iter().collect();
    }
    // Least-farmed first, so each start spreads time across the list instead
    // of always beginning with the same game. Stable: ties keep list order.
    let mut list = list;
    list.sort_by_key(|g| cfg.per_game.get(&g.id).map(|s| s.secs).unwrap_or(0));
    list
}

// ---------------------------------------------------------------------------
// The stand-in game process
// ---------------------------------------------------------------------------

/// `--farm-stub <title> [image url]`: show a small card window and idle until
/// our stdin closes (the app stopped us or exited). Discord's detector only
/// counts processes that own a visible top-level window (a windowless stub is
/// ignored), so the card sits in the bottom-right corner, shows the game's art
/// and says plainly that Aurora is farming it.
pub fn run_stub(title: &str, image: &str) {
    use tao::dpi::{LogicalSize, PhysicalPosition};
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;

    exit_with_parent();

    let event_loop = EventLoop::new();
    let (w, h) = (340.0, 190.0);
    let builder = WindowBuilder::new()
        .with_title(format!("{title} · Aurora Quests badge farm"))
        .with_inner_size(LogicalSize::new(w, h))
        .with_decorations(false)
        .with_resizable(false)
        .with_focused(false)
        // Built hidden and shown by show_quietly, so it never steals focus
        // from (or pops over) a game the user is playing.
        .with_visible(false);
    let Ok(window) = builder.build(&event_loop) else {
        // No window possible: still idle so the process at least exists.
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    };
    // Bottom-right of the primary screen, clear of the taskbar.
    if let Some(m) = window.primary_monitor() {
        let (sz, at, sf) = (m.size(), m.position(), m.scale_factor());
        window.set_outer_position(PhysicalPosition::new(
            at.x + sz.width as i32 - ((w + 16.0) * sf) as i32,
            at.y + sz.height as i32 - ((h + 64.0) * sf) as i32,
        ));
    }
    let html = stub_html(title, image);
    let _webview = wry::WebViewBuilder::new()
        .with_html(html)
        .with_focused(false)
        .build(&window);
    show_quietly(&window);
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            std::process::exit(0);
        }
    });
}

/// Quit when Aurora goes away: it holds our stdin pipe, which closes when it
/// exits (or kills us), so a crash never leaves helper windows behind.
pub fn exit_with_parent() {
    std::thread::spawn(|| {
        use std::io::Read;
        let mut buf = [0u8; 64];
        let mut stdin = std::io::stdin();
        while let Ok(n) = stdin.read(&mut buf) {
            if n == 0 {
                break;
            }
        }
        std::process::exit(0);
    });
}

/// Discord CDN art for a game (cover, else icon), or "" if it has none.
pub fn art_url(game: &Game) -> String {
    game.cover
        .as_ref()
        .or(game.icon.as_ref())
        .map(|h| format!("https://cdn.discordapp.com/app-icons/{}/{h}.png?size=512", game.id))
        .unwrap_or_default()
}

/// Show the card without taking focus: put it at the bottom of the window
/// stack, let Discord's scan see it, then minimise it (still without
/// activating), so it never pops over or tabs the user out of a game. Detection
/// survives the minimise. `AURORA_STUB_SHOW=corner` leaves it showing.
#[cfg(windows)]
fn show_quietly(window: &tao::window::Window) {
    use tao::platform::windows::WindowExtWindows;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, ShowWindow, HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
        SW_SHOWMINNOACTIVE,
    };
    let raw = window.hwnd();
    unsafe {
        let _ = SetWindowPos(
            HWND(raw as _),
            HWND_BOTTOM,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
    if std::env::var("AURORA_STUB_SHOW").as_deref() == Ok("corner") {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(15));
        unsafe {
            let _ = ShowWindow(HWND(raw as _), SW_SHOWMINNOACTIVE);
        }
    });
}

#[cfg(not(windows))]
fn show_quietly(window: &tao::window::Window) {
    window.set_visible(true);
}

/// The stand-in's card: game art behind an Aurora banner.
fn stub_html(title: &str, image: &str) -> String {
    let esc = |s: &str| {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
    };
    let art = if image.starts_with("https://cdn.discordapp.com/") {
        format!("background-image:url(\"{}\")", esc(image))
    } else {
        String::new()
    };
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><style>
html,body{{margin:0;height:100%;overflow:hidden;font-family:"Segoe UI",system-ui,sans-serif;color:#fff;background:#0b0c18;user-select:none}}
.art{{position:absolute;inset:0;background:#141633 center/cover no-repeat;{art}}}
.art::after{{content:"";position:absolute;inset:0;background:linear-gradient(180deg,rgba(11,12,24,.15),rgba(11,12,24,.92) 72%)}}
.top{{position:absolute;top:10px;left:10px;right:10px;display:flex;align-items:center;gap:8px}}
.logo{{width:26px;height:26px;border-radius:8px}}
.badge{{font-size:11px;font-weight:800;letter-spacing:.6px;text-transform:uppercase;padding:4px 9px;border-radius:8px;
  background:linear-gradient(135deg,#b48cff,#34d399);color:#0b0616}}
.live{{margin-left:auto;font-size:11px;font-weight:700;padding:4px 8px;border-radius:8px;background:rgba(11,12,24,.7)}}
.live i{{display:inline-block;width:7px;height:7px;border-radius:50%;background:#34d399;margin-right:6px;animation:p 1.4s infinite}}
@keyframes p{{50%{{opacity:.3}}}}
.foot{{position:absolute;left:14px;right:14px;bottom:12px}}
.g{{font-size:18px;font-weight:800;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}}
.s{{font-size:12px;opacity:.75;margin-top:2px}}
</style></head><body><div class="art"></div>
<div class="top"><img class="logo" src="{logo}"><span class="badge">Aurora Quests</span><span class="live"><i></i>Badge farming</span></div>
<div class="foot"><div class="g">{name}</div><div class="s">Stand-in run by Aurora Quests so Discord counts this game. Stop it from Aurora.</div></div>
</body></html>"#,
        logo = crate::assets::logo_uri(),
        name = esc(title),
    )
}

/// Copy our binary to `<config>/farm/<app id>/<exe path>` and start it there.
fn spawn_stub(game: &Game) -> Result<Child, String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = platform::config_dir()
        .join("farm")
        .join(&game.id)
        .join(game.exe.split('/').collect::<PathBuf>());
    if let Some(d) = target.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    // Refresh the copy when the app updates (size differs); a copy still
    // running from a previous session can't be overwritten, so keep it.
    let stale = match (std::fs::metadata(&me), std::fs::metadata(&target)) {
        (Ok(a), Ok(b)) => a.len() != b.len(),
        _ => true,
    };
    // A hard link costs no disk space (variety mode makes dozens of these);
    // fall back to a full copy across drives.
    if stale {
        let _ = std::fs::remove_file(&target);
        if std::fs::hard_link(&me, &target).is_err() {
            if let Err(e) = std::fs::copy(&me, &target) {
                if !target.exists() {
                    return Err(e.to_string());
                }
            }
        }
    }
    let mut cmd = Command::new(&target);
    let image = art_url(game);
    cmd.arg("--farm-stub")
        .arg(&game.name)
        .arg(image)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(d) = target.parent() {
        cmd.current_dir(d);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn().map_err(|e| e.to_string())
}

fn kill(child: &mut Option<Child>) {
    if let Some(mut c) = child.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

// ---------------------------------------------------------------------------
// The farm loop
// ---------------------------------------------------------------------------

/// Run a farm until `stop` is set. `status` receives a JSON status object every
/// few seconds (and `null` when the farm ends).
pub fn run(
    list: Vec<Game>,
    minutes: u64,
    pause: Option<(Duration, Duration)>,
    stop: Arc<AtomicBool>,
    presence: Arc<Presence>,
    status: impl Fn(Value),
) {
    if list.is_empty() {
        status(json!({ "error": "No games to farm — add some to your list first." }));
        return;
    }
    // 0 minutes = stay on the one game until the user switches.
    let slice = (minutes > 0).then(|| Duration::from_secs(minutes * 60));
    let mut cfg = load();
    let mut idx = 0usize;
    let mut round = 1u64;
    let started = Instant::now();
    // Farming time since the last scheduled break.
    let mut active = Duration::ZERO;
    'outer: while !stop.load(Ordering::SeqCst) {
        let game = &list[idx];
        let mut error = Value::Null;
        let mut child = match spawn_stub(game) {
            Ok(c) => Some(c),
            Err(e) => {
                error = Value::from(e);
                None
            }
        };
        if crate::settings::load().show_presence {
            presence.set(&game.name, &game.id);
        }
        crate::stream::set_now(Some(game));
        if !cfg.played.contains(&game.id) {
            cfg.played.push(game.id.clone());
        }
        let entry = cfg.per_game.entry(game.id.clone()).or_default();
        entry.name = game.name.clone();
        entry.icon = game.icon.clone();
        entry.cover = game.cover.clone();
        save(&cfg);
        let mut game_start = Instant::now();
        let mut unsaved = 0u64;
        let mut tick = 0u64;
        let flush = |cfg: &mut Config, secs: &mut u64| {
            cfg.total_secs += *secs;
            cfg.per_game.entry(game.id.clone()).or_default().secs += *secs;
            *secs = 0;
            save(cfg);
        };
        loop {
            if stop.load(Ordering::SeqCst) {
                flush(&mut cfg, &mut unsaved);
                kill(&mut child);
                break 'outer;
            }
            // The stand-in died (antivirus, user closed it): start it again.
            if let Some(c) = child.as_mut() {
                if let Ok(Some(_)) = c.try_wait() {
                    child = spawn_stub(game).ok();
                }
            }
            if tick % 5 == 0 {
                let game_secs = cfg.per_game.get(&game.id).map(|g| g.secs).unwrap_or(0) + unsaved;
                status(json!({
                    "game": game.name,
                    "id": game.id,
                    "index": idx,
                    "count": list.len(),
                    "round": round,
                    "single": slice.is_none(),
                    "runSecs": game_start.elapsed().as_secs(),
                    "sliceSecs": slice.map(|d| d.as_secs()),
                    "gameSecs": game_secs,
                    "sessionSecs": started.elapsed().as_secs(),
                    "totalSecs": cfg.total_secs + unsaved,
                    "played": cfg.played.len(),
                    "error": error,
                }));
            }
            if slice.map(|d| game_start.elapsed() >= d).unwrap_or(false) {
                break;
            }
            // Scheduled break: drop the game, wait, then pick up where we were.
            if let Some((after, rest)) = pause {
                if active >= after {
                    flush(&mut cfg, &mut unsaved);
                    kill(&mut child);
                    presence.clear();
                    crate::stream::set_now(None);
                    let rest_start = Instant::now();
                    while rest_start.elapsed() < rest {
                        if stop.load(Ordering::SeqCst) {
                            break 'outer;
                        }
                        if rest_start.elapsed().as_secs() % 5 == 0 {
                            status(json!({
                                "resting": true,
                                "game": game.name,
                                "id": game.id,
                                "restLeft": (rest - rest_start.elapsed()).as_secs(),
                                "totalSecs": cfg.total_secs,
                                "played": cfg.played.len(),
                            }));
                        }
                        std::thread::sleep(Duration::from_secs(1));
                    }
                    active = Duration::ZERO;
                    game_start += rest_start.elapsed();
                    child = spawn_stub(game).ok();
                    if crate::settings::load().show_presence {
                        presence.set(&game.name, &game.id);
                    }
                    crate::stream::set_now(Some(game));
                    tick = 0;
                    continue;
                }
            }
            std::thread::sleep(Duration::from_secs(1));
            tick += 1;
            unsaved += 1;
            active += Duration::from_secs(1);
            if unsaved >= 60 {
                flush(&mut cfg, &mut unsaved);
            }
        }
        flush(&mut cfg, &mut unsaved);
        kill(&mut child);
        idx += 1;
        if idx >= list.len() {
            idx = 0;
            round += 1;
        }
    }
    crate::stream::set_now(None);
    // Presence is cleared by whoever stopped us (farmStop), so a farm that's
    // being replaced doesn't wipe its successor's game.
    status(Value::Null);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_paths_are_sanitised() {
        assert_eq!(clean_rel_path(">valorant.exe").as_deref(), Some("valorant.exe"));
        assert_eq!(
            clean_rel_path("steamapps/common/Terraria/Terraria.exe").as_deref(),
            Some("steamapps/common/Terraria/Terraria.exe")
        );
        assert_eq!(clean_rel_path("../../evil/x.exe").as_deref(), Some("evil/x.exe"));
        assert_eq!(clean_rel_path("C:\\Games\\a.exe").as_deref(), Some("C/Games/a.exe"));
        assert_eq!(clean_rel_path(" / .. / "), None);
    }

    #[test]
    fn exe_prefers_this_os_and_skips_launchers() {
        let app = json!({ "executables": [
            { "os": "win32", "name": "launcher.exe", "is_launcher": true },
            { "os": "darwin", "name": "Game.app", "is_launcher": false },
            { "os": "win32", "name": "game/game.exe", "is_launcher": false },
        ]});
        assert_eq!(pick_exe(&app).as_deref(), Some("game/game.exe"));
    }

    #[test]
    fn exe_prefers_the_plain_main_exe() {
        let app = json!({ "executables": [
            { "os": "win32", "name": "garenalolth/gamedata/apps/lolth/lolex.exe", "is_launcher": false },
            { "os": "win32", "name": "league of legends.exe", "is_launcher": false },
            { "os": "win32", "name": "leagueclientux.exe", "is_launcher": true },
        ]});
        assert_eq!(pick_exe(&app).as_deref(), Some("league of legends.exe"));
    }

    #[test]
    fn plan_picks_by_mode() {
        let g = |id: &str, rank| Game {
            id: id.into(), name: id.into(), exe: "x.exe".into(), rank, icon: None, cover: None,
        };
        let all = vec![g("a", Some(0)), g("b", Some(1)), g("c", None)];
        let mut cfg = Config::default();
        assert_eq!(plan(&cfg, &all).len(), 2);
        cfg.use_custom = true;
        cfg.custom = vec!["c".into(), "gone".into(), "a".into()];
        let ids: Vec<_> = plan(&cfg, &all).into_iter().map(|g| g.id).collect();
        assert_eq!(ids, ["c", "a"]);
        // One-game mode: the chosen game, else the list's first.
        cfg.minutes = 0;
        assert_eq!(plan(&cfg, &all).iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["c"]);
        cfg.single_game = "b".into();
        assert_eq!(plan(&cfg, &all)[0].id, "b");
    }

    #[test]
    fn plan_starts_with_least_farmed() {
        let g = |id: &str, rank| Game {
            id: id.into(), name: id.into(), exe: "x.exe".into(), rank, icon: None, cover: None,
        };
        let all = vec![g("a", Some(0)), g("b", Some(1)), g("c", Some(2))];
        let mut cfg = Config::default();
        for (id, secs) in [("a", 900), ("c", 60)] {
            cfg.per_game.insert(id.into(), GameStat { secs, ..Default::default() });
        }
        let order: Vec<_> = plan(&cfg, &all).into_iter().map(|g| g.id).collect();
        assert_eq!(order, ["b", "c", "a"]);
    }

    #[test]
    fn old_config_keys_still_load() {
        let c: Config = serde_json::from_str(
            r#"{"hours_game":"x","variety_custom":["a"],"variety_use_custom":true,"variety_minutes":5}"#,
        ).unwrap();
        assert_eq!((c.single_game.as_str(), c.custom.len(), c.use_custom, c.minutes), ("x", 1, true, 5));
    }
}
