//! Settings, direct messages, phrases-only chat and weekly pot attempts in
//! PostgreSQL (migrations 0030 and 0038; see `store/match_social.rs`).

use snook_legend_proto::Rules;
use snook_legend_proto::analysis::ShotAnalysis;
use sqlx::postgres::PgRow;
use sqlx::{Row, query};

use super::super::{DM_KEPT, DM_THREADS, DmRow, DmThreadRow, StoreResult, WeekRow};
use super::PgStore;

const DM_COLUMNS: &str = "id, from_id, to_id, text, at, read_at, delivered_at";

fn dm(r: &PgRow) -> DmRow {
    DmRow {
        id: r.get("id"),
        from: r.get("from_id"),
        to: r.get("to_id"),
        text: r.get("text"),
        at: r.get("at"),
        read_at: r.get("read_at"),
        delivered_at: r.get("delivered_at"),
    }
}

/// A count stored as INTEGER.
fn count(n: u32) -> i32 {
    n.min(i32::MAX as u32) as i32
}

impl PgStore {
    pub async fn account_settings(&self, user_id: i64) -> StoreResult<Option<(String, i64)>> {
        let row = query("SELECT data, updated_at FROM account_settings WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| (r.get("data"), r.get("updated_at"))))
    }

    pub async fn save_account_settings(
        &self,
        user_id: i64,
        data: &str,
        at: i64,
    ) -> StoreResult<()> {
        query(
            "INSERT INTO account_settings (user_id, data, updated_at) VALUES ($1, $2, $3)
             ON CONFLICT (user_id) DO UPDATE SET data = EXCLUDED.data, updated_at = EXCLUDED.updated_at",
        )
        .bind(user_id)
        .bind(data)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn dm_send(&self, from: i64, to: i64, text: &str, at: i64) -> StoreResult<DmRow> {
        let mut tx = self.pool.begin().await?;
        let row = query(&format!(
            "INSERT INTO direct_messages (from_id, to_id, text, at) VALUES ($1, $2, $3, $4)
             RETURNING {DM_COLUMNS}"
        ))
        .bind(from)
        .bind(to)
        .bind(text)
        .bind(at)
        .fetch_one(&mut *tx)
        .await?;
        // The conversation keeps its newest messages.
        query(
            "DELETE FROM direct_messages WHERE id IN (
                SELECT id FROM direct_messages
                WHERE LEAST(from_id, to_id) = LEAST($1::bigint, $2::bigint)
                  AND GREATEST(from_id, to_id) = GREATEST($1::bigint, $2::bigint)
                ORDER BY id DESC OFFSET $3)",
        )
        .bind(from)
        .bind(to)
        .bind(DM_KEPT as i64)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(dm(&row))
    }

    pub async fn dm_history(
        &self,
        a: i64,
        b: i64,
        before: Option<i64>,
        limit: u32,
    ) -> StoreResult<Vec<DmRow>> {
        let rows = query(&format!(
            "SELECT {DM_COLUMNS} FROM direct_messages
             WHERE LEAST(from_id, to_id) = LEAST($1::bigint, $2::bigint)
               AND GREATEST(from_id, to_id) = GREATEST($1::bigint, $2::bigint)
               AND ($3::bigint IS NULL OR id < $3)
             ORDER BY id DESC LIMIT $4"
        ))
        .bind(a)
        .bind(b)
        .bind(before)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(dm).collect())
    }

