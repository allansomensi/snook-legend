-- The rated wins in a row are 5, 10, 15 and 30 (bronze, silver, gold, ruby):
-- 3 and 7 are withdrawn, with what the inbox said of them. Whoever has 15
-- or 30 gets them the next time they sign in.
DELETE FROM achievements WHERE code IN ('streak_3', 'streak_7');
DELETE FROM notifications
 WHERE kind ->> 'kind' = 'achievement' AND kind ->> 'code' IN ('streak_3', 'streak_7');
