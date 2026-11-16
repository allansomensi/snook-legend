-- A short line each player writes about themself ("looking for a practice
-- partner"), shown on the profile and in the lounge. Empty: none.
ALTER TABLE users ADD COLUMN status_message TEXT NOT NULL DEFAULT '';
