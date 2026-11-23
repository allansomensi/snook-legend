-- The daily challenge's ties go to the quickest: how long the best attempt
-- took (seconds from its start to its strokes arriving). Scores made before
-- this count as having taken the whole time allowed.
ALTER TABLE daily_scores ADD COLUMN best_secs INTEGER NOT NULL DEFAULT 1200;
DROP INDEX daily_scores_board;
CREATE INDEX daily_scores_board ON daily_scores (day, best DESC, best_secs, best_at);
