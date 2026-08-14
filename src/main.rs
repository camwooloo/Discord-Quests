// Aurora Quests — a frameless WebView2 (tao + wry) desktop app for Discord
// Quests. The Rust side finds the local token, talks to Discord, and drives an
// HTML/CSS UI (see ui.rs); the front-end talks back over wry IPC.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod discord;
mod gateway;
mod quests;
mod settings;
mod stats;
mod studio;
mod update;
mod token;
mod ui;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

use discord::DiscordClient;

/// Events the front-end (or worker threads) send back to the main event loop.
enum UserEvent {
    Eval(String),
    Minimize,
    Hide,
    ShowWindow,
    Close,
    Drag,
    OpenExternal(String),
}

/// id -> (task_key, target_seconds) for the current quest set.
type Sessions = Arc<Mutex<HashMap<String, (String, u64)>>>;
/// id -> sealed traffic metadata, echoed back when claiming.
type Sealed = Arc<Mutex<HashMap<String, String>>>;
/// id -> (application_id, executable_path, target_seconds, game_name) for play quests.
type PlayInfo = Arc<Mutex<HashMap<String, (String, String, u64, String)>>>;
/// The single active play session's stop flag + quest id.
type Player = Arc<Mutex<Option<(String, Arc<AtomicBool>)>>>;

/// Heartbeat interval for play quests (seconds). Discord accrues playtime from
/// real elapsed time between beats, so a game quest still takes its full target.
const PLAY_BEAT_SECS: u64 = 25;

fn main() -> wry::Result<()> {
    // Headless helpers for quick checks / debugging (see functions below).
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--list") {
        run_cli();
        return Ok(());
    }
    if args.iter().any(|a| a == "--html") {
        run_html_preview();
        return Ok(());
    }

    run_gui()
}

/// Procedurally render the Aurora logo (rounded square, purple→green aurora
/// gradient, white four-point sparkle) as raw RGBA pixels + edge size.
fn icon_rgba() -> (Vec<u8>, u32) {
    let s = 128usize;
    let sf = s as f32;
    let (cx, cy) = (sf / 2.0, sf / 2.0);
    let margin = 8.0;
    let hw = sf / 2.0 - margin;
    let rad = 30.0;
    let spark_r = hw * 0.64;
    let (pr, pg, pb) = (183.0f32, 148.0, 246.0); // #b794f6
    let (tr, tg, tb) = (52.0f32, 211.0, 153.0); // #34d399
    let mut rgba = vec![0u8; s * s * 4];
    for y in 0..s {
        for x in 0..s {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // rounded-rect signed distance for a crisp, anti-aliased tile
            let (px, py) = ((fx - cx).abs(), (fy - cy).abs());
            let (qx, qy) = (px - (hw - rad), py - (hw - rad));
            let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt()
                + qx.max(qy).min(0.0)
                - rad;
            let bg_a = (0.5 - d).clamp(0.0, 1.0);
            if bg_a <= 0.0 {
                continue;
            }
            let t = ((fx + fy) / (2.0 * sf)).clamp(0.0, 1.0);
            let mut r = pr + (tr - pr) * t;
            let mut g = pg + (tg - pg) * t;
            let mut b = pb + (tb - pb) * t;
            // four-point sparkle via a concave superellipse
            let nx = ((fx - cx) / spark_r).abs();
            let ny = ((fy - cy) / spark_r).abs();
            let star = nx.powf(0.42) + ny.powf(0.42);
            let sp = (1.0 - (star - 1.0) * 6.0).clamp(0.0, 1.0);
            r += (255.0 - r) * sp;
            g += (255.0 - g) * sp;
            b += (255.0 - b) * sp;
            let i = (y * s + x) * 4;
            rgba[i] = r as u8;
            rgba[i + 1] = g as u8;
            rgba[i + 2] = b as u8;
            rgba[i + 3] = (bg_a * 255.0) as u8;
        }
    }
    (rgba, s as u32)
}

/// The Aurora logo as a tao window/taskbar icon.
fn app_icon() -> Option<tao::window::Icon> {
    let (rgba, s) = icon_rgba();
    tao::window::Icon::from_rgba(rgba, s, s).ok()
}

