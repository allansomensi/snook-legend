//! The collection in memory: when and how each owned item was got (the same
//! rules as the database: see `store/postgres/collection.rs`, migration
//! 0029).

use super::super::{OwnedRow, StoreResult};
use super::MemoryStore;

/// What the memory store keeps of an owned item besides its code: when it
/// was got, how it was added (`source`) and its recorded origin (JSON).
#[derive(Debug, Clone, Default)]
pub(super) struct Acquired {
    pub(super) at: i64,
    pub(super) source: String,
    pub(super) origin: Option<String>,
}

impl MemoryStore {
    pub fn owned_items(&self, user_id: i64) -> StoreResult<Vec<OwnedRow>> {
        let s = self.lock();
        let Some(w) = s.wallets.get(&user_id) else { return Ok(Vec::new()) };
        let mut rows: Vec<OwnedRow> = w
            .owned
            .iter()
            .map(|code| {
                let a = s.acquired.get(&(user_id, code.clone())).cloned().unwrap_or_default();
                let giver = a
                    .source
                    .strip_prefix("gift:")
                    .and_then(|id| id.parse::<i64>().ok())
                    .and_then(|id| s.users.get(&id))
                    .map(|u| u.username.clone());
                OwnedRow {
                    code: code.clone(),
                    acquired_at: a.at,
                    source: a.source,
                    origin: a.origin,
                    giver,
                }
            })
            .collect();
        rows.sort_by_key(|r| r.acquired_at);
        Ok(rows)
    }

    pub fn record_origin(
        &self,
        user_id: i64,
        code: &str,
        origin: &str,
        at: i64,
    ) -> StoreResult<bool> {
        let mut s = self.lock();
        let w = s.wallets.entry(user_id).or_default();
        let new = !w.owned.iter().any(|o| o == code);
        if new {
            w.owned.push(code.to_string());
        }
        let a = s.acquired.entry((user_id, code.to_string())).or_insert_with(|| Acquired {
            at,
            source: "origin".into(),
            origin: None,
        });
        // The first origin stays.
        if a.origin.is_none() {
            a.origin = Some(origin.to_string());
        }
        Ok(new)
    }
}

/// Notes when and how an item was added (buying, a gift, a grant).
pub(super) fn acquired(at: i64, source: &str) -> Acquired {
    Acquired { at, source: source.to_string(), origin: None }
}
