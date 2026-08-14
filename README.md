# Aurora Quests

A fast, native Windows companion for **Discord Quests** — a single portable `.exe`
(Rust + WebView2, no installer) that reads the Quests from your locally signed-in
Discord client and gives them a modern home.

![Aurora Quests](https://img.shields.io/badge/platform-Windows-6c5ce7) ![Rust](https://img.shields.io/badge/built%20with-Rust-orange)

## Features

- **Watch & Play tabs** — every video and game quest, with an *orbs-only* filter, sorting
  (suggested / recent / expiring / started) and an expiring-soon flag.
- **Watch videos in-app** — quests play muted in a docked player and count toward progress;
  Watch-all / Play-all run through them one by one.
- **Claim tab** — collect finished rewards from inside the app.
- **Orb Shop** — the real collectibles shop with colour/theme/category filters, sorting,
  "affordable now", and an orb goal you can pin.
- **Badges** — your real Discord badges with the full evolving-tier ladders.
- **Profile studio** — preview every decoration, nameplate, profile effect, frame, name
  style and two-colour theme on a live Discord-style profile card, and export edited
  avatars & banners.
- **Custom Rich Presence** — a click-to-edit, Discord-style activity editor.
- **Homepage** — your real equipped profile plus all-time stats.
- **Quality of life** — system tray, optional desktop notifications, light/dark theme +
  accent picker, launch-on-startup, and built-in auto-update from GitHub Releases.

## Install

Download the latest `discord_quests.exe` from [Releases](../../releases) and run it — no
installation. It updates itself when a new release is published.

## Build from source

```bash
cargo build --release
# → target/release/discord_quests.exe
```

Requires the Rust toolchain and the WebView2 runtime (preinstalled on Windows 11).

## ⚠️ Disclaimer

This automates quest completion and sets presence using your Discord **account token**,
which is against Discord's Terms of Service. It only ever touches your **own** account, for
your **own** rewards, and your data stays on your machine — but there is a small account
risk. Use at your own discretion.