/// The Aurora logo as a system-tray icon.
fn tray_icon() -> Option<tray_icon::Icon> {
    let (rgba, s) = icon_rgba();
    tray_icon::Icon::from_rgba(rgba, s, s).ok()
}

/// Show a Windows toast, piggybacking on PowerShell's registered AppID so no
/// manifest/registration is required. Best-effort — silently ignores failures.
fn notify(title: &str, body: &str) {
    use tauri_winrt_notification::Toast;
    let _ = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(body)
        .show();
}

fn run_gui() -> wry::Result<()> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = WindowBuilder::new()
        .with_title("Aurora Quests")
        .with_decorations(false)
        .with_resizable(false)
        .with_window_icon(app_icon())
        .with_inner_size(LogicalSize::new(1160.0, 740.0))
        .build(&event_loop)
        .expect("create window");

    if std::env::args().any(|a| a == "--minimized") {
        window.set_minimized(true);
    }

    let client = Arc::new(DiscordClient::from_local());
    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));
    let sealed: Sealed = Arc::new(Mutex::new(HashMap::new()));
    let play_info: PlayInfo = Arc::new(Mutex::new(HashMap::new()));
    let player: Player = Arc::new(Mutex::new(None));
    // Holds the pending update's download URL between the check and the user's OK.
    let update_url: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    // Gateway presence — connects lazily on first "show game" request.
    let presence = Arc::new(gateway::Presence::new(
        token::find_token().unwrap_or_default(),
    ));

    // Background: clean up any leftover update, then check GitHub for a newer one.
    {
        let proxy = proxy.clone();
        let update_url = update_url.clone();
        std::thread::spawn(move || {
            update::cleanup();
            if let Some((ver, url, notes)) = update::check() {
                *update_url.lock().unwrap() = Some(url);
                let payload = serde_json::json!({ "version": ver, "notes": notes });
                let _ = proxy.send_event(UserEvent::Eval(format!(
                    "window.updateAvailable&&window.updateAvailable({payload})"
                )));
            }
        });
    }

    let handler = {
        let ctx = IpcCtx {
            proxy: proxy.clone(),
            client: client.clone(),
            sessions: sessions.clone(),
            sealed: sealed.clone(),
            play_info: play_info.clone(),
            player: player.clone(),
            presence: presence.clone(),
            update_url: update_url.clone(),
        };
        move |req: wry::http::Request<String>| {
            handle_ipc(req.body(), &ctx);
        }
    };

    let webview = WebViewBuilder::new()
        .with_html(ui::page_html())
        .with_ipc_handler(handler)
        .build(&window)?;

    // System tray: right-click menu (Show / Watch all / Play all / Quit) plus a
    // left-click to restore. Menu/click events are forwarded into the tao loop.
    let _tray = build_tray(&proxy);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(ev) => match ev {
                UserEvent::Eval(js) => {
                    let _ = webview.evaluate_script(&js);
                }
                UserEvent::Minimize => window.set_minimized(true),
                UserEvent::Hide => window.set_visible(false),
                UserEvent::ShowWindow => {
                    window.set_visible(true);
                    window.set_minimized(false);
                    window.set_focus();
                }
                UserEvent::Close => *control_flow = ControlFlow::Exit,
                UserEvent::Drag => {
                    let _ = window.drag_window();
                }
                UserEvent::OpenExternal(url) => open_external(&url),
            },
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}

