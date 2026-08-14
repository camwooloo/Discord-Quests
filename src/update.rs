// Self-update from GitHub Releases (the app ships as a single portable exe).
//
// On launch we ask the releases API for the latest tag; if it's newer than this
// build we tell the UI. On the user's OK we download the new exe and swap it in
// place (Windows lets us rename a running exe, then write the replacement), then
// relaunch. A leftover ".old" from a prior update is cleaned up on next start.

use serde_json::Value;

const REPO: &str = "camwooloo/Discord-Quests";
const CURRENT: &str = env!("CARGO_PKG_VERSION");
const UA: &str = "AuroraQuests-Updater";

/// If a newer release exists, returns (version, exe_download_url, release_notes).
pub fn check() -> Option<(String, String, String)> {
    let r = ureq::get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .set("User-Agent", UA)
        .set("Accept", "application/vnd.github+json")
        .call()
        .ok()?;
    let v: Value = r.into_json().ok()?;
    let tag = v["tag_name"].as_str()?.trim_start_matches('v').to_string();
    if !is_newer(&tag, CURRENT) {
        return None;
    }
    let url = v["assets"].as_array()?.iter().find_map(|a| {
        let name = a["name"].as_str()?;
        name.ends_with(".exe")
            .then(|| a["browser_download_url"].as_str().map(str::to_string))
            .flatten()
    })?;
    let notes = v["body"].as_str().unwrap_or("").to_string();
    Some((tag, url, notes))
}

/// Compare dotted version strings (e.g. "0.3.1" > "0.3.0").
fn is_newer(remote: &str, current: &str) -> bool {
    let parse = |s: &str| {
        s.split('.')
            .filter_map(|p| p.trim().parse::<u32>().ok())
            .collect::<Vec<_>>()
    };
    let (a, b) = (parse(remote), parse(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

/// Download the new exe and swap it into place. Caller should then relaunch.
pub fn apply(download_url: &str) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let resp = ureq::get(download_url)
        .set("User-Agent", UA)
        .call()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    std::io::copy(&mut resp.into_reader(), &mut bytes).map_err(|e| e.to_string())?;
    // Sanity check: a real build is well over 1 MB; guard against error pages.
    if bytes.len() < 500_000 {
        return Err("downloaded file looks too small".into());
    }
    let old = exe.with_extension("old");
    let _ = std::fs::remove_file(&old);
    // Windows allows renaming the running executable; the replacement then takes
    // its original path so the next launch picks it up.
    std::fs::rename(&exe, &old).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::write(&exe, &bytes) {
        let _ = std::fs::rename(&old, &exe); // roll back on failure
        return Err(e.to_string());
    }
    Ok(())
}

/// Launch the (now updated) exe again.
pub fn restart() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
}

/// Remove the ".old" binary a previous update left behind.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
    }
}
