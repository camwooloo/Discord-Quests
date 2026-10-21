// Badge farming: the Game Time badge (hours played) and the Game Variety badge
// (number of different games played).
//
// Discord's client counts a game as "played" when it sees one of the game's
// known executables running. The list of those executables is Discord's public
// detectable-applications list (`/applications/detectable`), which we cache in
// the config dir. To "play" a game we copy our own binary to a path ending in
// that game's executable name and run it with `--farm-stub`, which just idles
// until we kill it (or we exit — it watches its stdin pipe). The local Discord
// client then detects it like the real game. If "Show game on my profile" is
// on, we also broadcast it over the gateway, same as quest play.
//
// Two modes, one farm at a time:
//   * hours   — one game, runs until stopped (Game Time)
//   * variety — cycles through a list of games, N minutes each (Game Variety)

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
}

/// Persisted farm settings + totals (`farm.json` in the config dir).
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    /// App id of the game the hours mode runs (empty = most popular).
    pub hours_game: String,
    /// The user's own variety list (app ids, in order).
    pub variety_custom: Vec<String>,
    /// Use `variety_custom` instead of the popular list.
    pub variety_use_custom: bool,
    /// Minutes each game runs for in variety mode.
    pub variety_minutes: u64,
    /// All-time seconds farmed (both modes).
    pub total_secs: u64,
    /// Distinct app ids the farm has run.
    pub played: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            hours_game: String::new(),
            variety_custom: Vec::new(),
            variety_use_custom: false,
            variety_minutes: 15,
            total_secs: 0,
            played: Vec::new(),
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
        .and_then(|r| r.into_string().map_err(|e| e.to_string()));
    match fetched {
        Ok(s) if s.trim_start().starts_with('[') => {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&p, &s);
            Ok(s)
        }
        Ok(_) => std::fs::read_to_string(&p).map_err(|_| "unexpected response from Discord".into()),
        Err(e) => std::fs::read_to_string(&p).map_err(|_| format!("network error: {e}")),
    }
}

#[cfg(windows)]
const OS_KEYS: &[&str] = &["win32"];
// Linux clients detect native games, and Proton/Wine games by their .exe name.
#[cfg(not(windows))]
const OS_KEYS: &[&str] = &["linux", "win32"];

