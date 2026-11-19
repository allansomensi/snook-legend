-- The official rules (0.8.0). Every game is played under the Brazilian rules
-- or the official (WPA) rules, and a player has a rating, a peak and rated
-- games under each: the columns `rating`, `peak_rating`, `games` and `wins`
-- are the Brazilian rules', the `official_` ones the official rules'. A
-- game records its rules ('brazilian' or 'official') instead of a version
-- number of the rules: every game stored so far was played under the
-- Brazilian rules.
ALTER TABLE users ADD COLUMN official_rating      INTEGER NOT NULL DEFAULT 1200;
ALTER TABLE users ADD COLUMN official_peak_rating INTEGER NOT NULL DEFAULT 1200;
ALTER TABLE users ADD COLUMN official_games       INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN official_wins        INTEGER NOT NULL DEFAULT 0;
CREATE INDEX users_official_leaderboard
    ON users (official_rating DESC, official_wins DESC) WHERE official_games > 0;

ALTER TABLE matches ADD COLUMN rules TEXT NOT NULL DEFAULT 'brazilian'
    CHECK (rules IN ('brazilian', 'official'));
ALTER TABLE matches DROP COLUMN ruleset;
CREATE INDEX matches_rules_rated ON matches (rules, ended_at) WHERE rated;

ALTER TABLE live_matches ADD COLUMN rules TEXT NOT NULL DEFAULT 'brazilian'
    CHECK (rules IN ('brazilian', 'official'));
ALTER TABLE live_matches DROP COLUMN ruleset;

ALTER TABLE async_games ADD COLUMN rules TEXT NOT NULL DEFAULT 'brazilian'
    CHECK (rules IN ('brazilian', 'official'));
ALTER TABLE async_games DROP COLUMN ruleset;

ALTER TABLE async_challenges ADD COLUMN rules TEXT NOT NULL DEFAULT 'brazilian'
    CHECK (rules IN ('brazilian', 'official'));

-- A themed tournament the staff schedule is played under one of the rules.
ALTER TABLE events ADD COLUMN rules TEXT NOT NULL DEFAULT 'brazilian'
    CHECK (rules IN ('brazilian', 'official'));
