//! The player's inbox: what the staff and the game tell one player, kept
//! until it is read (and after, to be looked up again). A warning, a mute,
//! a rating put right, a present from the staff, a tournament about to
//! start, an item won…
//!
//! The server sends the facts ([`Kind`] and its fields); the game words them
//! in the player's language. What a staff member wrote (`text`) comes
//! through as written.

use serde::{Deserialize, Serialize};

use crate::Rules;

/// Most notifications kept per player (the oldest read ones go first;
/// pinned and unread ones stay).
pub const MAX_KEPT: usize = 200;
/// The longest text a staff member may write in a notification.
pub const MAX_TEXT: usize = 1000;

/// The inbox's filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Messages and warnings from the staff.
    Staff,
    /// Mutes, ratings put right, games called off.
    Punishment,
    /// Presents (from the staff or a friend), items won, VIP's item of the
    /// month.
    Gift,
    /// Tournaments: about to start, results, prizes.
    Tournament,
    /// Achievements unlocked.
    Achievement,
    /// The rest: reports answered, auctions, maintenance…
    #[serde(other)]
    Game,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Staff,
        Category::Punishment,
        Category::Gift,
        Category::Tournament,
        Category::Achievement,
        Category::Game,
    ];
}

/// What happened. Every variant carries what the game needs to word it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Kind {
    /// A message from a staff member (`text` is theirs).
    StaffMessage,
    /// A warning from the staff (`text`: why).
    Warning,
    /// Muted until `until` (Unix seconds): no lounge, match or board chat.
    /// `text`: why.
    Muted { until: i64 },
    /// The mute was lifted early.
    Unmuted,
    /// The rating under `rules` was put right by `delta`, to `rating`.
    /// `text`: why. `match_id`: the game it is about (0: none).
    RatingAdjusted {
        rules: Rules,
        delta: i32,
        rating: i32,
        #[serde(default)]
        match_id: i64,
    },
    /// A game was called off (annulled) by the staff: its rating change
    /// (`delta`, for this player) was given back. `text`: why.
    MatchVoided { match_id: i64, opponent: String, delta: i32 },
    /// A stuck game was ended by the staff. `won`: this player was given the
    /// game (None: nobody, no rating changed). `text`: why.
    MatchEnded {
        match_id: i64,
        opponent: String,
        #[serde(default)]
        won: Option<bool>,
    },
    /// A report this player sent led the staff to act: thank you.
    ReportActed { username: String },
    /// A present from the staff: coins, an item (shop code; empty: none) and
    /// days of VIP. `text`: why.
    StaffGift {
        #[serde(default)]
        coins: i64,
        #[serde(default)]
        item: String,
        #[serde(default)]
        vip_days: u32,
    },
    /// An item was won (a tournament, an event, an auction, a season…):
    /// `origin` says how (see [`crate::collection::Origin`]).
    ItemAwarded { code: String, origin: crate::collection::Origin },
    /// VIP's item of the month `month` (`YYYY-MM`) is now this player's for
    /// good.
    VipItem { code: String, month: String },
    /// An achievement was unlocked.
    Achievement { code: String },
    /// A tournament this player signed up for starts at `starts_at`.
    TournamentStarting { id: u64, name: String, starts_at: i64 },
    /// A tournament ended: this player's place and prize (coins).
    TournamentResult {
        id: u64,
        name: String,
        place: u32,
        #[serde(default)]
        prize: i64,
    },
    /// Someone bid more on an auction this player led; their coins came
    /// back.
    AuctionOutbid { auction: i64, code: String, bid: i64 },
    /// This player won an auction.
    AuctionWon { auction: i64, code: String, price: i64 },
    /// The server stops for maintenance at `at` for about `minutes`.
    Maintenance { at: i64, minutes: u32 },
    /// A kind this version doesn't know yet (shown with its `text`).
    #[serde(other)]
    Other,
}

impl Kind {
    pub fn category(&self) -> Category {
        match self {
            Kind::StaffMessage | Kind::Warning => Category::Staff,
            Kind::Muted { .. }
            | Kind::Unmuted
            | Kind::RatingAdjusted { .. }
            | Kind::MatchVoided { .. }
            | Kind::MatchEnded { .. } => Category::Punishment,
            Kind::StaffGift { .. } | Kind::ItemAwarded { .. } | Kind::VipItem { .. } => {
                Category::Gift
            }
            Kind::Achievement { .. } => Category::Achievement,
            Kind::TournamentStarting { .. } | Kind::TournamentResult { .. } => Category::Tournament,
            Kind::ReportActed { .. }
            | Kind::AuctionOutbid { .. }
            | Kind::AuctionWon { .. }
            | Kind::Maintenance { .. }
            | Kind::Other => Category::Game,
        }
    }
}

/// Where a notification's button leads in the game.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum Action {
    #[default]
    None,
    /// Watch a stored game again ("see the game").
    Replay {
        match_id: i64,
    },
    /// Put an item on ("equip it now").
    Equip {
        code: String,
    },
    /// The collection, on this item.
    Collection {
        code: String,
    },
    /// A tournament ("join").
    Tournament {
        id: u64,
    },
    /// A player's profile.
    Profile {
        username: String,
    },
    /// The shop (on an item when `code` isn't empty).
    Shop {
        #[serde(default)]
        code: String,
    },
    Auctions,
    Vip,
    /// A version this game doesn't know.
    #[serde(other)]
    Other,
}

/// One notification, as the player sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub id: i64,
    #[serde(flatten)]
    pub kind: Kind,
    /// What the staff wrote (as written; empty for most of the game's own).
    #[serde(default)]
    pub text: String,
    /// The staff member who sent it (empty: the game).
    #[serde(default)]
    pub from: String,
    /// Unix seconds.
    pub created_at: i64,
    /// When the player read it (0: not yet). For an important one, when they
    /// confirmed they read it.
    #[serde(default)]
    pub read_at: i64,
    /// The staff want to know it was read: the game asks the player to
    /// confirm, and the portal shows when they did.
    #[serde(default)]
    pub important: bool,
    /// Stays at the top until then (Unix seconds; 0: not pinned;
    /// `i64::MAX`: until taken down).
    #[serde(default)]
    pub pinned_until: i64,
    #[serde(default)]
    pub action: Action,
}

impl Notification {
    pub fn category(&self) -> Category {
        self.kind.category()
    }

    pub fn unread(&self) -> bool {
        self.read_at == 0
    }

    pub fn pinned(&self, now: i64) -> bool {
        self.pinned_until > now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_the_unknown() {
        let n = Notification {
            id: 7,
            kind: Kind::RatingAdjusted {
                rules: Rules::Official,
                delta: -42,
                rating: 1500,
                match_id: 9,
            },
            text: "farm".into(),
            from: "mod".into(),
            created_at: 1,
            read_at: 0,
            important: true,
            pinned_until: 0,
            action: Action::Replay { match_id: 9 },
        };
        let json = serde_json::to_string(&n).unwrap();
        assert_eq!(serde_json::from_str::<Notification>(&json).unwrap(), n);
        assert_eq!(n.category(), Category::Punishment);
        let later: Notification = serde_json::from_str(
            r#"{"id":1,"kind":"something_new","created_at":5,"action":{"to":"somewhere"}}"#,
        )
        .unwrap();
        assert_eq!(later.kind, Kind::Other);
        assert_eq!(later.action, Action::Other);
    }
}
