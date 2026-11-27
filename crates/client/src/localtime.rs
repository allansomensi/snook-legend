//! The computer's own clock and calendar: when a message was sent, read in
//! the player's time zone ("14:32", "Yesterday", "Tuesday"), as messaging
//! apps show it. The rest of the game counts in UTC days (a date is the
//! same everywhere); a time of day only means something in local time.
//!
//! The time zone comes from the system (`localtime_r` on Linux and macOS,
//! the Windows time zone API), at each moment asked about, so a summer
//! time change in between reads right.

use crate::i18n::{Lang, t, tw};

/// A moment in local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Local {
    pub year: i64,
    /// 1 to 12.
    pub month: i64,
    /// 1 to 31.
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    /// 0 for Monday to 6 for Sunday.
    pub weekday: i64,
    /// Days since 1970-01-01 in local time (for "today", "yesterday").
    pub days: i64,
}

/// Seconds east of UTC at `at` (Unix seconds) where the computer is.
pub fn offset(at: i64) -> i64 {
    let off = sys::offset(at);
    // Nothing past ±14 h exists: a broken answer is no offset at all.
    if off.abs() <= 14 * 3600 { off } else { 0 }
}

/// `at` (Unix seconds) in local time.
pub fn local(at: i64) -> Local {
    civil(at + offset(at))
}

/// A moment already moved to local time, as a calendar date and a time.
fn civil(t: i64) -> Local {
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    // 1970-01-01 was a Thursday (3, counting from Monday).
    let weekday = (days + 3).rem_euclid(7);
    Local { year, month, day, hour: secs / 3600, minute: secs % 3600 / 60, weekday, days }
}

/// The weekday's name ("Monday"), translated.
pub fn weekday_name(weekday: i64) -> &'static str {
    t(match weekday.rem_euclid(7) {
        0 => "Monday",
        1 => "Tuesday",
        2 => "Wednesday",
        3 => "Thursday",
        4 => "Friday",
        5 => "Saturday",
        _ => "Sunday",
    })
}

/// A time of day as the player's language writes it: "14:32" in most,
/// "2:32 PM" in English and the languages that count twelve hours.
pub fn clock(hour: i64, minute: i64) -> String {
    let lang = crate::i18n::lang();
    let twelve = matches!(lang, Lang::English | Lang::Filipino | Lang::Hindi | Lang::Bengali);
    let h12 = if hour % 12 == 0 { 12 } else { hour % 12 };
    let pm = hour >= 12;
    match lang {
        _ if twelve => format!("{h12}:{minute:02} {}", if pm { "PM" } else { "AM" }),
        Lang::Korean => format!("{} {h12}:{minute:02}", if pm { "오후" } else { "오전" }),
        Lang::Arabic => format!("{h12}:{minute:02} {}", if pm { "م" } else { "ص" }),
        _ => format!("{hour}:{minute:02}"),
    }
}

/// The time of day `at` was, in local time ("14:32").
pub fn time_of_day(at: i64) -> String {
    let l = local(at);
    clock(l.hour, l.minute)
}

/// The date of `l` as the language writes it ("27/11/2026").
fn date_of(l: &Local) -> String {
    crate::i18n::lang().date(l.year, l.month, l.day)
}

/// A day of a conversation, for the line between its messages: "Today",
/// "Yesterday", a weekday within the week, else the date.
pub fn day_label(at: i64, now: i64) -> String {
    let (l, today) = (local(at), local(now));
    match today.days - l.days {
        0 => t("Today").to_string(),
        1 => t("Yesterday").to_string(),
        2..=6 => weekday_name(l.weekday).to_string(),
        _ => date_of(&l),
    }
}

/// When something happened, short, for a list (a conversation, a
/// notification): the time today, "Yesterday", a weekday within the week,
/// else the date.
pub fn when_short(at: i64, now: i64) -> String {
    let (l, today) = (local(at), local(now));
    match today.days - l.days {
        0 => clock(l.hour, l.minute),
        1 => t("Yesterday").to_string(),
        2..=6 => weekday_name(l.weekday).to_string(),
        _ => date_of(&l),
    }
}

/// The day and time in full ("Yesterday at 14:32", "27/11/2026 at 9:05"),
/// for a hint over a message.
pub fn when_full(at: i64, now: i64) -> String {
    let l = local(at);
    let time = clock(l.hour, l.minute);
    tw("{day} at {time}", &[("day", &day_label(at, now)), ("time", &time)])
}

/// When a player was last online, for their conversation's header: "last
/// seen today at 14:32", "last seen yesterday at…", "last seen on 2/11".
pub fn last_seen(at: i64, now: i64) -> String {
    let (l, today) = (local(at), local(now));
    let time = clock(l.hour, l.minute);
    match today.days - l.days {
        ..=0 if now - at < 60 => t("last seen just now").to_string(),
        ..=0 => tw("last seen today at {time}", &[("time", &time)]),
        1 => tw("last seen yesterday at {time}", &[("time", &time)]),
        _ => tw("last seen on {date}", &[("date", &date_of(&l))]),
    }
}

