# How online play works

## Pieces

- **`snook_legend_proto`**: every message between the game and the server (JSON
  objects with a `type` field, one per WebSocket text frame), plus what both
  sides must agree on: name rules, countries, rating tiers, achievements.
- **`snook_legend_server`**: the lounge, open tables, match rooms and tournaments
  live in memory behind one mutex (short, synchronous methods); everything
  slow runs outside it: the database (PostgreSQL, or memory for LAN games and
  tests) and shot simulation (a blocking task per shot).
- **The game** (`crates/client/src/net.rs`) keeps one WebSocket on a
  background thread (4 ms read slices, `TCP_NODELAY`), pings every 15 s and
  reconnects by itself. With a saved session it signs in as soon as the game
  starts, so the menu shows the player and the lounge opens instantly.

## A session

1. `hello { version, rules }`: the protocol version (22; older games are
   told to update) and the rules the player plays, `brazilian` or
   `official` (`set_rules { rules }` changes them later). "Play now", tables
   and invitations follow them, and the lounge shows who plays which.
2. `register { username, password, country }`, `login`, `resume { token }`
   or, on a LAN server, `guest { name }` → `auth_ok { player, token }`.
   `leaderboard` and `profile` also work before signing in.
3. The server sends `welcome`, `lounge`, `chat_history`, `tables` and
   `tournaments`, then keeps them current (`player_updated`, `chat`,
   `table_opened`, `tournament_updated`…).
4. Games start from an invitation (`invite { to, rated, rules }` →
   `answer_invite`), by sitting at an open table (`open_table { rated,
   rules, password }` → `join_table`), or by a tournament pairing, all
   ending in `match_started`. Every game is played under one of the rules,
   and its `game` carries them.

## A shot, without waiting for the network

```
shooter                     server                         opponent
  │ shoot{turn, stroke}        │                               │
  │──────────────────────────▶ │ validate (turn, cue ball…)    │
  │ (animates at once:         │── shot_taken ────────────────▶│ animates at once
  │  local prediction)         │ simulate (blocking task)      │
  │◀──────── shot_result{end} ─│── shot_result{end} ──────────▶│
  │ adopts `end` when the      │                               │ adopts `end`
  │ balls stop                 │                               │
```

- The physics is deterministic, so the prediction normally matches the
  server exactly (the end-to-end tests check it); the server's table is
  adopted at the end either way.
- A refused shot (`shot_rejected { turn, reason, game }`) rolls the table
  back. A `shot_taken` that isn't the predicted one (the clock ran out at the
  same moment) replaces the prediction.
- `match_ended` that arrives while a shot is still on screen is applied when
  the balls stop (the deciding shot is always shown, and a win on the 8-ball
  is never mistaken for a resignation).
- While aiming, `aim` is streamed ~20 times a second (relayed at most every
  33 ms); the opponent's cue glides towards the latest aim.

## Matches

