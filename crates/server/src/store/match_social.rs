//! What 0.12's match and social features keep (migration 0030): the
//! settings an account carries from computer to computer, direct messages
//! between friends, the quick-phrases-only chat a parent or guardian may
//! choose, and the week-by-week pot attempts of the advanced statistics.

use snook_legend_proto::Rules;
use snook_legend_proto::analysis::ShotAnalysis;

use super::{Store, StoreResult};

/// A direct message as stored (players by id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmRow {
    pub id: i64,
    pub from: i64,
    pub to: i64,
    pub text: String,
    pub at: i64,
    /// When the receiver read it.
    pub read_at: Option<i64>,
}

/// A conversation in the list: the other player, the newest message and how
/// many of theirs are unread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmThreadRow {
    pub other: i64,
    pub last: DmRow,
    pub unread: u32,
}

/// One week of pot attempts of a player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeekRow {
    pub week: i64,
    pub games: u32,
    pub attempts: u32,
    pub made: u32,
}

/// Conversations listed at most.
pub const DM_THREADS: u32 = 100;
/// Messages kept per conversation (the oldest go as new ones come).
pub const DM_KEPT: usize = 500;

impl Store {
    /// The account's settings and when they were saved.
    pub async fn account_settings(&self, user_id: i64) -> StoreResult<Option<(String, i64)>> {
        dispatch!(self, s => s.account_settings(user_id))
    }
    /// Keeps the account's settings (replacing the last ones).
    pub async fn save_account_settings(
        &self,
        user_id: i64,
        data: &str,
        at: i64,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.save_account_settings(user_id, data, at))
    }
    /// Stores a direct message; returns it as kept (with its number). The
    /// oldest messages of the conversation beyond [`DM_KEPT`] go.
    pub async fn dm_send(&self, from: i64, to: i64, text: &str, at: i64) -> StoreResult<DmRow> {
        dispatch!(self, s => s.dm_send(from, to, text, at))
    }
    /// The newest `limit` messages between `a` and `b` (before the message
    /// numbered `before`, if given), newest first.
    pub async fn dm_history(
        &self,
        a: i64,
        b: i64,
        before: Option<i64>,
        limit: u32,
    ) -> StoreResult<Vec<DmRow>> {
        dispatch!(self, s => s.dm_history(a, b, before, limit))
    }
    /// `me` read what `with` sent them; how many messages that was.
    pub async fn dm_read(&self, me: i64, with: i64, at: i64) -> StoreResult<u64> {
        dispatch!(self, s => s.dm_read(me, with, at))
    }
    /// `me`'s conversations, newest first (at most [`DM_THREADS`]).
    pub async fn dm_threads(&self, me: i64) -> StoreResult<Vec<DmThreadRow>> {
        dispatch!(self, s => s.dm_threads(me))
    }
    /// The messages `from` sent `to` since `to` last wrote back (all of
    /// them when `to` never did).
    pub async fn dm_waiting(&self, from: i64, to: i64) -> StoreResult<u32> {
        dispatch!(self, s => s.dm_waiting(from, to))
    }
    /// The conversations `from` started since `since` (their first message
    /// was `from`'s, at `since` or later).
    pub async fn dm_started(&self, from: i64, since: i64) -> StoreResult<u32> {
        dispatch!(self, s => s.dm_started(from, since))
    }
    /// A parent or guardian keeps the chat to the quick phrases (or not).
    pub async fn set_phrases_only(&self, user_id: i64, on: bool) -> StoreResult<()> {
        dispatch!(self, s => s.set_phrases_only(user_id, on))
    }
    /// Adds a game's pot attempts to its week.
    pub async fn add_shot_week(
        &self,
        user_id: i64,
        week: i64,
        attempts: u32,
        made: u32,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.add_shot_week(user_id, week, attempts, made))
    }
    /// The player's newest `limit` weeks with games, newest first.
    pub async fn shot_weeks(&self, user_id: i64, limit: u32) -> StoreResult<Vec<WeekRow>> {
        dispatch!(self, s => s.shot_weeks(user_id, limit))
    }
    /// The shot analysis of every player (not deleted) whose rating under
    /// `rules` is in `min..max` with rated games there, summed; and how many
    /// players that is.
    pub async fn league_analysis(
        &self,
        rules: Rules,
        min: i32,
        max: i32,
    ) -> StoreResult<(ShotAnalysis, u32)> {
        dispatch!(self, s => s.league_analysis(rules, min, max))
    }
}
