//! Storage for operations and the economy (see `crate::settings`,
//! `crate::promotions`, `crate::official`, `crate::maintenance`,
//! `crate::auction`, `crate::analytics` and migration 0028): what is kept,
//! and the [`Store`] methods (each back end implements them in its
//! `ops.rs`).

use snook_legend_proto::Rules;

use super::{HourRow, Store, StoreResult};

/// A setting changed from the portal.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SettingRow {
    pub key: String,
    /// JSON.
    pub value: String,
    /// Who changed it last (empty: unknown).
    pub updated_by: String,
    pub updated_at: i64,
}

/// A promotion in the shop, as the staff see it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PromotionRow {
    pub id: i64,
    pub title: String,
    /// "discount" or "featured".
    pub kind: String,
    pub percent: u32,
    pub codes: Vec<String>,
    pub categories: Vec<String>,
    pub starts_at: i64,
    pub ends_at: i64,
    pub created_by: String,
    pub created_at: i64,
    pub cancelled_at: Option<i64>,
}

/// A promotion to schedule (or the new terms of one).
#[derive(Debug, Clone)]
pub struct NewPromotion<'a> {
    pub title: &'a str,
    pub kind: &'a str,
    pub percent: u32,
    pub codes: &'a [String],
    pub categories: &'a [String],
    pub starts_at: i64,
    pub ends_at: i64,
    pub by: i64,
    pub at: i64,
}

/// An official tournament once over (see `crate::official`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct OfficialRecord {
    /// Unique per tournament: the same one is kept once.
    pub key: String,
    /// The staff's event it came from (none: the weekly cup).
    pub event_id: Option<i64>,
    pub name: String,
    pub rules: Rules,
    /// "knockout" or "arena".
    pub format: String,
    pub size: u32,
    pub players: u32,
    pub rated: bool,
    pub entry_fee: u32,
    pub prize_pool: i64,
    /// Coins paid to the champion and the runner-up.
    pub prizes: Vec<i64>,
    pub item_prize: String,
    pub champion_id: Option<i64>,
    pub runner_up_id: Option<i64>,
    /// JSON: players, rounds and standings, with the players' ids.
    pub detail: String,
    pub starts_at: i64,
    pub ends_at: i64,
}

/// A maintenance window.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MaintenanceRow {
    pub id: i64,
    pub at: i64,
    pub minutes: u32,
    pub message: String,
    pub created_by: String,
    pub created_at: i64,
    pub cancelled_at: Option<i64>,
    /// The server went down for it at this time.
    pub started_at: Option<i64>,
}

/// An auction, as stored.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AuctionRow {
    pub id: i64,
    pub code: String,
    pub starts_at: i64,
    pub ends_at: i64,
    pub min_bid: i64,
    pub min_increment: i64,
    pub leader_id: Option<i64>,
    /// The leader's name now (empty: nobody).
    pub leader: String,
    pub bid: i64,
    pub bids: u32,
    pub created_by: String,
    pub created_at: i64,
    pub cancelled_at: Option<i64>,
    /// The winner got the item (or it ended without bids) at this time.
    pub settled_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewAuction<'a> {
    pub code: &'a str,
    pub starts_at: i64,
    pub ends_at: i64,
    pub min_bid: i64,
    pub min_increment: i64,
    pub by: i64,
    pub at: i64,
}

/// What became of a bid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BidOutcome {
    /// Taken: the coins are held. `outbid`: the leader before (their user
    /// id and bid, given back now); `ends_at`: the end now (later when the
    /// bid came at the last moment).
    Placed {
        outbid: Option<(i64, i64)>,
        ends_at: i64,
    },
    /// Under the least the next bid may be (that least).
    TooLow(i64),
    /// Not open (coming, over, called off) or no such auction.
    NotOpen,
    NotEnoughCoins,
}

/// The daily bonus claimed today: the day in a row and its coins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreakClaim {
    pub day: u32,
    pub coins: i64,
}

