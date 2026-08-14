// Fetch and model Discord Quests.
//
// The client-facing endpoint `GET /api/v9/quests/@me` returns every quest the
// account can see. We parse each into a compact `Quest`, classify it as a
// video-watch or game-play quest from its task types, and flag orb rewards.
//
// Parsing is intentionally lenient: Discord evolves this schema often, so we
// pull fields defensively and fall back gracefully rather than hard-failing.

use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Video,
    Game,
}

#[derive(Clone)]
pub struct Reward {
    pub name: String,
    #[allow(dead_code)] // orb detection is surfaced via `has_orb`
    pub is_orb: bool,
    pub orb_quantity: Option<u64>,
    /// Nitro-tier payout (the "x1.2" the client shows), when present.
    pub premium_orb_quantity: Option<u64>,
}

/// Base URL for quest CDN assets (verified: serves the mp4 unauthenticated).
pub const CDN_BASE: &str = "https://cdn.discordapp.com/";

#[derive(Clone)]
pub struct Quest {
    #[allow(dead_code)] // used by the watch/enroll flow
    pub id: String,
    pub name: String,
    pub app_name: String,
    pub category: Category,
    pub tasks: Vec<String>,
    pub expires_at: String,
    pub starts_at: String,
    pub rewards: Vec<Reward>,
    pub has_orb: bool,
    pub claimed: bool,
    pub expired: bool,
    /// Sealed traffic metadata echoed back when claiming (mirrors the client).
    pub traffic_metadata_sealed: Option<String>,
    /// Play-quest application id (for heartbeats), when this is a game quest.
    pub app_id: Option<String>,
    // Progress / completion
    #[allow(dead_code)] // drives progress lookups; retained for diagnostics
    pub primary_task: Option<String>,
    pub target_seconds: Option<u64>,
    pub progress_seconds: u64,
    pub enrolled: bool,
    pub completed: bool,
    // Media / actions
    pub video_url: Option<String>,
    #[allow(dead_code)] // staged for card thumbnails
    pub thumb_url: Option<String>,
    pub cta_link: Option<String>,
    pub cta_label: Option<String>,
}

/// Full pipeline: locate token -> call Discord -> parse quests.
pub fn scan() -> Result<Vec<Quest>, String> {
    let token = crate::token::find_token()?;
    fetch(&token)
}

/// Desktop-client User-Agent used for all Discord API calls.
pub const CLIENT_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) discord/1.0.9251 Chrome/128 Electron/32 Safari/537.36";

/// Desktop-client identity. Discord targets quests by client build/platform via
/// this header — WITHOUT it, `quests/@me` returns only a stripped generic subset
/// (no watch-video quests). The build number is intentionally high so it never
/// reads as stale; Discord accepts it.
pub fn super_properties() -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let json = r#"{"os":"Windows","browser":"Discord Client","release_channel":"stable","client_version":"1.0.9251","os_version":"10.0.26200","system_locale":"en-US","client_build_number":9999999,"native_build_number":null,"client_event_source":null}"#;
    STANDARD.encode(json)
}

