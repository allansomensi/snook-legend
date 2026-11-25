-- Doubles games in progress are saved and handed to the next server like
-- singles ones (0.12.1): a doubles game's row has its teams' second players
-- here (`player0`, `player1`: the teams' first players). NULL: a singles
-- game.
ALTER TABLE live_matches ADD COLUMN partners BIGINT[];
CREATE INDEX live_matches_partners ON live_matches USING GIN (partners);