    pub async fn dm_delivered(&self, id: i64, at: i64) -> StoreResult<bool> {
        let done = query(
            "UPDATE direct_messages SET delivered_at = $2 WHERE id = $1 AND delivered_at IS NULL",
        )
        .bind(id)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    pub async fn dm_delivered_to(&self, to: i64, at: i64) -> StoreResult<Vec<(i64, i64)>> {
        let rows = query(
            "WITH done AS (
                UPDATE direct_messages SET delivered_at = $2
                WHERE to_id = $1 AND delivered_at IS NULL
                RETURNING from_id, id
             )
             SELECT from_id, MAX(id) AS up_to FROM done GROUP BY from_id",
        )
        .bind(to)
        .bind(at)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| (r.get("from_id"), r.get("up_to"))).collect())
    }

    pub async fn dm_read(&self, me: i64, with: i64, at: i64) -> StoreResult<Option<i64>> {
        let row = query(
            "WITH done AS (
                UPDATE direct_messages SET read_at = $3, delivered_at = COALESCE(delivered_at, $3)
                WHERE to_id = $1 AND from_id = $2 AND read_at IS NULL
                RETURNING id
             )
             SELECT MAX(id) FROM done",
        )
        .bind(me)
        .bind(with)
        .bind(at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get(0))
    }

    pub async fn dm_threads(&self, me: i64) -> StoreResult<Vec<DmThreadRow>> {
        let rows = query(&format!(
            "WITH mine AS (
                SELECT {DM_COLUMNS}, CASE WHEN from_id = $1 THEN to_id ELSE from_id END AS other
                FROM direct_messages WHERE from_id = $1 OR to_id = $1
             ),
             last AS (
                SELECT DISTINCT ON (other) * FROM mine ORDER BY other, id DESC
             ),
             unread AS (
                SELECT from_id AS other, COUNT(*) AS n FROM direct_messages
                WHERE to_id = $1 AND read_at IS NULL GROUP BY from_id
             )
             SELECT last.*, COALESCE(unread.n, 0) AS unread
             FROM last LEFT JOIN unread ON unread.other = last.other
             ORDER BY last.id DESC LIMIT $2"
        ))
        .bind(me)
        .bind(DM_THREADS as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| DmThreadRow {
                other: r.get("other"),
                last: dm(r),
                unread: r.get::<i64, _>("unread").clamp(0, u32::MAX as i64) as u32,
            })
            .collect())
    }

    pub async fn dm_waiting(&self, from: i64, to: i64) -> StoreResult<u32> {
        let n: i64 = query(
            "SELECT COUNT(*) FROM direct_messages
             WHERE from_id = $1 AND to_id = $2
               AND id > COALESCE(
                   (SELECT MAX(id) FROM direct_messages WHERE from_id = $2 AND to_id = $1), 0)",
        )
        .bind(from)
        .bind(to)
        .fetch_one(&self.pool)
        .await?
        .get(0);
        Ok(n.clamp(0, u32::MAX as i64) as u32)
    }

    pub async fn dm_started(&self, from: i64, since: i64) -> StoreResult<u32> {
        let n: i64 = query(
            "WITH firsts AS (
                SELECT DISTINCT ON (LEAST(from_id, to_id), GREATEST(from_id, to_id)) from_id, at
                FROM direct_messages WHERE from_id = $1 OR to_id = $1
                ORDER BY LEAST(from_id, to_id), GREATEST(from_id, to_id), id
             )
             SELECT COUNT(*) FROM firsts WHERE from_id = $1 AND at >= $2",
        )
        .bind(from)
        .bind(since)
        .fetch_one(&self.pool)
        .await?
        .get(0);
        Ok(n.clamp(0, u32::MAX as i64) as u32)
    }

    pub async fn set_phrases_only(&self, user_id: i64, on: bool) -> StoreResult<()> {
        query("UPDATE users SET chat_phrases_only = $2 WHERE id = $1")
            .bind(user_id)
            .bind(on)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_shot_week(
        &self,
        user_id: i64,
        week: i64,
        attempts: u32,
        made: u32,
    ) -> StoreResult<()> {
        query(
            "INSERT INTO shot_weeks (user_id, week, games, attempts, made) VALUES ($1, $2, 1, $3, $4)
             ON CONFLICT (user_id, week) DO UPDATE SET
                games = shot_weeks.games + 1,
                attempts = shot_weeks.attempts + EXCLUDED.attempts,
                made = shot_weeks.made + EXCLUDED.made",
        )
        .bind(user_id)
        .bind(week.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
        .bind(count(attempts))
        .bind(count(made.min(attempts)))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn shot_weeks(&self, user_id: i64, limit: u32) -> StoreResult<Vec<WeekRow>> {
        let rows = query(
            "SELECT week, games, attempts, made FROM shot_weeks WHERE user_id = $1
             ORDER BY week DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        let n = |r: &PgRow, c: &str| r.get::<i32, _>(c).max(0) as u32;
        Ok(rows
            .iter()
            .map(|r| WeekRow {
                week: r.get::<i32, _>("week") as i64,
                games: n(r, "games"),
                attempts: n(r, "attempts"),
                made: n(r, "made"),
            })
            .collect())
    }

    pub async fn league_analysis(
        &self,
        rules: Rules,
        min: i32,
        max: i32,
    ) -> StoreResult<(ShotAnalysis, u32)> {
        let (rating, games) = match rules {
            Rules::Brazilian => ("u.rating", "u.games"),
            Rules::Official => ("u.official_rating", "u.official_games"),
        };
        // Only what the advanced statistics show: games, pockets, kinds.
        let rows = query(&format!(
            "SELECT a.kind, a.key, SUM(a.attempts)::bigint AS attempts, SUM(a.made)::bigint AS made,
                    COUNT(DISTINCT a.user_id) AS players
             FROM shot_analysis a JOIN users u ON u.id = a.user_id
             WHERE u.deleted_at IS NULL AND {games} > 0 AND {rating} >= $1 AND {rating} < $2
               AND a.kind IN (0, 1, 3)
             GROUP BY a.kind, a.key"
        ))
        .bind(min)
        .bind(max)
        .fetch_all(&self.pool)
        .await?;
        let clamp = |n: i64| n.clamp(0, u32::MAX as i64) as u32;
        let mut players = 0;
        let sums = ShotAnalysis::from_rows(
            rows.iter()
                .map(|r| {
                    let kind: i16 = r.get("kind");
                    if kind == snook_legend_proto::analysis::ROW_GAMES {
                        players = clamp(r.get::<i64, _>("players"));
                    }
                    (
                        kind,
                        r.get::<i16, _>("key"),
                        clamp(r.get::<i64, _>("attempts")),
                        clamp(r.get::<i64, _>("made")),
                    )
                })
                .collect::<Vec<_>>(),
        );
        Ok((sums, players))
    }
}
