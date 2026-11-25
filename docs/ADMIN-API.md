# Staff API

The staff portal's calls to the server (`/admin/api`). Every call needs the
staff session's bearer token (`Authorization: Bearer …`); each area below
says which role it needs. Errors answer `{"error": "…"}` with a 4xx status.

## Collection and content

### `GET /admin/api/users/{name}/collection`

Moderators and administrators. Everything the player has, with how and when
they got it: owned items, the items of their achievements, and the
reactions those achievements bring (code `reaction_<code>`, also for
reactions that are not shop items). Free items and those VIP members use
while VIP are not listed. Read only (nothing is written to the audit log).

```json
{
  "username": "ana",
  "items": [
    {"code": "cue_christmas", "name": "Candy cane", "category": "cue",
     "rarity": "epic", "bound": true,
     "origin": {"how": "staff", "reason": "Christmas 2026 winner"},
     "acquired_at": 1798000000},
    {"code": "cloth_blue", "name": "Electric blue", "category": "cloth",
     "rarity": "common", "bound": false,
     "origin": {"how": "bought", "price": 1000}, "acquired_at": 1797000000},
    {"code": "reaction_fire", "name": null, "category": "reaction",
     "rarity": null, "bound": false,
     "origin": {"how": "achievement", "code": "streak_10"}, "acquired_at": 1796000000}
  ]
}
```

`origin.how` is one of `bought` (`price`), `gift` (`from`), `staff`
(`reason`), `tournament` (`name`, `place`), `achievement` (`code`),
`auction` (`price`), `vip_month` (`month`, `YYYY-MM`), `event` (`name`),
`season` (`name`), `welcome_pack`, `unknown`. `acquired_at`: Unix seconds,
0 when not known. Items owned before 0.12 have their origin worked out from
how they were added (bought, a present, the welcome pack, the staff).

### Changes to existing calls

- `POST /admin/api/users/{name}/item` (administrators) also gives items
  only won (`"how": "earned"` in the catalogue). The `reason` is kept as the
  item's origin and shown to the player under it in the collection.
- `GET /admin/api/economy`: each entry of `catalog` has `"rarity"`
  (`common`, `rare`, `epic`, `legendary`) and `"bound"` (true: can't be
  given nor auctioned).

# Staff API and public API

