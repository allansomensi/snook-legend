-- The official rules call the 8 (0.12.0): a shot at it names its pocket,
-- and the 8 made anywhere else loses; the group's balls still count in any
-- pocket. Every game stored so far was played without that call (0.11.2 to
-- 0.11.x: the 8 won in any pocket; until 0.11.1 the `calls` column has
-- every shot called): they keep replaying, and the ones in progress keep
-- playing, as they were. New games are stored with it (the default from
-- here on).
ALTER TABLE matches ADD COLUMN eight_call BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE matches ALTER COLUMN eight_call SET DEFAULT TRUE;
ALTER TABLE live_matches ADD COLUMN eight_call BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE live_matches ALTER COLUMN eight_call SET DEFAULT TRUE;
ALTER TABLE async_games ADD COLUMN eight_call BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE async_games ALTER COLUMN eight_call SET DEFAULT TRUE;