/// Build the tray icon and wire its menu/click events back to the event loop.
/// Returns the `TrayIcon` handle, which must be kept alive for the tray to show.
fn build_tray(
    proxy: &tao::event_loop::EventLoopProxy<UserEvent>,
) -> Option<tray_icon::TrayIcon> {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let menu = Menu::new();
    let mi_show = MenuItem::new("Show Aurora Quests", true, None);
    let mi_watch = MenuItem::new("Watch all videos", true, None);
    let mi_play = MenuItem::new("Play all games", true, None);
    let mi_quit = MenuItem::new("Quit", true, None);
    menu.append_items(&[&mi_show, &mi_watch, &mi_play, &mi_quit]).ok()?;
    let (id_show, id_watch, id_play, id_quit) = (
        mi_show.id().clone(),
        mi_watch.id().clone(),
        mi_play.id().clone(),
        mi_quit.id().clone(),
    );

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Aurora Quests")
        .with_icon(tray_icon()?)
        .build()
        .ok()?;

    let p = proxy.clone();
    MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
        if ev.id == id_show {
            let _ = p.send_event(UserEvent::ShowWindow);
        } else if ev.id == id_watch {
            let _ = p.send_event(UserEvent::ShowWindow);
            let _ = p.send_event(UserEvent::Eval("window.watchAll&&watchAll()".into()));
        } else if ev.id == id_play {
            let _ = p.send_event(UserEvent::ShowWindow);
            let _ = p.send_event(UserEvent::Eval("window.playAll&&playAll()".into()));
        } else if ev.id == id_quit {
            let _ = p.send_event(UserEvent::Close);
        }
    }));

    let p = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = ev
        {
            let _ = p.send_event(UserEvent::ShowWindow);
        }
    }));

    Some(tray)
}

struct IpcCtx {
    proxy: tao::event_loop::EventLoopProxy<UserEvent>,
    client: Arc<DiscordClient>,
    sessions: Sessions,
    sealed: Sealed,
    play_info: PlayInfo,
    player: Player,
    presence: Arc<gateway::Presence>,
    update_url: Arc<Mutex<Option<String>>>,
}

