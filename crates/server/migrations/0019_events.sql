-- Events the staff schedule (0.7.0): experience or coins that count more
-- for a while, and themed tournaments the server organises.
CREATE TABLE events (
    id           BIGSERIAL PRIMARY KEY,
    kind         TEXT     NOT NULL,
    title        TEXT     NOT NULL,
    multiplier   SMALLINT NOT NULL DEFAULT 1,
    starts_at    BIGINT   NOT NULL,
    ends_at      BIGINT   NOT NULL,
    -- A tournament's settings (empty or zero for a boost).
    format       TEXT     NOT NULL DEFAULT '',
    size         INTEGER  NOT NULL DEFAULT 0,
    rated        BOOLEAN  NOT NULL DEFAULT TRUE,
    entry_fee    INTEGER  NOT NULL DEFAULT 0,
    prize        INTEGER  NOT NULL DEFAULT 0,
    created_by   BIGINT   REFERENCES users (id) ON DELETE SET NULL,
    created_at   BIGINT   NOT NULL,
    cancelled_at BIGINT
);
CREATE INDEX events_ends ON events (ends_at);
