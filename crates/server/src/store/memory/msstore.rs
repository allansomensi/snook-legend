//! Microsoft Store purchases in memory (the same rules as the database:
//! see `store/postgres/msstore.rs` and migration 0011).

use super::super::{
    MS_CURRENCY, MS_FIRST_RETRY, MS_LEASE, MsConsume, MsRecord, MsState, Payment, StoreError,
    StoreResult, ms_payment_id,
};
use super::MemoryStore;

/// A row of `msstore_consumes`.
#[derive(Debug, Clone)]
pub(super) struct MsRow {
    consume: MsConsume,
    state: &'static str,
    last_error: Option<String>,
    order_ids: Option<String>,
    next_try_at: i64,
    leased_until: i64,
    settled_at: Option<i64>,
}

impl MsRow {
    /// Counted by the unique index on the purchase.
    fn holds_purchase(&self) -> bool {
        matches!(self.state, "pending" | "done")
    }
}

impl MemoryStore {
    pub fn ms_open(&self, c: &MsConsume) -> StoreResult<bool> {
        let mut s = self.lock();
        let taken = s.ms_consumes.iter().any(|r| {
            r.consume.tracking_id == c.tracking_id
                || (r.consume.transaction_id == c.transaction_id && r.holds_purchase())
        });
        if taken {
            return Ok(false);
        }
        s.ms_consumes.push(MsRow {
            consume: MsConsume { attempts: 1, ..c.clone() },
            state: "pending",
            last_error: None,
            order_ids: None,
            next_try_at: c.created_at + MS_FIRST_RETRY,
            leased_until: c.created_at + MS_LEASE,
            settled_at: None,
        });
        Ok(true)
    }

    pub fn ms_lease(&self, tracking_id: &str, now: i64) -> StoreResult<Option<MsConsume>> {
        let mut s = self.lock();
        let Some(row) = s.ms_consumes.iter_mut().find(|r| {
            r.consume.tracking_id == tracking_id && r.state == "pending" && r.leased_until <= now
        }) else {
            return Ok(None);
        };
        row.consume.attempts += 1;
        row.leased_until = now + MS_LEASE;
        Ok(Some(row.consume.clone()))
    }

    pub fn ms_pending_of(&self, user_id: i64) -> StoreResult<Vec<MsConsume>> {
        let s = self.lock();
        let mut rows: Vec<MsConsume> = s
            .ms_consumes
            .iter()
            .filter(|r| r.consume.user_id == user_id && r.state == "pending")
            .map(|r| r.consume.clone())
            .collect();
        rows.sort_by_key(|c| c.created_at);
        Ok(rows)
    }

    pub fn ms_due(&self, now: i64, limit: u32) -> StoreResult<Vec<MsConsume>> {
        let s = self.lock();
        let mut rows: Vec<&MsRow> = s
            .ms_consumes
            .iter()
            .filter(|r| r.state == "pending" && r.next_try_at <= now && r.leased_until <= now)
            .collect();
        rows.sort_by_key(|r| r.next_try_at);
        Ok(rows.into_iter().take(limit as usize).map(|r| r.consume.clone()).collect())
    }

