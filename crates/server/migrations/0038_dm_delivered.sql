-- 0.12.4: a private message says when it reached the other player's game
-- (`delivered_at`: as it was sent, if they were online, else when they
-- next signed in), not only when they read it.
ALTER TABLE direct_messages ADD COLUMN delivered_at BIGINT;

-- Those sent until now were delivered, as far as anyone can tell.
UPDATE direct_messages SET delivered_at = at;

-- What waits for a player's game (every sign-in asks).
CREATE INDEX direct_messages_undelivered ON direct_messages (to_id)
    WHERE delivered_at IS NULL;
