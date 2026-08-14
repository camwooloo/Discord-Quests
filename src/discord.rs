// Thin Discord client for the actions the UI needs: fetch quests, enroll, and
// honest watch-progress heartbeats. All calls carry the desktop-client identity
// so quests (including watch quests) are returned and credited correctly.

use serde_json::{json, Value};

use crate::quests::{self, Quest, CLIENT_UA};

pub struct DiscordClient {
    token: String,
    token_err: Option<String>,
}

impl DiscordClient {
    /// Build from the locally logged-in Discord client.
    pub fn from_local() -> Self {
        match crate::token::find_token() {
            Ok(token) => DiscordClient { token, token_err: None },
            Err(e) => DiscordClient { token: String::new(), token_err: Some(e) },
        }
    }

    pub fn fetch_quests(&self) -> Result<Vec<Quest>, String> {
        if let Some(e) = &self.token_err {
            return Err(e.clone());
        }
        quests::fetch(&self.token)
    }

    /// Profile facts used to derive the user's badges.
    pub fn fetch_profile(&self) -> Result<Value, String> {
        if self.token.is_empty() {
            return Err("no token".into());
        }
        let me: Value = ureq::get("https://discord.com/api/v9/users/@me")
            .set("Authorization", &self.token)
            .set("User-Agent", CLIENT_UA)
            .set("X-Super-Properties", &quests::super_properties())
            .call()
            .map_err(|e| e.to_string())?
            .into_json()
            .map_err(|e| e.to_string())?;
        let id = me["id"].as_str().unwrap_or("").to_string();
        // Discord snowflake epoch (2015-01-01) → account creation time.
        let created_ms = id
            .parse::<u64>()
            .ok()
            .map(|n| (n >> 22) + 1_420_070_400_000);

        // The richer /profile endpoint carries premium tenure + boost dates.
        let mut premium_guild_since = Value::Null;
        let mut badges = Value::Array(vec![]);
        if !id.is_empty() {
            if let Ok(r) = ureq::get(&format!(
                "https://discord.com/api/v9/users/{id}/profile?with_mutual_guilds=false"
            ))
            .set("Authorization", &self.token)
            .set("User-Agent", CLIENT_UA)
            .set("X-Super-Properties", &quests::super_properties())
            .call()
            {
                if let Ok(p) = r.into_json::<Value>() {
                    premium_guild_since = p["premium_guild_since"].clone();
                    // The profile's own badge list: real icon hashes + descriptions.
                    badges = p["badges"].clone();
                }
            }
        }
        Ok(json!({
            "publicFlags": me["public_flags"].as_u64().unwrap_or(0),
            "premiumType": me["premium_type"].as_u64().unwrap_or(0),
            "premiumGuildSince": premium_guild_since,
            "createdMs": created_ms,
            "badges": badges,
        }))
    }

