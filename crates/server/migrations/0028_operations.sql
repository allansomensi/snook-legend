-- Operations and economy (0.12.0): settings changed from the portal, sales
-- in the shop, official tournaments kept for the website, maintenance
-- announced ahead, auctions, the daily bonus's streak and the supporters'
-- credits.

-- Settings the staff change while the server runs (the environment gives
-- the first values): `value` is JSON.
CREATE TABLE live_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT   NOT NULL,
    updated_by BIGINT REFERENCES users (id) ON DELETE SET NULL,
    updated_at BIGINT NOT NULL
);

-- Sales and showcases in the shop.
CREATE TABLE promotions (
    id           BIGSERIAL PRIMARY KEY,
    title        TEXT     NOT NULL,
    -- 'discount' or 'featured'.
    kind         TEXT     NOT NULL,
    percent      SMALLINT NOT NULL DEFAULT 0,
    -- Comma-separated shop codes and category codes.
    codes        TEXT     NOT NULL DEFAULT '',
    categories   TEXT     NOT NULL DEFAULT '',
    starts_at    BIGINT   NOT NULL,
    ends_at      BIGINT   NOT NULL,
    created_by   BIGINT   REFERENCES users (id) ON DELETE SET NULL,
    created_at   BIGINT   NOT NULL,
    cancelled_at BIGINT
);
CREATE INDEX promotions_ends ON promotions (ends_at);

-- Official tournaments: the champion's share of the prizes (the runner-up
-- has the rest) and an item for the champion.
ALTER TABLE events ADD COLUMN champion_share SMALLINT NOT NULL DEFAULT 0;
ALTER TABLE events ADD COLUMN item_prize TEXT NOT NULL DEFAULT '';

-- Official tournaments once over (the weekly cup, the staff's): the
-- bracket or the standings, for the website and the trophies. `detail` is
-- JSON (players, rounds, standings) with the players' ids.
CREATE TABLE official_tournaments (
    id           BIGSERIAL PRIMARY KEY,
    -- One record per tournament however often the write is tried.
    key          TEXT     NOT NULL UNIQUE,
    event_id     BIGINT,
    name         TEXT     NOT NULL,
    rules        TEXT     NOT NULL,
    format       TEXT     NOT NULL,
    size         INTEGER  NOT NULL,
    players      INTEGER  NOT NULL,
    rated        BOOLEAN  NOT NULL,
    entry_fee    INTEGER  NOT NULL,
    prize_pool   BIGINT   NOT NULL,
    prizes       TEXT     NOT NULL,
    item_prize   TEXT     NOT NULL DEFAULT '',
    champion_id  BIGINT   REFERENCES users (id) ON DELETE SET NULL,
    runner_up_id BIGINT   REFERENCES users (id) ON DELETE SET NULL,
    detail       TEXT     NOT NULL,
    starts_at    BIGINT   NOT NULL,
    ends_at      BIGINT   NOT NULL
);
CREATE INDEX official_tournaments_ends ON official_tournaments (ends_at DESC);
CREATE INDEX official_tournaments_champion ON official_tournaments (champion_id);

-- Maintenance announced ahead: the server stops at `at` for about
-- `minutes` (`started_at`: it went down for it).
CREATE TABLE maintenance (
    id           BIGSERIAL PRIMARY KEY,
    at           BIGINT  NOT NULL,
    minutes      INTEGER NOT NULL,
    message      TEXT    NOT NULL DEFAULT '',
    created_by   BIGINT  REFERENCES users (id) ON DELETE SET NULL,
    created_at   BIGINT  NOT NULL,
    cancelled_at BIGINT,
    started_at   BIGINT
);

-- Auctions of rare items. The leader's bid is held (taken from their
-- wallet) until someone bids more.
CREATE TABLE auctions (
    id            BIGSERIAL PRIMARY KEY,
    code          TEXT    NOT NULL,
    starts_at     BIGINT  NOT NULL,
    ends_at       BIGINT  NOT NULL,
    min_bid       BIGINT  NOT NULL,
    min_increment BIGINT  NOT NULL,
    leader_id     BIGINT  REFERENCES users (id) ON DELETE SET NULL,
    bid           BIGINT  NOT NULL DEFAULT 0,
    bids          INTEGER NOT NULL DEFAULT 0,
    created_by    BIGINT  REFERENCES users (id) ON DELETE SET NULL,
    created_at    BIGINT  NOT NULL,
    cancelled_at  BIGINT,
    settled_at    BIGINT
);
CREATE INDEX auctions_ends ON auctions (ends_at);

CREATE TABLE auction_bids (
    id         BIGSERIAL PRIMARY KEY,
    auction_id BIGINT NOT NULL REFERENCES auctions (id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    amount     BIGINT NOT NULL,
    at         BIGINT NOT NULL
);
CREATE INDEX auction_bids_auction ON auction_bids (auction_id);

-- The daily bonus's streak: the last day it was claimed and the days in a
-- row up to it.
CREATE TABLE daily_streaks (
    user_id  BIGINT PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    last_day BIGINT  NOT NULL,
    streak   INTEGER NOT NULL
);

-- Supporters who chose to appear in the credits.
CREATE TABLE credits (
    user_id    BIGINT PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    updated_at BIGINT NOT NULL
);
