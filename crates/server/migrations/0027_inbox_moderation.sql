-- The inbox and the moderation of 0.12.

-- What the staff and the game tell one player (see `snook_legend_proto::inbox`).
-- `kind` is the notification's kind with its fields, as the game reads it;
-- `action` its button. Taken out of the inbox by the player (`deleted_at`),
-- a notification stays for the staff, who see when it was read.
CREATE TABLE notifications (
    id           BIGSERIAL PRIMARY KEY,
    user_id      BIGINT  NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind         JSONB   NOT NULL,
    text         TEXT    NOT NULL DEFAULT '',
    from_id      BIGINT REFERENCES users (id) ON DELETE SET NULL,
    created_at   BIGINT  NOT NULL,
    read_at      BIGINT,
    important    BOOLEAN NOT NULL DEFAULT FALSE,
    pinned_until BIGINT  NOT NULL DEFAULT 0,
    action       JSONB   NOT NULL DEFAULT '{"to":"none"}',
    deleted_at   BIGINT
);
CREATE INDEX notifications_user ON notifications (user_id, id DESC);

-- Words the chat doesn't allow, per language ('*': all of them). The
-- starting list (pt, en, es) is the server's `wordfilter::SEED`.
CREATE TABLE banned_words (
    word     TEXT   NOT NULL,
    lang     TEXT   NOT NULL,
    added_by BIGINT REFERENCES users (id) ON DELETE SET NULL,
    added_at BIGINT NOT NULL,
    PRIMARY KEY (lang, word)
);
INSERT INTO banned_words (word, lang, added_at) VALUES
    ('puta', 'pt', 0), ('puto', 'pt', 0), ('caralho', 'pt', 0), ('porra', 'pt', 0),
    ('foda', 'pt', 0), ('fodase', 'pt', 0), ('foder', 'pt', 0), ('fdp', 'pt', 0),
    ('arrombado', 'pt', 0), ('arrombada', 'pt', 0), ('viado', 'pt', 0),
    ('sapatao', 'pt', 0), ('traveco', 'pt', 0), ('buceta', 'pt', 0),
    ('cuzao', 'pt', 0), ('otario', 'pt', 0), ('vagabunda', 'pt', 0), ('vadia', 'pt', 0),
    ('crioulo', 'pt', 0), ('retardado', 'pt', 0),
    ('mongoloide', 'pt', 0), ('corno', 'pt', 0), ('babaca', 'pt', 0), ('pau no cu', 'pt', 0),
    ('vai se foder', 'pt', 0), ('filho da puta', 'pt', 0),
    ('fuck', 'en', 0), ('fucker', 'en', 0), ('fucking', 'en', 0), ('motherfucker', 'en', 0),
    ('shit', 'en', 0), ('bitch', 'en', 0), ('cunt', 'en', 0),
    ('asshole', 'en', 0), ('bastard', 'en', 0), ('whore', 'en', 0), ('slut', 'en', 0),
    ('nigger', 'en', 0), ('nigga', 'en', 0), ('faggot', 'en', 0),
    ('retard', 'en', 0), ('tranny', 'en', 0), ('kys', 'en', 0), ('kill yourself', 'en', 0),
    ('puta', 'es', 0), ('puto', 'es', 0), ('cabron', 'es', 0), ('cabrona', 'es', 0),
    ('pendejo', 'es', 0), ('pendeja', 'es', 0), ('gilipollas', 'es', 0), ('maricon', 'es', 0),
    ('marica', 'es', 0), ('joder', 'es', 0), ('mierda', 'es', 0),
    ('zorra', 'es', 0), ('chingada', 'es', 0), ('chinga tu madre', 'es', 0), ('hijo de puta', 'es', 0),
    ('culero', 'es', 0), ('verga', 'es', 0), ('subnormal', 'es', 0);

-- The names and countries an account had (from 0.12 on: earlier changes
-- were not kept). `by_id`: the staff member who made it (none: the player).
CREATE TABLE profile_changes (
    id      BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind    TEXT   NOT NULL,
    old     TEXT   NOT NULL,
    new     TEXT   NOT NULL,
    at      BIGINT NOT NULL,
    by_id   BIGINT REFERENCES users (id) ON DELETE SET NULL
);
CREATE INDEX profile_changes_user ON profile_changes (user_id, at DESC);

-- Ratings put right by the staff, and the changes given back when a game
-- is called off: part of the player's rating history.
CREATE TABLE rating_adjustments (
    id           BIGSERIAL PRIMARY KEY,
    user_id      BIGINT  NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    rules        TEXT    NOT NULL,
    delta        INTEGER NOT NULL,
    rating_after INTEGER NOT NULL,
    reason       TEXT    NOT NULL,
    match_id     BIGINT REFERENCES matches (id) ON DELETE SET NULL,
    staff_id     BIGINT REFERENCES users (id) ON DELETE SET NULL,
    at           BIGINT  NOT NULL
);
CREATE INDEX rating_adjustments_user ON rating_adjustments (user_id, rules, at DESC);

-- A game the staff called off: its rating changes were given back.
ALTER TABLE matches ADD COLUMN voided_at BIGINT;

-- How long each shot of a live game was thought over (tenths of a second
-- from the shot clock's start to the shot), one entry per stroke of the
-- replay (null for what isn't a shot), by the game's room: games stored
-- before 0.12 have none.
CREATE TABLE shot_times (
    room_id BIGINT PRIMARY KEY,
    times   JSONB  NOT NULL,
    at      BIGINT NOT NULL
);