The staff portal (the website's `/admin` pages) is built from this document.
Every staff endpoint is under `/admin/api`, takes the bearer token from
`POST /admin/api/login` (`Authorization: Bearer …`) and answers JSON; errors
are `{"error": "…"}` with a 4xx/5xx status. **Moderators** may read what is
marked *staff*; only **administrators** change things (and see money). Every
change is written to the audit log (the action named in each section). Times
are Unix seconds (UTC).

## Operations & economy

### Live settings

Settings the server applies at once, without a restart. The environment
(`SNOOK_LEGEND_SHOT_CLOCK`, `SNOOK_LEGEND_MOTD`, `SNOOK_LEGEND_CUP`…) gives
the defaults; a value set here wins until it is reset.

`GET /admin/api/settings` — staff.

```json
{"settings": [
  {"key": "shot_clock_secs", "group": "games", "type": "int", "value": 30, "default": 30,
   "source": "default", "description": "Seconds for each shot.",
   "range": {"min": 10, "max": 120}, "updated_by": null, "updated_at": null},
  {"key": "cup_rules", "group": "weekly_cup", "type": "choice", "value": "official",
   "default": "brazilian", "source": "portal", "description": "…",
   "range": {"options": ["brazilian", "official"]}, "updated_by": "boss", "updated_at": 1795291200}
 ], "now": 1795291200}
```

- `type`: `int` (`range.min`/`range.max`), `bool`, `text` (`range.max_length`),
  `choice` (`range.options`; the value is one of those strings), `time`
  (`"HH:MM"`), `item` (a shop code, or `""`).
- `source`: `default` (built in), `env` (from an environment variable) or
  `portal`.
- `group`: `games`, `lounge`, `limits`, `tournaments`, `weekly_cup`, `economy`.

| key | type | what |
|---|---|---|
| `shot_clock_secs` | int 10–120 | seconds per shot (the next shot clock) |
| `reconnect_grace_secs` | int 15–600 | time to come back after losing the connection |
| `motd` | text ≤300 | the lounge's message (`POST /admin/api/motd` sets this too) |
| `chat_history` | int 10–500 | lounge chat lines kept |
| `seek_lifetime_secs` | int 60–86400 | a "looking for a game" note's life |
| `max_connections_per_ip` | int 1–1000 | connections at once from one address |
| `messages_per_sec`, `message_burst` | int | one connection's message rate and burst (new connections) |
| `max_open_tournaments` | int 1–200 | tournaments open at once |
| `tournament_no_show_secs` | int 30–1800 | a missing tournament player loses after this |
| `tournament_house_cut_percent` | int 0–50 | the house's share of entry fees (a coin sink); tournaments opened from then on |
| `cup_enabled` | bool | the weekly cup is organised |
| `cup_name` | text ≤32 | its name |
| `cup_weekday` | int 0–6 | 0 Sunday … 6 Saturday |
| `cup_time` | time | local start time |
| `cup_utc_offset_minutes` | int −720–840 | the local time's offset (Brasília −180) |
| `cup_rules` | choice | `brazilian` / `official` |
| `cup_format` | choice | `arena` / `knockout` |
| `cup_minutes` | int 10–180 | an arena's length |
| `cup_size` | choice | `4` `8` `16` `32` (a knockout's seats) |
| `cup_entry_fee` | int 0–1000 | coins to enter |
| `cup_house_prize` | int 0–100000 | coins the house adds |
| `name_change_price` | int 0–100000 | coins for a username change once the free ones are used (0: none) |
| `daily_bonus_base`, `daily_bonus_step`, `daily_streak_days` | int | the daily bonus: base on day 1, +step each day in a row, up to that day |
| `vip_month_item` | item | VIP's item of this month instead of the catalogue's `vip_month_YYYY_MM` |

`POST /admin/api/settings/{key}` — administrators. Body `{"value": 45}`.
Answer `{"ok": true, "key": "shot_clock_secs", "value": 45}`; 400 with the
reason when out of range. Audit: `setting` (`old → new`).

`POST /admin/api/settings/{key}/reset` — administrators. Back to the
default; answer as above. Audit: `setting-reset`.

### Promotions

`GET /admin/api/promotions` — staff.

```json
{"promotions": [{"id": 3, "title": "Weekend sale", "kind": "discount", "percent": 30,
   "codes": ["cue_dragon"], "categories": ["cloth"], "starts_at": 1795291200,
   "ends_at": 1795464000, "created_by": "boss", "created_at": 1795200000,
   "cancelled_at": null, "status": "live"}],
 "categories": ["cloth", "cue", "chalk", "balls", "table", "hall", "frame", "name", "effect", "reaction"],
 "limits": {"max_percent": 90, "max_days": 31, "max_title": 60}, "now": 1795291200}
```

`status`: `scheduled`, `live`, `ended`, `cancelled`.

`POST /admin/api/promotions` — administrators. Body:
`{"title": "Weekend sale", "kind": "discount" | "featured", "percent": 30,
"codes": ["cue_dragon"], "categories": ["cloth"], "starts_at": …, "ends_at": …}`.
A discount is 1–90 % off items sold for coins (by code and/or category); a
`featured` one only shows its items first, highlighted. At most a month
long. Answer `{"ok": true, "id": 3}`. Audit: `promotion`.

`POST /admin/api/promotions/{id}` — administrators, the same body: new terms
(not for one called off). Audit: `promotion-edit`.

`POST /admin/api/promotions/{id}/cancel` — administrators. Audit:
`promotion-cancel`.

Players see the promotions running and starting within 3 days at once
(`ServerMsg::Promotions`); `BuyItem` and `GiftItem` charge the best running
discount (the coin ledger records the price paid).

### Official tournaments

Official tournaments are the weekly cup and the staff's (events of kind
`tournament`). They are scheduled with `POST /admin/api/events` (body below)
or, the same, `POST /admin/api/tournaments/official` (the kind is set for
you), and called off with `POST /admin/api/events/{id}/cancel`.

Body (administrators): `{"title": "Winter Cup", "starts_at": …, "ends_at": …,
"format": "knockout" | "arena", "size": 4 | 8 | 16 | 32, "rated": true,
"rules": "brazilian" | "official", "entry_fee": 100, "prize": 5000,
"champion_share": 70, "item_prize": "cue_winter_2026"}`.

- `prize`: coins the house adds (≤100,000), paid only to players who won at
  the table.
- `champion_share`: 50–100, the champion's percent of the prizes (the
  runner-up has the rest); 0 or absent: the usual (70:20 of the fees after
  the house's cut, 70:30 of the house's prize).
