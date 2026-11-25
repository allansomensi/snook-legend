//! Persistent storage: accounts, sessions, match history, achievements,
//! friends, reports and the staff audit log.
//!
//! Two back ends with the same behaviour: PostgreSQL for production
//! (feature `postgres`) and an in-memory store for tests and LAN games.

mod memory;
#[cfg(feature = "postgres")]
mod postgres;

pub use memory::MemoryStore;
#[cfg(feature = "postgres")]
pub use postgres::PgStore;

use std::collections::BTreeMap;

use snook_legend_core::stats::ShotStats;
use snook_legend_proto::replay::Replay;
use snook_legend_proto::{CareerNumbers, LeaderboardEntry, PerRules, Role, Rules, Standing, Stats};

#[derive(Debug, Clone)]
pub struct StoreError(pub String);

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StoreError {}

pub type StoreResult<T> = Result<T, StoreError>;

/// An account.
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub country: String,
    pub role: Role,
    /// Their rating and rated games under each of the rules.
    pub records: PerRules<Record>,
    pub eight_wins: u32,
    pub casual_games: u32,
    pub streak: u32,
    pub best_streak: u32,
    pub runouts: u32,
    pub shutouts: u32,
    pub tournaments_played: u32,
    pub tournaments_won: u32,
    /// Unix seconds.
    pub created_at: i64,
    pub last_seen: i64,
    /// Banned until this time (Unix seconds); `i64::MAX` is permanent.
    pub banned_until: Option<i64>,
    pub ban_reason: Option<String>,
    pub muted_until: Option<i64>,
    /// Where a forgotten password can be sent (set by the player).
    pub recovery_email: Option<String>,
    /// The account was deleted (and anonymised) at this time.
    pub deleted_at: Option<i64>,
    /// Games lost by leaving or by running out of reconnection time.
    pub abandons: u32,
    /// The player's status message (empty: none).
    pub status_message: String,
    /// Experience for the account level (see `snook_legend_proto::xp`).
    pub xp: i64,
    /// Times the player changed their username (see
    /// `snook_legend_proto::MAX_NAME_CHANGES`).
    pub name_changes: u32,
    /// The profile picture (a code from `snook_legend_proto::AVATARS`).
    pub avatar: String,
    /// The age group the player gave: under 18 (`Some(true)`), 18 or older
    /// (`Some(false)`), or not given yet (the game asks it when the player
    /// first writes to others; games of 0.7.3 asked it when signing up).
    pub minor: Option<bool>,
    /// When a parent or guardian turned the chat on for an account under 18
    /// (Unix seconds); see [`User::chat_restricted`].
    pub chat_allowed_at: Option<i64>,
}

/// An account's record under one of the rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub rating: i32,
    /// The highest rating they reached.
    pub peak_rating: i32,
    /// Rated games.
    pub games: u32,
    /// Rated wins.
    pub wins: u32,
}

impl Record {
    /// No rated game yet, at `rating`.
    pub fn new(rating: i32) -> Record {
        Record { rating, peak_rating: rating, games: 0, wins: 0 }
    }

    pub fn standing(&self) -> Standing {
        Standing { rating: self.rating, games: self.games, wins: self.wins }
    }
}

impl User {
    /// The chat with other players is off for them: it is off for everyone
    /// until they say they are 18 or older, or, under 18, until a parent or
    /// guardian turns it on (the game asks when they first write). Nothing
    /// written by other players reaches them, and they write nothing to
    /// others (the lounge's and the games' chat, the notes on the board,
    /// status messages, gift messages). Predefined reactions, invitations
    /// and games are not affected. Lei 15.211/2025 (ECA Digital), art. 20:
    /// games likely played by children and adolescents limit what players
    /// write to each other by default.
    pub fn chat_restricted(&self) -> bool {
        self.minor != Some(false) && self.chat_allowed_at.is_none()
    }

    /// Their rating under `rules`, for the Elo.
    pub fn rating(&self, rules: Rules) -> snook_legend_core::rating::Rating {
        let r = self.records.get(rules);
        snook_legend_core::rating::Rating { rating: r.rating, games: r.games }
    }

    /// Their record under each of the rules, as players see it.
    pub fn standings(&self) -> PerRules<Standing> {
        self.records.map(|_, r| r.standing())
    }

    /// Rated games under either of the rules.
    pub fn rated_games(&self) -> u32 {
        self.records.iter().map(|(_, r)| r.games).sum()
    }

    pub fn is_banned(&self, now: i64) -> bool {
        self.banned_until.is_some_and(|t| t > now)
    }

    pub fn is_muted(&self, now: i64) -> bool {
        self.muted_until.is_some_and(|t| t > now)
    }

    /// Lifetime numbers (`friends` must be filled in by the caller): the
    /// rated games and wins under both rules, the highest rating under
    /// either.
    pub fn stats(&self, now: i64) -> Stats {
        let (b, o) = (self.records.brazilian, self.records.official);
        Stats {
            games: b.games + o.games,
            wins: b.wins + o.wins,
            eight_wins: self.eight_wins,
            casual_games: self.casual_games,
            peak_rating: b.peak_rating.max(o.peak_rating),
            streak: self.streak,
            best_streak: self.best_streak,
            runouts: self.runouts,
            shutouts: self.shutouts,
            friends: 0,
            tournaments_played: self.tournaments_played,
            tournaments_won: self.tournaments_won,
            account_days: ((now - self.created_at).max(0) / 86_400) as u32,
            today: 0,
        }
    }
}

/// A note on the lounge's board.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardNote {
    pub id: i64,
    pub text: String,
    /// Who pinned it (empty if the account is gone).
    pub author: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    /// `notice`, `event` or `rule`.
    pub kind: String,
    pub pinned: bool,
    /// `tournaments`, `shop`, `vip` or `missions`.
    pub action: Option<String>,
}

/// A note to pin on the board.
pub struct NewNote<'a> {
    pub text: &'a str,
    pub author_id: i64,
    pub at: i64,
    pub expires_at: Option<i64>,
    pub kind: &'a str,
    pub pinned: bool,
    pub action: Option<&'a str>,
}

pub struct NewUser<'a> {
    pub username: &'a str,
    pub password_hash: &'a str,
    pub country: &'a str,
    /// The starting rating, under both rules.
    pub rating: i32,
    pub role: Role,
    pub ip: &'a str,
    /// The age group given at sign-up (`None`: an older game that doesn't ask).
    pub minor: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Game = 0,
    Admin = 1,
}

/// A finished match, as stored.
#[derive(Debug, Clone)]
pub struct MatchRecord {
    pub players: [i64; 2],
    pub winner: usize,
    pub reason: snook_legend_core::GameOverReason,
    pub rated: bool,
    /// The rules it was played under (the ratings are those under them).
    pub rules: Rules,
    pub delta: [i32; 2],
    pub ratings_after: [i32; 2],
    pub shots: u32,
    pub tournament: Option<i64>,
    /// The winner legally pocketed the 8.
    pub eight_win: bool,
    /// The winner ran the table from the break.
    pub runout: bool,
    /// The loser never pocketed a ball of their group.
    pub shutout: bool,
    /// The winner made only the 8 on a break (the house rules).
    pub break_win: bool,
    pub ended_at: i64,
    /// The whole game (seed, breaker, strokes).
    pub replay: Option<Replay>,
    /// Each seat's numbers this game.
    pub stats: [ShotStats; 2],
    /// The seat that left the game unfinished (lost on the reconnection clock).
    pub abandoned: Option<usize>,
    /// The live game it was (its room's id, also its saved copy's): stored
    /// once whatever happens (an answer lost on the way, the write tried
    /// again), and its saved copy goes in the same step. None for a
    /// correspondence game.
    pub room: Option<i64>,
    /// Whether the wins, the streak and the feats count it: not a game two
    /// friends farm, nor one resigned or left before it was played (the
    /// rating already follows its own rules for those).
    pub counts: bool,
}

/// A game in progress, saved after every stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveMatch {
    pub id: i64,
    pub players: [i64; 2],
    pub rated: bool,
    pub replay: Replay,
    pub updated_at: i64,
}

/// A stored game with its replay.
#[derive(Debug, Clone)]
pub struct StoredReplay {
    pub id: i64,
    pub players: [String; 2],
    pub countries: [String; 2],
    pub ratings: [i32; 2],
    pub winner: usize,
    pub rated: bool,
    pub at: i64,
    /// The whole game, and the rules it was played under.
    pub replay: Replay,
}

/// A player's potting in the hard bands, for the aim review queue.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AimRow {
    pub user_id: i64,
    pub username: String,
    pub rating: i32,
    pub attempts: u32,
    pub made: u32,
}

