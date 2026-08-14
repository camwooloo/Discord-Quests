// Persisted user settings + Windows "launch on startup" integration.
//
// Settings live in %APPDATA%\AuroraQuests\settings.json. Startup is the standard
// per-user Run key (HKCU), written via `reg` so no extra registry crate is needed.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "AuroraQuests";

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    /// Start Aurora Quests when Windows starts.
    pub launch_on_startup: bool,
    /// Automatically watch pending video quests one after another.
    pub auto_watch: bool,
    /// Automatically play (complete) pending game quests one after another.
    pub auto_play: bool,
    /// Show the mimicked game on your Discord profile (Rich Presence) while playing.
    pub show_presence: bool,
    /// Start minimized (useful together with launch_on_startup).
    pub start_minimized: bool,
    /// Whether the first-run explainer has been dismissed.
    pub splash_seen: bool,
    /// Which page to open on launch (home/video/game/claim/shop/badges).
    pub default_page: String,
    /// Shop SKU the user is saving toward (orb goal on the homepage).
    pub orb_goal: String,
    /// Colour theme: "dark" or "light".
    pub theme: String,
    /// Accent preset id (aurora/emerald/sky/gold/cyber).
    pub accent: String,
    /// Desktop notifications for new/finished quests.
    pub notifications: bool,
    /// Minimize to the system tray instead of the taskbar.
    pub minimize_to_tray: bool,
    /// Last app version whose changelog the user has seen (for the update popup).
    pub last_seen_version: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            launch_on_startup: false,
            auto_watch: false,
            auto_play: false,
            show_presence: true, // on by default — broadcast the mimicked game
            start_minimized: false,
            splash_seen: false,
            default_page: "home".into(),
            orb_goal: String::new(),
            theme: "dark".into(),
            accent: "aurora".into(),
            notifications: false,
            minimize_to_tray: false,
            last_seen_version: String::new(),
        }
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_default();
    PathBuf::from(base).join("AuroraQuests").join("settings.json")
}

pub fn load() -> Settings {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(s: &Settings) {
    let path = config_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(&path, json);
    }
    apply_startup(s.launch_on_startup, s.start_minimized);
}

/// Add/remove the HKCU Run entry so Windows launches the app at sign-in.
fn apply_startup(enabled: bool, minimized: bool) {
    let Ok(exe) = std::env::current_exe() else { return };
    if enabled {
        let mut cmd = format!("\"{}\"", exe.display());
        if minimized {
            cmd.push_str(" --minimized");
        }
        let _ = std::process::Command::new("reg")
            .args(["add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &cmd, "/f"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    } else {
        let _ = std::process::Command::new("reg")
            .args(["delete", RUN_KEY, "/v", RUN_VALUE, "/f"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
}

/// Don't flash a console window when shelling out to `reg`.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

use std::os::windows::process::CommandExt;