- `item_prize`: an item that is only won (`Earned`), for the champion.
- The entry fees less the house's cut (`tournament_house_cut_percent`) plus
  the house's prize make the **prize pool**, shown live in the game.

`GET /admin/api/tournaments/official` — staff.

```json
{"scheduled": [{"id": 12, "kind": "tournament", "title": "Winter Cup", "format": "knockout",
   "size": 32, "rules": "official", "entry_fee": 100, "prize": 5000, "champion_share": 70,
   "item_prize": "cue_winter_2026", "starts_at": …, "ends_at": …, "…": "…"}],
 "live": [{"id": 77, "name": "Saturday Cup", "state": "registering", "format": "arena",
   "rules": "brazilian", "players": 14, "size": 64, "entry_fee": 100, "prize_pool": 1260,
   "prizes": [882, 252], "item_prize": "", "starts_at": …, "ends_at": …}],
 "finished": [{"id": 42, "event_id": 12, "name": "Winter Cup", "rules": "official",
   "format": "knockout", "size": 32, "players": 30, "rated": true, "entry_fee": 100,
   "prize_pool": 7700, "prizes": [5390, 2310], "item_prize": "cue_winter_2026",
   "champion_id": 5, "runner_up_id": 9, "starts_at": …, "ends_at": …}],
 "weekly_cup": {"name": "Saturday Cup", "next_start": …, "format": "arena", "entry_fee": 0,
   "house_prize": 0, "rules": "brazilian"},
 "house_cut_percent": 10, "item_prizes": ["cue_winter_2026"], "now": …}
```

Players signed up are reminded in their inbox 10 minutes before the start
(`TournamentStarting`, button to the tournament); at the end each player
gets `TournamentResult` (place and coins; `id` is the finished tournament's
id above) and the champion the item prize (`ItemAwarded`, kept in the
collection). Finished official tournaments are kept for the website
(`/api/tournaments`).

### Maintenance

`GET /admin/api/maintenance` — staff.

```json
{"maintenance": [{"id": 4, "at": 1795300000, "minutes": 20, "message": "New tables",
   "created_by": "boss", "created_at": …, "cancelled_at": null, "started_at": null,
   "status": "scheduled"}],
 "limits": {"max_minutes": 1440, "max_message": 200}, "notices_minutes": [60, 30, 15, 5, 1],
 "draining": false, "now": …}
```

`status`: `scheduled`, `due`, `started`, `cancelled`.

`POST /admin/api/maintenance` — administrators. Body `{"at": …, "minutes": 20,
"message": "…"}` (at least a minute ahead, within 90 days; one planned at a
time). Players get a banner with a countdown at once
(`ServerMsg::Maintenance`) and lounge notices 60, 30, 15, 5 and 1 minutes
before. At the time the server stops starting games, lets those in progress
finish (handed over after `SNOOK_LEGEND_DRAIN_SECS`), and the process ends
cleanly for the host to start it again. Audit: `maintenance`.

`POST /admin/api/maintenance/{id}/cancel` — administrators (before it
starts). Audit: `maintenance-cancel`.

### Auctions

`GET /admin/api/auctions` — staff.

```json
{"auctions": [{"id": 2, "code": "cue_dragon", "name": "Dragon", "starts_at": …, "ends_at": …,
   "min_bid": 2000, "min_increment": 100, "leader_id": 5, "leader": "ana", "bid": 2600,
   "bids": 7, "created_by": "boss", "created_at": …, "cancelled_at": null,
   "settled_at": null, "state": "open",
   "latest_bids": [{"username": "ana", "amount": 2600, "at": …}]}],
 "items": [{"code": "cue_dragon", "name": "Dragon", "price": 12000, "rarity": "legendary"}],
 "limits": {"max_days": 14, "max_bid": 10000000, "extension_secs": 120, "snipe_window_secs": 120},
 "now": …}
```

`state`: `coming`, `open`, `ended`, `cancelled`.

