-- The tour's finals the server dealt (0.7.0): only a final played from one
-- of these racks, with its breaker, earns the reward.
CREATE TABLE ladder_tickets (
    user_id   BIGINT   NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    seed      BIGINT   NOT NULL,
    breaker   SMALLINT NOT NULL,
    issued_at BIGINT   NOT NULL,
    PRIMARY KEY (user_id, seed)
);
