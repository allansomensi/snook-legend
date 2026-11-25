//! The player's collection: every item they have, with how and when they
//! got it ("Champion of the Winter Cup 2026", 12 Dec 2026). An item is a
//! keepsake as much as a look.

use serde::{Deserialize, Serialize};

/// How an item came to the player.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "how", rename_all = "snake_case")]
pub enum Origin {
    /// Bought in the shop for `price` coins.
    Bought { price: i64 },
    /// A present from a friend.
    Gift { from: String },
    /// Given by the staff (`reason`: what for, e.g. "Community cup winner").
    Staff {
        #[serde(default)]
        reason: String,
    },
    /// Won in a tournament (`place`: 1 for the champion).
    Tournament { name: String, place: u32 },
    /// Came with an achievement.
    Achievement { code: String },
    /// Won at auction for `price` coins.
    Auction { price: i64 },
    /// VIP's item of the month `month` (`YYYY-MM`).
    VipMonth { month: String },
    /// An event's prize (`name`: the event's title).
    Event { name: String },
    /// A season's prize (`name`: e.g. "Season 3").
    Season { name: String },
    /// The welcome pack.
    WelcomePack,
    /// From before origins were kept, or not known.
    #[default]
    #[serde(other)]
    Unknown,
}

/// An item this player has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedItem {
    /// Shop code (see [`crate::shop::ITEMS`]).
    pub code: String,
    #[serde(default)]
    pub origin: Origin,
    /// Unix seconds (0: not known).
    #[serde(default)]
    pub acquired_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_the_unknown() {
        let item = OwnedItem {
            code: "cue_champion".into(),
            origin: Origin::Tournament { name: "Winter Cup 2026".into(), place: 1 },
            acquired_at: 1_800_000_000,
        };
        let json = serde_json::to_string(&item).unwrap();
        assert_eq!(serde_json::from_str::<OwnedItem>(&json).unwrap(), item);
        let later: OwnedItem =
            serde_json::from_str(r#"{"code":"x","origin":{"how":"raffle"}}"#).unwrap();
        assert_eq!(later.origin, Origin::Unknown);
    }
}
