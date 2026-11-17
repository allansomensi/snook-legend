-- The profile picture a player chose (a code from snook_legend_proto::AVATARS;
-- empty: none).
ALTER TABLE users ADD COLUMN avatar TEXT NOT NULL DEFAULT '';
