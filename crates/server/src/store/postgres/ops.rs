//! Operations and the economy in PostgreSQL (migration 0028): settings,
//! promotions, official tournaments, maintenance, auctions, the daily
//! bonus's streak, the supporters' credits, and the staff's analytics.

use snook_legend_proto::Rules;
use snook_legend_proto::shop::VIP_GIFT;
use sqlx::postgres::PgRow;
use sqlx::{Row, query};

use super::super::{
    AuctionRow, BidOutcome, CoinDay, CountryRow, Export, HourRow, ItemSales, MS_CURRENCY,
    MaintenanceRow, NewAuction, NewPromotion, OfficialRecord, PromotionRow, SettingRow,
    StoreResult, StreakClaim, SupporterRow, VipMember, sold_item, streak_coins,
};
use super::PgStore;

fn list(text: &str) -> Vec<String> {
    text.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

fn promotion(r: &PgRow) -> PromotionRow {
    PromotionRow {
        id: r.get("id"),
        title: r.get("title"),
        kind: r.get("kind"),
        percent: r.get::<i16, _>("percent").max(0) as u32,
        codes: list(r.get("codes")),
        categories: list(r.get("categories")),
        starts_at: r.get("starts_at"),
        ends_at: r.get("ends_at"),
        created_by: r.get("author"),
        created_at: r.get("created_at"),
        cancelled_at: r.get("cancelled_at"),
    }
}

fn official(r: &PgRow) -> (i64, OfficialRecord) {
    let prizes: String = r.get("prizes");
    (
        r.get("id"),
        OfficialRecord {
            key: r.get("key"),
            event_id: r.get("event_id"),
            name: r.get("name"),
            rules: Rules::parse(r.get("rules")).unwrap_or_default(),
            format: r.get("format"),
            size: r.get::<i32, _>("size").max(0) as u32,
            players: r.get::<i32, _>("players").max(0) as u32,
            rated: r.get("rated"),
            entry_fee: r.get::<i32, _>("entry_fee").max(0) as u32,
            prize_pool: r.get("prize_pool"),
            prizes: serde_json::from_str(&prizes).unwrap_or_default(),
            item_prize: r.get("item_prize"),
            champion_id: r.get("champion_id"),
            runner_up_id: r.get("runner_up_id"),
            detail: r.get("detail"),
            starts_at: r.get("starts_at"),
            ends_at: r.get("ends_at"),
        },
    )
}

fn maintenance(r: &PgRow) -> MaintenanceRow {
    MaintenanceRow {
        id: r.get("id"),
        at: r.get("at"),
        minutes: r.get::<i32, _>("minutes").max(0) as u32,
        message: r.get("message"),
        created_by: r.get("author"),
        created_at: r.get("created_at"),
        cancelled_at: r.get("cancelled_at"),
        started_at: r.get("started_at"),
    }
}

const AUCTION_SELECT: &str = "SELECT a.*, COALESCE(l.username, '') AS leader,
        COALESCE(c.username, '') AS author
    FROM auctions a LEFT JOIN users l ON l.id = a.leader_id
    LEFT JOIN users c ON c.id = a.created_by";

fn auction(r: &PgRow) -> AuctionRow {
    AuctionRow {
        id: r.get("id"),
        code: r.get("code"),
        starts_at: r.get("starts_at"),
        ends_at: r.get("ends_at"),
        min_bid: r.get("min_bid"),
        min_increment: r.get("min_increment"),
        leader_id: r.get("leader_id"),
        leader: r.get("leader"),
        bid: r.get("bid"),
        bids: r.get::<i32, _>("bids").max(0) as u32,
        created_by: r.get("author"),
        created_at: r.get("created_at"),
        cancelled_at: r.get("cancelled_at"),
        settled_at: r.get("settled_at"),
    }
}

/// A day as `YYYY-MM-DD`.
fn date(day: i64) -> String {
    crate::analytics::date(day * 86_400)
}

impl PgStore {
    pub async fn live_settings(&self) -> StoreResult<Vec<SettingRow>> {
        let rows = query(
            "SELECT s.key, s.value, s.updated_at, COALESCE(u.username, '') AS author
             FROM live_settings s LEFT JOIN users u ON u.id = s.updated_by ORDER BY s.key",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| SettingRow {
                key: r.get("key"),
                value: r.get("value"),
                updated_by: r.get("author"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    pub async fn set_live_setting(
        &self,
        key: &str,
        value: Option<&str>,
        by: i64,
        at: i64,
    ) -> StoreResult<()> {
        match value {
            Some(v) => {
                query(
                    "INSERT INTO live_settings (key, value, updated_by, updated_at)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value,
                        updated_by = EXCLUDED.updated_by, updated_at = EXCLUDED.updated_at",
                )
                .bind(key)
                .bind(v)
                .bind(by)
                .bind(at)
                .execute(&self.pool)
                .await?;
            }
            None => {
                query("DELETE FROM live_settings WHERE key = $1")
                    .bind(key)
                    .execute(&self.pool)
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn add_promotion(&self, p: &NewPromotion<'_>) -> StoreResult<i64> {
        let row = query(
            "INSERT INTO promotions (title, kind, percent, codes, categories, starts_at, ends_at,
                                     created_by, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
        )
        .bind(p.title)
        .bind(p.kind)
        .bind(p.percent as i16)
        .bind(p.codes.join(","))
        .bind(p.categories.join(","))
        .bind(p.starts_at)
        .bind(p.ends_at)
        .bind(p.by)
        .bind(p.at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("id"))
    }

    pub async fn update_promotion(&self, id: i64, p: &NewPromotion<'_>) -> StoreResult<bool> {
        let n = query(
            "UPDATE promotions SET title = $2, kind = $3, percent = $4, codes = $5,
                categories = $6, starts_at = $7, ends_at = $8
             WHERE id = $1 AND cancelled_at IS NULL",
        )
        .bind(id)
        .bind(p.title)
        .bind(p.kind)
        .bind(p.percent as i16)
        .bind(p.codes.join(","))
        .bind(p.categories.join(","))
        .bind(p.starts_at)
        .bind(p.ends_at)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(n > 0)
    }

    pub async fn promotions(&self, limit: u32) -> StoreResult<Vec<PromotionRow>> {
        let rows = query(
            "SELECT p.*, COALESCE(u.username, '') AS author FROM promotions p
             LEFT JOIN users u ON u.id = p.created_by ORDER BY p.ends_at DESC, p.id DESC LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(promotion).collect())
    }

    pub async fn cancel_promotion(&self, id: i64, at: i64) -> StoreResult<bool> {
        let n =
            query("UPDATE promotions SET cancelled_at = $2 WHERE id = $1 AND cancelled_at IS NULL")
                .bind(id)
                .bind(at)
                .execute(&self.pool)
                .await?
                .rows_affected();
        Ok(n > 0)
    }

    pub async fn record_official(&self, r: &OfficialRecord) -> StoreResult<Option<i64>> {
        let row = query(
            "INSERT INTO official_tournaments (key, event_id, name, rules, format, size, players,
                rated, entry_fee, prize_pool, prizes, item_prize, champion_id, runner_up_id,
                detail, starts_at, ends_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
             ON CONFLICT (key) DO NOTHING RETURNING id",
        )
        .bind(&r.key)
        .bind(r.event_id)
        .bind(&r.name)
        .bind(r.rules.code())
        .bind(&r.format)
        .bind(r.size as i32)
        .bind(r.players as i32)
        .bind(r.rated)
        .bind(r.entry_fee as i32)
        .bind(r.prize_pool)
        .bind(serde_json::to_string(&r.prizes).unwrap_or_else(|_| "[]".into()))
        .bind(&r.item_prize)
        .bind(r.champion_id)
        .bind(r.runner_up_id)
        .bind(&r.detail)
        .bind(r.starts_at)
        .bind(r.ends_at)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get("id")))
    }

    pub async fn officials(&self, limit: u32) -> StoreResult<Vec<(i64, OfficialRecord)>> {
        let rows =
            query("SELECT * FROM official_tournaments ORDER BY ends_at DESC, id DESC LIMIT $1")
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.iter().map(official).collect())
    }

    pub async fn official(&self, id: i64) -> StoreResult<Option<(i64, OfficialRecord)>> {
        let row = query("SELECT * FROM official_tournaments WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.as_ref().map(official))
    }

    pub async fn officials_won(&self, user_id: i64) -> StoreResult<Vec<(i64, String, i64)>> {
        let rows = query(
            "SELECT id, name, ends_at FROM official_tournaments WHERE champion_id = $1
             ORDER BY ends_at DESC LIMIT 200",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| (r.get("id"), r.get("name"), r.get("ends_at"))).collect())
    }

    pub async fn add_maintenance(
        &self,
        at: i64,
        minutes: u32,
        message: &str,
        by: i64,
        created_at: i64,
    ) -> StoreResult<i64> {
        let row = query(
            "INSERT INTO maintenance (at, minutes, message, created_by, created_at)
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(at)
        .bind(minutes as i32)
        .bind(message)
        .bind(by)
        .bind(created_at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("id"))
    }

    pub async fn maintenances(&self, limit: u32) -> StoreResult<Vec<MaintenanceRow>> {
        let rows = query(
            "SELECT m.*, COALESCE(u.username, '') AS author FROM maintenance m
             LEFT JOIN users u ON u.id = m.created_by ORDER BY m.at DESC, m.id DESC LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(maintenance).collect())
    }

    pub async fn cancel_maintenance(&self, id: i64, at: i64) -> StoreResult<bool> {
        let n = query(
            "UPDATE maintenance SET cancelled_at = $2
             WHERE id = $1 AND cancelled_at IS NULL AND started_at IS NULL",
        )
        .bind(id)
        .bind(at)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(n > 0)
    }

    pub async fn start_maintenance(&self, id: i64, at: i64) -> StoreResult<bool> {
        let n = query(
            "UPDATE maintenance SET started_at = $2
             WHERE id = $1 AND cancelled_at IS NULL AND started_at IS NULL",
        )
        .bind(id)
        .bind(at)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(n > 0)
    }

    pub async fn add_auction(&self, a: &NewAuction<'_>) -> StoreResult<i64> {
        let row = query(
            "INSERT INTO auctions (code, starts_at, ends_at, min_bid, min_increment, created_by,
                                   created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        )
        .bind(a.code)
        .bind(a.starts_at)
        .bind(a.ends_at)
        .bind(a.min_bid)
        .bind(a.min_increment)
        .bind(a.by)
        .bind(a.at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("id"))
    }

    pub async fn auctions(&self, limit: u32) -> StoreResult<Vec<AuctionRow>> {
        let rows = query(&format!("{AUCTION_SELECT} ORDER BY a.ends_at DESC, a.id DESC LIMIT $1"))
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(auction).collect())
    }

    pub async fn auction(&self, id: i64) -> StoreResult<Option<AuctionRow>> {
        let row = query(&format!("{AUCTION_SELECT} WHERE a.id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.as_ref().map(auction))
    }

    pub async fn bid(
        &self,
        id: i64,
        user_id: i64,
        amount: i64,
        now: i64,
        window: i64,
        extension: i64,
    ) -> StoreResult<BidOutcome> {
        let mut tx = self.pool.begin().await?;
        // The auction's row is held until the bid is in: two bids at once
        // are taken one after the other.
        let Some(a) = query("SELECT * FROM auctions WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
        else {
            return Ok(BidOutcome::NotOpen);
        };
        let (starts_at, ends_at): (i64, i64) = (a.get("starts_at"), a.get("ends_at"));
        let cancelled: Option<i64> = a.get("cancelled_at");
        let settled: Option<i64> = a.get("settled_at");
        if cancelled.is_some() || settled.is_some() || now < starts_at || now >= ends_at {
            return Ok(BidOutcome::NotOpen);
        }
        let (bid, leader): (i64, Option<i64>) = (a.get("bid"), a.get("leader_id"));
        let (min_bid, inc): (i64, i64) = (a.get("min_bid"), a.get("min_increment"));
        let least = if bid == 0 { min_bid } else { bid + inc.max(1) };
        if amount < least {
            return Ok(BidOutcome::TooLow(least));
        }
        let bid_id: i64 = query(
            "INSERT INTO auction_bids (auction_id, user_id, amount, at) VALUES ($1, $2, $3, $4)
             RETURNING id",
        )
        .bind(id)
        .bind(user_id)
        .bind(amount)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?
        .get("id");
        let outbid = leader.map(|l| (l, bid));
        // The leader before gets their coins back first (the same player
        // raising their bid puts in only the difference).
        if let Some((l, held)) = outbid {
            query("UPDATE wallets SET coins = coins + $2 WHERE user_id = $1")
                .bind(l)
                .bind(held)
                .execute(&mut *tx)
                .await?;
            query(
                "INSERT INTO coin_ledger (user_id, delta, reason, reference, at)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(l)
            .bind(held)
            .bind(format!("auction refund #{id}"))
            .bind(format!("auction-refund:{id}:{bid_id}"))
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        let paid =
            query("UPDATE wallets SET coins = coins - $2 WHERE user_id = $1 AND coins >= $2")
                .bind(user_id)
                .bind(amount)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        if paid == 0 {
            // Nothing happened: the refund above is undone with the rest.
            return Ok(BidOutcome::NotEnoughCoins);
        }
        query(
            "INSERT INTO coin_ledger (user_id, delta, reason, reference, at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(user_id)
        .bind(-amount)
        .bind(format!("auction bid #{id}"))
        .bind(format!("auction:{id}:{bid_id}"))
        .bind(now)
        .execute(&mut *tx)
        .await?;
        let new_end = if ends_at - now <= window { now + extension } else { ends_at };
        query(
            "UPDATE auctions SET leader_id = $2, bid = $3, bids = bids + 1, ends_at = $4
             WHERE id = $1",
        )
        .bind(id)
        .bind(user_id)
        .bind(amount)
        .bind(new_end)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(BidOutcome::Placed { outbid, ends_at: new_end })
    }

    pub async fn cancel_auction(
        &self,
        id: i64,
        at: i64,
    ) -> StoreResult<Option<Option<(i64, i64)>>> {
        let mut tx = self.pool.begin().await?;
        let Some(a) = query(
            "UPDATE auctions SET cancelled_at = $2
             WHERE id = $1 AND cancelled_at IS NULL AND settled_at IS NULL
             RETURNING leader_id, bid",
        )
        .bind(id)
        .bind(at)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(None);
        };
        let back = a.get::<Option<i64>, _>("leader_id").map(|l| (l, a.get::<i64, _>("bid")));
        if let Some((l, held)) = back {
            query("UPDATE wallets SET coins = coins + $2 WHERE user_id = $1")
                .bind(l)
                .bind(held)
                .execute(&mut *tx)
                .await?;
            query(
                "INSERT INTO coin_ledger (user_id, delta, reason, reference, at)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(l)
            .bind(held)
            .bind(format!("auction refund #{id}"))
            .bind(format!("auction-cancel:{id}"))
            .bind(at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(Some(back))
    }

    pub async fn settle_auction(&self, id: i64, at: i64) -> StoreResult<Option<AuctionRow>> {
        let mut tx = self.pool.begin().await?;
        let Some(a) = query(
            "UPDATE auctions SET settled_at = $2
             WHERE id = $1 AND cancelled_at IS NULL AND settled_at IS NULL AND ends_at <= $2
             RETURNING leader_id, code, bid",
        )
        .bind(id)
        .bind(at)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(None);
        };
        if let Some(winner) = a.get::<Option<i64>, _>("leader_id") {
            query(
                "INSERT INTO owned_items (user_id, code, acquired_at, source)
                 VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
            )
            .bind(winner)
            .bind(a.get::<String, _>("code"))
            .bind(at)
            .bind(format!("auction:{id}"))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.auction(id).await
    }

    pub async fn auction_bids(&self, id: i64, limit: u32) -> StoreResult<Vec<(String, i64, i64)>> {
        let rows = query(
            "SELECT u.username, b.amount, b.at FROM auction_bids b JOIN users u ON u.id = b.user_id
             WHERE b.auction_id = $1 ORDER BY b.id DESC LIMIT $2",
        )
        .bind(id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| (r.get("username"), r.get("amount"), r.get("at"))).collect())
    }

    pub async fn claim_daily_streak(
        &self,
        user_id: i64,
        day: i64,
        base: i64,
        step: i64,
        days: u32,
    ) -> StoreResult<Option<StreakClaim>> {
        let mut tx = self.pool.begin().await?;
        query("INSERT INTO wallets (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        // Today's claim first (once a day, as the plain bonus): the streak
        // only moves with it.
        let n = query("UPDATE wallets SET daily_day = $2 WHERE user_id = $1 AND daily_day < $2")
            .bind(user_id)
            .bind(day)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if n == 0 {
            return Ok(None);
        }
        let streak: i32 = query(
            "INSERT INTO daily_streaks (user_id, last_day, streak) VALUES ($1, $2, 1)
             ON CONFLICT (user_id) DO UPDATE SET
                streak = CASE WHEN daily_streaks.last_day = $2 - 1 THEN daily_streaks.streak + 1
                              WHEN daily_streaks.last_day = $2 THEN daily_streaks.streak
                              ELSE 1 END,
                last_day = $2
             RETURNING streak",
        )
        .bind(user_id)
        .bind(day)
        .fetch_one(&mut *tx)
        .await?
        .get("streak");
        let streak = streak.max(1) as u32;
        let coins = streak_coins(streak, base, step, days);
        query("UPDATE wallets SET coins = coins + $2 WHERE user_id = $1")
            .bind(user_id)
            .bind(coins)
            .execute(&mut *tx)
            .await?;
        query(
            "INSERT INTO coin_ledger (user_id, delta, reason, reference, at)
             VALUES ($1, $2, 'daily bonus', NULL, $3)",
        )
        .bind(user_id)
        .bind(coins)
        .bind(day * 86_400)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(StreakClaim { day: streak, coins }))
    }

    pub async fn set_credits(&self, user_id: i64, show: bool, at: i64) -> StoreResult<()> {
        if show {
            query(
                "INSERT INTO credits (user_id, updated_at) VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE SET updated_at = EXCLUDED.updated_at",
            )
            .bind(user_id)
            .bind(at)
            .execute(&self.pool)
            .await?;
        } else {
            query("DELETE FROM credits WHERE user_id = $1")
                .bind(user_id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    pub async fn in_credits(&self, user_id: i64) -> StoreResult<bool> {
        let row = query("SELECT 1 AS x FROM credits WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    pub async fn supporters(&self, now: i64) -> StoreResult<Vec<SupporterRow>> {
        let rows = query(
            "WITH months AS (
                SELECT user_id, at FROM payments WHERE kind = 'vip' AND NOT refunded
                UNION ALL
                SELECT to_id AS user_id, at FROM gifts WHERE code = $2
             )
             SELECT u.id, u.username, u.country, w.vip_until,
                    MIN(m.at) AS since, COUNT(m.at) AS months
             FROM credits c JOIN users u ON u.id = c.user_id
             JOIN wallets w ON w.user_id = u.id
             LEFT JOIN months m ON m.user_id = u.id
             WHERE u.deleted_at IS NULL AND (u.banned_until IS NULL OR u.banned_until <= $1)
               AND w.vip_until > 0
             GROUP BY u.id, u.username, u.country, w.vip_until",
        )
        .bind(now)
        .bind(VIP_GIFT)
        .fetch_all(&self.pool)
        .await?;
        let mut out: Vec<SupporterRow> = rows
            .iter()
            .map(|r| {
                let vip_until: i64 = r.get("vip_until");
                let since: Option<i64> = r.get("since");
                SupporterRow {
                    user_id: r.get("id"),
                    username: r.get("username"),
                    country: r.get("country"),
                    since: since
                        .unwrap_or(vip_until - snook_legend_proto::shop::VIP_MONTH_SECS)
                        .max(0),
                    months: (r.get::<i64, _>("months").max(1)) as u32,
                    vip_until,
                }
            })
            .collect();
        out.sort_by(|a, b| a.since.cmp(&b.since).then(a.username.cmp(&b.username)));
        Ok(out)
    }

    pub async fn coin_days(&self, from: i64, to: i64) -> StoreResult<Vec<CoinDay>> {
        let rows = query(
            "SELECT at / 86400 AS day, reason,
                    COALESCE(SUM(delta) FILTER (WHERE delta >= 0), 0)::BIGINT AS credited,
                    COALESCE(-SUM(delta) FILTER (WHERE delta < 0), 0)::BIGINT AS debited,
                    COUNT(*) AS entries
             FROM coin_ledger WHERE at >= $1 AND at < $2
             GROUP BY 1, 2 ORDER BY 1, 2",
        )
        .bind(from * 86_400)
        .bind((to + 1) * 86_400)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| CoinDay {
                day: r.get("day"),
                reason: r.get("reason"),
                credited: r.get("credited"),
                debited: r.get("debited"),
                entries: r.get("entries"),
            })
            .collect())
    }

    pub async fn item_sales(&self, since: i64) -> StoreResult<Vec<ItemSales>> {
        // Grouped by the reason as written ("buy x", "gift x to 12"); the
        // items are made here.
        let rows = query(
            "SELECT reason, user_id, COUNT(*) AS n, -SUM(delta)::BIGINT AS coins, MAX(at) AS last
             FROM coin_ledger
             WHERE at >= $1 AND delta <= 0 AND (reason LIKE 'buy %' OR reason LIKE 'gift %')
             GROUP BY reason, user_id",
        )
        .bind(since)
        .fetch_all(&self.pool)
        .await?;
        let mut by: std::collections::BTreeMap<
            String,
            (ItemSales, std::collections::HashSet<i64>),
        > = Default::default();
        for r in &rows {
            let reason: String = r.get("reason");
            let Some(code) = sold_item(&reason) else { continue };
            let (row, buyers) = by.entry(code.to_string()).or_insert_with(|| {
                (
                    ItemSales { code: code.to_string(), sales: 0, coins: 0, buyers: 0, last_at: 0 },
                    Default::default(),
                )
            });
            row.sales += r.get::<i64, _>("n");
            row.coins += r.get::<i64, _>("coins");
            row.last_at = row.last_at.max(r.get("last"));
            buyers.insert(r.get("user_id"));
        }
        Ok(by
            .into_values()
            .map(|(mut row, buyers)| {
                row.buyers = buyers.len() as i64;
                row
            })
            .collect())
    }

    pub async fn countries(&self, since: i64) -> StoreResult<Vec<CountryRow>> {
        let rows = query(
            "SELECT country, COUNT(*) AS players,
                    COUNT(*) FILTER (WHERE created_at >= $1) AS new_players
             FROM users WHERE deleted_at IS NULL GROUP BY country ORDER BY country",
        )
        .bind(since)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| CountryRow {
                country: r.get("country"),
                players: r.get("players"),
                new_players: r.get("new_players"),
            })
            .collect())
    }

    pub async fn hour_samples(&self, from: i64) -> StoreResult<Vec<HourRow>> {
        let rows = query(
            "SELECT hour, peak_players, peak_games, samples, player_sum FROM online_hours
             WHERE hour >= $1 ORDER BY hour",
        )
        .bind(from)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                let samples = r.get::<i32, _>("samples") as i64;
                let sum: i64 = r.get("player_sum");
                HourRow {
                    hour: r.get("hour"),
                    active: 0,
                    peak_players: r.get::<i32, _>("peak_players") as i64,
                    peak_games: r.get::<i32, _>("peak_games") as i64,
                    avg_players: if samples > 0 { sum as f64 / samples as f64 } else { 0.0 },
                    games: 0,
                }
            })
            .collect())
    }

    pub async fn vip_members(&self, since: i64) -> StoreResult<Vec<VipMember>> {
        let rows = query(
            "WITH paid AS (
                SELECT user_id,
                       COUNT(*) FILTER (WHERE currency <> $2) AS stripe,
                       COUNT(*) FILTER (WHERE currency = $2) AS store,
                       MIN(at) AS first, MAX(at) AS last
                FROM payments WHERE kind = 'vip' GROUP BY user_id
             ), given AS (
                SELECT to_id AS user_id, COUNT(*) AS n, MIN(at) AS first, MAX(at) AS last
                FROM gifts WHERE code = $3 GROUP BY to_id
             )
             SELECT u.id, u.username, u.country, w.vip_until,
                    w.stripe_customer IS NOT NULL AS subscriber,
                    COALESCE(p.stripe, 0) AS stripe, COALESCE(p.store, 0) AS store,
                    COALESCE(g.n, 0) AS gifted,
                    LEAST(p.first, g.first) AS first, GREATEST(p.last, g.last) AS last
             FROM wallets w JOIN users u ON u.id = w.user_id
             LEFT JOIN paid p ON p.user_id = u.id
             LEFT JOIN given g ON g.user_id = u.id
             WHERE w.vip_until > 0 AND w.vip_until >= $1
             ORDER BY w.vip_until DESC, u.id DESC",
        )
        .bind(since)
        .bind(MS_CURRENCY)
        .bind(VIP_GIFT)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| VipMember {
                user_id: r.get("id"),
                username: r.get("username"),
                country: r.get("country"),
                vip_until: r.get("vip_until"),
                stripe_months: r.get("stripe"),
                store_months: r.get("store"),
                gifted_months: r.get("gifted"),
                first_at: r.get::<Option<i64>, _>("first").unwrap_or(0),
                last_at: r.get::<Option<i64>, _>("last").unwrap_or(0),
                subscriber: r.get("subscriber"),
            })
            .collect())
    }

    pub async fn export_page(
        &self,
        kind: Export,
        from: i64,
        to: i64,
        after: i64,
        limit: u32,
    ) -> StoreResult<Vec<(i64, Vec<String>)>> {
        let s = |r: &PgRow, c: &str| -> String {
            if let Ok(v) = r.try_get::<String, _>(c) {
                return v;
            }
            if let Ok(v) = r.try_get::<Option<i64>, _>(c) {
                return v.map(|v| v.to_string()).unwrap_or_default();
            }
            if let Ok(v) = r.try_get::<Option<i32>, _>(c) {
                return v.map(|v| v.to_string()).unwrap_or_default();
            }
            if let Ok(v) = r.try_get::<Option<i16>, _>(c) {
                return v.map(|v| v.to_string()).unwrap_or_default();
            }
            if let Ok(v) = r.try_get::<Option<bool>, _>(c) {
                return v.map(|v| v.to_string()).unwrap_or_default();
            }
            r.try_get::<Option<String>, _>(c).ok().flatten().unwrap_or_default()
        };
        let (sql, cols): (&str, &[&str]) = match kind {
            Export::Players => (
                "SELECT u.id AS cursor, u.id, u.username, u.country,
                        CASE u.role WHEN 2 THEN 'admin' WHEN 1 THEN 'moderator' ELSE 'player' END AS role,
                        u.created_at, u.last_seen, u.rating, u.games, u.official_rating,
                        u.official_games, u.casual_games, u.xp, COALESCE(w.coins, 0) AS coins,
                        COALESCE(w.vip_until, 0) AS vip_until, u.banned_until, u.deleted_at
                 FROM users u LEFT JOIN wallets w ON w.user_id = u.id
                 WHERE u.id > $3 AND u.created_at >= $1 AND u.created_at < $2
                 ORDER BY u.id LIMIT $4",
                &[
                    "id",
                    "username",
                    "country",
                    "role",
                    "created_at",
                    "last_seen",
                    "rating",
                    "games",
                    "official_rating",
                    "official_games",
                    "casual_games",
                    "xp",
                    "coins",
                    "vip_until",
                    "banned_until",
                    "deleted_at",
                ],
            ),
            Export::Matches => (
                "SELECT m.id AS cursor, m.id, m.ended_at, m.rules, m.rated::TEXT AS rated,
                        COALESCE(a.username, '') AS p0, COALESCE(b.username, '') AS p1,
                        m.winner_seat, m.reason, m.delta0, m.delta1, m.shots, m.tournament_id
                 FROM matches m LEFT JOIN users a ON a.id = m.player0
                 LEFT JOIN users b ON b.id = m.player1
                 WHERE m.id > $3 AND m.ended_at >= $1 AND m.ended_at < $2
                 ORDER BY m.id LIMIT $4",
                &[
                    "id",
                    "ended_at",
                    "rules",
                    "rated",
                    "p0",
                    "p1",
                    "winner_seat",
                    "reason",
                    "delta0",
                    "delta1",
                    "shots",
                    "tournament_id",
                ],
            ),
            Export::Purchases => (
                "SELECT ROW_NUMBER() OVER (ORDER BY p.at, p.id) AS cursor, p.id, p.at, p.user_id,
                        COALESCE(u.username, '') AS username, p.kind, p.coins, p.amount,
                        p.currency, p.refunded::TEXT AS refunded, p.test::TEXT AS test
                 FROM payments p LEFT JOIN users u ON u.id = p.user_id
                 WHERE p.at >= $1 AND p.at < $2
                 ORDER BY p.at, p.id OFFSET $3 LIMIT $4",
                &[
                    "id", "at", "user_id", "username", "kind", "coins", "amount", "currency",
                    "refunded", "test",
                ],
            ),
            Export::Ledger => (
                "SELECT l.id AS cursor, l.id, l.at, l.user_id, COALESCE(u.username, '') AS username,
                        l.delta, l.reason, COALESCE(l.reference, '') AS reference
                 FROM coin_ledger l LEFT JOIN users u ON u.id = l.user_id
                 WHERE l.id > $3 AND l.at >= $1 AND l.at < $2
                 ORDER BY l.id LIMIT $4",
                &["id", "at", "user_id", "username", "delta", "reason", "reference"],
            ),
            Export::Daily => {
                let first = (from.div_euclid(86_400)).max(after + 1);
                let last = (to - 1).div_euclid(86_400);
                let rows = query(
                    "SELECT d.day,
                        (SELECT COUNT(*) FROM activity a WHERE a.day = d.day) AS active,
                        (SELECT COUNT(*) FROM matches m
                          WHERE m.ended_at >= d.day * 86400 AND m.ended_at < (d.day + 1) * 86400) AS games,
                        (SELECT COUNT(*) FROM users u
                          WHERE u.created_at >= d.day * 86400 AND u.created_at < (d.day + 1) * 86400) AS new,
                        (SELECT COALESCE(SUM(delta) FILTER (WHERE delta >= 0), 0)::BIGINT FROM coin_ledger l
                          WHERE l.at >= d.day * 86400 AND l.at < (d.day + 1) * 86400) AS made,
                        (SELECT COALESCE(-SUM(delta) FILTER (WHERE delta < 0), 0)::BIGINT FROM coin_ledger l
                          WHERE l.at >= d.day * 86400 AND l.at < (d.day + 1) * 86400) AS spent
                     FROM generate_series($1::bigint, $2::bigint) AS d(day)
                     ORDER BY d.day LIMIT $3",
                )
                .bind(first)
                .bind(last)
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await?;
                return Ok(rows
                    .iter()
                    .map(|r| {
                        let day: i64 = r.get("day");
                        let n = |c: &str| r.get::<i64, _>(c).to_string();
                        (
                            day,
                            vec![
                                day.to_string(),
                                date(day),
                                n("active"),
                                n("games"),
                                n("new"),
                                n("made"),
                                n("spent"),
                            ],
                        )
                    })
                    .collect());
            }
        };
        let rows = query(sql)
            .bind(from)
            .bind(to)
            .bind(after)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get::<i64, _>("cursor"), cols.iter().map(|c| s(r, c)).collect()))
            .collect())
    }
}
