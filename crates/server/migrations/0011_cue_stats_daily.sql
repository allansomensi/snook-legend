-- Statistics of each cue: online games played with it and object balls
-- made with it, beside its wins. The wins so far were all counted games.
ALTER TABLE cue_wins
    ADD COLUMN games  INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN potted INTEGER NOT NULL DEFAULT 0;
UPDATE cue_wins SET games = wins;

-- The daily challenge: each day's position comes from a seed kept here
-- (drawn the first time the day is asked for, never shown before an
-- attempt starts).
CREATE TABLE daily_challenges (
    day  BIGINT PRIMARY KEY,
    seed BIGINT NOT NULL
);

-- Each player's attempts at a day's challenge and their best (the most
-- balls; on a tie, who made it first leads).
CREATE TABLE daily_scores (
    day        BIGINT  NOT NULL,
    user_id    BIGINT  NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    attempts   INTEGER NOT NULL DEFAULT 0,
    -- The attempt under way: when it started (0: none).
    started_at BIGINT  NOT NULL DEFAULT 0,
    best       INTEGER,
    best_at    BIGINT,
    cleared    BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (day, user_id)
);
CREATE INDEX daily_scores_board ON daily_scores (day, best DESC, best_at);

-- The account level's experience (see snookered_proto::xp). Accounts start
-- from the games they played so far: 60 a win, 30 any other game.
ALTER TABLE users ADD COLUMN xp BIGINT NOT NULL DEFAULT 0;
UPDATE users SET xp = 60 * wins + 30 * GREATEST(games + casual_games - wins, 0);

-- Drills (see snookered_proto::drills): each player's best score per drill
-- (on a tie, who made it first leads), and their challenge rating (from
-- the stars of every drill), kept for its table.
CREATE TABLE drill_scores (
    user_id BIGINT  NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    code    TEXT    NOT NULL,
    best    INTEGER NOT NULL,
    best_at BIGINT  NOT NULL,
    PRIMARY KEY (user_id, code)
);
CREATE INDEX drill_scores_board ON drill_scores (code, best DESC, best_at);

CREATE TABLE challenge_ratings (
    user_id    BIGINT  PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    rating     INTEGER NOT NULL,
    stars      INTEGER NOT NULL,
    updated_at BIGINT  NOT NULL
);
CREATE INDEX challenge_ratings_board ON challenge_ratings (rating DESC, updated_at);