fn handle_ipc(body: &str, ctx: &IpcCtx) {
    let proxy = &ctx.proxy;
    let client = &ctx.client;
    let sessions = &ctx.sessions;
    let sealed = &ctx.sealed;
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    match v["type"].as_str().unwrap_or("") {
        "minimize" => {
            // Hide to the tray if the user enabled it; otherwise minimize normally.
            if settings::load().minimize_to_tray {
                let _ = proxy.send_event(UserEvent::Hide);
            } else {
                let _ = proxy.send_event(UserEvent::Minimize);
            }
        }
        "notify" => {
            // Fire a desktop toast (only when the user opted in).
            if settings::load().notifications {
                let title = v["title"].as_str().unwrap_or("Aurora Quests").to_string();
                let body = v["body"].as_str().unwrap_or("").to_string();
                std::thread::spawn(move || notify(&title, &body));
            }
        }
        "applyUpdate" => {
            let url = ctx.update_url.lock().unwrap().clone();
            if let Some(url) = url {
                let proxy = proxy.clone();
                std::thread::spawn(move || match update::apply(&url) {
                    Ok(()) => {
                        update::restart();
                        let _ = proxy.send_event(UserEvent::Close);
                    }
                    Err(e) => {
                        let _ = proxy.send_event(UserEvent::Eval(format!(
                            "window.updateFailed&&window.updateFailed({})",
                            serde_json::to_string(&e).unwrap_or_else(|_| "\"error\"".into())
                        )));
                    }
                });
            }
        }
        "close" => {
            let _ = proxy.send_event(UserEvent::Close);
        }
        "drag" => {
            let _ = proxy.send_event(UserEvent::Drag);
        }
        "openDiscord" => {
            let _ = proxy.send_event(UserEvent::OpenExternal(
                "discord://-/discovery/quests".to_string(),
            ));
        }
        "openExternal" => {
            if let Some(url) = v["url"].as_str() {
                let _ = proxy.send_event(UserEvent::OpenExternal(url.to_string()));
            }
        }
        "playStart" => {
            if let Some(id) = v["id"].as_str() {
                start_play(id, ctx);
            }
        }
        "playStop" => {
            if let Some((_, stop)) = ctx.player.lock().unwrap().take() {
                stop.store(true, Ordering::SeqCst);
            }
            ctx.presence.clear();
        }
        "setPresence" => {
            // Custom Rich Presence. Text fields apply directly; image slots that
            // are hosted URLs are registered as external assets so they render on
            // the live profile (local uploads can only preview in-app).
            let atype = v["atype"].as_u64().unwrap_or(0);
            let name = v["name"].as_str().filter(|s| !s.is_empty()).unwrap_or("Aurora").to_string();
            let details = v["details"].as_str().filter(|s| !s.is_empty()).map(str::to_string);
            let state = v["state"].as_str().filter(|s| !s.is_empty()).map(str::to_string);
            let large_img = v["largeImg"].as_str().unwrap_or("").to_string();
            let large_text = v["largeText"].as_str().filter(|s| !s.is_empty()).map(str::to_string);
            let small_img = v["smallImg"].as_str().unwrap_or("").to_string();
            let small_text = v["smallText"].as_str().filter(|s| !s.is_empty()).map(str::to_string);
            let client = ctx.client.clone();
            let presence = ctx.presence.clone();
            std::thread::spawn(move || {
                // App bucket the external assets are registered under.
                const APP_ID: &str = "1173038813607530546";
                let mut act = serde_json::Map::new();
                act.insert("type".into(), Value::from(atype));
                act.insert("name".into(), Value::from(name));
                if let Some(d) = details { act.insert("details".into(), Value::from(d)); }
                if let Some(st) = state { act.insert("state".into(), Value::from(st)); }
                let mut assets = serde_json::Map::new();
                if let Some(r) = client.external_asset(APP_ID, &large_img) {
                    assets.insert("large_image".into(), Value::from(r));
                }
                if let Some(t) = large_text { assets.insert("large_text".into(), Value::from(t)); }
                if let Some(r) = client.external_asset(APP_ID, &small_img) {
                    assets.insert("small_image".into(), Value::from(r));
                }
                if let Some(t) = small_text { assets.insert("small_text".into(), Value::from(t)); }
                if assets.contains_key("large_image") || assets.contains_key("small_image") {
                    act.insert("application_id".into(), Value::from(APP_ID));
                }
                if !assets.is_empty() {
                    act.insert("assets".into(), Value::Object(assets));
                }
                presence.set_activity(Value::Object(act));
            });
        }
        "clearPresence" => ctx.presence.clear(),
        "saveImage" => {
            use base64::{engine::general_purpose::STANDARD, Engine as _};
            let name = v["name"].as_str().unwrap_or("aurora-image").to_string();
            let data = v["data"].as_str().unwrap_or("");
            if let Some(b64) = data.split(',').nth(1) {
                if let Ok(bytes) = STANDARD.decode(b64) {
                    let dir = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default())
                        .join("Downloads");
                    let _ = std::fs::create_dir_all(&dir);
                    let path = dir.join(format!("{name}.png"));
                    let _ = std::fs::write(&path, bytes);
                }
            }
        }
        "loadShop" => {
            let proxy = proxy.clone();
            let client = client.clone();
            std::thread::spawn(move || {
                let js = match client.fetch_shop() {
                    Ok(json) => format!("window.setShop({json})"),
                    Err(e) => format!(
                        "window.setShop(null,{})",
                        serde_json::to_string(&e).unwrap_or_else(|_| "\"error\"".into())
                    ),
                };
                let _ = proxy.send_event(UserEvent::Eval(js));
                if let Ok(owned) = client.fetch_owned_items() {
                    let _ = proxy.send_event(UserEvent::Eval(format!("window.setOwned({owned})")));
                }
            });
        }
        "loadCatalog" => {
            let proxy = proxy.clone();
            let client = client.clone();
            std::thread::spawn(move || {
                let js = match client.fetch_catalog() {
                    Ok(json) => format!("window.setCatalog({json})"),
                    Err(_) => "window.setCatalog([])".to_string(),
                };
                let _ = proxy.send_event(UserEvent::Eval(js));
            });
        }
        "saveStudio" => {
            let data = v["data"].clone();
            std::thread::spawn(move || studio::save(&data));
        }
        "setSetting" => {
            let key = v["key"].as_str().unwrap_or("").to_string();
            let val = v["value"].as_bool().unwrap_or(false);
            std::thread::spawn(move || {
                let mut s = settings::load();
                match key.as_str() {
                    "launch_on_startup" => s.launch_on_startup = val,
                    "auto_watch" => s.auto_watch = val,
                    "auto_play" => s.auto_play = val,
                    "show_presence" => s.show_presence = val,
                    "start_minimized" => s.start_minimized = val,
                    "splash_seen" => s.splash_seen = val,
                    "notifications" => s.notifications = val,
                    "minimize_to_tray" => s.minimize_to_tray = val,
                    _ => return,
                }
                settings::save(&s);
            });
        }
        "setSettingStr" => {
            let key = v["key"].as_str().unwrap_or("").to_string();
            let val = v["value"].as_str().unwrap_or("").to_string();
            std::thread::spawn(move || {
                let mut s = settings::load();
                match key.as_str() {
                    "default_page" => s.default_page = val,
                    "orb_goal" => s.orb_goal = val,
                    "theme" => s.theme = val,
                    "accent" => s.accent = val,
                    _ => return,
                }
                settings::save(&s);
            });
        }
        "stat" => {
            let orbs = v["orbs"].as_u64().unwrap_or(0);
            let seconds = v["seconds"].as_u64().unwrap_or(0);
            let proxy = proxy.clone();
            std::thread::spawn(move || {
                let mut st = stats::load();
                stats::add(&mut st, orbs, seconds);
                stats::save(&st);
                if let Ok(js) = serde_json::to_string(&st) {
                    let _ = proxy.send_event(UserEvent::Eval(format!("window.setStats({js})")));
                }
            });
        }
        "claim" => {
            let Some(id) = v["id"].as_str().map(str::to_string) else { return };
            let meta = sealed.lock().unwrap().get(&id).cloned();
            let proxy = proxy.clone();
            let client = client.clone();
            std::thread::spawn(move || {
                let idj = serde_json::to_string(&id).unwrap();
                let js = match client.claim(&id, meta.as_deref()) {
                    Ok(orbs) => {
                        if settings::load().notifications {
                            let body = match orbs {
                                Some(n) => format!("+{n} orbs claimed from Discord Quests"),
                                None => "Quest reward claimed".to_string(),
                            };
                            notify("Reward claimed", &body);
                        }
                        format!(
                            "window.claimResult({idj},true,{})",
                            orbs.map(|n| n.to_string()).unwrap_or_else(|| "null".into())
                        )
                    }
                    Err(e) => format!(
                        "window.claimResult({idj},false,{})",
                        serde_json::to_string(&e).unwrap_or_else(|_| "\"error\"".into())
                    ),
                };
                let _ = proxy.send_event(UserEvent::Eval(js));
            });
        }
        "ready" | "rescan" => {
            // Push saved settings alongside the quest list.
            let s = settings::load();
            if let Ok(js) = serde_json::to_string(&s) {
                let _ = proxy.send_event(UserEvent::Eval(format!("window.setSettings({js})")));
            }
            // Restore the remembered profile-studio look.
            let st = studio::load();
            if !st.is_null() {
                let _ = proxy.send_event(UserEvent::Eval(format!("window.setStudio({st})")));
            }
            // Update daily streak once and push all-time stats.
            {
                let mut st = stats::load();
                stats::touch_streak(&mut st);
                stats::save(&st);
                if let Ok(js) = serde_json::to_string(&st) {
                    let _ = proxy.send_event(UserEvent::Eval(format!("window.setStats({js})")));
                }
            }
            let proxy = proxy.clone();
            let client = client.clone();
            let sessions = sessions.clone();
            let sealed = sealed.clone();
            let play_info = ctx.play_info.clone();
            std::thread::spawn(move || {
              // Identity first (fast) so the welcome screen can play while quests load.
              if let Ok((name, avatar)) = client.fetch_me() {
                  let u = serde_json::json!({ "name": name, "avatar": avatar });
                  let _ = proxy.send_event(UserEvent::Eval(format!("window.setUser({u})")));
              } else {
                  let _ = proxy.send_event(UserEvent::Eval("window.setUser(null)".into()));
              }
              if let Ok(orbs) = client.fetch_orbs() {
                  let _ = proxy.send_event(UserEvent::Eval(format!("window.setOrbs({orbs})")));
              }
              if let Ok(profile) = client.fetch_profile() {
                  let _ = proxy.send_event(UserEvent::Eval(format!("window.setBadges({profile})")));
              }
              if let Ok(equipped) = client.fetch_equipped() {
                  let _ = proxy.send_event(UserEvent::Eval(format!("window.setEquipped({equipped})")));
              }
              match client.fetch_quests() {
                Ok(qs) => {
                    {
                        let mut map = sessions.lock().unwrap();
                        let mut meta = sealed.lock().unwrap();
                        let mut play = play_info.lock().unwrap();
                        map.clear();
                        meta.clear();
                        play.clear();
                        for q in &qs {
                            if let Some(k) = &q.primary_task {
                                map.insert(q.id.clone(), (k.clone(), q.target_seconds.unwrap_or(0)));
                            }
                            if let Some(t) = &q.traffic_metadata_sealed {
                                meta.insert(q.id.clone(), t.clone());
                            }
                            if let (Some(app), true) =
                                (&q.app_id, q.category == quests::Category::Game)
                            {
                                let exe = plausible_exe_path(&q.app_name);
                                let name = if q.app_name.is_empty() {
                                    q.name.clone()
                                } else {
                                    q.app_name.clone()
                                };
                                play.insert(
                                    q.id.clone(),
                                    (app.clone(), exe, q.target_seconds.unwrap_or(900), name),
                                );
                            }
                        }
                    }
                    let json = ui::quests_json(&qs);
                    let _ = proxy.send_event(UserEvent::Eval(format!("window.setQuests({json})")));
                }
                Err(e) => {
                    let _ = proxy.send_event(UserEvent::Eval(format!(
                        "window.setError({})",
                        serde_json::to_string(&e).unwrap_or_else(|_| "\"error\"".into())
                    )));
                }
              }
            });
        }
        "watch" => {
            if let Some(id) = v["id"].as_str() {
                let id = id.to_string();
                let client = client.clone();
                std::thread::spawn(move || {
                    let _ = client.enroll(&id);
                });
            }
        }
        "progress" => {
            let id = v["id"].as_str().unwrap_or("").to_string();
            let seconds = v["seconds"].as_u64().unwrap_or(0);
            if id.is_empty() {
                return;
            }
            let (key, target) = sessions
                .lock()
                .unwrap()
                .get(&id)
                .cloned()
                .unwrap_or_default();
            let proxy = proxy.clone();
            let client = client.clone();
            std::thread::spawn(move || match client.video_progress(&id, &key, seconds) {
                Ok((prog, completed)) => {
                    let done = completed || (target > 0 && prog >= target);
                    let _ = proxy.send_event(UserEvent::Eval(format!(
                        "window.updateProgress({},{prog},{target},{})",
                        serde_json::to_string(&id).unwrap(),
                        done
                    )));
                }
                Err(e) => {
                    let _ = proxy.send_event(UserEvent::Eval(format!(
                        "window.progressError({},{})",
                        serde_json::to_string(&id).unwrap(),
                        serde_json::to_string(&e).unwrap_or_else(|_| "\"error\"".into())
                    )));
                }
            });
        }
        _ => {}
    }
}