/// The first non-launcher executable for this OS, as a safe relative path.
fn pick_exe(app: &Value) -> Option<String> {
    let exes = app["executables"].as_array()?;
    for os in OS_KEYS {
        for e in exes {
            if e["os"].as_str() != Some(os) || e["is_launcher"].as_bool() == Some(true) {
                continue;
            }
            if let Some(rel) = e["name"].as_str().and_then(clean_rel_path) {
                return Some(rel);
            }
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
            Some(Game { id, name, exe, rank })
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
        .map(|g| json!({ "id": g.id, "name": g.name, "pop": g.rank }))
        .collect();
    Value::Array(arr).to_string()
}

/// The games a farm run uses, given the mode and saved config.
pub fn plan(mode: &str, cfg: &Config, all: &[Game]) -> Vec<Game> {
    let popular = all.iter().filter(|g| g.rank.is_some());
    if mode == "hours" {
        let chosen = all.iter().find(|g| g.id == cfg.hours_game);
        return chosen.or_else(|| all.first()).cloned().into_iter().collect();
    }
    if cfg.variety_use_custom && !cfg.variety_custom.is_empty() {
        return cfg
            .variety_custom
            .iter()
            .filter_map(|id| all.iter().find(|g| &g.id == id).cloned())
            .collect();
    }
    popular.take(DEFAULT_VARIETY).cloned().collect()
}

// ---------------------------------------------------------------------------
// The stand-in game process
// ---------------------------------------------------------------------------

/// `--farm-stub`: idle until our stdin closes (the app stopped us or exited).
pub fn run_stub() {
    use std::io::Read;
    let mut buf = [0u8; 64];
    let mut stdin = std::io::stdin();
    while let Ok(n) = stdin.read(&mut buf) {
        if n == 0 {
            break;
        }
    }
}

/// Copy our binary to `<config>/farm/<app id>/<exe path>` and start it there.
fn spawn_stub(game: &Game) -> Result<Child, String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = platform::config_dir()
        .join("farm")
        .join(&game.id)
        .join(&game.exe);
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
    cmd.arg("--farm-stub")
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
    mode: String,
    list: Vec<Game>,
    minutes: u64,
    stop: Arc<AtomicBool>,
    presence: Arc<Presence>,
    status: impl Fn(Value),
) {
    if list.is_empty() {
        status(json!({ "mode": mode, "error": "No games to run — pick some first." }));
        return;
    }
    let slice = if mode == "hours" {
        None
    } else {
        Some(Duration::from_secs(minutes.max(1) * 60))
    };
    let mut cfg = load();
    let mut idx = 0usize;
    let mut round = 1u64;
    let started = Instant::now();
    'outer: while !stop.load(Ordering::SeqCst) {
        let game = &list[idx];
        let mut child = None;
        let mut error = Value::Null;
        match spawn_stub(game) {
            Ok(c) => child = Some(c),
            Err(e) => error = Value::from(e),
        }
        if crate::settings::load().show_presence {
            presence.set(&game.name, &game.id);
        }
        if !cfg.played.contains(&game.id) {
            cfg.played.push(game.id.clone());
            save(&cfg);
        }
        let game_start = Instant::now();
        let mut unsaved = 0u64;
        let mut tick = 0u64;
        loop {
            if stop.load(Ordering::SeqCst) {
                cfg.total_secs += unsaved;
                save(&cfg);
                kill(&mut child);
                break 'outer;
            }
            // The stand-in died (antivirus, user killed it): try once more.
            if let Some(c) = child.as_mut() {
                if let Ok(Some(_)) = c.try_wait() {
                    child = spawn_stub(game).ok();
                }
            }
            if tick % 5 == 0 {
                status(json!({
                    "mode": mode,
                    "game": game.name,
                    "id": game.id,
                    "index": idx,
                    "count": list.len(),
                    "round": round,
                    "gameSecs": game_start.elapsed().as_secs(),
                    "sliceSecs": slice.map(|d| d.as_secs()),
                    "sessionSecs": started.elapsed().as_secs(),
                    "totalSecs": cfg.total_secs + unsaved,
                    "played": cfg.played.len(),
                    "error": error,
                }));
            }
            if slice.map(|d| game_start.elapsed() >= d).unwrap_or(false) {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
            tick += 1;
            unsaved += 1;
            if unsaved >= 60 {
                cfg.total_secs += unsaved;
                unsaved = 0;
                save(&cfg);
            }
        }
        cfg.total_secs += unsaved;
        save(&cfg);
        kill(&mut child);
        idx += 1;
        if idx >= list.len() {
            idx = 0;
            round += 1;
        }
    }
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
    fn plan_picks_by_mode() {
        let g = |id: &str, rank| Game { id: id.into(), name: id.into(), exe: "x.exe".into(), rank };
        let all = vec![g("a", Some(0)), g("b", Some(1)), g("c", None)];
        let mut cfg = Config::default();
        assert_eq!(plan("hours", &cfg, &all)[0].id, "a");
        cfg.hours_game = "c".into();
        assert_eq!(plan("hours", &cfg, &all)[0].id, "c");
        assert_eq!(plan("variety", &cfg, &all).len(), 2);
        cfg.variety_use_custom = true;
        cfg.variety_custom = vec!["c".into(), "gone".into(), "a".into()];
        let ids: Vec<_> = plan("variety", &cfg, &all).into_iter().map(|g| g.id).collect();
        assert_eq!(ids, ["c", "a"]);
    }
}
