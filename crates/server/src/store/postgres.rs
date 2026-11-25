//! PostgreSQL storage (production; works with Neon, Supabase, RDS…).

use snook_legend_proto::{LeaderboardEntry, PerRules, Role, Rules};
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow};
use sqlx::{Row, query};

use snook_legend_proto::CareerNumbers;
use snook_legend_proto::replay::Replay;

mod correspondence;
mod match_social;
mod msstore;
mod ops;

use super::{
    AimRow, AuditRow, BoardNote, BuyOutcome, FriendOutcome, FriendState, GiftRow, HistoryRow,
    LedgerRow, LiveMatch, MatchRecord, Metrics, NewGift, NewNote, NewReport, NewUser, Overview,
    Payment, RECENT_SCAN, RecentRow, Record, Rename, ReportRow, SessionKind, StoreError,
    StoreResult, StoredReplay, TournamentRecord, User, Wallet, day_of, hour_of,
};
use snook_legend_proto::analysis::ShotAnalysis;

/// A present with the players' names and countries as they are now.
const GIFT_SELECT: &str = "SELECT g.id, g.from_id, COALESCE(f.username, '') AS from_name,
        COALESCE(f.country, '') AS from_country, g.to_id, t.username AS to_name,
        t.country AS to_country, g.code, g.message, g.at, g.seen, g.thanked
    FROM gifts g LEFT JOIN users f ON f.id = g.from_id JOIN users t ON t.id = g.to_id";

fn gift_row(r: &PgRow) -> Result<GiftRow, sqlx::Error> {
    Ok(GiftRow {
        id: r.try_get("id")?,
        from_id: r.try_get("from_id")?,
        from: r.try_get("from_name")?,
        from_country: r.try_get("from_country")?,
        to_id: r.try_get("to_id")?,
        to: r.try_get("to_name")?,
        to_country: r.try_get("to_country")?,
        code: r.try_get("code")?,
        message: r.try_get("message")?,
        at: r.try_get("at")?,
        seen: r.try_get("seen")?,
        thanked: r.try_get("thanked")?,
    })
}

const INSERT_GIFT: &str = "INSERT INTO gifts (from_id, to_id, code, message, at) VALUES ($1, $2, $3, $4, $5) RETURNING id";

fn replay_json(r: &Replay) -> String {
    serde_json::to_string(&r.strokes).unwrap_or_else(|_| "[]".into())
}

/// A stored game's replay. `calls`: played with the official rules' calls
/// as they were until 0.11.1 (the `calls` column), and with the 8 called as
/// since 0.12.0 (the `eight_call` column).
fn replay_of(
    seed: i64,
    breaker: i16,
    strokes: &str,
    rules: &str,
    (calls, eight_call): (bool, bool),
) -> Option<Replay> {
    Some(Replay {
        seed: seed as u64,
        breaker: breaker as usize % 2,
        strokes: serde_json::from_str(strokes).ok()?,
        rules: Rules::parse(rules)?,
        calls,
        eight_call,
    })
}

/// The calls a stored game was played with: its `calls` and `eight_call`
/// columns (see [`replay_of`]).
fn calls_of(r: &PgRow) -> (bool, bool) {
    (r.get("calls"), r.get("eight_call"))
}

/// The prefix of the `users` columns that hold a player's record under
/// `rules` (`rating`, `peak_rating`, `games`, `wins`).
fn record_columns(rules: Rules) -> &'static str {
    match rules {
        Rules::Brazilian => "",
        Rules::Official => "official_",
    }
}

pub struct PgStore {
    pool: PgPool,
}

impl From<sqlx::Error> for StoreError {
    fn from(e: sqlx::Error) -> Self {
        StoreError(e.to_string())
    }
}

impl From<sqlx::migrate::MigrateError> for StoreError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        StoreError(e.to_string())
    }
}

const USER_COLUMNS: &str = "id, username, password_hash, country, role, rating, peak_rating, \
    games, wins, official_rating, official_peak_rating, official_games, official_wins, \
    eight_wins, casual_games, streak, best_streak, runouts, shutouts, \
    tournaments_played, tournaments_won, created_at, last_seen, banned_until, ban_reason, muted_until, \
    recovery_email, deleted_at, abandons, status_message, xp, name_changes, avatar, minor, chat_allowed_at, \
    chat_phrases_only";

fn user(r: &PgRow) -> Result<User, sqlx::Error> {
    let n = |c: &str| -> Result<u32, sqlx::Error> { Ok(r.try_get::<i32, _>(c)?.max(0) as u32) };
    Ok(User {
        id: r.try_get("id")?,
        username: r.try_get("username")?,
        password_hash: r.try_get("password_hash")?,
        country: r.try_get("country")?,
        role: Role::from_i16(r.try_get("role")?),
        records: PerRules::new(
            Record {
                rating: r.try_get("rating")?,
                peak_rating: r.try_get("peak_rating")?,
                games: n("games")?,
                wins: n("wins")?,
            },
            Record {
                rating: r.try_get("official_rating")?,
                peak_rating: r.try_get("official_peak_rating")?,
                games: n("official_games")?,
                wins: n("official_wins")?,
            },
        ),
        eight_wins: n("eight_wins")?,
        casual_games: n("casual_games")?,
        streak: n("streak")?,
        best_streak: n("best_streak")?,
        runouts: n("runouts")?,
        shutouts: n("shutouts")?,
        tournaments_played: n("tournaments_played")?,
        tournaments_won: n("tournaments_won")?,
        created_at: r.try_get("created_at")?,
        last_seen: r.try_get("last_seen")?,
        banned_until: r.try_get("banned_until")?,
        ban_reason: r.try_get("ban_reason")?,
        muted_until: r.try_get("muted_until")?,
        recovery_email: r.try_get("recovery_email")?,
        deleted_at: r.try_get("deleted_at")?,
        abandons: n("abandons")?,
        status_message: r.try_get("status_message")?,
        xp: r.try_get("xp")?,
        name_changes: r.try_get::<i16, _>("name_changes")?.max(0) as u32,
        avatar: r.try_get("avatar")?,
        minor: r.try_get("minor")?,
        chat_allowed_at: r.try_get("chat_allowed_at")?,
        chat_phrases_only: r.try_get("chat_phrases_only")?,
    })
}

