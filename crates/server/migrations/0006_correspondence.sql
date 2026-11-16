-- Correspondence games: each player takes their turn whenever they can.
-- The game is its seed and strokes (like `matches`); the table is rebuilt by
-- replaying them. A turn is a whole visit with a deadline.
CREATE TABLE async_games (
    id          BIGSERIAL PRIMARY KEY,
    player0     BIGINT   NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    player1     BIGINT   NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    rated       BOOLEAN  NOT NULL,
    seed        BIGINT   NOT NULL,
    breaker     SMALLINT NOT NULL,
    strokes     JSONB    NOT NULL DEFAULT '[]',
    -- How many strokes there are (the version a change is made against).
    stroke_count INTEGER NOT NULL DEFAULT 0,
    -- The seat to play and when their turn runs out (Unix seconds).
    turn        SMALLINT NOT NULL,
    deadline    BIGINT   NOT NULL,
    -- Strokes each player has watched.
    seen0       INTEGER  NOT NULL DEFAULT 0,
    seen1       INTEGER  NOT NULL DEFAULT 0,
    -- For the list without replaying: each seat's group (0 open, 1 solids,
    -- 2 stripes) and the balls they have left.
    group0      SMALLINT NOT NULL DEFAULT 0,
    group1      SMALLINT NOT NULL DEFAULT 0,
    left0       SMALLINT NOT NULL DEFAULT 7,
    left1       SMALLINT NOT NULL DEFAULT 7,
    started_at  BIGINT   NOT NULL,
    updated_at  BIGINT   NOT NULL,
    -- Set when the game is over.
    finished_at BIGINT,
    winner      SMALLINT,
    reason      TEXT,
    timeout     BOOLEAN  NOT NULL DEFAULT FALSE,
    -- Rating change and rating after the game, per seat.
    delta0      INTEGER  NOT NULL DEFAULT 0,
    delta1      INTEGER  NOT NULL DEFAULT 0,
    rating0     INTEGER  NOT NULL DEFAULT 0,
    rating1     INTEGER  NOT NULL DEFAULT 0,
    match_id    BIGINT REFERENCES matches (id) ON DELETE SET NULL
);
-- "My games": the active ones, and the latest finished.
CREATE INDEX async_games_active0 ON async_games (player0) WHERE finished_at IS NULL;
CREATE INDEX async_games_active1 ON async_games (player1) WHERE finished_at IS NULL;
CREATE INDEX async_games_finished0 ON async_games (player0, finished_at DESC) WHERE finished_at IS NOT NULL;
CREATE INDEX async_games_finished1 ON async_games (player1, finished_at DESC) WHERE finished_at IS NOT NULL;
-- Turns that ran out.
CREATE INDEX async_games_deadline ON async_games (deadline) WHERE finished_at IS NULL;

-- Challenges waiting for an answer (one per pair and direction).
CREATE TABLE async_challenges (
    id         BIGSERIAL PRIMARY KEY,
    from_id    BIGINT  NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    to_id      BIGINT  NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    rated      BOOLEAN NOT NULL,
    created_at BIGINT  NOT NULL,
    UNIQUE (from_id, to_id)
);
CREATE INDEX async_challenges_to ON async_challenges (to_id);
CREATE INDEX async_challenges_created ON async_challenges (created_at);

-- Matches that were correspondence games (for the history).
ALTER TABLE matches ADD COLUMN correspondence BOOLEAN NOT NULL DEFAULT FALSE;
