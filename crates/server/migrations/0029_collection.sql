-- The collection: how each owned item was got (JSON of the game's
-- `collection::Origin`: bought, a gift, a tournament, an event, VIP's item
-- of the month…). NULL for the items owned before 0.12: the game works
-- their origin out from `source` and keeps `acquired_at`.
ALTER TABLE owned_items ADD COLUMN origin TEXT;