/// Escapes `%`, `_` and `\` for a LIKE pattern.
fn like_pattern(q: &str) -> String {
    let mut out = String::from("%");
    for c in q.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

impl PgStore {
    pub async fn connect(url: &str, max_connections: u32) -> StoreResult<PgStore> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections.max(1))
            .min_connections(0)
            .acquire_timeout(std::time::Duration::from_secs(15))
            // Serverless databases (Neon) close idle connections themselves.
            .idle_timeout(std::time::Duration::from_secs(240))
            .test_before_acquire(true)
            .connect(url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(PgStore { pool })
    }

    pub async fn create_user(&self, new: &NewUser<'_>) -> StoreResult<Option<User>> {
        let now = crate::now();
        let row = query(&format!(
            "INSERT INTO users (username, password_hash, country, role, rating, peak_rating,
                                official_rating, official_peak_rating, created_at, last_seen, register_ip, minor)
             VALUES ($1, $2, $3, $4, $5, $5, $5, $5, $6, $6, $7, $8)
             ON CONFLICT DO NOTHING RETURNING {USER_COLUMNS}"
        ))
        .bind(new.username)
        .bind(new.password_hash)
        .bind(new.country)
        .bind(new.role.as_i16())
        .bind(new.rating)
        .bind(now)
        .bind(new.ip)
        .bind(new.minor)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| user(&r)).transpose()?)
    }

    pub async fn user_by_name(&self, name: &str) -> StoreResult<Option<User>> {
        let row =
            query(&format!("SELECT {USER_COLUMNS} FROM users WHERE lower(username) = lower($1)"))
                .bind(name)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|r| user(&r)).transpose()?)
    }

    pub async fn user_by_id(&self, id: i64) -> StoreResult<Option<User>> {
        let row = query(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| user(&r)).transpose()?)
    }

    pub async fn create_session(
        &self,
        hash: &str,
        user_id: i64,
        kind: SessionKind,
        created: i64,
        expires: i64,
    ) -> StoreResult<()> {
        query("INSERT INTO sessions (token_hash, user_id, kind, created_at, expires_at) VALUES ($1, $2, $3, $4, $5)")
            .bind(hash)
            .bind(user_id)
            .bind(kind as i16)
            .bind(created)
            .bind(expires)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn session_user(
        &self,
        hash: &str,
        kind: SessionKind,
        now: i64,
    ) -> StoreResult<Option<(User, i64)>> {
        let cols = USER_COLUMNS
            .split(", ")
            .map(|c| format!("u.{}", c.trim()))
            .collect::<Vec<_>>()
            .join(", ");
        let row = query(&format!(
            "SELECT {cols}, s.created_at AS session_created FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = $1 AND s.kind = $2 AND s.expires_at > $3"
        ))
        .bind(hash)
        .bind(kind as i16)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => Ok(Some((user(&r)?, r.try_get("session_created")?))),
            None => Ok(None),
        }
    }

    pub async fn delete_session(&self, hash: &str) -> StoreResult<()> {
        query("DELETE FROM sessions WHERE token_hash = $1").bind(hash).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn delete_user_sessions(&self, user_id: i64) -> StoreResult<()> {
        query("DELETE FROM sessions WHERE user_id = $1").bind(user_id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn purge_sessions(&self, now: i64) -> StoreResult<u64> {
        let r = query("DELETE FROM sessions WHERE expires_at <= $1")
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }

    pub async fn set_country(&self, user_id: i64, country: &str) -> StoreResult<()> {
        query("UPDATE users SET country = $1 WHERE id = $2")
            .bind(country)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn touch(&self, user_id: i64, now: i64) -> StoreResult<()> {
        query("UPDATE users SET last_seen = $1 WHERE id = $2")
            .bind(now)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn record_match(
        &self,
        m: &MatchRecord,
    ) -> StoreResult<Option<([Option<User>; 2], i64)>> {
        let mut tx = self.pool.begin().await?;
        if let Some(room) = m.room {
            let stored = query("SELECT 1 AS one FROM matches WHERE room_id = $1")
                .bind(room)
                .fetch_optional(&mut *tx)
                .await?;
            if stored.is_some() {
                return Ok(None);
            }
            // Its saved copy goes with it: never a result without the game
            // gone, nor the game gone without its result.
            query("DELETE FROM live_matches WHERE id = $1").bind(room).execute(&mut *tx).await?;
        }
        let out = Self::record_match_in(&mut tx, m).await?;
        tx.commit().await?;
        Ok(Some(out))
    }

    /// Stores a match inside a transaction (see [`PgStore::record_match`]).
    async fn record_match_in(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        m: &MatchRecord,
    ) -> StoreResult<([Option<User>; 2], i64)> {
        let mut out = [None, None];
        for (seat, slot) in out.iter_mut().enumerate() {
            let won = seat == m.winner;
            let (rated, delta) = (m.rated, m.delta[seat]);
            // Feats only count in rated games (casual ones are easy to arrange).
            let bonus = |flag: bool| (rated && won && flag && m.counts) as i32;
            let p = record_columns(m.rules);
            let row = query(&format!(
                "UPDATE users SET
                    {p}rating = {p}rating + $2,
                    {p}peak_rating = GREATEST({p}peak_rating, {p}rating + $2),
                    {p}games = {p}games + $3,
                    {p}wins = {p}wins + $4,
                    streak = CASE WHEN $3 = 0 OR NOT $10 THEN streak WHEN $4 = 1 THEN streak + 1 ELSE 0 END,
                    best_streak = GREATEST(best_streak, CASE WHEN $3 = 1 AND $4 = 1 THEN streak + 1 ELSE 0 END),
                    casual_games = casual_games + $5,
                    eight_wins = eight_wins + $6,
                    runouts = runouts + $7,
                    shutouts = shutouts + $8,
                    abandons = abandons + $9
                 WHERE id = $1 RETURNING {USER_COLUMNS}"
            ))
            .bind(m.players[seat])
            .bind(if rated { delta } else { 0 })
            .bind(rated as i32)
            .bind((rated && won && m.counts) as i32)
            .bind((!rated) as i32)
            .bind(bonus(m.eight_win))
            .bind(bonus(m.runout))
            .bind(bonus(m.shutout))
            .bind((m.abandoned == Some(seat)) as i32)
            .bind(m.counts)
            .fetch_optional(&mut **tx)
            .await?;
            *slot = row.map(|r| user(&r)).transpose()?;
        }
        let id: i64 = query(
            "INSERT INTO matches (player0, player1, winner_seat, reason, rated, delta0, delta1, rating0, rating1, shots, tournament_id, ended_at, seed, breaker, strokes, rules, room_id, calls, eight_call)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15::jsonb, $16, $17, $18, $19) RETURNING id",
        )
        .bind(m.players[0])
        .bind(m.players[1])
        .bind(m.winner as i16)
        .bind(crate::reason_code(m.reason))
        .bind(m.rated)
        .bind(m.delta[0])
        .bind(m.delta[1])
        .bind(out[0].as_ref().map(|u| u.records.get(m.rules).rating).unwrap_or(m.ratings_after[0]))
        .bind(out[1].as_ref().map(|u| u.records.get(m.rules).rating).unwrap_or(m.ratings_after[1]))
        .bind(m.shots as i32)
        .bind(m.tournament)
        .bind(m.ended_at)
        .bind(m.replay.as_ref().map(|r| r.seed as i64))
        .bind(m.replay.as_ref().map(|r| r.breaker as i16))
        .bind(m.replay.as_ref().map(replay_json))
        .bind(m.rules.code())
        .bind(m.room)
        .bind(m.replay.as_ref().is_some_and(|r| r.calls))
        // (A game with no replay has none to play back.)
        .bind(m.replay.as_ref().is_none_or(|r| r.eight_call))
        .fetch_one(&mut **tx)
        .await?
        .get("id");
        for seat in 0..2 {
            let st = &m.stats[seat];
            if st.shots == 0 {
                continue;
            }
            query(
                "INSERT INTO match_stats (match_id, user_id, shots, potting_shots, balls, fouls, best_run, in_hand)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(m.players[seat])
            .bind(st.shots as i32)
            .bind(st.potting_shots as i32)
            .bind(st.balls as i32)
            .bind(st.fouls as i32)
            .bind(st.best_run as i32)
            .bind(st.in_hand as i32)
            .execute(&mut **tx)
            .await?;
        }
        Ok((out, id))
    }

    pub async fn career(&self, user_id: i64) -> StoreResult<CareerNumbers> {
        let r = query(
            "SELECT COUNT(*) AS games, COALESCE(SUM(shots), 0) AS shots,
                    COALESCE(SUM(potting_shots), 0) AS potting, COALESCE(SUM(balls), 0) AS balls,
                    COALESCE(SUM(fouls), 0) AS fouls, COALESCE(MAX(best_run), 0) AS best
             FROM match_stats WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        let n = |c: &str| r.get::<i64, _>(c).max(0) as u32;
        Ok(CareerNumbers {
            games: n("games"),
            shots: n("shots"),
            potting_shots: n("potting"),
            balls: n("balls"),
            fouls: n("fouls"),
            best_run: r.get::<i32, _>("best").max(0) as u32,
        })
    }

    pub async fn rating_history(
        &self,
        user_id: i64,
        rules: Rules,
        limit: u32,
    ) -> StoreResult<Vec<(i64, i32)>> {
        let rows = query(
            "SELECT ended_at, CASE WHEN player0 = $1 THEN rating0 ELSE rating1 END AS rating
             FROM matches WHERE rated AND rules = $3 AND (player0 = $1 OR player1 = $1)
             ORDER BY ended_at DESC, id DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit as i64)
        .bind(rules.code())
        .fetch_all(&self.pool)
        .await?;
        let mut list: Vec<(i64, i32)> =
            rows.iter().map(|r| (r.get("ended_at"), r.get("rating"))).collect();
        list.reverse();
        Ok(list)
    }

    pub async fn replay(&self, match_id: i64) -> StoreResult<Option<StoredReplay>> {
        let row = query(
            "SELECT m.id, m.winner_seat, m.rated, m.ended_at, m.rating0 - m.delta0 AS r0,
                    m.rating1 - m.delta1 AS r1, m.seed, m.breaker, m.strokes::text AS strokes, m.rules,
                    m.calls, m.eight_call, a.username AS n0, a.country AS c0, b.username AS n1, b.country AS c1
             FROM matches m JOIN users a ON a.id = m.player0 JOIN users b ON b.id = m.player1
             WHERE m.id = $1 AND m.strokes IS NOT NULL",
        )
        .bind(match_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(r) = row else { return Ok(None) };
        let strokes: String = r.get("strokes");
        let Some(replay) =
            replay_of(r.get("seed"), r.get("breaker"), &strokes, r.get("rules"), calls_of(&r))
        else {
            return Ok(None);
        };
        Ok(Some(StoredReplay {
            id: r.get("id"),
            players: [r.get("n0"), r.get("n1")],
            countries: [r.get("c0"), r.get("c1")],
            ratings: [r.get("r0"), r.get("r1")],
            winner: r.get::<i16, _>("winner_seat") as usize % 2,
            rated: r.get("rated"),
            at: r.get("ended_at"),
            replay,
        }))
    }

    pub async fn recent_replays(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<(i64, usize, Replay)>> {
        let rows = query(
            "SELECT id, CASE WHEN player0 = $1 THEN 0 ELSE 1 END AS seat, seed, breaker,
                    strokes::text AS strokes, rules, calls, eight_call
             FROM matches WHERE (player0 = $1 OR player1 = $1) AND strokes IS NOT NULL
             ORDER BY ended_at DESC, id DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                let strokes: String = r.get("strokes");
                let replay = replay_of(
                    r.get("seed"),
                    r.get("breaker"),
                    &strokes,
                    r.get("rules"),
                    calls_of(r),
                )?;
                Some((r.get("id"), r.get::<i32, _>("seat") as usize, replay))
            })
            .collect())
    }

    pub async fn record_ladder_claim(
        &self,
        user_id: i64,
        rack_seed: u64,
        strokes: u32,
        at: i64,
    ) -> StoreResult<bool> {
        let added = query(
            "INSERT INTO ladder_claims (user_id, rack_seed, strokes, claimed_at)
             VALUES ($1, $2, $3, $4) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(rack_seed as i64)
        .bind(strokes as i32)
        .bind(at)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(added > 0)
    }

    pub async fn issue_ladder_ticket(
        &self,
        user_id: i64,
        seed: u64,
        breaker: usize,
        at: i64,
    ) -> StoreResult<()> {
        query(
            "INSERT INTO ladder_tickets (user_id, seed, breaker, issued_at) VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id, seed) DO NOTHING",
        )
        .bind(user_id)
        .bind(seed as i64)
        .bind(breaker as i16)
        .bind(at)
        .execute(&self.pool)
        .await?;
        // Only the newest few stay (a final is played soon after it's dealt).
        query(
            "DELETE FROM ladder_tickets WHERE user_id = $1 AND seed NOT IN (
                 SELECT seed FROM ladder_tickets WHERE user_id = $1
                 ORDER BY issued_at DESC, seed DESC LIMIT $2)",
        )
        .bind(user_id)
        .bind(super::LADDER_TICKETS_KEPT as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn add_cup_medal(
        &self,
        user_id: i64,
        medal: &snook_legend_proto::cup::Medal,
        at: i64,
    ) -> StoreResult<bool> {
        let r = query(
            "INSERT INTO cup_medals (user_id, cup, format, place, won_at) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (user_id, cup) DO NOTHING",
        )
        .bind(user_id)
        .bind(medal.cup as i64)
        .bind(medal.format.code())
        .bind(medal.place as i16)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn cup_medals(&self, user_id: i64) -> StoreResult<snook_legend_proto::cup::Medals> {
        let rows = query(
            "SELECT format, place, COUNT(*) AS n FROM cup_medals WHERE user_id = $1
             GROUP BY format, place",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        let mut medals = snook_legend_proto::cup::Medals::default();
        for r in rows {
            let format = snook_legend_proto::cup::Format::from_code(r.get("format"));
            let place = r.get::<i16, _>("place");
            let n = r.get::<i64, _>("n").max(0) as u32;
            if (1..=3).contains(&place) {
                let k = place as usize - 1;
                match format {
                    snook_legend_proto::cup::Format::Knockout => medals.knockout[k] += n,
                    snook_legend_proto::cup::Format::Arena => medals.arena[k] += n,
                    snook_legend_proto::cup::Format::Unknown => {}
                }
            }
        }
        Ok(medals)
    }

    pub async fn cup_medals_since(&self, user_id: i64, since: i64) -> StoreResult<u32> {
        let row = query("SELECT COUNT(*) AS n FROM cup_medals WHERE user_id = $1 AND won_at >= $2")
            .bind(user_id)
            .bind(since)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("n").max(0) as u32)
    }

    pub async fn cup_medal_list(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<(snook_legend_proto::cup::Medal, i64)>> {
        let rows = query(
            "SELECT cup, format, place, won_at FROM cup_medals WHERE user_id = $1
             ORDER BY won_at DESC, cup DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let medal = snook_legend_proto::cup::Medal {
                    cup: r.get::<i64, _>("cup") as u64,
                    format: snook_legend_proto::cup::Format::from_code(r.get("format")),
                    place: r.get::<i16, _>("place").clamp(0, 255) as u8,
                };
                (medal, r.get("won_at"))
            })
            .collect())
    }

    pub async fn ladder_finals(&self, user_id: i64) -> StoreResult<(u32, Option<i64>)> {
        let row = query(
            "SELECT COUNT(*) AS n, MAX(issued_at) AS latest FROM ladder_tickets WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok((row.get::<i64, _>("n").max(0) as u32, row.get("latest")))
    }

    pub async fn ladder_ticket(
        &self,
        user_id: i64,
        seed: u64,
    ) -> StoreResult<Option<(usize, i64)>> {
        let row =
            query("SELECT breaker, issued_at FROM ladder_tickets WHERE user_id = $1 AND seed = $2")
                .bind(user_id)
                .bind(seed as i64)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|r| (r.get::<i16, _>("breaker") as usize, r.get("issued_at"))))
    }

    pub async fn ladder_claim(&self, user_id: i64) -> StoreResult<Option<i64>> {
        let row = query("SELECT claimed_at FROM ladder_claims WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get("claimed_at")))
    }

    pub async fn save_live(&self, m: &LiveMatch) -> StoreResult<()> {
        query(
            "INSERT INTO live_matches (id, player0, player1, rated, seed, breaker, strokes, updated_at, rules, calls, eight_call)
             VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8, $9, $10, $11)
             ON CONFLICT (id) DO UPDATE SET strokes = EXCLUDED.strokes, updated_at = EXCLUDED.updated_at",
        )
        .bind(m.id)
        .bind(m.players[0])
        .bind(m.players[1])
        .bind(m.rated)
        .bind(m.replay.seed as i64)
        .bind(m.replay.breaker as i16)
        .bind(replay_json(&m.replay))
        .bind(m.updated_at)
        .bind(m.replay.rules.code())
        .bind(m.replay.calls)
        .bind(m.replay.eight_call)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_live(&self, id: i64) -> StoreResult<()> {
        query("DELETE FROM live_matches WHERE id = $1").bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn live_match_of(&self, user_id: i64) -> StoreResult<Option<LiveMatch>> {
        let row = query(
            "SELECT id, player0, player1, rated, seed, breaker, strokes::text AS strokes, updated_at,
                    rules, calls, eight_call
             FROM live_matches WHERE player0 = $1 OR player1 = $1 ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(|r| {
            let strokes: String = r.get("strokes");
            Some(LiveMatch {
                id: r.get("id"),
                players: [r.get("player0"), r.get("player1")],
                rated: r.get("rated"),
                replay: replay_of(
                    r.get("seed"),
                    r.get("breaker"),
                    &strokes,
                    r.get("rules"),
                    calls_of(&r),
                )?,
                updated_at: r.get("updated_at"),
            })
        }))
    }

    pub async fn purge_live(&self, before: i64) -> StoreResult<u64> {
        Ok(query("DELETE FROM live_matches WHERE updated_at < $1")
            .bind(before)
            .execute(&self.pool)
            .await?
            .rows_affected())
    }

    pub async fn spend(
        &self,
        user_id: i64,
        amount: i64,
        reason: &str,
        reference: Option<&str>,
        at: i64,
    ) -> StoreResult<bool> {
        let mut tx = self.pool.begin().await?;
        let n = query("UPDATE wallets SET coins = coins - $2 WHERE user_id = $1 AND coins >= $2")
            .bind(user_id)
            .bind(amount)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if n == 0 {
            return Ok(false);
        }
        query(
            "INSERT INTO coin_ledger (user_id, delta, reason, reference, at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(user_id)
        .bind(-amount)
        .bind(reason)
        .bind(reference)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn add_shot_stats(
        &self,
        user_id: i64,
        bands: &[(usize, u32, u32)],
    ) -> StoreResult<()> {
        if bands.is_empty() {
            return Ok(());
        }
        let b: Vec<i16> = bands.iter().map(|x| x.0 as i16).collect();
        let a: Vec<i32> = bands.iter().map(|x| x.1 as i32).collect();
        let m: Vec<i32> = bands.iter().map(|x| x.2 as i32).collect();
        query(
            "INSERT INTO shot_stats (user_id, band, attempts, made)
             SELECT $1, b, a, m FROM UNNEST($2::smallint[], $3::int[], $4::int[]) AS t(b, a, m)
             ON CONFLICT (user_id, band) DO UPDATE SET
                attempts = shot_stats.attempts + EXCLUDED.attempts,
                made = shot_stats.made + EXCLUDED.made",
        )
        .bind(user_id)
        .bind(&b)
        .bind(&a)
        .bind(&m)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn aim_rows(&self, bands: &[usize], min: u32) -> StoreResult<Vec<AimRow>> {
        let b: Vec<i16> = bands.iter().map(|x| *x as i16).collect();
        let rows = query(
            "SELECT u.id, u.username, GREATEST(u.rating, u.official_rating) AS rating,
                    SUM(s.attempts) AS attempts, SUM(s.made) AS made
             FROM shot_stats s JOIN users u ON u.id = s.user_id
             WHERE s.band = ANY($1) AND u.deleted_at IS NULL
             GROUP BY u.id HAVING SUM(s.attempts) >= $2",
        )
        .bind(&b)
        .bind(min as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| AimRow {
                user_id: r.get("id"),
                username: r.get("username"),
                rating: r.get("rating"),
                attempts: r.get::<i64, _>("attempts").max(0) as u32,
                made: r.get::<i64, _>("made").max(0) as u32,
            })
            .collect())
    }

    pub async fn mark_active(&self, user_id: i64, day: i64) -> StoreResult<()> {
        query("INSERT INTO activity (day, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(day)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn metrics(&self, now: i64) -> StoreResult<Metrics> {
        let today = day_of(now);
        let first = today - 29;
        let rows = query(
            "SELECT d.day,
                (SELECT COUNT(*) FROM activity a WHERE a.day = d.day) AS active,
                (SELECT COUNT(*) FROM matches m WHERE m.ended_at >= d.day * 86400 AND m.ended_at < (d.day + 1) * 86400) AS matches,
                (SELECT COUNT(*) FROM users u WHERE u.created_at >= d.day * 86400 AND u.created_at < (d.day + 1) * 86400) AS new
             FROM generate_series($1::bigint, $2::bigint) AS d(day) ORDER BY d.day",
        )
        .bind(first)
        .bind(today)
        .fetch_all(&self.pool)
        .await?;
        let days = rows
            .iter()
            .map(|r| (r.get("day"), r.get("active"), r.get("matches"), r.get("new")))
            .collect();
        // Accounts created from `from` to `to` (days) that came back
        // `after` days later, among those for which that day has come.
        let back_between = |from: i64, to: i64, after: i64| async move {
            let r = query(
                "SELECT COUNT(*) AS eligible,
                    COUNT(*) FILTER (WHERE EXISTS (SELECT 1 FROM activity a
                        WHERE a.user_id = u.id AND a.day = u.created_at / 86400 + $3)) AS came
                 FROM users u WHERE u.created_at >= $1 * 86400 AND u.created_at < ($2 + 1) * 86400
                    AND u.created_at / 86400 + $3 <= $4",
            )
            .bind(from)
            .bind(to)
            .bind(after)
            .bind(today)
            .fetch_one(&self.pool)
            .await?;
            Ok::<_, sqlx::Error>(super::percent(r.get("came"), r.get("eligible")))
        };
        let back = |after: i64| async move {
            Ok::<_, sqlx::Error>(
                back_between(today - after - 29, today - after, after).await?.unwrap_or(0.0),
            )
        };
        let retention_d1 = back(1).await?;
        let retention_d7 = back(7).await?;
        let retention_d30 = back(30).await?;
        let mut cohorts = Vec::new();
        for w in 0..super::COHORT_WEEKS {
            let first_day = today - 6 - 7 * w;
            let last = first_day + 6;
            let accounts: i64 = query(
                "SELECT COUNT(*) AS n FROM users
                     WHERE created_at >= $1 * 86400 AND created_at < ($2 + 1) * 86400",
            )
            .bind(first_day)
            .bind(last)
            .fetch_one(&self.pool)
            .await?
            .get("n");
            cohorts.push(super::Cohort {
                first_day,
                accounts,
                d1: back_between(first_day, last, 1).await?,
                d7: back_between(first_day, last, 7).await?,
                d30: back_between(first_day, last, 30).await?,
            });
        }
        let f = query(
            "SELECT COUNT(*) AS created,
                COUNT(*) FILTER (WHERE games + official_games + casual_games >= 1) AS one,
                COUNT(*) FILTER (WHERE games + official_games + casual_games >= 5) AS five
             FROM users WHERE created_at >= $1 * 86400",
        )
        .bind(today - 30)
        .fetch_one(&self.pool)
        .await?;
        let revenue = query(
            "SELECT currency, SUM(amount)::bigint AS cents, COUNT(*) AS n FROM payments
             WHERE NOT refunded AND NOT test AND at > $1 GROUP BY currency ORDER BY currency",
        )
        .bind(now - 30 * 86_400)
        .fetch_all(&self.pool)
        .await?
        .iter()
        .map(|r| (r.get("currency"), r.get("cents"), r.get("n")))
        .collect();
        let revenue_days = query(
            "SELECT at / 86400 AS day, currency, SUM(amount)::bigint AS cents, COUNT(*) AS n
             FROM payments WHERE NOT refunded AND NOT test AND at > $1
             GROUP BY day, currency ORDER BY day, currency",
        )
        .bind(now - 30 * 86_400)
        .fetch_all(&self.pool)
        .await?
        .iter()
        .map(|r| (r.get("day"), r.get("currency"), r.get("cents"), r.get("n")))
        .collect();
        let this_hour = hour_of(now);
        let hours = query(
            "SELECT h.hour,
                (SELECT COUNT(*) FROM active_hours a WHERE a.hour = h.hour) AS active,
                COALESCE(o.peak_players, 0)::bigint AS peak_players,
                COALESCE(o.peak_games, 0)::bigint AS peak_games,
                CASE WHEN COALESCE(o.samples, 0) > 0
                     THEN o.player_sum::float8 / o.samples ELSE 0 END AS avg_players,
                (SELECT COUNT(*) FROM matches m
                  WHERE m.ended_at >= h.hour * 3600 AND m.ended_at < (h.hour + 1) * 3600) AS games
             FROM generate_series($1::bigint, $2::bigint) AS h(hour)
             LEFT JOIN online_hours o ON o.hour = h.hour
             ORDER BY h.hour",
        )
        .bind(this_hour - 47)
        .bind(this_hour)
        .fetch_all(&self.pool)
        .await?
        .iter()
        .map(|r| super::HourRow {
            hour: r.get("hour"),
            active: r.get("active"),
            peak_players: r.get("peak_players"),
            peak_games: r.get("peak_games"),
            avg_players: r.get("avg_players"),
            games: r.get("games"),
        })
        .collect();
        Ok(Metrics {
            days,
            retention_d1,
            retention_d7,
            retention_d30,
            cohorts,
            funnel: [f.get("created"), f.get("one"), f.get("five")],
            revenue,
            hours,
            revenue_days,
        })
    }

    pub async fn set_recovery_email(&self, user_id: i64, email: Option<&str>) -> StoreResult<()> {
        query("UPDATE users SET recovery_email = $2 WHERE id = $1")
            .bind(user_id)
            .bind(email)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn change_username(
        &self,
        user_id: i64,
        name: &str,
        limit: u32,
    ) -> StoreResult<Rename> {
        self.rename(user_id, name, Some(limit)).await
    }

    /// Renames a player (within `limit` changes, unless it is the staff),
    /// the audit log's entries about them with them.
    async fn rename(&self, user_id: i64, name: &str, limit: Option<u32>) -> StoreResult<Rename> {
        let mut tx = self.pool.begin().await?;
        // The row is locked: two changes at once can't both get under the
        // limit, and the unique index settles who gets a name.
        let old: Option<String> = query(
            "SELECT username FROM users WHERE id = $1 AND deleted_at IS NULL
               AND ($2::INT IS NULL OR name_changes < $2) FOR UPDATE",
        )
        .bind(user_id)
        .bind(limit.map(|l| l as i32))
        .fetch_optional(&mut *tx)
        .await?
        .map(|r| r.get("username"));
        let Some(old) = old else { return Ok(Rename::NoChangesLeft) };
        let counted = if limit.is_some() { 1 } else { 0 };
        let done =
            query("UPDATE users SET username = $2, name_changes = name_changes + $3 WHERE id = $1")
                .bind(user_id)
                .bind(name)
                .bind(counted)
                .execute(&mut *tx)
                .await;
        match done {
            Ok(_) => {}
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => return Ok(Rename::Taken),
            Err(e) => return Err(e.into()),
        }
        move_audit(&mut tx, &old, name).await?;
        tx.commit().await?;
        Ok(Rename::Done)
    }

    pub async fn payment_email(&self, user_id: i64) -> StoreResult<Option<String>> {
        let row = query(
            "SELECT email FROM payments
             WHERE user_id = $1 AND email IS NOT NULL AND kind <> 'vip_gift'
             ORDER BY at DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get("email")))
    }

    pub async fn delete_account(&self, user_id: i64, now: i64) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        let old: Option<String> =
            query("SELECT username FROM users WHERE id = $1 AND deleted_at IS NULL FOR UPDATE")
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await?
                .map(|r| r.get("username"));
        if let Some(old) = old {
            move_audit(&mut tx, &old, &super::deleted_name(user_id)).await?;
        }
        query(
            "UPDATE users SET username = $2, password_hash = '', country = '',
                recovery_email = NULL, deleted_at = $3, register_ip = '', status_message = '',
                avatar = '', minor = NULL, chat_allowed_at = NULL, chat_phrases_only = FALSE
             WHERE id = $1",
        )
        .bind(user_id)
        .bind(super::deleted_name(user_id))
        .bind(now)
        .execute(&mut *tx)
        .await?;
        for sql in [
            "DELETE FROM sessions WHERE user_id = $1",
            "DELETE FROM friendships WHERE user_id = $1 OR friend_id = $1",
            "DELETE FROM blocks WHERE user_id = $1 OR blocked_id = $1",
            "DELETE FROM achievements WHERE user_id = $1",
            "DELETE FROM ladder_claims WHERE user_id = $1",
            "DELETE FROM ladder_tickets WHERE user_id = $1",
            "DELETE FROM cup_medals WHERE user_id = $1",
            "DELETE FROM staff_notes WHERE user_id = $1",
            "UPDATE gifts SET message = '' WHERE from_id = $1",
            "DELETE FROM equipped WHERE user_id = $1",
            "DELETE FROM shot_stats WHERE user_id = $1",
            "DELETE FROM live_matches WHERE player0 = $1 OR player1 = $1",
            "DELETE FROM async_challenges WHERE from_id = $1 OR to_id = $1",
            "UPDATE wallets SET stripe_customer = NULL WHERE user_id = $1",
            "UPDATE payments SET email = NULL WHERE user_id = $1",
            // The keys to their Microsoft account's purchases go too.
            "UPDATE msstore_consumes SET store_key = NULL WHERE user_id = $1",
            // Their settings and conversations go too.
            "DELETE FROM account_settings WHERE user_id = $1",
            "DELETE FROM direct_messages WHERE from_id = $1 OR to_id = $1",
            "DELETE FROM shot_weeks WHERE user_id = $1",
        ] {
            query(sql).bind(user_id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn mission_progress(
        &self,
        user_id: i64,
        periods: &[String],
    ) -> StoreResult<Vec<(String, String, u32)>> {
        let rows = query(
            "SELECT period, code, progress FROM mission_progress
             WHERE user_id = $1 AND period = ANY($2)",
        )
        .bind(user_id)
        .bind(periods)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get("period"), r.get("code"), r.get::<i32, _>("progress").max(0) as u32))
            .collect())
    }

    pub async fn advance_mission(
        &self,
        user_id: i64,
        period: &str,
        code: &str,
        amount: u32,
        target: u32,
        ends: i64,
    ) -> StoreResult<u32> {
        let r = query(
            "INSERT INTO mission_progress (user_id, period, code, progress, ends)
             VALUES ($1, $2, $3, LEAST($4, $5), $6)
             ON CONFLICT (user_id, period, code) DO UPDATE
             SET progress = LEAST(mission_progress.progress + $4, $5)
             RETURNING progress",
        )
        .bind(user_id)
        .bind(period)
        .bind(code)
        .bind(amount as i32)
        .bind(target as i32)
        .bind(ends)
        .fetch_one(&self.pool)
        .await?;
        Ok(r.get::<i32, _>("progress").max(0) as u32)
    }

    pub async fn purge_old(&self, day: i64, now: i64) -> StoreResult<u64> {
        let a = query("DELETE FROM activity WHERE day < $1").bind(day).execute(&self.pool).await?;
        let b = query("DELETE FROM board_notes WHERE expires_at IS NOT NULL AND expires_at <= $1")
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(a.rows_affected() + b.rows_affected())
    }

    pub async fn purge_missions(&self, now: i64) -> StoreResult<u64> {
        let r = query("DELETE FROM mission_progress WHERE ends <= $1")
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }

    pub async fn board_notes(&self, now: i64) -> StoreResult<Vec<BoardNote>> {
        let rows = query(
            "SELECT n.id, n.text, COALESCE(u.username, '') AS author, n.created_at, n.expires_at,
                    n.kind, n.pinned, n.action
             FROM board_notes n LEFT JOIN users u ON u.id = n.author_id
             WHERE n.expires_at IS NULL OR n.expires_at > $1
             ORDER BY n.pinned DESC, n.created_at DESC, n.id DESC",
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| BoardNote {
                id: r.get("id"),
                text: r.get("text"),
                author: r.get("author"),
                created_at: r.get("created_at"),
                expires_at: r.get("expires_at"),
                kind: r.get("kind"),
                pinned: r.get("pinned"),
                action: r.get("action"),
            })
            .collect())
    }

    pub async fn add_board_note(&self, n: &NewNote<'_>) -> StoreResult<i64> {
        let r = query(
            "INSERT INTO board_notes (text, author_id, created_at, expires_at, kind, pinned, action)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        )
        .bind(n.text)
        .bind(n.author_id)
        .bind(n.at)
        .bind(n.expires_at)
        .bind(n.kind)
        .bind(n.pinned)
        .bind(n.action)
        .fetch_one(&self.pool)
        .await?;
        Ok(r.get("id"))
    }

    pub async fn remove_board_note(&self, id: i64) -> StoreResult<bool> {
        let r = query("DELETE FROM board_notes WHERE id = $1").bind(id).execute(&self.pool).await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn claim_daily(&self, user_id: i64, day: i64, coins: i64) -> StoreResult<bool> {
        let mut tx = self.pool.begin().await?;
        query("INSERT INTO wallets (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        let n = query(
            "UPDATE wallets SET daily_day = $2, coins = coins + $3
             WHERE user_id = $1 AND daily_day < $2",
        )
        .bind(user_id)
        .bind(day)
        .bind(coins)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if n > 0 {
            query(
                "INSERT INTO coin_ledger (user_id, delta, reason, reference, at)
                 VALUES ($1, $2, 'daily bonus', NULL, $3)",
            )
            .bind(user_id)
            .bind(coins)
            .bind(day * 86_400)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(n > 0)
    }

    pub async fn earn(
        &self,
        user_id: i64,
        day: i64,
        amount: i64,
        cap: i64,
        reason: &str,
        at: i64,
    ) -> StoreResult<i64> {
        let mut tx = self.pool.begin().await?;
        query("INSERT INTO wallets (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        // Lock the row, start a new day if needed, give what fits under the cap.
        let r = query(
            "SELECT CASE WHEN earned_day = $2 THEN earned_today ELSE 0 END AS so_far
             FROM wallets WHERE user_id = $1 FOR UPDATE",
        )
        .bind(user_id)
        .bind(day)
        .fetch_one(&mut *tx)
        .await?;
        let so_far: i32 = r.get("so_far");
        let given = amount.min(cap - so_far as i64).max(0);
        if given > 0 {
            query(
                "UPDATE wallets SET earned_day = $2, earned_today = $3, coins = coins + $4
                 WHERE user_id = $1",
            )
            .bind(user_id)
            .bind(day)
            .bind(so_far + given as i32)
            .bind(given)
            .execute(&mut *tx)
            .await?;
            query(
                "INSERT INTO coin_ledger (user_id, delta, reason, reference, at)
                 VALUES ($1, $2, $3, NULL, $4)",
            )
            .bind(user_id)
            .bind(given)
            .bind(reason)
            .bind(at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(given)
    }

    pub async fn give_item(
        &self,
        gift: &NewGift<'_>,
        price: i64,
    ) -> StoreResult<(BuyOutcome, Option<i64>)> {
        let (from, to, code, at) = (gift.from, gift.to, gift.code, gift.at);
        let mut tx = self.pool.begin().await?;
        let added = query(
            "INSERT INTO owned_items (user_id, code, acquired_at, source) VALUES ($1, $2, $3, $4)
             ON CONFLICT DO NOTHING",
        )
        .bind(to)
        .bind(code)
        .bind(at)
        .bind(format!("gift:{from}"))
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if added == 0 {
            return Ok((BuyOutcome::AlreadyOwned, None));
        }
        let paid =
            query("UPDATE wallets SET coins = coins - $2 WHERE user_id = $1 AND coins >= $2")
                .bind(from)
                .bind(price)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        if paid == 0 {
            return Ok((BuyOutcome::NotEnoughCoins, None));
        }
        query(
            "INSERT INTO coin_ledger (user_id, delta, reason, reference, at)
             VALUES ($1, $2, $3, NULL, $4)",
        )
        .bind(from)
        .bind(-price)
        .bind(format!("gift {code} to {to}"))
        .bind(at)
        .execute(&mut *tx)
        .await?;
        let id: i64 = query(INSERT_GIFT)
            .bind(from)
            .bind(to)
            .bind(code)
            .bind(gift.message)
            .bind(at)
            .fetch_one(&mut *tx)
            .await?
            .try_get("id")?;
        tx.commit().await?;
        Ok((BuyOutcome::Bought, Some(id)))
    }

    pub async fn record_gift(&self, gift: &NewGift<'_>) -> StoreResult<i64> {
        let row = query(INSERT_GIFT)
            .bind(gift.from)
            .bind(gift.to)
            .bind(gift.code)
            .bind(gift.message)
            .bind(gift.at)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.try_get("id")?)
    }

    pub async fn gift(&self, id: i64) -> StoreResult<Option<GiftRow>> {
        let row = query(&format!("{GIFT_SELECT} WHERE g.id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| gift_row(&r)).transpose()?)
    }

    pub async fn gifts_of(&self, user_id: i64, limit: u32) -> StoreResult<Vec<GiftRow>> {
        let rows = query(&format!(
            "{GIFT_SELECT} WHERE g.to_id = $1 OR g.from_id = $1 ORDER BY g.at DESC, g.id DESC LIMIT $2"
        ))
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(gift_row).collect::<Result<_, _>>()?)
    }

    pub async fn unseen_gifts(&self, user_id: i64) -> StoreResult<Vec<GiftRow>> {
        let rows = query(&format!(
            "{GIFT_SELECT} WHERE g.to_id = $1 AND NOT g.seen ORDER BY g.at, g.id LIMIT 100"
        ))
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(gift_row).collect::<Result<_, _>>()?)
    }

    pub async fn mark_gifts_seen(&self, user_id: i64, ids: &[i64]) -> StoreResult<u64> {
        let done =
            query("UPDATE gifts SET seen = TRUE WHERE to_id = $1 AND id = ANY($2) AND NOT seen")
                .bind(user_id)
                .bind(ids)
                .execute(&self.pool)
                .await?;
        Ok(done.rows_affected())
    }

    pub async fn thank_gift(&self, user_id: i64, id: i64) -> StoreResult<Option<GiftRow>> {
        let done = query(
            "UPDATE gifts SET thanked = TRUE, seen = TRUE WHERE id = $1 AND to_id = $2 AND NOT thanked",
        )
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        if done.rows_affected() == 0 {
            return Ok(None);
        }
        self.gift(id).await
    }

    pub async fn add_achievements(
        &self,
        user_id: i64,
        codes: &[&str],
        at: i64,
    ) -> StoreResult<Vec<String>> {
        if codes.is_empty() {
            return Ok(Vec::new());
        }
        let codes: Vec<String> = codes.iter().map(|c| c.to_string()).collect();
        let rows = query(
            "INSERT INTO achievements (user_id, code, unlocked_at)
             SELECT $1, c, $3 FROM UNNEST($2::text[]) AS c
             ON CONFLICT DO NOTHING RETURNING code",
        )
        .bind(user_id)
        .bind(&codes)
        .bind(at)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| r.get::<String, _>("code")).collect())
    }

    pub async fn achievements(&self, user_id: i64) -> StoreResult<Vec<(String, i64)>> {
        let rows = query(
            "SELECT code, unlocked_at FROM achievements WHERE user_id = $1 ORDER BY unlocked_at",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| (r.get("code"), r.get("unlocked_at"))).collect())
    }

    pub async fn set_avatar(&self, user_id: i64, avatar: &str) -> StoreResult<()> {
        query("UPDATE users SET avatar = $2 WHERE id = $1")
            .bind(user_id)
            .bind(avatar)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_minor(&self, user_id: i64, minor: bool) -> StoreResult<bool> {
        let done = query("UPDATE users SET minor = $2 WHERE id = $1 AND minor IS NULL")
            .bind(user_id)
            .bind(minor)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    pub async fn set_chat_allowed(&self, user_id: i64, at: Option<i64>) -> StoreResult<()> {
        query("UPDATE users SET chat_allowed_at = $2 WHERE id = $1")
            .bind(user_id)
            .bind(at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_status_message(&self, user_id: i64, text: &str) -> StoreResult<()> {
        query("UPDATE users SET status_message = $2 WHERE id = $1")
            .bind(user_id)
            .bind(text)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn recent_opponents(&self, user_id: i64, limit: u32) -> StoreResult<Vec<RecentRow>> {
        // The player's latest games; of those, the latest against each
        // opponent (and how many there were).
        let rows = query(&format!(
            "WITH mine AS (
                SELECT id, ended_at, rated,
                       CASE WHEN player0 = $1 THEN player1 ELSE player0 END AS opp,
                       winner_seat = CASE WHEN player0 = $1 THEN 0 ELSE 1 END AS won
                FROM matches WHERE player0 = $1 OR player1 = $1
                ORDER BY ended_at DESC, id DESC LIMIT $3
             ), latest AS (
                SELECT DISTINCT ON (opp) opp, ended_at AS m_at, id AS m_id, rated AS m_rated,
                       won AS m_won, COUNT(*) OVER (PARTITION BY opp) AS m_games
                FROM mine ORDER BY opp, ended_at DESC, id DESC
             )
             SELECT {USER_COLUMNS}, l.m_at, l.m_rated, l.m_won, l.m_games
             FROM latest l JOIN users ON users.id = l.opp
             WHERE users.deleted_at IS NULL
             ORDER BY l.m_at DESC, l.m_id DESC LIMIT $2"
        ))
        .bind(user_id)
        .bind(limit as i64)
        .bind(RECENT_SCAN as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                Ok(RecentRow {
                    user: user(r)?,
                    at: r.try_get("m_at")?,
                    won: r.try_get("m_won")?,
                    rated: r.try_get("m_rated")?,
                    games: r.try_get::<i64, _>("m_games")?.max(0) as u32,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    pub async fn history(&self, user_id: i64, limit: u32) -> StoreResult<Vec<HistoryRow>> {
        // Not the strokes (a whole game each): whether there is a replay is
        // whether the game kept its seed.
        let rows = query(
            "SELECT m.id, m.winner_seat, m.reason, m.rated, m.rules, m.delta0, m.delta1, m.rating0,
                    m.rating1, m.tournament_id, m.ended_at, m.seed, m.correspondence,
                    CASE WHEN m.player0 = $1 THEN 0 ELSE 1 END AS seat,
                    o.username AS opp_name, o.country AS opp_country, o.role AS opp_role
             FROM matches m
             JOIN users o ON o.id = CASE WHEN m.player0 = $1 THEN m.player1 ELSE m.player0 END
             WHERE m.player0 = $1 OR m.player1 = $1
             ORDER BY m.ended_at DESC, m.id DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                let seat: i32 = r.try_get("seat")?;
                let pick =
                    |a: &'static str, b: &'static str| if seat == 0 { (a, b) } else { (b, a) };
                let (my_delta, _) = pick("delta0", "delta1");
                let (my_rating, opp_rating) = pick("rating0", "rating1");
                let winner: i16 = r.try_get("winner_seat")?;
                Ok(HistoryRow {
                    opponent: r.try_get("opp_name")?,
                    opponent_country: r.try_get("opp_country")?,
                    opponent_admin: Role::from_i16(r.try_get("opp_role")?) == Role::Admin,
                    opponent_rating: r.try_get(opp_rating)?,
                    won: winner as i32 == seat,
                    reason: r.try_get("reason")?,
                    rated: r.try_get("rated")?,
                    rules: Rules::parse(r.try_get("rules")?).unwrap_or_default(),
                    delta: r.try_get(my_delta)?,
                    rating_after: r.try_get(my_rating)?,
                    tournament: r.try_get::<Option<i64>, _>("tournament_id")?.is_some(),
                    at: r.try_get("ended_at")?,
                    match_id: r.try_get("id")?,
                    replay: r.try_get::<Option<i64>, _>("seed")?.is_some(),
                    correspondence: r.try_get("correspondence")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }

    pub async fn leaderboard(
        &self,
        rules: Rules,
        limit: u32,
        now: i64,
    ) -> StoreResult<Vec<LeaderboardEntry>> {
        let p = record_columns(rules);
        let rows = query(&format!(
            "SELECT u.username, u.{p}rating AS rating, u.{p}games AS games, u.{p}wins AS wins,
                    u.country, u.role, COALESCE(w.vip_until, 0) > $2 AS vip
             FROM users u LEFT JOIN wallets w ON w.user_id = u.id
             WHERE u.{p}games > 0 AND (u.banned_until IS NULL OR u.banned_until <= $2)
               AND u.deleted_at IS NULL
             ORDER BY u.{p}rating DESC, u.{p}wins DESC, u.username COLLATE \"C\" ASC LIMIT $1"
        ))
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
                games: r.get::<i32, _>("games") as u32,
                wins: r.get::<i32, _>("wins") as u32,
                country: r.get("country"),
                vip: r.get("vip"),
                admin: Role::from_i16(r.get("role")) == Role::Admin,
            })
            .collect())
    }

    pub async fn rank_of(&self, user_id: i64, rules: Rules) -> StoreResult<Option<u32>> {
        let p = record_columns(rules);
        let row = query(&format!(
            "SELECT (SELECT COUNT(*) FROM users o WHERE o.{p}games > 0
                       AND (o.banned_until IS NULL OR o.banned_until <= $2) AND o.deleted_at IS NULL
                       AND (o.{p}rating > u.{p}rating OR (o.{p}rating = u.{p}rating
                            AND (o.{p}wins > u.{p}wins OR (o.{p}wins = u.{p}wins
                                 AND o.username COLLATE \"C\" < u.username COLLATE \"C\"))))) AS above
             FROM users u WHERE u.id = $1 AND u.{p}games > 0"
        ))
        .bind(user_id)
        .bind(crate::now())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<i64, _>("above") as u32 + 1))
    }

    pub async fn friends(&self, user_id: i64) -> StoreResult<Vec<(User, FriendState)>> {
        let cols = USER_COLUMNS
            .split(", ")
            .map(|c| format!("u.{}", c.trim()))
            .collect::<Vec<_>>()
            .join(", ");
        let rows = query(&format!(
            "SELECT {cols}, 0 AS dir, f.accepted FROM friendships f JOIN users u ON u.id = f.friend_id WHERE f.user_id = $1
             UNION ALL
             SELECT {cols}, 1 AS dir, f.accepted FROM friendships f JOIN users u ON u.id = f.user_id
             WHERE f.friend_id = $1 AND NOT f.accepted"
        ))
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for r in &rows {
            let accepted: bool = r.try_get("accepted")?;
            let dir: i32 = r.try_get("dir")?;
            let state = match (accepted, dir) {
                (true, _) => FriendState::Friends,
                (false, 0) => FriendState::Outgoing,
                _ => FriendState::Incoming,
            };
            out.push((user(r)?, state));
        }
        out.sort_by_key(|a| a.0.username.to_lowercase());
        Ok(out)
    }

    pub async fn friend_request(&self, from: i64, to: i64, at: i64) -> StoreResult<FriendOutcome> {
        let mut tx = self.pool.begin().await?;
        // One request between two players at a time: two sent to each other
        // at once make friends (not two requests waiting), and the same one
        // sent twice is sent once.
        friend_lock(&mut tx, from, to).await?;
        let mine: Option<bool> =
            query("SELECT accepted FROM friendships WHERE user_id = $1 AND friend_id = $2")
                .bind(from)
                .bind(to)
                .fetch_optional(&mut *tx)
                .await?
                .map(|r| r.get("accepted"));
        let theirs: Option<bool> =
            query("SELECT accepted FROM friendships WHERE user_id = $1 AND friend_id = $2")
                .bind(to)
                .bind(from)
                .fetch_optional(&mut *tx)
                .await?
                .map(|r| r.get("accepted"));
        let outcome = match (mine, theirs) {
            (Some(true), _) => FriendOutcome::AlreadyFriends,
            (_, Some(false)) => {
                query(
                    "UPDATE friendships SET accepted = TRUE WHERE user_id = $1 AND friend_id = $2",
                )
                .bind(to)
                .bind(from)
                .execute(&mut *tx)
                .await?;
                query(
                    "INSERT INTO friendships (user_id, friend_id, accepted, created_at) VALUES ($1, $2, TRUE, $3)
                     ON CONFLICT (user_id, friend_id) DO UPDATE SET accepted = TRUE",
                )
                .bind(from)
                .bind(to)
                .bind(at)
                .execute(&mut *tx)
                .await?;
                FriendOutcome::Accepted
            }
            (Some(false), _) => FriendOutcome::AlreadySent,
            (None, _) => {
                query("INSERT INTO friendships (user_id, friend_id, accepted, created_at) VALUES ($1, $2, FALSE, $3)")
                    .bind(from)
                    .bind(to)
                    .bind(at)
                    .execute(&mut *tx)
                    .await?;
                FriendOutcome::Sent
            }
        };
        tx.commit().await?;
        Ok(outcome)
    }

    pub async fn friend_accept(&self, me: i64, from: i64) -> StoreResult<bool> {
        let mut tx = self.pool.begin().await?;
        // (With the pair's lock: accepted and removed at the same moment,
        // one happens after the other, never half of each.)
        friend_lock(&mut tx, me, from).await?;
        let updated = query(
            "UPDATE friendships SET accepted = TRUE WHERE user_id = $1 AND friend_id = $2 AND NOT accepted",
        )
        .bind(from)
        .bind(me)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated == 0 {
            return Ok(false);
        }
        query(
            "INSERT INTO friendships (user_id, friend_id, accepted, created_at) VALUES ($1, $2, TRUE, $3)
             ON CONFLICT (user_id, friend_id) DO UPDATE SET accepted = TRUE",
        )
        .bind(me)
        .bind(from)
        .bind(crate::now())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn friend_remove(&self, a: i64, b: i64) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        friend_lock(&mut tx, a, b).await?;
        query(
            "DELETE FROM friendships WHERE (user_id = $1 AND friend_id = $2) OR (user_id = $2 AND friend_id = $1)",
        )
        .bind(a)
        .bind(b)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn friend_count(&self, user_id: i64) -> StoreResult<u32> {
        let r = query("SELECT COUNT(*) AS n FROM friendships WHERE user_id = $1 AND accepted")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(r.get::<i64, _>("n") as u32)
    }

    pub async fn block(&self, user_id: i64, blocked: i64) -> StoreResult<()> {
        query("INSERT INTO blocks (user_id, blocked_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(blocked)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn unblock(&self, user_id: i64, blocked: i64) -> StoreResult<()> {
        query("DELETE FROM blocks WHERE user_id = $1 AND blocked_id = $2")
            .bind(user_id)
            .bind(blocked)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn blocked(&self, user_id: i64) -> StoreResult<Vec<User>> {
        let cols = USER_COLUMNS
            .split(", ")
            .map(|c| format!("u.{}", c.trim()))
            .collect::<Vec<_>>()
            .join(", ");
        let rows = query(&format!(
            "SELECT {cols} FROM blocks b JOIN users u ON u.id = b.blocked_id WHERE b.user_id = $1"
        ))
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(user).collect::<Result<_, _>>()?)
    }

    pub async fn create_report_with_game(
        &self,
        r: &NewReport,
        match_id: Option<i64>,
        live_id: Option<i64>,
    ) -> StoreResult<i64> {
        let id = query(
            "INSERT INTO reports (reporter_id, target_id, reason, details, context, created_at, match_id, live_id)
             VALUES ($1, $2, $3, $4, $5, $6, (SELECT id FROM matches WHERE id = $7), $8)
             RETURNING id",
        )
        .bind(r.reporter)
        .bind(r.target)
        .bind(&r.reason)
        .bind(&r.details)
        .bind(&r.context)
        .bind(r.at)
        .bind(match_id)
        .bind(live_id)
        .fetch_one(&self.pool)
        .await?
        .get("id");
        Ok(id)
    }

    pub async fn report(&self, id: i64) -> StoreResult<Option<ReportRow>> {
        let row = query(&format!("{} WHERE r.id = $1", Self::REPORT_SELECT))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.as_ref().map(Self::report_row).transpose()?)
    }

    pub async fn set_report_match(&self, id: i64, match_id: i64) -> StoreResult<()> {
        query(
            "UPDATE reports SET match_id = $2 WHERE id = $1 AND EXISTS (SELECT 1 FROM matches WHERE id = $2)",
        )
        .bind(id)
        .bind(match_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn matches_between(
        &self,
        a: i64,
        b: i64,
        from: i64,
        to: i64,
        limit: u32,
    ) -> StoreResult<Vec<(i64, i64)>> {
        let rows = query(
            "SELECT id, ended_at FROM matches
             WHERE LEAST(player0, player1) = LEAST($1::bigint, $2::bigint)
               AND GREATEST(player0, player1) = GREATEST($1::bigint, $2::bigint)
               AND ended_at BETWEEN $3 AND $4 AND strokes IS NOT NULL
             ORDER BY ended_at DESC, id DESC LIMIT $5",
        )
        .bind(a)
        .bind(b)
        .bind(from)
        .bind(to)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| (r.get("id"), r.get("ended_at"))).collect())
    }

    pub async fn add_shot_analysis(
        &self,
        user_id: i64,
        analysis: &ShotAnalysis,
    ) -> StoreResult<()> {
        let rows = analysis.rows();
        if rows.is_empty() {
            return Ok(());
        }
        let kinds: Vec<i16> = rows.iter().map(|r| r.0).collect();
        let keys: Vec<i16> = rows.iter().map(|r| r.1).collect();
        let attempts: Vec<i32> = rows.iter().map(|r| r.2.min(i32::MAX as u32) as i32).collect();
        let made: Vec<i32> = rows.iter().map(|r| r.3.min(i32::MAX as u32) as i32).collect();
        query(
            "INSERT INTO shot_analysis (user_id, kind, key, attempts, made)
             SELECT $1, k, y, a, m FROM UNNEST($2::smallint[], $3::smallint[], $4::int[], $5::int[]) AS t(k, y, a, m)
             ON CONFLICT (user_id, kind, key) DO UPDATE SET
                attempts = shot_analysis.attempts + EXCLUDED.attempts,
                made = shot_analysis.made + EXCLUDED.made",
        )
        .bind(user_id)
        .bind(&kinds)
        .bind(&keys)
        .bind(&attempts)
        .bind(&made)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn shot_analysis(&self, user_id: i64) -> StoreResult<ShotAnalysis> {
        let rows = query("SELECT kind, key, attempts, made FROM shot_analysis WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(ShotAnalysis::from_rows(rows.iter().map(|r| {
            (
                r.get::<i16, _>("kind"),
                r.get::<i16, _>("key"),
                r.get::<i32, _>("attempts").max(0) as u32,
                r.get::<i32, _>("made").max(0) as u32,
            )
        })))
    }

    pub async fn sample_online(
        &self,
        hour: i64,
        users: &[i64],
        players: u32,
        games: u32,
    ) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        query(
            "INSERT INTO online_hours (hour, peak_players, peak_games, samples, player_sum)
             VALUES ($1, $2, $3, 1, $2)
             ON CONFLICT (hour) DO UPDATE SET
                peak_players = GREATEST(online_hours.peak_players, EXCLUDED.peak_players),
                peak_games = GREATEST(online_hours.peak_games, EXCLUDED.peak_games),
                samples = online_hours.samples + 1,
                player_sum = online_hours.player_sum + EXCLUDED.player_sum",
        )
        .bind(hour)
        .bind(players as i32)
        .bind(games as i32)
        .execute(&mut *tx)
        .await?;
        if !users.is_empty() {
            query(
                "INSERT INTO active_hours (hour, user_id)
                 SELECT $1, u.id FROM users u WHERE u.id = ANY($2)
                 ON CONFLICT DO NOTHING",
            )
            .bind(hour)
            .bind(users)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn purge_active_hours(&self, hour: i64) -> StoreResult<u64> {
        let r = query("DELETE FROM active_hours WHERE hour < $1")
            .bind(hour)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }

    pub async fn daily_seed(&self, day: i64, fresh: u64) -> StoreResult<u64> {
        query(
            "INSERT INTO daily_challenges (day, seed) VALUES ($1, $2) ON CONFLICT (day) DO NOTHING",
        )
        .bind(day)
        .bind(fresh as i64)
        .execute(&self.pool)
        .await?;
        let seed: i64 = query("SELECT seed FROM daily_challenges WHERE day = $1")
            .bind(day)
            .fetch_one(&self.pool)
            .await?
            .get("seed");
        Ok(seed as u64)
    }

    pub async fn daily_start(
        &self,
        user_id: i64,
        day: i64,
        now: i64,
        max: u32,
    ) -> StoreResult<bool> {
        let row = query(
            "INSERT INTO daily_scores (day, user_id, attempts, started_at) VALUES ($1, $2, 1, $3)
             ON CONFLICT (day, user_id) DO UPDATE
                SET attempts = daily_scores.attempts + 1, started_at = $3
                WHERE daily_scores.attempts < $4
             RETURNING attempts",
        )
        .bind(day)
        .bind(user_id)
        .bind(now)
        .bind(max as i32)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    pub async fn daily_finish(
        &self,
        user_id: i64,
        day: i64,
        now: i64,
        balls: Option<u32>,
        cleared: bool,
        limit: i64,
    ) -> StoreResult<super::DailyFinish> {
        let mut tx = self.pool.begin().await?;
        let row = query(
            "SELECT best, best_secs, started_at, cleared FROM daily_scores
             WHERE day = $1 AND user_id = $2 AND started_at > 0 AND $3 - started_at <= $4
             FOR UPDATE",
        )
        .bind(day)
        .bind(user_id)
        .bind(now)
        .bind(limit)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Ok(super::DailyFinish::NotStarted);
        };
        let best: Option<i32> = row.get("best");
        let best_secs: i32 = row.get("best_secs");
        let started_at: i64 = row.get("started_at");
        let was_cleared: bool = row.get("cleared");
        let secs = (now - started_at).clamp(0, limit) as i32;
        // A refused attempt (no balls) ends without a score; as many balls
        // in less time is better.
        let improved = balls.is_some_and(|n| {
            best.is_none_or(|b| n as i32 > b || (n as i32 == b && secs < best_secs))
        });
        query(
            "UPDATE daily_scores SET started_at = 0, cleared = cleared OR $3,
                best = CASE WHEN $4 THEN $5 ELSE best END,
                best_at = CASE WHEN $4 THEN $6 ELSE best_at END,
                best_secs = CASE WHEN $4 THEN $7 ELSE best_secs END
             WHERE day = $1 AND user_id = $2",
        )
        .bind(day)
        .bind(user_id)
        .bind(cleared)
        .bind(improved)
        .bind(balls.unwrap_or(0) as i32)
        .bind(now)
        .bind(secs)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(super::DailyFinish::Counted { improved, first_clear: cleared && !was_cleared })
    }

    pub async fn daily_row(&self, user_id: i64, day: i64) -> StoreResult<super::DailyRow> {
        let row = query(
            "SELECT attempts, started_at, best, best_at, best_secs, cleared FROM daily_scores
             WHERE day = $1 AND user_id = $2",
        )
        .bind(day)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map_or_else(Default::default, |r| super::DailyRow {
            attempts: r.get::<i32, _>("attempts").max(0) as u32,
            started_at: r.get("started_at"),
            best: r.get::<Option<i32>, _>("best").map(|b| b.max(0) as u32),
            best_at: r.get::<Option<i64>, _>("best_at").unwrap_or(0),
            best_secs: r.get::<i32, _>("best_secs").max(0) as u32,
            cleared: r.get("cleared"),
        }))
    }

    pub async fn daily_board(
        &self,
        day: i64,
        limit: u32,
    ) -> StoreResult<(Vec<super::DailyBoardRow>, u32)> {
        let rows = query(
            "SELECT d.user_id, u.username, u.country, d.best, d.best_at, d.best_secs
             FROM daily_scores d JOIN users u ON u.id = d.user_id
             WHERE d.day = $1 AND d.best IS NOT NULL AND u.deleted_at IS NULL
               AND (u.banned_until IS NULL OR u.banned_until <= $3)
             ORDER BY d.best DESC, d.best_secs, d.best_at, d.user_id LIMIT $2",
        )
        .bind(day)
        .bind(limit as i64)
        .bind(crate::now())
        .fetch_all(&self.pool)
        .await?;
        let players: i64 = query(
            "SELECT COUNT(*) AS n FROM daily_scores d JOIN users u ON u.id = d.user_id
             WHERE d.day = $1 AND d.best IS NOT NULL AND u.deleted_at IS NULL
               AND (u.banned_until IS NULL OR u.banned_until <= $2)",
        )
        .bind(day)
        .bind(crate::now())
        .fetch_one(&self.pool)
        .await?
        .get("n");
        let board = rows
            .iter()
            .map(|r| super::DailyBoardRow {
                user_id: r.get("user_id"),
                username: r.get("username"),
                country: r.get("country"),
                balls: r.get::<i32, _>("best").max(0) as u32,
                at: r.get("best_at"),
                secs: r.get::<i32, _>("best_secs").max(0) as u32,
            })
            .collect();
        Ok((board, players as u32))
    }

    pub async fn daily_rank(&self, user_id: i64, day: i64) -> StoreResult<Option<u32>> {
        let row = query(
            "SELECT (SELECT COUNT(*) FROM daily_scores o JOIN users ou ON ou.id = o.user_id
                     WHERE o.day = me.day AND o.best IS NOT NULL AND ou.deleted_at IS NULL
                       AND (ou.banned_until IS NULL OR ou.banned_until <= $3)
                       AND (o.best > me.best OR (o.best = me.best
                            AND (o.best_secs, o.best_at, o.user_id)
                                < (me.best_secs, me.best_at, me.user_id)))) AS ahead
             FROM daily_scores me JOIN users mu ON mu.id = me.user_id
             WHERE me.day = $1 AND me.user_id = $2 AND me.best IS NOT NULL
               AND mu.deleted_at IS NULL AND (mu.banned_until IS NULL OR mu.banned_until <= $3)",
        )
        .bind(day)
        .bind(user_id)
        .bind(crate::now())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get::<i64, _>("ahead") as u32 + 1))
    }

    pub async fn daily_wins(&self, user_id: i64, before: i64) -> StoreResult<Vec<(i64, u32)>> {
        let rows = query(
            "SELECT day, n FROM (
                SELECT d.day, d.user_id, COUNT(*) OVER (PARTITION BY d.day) AS n,
                    ROW_NUMBER() OVER (PARTITION BY d.day ORDER BY d.best DESC, d.best_secs, d.best_at, d.user_id) AS place
                FROM daily_scores d JOIN users u ON u.id = d.user_id
                WHERE d.best IS NOT NULL AND d.day < $2 AND u.deleted_at IS NULL
                  AND (u.banned_until IS NULL OR u.banned_until <= $3)
                  AND d.day IN (SELECT day FROM daily_scores
                                WHERE user_id = $1 AND best IS NOT NULL AND day < $2)
             ) ranked WHERE user_id = $1 AND place = 1 ORDER BY day DESC LIMIT 100",
        )
        .bind(user_id)
        .bind(before)
        .bind(crate::now())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| (r.get("day"), r.get::<i64, _>("n") as u32)).collect())
    }

    pub async fn drill_bests(&self, user_id: i64) -> StoreResult<Vec<(String, u32)>> {
        let rows = query("SELECT code, best FROM drill_scores WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(|r| (r.get("code"), r.get::<i32, _>("best").max(0) as u32)).collect())
    }

    pub async fn drill_record(
        &self,
        user_id: i64,
        code: &str,
        score: u32,
        at: i64,
    ) -> StoreResult<Option<u32>> {
        let mut tx = self.pool.begin().await?;
        let before: Option<i32> =
            query("SELECT best FROM drill_scores WHERE user_id = $1 AND code = $2 FOR UPDATE")
                .bind(user_id)
                .bind(code)
                .fetch_optional(&mut *tx)
                .await?
                .map(|r| r.get("best"));
        query(
            "INSERT INTO drill_scores (user_id, code, best, best_at) VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id, code) DO UPDATE SET best = $3, best_at = $4
             WHERE drill_scores.best < $3",
        )
        .bind(user_id)
        .bind(code)
        .bind(score as i32)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(before.map(|b| b.max(0) as u32))
    }

    pub async fn drill_board(&self, code: &str, limit: u32) -> StoreResult<Vec<super::BoardRow>> {
        let rows = query(
            "SELECT u.username, u.country, d.best FROM drill_scores d JOIN users u ON u.id = d.user_id
             WHERE d.code = $1 AND u.deleted_at IS NULL
               AND (u.banned_until IS NULL OR u.banned_until <= $3)
             ORDER BY d.best DESC, d.best_at, d.user_id LIMIT $2",
        )
        .bind(code)
        .bind(limit as i64)
        .bind(crate::now())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| super::BoardRow {
                username: r.get("username"),
                country: r.get("country"),
                score: r.get::<i32, _>("best").max(0) as u32,
                stars: 0,
            })
            .collect())
    }

    pub async fn set_challenge_rating(
        &self,
        user_id: i64,
        rating: u32,
        stars: u32,
        at: i64,
    ) -> StoreResult<()> {
        query(
            "INSERT INTO challenge_ratings (user_id, rating, stars, updated_at) VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id) DO UPDATE SET rating = $2, stars = $3, updated_at = $4",
        )
        .bind(user_id)
        .bind(rating as i32)
        .bind(stars as i32)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn challenge_board(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<(Vec<super::BoardRow>, Option<u32>)> {
        let rows = query(
            "SELECT u.username, u.country, c.rating, c.stars FROM challenge_ratings c
             JOIN users u ON u.id = c.user_id WHERE u.deleted_at IS NULL
               AND (u.banned_until IS NULL OR u.banned_until <= $2)
             ORDER BY c.rating DESC, c.updated_at, c.user_id LIMIT $1",
        )
        .bind(limit as i64)
        .bind(crate::now())
        .fetch_all(&self.pool)
        .await?;
        let rank: Option<i64> = query(
            "SELECT (SELECT COUNT(*) FROM challenge_ratings o JOIN users ou ON ou.id = o.user_id
                     WHERE ou.deleted_at IS NULL
                       AND (ou.banned_until IS NULL OR ou.banned_until <= $2)
                       AND (o.rating > me.rating OR (o.rating = me.rating
                       AND (o.updated_at, o.user_id) < (me.updated_at, me.user_id)))) AS ahead
             FROM challenge_ratings me JOIN users mu ON mu.id = me.user_id
             WHERE me.user_id = $1
               AND mu.deleted_at IS NULL AND (mu.banned_until IS NULL OR mu.banned_until <= $2)",
        )
        .bind(user_id)
        .bind(crate::now())
        .fetch_optional(&self.pool)
        .await?
        .map(|r| r.get("ahead"));
        let board = rows
            .iter()
            .map(|r| super::BoardRow {
                username: r.get("username"),
                country: r.get("country"),
                score: r.get::<i32, _>("rating").max(0) as u32,
                stars: r.get::<i32, _>("stars").max(0) as u32,
            })
            .collect();
        Ok((board, rank.map(|a| a as u32 + 1)))
    }

    pub async fn add_xp(&self, user_id: i64, amount: u32) -> StoreResult<i64> {
        let xp: Option<i64> = query("UPDATE users SET xp = xp + $2 WHERE id = $1 RETURNING xp")
            .bind(user_id)
            .bind(amount as i64)
            .fetch_optional(&self.pool)
            .await?
            .map(|r| r.get("xp"));
        Ok(xp.unwrap_or(0))
    }

    pub async fn tournaments_won(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<(String, u32, i64)>> {
        let rows = query(
            "SELECT name, players, finished_at FROM tournaments WHERE winner_id = $1
             ORDER BY finished_at DESC, id DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                (r.get("name"), r.get::<i32, _>("players").max(0) as u32, r.get("finished_at"))
            })
            .collect())
    }

    pub async fn record_tournament(&self, t: &TournamentRecord) -> StoreResult<bool> {
        let mut tx = self.pool.begin().await?;
        let inserted = query(
            "INSERT INTO tournaments (name, host_id, size, rated, players, winner_id, finished_at, entry_fee, prizes, lounge_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (lounge_id) WHERE lounge_id IS NOT NULL DO NOTHING",
        )
        .bind(&t.name)
        .bind(t.host)
        .bind(t.size as i32)
        .bind(t.rated)
        .bind(t.players.len() as i32)
        .bind(t.winner)
        .bind(t.finished_at)
        .bind(t.entry_fee as i32)
        .bind(&t.prizes)
        .bind(t.lounge_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted == 0 {
            return Ok(false);
        }
        query(
            "UPDATE users SET tournaments_played = tournaments_played + 1,
                tournaments_won = tournaments_won + CASE WHEN id = $2 THEN 1 ELSE 0 END
             WHERE id = ANY($1)",
        )
        .bind(&t.players)
        .bind(t.winner.unwrap_or(-1))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn search_users(
        &self,
        search: &super::UserSearch,
        offset: u32,
        limit: u32,
    ) -> StoreResult<(Vec<User>, i64)> {
        use super::{UserFilter, UserSort};
        let q = search.q.trim().to_lowercase();
        // Fixed SQL for each filter and order (no text from the request in
        // it): the values are bound.
        let filter = match search.filter {
            UserFilter::All => "deleted_at IS NULL",
            UserFilter::Staff => "role > 0 AND deleted_at IS NULL",
            UserFilter::Banned => "banned_until > $3",
            UserFilter::Muted => "muted_until > $3",
            UserFilter::Deleted => "deleted_at IS NOT NULL",
        };
        let order = match search.sort {
            UserSort::Newest => "id DESC",
            UserSort::Name => "lower(username) COLLATE \"C\", id",
            UserSort::Rating(Rules::Brazilian) => "rating DESC, id DESC",
            UserSort::Rating(Rules::Official) => "official_rating DESC, id DESC",
            UserSort::LastSeen => "last_seen DESC, id DESC",
        };
        let matching = format!(
            "(lower(username) LIKE $1 OR lower(recovery_email) = $2) AND $3::BIGINT IS NOT NULL AND {filter}"
        );
        let rows = query(&format!(
            "SELECT {USER_COLUMNS} FROM users WHERE {matching} ORDER BY {order} OFFSET $4 LIMIT $5"
        ))
        .bind(like_pattern(&q))
        .bind(&q)
        .bind(search.now)
        .bind(offset as i64)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        let total: i64 = query(&format!("SELECT COUNT(*) AS n FROM users WHERE {matching}"))
            .bind(like_pattern(&q))
            .bind(&q)
            .bind(search.now)
            .fetch_one(&self.pool)
            .await?
            .get("n");
        Ok((rows.iter().map(user).collect::<Result<_, _>>()?, total))
    }

    pub async fn staff_rename(&self, user_id: i64, name: &str) -> StoreResult<Rename> {
        self.rename(user_id, name, None).await
    }

    pub async fn add_staff_note(
        &self,
        user_id: i64,
        author_id: i64,
        kind: &str,
        text: &str,
        at: i64,
    ) -> StoreResult<i64> {
        let row = query(
            "INSERT INTO staff_notes (user_id, author_id, kind, text, created_at)
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(user_id)
        .bind(author_id)
        .bind(kind)
        .bind(text)
        .bind(at)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("id"))
    }

    pub async fn staff_notes(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<super::StaffNote>> {
        let rows = query(
            "SELECT n.id, n.user_id, COALESCE(u.username, '?') AS author, n.kind, n.text,
                    n.created_at, n.delivered_at
             FROM staff_notes n LEFT JOIN users u ON u.id = n.author_id
             WHERE n.user_id = $1 ORDER BY n.id DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| super::StaffNote {
                id: r.get("id"),
                user_id: r.get("user_id"),
                author: r.get("author"),
                kind: r.get("kind"),
                text: r.get("text"),
                created_at: r.get("created_at"),
                delivered_at: r.get("delivered_at"),
            })
            .collect())
    }

    pub async fn remove_staff_note(&self, id: i64, author: Option<i64>) -> StoreResult<bool> {
        let done = query(
            "DELETE FROM staff_notes WHERE id = $1 AND ($2::BIGINT IS NULL OR author_id = $2)",
        )
        .bind(id)
        .bind(author)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    pub async fn take_warnings(&self, user_id: i64, at: i64) -> StoreResult<Vec<String>> {
        let rows = query(
            "UPDATE staff_notes SET delivered_at = $2
             WHERE user_id = $1 AND kind = 'warning' AND delivered_at IS NULL
             RETURNING id, text",
        )
        .bind(user_id)
        .bind(at)
        .fetch_all(&self.pool)
        .await?;
        let mut out: Vec<(i64, String)> =
            rows.iter().map(|r| (r.get("id"), r.get("text"))).collect();
        out.sort_by_key(|x| x.0);
        Ok(out.into_iter().map(|x| x.1).collect())
    }

    pub async fn add_event(&self, e: &super::NewEvent<'_>) -> StoreResult<i64> {
        let row = query(
            "INSERT INTO events (kind, title, multiplier, starts_at, ends_at, format, size, rated,
                                 rules, entry_fee, prize, created_by, created_at,
                                 champion_share, item_prize)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
             RETURNING id",
        )
        .bind(e.kind)
        .bind(e.title)
        .bind(e.multiplier as i16)
        .bind(e.starts_at)
        .bind(e.ends_at)
        .bind(e.format)
        .bind(e.size as i32)
        .bind(e.rated)
        .bind(e.rules.code())
        .bind(e.entry_fee as i32)
        .bind(e.prize as i32)
        .bind(e.created_by)
        .bind(e.at)
        .bind(e.champion_share as i16)
        .bind(e.item_prize)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get("id"))
    }

    async fn event_rows(
        &self,
        filter: &str,
        order: &str,
        limit: i64,
        t: i64,
    ) -> StoreResult<Vec<super::GameEvent>> {
        let rows = query(&format!(
            "SELECT e.*, COALESCE(u.username, '') AS author FROM events e
             LEFT JOIN users u ON u.id = e.created_by
             WHERE {filter} AND $1::BIGINT IS NOT NULL ORDER BY {order} LIMIT $2"
        ))
        .bind(t)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let n = |r: &PgRow, c: &str| r.get::<i32, _>(c).max(0) as u32;
        Ok(rows
            .iter()
            .map(|r| super::GameEvent {
                id: r.get("id"),
                kind: r.get("kind"),
                title: r.get("title"),
                multiplier: r.get::<i16, _>("multiplier").max(1) as u32,
                starts_at: r.get("starts_at"),
                ends_at: r.get("ends_at"),
                format: r.get("format"),
                size: n(r, "size"),
                rated: r.get("rated"),
                rules: Rules::parse(r.get("rules")).unwrap_or_default(),
                entry_fee: n(r, "entry_fee"),
                prize: n(r, "prize"),
                champion_share: r.get::<i16, _>("champion_share").clamp(0, 100) as u32,
                item_prize: r.get("item_prize"),
                created_by: r.get("author"),
                created_at: r.get("created_at"),
                cancelled_at: r.get("cancelled_at"),
            })
            .collect())
    }

    pub async fn events_after(&self, t: i64) -> StoreResult<Vec<super::GameEvent>> {
        self.event_rows("e.cancelled_at IS NULL AND e.ends_at > $1", "e.starts_at, e.id", 1000, t)
            .await
    }

    pub async fn recent_events(&self, limit: u32) -> StoreResult<Vec<super::GameEvent>> {
        self.event_rows("TRUE", "e.starts_at DESC, e.id DESC", limit as i64, 0).await
    }

    pub async fn cancel_event(&self, id: i64, at: i64) -> StoreResult<bool> {
        let done =
            query("UPDATE events SET cancelled_at = $2 WHERE id = $1 AND cancelled_at IS NULL")
                .bind(id)
                .bind(at)
                .execute(&self.pool)
                .await?;
        Ok(done.rows_affected() > 0)
    }

    pub async fn economy(&self, now: i64) -> StoreResult<super::Economy> {
        let totals = query(
            "SELECT COALESCE(SUM(coins), 0)::BIGINT AS coins,
                    COUNT(*) FILTER (WHERE coins > 0) AS wallets,
                    COUNT(*) FILTER (WHERE vip_until > $1) AS vip
             FROM wallets",
        )
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        // Grouped by the reason as written; the kinds are made here (the
        // reasons are few: "buy x" for each item, "staff: …" for each note).
        let flows = query(
            "SELECT reason,
                    COALESCE(SUM(delta) FILTER (WHERE delta >= 0), 0)::BIGINT AS credited,
                    COALESCE(-SUM(delta) FILTER (WHERE delta < 0), 0)::BIGINT AS debited,
                    COUNT(*) AS entries
             FROM coin_ledger WHERE at > $1 GROUP BY reason",
        )
        .bind(now - 30 * 86_400)
        .fetch_all(&self.pool)
        .await?;
        let mut by: std::collections::BTreeMap<String, super::CoinFlow> = Default::default();
        for r in &flows {
            let kind = super::coin_kind(&r.get::<String, _>("reason"));
            let f = by.entry(kind.clone()).or_insert(super::CoinFlow {
                kind,
                credited: 0,
                debited: 0,
                entries: 0,
            });
            f.credited += r.get::<i64, _>("credited");
            f.debited += r.get::<i64, _>("debited");
            f.entries += r.get::<i64, _>("entries");
        }
        let mut flows: Vec<super::CoinFlow> = by.into_values().collect();
        flows.sort_by_key(|f| (std::cmp::Reverse(f.credited + f.debited), f.kind.clone()));
        let richest = query(
            "SELECT u.username, w.coins FROM wallets w JOIN users u ON u.id = w.user_id
             WHERE w.coins > 0 AND u.deleted_at IS NULL
             ORDER BY w.coins DESC, u.username COLLATE \"C\" LIMIT 10",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(super::Economy {
            coins_total: totals.get("coins"),
            wallets: totals.get("wallets"),
            vip_now: totals.get("vip"),
            flows,
            richest: richest.iter().map(|r| (r.get("username"), r.get("coins"))).collect(),
        })
    }

    pub async fn recent_payments(&self, limit: u32) -> StoreResult<Vec<(Payment, String)>> {
        let rows = query(
            "SELECT p.*, u.username AS payer FROM payments p JOIN users u ON u.id = p.user_id
             ORDER BY p.at DESC LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| (payment_row(r), r.get("payer"))).collect())
    }

    pub async fn active_countries(&self, since: i64) -> StoreResult<Vec<(String, i64)>> {
        let rows = query(
            "SELECT country, COUNT(*) AS n FROM users
             WHERE last_seen > $1 AND deleted_at IS NULL
             GROUP BY country ORDER BY n DESC, country",
        )
        .bind(since)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| (r.get("country"), r.get("n"))).collect())
    }

    pub async fn overview(&self, now: i64) -> StoreResult<Overview> {
        let week = now - 7 * 86_400;
        let r = query(
            "SELECT
                (SELECT COUNT(*) FROM users) AS users,
                (SELECT COUNT(*) FROM users WHERE created_at > $1) AS new_users,
                (SELECT COUNT(*) FROM users WHERE last_seen > $1) AS active_users,
                (SELECT COUNT(*) FROM matches) AS matches_total,
                (SELECT COUNT(*) FROM matches WHERE ended_at > $2) AS matches_day,
                (SELECT COUNT(*) FROM matches WHERE ended_at > $2 AND rules = 'official')
                    AS official_matches_day,
                (SELECT COUNT(*) FROM reports WHERE status = 0) AS open_reports,
                (SELECT COUNT(*) FROM users WHERE banned_until > $3) AS banned",
        )
        .bind(week)
        .bind(now - 86_400)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(Overview {
            users: r.get("users"),
            new_users_7d: r.get("new_users"),
            active_users_7d: r.get("active_users"),
            matches_total: r.get("matches_total"),
            matches_24h: r.get("matches_day"),
            matches_24h_by_rules: {
                let (day, official): (i64, i64) =
                    (r.get("matches_day"), r.get("official_matches_day"));
                PerRules::new(day - official, official)
            },
            open_reports: r.get("open_reports"),
            banned: r.get("banned"),
        })
    }

    pub async fn set_ban(
        &self,
        user_id: i64,
        until: Option<i64>,
        reason: Option<&str>,
    ) -> StoreResult<()> {
        query("UPDATE users SET banned_until = $1, ban_reason = $2 WHERE id = $3")
            .bind(until)
            .bind(reason)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_mute(&self, user_id: i64, until: Option<i64>) -> StoreResult<()> {
        query("UPDATE users SET muted_until = $1 WHERE id = $2")
            .bind(until)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_role(&self, user_id: i64, role: Role) -> StoreResult<()> {
        query("UPDATE users SET role = $1 WHERE id = $2")
            .bind(role.as_i16())
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_password(&self, user_id: i64, hash: &str) -> StoreResult<()> {
        query("UPDATE users SET password_hash = $1 WHERE id = $2")
            .bind(hash)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    fn report_row(r: &PgRow) -> Result<ReportRow, sqlx::Error> {
        Ok(ReportRow {
            id: r.try_get("id")?,
            reporter: r.try_get("reporter")?,
            target: r.try_get("target")?,
            reason: r.try_get("reason")?,
            details: r.try_get("details")?,
            context: r.try_get("context")?,
            status: r.try_get("status")?,
            created_at: r.try_get("created_at")?,
            resolved_by: r.try_get("resolver")?,
            resolution: r.try_get("resolution")?,
            reporter_id: r.try_get("reporter_id")?,
            target_id: r.try_get("target_id")?,
            match_id: r.try_get("match_id")?,
            live_id: r.try_get("live_id")?,
        })
    }

    const REPORT_SELECT: &str =
        "SELECT r.id, a.username AS reporter, t.username AS target, r.reason, r.details,
            r.context, r.status, r.created_at, s.username AS resolver, r.resolution,
            r.reporter_id, r.target_id, r.match_id, r.live_id
         FROM reports r JOIN users a ON a.id = r.reporter_id JOIN users t ON t.id = r.target_id
         LEFT JOIN users s ON s.id = r.resolved_by";

    pub async fn reports(&self, status: Option<i16>, limit: u32) -> StoreResult<Vec<ReportRow>> {
        let rows = query(&format!(
            "{} WHERE ($1::smallint IS NULL OR r.status = $1) ORDER BY r.created_at DESC LIMIT $2",
            Self::REPORT_SELECT
        ))
        .bind(status)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(Self::report_row).collect::<Result<_, _>>()?)
    }

    pub async fn reports_about(&self, user_id: i64, limit: u32) -> StoreResult<Vec<ReportRow>> {
        let rows = query(&format!(
            "{} WHERE r.target_id = $1 ORDER BY r.created_at DESC LIMIT $2",
            Self::REPORT_SELECT
        ))
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(Self::report_row).collect::<Result<_, _>>()?)
    }

    pub async fn resolve_report(
        &self,
        id: i64,
        staff: i64,
        status: i16,
        note: &str,
        at: i64,
    ) -> StoreResult<bool> {
        let r = query(
            "UPDATE reports SET status = $1, resolved_by = $2, resolution = $3, resolved_at = $4 WHERE id = $5",
        )
        .bind(status)
        .bind(staff)
        .bind(note)
        .bind(at)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn audit(
        &self,
        staff: i64,
        action: &str,
        target: &str,
        details: &str,
        at: i64,
    ) -> StoreResult<()> {
        query("INSERT INTO audit_log (staff_id, action, target, details, at) VALUES ($1, $2, $3, $4, $5)")
            .bind(staff)
            .bind(action)
            .bind(target)
            .bind(details)
            .bind(at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn audit_search(
        &self,
        search: &super::AuditSearch,
        limit: u32,
    ) -> StoreResult<Vec<AuditRow>> {
        let q = search.q.trim().to_lowercase();
        let rows = query(
            "SELECT l.id, COALESCE(u.username, '?') AS staff, l.action, l.target, l.details, l.at
             FROM audit_log l LEFT JOIN users u ON u.id = l.staff_id
             WHERE ($1 = '' OR lower(COALESCE(u.username, '')) LIKE $2
                    OR lower(l.target) LIKE $2 OR lower(l.details) LIKE $2)
               AND ($3::TEXT IS NULL OR l.action = $3)
               AND ($4::TEXT IS NULL OR lower(l.target) = lower($4))
               AND ($5::BIGINT IS NULL OR l.id < $5)
             ORDER BY l.id DESC LIMIT $6",
        )
        .bind(&q)
        .bind(like_pattern(&q))
        .bind(search.action.as_deref())
        .bind(search.target.as_deref())
        .bind(search.before)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                Ok(AuditRow {
                    id: r.try_get("id")?,
                    staff: r.try_get("staff")?,
                    action: r.try_get("action")?,
                    target: r.try_get("target")?,
                    details: r.try_get("details")?,
                    at: r.try_get("at")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }
    // The shop.

    pub async fn wallet(&self, user_id: i64) -> StoreResult<Wallet> {
        let mut w = Wallet::default();
        if let Some(r) =
            query("SELECT coins, vip_until, stripe_customer FROM wallets WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?
        {
            w.coins = r.get("coins");
            w.vip_until = r.get("vip_until");
            w.stripe_customer = r.get("stripe_customer");
        }
        w.owned = query("SELECT code FROM owned_items WHERE user_id = $1 ORDER BY acquired_at")
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|r| r.get("code"))
            .collect();
        w.equipped = query("SELECT category, code FROM equipped WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|r| (r.get("category"), r.get("code")))
            .collect();
        let cues = query("SELECT code, wins, games, potted FROM cue_wins WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        let n = |r: &PgRow, c: &str| r.get::<i32, _>(c).max(0) as u32;
        w.cue_wins = cues.iter().map(|r| (r.get("code"), n(r, "wins"))).collect();
        w.cue_games =
            cues.iter().map(|r| (r.get("code"), (n(r, "games"), n(r, "potted")))).collect();
        Ok(w)
    }

    pub async fn buy_item(
        &self,
        user_id: i64,
        code: &str,
        price: i64,
        at: i64,
    ) -> StoreResult<BuyOutcome> {
        let mut tx = self.pool.begin().await?;
        let added = query(
            "INSERT INTO owned_items (user_id, code, acquired_at, source) VALUES ($1, $2, $3, 'coins')
             ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(code)
        .bind(at)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if added == 0 {
            return Ok(BuyOutcome::AlreadyOwned);
        }
        // Takes the coins only if there are enough (never below zero).
        let paid =
            query("UPDATE wallets SET coins = coins - $2 WHERE user_id = $1 AND coins >= $2")
                .bind(user_id)
                .bind(price)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        if paid == 0 {
            return Ok(BuyOutcome::NotEnoughCoins); // Rolled back on drop.
        }
        query("INSERT INTO coin_ledger (user_id, delta, reason, at) VALUES ($1, $2, $3, $4)")
            .bind(user_id)
            .bind(-price)
            .bind(format!("buy {code}"))
            .bind(at)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(BuyOutcome::Bought)
    }

    pub async fn grant_item(
        &self,
        user_id: i64,
        code: &str,
        source: &str,
        at: i64,
    ) -> StoreResult<bool> {
        let added = query(
            "INSERT INTO owned_items (user_id, code, acquired_at, source) VALUES ($1, $2, $3, $4)
             ON CONFLICT DO NOTHING",
        )
        .bind(user_id)
        .bind(code)
        .bind(at)
        .bind(source)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(added > 0)
    }

    pub async fn add_cue_game(
        &self,
        user_id: i64,
        code: &str,
        won: bool,
        potted: u32,
    ) -> StoreResult<u32> {
        let wins: i32 = query(
            "INSERT INTO cue_wins (user_id, code, wins, games, potted) VALUES ($1, $2, $3, 1, $4)
             ON CONFLICT (user_id, code) DO UPDATE SET wins = cue_wins.wins + $3,
                 games = cue_wins.games + 1, potted = cue_wins.potted + $4
             RETURNING wins",
        )
        .bind(user_id)
        .bind(code)
        .bind(i32::from(won))
        .bind(potted.min(i32::MAX as u32) as i32)
        .fetch_one(&self.pool)
        .await?
        .get("wins");
        Ok(wins.max(0) as u32)
    }

    pub async fn revoke_items(&self, user_id: i64, codes: &[&str]) -> StoreResult<u64> {
        let codes: Vec<String> = codes.iter().map(|c| c.to_string()).collect();
        let removed = query("DELETE FROM owned_items WHERE user_id = $1 AND code = ANY($2)")
            .bind(user_id)
            .bind(&codes)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(removed)
    }

    pub async fn equip(&self, user_id: i64, category: &str, code: &str) -> StoreResult<()> {
        query(
            "INSERT INTO equipped (user_id, category, code) VALUES ($1, $2, $3)
             ON CONFLICT (user_id, category) DO UPDATE SET code = EXCLUDED.code",
        )
        .bind(user_id)
        .bind(category)
        .bind(code)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn credit(
        &self,
        user_id: i64,
        delta: i64,
        reason: &str,
        reference: Option<&str>,
        at: i64,
    ) -> StoreResult<bool> {
        let mut tx = self.pool.begin().await?;
        let inserted = query(
            "INSERT INTO coin_ledger (user_id, delta, reason, reference, at) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (reference) DO NOTHING",
        )
        .bind(user_id)
        .bind(delta)
        .bind(reason)
        .bind(reference)
        .bind(at)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted == 0 {
            return Ok(false);
        }
        query(
            "INSERT INTO wallets (user_id, coins) VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE SET coins = wallets.coins + EXCLUDED.coins",
        )
        .bind(user_id)
        .bind(delta)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn credit_vip(
        &self,
        user_id: i64,
        delta: i64,
        reason: &str,
        reference: &str,
        at: i64,
        vip: super::VipChange,
    ) -> StoreResult<bool> {
        let mut tx = self.pool.begin().await?;
        let inserted = query(
            "INSERT INTO coin_ledger (user_id, delta, reason, reference, at) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (reference) DO NOTHING",
        )
        .bind(user_id)
        .bind(delta)
        .bind(reason)
        .bind(reference)
        .bind(at)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted == 0 {
            return Ok(false);
        }
        query(
            "INSERT INTO wallets (user_id, coins) VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE SET coins = wallets.coins + EXCLUDED.coins",
        )
        .bind(user_id)
        .bind(delta)
        .execute(&mut *tx)
        .await?;
        let (sql, a, b) = match vip {
            super::VipChange::Add { from, secs } => (
                "UPDATE wallets SET vip_until = GREATEST(vip_until, $2) + $3 WHERE user_id = $1",
                from,
                secs,
            ),
            super::VipChange::Take { secs, floor } => (
                "UPDATE wallets SET vip_until = LEAST(vip_until, GREATEST(vip_until - $3, $2))
                 WHERE user_id = $1",
                floor,
                secs,
            ),
        };
        query(sql).bind(user_id).bind(a).bind(b).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn set_vip(&self, user_id: i64, until: i64, force: bool) -> StoreResult<()> {
        let sql = if force {
            "INSERT INTO wallets (user_id, vip_until) VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE SET vip_until = EXCLUDED.vip_until"
        } else {
            "INSERT INTO wallets (user_id, vip_until) VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE SET vip_until = GREATEST(wallets.vip_until, EXCLUDED.vip_until)"
        };
        query(sql).bind(user_id).bind(until).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn add_vip(&self, user_id: i64, from: i64, secs: i64) -> StoreResult<()> {
        query(
            "INSERT INTO wallets (user_id, vip_until) VALUES ($1, $2 + $3)
             ON CONFLICT (user_id) DO UPDATE SET vip_until = GREATEST(wallets.vip_until, $2) + $3",
        )
        .bind(user_id)
        .bind(from)
        .bind(secs)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn take_vip(&self, user_id: i64, secs: i64, floor: i64) -> StoreResult<()> {
        query(
            "UPDATE wallets SET vip_until = LEAST(vip_until, GREATEST(vip_until - $2, $3))
             WHERE user_id = $1",
        )
        .bind(user_id)
        .bind(secs)
        .bind(floor)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_stripe_customer(
        &self,
        user_id: i64,
        customer: &str,
        replace: bool,
    ) -> StoreResult<()> {
        let sql = if replace {
            "INSERT INTO wallets (user_id, stripe_customer) VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE SET stripe_customer = EXCLUDED.stripe_customer"
        } else {
            "INSERT INTO wallets (user_id, stripe_customer) VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE
             SET stripe_customer = COALESCE(wallets.stripe_customer, EXCLUDED.stripe_customer)"
        };
        query(sql).bind(user_id).bind(customer).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn user_by_stripe_customer(&self, customer: &str) -> StoreResult<Option<i64>> {
        let row = query("SELECT user_id FROM wallets WHERE stripe_customer = $1")
            .bind(customer)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get("user_id")))
    }

    pub async fn record_payment(&self, p: &Payment) -> StoreResult<bool> {
        let added = query(
            "INSERT INTO payments (id, user_id, kind, coins, amount, currency, at, refunded, email, test)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) ON CONFLICT DO NOTHING",
        )
        .bind(&p.id)
        .bind(p.user_id)
        .bind(&p.kind)
        .bind(p.coins)
        .bind(p.amount)
        .bind(&p.currency)
        .bind(p.at)
        .bind(p.refunded)
        .bind(&p.email)
        .bind(p.test)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(added > 0)
    }

    pub async fn payment(&self, id: &str) -> StoreResult<Option<Payment>> {
        let row = query("SELECT * FROM payments WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| payment_row(&r)))
    }

    pub async fn set_payment_test(&self, id: &str, test: bool) -> StoreResult<bool> {
        let r = query("UPDATE payments SET test = $2 WHERE id = $1")
            .bind(id)
            .bind(test)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn refund_payment(&self, id: &str) -> StoreResult<bool> {
        let n = query("UPDATE payments SET refunded = TRUE WHERE id = $1 AND NOT refunded")
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(n > 0)
    }

    pub async fn ledger(&self, user_id: i64, limit: u32) -> StoreResult<Vec<LedgerRow>> {
        let rows = query(
            "SELECT delta, reason, reference, at FROM coin_ledger WHERE user_id = $1
             ORDER BY id DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| LedgerRow {
                delta: r.get("delta"),
                reason: r.get("reason"),
                reference: r.get("reference"),
                at: r.get("at"),
            })
            .collect())
    }

    pub async fn payments_of(&self, user_id: i64, limit: u32) -> StoreResult<Vec<Payment>> {
        let rows = query("SELECT * FROM payments WHERE user_id = $1 ORDER BY at DESC LIMIT $2")
            .bind(user_id)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(payment_row).collect())
    }
}

fn payment_row(r: &PgRow) -> Payment {
    Payment {
        id: r.get("id"),
        user_id: r.get("user_id"),
        kind: r.get("kind"),
        coins: r.get("coins"),
        amount: r.get("amount"),
        currency: r.get("currency"),
        at: r.get("at"),
        refunded: r.get("refunded"),
        email: r.get("email"),
        test: r.get("test"),
    }
}

/// The audit log's entries about the player `old` now name them `new`.
async fn move_audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    old: &str,
    new: &str,
) -> StoreResult<()> {
    query(
        "UPDATE audit_log SET target = $2
         WHERE lower(target) = lower($1) AND NOT (action = ANY($3))",
    )
    .bind(old)
    .bind(new)
    .bind(super::AUDIT_NOT_PLAYERS)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// One change to the friendship of two players at a time (for the rest of
/// the transaction).
async fn friend_lock(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    a: i64,
    b: i64,
) -> StoreResult<()> {
    let (lo, hi) = (a.min(b), a.max(b));
    query("SELECT pg_advisory_xact_lock($1)")
        .bind(lo.wrapping_mul(0x9E37_79B9_7F4A_7C15_u64 as i64) ^ hi)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
