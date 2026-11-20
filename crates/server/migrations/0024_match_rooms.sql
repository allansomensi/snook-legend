-- The live game a match was (its room, also the id of its saved copy): a
-- result is stored once, even when the write is tried again after an answer
-- lost on the way. Empty for older matches and correspondence games.
ALTER TABLE matches ADD COLUMN room_id BIGINT;
CREATE UNIQUE INDEX matches_room ON matches (room_id) WHERE room_id IS NOT NULL;