/// A plausible per-game executable path so heartbeats mirror a real client.
fn plausible_exe_path(game: &str) -> String {
    let clean: String = game
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let name = if clean.is_empty() { "Game".to_string() } else { clean };
    let exe = name.replace(' ', "");
    format!("C:\\Program Files\\{name}\\{exe}.exe")
}

/// Begin (or restart) a play session: heartbeat the quest on an interval until
/// its target playtime is reached, reporting progress back to the UI.
fn start_play(id: &str, ctx: &IpcCtx) {
    let Some((app_id, exe, target, name)) = ctx.play_info.lock().unwrap().get(id).cloned() else {
        let _ = ctx.proxy.send_event(UserEvent::Eval(format!(
            "window.playError({},{})",
            serde_json::to_string(id).unwrap(),
            "\"not a game quest\""
        )));
        return;
    };

    // Broadcast the game on the user's profile, if enabled.
    if settings::load().show_presence {
        ctx.presence.set(&name, &app_id);
    }

    // Stop any existing session and register the new one.
    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut p = ctx.player.lock().unwrap();
        if let Some((_, old)) = p.take() {
            old.store(true, Ordering::SeqCst);
        }
        *p = Some((id.to_string(), stop.clone()));
    }

    let id = id.to_string();
    let proxy = ctx.proxy.clone();
    let client = ctx.client.clone();
    let player = ctx.player.clone();
    let presence = ctx.presence.clone();
    let idj = serde_json::to_string(&id).unwrap();

    std::thread::spawn(move || {
        // Accept the quest first — heartbeats to an un-enrolled quest are rejected.
        let _ = client.enroll(&id);
        loop {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            match client.play_heartbeat(&id, &app_id, &exe, false) {
                Ok((prog, done)) => {
                    let finished = done || prog >= target;
                    let _ = proxy.send_event(UserEvent::Eval(format!(
                        "window.playProgress({idj},{prog},{target},{finished})"
                    )));
                    if finished {
                        let _ = client.play_heartbeat(&id, &app_id, &exe, true);
                        // Clear first; if auto-play starts a next game its set() wins.
                        presence.clear();
                        let _ = proxy.send_event(UserEvent::Eval(format!("window.playDone({idj})")));
                        break;
                    }
                }
                Err(e) => {
                    let _ = proxy.send_event(UserEvent::Eval(format!(
                        "window.playError({idj},{})",
                        serde_json::to_string(&e).unwrap_or_else(|_| "\"error\"".into())
                    )));
                }
            }
            // Sleep in 1s slices so a stop is responsive.
            for _ in 0..PLAY_BEAT_SECS {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        // Clear ourselves if we're still the active session.
        let mut p = player.lock().unwrap();
        if p.as_ref().map(|(pid, _)| pid == &id).unwrap_or(false) {
            *p = None;
        }
    });
}

fn open_external(url: &str) {
    use std::os::windows::process::CommandExt;
    // http(s) links open in the browser; discord:// hands off to the client.
    // CREATE_NO_WINDOW avoids the console flash the old `cmd /C start` caused.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("discord://") {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

/// `--html`: render the page with live quest data to a temp file (for design
/// preview in a normal browser; IPC actions are inert there).
fn run_html_preview() {
    let quests = quests::scan().unwrap_or_default();
    let json = ui::quests_json(&quests);
    let client = DiscordClient::from_local();
    let shop = client.fetch_shop().unwrap_or_else(|_| "[]".into());
    let catalog = client.fetch_catalog().unwrap_or_else(|_| "[]".into());
    let orbs = client.fetch_orbs().unwrap_or(0);
    let prof = client.fetch_profile().map(|p| p.to_string()).unwrap_or_else(|_| "null".into());
    let equipped = client.fetch_equipped().unwrap_or_else(|_| "null".into());
    let page = ui::page_html().replace(
        "/*BOOTSTRAP*/",
        &format!("window.setUser({{name:'Aurora',avatar:null}});window.setOrbs({orbs});window.setBadges({prof});window.setQuests({json});window.setShop({shop});window.setCatalog({catalog});window.setEquipped({equipped});"),
    );
    let out = std::env::temp_dir().join("aurora_quests_preview.html");
    let _ = std::fs::write(&out, page);
    println!("{}", out.display());
}

/// `--list`: print quests to stdout (debug build keeps a console).
fn run_cli() {
    match quests::scan() {
        Err(e) => eprintln!("error: {e}"),
        Ok(all) => {
            for (label, cat) in [
                ("WATCH VIDEOS", quests::Category::Video),
                ("PLAY GAMES", quests::Category::Game),
            ] {
                println!("\n=== {label} ===");
                for q in all.iter().filter(|q| q.category == cat) {
                    let orb = if q.has_orb { "  [ORBS]" } else { "" };
                    println!("  • {}{}", q.name, orb);
                }
            }
        }
    }
}
