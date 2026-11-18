-- What the staff keep about a player (0.7.0): notes among themselves, and
-- warnings the player is shown (at once if online, else at the next
-- sign-in: `delivered_at` stays empty until then).
CREATE TABLE staff_notes (
    id           BIGSERIAL PRIMARY KEY,
    user_id      BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    author_id    BIGINT REFERENCES users (id) ON DELETE SET NULL,
    kind         TEXT   NOT NULL,
    text         TEXT   NOT NULL,
    created_at   BIGINT NOT NULL,
    delivered_at BIGINT
);
CREATE INDEX staff_notes_user ON staff_notes (user_id, id DESC);
CREATE INDEX staff_notes_waiting ON staff_notes (user_id)
    WHERE kind = 'warning' AND delivered_at IS NULL;

-- The staff's list of players sorts by these.
CREATE INDEX users_last_seen ON users (last_seen DESC);
CREATE INDEX users_rating ON users (rating DESC);