/// Numbers for the staff panel's metrics page.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Metrics {
    /// (day, active players, matches, new accounts), oldest first.
    pub days: Vec<(i64, i64, i64, i64)>,
    /// Accounts that came back 1 / 7 / 30 days after the day they were
    /// created (percent), over the accounts of the last 30 days for which
    /// that day has come.
    pub retention_d1: f64,
    pub retention_d7: f64,
    pub retention_d30: f64,
    /// The same, week by week of sign-ups (the last [`COHORT_WEEKS`]),
    /// newest first.
    pub cohorts: Vec<Cohort>,
    /// Accounts created in the last 30 days, and how many played 1 and 5 games.
    pub funnel: [i64; 3],
    /// Payments in the last 30 days by currency: (currency, cents, count).
    pub revenue: Vec<(String, i64, i64)>,
    /// The last 48 hours, oldest first (the current hour last).
    pub hours: Vec<HourRow>,
    /// Payments of the last 30 days by day and currency: (day, currency,
    /// cents, count), oldest first.
    pub revenue_days: Vec<(i64, String, i64, i64)>,
}

/// A player's day at the daily challenge.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DailyRow {
    pub attempts: u32,
    /// When the attempt under way started (0: none).
    pub started_at: i64,
    pub best: Option<u32>,
    pub best_at: i64,
    /// How long the best attempt took (seconds): the quicker leads a tie.
    pub best_secs: u32,
    pub cleared: bool,
}

/// A line of a day's challenge table.
#[derive(Debug, Clone, PartialEq)]
pub struct DailyBoardRow {
    pub user_id: i64,
    pub username: String,
    pub country: String,
    pub balls: u32,
    pub at: i64,
    /// How long the attempt took (seconds).
    pub secs: u32,
}

/// A line of a drill's table, or of the challenge rating's.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardRow {
    pub username: String,
    pub country: String,
    pub score: u32,
    /// The stars (the challenge rating's table only).
    pub stars: u32,
}

/// What became of an attempt sent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DailyFinish {
    /// No attempt was under way, or it started too long ago.
    NotStarted,
    /// Counted: whether it beat their best, and whether the table was
    /// cleared for the first time that day.
    Counted { improved: bool, first_clear: bool },
}

/// How many weeks of sign-ups the metrics follow.
pub const COHORT_WEEKS: i64 = 8;

/// The accounts created in one week and how many came back.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Cohort {
    /// The week's first day (Unix days).
    pub first_day: i64,
    pub accounts: i64,
    /// Percent back 1, 7 and 30 days after their first day (None: no
    /// account of the week has reached that day yet).
    pub d1: Option<f64>,
    pub d7: Option<f64>,
    pub d30: Option<f64>,
}

/// Percent of `came` in `eligible` (None without any).
pub fn percent(came: i64, eligible: i64) -> Option<f64> {
    (eligible > 0).then(|| came as f64 * 100.0 / eligible as f64)
}

/// One hour of activity (`hour` is Unix seconds / 3600).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct HourRow {
    pub hour: i64,
    /// Signed-in players seen online during the hour.
    pub active: i64,
    /// The most players (guests too) and games online at once.
    pub peak_players: i64,
    pub peak_games: i64,
    /// Players online on average over the samples (0 without samples).
    pub avg_players: f64,
    /// Games that ended in the hour.
    pub games: i64,
}

/// Seconds in an hour bucket.
pub fn hour_of(t: i64) -> i64 {
    t.div_euclid(3600)
}

#[derive(Debug, Clone)]
pub struct HistoryRow {
    pub opponent: String,
    pub opponent_country: String,
    pub opponent_rating: i32,
    pub opponent_admin: bool,
    pub won: bool,
    pub reason: String,
    pub rated: bool,
    pub rules: Rules,
    pub delta: i32,
    pub rating_after: i32,
    pub tournament: bool,
    pub at: i64,
    pub match_id: i64,
    pub replay: bool,
    /// A correspondence game.
    pub correspondence: bool,
}

/// A correspondence game, as stored: its seed and strokes (the table is
/// rebuilt by replaying them) and what the list shows without replaying.
#[derive(Debug, Clone, PartialEq)]
pub struct AsyncGame {
    pub id: i64,
    pub players: [i64; 2],
    pub rated: bool,
    pub replay: Replay,
    /// The seat to play.
    pub turn: usize,
    /// When that turn runs out (Unix seconds).
    pub deadline: i64,
    /// Strokes each seat has watched.
    pub seen: [u32; 2],
    pub groups: [Option<snook_legend_core::Group>; 2],
    pub left: [u8; 2],
    pub started_at: i64,
    pub updated_at: i64,
    /// Set once the game is over (with the winner and how).
    pub finished_at: Option<i64>,
    pub winner: Option<usize>,
    pub reason: Option<String>,
    /// The loser ran out of time.
    pub timeout: bool,
    /// Each seat's rating change and rating after the game (once over).
    pub delta: [i32; 2],
    pub ratings: [i32; 2],
    /// The match it was recorded as.
    pub match_id: Option<i64>,
}

impl AsyncGame {
    /// The seat of `user_id`, if they play this game.
    pub fn seat_of(&self, user_id: i64) -> Option<usize> {
        self.players.iter().position(|p| *p == user_id)
    }

    pub fn is_active(&self) -> bool {
        self.finished_at.is_none()
    }
}

/// A challenge to a correspondence game.
#[derive(Debug, Clone, PartialEq)]
pub struct AsyncChallenge {
    pub id: i64,
    pub from: i64,
    pub to: i64,
    pub rated: bool,
    pub rules: Rules,
    pub created_at: i64,
}

/// The game that starts when a challenge is accepted (the challenger is
/// seat 0).
#[derive(Debug, Clone, Copy)]
pub struct NewAsyncGame {
    pub seed: u64,
    pub breaker: usize,
    /// The breaker's deadline.
    pub deadline: i64,
    pub at: i64,
}

/// Someone a player played recently: their account and the latest game
/// against them.
#[derive(Debug, Clone)]
pub struct RecentRow {
    pub user: User,
    /// When the latest game ended.
    pub at: i64,
    /// The player (not the opponent) won it.
    pub won: bool,
    pub rated: bool,
    /// Games between the two among the player's latest [`RECENT_SCAN`].
    pub games: u32,
}

/// A present from one player to another, as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct GiftRow {
    pub id: i64,
    /// The giver (None if the account is gone) and their name and country
    /// now (empty if gone).
    pub from_id: Option<i64>,
    pub from: String,
    pub from_country: String,
    pub to_id: i64,
    pub to: String,
    pub to_country: String,
    /// An item code, or [`snook_legend_proto::shop::VIP_GIFT`].
    pub code: String,
    pub message: String,
    pub at: i64,
    pub seen: bool,
    pub thanked: bool,
}

/// A present to record.
pub struct NewGift<'a> {
    pub from: i64,
    pub to: i64,
    pub code: &'a str,
    pub message: &'a str,
    pub at: i64,
}

/// How many of a player's latest games are looked through for their recent
/// opponents.
pub const RECENT_SCAN: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FriendState {
    Friends,
    /// The user asked the other.
    Outgoing,
    /// The other asked the user.
    Incoming,
}

/// What became of a username change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rename {
    Done,
    /// Someone else has the name (in any letter case).
    Taken,
    /// The player used up their changes (or the account is gone).
    NoChangesLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FriendOutcome {
    Sent,
    /// The other had already asked: now they are friends.
    Accepted,
    AlreadyFriends,
    AlreadySent,
}

#[derive(Debug, Clone)]
pub struct NewReport {
    pub reporter: i64,
    pub target: i64,
    pub reason: String,
    pub details: String,
    pub context: String,
    pub at: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReportRow {
    pub id: i64,
    pub reporter: String,
    pub target: String,
    pub reason: String,
    pub details: String,
    pub context: String,
    /// 0 open, 1 action taken, 2 dismissed.
    pub status: i16,
    pub created_at: i64,
    pub resolved_by: Option<String>,
    pub resolution: Option<String>,
    pub reporter_id: i64,
    pub target_id: i64,
    /// The finished game the report is about.
    pub match_id: Option<i64>,
    /// The game that was being played when the report was sent (a room id).
    pub live_id: Option<i64>,
}

/// A player's coins, VIP time, cosmetics and payment customer.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Wallet {
    pub coins: i64,
    /// VIP until this time (Unix seconds; 0 = never).
    pub vip_until: i64,
    pub owned: Vec<String>,
    /// Category code -> equipped item code.
    pub equipped: BTreeMap<String, String>,
    pub stripe_customer: Option<String>,
    /// Cue code -> online games won with it (its level).
    pub cue_wins: BTreeMap<String, u32>,
    /// Cue code -> (online games played with it, object balls made with it).
    pub cue_games: BTreeMap<String, (u32, u32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BuyOutcome {
    Bought,
    AlreadyOwned,
    NotEnoughCoins,
}

/// A payment received (coins or a VIP month).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Payment {
    /// The provider's id (payment intent or invoice).
    pub id: String,
    pub user_id: i64,
    /// "coins" or "vip".
    pub kind: String,
    pub coins: i64,
    /// In the currency's smallest unit (cents).
    pub amount: i64,
    pub currency: String,
    pub at: i64,
    pub refunded: bool,
    /// The e-mail the customer gave at checkout.
    pub email: Option<String>,
    /// Made with the provider's test keys: no real money.
    pub test: bool,
}

/// An item a player owns, as stored (see `crate::collection`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OwnedRow {
    pub code: String,
    /// Unix seconds (0: not known).
    pub acquired_at: i64,
    /// How it was added: `coins`, `gift:<giver's id>`, `welcome pack`,
    /// `staff`, `origin` (by [`Store::record_origin`])…
    pub source: String,
    /// The origin recorded for it (JSON of `collection::Origin`), if any.
    pub origin: Option<String>,
    /// The giver's name, for a present.
    pub giver: Option<String>,
}

