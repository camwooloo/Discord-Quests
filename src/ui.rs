// The entire front-end: one self-contained HTML/CSS/JS document rendered inside
// the frameless WebView2 window. Data flows in via `window.setQuests(...)`
// (pushed from Rust), and actions flow out via `window.ipc.postMessage(...)`.

use serde_json::json;

use crate::quests::{self, Category, Quest};

/// Serialize quests into the JSON array the front-end consumes.
pub fn quests_json(quests: &[Quest]) -> String {
    let arr: Vec<_> = quests
        .iter()
        .enumerate()
        .map(|(idx, q)| {
            let orbs = q.rewards.iter().find_map(|r| r.orb_quantity);
            let premium = q.rewards.iter().find_map(|r| r.premium_orb_quantity);
            let reward_name = q.rewards.first().map(|r| r.name.clone());
            let mobile_only = q.category == Category::Video
                && !q.tasks.iter().any(|t| t == "WATCH_VIDEO")
                && q.tasks.iter().any(|t| t == "WATCH_VIDEO_ON_MOBILE");
            json!({
                "idx": idx,
                "id": q.id,
                "name": q.name,
                "app": q.app_name,
                "category": match q.category { Category::Video => "video", Category::Game => "game" },
                "orbs": orbs,
                "premiumOrbs": premium,
                "reward": reward_name,
                "expiry": quests::pretty_expiry(&q.expires_at),
                "expiresAt": q.expires_at,
                "startsAt": q.starts_at,
                "video": q.video_url,
                "thumb": q.thumb_url,
                "taskKey": q.primary_task,
                "appId": q.app_id,
                "target": q.target_seconds,
                "progress": q.progress_seconds,
                "enrolled": q.enrolled,
                "completed": q.completed,
                "claimed": q.claimed,
                "expired": q.expired,
                "mobileOnly": mobile_only,
                "cta": q.cta_link,
                "ctaLabel": q.cta_label,
            })
        })
        .collect();
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into())
}

/// The full front-end document.
pub fn page_html() -> String {
    HTML.to_string()
}

const HTML: &str = r####"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Aurora Quests</title>
<style>
/* =========================================================================
   Aurora Quests — design system aligned with Aurora Launcher
   (glass surfaces, aurora purple accent, spring motion, 88px rail)
   ========================================================================= */
:root{
  color-scheme: dark;
  --accent:#b794f6; --accent-2:#34d399; --accent-rgb:183,148,246;
  --bg-0:#05060d; --bg-1:#0a0c18;
  --text:#eef1fa; --text-dim:#a6acc4; --text-mute:#6b7290;
  --glass:rgba(255,255,255,.05); --glass-2:rgba(255,255,255,.085); --glass-hi:rgba(255,255,255,.12);
  --stroke:rgba(255,255,255,.10); --stroke-2:rgba(255,255,255,.20); --hairline:rgba(255,255,255,.08);
  --shadow:0 30px 80px -28px rgba(0,0,0,.7);
  --on-accent:#0b0616;
  --radius:26px; --radius-sm:16px; --rail-w:88px;
  --ease:cubic-bezier(.22,.9,.26,1); --spring:cubic-bezier(.34,1.56,.64,1);
  --ok:#34d399; --warn:#f3c969; --danger:#ff6b6b;
}
:root[data-theme="light"]{
  color-scheme:light;
  --bg-0:#eef1f8; --bg-1:#e3e8f5;
  --text:#10131f; --text-dim:#414863; --text-mute:#6b7290; --faint:#8890a8;
  --glass:rgba(20,26,48,.045); --glass-2:rgba(20,26,48,.075); --glass-hi:rgba(20,26,48,.11);
  --stroke:rgba(20,26,48,.12); --stroke-2:rgba(20,26,48,.2); --hairline:rgba(20,26,48,.08);
  --shadow:0 24px 60px -30px rgba(40,50,90,.35);
}
*{box-sizing:border-box;margin:0;padding:0}
html,body{width:100%;height:100%;overflow:hidden}
body{
  font-family:"Geist","Segoe UI Variable Text","Segoe UI",system-ui,sans-serif;
  color:var(--text); background:var(--bg-0);
  -webkit-font-smoothing:antialiased; user-select:none; cursor:default;
  font-size:14px;
}
button{font-family:inherit;cursor:pointer;color:inherit;border:0;background:none}
input{font-family:inherit}
svg{display:block;flex:none}

#app{position:relative;height:100vh;display:flex;overflow:hidden;background:var(--bg-0)}
/* aurora blooms */
.bg{position:absolute;inset:0;pointer-events:none;overflow:hidden}
.bg::before,.bg::after{content:"";position:absolute;border-radius:50%;filter:blur(90px);opacity:.5}
.bg::before{width:820px;height:820px;left:-260px;top:-360px;
  background:radial-gradient(circle,rgba(var(--accent-rgb),.42),transparent 68%);animation:float1 24s ease-in-out infinite}
.bg::after{width:760px;height:760px;right:-240px;bottom:-340px;
  background:radial-gradient(circle,rgba(52,211,153,.24),transparent 68%);animation:float2 28s ease-in-out infinite}
@keyframes float1{0%,100%{transform:translate(0,0)}50%{transform:translate(70px,50px)}}
@keyframes float2{0%,100%{transform:translate(0,0)}50%{transform:translate(-60px,-45px)}}

/* ---------------- rail ---------------- */
.rail{width:var(--rail-w);flex:none;display:flex;flex-direction:column;align-items:center;
  gap:10px;padding:14px 0 18px;position:relative;z-index:3;
  background:linear-gradient(180deg,rgba(255,255,255,.045),rgba(255,255,255,.015));
  border-right:1px solid var(--hairline);backdrop-filter:blur(20px)}
.mark{width:38px;height:38px;border-radius:13px;display:grid;place-items:center;margin-bottom:12px;
  background:linear-gradient(140deg,var(--accent),var(--accent-2));
  box-shadow:0 8px 24px -6px rgba(var(--accent-rgb),.7)}
.navbtn{position:relative;width:52px;height:48px;border-radius:15px;display:grid;place-items:center;
  color:var(--text-mute);transition:color .22s var(--ease),background .22s var(--ease)}
.navbtn:hover{color:var(--text);background:var(--glass)}
.navbtn.active{color:var(--text);background:var(--glass-2)}
.navbtn.active::before{content:"";position:absolute;left:-14px;top:50%;transform:translateY(-50%);
  width:3px;height:22px;border-radius:0 3px 3px 0;background:linear-gradient(180deg,var(--accent),var(--accent-2));
  box-shadow:0 0 12px rgba(var(--accent-rgb),.9);animation:pop .34s var(--spring)}
@keyframes pop{from{height:4px;opacity:0}to{height:22px;opacity:1}}
.navbtn .tip{position:absolute;left:60px;white-space:nowrap;background:rgba(12,14,26,.96);border:1px solid var(--stroke);
  padding:7px 11px;border-radius:11px;font-size:12.5px;font-weight:600;opacity:0;pointer-events:none;transform:translateX(-6px);
  transition:.18s var(--ease);z-index:40;box-shadow:var(--shadow)}
.navbtn:hover .tip{opacity:1;transform:none}
.navbtn .cnt{position:absolute;top:5px;right:5px;min-width:17px;height:17px;padding:0 4px;border-radius:9px;
  background:var(--accent);color:var(--on-accent);font-size:10.5px;font-weight:800;display:grid;place-items:center}
.navbtn .cnt:empty{display:none}
.rail .sp{flex:1}

/* ---------------- main ---------------- */
main{flex:1;display:flex;flex-direction:column;min-width:0;position:relative;z-index:2}
#titlebar{height:56px;flex:none;display:flex;align-items:center;gap:12px;padding:0 12px 0 24px}
#titlebar h1{font-size:17px;font-weight:700;letter-spacing:-.2px;
  font-family:"Bricolage Grotesque","Segoe UI Variable Display",system-ui,sans-serif}
#titlebar .sub{color:var(--text-mute);font-size:12.5px;font-weight:600}
.tb-grow{flex:1;align-self:stretch}
.winbtn{width:44px;height:34px;border-radius:11px;display:grid;place-items:center;color:var(--text-dim);
  transition:.16s var(--ease)}
