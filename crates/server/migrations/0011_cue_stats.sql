-- Statistics of each cue: online games played with it and object balls
-- made with it, beside its wins. The wins so far were all counted games.
ALTER TABLE cue_wins
    ADD COLUMN games  INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN potted INTEGER NOT NULL DEFAULT 0;
UPDATE cue_wins SET games = wins;