/// Fetch and parse all quests for the given token.
pub fn fetch(token: &str) -> Result<Vec<Quest>, String> {
    let result = ureq::get("https://discord.com/api/v9/quests/@me")
        .set("Authorization", token)
        .set("User-Agent", CLIENT_UA)
        .set("X-Super-Properties", &super_properties())
        .set("X-Discord-Locale", "en-US")
        .set("Accept", "*/*")
        .timeout(std::time::Duration::from_secs(20))
        .call();

    let text = match result {
        Ok(resp) => resp.into_string().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            let hint = match code {
                401 => " (token rejected — try restarting Discord and rescanning)",
                403 => " (forbidden)",
                429 => " (rate limited — wait a bit and rescan)",
                _ => "",
            };
            return Err(format!("Discord API returned HTTP {code}{hint}.\n{body}"));
        }
        Err(e) => return Err(format!("network error: {e}")),
    };

    let root: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;

    // Response is `{ "quests": [...] }`; tolerate a bare array too.
    let arr = root
        .get("quests")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| root.as_array().cloned())
        .unwrap_or_default();

    let mut out: Vec<Quest> = arr.iter().map(parse_quest).collect();

    // Drop quests we can't complete here — Discord Activity achievement quests
    // (ACHIEVEMENT_IN_ACTIVITY) and console-only quests have no watch/desktop-play task.
    out.retain(|q| {
        q.tasks.iter().any(|t| {
            t == "WATCH_VIDEO" || t == "WATCH_VIDEO_ON_MOBILE" || t == "PLAY_ON_DESKTOP"
        })
    });

    // Active quests first, then alphabetical.
    out.sort_by(|a, b| a.expired.cmp(&b.expired).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

fn parse_quest(q: &Value) -> Quest {
    let config = &q["config"];
    let messages = &config["messages"];

    let id = q["id"].as_str().unwrap_or("").to_string();

    let app_name = config["application"]["name"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let name = messages["quest_name"]
        .as_str()
        .or_else(|| messages["game_title"].as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if app_name.is_empty() {
                "Unnamed quest".to_string()
            } else {
                app_name.clone()
            }
        });

    // Task types can live under task_config or task_config_v2.
    let task_cfg = if config["task_config_v2"].is_object() {
        &config["task_config_v2"]
    } else {
        &config["task_config"]
    };
    let mut tasks: Vec<String> = task_cfg["tasks"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    tasks.sort();

    let is_video = tasks
        .iter()
        .any(|t| t.to_ascii_uppercase().contains("WATCH") || t.to_ascii_uppercase().contains("VIDEO"));
    let category = if is_video {
        Category::Video
    } else {
        Category::Game
    };

    // Pick the task that drives progress: the watch task for videos, else desktop play.
    let primary_task = if is_video {
        tasks
            .iter()
            .find(|t| t.to_ascii_uppercase().contains("WATCH"))
            .cloned()
    } else {
        tasks
            .iter()
            .find(|t| *t == "PLAY_ON_DESKTOP")
            .or_else(|| tasks.first())
            .cloned()
    };

    let target_seconds = primary_task
        .as_ref()
        .and_then(|k| task_cfg["tasks"][k]["target"].as_u64());

    // Application id strictly from the desktop-play task (heartbeats need it).
    let app_id = task_cfg["tasks"]["PLAY_ON_DESKTOP"]["applications"][0]["id"]
        .as_str()
        .map(str::to_string);

    // Media + call-to-action.
    let assets = &config["assets"];
    let asset_url = |key: &str| {
        assets[key]
            .as_str()
            .filter(|s| !s.is_empty() && *s != "PLACEHOLDER")
            .map(|p| format!("{CDN_BASE}{p}"))
    };
    // The real watch-video lives inside the WATCH task's own assets. Prefer the
    // 720p variant for faster load, fall back to 1080p, then the hero trailer.
    let task_assets = primary_task
        .as_ref()
        .map(|k| &task_cfg["tasks"][k]["assets"]);
    let task_video = task_assets.and_then(|a| {
        a["video_low_res"]["url"]
            .as_str()
            .or_else(|| a["video"]["url"].as_str())
            .map(|p| format!("{CDN_BASE}{p}"))
    });
    let task_thumb = task_assets.and_then(|a| {
        a["video"]["thumbnail"]
            .as_str()
            .map(|p| format!("{CDN_BASE}{p}"))
    });

    let video_url = task_video
        .or_else(|| asset_url("asset_video"))
        .or_else(|| asset_url("hero_video"));
    let thumb_url = task_thumb
        .or_else(|| asset_url("game_tile"))
        .or_else(|| asset_url("hero"));

    let cta_link = config["cta_config"]["link"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let cta_label = config["cta_config"]["button_label"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    // Rewards + orb detection.
    let mut rewards = Vec::new();
    let mut has_orb = false;
    if let Some(list) = config["rewards_config"]["rewards"].as_array() {
        for r in list {
            let rname = r["messages"]["name"]
                .as_str()
                .or_else(|| r["name"].as_str())
                .unwrap_or("")
                .to_string();
            let orb_quantity = r["orb_quantity"]
                .as_u64()
                .or_else(|| r["orb_amount"].as_u64())
                .filter(|n| *n > 0);
            let raw = r.to_string().to_ascii_lowercase();
            let is_orb =
                orb_quantity.is_some() || rname.to_ascii_lowercase().contains("orb") || raw.contains("orb_");
            if is_orb {
                has_orb = true;
            }
            rewards.push(Reward {
                name: if rname.is_empty() {
                    "Reward".to_string()
                } else {
                    rname
                },
                is_orb,
                orb_quantity,
                premium_orb_quantity: r["premium_orb_quantity"].as_u64().filter(|n| *n > 0),
            });
        }
    }

    let expires_at = config["expires_at"]
        .as_str()
        .or_else(|| q["expires_at"].as_str())
        .unwrap_or("")
        .to_string();
    let starts_at = config["starts_at"].as_str().unwrap_or("").to_string();
    let expired = is_expired(&expires_at);

    let status = &q["user_status"];
    let enrolled = status["enrolled_at"].as_str().is_some();
    let completed = status["completed_at"].as_str().is_some();
    // Claimed is strictly "reward collected" — completed-but-unclaimed quests
    // are what the Claim tab surfaces.
    let claimed = status["claimed_at"].as_str().is_some();
    let progress_seconds = primary_task
        .as_ref()
        .and_then(|k| status["progress"][k]["value"].as_u64())
        .unwrap_or(0);

    Quest {
        id,
        name,
        app_name,
        category,
        tasks,
        expires_at,
        starts_at,
        rewards,
        has_orb,
        claimed,
        expired,
        traffic_metadata_sealed: q["traffic_metadata_sealed"].as_str().map(str::to_string),
        app_id,
        primary_task,
        target_seconds,
        progress_seconds,
        enrolled,
        completed,
        video_url,
        thumb_url,
        cta_link,
        cta_label,
    }
}

fn is_expired(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    match chrono::DateTime::parse_from_rfc3339(s) {
        Ok(dt) => dt < chrono::Utc::now(),
        Err(_) => false,
    }
}

/// Human-friendly label for a raw task type key.
#[allow(dead_code)]
pub fn pretty_task(task: &str) -> String {
    match task {
        "WATCH_VIDEO" => "Watch video".into(),
        "WATCH_VIDEO_ON_MOBILE" => "Watch video (mobile)".into(),
        "PLAY_ON_DESKTOP" | "PLAY_ON_DESKTOP_V2" => "Play on desktop".into(),
        "STREAM_ON_DESKTOP" => "Stream on desktop".into(),
        "PLAY_ON_XBOX" => "Play on Xbox".into(),
        "PLAY_ON_PLAYSTATION" => "Play on PlayStation".into(),
        "PLAY_ACTIVITY" => "Play activity".into(),
        other => other
            .replace('_', " ")
            .to_ascii_lowercase(),
    }
}

/// Nicely formatted expiry, e.g. "expires 2026-08-20 07:00 UTC".
pub fn pretty_expiry(s: &str) -> String {
    if s.is_empty() {
        return "no expiry".to_string();
    }
    match chrono::DateTime::parse_from_rfc3339(s) {
        Ok(dt) => dt
            .with_timezone(&chrono::Utc)
            .format("%Y-%m-%d %H:%M UTC")
            .to_string(),
        Err(_) => s.to_string(),
    }
}
