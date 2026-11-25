# Staff portal

The staff portal is part of the game's website,
[snooklegend.app/admin](https://snooklegend.app/admin/): Next.js, React and
TypeScript, in the website's repository
([snooklegend.app](https://github.com/allansomensi/snooklegend.app), `src/app/admin`
and `src/admin`), published by Vercel with the rest of the site. It calls the server's staff API (`/admin/api`,
`crates/server/src/admin.rs`), which answers browsers only from the portal's
origin (the website's, `SNOOK_LEGEND_ADMIN_ORIGIN`, CORS without credentials); `/admin` on
the server leads to the portal. The portal has a strict Content Security
Policy (a nonce per page; the only server it talks to is the API).
Staff sign in with their game account; the portal's session is separate
from the game's, is sent as a bearer token (never a cookie, so there is no
CSRF surface), is kept for the browser tab only, and lasts 8 hours.

The server keeps what needs the live lounge (who's online, games in
progress, the chat, the replays rebuilt with the physics) behind the API;
everything on screen is the portal's.

## Using it

- **Languages:** Português (Brasil), English and Español — picked from the
  browser, changed on the sign-in page, in the top bar or the account menu,
  and remembered. Dates, numbers, money and country names follow the
  language. The server's own messages (errors) are translated too.
- **Themes:** dark (default) and light, from the top bar; remembered.
- **Search** (top bar, or press `/`): players by name (for administrators,
  a recovery e-mail, whole, finds its account too); `#123` opens game 123
  in the replay viewer.
- **Links:** every page and filter has its own address
  (`/admin/players/ana/moderation/`, `/admin/reports/?status=open`…): bookmark it, send
  it to another staff member, use the browser's back button.
- **Keyboard:** `?` lists the shortcuts; `g` then a letter goes to a page
  (`g d` dashboard, `g p` players, `g r` reports, `g l` live, `g c` chat…);
  `Esc` closes a dialog or the replay viewer.
- **Phones:** the sidebar folds into a menu; tables scroll sideways.
- Destructive actions always ask first, and say what the player will see.

## Pages

| Page | What it is for |
|---|---|
| **Dashboard** | Who is online and the games of the last 24 hours (each split by rules), games now, the queue, open reports, accounts, revenue; activity in the last 48 hours; the latest staff actions; the server's health; warnings (nothing saved, restarting, every player behind one address). |
| **Live** | Games in progress with their rules (watch any of them as it is played), the players online with what they are doing (looking for a game, against the computer, away…), their rules and their rating under them, tournaments with their rules and brackets or arena standings. Refreshes every 10 s. |
| **Players** | Every account with its rating under each of the rules (tier and rated games): search, filter (online, staff, banned, muted, deleted), sort (newest, seen most recently, highest rating under the Brazilian or the official rules, name), pages of 50. |
| **A player** | Tabs: *Overview* (a card for each of the rules: rating, tier, peak, record; numbers, account, standing), *Moderation* (reports about them, staff notes and warnings, the history of every staff action on the account), *Games* (the rules of each, filtered by rules; watch or download each replay), *Economy* (coins, VIP, items, payments, Microsoft Store purchases, the coin history), *Achievements*. Actions in the header. |
| **Reports** | Open / action taken / dismissed / all, by reason, by player. Each report shows the game it is about (watch it), the chat before it, and how many reports there are about the same player. Warn, mute or ban from the report: it is then closed with what was done. |
| **Lounge chat** | The latest lines, newest first, with deleted ones (and who deleted them). Delete a line, remove it without a trace (administrators), warn or mute its author, clear the chat (administrators). Refreshes every 10 s. |
| **Fair play** | Players whose potting on hard shots is far above their tier's, by the higher of their two ratings (a list to look at with the replays; nobody is punished by it). |
| **Board & announcements** | An announcement to the lounge now; the lounge's board (notice, event or rule, a button, pinned, for some days) with a live preview; the welcome message; players' "looking for a game" notes. |
| **Events** | Boosts and themed tournaments with a start and an end (see below): running now, coming up, past; how much experience and coins from games count right now. |
| **Economy** | Coins in wallets, VIP members, coins given and spent in 30 days by reason, the largest balances, revenue; the latest payments of everyone (mark test payments); the shop's catalogue. |
| **Metrics** | Players and games by day and by hour, retention by week of sign-up, from account to player, where players come from (country), revenue by day, the "Play now" waits (who waits, by rules). |
| **Audit log** | Every staff action: search by words, filter by action or player, load more, export as CSV. |
| **Server** | Version and protocol, uptime, memory, connections (by rules), writes waiting; health (database, restarts, addresses, Stripe, Microsoft Store); this server's settings; the weekly cup and its rules; the staff. |

## Roles

| | Moderator | Administrator |
|---|---|---|
| Every page above | ✓ | ✓ |
| Players' e-mails, payments and Microsoft Store purchases; where the staff sign in from | | ✓ |
| Warn, mute / unmute, disconnect, ban / lift a ban, sign out everywhere, clear a status message | ✓ (not on staff) | ✓ |
| Staff notes (remove one's own) | ✓ | ✓ (remove any) |
| Answer reports (moderators: not about themselves nor about staff; an administrator also those about themselves, logged), delete a chat line, announcements, the board, "looking for a game" notes | ✓ | ✓ |
| Remove a chat line without a trace, clear the chat, the welcome message | | ✓ |
| Rename, change a role, set a new password | | ✓ |
| Give or take coins, give items or VIP days, take an item back, mark payments as test | | ✓ |
| Call off a tournament, send a stuck Microsoft Store purchase again | | ✓ |
| Schedule and call off events | | ✓ (moderators see them) |

The first administrators come from `SNOOK_LEGEND_ADMINS`; they can then
promote others from the portal. Nobody acts on their own account, except
signing out everywhere, notes and, for administrators, their own wallet —
all in the audit log like any other change.

## What each action does

- **Warn** (a text): the player sees it at once if online, else when they
  next sign in (in their game, in a notice that stays up longer than
  others). The player's page shows whether it was seen.
- **Staff note**: kept among the staff on the player's page; the player
  never sees it.
- **Mute** (15 minutes … 30 days, with a reason): the player can play but
  not chat or post on the board.
- **Ban** (1 day … for good, with a reason, previewed as the player reads
  it): the player is disconnected, every session is revoked and signing in
  shows the reason and how long. Banned players leave the leaderboard.
- **Disconnect**: closes the connection; a game in progress waits for them.
- **Sign out everywhere**: revokes every saved session.
- **Rename** (administrators): for a name that breaks the rules. It doesn't
  use one of the player's own three changes; they are signed out and sign
  in again under the new name. Not while they are in a game, at a table,
  in the queue or in a tournament (whoever took the name freed could take
  their seat): rename them once they are out.
- **New password** (administrators): for account recovery, after
  confirming who they are by their recovery e-mail (the server sends no
  e-mail); signs the player out everywhere.
- **Coins, items, VIP; take an item back** (administrators, always with a
  reason).
- **Delete a chat line**: players see it was deleted by the staff;
  **remove without a trace** (administrators): it disappears.
- **Call off a tournament** (administrators, before it starts): every
  entry fee goes back and the players are told. A weekly cup called off
  stays off; the next week's opens as usual.
- **Board**: notes pinned for everyone in the lounge's *Board* tab (rules,
  events, the next cup), for some days or until taken down. Each note is a
  notice (gold), an event (green) or a rule (blue), may be pinned above the
  others and may carry a button (to the tournaments, the shop, VIP or the
  missions). Players see a new note at once.
- **Test payments**: payments made with Stripe's test keys are marked
  *test* and left out of the revenue; others can be marked by hand. Coins
  they brought stay in the wallet: take them back with *Give coins* and a
  negative amount.

## Events

Scheduled on the *Events* page (administrators), with a title players see
as written, a start and an end in the staff member's time zone, and quick
picks (now for 24 hours, this weekend, the next 7 days; tonight or
Saturday at 8 pm for a tournament).

- **Experience ×2 or ×3**: every bit of experience counts that many times
  while it runs (games, the daily challenge, drills). Up to two weeks.
- **Coins from games ×2 or ×3**: the coins for finishing a rated game, and
  the day's cap with them. Purchases, prizes and the daily bonus are as
  usual. Up to two weeks.
- **Themed tournament**: under the Brazilian or the official rules (a rated
  one counts for the rating under them), a knockout (4, 8 or 16 seats) or an arena (10
  minutes to 3 hours, from the start to the end) that the server organises
  like the weekly cup: it opens for sign-up two hours before it starts,
  starts by itself, and is called off (fees back) if fewer than two
  signed up. It may be rated, take an entry fee and carry a **prize from
  the house** (paid 70% to the champion and 30% to the runner-up, on top of
  the entry fees' prizes). The house's prize needs four players or more,
  and is paid only to a player who won a game at the table (a knockout) or
  scored in games played (an arena): two accounts of one person meeting by
  walkover earn nothing from it.

Boosts of the same kind don't add up: the largest one running counts.
Players see the events running and those starting within two days on the
main menu and in the lounge, with what each does and when it starts or
ends, and an experience or coin notice says "(×2)" while a boost runs.
Calling an event off stops a boost at once; a tournament that is open for
sign-up is called off with every fee back (one that started plays on).

Every staff action is written to the audit log with who, what, whom and
when, and shows on the player's *Moderation* tab.

## Operations & economy

Pages added in 0.12 (the endpoints are in [ADMIN-API.md](ADMIN-API.md),
"Operations & economy"). Moderators see them all except VIP members and the
exports; administrators change them. Every change is in the audit log.

| Page | What it is for |
|---|---|
| **Settings** | The server's settings, changed live without a restart: the shot clock, the lounge's message, connection limits, tournaments (how many at once, the house's cut of entry fees), the weekly cup (on or off, day, time, time zone, rules, arena or knockout, length, seats, fee, house prize), the username change's price, the daily bonus's streak, VIP's item of the month. Each shows its value, its default, where that comes from (built in, the environment, the portal), what it does and its range; reset one to its default. |
| **Promotions** | Sales (1–90 % off items or whole categories) and showcases (items shown first, highlighted) with a start and an end (up to a month): scheduled, running, ended, called off. Players see them at once; purchases and gifts charge the sale's price. |
| **Official tournaments** | The weekly cup's next and the staff's tournaments (knockouts of 4 to 32 seats or arenas): scheduled, open with their prize pool now, finished with champion and prizes. Schedule one with an entry fee, a prize from the house, the champion's share and an item for the champion. |
| **Maintenance** | Plan a stop (time, expected length, a message): players get a countdown banner and lounge notices at 60, 30, 15, 5 and 1 minutes; at the time no new games start, games in progress finish, and the server restarts. Call it off before it starts. |
| **Auctions** | Put a rare item up (start, end, least bid, least raise), follow the bids, call one off (the leader's coins go back). |
| **Analytics** | Coins created and spent per day by source and by sink, coins in wallets over time and the economy's health; best and worst sellers (7 days, 30 days, all time, never sold); players by country (new this week, online now); players online by weekday and hour in a chosen time zone; VIP members (active, renewed, new, lapsed, by how they pay — administrators). |
| **Exports** | CSV of players, games, purchases, the coin ledger or the daily numbers for a date range (administrators). |
| **Calendar** | Everything scheduled in one place — events, tournaments, the weekly cup, promotions, maintenance, auctions and VIP's items of the month — to avoid clashes. |

- **The house's cut** of entry fees leaves the economy (a coin sink); the
  rest are the prizes (70 % to the champion and 20 % to the runner-up with
  the usual 10 % cut, or the champion's share set on an official
  tournament). The game shows the **current prize** of each tournament as
  players join.
- **Auctions**: a bid holds the coins; whoever is outbid gets them back at
  once and a notice in their inbox; a bid in the last two minutes adds two
  minutes; the winner gets the item for good and the coins are gone.
- **Paid username changes**: after the three free ones, a change costs the
  set price (0: no more changes).
- **The daily bonus's streak**: day 1 brings the base, each day in a row the
  step more, up to the set day; a day missed starts again.
- **VIP's item of the month**: every member VIP during the month gets it on
  signing in (or renewing) and keeps it after cancelling.
