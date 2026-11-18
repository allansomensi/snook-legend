//! Events the staff schedule: a time when experience or coins count double
//! (or triple), or a themed tournament the server organises. Players are told
//! about the events running and coming ([`crate::ServerMsg::Events`]) when
//! they sign in and whenever the list changes.

use serde::{Deserialize, Serialize};

/// What an event does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Experience counts `multiplier` times (games, the daily challenge,
    /// drills).
    Xp,
    /// Coins from games count `multiplier` times (and so does the day's cap).
    Coins,
    /// A tournament the server organises (in Tournaments, open for sign-up
    /// two hours before it starts).
    Tournament,
    /// A kind a newer server knows.
    #[serde(other)]
    Unknown,
}

impl EventKind {
    pub fn code(self) -> &'static str {
        match self {
            EventKind::Xp => "xp",
            EventKind::Coins => "coins",
            EventKind::Tournament => "tournament",
            EventKind::Unknown => "unknown",
        }
    }

    pub fn from_code(code: &str) -> EventKind {
        match code {
            "xp" => EventKind::Xp,
            "coins" => EventKind::Coins,
            "tournament" => EventKind::Tournament,
            _ => EventKind::Unknown,
        }
    }

    /// A boost (experience or coins), not a tournament.
    pub fn is_boost(self) -> bool {
        matches!(self, EventKind::Xp | EventKind::Coins)
    }
}

/// An event as players see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventInfo {
    pub id: i64,
    pub kind: EventKind,
    /// Written by the staff (one language: the server's).
    pub title: String,
    /// 2 or 3 for a boost; 1 for a tournament.
    pub multiplier: u32,
    /// Unix seconds.
    pub starts_at: i64,
    pub ends_at: i64,
}

impl EventInfo {
    /// Running at `now` (Unix seconds).
    pub fn is_on(&self, now: i64) -> bool {
        self.starts_at <= now && now < self.ends_at
    }
}

/// The boost of `kind` at `now` among `events`: the largest multiplier of
/// those running (boosts don't add up), 1 when none is.
pub fn boost(events: &[EventInfo], kind: EventKind, now: i64) -> u32 {
    events
        .iter()
        .filter(|e| e.kind == kind && e.is_on(now))
        .map(|e| e.multiplier.clamp(1, MAX_MULTIPLIER))
        .max()
        .unwrap_or(1)
}

/// The largest boost an event may give.
pub const MAX_MULTIPLIER: u32 = 3;
/// The longest a boost may last (seconds): two weeks.
pub const MAX_BOOST_LENGTH: i64 = 14 * 86_400;
/// Longest title.
pub const MAX_TITLE: usize = 60;
/// Events further ahead than this aren't sent to players yet (seconds).
pub const SHOWN_AHEAD: i64 = 14 * 86_400;

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: EventKind, multiplier: u32, starts_at: i64, ends_at: i64) -> EventInfo {
        EventInfo { id: 1, kind, title: String::new(), multiplier, starts_at, ends_at }
    }

    #[test]
    fn the_largest_running_boost_counts() {
        let events = [
            event(EventKind::Xp, 2, 100, 200),
            event(EventKind::Xp, 3, 150, 160),
            event(EventKind::Coins, 2, 300, 400),
            event(EventKind::Tournament, 1, 100, 200),
            event(EventKind::Xp, 9, 100, 200),
        ];
        assert_eq!(boost(&events[..2], EventKind::Xp, 99), 1, "not yet");
        assert_eq!(boost(&events[..2], EventKind::Xp, 100), 2);
        assert_eq!(boost(&events[..2], EventKind::Xp, 155), 3, "the larger, not the sum");
        assert_eq!(boost(&events[..2], EventKind::Xp, 200), 1, "over at the end");
        assert_eq!(boost(&events, EventKind::Coins, 155), 1);
        assert_eq!(boost(&events, EventKind::Coins, 350), 2);
        assert_eq!(boost(&events[4..], EventKind::Xp, 150), MAX_MULTIPLIER, "kept in range");
    }

    #[test]
    fn kinds_round_trip_and_unknown_ones_are_kept_out() {
        for k in [EventKind::Xp, EventKind::Coins, EventKind::Tournament] {
            assert_eq!(EventKind::from_code(k.code()), k);
            let json = serde_json::to_string(&k).unwrap();
            assert_eq!(serde_json::from_str::<EventKind>(&json).unwrap(), k);
        }
        assert_eq!(serde_json::from_str::<EventKind>("\"raffle\"").unwrap(), EventKind::Unknown);
        assert!(EventKind::Xp.is_boost() && !EventKind::Tournament.is_boost());
    }
}
