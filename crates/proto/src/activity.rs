//! What a friend is doing right now, in detail ("Playing against Ana",
//! "In the daily challenge", "Looking for a game"): shown in the friends
//! list, with a button to watch the game they play.
//!
//! The server works it out from what it knows (a game, the queue, a table,
//! a tournament, a game being watched) and from what the game says the
//! player does outside online games ([`Pastime`]), and tells the player's
//! friends whenever it changes ([`crate::ServerMsg::FriendActivity`]).

use serde::{Deserialize, Serialize};

use crate::Rules;

/// What a friend online is doing.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Activity {
    /// In the lounge, doing nothing in particular.
    #[default]
    Lounge,
    /// In the "Play now" queue.
    Searching {
        rules: Rules,
    },
    /// At a table of the lounge, waiting for an opponent.
    AtTable {
        rules: Rules,
    },
    /// Playing an online game (`match_id` can be watched).
    Playing {
        opponent: String,
        match_id: u64,
        rules: Rules,
        #[serde(default)]
        rated: bool,
        /// A tournament's game.
        #[serde(default)]
        tournament: bool,
    },
    /// Watching someone's game.
    Watching {
        player: String,
    },
    /// In a tournament, between games.
    Tournament {
        name: String,
    },
    /// A game against the computer.
    VsComputer,
    /// A table to themself.
    Practice,
    /// The day's challenge.
    DailyChallenge,
    Drills,
    /// The tour against the computer.
    Ladder,
    /// Nobody has touched their game for a while.
    Away,
    /// Something a newer game knows and this one doesn't.
    #[serde(other)]
    Other,
}

impl Activity {
    /// A game that can be watched.
    pub fn watchable(&self) -> bool {
        matches!(self, Activity::Playing { .. })
    }
}

/// What a player does outside online games, as their game says it
/// ([`crate::ClientMsg::SetActivity`]): it refines "playing the computer"
/// for their friends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pastime {
    #[default]
    None,
    Computer,
    Practice,
    Daily,
    Drills,
    Ladder,
    #[serde(other)]
    Other,
}

impl Pastime {
    pub fn is_none(&self) -> bool {
        *self == Pastime::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activities_round_trip_and_unknown_ones_are_other() {
        let a = Activity::Playing {
            opponent: "ana".into(),
            match_id: 7,
            rules: Rules::Official,
            rated: true,
            tournament: false,
        };
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(serde_json::from_str::<Activity>(&json).unwrap(), a);
        assert!(a.watchable());
        let future: Activity = serde_json::from_str(r#"{"kind":"skydiving"}"#).unwrap();
        assert_eq!(future, Activity::Other);
        let p: Pastime = serde_json::from_str(r#""daily""#).unwrap();
        assert_eq!(p, Pastime::Daily);
        assert_eq!(serde_json::from_str::<Pastime>(r#""chess""#).unwrap(), Pastime::Other);
    }
}