/// A supporter in the credits.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SupporterRow {
    pub user_id: i64,
    pub username: String,
    pub country: String,
    /// Their first VIP month (0: not known).
    pub since: i64,
    /// VIP months paid or given.
    pub months: u32,
    pub vip_until: i64,
}

/// Coins moved on one day for one reason (as written in the ledger).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CoinDay {
    pub day: i64,
    pub reason: String,
    pub credited: i64,
    /// Coins taken (a positive number).
    pub debited: i64,
    pub entries: i64,
}

/// An item's sales over a period: bought and given (paid with coins).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ItemSales {
    pub code: String,
    pub sales: i64,
    pub coins: i64,
    pub buyers: i64,
    pub last_at: i64,
}

/// Players of one country.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CountryRow {
    /// Empty: not given.
    pub country: String,
    pub players: i64,
    /// Accounts created since the time asked.
    pub new_players: i64,
}

/// A VIP member, now or lately.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct VipMember {
    pub user_id: i64,
    pub username: String,
    pub country: String,
    pub vip_until: i64,
    /// VIP months paid through Stripe (the subscription), bought in the
    /// Microsoft Store and given by friends.
    pub stripe_months: i64,
    pub store_months: i64,
    pub gifted_months: i64,
    /// Their first and latest VIP payment or gift (0: none: the staff's).
    pub first_at: i64,
    pub last_at: i64,
    /// They pay with a Stripe subscription (a customer is kept).
    pub subscriber: bool,
}

/// What a CSV export holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Export {
    Players,
    Matches,
    Purchases,
    Ledger,
    /// One line per day: active players, games, new accounts, coins.
    Daily,
}

impl Export {
    pub const ALL: [Export; 5] =
        [Export::Players, Export::Matches, Export::Purchases, Export::Ledger, Export::Daily];

    pub fn code(self) -> &'static str {
        match self {
            Export::Players => "players",
            Export::Matches => "matches",
            Export::Purchases => "purchases",
            Export::Ledger => "ledger",
            Export::Daily => "daily",
        }
    }

    pub fn parse(code: &str) -> Option<Export> {
        Export::ALL.into_iter().find(|e| e.code() == code)
    }

    /// The CSV's header.
    pub fn header(self) -> &'static [&'static str] {
        match self {
            Export::Players => &[
                "id",
                "username",
                "country",
                "role",
                "created_at",
                "last_seen",
                "rating_brazilian",
                "games_brazilian",
                "rating_official",
                "games_official",
                "casual_games",
                "xp",
                "coins",
                "vip_until",
                "banned_until",
                "deleted_at",
            ],
            Export::Matches => &[
                "id",
                "ended_at",
                "rules",
                "rated",
                "player0",
                "player1",
                "winner_seat",
                "reason",
                "delta0",
                "delta1",
                "shots",
                "tournament",
            ],
            Export::Purchases => &[
                "id", "at", "user_id", "username", "kind", "coins", "amount", "currency",
                "refunded", "test",
            ],
            Export::Ledger => &["id", "at", "user_id", "username", "delta", "reason", "reference"],
            Export::Daily => &[
                "day",
                "date",
                "active_players",
                "games",
                "new_players",
                "coins_created",
                "coins_spent",
            ],
        }
    }
}

/// The item a ledger entry paid for: `buy {code}` (bought) or
/// `gift {code} to {id}` (given to a friend).
pub fn sold_item(reason: &str) -> Option<&str> {
    let rest = reason.strip_prefix("buy ").or_else(|| reason.strip_prefix("gift "))?;
    let code = rest.split(' ').next()?;
    (!code.is_empty()).then_some(code)
}

/// The daily bonus's coins on day `day` in a row: `base` on the first,
/// `step` more each day after, up to day `days` (see
/// `snook_legend_proto::ops::streak_coins`).
pub fn streak_coins(day: u32, base: i64, step: i64, days: u32) -> i64 {
    let d = day.clamp(1, days.max(1)) as i64;
    base.max(0) + step.max(0) * (d - 1)
}

