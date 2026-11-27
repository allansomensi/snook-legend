//! What keeps accounts safe (migration 0037): the recovery e-mail each
//! player confirmed, the one-time links sent by e-mail (a forgotten
//! password, an address to confirm), the staff's two-step verification
//! with its recovery codes, and the devices a staff member keeps signed in
//! to the portal.
//!
//! An account's recovery e-mail counts as confirmed when it is the address
//! kept here (`confirmed_email`): changing it to a new one leaves it
//! unconfirmed until its link is opened, and going back to the confirmed
//! one needs nothing. Addresses set before 0.12.3 were all confirmed by
//! the migration (each was typed with the account's password).

use super::{Store, StoreResult, User};

/// What a one-time link is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenPurpose {
    /// Choosing a new password (sent to the confirmed recovery e-mail).
    PasswordReset = 1,
    /// Confirming the recovery e-mail it was sent to.
    ConfirmEmail = 2,
}

/// A staff member's two-step verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Totp {
    /// The shared secret (base32).
    pub secret: String,
    /// When it was turned on (Unix seconds).
    pub enabled_at: i64,
    /// The last step whose code was used (none is accepted twice).
    pub last_step: i64,
}

/// A device kept signed in to the staff portal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// The hash of its token (only the start of it is shown).
    pub token_hash: String,
    pub user_id: i64,
    /// The browser and system it signed in from ("Chrome · Android").
    pub label: String,
    pub created_at: i64,
    pub last_used_at: i64,
    pub expires_at: i64,
}

impl Store {
    /// The recovery e-mail the player confirmed, if any.
    pub async fn confirmed_email(&self, user_id: i64) -> StoreResult<Option<String>> {
        dispatch!(self, s => s.confirmed_email(user_id))
    }
    /// Marks `email` as the player's confirmed recovery e-mail.
    pub async fn confirm_email(&self, user_id: i64, email: &str) -> StoreResult<()> {
        dispatch!(self, s => s.confirm_email(user_id, email))
    }
    /// The accounts (not deleted) whose confirmed recovery e-mail is
    /// `email`, at most `limit` of them.
    pub async fn users_by_confirmed_email(
        &self,
        email: &str,
        limit: u32,
    ) -> StoreResult<Vec<User>> {
        dispatch!(self, s => s.users_by_confirmed_email(email, limit))
    }

    /// Keeps a new one-time link of `purpose` for the account, sent to
    /// `email`; the account's earlier links of that purpose stop working
    /// (only the latest e-mail's link does).
    pub async fn replace_account_token(
        &self,
        token_hash: &str,
        user_id: i64,
        purpose: TokenPurpose,
        email: &str,
        now: i64,
        expires_at: i64,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.replace_account_token(token_hash, user_id, purpose, email, now, expires_at))
    }
    /// Uses a one-time link: the account and the address it was sent to,
    /// if it exists, is for `purpose` and has not expired. It works once.
    pub async fn take_account_token(
        &self,
        token_hash: &str,
        purpose: TokenPurpose,
        now: i64,
    ) -> StoreResult<Option<(i64, String)>> {
        dispatch!(self, s => s.take_account_token(token_hash, purpose, now))
    }

    /// The account's two-step verification, if it is on.
    pub async fn totp(&self, user_id: i64) -> StoreResult<Option<Totp>> {
        dispatch!(self, s => s.totp(user_id))
    }
    /// Turns two-step verification on with `secret` (its code for `step`
    /// was just used) and these recovery codes (their hashes), replacing
    /// any there were.
    pub async fn enable_totp(
        &self,
        user_id: i64,
        secret: &str,
        step: i64,
        now: i64,
        recovery_hashes: &[String],
    ) -> StoreResult<()> {
        dispatch!(self, s => s.enable_totp(user_id, secret, step, now, recovery_hashes))
    }
    /// Turns it off, with its recovery codes.
    pub async fn disable_totp(&self, user_id: i64) -> StoreResult<()> {
        dispatch!(self, s => s.disable_totp(user_id))
    }
    /// Records that the code of `step` was used; false if that step (or a
    /// later one) already was, so a code is never accepted twice.
    pub async fn use_totp_step(&self, user_id: i64, step: i64) -> StoreResult<bool> {
        dispatch!(self, s => s.use_totp_step(user_id, step))
    }
    /// Uses a recovery code (by its hash): false if the account has no
    /// such code left.
    pub async fn use_recovery_code(&self, user_id: i64, code_hash: &str) -> StoreResult<bool> {
        dispatch!(self, s => s.use_recovery_code(user_id, code_hash))
    }
    /// Replaces the account's recovery codes.
    pub async fn set_recovery_codes(&self, user_id: i64, hashes: &[String]) -> StoreResult<()> {
        dispatch!(self, s => s.set_recovery_codes(user_id, hashes))
    }
    /// How many recovery codes the account has left.
    pub async fn recovery_codes_left(&self, user_id: i64) -> StoreResult<u32> {
        dispatch!(self, s => s.recovery_codes_left(user_id))
    }

    /// Keeps a device signed in to the portal.
    pub async fn create_device(&self, d: &Device) -> StoreResult<()> {
        dispatch!(self, s => s.create_device(d))
    }
    /// The staff member and the device of a device token's hash, if it
    /// hasn't expired.
    pub async fn device_user(
        &self,
        token_hash: &str,
        now: i64,
    ) -> StoreResult<Option<(User, Device)>> {
        dispatch!(self, s => s.device_user(token_hash, now))
    }
    /// The device was used: it stays signed in until `expires_at`.
    pub async fn touch_device(
        &self,
        token_hash: &str,
        now: i64,
        expires_at: i64,
    ) -> StoreResult<()> {
        dispatch!(self, s => s.touch_device(token_hash, now, expires_at))
    }
    /// The staff member's devices that haven't expired, the latest used first.
    pub async fn devices(&self, user_id: i64, now: i64) -> StoreResult<Vec<Device>> {
        dispatch!(self, s => s.devices(user_id, now))
    }
    /// Signs one device out; false if the account has none with that hash.
    pub async fn delete_device(&self, user_id: i64, token_hash: &str) -> StoreResult<bool> {
        dispatch!(self, s => s.delete_device(user_id, token_hash))
    }
    /// Signs out of the portal everywhere: every device and portal session
    /// of the account (its game sessions stay).
    pub async fn delete_staff_sessions(&self, user_id: i64) -> StoreResult<()> {
        dispatch!(self, s => s.delete_staff_sessions(user_id))
    }
}
