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
| `time_attack_prize_1`, `_2`, `_3` | int 0–10000 | coins for the week's first three against the clock (default 100, 60, 30; 0: no prize for that place) |
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

## Inbox & moderation

What the staff tell one player goes to their **inbox** in the game. A
notification is JSON as the game reads it (see `snook_legend_proto::inbox`):

```json
{"id": 41, "kind": "warning", "text": "Mind your words.", "from": "boss",
 "created_at": 1795291200, "read_at": 0, "important": true,
 "pinned_until": 1795896000, "action": {"to": "none"}}
```

`kind` carries its own fields (`{"kind": "muted", "until": …}`,
`{"kind": "rating_adjusted", "rules": "official", "delta": -10, "rating": 1490,
"match_id": 9001}`, `{"kind": "staff_gift", "coins": 500, "item": "", "vip_days": 7}`,
`{"kind": "match_voided", "match_id": 9001, "opponent": "bob", "delta": 16}`,
`{"kind": "match_ended", "match_id": 0, "opponent": "bob", "won": true}`,
`{"kind": "report_acted", "username": "troll"}`, `unmuted`, `staff_message`,
`achievement`…). `action` is the button: `{"to": "replay", "match_id": …}`,
`{"to": "equip", "code": …}`, `{"to": "vip"}`, `{"to": "none"}`…

### Preview

Every endpoint below that notifies a player, and the player actions `warn`,
`mute`, `unmute`, `coins`, `item` and `vip` (`POST /admin/api/users/{name}/{action}`),
take `"preview": true` in their body: nothing is done, and the answer is the
notification the player would get, worded in English, Portuguese and
Spanish (the game words it in each of its languages):

```json
{"preview": true, "category": "staff",
 "notification": {"id": 0, "kind": "warning", "text": "Mind your words.", "from": "boss",
                  "created_at": 1795291200, "read_at": 0, "important": true,
                  "pinned_until": 1795896000, "action": {"to": "none"}},
 "rendered": {"en": {"title": "Warning from the staff", "body": "Mind your words."},
              "pt": {"title": "Advertência da equipe", "body": "Mind your words."},
              "es": {"title": "Advertencia del equipo", "body": "Mind your words."}}}
```

A preview of an action that tells nobody (`kick`, `ban`, …) answers 400.
Actions that notify two players (annul, end a game) add
`"players": [preview of seat 0, preview of seat 1]`.

What the existing player actions now also do: **warn** → `warning`,
important, pinned for 7 days; **mute** (`minutes`, `reason`) → `muted`,
pinned until it ends (the game shows the reason whenever the player tries
to write); **unmute** → `unmuted`; **coins** (positive), **item**, **vip** →
`staff_gift` (an item also records its origin in the collection). Resolving
a report with `"status": "actioned"` sends `report_acted` to the reporter
and to whoever has an open report about the same player (never the
punishment; a dismissal sends nothing).

### Message a player

`POST /admin/api/users/{name}/message` — staff (moderators not on staff).
Audit: `message`.

```json
{"text": "Please change your status.", "important": true, "pinned_days": 3, "preview": false}
```

`text` up to 1000 characters; `important`: the player confirms reading it;
`pinned_days` (0–366): kept at the top of the inbox. Answer:
`{"ok": true, "id": 41}`.

### Notifications sent, with read receipts

`GET /admin/api/users/{name}/notifications?limit=100` — staff. Newest first,
also those the player took out of their inbox (`deleted_at`):

```json
{"notifications": [
  {"id": 41, "kind": "staff_message", "category": "staff", "text": "Please change your status.",
   "from": "boss", "created_at": 1795291200, "read_at": 1795291260, "important": true,
   "pinned_until": 1795550400, "action": {"to": "none"}, "deleted_at": null,
   "title": "Message from the staff", "body": "Please change your status."}]}
```

