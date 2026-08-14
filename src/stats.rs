// Persisted all-time stats: orbs earned, quests completed, seconds farmed, and a
// daily-use streak. Stored alongside settings in %APPDATA%\AuroraQuests\stats.json.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Stats {
    pub orbs_earned: u64,
    pub quests_completed: u64,
    pub seconds_farmed: u64,
    pub streak_days: u32,
    pub last_open: String, // YYYY-MM-DD
}

fn path() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
        .join("AuroraQuests")
        .join("stats.json")
}

pub fn load() -> Stats {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(s: &Stats) {
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(j) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(&p, j);
    }
}

/// Update the daily streak on launch (called once at startup).
pub fn touch_streak(s: &mut Stats) {
    let today = chrono::Local::now().date_naive();
    let today_s = today.format("%Y-%m-%d").to_string();
    if s.last_open == today_s {
        return; // already opened today
    }
    let yesterday = today.pred_opt().map(|d| d.format("%Y-%m-%d").to_string());
    s.streak_days = if Some(&s.last_open) == yesterday.as_ref() {
        s.streak_days.saturating_add(1)
    } else {
        1
    };
    s.last_open = today_s;
}

/// Record a completed quest's reward toward the all-time totals.
pub fn add(s: &mut Stats, orbs: u64, seconds: u64) {
    s.orbs_earned = s.orbs_earned.saturating_add(orbs);
    s.quests_completed = s.quests_completed.saturating_add(1);
    s.seconds_farmed = s.seconds_farmed.saturating_add(seconds);
}
