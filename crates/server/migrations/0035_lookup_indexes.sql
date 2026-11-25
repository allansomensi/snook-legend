-- Lookups that read whole tables without these: a player's conversations
-- (every sign-in asks for them: `dm_threads`, by sender or by addressee),
-- their correspondence challenges both ways, the reports a player sent
-- (the staff's page) and the accounts made from the same address.
CREATE INDEX direct_messages_from ON direct_messages (from_id, id DESC);
CREATE INDEX direct_messages_to ON direct_messages (to_id, id DESC);
CREATE INDEX async_challenges_from ON async_challenges (from_id);
CREATE INDEX reports_reporter ON reports (reporter_id, created_at DESC);
CREATE INDEX users_register_ip ON users (register_ip);
