//! What 0.12's new modes keep (migration 0031): each player's doubles
//! records and the doubles games played, and the attempts, weekly bests and
//! prizes against the clock.

use snook_legend_core::GameOverReason;
use snook_legend_proto::replay::Replay;
use snook_legend_proto::{LeaderboardEntry, PerRules, Rules};

use super::{Record, Store, StoreResult};

/// The doubles rating everyone starts at.
pub const DOUBLES_START: i32 = 1200;

/// A finished doubles game, as recorded.
#[derive(Debug, Clone, PartialEq)]
pub struct DoublesRecord {
    /// The teams in seat order, each with its first and second player.
    pub players: [[i64; 2]; 2],
    /// The winning seat.
    pub winner: usize,
    pub reason: GameOverReason,
    pub rated: bool,
    pub rules: Rules,
    /// Each player's rating change (`[team][player]`; zeros when casual).
    pub deltas: [[i32; 2]; 2],
    pub shots: u32,
    pub ended_at: i64,
    pub replay: Option<Replay>,
    /// The lounge's id of the game: it is recorded once (None in tests).
    pub room: Option<i64>,
}

/// An attempt against the clock under way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptRow {
    pub id: i64,
    pub seed: u64,
    pub started_ms: i64,
}

/// A line of a week's table against the clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BestRow {
    pub user_id: i64,
    pub username: String,
    pub country: String,
    pub ms: u32,
    pub at: i64,
}

/// What finishing an attempt did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptFinish {
    /// No such attempt under way (sent already, or too late).
    NotStarted,
    /// Counted: `best` if it is the player's best of the week now.
    Counted { best: bool },
}

impl Store {
    /// A player's doubles records under each of the rules.
    pub async fn doubles_records(&self, user_id: i64) -> StoreResult<PerRules<Record>> {
        dispatch!(self, s => s.doubles_records(user_id))
    }
    /// Records a finished doubles game: its row, and for a rated game each
    /// player's new record. False (and nothing written) when the game was
    /// recorded already (`room`).
    pub async fn record_doubles(&self, record: &DoublesRecord) -> StoreResult<bool> {
        dispatch!(self, s => s.record_doubles(record))
    }
    /// The doubles ranking of `rules` (players with rated doubles games;
    /// no banned or deleted account).
    pub async fn doubles_leaderboard(
        &self,
        rules: Rules,
        limit: u32,
        now: i64,
    ) -> StoreResult<Vec<LeaderboardEntry>> {
        dispatch!(self, s => s.doubles_leaderboard(rules, limit, now))
    }
    /// Starts an attempt against the clock (its rack `seed`, at
    /// `now_ms`) if the player has one left on `day` (at most `max`): its
    /// number. An attempt still under way is given up.
    pub async fn time_attack_start(
        &self,
        user_id: i64,
        day: i64,
        now_ms: i64,
        seed: u64,
        max: u32,
    ) -> StoreResult<Option<i64>> {
        dispatch!(self, s => s.time_attack_start(user_id, day, now_ms, seed, max))
    }
    /// The player's attempt under way (the newest not sent), if any.
    pub async fn time_attack_open(&self, user_id: i64) -> StoreResult<Option<AttemptRow>> {
        dispatch!(self, s => s.time_attack_open(user_id))
    }
    /// Attempts started on `day`.
    pub async fn time_attack_attempts(&self, user_id: i64, day: i64) -> StoreResult<u32> {
        dispatch!(self, s => s.time_attack_attempts(user_id, day))
    }
    /// Ends attempt `id` of the player: with its time `ms` (milliseconds,
    /// counted in `week`, made at `at`), or refused (`None`: nothing
    /// counts). Only an attempt still under way.
    #[allow(clippy::too_many_arguments)]
    pub async fn time_attack_finish(
        &self,
        user_id: i64,
        id: i64,
        now_ms: i64,
        week: i64,
        at: i64,
        result: Option<(u32, u32, u32)>,
    ) -> StoreResult<AttemptFinish> {
        dispatch!(self, s => s.time_attack_finish(user_id, id, now_ms, week, at, result))
    }
    /// The quickest `limit` of `week`, and how many cleared the table.
    pub async fn time_attack_board(
        &self,
        week: i64,
        limit: u32,
    ) -> StoreResult<(Vec<BestRow>, u32)> {
        dispatch!(self, s => s.time_attack_board(week, limit))
    }
    /// The player's best of `week` and their place (none while they are
    /// off the boards: banned).
    pub async fn time_attack_best(
        &self,
        user_id: i64,
        week: i64,
    ) -> StoreResult<Option<(u32, Option<u32>)>> {
        dispatch!(self, s => s.time_attack_best(user_id, week))
    }
    /// Keeps that `week`'s `place` (1 to 3) went to `user_id`: true the
    /// first time only (the prize is paid then).
    pub async fn time_attack_prize(
        &self,
        week: i64,
        place: u32,
        user_id: i64,
        coins: i64,
        ms: u32,
        at: i64,
    ) -> StoreResult<bool> {
        dispatch!(self, s => s.time_attack_prize(week, place, user_id, coins, ms, at))
    }
}
