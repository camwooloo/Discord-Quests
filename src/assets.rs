// Embedded image assets: the four evolving-badge tier icons and the app logo.
// Everything ships inside the single portable exe, exposed to the WebView UI as
// base64 data URIs and to the window/tray as decoded RGBA.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{Map, Value};

macro_rules! badge_list {
    ($($n:literal),* $(,)?) => {
        &[ $( ($n, &include_bytes!(concat!("badges/", $n, ".webp"))[..]) ),* ]
    };
}

/// `{ "account_age_1": "data:image/webp;base64,…", … }` for all four badge
/// families' ten tiers — used as the real tier icons in the Badges tab.
pub fn badge_uris() -> String {
    let items: &[(&str, &[u8])] = badge_list![
        "account_age_1", "account_age_2", "account_age_3", "account_age_4", "account_age_5",
        "account_age_6", "account_age_7", "account_age_8", "account_age_9", "account_age_10",
        "game_depth_tier_1", "game_depth_tier_2", "game_depth_tier_3", "game_depth_tier_4",
        "game_depth_tier_5", "game_depth_tier_6", "game_depth_tier_7", "game_depth_tier_8",
        "game_depth_tier_9", "game_depth_tier_10",
        "game_diversity_tier_1", "game_diversity_tier_2", "game_diversity_tier_3",
        "game_diversity_tier_4", "game_diversity_tier_5", "game_diversity_tier_6",
        "game_diversity_tier_7", "game_diversity_tier_8", "game_diversity_tier_9",
        "game_diversity_tier_10",
        "streaming_tier_1", "streaming_tier_2", "streaming_tier_3", "streaming_tier_4",
        "streaming_tier_5", "streaming_tier_6", "streaming_tier_7", "streaming_tier_8",
        "streaming_tier_9", "streaming_tier_10",
    ];
    let mut m = Map::new();
    for (k, b) in items {
        m.insert(
            (*k).to_string(),
            Value::from(format!("data:image/webp;base64,{}", STANDARD.encode(b))),
        );
    }
    serde_json::to_string(&Value::Object(m)).unwrap_or_else(|_| "{}".into())
}

const LOGO: &[u8] = include_bytes!("logo.png");

/// The app logo as a base64 data URI (for the sidebar mark).
pub fn logo_uri() -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(LOGO))
}

/// The app logo decoded to RGBA (for the window + tray icons).
pub fn logo_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let mut reader = png::Decoder::new(LOGO).read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width, info.height);
    match info.color_type {
        png::ColorType::Rgba => {
            buf.truncate((w * h * 4) as usize);
            Some((buf, w, h))
        }
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity((w * h * 4) as usize);
            for px in buf.chunks_exact(3) {
                out.extend_from_slice(px);
                out.push(255);
            }
            Some((out, w, h))
        }
        _ => None,
    }
}
