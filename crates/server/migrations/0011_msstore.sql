-- Coin packs bought in the Microsoft Store (see src/msstore.rs).
--
-- Every purchase found in a player's Store collection gets a row here
-- BEFORE the server asks Microsoft to consume it. The row's tracking id goes
-- with that request; Microsoft answers the same tracking id with the same
-- result however often it is sent, so a request whose answer was lost is
-- simply sent again until there is an answer. Coins are only credited once
-- Microsoft confirms the consumption, in the same transaction that marks the
-- row done (and the ledger reference keeps the credit single).

CREATE TABLE msstore_consumes (
    -- Our GUID for the consume request (sent to Microsoft).
    tracking_id    TEXT   PRIMARY KEY,
    user_id        BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- The add-on's Store ID and the coin pack it stands for.
    product_id     TEXT   NOT NULL,
    pack           TEXT   NOT NULL,
    coins          BIGINT NOT NULL,
    -- The purchase, as the Store's collection named it.
    transaction_id TEXT   NOT NULL,
    -- The player's Microsoft Store ID key, to send the request again; it is
    -- cleared once the row is done or failed (kept while stuck, for the
    -- staff to send it again).
    store_key      TEXT,
    -- pending: waiting for Microsoft's answer; done: consumed and credited;
    -- failed: Microsoft refused (nothing was consumed); stuck: no answer for
    -- days (for the staff to look at).
    state          TEXT   NOT NULL DEFAULT 'pending',
    -- Requests sent so far (counted before each one goes out).
    attempts       INT    NOT NULL DEFAULT 0,
    -- A server is sending it until then (one at a time, even during a
    -- deploy when two servers share the database).
    leased_until   BIGINT NOT NULL DEFAULT 0,
    last_error     TEXT,
    -- Microsoft's order ids for the purchase (refunds name them).
    order_ids      TEXT,
    created_at     BIGINT NOT NULL,
    next_try_at    BIGINT NOT NULL,
    settled_at     BIGINT
);

-- A purchase is worked on (or was credited) by one row at a time.
CREATE UNIQUE INDEX msstore_consumes_purchase ON msstore_consumes (transaction_id)
    WHERE state IN ('pending', 'done');
CREATE INDEX msstore_consumes_due ON msstore_consumes (next_try_at) WHERE state = 'pending';
CREATE INDEX msstore_consumes_user ON msstore_consumes (user_id, created_at DESC);
