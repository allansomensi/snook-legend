//! The inbox and the moderation in PostgreSQL (migration 0027: tables
//! `notifications`, `banned_words`, `profile_changes`,
//! `rating_adjustments`, `shot_times` and `matches.voided_at`).

use snook_legend_proto::Rules;
use sqlx::postgres::PgRow;
use sqlx::{Row, query};

use super::super::{
    BannedWord, LinkedAccount, MatchRow, NewNotification, NotificationRow, ProfileChangeRow,
    RatingAdjustment, ReportRow, StoreResult, User,
};
use super::{PgStore, USER_COLUMNS, record_columns, user};

const NOTIFICATION_SELECT: &str = "SELECT n.id, n.user_id, n.kind::text AS kind, n.text, \
    COALESCE(f.username, '') AS from_name, n.from_id, n.created_at, n.read_at, n.important, \
    n.pinned_until, n.action::text AS action, n.deleted_at \
    FROM notifications n LEFT JOIN users f ON f.id = n.from_id";

fn notification(r: &PgRow) -> Result<NotificationRow, sqlx::Error> {
    Ok(NotificationRow {
        id: r.try_get("id")?,
        user_id: r.try_get("user_id")?,
        kind: r.try_get("kind")?,
        text: r.try_get("text")?,
        from: r.try_get("from_name")?,
        from_id: r.try_get("from_id")?,
        created_at: r.try_get("created_at")?,
        read_at: r.try_get("read_at")?,
        important: r.try_get("important")?,
        pinned_until: r.try_get("pinned_until")?,
        action: r.try_get("action")?,
        deleted_at: r.try_get("deleted_at")?,
    })
}

fn adjustment(r: &PgRow) -> Result<RatingAdjustment, sqlx::Error> {
    let rules: String = r.try_get("rules")?;
    Ok(RatingAdjustment {
        id: r.try_get("id")?,
        user_id: r.try_get("user_id")?,
        rules: Rules::parse(&rules).unwrap_or_default(),
        delta: r.try_get("delta")?,
        rating_after: r.try_get("rating_after")?,
        reason: r.try_get("reason")?,
        match_id: r.try_get("match_id")?,
        staff: r.try_get("staff")?,
        at: r.try_get("at")?,
    })
}

