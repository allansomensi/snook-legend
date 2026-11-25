//! Doubles and against the clock in PostgreSQL (migration 0031; see
//! `store/modes.rs`).

use snook_legend_proto::{LeaderboardEntry, PerRules, Role, Rules};
use sqlx::{Row, query};

use super::super::{
    AttemptFinish, AttemptRow, BestRow, DOUBLES_START, DoublesGame, DoublesRecord, Record,
    StoreResult, TimeAttempt,
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
        // Its copy saved for a restart is done with.
        if let Some(room) = m.room {
            query("DELETE FROM live_matches WHERE id = $1").bind(room).execute(&mut *tx).await?;
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
            "UPDATE time_attack_attempts SET finished_ms = $3, ms = $4, shots = $5, fouls = $6,
                    week = CASE WHEN $4::INTEGER IS NULL THEN NULL ELSE $7::BIGINT END,
                    at = CASE WHEN $4::INTEGER IS NULL THEN NULL ELSE $8::BIGINT END
             WHERE id = $1 AND user_id = $2 AND finished_ms IS NULL",
        )
        .bind(id)
        .bind(user_id)
        .bind(now_ms)
        .bind(ms)
        .bind(shots)
        .bind(fouls)
        .bind(week)
        .bind(at)
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

    pub async fn doubles_games(
        &self,
        player: Option<i64>,
        before: Option<i64>,
        limit: u32,
    ) -> StoreResult<Vec<DoublesGame>> {
        let rows = query(&format!(
            "{DOUBLES_SELECT}, NULL::TEXT AS replay_text
             FROM doubles_matches m {DOUBLES_JOIN}
             WHERE ($1::BIGINT IS NULL OR $1 IN (m.a1, m.a2, m.b1, m.b2))
               AND ($2::BIGINT IS NULL OR m.id < $2)
             ORDER BY m.id DESC LIMIT $3"
        ))
        .bind(player)
        .bind(before)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().filter_map(doubles_game).collect())
    }

    pub async fn doubles_game(&self, id: i64) -> StoreResult<Option<DoublesGame>> {
        let row = query(&format!(
            "{DOUBLES_SELECT}, m.replay::TEXT AS replay_text
             FROM doubles_matches m {DOUBLES_JOIN} WHERE m.id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.as_ref().and_then(doubles_game))
    }

    pub async fn void_doubles(&self, id: i64, at: i64) -> StoreResult<bool> {
        let mut tx = self.pool.begin().await?;
        let row = query(
            "UPDATE doubles_matches SET voided_at = $2
             WHERE id = $1 AND rated AND voided_at IS NULL
             RETURNING rules, a1, a2, b1, b2, winner, deltas",
        )
        .bind(id)
        .bind(at)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else { return Ok(false) };
        let rules: String = row.get("rules");
        let winner = row.get::<i16, _>("winner") as usize;
        let deltas: Vec<i32> = row.get("deltas");
        let players = [row.get::<i64, _>("a1"), row.get("a2"), row.get("b1"), row.get("b2")];
        for (n, user) in players.into_iter().enumerate() {
            let delta = deltas.get(n).copied().unwrap_or(0);
            let win = i32::from(n / 2 == winner);
            query(
                "UPDATE doubles_ratings SET
                    rating = rating - $3,
                    peak_rating = GREATEST(rating - $3,
                        CASE WHEN $3 > 0 AND peak_rating = rating
                             THEN GREATEST(rating - $3, $5) ELSE peak_rating END),
                    games = GREATEST(games - 1, 0),
                    wins = GREATEST(wins - $4, 0)
                 WHERE user_id = $1 AND rules = $2",
            )
            .bind(user)
            .bind(&rules)
            .bind(delta)
            .bind(win)
            .bind(DOUBLES_START)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    pub async fn time_attack_user_attempts(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<TimeAttempt>> {
        let rows =
            query(&format!("{ATTEMPT_SELECT} WHERE a.user_id = $1 ORDER BY a.id DESC LIMIT $2"))
                .bind(user_id)
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.iter().map(attempt).collect())
    }

    pub async fn time_attack_remove(
        &self,
        id: i64,
        reason: &str,
        at: i64,
    ) -> StoreResult<Option<TimeAttempt>> {
        let mut tx = self.pool.begin().await?;
        let row = query(
            "UPDATE time_attack_attempts SET removed_at = $2, removed_reason = $3
             WHERE id = $1 AND removed_at IS NULL AND week IS NOT NULL AND ms IS NOT NULL
             RETURNING user_id, week",
        )
        .bind(id)
        .bind(at)
        .bind(reason)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let (user, week): (i64, i64) = (row.get("user_id"), row.get("week"));
        // The best of the week from the attempts left (the earlier on a tie).
        query("DELETE FROM time_attack_bests WHERE week = $1 AND user_id = $2")
            .bind(week)
            .bind(user)
            .execute(&mut *tx)
            .await?;
        query(
            "INSERT INTO time_attack_bests (week, user_id, ms, at)
             SELECT week, user_id, ms, at FROM time_attack_attempts
             WHERE week = $1 AND user_id = $2 AND ms IS NOT NULL AND removed_at IS NULL
             ORDER BY ms, at LIMIT 1",
        )
        .bind(week)
        .bind(user)
        .execute(&mut *tx)
        .await?;
        let row = query(&format!("{ATTEMPT_SELECT} WHERE a.id = $1"))
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(attempt(&row)))
    }
}

const DOUBLES_SELECT: &str = "SELECT m.id, m.room, m.rules, m.rated, m.a1, m.a2, m.b1, m.b2,
        m.winner, m.reason, m.deltas, m.shots, m.ended_at, m.voided_at,
        m.replay IS NOT NULL AS has_replay,
        ua1.username AS na1, ua2.username AS na2, ub1.username AS nb1, ub2.username AS nb2,
        ua1.country AS ca1, ua2.country AS ca2, ub1.country AS cb1, ub2.country AS cb2";
const DOUBLES_JOIN: &str = "JOIN users ua1 ON ua1.id = m.a1 JOIN users ua2 ON ua2.id = m.a2
        JOIN users ub1 ON ub1.id = m.b1 JOIN users ub2 ON ub2.id = m.b2";

fn doubles_game(r: &sqlx::postgres::PgRow) -> Option<DoublesGame> {
    let deltas: Vec<i32> = r.get("deltas");
    let d = |n: usize| deltas.get(n).copied().unwrap_or(0);
    let replay_text: Option<String> = r.get("replay_text");
    let replay = replay_text.and_then(|t| serde_json::from_str(&t).ok());
    let s = |c: &str| r.get::<String, _>(c);
    Some(DoublesGame {
        id: r.get("id"),
        record: DoublesRecord {
            players: [[r.get("a1"), r.get("a2")], [r.get("b1"), r.get("b2")]],
            winner: r.get::<i16, _>("winner") as usize,
            reason: crate::parse_reason(r.get("reason")),
            rated: r.get("rated"),
            rules: Rules::parse(r.get("rules"))?,
            deltas: [[d(0), d(1)], [d(2), d(3)]],
            shots: r.get::<i32, _>("shots").max(0) as u32,
            ended_at: r.get("ended_at"),
            replay,
            room: r.get("room"),
        },
        names: [[s("na1"), s("na2")], [s("nb1"), s("nb2")]],
        countries: [[s("ca1"), s("ca2")], [s("cb1"), s("cb2")]],
        has_replay: r.get("has_replay"),
        voided_at: r.get("voided_at"),
    })
}

const ATTEMPT_SELECT: &str = "SELECT a.id, a.user_id, u.username, a.day, a.started_ms,
        a.finished_ms, a.ms, a.shots, a.fouls, a.week, a.at, a.removed_at, a.removed_reason
        FROM time_attack_attempts a JOIN users u ON u.id = a.user_id";

fn attempt(r: &sqlx::postgres::PgRow) -> TimeAttempt {
    let n = |c: &str| r.get::<Option<i32>, _>(c).map(|v| v.max(0) as u32);
    TimeAttempt {
        id: r.get("id"),
        user_id: r.get("user_id"),
        username: r.get("username"),
        day: r.get("day"),
        started_ms: r.get("started_ms"),
        finished_ms: r.get("finished_ms"),
        ms: n("ms"),
        shots: n("shots"),
        fouls: n("fouls"),
        week: r.get("week"),
        at: r.get("at"),
        removed_at: r.get("removed_at"),
        removed_reason: r.get("removed_reason"),
    }
}
