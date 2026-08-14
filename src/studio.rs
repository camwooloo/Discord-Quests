// Persisted profile-studio selections (decoration/nameplate/effect/frame, name
// style, theme, slider adjustments). Stored as an opaque JSON blob the front-end
// owns, in %APPDATA%\AuroraQuests\studio.json — so the look is remembered until
// the user hits "Reset all".

use std::path::PathBuf;

use serde_json::Value;

fn path() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
        .join("AuroraQuests")
        .join("studio.json")
}

/// The saved studio blob, or JSON null if nothing has been saved yet.
pub fn load() -> Value {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null)
}

pub fn save(v: &Value) {
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(j) = serde_json::to_string(v) {
        let _ = std::fs::write(&p, j);
    }
}