/// Puts a rating right inside a transaction (see [`PgStore::adjust_rating`]):
/// the peak follows (up with it, or down by as much, never under the
/// rating), and the change is kept in the rating history.
#[allow(clippy::too_many_arguments)]
async fn adjust_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: i64,
    rules: Rules,
    delta: i32,
    reason: &str,
    match_id: Option<i64>,
    staff_id: i64,
    at: i64,
) -> StoreResult<Option<User>> {
    let p = record_columns(rules);
    let row = query(&format!(
        "UPDATE users SET {p}rating = {p}rating + $2,
            {p}peak_rating = CASE WHEN $2 >= 0 THEN GREATEST({p}peak_rating, {p}rating + $2)
                                  ELSE GREATEST({p}peak_rating + $2, {p}rating + $2) END
         WHERE id = $1 RETURNING {USER_COLUMNS}"
    ))
    .bind(user_id)
    .bind(delta)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let fresh = user(&row)?;
    query(
        "INSERT INTO rating_adjustments (user_id, rules, delta, rating_after, reason, match_id, staff_id, at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(user_id)
    .bind(rules.code())
    .bind(delta)
    .bind(fresh.records.get(rules).rating)
    .bind(reason)
    .bind(match_id)
    .bind(staff_id)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(Some(fresh))
}

impl PgStore {
    pub async fn add_notification(&self, n: &NewNotification<'_>, keep: usize) -> StoreResult<i64> {
        let mut tx = self.pool.begin().await?;
        let id: i64 = query(
            "INSERT INTO notifications (user_id, kind, text, from_id, created_at, important, pinned_until, action)
             VALUES ($1, $2::jsonb, $3, $4, $5, $6, $7, $8::jsonb) RETURNING id",
        )
        .bind(n.user_id)
        .bind(n.kind)
        .bind(n.text)
        .bind(n.from_id)
        .bind(n.created_at)
        .bind(n.important)
        .bind(n.pinned_until)
        .bind(n.action)
        .fetch_one(&mut *tx)
        .await?
        .get("id");
        // The oldest read ones that aren't pinned go, past `keep`.
        query(
            "DELETE FROM notifications WHERE id IN (
                SELECT id FROM notifications
                WHERE user_id = $1 AND deleted_at IS NULL AND read_at IS NOT NULL AND pinned_until <= $3
                ORDER BY id
                LIMIT GREATEST(0, (SELECT COUNT(*) FROM notifications
                                   WHERE user_id = $1 AND deleted_at IS NULL) - $2))",
        )
        .bind(n.user_id)
        .bind(keep as i64)
        .bind(n.created_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn notifications(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<NotificationRow>> {
        let rows = query(&format!(
            "{NOTIFICATION_SELECT} WHERE n.user_id = $1 AND n.deleted_at IS NULL ORDER BY n.id DESC LIMIT $2"
        ))
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(notification).collect::<Result<_, _>>()?)
    }

    pub async fn notifications_sent(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<NotificationRow>> {
        let rows = query(&format!(
            "{NOTIFICATION_SELECT} WHERE n.user_id = $1 ORDER BY n.id DESC LIMIT $2"
        ))
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(notification).collect::<Result<_, _>>()?)
    }

    pub async fn read_notifications(&self, user_id: i64, ids: &[i64], at: i64) -> StoreResult<u64> {
        let r = query(
            "UPDATE notifications SET read_at = $3
             WHERE user_id = $1 AND id = ANY($2) AND read_at IS NULL AND deleted_at IS NULL",
        )
        .bind(user_id)
        .bind(ids)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected())
    }

    pub async fn delete_notifications(
        &self,
        user_id: i64,
        ids: &[i64],
        now: i64,
    ) -> StoreResult<u64> {
        let r = query(
            "UPDATE notifications SET deleted_at = $3
             WHERE user_id = $1 AND id = ANY($2) AND deleted_at IS NULL AND pinned_until <= $3
               AND NOT (important AND read_at IS NULL)",
        )
        .bind(user_id)
        .bind(ids)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected())
    }

    pub async fn banned_words(&self) -> StoreResult<Vec<BannedWord>> {
        let rows = query(
            "SELECT w.word, w.lang, COALESCE(u.username, '') AS added_by, w.added_at
             FROM banned_words w LEFT JOIN users u ON u.id = w.added_by ORDER BY w.lang, w.word",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                Ok(BannedWord {
                    word: r.try_get("word")?,
                    lang: r.try_get("lang")?,
                    added_by: r.try_get("added_by")?,
                    added_at: r.try_get("added_at")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()?)
    }

    pub async fn add_banned_word(
        &self,
        word: &str,
        lang: &str,
        by: i64,
        at: i64,
    ) -> StoreResult<bool> {
        let r = query(
            "INSERT INTO banned_words (word, lang, added_by, added_at) VALUES ($1, $2, $3, $4)
             ON CONFLICT DO NOTHING",
        )
        .bind(word)
        .bind(lang)
        .bind(by)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn remove_banned_word(&self, word: &str, lang: &str) -> StoreResult<bool> {
        let r = query("DELETE FROM banned_words WHERE word = $1 AND lang = $2")
            .bind(word)
            .bind(lang)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    pub async fn add_profile_change(
        &self,
        user_id: i64,
        kind: &str,
        old: &str,
        new: &str,
        at: i64,
        by: Option<i64>,
    ) -> StoreResult<()> {
        query(
            "INSERT INTO profile_changes (user_id, kind, old, new, at, by_id) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(user_id)
        .bind(kind)
        .bind(old)
        .bind(new)
        .bind(at)
        .bind(by)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn profile_changes(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<ProfileChangeRow>> {
        let rows = query(
            "SELECT c.user_id, c.kind, c.old, c.new, c.at, COALESCE(u.username, '') AS by_name
             FROM profile_changes c LEFT JOIN users u ON u.id = c.by_id
             WHERE c.user_id = $1 ORDER BY c.at DESC, c.id DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                Ok(ProfileChangeRow {
                    user_id: r.try_get("user_id")?,
                    kind: r.try_get("kind")?,
                    old: r.try_get("old")?,
                    new: r.try_get("new")?,
                    at: r.try_get("at")?,
                    by: r.try_get("by_name")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()?)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn adjust_rating(
        &self,
        user_id: i64,
        rules: Rules,
        delta: i32,
        reason: &str,
        match_id: Option<i64>,
        staff_id: i64,
        at: i64,
    ) -> StoreResult<Option<User>> {
        let mut tx = self.pool.begin().await?;
        let fresh =
            adjust_in(&mut tx, user_id, rules, delta, reason, match_id, staff_id, at).await?;
        tx.commit().await?;
        Ok(fresh)
    }

    pub async fn rating_adjustments(
        &self,
        user_id: i64,
        limit: u32,
    ) -> StoreResult<Vec<RatingAdjustment>> {
        let rows = query(
            "SELECT a.id, a.user_id, a.rules, a.delta, a.rating_after, a.reason, a.match_id,
                    COALESCE(s.username, '') AS staff, a.at
             FROM rating_adjustments a LEFT JOIN users s ON s.id = a.staff_id
             WHERE a.user_id = $1 ORDER BY a.at DESC, a.id DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(adjustment).collect::<Result<_, _>>()?)
    }

    pub async fn match_row(&self, id: i64) -> StoreResult<Option<MatchRow>> {
        let row = query(
            "SELECT m.id, m.player0, m.player1, a.username AS n0, b.username AS n1, m.winner_seat,
                    m.reason, m.rated, m.rules, m.delta0, m.delta1, m.rating0, m.rating1, m.shots,
                    m.ended_at, m.tournament_id, m.room_id, m.voided_at
             FROM matches m JOIN users a ON a.id = m.player0 JOIN users b ON b.id = m.player1
             WHERE m.id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(r) = row else { return Ok(None) };
        let rules: String = r.try_get("rules")?;
        Ok(Some(MatchRow {
            id: r.try_get("id")?,
            players: [r.try_get("player0")?, r.try_get("player1")?],
            names: [r.try_get("n0")?, r.try_get("n1")?],
            winner: r.try_get::<i16, _>("winner_seat")? as usize % 2,
            reason: r.try_get("reason")?,
            rated: r.try_get("rated")?,
            rules: Rules::parse(&rules).unwrap_or_default(),
            delta: [r.try_get("delta0")?, r.try_get("delta1")?],
            ratings_after: [r.try_get("rating0")?, r.try_get("rating1")?],
            shots: r.try_get::<i32, _>("shots")?.max(0) as u32,
            ended_at: r.try_get("ended_at")?,
            tournament: r.try_get("tournament_id")?,
            room: r.try_get("room_id")?,
            voided_at: r.try_get("voided_at")?,
        }))
    }

    pub async fn void_match(
        &self,
        id: i64,
        staff_id: i64,
        reason: &str,
        at: i64,
    ) -> StoreResult<Option<[Option<User>; 2]>> {
        let mut tx = self.pool.begin().await?;
        // Called off once, whatever two staff members do at the same time.
        let row = query(
            "UPDATE matches SET voided_at = $2 WHERE id = $1 AND rated AND voided_at IS NULL
             RETURNING player0, player1, winner_seat, rules, delta0, delta1, rating0, rating1",
        )
        .bind(id)
        .bind(at)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(m) = row else { return Ok(None) };
        let rules: String = m.try_get("rules")?;
        let rules = Rules::parse(&rules).unwrap_or_default();
        let winner = m.try_get::<i16, _>("winner_seat")? as usize % 2;
        let p = record_columns(rules);
        let mut out = [None, None];
        for (seat, slot) in out.iter_mut().enumerate() {
            let user_id: i64 = m.try_get(format!("player{seat}").as_str())?;
            let delta: i32 = m.try_get(format!("delta{seat}").as_str())?;
            let after: i32 = m.try_get(format!("rating{seat}").as_str())?;
            let old_peak: Option<i32> = query(&format!(
                "SELECT {p}peak_rating AS peak FROM users WHERE id = $1 FOR UPDATE"
            ))
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?
            .map(|r| r.get("peak"));
            let Some(old_peak) = old_peak else { continue };
            if adjust_in(&mut tx, user_id, rules, -delta, reason, Some(id), staff_id, at)
                .await?
                .is_none()
            {
                continue;
            }
            // Out of the rated games (and the wins) it counted in. The peak
            // it made is what the player had before it, or after another
            // game, never above what it was.
            let row = query(&format!(
                "UPDATE users SET
                    {p}games = GREATEST({p}games - 1, 0),
                    {p}wins = GREATEST({p}wins - $3, 0),
                    {p}peak_rating = CASE WHEN $4 = $5 THEN LEAST($4, GREATEST({p}rating, $5 - $6,
                        COALESCE((SELECT MAX(CASE WHEN player0 = $1 THEN rating0 ELSE rating1 END)
                                  FROM matches WHERE rated AND rules = $2 AND voided_at IS NULL
                                    AND (player0 = $1 OR player1 = $1)), {p}rating)))
                      ELSE GREATEST($4, {p}rating) END
                 WHERE id = $1 RETURNING {USER_COLUMNS}"
            ))
            .bind(user_id)
            .bind(rules.code())
            .bind((winner == seat) as i32)
            .bind(old_peak)
            .bind(after)
            .bind(delta)
            .fetch_optional(&mut *tx)
            .await?;
            *slot = row.map(|r| user(&r)).transpose()?;
        }
        tx.commit().await?;
        Ok(Some(out))
    }

    pub async fn save_shot_times(
        &self,
        room: i64,
        times: &[Option<u32>],
        at: i64,
    ) -> StoreResult<()> {
        let json = serde_json::to_string(times).unwrap_or_else(|_| "[]".into());
        query(
            "INSERT INTO shot_times (room_id, times, at) VALUES ($1, $2::jsonb, $3)
             ON CONFLICT (room_id) DO UPDATE SET times = EXCLUDED.times, at = EXCLUDED.at",
        )
        .bind(room)
        .bind(json)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn shot_times(&self, room: i64) -> StoreResult<Option<Vec<Option<u32>>>> {
        let row = query("SELECT times::text AS times FROM shot_times WHERE room_id = $1")
            .bind(room)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.and_then(|r| serde_json::from_str(&r.get::<String, _>("times")).ok()))
    }

    pub async fn reports_by(&self, user_id: i64, limit: u32) -> StoreResult<Vec<ReportRow>> {
        let rows = query(&format!(
            "{} WHERE r.reporter_id = $1 ORDER BY r.created_at DESC LIMIT $2",
            Self::REPORT_SELECT
        ))
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(Self::report_row).collect::<Result<_, _>>()?)
    }

    pub async fn linked_accounts(&self, user_id: i64) -> StoreResult<Vec<LinkedAccount>> {
        let rows = query(
            "SELECT o.id, o.username, o.created_at FROM users me JOIN users o
               ON o.register_ip = me.register_ip AND o.id <> me.id
             WHERE me.id = $1 AND me.register_ip <> '' ORDER BY o.created_at DESC LIMIT 50",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                Ok(LinkedAccount {
                    id: r.try_get("id")?,
                    username: r.try_get("username")?,
                    created_at: r.try_get("created_at")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()?)
    }
}
