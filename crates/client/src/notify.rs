//! Telling a player who is in another window: a notification from the
//! system (and, on Windows, a flashing taskbar button) when an invitation
//! arrives, "Play now" finds an opponent, a friend comes online or a
//! tournament is about to start. Settings → Game turns them off.
//!
//! On Windows the game asks the system whether its window is the one in
//! front: behind another window or minimised, it notifies at once.
//! Elsewhere it can't see that, so it notifies when nobody has touched it
//! for a while (the player is somewhere else).

use std::time::{Duration, Instant};

use macroquad::prelude::*;

/// Nobody touched the game for this long: they are probably elsewhere.
const AWAY_AFTER: Duration = Duration::from_secs(12);
/// At most one notification this often.
const MIN_GAP: Duration = Duration::from_secs(8);

pub struct Notifier {
    last_input: Instant,
    last_mouse: Vec2,
    last_sent: Option<Instant>,
}

impl Default for Notifier {
    fn default() -> Self {
        Notifier { last_input: Instant::now(), last_mouse: Vec2::ZERO, last_sent: None }
    }
}

impl Notifier {
    /// Call every frame: notices the player using the game.
    pub fn update(&mut self) {
        let mouse = Vec2::from(mouse_position());
        let used = mouse != self.last_mouse
            || get_last_key_pressed().is_some()
            || is_mouse_button_down(MouseButton::Left)
            || mouse_wheel().1 != 0.0;
        self.last_mouse = mouse;
        if used {
            self.last_input = Instant::now();
        }
    }

    /// How long nobody has touched the game.
    pub fn idle(&self) -> Duration {
        self.last_input.elapsed()
    }

    /// Shows `text` from the system if the player seems to be away: the
    /// game's window isn't in front (Windows), or nobody touched it lately.
    pub fn tell(&mut self, text: &str) {
        let behind = in_front() == Some(false);
        if !behind && self.last_input.elapsed() < AWAY_AFTER {
            return;
        }
        if self.last_sent.is_some_and(|t| t.elapsed() < MIN_GAP) {
            return;
        }
        self.last_sent = Some(Instant::now());
        let text = text.to_string();
        // Never hold the game up: the commands run on their own.
        std::thread::spawn(move || send(&text));
    }
}

/// Whether the game's window is the one in front and not minimised; `None`
/// where the game can't tell.
#[cfg(windows)]
fn in_front() -> Option<bool> {
    windows::in_front()
}

#[cfg(not(windows))]
fn in_front() -> Option<bool> {
    None
}

/// The text as given to the system's notifier: one line, not too long, and
/// never starting with a dash (a command line would take it for an option).
fn plain(text: &str) -> String {
    let line: String =
        text.chars().map(|c| if c.is_control() { ' ' } else { c }).take(240).collect();
    line.trim().trim_start_matches('-').trim_start().to_string()
}

/// The text never becomes part of a command or a script: it is handed over
/// as an argument (or, on Windows, in the environment), so a tournament's
/// name or a player's message can't be run as code, whatever quotes it
/// holds (PowerShell also takes ‘ ’ ‚ ‛ for quotes).
fn send(text: &str) {
    let text = plain(text);
    if text.is_empty() {
        return;
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("notify-send")
            .args([
                &format!("--app-name={}", snook_legend_proto::GAME_NAME),
                "--urgency=normal",
                "--",
                snook_legend_proto::GAME_NAME,
                &text,
            ])
            // Waited for (on this thread of its own): no process left over.
            .status();
    }
    #[cfg(target_os = "macos")]
    {
        let show = format!(
            "display notification (item 1 of argv) with title \"{}\" sound name \"Glass\"",
            snook_legend_proto::GAME_NAME
        );
        let _ = std::process::Command::new("osascript")
            .args(["-e", "on run argv", "-e", &show, "-e", "end run", &text])
            .status();
    }
    #[cfg(windows)]
    {
        windows::flash();
        windows::toast(&text);
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    let _ = text;
}

#[cfg(windows)]
mod windows {
    use std::os::windows::process::CommandExt;

    #[repr(C)]
    struct FlashWInfo {
        cb_size: u32,
        hwnd: *mut core::ffi::c_void,
        flags: u32,
        count: u32,
        timeout: u32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(class: *const u16, title: *const u16) -> *mut core::ffi::c_void;
        fn FlashWindowEx(info: *const FlashWInfo) -> i32;
        fn GetForegroundWindow() -> *mut core::ffi::c_void;
        fn IsIconic(hwnd: *mut core::ffi::c_void) -> i32;
    }

    /// The game's window is the foreground one and not minimised (`None`:
    /// no window of the game's title was found).
    pub fn in_front() -> Option<bool> {
        let title: Vec<u16> = snook_legend_proto::GAME_NAME.encode_utf16().chain(Some(0)).collect();
        // SAFETY: a null-terminated UTF-16 title and a null class are valid
        // arguments; the handles are only compared and passed back.
        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
            if hwnd.is_null() {
                return None;
            }
            Some(GetForegroundWindow() == hwnd && IsIconic(hwnd) == 0)
        }
    }

    /// Flashes the game's taskbar button until it is brought to the front.
    pub fn flash() {
        let title: Vec<u16> = snook_legend_proto::GAME_NAME.encode_utf16().chain(Some(0)).collect();
        // SAFETY: a null-terminated UTF-16 title and a null class are valid
        // arguments; the window handle is only passed back to the system.
        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
            if hwnd.is_null() {
                return;
            }
            const FLASHW_ALL: u32 = 3;
            const FLASHW_TIMERNOFG: u32 = 12;
            let info = FlashWInfo {
                cb_size: std::mem::size_of::<FlashWInfo>() as u32,
                hwnd,
                flags: FLASHW_ALL | FLASHW_TIMERNOFG,
                count: 0,
                timeout: 0,
            };
            FlashWindowEx(&info);
        }
    }

    /// A toast from the system (Windows 10 and later). Windows only shows
    /// toasts from an app it knows: the game's Start menu entry (the
    /// Microsoft Store package, or the installer's shortcut), else
    /// PowerShell's own (a portable copy has no entry).
    pub fn toast(text: &str) {
        // The script is always the same; the text comes from the
        // environment, never pasted into it.
        let script = format!(
            "$id = (Get-StartApps | Where-Object {{ $_.Name -eq '{name}' }} | Select-Object -First 1).AppID; \
             if (-not $id) {{ $id = '{{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}}\\WindowsPowerShell\\v1.0\\powershell.exe' }}; \
             [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
             $t = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
             $n = $t.GetElementsByTagName('text'); \
             $n.Item(0).AppendChild($t.CreateTextNode('{name}')) > $null; \
             $n.Item(1).AppendChild($t.CreateTextNode([string]$env:SNOOK_LEGEND_TOAST)) > $null; \
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($id).Show([Windows.UI.Notifications.ToastNotification]::new($t))",
            name = snook_legend_proto::GAME_NAME
        );
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .env("SNOOK_LEGEND_TOAST", text)
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_is_one_plain_line() {
        assert_eq!(plain("--help me"), "help me");
        assert_eq!(plain(" - Ana’s Cup\nhas started!"), "Ana’s Cup has started!");
        assert_eq!(plain("’+(calc)+’"), "’+(calc)+’", "kept as text, never run");
        assert_eq!(plain(&"x".repeat(1000)).len(), 240);
        assert_eq!(plain("---"), "");
    }
}