`POST /admin/api/auctions` — administrators. Body `{"code": "cue_dragon",
"starts_at": …, "ends_at": …, "min_bid": 2000, "min_increment": 100}`. Items
sold for coins or only won (not free, achievements', VIP's or the welcome
pack's). Up to two weeks. Audit: `auction`.

`POST /admin/api/auctions/{id}/cancel` — administrators: the leader's coins
go back. Answer `{"ok": true, "refunded": 2600}`. Audit: `auction-cancel`.

A bid holds the coins (ledger `auction bid #id`); the player outbid gets them
back at once (`auction refund #id`) and an inbox `AuctionOutbid`; a bid in
the last 2 minutes moves the end 2 minutes later; at the end the leader gets
the item (`AuctionWon`) and the coins leave the economy.

### Analytics

`GET /admin/api/analytics/coins?days=30` — staff (1–400 days).

```json
{"days": [{"day": 20778, "date": "2026-11-21", "created": 5400, "spent": 4100, "net": 1300,
   "by_source": {"games": 3000, "daily_bonus": 900, "missions": 800, "tournament_prizes": 700},
   "by_sink": {"shop:cue": 2400, "tournament_fees": 900, "auctions": 800},
   "supply": 1250000}],
 "totals": {"created": {"games": 90000}, "spent": {"shop:cloth": 40000}},
 "supply": 1250000, "wallets": 3100, "net_7d": 9100, "net_7d_percent": 0.73, "health": "healthy"}
```

Sources: `games`, `daily_bonus`, `daily_challenge`, `missions`,
`tournament_prizes`, `rewards` (leagues, levels), `staff`, `purchases`
(money), `refunds`, `other`. Sinks: `shop:{category}`, `gifts:{category}`,
`tournament_fees`, `auctions`, `name_changes`, `staff`, `payment_refunds`,
`other`. `supply` is the coins in wallets at the end of the day. `health`:
the last 7 days' net creation as a share of the supply: `deflating` (< −2 %),
`healthy` (≤ 3 %), `watch` (≤ 7 %), `inflating`, `unknown`.

`GET /admin/api/analytics/sellers` — staff.

```json
{"items": [{"code": "cue_dragon", "name": "Dragon", "category": "cue", "rarity": "legendary",
   "price": 12000, "last_sold_at": …,
   "days_7": {"sales": 2, "coins": 24000, "buyers": 2},
   "days_30": {"sales": 5, "coins": 60000, "buyers": 5},
   "all_time": {"sales": 9, "coins": 108000, "buyers": 9}}],
 "categories": [{"category": "cue", "sales": 120, "coins": 300000, "coins_30d": 40000}],
 "never_sold": [{"code": "chalk_mint", "name": "Mint", "category": "chalk", "price": 400}]}
```

Sales are items bought and given with coins (reactions included), best
first by coins all time.

`GET /admin/api/analytics/countries` — staff.
`{"countries": [{"country": "BR", "players": 5200, "new_7d": 140, "online": 37}], "online": 52}`
(`""`: not given).

`GET /admin/api/analytics/hours?weeks=4&tz=-180` — staff. Players online by
weekday (0 Sunday) and hour, in the time zone `tz` minutes from UTC, over
the last 1–52 weeks (sampled every minute, by the hour):
`{"weeks": 4, "utc_offset_minutes": -180, "cells": [{"weekday": 6, "hour": 21, "avg": 41.5, "max": 63, "samples": 4}]}`
(168 cells).

`GET /admin/api/analytics/vip` — administrators.

```json
{"active": 120, "renewed_this_month": 80, "new_this_month": 12, "lapsed_90d": 9,
 "by_source": {"stripe": 100, "microsoft_store": 12, "gift": 6, "staff": 2},
 "members": [{"username": "ana", "country": "BR", "until": …, "source": "stripe",
   "since": …, "last_payment": …, "months": 5, "subscriber": true}],
 "lapsed": [{"username": "bob", "country": "PT", "ended_at": …, "source": "gift", "months": 1}],
 "now": …}
```

A cancelled subscription shows as `lapsed` once its paid month ends.

### CSV exports

`GET /admin/api/export/{kind}.csv?from=…&to=…` — administrators. `kind`:
`players` (accounts created in the range), `matches`, `purchases`, `ledger`
(the coin ledger), `daily` (a line per day: active players, games, new
players, coins created and spent). The range defaults to the last 30 days.
Streamed (`text/csv; charset=utf-8`, an attachment), comma-separated with a
header line, fields quoted when needed and never read as a formula. Audit:
`export`.

### Calendar

`GET /admin/api/calendar?from=…&to=…` — staff (up to 400 days; default: a
week ago to six weeks ahead).

```json
{"from": …, "to": …, "now": …, "items": [
  {"kind": "promotion", "title": "Weekend sale", "starts_at": …, "ends_at": …, "id": 3, "status": "scheduled"},
  {"kind": "weekly_cup", "title": "Saturday Cup", "starts_at": …, "ends_at": …, "id": null, "status": "scheduled"}]}
```

`kind`: `event` (a boost), `tournament` (the staff's), `weekly_cup` (coming
ones as set now, and those finished), `promotion`, `maintenance`, `auction`,
`vip_item` (`id`: the item's code). `status`: `scheduled`, `live`, `ended`,
`cancelled`, `started` (maintenance).

## Public API

Read by the website. Under the server's public origin, CORS open
(`Access-Control-Allow-Origin: *`), JSON, `Cache-Control: public, max-age=60`.
Unix seconds. An unknown, banned or deleted player — or a name a player
left — answers 404 `{"error": "not found"}`.

`GET /api/players/{name}` (any case):

```json
{"username": "ana", "country": "BR", "avatar": "pic_eight", "vip": true, "supporter": true,
 "role": "player", "created_at": 1700000000, "last_seen_days": 2, "level": 23,
 "frame": "frame_gold", "cue": "cue_dragon", "cue_rarity": "legendary",
 "ratings": [
   {"rules": "brazilian", "rating": 1650, "peak": 1702, "tier": "master", "games": 120, "wins": 70, "rank": 12},
   {"rules": "official", "rating": 1200, "peak": 1200, "tier": "rookie", "games": 0, "wins": 0, "rank": null}],
 "achievements": [{"code": "golden_break", "name": "Golden break", "description": "…",
   "unlocked_at": 1700000000, "medal": "ruby", "secret": false}],
 "trophies": [{"kind": "tournament", "name": "Saturday Cup", "players": 16, "at": 1700000000, "tournament_id": 42}],
 "recent": [{"id": 9001, "at": 1700000000, "rules": "official", "rated": true, "opponent": "bob",
   "opponent_country": "PT", "won": true, "delta": 12, "reason": "EightBall"}],
 "career": {"games": 140, "wins": 80, "break_and_runs": 3, "best_streak": 7}}
```

- `avatar`: a code of the game's pictures (`""`: none). `tier`: `rookie`,
  `amateur`, `skilled`, `expert`, `master`, `grandmaster`, `legend`.
  `rank`: null without a rated game under those rules.
- `medal`: `bronze`, `silver`, `gold`, `diamond`, `ruby`. Names in English.
- `trophies[].kind`: `tournament`, `ladder`, `daily_challenge`;
  `tournament_id` links an official tournament (null otherwise).
- `recent`: the last 10 games; `reason` as the game's protocol writes
  `GameOverReason`. `supporter`: VIP now or before and chose to be in the
  credits. `career.games` counts rated and casual games, `wins` rated wins.
- `frame`, `cue`: what they use (`frame_none`, `cue_classic` by default).

`GET /api/tournaments?limit=50` (1–200) — finished official tournaments,
newest first:

```json
[{"id": 42, "name": "Saturday Cup", "rules": "official", "format": "knockout", "size": 16,
  "players": 14, "state": "finished", "starts_at": 1700000000, "ends_at": 1700003600,
  "champion": "ana", "runner_up": "bob", "prize_pool": 3400, "entry_fee": 100, "rated": true}]
```

`GET /api/tournaments/{id}`:

```json
{"id": 42, "name": "Saturday Cup", "rules": "official", "format": "knockout", "size": 16,
 "state": "finished", "starts_at": 0, "ends_at": 0, "rated": true, "entry_fee": 100,
 "prize_pool": 3400, "prizes": [2380, 1020], "item_prize": "cue_winter_2026",
 "champion": "ana", "runner_up": "bob",
 "players": [{"username": "ana", "country": "BR", "seed": 1}],
 "rounds": [[{"a": "ana", "b": "bob", "winner": "ana", "match_id": null, "score": ""}]],
 "standings": [{"username": "ana", "points": 12, "games": 5, "streak": 3}]}
```

`rounds` for knockouts (round 0 first; `a`/`b` null for a bye), `standings`
for arenas. Names are the players' names now. `match_id` is null (the
bracket doesn't keep the stored games' ids yet); `item_prize` null when none.

`GET /api/supporters` — the players who chose to appear in the credits (VIP
now or before), longest supporters first:

```json
[{"username": "ana", "country": "BR", "since": 1700000000, "months": 5, "vip": true}]
```
