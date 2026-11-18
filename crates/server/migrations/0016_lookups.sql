-- The tournaments a player won (the trophies on their profile) and the
-- days a player has a daily challenge score on (the days they topped),
-- looked up by player.
CREATE INDEX tournaments_winner ON tournaments (winner_id, finished_at DESC);
CREATE INDEX daily_scores_user ON daily_scores (user_id, day);
