-- The computer ladder's reward, once per account: the final game the
-- server checked (its rack seed and length, for the staff) and when.
CREATE TABLE ladder_claims (
    user_id    BIGINT  PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    rack_seed  BIGINT  NOT NULL,
    strokes    INTEGER NOT NULL,
    claimed_at BIGINT  NOT NULL
);