    pub fn ms_fulfil(&self, tracking_id: &str, order_ids: &str, at: i64) -> StoreResult<bool> {
        let mut s = self.lock();
        let Some(i) = s.ms_consumes.iter().position(|r| {
            r.consume.tracking_id == tracking_id
                && matches!(r.state, "pending" | "failed" | "stuck")
        }) else {
            return Ok(false);
        };
        // The unique index: another row may hold the purchase already.
        let transaction = s.ms_consumes[i].consume.transaction_id.clone();
        let clash =
            s.ms_consumes.iter().enumerate().any(|(k, r)| {
                k != i && r.consume.transaction_id == transaction && r.holds_purchase()
            });
        if clash {
            return Err(StoreError("another row holds this purchase".into()));
        }
        let row = &mut s.ms_consumes[i];
        row.state = "done";
        row.consume.store_key.clear();
        row.settled_at = Some(at);
        row.order_ids = Some(order_ids.to_string());
        row.last_error = None;
        row.leased_until = 0;
        let c = row.consume.clone();
        let reference = ms_payment_id(tracking_id);
        if !s.payments.iter().any(|p| p.id == reference) {
            s.payments.push(Payment {
                id: reference.clone(),
                user_id: c.user_id,
                kind: "coins".into(),
                coins: c.coins,
                amount: 0,
                currency: MS_CURRENCY.into(),
                at,
                refunded: false,
                email: None,
                test: false,
            });
        }
        if s.ledger.iter().any(|l| l.3.as_deref() == Some(reference.as_str())) {
            return Ok(false);
        }
        s.wallets.entry(c.user_id).or_default().coins += c.coins;
        let reason = format!("pack {} (Microsoft Store)", c.pack);
        s.ledger.push((c.user_id, c.coins, reason, Some(reference), at));
        Ok(true)
    }

    pub fn ms_retry_later(
        &self,
        tracking_id: &str,
        error: &str,
        next_try_at: i64,
    ) -> StoreResult<()> {
        let mut s = self.lock();
        if let Some(row) = s
            .ms_consumes
            .iter_mut()
            .find(|r| r.consume.tracking_id == tracking_id && r.state == "pending")
        {
            row.last_error = Some(error.to_string());
            row.next_try_at = next_try_at;
            row.leased_until = 0;
        }
        Ok(())
    }

    pub fn ms_close(
        &self,
        tracking_id: &str,
        state: MsState,
        error: &str,
        at: i64,
    ) -> StoreResult<bool> {
        let mut s = self.lock();
        let Some(row) = s
            .ms_consumes
            .iter_mut()
            .find(|r| r.consume.tracking_id == tracking_id && r.state == "pending")
        else {
            return Ok(false);
        };
        row.state = state.code();
        row.last_error = Some(error.to_string());
        row.settled_at = Some(at);
        row.leased_until = 0;
        // A stuck row keeps its key: the staff can send it again.
        if state != MsState::Stuck {
            row.consume.store_key.clear();
        }
        Ok(true)
    }

    pub fn ms_reopen(&self, tracking_id: &str, now: i64) -> StoreResult<bool> {
        let mut s = self.lock();
        let Some(i) = s.ms_consumes.iter().position(|r| {
            r.consume.tracking_id == tracking_id
                && r.state == "stuck"
                && !r.consume.store_key.is_empty()
        }) else {
            return Ok(false);
        };
        let transaction = s.ms_consumes[i].consume.transaction_id.clone();
        let clash = s
            .ms_consumes
            .iter()
            .any(|r| r.consume.transaction_id == transaction && r.holds_purchase());
        if clash {
            return Err(StoreError("another row holds this purchase".into()));
        }
        let row = &mut s.ms_consumes[i];
        row.state = "pending";
        row.settled_at = None;
        row.next_try_at = now;
        row.leased_until = 0;
        Ok(true)
    }

    pub fn ms_consumes_of(&self, user_id: i64, limit: u32) -> StoreResult<Vec<MsRecord>> {
        let s = self.lock();
        let mut rows: Vec<&MsRow> =
            s.ms_consumes.iter().filter(|r| r.consume.user_id == user_id).collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.consume.created_at));
        Ok(rows
            .into_iter()
            .take(limit as usize)
            .map(|r| MsRecord {
                tracking_id: r.consume.tracking_id.clone(),
                pack: r.consume.pack.clone(),
                coins: r.consume.coins,
                transaction_id: r.consume.transaction_id.clone(),
                state: r.state.to_string(),
                attempts: r.consume.attempts,
                last_error: r.last_error.clone(),
                order_ids: r.order_ids.clone(),
                created_at: r.consume.created_at,
                settled_at: r.settled_at,
            })
            .collect())
    }
}
