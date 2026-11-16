//! Correspondence games in PostgreSQL (tables `async_games` and
//! `async_challenges`, migration 0006).

use sqlx::postgres::PgRow;
use sqlx::{Row, query};

use super::super::{AsyncChallenge, AsyncGame, MatchRecord, NewAsyncGame, StoreResult, User};
use super::{PgStore, replay_json, replay_of};

const GAME_COLUMNS: &str = "id, player0, player1, rated, seed, breaker, strokes::text AS strokes, ruleset, \
    turn, deadline, seen0, seen1, group0, group1, left0, left1, started_at, updated_at, \
    finished_at, winner, reason, timeout, delta0, delta1, rating0, rating1, match_id";

/// How a group is stored (0: open table).
fn group_code(g: Option<snookered_core::Group>) -> i16 {
    match g {
        None => 0,
        Some(snookered_core::Group::Solids) => 1,
        Some(snookered_core::Group::Stripes) => 2,
    }
}

fn group_of_code(c: i16) -> Option<snookered_core::Group> {
    match c {
        1 => Some(snookered_core::Group::Solids),
        2 => Some(snookered_core::Group::Stripes),
        _ => None,
    }
}

const CHALLENGE_COLUMNS: &str = "id, from_id, to_id, rated, created_at";

fn game(r: &PgRow) -> Result<AsyncGame, sqlx::Error> {
    let strokes: String = r.try_get("strokes")?;
    let replay =
        replay_of(r.try_get("seed")?, r.try_get("breaker")?, &strokes, r.try_get("ruleset")?)
            .ok_or_else(|| sqlx::Error::Decode("unreadable strokes".into()))?;
    let n = |c: &str| -> Result<u32, sqlx::Error> { Ok(r.try_get::<i32, _>(c)?.max(0) as u32) };
    let small = |c: &str| -> Result<i16, sqlx::Error> { r.try_get::<i16, _>(c) };
    Ok(AsyncGame {
        id: r.try_get("id")?,
        players: [r.try_get("player0")?, r.try_get("player1")?],
        rated: r.try_get("rated")?,
        replay,
        turn: small("turn")? as usize % 2,
        deadline: r.try_get("deadline")?,
        seen: [n("seen0")?, n("seen1")?],
        groups: [group_of_code(small("group0")?), group_of_code(small("group1")?)],
        left: [small("left0")?.clamp(0, 7) as u8, small("left1")?.clamp(0, 7) as u8],
        started_at: r.try_get("started_at")?,
        updated_at: r.try_get("updated_at")?,
        finished_at: r.try_get("finished_at")?,
        winner: r.try_get::<Option<i16>, _>("winner")?.map(|w| w as usize % 2),
        reason: r.try_get("reason")?,
        timeout: r.try_get("timeout")?,
        delta: [r.try_get("delta0")?, r.try_get("delta1")?],
        ratings: [r.try_get("rating0")?, r.try_get("rating1")?],
        match_id: r.try_get("match_id")?,
    })
}

fn challenge(r: &PgRow) -> Result<AsyncChallenge, sqlx::Error> {
    Ok(AsyncChallenge {
        id: r.try_get("id")?,
        from: r.try_get("from_id")?,
        to: r.try_get("to_id")?,
        rated: r.try_get("rated")?,
        created_at: r.try_get("created_at")?,
    })
}

