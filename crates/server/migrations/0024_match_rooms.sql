-- The live game a match was (its room, also the id of its saved copy): a
-- result is stored once, even when the write is tried again after an answer
-- lost on the way. Empty for older matches and correspondence games.
ALTER TABLE matches ADD COLUMN room_id BIGINT;
CREATE UNIQUE INDEX matches_room ON matches (room_id) WHERE room_id IS NOT NULL;

-- The same for a tournament: its id in the lounge.
ALTER TABLE tournaments ADD COLUMN lounge_id BIGINT;
CREATE UNIQUE INDEX tournaments_lounge ON tournaments (lounge_id) WHERE lounge_id IS NOT NULL;
