//! Account security in memory (the same rules as the database: see
//! `store/postgres/security.rs` and migration 0037).

use super::super::{Device, SessionKind, StoreResult, TokenPurpose, Totp, User};
use super::MemoryStore;

/// A one-time link as kept here.
#[derive(Debug, Clone)]
pub(super) struct TokenRow {
    user_id: i64,
    purpose: TokenPurpose,
    email: String,
    expires_at: i64,
}

impl MemoryStore {
    pub fn confirmed_email(&self, user_id: i64) -> StoreResult<Option<String>> {
        Ok(self.lock().confirmed_emails.get(&user_id).cloned())
    }

    pub fn confirm_email(&self, user_id: i64, email: &str) -> StoreResult<()> {
        self.lock().confirmed_emails.insert(user_id, email.to_lowercase());
        Ok(())
    }

    pub fn users_by_confirmed_email(&self, email: &str, limit: u32) -> StoreResult<Vec<User>> {
        let s = self.lock();
        let email = email.to_lowercase();
        Ok(s.users
            .values()
            .filter(|u| {
                u.deleted_at.is_none()
                    && u.recovery_email.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(&email))
                    && s.confirmed_emails.get(&u.id).is_some_and(|c| *c == email)
            })
            .take(limit as usize)
            .cloned()
            .collect())
    }

    pub fn replace_account_token(
        &self,
        hash: &str,
        user_id: i64,
        purpose: TokenPurpose,
        email: &str,
        _now: i64,
        expires_at: i64,
    ) -> StoreResult<()> {
        let mut s = self.lock();
        s.account_tokens.retain(|_, t| !(t.user_id == user_id && t.purpose == purpose));
        s.account_tokens.insert(
            hash.to_string(),
            TokenRow { user_id, purpose, email: email.to_lowercase(), expires_at },
        );
        Ok(())
    }

    pub fn take_account_token(
        &self,
        hash: &str,
        purpose: TokenPurpose,
        now: i64,
    ) -> StoreResult<Option<(i64, String)>> {
        let mut s = self.lock();
        let ok = s.account_tokens.get(hash).is_some_and(|t| t.purpose == purpose);
        if !ok {
            return Ok(None);
        }
        let t = s.account_tokens.remove(hash).expect("checked");
        Ok((t.expires_at > now).then_some((t.user_id, t.email)))
    }

    pub fn totp(&self, user_id: i64) -> StoreResult<Option<Totp>> {
        Ok(self.lock().totp.get(&user_id).cloned())
    }

    pub fn enable_totp(
        &self,
        user_id: i64,
        secret: &str,
        step: i64,
        now: i64,
        recovery_hashes: &[String],
    ) -> StoreResult<()> {
        let mut s = self.lock();
        s.totp
            .insert(user_id, Totp { secret: secret.to_string(), enabled_at: now, last_step: step });
        s.recovery_codes.insert(user_id, recovery_hashes.iter().cloned().collect());
        Ok(())
    }

    pub fn disable_totp(&self, user_id: i64) -> StoreResult<()> {
        let mut s = self.lock();
        s.totp.remove(&user_id);
        s.recovery_codes.remove(&user_id);
        Ok(())
    }

    pub fn use_totp_step(&self, user_id: i64, step: i64) -> StoreResult<bool> {
        let mut s = self.lock();
        match s.totp.get_mut(&user_id) {
            Some(t) if t.last_step < step => {
                t.last_step = step;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn use_recovery_code(&self, user_id: i64, hash: &str) -> StoreResult<bool> {
        Ok(self.lock().recovery_codes.get_mut(&user_id).is_some_and(|c| c.remove(hash)))
    }

    pub fn set_recovery_codes(&self, user_id: i64, hashes: &[String]) -> StoreResult<()> {
        self.lock().recovery_codes.insert(user_id, hashes.iter().cloned().collect());
        Ok(())
    }

    pub fn recovery_codes_left(&self, user_id: i64) -> StoreResult<u32> {
        Ok(self.lock().recovery_codes.get(&user_id).map_or(0, |c| c.len() as u32))
    }

    pub fn create_device(&self, d: &Device) -> StoreResult<()> {
        self.lock().devices.insert(d.token_hash.clone(), d.clone());
        Ok(())
    }

    pub fn device_user(&self, hash: &str, now: i64) -> StoreResult<Option<(User, Device)>> {
        let s = self.lock();
        Ok(s.devices
            .get(hash)
            .filter(|d| d.expires_at > now)
            .and_then(|d| s.users.get(&d.user_id).map(|u| (u.clone(), d.clone()))))
    }

    pub fn touch_device(&self, hash: &str, now: i64, expires_at: i64) -> StoreResult<()> {
        if let Some(d) = self.lock().devices.get_mut(hash) {
            d.last_used_at = now;
            d.expires_at = expires_at;
        }
        Ok(())
    }

    pub fn devices(&self, user_id: i64, now: i64) -> StoreResult<Vec<Device>> {
        let mut out: Vec<Device> = self
            .lock()
            .devices
            .values()
            .filter(|d| d.user_id == user_id && d.expires_at > now)
            .cloned()
            .collect();
        out.sort_by(|a, b| {
            b.last_used_at.cmp(&a.last_used_at).then(a.token_hash.cmp(&b.token_hash))
        });
        Ok(out)
    }

    pub fn delete_device(&self, user_id: i64, hash: &str) -> StoreResult<bool> {
        let mut s = self.lock();
        let mine = s.devices.get(hash).is_some_and(|d| d.user_id == user_id);
        if mine {
            s.devices.remove(hash);
        }
        Ok(mine)
    }

    pub fn delete_staff_sessions(&self, user_id: i64) -> StoreResult<()> {
        let mut s = self.lock();
        s.devices.retain(|_, d| d.user_id != user_id);
        s.sessions.retain(|_, (id, kind, ..)| !(*id == user_id && *kind == SessionKind::Admin));
        Ok(())
    }

    /// What an account deleted or signed out everywhere leaves here.
    pub(super) fn forget_security(s: &mut super::Inner, user_id: i64, deleted: bool) {
        s.devices.retain(|_, d| d.user_id != user_id);
        if deleted {
            s.confirmed_emails.remove(&user_id);
            s.account_tokens.retain(|_, t| t.user_id != user_id);
            s.totp.remove(&user_id);
            s.recovery_codes.remove(&user_id);
        }
    }

    /// Expired links and devices.
    pub(super) fn purge_security(s: &mut super::Inner, now: i64) -> usize {
        let before = s.account_tokens.len() + s.devices.len();
        s.account_tokens.retain(|_, t| t.expires_at > now);
        s.devices.retain(|_, d| d.expires_at > now);
        before - s.account_tokens.len() - s.devices.len()
    }
}
