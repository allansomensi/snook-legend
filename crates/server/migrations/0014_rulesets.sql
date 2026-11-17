-- The rules, physics and table each recorded game was played with (see
-- snook_legend_core::RULESET). Games stored before are ruleset 1 (0.3.0): they
-- keep replaying, and the ones in progress keep playing, under it.
ALTER TABLE matches ADD COLUMN ruleset SMALLINT NOT NULL DEFAULT 1;
ALTER TABLE live_matches ADD COLUMN ruleset SMALLINT NOT NULL DEFAULT 1;
ALTER TABLE async_games ADD COLUMN ruleset SMALLINT NOT NULL DEFAULT 1;