`read_at`: null until read (for an important one: until confirmed).
`GET /admin/api/users/{name}` also returns the latest 50 as
`"notifications"`, plus `"renames"` (changes of name and country:
`{"user_id", "kind": "name"|"country", "old", "new", "at", "by"}`, `by`
empty when the player made it) and `"rating_adjustments"`
(`{"id", "user_id", "rules", "delta", "rating_after", "reason", "match_id", "staff", "at"}`).
Player objects (lists, detail, `GET /admin/api/me`) carry `"avatar"` (a code
from the game's avatars).

### Adjust a rating

`POST /admin/api/users/{name}/rating` — administrators. Audit: `rating`.

```json
{"rules": "official", "delta": -40, "reason": "Rating farmed with a second account",
 "match_id": 9001, "preview": false}
```

Either `delta` or `rating` (the rating to put); the result stays within
100–4000. The peak goes up with it, or down by as much (never under the
rating). The rating history (the player's chart) gets the change. The player
gets `rating_adjusted` (important; with `match_id`, a button to watch it; the
game must be one of theirs). Answer: `{"ok": true, "notification": 42, "user": {…player…}}`.

### Timeline

`GET /admin/api/users/{name}/timeline?limit=100&before=1795291200` — staff.
Newest first; `next`: pass it as `before` for the next page (null: no more).

```json
{"items": [
  {"type": "notification", "at": 1795291200, "id": 41, "kind": "warning", "title": "…", "read_at": null, …},
  {"type": "staff_action", "at": 1795291100, "action": "mute", "staff": "boss", "details": "30 min: spam"},
  {"type": "match", "at": 1795290000, "match_id": 9001, "opponent": "bob", "won": true, "rated": true,
   "rules": "official", "delta": 12, "reason": "eight_ball", "correspondence": false},
  {"type": "purchase", "at": 1795280000, "coins": -500, "reason": "buy cue_dragon"},
  {"type": "payment", "at": 1795270000, "kind": "coins", "coins": 1200, "amount": 499, "currency": "brl",
   "refunded": false, "test": false},
  {"type": "report_sent", "at": 1795260000, "id": 7, "target": "troll", "reason": "Harassment", "status": 1},
  {"type": "report_received", "at": 1795250000, "id": 8, "reporter": "ana", "reason": "Spam", "status": 0},
  {"type": "profile_change", "at": 1795240000, "kind": "country", "old": "BR", "new": "PT", "by": ""},
  {"type": "linked_account", "at": 1795230000, "username": "ana_alt"},
  {"type": "account", "at": 1795000000, "username": "ana", "country": "BR"}],
 "next": 1795000000}
```

`payment` items are for administrators only. `linked_account`: another
account registered from the same address (the database keeps it; a
server without one shows none).

### Presents to many players

`POST /admin/api/gifts/batch` — administrators. Audit: `gift` per player and
`gift-batch` (target `batch`) for the whole.

```json
{"usernames": ["ana", "bob"], "text": "caio, dani\nedu", "coins": 500,
 "item": "cue_winter_2026", "vip_days": 7, "reason": "Winter Cup finalists",
 "message": "Thanks for playing!", "batch": "cup-2026-12", "dry_run": true}
```

Names from `usernames` and/or pasted in `text` (commas, spaces, new lines;
a leading `@` is fine), 500 at most, each once. At least one of `coins`
(0–1,000,000), `item` (one sold for coins, a pack's or one only won) and
`vip_days` (0–366). `dry_run` answers without giving anything:

```json
{"dry_run": true, "players": 4, "unknown": ["edu"], "already_own": ["bob"],
 "notification": {…}}
```

Applied:

```json
{"ok": true, "batch": "cup-2026-12", "given": ["ana", "bob", "caio"],
 "already_given": ["dani"], "failed": [], "unknown": ["edu"], "already_own": ["bob"],
 "results": [{"username": "ana", "status": "given", "error": null}, …]}
```

Who has the item already gets the rest. Each gets `staff_gift` (with "Equip
now" for the item) for what was given to them now.

Safe to send again: the coins and the VIP days are given once per batch and
player (a ledger reference `gift:{batch}:{player id}`), so a retry after a
timeout or a partial failure only gives what is missing. `batch` (optional,
100 characters at most) names the batch; without it, the same present (the
same staff, reason, coins, item, VIP days and message) on the same day (UTC)
is the same batch. The answer's `batch` is the one to send with a retry.
One player's error doesn't stop the rest: each player's `status` is `given`,
`already_given` (nothing new for them) or `failed` (with `error`; `ok` is
false then and `failed` lists them: send it again).

### A stored game, shot by shot

`GET /admin/api/matches/{id}` — staff.

```json
{"match": {"id": 9001, "players": [12, 34], "names": ["ana", "bob"], "winner": 0,
           "reason": "eight_ball", "rated": true, "rules": "official", "delta": [12, -12],
           "ratings_after": [1512, 1488], "shots": 41, "ended_at": 1795290000,
           "tournament": null, "room": 1795289000123, "voided_at": null},
 "strokes": [
   {"index": 0, "kind": "shot", "seat": 0, "player": "ana", "power": 6.2, "aim_degrees": 181.4,
    "elevation_degrees": 0.0, "spin": {"side": -0.2, "top": 0.4}, "cue_ball": [635.0, 635.0],
    "called": null, "timeout": false, "think_secs": 7.3,
    "potted": [{"ball": 3, "pocket": "top_left"}], "fouls": [], "first_contact": 1,
    "break": true, "continues": true},
   {"index": 1, "kind": "time_foul", "seat": 1, "player": "bob", "think_secs": null}]}
```

`power`: the cue's speed (m/s); `spin`: where the tip struck (−1…1);
`cue_ball`: where it was placed with the ball in hand (mm); `think_secs`:
from the shot clock's start to the shot (games from 0.12 on; null
before, and for what isn't a shot). `strokes` is null without a replay.

### Annul a game

`POST /admin/api/matches/{id}/void` — administrators. Audit: `match-void`
per player. Body: `{"reason": "…", "preview": false}` (a reason is needed).

Only a rated game, once (409 after). Both players' rating changes are given
back (in their rating history), it leaves their rated games and wins, and a
peak it made is worked out again. Each gets `match_voided` with a button to
watch it. Answer: `{"ok": true, "match": {…as above, "voided_at": 1795291300…}}`.

### End a stuck live game

`POST /admin/api/live/{id}/end` — administrators (`id`: the live game's, from
`GET /admin/api/live`). Audit: `match-end` per player.

```json
{"winner": 0, "reason": "Opponent's game froze", "preview": false}
```

`winner`: the seat (0 or 1) given the game: the other resigns it (rating as
for a resignation; a tournament goes on). Without `winner`: ended without a
result, nothing recorded, the players are sent back to sign in (refused for
a tournament's game). Both get `match_ended`. Answer: `{"ok": true}`.

### Banned words

`GET /admin/api/words` — staff:

```json
{"words": [{"word": "puta", "lang": "pt", "added_by": "", "added_at": 0}], "max_word": 40}
```

`POST /admin/api/words` — staff. Audit: `banned-word` (target: the language).
Body `{"word": "zonk", "lang": "en"}`: a word or a phrase (40 characters at
most), `lang` a language code (`pt`, `en`, `es`, `pt-BR`…) or `*`. 409 if
listed already.

`POST /admin/api/words/remove` — staff. Audit: `banned-word-remove`. Same body.
404 if not listed.

`POST /admin/api/words/preview` — staff. `{"text": "Z0NK you"}` →

```json
{"blocked": true, "word": "zonk", "lang": "en", "tokens": ["zonk", "you"]}
```

(`tokens`: the text's words as matched: lower case, no accents, leetspeak
read, repeated letters once.) Every list applies to every text; matching is
by whole words (and runs of one- or two-letter words read together, for
"p.u.t.a"). Refused: lounge and table chat, status messages, "looking for a
game" notes, presents' messages and new usernames.

### The pinned chat line

`GET /admin/api/chat/pinned` — staff: `{"line": {…chat line…} | null}`.

`POST /admin/api/chat/{id}/pin` — staff (`id` from `GET /admin/api/chat`).
Audit: `chat-pin`. Answer `{"ok": true, "line": {…}}`; 404 for a line gone
or deleted. One line at a time: it replaces the one pinned before. Deleting
the line or clearing the chat takes it down.

`POST /admin/api/chat/unpin` — staff. Audit: `chat-unpin`. 400 if none.

Chat lines (`GET /admin/api/chat`, the pinned one) may carry `"mentions"`:
the real usernames written with `@` (also in announcements). A note on the
lounge's board (`POST /admin/api/board`) mentions players the same way:
the game shows them in gold (a click opens the profile) and a player
mentioned in a note just pinned is told so, as for the chat.