/// A day number (local) of `at`: messages of the same day share it.
pub fn day_of(at: i64) -> i64 {
    local(at).days
}

#[cfg(unix)]
mod sys {
    use std::ffi::{c_char, c_int, c_long};

    /// `struct tm` as glibc and the BSDs (macOS) lay it out.
    #[repr(C)]
    struct Tm {
        tm_sec: c_int,
        tm_min: c_int,
        tm_hour: c_int,
        tm_mday: c_int,
        tm_mon: c_int,
        tm_year: c_int,
        tm_wday: c_int,
        tm_yday: c_int,
        tm_isdst: c_int,
        tm_gmtoff: c_long,
        tm_zone: *const c_char,
    }

    unsafe extern "C" {
        fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
    }

    pub fn offset(at: i64) -> i64 {
        let mut tm = Tm {
            tm_sec: 0,
            tm_min: 0,
            tm_hour: 0,
            tm_mday: 0,
            tm_mon: 0,
            tm_year: 0,
            tm_wday: 0,
            tm_yday: 0,
            tm_isdst: 0,
            tm_gmtoff: 0,
            tm_zone: std::ptr::null(),
        };
        // SAFETY: both pointers are valid for the call; `localtime_r` is
        // the thread-safe form, writing only into `tm`.
        let done = unsafe { localtime_r(&at, &mut tm) };
        if done.is_null() { 0 } else { tm.tm_gmtoff as i64 }
    }
}

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn FileTimeToSystemTime(file: *const FileTime, system: *mut SystemTime) -> i32;
        fn SystemTimeToTzSpecificLocalTime(
            zone: *const c_void,
            utc: *const SystemTime,
            local: *mut SystemTime,
        ) -> i32;
        fn SystemTimeToFileTime(system: *const SystemTime, file: *mut FileTime) -> i32;
    }

    /// 100 ns steps between 1601-01-01 (Windows) and 1970-01-01 (Unix).
    const EPOCH: i64 = 116_444_736_000_000_000;

    pub fn offset(at: i64) -> i64 {
        let ticks = at.saturating_mul(10_000_000).saturating_add(EPOCH);
        if ticks < 0 {
            return 0;
        }
        let utc_file = FileTime { low: ticks as u32, high: (ticks >> 32) as u32 };
        let mut utc = SystemTime::default();
        let mut local = SystemTime::default();
        let mut local_file = FileTime::default();
        // SAFETY: every pointer is to a local of the right layout; a null
        // zone is the computer's own time zone.
        let ok = unsafe {
            FileTimeToSystemTime(&utc_file, &mut utc) != 0
                && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) != 0
                && SystemTimeToFileTime(&local, &mut local_file) != 0
        };
        if !ok {
            return 0;
        }
        let local_ticks = ((local_file.high as i64) << 32) | local_file.low as i64;
        (local_ticks - ticks).div_euclid(10_000_000)
    }
}

#[cfg(not(any(unix, windows)))]
mod sys {
    pub fn offset(_at: i64) -> i64 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_calendar_day_and_its_weekday() {
        // 2026-11-27 13:45 UTC was a Friday.
        let l = civil(1_795_787_100);
        assert_eq!((l.year, l.month, l.day, l.hour, l.minute), (2026, 11, 27, 13, 45));
        assert_eq!(l.weekday, 4);
        assert_eq!(civil(0).weekday, 3, "1970-01-01 was a Thursday");
        assert_eq!(civil(-1).day, 31);
    }

    #[test]
    fn the_offset_is_a_real_time_zone() {
        let now = crate::game::unix_now();
        let off = offset(now);
        assert!(off.abs() <= 14 * 3600, "{off}");
        assert_eq!(off % 900, 0, "time zones are whole quarters of an hour: {off}");
    }

    #[test]
    fn times_of_day_per_language() {
        let _lang = crate::i18n::lang_lock();
        crate::i18n::set_lang(Lang::Portuguese);
        assert_eq!(clock(14, 5), "14:05");
        assert_eq!(clock(0, 30), "0:30");
        crate::i18n::set_lang(Lang::English);
        assert_eq!(clock(14, 5), "2:05 PM");
        assert_eq!(clock(0, 30), "12:30 AM");
        assert_eq!(clock(12, 0), "12:00 PM");
    }

    #[test]
    fn days_read_like_a_messaging_app() {
        let _lang = crate::i18n::lang_lock();
        crate::i18n::set_lang(Lang::English);
        let now = crate::game::unix_now();
        assert_eq!(day_label(now, now), "Today");
        assert_eq!(day_label(now - 86_400, now), "Yesterday");
        assert!(!day_label(now - 3 * 86_400, now).contains('-'), "a weekday");
        assert!(day_label(now - 30 * 86_400, now).contains('-'), "a date");
        assert_eq!(last_seen(now - 10, now), "last seen just now");
    }
}