/// Rows a CSV export reads at a time.
pub const EXPORT_PAGE: u32 = 2000;

impl Store {
    // Live settings.

    pub async fn live_settings(&self) -> StoreResult<Vec<SettingRow>> {
        dispatch!(self, s => s.live_settings())
    }
    /// Sets a setting (`None`: back to its default).
    pub async fn set_live_setting(
        &self,
        key: &str,
        value: Option<&str>,
        by: i64,
        at: i64,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.set_live_setting(key, value, by, at))
    }

    // Promotions.

    pub async fn add_promotion(&self, p: &NewPromotion<'_>) -> StoreResult<i64> {
        dispatch!(self, s => s.add_promotion(p))
    }
    /// New terms for a promotion not called off; false if there is none.
    pub async fn update_promotion(&self, id: i64, p: &NewPromotion<'_>) -> StoreResult<bool> {
        dispatch!(self, s => s.update_promotion(id, p))
    }
    /// The latest promotions (by their end), newest first.
    pub async fn promotions(&self, limit: u32) -> StoreResult<Vec<PromotionRow>> {
        dispatch!(self, s => s.promotions(limit))
    }
    /// Calls a promotion off; false if it was already.
    pub async fn cancel_promotion(&self, id: i64, at: i64) -> StoreResult<bool> {
        dispatch!(self, s => s.cancel_promotion(id, at))
    }

    // Official tournaments.

    /// Keeps an official tournament once over; its id (None: kept already).
    pub async fn record_official(&self, r: &OfficialRecord) -> StoreResult<Option<i64>> {
        dispatch!(self, s => s.record_official(r))
    }
    /// The latest, newest first.
    pub async fn officials(&self, limit: u32) -> StoreResult<Vec<(i64, OfficialRecord)>> {
        dispatch!(self, s => s.officials(limit))
    }
    pub async fn official(&self, id: i64) -> StoreResult<Option<(i64, OfficialRecord)>> {
        dispatch!(self, s => s.official(id))
    }
    /// Those `user_id` won: (id, name, ended at).
    pub async fn officials_won(&self, user_id: i64) -> StoreResult<Vec<(i64, String, i64)>> {
        dispatch!(self, s => s.officials_won(user_id))
    }

    // Maintenance.

    pub async fn add_maintenance(
        &self,
        at: i64,
        minutes: u32,
        message: &str,
        by: i64,
        created_at: i64,
    ) -> StoreResult<i64> {
        dispatch!(self, s => s.add_maintenance(at, minutes, message, by, created_at))
    }
    /// The latest windows (by their time), newest first.
    pub async fn maintenances(&self, limit: u32) -> StoreResult<Vec<MaintenanceRow>> {
        dispatch!(self, s => s.maintenances(limit))
    }
    /// Calls a window off before it started; false otherwise.
    pub async fn cancel_maintenance(&self, id: i64, at: i64) -> StoreResult<bool> {
        dispatch!(self, s => s.cancel_maintenance(id, at))
    }
    /// Marks a window as started; false when it was already (by this
    /// server or another): only one goes down for it.
    pub async fn start_maintenance(&self, id: i64, at: i64) -> StoreResult<bool> {
        dispatch!(self, s => s.start_maintenance(id, at))
    }

    // Auctions.

    pub async fn add_auction(&self, a: &NewAuction<'_>) -> StoreResult<i64> {
        dispatch!(self, s => s.add_auction(a))
    }
    /// The latest auctions (by their end), newest first.
    pub async fn auctions(&self, limit: u32) -> StoreResult<Vec<AuctionRow>> {
        dispatch!(self, s => s.auctions(limit))
    }
    pub async fn auction(&self, id: i64) -> StoreResult<Option<AuctionRow>> {
        dispatch!(self, s => s.auction(id))
    }
    /// A bid, in one step: the bidder's coins are held, the leader before
    /// gets theirs back, a bid in the last `window` seconds moves the end
    /// `extension` seconds later.
    pub async fn bid(
        &self,
        id: i64,
        user_id: i64,
        amount: i64,
        now: i64,
        window: i64,
        extension: i64,
    ) -> StoreResult<BidOutcome> {
        dispatch!(self, s => s.bid(id, user_id, amount, now, window, extension))
    }
    /// Calls an auction off (not settled yet), giving the leader's coins
    /// back: Some(the leader and bid given back), None when it can't be.
    pub async fn cancel_auction(
        &self,
        id: i64,
        at: i64,
    ) -> StoreResult<Option<Option<(i64, i64)>>> {
        dispatch!(self, s => s.cancel_auction(id, at))
    }
    /// Settles an auction that ended: the winner gets the item, in the same
    /// step. The row as settled (None: not over, or settled already), and
    /// whether the item was given: a winner who has it already (got it
    /// meanwhile) gets the bid back instead.
    pub async fn settle_auction(
        &self,
        id: i64,
        at: i64,
    ) -> StoreResult<Option<(AuctionRow, bool)>> {
        dispatch!(self, s => s.settle_auction(id, at))
    }
    /// The bids made, newest first: (bidder, amount, when).
    pub async fn auction_bids(&self, id: i64, limit: u32) -> StoreResult<Vec<(String, i64, i64)>> {
        dispatch!(self, s => s.auction_bids(id, limit))
    }

    // The daily bonus's streak.

    /// The daily bonus of `day` with its streak, once a day: day 1 brings
    /// `base`, each day in a row `step` more, up to day `days`. None if it
    /// was claimed today already.
    pub async fn claim_daily_streak(
        &self,
        user_id: i64,
        day: i64,
        base: i64,
        step: i64,
        days: u32,
    ) -> StoreResult<Option<StreakClaim>> {
        dispatch!(self, s => s.claim_daily_streak(user_id, day, base, step, days))
    }

    // The supporters' credits.

    pub async fn set_credits(&self, user_id: i64, show: bool, at: i64) -> StoreResult<()> {
        dispatch!(self, s => s.set_credits(user_id, show, at))
    }
    pub async fn in_credits(&self, user_id: i64) -> StoreResult<bool> {
        dispatch!(self, s => s.in_credits(user_id))
    }
    /// Those in the credits (VIP now or before, not banned or deleted),
    /// longest supporters first.
    pub async fn supporters(&self, now: i64) -> StoreResult<Vec<SupporterRow>> {
        dispatch!(self, s => s.supporters(now))
    }

    // Analytics.

    /// Coins moved per day and reason, from day `from` to day `to`.
    pub async fn coin_days(&self, from: i64, to: i64) -> StoreResult<Vec<CoinDay>> {
        dispatch!(self, s => s.coin_days(from, to))
    }
    /// Items bought and given with coins since `since`.
    pub async fn item_sales(&self, since: i64) -> StoreResult<Vec<ItemSales>> {
        dispatch!(self, s => s.item_sales(since))
    }
    /// Players per country (not deleted), and those new since `since`.
    pub async fn countries(&self, since: i64) -> StoreResult<Vec<CountryRow>> {
        dispatch!(self, s => s.countries(since))
    }
    /// Who was online hour by hour since hour `from`, oldest first.
    pub async fn hour_samples(&self, from: i64) -> StoreResult<Vec<HourRow>> {
        dispatch!(self, s => s.hour_samples(from))
    }
    /// VIP members now, and those whose VIP ended since `since`.
    pub async fn vip_members(&self, since: i64) -> StoreResult<Vec<VipMember>> {
        dispatch!(self, s => s.vip_members(since))
    }
    /// A page of a CSV export between `from` and `to` (Unix seconds; a day
    /// for [`Export::Daily`]), after the row `after`: (cursor, fields).
    pub async fn export_page(
        &self,
        kind: Export,
        from: i64,
        to: i64,
        after: i64,
        limit: u32,
    ) -> StoreResult<Vec<(i64, Vec<String>)>> {
        dispatch!(self, s => s.export_page(kind, from, to, after, limit))
    }
}
