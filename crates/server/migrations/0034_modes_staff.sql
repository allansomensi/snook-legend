-- The staff and the new modes (0.12.1).

-- A doubles game annulled by the staff (its rating changes given back).
ALTER TABLE doubles_matches ADD COLUMN voided_at BIGINT;

-- Against the clock: the week an attempt was counted in and when (Unix
-- seconds), and an attempt the staff took off the ranking (cheating), why.
ALTER TABLE time_attack_attempts ADD COLUMN week BIGINT;
ALTER TABLE time_attack_attempts ADD COLUMN at BIGINT;
ALTER TABLE time_attack_attempts ADD COLUMN removed_at BIGINT;
ALTER TABLE time_attack_attempts ADD COLUMN removed_reason TEXT;
UPDATE time_attack_attempts
   SET week = (finished_ms / 1000 / 86400 + 3) / 7, at = finished_ms / 1000
 WHERE ms IS NOT NULL AND finished_ms IS NOT NULL;
CREATE INDEX time_attack_attempts_week ON time_attack_attempts (week, user_id)
    WHERE week IS NOT NULL;
