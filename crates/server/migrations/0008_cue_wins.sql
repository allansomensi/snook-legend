-- Online games won with each cue (rated or casual, counted by the server at
-- the end of the game): a cue's level is a purely cosmetic mark of these.
CREATE TABLE cue_wins (
    user_id BIGINT  NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    code    TEXT    NOT NULL,
    wins    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, code)
);
