//! Account security in PostgreSQL (migration 0037; see `store/security.rs`).

use sqlx::{Row, query};

use super::super::{Device, SessionKind, StoreResult, TokenPurpose, Totp, User};
use super::{PgStore, USER_COLUMNS, user};

fn device(r: &sqlx::postgres::PgRow) -> Result<Device, sqlx::Error> {
    Ok(Device {
        token_hash: r.try_get("token_hash")?,
        user_id: r.try_get("user_id")?,
        label: r.try_get("label")?,
        created_at: r.try_get("created_at")?,
        last_used_at: r.try_get("last_used_at")?,
        expires_at: r.try_get("expires_at")?,
    })
}

/// The users table's columns prefixed with `u.`.
fn user_columns() -> String {
    USER_COLUMNS.split(", ").map(|c| format!("u.{}", c.trim())).collect::<Vec<_>>().join(", ")
}

impl PgStore {
    pub async fn confirmed_email(&self, user_id: i64) -> StoreResult<Option<String>> {
        let row = query("SELECT confirmed_email FROM account_security WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.and_then(|r| r.get("confirmed_email")))
    }

    pub async fn confirm_email(&self, user_id: i64, email: &str) -> StoreResult<()> {
        query(
            "INSERT INTO account_security (user_id, confirmed_email) VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE SET confirmed_email = EXCLUDED.confirmed_email",
        )
        .bind(user_id)
        .bind(email.to_lowercase())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn users_by_confirmed_email(
        &self,
        email: &str,
        limit: u32,
    ) -> StoreResult<Vec<User>> {
        let rows = query(&format!(
            "SELECT {} FROM account_security a JOIN users u ON u.id = a.user_id
             WHERE a.confirmed_email = $1 AND lower(u.recovery_email) = $1
               AND u.deleted_at IS NULL
             ORDER BY u.id LIMIT $2",
            user_columns()
        ))
        .bind(email.to_lowercase())
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(user).collect::<Result<_, _>>()?)
    }

    pub async fn replace_account_token(
        &self,
        hash: &str,
        user_id: i64,
        purpose: TokenPurpose,
        email: &str,
        now: i64,
        expires_at: i64,
    ) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        query("DELETE FROM account_tokens WHERE user_id = $1 AND purpose = $2")
            .bind(user_id)
            .bind(purpose as i16)
            .execute(&mut *tx)
            .await?;
        query(
            "INSERT INTO account_tokens (token_hash, user_id, purpose, email, created_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(hash)
        .bind(user_id)
        .bind(purpose as i16)
        .bind(email.to_lowercase())
        .bind(now)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn take_account_token(
        &self,
        hash: &str,
        purpose: TokenPurpose,
        now: i64,
    ) -> StoreResult<Option<(i64, String)>> {
        // Gone once read, valid or not: a link works once.
        let row = query(
            "DELETE FROM account_tokens WHERE token_hash = $1 AND purpose = $2
             RETURNING user_id, email, expires_at",
        )
        .bind(hash)
        .bind(purpose as i16)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.and_then(|r| {
            let expires: i64 = r.get("expires_at");
            (expires > now).then(|| (r.get("user_id"), r.get("email")))
        }))
    }

    pub async fn totp(&self, user_id: i64) -> StoreResult<Option<Totp>> {
        let row = query(
            "SELECT totp_secret, totp_enabled_at, totp_last_step FROM account_security
             WHERE user_id = $1 AND totp_secret IS NOT NULL",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| Totp {
            secret: r.get("totp_secret"),
            enabled_at: r.get::<Option<i64>, _>("totp_enabled_at").unwrap_or(0),
            last_step: r.get("totp_last_step"),
        }))
    }

    pub async fn enable_totp(
        &self,
        user_id: i64,
        secret: &str,
        step: i64,
        now: i64,
        recovery_hashes: &[String],
    ) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        query(
            "INSERT INTO account_security (user_id, totp_secret, totp_enabled_at, totp_last_step)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id) DO UPDATE SET totp_secret = EXCLUDED.totp_secret,
                totp_enabled_at = EXCLUDED.totp_enabled_at,
                totp_last_step = EXCLUDED.totp_last_step",
        )
        .bind(user_id)
        .bind(secret)
        .bind(now)
        .bind(step)
        .execute(&mut *tx)
        .await?;
        query("DELETE FROM recovery_codes WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        for h in recovery_hashes {
            query(
                "INSERT INTO recovery_codes (user_id, code_hash) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(user_id)
            .bind(h)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn disable_totp(&self, user_id: i64) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        query(
            "UPDATE account_security SET totp_secret = NULL, totp_enabled_at = NULL, totp_last_step = 0
             WHERE user_id = $1",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        query("DELETE FROM recovery_codes WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn use_totp_step(&self, user_id: i64, step: i64) -> StoreResult<bool> {
        let r = query(
            "UPDATE account_security SET totp_last_step = $2
             WHERE user_id = $1 AND totp_secret IS NOT NULL AND totp_last_step < $2",
        )
        .bind(user_id)
        .bind(step)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() == 1)
    }

    pub async fn use_recovery_code(&self, user_id: i64, hash: &str) -> StoreResult<bool> {
        let r = query("DELETE FROM recovery_codes WHERE user_id = $1 AND code_hash = $2")
            .bind(user_id)
            .bind(hash)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() == 1)
    }

    pub async fn set_recovery_codes(&self, user_id: i64, hashes: &[String]) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        query("DELETE FROM recovery_codes WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        for h in hashes {
            query(
                "INSERT INTO recovery_codes (user_id, code_hash) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(user_id)
            .bind(h)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn recovery_codes_left(&self, user_id: i64) -> StoreResult<u32> {
        let n: i64 = query("SELECT count(*) AS n FROM recovery_codes WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await?
            .get("n");
        Ok(n as u32)
    }

    pub async fn create_device(&self, d: &Device) -> StoreResult<()> {
        query(
            "INSERT INTO staff_devices (token_hash, user_id, label, created_at, last_used_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&d.token_hash)
        .bind(d.user_id)
        .bind(&d.label)
        .bind(d.created_at)
        .bind(d.last_used_at)
        .bind(d.expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn device_user(&self, hash: &str, now: i64) -> StoreResult<Option<(User, Device)>> {
        let row = query(&format!(
            "SELECT {}, d.token_hash, d.user_id, d.label, d.created_at, d.last_used_at, d.expires_at
             FROM staff_devices d JOIN users u ON u.id = d.user_id
             WHERE d.token_hash = $1 AND d.expires_at > $2",
            user_columns()
        ))
        .bind(hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) => Ok(Some((user(&r)?, device(&r)?))),
            None => Ok(None),
        }
    }

    pub async fn touch_device(&self, hash: &str, now: i64, expires_at: i64) -> StoreResult<()> {
        query("UPDATE staff_devices SET last_used_at = $2, expires_at = $3 WHERE token_hash = $1")
            .bind(hash)
            .bind(now)
            .bind(expires_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn devices(&self, user_id: i64, now: i64) -> StoreResult<Vec<Device>> {
        let rows = query(
            "SELECT token_hash, user_id, label, created_at, last_used_at, expires_at
             FROM staff_devices WHERE user_id = $1 AND expires_at > $2
             ORDER BY last_used_at DESC, token_hash",
        )
        .bind(user_id)
        .bind(now)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(device).collect::<Result<_, _>>()?)
    }

    pub async fn delete_device(&self, user_id: i64, hash: &str) -> StoreResult<bool> {
        let r = query("DELETE FROM staff_devices WHERE user_id = $1 AND token_hash = $2")
            .bind(user_id)
            .bind(hash)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected() == 1)
    }

    pub async fn delete_staff_sessions(&self, user_id: i64) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        query("DELETE FROM staff_devices WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
        query("DELETE FROM sessions WHERE user_id = $1 AND kind = $2")
            .bind(user_id)
            .bind(SessionKind::Admin as i16)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
