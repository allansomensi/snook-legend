-- The medals of the tournaments against the computer (0.7.2): a gold, a
-- silver or a bronze for each tournament a player played to its end in the
-- first three places. The game sends them once the account is online; one
-- per tournament (its draw's number).
CREATE TABLE cup_medals (
    user_id  BIGINT   NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    cup      BIGINT   NOT NULL,
    format   SMALLINT NOT NULL,
    place    SMALLINT NOT NULL,
    won_at   BIGINT   NOT NULL,
    PRIMARY KEY (user_id, cup)
);
