-- The age group a player gave (0.7.3): NULL when not asked yet, TRUE under
-- 18, FALSE 18 or older; and when a parent or guardian turned the chat on
-- for an account under 18 (Lei 15.211/2025, the ECA Digital, art. 20: games
-- likely played by children and adolescents limit what players can write
-- to each other by default).
ALTER TABLE users ADD COLUMN minor BOOLEAN;
ALTER TABLE users ADD COLUMN chat_allowed_at BIGINT;