/// A Microsoft Store purchase being fulfilled (see `crate::msstore`).
#[derive(Debug, Clone, PartialEq)]
pub struct MsConsume {
    /// Our GUID for the consume request.
    pub tracking_id: String,
    pub user_id: i64,
    /// The add-on's Store ID.
    pub product_id: String,
    /// What it stands for (a coin pack, or `vip_month`), and its coins.
    pub pack: String,
    pub coins: i64,
    /// The purchase, as the Store's collection names it.
    pub transaction_id: String,
    /// The player's Microsoft Store ID key, to send the request again.
    pub store_key: String,
    pub attempts: i32,
    pub created_at: i64,
}

/// How a Store purchase was settled without coins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsState {
    /// Microsoft refused the request: nothing was consumed.
    Failed,
    /// No answer for days: for the staff to look at.
    Stuck,
}

impl MsState {
    pub fn code(self) -> &'static str {
        match self {
            MsState::Failed => "failed",
            MsState::Stuck => "stuck",
        }
    }
}

/// A Store purchase as the staff see it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MsRecord {
    pub tracking_id: String,
    pub pack: String,
    pub coins: i64,
    pub transaction_id: String,
    pub state: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub order_ids: Option<String>,
    pub created_at: i64,
    pub settled_at: Option<i64>,
}

/// How many finals of the tour dealt to one account are kept (the oldest go
/// first): a final is played soon after it is dealt.
pub const LADDER_TICKETS_KEPT: usize = 16;

/// A new Store purchase is sent at once by whoever opened it; the
/// background task only sends it again after this many seconds.
pub const MS_FIRST_RETRY: i64 = 60;
/// How long a server keeps a Store purchase to itself while it sends the
/// request (longer than the request can take).
pub const MS_LEASE: i64 = 120;

/// The payment id of a Store purchase (the ledger uses the same reference).
pub fn ms_payment_id(tracking_id: &str) -> String {
    format!("ms:{tracking_id}")
}

/// What a Store purchase records in `payments.currency`: Microsoft keeps the
/// price (and pays out through Partner Center), so the amount is 0.
pub const MS_CURRENCY: &str = "msstore";

/// How a fulfilled Store purchase of `pack` is recorded, and what it gives
/// besides its coins.
#[derive(Debug, Clone, PartialEq)]
pub struct MsGrant {
    /// The payment's kind ("coins" or "vip").
    pub kind: &'static str,
    /// The coin ledger's reason.
    pub reason: String,
    /// VIP added after the time left (0: none).
    pub vip_secs: i64,
}

pub fn ms_grant(pack: &str) -> MsGrant {
    use snook_legend_proto::shop::{VIP_MONTH, VIP_MONTH_SECS};
    if pack == VIP_MONTH {
        MsGrant {
            kind: "vip",
            reason: "vip month (Microsoft Store)".into(),
            vip_secs: VIP_MONTH_SECS,
        }
    } else {
        MsGrant { kind: "coins", reason: format!("pack {pack} (Microsoft Store)"), vip_secs: 0 }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LedgerRow {
    pub delta: i64,
    pub reason: String,
    pub reference: Option<String>,
    pub at: i64,
}

/// Which accounts the staff's list of players shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserFilter {
    /// Every account but the deleted ones.
    #[default]
    All,
    /// Moderators and administrators.
    Staff,
    /// Banned now (suspended or for good).
    Banned,
    /// Muted now.
    Muted,
    /// Deleted (anonymised) accounts.
    Deleted,
}

impl UserFilter {
    pub fn from_code(code: &str) -> UserFilter {
        match code {
            "staff" => UserFilter::Staff,
            "banned" => UserFilter::Banned,
            "muted" => UserFilter::Muted,
            "deleted" => UserFilter::Deleted,
            _ => UserFilter::All,
        }
    }
}

/// The order of the staff's list of players.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserSort {
    /// Newest accounts first.
    #[default]
    Newest,
    Name,
    /// Highest rating under these rules first.
    Rating(Rules),
    /// Seen most recently first.
    LastSeen,
}

impl UserSort {
    pub fn from_code(code: &str) -> UserSort {
        match code {
            "name" => UserSort::Name,
            "last_seen" => UserSort::LastSeen,
            _ => match code.strip_prefix("rating-").and_then(Rules::parse) {
                Some(rules) => UserSort::Rating(rules),
                None => UserSort::Newest,
            },
        }
    }
}

/// A search of the staff's list of players: a name (or a recovery e-mail,
/// whole) and a filter, in an order.
#[derive(Debug, Clone, Default)]
pub struct UserSearch {
    pub q: String,
    pub filter: UserFilter,
    pub sort: UserSort,
    /// Now (bans and mutes in force).
    pub now: i64,
}

impl UserSearch {
    /// Whether `u` is one of the accounts searched for (the memory store's
    /// version of the database's query).
    pub fn matches(&self, u: &User) -> bool {
        let q = self.q.trim().to_lowercase();
        let named = q.is_empty()
            || u.username.to_lowercase().contains(&q)
            || u.recovery_email.as_deref().is_some_and(|e| e.to_lowercase() == q);
        named
            && match self.filter {
                UserFilter::All => u.deleted_at.is_none(),
                UserFilter::Staff => u.role.is_staff() && u.deleted_at.is_none(),
                UserFilter::Banned => u.banned_until.is_some_and(|t| t > self.now),
                UserFilter::Muted => u.muted_until.is_some_and(|t| t > self.now),
                UserFilter::Deleted => u.deleted_at.is_some(),
            }
    }
}

/// A search of the audit log: words in the staff member's name, the target
/// or the details; an action; a target (whole name, any case); entries
/// before an id (the next page).
#[derive(Debug, Clone, Default)]
pub struct AuditSearch {
    pub q: String,
    pub action: Option<String>,
    pub target: Option<String>,
    pub before: Option<i64>,
}

impl AuditSearch {
    pub fn matches(&self, a: &AuditRow) -> bool {
        let q = self.q.trim().to_lowercase();
        (q.is_empty()
            || a.staff.to_lowercase().contains(&q)
            || a.target.to_lowercase().contains(&q)
            || a.details.to_lowercase().contains(&q))
            && self.action.as_deref().is_none_or(|x| a.action == x)
            && self.target.as_deref().is_none_or(|x| a.target.eq_ignore_ascii_case(x))
            && self.before.is_none_or(|b| a.id < b)
    }
}

/// What the staff keep about a player: a note among themselves, or a
/// warning the player is shown (at once if online, else when they next
/// sign in).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct StaffNote {
    pub id: i64,
    pub user_id: i64,
    pub author: String,
    /// "note" or "warning".
    pub kind: String,
    pub text: String,
    pub created_at: i64,
    /// A warning shown to the player at this time (none yet: waiting).
    pub delivered_at: Option<i64>,
}

/// An event the staff scheduled (see `crate::events`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GameEvent {
    pub id: i64,
    /// "xp", "coins" or "tournament".
    pub kind: String,
    pub title: String,
    pub multiplier: u32,
    pub starts_at: i64,
    pub ends_at: i64,
    /// A tournament's: "knockout" or "arena" (empty for a boost).
    pub format: String,
    /// A knockout's seats (0 for an arena or a boost).
    pub size: u32,
    pub rated: bool,
    /// The rules a tournament is played under.
    pub rules: Rules,
    pub entry_fee: u32,
    /// Coins the house adds to a tournament's prizes.
    pub prize: u32,
    pub created_by: String,
    pub created_at: i64,
    pub cancelled_at: Option<i64>,
}

/// An event to schedule.
#[derive(Debug, Clone)]
pub struct NewEvent<'a> {
    pub kind: &'a str,
    pub title: &'a str,
    pub multiplier: u32,
    pub starts_at: i64,
    pub ends_at: i64,
    pub format: &'a str,
    pub size: u32,
    pub rated: bool,
    pub rules: Rules,
    pub entry_fee: u32,
    pub prize: u32,
    pub created_by: i64,
    pub at: i64,
}

/// Longest staff note or warning.
pub const MAX_STAFF_NOTE: usize = 600;

/// Coins moved for one kind of reason in a period (the reason's first
/// word: "buy", "daily", "prize", "staff"…).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CoinFlow {
    pub kind: String,
    /// Coins given.
    pub credited: i64,
    /// Coins taken (a positive number).
    pub debited: i64,
    pub entries: i64,
}

/// The economy at a glance, for the staff.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Economy {
    /// Coins in every wallet.
    pub coins_total: i64,
    /// Wallets with coins in them.
    pub wallets: i64,
    /// VIP members now.
    pub vip_now: i64,
    /// Coins moved in the last 30 days, by kind, the largest first.
    pub flows: Vec<CoinFlow>,
    /// The largest balances: (username, coins).
    pub richest: Vec<(String, i64)>,
}

