// Platform integration — the OS-specific bits gathered in one place so the rest
// of the app stays portable. Windows and Linux each provide the same surface:
//
//   * config_dir()          where settings/history/stats/studio JSON lives
//   * notify()              a desktop toast/notification (best-effort)
//   * open_external()       hand a URL to the system browser / Discord client
//   * set_autostart()       add/remove "launch when I sign in"
//   * plausible_exe_path()  an OS-appropriate game path for play heartbeats
//
// Anything not listed here (the WebView window, tray icon, HTTP) is already
// cross-platform via wry/tao/ureq.

use std::path::PathBuf;

/// `…/AuroraQuests` — the per-user directory holding our JSON state.
/// Windows: `%APPDATA%\AuroraQuests`. Linux: `$XDG_CONFIG_HOME/AuroraQuests`
/// (falling back to `~/.config/AuroraQuests`).
pub fn config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("AuroraQuests")
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                format!("{}/.config", std::env::var("HOME").unwrap_or_default())
            });
        PathBuf::from(base).join("AuroraQuests")
    }
}

// ---------------------------------------------------------------------------
// Desktop notifications
// ---------------------------------------------------------------------------

/// Show a desktop notification. Best-effort — silently ignores failures.
#[cfg(windows)]
pub fn notify(title: &str, body: &str) {
    use tauri_winrt_notification::Toast;
    // Piggyback on PowerShell's registered AppID so no manifest is required.
    let _ = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(body)
        .show();
}

/// Show a desktop notification via libnotify's `notify-send` (standard on Arch
/// under the `libnotify` package). Best-effort — silently ignores failures.
#[cfg(not(windows))]
pub fn notify(title: &str, body: &str) {
    let _ = std::process::Command::new("notify-send")
        .args(["-a", "Aurora Quests", "--", title, body])
        .spawn();
}

// ---------------------------------------------------------------------------
// Open a URL in the user's browser / Discord client
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub fn open_external(url: &str) {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW avoids the console flash the old `cmd /C start` caused.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    if is_openable(url) {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

/// `xdg-open` (from `xdg-utils`) routes http(s) to the default browser and
/// `discord://` to the installed client, mirroring the Windows behaviour.
#[cfg(not(windows))]
pub fn open_external(url: &str) {
    if is_openable(url) {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

/// Only hand off schemes we expect — never an arbitrary local path/argument.
fn is_openable(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with("discord://")
}

// ---------------------------------------------------------------------------
// Launch on sign-in
// ---------------------------------------------------------------------------

/// Add or remove the "start Aurora Quests when I sign in" integration.
#[cfg(windows)]
pub fn set_autostart(enabled: bool, minimized: bool) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN_VALUE: &str = "AuroraQuests";

    let Ok(exe) = std::env::current_exe() else { return };
    if enabled {
        let mut cmd = format!("\"{}\"", exe.display());
        if minimized {
            cmd.push_str(" --minimized");
        }
        // HKCU Run key — the standard per-user autostart, no registry crate needed.
        let _ = std::process::Command::new("reg")
            .args(["add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &cmd, "/f"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    } else {
        let _ = std::process::Command::new("reg")
            .args(["delete", RUN_KEY, "/v", RUN_VALUE, "/f"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
}

/// Linux autostart is a `.desktop` file under `~/.config/autostart` — the
/// freedesktop standard honoured by GNOME, KDE, and most other environments.
#[cfg(not(windows))]
pub fn set_autostart(enabled: bool, minimized: bool) {
    let base = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}/.config", std::env::var("HOME").unwrap_or_default()));
    let dir = PathBuf::from(base).join("autostart");
    let entry = dir.join("AuroraQuests.desktop");

    if !enabled {
        let _ = std::fs::remove_file(&entry);
        return;
    }

    let Ok(exe) = std::env::current_exe() else { return };
    let exec = if minimized {
        format!("\"{}\" --minimized", exe.display())
    } else {
        format!("\"{}\"", exe.display())
    };
    let desktop = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Aurora Quests\n\
         Comment=Native viewer/completer for Discord Quests\n\
         Exec={exec}\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    );
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&entry, desktop);
}

// ---------------------------------------------------------------------------
// Keep the device awake while a quest is running
// ---------------------------------------------------------------------------

/// Hold (or release) a "don't sleep" request so an idle laptop doesn't suspend
/// mid-quest. Only the system is kept awake — the display may still turn off.
///
/// Windows: `SetThreadExecutionState` is per-thread, so this must be called
/// from a long-lived thread (the main event loop does it).
#[cfg(windows)]
pub fn keep_awake(on: bool) {
    use windows::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
    };
    let flags = if on {
        ES_CONTINUOUS | ES_SYSTEM_REQUIRED
    } else {
        ES_CONTINUOUS
    };
    unsafe {
        SetThreadExecutionState(flags);
    }
}

/// Linux: a `systemd-inhibit` idle+sleep lock held by a child process. The
/// child is `tail --pid=<us>`, so the lock also drops if we exit or crash.
#[cfg(not(windows))]
pub fn keep_awake(on: bool) {
    use std::process::{Child, Command, Stdio};
    use std::sync::Mutex;
    static INHIBIT: Mutex<Option<Child>> = Mutex::new(None);

    let mut held = INHIBIT.lock().unwrap();
    if on {
        if held.is_some() {
            return;
        }
        let pid = std::process::id().to_string();
        *held = Command::new("systemd-inhibit")
            .args([
                "--what=idle:sleep",
                "--who=Aurora Quests",
                "--why=Running a quest",
                "--mode=block",
                "tail",
                "-f",
                "/dev/null",
                "--pid",
                &pid,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok();
    } else if let Some(mut child) = held.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

// ---------------------------------------------------------------------------
// A plausible per-game executable path for play heartbeats
// ---------------------------------------------------------------------------

/// A believable install path for `game`, so heartbeats mirror a real client's
/// `executable_path`. Shape matches the host OS.
pub fn plausible_exe_path(game: &str) -> String {
    let clean: String = game
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let name = if clean.is_empty() { "Game".to_string() } else { clean };
    let exe = name.replace(' ', "");

    #[cfg(windows)]
    {
        format!("C:\\Program Files\\{name}\\{exe}.exe")
    }
    #[cfg(not(windows))]
    {
        // Where a native Linux game or Proton shim typically lands.
        format!("/usr/bin/{}", exe.to_ascii_lowercase())
    }
}