- The shot clock runs on the server (it waits while the shooter's
  connection is down: the reconnection grace is their limit, and back at
  their turn they have at least 15 s; the opponent gets
  `opponent_connection { connected: true, clock_left }`; the first shot's
  clock also waits the 3 s of the match's introduction); when it runs out the server plays a full-power shot where the
  player was aiming (under the official rules it is a foul instead:
  `time_foul`, the cue ball in hand for the opponent).
- The official rules call the 8 (since protocol 25; games recorded before
  keep theirs: until 0.11.1 every shot but the break, from 0.11.2 to 0.11.x
  nothing, as the game's `calls` and `eight_call` say): on a shot at the 8,
  `shoot { called_pocket }` names its pocket (the player clicks it, as under
  the Brazilian rules), and the 8 made anywhere else loses. A shot at the 8
  without a call is refused (`shot_rejected`), as under the Brazilian rules.
  A game of protocol 24 can't call it: its shots are taken without a call,
  the server counts the 8 it makes as called in the pocket it went into, and
  records that call with the stroke. Some breaks leave a choice
  (`game.choice`): the player to move answers `choose { turn, option }`
  (`play_on`, `cue_ball_in_hand`, `break_again`, `opponent_breaks`) and both
  get `chose { seat, option, game }`; the shot clock takes the first option.
- `resign`, a disconnection longer than the grace period or the last ball end
  the match with `match_ended { result }`, ratings included. Then `rematch`
  (both players; not in tournaments) or `leave_match`. From `match_ended`
  on, both players are `available` in the lounge (also while they look at
  the result) and can be invited: accepting leaves the result (the opponent
  gets `opponent_left`). A `leave_match` right after a `resign` that waits
  for the shot being played waits with it.
- Reconnecting within the grace period resumes the match with a fresh
  `match_started`, followed by what the player would otherwise miss: the
  game's end, the opponent still away (`opponent_connection { connected:
  false, grace_secs }` with the time they have left), the opponent's chat
  lines said meanwhile (those of the last minute when the game signed in
  again before the server saw its old connection go; the game leaves out
  the lines it has, by writer, text and time), a rematch asked for, and
  who is watching.
- Ratings: Elo with K 24 for the first 10 rated games, 14 after, 8 from
  1900 (`crates/core/src/rating.rs`), kept apart for each rules: a player has
  a rating, peak and record under each (`PlayerInfo.standings`), and the
  ranking (`leaderboard { rules }`), ranks and rating charts are per rules.
  Feats (8-ball wins, runouts, shutouts) count in rated games only.
- `spectate { username }` watches a friend's games (anyone's on a local
  network): `match_started { spectating: true, seat }` with `seat` the
  friend's, then every shot, aim and result of that table, and the next game
  too (rematch or a new opponent) until `stop_spectating` or
  `spectate_ended` (the friend went back to the lounge). The players' chat
  is not sent to spectators. Everyone at the table gets
  `spectators { count }`. A spectator's `react` arrives as
  `reaction { seat: 2 }`, through a limit shared by the whole crowd.
- The shop: `shop` answers `shop_info` (what is sold for money) and
  `wallet` (coins, VIP time, owned and equipped items; also pushed after
  signing in and on every change). `buy_item { code }` and `equip { code }`
  are checked against `crates/proto/src/shop.rs`; `buy_coins { pack }`,
  `subscribe_vip` and `manage_vip` answer `payment_page { url }` (Stripe).
  Payments arrive through `POST /stripe/webhook` (see
  `crates/server/src/shop.rs`).
- Microsoft Store purchases (the Store build): `shop_info.store_packs` lists
  the coin packs and their add-ons' Store IDs. After a purchase in the
  Store's dialog (and once per session), the game sends `store_ticket`; the
  server answers `store_ticket { ticket, user_id }`, the Store makes a key
  from them and the game sends `store_redeem { key }`. The server asks
  Microsoft for the purchases, consumes and credits them, and answers
  `store_redeemed { coins, pending, error }` (also sent by itself when a
  purchase that was waiting for Microsoft is credited later). See
  `crates/server/src/msstore.rs`.
- `queue_join { rated, rules }` / `queue_leave`: "Play now", which pairs
  only players of the same rules; the server sends `queue { searching,
  waited_secs, range }` and then `match_started`.
- `react { code }` sends a reaction to the table (`reaction { seat, code }`
  to both players). Codes and what unlocks each are in
  `crates/proto/src/reactions.rs`; the server drops locked ones and allows a
  few a second. After signing in (and whenever one is unlocked) the server
  sends `my_achievements { codes }`, which the game uses to offer them.

## Tournaments

Single elimination for 4, 8 or 16 players (or an arena), under one of the
rules, seeded by the rating under them with byes for empty seats. It starts when full (or when the host starts it). Matches start by
themselves when both players are free; a player who is offline for two
minutes when their match is ready loses it. Players still in a running
tournament can't start other games.

Rated games use Elo (`snook_legend_core::rating`). See `docs/DEPLOY.md` for limits.

## Match and social (0.12)

- `match_started { stakes }`: in a rated game, each seat's `[win, loss]` in
  rating as the server reckons them at the start (the same Elo, halved and
  then nothing between players who met often that day). The game opens the
  match with an introduction (the two players face each other).
- Settings that follow the account: `save_settings { data }` (the game's
  JSON, at most 8 KB) → `settings_saved { updated_at }`; after signing in
  the server sends `settings { data, updated_at }` (empty, at 0, when none).
  The game keeps the newest (see `crates/client/src/sync.rs`).
- Direct messages between friends: `dm_send { to, text }` → `dm { message }`
  to both (stored for an offline friend); `dm_history { with, before }` →
  `dm_history { with, messages, more }` (50 a page); `dm_read { with }`;
  `dm_threads` → `dm_threads { threads }` (also after signing in when there
  are some). Only friends, never across a block, the chat's rules (mutes,
  age, quick phrases only) apply. A game older than protocol 25 (0.11) has
  no direct messages: it is sent neither `dm` nor `dm_threads` (it would
  drop them as malformed); what its player is sent waits for the update.
- Friends see what each other does: `friends.friends[].activity` and
  `friend_activity { username, activity }` whenever it changes (playing
  against someone, looking for a game, watching, the daily challenge…;
  `set_activity { pastime }` says what the game plays offline).
- Spectators chat apart: `spectator_chat { text }` → `spectator_chat { line }`
  to everyone watching that table (never to the players); those starting to
  watch get the last lines.
- A parent or guardian may keep an account under 18 to the quick phrases:
  `allow_chat { allowed, phrases_only }` (`age { phrases_only }`); the
  server refuses anything else in every chat and in messages.
- `advanced_stats { rules }` → `advanced_stats { stats }`: a VIP's success
  by pocket and cut angle, week by week, and their league's (others get
  `vip: false` and no numbers).

## New modes (0.12)

### Doubles

Two teams of two at one table (`crates/server/src/lounge/doubles.rs`,
`snook_legend_core::doubles`, `snook_legend_proto::doubles`). Only games of
protocol 25 or later are paired (the connection tells the lounge its
protocol after signing in).

- A pair: `doubles_invite { to }` (friends only; anyone on a LAN) →
  `doubles_invited { from }` / `doubles_invite_sent { to }`;
  `doubles_answer { from, accept }` → `doubles_pair { pair }` to both (the
  partner, the pair's search or table). `doubles_leave` ends the pair (and
  what it had going); signing out does too.
- Finding another pair: `doubles_queue { rated, rules }` ("Play now": pairs
  matched by the average of their doubles ratings, the range widening as
  the singles queue's), `doubles_queue_leave`; or `doubles_open_table
  { rated, rules }` → `doubles_tables { tables }` to everyone,
  `doubles_join_table { id }`, `doubles_close_table`.
- The game is a match room whose seats are the teams: each player gets
  `doubles_lineup { lineup }` and then the usual `match_started` (`seat`:
  their team; `players`: the teams' first players), and every match
  message after it. The line-up comes again whenever it changes (a visit, a
  connection, the result: `deltas`).
- The rotation: every time the table comes to a team, a new visit of that
  team begins, and the team's players take its visits in turn (the first
  player the first visit, the breaking team's first player breaks). Within
  a visit the same player shoots on. Only the player holding the cue may
  aim, place the cue ball, shoot or choose; either may resign for the team.
- Connections: a player whose connection drops is not waited for: their
  partner takes the visit (with at least 15 s on the clock). The team is
  away (`opponent_connection` to the other team, the reconnection grace
  running) only when both are; it has left the game when both are gone past
  the grace (or the shot clock runs out three times in a row, as in
  singles). Doubles games are saved after every stroke and handed to the
  next server like singles ones (since 0.12.1; `live_matches.partners` has
  the teams' second players): the next server rebuilds the table with all
  four away, each is back at it as they sign in again (the clock starts
  once both teams are), and a team none of whose players comes back within
  the grace has left the game. While a server drains, the doubles "Play
  now" and tables stop (each pair is told) and no doubles game starts.
- The rating: a doubles rating per player and rules, apart from the singles
  one (1200 to start). The expected score comes from the teams' averages;
  each player moves by their own K-factor. Leaving costs both players the
  usual extra. `doubles_leaderboard { rules }` → `doubles_leaderboard
  { rules, entries }`; `/api/leaderboard?mode=doubles`. A doubles game
  played to the end gives experience, and coins when rated (as singles);
  it doesn't count for the singles statistics, missions or achievements.
- Rematch: all four must ask (`rematch`); the other team breaks.
- History and replays (0.12.1): a player's doubles games are in their
  profile's `history` and in `replays`, by date among the singles ones.
  Such an entry has `doubles { partner, opponents }`, `opponent` "caio &
  duda", and `match_id` the doubles game's id made negative;
  `replay { match_id }` with it answers `replay` with the teams as the two
  sides ("ana & bia"). A 0.12.0 game shows the line without the doubles
  details and offers no button to watch it.

### Against the clock

A full rack cleared alone (`snook_legend_core::rules::time_attack`,
`snook_legend_proto::time_attack`, `crates/server/src/time_attack.rs`):
any ball first, every ball made stays down, the 8 last (made early it is
spotted and the shot is a foul); fouls (scratch, no ball hit, a ball off the
table, the early 8) give the cue ball in hand anywhere and cost 10 s.

- `time_attack` → `time_attack { info }` (the week's table, last week's
  winners, attempts left today, the player's best and place).
- `time_attack_start` → `time_attack_started { attempt, seed }` (a rack of
  its own for each attempt; 10 a day) or `time_attack_result { attempt: 0,
  result: Err(no_attempts_left) }`.
- `time_attack_submit { attempt, strokes }` (each stroke with the cue ball's
  place when in hand) → `time_attack_result { attempt, result }`: the server
  plays the strokes again; they must clear the table. The time is the
  server's, from the start to the strokes arriving (20 minutes at most),
  never less than the balls rolled, plus the penalties.
- The week runs Monday to Sunday (UTC). Once it is over, its three quickest
  get 300, 200 and 100 coins (paid once, checked every five minutes) and a
  note in the inbox (`tournament_result` named "Against the clock").