    /// The cosmetics the user actually has equipped on Discord right now: avatar,
    /// banner, decoration, nameplate, profile effect, theme + display-name colours,
    /// pronouns, bio and connections. Used for the "real profile" home/profile card.
    pub fn fetch_equipped(&self) -> Result<String, String> {
        if self.token.is_empty() {
            return Err("no token".into());
        }
        let me: Value = ureq::get("https://discord.com/api/v9/users/@me")
            .set("Authorization", &self.token).set("User-Agent", CLIENT_UA)
            .set("X-Super-Properties", &quests::super_properties())
            .call().map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())?;
        let id = me["id"].as_str().unwrap_or("").to_string();
        let prof: Value = ureq::get(&format!(
            "https://discord.com/api/v9/users/{id}/profile?with_mutual_guilds=false"
        ))
        .set("Authorization", &self.token).set("User-Agent", CLIENT_UA)
        .set("X-Super-Properties", &quests::super_properties())
        .call().map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())?;
        let up = &prof["user_profile"];

        let ext = |h: &str| if h.starts_with("a_") { "gif" } else { "png" };
        let avatar = me["avatar"].as_str().map(|h| {
            format!("https://cdn.discordapp.com/avatars/{id}/{h}.{}?size=256", ext(h))
        });
        let banner = me["banner"].as_str().map(|h| {
            format!("https://cdn.discordapp.com/banners/{id}/{h}.{}?size=600", ext(h))
        });
        let deco = me["avatar_decoration_data"]["asset"].as_str().map(|a| {
            format!("https://cdn.discordapp.com/avatar-decoration-presets/{a}.png?size=240")
        });
        let nameplate = me["collectibles"]["nameplate"]["asset"].as_str().map(|a| {
            format!("https://cdn.discordapp.com/assets/collectibles/{a}static.png")
        });
        // Resolve equipped profile effect (animated) and profile frame (layers +
        // geometry) against the catalog — best-effort, one catalog fetch.
        let catalog: Vec<Value> = self
            .fetch_catalog()
            .ok()
            .and_then(|c| serde_json::from_str::<Value>(&c).ok())
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let find = |sku: &str| catalog.iter().find(|it| it["sku"].as_str() == Some(sku));
        let effect_anim = up["profile_effect"]["sku_id"]
            .as_str()
            .and_then(|sku| find(sku).map(|it| it["anim"].clone()))
            .unwrap_or(Value::Null);
        let (mut frame_layers, mut frame_metrics) = (Value::Null, Value::Null);
        if let Some(cols) = me["collectibles"].as_object() {
            for v in cols.values() {
                if let Some(it) = v["sku_id"].as_str().and_then(&find) {
                    if it["kind"].as_str() == Some("frame") {
                        frame_layers = it["layers"].clone();
                        frame_metrics = it["metrics"].clone();
                        break;
                    }
                }
            }
        }
        fn hexc(v: &Value) -> Option<String> {
            v.as_u64().map(|n| format!("#{:06x}", n & 0xff_ffff))
        }
        let ncolors: Vec<String> = me["display_name_styles"]["colors"]
            .as_array()
            .map(|a| a.iter().filter_map(hexc).collect())
            .unwrap_or_default();
        let connections: Vec<Value> = prof["connected_accounts"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|c| json!({ "type": c["type"], "name": c["name"], "verified": c["verified"] }))
                    .collect()
            })
            .unwrap_or_default();
        Ok(json!({
            "name": me["global_name"].as_str().or(me["username"].as_str()).unwrap_or("You"),
            "username": me["username"],
            "avatar": avatar, "banner": banner, "decoration": deco,
            "nameplate": nameplate, "effectAnim": effect_anim,
            "frameLayers": frame_layers, "frameMetrics": frame_metrics,
            "themeA": hexc(&up["theme_colors"][0]), "themeB": hexc(&up["theme_colors"][1]),
            "nameColors": ncolors,
            "pronouns": up["pronouns"].as_str().unwrap_or(""),
            "bio": up["bio"].as_str().unwrap_or(""),
            "connections": connections,
        })
        .to_string())
    }

    /// The signed-in user's orb (virtual currency) balance.
    pub fn fetch_orbs(&self) -> Result<u64, String> {
        if self.token.is_empty() {
            return Err("no token".into());
        }
        let r = ureq::get("https://discord.com/api/v9/users/@me/virtual-currency/balance")
            .set("Authorization", &self.token)
            .set("User-Agent", CLIENT_UA)
            .set("X-Super-Properties", &quests::super_properties())
            .call()
            .map_err(|e| e.to_string())?;
        let v: Value = r.into_json().map_err(|e| e.to_string())?;
        v["balance"].as_u64().ok_or_else(|| "no balance".into())
    }

    /// The signed-in user's display name and avatar URL (for the welcome screen).
    pub fn fetch_me(&self) -> Result<(String, Option<String>), String> {
        if self.token.is_empty() {
            return Err("no token".into());
        }
        let r = ureq::get("https://discord.com/api/v9/users/@me")
            .set("Authorization", &self.token)
            .set("User-Agent", CLIENT_UA)
            .set("X-Super-Properties", &quests::super_properties())
            .call()
            .map_err(|e| e.to_string())?;
        let v: Value = r.into_json().map_err(|e| e.to_string())?;
        let id = v["id"].as_str().unwrap_or("");
        let name = v["global_name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| v["username"].as_str())
            .unwrap_or("there")
            .to_string();
        let avatar = v["avatar"].as_str().map(|a| {
            let ext = if a.starts_with("a_") { "gif" } else { "png" };
            format!("https://cdn.discordapp.com/avatars/{id}/{a}.{ext}?size=160")
        });
        Ok((name, avatar))
    }

    /// Accept a quest (idempotent; already-enrolled quests just error harmlessly).
    pub fn enroll(&self, quest_id: &str) -> Result<(), String> {
        let url = format!("https://discord.com/api/v9/quests/{quest_id}/enroll");
        ureq::post(&url)
            .set("Authorization", &self.token)
            .set("User-Agent", CLIENT_UA)
            .set("X-Super-Properties", &quests::super_properties())
            .set("Content-Type", "application/json")
            .send_json(json!({ "location": 0 }))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// SKU ids the user already owns (to hide from the shop).
    fn fetch_owned(&self) -> std::collections::HashSet<String> {
        let mut owned = std::collections::HashSet::new();
        if let Ok(r) = ureq::get("https://discord.com/api/v9/users/@me/collectibles-purchases")
            .set("Authorization", &self.token)
            .set("User-Agent", CLIENT_UA)
            .set("X-Super-Properties", &quests::super_properties())
            .call()
        {
            if let Ok(v) = r.into_json::<Value>() {
                if let Some(arr) = v.as_array() {
                    for p in arr {
                        if let Some(s) = p["sku_id"].as_str() {
                            owned.insert(s.to_string());
                        }
                        for it in p["items"].as_array().into_iter().flatten() {
                            if let Some(s) = it["sku_id"].as_str() {
                                owned.insert(s.to_string());
                            }
                        }
                    }
                }
            }
        }
        owned
    }

    /// The collectibles the user already owns (for the Owned view).
    pub fn fetch_owned_items(&self) -> Result<String, String> {
        if self.token.is_empty() {
            return Err("no token".into());
        }
        let r = ureq::get("https://discord.com/api/v9/users/@me/collectibles-purchases")
            .set("Authorization", &self.token)
            .set("User-Agent", CLIENT_UA)
            .set("X-Super-Properties", &quests::super_properties())
            .call()
            .map_err(|e| e.to_string())?;
        let v: Value = r.into_json().map_err(|e| e.to_string())?;
        let kind = |t: i64| match t {
            0 => "deco",
            1 => "effect",
            2 => "nameplate",
            3 => "frame",
            1000 => "bundle",
            _ => "other",
        };
        let mut items = Vec::new();
        for p in v.as_array().into_iter().flatten() {
            let sku = p["sku_id"].as_str().unwrap_or("");
            let ptype = p["type"].as_i64().unwrap_or(-1);
            let image = if ptype == 1000 {
                p["preview_assets"]["bg_static"].as_str().map(str::to_string)
            } else {
                Some(format!(
                    "https://cdn.discordapp.com/media/v1/collectibles-shop/{sku}/static.png?size=256"
                ))
            };
            let hex = p["styles"]["background_colors"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|n| n.as_u64())
                .map(|n| format!("#{:06x}", n & 0xff_ffff));
            items.push(json!({
                "sku": sku,
                "name": p["name"].as_str().unwrap_or("Item"),
                "kind": kind(ptype),
                "image": image,
                "colorA": hex,
            }));
        }
        serde_json::to_string(&items).map_err(|e| e.to_string())
    }

    /// Register a hosted image URL as an external Rich Presence asset and return
    /// the `mp:`-prefixed media-proxy path Discord renders. Only http(s) URLs can
    /// be shown on a live profile — local uploads can't be pushed to Discord.
    pub fn external_asset(&self, app_id: &str, url: &str) -> Option<String> {
        if self.token.is_empty() || !url.starts_with("http") {
            return None;
        }
        let r = ureq::post(&format!(
            "https://discord.com/api/v9/applications/{app_id}/external-assets"
        ))
        .set("Authorization", &self.token)
        .set("User-Agent", CLIENT_UA)
        .set("X-Super-Properties", &quests::super_properties())
        .send_json(json!({ "urls": [url] }))
        .ok()?;
        let v: Value = r.into_json().ok()?;
        v.as_array()?
            .first()?
            .get("external_asset_path")?
            .as_str()
            .map(|s| format!("mp:{s}"))
    }

    /// The full collectibles catalog for the profile studio: every decoration,
    /// nameplate, effect and frame — owned or not, orb-priced or not — with the
    /// correct preview art per type (frames carry their composite layers).
    pub fn fetch_catalog(&self) -> Result<String, String> {
        if self.token.is_empty() {
            return Err("no token".into());
        }
        let r = ureq::get("https://discord.com/api/v9/collectibles-categories/v2")
            .set("Authorization", &self.token)
            .set("User-Agent", CLIENT_UA)
            .set("X-Super-Properties", &quests::super_properties())
            .set("X-Discord-Locale", "en-US")
            .call()
            .map_err(|e| e.to_string())?;
        let root: Value = r.into_json().map_err(|e| e.to_string())?;
        let kind = |t: i64| match t {
            0 => "deco",
            1 => "effect",
            2 => "nameplate",
            3 => "frame",
            _ => "other",
        };
        const CS: &str = "https://cdn.discordapp.com/media/v1/collectibles-shop";
        let mut items = Vec::new();
        for cat in root["categories"].as_array().into_iter().flatten() {
            let cat_name = cat["name"].as_str().unwrap_or("");
            for p in cat["products"].as_array().into_iter().flatten() {
                let ptype = p["type"].as_i64().unwrap_or(-1);
                if !(0..=3).contains(&ptype) {
                    continue; // skip bundles (1000) and Nitro credits (3000)
                }
                let sku = p["sku_id"].as_str().unwrap_or("");
                let item0 = &p["items"][0];
                // Preview art differs per collectible type.
                let image = match ptype {
                    1 => item0["thumbnailPreviewSrc"].as_str().map(str::to_string),
                    3 => item0["layers"][0]["id"]
                        .as_str()
                        .map(|lid| format!("{CS}/{sku}/{lid}/static.png")),
                    _ => Some(format!("{CS}/{sku}/static.png?size=256")),
                };
                // Frames are composited from anchored layers — carry them through.
                let layers = if ptype == 3 {
                    item0["layers"].as_array().map(|ls| {
                        ls.iter()
                            .filter_map(|l| {
                                let lid = l["id"].as_str()?;
                                Some(json!({
                                    "url": format!("{CS}/{sku}/{lid}/static.png"),
                                    "anchor": l["anchor"].as_str().unwrap_or("top"),
                                    "order": l["order"].as_str().unwrap_or("front"),
                                }))
                            })
                            .collect::<Vec<_>>()
                    })
                } else {
                    None
                };
                // Frame geometry: how far the frame extends past the profile on
                // each side, so the preview can inset the card into the frame's
                // window instead of laying the frame on top of it.
                let metrics = if ptype == 3 {
                    Some(json!({
                        "iw": item0["inner_width"].as_i64().unwrap_or(1200),
                        "ot": item0["overflow_top"].as_i64().unwrap_or(0),
                        "ob": item0["overflow_bottom"].as_i64().unwrap_or(0),
                        "oh": item0["overflow_horizontal"].as_i64().unwrap_or(0),
                    }))
                } else {
                    None
                };
                // Effects animate: carry the animated layer sources (APNG) so the
                // preview plays them instead of the static thumbnail.
                let anim = if ptype == 1 {
                    item0["effects"].as_array().map(|es| {
                        es.iter()
                            .filter_map(|e| e["src"].as_str().map(str::to_string))
                            .collect::<Vec<_>>()
                    })
                } else {
                    None
                };
                // Orb price if it has one (for a small "orb" tag; not a filter).
                let mut orbs = None;
                if let Some(tiers) = p["prices"].as_object() {
                    'o: for t in tiers.values() {
                        if let Some(ps) = t["country_prices"]["prices"].as_array() {
                            for pr in ps {
                                if pr["currency"].as_str() == Some("discord_orb") {
                                    orbs = pr["amount"].as_u64();
                                    break 'o;
                                }
                            }
                        }
                    }
                }
                items.push(json!({
                    "sku": sku,
                    "name": p["name"].as_str().unwrap_or("Item"),
                    "kind": kind(ptype),
                    "image": image,
                    "layers": layers,
                    "metrics": metrics,
                    "anim": anim,
                    "orbs": orbs,
                    "collection": cat_name,
                }));
            }
        }
        serde_json::to_string(&items).map_err(|e| e.to_string())
    }

    /// Fetch the collectibles shop and return every orb-purchasable, unowned item.
    pub fn fetch_shop(&self) -> Result<String, String> {
        if self.token.is_empty() {
            return Err("no token".into());
        }
        let owned = self.fetch_owned();
        let r = ureq::get("https://discord.com/api/v9/collectibles-categories/v2")
            .set("Authorization", &self.token)
            .set("User-Agent", CLIENT_UA)
            .set("X-Super-Properties", &quests::super_properties())
            .set("X-Discord-Locale", "en-US")
            .call()
            .map_err(|e| e.to_string())?;
        let root: Value = r.into_json().map_err(|e| e.to_string())?;

        let hex = |ints: &Value| -> Option<String> {
            ints.as_array()
                .and_then(|a| a.first())
                .and_then(|n| n.as_u64())
                .map(|n| format!("#{:06x}", n & 0xff_ffff))
        };
        let kind = |t: i64| match t {
            0 => "deco",
            1 => "effect",
            2 => "nameplate",
            3 => "frame",
            1000 => "bundle",
            _ => "other",
        };

        let mut items = Vec::new();
        if let Some(cats) = root["categories"].as_array() {
            for cat in cats {
                let rank: std::collections::HashMap<&str, usize> = cat["hero_ranking"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .enumerate()
                            .filter_map(|(i, v)| v.as_str().map(|s| (s, i)))
                            .collect()
                    })
                    .unwrap_or_default();
                let cat_name = cat["name"].as_str().unwrap_or("").to_string();
                if let Some(prods) = cat["products"].as_array() {
                    for p in prods {
                        // orb price (currency == discord_orb, exponent 0)
                        let mut orbs = None;
                        if let Some(tiers) = p["prices"].as_object() {
                            'outer: for tier in tiers.values() {
                                if let Some(ps) = tier["country_prices"]["prices"].as_array() {
                                    for pr in ps {
                                        if pr["currency"].as_str() == Some("discord_orb") {
                                            orbs = pr["amount"].as_u64();
                                            break 'outer;
                                        }
                                    }
                                }
                            }
                        }
                        let Some(orbs) = orbs else { continue };
                        let sku = p["sku_id"].as_str().unwrap_or("");
                        if owned.contains(sku) {
                            continue; // hide items the user already owns
                        }
                        let ptype = p["type"].as_i64().unwrap_or(-1);
                        // Real preview art (loads in the webview with browser headers).
                        let image = if ptype == 1000 {
                            p["preview_assets"]["bg_static"]
                                .as_str()
                                .or_else(|| p["preview_assets"]["fg_static"].as_str())
                                .map(str::to_string)
                        } else {
                            Some(format!(
                                "https://cdn.discordapp.com/media/v1/collectibles-shop/{sku}/static.png?size=256"
                            ))
                        };
                        items.push(json!({
                            "sku": sku,
                            "name": p["name"].as_str().unwrap_or("Item"),
                            "kind": kind(ptype),
                            "orbs": orbs,
                            "image": image,
                            "colorA": hex(&p["styles"]["background_colors"]),
                            "colorB": p["styles"]["background_colors"].as_array()
                                .and_then(|a| a.get(1)).and_then(|n| n.as_u64())
                                .map(|n| format!("#{:06x}", n & 0xff_ffff)),
                            "collection": cat_name,
                            "rank": rank.get(sku).copied(),
                        }));
                    }
                }
            }
        }
        serde_json::to_string(&items).map_err(|e| e.to_string())
    }

    /// Send one play heartbeat, mirroring the desktop client's game detection.
    /// `terminal=true` on the final beat. Returns (progress_seconds, completed).
    pub fn play_heartbeat(
        &self,
        quest_id: &str,
        app_id: &str,
        exe_path: &str,
        terminal: bool,
    ) -> Result<(u64, bool), String> {
        // A stable, plausible executable fingerprint (sha-like hex) per game.
        let fp: String = {
            let mut h: u64 = 1469598103934665603;
            for b in app_id.bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(1099511628211);
            }
            format!("{h:016x}{:016x}", h.rotate_left(32))
        };
        let body = json!({
            "stream_key": Value::Null,
            "application_id": app_id,
            "terminal": terminal,
            "executable_path": exe_path,
            "executable_fingerprint": fp,
        });
        let url = format!("https://discord.com/api/v9/quests/{quest_id}/heartbeat");
        match ureq::post(&url)
            .set("Authorization", &self.token)
            .set("User-Agent", CLIENT_UA)
            .set("X-Super-Properties", &quests::super_properties())
            .set("Content-Type", "application/json")
            .send_json(body)
        {
            Ok(r) => {
                let v: Value = r.into_json().unwrap_or(Value::Null);
                let prog = v["user_status"]["progress"]["PLAY_ON_DESKTOP"]["value"]
                    .as_u64()
                    .or_else(|| v["progress"]["PLAY_ON_DESKTOP"]["value"].as_u64())
                    .unwrap_or(0);
                let done = v["user_status"]["completed_at"].as_str().is_some()
                    || v["user_status"]["progress"]["PLAY_ON_DESKTOP"]["completed_at"].as_str().is_some();
                Ok((prog, done))
            }
            Err(ureq::Error::Status(c, r)) => {
                Err(format!("HTTP {c}: {}", r.into_string().unwrap_or_default()))
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Claim a completed quest's reward. Mirrors the desktop client's call:
    /// POST /quests/{id}/claim-reward with platform/location + sealed metadata.
    /// Returns the orb amount granted, when the response reports one.
    pub fn claim(&self, quest_id: &str, traffic_metadata_sealed: Option<&str>) -> Result<Option<u64>, String> {
        let url = format!("https://discord.com/api/v9/quests/{quest_id}/claim-reward");
        let body = json!({
            "platform": 0,
            "location": 11,
            "metadata_sealed": Value::Null,
            "traffic_metadata_sealed": traffic_metadata_sealed,
        });
        match ureq::post(&url)
            .set("Authorization", &self.token)
            .set("User-Agent", CLIENT_UA)
            .set("X-Super-Properties", &quests::super_properties())
            .set("Content-Type", "application/json")
            .send_json(body)
        {
            Ok(r) => {
                let v: Value = r.into_json().unwrap_or(Value::Null);
                Ok(v["user_status"]["orb_quantity_claimed"]
                    .as_u64()
                    .or_else(|| v["orb_quantity_claimed"].as_u64()))
            }
            Err(ureq::Error::Status(c, r)) => {
                let raw = r.into_string().unwrap_or_default();
                let parsed: Option<Value> = serde_json::from_str(&raw).ok();
                // Discord gates claiming behind an hCaptcha. We deliberately do
                // not solve or relay it — the user finishes that one in Discord.
                let captcha = parsed
                    .as_ref()
                    .map(|v| !v["captcha_key"].is_null() || !v["captcha_sitekey"].is_null())
                    .unwrap_or(false)
                    || raw.contains("captcha-required");
                if captcha {
                    return Err("CAPTCHA".into());
                }
                let msg = parsed
                    .and_then(|v| v["message"].as_str().map(str::to_string))
                    .unwrap_or(raw);
                Err(format!("HTTP {c}: {msg}"))
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Report one honest watch-time heartbeat; returns (progress_seconds, completed).
    /// `key` is the quest's own task (WATCH_VIDEO or WATCH_VIDEO_ON_MOBILE).
    pub fn video_progress(
        &self,
        quest_id: &str,
        key: &str,
        seconds: u64,
    ) -> Result<(u64, bool), String> {
        let url = format!("https://discord.com/api/v9/quests/{quest_id}/video-progress");
        let resp = ureq::post(&url)
            .set("Authorization", &self.token)
            .set("User-Agent", CLIENT_UA)
            .set("X-Super-Properties", &quests::super_properties())
            .set("Content-Type", "application/json")
            .send_json(json!({ "timestamp": seconds }));

        match resp {
            Ok(r) => {
                let v: Value = r.into_json().unwrap_or(Value::Null);
                let read = |root: &Value| root[key]["value"].as_u64();
                let progress = read(&v["user_status"]["progress"])
                    .or_else(|| read(&v["progress"]))
                    .unwrap_or(seconds);
                let completed = v["user_status"]["completed_at"].as_str().is_some()
                    || v["completed_at"].as_str().is_some()
                    || v["user_status"]["progress"][key]["completed_at"].as_str().is_some()
                    || v["progress"][key]["completed_at"].as_str().is_some();
                Ok((progress, completed))
            }
            Err(ureq::Error::Status(c, r)) => {
                Err(format!("HTTP {c}: {}", r.into_string().unwrap_or_default()))
            }
            Err(e) => Err(e.to_string()),
        }
    }
}
