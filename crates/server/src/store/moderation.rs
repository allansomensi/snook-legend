//! The store's side of the inbox and the moderation (0.12, migration 0027):
//! notifications, the banned words, the history of names and countries,
//! ratings put right, games called off and how long each shot was thought
//! over. Both back ends behave the same (`memory/moderation.rs`,
//! `postgres/moderation.rs`).

use snook_legend_proto::Rules;

use super::{ReportRow, Store, StoreResult, User};

/// A notification as stored. `kind` and `action` are JSON as the game reads
/// them (see `snook_legend_proto::inbox`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NotificationRow {
    pub id: i64,
    pub user_id: i64,
    pub kind: String,
    pub text: String,
    /// The staff member who sent it (empty: the game, or an account gone).
    pub from: String,
    pub from_id: Option<i64>,
    pub created_at: i64,
    pub read_at: Option<i64>,
    pub important: bool,
    pub pinned_until: i64,
    pub action: String,
    /// The player took it out of their inbox (the staff still see it).
    pub deleted_at: Option<i64>,
}

/// A notification to store.
#[derive(Debug, Clone)]
pub struct NewNotification<'a> {
    pub user_id: i64,
    pub kind: &'a str,
    pub text: &'a str,
    pub from_id: Option<i64>,
    pub created_at: i64,
    pub important: bool,
    pub pinned_until: i64,
    pub action: &'a str,
}

/// A word on the list of banned words.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BannedWord {
    pub word: String,
    /// A language code, or `*` for all.
    pub lang: String,
    /// Who added it (empty: the starting list, or an account gone).
    pub added_by: String,
    pub added_at: i64,
}

/// A change of name or country.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProfileChangeRow {
    pub user_id: i64,
    /// `name` or `country`.
    pub kind: String,
    pub old: String,
    pub new: String,
    pub at: i64,
    /// The staff member who made it (empty: the player).
    pub by: String,
}

/// A rating put right by the staff (or given back with a game called off).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RatingAdjustment {
    pub id: i64,
    pub user_id: i64,
    pub rules: Rules,
    pub delta: i32,
    pub rating_after: i32,
    pub reason: String,
    pub match_id: Option<i64>,
    /// Who did it (empty: an account gone).
    pub staff: String,
    pub at: i64,
}

/// A stored game, as the staff look it up.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MatchRow {
    pub id: i64,
    pub players: [i64; 2],
    pub names: [String; 2],
    pub winner: usize,
    pub reason: String,
    pub rated: bool,
    pub rules: Rules,
    pub delta: [i32; 2],
    pub ratings_after: [i32; 2],
    pub shots: u32,
    pub ended_at: i64,
    pub tournament: Option<i64>,
    /// The live game it was (its room).
    pub room: Option<i64>,
    /// Called off by the staff at this time.
    pub voided_at: Option<i64>,
}

/// Another account seen from the same address as a player.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LinkedAccount {
    pub id: i64,
    pub username: String,
    pub created_at: i64,
}