.winbtn:hover{background:var(--glass-2);color:var(--text)}
.winbtn.close:hover{background:#e5484d;color:#fff}

/* playing pill */
.pill{display:none;align-items:center;gap:9px;height:34px;padding:0 13px;border-radius:12px;
  background:var(--glass-2);border:1px solid rgba(var(--accent-rgb),.4);color:var(--text);
  font-size:12.5px;font-weight:650;max-width:290px;transition:.18s var(--ease)}
.pill.on{display:flex}
.pill:hover{background:var(--glass-hi);border-color:rgba(var(--accent-rgb),.75)}
.pill .nm{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:150px}
.pill .eq{display:flex;align-items:flex-end;gap:2px;height:13px}
.pill .eq i{width:2.5px;background:linear-gradient(180deg,var(--accent),var(--accent-2));border-radius:2px;animation:eq .9s ease-in-out infinite}
.pill .eq i:nth-child(2){animation-delay:.15s}.pill .eq i:nth-child(3){animation-delay:.3s}
@keyframes eq{0%,100%{height:4px}50%{height:13px}}
.pill .tm{color:var(--text-mute);font-variant-numeric:tabular-nums}

/* ---------------- toolbar ---------------- */
.toolbar{flex:none;display:flex;align-items:center;gap:10px;padding:2px 24px 16px}
.ctl{height:40px;display:flex;align-items:center;gap:9px;padding:0 14px;border-radius:14px;
  background:var(--glass);border:1px solid var(--stroke);color:var(--text-dim);
  font-size:13px;font-weight:650;transition:.18s var(--ease);white-space:nowrap}
.ctl:hover{background:var(--glass-2);color:var(--text)}
.ctl.on{color:var(--text);border-color:rgba(var(--accent-rgb),.55);background:rgba(var(--accent-rgb),.14)}
.ctl.icon{width:40px;padding:0;justify-content:center}
.search{flex:0 1 300px;min-width:170px}
.search input{flex:1;min-width:0;background:none;border:0;outline:0;color:var(--text);font-size:13px;font-weight:500}
.search input::placeholder{color:var(--text-mute);font-weight:500}
.tb-spacer{flex:1}
.orb-dot{width:15px;height:15px;border-radius:50%;flex:none;
  background:radial-gradient(circle at 34% 30%,#efe6ff,var(--accent));box-shadow:0 0 10px rgba(var(--accent-rgb),.75)}

/* dropdown */
.dd{position:relative}
.menu{position:absolute;top:46px;left:0;min-width:210px;padding:7px;border-radius:var(--radius-sm);
  background:rgba(13,16,30,.97);border:1px solid var(--stroke);box-shadow:var(--shadow);z-index:50;
  display:none;flex-direction:column;gap:2px;backdrop-filter:blur(24px)}
.menu.open{display:flex;animation:menuIn .2s var(--ease)}
@keyframes menuIn{from{opacity:0;transform:translateY(-7px)}to{opacity:1;transform:none}}
.menu .lbl{padding:8px 12px 5px;color:var(--text-mute);font-size:11px;font-weight:800;letter-spacing:.8px;text-transform:uppercase}
.mi{display:flex;align-items:center;gap:11px;padding:10px 12px;border-radius:11px;color:var(--text-dim);
  font-size:13.5px;font-weight:600;transition:.14s var(--ease);text-align:left;width:100%}
.mi:hover{background:var(--glass-2);color:var(--text)}
.mi .radio{width:16px;height:16px;border-radius:50%;border:2px solid var(--text-mute);flex:none;display:grid;place-items:center}
.mi.sel{color:var(--text)}
.mi.sel .radio{border-color:var(--accent)}
.mi.sel .radio::after{content:"";width:8px;height:8px;border-radius:50%;background:var(--accent)}

/* ---------------- content ---------------- */
#content{flex:1;overflow-y:auto;overflow-x:hidden;padding:0 24px 26px;scrollbar-gutter:stable}
#content::-webkit-scrollbar{width:11px}
#content::-webkit-scrollbar-thumb{background:rgba(255,255,255,.13);border-radius:10px;border:3px solid transparent;background-clip:content-box}
#content::-webkit-scrollbar-thumb:hover{background:rgba(255,255,255,.22);background-clip:content-box}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(296px,1fr));gap:18px;
  grid-auto-rows:1fr}  /* equal row heights → every card is the same size */

.card{position:relative;display:flex;flex-direction:column;height:100%;border-radius:var(--radius);overflow:hidden;
  background:var(--glass);border:1px solid var(--stroke);backdrop-filter:blur(18px);
  transition:transform .3s var(--ease),border-color .3s var(--ease),box-shadow .3s var(--ease)}
.card:hover{transform:translateY(-4px);border-color:var(--stroke-2);box-shadow:var(--shadow)}
.thumb{position:relative;aspect-ratio:16/9;background:#080a14 center/cover no-repeat;flex:none}
.thumb::after{content:"";position:absolute;inset:0;background:linear-gradient(180deg,rgba(5,6,13,.15) 45%,rgba(5,6,13,.92))}
.tag{position:absolute;top:12px;left:12px;z-index:2;display:flex;align-items:center;gap:6px;height:26px;padding:0 10px;
  border-radius:9px;background:rgba(5,6,13,.62);border:1px solid var(--stroke);backdrop-filter:blur(10px);
  font-size:10.5px;font-weight:800;letter-spacing:.7px;text-transform:uppercase;color:var(--text-dim)}
.orbchip{position:absolute;top:12px;right:12px;z-index:2;display:flex;align-items:center;gap:7px;height:28px;padding:0 11px;
  border-radius:10px;background:rgba(5,6,13,.62);border:1px solid rgba(var(--accent-rgb),.45);
  backdrop-filter:blur(10px);font-size:13px;font-weight:750;color:#f2ebff}
.mult{font-size:10.5px;font-weight:800;color:var(--accent);background:rgba(var(--accent-rgb),.2);
  padding:2px 6px;border-radius:6px;border:1px solid rgba(var(--accent-rgb),.35)}
.body{display:flex;flex-direction:column;gap:9px;padding:15px 16px 16px;flex:1}
.name{font-size:15.5px;font-weight:700;line-height:1.3;letter-spacing:-.15px;
  display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden;min-height:40px}
.row{display:flex;align-items:center;gap:8px;color:var(--text-mute);font-size:12.5px;font-weight:550;line-height:1}
.row svg{opacity:.75}
.bar{height:7px;border-radius:5px;background:rgba(255,255,255,.07);overflow:hidden}
.bar>i{display:block;height:100%;width:0;border-radius:5px;
  background:linear-gradient(90deg,var(--accent),var(--accent-2));transition:width .5s var(--ease)}
.bar.done>i{background:linear-gradient(90deg,var(--ok),var(--accent-2))}
.grow{flex:1}
.act{display:flex;align-items:center;justify-content:center;gap:9px;height:42px;border-radius:14px;
  font-size:13.5px;font-weight:750;letter-spacing:-.1px;transition:.2s var(--ease);width:100%}
.act.primary{background:linear-gradient(135deg,var(--accent),#8b6ee0);color:var(--on-accent);
  box-shadow:0 10px 26px -12px rgba(var(--accent-rgb),.9)}
.act.primary:hover{filter:brightness(1.08);transform:translateY(-1px)}
.act.claim{background:linear-gradient(135deg,var(--accent-2),#2bb583);color:#04120c;
  box-shadow:0 10px 26px -12px rgba(52,211,153,.9)}
.act.claim:hover{filter:brightness(1.08);transform:translateY(-1px)}
.act.play{background:linear-gradient(135deg,#5b8cff,#8b6ee0);color:#fff;
  box-shadow:0 10px 26px -12px rgba(91,140,255,.85)}
.act.play:hover{filter:brightness(1.08);transform:translateY(-1px)}
.act.ghost{background:var(--glass-2);border:1px solid var(--stroke);color:var(--text)}
.act.ghost:hover{background:var(--glass-hi)}
.act:disabled{opacity:.5;transform:none;filter:none;cursor:default}
.state{display:flex;align-items:center;justify-content:center;gap:8px;height:42px;color:var(--ok);font-size:13px;font-weight:700}
.state.mute{color:var(--text-mute);font-weight:600}

/* empty / loading */
.mid{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:12px;
  min-height:56vh;text-align:center;padding:0 30px}
.mid .ic{width:60px;height:60px;border-radius:20px;display:grid;place-items:center;
  background:var(--glass-2);border:1px solid var(--stroke);color:var(--text-mute);margin-bottom:2px}
.mid h2{font-size:16.5px;font-weight:700}
.mid p{color:var(--text-mute);font-size:13.5px;max-width:430px;line-height:1.6;font-weight:500}
.spin{width:34px;height:34px;border-radius:50%;border:3px solid rgba(255,255,255,.11);
  border-top-color:var(--accent);animation:spin .9s linear infinite}
@keyframes spin{to{transform:rotate(360deg)}}

/* ---------------- settings ---------------- */
.settings{max-width:660px;display:flex;flex-direction:column;gap:12px;padding-bottom:10px}
.sgroup{border-radius:var(--radius);background:var(--glass);border:1px solid var(--stroke);
  backdrop-filter:blur(18px);overflow:hidden}
.sgroup h3{padding:16px 20px 6px;font-size:11.5px;font-weight:800;letter-spacing:.9px;
  text-transform:uppercase;color:var(--text-mute)}
.srow{display:flex;align-items:center;gap:16px;padding:15px 20px;border-top:1px solid var(--hairline)}
.sgroup h3+.srow{border-top:0}
.srow .txt{flex:1;min-width:0}
.srow .t{font-size:14px;font-weight:650;margin-bottom:3px}
.srow .d{font-size:12.5px;color:var(--text-mute);line-height:1.5;font-weight:500}
.sw{width:46px;height:27px;border-radius:99px;background:rgba(255,255,255,.11);border:1px solid var(--stroke);
  flex:none;position:relative;transition:.26s var(--ease)}
.sw::after{content:"";position:absolute;top:2px;left:2px;width:19px;height:19px;border-radius:50%;
  background:var(--text-dim);transition:.26s var(--spring)}
.sw.on{background:rgba(var(--accent-rgb),.34);border-color:rgba(var(--accent-rgb),.6)}
.sw.on::after{left:22px;background:linear-gradient(140deg,var(--accent),var(--accent-2))}

/* ---------------- hidden player dock ---------------- */
.dock{position:fixed;right:22px;bottom:22px;width:352px;z-index:60;border-radius:20px;overflow:hidden;
  background:rgba(10,12,24,.97);border:1px solid var(--stroke);box-shadow:var(--shadow);backdrop-filter:blur(24px);
  transform:translateY(calc(100% + 40px));opacity:0;pointer-events:none;
  transition:transform .5s var(--spring),opacity .3s var(--ease)}
.dock.show{transform:none;opacity:1;pointer-events:auto}
.dock .dhead{display:flex;align-items:center;gap:10px;padding:12px 13px}
.dock .dhead .dn{flex:1;min-width:0;font-size:13px;font-weight:700;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.dock video{width:100%;display:block;background:#000;aspect-ratio:16/9;object-fit:contain}
.dock .dfoot{padding:12px 13px 13px;display:flex;flex-direction:column;gap:8px}
.dock .dst{font-size:12px;color:var(--text-mute);font-weight:600;display:flex;justify-content:space-between;gap:8px}
.minibtn{width:30px;height:30px;border-radius:10px;display:grid;place-items:center;color:var(--text-dim);
  background:var(--glass-2);transition:.16s var(--ease);flex:none}
.minibtn:hover{background:var(--glass-hi);color:var(--text)}
.minibtn.danger:hover{background:#e5484d;color:#fff}
/* the video keeps playing while the dock is hidden (it stays laid out) */

.toast{position:fixed;left:50%;bottom:26px;transform:translate(-50%,20px);z-index:80;opacity:0;
  display:flex;align-items:center;gap:10px;padding:12px 18px;border-radius:14px;font-size:13.5px;font-weight:650;
  background:rgba(13,16,30,.98);border:1px solid var(--stroke);box-shadow:var(--shadow);
  transition:.3s var(--ease);pointer-events:none;max-width:76vw}
.toast.show{opacity:1;transform:translate(-50%,0)}
.toast.ok{border-color:rgba(52,211,153,.5)} .toast.bad{border-color:rgba(255,107,107,.5)}

/* ---------------- boot / welcome ---------------- */
.boot{position:fixed;inset:0;z-index:100;display:grid;place-items:center;
  background:radial-gradient(900px 700px at 50% 40%,rgba(var(--accent-rgb),.16),transparent 60%),var(--bg-0);
  transition:opacity .6s var(--ease)}
.boot.hide{opacity:0;pointer-events:none}
.boot-load{display:flex;flex-direction:column;align-items:center;gap:20px;text-align:center;
  animation:fadeUp .5s var(--ease)}
.boot-mark{width:66px;height:66px;border-radius:22px;box-shadow:0 14px 40px -8px rgba(var(--accent-rgb),.75);
  animation:breathe 3s ease-in-out infinite}
@keyframes breathe{0%,100%{transform:translateY(0) scale(1)}50%{transform:translateY(-4px) scale(1.03)}}
.boot-msg{color:var(--text-dim);font-size:14px;font-weight:600;letter-spacing:.2px}
.boot-welcome{display:none;flex-direction:column;align-items:center;gap:14px;text-align:center}
.boot.welcome .boot-load{display:none}
.boot.welcome .boot-welcome{display:flex}
.wav-wrap{position:relative;width:132px;height:132px;border-radius:50%;display:grid;place-items:center}
.wav-wrap::before{content:"";position:absolute;inset:-10px;border-radius:50%;
  background:conic-gradient(from 0deg,var(--accent),var(--accent-2),var(--accent));filter:blur(14px);opacity:.85;
  animation:spin 6s linear infinite}
.wav,.wav-fb{position:relative;width:120px;height:120px;border-radius:50%;object-fit:cover;
  border:3px solid rgba(255,255,255,.14);background:var(--bg-1);box-shadow:var(--shadow);z-index:1;
  transform-origin:center;will-change:transform}
.wav-fb{display:none;place-items:center;font-size:46px;font-weight:800;color:var(--accent);
  font-family:"Bricolage Grotesque",system-ui}
.boot-welcome.noimg .wav{display:none}
.boot-welcome.noimg .wav-fb{display:grid}
.wtext{color:var(--text-dim);font-size:15px;font-weight:600;opacity:0;animation:fadeUp .5s var(--ease) .25s forwards}
.wname{font-size:30px;font-weight:800;letter-spacing:-.5px;opacity:0;
  font-family:"Bricolage Grotesque","Segoe UI Variable Display",system-ui;
  background:linear-gradient(120deg,#fff,var(--accent));-webkit-background-clip:text;background-clip:text;-webkit-text-fill-color:transparent;
  animation:fadeUp .55s var(--ease) .4s forwards}
@keyframes fadeUp{from{opacity:0;transform:translateY(10px)}to{opacity:1;transform:none}}

/* orb balance chip (bottom, above avatar) */
.orbbal{display:none;flex-direction:column;align-items:center;gap:3px;margin-bottom:8px;
  color:#efe6ff;font-size:11px;font-weight:800;line-height:1;text-align:center}
.orbbal.show{display:flex}
.orbbal .orb-dot{width:16px;height:16px;margin-bottom:1px}
#orbBalNum{font-size:11px;letter-spacing:.2px}

/* rail avatar (bottom, above settings) */
.ravatar{position:relative;width:44px;height:44px;border-radius:50%;overflow:hidden;flex:none;margin-bottom:4px;
  border:2px solid rgba(255,255,255,.14);background:var(--bg-1);display:none;place-items:center;
  box-shadow:0 6px 18px -6px rgba(0,0,0,.7)}
.ravatar.show{display:grid}
.ravatar img{width:100%;height:100%;object-fit:cover}
.ravatar>span:not(.ravatar-gear){font-size:17px;font-weight:800;color:var(--accent)}
.ravatar-gear{position:absolute;inset:0;display:grid;place-items:center;color:#fff;
  background:rgba(5,6,13,.62);opacity:0;transition:opacity .16s var(--ease)}
.ravatar:hover .ravatar-gear,.ravatar.active .ravatar-gear{opacity:1}
.ravatar.active{border-color:rgba(var(--accent-rgb),.7)}
.ravatar::after{content:"";position:absolute;inset:-3px;border-radius:50%;border:2px solid transparent;
  background:linear-gradient(var(--bg-0),var(--bg-0)) padding-box,linear-gradient(140deg,var(--accent),var(--accent-2)) border-box;
  -webkit-mask:linear-gradient(#000 0 0) padding-box,linear-gradient(#000 0 0);pointer-events:none;opacity:.0;transition:.3s}
.ravatar:hover::after{opacity:1}

/* content reveal after welcome */
#app main,#app .rail .navbtn,#app .rail .mark{opacity:1}
#app.intro main{opacity:0;transform:translateY(10px)}
#app.intro .rail .navbtn{opacity:0;transform:translateX(-12px)}
#app.reveal main{opacity:1;transform:none;transition:opacity .6s var(--ease) .05s,transform .6s var(--ease) .05s}
#app.reveal .rail .navbtn{opacity:1;transform:none;transition:.5s var(--spring)}
#app.reveal .rail .navbtn:nth-child(2){transition-delay:.06s}
#app.reveal .rail .navbtn:nth-child(3){transition-delay:.12s}
#app.reveal .rail .navbtn:nth-child(4){transition-delay:.18s}

/* ---------------- badges + tooltips ---------------- */
#tt{position:fixed;z-index:200;max-width:240px;background:rgba(10,12,22,.98);color:var(--text);
  border:1px solid var(--stroke);border-radius:10px;padding:8px 11px;font-size:12px;font-weight:600;line-height:1.4;
  box-shadow:var(--shadow);pointer-events:none;opacity:0;transform:translateY(4px);transition:opacity .12s,transform .12s}
#tt.show{opacity:1;transform:none}
.splashwrap{position:fixed;inset:0;z-index:300;display:none;align-items:center;justify-content:center;
  background:rgba(4,6,12,.72);backdrop-filter:blur(10px)}
.splashwrap.show{display:flex}
.splashcard{width:min(460px,92vw);background:linear-gradient(180deg,#131a2b,#0c1120);border:1px solid var(--stroke);
  border-radius:20px;padding:26px 26px 24px;box-shadow:var(--shadow);text-align:center;animation:fadeUp .4s var(--ease)}
.splashcard h2{font-size:20px;font-weight:800;margin:12px 0 10px;font-family:"Bricolage Grotesque",system-ui}
.splashcard p{color:var(--text-dim);font-size:13.5px;line-height:1.6;margin-bottom:12px}
.splashcard .warn-p{color:#f3c969;background:rgba(243,201,105,.08);border:1px solid rgba(243,201,105,.25);
  border-radius:12px;padding:11px 13px;font-size:12.5px}
.badges{display:flex;flex-direction:column;gap:22px}
.bsec-h{font-size:13px;font-weight:800;letter-spacing:.6px;text-transform:uppercase;color:var(--text-mute);margin-bottom:12px}
.bsec-h span{display:inline-grid;place-items:center;min-width:20px;height:20px;padding:0 6px;border-radius:10px;
  background:rgba(var(--accent-rgb),.2);color:var(--accent);font-size:11px;margin-left:6px}
.bgrid{display:grid;grid-template-columns:repeat(auto-fill,minmax(128px,1fr));gap:14px}
.bgrid.tiers{grid-template-columns:repeat(auto-fill,minmax(108px,1fr));gap:11px}
.bgrid.tiers .btile{padding:14px 8px 11px;gap:7px}
.bgrid.tiers .bimg,.bgrid.tiers .bimg img{width:44px;height:44px}
.bgrid.tiers .bimg.gl{font-size:24px}
.btile{display:flex;flex-direction:column;align-items:center;gap:9px;padding:18px 12px 14px;border-radius:var(--radius-sm);
  background:var(--glass);border:1px solid var(--stroke);text-align:center;transition:transform .2s var(--ease),border-color .2s}
.btile:hover{transform:translateY(-3px);border-color:var(--stroke-2)}
.btile .bimg{width:56px;height:56px;display:grid;place-items:center}
.btile .bimg img{width:56px;height:56px;object-fit:contain;filter:drop-shadow(0 6px 14px rgba(0,0,0,.5))}
.btile .bimg.gl{font-size:30px;border-radius:15px;background:rgba(255,255,255,.05);border:1px solid var(--stroke)}
.btile .bnm{font-size:12.5px;font-weight:700;line-height:1.25}
.btile .bsub{font-size:11px;color:var(--text-mute);font-weight:600}
.btile .soon-t{color:var(--warn)}
.btile.locked{opacity:.42;filter:grayscale(.9);transition:opacity .2s var(--ease),filter .2s var(--ease),transform .2s var(--ease),border-color .2s}
.btile.locked:hover{opacity:1;filter:none} /* preview the badge in colour on hover */
.btile.soon{opacity:.72}
.btile.current{border-color:rgba(var(--accent-rgb),.6);box-shadow:0 0 0 1px rgba(var(--accent-rgb),.3) inset}
.btile.current .bsub{color:var(--accent)}
.hbadges{display:inline-flex;gap:5px;align-items:center;flex-wrap:wrap}
.hb{width:24px;height:24px;display:grid;place-items:center;font-size:14px}
.hb img{width:24px;height:24px;object-fit:contain;filter:drop-shadow(0 3px 6px rgba(0,0,0,.5))}
.hnamerow{display:flex;align-items:center;gap:10px;flex-wrap:wrap;margin:1px 0 6px}
.hnamerow .hname{margin:0}

/* ---------------- profile / stats / presence editor ---------------- */
.profile{display:flex;flex-direction:column;gap:22px;padding-bottom:8px}
.statgrid{display:grid;grid-template-columns:repeat(auto-fill,minmax(160px,1fr));gap:14px}
.stat{display:flex;flex-direction:column;gap:4px;padding:16px 18px;border-radius:var(--radius-sm);
  background:var(--glass);border:1px solid var(--stroke)}
.stat-ic{font-size:22px;margin-bottom:2px}
.stat-v{font-size:22px;font-weight:800;letter-spacing:-.4px;font-family:"Bricolage Grotesque",system-ui}
.stat-l{font-size:12px;color:var(--text-mute);font-weight:600}
/* Discord-style rich presence card (click text to edit) */
.rpwrap{max-width:460px}
.rpcard{background:#232428;border:1px solid rgba(255,255,255,.06);border-radius:12px;padding:16px}
.rp-head{display:flex;align-items:center;gap:5px;font-size:12px;font-weight:800;letter-spacing:.4px;color:#b5bac1;
  cursor:pointer;width:max-content;padding:3px 6px;margin:-3px -6px 10px;border-radius:6px;transition:.15s var(--ease)}
.rp-head:hover{background:rgba(255,255,255,.06);color:#fff}
.rp-body{display:flex;gap:14px}
.rp-ic{width:60px;height:60px;flex:none;border-radius:12px;display:grid;place-items:center;color:#fff;
  background:linear-gradient(140deg,var(--accent),var(--accent-2))}
.rp-lines{flex:1;min-width:0;display:flex;flex-direction:column;justify-content:center;gap:2px}
.rp-name,.rp-line{outline:none;border-radius:5px;padding:1px 4px;margin:0 -4px;min-height:19px}
.rp-name{font-size:15px;font-weight:700;color:#fff}
.rp-line{font-size:13px;color:#dbdee1}
.rp-name:hover,.rp-line:hover{background:rgba(255,255,255,.05)}
.rp-name:focus,.rp-line:focus{background:rgba(88,101,242,.18);box-shadow:0 0 0 1px rgba(88,101,242,.6)}
[contenteditable].rp-name:empty::before,[contenteditable].rp-line:empty::before{content:attr(data-ph);color:#72767d;cursor:text}
.rp-actions{display:flex;gap:10px;margin-top:14px}
.rp-actions .act{width:auto;padding:0 18px;height:42px}
.rp-actions .act.primary{flex:none}

/* studio */
.studio{display:grid;grid-template-columns:1fr 300px;gap:22px;align-items:start}
@media(max-width:840px){.studio{grid-template-columns:1fr}}
.st-preview{border-radius:var(--radius);overflow:hidden;background:#232428;border:1px solid var(--stroke);position:relative}
.st-banner{height:130px;background:linear-gradient(120deg,var(--accent),var(--accent-2)) center/cover no-repeat}
.st-avwrap{position:relative;width:96px;height:96px;margin:-48px 0 0 20px}
.st-av{width:96px;height:96px;border-radius:50%;border:6px solid #232428;background:#111 center/cover no-repeat;cursor:grab}
.st-av:active{cursor:grabbing}
.st-deco{position:absolute;inset:-18px;background:center/contain no-repeat;pointer-events:none}
.st-meta{padding:10px 20px 18px}
.st-name{font-size:17px;font-weight:800}
.st-tag{font-size:12px;color:var(--text-mute)}
.st-controls{display:flex;flex-direction:column;gap:11px}
.st-row{display:flex;gap:10px}
.st-row .act{flex:1;height:40px;padding:0 12px}
.sl{display:grid;grid-template-columns:84px 1fr 44px;align-items:center;gap:10px;font-size:12px;color:var(--text-dim);font-weight:600}
.sl input[type=range]{-webkit-appearance:none;height:5px;border-radius:5px;background:var(--glass-2);outline:none}
.sl input[type=range]::-webkit-slider-thumb{-webkit-appearance:none;width:15px;height:15px;border-radius:50%;background:var(--accent);cursor:pointer;box-shadow:0 0 8px rgba(var(--accent-rgb),.6)}
.sl b{color:var(--text);text-align:right;font-variant-numeric:tabular-nums}
.decopicks{display:flex;gap:8px;flex-wrap:wrap}
.decopick{width:46px;height:46px;border-radius:11px;overflow:hidden;border:2px solid var(--stroke);background:var(--glass-2)}
.decopick img{width:100%;height:100%;object-fit:contain}
.decopick.on{border-color:var(--accent)}

/* ---------------- home ---------------- */
.home{display:flex;flex-direction:column;gap:22px;padding-bottom:6px}
.hero{display:flex;align-items:center;gap:20px;padding:22px 24px;border-radius:var(--radius);
  background:linear-gradient(135deg,rgba(var(--accent-rgb),.16),rgba(52,211,153,.08)),var(--glass);
  border:1px solid var(--stroke);backdrop-filter:blur(18px)}
.hero-av{width:84px;height:84px;border-radius:50%;overflow:hidden;flex:none;display:grid;place-items:center;
  border:3px solid rgba(255,255,255,.16);background:var(--bg-1);box-shadow:0 10px 30px -10px rgba(var(--accent-rgb),.7)}
.hero-av img{width:100%;height:100%;object-fit:cover}
.hero-av span{font-size:34px;font-weight:800;color:var(--accent);font-family:"Bricolage Grotesque",system-ui}
.hero-txt{flex:1;min-width:0}
.hero-txt .hi{color:var(--text-dim);font-size:13px;font-weight:600}
.hero-txt .hname{font-size:26px;font-weight:800;letter-spacing:-.4px;
  font-family:"Bricolage Grotesque","Segoe UI Variable Display",system-ui;margin:1px 0 6px}
.horbs{display:inline-flex;align-items:center;gap:8px;font-size:14px;font-weight:750;color:#efe6ff}
.hero-cta{display:flex;gap:10px;flex:none;align-items:center}
.hero-cta .act{width:auto;padding:0 18px;height:44px}
.hicon{height:44px;width:44px}
.earnable{margin-left:12px;color:var(--accent-2);font-weight:750;font-size:13.5px}
.earnable::before{content:"";display:inline-block;width:5px;height:5px;border-radius:50%;background:var(--text-mute);margin:0 10px 2px 0;vertical-align:middle}
.hsec-head{display:flex;align-items:center;justify-content:space-between;margin-bottom:12px}
.hsec-head h3{font-size:16px;font-weight:750;letter-spacing:-.2px}
.viewall{color:var(--accent);font-size:13px;font-weight:700;padding:6px 10px;border-radius:9px;transition:.15s var(--ease)}
.viewall:hover{background:rgba(var(--accent-rgb),.14)}
.grid.home2{grid-template-columns:repeat(auto-fill,minmax(320px,1fr))}
.hempty{color:var(--text-mute);font-size:13.5px;padding:14px 2px;font-weight:500}
.hstats{display:grid;grid-template-columns:repeat(4,1fr);gap:12px}
@media(max-width:720px){.hstats{grid-template-columns:repeat(2,1fr)}}
/* homepage: profile card (left) + actions & stacked stats (right) */
.home-top{display:grid;grid-template-columns:1.25fr 1fr;gap:18px;align-items:start}
@media(max-width:860px){.home-top{grid-template-columns:1fr}}
.home-profile{min-width:0}
.home-side{display:flex;flex-direction:column;gap:14px}
.home-welcome{padding:16px 18px;border-radius:var(--radius);
  background:linear-gradient(135deg,rgba(var(--accent-rgb),.16),rgba(52,211,153,.08)),var(--glass);border:1px solid var(--stroke)}
.home-welcome .hi{color:var(--text-dim);font-size:13px;font-weight:600}
.home-welcome .hname{font-size:24px;font-weight:800;letter-spacing:-.4px;margin:2px 0 8px;
  font-family:"Bricolage Grotesque","Segoe UI Variable Display",system-ui}
.home-cta{display:flex;gap:10px;align-items:center}
.home-cta .act{flex:1;height:46px;padding:0 14px}
.home-cta .hicon{flex:none;width:46px;height:46px}
.hstats-col{display:flex;flex-direction:column;gap:12px}
.hstats-col .stat{flex-direction:row;align-items:center;gap:14px}
.hstats-col .stat-ic{font-size:22px;margin:0}
.hstats-col .stat-v{font-size:22px}
.hstats-col .stat .stat-l{margin-left:auto}
.goal{display:flex;gap:14px;align-items:center;padding:14px;border-radius:var(--radius);
  background:linear-gradient(135deg,rgba(var(--accent-rgb),.12),rgba(52,211,153,.06)),var(--glass);border:1px solid var(--stroke)}
.goal-art{width:64px;height:44px;border-radius:11px;overflow:hidden;flex:none;position:relative}
.goal-art img{width:100%;height:100%;object-fit:contain}
.goal-body{flex:1;min-width:0;display:flex;flex-direction:column;gap:7px}
.goal-t{display:flex;justify-content:space-between;align-items:center;font-size:13.5px}
.goal-x{color:var(--text-mute);font-size:13px;padding:2px 6px;border-radius:7px}
.goal-x:hover{background:var(--glass-2);color:var(--text)}
.goalstar{position:absolute;top:10px;right:10px;z-index:2;width:28px;height:28px;border-radius:9px;
  background:rgba(5,6,13,.55);border:1px solid var(--stroke);font-size:13px;opacity:0;transition:.15s var(--ease);filter:grayscale(1)}
.stile:hover .goalstar{opacity:1}
.goalstar.on{opacity:1;filter:none;border-color:rgba(var(--accent-rgb),.7)}
.afford{color:var(--ok);font-weight:800;margin-left:7px}
.owned-b{position:absolute;top:10px;right:10px;z-index:2;font-size:11px;font-weight:800;color:#04120c;
  background:var(--accent-2);padding:4px 9px;border-radius:8px}
.expsoon{color:#ff9d5c;font-weight:700}

/* ---------------- shop ---------------- */
.shopbar{display:flex;align-items:center;gap:10px;padding:2px 0 16px;flex-wrap:wrap}
.chips{display:flex;gap:7px;flex-wrap:wrap}
.chip{height:34px;padding:0 14px;border-radius:11px;background:var(--glass);border:1px solid var(--stroke);
  color:var(--text-dim);font-size:12.5px;font-weight:650;transition:.16s var(--ease)}
.chip:hover{color:var(--text);background:var(--glass-2)}
.chip.on{color:var(--on-accent);background:linear-gradient(135deg,var(--accent),var(--accent-2));border-color:transparent}
.shopbar .grow{flex:1;min-width:20px}
.shopfilters{display:flex;gap:26px;flex-wrap:wrap;padding:0 0 16px;align-items:flex-start}
.ff{display:flex;flex-direction:column;gap:8px}
.ff-l{font-size:11px;font-weight:800;letter-spacing:.7px;text-transform:uppercase;color:var(--text-mute)}
.swatches{display:flex;gap:8px}
.swatch{width:26px;height:26px;border-radius:50%;background:var(--sw);border:2px solid transparent;cursor:pointer;
  transition:.15s var(--ease);box-shadow:0 2px 8px rgba(0,0,0,.4)}
.swatch:hover{transform:scale(1.12)}
.swatch.on{border-color:#fff;box-shadow:0 0 0 2px rgba(var(--accent-rgb),.7)}
.chips.wrap{flex-wrap:wrap;max-width:640px}
.chip.sm{height:30px;padding:0 11px;font-size:12px}
.grid.shop{grid-template-columns:repeat(auto-fill,minmax(210px,1fr))}
.stile-art{position:relative;aspect-ratio:16/10;display:flex;align-items:flex-end;padding:11px;overflow:hidden}
.stile-img{position:absolute;inset:0;width:100%;height:100%;object-fit:contain;z-index:0}
.stile-art::after{content:"";position:absolute;inset:0;background:
  radial-gradient(120px 80px at 78% 22%,rgba(255,255,255,.28),transparent 70%),
  linear-gradient(180deg,transparent 55%,rgba(5,6,13,.5))}
.stile-kind{position:relative;z-index:1;font-size:10.5px;font-weight:800;letter-spacing:.5px;text-transform:uppercase;
  color:#fff;background:rgba(5,6,13,.42);border:1px solid rgba(255,255,255,.18);padding:4px 9px;border-radius:8px;backdrop-filter:blur(4px)}
.stile .body{gap:6px;padding:12px 13px 13px}
.stile .name{font-size:14px;min-height:0;-webkit-line-clamp:1}
.stile-foot{display:flex;flex-direction:column;align-items:stretch;gap:10px}
.orbcost{display:flex;align-items:center;gap:7px;font-weight:750;font-size:15px;color:#efe6ff}
.sbuy{width:100%;height:36px;padding:0 12px;font-size:12.5px;border-radius:11px}

/* watch-all button */
.ctl.watchall{color:var(--on-accent);font-weight:750;border-color:transparent;
  background:linear-gradient(135deg,var(--accent),#8b6ee0);box-shadow:0 8px 22px -12px rgba(var(--accent-rgb),.9)}
.ctl.watchall:hover{filter:brightness(1.08);color:var(--on-accent)}
.ctl.watchall svg{opacity:1}

/* ---------------- profile studio (Discord-style customizer) ---------------- */
.studio2{display:grid;grid-template-columns:340px 1fr;gap:22px;align-items:stretch}
@media(max-width:900px){.studio2{grid-template-columns:1fr}}
.studio2.tuck{grid-template-columns:1fr}
.pcust{display:flex;flex-direction:column;gap:10px;align-self:start}
.ppcol{position:relative}
.cust{background:var(--glass);border:1px solid var(--stroke);border-radius:14px;overflow:hidden}
.cust.open{border-color:rgba(var(--accent-rgb),.5)}
.cust-h{display:flex;align-items:center;gap:10px;width:100%;padding:13px 15px;color:var(--text);
  font-size:13.5px;font-weight:750;transition:.15s var(--ease)}
.cust-h:hover{background:var(--glass-2)}
.cust-t{flex:1;text-align:left}
.cust-sw{display:flex;align-items:center;gap:5px}
.cust-cv{color:var(--text-mute);transition:transform .2s var(--ease);flex:none}
.cust.open .cust-cv{transform:rotate(180deg)}
.cust-b{padding:4px 15px 16px;display:flex;flex-direction:column;gap:10px;border-top:1px solid var(--stroke)}
.cust-b .ff-l{margin-top:4px}
.cust-b .st-row .act{flex:1;height:38px}
.mini{display:inline-block;width:22px;height:22px;border-radius:6px;background:center/cover no-repeat var(--glass-2);border:1px solid var(--stroke);vertical-align:middle}
.mini.ph{background:repeating-conic-gradient(var(--glass-2) 0% 25%,transparent 0% 50%) 0/10px 10px}
.pgrid{display:grid;grid-template-columns:repeat(auto-fill,minmax(52px,1fr));gap:8px}
.ppick{aspect-ratio:1;border-radius:11px;overflow:hidden;border:2px solid var(--stroke);background:var(--glass-2);
  display:grid;place-items:center;transition:.14s var(--ease);position:relative}
.ppick:hover{border-color:rgba(var(--accent-rgb),.6);transform:translateY(-1px)}
.ppick img{width:100%;height:100%;object-fit:contain}
.ppick.on{border-color:var(--accent);box-shadow:0 0 0 2px rgba(var(--accent-rgb),.35)}
.ppick.none{color:var(--text-mute)}.ppick.none svg{width:16px;height:16px}
.ppick.none.on{color:var(--accent)}
.pp-load{grid-column:1/-1;color:var(--text-mute);font-size:12.5px;padding:8px 2px;font-weight:600}
.pgrid-c{grid-column:1/-1;font-size:11px;font-weight:700;color:var(--text-mute);letter-spacing:.3px}
.mini.nsw{width:auto;padding:0 7px;font-size:13px;font-weight:900;line-height:22px;background:var(--glass-2)}
/* display-name-style pickers (font / effect / colour) */
.fontgrid{display:grid;grid-template-columns:repeat(6,1fr);gap:7px}
.fontpick{aspect-ratio:1;border-radius:10px;border:2px solid var(--stroke);background:var(--glass-2);color:var(--text);
  font-size:19px;font-weight:800;transition:.14s var(--ease)}
.fontpick:hover{border-color:rgba(var(--accent-rgb),.6)}
.fontpick.on{border-color:var(--accent);box-shadow:0 0 0 2px rgba(var(--accent-rgb),.3)}
.effgrid{display:grid;grid-template-columns:repeat(4,1fr);gap:7px}
.effpick{padding:9px 4px;border-radius:10px;border:2px solid var(--stroke);background:var(--glass-2);
  font-size:13px;font-weight:800;transition:.14s var(--ease)}
.effpick:hover{border-color:rgba(var(--accent-rgb),.6)}
.effpick.on{border-color:var(--accent);box-shadow:0 0 0 2px rgba(var(--accent-rgb),.3)}
.colgrid{display:flex;gap:9px;flex-wrap:wrap}
.colpick{width:34px;height:34px;border-radius:50%;border:2px solid transparent;cursor:pointer;transition:.14s var(--ease);box-shadow:0 2px 8px rgba(0,0,0,.4)}
.colpick:hover{transform:scale(1.1)}
.colpick.on{border-color:#fff;box-shadow:0 0 0 2px rgba(var(--accent-rgb),.7)}
/* full colour picker row (theme) */
.colrow{display:flex;align-items:center;gap:10px;margin-bottom:2px}
.colpk{width:46px;height:34px;border:1px solid var(--stroke);border-radius:9px;background:none;cursor:pointer;padding:2px}
.colpk::-webkit-color-swatch{border:none;border-radius:6px}.colpk::-webkit-color-swatch-wrapper{padding:0}
.colhex{font-size:13px;font-weight:700;font-variant-numeric:tabular-nums;color:var(--text);text-transform:uppercase}
.colclr{margin-left:auto;width:28px;height:28px;border-radius:8px;color:var(--text-mute);display:grid;place-items:center}
.colclr:hover{background:var(--glass-2);color:var(--text)}
/* live profile preview */
/* the theme's two colours tint the profile body (under the banner), not the banner */
.ppreview{position:relative;z-index:1;border-radius:16px;overflow:hidden;border:1px solid var(--stroke);min-height:520px;--pt:#232428;--pt2:#232428;
  background:linear-gradient(180deg,color-mix(in srgb,var(--pt) 55%,#0e0f16),color-mix(in srgb,var(--pt2) 55%,#0e0f16))}
.ppreview.pp-sticky{transition:box-shadow .2s var(--ease)}
.ppreview.pp-static .pp-av{cursor:default}
/* the big editable preview at the top of the profile studio */
.studio-top{position:relative;min-height:560px;margin-bottom:6px}
.studio-preview{max-width:560px}
/* tucked: taller top-right popout while scrolling; click returns to top */
.ppreview.tucked{position:fixed;top:54px;right:24px;width:292px;min-height:0;max-height:calc(100vh - 88px);overflow:auto;margin:0!important;
  z-index:60;cursor:pointer;border-color:rgba(var(--accent-rgb),.55);box-shadow:0 24px 60px -12px rgba(0,0,0,.7)}
.ppreview.tucked .pp-btns,.ppreview.tucked .pp-hint{display:none}
.ppreview.tucked .pp-banner{height:96px}
.ppreview.tucked .pp-avwrap{width:64px;height:64px;margin:-34px 0 0 16px}
.ppreview.tucked .pp-av{width:64px;height:64px;border-width:5px}
.ppreview.tucked .pp-av span{font-size:26px}
.ppreview.tucked .pp-status{width:16px;height:16px;border-width:4px}
.ppreview.tucked .pp-body{padding:8px 16px 16px}
.ppreview.tucked .pp-name{font-size:18px}
.ppreview.tucked .pp-details{margin-top:10px}
.ppreview.tucked::after{content:"↑ Back to top";position:sticky;float:right;top:8px;margin:8px 10px 0 0;z-index:9;
  font-size:10px;font-weight:800;color:#fff;background:rgba(5,6,13,.6);border:1px solid var(--stroke);
  padding:3px 7px;border-radius:7px;backdrop-filter:blur(4px)}
.pp-effect{position:absolute;inset:0;z-index:4;background:center/cover no-repeat;pointer-events:none;mix-blend-mode:screen;opacity:.9;display:none}
.pp-effect .fx-layer{position:absolute;inset:0;width:100%;height:100%;object-fit:cover}
.pp-banner{height:150px;background:linear-gradient(145deg,#41445a,#2a2c38) center/cover no-repeat;z-index:0}
/* frame wraps AROUND the card: the card is margined into the frame's window,
   the frame fills the wrapper. Back layers behind the card, front in front. */
.pp-wrap{position:relative}
.pp-frame{position:absolute;inset:0;pointer-events:none;display:none}
.pp-frame.back{z-index:0}
.pp-frame.front{z-index:2}
.pp-frame .fl{position:absolute;left:0;width:100%;height:auto}
.pp-frame .fl-top{top:0}
.pp-frame .fl-bottom{bottom:0}
.pp-avwrap{position:relative;width:92px;height:92px;margin:-46px 0 0 22px;z-index:5}
.pp-av{width:92px;height:92px;border-radius:50%;border:6px solid #232428;background:#111 center/cover no-repeat;cursor:grab;overflow:hidden;display:grid;place-items:center}
.pp-av:active{cursor:grabbing}
.pp-av img{width:100%;height:100%;object-fit:cover}
.pp-av span{font-size:36px;font-weight:800;color:#fff;font-family:"Bricolage Grotesque",system-ui}
.pp-deco{position:absolute;inset:-16px;background:center/contain no-repeat;pointer-events:none;display:none}
.pp-status{position:absolute;right:2px;bottom:2px;width:22px;height:22px;border-radius:50%;background:#23a55a;border:5px solid #232428}
.pp-body{position:relative;z-index:5;padding:10px 22px 22px}
.pp-nameplate{position:absolute;left:0;right:0;top:2px;height:44px;background:center/cover no-repeat;border-radius:10px;display:none;z-index:-1;opacity:.9}
.pp-name{font-size:22px;font-weight:800;letter-spacing:-.3px;font-family:"Bricolage Grotesque",system-ui}
.pp-tag{font-size:13px;color:var(--text-dim);margin:1px 0 8px}.pp-tag span{color:var(--text-mute)}
.ppreview .hbadges{margin:2px 0 4px}
.pp-btns{display:flex;gap:8px;margin:12px 0 6px}
.pp-btn{height:34px;padding:0 14px;border-radius:9px;background:var(--glass-2);border:1px solid var(--stroke);
  display:flex;align-items:center;gap:6px;font-size:13px;font-weight:650;color:var(--text-dim)}
.pp-btn.wide{flex:1}.pp-btn svg{width:14px;height:14px}
.pp-details{border-top:1px solid var(--stroke);margin-top:12px;padding-top:4px}
.pp-bio{font-size:13.5px;color:var(--text-dim);margin-top:3px;font-style:italic}
.pp-sub{font-size:11px;font-weight:800;letter-spacing:.5px;text-transform:uppercase;color:var(--text-mute);margin-top:14px}
.pp-val{font-size:13.5px;font-weight:600;margin-top:3px;display:flex;align-items:center;gap:4px}
.pp-conn{display:flex;align-items:center;gap:8px;font-size:13.5px;font-weight:600;margin-top:6px}
.pp-conn-d{width:20px;height:20px;border-radius:6px;background:var(--glass-2);border:1px solid var(--stroke);flex:none}
.pp-conn-n{color:var(--text)}
.pp-conn-t{margin-left:auto;font-size:11px;font-weight:700;color:var(--text-mute);text-transform:capitalize}
.pp-hint{margin-top:14px;font-size:12px;color:var(--text-mute);font-style:italic}
/* the big equipped-profile header at the top of the profile page */
.profile-hero{max-width:560px}
/* presence: large + small image slots */
.rp-imgs{position:relative;flex:none}
.rp-art{width:60px;height:60px;border-radius:12px;overflow:hidden;position:relative;cursor:pointer;background:var(--glass-2)}
.rp-art img{width:100%;height:100%;object-fit:cover}
.rp-art.empty{display:flex;flex-direction:column;align-items:center;justify-content:center;gap:3px;color:#b5bac1;
  border:1.5px dashed rgba(255,255,255,.2)}
.rp-art.empty svg{width:18px;height:18px}.rp-art.empty small{font-size:8.5px;font-weight:700}
.rp-sm{position:absolute;right:-7px;bottom:-7px;width:26px;height:26px;border-radius:50%;overflow:hidden;
  border:3px solid #232428;cursor:pointer;background:var(--glass-2)}
.rp-sm img{width:100%;height:100%;object-fit:cover}
.rp-sm.empty{display:grid;place-items:center;color:#b5bac1;font-size:15px;font-weight:800;border:2.5px solid #232428;background:var(--glass)}
.rp-x{position:absolute;top:2px;right:2px;width:16px;height:16px;border-radius:50%;background:rgba(0,0,0,.6);color:#fff;
  display:grid;place-items:center;opacity:0;transition:.14s}.rp-x svg{width:10px;height:10px}
.rp-art:hover .rp-x{opacity:1}.rp-x.sm{width:14px;height:14px;top:-2px;right:-2px}
.rp-tips{display:grid;grid-template-columns:1fr 1fr;gap:10px;margin-top:14px}
@media(max-width:600px){.rp-tips{grid-template-columns:1fr}}
.rp-tl{display:flex;flex-direction:column;gap:5px;font-size:11px;font-weight:700;color:var(--text-mute)}
.rp-te{background:#1e1f22;border:1px solid rgba(255,255,255,.08);border-radius:8px;padding:8px 10px;font-size:13px;
  color:#dbdee1;font-weight:500;min-height:17px;outline:none}
.rp-te:focus{border-color:rgba(88,101,242,.6)}
[contenteditable].rp-te:empty::before{content:attr(data-ph);color:#5c5e66;cursor:text}
.rpwrap{max-width:520px}
/* auto-update banner */
.updbar{position:fixed;left:50%;bottom:22px;transform:translateX(-50%);z-index:200;display:flex;align-items:center;gap:12px;
  padding:11px 12px 11px 16px;border-radius:14px;background:rgba(20,22,32,.92);border:1px solid rgba(var(--accent-rgb),.5);
  box-shadow:0 20px 50px -12px rgba(0,0,0,.7);backdrop-filter:blur(14px);animation:updIn .4s var(--spring)}
@keyframes updIn{from{opacity:0;transform:translate(-50%,16px)}to{opacity:1;transform:translate(-50%,0)}}
.updbar .upd-i{color:var(--accent);display:flex}
.updbar .upd-t{font-size:13.5px;font-weight:600;color:var(--text)}.updbar .upd-t b{color:var(--accent)}
.updbar .upd-go{height:34px;padding:0 16px;border-radius:10px;font-size:13px;font-weight:750;color:var(--on-accent);
  background:linear-gradient(135deg,var(--accent),var(--accent-2))}
.updbar.busy .upd-go{opacity:.7;pointer-events:none}
.updbar .upd-x{width:32px;height:32px;border-radius:9px;color:var(--text-mute);display:grid;place-items:center}
.updbar .upd-x:hover{background:var(--glass-2);color:var(--text)}
.updbar.busy .upd-x{display:none}
/* changelog / what's-new popup */
.clog-wrap{position:fixed;inset:0;z-index:300;display:grid;place-items:center;background:rgba(4,6,12,.72);backdrop-filter:blur(6px);animation:updIn .25s ease}
.clog{width:min(440px,90vw);background:var(--bg-1);border:1px solid var(--stroke);border-radius:20px;padding:24px;box-shadow:0 30px 80px -20px rgba(0,0,0,.8)}
.clog-h{display:flex;gap:14px;align-items:center;margin-bottom:16px}
.clog-badge{width:52px;height:52px;flex:none;border-radius:15px;display:grid;place-items:center;color:#fff;background:linear-gradient(140deg,var(--accent),var(--accent-2))}
.clog-t{font-size:19px;font-weight:800;letter-spacing:-.3px}
.clog-d{font-size:13px;color:var(--text-mute);margin-top:2px}
.clog-list{margin:0 0 18px;padding-left:18px;display:flex;flex-direction:column;gap:7px;font-size:14px;color:var(--text-dim)}
.clog-list li{line-height:1.4}
/* settings two-column: settings left, about/patch-notes right */
.settings2{display:grid;grid-template-columns:1fr 340px;gap:22px;align-items:start}
@media(max-width:920px){.settings2{grid-template-columns:1fr}}
.set-side{display:flex;flex-direction:column;gap:16px;position:sticky;top:14px}
.who{padding:16px;border-radius:var(--radius);background:linear-gradient(135deg,rgba(var(--accent-rgb),.16),rgba(52,211,153,.08)),var(--glass);border:1px solid var(--stroke)}
.who-t{font-size:12px;font-weight:700;color:var(--text-mute);letter-spacing:.4px}
.who-row{display:flex;align-items:center;gap:12px;margin-top:8px}
.who-av{width:44px;height:44px;border-radius:13px;flex:none;display:grid;place-items:center;color:#fff;font-weight:800;font-size:20px;background:linear-gradient(140deg,var(--accent),var(--accent-2));font-family:"Bricolage Grotesque",system-ui}
.who-n{font-size:16px;font-weight:800}
.who-l{font-size:13px;color:var(--accent);font-weight:650;display:inline-flex;align-items:center;gap:4px;cursor:pointer}
.who-l:hover{text-decoration:underline}
.who-l svg{width:12px;height:12px}
.patch{padding:16px;border-radius:var(--radius);background:var(--glass);border:1px solid var(--stroke)}
.patch-h{display:flex;align-items:center;justify-content:space-between;margin-bottom:12px}
.patch-h h3{font-size:15px;font-weight:750}
.patch-ver{margin-top:2px}
.patch-v{font-size:14px;font-weight:800;color:var(--accent)}
.patch-d{font-size:12px;color:var(--text-mute)}
.patch-list{margin:8px 0 0;padding-left:16px;display:flex;flex-direction:column;gap:6px;font-size:13px;color:var(--text-dim)}
.patch-list li{line-height:1.35}
.patch-old{margin-top:12px;border-top:1px solid var(--stroke);padding-top:10px}
.patch-old .patch-v{font-size:12.5px;color:var(--text-dim)}
.chk-upd{height:34px;padding:0 12px;border-radius:10px;font-size:12.5px;font-weight:700;color:var(--text);background:var(--glass-2);border:1px solid var(--stroke)}
.chk-upd:hover{background:var(--glass);border-color:rgba(var(--accent-rgb),.5)}
/* quest history */
.history{display:flex;flex-direction:column;gap:22px;padding-bottom:8px}
.heatwrap{display:flex;flex-direction:column;gap:8px}
.heatgrid{display:grid;grid-template-rows:repeat(7,14px);grid-auto-flow:column;grid-auto-columns:14px;gap:4px}
.heat{width:14px;height:14px;border-radius:4px;background:var(--glass-2)}
.heat.pad{background:transparent}
.heat.l0{background:rgba(255,255,255,.05)}
.heat.l1{background:rgba(var(--accent-rgb),.35)}
.heat.l2{background:rgba(var(--accent-rgb),.65)}
.heat.l3{background:var(--accent)}
.heatkey{display:flex;align-items:center;gap:5px;font-size:11.5px;color:var(--text-mute);font-weight:600}
.heatkey .heat{width:11px;height:11px}
.hist-list{display:flex;flex-direction:column;gap:16px}
.hist-day{display:flex;flex-direction:column;gap:7px}
.hist-date{font-size:13px;font-weight:750;color:var(--text-dim);display:flex;align-items:center;gap:8px}
.hist-date span{font-size:11.5px;font-weight:600;color:var(--text-mute);background:var(--glass);padding:2px 8px;border-radius:7px}
.hist-row{display:flex;align-items:center;gap:12px;padding:11px 14px;border-radius:12px;background:var(--glass);border:1px solid var(--stroke)}
.hist-cat{width:30px;height:30px;flex:none;border-radius:9px;display:grid;place-items:center;color:#fff;background:linear-gradient(140deg,var(--accent),#8b6ee0)}
.hist-cat.game{background:linear-gradient(140deg,#3ba55d,var(--accent-2))}
.hist-cat svg{width:15px;height:15px}
.hist-n{flex:1;min-width:0;font-size:14px;font-weight:650;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.hist-o{display:inline-flex;align-items:center;gap:6px;font-size:13.5px;font-weight:750;color:#efe6ff}
</style>
</head>
<body>
<!-- Fullscreen boot / welcome overlay -->
<div id="boot" class="boot">
  <div class="bg"></div>
  <div class="boot-load" id="bootLoad">
    <div class="mark boot-mark">
      <svg width="30" height="30" viewBox="0 0 24 24" fill="none" stroke="#0b0616" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l2.4 5.6L20 11l-5.6 2.4L12 19l-2.4-5.6L4 11l5.6-2.4z"/></svg>
    </div>
    <div class="spin"></div>
    <div class="boot-msg">Reading your Discord client…</div>
  </div>
  <div class="boot-welcome" id="bootWelcome">
    <div class="wav-wrap"><img id="wAvatar" class="wav" alt=""><span id="wAvatarFallback" class="wav-fb"></span></div>
    <div class="wtext" id="wText">Welcome back</div>
    <div class="wname" id="wName">—</div>
  </div>
</div>

<div id="app">
  <div class="bg"></div>

  <aside class="rail">
    <div class="mark">
      <svg width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="#0b0616" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 3l2.4 5.6L20 11l-5.6 2.4L12 19l-2.4-5.6L4 11l5.6-2.4z"/>
      </svg>
    </div>
    <button class="navbtn" data-nav="home" onclick="setNav('home')">
      <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M4 11.5L12 4l8 7.5"/><path d="M6 10.5V20h12v-9.5"/><path d="M10 20v-5h4v5"/></svg>
      <span class="tip">Home</span>
    </button>
    <button class="navbtn" data-nav="video" onclick="setNav('video')">
      <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="5" width="19" height="14" rx="3.2"/><path d="M10.2 9.3l4.6 2.7-4.6 2.7z" fill="currentColor" stroke="none"/></svg>
      <span class="tip">Watch Videos</span><span class="cnt" id="n-video"></span>
    </button>
    <button class="navbtn" data-nav="game" onclick="setNav('game')">
      <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M6.5 7h11a4.5 4.5 0 014.4 5.4l-.8 4A3.2 3.2 0 0118 19c-1.2 0-1.9-.7-2.6-1.5l-.7-.8h-5.4l-.7.8C7.9 18.3 7.2 19 6 19a3.2 3.2 0 01-3.1-2.6l-.8-4A4.5 4.5 0 016.5 7z"/><path d="M8 11.2v2.2M6.9 12.3h2.2M15.4 11.6h.01M17.3 13.3h.01"/></svg>
      <span class="tip">Play Games</span><span class="cnt" id="n-game"></span>
    </button>
    <button class="navbtn" data-nav="claim" onclick="setNav('claim')">
      <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="2.8" y="8.5" width="18.4" height="12.2" rx="2.6"/><path d="M2.8 12.6h18.4M12 8.5v12.2"/><path d="M12 8.5S10.6 4 8.2 4a2.2 2.2 0 000 4.5zM12 8.5S13.4 4 15.8 4a2.2 2.2 0 010 4.5z"/></svg>
      <span class="tip">Claim Rewards</span><span class="cnt" id="n-claim"></span>
    </button>
    <button class="navbtn" data-nav="shop" onclick="setNav('shop')">
      <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M4.5 8h15l-1 11.2a1.6 1.6 0 01-1.6 1.5H7.1a1.6 1.6 0 01-1.6-1.5z"/><path d="M8.5 8V6.4a3.5 3.5 0 017 0V8"/></svg>
      <span class="tip">Orb Shop</span>
    </button>
    <button class="navbtn" data-nav="badges" onclick="setNav('badges')">
      <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l2.6 1.9 3.2-.2 1 3 2.6 1.8-1.2 3 1.2 3-2.6 1.8-1 3-3.2-.2L12 21l-2.6-1.9-3.2.2-1-3L2.6 14.5l1.2-3-1.2-3 2.6-1.8 1-3 3.2.2z"/><path d="M9.2 12l2 2 3.6-4"/></svg>
      <span class="tip">Badges</span>
    </button>
    <button class="navbtn" data-nav="history" onclick="setNav('history')">
      <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M3 4.5h18v16H3z"/><path d="M3 9h18M8 3v3M16 3v3"/><path d="M7.5 13h2M11 13h2M14.5 13h2M7.5 16.5h2M11 16.5h2"/></svg>
      <span class="tip">History</span>
    </button>
    <div class="sp"></div>
    <div class="orbbal" id="orbBal" title="Your orb balance"><span class="orb-dot"></span><span id="orbBalNum">—</span></div>
    <button class="navbtn" data-nav="settings" onclick="setNav('settings')">
      <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3.1"/><path d="M19.4 15a1.6 1.6 0 00.3 1.8l.1.1a2 2 0 11-2.8 2.8l-.1-.1a1.6 1.6 0 00-1.8-.3 1.6 1.6 0 00-1 1.5v.2a2 2 0 11-4 0v-.1a1.6 1.6 0 00-1-1.5 1.6 1.6 0 00-1.8.3l-.1.1a2 2 0 11-2.8-2.8l.1-.1a1.6 1.6 0 00.3-1.8 1.6 1.6 0 00-1.5-1H3a2 2 0 110-4h.1a1.6 1.6 0 001.5-1 1.6 1.6 0 00-.3-1.8l-.1-.1a2 2 0 112.8-2.8l.1.1a1.6 1.6 0 001.8.3H9a1.6 1.6 0 001-1.5V3a2 2 0 114 0v.1a1.6 1.6 0 001 1.5 1.6 1.6 0 001.8-.3l.1-.1a2 2 0 112.8 2.8l-.1.1a1.6 1.6 0 00-.3 1.8V9a1.6 1.6 0 001.5 1h.2a2 2 0 110 4h-.1a1.6 1.6 0 00-1.5 1z"/></svg>
      <span class="tip">Settings</span>
    </button>
    <button class="ravatar" id="railAvatar" title="Your profile" onclick="setNav('profile')">
      <img id="railAvatarImg" alt="">
      <span id="railAvatarFallback"></span>
      <span class="ravatar-gear"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="8" r="3.4"/><path d="M5.5 20a6.5 6.5 0 0113 0"/></svg></span>
    </button>
  </aside>

  <main>
    <header id="titlebar">
      <div>
        <h1 id="pageTitle">Watch Videos</h1>
      </div>
      <div class="tb-grow"></div>
      <button class="pill" id="pill" onclick="onPill()" title="Active quest — click to show or stop">
        <span class="eq"><i></i><i></i><i></i></span>
        <span class="nm" id="pillName">—</span>
        <span class="tm" id="pillTime"></span>
      </button>
      <button class="winbtn" onclick="send('minimize')" title="Minimize">
        <svg width="15" height="15" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M5 12h14"/></svg>
      </button>
      <button class="winbtn close" onclick="send('close')" title="Close">
        <svg width="15" height="15" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18"/></svg>
      </button>
    </header>

    <div class="toolbar" id="toolbar">
      <div class="dd">
        <button class="ctl" id="sortBtn" onclick="toggleMenu(event)">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16M6.5 12h11M10 17h4"/></svg>
          <span id="sortLabel">Suggested</span>
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg>
        </button>
        <div class="menu" id="sortMenu">
          <div class="lbl">Sort by</div>
          <button class="mi" data-sort="suggested" onclick="setSort('suggested')"><span class="radio"></span>Suggested</button>
          <button class="mi" data-sort="recent" onclick="setSort('recent')"><span class="radio"></span>Most Recent</button>
          <button class="mi" data-sort="expiring" onclick="setSort('expiring')"><span class="radio"></span>Expiring Soon</button>
          <button class="mi" data-sort="started" onclick="setSort('started')"><span class="radio"></span>Started</button>
        </div>
      </div>
      <button class="ctl" id="orbBtn" onclick="toggleOrb()"><span class="orb-dot"></span>Orbs only</button>
      <div class="ctl search">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="11" cy="11" r="6.5"/><path d="M20 20l-3.6-3.6"/></svg>
        <input id="q" placeholder="Search quests" oninput="render()" spellcheck="false">
      </div>
      <div class="tb-spacer"></div>
      <button class="ctl watchall" id="watchAllBtn" onclick="watchAll()">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor"><path d="M4 4.6l8 5.4-8 5.4z"/><path d="M14 4.6l8 5.4-8 5.4z"/></svg>
        Watch all
      </button>
      <button class="ctl" id="presenceBtn" onclick="togglePresence()" style="display:none" title="Show the mimicked game on your Discord profile">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="8" r="3.4"/><path d="M5.5 20a6.5 6.5 0 0113 0"/></svg>
        On profile
      </button>
      <button class="ctl watchall" id="playAllBtn" onclick="playAll()" style="display:none">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor"><path d="M4 4.6l8 5.4-8 5.4z"/><path d="M14 4.6l8 5.4-8 5.4z"/></svg>
        Play all
      </button>
      <button class="ctl icon" onclick="rescan()" title="Refresh">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20.5 12a8.5 8.5 0 11-2.6-6.1"/><path d="M20.5 4.5V10H15"/></svg>
      </button>
    </div>

    <div id="content">
      <div class="mid"><div class="spin"></div><p>Reading your Discord client…</p></div>
    </div>
  </main>

  <!-- Playback dock: hidden by default; the video element stays laid out so
       playback (and therefore quest credit) continues while it's tucked away. -->
  <div class="dock" id="dock">
    <div class="dhead">
      <span class="dn" id="dockName">—</span>
      <button class="minibtn" id="muteBtn" onclick="toggleMute()" title="Unmute">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5L6.5 9H3v6h3.5L11 19z"/><path d="M16.5 8.5l4 7M20.5 8.5l-4 7"/></svg>
      </button>
      <button class="minibtn danger" onclick="stopWatch()" title="Stop">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18"/></svg>
      </button>
    </div>
    <video id="vid" playsinline></video>
    <div class="dfoot">
      <div class="bar" id="dockBar"><i id="dockFill"></i></div>
      <div class="dst"><span id="dockStatus">Idle</span><span id="dockQueue"></span></div>
    </div>
  </div>

  <div class="toast" id="toast"></div>
  <div id="tt"></div>

  <div id="splash" class="splashwrap">
    <div class="splashcard">
      <div class="mark boot-mark" style="width:54px;height:54px;border-radius:18px;margin:0 auto 4px">
        <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="#0b0616" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l2.4 5.6L20 11l-5.6 2.4L12 19l-2.4-5.6L4 11l5.6-2.4z"/></svg>
      </div>
      <h2>Welcome to Aurora Quests</h2>
      <p>This app reads the Discord Quests from <b>your own Discord client</b> signed in on this PC. Your data stays on your machine — nothing is uploaded anywhere.</p>
      <p class="warn-p">⚠ It completes quests and sets your presence using your account token, which is against Discord's Terms of Service. It only touches your own account, for your own rewards, but there's a small account risk — use at your discretion.</p>
      <button class="act primary" style="width:100%;margin-top:6px" onclick="dismissSplash()">Got it</button>
    </div>
  </div>
</div>

<script>
/* ====================== state ====================== */
let QUESTS=[], NAV='video', SORT='suggested', ORB=false, GOT=false;
let SET={launch_on_startup:false,auto_watch:false,auto_play:false,show_presence:true,start_minimized:false,splash_seen:false};
let splashShown=false;
let CUR=null, lastSent=-1, autoQueue=[], claiming={};
let CURPLAY=null, playQueue=[];
let USER=null, userShown=false, dataReady=false, finished=false, welcomeAt=0;
let SHOP=null, shopLoading=false, shopFilter='all', shopSort='recent', shopErr=null, OWNED=null;
let ORBS=null, BADGES=null, STATS=null, EQUIPPED=null, HISTORY=null, navInit=false;
let APP_VERSION='', changelogShown=false;
// In-app patch notes (keep the top entry's version in sync with Cargo.toml).
const CHANGELOG=[
  {v:'0.3.2',d:'15 Aug 2026',notes:[
    'Profile frames now wrap correctly around the profile (inset into the frame window) instead of sitting on top',
    'Your equipped frame now shows on the Home profile too',
    'The new Account Age, Streaming, Game Variety & Game Time badges light up automatically the moment Discord rolls them out to you',
  ]},
  {v:'0.3.1',d:'14 Aug 2026',notes:[
    'Profile theme is now a full colour picker and tints your profile (under the banner), like Discord — not the banner',
    'Profile frames wrap around the profile instead of covering it',
    'New Quest History tab with an activity calendar and completed-quest log',
    'Settings now shows who made it, patch notes, and a Check for updates button',
    'A "what\\u2019s new" popup after each update',
  ]},
  {v:'0.3.0',d:'14 Aug 2026',notes:[
    'First public release',
    'Profile studio: preview every decoration, nameplate, effect, frame, name style & theme',
    'Animated profile effects and real equipped-profile card on Home',
    'System tray, desktop notifications, light theme + accent picker',
    'Built-in auto-update from GitHub Releases',
  ]},
];
let pType=0, pName='', pDetails='', pState='';
let pLargeImg='', pLargeText='', pSmallImg='', pSmallText='';
let STU={avatar:'',banner:'',deco:'',nameplate:'',effect:'',effectAnim:null,frame:'',frameLayers:null,frameMetrics:null,
  avBright:100,avContrast:100,avSat:100,avHue:0,zoom:100,posX:50,posY:50,
  bnBright:100,bnContrast:100,bnSat:100,bnHue:0,
  nameFont:'default',nameEffect:'solid',nameColor:0,themeA:'',themeB:'',open:'avatar',drag:false};
let CATALOG=null, catLoading=false;
const $=s=>document.querySelector(s);
const byId=i=>document.getElementById(i);

function send(type,extra){ try{ window.ipc.postMessage(JSON.stringify(Object.assign({type},extra||{}))); }catch(e){} }

/* clamped tooltip that never leaves the window */
(function(){
  const tt=()=>byId('tt');
  document.addEventListener('mouseover',e=>{
    const el=e.target.closest('[data-tip]'); if(!el) return;
    const t=tt(); t.textContent=el.getAttribute('data-tip'); t.classList.add('show');
    const r=el.getBoundingClientRect(); const tw=t.offsetWidth, th=t.offsetHeight, pad=8;
    let left=r.left+r.width/2-tw/2;
    left=Math.max(pad,Math.min(left,innerWidth-tw-pad));
    let top=r.top-th-8;
    if(top<pad) top=r.bottom+8;            // flip below if it would clip the top
    t.style.left=left+'px'; t.style.top=top+'px';
  });
  document.addEventListener('mouseout',e=>{ if(e.target.closest&&e.target.closest('[data-tip]')) tt().classList.remove('show'); });
})();
function esc(s){return (s==null?'':(''+s)).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}

/* drag the window from the header (buttons excluded) */
byId('titlebar').addEventListener('mousedown',e=>{
  if(e.button!==0) return;
  if(e.target.closest('button')) return;
  send('drag');
});

/* ====================== welcome sequence ====================== */
window.setUser=function(u){
  if(userShown) return; userShown=true; USER=u;
  const initial=(u&&u.name?u.name.trim()[0]:'?').toUpperCase();
  const bw=byId('bootWelcome');
  if(u&&u.avatar){
    byId('wAvatar').src=u.avatar;
    byId('wAvatar').onerror=()=>bw.classList.add('noimg');
    byId('railAvatarImg').src=u.avatar;
    byId('railAvatar').style.display='';
    byId('railAvatarImg').style.display='';
    byId('railAvatarFallback').textContent='';
  } else {
    bw.classList.add('noimg');
    byId('railAvatarImg').style.display='none';
    byId('railAvatarFallback').textContent=initial;
  }
  byId('wAvatarFallback').textContent=initial;
  byId('wName').textContent=(u&&u.name)?u.name:'there';
  byId('railAvatar').title=(u&&u.name)?u.name:'';
  byId('boot').classList.add('welcome');
  welcomeAt=performance.now();
  if(NAV==='home') render();
  maybeFinish();
};
function maybeFinish(){
  if(finished) return;
  if(userShown && dataReady){
    finished=true;
    const wait=Math.max(0,1500-(performance.now()-welcomeAt));
    setTimeout(finishIntro,wait);
  }
}
function finishIntro(){
  const app=byId('app'); app.classList.add('reveal');
  const wa=byId(byId('bootWelcome').classList.contains('noimg')?'wAvatarFallback':'wAvatar');
  const ra=byId('railAvatar');
  ra.classList.add('show');
  // Flip the welcome avatar to the HOMEPAGE hero spot (rail avatar also appears).
  const target=byId('homeAvatar')||ra;
  try{
    const a=wa.getBoundingClientRect(), b=target.getBoundingClientRect();
    const dx=(b.left+b.width/2)-(a.left+a.width/2);
    const dy=(b.top+b.height/2)-(a.top+a.height/2);
    const sc=b.width/a.width;
    if(target!==ra) target.style.visibility='hidden';
    wa.style.transition='transform .85s var(--spring),opacity .4s ease .6s';
    wa.style.transform='translate('+dx+'px,'+dy+'px) scale('+sc+')';
    byId('wName').style.transition='opacity .3s'; byId('wName').style.opacity='0';
    byId('wText').style.opacity='0'; byId('wText').style.transition='opacity .3s';
  }catch(e){}
  setTimeout(()=>{ const t=byId('homeAvatar'); if(t) t.style.visibility='visible'; byId('boot').classList.add('hide'); },820);
  setTimeout(()=>{ byId('boot').style.display='none'; maybeSplash(); },1500);
}
function onData(){
  dataReady=true;
  // If identity never arrives, dismiss the boot gracefully without a welcome.
  if(!userShown) setTimeout(()=>{ if(!finished&&!userShown){ finished=true; noWelcomeDismiss(); } },2200);
  maybeFinish();
}
function noWelcomeDismiss(){
  byId('app').classList.add('reveal');
  byId('boot').classList.add('hide');
  setTimeout(()=>{ byId('boot').style.display='none'; maybeSplash(); },700);
}

/* ====================== inbound from Rust ====================== */
window.setQuests=function(list){ GOT=true; QUESTS=list||[]; syncAuto(); syncAutoPlay(); render(); onData(); };
window.setError=function(msg){ GOT=true;
  byId('content').innerHTML='<div class="mid"><div class="ic">'+ICO.warn+'</div><h2>Couldn\'t read your quests</h2><p>'+esc(msg)+'</p><p>Make sure Discord is installed and you are signed in, then hit refresh.</p></div>';
  finished=true; noWelcomeDismiss();
};
window.setSettings=function(s){ SET=Object.assign(SET,s||{}); applyTheme();
  if(!navInit){ navInit=true; if(SET.default_page && TITLES[SET.default_page] && SET.default_page!=='home') setNav(SET.default_page); }
  if(NAV==='settings') render(); syncAuto(); maybeSplash(); maybeChangelog(); };
function setDefaultPage(p){ SET.default_page=p; send('setSettingStr',{key:'default_page',value:p}); render(); }
const ACCENTS={aurora:['#b794f6','#34d399','183,148,246'],emerald:['#3ddc84','#22d3ee','61,220,132'],sky:['#9ec5ff','#6ea8fe','158,197,255'],gold:['#f3c969','#e0a13a','243,201,105'],cyber:['#fcee0a','#00f0ff','252,238,10']};
function applyTheme(){
  const r=document.documentElement;
  r.setAttribute('data-theme',SET.theme==='light'?'light':'dark');
  const a=ACCENTS[SET.accent]||ACCENTS.aurora;
  r.style.setProperty('--accent',a[0]); r.style.setProperty('--accent-2',a[1]); r.style.setProperty('--accent-rgb',a[2]);
}
function setTheme(t){ SET.theme=t; applyTheme(); send('setSettingStr',{key:'theme',value:t}); render(); }
function setAccent(a){ SET.accent=a; applyTheme(); send('setSettingStr',{key:'accent',value:a}); render(); }
// First-run popup removed — the same disclaimer lives permanently in Settings.
function maybeSplash(){ }
function dismissSplash(){ SET.splash_seen=true; byId('splash').classList.remove('show'); send('setSetting',{key:'splash_seen',value:true}); }
window.setShop=function(list,err){ shopLoading=false; shopErr=err||null; SHOP=list||[]; if(NAV==='shop'||NAV==='profile') render(); };
window.setOwned=function(list){ OWNED=list||[]; if((NAV==='shop'&&shopFilter==='owned')||NAV==='profile') render(); };
window.setCatalog=function(list){ catLoading=false; CATALOG=list||[]; if(NAV==='profile') render(); };
window.setStudio=function(s){ if(s&&typeof s==='object'){ Object.assign(STU,s); } if(NAV==='profile'){ render(); stuApply(); } };
window.setEquipped=function(e){ EQUIPPED=e||null; if(NAV==='home'||NAV==='profile') render(); };
window.setHistory=function(h){ HISTORY=h||[]; if(NAV==='history') render(); };
window.setVersion=function(v){ APP_VERSION=v||''; maybeChangelog(); };
window.noUpdate=function(){ toast('You\'re on the latest version'); };
// Show the changelog once after an update (last-seen version differs from now).
function maybeChangelog(){
  if(changelogShown||!APP_VERSION||SET.last_seen_version===undefined) return;
  const seen=SET.last_seen_version||'';
  if(seen && seen!==APP_VERSION && CHANGELOG[0] && CHANGELOG[0].v===APP_VERSION){
    changelogShown=true; showChangelog(true);
  }
  if(seen!==APP_VERSION){ SET.last_seen_version=APP_VERSION; send('setSettingStr',{key:'last_seen_version',value:APP_VERSION}); }
}
function showChangelog(isUpdate){
  const e=CHANGELOG[0]; if(!e) return;
  const items=e.notes.map(n=>'<li>'+esc(n)+'</li>').join('');
  const w=document.createElement('div'); w.className='clog-wrap'; w.id='clogWrap';
  w.innerHTML='<div class="clog"><div class="clog-h"><div class="clog-badge">'+ICO.party+'</div>'
    +'<div><div class="clog-t">'+(isUpdate?'Updated to':'What\\u2019s new in')+' v'+esc(e.v)+'</div><div class="clog-d">'+esc(e.d)+'</div></div></div>'
    +'<ul class="clog-list">'+items+'</ul>'
    +'<button class="act primary" style="width:100%" onclick="byId(\'clogWrap\').remove()">Got it</button></div>';
  document.body.appendChild(w);
}
function checkUpdateNow(){ toast('Checking for updates…'); send('checkUpdate'); }
window.updateAvailable=function(info){
  if(!info||byId('updbar')) return;
  const b=document.createElement('div'); b.className='updbar'; b.id='updbar';
  b.innerHTML='<span class="upd-i">'+ICO.refresh+'</span><span class="upd-t">Update <b>v'+esc(info.version)+'</b> is available</span>'
    +'<button class="upd-go" onclick="doUpdate()">Update now</button>'
    +'<button class="upd-x" title="Later" onclick="byId(\'updbar\').remove()">'+ICO.close+'</button>';
  document.body.appendChild(b);
};
window.updateFailed=function(e){ const el=byId('updbar'); if(el){ el.classList.remove('busy'); const g=el.querySelector('.upd-go'); if(g) g.textContent='Update now'; } toast('Update failed: '+esc(e)); };
function doUpdate(){ const el=byId('updbar'); if(el){ el.classList.add('busy'); const g=el.querySelector('.upd-go'); if(g) g.textContent='Downloading…'; } send('applyUpdate'); }
window.setBadges=function(p){ BADGES=p||null; if(NAV==='badges'||NAV==='home'||NAV==='profile') render(); };
window.setStats=function(s){ STATS=s||null; if(NAV==='home'||NAV==='profile') render(); };
window.setOrbs=function(n){
  const el=byId('orbBal'); if(n==null) return;
  ORBS=n;
  byId('orbBalNum').textContent=(n>=100000)?(Math.round(n/100)/10)+'k':n.toLocaleString();
  el.title='You have '+n.toLocaleString()+' orbs';
  el.classList.add('show');
  if(NAV==='home') render();
};
window.claimResult=function(id,ok,payload){
  delete claiming[id];
  const q=QUESTS.find(x=>x.id===id);
  if(ok){ if(q){ q.claimed=true; } toast('Reward claimed'+(payload?' — '+payload+' orbs':''),'ok'); }
  else if(payload==='CAPTCHA'){
    if(q) q.captcha=true;
    toast('Discord asks for a captcha on this one — finish it in Discord','bad');
  }
  else{ toast('Claim failed: '+payload,'bad'); }
  render();
};
window.updateProgress=function(id,progress,target,completed){
  const q=QUESTS.find(x=>x.id===id);
  if(q){ q.progress=progress; if(completed&&!q.completed){ q.completed=true; onCompleted(q); } }
  const bar=byId('bar-'+id), txt=byId('txt-'+id);
  const pct=target?Math.min(100,Math.round(100*progress/target)):0;
  if(bar){ bar.classList.toggle('done',!!completed); bar.firstElementChild.style.width=(completed?100:pct)+'%'; }
  if(txt) txt.textContent=completed?'Completed':progress+' / '+target+'s watched';
  if(CUR&&CUR.id===id){
    byId('dockFill').style.width=(completed?100:pct)+'%';
    byId('dockBar').classList.toggle('done',!!completed);
    byId('dockStatus').textContent=completed?'Completed':'Watching · '+progress+' / '+target+'s';
    byId('pillTime').textContent=completed?'done':progress+'/'+target+'s';
    if(completed) finishCurrent();
  }
  if(NAV==='claim') render();
};
window.progressError=function(id,msg){ if(CUR&&CUR.id===id) byId('dockStatus').textContent='Discord: '+msg; };
// Record a completed quest toward all-time stats (once per quest).
let countedStats=new Set();
function onCompleted(q){
  if(!q||countedStats.has(q.id)) return;
  countedStats.add(q.id);
  send('stat',{orbs:(q.premiumOrbs||q.orbs||0),seconds:(q.target||0),name:(q.name||'Quest'),category:(q.category||'')});
  send('notify',{title:'Quest complete',body:(q.name||'A quest')+' is ready to claim'});
}

/* ====================== nav / filters ====================== */
const TITLES={home:'Home',video:'Watch Videos',game:'Play Games',claim:'Claim Rewards',shop:'Orb Shop',badges:'Badges',history:'Quest History',profile:'Your Profile',settings:'Settings'};
function setNav(n){
  NAV=n; byId('pageTitle').textContent=TITLES[n];
  document.querySelectorAll('.navbtn').forEach(b=>b.classList.toggle('active',b.dataset.nav===n));
  byId('railAvatar').classList.toggle('active',n==='profile');
  byId('toolbar').style.display=(['settings','shop','home','badges','profile'].includes(n))?'none':'flex';
  byId('watchAllBtn').style.display=(n==='video')?'flex':'none';
  byId('playAllBtn').style.display=(n==='game')?'flex':'none';
  byId('presenceBtn').style.display=(n==='game')?'flex':'none';
  byId('presenceBtn').classList.toggle('on',!!SET.show_presence);
  if(n==='shop' && SHOP===null && !shopLoading){ shopLoading=true; send('loadShop'); }
  render();
}
function onPill(){ if(CURPLAY) stopPlay(); else toggleDock(); }
function togglePresence(){ SET.show_presence=!SET.show_presence; byId('presenceBtn').classList.toggle('on',SET.show_presence); send('setSetting',{key:'show_presence',value:SET.show_presence}); toast(SET.show_presence?'Games will show on your profile':'Games hidden from your profile'); }
function setSort(s){
  SORT=s; byId('sortLabel').textContent={suggested:'Suggested',recent:'Most Recent',expiring:'Expiring Soon',started:'Started'}[s];
  document.querySelectorAll('.mi').forEach(m=>m.classList.toggle('sel',m.dataset.sort===s));
  byId('sortMenu').classList.remove('open'); render();
}
function toggleMenu(e){ e.stopPropagation(); byId('sortMenu').classList.toggle('open'); }
document.addEventListener('click',()=>{ byId('sortMenu').classList.remove('open'); const m=byId('sMenu'); if(m) m.classList.remove('open'); });
function toggleOrb(){ ORB=!ORB; byId('orbBtn').classList.toggle('on',ORB); render(); }
function rescan(){ byId('content').innerHTML='<div class="mid"><div class="spin"></div><p>Refreshing…</p></div>'; send('rescan'); }

function visible(){
  const term=(byId('q').value||'').trim().toLowerCase();
  let list;
  if(NAV==='claim') list=QUESTS.filter(q=>q.completed&&!q.claimed);
  else list=QUESTS.filter(q=>q.category===NAV&&!q.expired&&!q.claimed);
  if(ORB) list=list.filter(q=>q.orbs>0);
  if(term) list=list.filter(q=>(q.name||'').toLowerCase().includes(term)||(q.app||'').toLowerCase().includes(term));
  const t=s=>s?Date.parse(s)||0:0;
  const started=q=>(q.progress>0||q.enrolled)?0:1;
  if(SORT==='recent') list.sort((a,b)=>t(b.startsAt)-t(a.startsAt));
  else if(SORT==='expiring') list.sort((a,b)=>(t(a.expiresAt)||8e15)-(t(b.expiresAt)||8e15));
  else if(SORT==='started') list.sort((a,b)=>started(a)-started(b)||(b.progress||0)-(a.progress||0));
  else list.sort((a,b)=>(a.completed-b.completed)||a.idx-b.idx);
  return list;
}

/* ====================== badges ====================== */
const BADGE_ICON=h=>'https://cdn.discordapp.com/badge-icons/'+h+'.png?size=256';
// Newer/progressive badges ship a full asset URL; classic ones use the hash.
function badgeSrc(b){ return (b&&b.simple_icon_url)?b.simple_icon_url:BADGE_ICON(b&&b.icon); }
function nameFor(id){
  const m={staff:'Discord Staff',partner:'Partnered Server Owner',hypesquad:'HypeSquad Events',
    hypesquad_house_1:'HypeSquad Bravery',hypesquad_house_2:'HypeSquad Brilliance',hypesquad_house_3:'HypeSquad Balance',
    bug_hunter_level_1:'Bug Hunter',bug_hunter_level_2:'Bug Hunter Gold',early_supporter:'Early Supporter',
    premium_early_supporter:'Early Supporter',verified_developer:'Early Verified Bot Developer',
    certified_moderator:'Moderator Alumni',legacy_username:'Legacy Username',quest_completed:'Quest Completed',
    orb_profile_badge:'Orb Badge',gifting:'Gift Giver',bot_commands:'Supports Commands'};
  if(m[id]) return m[id];
  if(id.startsWith('guild_booster')) return 'Server Booster';
  if(id.startsWith('premium_tenure')||id==='premium') return 'Discord Nitro';
  return id.replace(/_v?\d*$/,'').replace(/_/g,' ').replace(/\b\w/g,c=>c.toUpperCase());
}
function earnedBadges(){ return (BADGES&&Array.isArray(BADGES.badges))?BADGES.badges:[]; }
function ageYears(){ if(!BADGES||!BADGES.createdMs) return 0; return (NOW()-BADGES.createdMs)/(365.25*864e5); }
function NOW(){ return Date.now(); }
function realTile(b,extra,locked){
  const nm=nameFor(b.id);
  return '<div class="btile'+(locked?' locked':'')+'" data-tip="'+esc(b.description||nm)+(locked?' · Locked':'')+'">'
    +'<div class="bimg"><img src="'+badgeSrc(b)+'" alt="" onerror="this.parentElement.innerHTML=\'🏅\'"></div>'
    +'<div class="bnm">'+esc(nm)+'</div>'+(extra?'<div class="bsub">'+esc(extra)+'</div>':'')+'</div>';
}
// Classic badges (known icon hashes) for the locked showcase.
const LOCKED_CATALOG=[
 {id:'staff',icon:'5e74e9b61934fc1f67c65515d1f7e60d'},
 {id:'partner',icon:'3f9748e53446a137a052f3454e2de41e'},
 {id:'hypesquad',icon:'bf01d1073931f921909045f3a39fd264'},
 {id:'bug_hunter_level_1',icon:'2717692c7dca7289b35297368a940dd0'},
 {id:'hypesquad_house_2',icon:'011940fd013da3f7fb926e4a1cd2e618'},
 {id:'hypesquad_house_3',icon:'3aa41de486fa12454c3761e8e223442e'},
 {id:'bug_hunter_level_2',icon:'848f79194d4be5ff5f81505cbd0ce1e6'},
 {id:'certified_moderator',icon:'fee1624003e2fee35cb398e125dc479b'},
];
const CLASSIC_DESC={staff:'Works at Discord.',partner:'Owns a Partnered community.',hypesquad:'A HypeSquad Events member.',
 bug_hunter_level_1:'Squashed bugs for Discord.',hypesquad_house_2:'HypeSquad House of Brilliance.',
 hypesquad_house_3:'HypeSquad House of Balance.',bug_hunter_level_2:'An elite bug hunter.',certified_moderator:'Certified Discord moderator.'};
// Escalating tier colours (Tier 1 → Tier 10), roughly matching Discord.
const TIER_C=['#4a8cff','#3ba55d','#2fd3b6','#e065c9','#9c84ef','#7b6ef6','#c0a13a','#f0b232','#f47fff','#ffd166'];
const GIFT_TIERS=[['Patron','Gifted 1×'],['Champion','Gifted 2×'],['Luminary','Gifted 3×'],['Icon','Gifted 6×'],['Hero','Gifted 10×'],['Legend','Gifted 20×']];
const VARIETY_TIERS=[['Sampler','2 games'],['Dabbler','5 games'],['Enthusiast','10 games'],['Ranger','15 games'],['Explorer','20 games'],['Adventurer','30 games'],['Voyager','40 games'],['Maverick','60 games'],['Polymath','80 games'],['Universalist','100+ games']];
const TIME_TIERS=[['Casual','1 hour'],['Recreational','5 hours'],['Dedicated','20 hours'],['Committed','75 hours'],['Serious','150 hours'],['Devoted','300 hours'],['Seasoned','500 hours'],['Ironclad','1000 hours'],['Unshakeable','2000 hours'],['Eternal','5000+ hours']];
const STREAM_TIERS=[['Newcomer','1 hour'],['Fledgling','5 hours'],['Breakout','20 hours'],['Standout','75 hours'],['Trendsetter','150 hours'],['Headliner','300 hours'],['Star','500 hours'],['Sensation','1000 hours'],['Visionary','2000 hours'],['Phenomenon','5000+ hours']];
const AGE_TIERS=[['Seed','1 year'],['Sprout','2 years'],['Bud','3 years'],['Sapling','4 years'],['Blossom','5 years'],['Redwood','6 years'],['Sequoia','7 years'],['Bristlecone','8 years'],['Stromatolite','9 years'],['Primordial','10+ years']];
function giftCurrentTier(){
  const g=earnedBadges().find(b=>b.id==='gifting'); if(!g) return 0;
  const d=(g.description||'').toLowerCase();
  for(let i=GIFT_TIERS.length-1;i>=0;i--) if(d.includes(GIFT_TIERS[i][0].toLowerCase())) return i+1;
  return 1; // has the badge but tier name not found
}
// The evolving Account Age / Game Variety / Game Time / Streaming badges (rolling
// out from Discord). Detect the earned badge by id keyword or a tier-name match,
// then read its current tier so the ladder lights up with the real icon.
function familyTier(tiers,idHints){
  const bs=earnedBadges();
  let b=bs.find(x=>idHints.some(h=>(x.id||'').toLowerCase().includes(h)));
  if(!b) b=bs.find(x=>tiers.some(t=>(x.description||'').toLowerCase().includes(t[0].toLowerCase())));
  if(!b) return {tier:0,badge:null};
  const d=((b.description||'')+' '+(b.id||'')).toLowerCase();
  for(let i=tiers.length-1;i>=0;i--) if(d.includes(tiers[i][0].toLowerCase())) return {tier:i+1,badge:b};
  return {tier:1,badge:b};
}
function tierFamily(title,emoji,intro,tiers,achievedCount,realIcon){
  const tiles=tiers.map((t,i)=>{
    const on=i<achievedCount, cur=i===achievedCount-1;
    const glyph=(cur&&realIcon)
      ? '<div class="bimg"><img src="'+realIcon+'" alt="" onerror="this.parentElement.textContent=\''+emoji+'\'"></div>'
      : '<div class="bimg gl" style="background:radial-gradient(circle at 50% 35%,color-mix(in srgb,'+TIER_C[i]+' 60%,transparent),transparent 70%),rgba(255,255,255,.05);border-color:color-mix(in srgb,'+TIER_C[i]+' 50%,var(--stroke))">'+emoji+'</div>';
    return '<div class="btile'+(on?'':' locked')+(cur?' current':'')+'" data-tip="'+esc(title+' · '+t[0]+' — '+t[1]+(on?' · achieved':' · locked'))+'" style="--bc:'+TIER_C[i]+'">'
      +glyph+'<div class="bnm">'+t[0]+'</div><div class="bsub">'+t[1]+'</div></div>';
  }).join('');
  return '<div class="bsec"><div class="bsec-h">'+title+'</div>'+(intro?'<div class="hempty" style="margin:-4px 0 12px">'+intro+'</div>':'')+'<div class="bgrid tiers">'+tiles+'</div></div>';
}
function badgesHtml(){
  if(!BADGES) return '<div class="mid"><div class="spin"></div><p>Reading your badges…</p></div>';
  const all=earnedBadges();
  const owned=all.filter(b=>b.id!=='gifting');
  const ownedNames=new Set(all.map(b=>nameFor(b.id)));
  const locked=LOCKED_CATALOG.filter(d=>!ownedNames.has(nameFor(d.id)));
  const ay=ageYears();
  let ageDone=0; AGE_TIERS.forEach((t,i)=>{ if(ay>=(i+1===10?10:i+1)) ageDone=i+1; });
  const giftDone=giftCurrentTier();

  let out='<div class="badges">';
  out+='<div class="bsec"><div class="bsec-h">Your badges <span>'+all.length+'</span></div><div class="bgrid">'
     +(owned.length?owned.map(b=>realTile(b)).join(''):'<div class="hempty">No profile badges yet.</div>')+'</div></div>';

  const giftBadge=earnedBadges().find(b=>b.id==='gifting');
  const age=familyTier(AGE_TIERS,['account_age','account age']);
  const variety=familyTier(VARIETY_TIERS,['game_variety','variety']);
  const gtime=familyTier(TIME_TIERS,['game_time','play_time','playtime']);
  const stream=familyTier(STREAM_TIERS,['stream']);
  const ageT=age.tier||ageDone; // fall back to computed age if the badge isn't rolled out yet
  const rolling='Discord is rolling this badge out — it lights up here automatically once it reaches your profile.';
  out+=tierFamily('Gift Giver','🎁', giftDone? 'You\'ve reached tier '+giftDone+' of 6 — send more gifts to evolve it.' : 'Send Nitro gifts to earn this evolving badge.', GIFT_TIERS, giftDone, giftBadge?badgeSrc(giftBadge):null);
  out+=tierFamily('Account Age','🌳', age.tier? ('Tier '+age.tier+' of 10.') : ('Your account is ~'+Math.floor(ay)+' years old (tier '+ageDone+'). '+rolling), AGE_TIERS, ageT, age.badge?badgeSrc(age.badge):null);
  out+=tierFamily('Game Variety','🎮', variety.tier? ('Tier '+variety.tier+' of 10.') : ('Play more detectable games with Discord open. '+rolling), VARIETY_TIERS, variety.tier, variety.badge?badgeSrc(variety.badge):null);
  out+=tierFamily('Game Time','⏱️', gtime.tier? ('Tier '+gtime.tier+' of 10.') : ('Play more hours of detectable PC games. '+rolling), TIME_TIERS, gtime.tier, gtime.badge?badgeSrc(gtime.badge):null);
  out+=tierFamily('Streaming','📹', stream.tier? ('Tier '+stream.tier+' of 10.') : ('Stream more hours to other users. '+rolling), STREAM_TIERS, stream.tier, stream.badge?badgeSrc(stream.badge):null);

  out+='<div class="bsec"><div class="bsec-h">Locked <span>'+locked.length+'</span></div><div class="bgrid">'
     +locked.map(d=>realTile({id:d.id,icon:d.icon,description:CLASSIC_DESC[d.id]},'',true)).join('')+'</div></div>';
  return out+'</div>';
}
function heroBadges(){
  const e=earnedBadges().slice(0,12);
  if(!e.length) return '';
  return '<div class="hbadges">'+e.map(b=>'<span class="hb" data-tip="'+esc(b.description||nameFor(b.id))+'"><img src="'+badgeSrc(b)+'" alt="" onerror="this.parentElement.textContent=\'🏅\'"></span>').join('')+'</div>';
}

/* ====================== avatar & banner studio ====================== */
function stuFilterAv(){ return 'brightness('+STU.avBright+'%) contrast('+STU.avContrast+'%) saturate('+STU.avSat+'%) hue-rotate('+STU.avHue+'deg)'; }
function stuFilterBn(){ return 'brightness('+STU.bnBright+'%) contrast('+STU.bnContrast+'%) saturate('+STU.bnSat+'%) hue-rotate('+STU.bnHue+'deg)'; }
// Display-name-style options (Discord-style: font + effect + colour).
const NFONTS=[['default','inherit'],['round','"Bricolage Grotesque",system-ui'],['serif','Georgia,"Times New Roman",serif'],
  ['slab','"Rockwell","Roboto Slab",serif'],['mono','"Cascadia Code","Consolas",monospace'],['condensed','"Arial Narrow","Bahnschrift Condensed",sans-serif'],
  ['wide','"Bahnschrift","Segoe UI",sans-serif'],['impact','Impact,"Arial Black",sans-serif'],['script','"Segoe Script","Brush Script MT",cursive'],
  ['playful','"Comic Sans MS",cursive'],['light','"Segoe UI Light","Century Gothic",system-ui'],['times','"Times New Roman",serif']];
const NEFFECTS=[['solid','Solid'],['gradient','Gradient'],['neon','Neon'],['toon','Toon'],['pop','Pop'],['gummy','Gummy'],['prism','Prism']];
const NCOLORS=[['#b794f6','#34d399'],['#f59e0b','#ef4444'],['#22d3ee','#6366f1'],['#fbbf24','#dc2626'],['#ff6ec4','#7873f5'],['#00f0ff','#5865f2'],['#ffd166','#f3c969'],['#ffffff','#c9c9d6']];
function fontStack(id){ const f=NFONTS.find(x=>x[0]===id); return f?f[1]:'inherit'; }
function applyEffectStyle(el,eff,cc){
  el.style.background=''; el.style.webkitBackgroundClip=''; el.style.backgroundClip=''; el.style.color=''; el.style.textShadow=''; el.style.webkitTextStroke='';
  const grad='linear-gradient(95deg,'+cc[0]+','+cc[1]+')', clip=()=>{ el.style.webkitBackgroundClip='text'; el.style.backgroundClip='text'; el.style.color='transparent'; };
  if(eff==='gradient'){ el.style.backgroundImage=grad; clip(); }
  else if(eff==='gummy'){ el.style.backgroundImage=grad; clip(); el.style.textShadow='0 2px 3px rgba(0,0,0,.4)'; }
  else if(eff==='prism'){ el.style.backgroundImage='linear-gradient(95deg,#ff5f6d,#ffc371,#3ee9c3,#5b9dff,#c86dff,#ff5f6d)'; clip(); }
  else if(eff==='neon'){ el.style.color=cc[0]; el.style.textShadow='0 0 5px '+cc[0]+',0 0 12px '+cc[1]+',0 0 22px '+cc[1]; }
  else if(eff==='toon'){ el.style.color='#fff'; el.style.webkitTextStroke='1.4px '+cc[0]; el.style.textShadow='2px 2px 0 rgba(0,0,0,.28)'; }
  else if(eff==='pop'){ el.style.color=cc[0]; el.style.textShadow='2px 2px 0 rgba(0,0,0,.4),4px 4px 0 rgba(0,0,0,.18)'; }
  else { el.style.color=cc[0]; } // solid
}
function applyNameStyle(){ const el=byId('stName'); if(!el) return; el.style.fontFamily=fontStack(STU.nameFont); applyEffectStyle(el,STU.nameEffect||'solid',NCOLORS[STU.nameColor||0]||NCOLORS[0]); }
function decorateNameTiles(){ const cc=NCOLORS[STU.nameColor||0]||NCOLORS[0]; NEFFECTS.forEach(e=>{ const el=byId('efs_'+e[0]); if(el) applyEffectStyle(el,e[0],cc); }); }
// Persist the studio look (debounced) so it's remembered across launches.
let studioSaveT=0;
function studioBlob(){ return {deco:STU.deco,nameplate:STU.nameplate,effect:STU.effect,effectAnim:STU.effectAnim,frame:STU.frame,frameLayers:STU.frameLayers,frameMetrics:STU.frameMetrics,
  nameFont:STU.nameFont,nameEffect:STU.nameEffect,nameColor:STU.nameColor,themeA:STU.themeA,themeB:STU.themeB,open:STU.open,
  avBright:STU.avBright,avContrast:STU.avContrast,avSat:STU.avSat,avHue:STU.avHue,zoom:STU.zoom,
  bnBright:STU.bnBright,bnContrast:STU.bnContrast,bnSat:STU.bnSat,bnHue:STU.bnHue}; }
function saveStudio(){ clearTimeout(studioSaveT); studioSaveT=setTimeout(()=>send('saveStudio',{data:studioBlob()}),400); }
// Render a profile effect: animated APNG layers if available, else a static image.
function fxRender(el,anim,img){
  if(!el) return;
  if(anim&&anim.length){ el.style.display='block'; el.style.backgroundImage='';
    el.innerHTML=anim.map(s=>'<img class="fx-layer" src="'+esc(s)+'">').join(''); }
  else if(img){ el.style.display='block'; el.innerHTML=''; el.style.backgroundImage='url("'+img+'")'; }
  else { el.style.display='none'; el.innerHTML=''; el.style.backgroundImage=''; }
}
// A frame wraps AROUND the profile: "back" layers sit behind the avatar/text,
// "front" layers (the top crown) sit in front. Layers keep their natural aspect
// so the transparent centre lets the profile show through.
// Inset the card into the frame's window: the frame extends past the profile by
// overflow_top/bottom/horizontal (as a fraction of its full width), so we margin
// the card in by exactly those amounts and let the frame fill the wrapper around.
function frameMargin(fm){ if(!fm) return ''; const w=(fm.iw||0)+2*(fm.oh||0); if(w<=0) return ''; return 'margin:'+(fm.ot/w*100).toFixed(3)+'% '+(fm.oh/w*100).toFixed(3)+'% '+(fm.ob/w*100).toFixed(3)+'%'; }
// Static frame layer HTML (for the read-only equipped card).
function frameHtml(cls,layers){ if(!layers||!layers.length) return ''; return '<div class="pp-frame '+cls+'">'+layers.map(l=>'<img class="fl fl-'+esc(l.anchor)+'" src="'+esc(l.url)+'">').join('')+'</div>'; }
function frameRender(layers){
  const back=byId('stFrameBack'), front=byId('stFrameFront');
  const put=(el,ls)=>{ if(!el) return; if(ls&&ls.length){ el.style.display='block';
      el.innerHTML=ls.map(l=>'<img class="fl fl-'+esc(l.anchor)+'" src="'+esc(l.url)+'">').join(''); }
    else { el.style.display='none'; el.innerHTML=''; } };
  const ls=layers||[];
  put(back, ls.filter(l=>l.order!=='front'));
  put(front, ls.filter(l=>l.order==='front'));
}
function stuApply(){
  const av=byId('stAvatar'), bn=byId('stBanner');
  if(av){ av.style.filter=stuFilterAv(); av.style.backgroundImage=STU.avatar?'url("'+STU.avatar+'")':''; av.style.backgroundSize=STU.zoom+'%'; av.style.backgroundPosition=STU.posX+'% '+STU.posY+'%'; }
  if(bn){ bn.style.filter=stuFilterBn(); bn.style.backgroundImage=STU.banner?'url("'+STU.banner+'")':''; }
  const set=(id,url)=>{ const e=byId(id); if(e){ e.style.backgroundImage=url?'url("'+url+'")':''; e.style.display=url?'block':'none'; } };
  set('stDeco',STU.deco); set('stNameplate',STU.nameplate);
  fxRender(byId('stEffect'),STU.effectAnim,STU.effect);
  frameRender(STU.frameLayers);
  applyNameStyle();
  const pv=byId('ppreview');
  if(pv){ pv.style.setProperty('--pt',STU.themeA||'#232428'); pv.style.setProperty('--pt2',STU.themeB||STU.themeA||'#232428'); }
}
function stuSet(k,v,el){ STU[k]=+v; if(el){ const b=el.parentElement.querySelector('b'); if(b) b.textContent=v+(el.dataset.u||''); } stuApply(); saveStudio(); }
function stuUpload(kind){
  const inp=document.createElement('input'); inp.type='file'; inp.accept='image/*,image/gif';
  inp.onchange=()=>{ const f=inp.files[0]; if(!f) return; const r=new FileReader(); r.onload=()=>{ STU[kind]=r.result; stuApply(); }; r.readAsDataURL(f); };
  inp.click();
}
function catItem(sku){ return (CATALOG||SHOP||[]).find(i=>i.sku===sku); }
// Toggle a collectible selection (deco/nameplate/effect/frame) by SKU.
function stuPick(kind,sku){
  if(!sku){ STU[kind]=''; if(kind==='frame'){STU.frameLayers=null;STU.frameMetrics=null;} if(kind==='effect')STU.effectAnim=null; stuApply(); saveStudio(); render(); return; }
  const it=catItem(sku); if(!it) return;
  const same=STU[kind]===it.image;
  STU[kind]=same?'':it.image;
  if(kind==='frame'){ STU.frameLayers=same?null:(it.layers||null); STU.frameMetrics=same?null:(it.metrics||null); }
  if(kind==='effect') STU.effectAnim=same?null:(it.anim||null);
  stuApply(); saveStudio(); render();
}
function stuFont(id){ STU.nameFont=id; applyNameStyle(); saveStudio(); render(); }
function stuEffect(id){ STU.nameEffect=id; applyNameStyle(); saveStudio(); render(); }
function stuColor(i){ STU.nameColor=i; applyNameStyle(); saveStudio(); render(); }
function stuTheme(which,hex){ const k='theme'+which; STU[k]=(STU[k]===hex?'':hex); stuApply(); saveStudio(); render(); }
// Live colour-picker updates (no full re-render, so the OS picker stays open).
function stuThemeVal(which,hex,el){ STU['theme'+which]=hex; stuApply(); saveStudio(); if(el){ const l=el.parentElement.querySelector('.colhex'); if(l) l.textContent=hex; } }
function stuOpen(s){ STU.open=(STU.open===s?'':s); saveStudio(); render(); }
function stuReset(){
  Object.assign(STU,{deco:'',nameplate:'',effect:'',effectAnim:null,frame:'',frameLayers:null,frameMetrics:null,nameFont:'default',nameEffect:'solid',nameColor:0,themeA:'',themeB:'',
    avBright:100,avContrast:100,avSat:100,avHue:0,zoom:100,posX:50,posY:50,bnBright:100,bnContrast:100,bnSat:100,bnHue:0});
  saveStudio(); render(); toast('Reset to default');
}
function stuExport(kind){
  const isGif=(STU[kind]||'').slice(0,14).includes('image/gif');
  const c=document.createElement('canvas'); const ctx=c.getContext('2d');
  const img=new Image(); img.onload=()=>{
    if(kind==='avatar'){ c.width=c.height=256; ctx.filter=stuFilterAv();
      // cover crop with zoom+position
      const z=STU.zoom/100, s=Math.max(256/img.width,256/img.height)*z, w=img.width*s, h=img.height*s;
      const x=(256-w)*(STU.posX/100), y=(256-h)*(STU.posY/100);
      ctx.save(); ctx.beginPath(); ctx.arc(128,128,128,0,7); ctx.clip(); ctx.drawImage(img,x,y,w,h); ctx.restore();
      if(STU.deco){ const d=new Image(); d.onload=()=>{ ctx.filter='none'; ctx.drawImage(d,-24,-24,304,304); save(); }; d.onerror=save; d.src=STU.deco; return; }
    } else { c.width=680; c.height=240; ctx.filter=stuFilterBn(); const s=Math.max(680/img.width,240/img.height), w=img.width*s, h=img.height*s; ctx.drawImage(img,(680-w)/2,(240-h)/2,w,h); }
    save();
    function save(){ send('saveImage',{name:'aurora-'+kind, data:c.toDataURL('image/png')}); toast(isGif?'Saved current frame as PNG (animated export coming soon)':'Saved to your Downloads'); }
  };
  img.onerror=()=>toast('Upload an image first');
  img.src=STU[kind]||''; if(!STU[kind]) toast('Upload a '+kind+' first');
}
function setupStuDrag(){
  const av=byId('stAvatar'); if(!av) return;
  let dragging=false,lx=0,ly=0;
  av.onmousedown=e=>{ if(!STU.avatar) return; dragging=true; lx=e.clientX; ly=e.clientY; e.preventDefault(); };
  document.addEventListener('mousemove',e=>{ if(!dragging) return; STU.posX=Math.max(0,Math.min(100,STU.posX+(lx-e.clientX)/2)); STU.posY=Math.max(0,Math.min(100,STU.posY+(ly-e.clientY)/2)); lx=e.clientX; ly=e.clientY; stuApply(); });
  document.addEventListener('mouseup',()=>dragging=false);
}
// The full catalog for one collectible kind (owned + non-orb included), plus a
// "None" option. Every item is selectable for preview.
function stuPicker(kind){
  if(CATALOG===null) return '<div class="pp-load">Loading collectibles…</div>';
  const items=(CATALOG||[]).filter(i=>i.kind===kind&&i.image);
  const none='<button class="ppick none'+(STU[kind]?'':' on')+'" onclick="stuPick(\''+kind+'\',\'\')" data-tip="None">'+ICO.close+'</button>';
  if(!items.length) return '<div class="pgrid">'+none+'<div class="pp-load">No '+kind+' items available.</div></div>';
  const tiles=items.map(i=>'<button class="ppick'+(STU[kind]===i.image?' on':'')+'" data-tip="'+esc(i.name)+(i.orbs?'':' · not orb')+'" onclick="stuPick(\''+kind+'\',\''+esc(i.sku)+'\')"><img src="'+esc(i.image)+'" loading="lazy" onerror="this.style.opacity=.12"></button>').join('');
  return '<div class="pgrid"><div class="pgrid-c">'+items.length+' options</div>'+none+tiles+'</div>';
}
function stuSlider(k,l,min,max,unit){
  return '<label class="sl"><span>'+l+'</span><input type="range" min="'+min+'" max="'+max+'" value="'+STU[k]+'" data-u="'+(unit||'')+'" oninput="stuSet(\''+k+'\',this.value,this)"><b>'+STU[k]+(unit||'')+'</b></label>';
}
// A collapsible customizer section (Discord-style left rail).
function stuSection(id,title,swatchHtml,body){
  const open=STU.open===id;
  return '<div class="cust'+(open?' open':'')+'">'
    +'<button class="cust-h" onclick="stuOpen(\''+id+'\')"><span class="cust-t">'+title+'</span>'
      +'<span class="cust-sw">'+(swatchHtml||'')+'</span>'
      +'<svg class="cust-cv" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg></button>'
    +(open?'<div class="cust-b">'+body+'</div>':'')+'</div>';
}
function swImg(url,fallback){ return url?'<span class="mini" style="background-image:url(\''+esc(url)+'\')"></span>':(fallback||''); }
function nameStyleSwatch(){ const cc=NCOLORS[STU.nameColor||0]||NCOLORS[0]; return '<span class="mini nsw" style="background:linear-gradient(135deg,'+cc[0]+','+cc[1]+');-webkit-background-clip:text;background-clip:text;color:transparent;font-family:'+fontStack(STU.nameFont).replace(/"/g,'&quot;')+'">Aa</span>'; }
// The user's REAL equipped Discord profile (avatar, decoration, banner,
// nameplate, effect, name style, pronouns, connections) — read-only.
function equippedProfile(){
  const e=EQUIPPED||{};
  const name=esc(e.name||(USER&&USER.name)||'You');
  const uname=esc(e.username||(name.toLowerCase().replace(/[^a-z0-9]/g,'')||'you'));
  const av=e.avatar||(USER&&USER.avatar)||'';
  const cols=(e.nameColors&&e.nameColors.length)?e.nameColors:null;
  const c2=cols?(cols.find(c=>c&&c!==cols[0])||cols[0]):null;
  const nameSty=cols?('background:linear-gradient(95deg,'+cols[0]+','+c2+');-webkit-background-clip:text;background-clip:text;color:transparent'):'';
  const pt=e.themeA||'#232428', pt2=e.themeB||e.themeA||'#232428';
  const bstyle=e.banner?('background-image:url(\''+esc(e.banner)+'\')'):'';
  const since=(BADGES&&BADGES.createdMs)?new Date(BADGES.createdMs).toLocaleDateString(undefined,{day:'2-digit',month:'short',year:'numeric'}):'—';
  const orbs=(ORBS!=null)?ORBS.toLocaleString():'—';
  const conns=(e.connections||[]).slice(0,4).map(c=>'<div class="pp-conn"><span class="pp-conn-d"></span><span class="pp-conn-n">'+esc(c.name||'')+'</span><span class="pp-conn-t">'+esc(c.type||'')+'</span></div>').join('');
  const fx=(e.effectAnim&&e.effectAnim.length)?('<div class="pp-effect" style="display:block">'+e.effectAnim.map(s=>'<img class="fx-layer" src="'+esc(s)+'">').join('')+'</div>'):'';
  const fl=e.frameLayers||[];
  const fBack=frameHtml('back',fl.filter(l=>l.order!=='front')), fFront=frameHtml('front',fl.filter(l=>l.order==='front'));
  return '<div class="pp-wrap">'+fBack
    +'<div class="ppreview pp-static" style="--pt:'+esc(pt)+';--pt2:'+esc(pt2)+';'+frameMargin(e.frameMetrics)+'">'
    +fx
    +'<div class="pp-banner" style="'+bstyle+'"></div>'
    +'<div class="pp-avwrap"><div class="pp-av">'+(av?'<img src="'+esc(av)+'">':'<span>'+name.charAt(0).toUpperCase()+'</span>')+'</div>'
      +(e.decoration?'<div class="pp-deco" style="display:block;background-image:url(\''+esc(e.decoration)+'\')"></div>':'')
      +'<span class="pp-status"></span></div>'
    +'<div class="pp-body">'
      +(e.nameplate?'<div class="pp-nameplate" style="display:block;background-image:url(\''+esc(e.nameplate)+'\')"></div>':'')
      +'<div class="pp-name" style="'+nameSty+'">'+name+'</div>'
      +'<div class="pp-tag">'+uname+(e.pronouns?' · <span>'+esc(e.pronouns)+'</span>':'')+'</div>'
      +heroBadges()
      +'<div class="pp-btns"><span class="pp-btn wide">Message</span><span class="pp-btn">'+ICO.gift+'</span><span class="pp-btn">•••</span></div>'
      +'<div class="pp-details">'
        +(e.bio?('<div class="pp-sub">About me</div><div class="pp-bio">'+esc(e.bio)+'</div>'):'')
        +'<div class="pp-sub">Member Since</div><div class="pp-val">'+since+'</div>'
        +(conns?('<div class="pp-sub">Connections</div>'+conns):'')
        +'<div class="pp-sub">Aurora Orbs</div><div class="pp-val"><span class="orb-dot"></span> '+orbs+' orbs</div>'
      +'</div>'
    +'</div></div>'+fFront+'</div>';
}
// The Discord-style live profile card. `mode==='studio'` makes it the sticky,
// editable preview; otherwise it's the read-only card shown on the homepage.
function profileCard(mode){
  const name=esc((USER&&USER.name)||'You');
  const handle=name.toLowerCase().replace(/[^a-z0-9]/g,'')||'you';
  const badges=heroBadges();
  const orbs=(ORBS!=null)?ORBS.toLocaleString():'—';
  const since=(BADGES&&BADGES.createdMs)?new Date(BADGES.createdMs).toLocaleDateString(undefined,{day:'2-digit',month:'short',year:'numeric'}):'—';
  const sticky=(mode==='studio');
  const tA=STU.themeA||'#232428', tB=STU.themeB||STU.themeA||'#232428';
  return '<div class="pp-wrap" id="ppWrap">'
    +'<div class="pp-frame back" id="stFrameBack"></div>'
    +'<div class="ppreview'+(sticky?' pp-sticky':' pp-static')+'" id="ppreview" style="--pt:'+tA+';--pt2:'+tB+';'+frameMargin(STU.frameMetrics)+'" onclick="ppClick()">'
    +'<div class="pp-effect" id="stEffect"></div>'
    +'<div class="pp-banner" id="stBanner"></div>'
    +'<div class="pp-avwrap"><div class="pp-av" id="stAvatar">'+(STU.avatar?'':(USER&&USER.avatar?'<img src="'+esc(USER.avatar)+'">':'<span>'+name.charAt(0).toUpperCase()+'</span>'))+'</div><div class="pp-deco" id="stDeco"></div><span class="pp-status"></span></div>'
    +'<div class="pp-body">'
      +'<div class="pp-nameplate" id="stNameplate"></div>'
      +'<div class="pp-name" id="stName">'+name+'</div>'
      +'<div class="pp-tag">'+esc(handle)+' · <span>Aurora</span></div>'
      +(badges||'')
      +'<div class="pp-btns"><span class="pp-btn wide">Message</span><span class="pp-btn">'+ICO.gift+'</span><span class="pp-btn">•••</span></div>'
      +'<div class="pp-details">'
        +'<div class="pp-sub">About me</div><div class="pp-bio">'+(mode==='home'?'Playing Quests with Aurora ✦':'Write a brief intro…')+'</div>'
        +'<div class="pp-sub">Member Since</div><div class="pp-val">'+since+'</div>'
        +'<div class="pp-sub">Aurora Orbs</div><div class="pp-val"><span class="orb-dot"></span> '+orbs+' orbs</div>'
      +'</div>'
      +(sticky?'<div class="pp-hint" id="ppHint">Live preview — edit on the left</div>':'')
    +'</div></div><div class="pp-frame front" id="stFrameFront"></div></div>';
}
// When the sticky preview is tucked into the corner, a click returns to the top.
function ppClick(){ const pv=byId('ppreview'); if(pv&&pv.classList.contains('tucked')) byId('content').scrollTo({top:0,behavior:'smooth'}); }
// Tuck the sticky preview into a small top-right popout once the page scrolls.
function setupStickyPreview(){
  const c=byId('content'); if(!c) return;
  if(c._ppScroll) c.removeEventListener('scroll',c._ppScroll);
  c._ppScroll=function(){
    const pv=byId('ppreview');
    if(!pv||!pv.classList.contains('pp-sticky')) return;
    pv.classList.toggle('tucked',c.scrollTop>200);
  };
  c.addEventListener('scroll',c._ppScroll); c._ppScroll();
}
function studioHtml(){
  // ---- customizer (left): one dropdown per collectible type ----
  const fonts='<div class="ff-l">Choose font</div><div class="fontgrid">'
    +NFONTS.map(f=>'<button class="fontpick'+(STU.nameFont===f[0]?' on':'')+'" onclick="stuFont(\''+f[0]+'\')" style="font-family:'+f[1].replace(/"/g,'&quot;')+'">Gg</button>').join('')+'</div>';
  const effs='<div class="ff-l">Choose effect</div><div class="effgrid">'
    +NEFFECTS.map(e=>'<button class="effpick'+(STU.nameEffect===e[0]?' on':'')+'" onclick="stuEffect(\''+e[0]+'\')"><span id="efs_'+e[0]+'">'+e[1]+'</span></button>').join('')+'</div>';
  const cols='<div class="ff-l">Choose colour</div><div class="colgrid">'
    +NCOLORS.map((c,i)=>'<button class="colpick'+(STU.nameColor===i?' on':'')+'" onclick="stuColor('+i+')" style="background:linear-gradient(135deg,'+c[0]+','+c[1]+')"></button>').join('')+'</div>';
  const themeDot=(STU.themeA||STU.themeB)?('<span class="mini" style="background:linear-gradient(135deg,'+(STU.themeA||'#555')+','+(STU.themeB||STU.themeA||'#555')+')"></span>'):'';
  const themeSw=(which)=>'<div class="swatches wrap">'+THEMESW.map(h=>'<button class="swatch'+(STU['theme'+which]===h?' on':'')+'" style="--sw:'+h+'" onclick="stuTheme(\''+which+'\',\''+h+'\')"></button>').join('')+'</div>';
  const colInput=(which)=>{ const set=STU['theme'+which]; return '<div class="colrow"><input type="color" class="colpk" value="'+(set||'#5865f2')+'" oninput="stuThemeVal(\''+which+'\',this.value,this)"><span class="colhex">'+esc(set||'None')+'</span>'+(set?'<button class="colclr" onclick="stuTheme(\''+which+'\',\''+set+'\')" title="Clear">'+ICO.close+'</button>':'')+'</div>'; };
  const cust=
     stuSection('nameplate','Nameplate',swImg(STU.nameplate), stuPicker('nameplate'))
    +stuSection('avatar','Avatar',swImg(STU.avatar,'<span class="mini ph"></span>'),
        '<div class="st-row"><button class="act ghost" onclick="stuUpload(\'avatar\')">⬆ Upload avatar</button></div>'
        +stuSlider('avBright','Brightness',20,180,'%')+stuSlider('avContrast','Contrast',20,180,'%')
        +stuSlider('avSat','Saturation',0,200,'%')+stuSlider('avHue','Hue',0,360,'°')+stuSlider('zoom','Zoom',100,300,'%')
        +'<div class="hempty">Drag the avatar in the preview to reposition.</div>'
        +'<div class="st-row"><button class="act primary" onclick="stuExport(\'avatar\')">Save avatar</button></div>')
    +stuSection('decoration','Decoration',swImg(STU.deco,'<span class="mini ph"></span>'), stuPicker('deco'))
    +stuSection('namestyle','Display Name Style',nameStyleSwatch(), fonts+effs+cols)
    +stuSection('theme','Theme',themeDot,
        '<div class="ff-l">Primary colour</div>'+colInput('A')+themeSw('A')
        +'<div class="ff-l">Secondary colour</div>'+colInput('B')+themeSw('B')
        +'<div class="hempty">Like Discord, the two colours tint your profile (the area under the banner) — not the banner itself. Use the picker for any colour.</div>')
    +stuSection('banner','Banner',swImg(STU.banner),
        '<div class="st-row"><button class="act ghost" onclick="stuUpload(\'banner\')">⬆ Upload banner</button></div>'
        +stuSlider('bnBright','Brightness',20,180,'%')+stuSlider('bnContrast','Contrast',20,180,'%')
        +stuSlider('bnSat','Saturation',0,200,'%')+stuSlider('bnHue','Hue',0,360,'°')
        +'<div class="st-row"><button class="act primary" onclick="stuExport(\'banner\')">Save banner</button></div>')
    +stuSection('effect','Profile Effect',swImg(STU.effect,''), stuPicker('effect'))
    +stuSection('frame','Frame',swImg(STU.frame,''), stuPicker('frame'));
  return '<div class="hsec"><div class="hsec-head"><h3>Profile studio</h3>'
      +'<button class="viewall" onclick="stuReset()">Reset all</button></div>'
    +'<div class="studio-top"><div class="studio-preview">'+profileCard('studio')+'</div></div>'
    +'<div class="hempty" style="margin:2px 0 14px">Preview every profile collectible — every decoration, nameplate, effect, frame, name style and theme, owned or not — on your own profile. Your look is remembered until you Reset. Export edited avatars &amp; banners (GIF supported).</div>'
    +'<div class="pcust">'+cust+'</div></div>';
}
const THEMESW=['#5865f2','#b794f6','#34d399','#22d3ee','#f472b6','#f59e0b','#ef4444','#8b5cf6','#0ea5e9','#111827'];

/* ====================== stats + profile ====================== */
function fmtTime(sec){ const h=Math.floor(sec/3600), m=Math.floor((sec%3600)/60); return h>0?h+'h '+m+'m':m+'m'; }
function statTiles(){
  const s=STATS||{orbs_earned:0,quests_completed:0,seconds_farmed:0,streak_days:0};
  const items=[
    ['🔮',(s.orbs_earned||0).toLocaleString(),'Orbs earned'],
    ['✅',(s.quests_completed||0).toLocaleString(),'Quests completed'],
    ['⏱️',fmtTime(s.seconds_farmed||0),'Time farmed'],
    ['🔥',(s.streak_days||0)+(s.streak_days===1?' day':' days'),'Daily streak'],
  ];
  return items.map(i=>'<div class="stat"><div class="stat-ic">'+i[0]+'</div><div class="stat-v">'+i[1]+'</div><div class="stat-l">'+i[2]+'</div></div>').join('');
}
const PTYPES=[[0,'Playing'],[2,'Listening to'],[3,'Watching'],[5,'Competing in']];
function presenceVerb(){ return (PTYPES.find(p=>p[0]===pType)||PTYPES[0])[1]; }
function cyclePType(){ const i=PTYPES.findIndex(p=>p[0]===pType); pType=PTYPES[(i+1)%PTYPES.length][0]; const h=byId('rpHead'); if(h) h.firstChild.textContent=presenceVerb().toUpperCase()+' '; }
function edit(field,el){ const t=el.textContent.trim(); if(field==='name')pName=t; else if(field==='details')pDetails=t; else pState=t; }
function editTip(field,el){ const t=el.textContent.trim(); if(field==='large')pLargeText=t; else pSmallText=t; }
// Set a presence image. A hosted URL shows on your live profile; a local upload
// previews in-app only (Discord can't render local files as presence art).
function rpImg(slot){
  let url=null; try{ url=window.prompt('Paste an image URL to show on your live Discord profile.\n(Leave blank / cancel to upload a local image for in-app preview only.)',''); }catch(e){}
  if(url&&url.trim()){ if(slot==='large')pLargeImg=url.trim(); else pSmallImg=url.trim(); render(); return; }
  const inp=document.createElement('input'); inp.type='file'; inp.accept='image/*';
  inp.onchange=()=>{ const f=inp.files[0]; if(!f) return; const r=new FileReader(); r.onload=()=>{ if(slot==='large')pLargeImg=r.result; else pSmallImg=r.result; render(); }; r.readAsDataURL(f); };
  inp.click();
}
function rpImgClear(slot,e){ if(e)e.stopPropagation(); if(slot==='large')pLargeImg=''; else pSmallImg=''; render(); }
// A Discord-style activity card you edit by clicking the text/art directly.
function presenceEditorCard(){
  const large=pLargeImg
    ? '<div class="rp-art" onclick="rpImg(\'large\')" data-tip="'+esc(pLargeText||'Large image')+'"><img src="'+esc(pLargeImg)+'"><button class="rp-x" onclick="rpImgClear(\'large\',event)">'+ICO.close+'</button></div>'
    : '<button class="rp-art empty" onclick="rpImg(\'large\')" title="Add large image"><span>'+ICO.play+'</span><small>Large image</small></button>';
  const small=pSmallImg
    ? '<div class="rp-sm" onclick="rpImg(\'small\')" data-tip="'+esc(pSmallText||'Small image')+'"><img src="'+esc(pSmallImg)+'"><button class="rp-x sm" onclick="rpImgClear(\'small\',event)">'+ICO.close+'</button></div>'
    : '<button class="rp-sm empty" onclick="rpImg(\'small\')" title="Add small image">+</button>';
  return '<div class="rpcard">'
    +'<div class="rp-head" id="rpHead" onclick="cyclePType()" title="Click to change activity type">'+presenceVerb().toUpperCase()+' '
      +'<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg></div>'
    +'<div class="rp-body"><div class="rp-imgs">'+large+small+'</div><div class="rp-lines">'
      +'<div class="rp-name" contenteditable="true" data-ph="Name of your game or activity" oninput="edit(\'name\',this)">'+esc(pName)+'</div>'
      +'<div class="rp-line" contenteditable="true" data-ph="Add a second line (optional)" oninput="edit(\'details\',this)">'+esc(pDetails)+'</div>'
      +'<div class="rp-line" contenteditable="true" data-ph="Add a third line (optional)" oninput="edit(\'state\',this)">'+esc(pState)+'</div>'
    +'</div></div>'
    +'<div class="rp-tips">'
      +'<label class="rp-tl"><span>Large image text</span><div class="rp-te" contenteditable="true" data-ph="Shown when hovering the big image" oninput="editTip(\'large\',this)">'+esc(pLargeText)+'</div></label>'
      +'<label class="rp-tl"><span>Small image text</span><div class="rp-te" contenteditable="true" data-ph="Shown when hovering the small image" oninput="editTip(\'small\',this)">'+esc(pSmallText)+'</div></label>'
    +'</div></div>'
    +'<div class="rp-actions"><button class="act primary" onclick="broadcastPresence()">'+ICO.play+'Broadcast to profile</button>'
    +'<button class="act ghost" onclick="clearPresenceUI()">Clear</button></div>';
}
function broadcastPresence(){
  if(!pName.trim()){ toast('Click the name and type something first'); return; }
  send('setPresence',{atype:pType,name:pName,details:pDetails,state:pState,
    largeImg:pLargeImg,largeText:pLargeText,smallImg:pSmallImg,smallText:pSmallText});
  toast('Now showing on your profile: '+presenceVerb()+' '+pName);
}
function clearPresenceUI(){ send('clearPresence'); toast('Presence cleared'); }
function profileHtml(){
  return '<div class="profile">'
    +studioHtml()
    +'<div class="hsec"><div class="hsec-head"><h3>All-time stats</h3></div><div class="statgrid">'+statTiles()+'</div></div>'
    +'<div class="hsec"><div class="hsec-head"><h3>Custom Rich Presence</h3></div>'
      +'<div class="hempty" style="margin:-4px 0 12px">Click the activity type or any line to edit it — just like Discord. It shows on your profile to friends while the app is open (a quest game overrides it).</div>'
      +'<div class="rpwrap">'+presenceEditorCard()+'</div></div>'
    +'</div>';
}

/* ====================== quest history ====================== */
function fmtDate(s){ try{ return new Date(s+'T00:00:00').toLocaleDateString(undefined,{weekday:'short',day:'numeric',month:'short',year:'numeric'}); }catch(e){ return s; } }
function calendarHeat(counts){
  const today=new Date(); today.setHours(0,0,0,0);
  const days=98, cells=[];
  for(let i=days-1;i>=0;i--){ const d=new Date(today); d.setDate(today.getDate()-i);
    const key=d.getFullYear()+'-'+String(d.getMonth()+1).padStart(2,'0')+'-'+String(d.getDate()).padStart(2,'0');
    cells.push({key,c:counts[key]||0,dow:d.getDay()}); }
  let grid=''; for(let i=0;i<cells[0].dow;i++) grid+='<span class="heat pad"></span>';
  cells.forEach(c=>{ const lvl=c.c===0?0:c.c<2?1:c.c<4?2:3;
    grid+='<span class="heat l'+lvl+'" data-tip="'+c.key+' · '+c.c+' quest'+(c.c===1?'':'s')+'"></span>'; });
  return '<div class="heatwrap"><div class="heatgrid">'+grid+'</div><div class="heatkey">Less '
    +'<span class="heat l0"></span><span class="heat l1"></span><span class="heat l2"></span><span class="heat l3"></span> More</div></div>';
}
function historyHtml(){
  if(HISTORY===null) return '<div class="mid"><div class="spin"></div><p>Loading history…</p></div>';
  if(!HISTORY.length) return '<div class="mid"><div class="ic">'+ICO.empty+'</div><h2>No quest history yet</h2><p>Every quest you finish gets logged here with the date — your activity builds up over time.</p></div>';
  const items=HISTORY.slice().reverse();
  const totalOrbs=HISTORY.reduce((s,e)=>s+(e.orbs||0),0);
  const byDate={}; items.forEach(e=>{ (byDate[e.date]=byDate[e.date]||[]).push(e); });
  const dates=Object.keys(byDate).sort((a,b)=>b.localeCompare(a));
  const counts={}; HISTORY.forEach(e=>counts[e.date]=(counts[e.date]||0)+1);
  const groups=dates.map(d=>{
    const rows=byDate[d].map(e=>'<div class="hist-row"><span class="hist-cat '+esc(e.category||'')+'">'+(e.category==='game'?ICO.play:ICO.check)+'</span>'
      +'<span class="hist-n">'+esc(e.name)+'</span>'+(e.orbs?'<span class="hist-o"><span class="orb-dot"></span>'+e.orbs+'</span>':'')+'</div>').join('');
    return '<div class="hist-day"><div class="hist-date">'+esc(fmtDate(d))+'<span>'+byDate[d].length+' quest'+(byDate[d].length===1?'':'s')+'</span></div>'+rows+'</div>';
  }).join('');
  const tile=(ic,v,l)=>'<div class="stat"><div class="stat-ic">'+ic+'</div><div class="stat-v">'+v+'</div><div class="stat-l">'+l+'</div></div>';
  return '<div class="history">'
    +'<div class="hstats">'+tile('✅',HISTORY.length,'Quests completed')+tile('🔮',totalOrbs.toLocaleString(),'Orbs earned')
      +tile('📅',dates.length,'Active days')+tile('🔥',(STATS?(STATS.streak_days||0):0)+(STATS&&STATS.streak_days===1?' day':' days'),'Current streak')+'</div>'
    +'<div class="hsec"><div class="hsec-head"><h3>Activity — last 14 weeks</h3></div>'+calendarHeat(counts)+'</div>'
    +'<div class="hsec"><div class="hsec-head"><h3>Completed quests</h3></div><div class="hist-list">'+groups+'</div></div>'
  +'</div>';
}

/* ====================== home ====================== */
function homeHtml(){
  const av=(USER&&USER.avatar)?'<img src="'+esc(USER.avatar)+'" alt="">':'<span>'+((USER&&USER.name?USER.name.trim()[0]:'?').toUpperCase())+'</span>';
  const name=(USER&&USER.name)?USER.name:'there';
  const orbs=(ORBS!=null)?ORBS.toLocaleString():'—';
  const t=s=>s?Date.parse(s)||0:0;
  const recent=(cat)=>QUESTS.filter(q=>q.category===cat&&!q.expired&&!q.claimed&&!q.completed).sort((a,b)=>t(b.startsAt)-t(a.startsAt)).slice(0,3);
  const rw=recent('video'), rp=recent('game'), rs=(SHOP||[]).slice(0,5);
  const earnable=QUESTS.filter(q=>!q.completed&&!q.claimed&&!q.expired).reduce((s,q)=>s+(q.premiumOrbs||q.orbs||0),0);
  const sec=(title,nav,inner)=> '<div class="hsec"><div class="hsec-head"><h3>'+title+'</h3><button class="viewall" onclick="setNav(\''+nav+'\')">View all</button></div>'+inner+'</div>';
  const qgrid=(arr,empty)=> arr.length?'<div class="grid home2">'+arr.map(card).join('')+'</div>':'<div class="hempty">'+empty+'</div>';
  const side='<div class="home-side">'
    +'<div class="home-welcome"><div class="hi">Welcome back</div><div class="hname">'+esc(name)+'</div>'
      +'<div class="horbs"><span class="orb-dot"></span>'+orbs+' orbs'
      +(earnable>0?'<span class="earnable" data-tip="Total orbs from your unclaimed quests">+'+earnable.toLocaleString()+' earnable</span>':'')+'</div></div>'
    +'<div class="home-cta"><button class="act primary" onclick="watchAll()">'+ICO.play+'Watch all</button>'
      +'<button class="act play" onclick="playAll()">'+ICO.play+'Play all</button>'
      +'<button class="iconbtn hicon" title="Refresh" onclick="rescan()">'+ICO.refresh+'</button></div>'
    +(STATS?'<div class="hstats-col">'+statTiles()+'</div>':'')
    +'</div>';
  return '<div class="home">'
    +'<div class="home-top"><div class="home-profile">'+equippedProfile()+'</div>'+side+'</div>'
    +orbGoalWidget()
    +sec('Watch Videos','video',qgrid(rw,'No watch quests right now.'))
    +sec('Play Games','game',qgrid(rp,'No game quests right now.'))
    +sec('Orb Shop','shop',rs.length?'<div class="grid shop">'+rs.map(stile).join('')+'</div>':'<div class="hempty">Loading shop…</div>')
    +'</div>';
}

/* ====================== render ====================== */
const ICO={
  clock:'<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="8.6"/><path d="M12 7.6V12l2.8 1.7"/></svg>',
  gift:'<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="9" width="18" height="11.5" rx="2.2"/><path d="M3 13h18M12 9v11.5"/><path d="M12 9S10.7 5 8.6 5a2 2 0 000 4zM12 9s1.3-4 3.4-4a2 2 0 010 4z"/></svg>',
  play:'<svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.2l11 6.8-11 6.8z"/></svg>',
  check:'<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M4.5 12.5l5 5 10-11"/></svg>',
  ext:'<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 4h6v6M20 4l-8.5 8.5"/><path d="M18 14.5V19a1.6 1.6 0 01-1.6 1.6H5A1.6 1.6 0 013.4 19V7.6A1.6 1.6 0 015 6h4.6"/></svg>',
  phone:'<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="6.5" y="2.5" width="11" height="19" rx="2.6"/><path d="M10.8 18.4h2.4"/></svg>',
  warn:'<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4.2l9 15.6H3z"/><path d="M12 10v4M12 17h.01"/></svg>',
  empty:'<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="5" width="18" height="14" rx="3"/><path d="M3 10h18"/></svg>',
  party:'<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20l4.5-11 6.5 6.5z"/><path d="M15 4.5v.01M19.5 9v.01M18 3l1.6 1.6M20.5 13.5h.01"/></svg>',
  refresh:'<svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20.5 12a8.5 8.5 0 11-2.6-6.1"/><path d="M20.5 4.5V10H15"/></svg>',
  close:'<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M6 6l12 12M18 6L6 18"/></svg>'
};

function counts(){
  byId('n-video').textContent=QUESTS.filter(q=>q.category==='video'&&!q.expired&&!q.claimed&&!q.completed).length||'';
  byId('n-game').textContent=QUESTS.filter(q=>q.category==='game'&&!q.expired&&!q.claimed&&!q.completed).length||'';
  const c=QUESTS.filter(q=>q.completed&&!q.claimed).length;
  byId('n-claim').textContent=c||'';
}

function render(){
  counts();
  const c=byId('content');
  if(NAV==='settings'){ c.innerHTML=settingsHtml(); return; }
  if(NAV==='shop'){ c.innerHTML=shopHtml(); return; }
  if(NAV==='badges'){ c.innerHTML=badgesHtml(); return; }
  if(NAV==='history'){ c.innerHTML=historyHtml(); return; }
  if(NAV==='profile'){ c.innerHTML=profileHtml(); stuApply(); decorateNameTiles(); setupStuDrag(); setupStickyPreview(); if(CATALOG===null&&!catLoading){ catLoading=true; send('loadCatalog'); } return; }
  if(NAV==='home'){ c.innerHTML=homeHtml(); if(SHOP===null&&!shopLoading){ shopLoading=true; send('loadShop'); } return; }
  if(!GOT) return;
  const list=visible();
  if(!list.length){ c.innerHTML=emptyHtml(); return; }
  c.innerHTML='<div class="grid">'+list.map(card).join('')+'</div>';
}

function emptyHtml(){
  if(NAV==='claim') return '<div class="mid"><div class="ic">'+ICO.party+'</div><h2>Nothing to claim</h2><p>Rewards you finish will land here, ready to collect in one click.</p></div>';
  if(NAV==='video') return '<div class="mid"><div class="ic">'+ICO.empty+'</div><h2>No watch quests right now</h2><p>Discord isn\'t offering any video quests that match your filters. Hit refresh to check again.</p></div>';
  return '<div class="mid"><div class="ic">'+ICO.empty+'</div><h2>No game quests match</h2><p>Try clearing the filters or search, then refresh.</p></div>';
}

function card(q){
  const thumb=q.thumb?'style="background-image:url(\''+esc(q.thumb)+'\')"':'';
  const amount=q.premiumOrbs||q.orbs;
  const chip=amount?'<div class="orbchip"><span class="orb-dot"></span>'+amount+(q.premiumOrbs&&q.orbs&&q.premiumOrbs!==q.orbs?' <span class="mult">x1.2</span>':'')+'</div>':'';
  const tag=q.category==='video'
      ? (q.mobileOnly?'<div class="tag">'+ICO.phone+'Mobile</div>':'<div class="tag">Video</div>')
      : '<div class="tag">Game</div>';
  const tgt=q.target||0, pr=Math.min(q.progress||0,tgt);
  const pct=tgt?Math.min(100,Math.round(100*pr/tgt)):0;
  const verb=q.category==='video'?'watched':'played';
  // Every quest shows a progress bar (0 / target when not yet started).
  const bar='<div class="bar '+(q.completed?'done':'')+'" id="bar-'+q.id+'"><i style="width:'+(q.completed?100:pct)+'%"></i></div>'
      +'<div class="row" id="txt-'+q.id+'">'+(q.completed?'Completed':pr+' / '+tgt+'s '+verb)+'</div>';
  const rew=(!amount&&q.reward)?'<div class="row">'+ICO.gift+esc(q.reward)+'</div>':'';

  let act;
  if(NAV==='claim'||(q.completed&&!q.claimed)){
    if(claiming[q.id]) act='<button class="act claim" disabled>Claiming…</button>';
    else if(q.captcha) act='<button class="act ghost" onclick="send(\'openDiscord\')">'+ICO.ext+'Claim in Discord</button>'
        +'<div class="row" style="justify-content:center;margin-top:7px">Discord requires a captcha to collect this</div>';
    else act='<button class="act claim" onclick="claim(\''+q.id+'\')">'+ICO.gift+'Claim '+(amount?amount+' orbs':'reward')+'</button>';
  } else if(q.claimed){
    act='<div class="state">'+ICO.check+'Claimed</div>';
  } else if(q.category==='video'){
    const on=CUR&&CUR.id===q.id;
    act='<button class="act primary" onclick="watch(\''+q.id+'\')" '+(on?'disabled':'')+'>'+ICO.play+(on?'Watching…':'Watch')+'</button>';
  } else if(q.appId){
    const on=CURPLAY&&CURPLAY.id===q.id;
    act='<button class="act play" onclick="playGame(\''+q.id+'\')" '+(on?'disabled':'')+'>'+ICO.play+(on?'Playing…':'Play')+'</button>';
  } else {
    act='<button class="act play" onclick="send(\'openDiscord\')">'+ICO.play+'Play</button>';
  }

  return '<article class="card"><div class="thumb" '+thumb+'>'+tag+chip+'</div>'
    +'<div class="body"><div class="name">'+esc(q.name)+'</div>'
    +rew
    +'<div class="row">'+ICO.clock+esc(q.expiry)+expFlag(q)+'</div>'
    +bar+'<div class="grow"></div>'+act+'</div></article>';
}
function expFlag(q){
  if(q.expired||q.completed||!q.expiresAt) return '';
  const ms=Date.parse(q.expiresAt)-Date.now();
  if(ms>0 && ms<864e5){ const h=Math.max(1,Math.round(ms/36e5)); return '<span class="expsoon">· expires in '+h+'h</span>'; }
  return '';
}

function settingsHtml(){
  const sw=(k)=>'<button class="sw '+(SET[k]?'on':'')+'" onclick="setOpt(\''+k+'\')"></button>';
  const pages=[['home','Home'],['video','Watch'],['game','Games'],['claim','Claim'],['shop','Shop'],['badges','Badges']];
  const dp=SET.default_page||'home';
  const pageBtns=pages.map(p=>'<button class="chip'+(dp===p[0]?' on':'')+'" onclick="setDefaultPage(\''+p[0]+'\')">'+p[1]+'</button>').join('');
  return '<div class="settings2"><div class="settings">'
   +'<div class="sgroup"><h3>General</h3>'
     +'<div class="srow"><div class="txt"><div class="t">Default page</div><div class="d">Which page opens when you launch the app.</div></div><div class="chips wrap" style="max-width:340px;justify-content:flex-end">'+pageBtns+'</div></div>'
   +'</div>'
   +'<div class="sgroup"><h3>Appearance</h3>'
     +'<div class="srow"><div class="txt"><div class="t">Theme</div><div class="d">Dark or light.</div></div><div class="chips">'
       +'<button class="chip'+((SET.theme||'dark')==='dark'?' on':'')+'" onclick="setTheme(\'dark\')">🌙 Dark</button>'
       +'<button class="chip'+(SET.theme==='light'?' on':'')+'" onclick="setTheme(\'light\')">☀ Light</button></div></div>'
     +'<div class="srow"><div class="txt"><div class="t">Accent</div><div class="d">Tint the app — mirrors the Aurora Launcher palettes.</div></div><div class="swatches">'
       +Object.entries(ACCENTS).map(([id,a])=>'<button class="swatch'+((SET.accent||'aurora')===id?' on':'')+'" title="'+id+'" onclick="setAccent(\''+id+'\')" style="--sw:'+a[0]+'"></button>').join('')+'</div></div>'
   +'</div>'
   +'<div class="sgroup"><h3>Startup</h3>'
     +'<div class="srow"><div class="txt"><div class="t">Launch on startup</div><div class="d">Start Aurora Quests automatically when you sign in to Windows.</div></div>'+sw('launch_on_startup')+'</div>'
     +'<div class="srow"><div class="txt"><div class="t">Start minimized</div><div class="d">Open tucked away instead of on screen.</div></div>'+sw('start_minimized')+'</div>'
     +'<div class="srow"><div class="txt"><div class="t">Minimize to tray</div><div class="d">Hide to the system tray (near the clock) instead of the taskbar. Right-click the tray icon for quick actions.</div></div>'+sw('minimize_to_tray')+'</div>'
     +'<div class="srow"><div class="txt"><div class="t">Desktop notifications</div><div class="d">Get a toast when a new quest drops or one finishes. Off by default.</div></div>'+sw('notifications')+'</div>'
   +'</div>'
   +'<div class="sgroup"><h3>Automation</h3>'
     +'<div class="srow"><div class="txt"><div class="t">Auto watch</div><div class="d">Work through your pending video quests one after another in the background. Videos play muted in the dock and progress is reported as they genuinely play.</div></div>'+sw('auto_watch')+'</div>'
     +'<div class="srow"><div class="txt"><div class="t">Auto play</div><div class="d">Work through your pending game quests one after another, mimicking each game so it counts toward the quest. Each game takes its full playtime (~15 min).</div></div>'+sw('auto_play')+'</div>'
     +'<div class="srow"><div class="txt"><div class="t">Show game on my profile</div><div class="d">While mimicking a game, broadcast it as your Discord status so it shows on your profile as if you were playing. On by default — turn off to keep it private.</div></div>'+sw('show_presence')+'</div>'
   +'</div>'
   +'<div class="sgroup"><h3>About</h3>'
     +'<div class="srow"><div class="txt"><div class="t">Aurora Quests</div><div class="d">Reads the quests from your signed-in Discord client on this PC — nothing is sent anywhere else. Videos and games run in real time (a 30s quest takes 30s).</div></div></div>'
     +'<div class="srow"><div class="txt"><div class="t">⚠ Heads up</div><div class="d">This automates quest completion and presence using your Discord account token, which is against Discord\'s Terms of Service. It works on your own account, for your own rewards, but there is a small risk to your account — use at your own discretion.</div></div></div>'
   +'</div></div>'+settingsSide()+'</div>';
}
function settingsSide(){
  const e=CHANGELOG[0];
  const running=(APP_VERSION&&e&&APP_VERSION!==e.v)?(' · running v'+esc(APP_VERSION)):'';
  const cur=e?('<div class="patch-ver"><div class="patch-v">v'+esc(e.v)+running+'</div><div class="patch-d">'+esc(e.d)+'</div></div><ul class="patch-list">'+e.notes.map(n=>'<li>'+esc(n)+'</li>').join('')+'</ul>'):'';
  const older=CHANGELOG.slice(1,4).map(c=>'<div class="patch-old"><div class="patch-v">v'+esc(c.v)+' · '+esc(c.d)+'</div><ul class="patch-list">'+c.notes.slice(0,3).map(n=>'<li>'+esc(n)+'</li>').join('')+'</ul></div>').join('');
  return '<div class="set-side">'
    +'<div class="who"><div class="who-t">MADE BY</div><div class="who-row"><div class="who-av">C</div>'
      +'<div><div class="who-n">camwooloo</div><span class="who-l" onclick="send(\'openExternal\',{url:\'https://camwooloo.com\'})">camwooloo.com '+ICO.ext+'</span></div></div></div>'
    +'<div class="patch"><div class="patch-h"><h3>Patch notes</h3><button class="chk-upd" onclick="checkUpdateNow()">Check for updates</button></div>'
      +cur+older+'</div>'
  +'</div>';
}
function setOpt(k){ SET[k]=!SET[k]; render(); send('setSetting',{key:k,value:SET[k]}); if(k==='auto_watch') syncAuto(); if(k==='auto_play') syncAutoPlay(); }
function syncAutoPlay(){ if(!SET.auto_play) return; if(!CURPLAY && !playQueue.length){ playQueue=pendingGames().map(q=>q.id); nextPlay(); } }

/* ====================== shop ====================== */
const SHOP_CATS=[['all','All'],['deco','Avatar Decorations'],['nameplate','Nameplates'],['effect','Profile Effects'],['frame','Profile Frames'],['bundle','Bundles'],['owned','Owned']];
const SHOP_SORTS={recent:'Recently Added','price-lo':'Price: Low to High','price-hi':'Price: High to Low',popular:'Popular'};
const SWATCHES=[['purple','#a259ff'],['blue','#3b82f6'],['green','#22c55e'],['brown','#8b5a2b'],['gold','#eab308'],['orange','#f97316'],['red','#ef4444'],['pink','#ec4899'],['white','#e5e7eb'],['black','#111318']];
const THEMES=[['anime','Anime'],['gaming','Gaming'],['cute','Cute & Cosy'],['scifi','Sci-Fi'],['food','Food & Drinks'],['fantasy','Fantasy'],['animals','Animals & Pets'],['nature','Nature'],['films','Films & TV'],['moody','Dark & Moody']];
const THEME_KW={anime:['anime','waifu','chibi','manga'],gaming:['game','gamer','pixel','arcade','controller','retro','8-bit'],cute:['cute','cosy','cozy','kawaii','pastel','sweet','bubble','heart'],scifi:['space','galaxy','cyber','neon','robot','sci-fi','cosmic','nebula','orbit','astro','star','solar','eclipse'],food:['food','coffee','pizza','cake','drink','snack','fruit','candy','berry','tea'],fantasy:['magic','fantasy','dragon','wizard','fairy','mystic','enchant','crystal','rune','arcane'],animals:['cat','dog','animal','pet','fox','bunny','paw','duck','creature'],nature:['nature','forest','flower','leaf','ocean','mountain','garden','bloom','tree','rain','wave','glitter'],films:['film','movie','cinema','tv','hollywood'],moody:['dark','moody','shadow','night','gothic','noir']};
let shopColour=null, shopTheme=null, shopAfford=false;
function toggleAfford(){ shopAfford=!shopAfford; render(); }
function hexRgb(h){ h=(''+(h||'#888')).replace('#',''); return [parseInt(h.slice(0,2),16)||0,parseInt(h.slice(2,4),16)||0,parseInt(h.slice(4,6),16)||0]; }
function bucketColour(hex){ const [r,g,b]=hexRgb(hex); let best='black',bd=1e9; for(const [id,ref] of SWATCHES){ const [R,G,B]=hexRgb(ref); const d=(r-R)**2+(g-G)**2+(b-B)**2; if(d<bd){bd=d;best=id;} } return best; }
function themeOf(i){ const s=((i.name||'')+' '+(i.collection||'')).toLowerCase(); for(const [id,kws] of Object.entries(THEME_KW)) if(kws.some(k=>s.includes(k))) return id; return null; }
function setShopFilter(f){ shopFilter=f; render(); }
function setShopSort(s){ shopSort=s; byId('sMenu').classList.remove('open'); render(); }
function toggleShopMenu(e){ e.stopPropagation(); byId('sMenu').classList.toggle('open'); }
function setColour(c){ shopColour=(shopColour===c?null:c); render(); }
function setTheme(t){ shopTheme=(shopTheme===t?null:t); render(); }

function shopVisible(){
  if(!SHOP) return [];
  let list=SHOP.slice();
  if(shopFilter!=='all') list=list.filter(i=>i.kind===shopFilter);
  if(shopColour) list=list.filter(i=>bucketColour(i.colorA)===shopColour);
  if(shopTheme) list=list.filter(i=>themeOf(i)===shopTheme);
  if(shopAfford&&ORBS!=null) list=list.filter(i=>i.orbs<=ORBS);
  if(shopSort==='price-lo') list.sort((a,b)=>a.orbs-b.orbs);
  else if(shopSort==='price-hi') list.sort((a,b)=>b.orbs-a.orbs);
  else if(shopSort==='popular') list.sort((a,b)=>(a.rank==null?1e9:a.rank)-(b.rank==null?1e9:b.rank));
  return list;
}
function shopHtml(){
  if(shopLoading||SHOP===null) return '<div class="mid"><div class="spin"></div><p>Loading the Orb Shop…</p></div>';
  if(shopErr) return '<div class="mid"><div class="ic">'+ICO.warn+'</div><h2>Couldn\'t load the shop</h2><p>'+esc(shopErr)+'</p></div>';
  const chips=SHOP_CATS.map(([k,l])=>'<button class="chip '+(shopFilter===k?'on':'')+'" onclick="setShopFilter(\''+k+'\')">'+l+'</button>').join('')
    +'<button class="chip'+(shopAfford?' on':'')+'" onclick="toggleAfford()" data-tip="Only items you can afford">✓ Affordable</button>';
  const bar='<div class="shopbar"><div class="chips">'+chips+'</div><div class="grow"></div>'
    +'<div class="dd"><button class="ctl" onclick="toggleShopMenu(event)"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M4 7h16M6.5 12h11M10 17h4"/></svg><span>'+SHOP_SORTS[shopSort]+'</span><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg></button>'
    +'<div class="menu" id="sMenu"><div class="lbl">Sort by</div>'+Object.keys(SHOP_SORTS).map(s=>'<button class="mi '+(shopSort===s?'sel':'')+'" onclick="setShopSort(\''+s+'\')"><span class="radio"></span>'+SHOP_SORTS[s]+'</button>').join('')+'</div></div></div>';
  if(shopFilter==='owned'){
    if(OWNED===null) return bar+'<div class="mid"><div class="spin"></div><p>Loading your collection…</p></div>';
    const g=OWNED.length?'<div class="grid shop">'+OWNED.map(ownedTile).join('')+'</div>'
      :'<div class="mid"><div class="ic">'+ICO.empty+'</div><h2>No collectibles yet</h2><p>Items you own will appear here.</p></div>';
    return bar+g;
  }
  // Colour + theme filter row.
  const swatches=SWATCHES.map(([id,c])=>'<button class="swatch'+(shopColour===id?' on':'')+'" title="'+id+'" onclick="setColour(\''+id+'\')" style="--sw:'+c+'"></button>').join('');
  const themeChips=THEMES.map(([id,l])=>'<button class="chip sm'+(shopTheme===id?' on':'')+'" onclick="setTheme(\''+id+'\')">'+l+'</button>').join('');
  const filters='<div class="shopfilters"><div class="ff"><span class="ff-l">Colour</span><div class="swatches">'+swatches+'</div></div>'
    +'<div class="ff"><span class="ff-l">Themes</span><div class="chips wrap">'+themeChips+'</div></div></div>';
  const list=shopVisible();
  const grid=list.length? '<div class="grid shop">'+list.map(stile).join('')+'</div>'
    : '<div class="mid"><div class="ic">'+ICO.empty+'</div><h2>Nothing here</h2><p>No orb items match these filters.</p></div>';
  return bar+filters+grid;
}
const KIND_LABEL={deco:'Avatar Decoration',nameplate:'Nameplate',effect:'Profile Effect',frame:'Profile Frame',bundle:'Bundle',other:'Collectible'};
function stile(i){
  const a=i.colorA||'#2a2350', b=i.colorB||a;
  const grad='linear-gradient(140deg,'+esc(a)+','+esc(b)+')';
  const img=i.image?'<img class="stile-img" src="'+esc(i.image)+'" loading="lazy" onerror="this.style.display=\'none\'">':'';
  const isGoal=SET.orb_goal===i.sku;
  const afford=(ORBS!=null&&ORBS>=i.orbs)?'<span class="afford" data-tip="You can afford this">✓</span>':'';
  return '<article class="card stile">'
    +'<div class="stile-art" style="background:'+grad+'">'+img+'<span class="stile-kind">'+(KIND_LABEL[i.kind]||'Item')+'</span>'
    +'<button class="goalstar'+(isGoal?' on':'')+'" title="Set as orb goal" onclick="event.stopPropagation();setGoal(\''+i.sku+'\')">🎯</button></div>'
    +'<div class="body"><div class="name">'+esc(i.name)+'</div>'
    +'<div class="row">'+esc(i.collection||'')+'</div><div class="grow"></div>'
    +'<div class="stile-foot"><span class="orbcost"><span class="orb-dot"></span>'+i.orbs.toLocaleString()+afford+'</span>'
    +'<button class="act ghost sbuy" onclick="send(\'openDiscord\')">'+ICO.ext+'Get in Discord</button></div>'
    +'</div></article>';
}
function ownedTile(i){
  const a=i.colorA||'#2a2350';
  const img=i.image?'<img class="stile-img" src="'+esc(i.image)+'" loading="lazy" onerror="this.style.display=\'none\'">':'';
  return '<article class="card stile"><div class="stile-art" style="background:linear-gradient(140deg,'+esc(a)+',#0c1120)">'+img
    +'<span class="stile-kind">'+(KIND_LABEL[i.kind]||'Item')+'</span><span class="owned-b">✓ Owned</span></div>'
    +'<div class="body"><div class="name">'+esc(i.name)+'</div><div class="grow"></div>'
    +'<button class="act ghost sbuy" onclick="send(\'openDiscord\')">'+ICO.ext+'View in Discord</button></div></article>';
}
function orbGoalWidget(){
  if(!SET.orb_goal||!SHOP||ORBS==null) return '';
  const item=SHOP.find(i=>i.sku===SET.orb_goal); if(!item) return '';
  const price=item.orbs, have=ORBS, pct=Math.min(100,Math.round(100*have/price)), rem=Math.max(0,price-have);
  return '<div class="goal"><div class="goal-art" style="background:linear-gradient(140deg,'+esc(item.colorA||'#2a2350')+','+esc(item.colorB||item.colorA||'#2a2350')+')">'+(item.image?'<img src="'+esc(item.image)+'">':'')+'</div>'
    +'<div class="goal-body"><div class="goal-t"><span>Saving for <b>'+esc(item.name)+'</b></span><button class="goal-x" onclick="clearGoal()">✕</button></div>'
    +'<div class="bar"><i style="width:'+pct+'%"></i></div>'
    +'<div class="row">'+have.toLocaleString()+' / '+price.toLocaleString()+' orbs · '+(rem>0?rem.toLocaleString()+' to go':'you can afford it! 🎉')+'</div></div></div>';
}
function setGoal(sku){ SET.orb_goal=(SET.orb_goal===sku?'':sku); send('setSettingStr',{key:'orb_goal',value:SET.orb_goal}); toast(SET.orb_goal?'Set as your orb goal':'Goal cleared'); render(); }
function clearGoal(){ SET.orb_goal=''; send('setSettingStr',{key:'orb_goal',value:''}); render(); }

/* ====================== watching ====================== */
const vid=byId('vid');

function watch(id,auto){
  const q=QUESTS.find(x=>x.id===id); if(!q||!q.video) return;
  CUR=q; lastSent=-1;
  byId('dockName').textContent=q.name;
  byId('pillName').textContent=q.name;
  byId('pill').classList.add('on');
  const tgt=q.target||0, pr=Math.min(q.progress||0,tgt);
  byId('dockFill').style.width=(tgt?Math.min(100,100*pr/tgt):0)+'%';
  byId('dockStatus').textContent='Starting…';
  byId('pillTime').textContent=pr+'/'+tgt+'s';
  vid.src=q.video; vid.muted=true; vid.volume=0;
  send('watch',{id:q.id});
  vid.play().catch(()=>{ byId('dockStatus').textContent='Tap the dock to start playback.'; showDock(true); });
  if(!auto) toast('Watching in the background — open it from the pill up top');
  render();
}
function finishCurrent(){
  const done=CUR;
  if(done&&SET.auto_claim) claim(done.id);
  setTimeout(()=>{ if(CUR&&done&&CUR.id===done.id){ stopWatch(true); nextAuto(); } },900);
}
function stopWatch(keepQueue){
  vid.pause(); vid.removeAttribute('src'); vid.load();
  CUR=null; byId('pill').classList.remove('on'); showDock(false);
  byId('dockStatus').textContent='Idle';
  if(!keepQueue) autoQueue=[];
  byId('dockQueue').textContent='';
  render();
}
function toggleDock(){ byId('dock').classList.toggle('show'); }
function showDock(on){ byId('dock').classList.toggle('show',!!on); }
function toggleMute(){ vid.muted=!vid.muted; vid.volume=vid.muted?0:1;
  byId('muteBtn').title=vid.muted?'Unmute':'Mute'; byId('muteBtn').style.color=vid.muted?'':'var(--accent)'; }

vid.addEventListener('timeupdate',()=>{
  if(!CUR) return;
  const s=Math.floor(vid.currentTime);
  if(s===lastSent) return;
  lastSent=s;
  const tgt=CUR.target||0;
  if(tgt){ byId('dockFill').style.width=Math.min(100,100*s/tgt)+'%'; byId('pillTime').textContent=s+'/'+tgt+'s'; }
  if(s%3===0) send('progress',{id:CUR.id,seconds:s});
});
vid.addEventListener('ended',()=>{ if(CUR) send('progress',{id:CUR.id,seconds:Math.floor(vid.duration||0)}); });

/* sequential watch queue — shared by Auto watch and the Watch all button.
   Videos always play ONE AT A TIME: the next only starts when the current
   one completes (see finishCurrent -> nextAuto). */
function pendingVideos(){ return QUESTS.filter(q=>q.category==='video'&&q.video&&!q.completed&&!q.claimed&&!q.expired); }
function syncAuto(){
  // Auto watch auto-populates the queue on load; Watch all fills it on demand.
  if(!SET.auto_watch) return;
  if(!CUR && !autoQueue.length){ autoQueue=pendingVideos().map(q=>q.id); nextAuto(); }
}
function watchAll(){
  const vids=pendingVideos();
  if(!vids.length){ toast('No unwatched video quests'); return; }
  autoQueue=vids.map(q=>q.id);
  toast('Queued '+vids.length+' videos — watching one at a time');
  if(!CUR) nextAuto(); else byId('dockQueue').textContent=(autoQueue.length)+' queued';
}
function nextAuto(){
  autoQueue=autoQueue.filter(id=>{ const q=QUESTS.find(x=>x.id===id); return q&&!q.completed&&!q.claimed; });
  const id=autoQueue.shift();
  if(!id){ byId('dockQueue').textContent=''; return; }
  byId('dockQueue').textContent=autoQueue.length+' queued';
  watch(id,true);
}

/* ====================== playing (game quests) ====================== */
function pendingGames(){ return QUESTS.filter(q=>q.category==='game'&&q.appId&&!q.completed&&!q.claimed&&!q.expired); }
function playGame(id,auto){
  const q=QUESTS.find(x=>x.id===id); if(!q||!q.appId) return;
  CURPLAY=q;
  byId('pillName').textContent=q.name;
  byId('pillTime').textContent=(q.progress||0)+'/'+(q.target||900)+'s';
  byId('pill').classList.add('on');
  send('playStart',{id:id});
  if(!auto) toast('Now mimicking '+q.name+' — it counts while it runs (~'+Math.round((q.target||900)/60)+ ' min)');
  render();
}
function stopPlay(){ send('playStop'); CURPLAY=null; playQueue=[]; if(!CUR) byId('pill').classList.remove('on'); render(); }
function playAll(){
  const g=pendingGames();
  if(!g.length){ toast('No unplayed game quests'); return; }
  playQueue=g.map(q=>q.id);
  toast('Queued '+g.length+' games — one at a time');
  if(!CURPLAY) nextPlay(); else byId('pillTime').textContent+=' · '+playQueue.length+' queued';
}
function nextPlay(){
  playQueue=playQueue.filter(id=>{ const q=QUESTS.find(x=>x.id===id); return q&&!q.completed&&!q.claimed; });
  const id=playQueue.shift();
  if(id) playGame(id,true);
}
window.playProgress=function(id,prog,target,done){
  const q=QUESTS.find(x=>x.id===id); if(q){ q.progress=prog; if(done) q.completed=true; }
  const bar=byId('bar-'+id), txt=byId('txt-'+id);
  const pct=target?Math.min(100,Math.round(100*prog/target)):0;
  if(bar){ bar.classList.toggle('done',!!done); bar.firstElementChild.style.width=(done?100:pct)+'%'; }
  if(txt) txt.textContent=done?'Completed':prog+' / '+target+'s played';
  if(CURPLAY&&CURPLAY.id===id) byId('pillTime').textContent=done?'done':prog+'/'+target+'s';
  if(NAV==='claim'||NAV==='home') render();
};
window.playDone=function(id){
  const q=QUESTS.find(x=>x.id===id); if(q){ q.completed=true; onCompleted(q); }
  if(CURPLAY&&CURPLAY.id===id){ CURPLAY=null; if(!CUR) byId('pill').classList.remove('on'); toast('Finished a game quest — claim it in the Claim tab'); nextPlay(); }
  render();
};
window.playError=function(id,msg){ if(CURPLAY&&CURPLAY.id===id) byId('pillTime').textContent='error'; };

/* ====================== claiming ====================== */
function claim(id){
  if(claiming[id]) return;
  claiming[id]=true; render();
  send('claim',{id:id});
}

/* ====================== misc ====================== */
let toastT;
function toast(msg,kind){
  const t=byId('toast'); t.textContent=msg; t.className='toast show '+(kind||'');
  clearTimeout(toastT); toastT=setTimeout(()=>t.className='toast '+(kind||''),3200);
}
document.addEventListener('keydown',e=>{ if(e.key==='Escape'){ byId('sortMenu').classList.remove('open'); showDock(false);} });

byId('app').classList.add('intro');
setNav('home'); setSort('suggested');
/*BOOTSTRAP*/
send('ready');
let tries=0;
const kick=setInterval(()=>{ if(GOT||tries++>12){clearInterval(kick);return;} send('ready'); },500);
</script>
</body>
</html>
"####;
