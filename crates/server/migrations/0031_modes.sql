-- New modes (0.12.0): doubles (two teams of two) and against the clock.

-- Each player's doubles record under each of the rules, apart from the
-- singles one in `users` (a player with no row has never played a rated
-- doubles game: 1200, nothing played).
CREATE TABLE doubles_ratings (
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    rules TEXT NOT NULL CHECK (rules IN ('brazilian', 'official')),
    rating INTEGER NOT NULL DEFAULT 1200,
    peak_rating INTEGER NOT NULL DEFAULT 1200,
    games INTEGER NOT NULL DEFAULT 0,
    wins INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, rules)
);
CREATE INDEX doubles_ratings_board ON doubles_ratings (rules, rating DESC, wins DESC)
    WHERE games > 0;

-- Doubles games played to the end. The teams: a1 and a2 played seat 0,
-- b1 and b2 seat 1; `winner` is the seat. `room` is the lounge's id of the
-- game (recorded once, however often the write is tried).
CREATE TABLE doubles_matches (
    id BIGSERIAL PRIMARY KEY,
    room BIGINT UNIQUE,
    rules TEXT NOT NULL CHECK (rules IN ('brazilian', 'official')),
    rated BOOLEAN NOT NULL,
    a1 BIGINT NOT NULL REFERENCES users (id),
    a2 BIGINT NOT NULL REFERENCES users (id),
    b1 BIGINT NOT NULL REFERENCES users (id),
    b2 BIGINT NOT NULL REFERENCES users (id),
    winner SMALLINT NOT NULL CHECK (winner IN (0, 1)),
    reason TEXT NOT NULL,
    -- The rating changes of a1, a2, b1, b2 (zeros for a casual game).
    deltas INTEGER[] NOT NULL,
    shots INTEGER NOT NULL,
    ended_at BIGINT NOT NULL,
    replay JSONB
);
CREATE INDEX doubles_matches_a1 ON doubles_matches (a1, ended_at);
CREATE INDEX doubles_matches_a2 ON doubles_matches (a2, ended_at);
CREATE INDEX doubles_matches_b1 ON doubles_matches (b1, ended_at);
CREATE INDEX doubles_matches_b2 ON doubles_matches (b2, ended_at);

-- Attempts against the clock: each is dealt its own rack (`seed`) when it
-- starts (`started_ms`, the server's clock in milliseconds); `ms` is its
-- time once the server played its strokes again (NULL: not sent, refused,
-- or too late). `day` counts the attempts of a day.
CREATE TABLE time_attack_attempts (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    day BIGINT NOT NULL,
    seed BIGINT NOT NULL,
    started_ms BIGINT NOT NULL,
    finished_ms BIGINT,
    ms INTEGER,
    shots INTEGER,
    fouls INTEGER
);
CREATE INDEX time_attack_attempts_user ON time_attack_attempts (user_id, day);

-- Each player's best time of each week (Monday to Sunday, UTC; the week
-- number as in `snook_legend_proto::time_attack::week_of`).
CREATE TABLE time_attack_bests (
    week BIGINT NOT NULL,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    ms INTEGER NOT NULL,
    at BIGINT NOT NULL,
    PRIMARY KEY (week, user_id)
);
CREATE INDEX time_attack_bests_board ON time_attack_bests (week, ms, at);

-- The week's prizes, paid once each (`place` 1 to 3).
CREATE TABLE time_attack_prizes (
    week BIGINT NOT NULL,
    place SMALLINT NOT NULL,
    user_id BIGINT NOT NULL REFERENCES users (id),
    coins INTEGER NOT NULL,
    ms INTEGER NOT NULL,
    paid_at BIGINT NOT NULL,
    PRIMARY KEY (week, place)
);
