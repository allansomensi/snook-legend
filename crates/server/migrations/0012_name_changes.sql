-- How many times each player has changed their username (a few at most:
-- see MAX_NAME_CHANGES).
ALTER TABLE users ADD COLUMN name_changes SMALLINT NOT NULL DEFAULT 0;
