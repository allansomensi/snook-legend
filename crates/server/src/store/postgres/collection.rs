//! The collection in PostgreSQL: `owned_items` with the origin of each item
//! (migration 0029). See `crate::collection`.

use sqlx::{Row, query};

use super::super::{OwnedRow, StoreResult};
use super::PgStore;

impl PgStore {
    pub async fn owned_items(&self, user_id: i64) -> StoreResult<Vec<OwnedRow>> {
        let rows = query(
            "SELECT o.code, o.acquired_at, o.source, o.origin, u.username AS giver
             FROM owned_items o
             LEFT JOIN users u ON o.source = 'gift:' || u.id::text
             WHERE o.user_id = $1
             ORDER BY o.acquired_at, o.code",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| OwnedRow {
                code: r.get("code"),
                acquired_at: r.get("acquired_at"),
                source: r.get("source"),
                origin: r.get("origin"),
                giver: r.get("giver"),
            })
            .collect())
    }

    pub async fn record_origin(
        &self,
        user_id: i64,
        code: &str,
        origin: &str,
        at: i64,
    ) -> StoreResult<bool> {
        // (xmax = 0: the row was inserted, not updated.)
        let inserted: bool = query(
            "INSERT INTO owned_items (user_id, code, acquired_at, source, origin)
             VALUES ($1, $2, $3, 'origin', $4)
             ON CONFLICT (user_id, code)
             DO UPDATE SET origin = COALESCE(owned_items.origin, EXCLUDED.origin)
             RETURNING (xmax = 0) AS inserted",
        )
        .bind(user_id)
        .bind(code)
        .bind(at)
        .bind(origin)
        .fetch_one(&self.pool)
        .await?
        .get("inserted");
        Ok(inserted)
    }
}
