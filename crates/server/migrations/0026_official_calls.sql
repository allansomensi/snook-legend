-- The official rules without calls (0.11.2): a ball of the shooter's group
-- made in any pocket keeps the table. Every game stored so far was played
-- with calls (only a ball made in the called pocket kept the table): they
-- keep replaying, and the ones in progress keep playing, with them. New
-- games are stored without (the default from here on).
ALTER TABLE matches ADD COLUMN calls BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE matches ALTER COLUMN calls SET DEFAULT FALSE;
ALTER TABLE live_matches ADD COLUMN calls BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE live_matches ALTER COLUMN calls SET DEFAULT FALSE;
ALTER TABLE async_games ADD COLUMN calls BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE async_games ALTER COLUMN calls SET DEFAULT FALSE;