/// The kind of a ledger entry's reason: its first word ("buy cue_x" is
/// "buy", "staff: goodwill" is "staff").
pub fn coin_kind(reason: &str) -> String {
    let word = reason.split([':', ' ']).next().unwrap_or("").trim();
    if word.is_empty() { "other".into() } else { word.to_lowercase() }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditRow {
    pub id: i64,
    pub staff: String,
    pub action: String,
    pub target: String,
    pub details: String,
    pub at: i64,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Overview {
    pub users: i64,
    pub new_users_7d: i64,
    pub active_users_7d: i64,
    pub matches_total: i64,
    pub matches_24h: i64,
    /// The games of the last 24 hours under each of the rules.
    pub matches_24h_by_rules: PerRules<i64>,
    pub open_reports: i64,
    pub banned: i64,
}

#[derive(Debug, Clone)]
pub struct TournamentRecord {
    pub name: String,
    /// The player who opened it (none for the server's own: the weekly cup
    /// and the staff's events, or a guest).
    pub host: Option<i64>,
    pub size: u32,
    pub rated: bool,
    pub players: Vec<i64>,
    pub winner: Option<i64>,
    pub finished_at: i64,
    pub entry_fee: u32,
    /// Who got which prize ("name:coins, …").
    pub prizes: String,
    /// The lounge's id of the tournament: recorded once, however often the
    /// write is tried (None in tests).
    pub lounge_id: Option<i64>,
}

/// The storage back end (one per server, so the size difference doesn't matter).
#[allow(clippy::large_enum_variant)]
pub enum Store {
    Memory(std::sync::Arc<MemoryStore>),
    #[cfg(feature = "postgres")]
    Postgres(PgStore),
}

macro_rules! dispatch {
    ($self:ident, $s:ident => $e:expr) => {
        match $self {
            Store::Memory($s) => $e,
            #[cfg(feature = "postgres")]
            Store::Postgres($s) => $e.await,
        }
    };
}

impl Store {
    pub fn memory() -> Store {
        Store::Memory(Default::default())
    }

    /// Another handle on the same in-memory store (tests: two servers, one
    /// database). `None` for a database (connect again instead).
    pub fn share(&self) -> Option<Store> {
        match self {
            Store::Memory(m) => Some(Store::Memory(m.clone())),
            #[cfg(feature = "postgres")]
            Store::Postgres(_) => None,
        }
    }

    #[cfg(feature = "postgres")]
    pub async fn postgres(url: &str, max_connections: u32) -> StoreResult<Store> {
        Ok(Store::Postgres(PgStore::connect(url, max_connections).await?))
    }

    pub fn is_persistent(&self) -> bool {
        !matches!(self, Store::Memory(_))
    }

    pub async fn create_user(&self, new: &NewUser<'_>) -> StoreResult<Option<User>> {
        dispatch!(self, s => s.create_user(new))
    }
    pub async fn user_by_name(&self, name: &str) -> StoreResult<Option<User>> {
        dispatch!(self, s => s.user_by_name(name))
    }
    pub async fn user_by_id(&self, id: i64) -> StoreResult<Option<User>> {
        dispatch!(self, s => s.user_by_id(id))
    }
    pub async fn create_session(
        &self,
        token_hash: &str,
        user_id: i64,
        kind: SessionKind,
        created_at: i64,
        expires_at: i64,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.create_session(token_hash, user_id, kind, created_at, expires_at))
    }
    /// The user of a valid session, with the session's creation time.
    pub async fn session_user(
        &self,
        token_hash: &str,
        kind: SessionKind,
        now: i64,
    ) -> StoreResult<Option<(User, i64)>> {
        dispatch!(self, s => s.session_user(token_hash, kind, now))
    }
    pub async fn delete_session(&self, token_hash: &str) -> StoreResult<()> {
        dispatch!(self, s => s.delete_session(token_hash))
    }
    pub async fn delete_user_sessions(&self, user_id: i64) -> StoreResult<()> {
        dispatch!(self, s => s.delete_user_sessions(user_id))
    }
    pub async fn purge_sessions(&self, now: i64) -> StoreResult<u64> {
        dispatch!(self, s => s.purge_sessions(now))
    }
    pub async fn set_country(&self, user_id: i64, country: &str) -> StoreResult<()> {
        dispatch!(self, s => s.set_country(user_id, country))
    }
    pub async fn touch(&self, user_id: i64, now: i64) -> StoreResult<()> {
        dispatch!(self, s => s.touch(user_id, now))
    }
    /// Sets (or clears, when empty) a player's profile picture.
    pub async fn set_avatar(&self, user_id: i64, avatar: &str) -> StoreResult<()> {
        dispatch!(self, s => s.set_avatar(user_id, avatar))
    }
    /// Records the age group of an account that has none yet (it never
    /// changes afterwards from the game); whether it was recorded.
    pub async fn set_minor(&self, user_id: i64, minor: bool) -> StoreResult<bool> {
        dispatch!(self, s => s.set_minor(user_id, minor))
    }
    /// A parent or guardian turned the chat of an account under 18 on (at
    /// this time) or off (`None`).
    pub async fn set_chat_allowed(&self, user_id: i64, at: Option<i64>) -> StoreResult<()> {
        dispatch!(self, s => s.set_chat_allowed(user_id, at))
    }
    /// Sets (or clears, when empty) a player's status message.
    pub async fn set_status_message(&self, user_id: i64, text: &str) -> StoreResult<()> {
        dispatch!(self, s => s.set_status_message(user_id, text))
    }
    /// The last `limit` different people a player played, latest first
    /// (deleted accounts left out), from their latest [`RECENT_SCAN`] games.
    pub async fn recent_opponents(&self, user_id: i64, limit: u32) -> StoreResult<Vec<RecentRow>> {
        dispatch!(self, s => s.recent_opponents(user_id, limit))
    }
    /// Stores a match and updates both players' numbers; returns them
    /// updated and the match id, or None when that game was stored already
    /// (see [`MatchRecord::room`]).
    pub async fn record_match(
        &self,
        m: &MatchRecord,
    ) -> StoreResult<Option<([Option<User>; 2], i64)>> {
        dispatch!(self, s => s.record_match(m))
    }
    /// Lifetime numbers from the games with stored statistics.
    pub async fn career(&self, user_id: i64) -> StoreResult<CareerNumbers> {
        dispatch!(self, s => s.career(user_id))
    }
    /// (time, rating after) of the last `limit` rated games under `rules`,
    /// oldest first.
    pub async fn rating_history(
        &self,
        user_id: i64,
        rules: Rules,
        limit: u32,
    ) -> StoreResult<Vec<(i64, i32)>> {
        dispatch!(self, s => s.rating_history(user_id, rules, limit))
    }
    pub async fn replay(&self, match_id: i64) -> StoreResult<Option<StoredReplay>> {
        dispatch!(self, s => s.replay(match_id))
    }
    /// Saves (or replaces) a game in progress.
    pub async fn save_live(&self, m: &LiveMatch) -> StoreResult<()> {
        dispatch!(self, s => s.save_live(m))
    }
    pub async fn delete_live(&self, id: i64) -> StoreResult<()> {
        dispatch!(self, s => s.delete_live(id))
    }
    /// The saved game in progress of this player, if any.
    pub async fn live_match_of(&self, user_id: i64) -> StoreResult<Option<LiveMatch>> {
        dispatch!(self, s => s.live_match_of(user_id))
    }
    /// Forgets saved games nobody came back to.
    pub async fn purge_live(&self, before: i64) -> StoreResult<u64> {
        dispatch!(self, s => s.purge_live(before))
    }
    /// Takes `amount` coins if the player has them; false otherwise.
    pub async fn spend(
        &self,
        user_id: i64,
        amount: i64,
        reason: &str,
        reference: Option<&str>,
        at: i64,
    ) -> StoreResult<bool> {
        dispatch!(self, s => s.spend(user_id, amount, reason, reference, at))
    }
    /// Adds attempts and pots per difficulty band.
    pub async fn add_shot_stats(
        &self,
        user_id: i64,
        bands: &[(usize, u32, u32)],
    ) -> StoreResult<()> {
        dispatch!(self, s => s.add_shot_stats(user_id, bands))
    }
    /// Potting in the given bands, for every player with at least `min` attempts.
    pub async fn aim_rows(&self, bands: &[usize], min: u32) -> StoreResult<Vec<AimRow>> {
        dispatch!(self, s => s.aim_rows(bands, min))
    }
    /// Marks the player active on `day` (Unix day number).
    pub async fn mark_active(&self, user_id: i64, day: i64) -> StoreResult<()> {
        dispatch!(self, s => s.mark_active(user_id, day))
    }
    pub async fn metrics(&self, now: i64) -> StoreResult<Metrics> {
        dispatch!(self, s => s.metrics(now))
    }

    // Accounts.

    pub async fn set_recovery_email(&self, user_id: i64, email: Option<&str>) -> StoreResult<()> {
        dispatch!(self, s => s.set_recovery_email(user_id, email))
    }
    /// Gives the player the username `name`, unless someone else has it or
    /// they already changed theirs `limit` times.
    pub async fn change_username(
        &self,
        user_id: i64,
        name: &str,
        limit: u32,
    ) -> StoreResult<Rename> {
        dispatch!(self, s => s.change_username(user_id, name, limit))
    }
    /// The e-mail given at this player's latest payment.
    pub async fn payment_email(&self, user_id: i64) -> StoreResult<Option<String>> {
        dispatch!(self, s => s.payment_email(user_id))
    }
    /// Deletes an account by anonymising it: the name becomes `deleted-<id>`,
    /// the password, e-mail, country, friends, blocks and sessions go; the
    /// games stay (the opponents keep their history) and so do payment
    /// records (accounting), without the e-mail.
    pub async fn delete_account(&self, user_id: i64, now: i64) -> StoreResult<()> {
        dispatch!(self, s => s.delete_account(user_id, now))
    }
    /// Unlocks achievements; returns the codes that were new.
    pub async fn add_achievements(
        &self,
        user_id: i64,
        codes: &[&str],
        at: i64,
    ) -> StoreResult<Vec<String>> {
        dispatch!(self, s => s.add_achievements(user_id, codes, at))
    }
    pub async fn achievements(&self, user_id: i64) -> StoreResult<Vec<(String, i64)>> {
        dispatch!(self, s => s.achievements(user_id))
    }
    pub async fn history(&self, user_id: i64, limit: u32) -> StoreResult<Vec<HistoryRow>> {
        dispatch!(self, s => s.history(user_id, limit))
    }
    /// The world ranking under `rules`: the players with rated games under
    /// them, highest rating first.
    pub async fn leaderboard(
        &self,
        rules: Rules,
        limit: u32,
        now: i64,
    ) -> StoreResult<Vec<LeaderboardEntry>> {
        dispatch!(self, s => s.leaderboard(rules, limit, now))
    }
    /// A player's place in the world ranking under `rules` (None: no rated
    /// game under them).
    pub async fn rank_of(&self, user_id: i64, rules: Rules) -> StoreResult<Option<u32>> {
        dispatch!(self, s => s.rank_of(user_id, rules))
    }
    pub async fn friends(&self, user_id: i64) -> StoreResult<Vec<(User, FriendState)>> {
        dispatch!(self, s => s.friends(user_id))
    }
    pub async fn friend_request(&self, from: i64, to: i64, at: i64) -> StoreResult<FriendOutcome> {
        dispatch!(self, s => s.friend_request(from, to, at))
    }
    /// `me` accepts `from`'s request; false if there was none.
    pub async fn friend_accept(&self, me: i64, from: i64) -> StoreResult<bool> {
        dispatch!(self, s => s.friend_accept(me, from))
    }
    /// Removes a friendship or a request in either direction.
    pub async fn friend_remove(&self, a: i64, b: i64) -> StoreResult<()> {
        dispatch!(self, s => s.friend_remove(a, b))
    }
    pub async fn friend_count(&self, user_id: i64) -> StoreResult<u32> {
        dispatch!(self, s => s.friend_count(user_id))
    }
    pub async fn block(&self, user_id: i64, blocked: i64) -> StoreResult<()> {
        dispatch!(self, s => s.block(user_id, blocked))
    }
    pub async fn unblock(&self, user_id: i64, blocked: i64) -> StoreResult<()> {
        dispatch!(self, s => s.unblock(user_id, blocked))
    }
    pub async fn blocked(&self, user_id: i64) -> StoreResult<Vec<User>> {
        dispatch!(self, s => s.blocked(user_id))
    }
    pub async fn create_report(&self, r: &NewReport) -> StoreResult<()> {
        self.create_report_with_game(r, None, None).await.map(|_| ())
    }
    /// A report with the game it is about (a stored match, or the room of
    /// the game being played); returns its id.
    pub async fn create_report_with_game(
        &self,
        r: &NewReport,
        match_id: Option<i64>,
        live_id: Option<i64>,
    ) -> StoreResult<i64> {
        dispatch!(self, s => s.create_report_with_game(r, match_id, live_id))
    }
    pub async fn report(&self, id: i64) -> StoreResult<Option<ReportRow>> {
        dispatch!(self, s => s.report(id))
    }
    /// Links a report to the stored match it is about.
    pub async fn set_report_match(&self, id: i64, match_id: i64) -> StoreResult<()> {
        dispatch!(self, s => s.set_report_match(id, match_id))
    }
    /// Stored games (with their replay) between two players that ended in
    /// `from..=to`, newest first: (match id, ended at).
    pub async fn matches_between(
        &self,
        a: i64,
        b: i64,
        from: i64,
        to: i64,
        limit: u32,
    ) -> StoreResult<Vec<(i64, i64)>> {
        dispatch!(self, s => s.matches_between(a, b, from, to, limit))
    }
    /// Adds a game's shots to a player's analysis.
    pub async fn add_shot_analysis(
        &self,
        user_id: i64,
        analysis: &snook_legend_proto::analysis::ShotAnalysis,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.add_shot_analysis(user_id, analysis))
    }
    pub async fn shot_analysis(
        &self,
        user_id: i64,
    ) -> StoreResult<snook_legend_proto::analysis::ShotAnalysis> {
        dispatch!(self, s => s.shot_analysis(user_id))
    }
    /// One sample of who is online: the signed-in accounts, and how many
    /// players (guests too) and games, into `hour`'s bucket.
    pub async fn sample_online(
        &self,
        hour: i64,
        users: &[i64],
        players: u32,
        games: u32,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.sample_online(hour, users, players, games))
    }
    /// Forgets who was online before `hour` (the hourly peaks stay).
    pub async fn purge_active_hours(&self, hour: i64) -> StoreResult<u64> {
        dispatch!(self, s => s.purge_active_hours(hour))
    }
    /// Records a tournament and counts it for its players; false if that
    /// tournament was recorded already.
    pub async fn record_tournament(&self, t: &TournamentRecord) -> StoreResult<bool> {
        dispatch!(self, s => s.record_tournament(t))
    }
    /// The seed of a day's challenge: `fresh` the first time it is asked
    /// for, the same one ever after.
    pub async fn daily_seed(&self, day: i64, fresh: u64) -> StoreResult<u64> {
        dispatch!(self, s => s.daily_seed(day, fresh))
    }
    /// Starts an attempt (false: `max` attempts were used already).
    pub async fn daily_start(
        &self,
        user_id: i64,
        day: i64,
        now: i64,
        max: u32,
    ) -> StoreResult<bool> {
        dispatch!(self, s => s.daily_start(user_id, day, now, max))
    }
    /// Ends the attempt under way with its result (it must have started at
    /// most `limit` seconds ago).
    #[allow(clippy::too_many_arguments)]
    pub async fn daily_finish(
        &self,
        user_id: i64,
        day: i64,
        now: i64,
        balls: Option<u32>,
        cleared: bool,
        limit: i64,
    ) -> StoreResult<DailyFinish> {
        dispatch!(self, s => s.daily_finish(user_id, day, now, balls, cleared, limit))
    }
    /// A player's day at the challenge.
    pub async fn daily_row(&self, user_id: i64, day: i64) -> StoreResult<DailyRow> {
        dispatch!(self, s => s.daily_row(user_id, day))
    }
    /// A day's leaders (best first; earlier first on a tie) and how many
    /// have a score.
    pub async fn daily_board(
        &self,
        day: i64,
        limit: u32,
    ) -> StoreResult<(Vec<DailyBoardRow>, u32)> {
        dispatch!(self, s => s.daily_board(day, limit))
    }
    /// A player's place on a day's table (None: no score).
    pub async fn daily_rank(&self, user_id: i64, day: i64) -> StoreResult<Option<u32>> {
        dispatch!(self, s => s.daily_rank(user_id, day))
    }
    /// The days before `before` a player topped, newest first: (day, players).
    pub async fn daily_wins(&self, user_id: i64, before: i64) -> StoreResult<Vec<(i64, u32)>> {
        dispatch!(self, s => s.daily_wins(user_id, before))
    }
    /// A player's best score in each drill they did: (code, best).
    pub async fn drill_bests(&self, user_id: i64) -> StoreResult<Vec<(String, u32)>> {
        dispatch!(self, s => s.drill_bests(user_id))
    }
    /// Records a drill session's score; returns the best before it (None:
    /// the first).
    pub async fn drill_record(
        &self,
        user_id: i64,
        code: &str,
        score: u32,
        at: i64,
    ) -> StoreResult<Option<u32>> {
        dispatch!(self, s => s.drill_record(user_id, code, score, at))
    }
    /// A drill's best scores, best first.
    pub async fn drill_board(&self, code: &str, limit: u32) -> StoreResult<Vec<BoardRow>> {
        dispatch!(self, s => s.drill_board(code, limit))
    }
    /// Keeps a player's challenge rating (for its table).
    pub async fn set_challenge_rating(
        &self,
        user_id: i64,
        rating: u32,
        stars: u32,
        at: i64,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.set_challenge_rating(user_id, rating, stars, at))
    }
    /// The challenge rating's leaders, and a player's place (None: no rating).
    pub async fn challenge_board(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<(Vec<BoardRow>, Option<u32>)> {
        dispatch!(self, s => s.challenge_board(user_id, limit))
    }
    /// Adds experience; returns the total now.
    pub async fn add_xp(&self, user_id: i64, amount: u32) -> StoreResult<i64> {
        dispatch!(self, s => s.add_xp(user_id, amount))
    }
    /// The tournaments a player won, newest first: (name, players, finished at).
    pub async fn tournaments_won(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<(String, u32, i64)>> {
        dispatch!(self, s => s.tournaments_won(user_id, limit))
    }

    // Staff.

    /// A page of the staff's list of players, and how many there are.
    pub async fn search_users(
        &self,
        search: &UserSearch,
        offset: u32,
        limit: u32,
    ) -> StoreResult<(Vec<User>, i64)> {
        dispatch!(self, s => s.search_users(search, offset, limit))
    }
    /// Staff give a player another username (an offensive one): free, it
    /// doesn't count among the player's own changes.
    pub async fn staff_rename(&self, user_id: i64, name: &str) -> StoreResult<Rename> {
        dispatch!(self, s => s.staff_rename(user_id, name))
    }
    pub async fn add_staff_note(
        &self,
        user_id: i64,
        author_id: i64,
        kind: &str,
        text: &str,
        at: i64,
    ) -> StoreResult<i64> {
        dispatch!(self, s => s.add_staff_note(user_id, author_id, kind, text, at))
    }
    /// The notes and warnings about a player, newest first.
    pub async fn staff_notes(&self, user_id: i64, limit: u32) -> StoreResult<Vec<StaffNote>> {
        dispatch!(self, s => s.staff_notes(user_id, limit))
    }
    /// Removes a note (only `author`'s, when given); false if there was none.
    pub async fn remove_staff_note(&self, id: i64, author: Option<i64>) -> StoreResult<bool> {
        dispatch!(self, s => s.remove_staff_note(id, author))
    }
    /// The warnings the player hasn't been shown (oldest first), marked
    /// shown at `at`.
    pub async fn take_warnings(&self, user_id: i64, at: i64) -> StoreResult<Vec<String>> {
        dispatch!(self, s => s.take_warnings(user_id, at))
    }
    pub async fn add_event(&self, e: &NewEvent<'_>) -> StoreResult<i64> {
        dispatch!(self, s => s.add_event(e))
    }
    /// The events not called off that end after `t`, soonest first.
    pub async fn events_after(&self, t: i64) -> StoreResult<Vec<GameEvent>> {
        dispatch!(self, s => s.events_after(t))
    }
    /// The latest events (called off and past ones too), the latest
    /// start first.
    pub async fn recent_events(&self, limit: u32) -> StoreResult<Vec<GameEvent>> {
        dispatch!(self, s => s.recent_events(limit))
    }
    /// Calls an event off; false if there is none, or it was called off.
    pub async fn cancel_event(&self, id: i64, at: i64) -> StoreResult<bool> {
        dispatch!(self, s => s.cancel_event(id, at))
    }
    /// Coins, VIP and where coins went in the last 30 days.
    pub async fn economy(&self, now: i64) -> StoreResult<Economy> {
        dispatch!(self, s => s.economy(now))
    }
    /// The latest payments of everyone, with the payer's name.
    pub async fn recent_payments(&self, limit: u32) -> StoreResult<Vec<(Payment, String)>> {
        dispatch!(self, s => s.recent_payments(limit))
    }
    /// Players active since `since` by country (code, players), the most
    /// first ("" for no country).
    pub async fn active_countries(&self, since: i64) -> StoreResult<Vec<(String, i64)>> {
        dispatch!(self, s => s.active_countries(since))
    }
    pub async fn overview(&self, now: i64) -> StoreResult<Overview> {
        dispatch!(self, s => s.overview(now))
    }
    pub async fn set_ban(
        &self,
        user_id: i64,
        until: Option<i64>,
        reason: Option<&str>,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.set_ban(user_id, until, reason))
    }
    pub async fn set_mute(&self, user_id: i64, until: Option<i64>) -> StoreResult<()> {
        dispatch!(self, s => s.set_mute(user_id, until))
    }
    pub async fn set_role(&self, user_id: i64, role: Role) -> StoreResult<()> {
        dispatch!(self, s => s.set_role(user_id, role))
    }
    pub async fn set_password(&self, user_id: i64, hash: &str) -> StoreResult<()> {
        dispatch!(self, s => s.set_password(user_id, hash))
    }
    pub async fn reports(&self, status: Option<i16>, limit: u32) -> StoreResult<Vec<ReportRow>> {
        dispatch!(self, s => s.reports(status, limit))
    }
    pub async fn reports_about(&self, user_id: i64, limit: u32) -> StoreResult<Vec<ReportRow>> {
        dispatch!(self, s => s.reports_about(user_id, limit))
    }
    pub async fn resolve_report(
        &self,
        id: i64,
        staff: i64,
        status: i16,
        note: &str,
        at: i64,
    ) -> StoreResult<bool> {
        dispatch!(self, s => s.resolve_report(id, staff, status, note, at))
    }
    pub async fn audit(
        &self,
        staff: i64,
        action: &str,
        target: &str,
        details: &str,
        at: i64,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.audit(staff, action, target, details, at))
    }
    pub async fn audit_log(&self, limit: u32) -> StoreResult<Vec<AuditRow>> {
        self.audit_search(&AuditSearch::default(), limit).await
    }
    /// The audit log, newest first, as searched.
    pub async fn audit_search(
        &self,
        search: &AuditSearch,
        limit: u32,
    ) -> StoreResult<Vec<AuditRow>> {
        dispatch!(self, s => s.audit_search(search, limit))
    }

    // The shop.

    pub async fn wallet(&self, user_id: i64) -> StoreResult<Wallet> {
        dispatch!(self, s => s.wallet(user_id))
    }
    /// Takes `price` coins and gives the item, atomically.
    pub async fn buy_item(
        &self,
        user_id: i64,
        code: &str,
        price: i64,
        at: i64,
    ) -> StoreResult<BuyOutcome> {
        dispatch!(self, s => s.buy_item(user_id, code, price, at))
    }
    /// Gives an item for free (staff, promotions); false if already owned.
    pub async fn grant_item(
        &self,
        user_id: i64,
        code: &str,
        source: &str,
        at: i64,
    ) -> StoreResult<bool> {
        dispatch!(self, s => s.grant_item(user_id, code, source, at))
    }
    /// Takes items back (a refunded purchase); returns how many were owned.
    /// What is equipped of them is no longer used (see `shop::effective`).
    pub async fn revoke_items(&self, user_id: i64, codes: &[&str]) -> StoreResult<u64> {
        dispatch!(self, s => s.revoke_items(user_id, codes))
    }
    /// Every item `user_id` owns, with when and how it was got, oldest
    /// first (items that come with achievements or VIP are not owned).
    pub async fn owned_items(&self, user_id: i64) -> StoreResult<Vec<OwnedRow>> {
        dispatch!(self, s => s.owned_items(user_id))
    }
    /// Records how `user_id` got `code` (`origin`: JSON of
    /// `collection::Origin`); the first origin recorded stays. Gives the
    /// item if it wasn't owned yet: true when it was given now.
    pub async fn record_origin(
        &self,
        user_id: i64,
        code: &str,
        origin: &str,
        at: i64,
    ) -> StoreResult<bool> {
        dispatch!(self, s => s.record_origin(user_id, code, origin, at))
    }
    pub async fn equip(&self, user_id: i64, category: &str, code: &str) -> StoreResult<()> {
        dispatch!(self, s => s.equip(user_id, category, code))
    }
    /// Counts an online game won with the cue `code`; returns its wins now.
    pub async fn add_cue_win(&self, user_id: i64, code: &str) -> StoreResult<u32> {
        self.add_cue_game(user_id, code, true, 0).await
    }
    /// Counts an online game played with the cue `code` (won or not, and
    /// the balls made with it); returns its wins now.
    pub async fn add_cue_game(
        &self,
        user_id: i64,
        code: &str,
        won: bool,
        potted: u32,
    ) -> StoreResult<u32> {
        dispatch!(self, s => s.add_cue_game(user_id, code, won, potted))
    }
    /// Adds (or removes) coins. With a `reference`, the same change is only
    /// ever applied once; returns whether it was applied now.
    pub async fn credit(
        &self,
        user_id: i64,
        delta: i64,
        reason: &str,
        reference: Option<&str>,
        at: i64,
    ) -> StoreResult<bool> {
        dispatch!(self, s => s.credit(user_id, delta, reason, reference, at))
    }
    /// [`Store::credit`] with VIP time added or taken back in the same step:
    /// both happen once (keyed by `reference`), or neither.
    pub async fn credit_vip(
        &self,
        user_id: i64,
        delta: i64,
        reason: &str,
        reference: &str,
        at: i64,
        vip: VipChange,
    ) -> StoreResult<bool> {
        dispatch!(self, s => s.credit_vip(user_id, delta, reason, reference, at, vip))
    }
    /// VIP until `until` (never shortens it unless `force`).
    pub async fn set_vip(&self, user_id: i64, until: i64, force: bool) -> StoreResult<()> {
        dispatch!(self, s => s.set_vip(user_id, until, force))
    }
    /// `secs` more VIP, counted from `from` or from the end of the VIP
    /// there is (in one step: two at once add up).
    pub async fn add_vip(&self, user_id: i64, from: i64, secs: i64) -> StoreResult<()> {
        dispatch!(self, s => s.add_vip(user_id, from, secs))
    }
    /// `secs` of VIP taken back, never ending it before `floor` (nor
    /// lengthening it), in one step.
    pub async fn take_vip(&self, user_id: i64, secs: i64, floor: i64) -> StoreResult<()> {
        dispatch!(self, s => s.take_vip(user_id, secs, floor))
    }
    /// The player's Stripe customer: the one their subscription is under
    /// (`replace`), else kept when they have one already (in one step).
    pub async fn set_stripe_customer(
        &self,
        user_id: i64,
        customer: &str,
        replace: bool,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.set_stripe_customer(user_id, customer, replace))
    }
    pub async fn user_by_stripe_customer(&self, customer: &str) -> StoreResult<Option<i64>> {
        dispatch!(self, s => s.user_by_stripe_customer(customer))
    }
    /// Records a payment; false if it was already recorded.
    pub async fn record_payment(&self, p: &Payment) -> StoreResult<bool> {
        dispatch!(self, s => s.record_payment(p))
    }
    pub async fn payment(&self, id: &str) -> StoreResult<Option<Payment>> {
        dispatch!(self, s => s.payment(id))
    }
    /// Marks a payment as made in test mode (or not); false if unknown.
    pub async fn set_payment_test(&self, id: &str, test: bool) -> StoreResult<bool> {
        dispatch!(self, s => s.set_payment_test(id, test))
    }
    /// Marks a payment refunded; false if it already was (or is unknown).
    pub async fn refund_payment(&self, id: &str) -> StoreResult<bool> {
        dispatch!(self, s => s.refund_payment(id))
    }
    pub async fn ledger(&self, user_id: i64, limit: u32) -> StoreResult<Vec<LedgerRow>> {
        dispatch!(self, s => s.ledger(user_id, limit))
    }
    pub async fn payments_of(&self, user_id: i64, limit: u32) -> StoreResult<Vec<Payment>> {
        dispatch!(self, s => s.payments_of(user_id, limit))
    }
    /// Starts fulfilling a Microsoft Store purchase (see `crate::msstore`):
    /// records the consume request before it is sent. False when the same
    /// purchase is already being fulfilled or was credited.
    pub async fn ms_open(&self, c: &MsConsume) -> StoreResult<bool> {
        dispatch!(self, s => s.ms_open(c))
    }
    /// Takes a pending Store purchase to send its request now: counts the
    /// attempt and keeps it from other servers for [`MS_LEASE`] seconds.
    /// `None` when it isn't pending or someone else is sending it.
    pub async fn ms_lease(&self, tracking_id: &str, now: i64) -> StoreResult<Option<MsConsume>> {
        dispatch!(self, s => s.ms_lease(tracking_id, now))
    }
    /// The staff send a stuck Store purchase again (it keeps its key).
    /// False if it isn't stuck; an error if another row holds the purchase.
    pub async fn ms_reopen(&self, tracking_id: &str, now: i64) -> StoreResult<bool> {
        dispatch!(self, s => s.ms_reopen(tracking_id, now))
    }
    /// This player's Store purchases still waiting for Microsoft's answer.
    pub async fn ms_pending_of(&self, user_id: i64) -> StoreResult<Vec<MsConsume>> {
        dispatch!(self, s => s.ms_pending_of(user_id))
    }
    /// Store purchases waiting for an answer whose next try is due.
    pub async fn ms_due(&self, now: i64, limit: u32) -> StoreResult<Vec<MsConsume>> {
        dispatch!(self, s => s.ms_due(now, limit))
    }
    /// Microsoft consumed the purchase: in one step, the row is done, the
    /// payment recorded and the coins credited (and, for a month of VIP,
    /// the month added after the VIP time left). Microsoft's confirmation of
    /// the row's own request counts even if the row was closed meanwhile
    /// (failed or stuck); false (nothing changes) when it was done already.
    pub async fn ms_fulfil(
        &self,
        tracking_id: &str,
        order_ids: &str,
        at: i64,
    ) -> StoreResult<bool> {
        dispatch!(self, s => s.ms_fulfil(tracking_id, order_ids, at))
    }
    /// No answer yet: try again at `next_try_at`.
    pub async fn ms_retry_later(
        &self,
        tracking_id: &str,
        error: &str,
        next_try_at: i64,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.ms_retry_later(tracking_id, error, next_try_at))
    }
    /// Settles a pending row without coins: `failed` (Microsoft refused the
    /// first request, so nothing was consumed) or `stuck` (no answer for
    /// days; the key is kept to send it again).
    pub async fn ms_close(
        &self,
        tracking_id: &str,
        state: MsState,
        error: &str,
        at: i64,
    ) -> StoreResult<bool> {
        dispatch!(self, s => s.ms_close(tracking_id, state, error, at))
    }
    /// This player's Store purchases (newest first), for the staff.
    pub async fn ms_consumes_of(&self, user_id: i64, limit: u32) -> StoreResult<Vec<MsRecord>> {
        dispatch!(self, s => s.ms_consumes_of(user_id, limit))
    }
    /// This player's progress on missions: (period, code, progress) for
    /// the given periods.
    pub async fn mission_progress(
        &self,
        user_id: i64,
        periods: &[String],
    ) -> StoreResult<Vec<(String, String, u32)>> {
        dispatch!(self, s => s.mission_progress(user_id, periods))
    }
    /// Adds `amount` to a mission's progress (never past `target`); returns
    /// the new progress. `ends`: when the period ends (for clean-up).
    pub async fn advance_mission(
        &self,
        user_id: i64,
        period: &str,
        code: &str,
        amount: u32,
        target: u32,
        ends: i64,
    ) -> StoreResult<u32> {
        dispatch!(self, s => s.advance_mission(user_id, period, code, amount, target, ends))
    }
    /// Removes progress on missions that ended before `now`.
    pub async fn purge_missions(&self, now: i64) -> StoreResult<u64> {
        dispatch!(self, s => s.purge_missions(now))
    }
    /// What is kept only for a while: the days players were active before
    /// `day` (the metrics look back 90 days), and board notes expired
    /// before `now`.
    pub async fn purge_old(&self, day: i64, now: i64) -> StoreResult<u64> {
        dispatch!(self, s => s.purge_old(day, now))
    }
    /// The notes on the lounge's board that are up at `now`: pinned ones
    /// first, then newest first.
    pub async fn board_notes(&self, now: i64) -> StoreResult<Vec<BoardNote>> {
        dispatch!(self, s => s.board_notes(now))
    }
    pub async fn add_board_note(&self, note: &NewNote<'_>) -> StoreResult<i64> {
        dispatch!(self, s => s.add_board_note(note))
    }
    /// Takes a note down; false if there was none.
    pub async fn remove_board_note(&self, id: i64) -> StoreResult<bool> {
        dispatch!(self, s => s.remove_board_note(id))
    }
    /// The daily bonus: `coins` once per `day`; false if already claimed.
    pub async fn claim_daily(&self, user_id: i64, day: i64, coins: i64) -> StoreResult<bool> {
        dispatch!(self, s => s.claim_daily(user_id, day, coins))
    }
    /// Coins for playing, up to `cap` a day; returns what was given.
    pub async fn earn(
        &self,
        user_id: i64,
        day: i64,
        amount: i64,
        cap: i64,
        reason: &str,
        at: i64,
    ) -> StoreResult<i64> {
        dispatch!(self, s => s.earn(user_id, day, amount, cap, reason, at))
    }
    /// Records that `user_id` won the ladder's reward with this game; false
    /// if they had it already (once per account).
    pub async fn record_ladder_claim(
        &self,
        user_id: i64,
        rack_seed: u64,
        strokes: u32,
        at: i64,
    ) -> StoreResult<bool> {
        dispatch!(self, s => s.record_ladder_claim(user_id, rack_seed, strokes, at))
    }
    /// Keeps a final of the tour dealt to `user_id`: its rack and breaker
    /// (only the newest [`LADDER_TICKETS_KEPT`] of each account stay).
    pub async fn issue_ladder_ticket(
        &self,
        user_id: i64,
        seed: u64,
        breaker: usize,
        at: i64,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.issue_ladder_ticket(user_id, seed, breaker, at))
    }
    /// Keeps a medal of a tournament against the computer (once per
    /// tournament); true if it was new.
    pub async fn add_cup_medal(
        &self,
        user_id: i64,
        medal: &snook_legend_proto::cup::Medal,
        at: i64,
    ) -> StoreResult<bool> {
        dispatch!(self, s => s.add_cup_medal(user_id, medal, at))
    }
    /// A player's medals of the tournaments against the computer.
    pub async fn cup_medals(&self, user_id: i64) -> StoreResult<snook_legend_proto::cup::Medals> {
        dispatch!(self, s => s.cup_medals(user_id))
    }
    /// A player's medals of the tournaments against the computer, newest
    /// first, with when each was kept (for the staff).
    pub async fn cup_medal_list(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<(snook_legend_proto::cup::Medal, i64)>> {
        dispatch!(self, s => s.cup_medal_list(user_id, limit))
    }
    /// How many finals of the tour were dealt to `user_id` (of the newest
    /// [`LADDER_TICKETS_KEPT`] kept) and when the latest was.
    pub async fn ladder_finals(&self, user_id: i64) -> StoreResult<(u32, Option<i64>)> {
        dispatch!(self, s => s.ladder_finals(user_id))
    }
    /// Medals kept for a player since `since` (a day's worth is limited).
    pub async fn cup_medals_since(&self, user_id: i64, since: i64) -> StoreResult<u32> {
        dispatch!(self, s => s.cup_medals_since(user_id, since))
    }
    /// The final dealt to `user_id` with this rack: (breaker, when).
    pub async fn ladder_ticket(
        &self,
        user_id: i64,
        seed: u64,
    ) -> StoreResult<Option<(usize, i64)>> {
        dispatch!(self, s => s.ladder_ticket(user_id, seed))
    }
    /// When `user_id` won the ladder's reward, if they have.
    pub async fn ladder_claim(&self, user_id: i64) -> StoreResult<Option<i64>> {
        dispatch!(self, s => s.ladder_claim(user_id))
    }
    /// The latest recorded games of `user_id` with a replay, newest first,
    /// with the seat they played: (match id, seat, replay).
    pub async fn recent_replays(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<(i64, usize, Replay)>> {
        dispatch!(self, s => s.recent_replays(user_id, limit))
    }
    /// `from` pays `price` and `to` gets the item, atomically (the present
    /// is recorded, without a message).
    pub async fn gift_item(
        &self,
        from: i64,
        to: i64,
        code: &str,
        price: i64,
        at: i64,
    ) -> StoreResult<BuyOutcome> {
        let gift = NewGift { from, to, code, message: "", at };
        Ok(self.give_item(&gift, price).await?.0)
    }
    /// `gift.from` pays `price` and `gift.to` gets the item, and the present
    /// is recorded, atomically. Returns the present's id when it was given.
    pub async fn give_item(
        &self,
        gift: &NewGift<'_>,
        price: i64,
    ) -> StoreResult<(BuyOutcome, Option<i64>)> {
        dispatch!(self, s => s.give_item(gift, price))
    }
    /// Records a present paid some other way (a VIP month).
    pub async fn record_gift(&self, gift: &NewGift<'_>) -> StoreResult<i64> {
        dispatch!(self, s => s.record_gift(gift))
    }
    pub async fn gift(&self, id: i64) -> StoreResult<Option<GiftRow>> {
        dispatch!(self, s => s.gift(id))
    }
    /// Presents a player received and gave, newest first.
    pub async fn gifts_of(&self, user_id: i64, limit: u32) -> StoreResult<Vec<GiftRow>> {
        dispatch!(self, s => s.gifts_of(user_id, limit))
    }
    /// Presents a player received and has not seen yet, oldest first.
    pub async fn unseen_gifts(&self, user_id: i64) -> StoreResult<Vec<GiftRow>> {
        dispatch!(self, s => s.unseen_gifts(user_id))
    }
    /// Marks presents `user_id` received as seen; returns how many changed.
    pub async fn mark_gifts_seen(&self, user_id: i64, ids: &[i64]) -> StoreResult<u64> {
        dispatch!(self, s => s.mark_gifts_seen(user_id, ids))
    }
    /// `user_id` thanks for present `id` (theirs, once): the present, when
    /// this was the first thanks.
    pub async fn thank_gift(&self, user_id: i64, id: i64) -> StoreResult<Option<GiftRow>> {
        dispatch!(self, s => s.thank_gift(user_id, id))
    }

    // Correspondence games.

    /// A challenge from `from` to `to`; `None` if one is already waiting.
    pub async fn create_async_challenge(
        &self,
        from: i64,
        to: i64,
        rated: bool,
        rules: Rules,
        at: i64,
    ) -> StoreResult<Option<i64>> {
        dispatch!(self, s => s.create_async_challenge(from, to, rated, rules, at))
    }
    pub async fn async_challenge(&self, id: i64) -> StoreResult<Option<AsyncChallenge>> {
        dispatch!(self, s => s.async_challenge(id))
    }
    /// The challenges this player sent or received, oldest first.
    pub async fn async_challenges_of(&self, user_id: i64) -> StoreResult<Vec<AsyncChallenge>> {
        dispatch!(self, s => s.async_challenges_of(user_id))
    }
    /// Withdraws or declines a challenge; false if there was none.
    pub async fn delete_async_challenge(&self, id: i64) -> StoreResult<bool> {
        dispatch!(self, s => s.delete_async_challenge(id))
    }
    /// Takes the challenge away and starts its game, atomically (the
    /// challenger is seat 0); `None` if the challenge is gone.
    pub async fn accept_async_challenge(
        &self,
        id: i64,
        new: &NewAsyncGame,
    ) -> StoreResult<Option<AsyncGame>> {
        dispatch!(self, s => s.accept_async_challenge(id, new))
    }
    /// Removes the challenges sent before `before`; returns them.
    pub async fn purge_async_challenges(&self, before: i64) -> StoreResult<Vec<AsyncChallenge>> {
        dispatch!(self, s => s.purge_async_challenges(before))
    }
    pub async fn async_game(&self, id: i64) -> StoreResult<Option<AsyncGame>> {
        dispatch!(self, s => s.async_game(id))
    }
    /// This player's games in progress and their latest `finished` ones.
    pub async fn async_games_of(&self, user_id: i64, finished: u32) -> StoreResult<Vec<AsyncGame>> {
        dispatch!(self, s => s.async_games_of(user_id, finished))
    }
    /// Saves a game in progress if it still has `expected` strokes and is
    /// not over (a change made against an older version is refused):
    /// strokes, turn, deadline, seen, groups and balls left.
    pub async fn update_async_game(&self, game: &AsyncGame, expected: usize) -> StoreResult<bool> {
        dispatch!(self, s => s.update_async_game(game, expected))
    }
    /// `seat` has watched the first `seen` strokes (never goes back).
    pub async fn set_async_seen(&self, id: i64, seat: usize, seen: u32) -> StoreResult<()> {
        dispatch!(self, s => s.set_async_seen(id, seat, seen))
    }
    /// Ends a game that still has `expected` strokes and records it as a
    /// match (history, ratings, the replay), atomically. `None` if the game
    /// changed or was already over.
    pub async fn finish_async_game(
        &self,
        game: &AsyncGame,
        expected: usize,
        record: &MatchRecord,
    ) -> StoreResult<Option<([Option<User>; 2], i64)>> {
        dispatch!(self, s => s.finish_async_game(game, expected, record))
    }
    /// Games in progress whose turn ran out before `now`, oldest first.
    pub async fn expired_async_games(&self, now: i64, limit: u32) -> StoreResult<Vec<AsyncGame>> {
        dispatch!(self, s => s.expired_async_games(now, limit))
    }
}

/// Updates a player's numbers after a match (the same rules as the
/// database update in both stores).
pub fn apply_match_numbers(u: &mut User, m: &MatchRecord, seat: usize) {
    let won = seat == m.winner;
    if m.rated {
        let r = u.records.get_mut(m.rules);
        r.rating += m.delta[seat];
        r.peak_rating = r.peak_rating.max(r.rating);
        r.games += 1;
        if !m.counts {
            // Neither a win nor a lost streak.
        } else if won {
            r.wins += 1;
            u.streak += 1;
            u.best_streak = u.best_streak.max(u.streak);
        } else {
            u.streak = 0;
        }
    } else {
        u.casual_games += 1;
    }
    if won && m.rated && m.counts {
        u.eight_wins += m.eight_win as u32;
        u.runouts += m.runout as u32;
        u.shutouts += m.shutout as u32;
    }
    if m.abandoned == Some(seat) {
        u.abandons += 1;
    }
}

/// The staff actions in the audit log whose target is not a player (a
/// report, a tournament, an event, the lounge...): the others name one,
/// and follow the account when it is renamed or deleted (its history
/// is its own, not that of whoever takes the old name next).
pub const AUDIT_NOT_PLAYERS: &[&str] = &[
    "report",
    "tournament-cancel",
    "event",
    "event-cancel",
    "note-remove",
    "announce",
    "clear-chat",
    "motd",
    "payment",
    "msstore-retry",
    "pin",
    "unpin",
];

/// VIP time changed along with a coin credit ([`Store::credit_vip`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VipChange {
    /// `secs` more, counted from `from` or from the end of the VIP there is.
    Add { from: i64, secs: i64 },
    /// `secs` fewer, never ending before `floor` (nor lengthened).
    Take { secs: i64, floor: i64 },
}

/// The username a deleted account is left with.
pub fn deleted_name(id: i64) -> String {
    format!("deleted-{id}")
}

/// Unix day number.
pub fn day_of(t: i64) -> i64 {
    t.div_euclid(86_400)
}
