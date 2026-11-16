-- A short line each player writes about themself ("looking for a practice
-- partner"), shown on the profile and in the lounge. Empty: none.
ALTER TABLE users ADD COLUMN status_message TEXT NOT NULL DEFAULT '';

-- Presents between friends: items bought with coins, and VIP months paid
-- for a friend. The receiver is told (`seen` once they saw it) and may
-- thank the giver once (`thanked`).
CREATE TABLE gifts (
    id        BIGSERIAL PRIMARY KEY,
    from_id   BIGINT  REFERENCES users (id) ON DELETE SET NULL,
    to_id     BIGINT  NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- An item code, or 'vip_gift' (a month of VIP).
    code      TEXT    NOT NULL,
    -- A few words from the giver (may be empty).
    message   TEXT    NOT NULL DEFAULT '',
    at        BIGINT  NOT NULL,
    seen      BOOLEAN NOT NULL DEFAULT FALSE,
    thanked   BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX gifts_to ON gifts (to_id, at DESC);
CREATE INDEX gifts_from ON gifts (from_id, at DESC);
