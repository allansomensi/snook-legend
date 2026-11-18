# Staff portal

The server serves the staff portal at `/admin`: plain HTML, CSS and
JavaScript compiled into the server (`crates/server/admin/`), no external
resources, a strict Content Security Policy (no inline code or styles).
Staff sign in with their game account; the portal's session is separate
from the game's, is sent as a bearer token (never a cookie, so there is no
CSRF surface) and lasts 8 hours.

**Why in the server and not a separate site?** One deployment, the same
origin (no CORS, no API exposed to other sites), the same database
connection and direct access to the live lounge (who's online, games in
progress, the chat). A separate site would need a public admin API, CORS and
a second set of credentials for no gain at this size.

## Using it

- **Languages:** Português (Brasil), English and Español — picked from the
  browser, changed on the sign-in page, in the top bar or the account menu,
  and remembered. Dates, numbers, money and country names follow the
  language. The server's own messages (errors) are translated too.
- **Themes:** dark (default) and light, from the top bar; remembered.
- **Search** (top bar, or press `/`): players by name (a recovery e-mail,
  whole, finds its account too); `#123` opens game 123 in the replay viewer.
- **Links:** every page and filter has its own address
  (`#/players/ana/moderation`, `#/reports?status=open`…): bookmark it, send
  it to another staff member, use the browser's back button.
- **Keyboard:** `?` lists the shortcuts; `g` then a letter goes to a page
  (`g d` dashboard, `g p` players, `g r` reports, `g l` live, `g c` chat…);
  `Esc` closes a dialog or the replay viewer.
- **Phones:** the sidebar folds into a menu; tables scroll sideways.
- Destructive actions always ask first, and say what the player will see.

## Pages

| Page | What it is for |
|---|---|
| **Dashboard** | Who is online, games now, the queue, open reports, accounts, revenue; activity in the last 48 hours; the latest staff actions; the server's health; warnings (nothing saved, restarting, every player behind one address). |
| **Live** | Games in progress (watch any of them as it is played), the players online and what they are doing, tournaments with their brackets or arena standings. Refreshes every 10 s. |
| **Players** | Every account: search, filter (online, staff, banned, muted, deleted), sort (newest, seen most recently, rating, name), pages of 50. |
| **A player** | Tabs: *Overview* (numbers, account, standing), *Moderation* (reports about them, staff notes and warnings, the history of every staff action on the account), *Games* (watch or download each replay), *Economy* (coins, VIP, items, payments, Microsoft Store purchases, the coin history), *Achievements*. Actions in the header. |
| **Reports** | Open / action taken / dismissed / all, by reason, by player. Each report shows the game it is about (watch it), the chat before it, and how many reports there are about the same player. Warn, mute or ban from the report: it is then closed with what was done. |
| **Lounge chat** | The latest lines, newest first, with deleted ones (and who deleted them). Delete a line, remove it without a trace (administrators), warn or mute its author, clear the chat (administrators). Refreshes every 10 s. |
| **Fair play** | Players whose potting on hard shots is far above their tier's (a list to look at with the replays; nobody is punished by it). |
| **Board & announcements** | An announcement to the lounge now; the lounge's board (notice, event or rule, a button, pinned, for some days) with a live preview; the welcome message; players' "looking for a game" notes. |
| **Economy** | Coins in wallets, VIP members, coins given and spent in 30 days by reason, the largest balances, revenue; the latest payments of everyone (mark test payments); the shop's catalogue. |
| **Metrics** | Players and games by day and by hour, retention by week of sign-up, from account to player, where players come from (country), revenue by day, the "Play now" waits. |
| **Audit log** | Every staff action: search by words, filter by action or player, load more, export as CSV. |
| **Server** | Version, protocol and ruleset, uptime, memory, connections, writes waiting; health (database, restarts, addresses, Stripe, Microsoft Store); this server's rules; the weekly cup; the staff. |

## Roles

| | Moderator | Administrator |
|---|---|---|
| Every page above | ✓ | ✓ |
| Warn, mute / unmute, disconnect, ban / lift a ban, sign out everywhere, clear a status message | ✓ (not on staff) | ✓ |
| Staff notes (remove one's own) | ✓ | ✓ (remove any) |
| Answer reports, delete a chat line, announcements, the board, "looking for a game" notes | ✓ | ✓ |
| Remove a chat line without a trace, clear the chat, the welcome message | | ✓ |
| Rename, change a role, set a new password | | ✓ |
| Give or take coins, give items or VIP days, take an item back, mark payments as test | | ✓ |
| Call off a tournament, send a stuck Microsoft Store purchase again | | ✓ |

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
  in again under the new name.
- **New password** (administrators): for account recovery, after
  confirming who they are by their recovery e-mail (the server sends no
  e-mail); signs the player out everywhere.
- **Coins, items, VIP; take an item back** (administrators, always with a
  reason).
- **Delete a chat line**: players see it was deleted by the staff;
  **remove without a trace** (administrators): it disappears.
- **Call off a tournament** (administrators, before it starts): every
  entry fee goes back and the players are told.
- **Board**: notes pinned for everyone in the lounge's *Board* tab (rules,
  events, the next cup), for some days or until taken down. Each note is a
  notice (gold), an event (green) or a rule (blue), may be pinned above the
  others and may carry a button (to the tournaments, the shop, VIP or the
  missions). Players see a new note at once.
- **Test payments**: payments made with Stripe's test keys are marked
  *test* and left out of the revenue; others can be marked by hand. Coins
  they brought stay in the wallet: take them back with *Give coins* and a
  negative amount.

Every staff action is written to the audit log with who, what, whom and
when, and shows on the player's *Moderation* tab.
