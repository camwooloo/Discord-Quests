// A log of completed quests (name, category, orbs, date) for the History view.
// Stored in %APPDATA%\AuroraQuests\history.json, newest last, capped at 500.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Entry {
    pub name: String,
    pub category: String,
    pub orbs: u64,
    pub date: String, // YYYY-MM-DD
}

fn path() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
        .join("AuroraQuests")
        .join("history.json")
}

pub fn load() -> Vec<Entry> {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(v: &[Entry]) {
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(j) = serde_json::to_string(v) {
        let _ = std::fs::write(&p, j);
    }
}

/// Record a completed quest (once per name per day) and return the updated log.
pub fn add(name: &str, category: &str, orbs: u64) -> Vec<Entry> {
    let mut v = load();
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    if v.iter().any(|e| e.name == name && e.date == date) {
        return v; // already logged today
    }
    v.push(Entry {
        name: name.to_string(),
        category: category.to_string(),
        orbs,
        date,
    });
    if v.len() > 500 {
        let drop = v.len() - 500;
        v.drain(0..drop);
    }
    save(&v);
    v
}
