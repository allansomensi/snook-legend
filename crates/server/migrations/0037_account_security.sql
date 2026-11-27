-- 0.12.3: what keeps accounts safe (see `store/security.rs`).
--
-- account_security: the recovery e-mail the player confirmed (an address
-- counts as confirmed while it is the account's `recovery_email`), and a
-- staff member's two-step verification (TOTP secret, when it was turned
-- on, and the last step used: a code is never accepted twice).
CREATE TABLE account_security (
    user_id         BIGINT PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    confirmed_email TEXT,
    totp_secret     TEXT,
    totp_enabled_at BIGINT,
    totp_last_step  BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX account_security_email ON account_security (confirmed_email)
    WHERE confirmed_email IS NOT NULL;

-- Every recovery e-mail set until now was typed with the account's
-- password: they count as confirmed.
INSERT INTO account_security (user_id, confirmed_email)
SELECT id, lower(recovery_email) FROM users
WHERE recovery_email IS NOT NULL AND deleted_at IS NULL;

-- One-time links sent by e-mail (1: a new password, 2: confirming the
-- address), by the hash of their token; only an account's latest link of
-- each kind works, and each works once.
CREATE TABLE account_tokens (
    token_hash TEXT     PRIMARY KEY,
    user_id    BIGINT   NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    purpose    SMALLINT NOT NULL,
    email      TEXT     NOT NULL,
    created_at BIGINT   NOT NULL,
    expires_at BIGINT   NOT NULL
);
CREATE INDEX account_tokens_user ON account_tokens (user_id, purpose);
CREATE INDEX account_tokens_expiry ON account_tokens (expires_at);

-- The staff's recovery codes (hashes), each used once.
CREATE TABLE recovery_codes (
    user_id   BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    code_hash TEXT   NOT NULL,
    PRIMARY KEY (user_id, code_hash)
);

-- Devices kept signed in to the staff portal ("keep me signed in on this
-- device"): each use moves its end 30 days on.
CREATE TABLE staff_devices (
    token_hash   TEXT   PRIMARY KEY,
    user_id      BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    label        TEXT   NOT NULL,
    created_at   BIGINT NOT NULL,
    last_used_at BIGINT NOT NULL,
    expires_at   BIGINT NOT NULL
);
CREATE INDEX staff_devices_user ON staff_devices (user_id);
CREATE INDEX staff_devices_expiry ON staff_devices (expires_at);