impl PgStore {
    pub async fn create_async_challenge(
        &self,
        from: i64,
        to: i64,
        rated: bool,
        at: i64,
    ) -> StoreResult<Option<i64>> {
        let row = query(
            "INSERT INTO async_challenges (from_id, to_id, rated, created_at) VALUES ($1, $2, $3, $4)
             ON CONFLICT (from_id, to_id) DO NOTHING RETURNING id",
        )
        .bind(from)
        .bind(to)
        .bind(rated)
        .bind(at)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get("id")))
    }

    pub async fn async_challenge(&self, id: i64) -> StoreResult<Option<AsyncChallenge>> {
        let row = query(&format!("SELECT {CHALLENGE_COLUMNS} FROM async_challenges WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| challenge(&r)).transpose()?)
    }

    pub async fn async_challenges_of(&self, user_id: i64) -> StoreResult<Vec<AsyncChallenge>> {
        let rows = query(&format!(
            "SELECT {CHALLENGE_COLUMNS} FROM async_challenges
             WHERE from_id = $1 OR to_id = $1 ORDER BY created_at, id"
        ))
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(challenge).collect::<Result<_, _>>()?)
    }

    pub async fn delete_async_challenge(&self, id: i64) -> StoreResult<bool> {
        let r = query("DELETE FROM async_challenges WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn accept_async_challenge(
        &self,
        id: i64,
        new: &NewAsyncGame,
    ) -> StoreResult<Option<AsyncGame>> {
        let mut tx = self.pool.begin().await?;
        let Some(c) = query(&format!(
            "DELETE FROM async_challenges WHERE id = $1 RETURNING {CHALLENGE_COLUMNS}"
        ))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(None);
        };
        let c = challenge(&c)?;
        let breaker = (new.breaker % 2) as i16;
        let row = query(&format!(
            "INSERT INTO async_games (player0, player1, rated, seed, breaker, turn, deadline, started_at, updated_at, ruleset)
             VALUES ($1, $2, $3, $4, $5, $5, $6, $7, $7, $8) RETURNING {GAME_COLUMNS}"
        ))
        .bind(c.from)
        .bind(c.to)
        .bind(c.rated)
        .bind(new.seed as i64)
        .bind(breaker)
        .bind(new.deadline)
        .bind(new.at)
        // New games are played under the current rules.
        .bind(snookered_core::RULESET as i16)
        .fetch_one(&mut *tx)
        .await?;
        let game = game(&row)?;
        tx.commit().await?;
        Ok(Some(game))
    }

    pub async fn purge_async_challenges(&self, before: i64) -> StoreResult<Vec<AsyncChallenge>> {
        let rows = query(&format!(
            "DELETE FROM async_challenges WHERE created_at < $1 RETURNING {CHALLENGE_COLUMNS}"
        ))
        .bind(before)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(challenge).collect::<Result<_, _>>()?)
    }

    pub async fn async_game(&self, id: i64) -> StoreResult<Option<AsyncGame>> {
        let row = query(&format!("SELECT {GAME_COLUMNS} FROM async_games WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| game(&r)).transpose()?)
    }

    pub async fn async_games_of(&self, user_id: i64, finished: u32) -> StoreResult<Vec<AsyncGame>> {
        let active = query(&format!(
            "SELECT {GAME_COLUMNS} FROM async_games
             WHERE (player0 = $1 OR player1 = $1) AND finished_at IS NULL ORDER BY id"
        ))
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        let mut list: Vec<AsyncGame> = active.iter().map(game).collect::<Result<_, _>>()?;
        if finished > 0 {
            let done = query(&format!(
                "SELECT {GAME_COLUMNS} FROM async_games
                 WHERE (player0 = $1 OR player1 = $1) AND finished_at IS NOT NULL
                 ORDER BY finished_at DESC, id DESC LIMIT $2"
            ))
            .bind(user_id)
            .bind(finished as i64)
            .fetch_all(&self.pool)
            .await?;
            for r in &done {
                list.push(game(r)?);
            }
        }
        Ok(list)
    }

    pub async fn update_async_game(&self, g: &AsyncGame, expected: usize) -> StoreResult<bool> {
        let r = query(
            "UPDATE async_games SET strokes = $2::jsonb, stroke_count = $3, turn = $4, deadline = $5,
                seen0 = GREATEST(seen0, $6), seen1 = GREATEST(seen1, $7),
                group0 = $8, group1 = $9, left0 = $10, left1 = $11, updated_at = $12
             WHERE id = $1 AND stroke_count = $13 AND finished_at IS NULL",
        )
        .bind(g.id)
        .bind(replay_json(&g.replay))
        .bind(g.replay.strokes.len() as i32)
        .bind(g.turn as i16)
        .bind(g.deadline)
        .bind(g.seen[0] as i32)
        .bind(g.seen[1] as i32)
        .bind(group_code(g.groups[0]))
        .bind(group_code(g.groups[1]))
        .bind(g.left[0] as i16)
        .bind(g.left[1] as i16)
        .bind(g.updated_at)
        .bind(expected as i32)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn set_async_seen(&self, id: i64, seat: usize, seen: u32) -> StoreResult<()> {
        let sql = if seat.is_multiple_of(2) {
            "UPDATE async_games SET seen0 = GREATEST(seen0, $2) WHERE id = $1"
        } else {
            "UPDATE async_games SET seen1 = GREATEST(seen1, $2) WHERE id = $1"
        };
        query(sql).bind(id).bind(seen as i32).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn finish_async_game(
        &self,
        g: &AsyncGame,
        expected: usize,
        record: &MatchRecord,
    ) -> StoreResult<Option<([Option<User>; 2], i64)>> {
        let mut tx = self.pool.begin().await?;
        let r = query(
            "UPDATE async_games SET strokes = $2::jsonb, stroke_count = $3, turn = $4,
                seen0 = GREATEST(seen0, $5), seen1 = GREATEST(seen1, $6),
                group0 = $7, group1 = $8, left0 = $9, left1 = $10,
                updated_at = $11, finished_at = $11, winner = $12, reason = $13, timeout = $14
             WHERE id = $1 AND stroke_count = $15 AND finished_at IS NULL",
        )
        .bind(g.id)
        .bind(replay_json(&g.replay))
        .bind(g.replay.strokes.len() as i32)
        .bind(g.turn as i16)
        .bind(g.seen[0] as i32)
        .bind(g.seen[1] as i32)
        .bind(group_code(g.groups[0]))
        .bind(group_code(g.groups[1]))
        .bind(g.left[0] as i16)
        .bind(g.left[1] as i16)
        .bind(record.ended_at)
        .bind(record.winner as i16)
        .bind(crate::reason_code(record.reason))
        .bind(g.timeout)
        .bind(expected as i32)
        .execute(&mut *tx)
        .await?;
        if r.rows_affected() == 0 {
            return Ok(None);
        }
        let (users, match_id) = Self::record_match_in(&mut tx, record).await?;
        query("UPDATE matches SET correspondence = TRUE WHERE id = $1")
            .bind(match_id)
            .execute(&mut *tx)
            .await?;
        let rating =
            |k: usize| users[k].as_ref().map(|u| u.rating).unwrap_or(record.ratings_after[k]);
        query(
            "UPDATE async_games SET match_id = $2, delta0 = $3, delta1 = $4, rating0 = $5, rating1 = $6
             WHERE id = $1",
        )
        .bind(g.id)
        .bind(match_id)
        .bind(record.delta[0])
        .bind(record.delta[1])
        .bind(rating(0))
        .bind(rating(1))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some((users, match_id)))
    }

    pub async fn expired_async_games(&self, now: i64, limit: u32) -> StoreResult<Vec<AsyncGame>> {
        let rows = query(&format!(
            "SELECT {GAME_COLUMNS} FROM async_games
             WHERE finished_at IS NULL AND deadline < $1 ORDER BY deadline, id LIMIT $2"
        ))
        .bind(now)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(game).collect::<Result<_, _>>()?)
    }
}
