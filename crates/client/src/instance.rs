//! One game at a time on a computer: a second start brings the first
//! one's window forward and ends, instead of opening another game (two of
//! them shared the settings, the saved game and the account's session,
//! each writing over the other's).
//!
//! The game holds a lock on a file in its settings folder for as long as it
//! runs; the system lets go of it when the game ends, however it ends, so a
//! crash never leaves the game locked out.

use std::fs::{File, TryLockError};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The lock, held until the game ends.
static LOCK: OnceLock<File> = OnceLock::new();

/// How long a start waits for a game that is closing (the one that just
/// started this one after an update, or one closed a moment ago).
const WAIT: Duration = Duration::from_secs(3);

/// Whether this start is the game (true), or a second one that should end
/// now (false). Where the lock can't be had at all (no settings folder, a
/// file system without locks), the game starts as it always did.
pub fn claim() -> bool {
    // Development aid: several games side by side (two of them can play an
    // online match on one computer).
    if crate::dev::flag("SNOOK_LEGEND_MANY") {
        return true;
    }
    let Some(dir) = crate::config::config_dir() else { return true };
    let _ = std::fs::create_dir_all(&dir);
    let Ok(file) =
        File::options().create(true).truncate(false).write(true).open(dir.join("instance.lock"))
    else {
        return true;
    };
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => {
                let _ = LOCK.set(file);
                return true;
            }
            Err(TryLockError::WouldBlock) if started.elapsed() < WAIT => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(TryLockError::WouldBlock) => {
                show_running_game();
                return false;
            }
            Err(TryLockError::Error(_)) => return true,
        }
    }
}

/// Brings the window of the game that is running to the front.
#[cfg(windows)]
fn show_running_game() {
    use std::ffi::c_void;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(class: *const u16, title: *const u16) -> *mut c_void;
        fn IsIconic(window: *mut c_void) -> i32;
        fn ShowWindow(window: *mut c_void, command: i32) -> i32;
        fn SetForegroundWindow(window: *mut c_void) -> i32;
    }
    const SW_RESTORE: i32 = 9;
    let title: Vec<u16> =
        snook_legend_proto::GAME_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: plain Win32 calls with a NUL-terminated title that outlives
    // them; a window handle that is null or gone is checked or ignored.
    unsafe {
        let window = FindWindowW(std::ptr::null(), title.as_ptr());
        if !window.is_null() {
            if IsIconic(window) != 0 {
                ShowWindow(window, SW_RESTORE);
            }
            SetForegroundWindow(window);
        }
    }
}

#[cfg(not(windows))]
fn show_running_game() {
    eprintln!("{} is already running.", snook_legend_proto::GAME_NAME);
}
