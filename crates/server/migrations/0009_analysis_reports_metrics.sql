-- How each player pots, summed over their games (see
-- `snookered_proto::analysis`): one row per (kind, key), e.g. a pocket, a
-- distance band, a kind of shot or a cell of the table.
CREATE TABLE shot_analysis (
    user_id  BIGINT   NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind     SMALLINT NOT NULL,
    key      SMALLINT NOT NULL,
    attempts INTEGER  NOT NULL DEFAULT 0,
    made     INTEGER  NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, kind, key)
);

-- The game a report is about: a finished match, or the game that was
-- being played when the report was sent (`live_id`, the room's id; the
-- finished match is linked once it is stored).
ALTER TABLE reports
    ADD COLUMN match_id BIGINT REFERENCES matches (id) ON DELETE SET NULL,
    ADD COLUMN live_id  BIGINT;
CREATE INDEX matches_pair ON matches (LEAST(player0, player1), GREATEST(player0, player1), ended_at);

-- Players and games online, sampled every minute into hourly buckets
-- (`hour` is Unix seconds / 3600), for the staff panel's metrics.
CREATE TABLE online_hours (
    hour         BIGINT  PRIMARY KEY,
    peak_players INTEGER NOT NULL DEFAULT 0,
    peak_games   INTEGER NOT NULL DEFAULT 0,
    samples      INTEGER NOT NULL DEFAULT 0,
    player_sum   BIGINT  NOT NULL DEFAULT 0
);

-- Who was online in which hour (signed-in accounts; kept for a week).
CREATE TABLE active_hours (
    hour    BIGINT NOT NULL,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    PRIMARY KEY (hour, user_id)
);
