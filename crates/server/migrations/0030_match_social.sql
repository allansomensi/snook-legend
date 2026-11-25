-- Match & social (0.12.0).

-- The settings an account keeps, so the game signed in on another computer
-- plays the same: the game's own JSON (at most 8 KB), the latest saved wins.
CREATE TABLE account_settings (
    user_id    BIGINT PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    data       TEXT NOT NULL,
    updated_at BIGINT NOT NULL
);

-- Direct messages between friends; `read_at` is when the receiver read it.
CREATE TABLE direct_messages (
    id      BIGSERIAL PRIMARY KEY,
    from_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    to_id   BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    text    TEXT NOT NULL,
    at      BIGINT NOT NULL,
    read_at BIGINT
);
-- A conversation, either way round, newest first.
CREATE INDEX direct_messages_pair
    ON direct_messages (LEAST(from_id, to_id), GREATEST(from_id, to_id), id DESC);
-- What waits unread for a player.
CREATE INDEX direct_messages_unread ON direct_messages (to_id) WHERE read_at IS NULL;

-- A parent or guardian who turned an account's chat on may keep it to the
-- quick phrases.
ALTER TABLE users ADD COLUMN chat_phrases_only BOOLEAN NOT NULL DEFAULT FALSE;

-- Pot attempts week by week (weeks from Monday, see
-- `snook_legend_proto::advanced::week_of`): the advanced statistics' progress.
CREATE TABLE shot_weeks (
    user_id  BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    week     INTEGER NOT NULL,
    games    INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0,
    made     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, week)
);
