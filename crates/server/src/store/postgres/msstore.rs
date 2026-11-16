//! Microsoft Store purchases in PostgreSQL (table `msstore_consumes`,
//! migration 0011). See `crate::msstore` for the whole flow.

use sqlx::postgres::PgRow;
use sqlx::{Row, query};

use super::super::{
    MS_CURRENCY, MS_FIRST_RETRY, MS_LEASE, MsConsume, MsRecord, MsState, StoreResult, ms_payment_id,
};
use super::PgStore;

const PENDING_COLUMNS: &str = "tracking_id, user_id, product_id, pack, coins, transaction_id, \
    COALESCE(store_key, '') AS store_key, attempts, created_at";

fn consume_row(r: &PgRow) -> MsConsume {
    MsConsume {
        tracking_id: r.get("tracking_id"),
        user_id: r.get("user_id"),
        product_id: r.get("product_id"),
        pack: r.get("pack"),
        coins: r.get("coins"),
        transaction_id: r.get("transaction_id"),
        store_key: r.get("store_key"),
        attempts: r.get("attempts"),
        created_at: r.get("created_at"),
    }
}

impl PgStore {
    pub async fn ms_open(&self, c: &MsConsume) -> StoreResult<bool> {
        // The partial unique index on the purchase turns a second row for
        // the same purchase (pending or done) into "nothing inserted". The
        // opener sends the first request at once: it is counted, and the
        // row is leased to the opener.
        let n = query(
            "INSERT INTO msstore_consumes (tracking_id, user_id, product_id, pack, coins,
                 transaction_id, store_key, attempts, leased_until, created_at, next_try_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, 1, $8, $9, $10) ON CONFLICT DO NOTHING",
        )
        .bind(&c.tracking_id)
        .bind(c.user_id)
        .bind(&c.product_id)
        .bind(&c.pack)
        .bind(c.coins)
        .bind(&c.transaction_id)
        .bind(&c.store_key)
        .bind(c.created_at + MS_LEASE)
        .bind(c.created_at)
        .bind(c.created_at + MS_FIRST_RETRY)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(n > 0)
    }

    pub async fn ms_lease(&self, tracking_id: &str, now: i64) -> StoreResult<Option<MsConsume>> {
        let row = query(&format!(
            "UPDATE msstore_consumes SET attempts = attempts + 1, leased_until = $2
             WHERE tracking_id = $1 AND state = 'pending' AND leased_until <= $3
             RETURNING {PENDING_COLUMNS}"
        ))
        .bind(tracking_id)
        .bind(now + MS_LEASE)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.as_ref().map(consume_row))
    }

    pub async fn ms_pending_of(&self, user_id: i64) -> StoreResult<Vec<MsConsume>> {
        let rows = query(&format!(
            "SELECT {PENDING_COLUMNS} FROM msstore_consumes
             WHERE user_id = $1 AND state = 'pending' ORDER BY created_at"
        ))
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(consume_row).collect())
    }

    pub async fn ms_due(&self, now: i64, limit: u32) -> StoreResult<Vec<MsConsume>> {
        let rows = query(&format!(
            "SELECT {PENDING_COLUMNS} FROM msstore_consumes
             WHERE state = 'pending' AND next_try_at <= $1 AND leased_until <= $1
             ORDER BY next_try_at LIMIT $2"
        ))
        .bind(now)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(consume_row).collect())
    }

    pub async fn ms_fulfil(
        &self,
        tracking_id: &str,
        order_ids: &str,
        at: i64,
    ) -> StoreResult<bool> {
        let mut tx = self.pool.begin().await?;
        // Microsoft confirmed this very request: it is credited whatever the
        // row came to say meanwhile (an answer can arrive late), but only
        // once (the row lock makes a second server wait here, then find it
        // done).
        let row = query(
            "UPDATE msstore_consumes SET state = 'done', store_key = NULL, settled_at = $2,
                 order_ids = $3, last_error = NULL, leased_until = 0
             WHERE tracking_id = $1 AND state IN ('pending', 'failed', 'stuck')
             RETURNING user_id, pack, coins",
        )
        .bind(tracking_id)
        .bind(at)
        .bind(order_ids)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(false);
        };
        let user_id: i64 = row.get("user_id");
        let pack: String = row.get("pack");
        let coins: i64 = row.get("coins");
        let reference = ms_payment_id(tracking_id);
        query(
            "INSERT INTO payments (id, user_id, kind, coins, amount, currency, at, refunded, email, test)
             VALUES ($1, $2, 'coins', $3, 0, $4, $5, FALSE, NULL, FALSE) ON CONFLICT DO NOTHING",
        )
        .bind(&reference)
        .bind(user_id)
        .bind(coins)
        .bind(MS_CURRENCY)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        let credited = query(
            "INSERT INTO coin_ledger (user_id, delta, reason, reference, at) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (reference) DO NOTHING",
        )
        .bind(user_id)
        .bind(coins)
        .bind(format!("pack {pack} (Microsoft Store)"))
        .bind(&reference)
        .bind(at)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if credited > 0 {
            query(
                "INSERT INTO wallets (user_id, coins) VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE SET coins = wallets.coins + EXCLUDED.coins",
            )
            .bind(user_id)
            .bind(coins)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(credited > 0)
    }

    pub async fn ms_retry_later(
        &self,
        tracking_id: &str,
        error: &str,
        next_try_at: i64,
    ) -> StoreResult<()> {
        query(
            "UPDATE msstore_consumes SET last_error = $2, next_try_at = $3, leased_until = 0
             WHERE tracking_id = $1 AND state = 'pending'",
        )
        .bind(tracking_id)
        .bind(error)
        .bind(next_try_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn ms_close(
        &self,
        tracking_id: &str,
        state: MsState,
        error: &str,
        at: i64,
    ) -> StoreResult<bool> {
        // A stuck row keeps its key: the staff can send it again.
        let n = query(
            "UPDATE msstore_consumes SET state = $2, last_error = $3, settled_at = $4,
                 leased_until = 0,
                 store_key = CASE WHEN $2 = 'stuck' THEN store_key ELSE NULL END
             WHERE tracking_id = $1 AND state = 'pending'",
        )
        .bind(tracking_id)
        .bind(state.code())
        .bind(error)
        .bind(at)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(n > 0)
    }

    pub async fn ms_reopen(&self, tracking_id: &str, now: i64) -> StoreResult<bool> {
        let n = query(
            "UPDATE msstore_consumes SET state = 'pending', settled_at = NULL, next_try_at = $2,
                 leased_until = 0
             WHERE tracking_id = $1 AND state = 'stuck' AND store_key IS NOT NULL",
        )
        .bind(tracking_id)
        .bind(now)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(n > 0)
    }

    pub async fn ms_consumes_of(&self, user_id: i64, limit: u32) -> StoreResult<Vec<MsRecord>> {
        let rows = query(
            "SELECT tracking_id, pack, coins, transaction_id, state, attempts, last_error,
                 order_ids, created_at, settled_at
             FROM msstore_consumes WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| MsRecord {
                tracking_id: r.get("tracking_id"),
                pack: r.get("pack"),
                coins: r.get("coins"),
                transaction_id: r.get("transaction_id"),
                state: r.get("state"),
                attempts: r.get("attempts"),
                last_error: r.get("last_error"),
                order_ids: r.get("order_ids"),
                created_at: r.get("created_at"),
                settled_at: r.get("settled_at"),
            })
            .collect())
    }
}