impl Store {
    /// Stores a notification; returns its id. Past `keep` in the player's
    /// inbox, the oldest read ones that aren't pinned go.
    pub async fn add_notification(&self, n: &NewNotification<'_>, keep: usize) -> StoreResult<i64> {
        dispatch!(self, s => s.add_notification(n, keep))
    }
    /// The player's inbox (not what they took out), newest first.
    pub async fn notifications(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<NotificationRow>> {
        dispatch!(self, s => s.notifications(user_id, limit))
    }
    /// Every notification sent to the player, also those they took out
    /// (for the staff), newest first.
    pub async fn notifications_sent(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<NotificationRow>> {
        dispatch!(self, s => s.notifications_sent(user_id, limit))
    }
    /// Marks the player's notifications `ids` read (those not read yet);
    /// returns how many.
    pub async fn read_notifications(&self, user_id: i64, ids: &[i64], at: i64) -> StoreResult<u64> {
        dispatch!(self, s => s.read_notifications(user_id, ids, at))
    }
    /// Takes the player's notifications `ids` out of their inbox, except
    /// those pinned now and the important ones not read yet; returns how
    /// many went.
    pub async fn delete_notifications(
        &self,
        user_id: i64,
        ids: &[i64],
        now: i64,
    ) -> StoreResult<u64> {
        dispatch!(self, s => s.delete_notifications(user_id, ids, now))
    }

    /// The banned words, by language then word.
    pub async fn banned_words(&self) -> StoreResult<Vec<BannedWord>> {
        dispatch!(self, s => s.banned_words())
    }
    /// Adds a word (false: it was listed already).
    pub async fn add_banned_word(
        &self,
        word: &str,
        lang: &str,
        by: i64,
        at: i64,
    ) -> StoreResult<bool> {
        dispatch!(self, s => s.add_banned_word(word, lang, by, at))
    }
    /// Takes a word off (false: it wasn't listed).
    pub async fn remove_banned_word(&self, word: &str, lang: &str) -> StoreResult<bool> {
        dispatch!(self, s => s.remove_banned_word(word, lang))
    }

    /// Records a change of name (`kind` "name") or country ("country").
    pub async fn add_profile_change(
        &self,
        user_id: i64,
        kind: &str,
        old: &str,
        new: &str,
        at: i64,
        by: Option<i64>,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.add_profile_change(user_id, kind, old, new, at, by))
    }
    /// The player's changes of name and country, newest first.
    pub async fn profile_changes(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<ProfileChangeRow>> {
        dispatch!(self, s => s.profile_changes(user_id, limit))
    }

    /// Puts the player's rating under `rules` right by `delta` (the peak
    /// follows: up with it, or down by as much, never under the rating),
    /// and keeps why in their rating history. The account as it is now
    /// (None: no such account).
    #[allow(clippy::too_many_arguments)]
    pub async fn adjust_rating(
        &self,
        user_id: i64,
        rules: Rules,
        delta: i32,
        reason: &str,
        match_id: Option<i64>,
        staff_id: i64,
        at: i64,
    ) -> StoreResult<Option<User>> {
        dispatch!(self, s => s.adjust_rating(user_id, rules, delta, reason, match_id, staff_id, at))
    }
    /// The player's ratings put right, newest first.
    pub async fn rating_adjustments(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<RatingAdjustment>> {
        dispatch!(self, s => s.rating_adjustments(user_id, limit))
    }

    /// A stored game.
    pub async fn match_row(&self, id: i64) -> StoreResult<Option<MatchRow>> {
        dispatch!(self, s => s.match_row(id))
    }
    /// Calls a rated game off: each player's rating change is given back
    /// (in their rating history as a change of its own), the game no
    /// longer counts in their rated games and wins, and the peak a player
    /// reached with it is worked out again. None when there is no such
    /// rated game, or it was called off already; else both accounts as
    /// they are now.
    pub async fn void_match(
        &self,
        id: i64,
        staff_id: i64,
        reason: &str,
        at: i64,
    ) -> StoreResult<Option<[Option<User>; 2]>> {
        dispatch!(self, s => s.void_match(id, staff_id, reason, at))
    }

    /// How long each stroke of the game of `room` was thought over (tenths
    /// of a second; None for what isn't a shot).
    pub async fn save_shot_times(
        &self,
        room: i64,
        times: &[Option<u32>],
        at: i64,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.save_shot_times(room, times, at))
    }
    pub async fn shot_times(&self, room: i64) -> StoreResult<Option<Vec<Option<u32>>>> {
        dispatch!(self, s => s.shot_times(room))
    }

    /// The reports the player sent, newest first.
    pub async fn reports_by(&self, user_id: i64, limit: u32) -> StoreResult<Vec<ReportRow>> {
        dispatch!(self, s => s.reports_by(user_id, limit))
    }
    /// Other accounts registered from the address this one was registered
    /// from (the database keeps it; the memory store doesn't).
    pub async fn linked_accounts(&self, user_id: i64) -> StoreResult<Vec<LinkedAccount>> {
        dispatch!(self, s => s.linked_accounts(user_id))
    }
}
