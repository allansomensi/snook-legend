-- Indexes for the staff's pages and the metrics: the audit log by player
-- (a player's page, renames and deletions move their entries), the
-- economy's ledger and the latest payments by time, the new accounts by day.
CREATE INDEX audit_log_target ON audit_log (lower(target), id DESC);
CREATE INDEX coin_ledger_at ON coin_ledger (at);
CREATE INDEX payments_at ON payments (at DESC);
CREATE INDEX users_created_at ON users (created_at);
