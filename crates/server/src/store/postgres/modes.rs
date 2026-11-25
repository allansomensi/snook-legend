//! Doubles and against the clock in PostgreSQL (migration 0031; see
//! `store/modes.rs`).

use snook_legend_proto::{LeaderboardEntry, PerRules, Role, Rules};
use sqlx::{Row, query};

use super::super::{
    AttemptFinish, AttemptRow, BestRow, DOUBLES_START, DoublesRecord, Record, StoreResult,
};
use super::PgStore;

/// A count stored as INTEGER.
fn int(n: u32) -> i32 {
    n.min(i32::MAX as u32) as i32
}

impl PgStore {
    pub async fn doubles_records(&self, user_id: i64) -> StoreResult<PerRules<Record>> {
        let rows = query(
            "SELECT rules, rating, peak_rating, games, wins FROM doubles_ratings WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        let mut out = PerRules::both(Record::new(DOUBLES_START));
        for r in rows {
            let Some(rules) = Rules::parse(r.get("rules")) else { continue };
            *out.get_mut(rules) = Record {
                rating: r.get("rating"),
                peak_rating: r.get("peak_rating"),
                games: r.get::<i32, _>("games").max(0) as u32,
                wins: r.get::<i32, _>("wins").max(0) as u32,
            };
        }
        Ok(out)
    }

    pub async fn record_doubles(&self, m: &DoublesRecord) -> StoreResult<bool> {
        let mut tx = self.pool.begin().await?;
        if let Some(room) = m.room {
            let stored = query("SELECT 1 AS one FROM doubles_matches WHERE room = $1")
                .bind(room)
                .fetch_optional(&mut *tx)
                .await?;
            if stored.is_some() {
                return Ok(false);
            }
        }
        let replay = match &m.replay {
            Some(r) => Some(
                serde_json::to_string(r).map_err(|e| super::super::StoreError(e.to_string()))?,
            ),
            None => None,
        };
        let deltas: Vec<i32> = m.deltas.iter().flatten().copied().collect();
        query(
            "INSERT INTO doubles_matches
                (room, rules, rated, a1, a2, b1, b2, winner, reason, deltas, shots, ended_at, replay)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13::jsonb)",
        )
        .bind(m.room)
        .bind(m.rules.code())
        .bind(m.rated)
        .bind(m.players[0][0])
        .bind(m.players[0][1])
        .bind(m.players[1][0])
        .bind(m.players[1][1])
        .bind(m.winner as i16)
        .bind(crate::reason_code(m.reason))
        .bind(&deltas)
        .bind(int(m.shots))
        .bind(m.ended_at)
        .bind(replay)
        .execute(&mut *tx)
        .await?;
        if m.rated {
            for (team, players) in m.players.iter().enumerate() {
                for (k, &id) in players.iter().enumerate() {
                    let delta = m.deltas[team][k];
                    let win = i32::from(team == m.winner);
                    query(
                        "INSERT INTO doubles_ratings (user_id, rules, rating, peak_rating, games, wins)
                         VALUES ($1, $2, $3 + $4, GREATEST($3, $3 + $4), 1, $5)
                         ON CONFLICT (user_id, rules) DO UPDATE SET
                            rating = doubles_ratings.rating + $4,
                            peak_rating = GREATEST(doubles_ratings.peak_rating, doubles_ratings.rating + $4),
                            games = doubles_ratings.games + 1,
                            wins = doubles_ratings.wins + $5",
                    )
                    .bind(id)
                    .bind(m.rules.code())
                    .bind(DOUBLES_START)
                    .bind(delta)
                    .bind(win)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
        tx.commit().await?;
        Ok(true)
    }

    pub async fn doubles_leaderboard(
        &self,
        rules: Rules,
        limit: u32,
        now: i64,
    ) -> StoreResult<Vec<LeaderboardEntry>> {
        let rows = query(
            "SELECT u.username, d.rating, d.games, d.wins, u.country, u.role,
                    COALESCE(w.vip_until, 0) > $3 AS vip
             FROM doubles_ratings d JOIN users u ON u.id = d.user_id
                  LEFT JOIN wallets w ON w.user_id = u.id
             WHERE d.rules = $1 AND d.games > 0
               AND (u.banned_until IS NULL OR u.banned_until <= $3) AND u.deleted_at IS NULL
             ORDER BY d.rating DESC, d.wins DESC, u.username COLLATE \"C\" ASC LIMIT $2",
        )
        .bind(rules.code())
        .bind(limit as i64)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .enumerate()
            .map(|(i, r)| LeaderboardEntry {
                rank: i as u32 + 1,
                username: r.get("username"),
                rating: r.get("rating"),
                games: r.get::<i32, _>("games").max(0) as u32,
                wins: r.get::<i32, _>("wins").max(0) as u32,
                country: r.get("country"),
                vip: r.get("vip"),
                admin: Role::from_i16(r.get("role")) == Role::Admin,
            })
            .collect())
    }

    pub async fn time_attack_start(
        &self,
        user_id: i64,
        day: i64,
        now_ms: i64,
        seed: u64,
        max: u32,
    ) -> StoreResult<Option<i64>> {
        let mut tx = self.pool.begin().await?;
        // One start at a time per player (two at once can't both take the
        // last attempt).
        query("SELECT pg_advisory_xact_lock(31, $1::int)")
            .bind((user_id % i32::MAX as i64) as i32)
            .execute(&mut *tx)
            .await?;
        let used: i64 =
            query("SELECT COUNT(*) AS n FROM time_attack_attempts WHERE user_id = $1 AND day = $2")
                .bind(user_id)
                .bind(day)
                .fetch_one(&mut *tx)
                .await?
                .get("n");
        if used >= max as i64 {
            return Ok(None);
        }
        // One attempt at a time: the one under way is given up.
        query(
            "UPDATE time_attack_attempts SET finished_ms = $2
             WHERE user_id = $1 AND finished_ms IS NULL",
        )
        .bind(user_id)
        .bind(now_ms)
        .execute(&mut *tx)
        .await?;
        let id: i64 = query(
            "INSERT INTO time_attack_attempts (user_id, day, seed, started_ms)
             VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(user_id)
        .bind(day)
        .bind(seed as i64)
        .bind(now_ms)
        .fetch_one(&mut *tx)
        .await?
        .get("id");
        tx.commit().await?;
        Ok(Some(id))
    }

    pub async fn time_attack_open(&self, user_id: i64) -> StoreResult<Option<AttemptRow>> {
        let row = query(
            "SELECT id, seed, started_ms FROM time_attack_attempts
             WHERE user_id = $1 AND finished_ms IS NULL ORDER BY id DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| AttemptRow {
            id: r.get("id"),
            seed: r.get::<i64, _>("seed") as u64,
            started_ms: r.get("started_ms"),
        }))
    }

    pub async fn time_attack_attempts(&self, user_id: i64, day: i64) -> StoreResult<u32> {
        let n: i64 =
            query("SELECT COUNT(*) AS n FROM time_attack_attempts WHERE user_id = $1 AND day = $2")
                .bind(user_id)
                .bind(day)
                .fetch_one(&self.pool)
                .await?
                .get("n");
        Ok(n.max(0) as u32)
    }

    pub async fn time_attack_finish(
        &self,
        user_id: i64,
        id: i64,
        now_ms: i64,
        week: i64,
        at: i64,
        result: Option<(u32, u32, u32)>,
    ) -> StoreResult<AttemptFinish> {
        let mut tx = self.pool.begin().await?;
        let (ms, shots, fouls) = match result {
            Some((ms, shots, fouls)) => (Some(int(ms)), Some(int(shots)), Some(int(fouls))),
            None => (None, None, None),
        };
        let done = query(
            "UPDATE time_attack_attempts SET finished_ms = $3, ms = $4, shots = $5, fouls = $6
             WHERE id = $1 AND user_id = $2 AND finished_ms IS NULL",
        )
        .bind(id)
        .bind(user_id)
        .bind(now_ms)
        .bind(ms)
        .bind(shots)
        .bind(fouls)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if done == 0 {
            return Ok(AttemptFinish::NotStarted);
        }
        let Some(ms) = ms else {
            tx.commit().await?;
            return Ok(AttemptFinish::Counted { best: false });
        };
        let best = query(
            "INSERT INTO time_attack_bests (week, user_id, ms, at) VALUES ($1, $2, $3, $4)
             ON CONFLICT (week, user_id) DO UPDATE SET ms = $3, at = $4
                WHERE time_attack_bests.ms > $3
             RETURNING ms",
        )
        .bind(week)
        .bind(user_id)
        .bind(ms)
        .bind(at)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
        tx.commit().await?;
        Ok(AttemptFinish::Counted { best })
    }

    pub async fn time_attack_board(
        &self,
        week: i64,
        limit: u32,
    ) -> StoreResult<(Vec<BestRow>, u32)> {
        let now = crate::now();
        let rows = query(
            "SELECT b.user_id, u.username, u.country, b.ms, b.at
             FROM time_attack_bests b JOIN users u ON u.id = b.user_id
             WHERE b.week = $1 AND u.deleted_at IS NULL
               AND (u.banned_until IS NULL OR u.banned_until <= $3)
             ORDER BY b.ms, b.at, b.user_id LIMIT $2",
        )
        .bind(week)
        .bind(limit as i64)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;
        let players: i64 = query(
            "SELECT COUNT(*) AS n FROM time_attack_bests b JOIN users u ON u.id = b.user_id
             WHERE b.week = $1 AND u.deleted_at IS NULL
               AND (u.banned_until IS NULL OR u.banned_until <= $2)",
        )
        .bind(week)
        .bind(now)
        .fetch_one(&self.pool)
        .await?
        .get("n");
        let board = rows
            .iter()
            .map(|r| BestRow {
                user_id: r.get("user_id"),
                username: r.get("username"),
                country: r.get("country"),
                ms: r.get::<i32, _>("ms").max(0) as u32,
                at: r.get("at"),
            })
            .collect();
        Ok((board, players.max(0) as u32))
    }

    pub async fn time_attack_best(
        &self,
        user_id: i64,
        week: i64,
    ) -> StoreResult<Option<(u32, Option<u32>)>> {
        let now = crate::now();
        let row = query(
            "SELECT b.ms,
                    CASE WHEN u.deleted_at IS NULL
                              AND (u.banned_until IS NULL OR u.banned_until <= $3)
                    THEN (SELECT COUNT(*) FROM time_attack_bests o JOIN users ou ON ou.id = o.user_id
                          WHERE o.week = $2 AND ou.deleted_at IS NULL
                            AND (ou.banned_until IS NULL OR ou.banned_until <= $3)
                            AND (o.ms < b.ms OR (o.ms = b.ms AND (o.at < b.at
                                 OR (o.at = b.at AND o.user_id < b.user_id))))) + 1
                    END AS rank
             FROM time_attack_bests b JOIN users u ON u.id = b.user_id
             WHERE b.week = $2 AND b.user_id = $1",
        )
        .bind(user_id)
        .bind(week)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            let rank: Option<i64> = r.get("rank");
            (r.get::<i32, _>("ms").max(0) as u32, rank.map(|n| n.max(1) as u32))
        }))
    }

    pub async fn time_attack_prize(
        &self,
        week: i64,
        place: u32,
        user_id: i64,
        coins: i64,
        ms: u32,
        at: i64,
    ) -> StoreResult<bool> {
        let done = query(
            "INSERT INTO time_attack_prizes (week, place, user_id, coins, ms, paid_at)
             VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING",
        )
        .bind(week)
        .bind(place as i16)
        .bind(user_id)
        .bind(coins.clamp(0, i32::MAX as i64) as i32)
        .bind(int(ms))
        .bind(at)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(done > 0)
    }
}
