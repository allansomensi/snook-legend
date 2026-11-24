# Changelog

## 0.11.1 (protocol 24)

Fixes for the lounge and the chat: players are free as soon as a game ends,
the match chat can be made taller, every chat has a scroll bar to drag,
and the chat shows many more languages. The protocol is unchanged: 0.10.x
and 0.11.x games play together.

### Lounge
- **Free as soon as the game ends**: a game over no longer counts as
  played. Both players show as free in the lounge (and on the friends'
  list) the moment it ends, also while they still look at the result, and
  can be invited then: accepting leaves the result for the new game (the
  opponent is told, as when going back to the lounge). Before, each stayed
  "playing" until they left the result screen.
- **Leaving as the last shot rolls**: "Leave" sends a resignation and then
  the leaving; with a shot still being played the resignation waits for
  it, and the server refused the leaving ("Resign first"), so the player
  stayed at the table, unable to join "Play now" ("Finish your game
  first"). The leaving now waits with the resignation, and the player is
  back in the lounge when the game is over.

### Chat
- **The match chat can be made taller or shorter**: its top edge is
  dragged (the pointer turns into up-down arrows, and a short bar marks
  it), from a small strip up to the top bar. The height is kept in the
  settings (`chat_height`) for the next games.
- **A scroll bar you can drag** in every chat (the match's, the lounge's
  and the game history offline): the thumb is as long as the part shown, is
  dragged, and a press on the track brings it there. The wheel still works.
- **The quick phrases no longer send one on the click that opens them**: in
  a small window the list, kept below the top bar, opened over its own
  button, and the click that opened it chose the phrase under the pointer.
  The list now opens above the button or, without room, below it (never
  over it), its rows a little closer when the window is short, and the
  opening click never chooses. The reactions' picker opens the same way.
- **Every language in the chat**: the Chinese font has all of GB 2312 and
  the traditional characters of Big5 level 1 (with Bopomofo), and the
  Japanese one the level 2 kanji too, so the characters of everyday chat
  ("喔", "呃", "喵", "來", "賽", "綺麗"…) show instead of a blank. New fonts
  for Gurmukhi, Gujarati, Odia, Tamil, Telugu, Kannada, Malayalam, Sinhala,
  Khmer and Myanmar (shaped) and Ethiopic. A test checks chat lines in
  about forty languages for a glyph for every character.
- **Joiners kept inside words**: the zero-width non-joiner and joiner
  between two letters of Arabic, the scripts of India and Sri Lanka,
  Myanmar and Khmer stay in chat lines (Persian's "ضربه‌ی", Sinhala's
  "ශ්‍රී"); between Latin letters they still go.

### Reactions
- **A reaction had still says how it is won**: under the pointer, in the
  match's picker and on the profile, every reaction shows its achievement
  (or the shop, VIP, or "Every player has it"), not only the locked ones.

### Official rules
- **The pocket the aim points at is no longer lit**: the call that follows
  the aim showed a ring on that pocket (and named it in the status line),
  which told where the aim sends the ball. Only a pocket the player calls
  by clicking it is shown now; the call itself still follows the aim. The
  rules' page no longer mentions the ring.

## 0.11.0 (protocol 24)

Ruby medals and 26 new reactions (12 of them in the shop), a clearer
profile, ties on the daily challenge settled by time, daily games set up
before they are sent, and a Store version that can sell nothing. The
protocol is unchanged: 0.10.x and 0.11.x games play together.

### Microsoft Store
- **The Store version can be built without any purchase for real money**:
  coin packs and months of VIP are sold in the Store package only when the
  repository variable `MSSTORE_PURCHASES` is `on` (the workflow then
  builds with the feature `store-purchases`). Without it, coins are only
  earned by playing and VIP's page is not shown (except to who is VIP), so
  the age rating needs no "in-game purchases". The server's Store
  settings stay as they are; a purchase paid in an earlier version and
  never received still arrives. Steps in docs/MSSTORE.md.

### Menu and settings
- **Quit is an open door, with its name**: the power sign read as "turn
  off"; the button now says "Quit" like the others in the row.
- **Interface sounds can be turned off** (Settings → Sound): the click of
  buttons, tabs and windows, apart from the game's own sounds and the
  system notifications.

### Daily challenge
- **On a tie, the quickest wins**: the day's table still goes by the balls
  made, and among those with as many, the attempt that took the least time
  (from its start to its strokes reaching the server, measured by the
  server) is ahead. A later attempt with as many balls in less time
  improves the day's best. The table shows each player's time, and the
  challenge's page the time of one's own best. Scores made before this
  count as having taken the whole time allowed (migration `0025`).

### Achievements and reactions
- **Ruby medals for the near impossible**: a fifth medal above diamond, a
  cut ruby set in gold on a black and gold ribbon, for the golden break,
  Legend (2100), ten wins in a row and ten break-and-runs.
- **Achievements of one set of rules say so**: the golden break can only
  happen under the Brazilian rules (the official ones spot the 8 again),
  and its description now starts with "(Brazilian rules)".
- **26 new reactions**: 14 more come with achievements (a first friend,
  ten wins, a break and run, the tour, a year at the table, level 50…),
  and 12 are sold in the shop for 300 to 600 coins, in a new Reactions
  category (they can be given to friends too). A bought reaction is kept,
  not "put on": it is in the match's picker at once. The server checks
  the wallet before passing a reaction on.

### Profile
- **An opponent's name opens their profile**: in the games list, the name
  is a link (underlined under the pointer); Back walks back through the
  profiles visited.
- **A daily game is set up before it is sent**: challenging a player to a
  daily game (from a profile or the lounge) opens the invitation dialog,
  to pick rated or casual and the rules, instead of sending at once.
- **The levels' ladder shows where the player is in it**: no card above
  the list; the current level is a lit step among the levels that bring
  an item (with the bar to the next level), the highest at the top like
  the leagues beside it.
- **The country is changed on the flag**: one's own flag by the name lifts
  under the pointer, with a pencil over it, and a click opens the list of
  countries (with no country yet, a globe stands in for it). The globe
  button is gone.
- **A calmer header**: the account button sits beside the ghost's, on the
  name's line; "Add a status message" is a quiet line that lights up under
  the pointer instead of a framed button; the rating cards don't show a
  peak before any rated game, and the win rate reads "–" until there is one.

## 0.10.3 (protocol 24)

The tour won is really restored, and the staff portal is a site of its
own. The protocol is unchanged: 0.10.x games play together.

### Fixes
- **A tour already won comes back, for real**: 0.10.2 looked for the
  champion's cue among the items bought, but the cue comes with the tour's
  achievement (it is never bought), so the tour never came back. Signed in
  with an account that has the tour's achievement, the tour is complete
  again (every opponent unlocked), also when the computer's `ladder.json`
  counted only some of them.

### The staff portal
- **The staff portal left the server**: it is now part of the website
  (Next.js, TypeScript and React, in the website's repository), at
  `snooklegend.app/admin`, calling the server's staff API (`/admin/api`)
  from the browser. Every page, action and language is there, with a
  strict Content Security Policy with a nonce per page.
- **Only the portal may call the staff API from a browser** (CORS): its
  origin is `SNOOK_LEGEND_ADMIN_ORIGIN` (`https://snooklegend.app`),
  no credentials (the session stays a bearer token). Without it, no
  browser on another site may call it.
- **`SNOOK_LEGEND_ADMIN_HOST` is no longer used** (the server warns when it
  is still set); the server's `/admin` now leads to the portal
  (`{origin}/admin/`; not found
  when no origin is set), and it no longer carries the portal's files.

## 0.10.2 (protocol 24)

A tour already won is never lost, and the game's own addresses: the game
on `api.snooklegend.app`, the terms and the privacy policy on the website,
the staff portal on `admin.snooklegend.app`. The protocol is unchanged:
0.10.x games play together.

### Fixes
- **A tour already won is never lost**: the tour's progress is kept on the
  computer (`ladder.json`), its reward (the champion's cue) on the account;
  a new install, another computer or the Store's own folder started the
  tour over, every opponent locked, though the cue was won. Signed in with
  an account that owns the tour's cue, the tour is complete again. (The
  wallet is now asked for on signing in.)

### The game's own addresses
- **The game connects to `wss://api.snooklegend.app/ws`**, the public
  server on the game's own domain (the host can change without a new
  version). A session saved for the old address
  (`snook-server.onrender.com`) is good for the new one: no one signs in
  again; and if the new address doesn't answer, the game uses the old one.
- **The terms and the privacy policy live on the website**
  (`snooklegend.app/privacy/`, `/pt/privacidade/`, `/es/terminos/`…, in the
  site's frame; their texts moved to the site's repository, who provides the
  game filled in from the site's `LEGAL_*` variables). The game opens them
  there in the player's language; the server keeps no copy: its old
  addresses (`/termos`, `/privacy`…) and its root redirect there, so old
  links keep working (`SNOOK_LEGEND_SITE_URL` points elsewhere). The
  server's `SNOOK_LEGEND_GAME_NAME`, `_PRIVACY_CONTROLLER`,
  `_LEGAL_DOCUMENT`, `_LEGAL_ADDRESS` and `_PRIVACY_CONTACT` are gone.
- **The staff portal on its own address**: with `SNOOK_LEGEND_ADMIN_HOST`
  (`admin.snooklegend.app`), `/admin` answers there only; it is never
  indexed by search engines.
- The steps, in order: `docs/DEPLOY.pt-BR.md`, section 11.

## 0.10.1 (protocol 24)

Fixes for 0.10.0: screens that stayed dark, and a session that asked to
sign in again. The protocol and the server are unchanged.

### Fixes
- **The screens no longer go dark**: since 0.10.0, opening the lounge left
  the interface dimmed, and back in the menu it stayed so. A screen fading
  in made the drawing opacity that some parts save and set back smaller
  each frame, for good; the fade now applies to what is drawn only, and
  every frame starts at full opacity.
- **Signed in stays signed in**: a passing trouble when the saved session
  was used (the server busy, too many tries) made the game forget it, and
  after the connection dropped (or the account was taken by another place)
  the lounge asked to sign in again. Now the session is forgotten only when
  the server no longer knows it, or on signing out; whenever the
  connection opens again it is used. The menu shows the account as offline
  (a click connects) instead of "Sign in". One place at a time, as before:
  signing in elsewhere disconnects the other place.
- **Over a game, no second count of players online**: the rules' badge at
  the top has it; under the top bar, for a moment, only who just came
  online.

## 0.10.0 (protocol 24)

A new look for the whole game, and profile pictures that are things of the
game. The protocol is unchanged: 0.9.x and 0.10.0 play together. Update the
server with this version: an older server refuses the new profile pictures.

### A new look
- **Inter for the interface's text**, a typeface made for screens (cut to
  the game's scripts by `tools/gen_ui_fonts.py`), with figures all as wide
  as each other so clocks and scores don't jiggle, and big titles in Inter
  Display. DejaVu stays for the symbols Inter lacks, Noto for the other
  scripts.
- **One family of icons**: Phosphor's (MIT), with the game's own (the table,
  a coin, a cue, a ball, chalk) drawn in its style, in a font made by
  `tools/gen_icons.py`: drawn as text, sharp at any interface size, in an
  outline and a filled style (the chosen tab, earned stars).
- **Smooth edges**: rounded corners, outlines, circles and lines get a
  one-pixel anti-aliased fringe (as Dear ImGui and egui draw), whatever the
  graphics card's anti-aliasing.
- **The wordmark**: the name in gold lit from above, with a fine dark edge
  and a soft shadow (`tools/gen_wordmark.py`), in the menu and the lounge.
- **The menu**: the ways to play as cards with their icon in a disc, a
  chevron and an edge that turns gold under the pointer; along the bottom,
  each place with its name (Ranking, Profile, Shop, Rules, Settings), and
  quitting apart.
- **Quieter, rounder surfaces**: panels, buttons, fields, tabs and switches
  with soft edges (gold is for what is chosen or under the pointer), softer
  shadows and a faint light along their tops; a field with the keyboard
  glows gold. The settings' tabs have icons.
- **Motion and sound**: a screen fades in as it opens; buttons light up and
  dim under the pointer instead of snapping; pressing one plays a soft,
  short click.
- **The shop's pictures**: cloths in their own woven texture on a corner of
  a table under the lamp; ball sets racked on the cloth, each shaded as a
  sphere with the lamp's glint and a soft shadow.
- **Clear states**: the shop signed out says once what signing in is for,
  and its buttons lead there (no row of dead grey buttons); the ranking and
  a profile, when the server can't be reached, say so plainly, with the
  details in small type and a Try again button.

- **Profile pictures are things of the game**: the 32 faces are gone, and
  32 new pictures take their place (the balls from 2 to 14, the nine-ball
  diamond, the empty rack, the rest, a wall rack of cues, a pocket's net,
  the score beads, a stopwatch, a bank shot, a massé, a jump, a
  combination, the ghost ball, a kiss, the table brush, a cue case, a gold
  8, a winged 8, a podium, the strike, the snooker reds and a cue on fire).
  A face chosen before is shown as one of them (`RETIRED_AVATARS`), and the
  server keeps that one when an older client sends a face. Update the
  server with this version: an older server refuses the new pictures.

### Fixes
- The game no longer crashes on the way out after drawing text in Chinese,
  Japanese, Korean or with the new icons (a font freed after the window).
- The credits line was never translated (its key had drifted); it is now,
  and names Inter and Phosphor.

## 0.9.1 (protocol 24)

The chats redone, a cleaner table top and profile, smoother watching, and
signing out to switch accounts. The protocol is unchanged: 0.9.0 and 0.9.1
play together; the server has the lounge's TV fix.

- **Signing out, to switch accounts**: on your own profile, the account
  (the gear) has "Sign out" by Close, as a quiet link; the sign-in form
  comes next, to sign in to another account or make a new one. The
  lounge's ⏻ button for it is gone (the same sign quits the game in the
  menu, and signing out is seldom wanted).
- **The chat field grows with what you write**: a long line wraps over up
  to four lines (it used to scroll sideways, out of sight), in the lounge
  and at the table, with "180/200" at its corner near the limit. Up and
  Down move between its lines. The chat at the table is a little wider
  and taller.
- **The shot's chance, while watching, no longer blinks**: it went off at
  every twitch of the aim (which arrives over the network and never quite
  stops) and came back a moment later. It now fades in and out, stays put
  while the aim moves (dimmed until the new answer comes), its bar slides
  to the new chance, and a waver of the aim isn't a new shot.
- **The chats, redone** (the lounge's and the table's, one design):
  each author in a colour of their own (yours gold), a player's lines in a
  row under one name, a little room between authors, the newest against
  the field (no gap down from the top), and the game's notices small,
  quiet and centred. At the table the chat is glass while nobody uses it
  (the game shows through) and solid under the pointer or while typing;
  its tools (quick phrases, reactions, hide) sit in its header instead of
  beside it. The lounge has the emoji tray and shows your lines at once
  too, and a single line in its field sits in the middle. "Newer
  messages" is a pill that brings them back with a click.
- **The profile's actions are icons**: a daily game (the hourglass, as in
  the lounge), add or accept a friend (green), end a friendship, block
  (red while it waits for the second click) and report (a flag) sit in a
  row under the ghost's button, each named by its hint; one's own
  profile has its account (a gear) and country (a globe) there. The row
  of buttons is gone, and the profile is shorter.
- **Achievement medals, drawn anew**: a ribbon in a V coloured by the
  medal's metal, a metal disc lit from the top left with a rim, an inner
  ring and an embossed star (diamond ones sparkle), on a soft glow of
  their metal. Medals still to win are grey and faint, with a small lock.
- **The profile, tidier**: on the Rating tab the rules' tag sat over the
  tabs ("Leagues and levels"); it is in the chart's corner now. The chart
  has a soft fill, a dot per game, a scale, dashed league lines and the
  rating now by its last point (one date when all games were the same
  day). The header no longer repeats the league (the rating cards have
  it) nor the country's name (the flag says it); both rating cards look
  alike, the one shown lit in its rules' colour, and the note that was
  cut short is the hint of that card. Block, report and ending a
  friendship are quiet words beside the buttons. Another player's level
  shows no empty XP bar, and the leagues show their whole prize, where
  they start under their name.
- **The top of the table, tidier**: each player's rail is just long
  enough for a group and its 8, with a faint place for each ball still to
  come (it had room left over even with the 8 down). The shot clock is a
  ring around the picture of the player at the table, red with the seconds
  for the last ten; the rails no longer end in the clock or the league.
  "Open table" shows once, across both rails. The players' boxes are a
  little wider (names are cut less). The rules' badge is compact (their
  mark and short name) and says how many are online who play them (a
  green dot and the number); its hint gives the full name.
- **The closed chat at the table** is a bubble button as small as the
  tools beside it, the number of new lines on a gold badge at its corner
  ("9+" past nine). The newest line shows above it for 8 seconds, stays
  while the pointer is on it, and a click on it opens the chat.
- **The chat at the table answers at once**: a line shows the moment it is
  sent (dimmed until the server passes it on), not after the round trip to
  the server; a line the server refuses (too fast, repeated, muted) leaves
  the chat and the notice says why.
- **Emoji in the chat**: a 😊 button by the field opens a small tray over
  the chat (forty emoji, the ones used lately on top). An emoji goes where
  the caret is, the field keeps the keyboard, and the tray stays open for
  more (Esc or a click elsewhere closes it).
- **A game just left isn't offered to watch**: the lounge's TV was picked
  again only every 4 seconds, so a player back in the lounge saw the game
  they had just left. It changes the moment a game starts, ends or loses
  a player, and the lounge never offers a player's own game.
- **Reactions while watching a game**: the list opened and closed at once
  (a spectator has two reaction buttons, and the second one closed it).

## 0.9.0 (protocol 24)

The game in 27 languages, with every script drawn as it is written.
The protocol and the server are unchanged: 0.8.3 and 0.9.0 play together.

### The game in 27 languages
- **24 new interface languages**, for the world's most spoken languages and
  more: Portuguese of Portugal, French, German, Italian, Dutch, Swedish,
  Polish, Czech, Hungarian, Romanian, Greek, Russian, Ukrainian, Turkish,
  Indonesian, Filipino, Vietnamese, Thai, Hindi, Bengali, Arabic, Chinese
  (Simplified), Japanese and Korean — every text, the rules of both games,
  "what's new" and the countries' names (iso-codes). The system's language
  is picked on the first start (Austria and Switzerland get German, Angola
  and Mozambique the Portuguese of Portugal).
- **Every script is drawn**: Chinese, Japanese and Korean from Noto Sans CJK
  (cut down to the common characters, so chat and names read too), and
  Arabic, Devanagari, Bengali and Thai shaped as a browser would (rustybuzz,
  a port of HarfBuzz), with Arabic read right to left and the numbers and
  Latin names inside it left to right. Lines break between Chinese and
  Japanese characters, never before their closing punctuation, and at the
  word breaks of Thai.
- **The language is chosen from a list** in Settings → Game (a row of
  buttons no longer fits them), each language in its own name.
- Dates are written the way each language writes them; the rules' names
  inside a sentence are each translation's own ("die brasilianischen
  Regeln"). The terms of use and the privacy policy stay in Portuguese,
  Spanish and English (legal texts are not machine-translated).

## 0.8.3 (protocol 24)

Two more reviews of the whole game and what players asked for: an item
earned is shown, the pockets are finished cleanly, the zoom stays into the
3D view, one game at a time on a computer, and the 8 on a break handed
over is the 8 of any shot. Protocol 24, the oldest let in too: the rules a
replay is played back with changed (the 8 on a Brazilian break handed
over), and a game of 0.8.2 would show other tables than the server's, so
it is asked to update. No new migration.

### A fourth review
The game:
- **A break that hits nothing and loses the cue ball** was told as "cue
  ball in hand behind the head string": it is played from the head spot,
  and the table says so.
- **A rematch moves the alternating break on**: after a rematch at this
  computer, the next game started was broken by the same side again.
- **A click on the cue** aims at the ball, or calls the pocket, lying under
  the stick (it was swallowed along the whole stick); on bare cloth it does
  nothing. No cue is grabbed while a decision after the break is pending
  (none is drawn).
- **A five-round arena that ended before 0.8.2** stays ended: it reopened
  at "round 6 of 7" (`cup.json` now says how many rounds its arena has).
- **A tournament game left while its last shot rolls** counts as that shot
  ends it, not as a game lost.
- A game saved by 0.8.1 resumes with the breaker it had.

- **The 8 on a break handed over** (the cue ball lost on the break, the
  balls lying where they stopped) is the 8 of any shot: hit first it is a
  foul, and made it loses the game. The incoming player could win at once
  by potting it straight in, and the computer went for it. Only the break
  of the rack (the game's first shot, or a break passed over with the balls
  still racked) has the 8 alone winning (`EightBallGame::break_on_rack`).
  The rules book in three languages says so.

Online, in the game:
- **A table joined after its game ended** (back from a lost connection,
  or watching a friend at the end panel) shows the end and its result, not
  a turn nobody can play: the server sends the result again.
- **Both back after the server restarted**: the first one back saw a shot
  clock of minutes, and lost a ball with time apparently left. The clock
  shown starts afresh when the opponent is back, as the server's does.
- **A rematch the server refused** (it is closing, a cooldown) left
  "Waiting for…" for ever, with no way to accept the opponent's.
- A shot that left as the clock ran out isn't kept as the last shot, and a
  stroke queued while reconnecting isn't sent to a table that changed
  meanwhile.

The server:
- **The break alternates by who broke last** between two players (within a
  day), not by the parity of their games: a game under the official rules,
  a correspondence game ending or the day's count rolling over gave the
  same player two breaks running.
- **A knockout's host away while it filled up** lost the first round at
  once: the time away before the start no longer counts.
- A game dropped with both players gone can't be rebuilt from its saved
  copy; a player back at a finished table doesn't announce it to an
  opponent who is already elsewhere.
- **A month of VIP given** counts from the payment, not from when the
  payment page was opened (a boleto paid three days later gave 28 days). A
  delivery that stopped half way still records and announces the present
  (and the welcome to VIP) when it comes again.
- Friendships accepted and removed at the same moment no longer end up
  one-way (PostgreSQL); a moderator can't act on a listed administrator
  who hasn't signed in yet.

### Things asked for
- **An item earned is shown**: one that comes with an achievement (a level
  reached, say) only turned up in the shop. A dialog now says
  "Congratulations!", shows it and offers to use it at once (after the
  game, one after another when there are several).
- **The pockets are finished cleanly**: the facing of a jaw follows the
  rubber's cut-back under its nose (it stood out in front of the cushion
  like a fin), and the hole's wall and its leather lip stop with the
  casting (they ran on over the rubber, a centimetre proud of it).
- **From the overhead view to 3D the zoom stays**: dragging into 3D jumped
  back to the whole table.
- **The profile's note on the two ratings** takes the room the buttons
  leave: it ran under "Daily game".
- **The staff portal draws the countries' flags** from the game's own
  atlas (`/admin/flags.png`, `/admin/flags.css`): the flag emoji aren't in
  every system's fonts, and Windows showed two letters.

### One game at a time
- **A second start of the game doesn't open another one**: it brings the
  window of the game that is running to the front (Windows) and ends.
  Two games on one computer shared the settings, the saved game and the
  account's session, each writing over the other's. The game holds a lock
  on `instance.lock` in its settings folder while it runs (the system
  frees it however the game ends); a start right after an update waits a
  few seconds for the old game to close. Development builds can still run
  side by side with `SNOOK_LEGEND_MANY=1`.

### A fifth review
- A private arena asks for its password once it is running too (joining
  it was refused, each try counting as a wrong password).
- Signed out after changing the game server in the settings, the game
  signs in to the server chosen.
- Microsoft Store build: no purchase begins while the Store is still
  being asked what was bought (the click was swallowed).
- The shop's dialogs leave Enter and Esc to a dialog drawn over them (a
  present arriving bought the item underneath).
- The wheel at the zoom bar's far end no longer zooms in; a size dragged as
  the settings close isn't shown as chosen; a LAN address pasted with
  "http://" in front connects.

The server and what it shares with the game (the same review):
- **An account deleted while it signed in from another place** left that
  connection online under a name that was free again, out of the staff's
  reach: it is closed, with any session made meanwhile, and a session of
  a deleted account can't be resumed.
- **Watching a friend's games ends with the friendship** (only a block
  ended it), and a minor's status message and note on the board go when
  their chat is turned off.
- The address of a player behind a proxy is read from every line of the
  forwarding header (a proxy adding its own line left the client's word
  first); profiles asked for before signing in count per address; a
  trusted-proxy entry that can't be read is said at startup.
- **The tour's achievement gives its experience**, as every other does.
- A flag emoji can't carry a long run of invisible tag characters past
  the chat's filters; drill sessions are scored a couple at a time; a
  first drill with nothing made is no place on the challenge table.
- The shot analysis, the ghost's style and a shot's difficulty count a
  break handed over on scattered balls as the ordinary shot it now is.
- The staff portal's report counts work for a player called
  "constructor".

### Notes
- What's new in the game (three languages) and the Store listing's notes
  for 0.8.3: the item earned, the pockets, the zoom into 3D, one game at a
  time and the fixes. The Store notes keep 0.8.2's and drop 0.8.1's.
- Left as they are, for another version: a refunded VIP month always
  takes back 31 days; a subscription ended with its event delivered late
  can take time from the next one; a daily game begun under 0.8.1 whose
  break lost the cue ball may not rebuild to the table its players saw;
  the staff portal's actions go by name, not by account id.

### Deploying
- The server and the game go out together: once this server is up, a game
  of 0.8.2 is asked to update (protocol 24), so publish the release (and the
  Store package) when the server goes out. Render builds the service from
  `main`: `main` has to get this commit along with the tag.
- The staff portal serves two new files (`/admin/flags.png`,
  `/admin/flags.css`); nothing to configure.

## 0.8.2 (protocol 23)

The Brazilian break and shot clock, the break taken in turn, the cue
grabbed anywhere along it, a seven-round arena, and three reviews of the
whole game. Protocol 23, the oldest let in too: the rules a replay is
played back with changed, and a game of 0.8.1 would show other tables than
the server's, so it is asked to update. Two migrations (0023: indexes;
0024: the room a match was played in and the lounge's id of a tournament,
so each is recorded once), which the server runs as it starts.

### The Brazilian rules: the break
- **A break that loses the cue ball** (down or off the table) with no ball
  made, or with balls of both groups, passes the break to the opponent with
  the cue ball **on the head spot**, not in hand: the balls stay where they
  stopped (the ones made stay down), no ball is owed, the table stays open
  and the shot is a new break. With balls of one group only, the first one
  down still decides the groups and it is an ordinary foul: the opponent's
  lowest ball comes off and they play from the head spot. A break that hits
  no ball still hands over the cue ball in hand behind the head string.
- **The break alternates**: one break each. Offline the game remembers who
  broke last (`next_breaker` in the settings) for games against the
  computer, the ladder and two players at one computer; a rematch is broken
  by the other player; the rounds of a cup alternate; online, two players
  who meet again on the same day take turns (the rematch in a room already
  did), in tournaments too.
- The rules book in three languages says so, and the table explains a
  break passed over and a clock run out on the break.

### The game
- **The arena against the computer is seven rounds** for everyone, on
  points.
- **The shop's icon is a cart**: the bag looked like a padlock.
- **The cue can be grabbed anywhere along the stick**: only the first
  70 cm or so, about half of it, took a click, at a guessed height. The
  test now follows the cue as it is drawn (its real elevation, drawn back
  while charging) from the tip to the butt, a bit wider where the stick is
  thicker, and the cue swings from where it was grabbed without jumping
  in a low view.

### The Brazilian rule: the shot clock gives a ball
- **The shot clock running out is a foul that gives a ball**: the
  opponent's lowest ball comes off (with none left for them, the 8 pays and
  the game is lost, as with any foul), the cue ball goes back to the head
  spot, not in hand, and the turn passes. With the table still open
  nothing is owed; on the break, the break passes to the opponent with the
  cue ball on the head spot, not in hand, and no ball owed. Online, the server no
  longer plays the shot at full power where the player was aiming: the
  clock is a foul under both rules (the room's last aim, kept only for that,
  is gone).
- `EightBallGame::time_foul` returns the balls it gave, and the penalty of
  a foul and of the clock is paid in one place (`pay_penalty`). A clock
  that ends the game finishes the match on the server (result, ratings)
  and in the game (offline, in replays and online), and the table says
  which ball came off, or that the 8 paid.
- The rules book in three languages and the README say so.

### Fixes from a review of the whole game
The rules:
- The official rules: after an illegal break that also drove a ball off
  the table, the incoming player may take the cue ball in hand behind the
  head string as well (a new `ball_off` on `IllegalBreak`, which older
  games skip); the clock gives the cue ball in hand from a place it can be
  played from; on an open table a shot at the 8 no longer hands the shooter
  the other group. The rules book says so.
- A saved table with the 8 gone while the game goes on, or with groups that
  don't go together, is refused.
- A ball that only rattles in a pocket's jaws no longer counts as a bank in
  the drills and the shot assessment (`Event::Cushion` says `jaw`; the
  rules still count the jaws as cushions).

The game:
- A clock that ends the game counts the win; drills and the daily challenge
  can't be resigned into the ordinary end panel; seeking a stored game
  resets its score and history.
- The chat's dialog holds the keyboard; the pointer works on the table
  while the chat has the keyboard; the HUD's keys wait under the pause
  menu; the e-mail offer over the invitation dialog gets the pointer.
- A clock or a choice refreshes the saved game; a tournament game asks
  before replacing a saved one, and its computer plays in the cup's form
  from its first shot.
- The ping shown is the game's own round trip: the connection's keepalive
  answers no longer pass for it (they made it look shorter).
- The official choice after the break takes the keys 1 to 4.

Online:
- Under 18 without the chat, friends' status messages and what came before
  the age are hidden; another account signing in forgets the last one's
  e-mail, wallet, offers and warnings; invitations and the table code end
  with the session; the server set in Settings isn't overwritten at sign
  in; "NaN" in the settings file is ignored; the restart notice goes away.
- A LAN host can't open payment pages or rewrite the wallet, and a bracket
  with an impossible winner is refused.
- Passwords need 8 characters, not bytes, and digits of any script count
  as digits; more invisible characters are cleaned from texts; the shot
  analysis credits the ball the shot was about, not a kissed one that
  dropped first.

The server:
- **A player who lets the shot clock run out three times in a row leaves
  the game** (it ends as abandoned): a game nobody plays (the official
  rules never end on the clock) no longer runs forever, nor holds up a
  tournament.
- A player put back at a restored game loses their open table, place in
  the queue, invitations, note on the board and the game they were
  watching (someone could have sat down at the table).
- The rate limits (chat, invitations, lounge actions, reactions, the
  board) outlive the connection for ten minutes: signing in again no
  longer fills them up.
- Banned players are off the daily challenge's, the drills' and the
  challenge rating's tables while the ban lasts, as on the leaderboard; a
  player's place on the leaderboard follows its order on a tie (the wins,
  then the name).
- The staff's audit log follows a renamed account (by the player or the
  staff), and a deleted one goes under its deleted name: whoever takes the
  name next doesn't inherit its history.
- Moderators no longer get recovery e-mails from player actions, payment
  references and Stripe customers, nor sign-in addresses through the audit
  search.
- Stripe: a cancelled subscription takes back only the unused period, a
  refunded month (bought or given) only that month; VIP time is added and
  taken back in one step (two months at once add up); the account's first
  Stripe customer stays; a subscription started for an account deleted
  meanwhile is cancelled, and a month given to one is logged for a refund.
- Two friend requests sent to each other at once make friends.
- A database hiccup no longer deletes a saved game; a game finished here
  is never played again from its saved copy.
- Arena standings rank those who played above those who didn't; the
  metrics count official and casual games too; the career in the memory
  store skips games with no shots.

### A deeper review
The rules and the physics:
- **An official game whose break made the 8 no longer gets stuck**: the
  table waiting for the choice (the 8 to spot or a new rack) is read back
  when it is sent or saved (it was refused, and the game stopped there).
- **The ball hit first is the one the cue ball touched first**: two balls
  touched in the same sub-step were taken in the order of their numbers
  (the 8 against an 11 was "the 8 first" whatever the geometry). Contacts
  carry the moment they happened.
- **A ball lying against a cushion and pushed into it isn't driven to it**
  (the official rail after the contact, the Brazilian cushion before the
  ball): it has to leave the cushion and come back. The rules books say so.
- The computer no longer puts the cue ball in hand for a ball behind the
  head string it may not hit from there (official rules).
- The Brazilian book's sentence on fouls while on the 8 says what the rule
  does (the usual penalty; you lose only if it gives more than they have).

Payments (Stripe):
- A renewal adds its month to the VIP there is: a month given, bought in
  the Store or from the staff isn't swallowed by it. A cancellation after a
  renewal that was never paid takes back nothing unpaid for.
- VIP time and its coins are given or taken back in one step, once: a
  delivery that stops half way gives (or takes) both when it comes again.
- The customer a subscription is under is the account's (the one its page
  is managed and cancelled through); a second subscription isn't sold over
  one Stripe still tries to charge; only a VIP subscription's invoice gives
  VIP; a refund Stripe couldn't be asked about is tried again; a welcome
  pack paid twice keeps its items when one is refunded; a deleted account's
  subscription is cancelled, never another product's.

The server:
- **A result is stored once**, its game's saved copy deleted in the same
  step, and the write is tried for minutes (a database's trouble no longer
  loses it, nor does an answer lost on the way store it twice).
- Games two friends farm, or resigned before they were played, give no win,
  streak or feat (the rating keeps its own rules for them); the golden
  break is only for a win on a break.
- Deploys: a game a player left while a shot was played moves to the next
  server; a game that just ended isn't handed over; one finished while its
  copy was being rebuilt isn't played again.
- A finished room only talks to the players still at it (a rematch offer,
  a chat line or "left" no longer reach a game they went on to); a rematch
  is checked like any new game and offered once; a chat line is deleted
  once, by its writer's account; the limits of a connection (shop, queries,
  status) outlive a reconnection; renames act on the connection that asked.
- Tournaments: the players of a match nobody turned up for are free again;
  a player who withdrew is never crowned; arenas skip players who are away
  or waiting after leaving a game; a player with the chat off can't name a
  tournament, and a player's tournament isn't announced in the chat.
- Correspondence: a choice after the break is applied (the game ignored the
  answer); a game that can't be rebuilt any more can still end; a deleted
  account's games end at once; a challenge from a player banned since is
  no longer good; the daily challenge refuses a result with no attempt
  under way before working it out, and its coins are tried again.
- The staff: renames logged under the new name; payment ids hidden from
  moderators; moderators can't remove other staff's notes; only items owning
  makes usable can be given; a permanent ban is logged as such; indexes for
  the audit log, the ledger, payments and new accounts.
- The memory store answers as the database does (staff names, ranks of a
  banned player, the order of names on a tie, renames, spending).

The game:
- **Signing in as another account forgets everything of the last one** (its
  e-mail, presents, warnings, friends, chat line, purchases): the check was
  made against the name typed in the form.
- A sign-in password written under the older rules (shorter in letters)
  signs in again.
- Clicks on the bars of the table (between the buttons, on the names, on the
  rules' badge) no longer aim the cue.
- A stored game: Previous goes to the shot before (it got stuck on a clock
  run out or a choice); its end offers to watch it again, not a rematch,
  and a resignation or a game left counts in its score.
- The shop: a purchase whose answer was lost no longer locks it; signed out,
  it says to sign in; the welcome pack's items only where it is sold.
- A game of the official server that starts while one on the local network
  has the table (or the other way round) is resigned, not lost unseen; a
  LAN host can't sign the player out of their account.
- Invitations can be accepted while watching a game or at a daily game's
  table; the present dialog doesn't take a key meant for something else; a
  refused break choice brings its buttons back.
- A larger interface stops where the screens fit the window; the profile's
  cards only where they fit; the camera survives a window with no size.
- The power bar starts each turn where it should (also after a clock or a
  choice); a run of balls ends with a turn without a shot; no clock on the
  free table; "Reset to defaults" doesn't show the first-game pointers
  again; the two-player setup keeps its names on Back; a daily attempt or
  a replay started during an online game waits for it.
- Weekly cups' names translated on the trophy shelf; translations for the
  texts that had none.

### A third review
The rules:
- **The Brazilian rules: an open table with only the 8 left** (every group
  ball down on fouls or breaks handed over): both players are on the 8, and
  a foul that gives a ball, or the clock, loses (it cost nothing). The book
  says so.
- The official rules: a ball made on an open table that gave no group (off
  its call, on a safety) isn't counted as the shooter's (numbers, XP,
  missions).
- A saved or sent table with a speed or a spin that isn't a number is
  refused (it turned the balls it met into nowhere).

The game:
- **Leaving while the balls roll keeps the shot**: the game against the
  computer is saved as the shot will leave it (it kept the table before
  it, and a missed shot, even in the tour's final, could be played again).
- **Renaming the account no longer forgets the account's things** (wallet,
  VIP, achievements, presents) until the next sign-in.
- **Accepting an invitation of one network while watching a game of the
  other no longer resigns the new game at once** (only declining is offered).
- A stroke of a daily game the server didn't take (asked too fast, a
  moment's trouble) no longer stays on the table as played: the game opens
  again as the server has it; a daily game that missed strokes while the
  connection was down asks for them; a late answer doesn't open its table
  over another screen; its table uses the players' own cloth.
- After a reconnection: a friend's game being watched is watched again (it
  froze); the tour final's deal and a ladder claim asked on the lost
  session are asked again; what was said at the table meanwhile still goes.
- Watching a game keeps the player's invitation and search; a live game
  against a daily-game opponent starts its own score.
- The daily challenge's "Setting up the table…" gives the button back when
  no answer comes; a replay that arrives after the player started a game of
  their own doesn't replace it.
- Signing in as another account forgets what was typed for the last one
  (a table's password among them) and checks its own Store purchases.
- A LAN host can't send coins, XP, achievements, presents, missions,
  medals, events or the daily challenge (only the official server does).
- Player-written tournament names are hidden from players with the chat
  off.
- Two players at one computer: a stroke being charged doesn't carry over
  to the next player after a clock runs out.
- A run of balls ends with a clock that runs out (the end panel, and the
  server's "runs" missions).
- Stored games: jumping to the end shows its score; the last shot watched
  again plays while the game is paused; one that can't be gone through
  ends with a way out.
- Esc in the list of controls goes back to the menu; the shot clock's hint
  says what it does under each rules; a new game replacing a saved
  tournament game says the tournament game will count as lost; taking a
  shot back on the free table takes its counts back too.
- The payment page's notices have their colours right.

The server:
- **Stripe: a subscription ended and a refund of its month take that month
  back once between them** (and each only once, however often delivered);
  two VIP pages paid at once end the second subscription (logged for a
  refund); a delivery cut short before a refund still cancels out; money
  paid for a deleted account is logged for a refund; a lookup refused for
  too many requests is asked again (only "no such thing" is an answer); the
  first month counts from the payment.
- A match stored by a write whose answer was lost still pays its rewards;
  a tournament is recorded once (migration 0024); the stored numbers of a
  player don't overwrite newer ones still on their way.
- Bans: a banned player is out of their tournaments (no standing, prize or
  crown) and, for longer than a turn lasts, out of their daily games.
- A finished game's chat and reactions only reach players still at it; a
  friend watching a player who dropped hears how the game ended.
- Deploys: a game handed over is written as it is before its players are
  moved (the next server played an older copy); the results still being
  written are waited for; invitations and board notes end with notice.
- Renames and deletions end the player's invitations first (one accepted
  meanwhile started a game under the old name); a device left signed in
  under the old name is signed out.
- Tournaments: a player who withdrew while waiting is free at once; players
  rejoining an arena (or starting a tournament) leave their table and board
  note; arenas never pair a player with one who blocked them; a player's
  tournament nobody joins closes after an hour (fees given back).
- Daily games: a database hiccup no longer deletes a challenge; one game
  that can't be read no longer stops the others; a forfeit is told as one;
  the golden break needs a game that counts. A refused daily-challenge
  attempt no longer puts the player on the day's table.
- Moderators can't find payment ids by searching; a deleted account's
  Microsoft Store keys go; old activity days and expired board notes are
  cleared; names sort the same in both stores.
- Drills: the "cue ball on the cushion" drill has it against the cushion,
  and "the plant" balls touching; a try that knocks the 8 in doesn't count.
- The ghost's style counts ordinary strokes at their own top speed.

### Notes
- What's new in the game (three languages) and the Store listing's notes
  for 0.8.2: the shot clock and the break under the Brazilian rule, the cue
  grabbed anywhere, the seven-round arena and the fixes. The Store notes
  keep 0.8.1's and drop 0.8.0's.

### Deploying
- The server and the game go out together: once this server is up, a game
  of 0.8.1 is asked to update (protocol 23), so publish the release (and the
  Store package) when the server goes out. Render builds the service from
  `main`: `main` has to get this commit along with the tag.
- The two migrations run on their own when the server starts.

## 0.8.1 (protocol 22)

The Brazilian rule's penalty paid with the 8, windows that close with a
click outside them, and a month of VIP sold in the Microsoft Store version.
Still protocol 22 (two new fields, which games of 0.8.0 skip) and no new
migration: games of 0.8.0 keep playing with this server, which can go out
first (a game of 0.8.0 still shows a foul the 8 paid as going on until the
server's table comes in), and only this game sells the month of VIP.

### The Brazilian rule: the 8 pays what the opponent doesn't have
- **A foul that gives more balls than the opponent has left loses the
  game**: their balls come off and the 8 pays the rest. With only the 8 left
  for them (and so with only the 8 on the table), any foul that gives a
  ball loses: a miss, the cue ball down or off the table, the wrong ball
  first. Two balls owed with one left: that one and the 8 come off. On the
  8 with the opponent still holding balls, a foul takes their lowest off and
  passes the turn, as before. (Until now the penalty stopped at the last
  ball and the 8 never came off.)
- The game ends as a foul on the 8 (`EightPocketedOnFoul`, "fouled on the
  8"), the report's `penalty` ends with the 8, and the table says why: the
  opponent had fewer balls left than the foul gives. The computer, which
  plays to win, now steers clear of such fouls by itself.
- The rules book says so in the three languages. The rule is not versioned:
  a game stored before in which this happened no longer replays past it.

### Windows close with a click outside them
- **Panels at the table** close when the button is pressed anywhere else:
  the match's chat (what was typed stays), the game history, and the spin
  and elevation panels (the setting stays). The press goes on to what is
  under it, so a click on the table closes the chat and aims; a list the
  chat opened (quick phrases, reactions) closes first, on its own.
- **Dialogs over a dimmed screen** close when the dimmed part is pressed,
  as with their Cancel, Close or Esc, and that press does nothing else: the
  pause menu (back to the game), the controls, the rules book and what's
  new, the invitation dialog, Settings, the shop's confirmation, the friend
  to give a present to and "Great!", the country and picture pickers, a
  present received, "a game against the computer is saved", and the chat's
  dialog ("Not now"). The report, the account and the recovery e-mail close
  that way only while nothing has been typed. The first choice of rules, a
  warning from the staff and the wait for the server still need their own
  answer.
- `Ui::pressed_outside` (panels: the press goes on) and `Ui::dismissed`
  (dialogs: the press is spent) do it everywhere; the quick phrases use it
  too, so a faded or covered HUD no longer closes them.
- The chat's dialog now holds the pointer like the other dialogs: a click on
  it no longer reaches the lounge or the game under it.

### A month of VIP in the Microsoft Store
- **The Store version sells VIP a month at a time**: a fifth add-on,
  `vip_month`, a developer-managed consumable like the coin packs, bought and
  confirmed the same way (the server asks Microsoft, consumes the purchase
  and credits it once). Each purchase gives 31 days of VIP and the month's
  400 coins; nothing renews by itself. Bought while VIP, the month starts when
  the current one ends.
- **The VIP tab** of the Store version shows "A month of VIP · {price}" (the
  Store's price, in the player's currency), and "Add a month · {price}" under
  the end date while VIP ("The new month starts when this one ends."); the
  perk about cancelling becomes "A month at a time: nothing renews by
  itself". Without the add-on on the server it still
  says VIP isn't sold in this version. The notices while Microsoft confirms
  talk about the VIP month, and the one when it arrives says until when.
- `SNOOK_LEGEND_MSSTORE_PRODUCTS` takes `vip_month=<Store ID>`; the start-up
  log says `Microsoft Store purchases on (4 packs, VIP month)`.
- `ShopInfo.store_vip` (the add-on's Store ID, empty when not on sale) and
  `StoreRedeemed.vip_until` (when VIP ends, when a month was credited). The
  month is never among `store_packs`, so a game of 0.8.0 never shows it as a
  pack of coins.
- **The staff portal**: the purchase shows as `vip_month` among a player's
  Microsoft Store purchases and as VIP among the payments; the Store's count
  in Economy reads "purchases in 30 days".
- `docs/MSSTORE.md`: the fifth add-on, its texts in three languages, and the
  titles and descriptions of every add-on; `docs/STORE-LISTING.md`: the VIP
  month in the "in-game purchases" paragraph.

### Notes
- What's new in the game (three languages) and the Store listing's notes
  for 0.8.1: the 8 paying the penalty and closing with a click outside. The
  month of VIP is left out of both until the server sells it.

### Deploying
- Put `vip_month` in `SNOOK_LEGEND_MSSTORE_PRODUCTS` only once this server is
  out: a server of 0.8.0 doesn't know it, refuses the whole list and turns
  the Store's coin packs off too.
- `render.yaml` names the service `snook-legend-server`, its name on Render
  (the address stays `snook-server.onrender.com`).

## 0.8.0 (protocol 22)

The official rules of the World Pool-Billiard Association next to the
Brazilian rule, chosen by each player, with a rating and a ranking for each,
everywhere in the game, online and in the staff portal. Protocol 22 is the
only one let in, so the server and the game go out together (a game of
0.7.x is told to update). Migration 0022 adds the official ratings and the
rules of every stored game; everything stored so far was played under the
Brazilian rule. No code is kept for older games any more.

### Two rules
- **The rules are a choice, not a version**: `Rules::Brazilian` and
  `Rules::Official`. The Brazilian rule is the former ruleset 7, bit for
  bit (the physics, rules and computer fingerprints are unchanged); the
  older rulesets, tables and physics, kept only to replay old games, are
  gone. A game, a replay and a game in progress record their rules.
- **The official rules** follow the WPA Rules of Play (2025-09-15): every
  shot but the break is called (a pocket; none is a safety, and whatever
  goes in on it stays down), a legal break makes a ball or drives four
  balls to a cushion, the break never decides the groups, the choices a
  break can leave (an illegal break, the 8 on the break, the 8 on a foul
  break, a ball off the table on the break: play the table as it lies,
  rack and break again, the other player breaks, the cue ball in hand
  behind the head string), the 8 made on the break spotted, ball in hand
  anywhere after any foul and no penalty balls, the cue ball in hand behind
  the head string crossing it before hitting a ball behind it (the ball
  nearest the head string spotted when every ball that may be hit lies
  behind it), object balls off the table staying off (the 8 comes back),
  and the 8 lost on a foul, too soon, in a pocket not called or off the
  table. Under them the shot clock running out is a foul.
- **The computer plays both**: under the official rules it calls the
  pocket of the shot it chose (or plays a safety) and makes the break's
  choices. A free game, the tour and the tournaments against the computer
  are played under the player's rules (the daily challenge and the drills
  set their own tables).
- Tests for every official rule, random official games checked after every
  shot, and the Brazilian rule's tests unchanged.

### The Brazilian rule: the break with balls of both groups
- On the break, the cue ball down (or off the table) together with balls of
  both groups, a solid and a stripe in either order, now hands the break to
  the opponent, as when it goes down alone: no penalty ball, the groups
  still open, the balls made stay down, the cue ball in hand behind the
  head string and the opponent's shot is a break. With one ball, or balls of
  one group only, it stays an ordinary foul: the first ball decides the
  groups, the opponent's lowest ball is owed and the cue ball is played from
  the head spot, without ball in hand. The rules book says so in the three
  languages.

### Online
- `hello { version, rules }` and `set_rules`: the rules a player plays, shown
  in the lounge (and who is looking for a game). Invitations, tables, "Play
  now" (which pairs only players of the same rules), tournaments, the weekly
  cup (`SNOOK_LEGEND_CUP` takes `official`), the staff's themed tournaments,
  invitation codes, notes on the board, daily games and replays all carry
  their rules.
- **A rating, peak and record under each rules** (`PlayerInfo.standings`):
  Elo, the queue's rating window, tournament seeding and the lounge's TV use
  those of the game's rules; the ranking, ranks, rating charts and history
  are kept per rules. Level, coins, achievements and missions are shared.
- `choose { turn, option }` / `chose` (and `corr_choose` / `corr_chose` in
  daily games): the choice an official break leaves, made by the player to
  move; the shot clock takes its first option.
- `/api/status` counts the players online under each rules;
  `/api/leaderboard?rules=official` gives the official ranking (the
  Brazilian one when left out).
- The version gates for older games (social, activity, analysis, age,
  correspondence, the tour) and the gift toast for older games are gone.

### The game
- **First start of 0.8.0: "How do you like to play?"**, a card for each rules
  with what changes and how many play them now. The choice is kept in the
  settings and switched from the main menu (a switch under the title, with
  the players online under each), the lounge's header, the screen against
  the computer, the two-players setup and Settings → Game.
- The main menu's "Play online" and "Against the computer" count the
  players of the player's rules; the player card shows the rating under
  them.
- **The lounge**: a rules pill in the header (a click switches), "Play now"
  with the players waiting under these rules, players and notes of the other
  rules marked, an invitation dialog (rated or casual, and the rules), and
  the rules on invitations, tables, tournaments, daily games and the TV.
- **At the table**: the rules' badge in the top bar (it opens them), notices
  that say what a foul gives under the game's rules, the official break's
  choice as buttons (or 1, 2, 3), the computer's own choice told, and the
  official calls: the called pocket follows the aim (a ring on it) until
  another is clicked, the top bar says which is called, and **Safety** calls
  nothing.
- **Ranking** with a tab for each rules; the **profile** with a rating card
  for each (the one lit sets the rank, the league and the chart), and the
  rules of every game in the history.
- **The rules book** has a tab for each: the Brazilian rule and the official
  (WPA) rules, in English, Portuguese and Spanish.
- Every new text in Portuguese and Spanish; a rules' name inside a
  sentence is in lower case except for the capital of "Brazilian" in
  English.
- The table no longer changes from one game to another (every game is
  played on the same table now): the code that swapped it is gone.

### The staff portal
- **A player's rating under each rules everywhere**: the list of players (a
  column for each, with the tier and the games, sortable by either), the
  search, the profile (a card for each with the peak and the record, and
  which they play now) and the players online.
- **The rules of every game**: a player's games (filtered by rules), live
  games, tournaments, reports and the replay viewer; a themed tournament is
  scheduled under either (and says so). The viewer understands the official
  calls, safeties and choices after the break, and the Brazilian rule's
  penalty balls.
- The dashboard, Live, Metrics and Server pages split the players online,
  the games of the last 24 hours, the games now and the queue by rules; the
  Server page shows the protocol and the server's settings (no longer
  called its "rules"), and the weekly cup's rules.
- **A role badge no longer touches the name** in the list of players (the
  name and its badges keep a gap and wrap when narrow); players online and
  the search do the same. Rating changes in a player's games are green or
  red.

## 0.7.4 (protocol 21)

The chat with other players is asked for when it is used, invitations as a
card in the top right corner on every screen, and a player's record against
the computer in the staff portal. No new migration, no new messages: the
protocol stays 21, and games of 0.7.3 and 0.7.4 play together with this
server (a game of 0.7.2 still plays, with its chat off). The server can go
out first.

### The chat with other players, asked for when it is used
- **Signing up no longer asks the age group, nor does the lounge.** The chat
  with other players (the lounge's and the games' chat, notes on the board,
  status messages, gift messages) starts off for everyone, both ways, and is
  turned on in context: writing with it off (or the "Turn the chat on"
  button in the empty lounge chat) opens a dialog, over any screen, that asks
  the age group once ("18 or older", "Under 18", "Not now"; it can't be
  changed from the game afterwards). 18 or older turns it on at once and what
  was written goes out; under 18, the dialog goes on to the parent's or
  guardian's declaration, as before. Whoever never writes is never asked.
- "Not now" keeps the chat off: a line comes back to the lounge's field, a
  gift goes without its message, a note without its text, a status message
  is dropped. The lounge's chat shows "The chat with other players is off
  until you turn it on." and the field "Write to turn the chat on…".
- **The server turns the rule around** (`User::chat_restricted`): the chat is
  off until the account says 18 or older, or a parent or guardian turned it
  on (it was on unless the account said under 18). Turned on or off, the
  lounge's chat comes again with or without what players wrote. Accounts
  that answered with 0.7.3 keep their answer.
- A game of 0.7.2 (protocol 20, which can't ask) has its chat off and is told
  "Update the game to chat with other players." when it writes.
- **A LAN server's guests chat as before**: they are the players of a local
  network, with no account to keep an answer.
- Under 18 with the chat on, a "For a parent or guardian…" button in the
  lounge's chat lets the parent or guardian turn it off again (it could not
  be reached before).
- The terms of use (section 4 and the summary), the privacy policy, the
  README and the Store notes say so, in the three languages.

### Invitations, like the system's notifications
- **One card in the top right corner, on every screen**: the main menu, the
  lounge (under its side's tabs), the shop, the profile, the ranking… and
  over any game (under the online count in a game against the computer,
  which accepting keeps; at an online table it can only be declined). It
  used to show in a different place on each screen and on none of the
  others, and not while the game's menu was open.
- The card slides in from the right with the player's picture, flag and
  name, "invites you to a casual game" (in two lines if it needs them) and
  Decline / Accept along the bottom; up to three, then "{n} more
  invitations". A new invitation rings once (the turn's chime). Nothing under
  the cards takes the click. Focus mode and the settings keep them away, as
  before.
- **Windows notifications at once when the game is not in front**: the game
  asks the system whether its window is the foreground one and not
  minimised; behind another window or minimised, the notification (and the
  flashing taskbar button) comes at once. It used to wait until nobody had
  touched the game for 12 seconds, which is still how other systems tell.

### The staff portal: a player's record against the computer
- **A new tab on a player's page, "Against the computer"**: whether they won
  the tour (when, with its cue and achievement) and how many finals the
  server dealt them (of the latest 16 kept, and when the last was); the
  medals of the tournaments against the computer, knockout and arena apart,
  gold, silver and bronze, and the latest thirty with their date; the
  drills' stars (of 72) and best score in each, by name in the portal's
  language; the daily challenges they topped. The stages of the tour before
  its final are kept by the game, not the server, so they don't show.
- New store queries for it: `cup_medal_list` and `ladder_finals` (both
  stores; no migration).

### Tests
- Tests that switch the game's language hold a lock while they do
  (`i18n::lang_lock`): run side by side, they changed each other's language
  and the translation tests failed now and then.

## 0.7.3 (protocol 21)

Ruleset 7 (the break handed over, the cue ball off the table costs a ball), terms of use, an age group and the chat
of players under 18, more drills without a ranking, and new profile
pictures. The server lets in protocol 20 and 21: games of 0.7.2 keep
playing (they are not asked the age group, and play ruleset 6). The server
goes out first. One new migration (`0021_age`).

### The rules (ruleset 7)
- **A break that hits no ball, or that loses the cue ball (down or off the
  table) with no ball made, hands the break to the opponent**: the balls
  stay where they are (still racked after a break that hit nothing), the
  cue ball is in hand behind the head string, and their shot is a break
  (any ball may be hit first, the 8 alone wins, it may be struck as hard).
  A break that makes a ball along with the cue ball is an ordinary foul, as
  before.
- **The first ball made on the break decides the groups even when the
  cue ball goes down with it**: the 11 and the cue ball, stripes for the
  breaker; the turn passes, the cue ball goes back to the head spot and the
  scratch costs the opponent's lowest solid, as any scratch does (the table
  stayed open before).
- **The cue ball knocked off the table gives the opponent a ball**, as a
  scratch does (it only cost the turn before): their lowest comes off, the
  turn passes and the cue ball goes back to the head spot. With a miss, two. The rules in the game say so, in the three languages; the game
  tells the players ("No ball was hit on the break: … breaks").
- **The computer** breaks a full rack as before and plays a break handed
  over on scattered balls like any shot, the cue ball behind the head
  string.
- **Games between 0.7.3 and 0.7.2** play ruleset 6 (`ruleset_for`, the
  newest rules both games know); daily games stay on ruleset 6 while 0.7.2
  is let in (each player's game replays them); the tour's final is
  accepted under ruleset 6 or 7. Games keep the ruleset they began with.

### Terms of use and privacy
- **The server publishes the terms of use** (`/termos`, `/terms`,
  `/terminos`) and the **privacy policy** (`/privacidade`, `/privacy`,
  `/privacidad`, Spanish new), in the game's colours, each linking the
  others. The terms cover the provider (Decreto 7.962/2013), accounts,
  minors, the community rules and sanctions, coins and items, paid
  purchases with 7 days to change one's mind (CDC art. 49), VIP and its
  cancellation, availability, the code's licence and credits, liability,
  the law and the forum. The privacy policy now says everything the server
  keeps (age group, the guardian's permission, presence, results of drills
  and daily challenges, medals, ghost styles…), the legal bases, the rights
  (LGPD art. 18) and how long data is kept.
- **Who provides the game** comes from the server's configuration:
  `SNOOK_LEGEND_PRIVACY_CONTROLLER` (name), `SNOOK_LEGEND_LEGAL_DOCUMENT`
  (CPF or CNPJ), `SNOOK_LEGEND_LEGAL_ADDRESS` and
  `SNOOK_LEGEND_PRIVACY_CONTACT`; a line whose value isn't set is left out,
  and with payments on the server warns about the missing ones.
- **The game links to them**: under the button that creates an account
  ("creating an account means accepting…"), in the shop's Coins and VIP tabs
  (with the 7 days to change one's mind and the renewal of VIP), and in
  Settings → Game → About, with the credits (DejaVu, flag-icons, Twemoji,
  iso-codes).

### Age group and chat (Lei 15.211/2025, the ECA Digital)
- **Signing up asks the age group**: 18 or older, or under 18. Accounts
  made before are asked once, in the lounge. It can't be changed from the
  game afterwards.
- **Under 18, the chat is off both ways until a parent or guardian turns it
  on**: the lounge's and the games' chat, notes on the "looking for a game"
  board, status messages and gift messages. The server refuses what such an
  account writes and passes on nothing others write; the game shows "Chat
  off: under 18" and a button for the guardian, who turns it on (with a
  declaration) or off again below the lounge's chat. Reactions, invitations,
  friends and games are not affected. The server keeps the date of the
  permission; deleting the account deletes both.
- New messages (`SetAge`, `AllowChat`, `Age`; `Register` carries the age
  group) for games of protocol 21 (`AGE_PROTOCOL`); the server sends `Age`
  to those only.

### Drills
- **24 drills, nine new**: the far corner (a cut back into a head corner),
  long into the side, cue ball on the cushion, the plant, the carom, the
  long draw, on the tangent line, two-cushion bank (a bank now asks for a
  number of cushions) and fifteen in a row. A test finds a way to do each.
- **No more ranking on the drills tab**: completing everything only tied
  first place. The tab shows the stars earned (out of 72) with a bar, the
  drills in two columns and, where the tables were, the drill's position
  drawn from above (the pocket to play in gold, the circle for the cue
  ball in green). The stars are kept in the account as before.

### Emoji, tabs and the online count
- **Emoji show as pictures** wherever the game draws text (the chat, the
  match chat, statuses, notes, names of tables and tournaments, text
  fields): 1878 Twemoji in an atlas made by `tools/gen_emoji.py`, flags,
  keycaps and joined sequences included (skin tones drawn as the base
  emoji), measured, wrapped, shortened and edited as whole characters.
  Text without emoji is drawn as before. The server keeps the joiners and
  selectors that belong to an emoji and still removes them elsewhere.
- **On Windows, emoji typed from the emoji panel reach the game**: a
  character beyond U+FFFF comes in two halves, which miniquad dropped
  (patched in `vendor/miniquad`).
- **Tabs whose labels all have one size**: each tab as wide as its label,
  the labels shrinking together only when the row is too narrow (the
  computer screen's tabs, the lounge's, the profile's switch).
- **The online count** is a green dot, a person and the number; hovering
  says "Users online" and who.

### Profile pictures
- **Drawn again**: the 32 portraits are illustrated head-and-shoulders
  busts (shaded faces, eyes with highlights, hair in strands, beards,
  glasses, earrings and clothes of many kinds), each keeping who it was
  (skin, hair, glasses, beard, colours); the 12 objects too, in the same
  style. **20 new pictures** of pool things (a medal, a laurel, the 1 and
  the 15, a flaming 8, a ball dropping into a pocket, the lamp over the
  table, chalk on a tip, a crown over the 8, a rocket, a comet…): 64 in
  all, shown in the picker as people, then things.
- They come from an atlas drawn by `tools/gen_avatars.py` (SVG rendered by
  Chromium) instead of shapes drawn by the game; small sizes stay smooth.
- The player's picture shows on the card at the top right of the start
  screen when an account is signed in.

## 0.7.2 (protocol 20)

A harder tour, medals for the tournaments against the computer and a
showroom camera like the table's. 0.7.2 plays only with 0.7.2: the server
lets in protocol 20 only (older games are told to update), since it checks
the tour's final against the new last opponent. One new migration
(`0020_cup_medals`).

### The tour
- **Harder at every stage, and the final very hard.** An opponent's
  strokes were played with 60% of their usual error on every stage; now
  with about half of it on the first and a quarter on the final. From the
  fifth stage every opponent looks at every shot (the widest search;
  before, those who usually see few shots never did), from the sixth they
  see bank shots (it was the seventh), and position play counts more the
  further up the tour, the most on the final.
- **Viktor, in the final, also values a safety more** and plays one
  sooner when a pot is in doubt.
- Measured over 40 games, a very good computer player (steadier than Hard)
  wins about one final in five (8 of 40), against about one in three before
  (14 of 40). A Viktor with no error at all wins no more often: what makes
  him hard is his choice of shots and position.
- **Tournaments against the computer keep the strength they had** (the
  tour's former form, now `ladder::cup_form`).
- The server checks a final against the new Viktor, hence protocol 20; the
  final's fixture is recorded again (`record_the_fixture`), and an ignored
  test measures the final (`measure_the_final`, `SNOOK_LEGEND_FINAL_GAMES`).

### Medals of the tournaments against the computer
- **A tournament played to its end gives a medal** to the first three: gold
  for the champion, silver for second, bronze for third (in a knockout both
  semi-final losers take the bronze). The finished tournament shows it, in
  its colour, and the game says it ("A gold medal!").
- **The Tournament tab counts them**, knockout and arena apart, gold, silver
  and bronze: under the sign-up and under a finished tournament. They are
  shown there only, not on the profile.
- **They are kept in the account.** A medal is kept in the game at once,
  for the account signed in (or whose session is saved; with none, the next
  one to sign in takes it), and sent to the server the next time it is
  online; the server's count comes back and shows offline too. Until then
  the tab says "Kept in your account the next time you're online." (or
  "Sign in to keep them in your account.").
- The server keeps one medal per tournament (its draw's number), refuses a
  place outside the first three or an unknown format, and takes at most 40
  a day per account. Deleting an account deletes its medals.

### The showroom's camera
- **It turns like the camera at the table:** drag with the right button
  (the left one turns too, and a click on a ball still picks it), smoothly,
  with the wheel to come closer. Nothing moves the whole view sideways any
  more: the camera always looks at the item.
- **It never goes under the floor or the table:** it stops just above the
  stage, and above the table's top for a table, a cloth or an item shown on
  the player's table. The arrow keys turn it the same way.

## 0.7.1 (protocol 19)

New games play ruleset 6: two house rules on the break and the cue ball.
0.7.1 plays only with 0.7.1: the server lets in protocol 19 only (older
games are told to update), so the server and the game go out together. No
new migrations.

### The rules (ruleset 6)
- **The first ball made decides the groups, on the break too.** The 3 and
  then the 13 on the break: the breaker takes the solids, and the 13, the
  opponent's, stays down for them and passes the turn (as on any shot).
  Ruleset 5 left the table open after a break with both kinds.
- **The cue ball is in hand on the break only**, behind the head string.
  One that goes down or leaves the table, on the break or later, goes back
  to the head spot (right behind a ball lying there, towards the head rail)
  and is played from there, with the foul's penalty balls as before; the
  opponent no longer places it anywhere.
- **A break that makes only the cue ball is no longer racked again:** the
  balls stay where they went, and the opponent plays on from the head spot.
- The game says when the cue ball comes back to the head spot. The rules in
  the game say all of it, in the three languages; the computer's remarks no
  longer thank anyone for a ball in hand.
- Games keep the ruleset they began with (replays and daily games started
  before play on as they were). The fingerprints of ruleset 6 are pinned.

### Playing the computer, with the lounge in sight
- **How many are online** shows in a discreet corner under the top bar
  during any game played here (against the computer, its tournaments, the
  tour, drills), with who is there on hover; for a few seconds after
  someone comes online it says who, in gold. The invitations show under
  it, over the game, as before.
- **Focus mode** (Settings → Game, off by default): no invitations while a
  game is played. The game tells the server, which withdraws the
  invitations waiting and turns new ones away with the reason; the lounge
  shows the player "in focus mode" and offers no invitation button.
  Invitations' desktop notifications keep quiet too.

### The shop's showroom
- **"Look at it closely" opens a showroom** instead of turning round the
  table: the item alone on a dark stage under the table's lamps, with soft
  shadows on a floor that melts into the dark. A cue floats over its
  shadow; a cube of chalk (modelled, its paper printed with the game's
  name, its top worn hollow) stands with the player's cue chalking it; a
  ball set is racked with the cue ball beside it; a table or a cloth stands
  on its own with the player's balls.
- **The camera turns round it by itself until it is taken:** drag to turn,
  the right button (or Shift) to move, the wheel to come closer to what is
  under the pointer, a double click to look closely at a point; a play and
  pause button, zoom buttons, a way back to the start (R), the keyboard
  (arrows, + and −, Space, 1–4), and a clean view with the interface hidden
  (H). Ready-made views per item: a cue's tip, joint and butt; the chalk's
  top and the tip it chalked; a set from above or low down; a table's
  corner, side pocket and leg; the cloth close up and its cushion.
- **A ball of a set** (clicked, or picked from a row of the sixteen) rises
  over the rack and turns on itself, its number upright.
- **Settings:** a cue, a chalk or a set also on the player's own table; a
  table or a cloth in the studio or in any of the eight halls (the hall is
  built while the view stays, with a spinner).
- The price and the button to buy or use the item stay at hand; items for
  VIP members or earned with an achievement say so. Each item has a line
  on what it is (a table's wood and castings, a hall's room, a set's
  finish).
- **Ball sets show their finish** on every table: pearl sets a soft pearly
  sheen, metal ones a mirror-like gloss, the old ivory ones a soft matte.

### The balls
- **The 7 and the 15 are brown**, not a darker red: under the lamps they
  read as the 3 and the 11. The same in the shop's sets where the two were
  close (TV set, Vintage, Earth tones, Pastel, Forest, Candy, Chrome,
  Royal; Forest's 3 is a brick red and its 4 a slate grey, to keep its
  browns apart). The sets made of shades of one colour stay as they were; a
  test keeps the 3 and the 7 apart in the others.

### The staff portal
- **An administrator answers reports about themselves** (with one
  administrator nobody could, and they stayed open for ever); the answer
  goes in the audit log like any other, with no sanctions on oneself. A
  moderator's reports about themselves stay an administrator's: the
  portal says so on the report instead of offering answers that fail, in
  the three languages.

## 0.7.0 (protocol 18)

New games play ruleset 5, with a new house rule on the break. 0.7.0 plays
only with 0.7.0: the server lets in protocol 18 only (older games are told
to update), so the server and the game go out together. The server runs
migrations 0016 (indexes for profiles), 0017 (the tour's finals it deals),
0018 (the staff's notes) and 0019 (events) when it starts.

### Events
- **Scheduled from the staff portal:** experience ×2 or ×3, coins from
  games ×2 or ×3 (the day's cap too), and themed tournaments — a knockout
  or an arena organised by the server like the weekly cup, with a prize the
  house adds on top of the entry fees (70% to the champion, 30% to the
  runner-up; with four players or more, to players who won at the table).
  Each with a start and an end; called off at any time.
- **Players see them** on the main menu and in the lounge (what each does,
  when it starts or ends), and experience and coin notices say "(×2)"
  while a boost runs. A tournament's prizes show also without an entry fee.
- Migration 0019 keeps the events.

### The staff portal, made again
- **A new portal at `/admin`**, in Portuguese, English and Spanish (from
  the browser, or chosen), dark or light: a sidebar of sections with the
  open reports counted, a search for players (and `#game` for a replay) on
  every page, an address for every page and filter, keyboard shortcuts,
  dialogs that ask before anything destructive and say what the player
  will see, and a layout for phones. See docs/ADMIN.md.
- **Players:** filtered (online, staff, banned, muted, deleted) and sorted;
  each player's page in tabs (overview, moderation, games, economy,
  achievements) with every staff action on the account in a timeline.
- **New actions:** warnings the player sees at once or at their next
  sign-in; notes among the staff; a rename by the staff that doesn't use
  the player's own changes; items taken back; chat lines deleted (or
  removed without a trace) from the portal; tournaments called off before
  they start (fees back). Reports are answered with a warning, a mute or a
  ban from the report itself.
- **New pages:** the lounge chat as the staff see it, the economy (coins
  given and spent by reason, the largest balances, everyone's latest
  payments, the shop's catalogue), the server (version, memory, health,
  rules, the weekly cup, the staff); metrics show where players come from.
  The audit log is searched, filtered, paged and exported as CSV.
- Migration 0018 keeps the notes and warnings (and indexes the players'
  list by rating and by last seen).

### The rules (ruleset 5)
- **On the break only one kind may go down.** A break that makes balls of
  both groups (the 3 and the 14, or the 5, the 13 and the 14) passes the
  turn and leaves the table open; one kind only sets the groups and the
  breaker shoots again, as before. The game says so when it happens.
- What ruleset 4 got wrong in rare cases: an open table with no group ball
  left puts both players on the 8; a time foul after a scratch on the break
  leaves the cue ball in hand as it was; the cue ball in hand may touch a
  cushion but never sit in a pocket's mouth; a ball that jumps onto the
  rail beside a pocket is off the table. The computer is stronger.
- Games keep the ruleset they began with (replays and daily games started
  before play on as they were). The rules in the game say all of it, in the
  three languages.

### Rewards that can't be made at home
- The tour's final is played from a rack the server deals to the account,
  the computer breaking: a rack searched for at home, or a game shared
  between accounts, no longer earns the exclusive cue. The game asks for
  the final when it starts (a word on screen for a moment); signed out, or
  if the server doesn't answer, the final is played but says it won't earn
  the reward. "Try again" on the final asks for a new one.
- A drill session played for the tables is started by the server: each
  try's cue ball lies a little off its place, different from session to
  session, so one stroke that worked can't be sent for every try. A session
  counts once.

### Accounts and payments
- Changing the recovery e-mail takes the password, always (a stolen
  session could move where the account is recovered from), also for the
  e-mail of the account's own payment, offered after it.
- An account can't be deleted in the middle of a game, at a table, in the
  queue or in a tournament (leave first).
- Deleting an account ends its VIP subscription first (it went on billing
  an account that was gone); if Stripe can't be reached, the account stays
  and the player is asked to try again.
- A refund or a dispute of a VIP month is found from the payment's invoice
  (VIP and its coins stayed after a chargeback).

### Watching and blocks (server)
- The lounge's TV shows public games only, and only the game on it can be
  watched that way (not any game whose number someone guesses); who watched
  it follows its player into public games only.
- A resignation or an abandonment while a shot was being played waits for
  the shot: the stored game showed a different game from the result.
- Blocking withdraws the invitations between the two; an invitation or a
  daily-game challenge accepted after a block is refused; nobody watches a
  game where a player blocked them; the chat history hides a blocked
  player's lines also after they left.
- A player online when their game is rebuilt after a restart is put back at
  the table at once (they lost it for not coming back).
- Deleted accounts leave the daily challenge's, the drills' and the
  challenge rating's boards; profiles ask the database for much less.

### Security
- **Notifications never run text as code.** On Windows a desktop
  notification put its text inside a PowerShell script, escaping only the
  plain apostrophe; PowerShell also takes ‘ ’ ‚ ‛ for quotes, so a
  tournament named with them could run a command on the computer of every
  player who joined it and was away from the game when it started. The
  text now goes to the notifier as data only (the environment on Windows,
  an argument on macOS, after "--" for notify-send).
- The updater's own PowerShell doubles every kind of quote (a profile
  folder such as "O’Brien" stopped the game from coming back), and the
  Windows installer is checked against its SHA-256 again right before it
  runs.
- "Reset to defaults" kept the session but not the server that gave it, so
  a session from another server was then sent to the default one.
- An administrator's name stays theirs: a listed administrator could rename
  their account, and whoever registered the old name became an
  administrator. Staff can't rename, and a listed name can't be registered.
- Signing in is limited per address and per account across connections;
  wrong passwords in "change password" and "delete account" count like
  wrong sign-ins; names that can't exist and passwords longer than any are
  turned away before they cost anything; the limits' memory has a ceiling.
- Private tournaments' and tables' passwords can't be guessed: lounge
  actions are limited for every way in, and five wrong passwords lock that
  table or tournament for the player for ten minutes.
- Chat lines, names and status messages lose the characters that show
  nothing (Hangul fillers, the braille blank, tags, variation selectors): a
  line or a tournament's name made only of them was blank, not empty. The
  aim a player's game sends is passed on within a cue's range (a modified
  game could send absurd numbers to the opponent's).
- A server, or a game on the local network, that names a seat no table has
  is ignored (the game stopped on it); a game snapshot no game could be in
  is refused when it is read; the list of games on the local network can't
  be flooded.

- **Address headers are believed from the proxies only**
  (`SNOOK_LEGEND_TRUSTED_PROXIES`, by default private networks, where a
  host's proxy connects from): a client connecting straight to the server
  could send a new `x-forwarded-for` with every connection and escape every
  limit per address.
- Eight wrong passwords from anywhere locked an account for 15 minutes, so
  anyone could keep its owner out. They are now counted per account and
  address (8), per address (20) and per account from everywhere (60).
- Moderators no longer see players' e-mails, payments and Store purchases
  or where the staff sign in from, nor find an account by its e-mail;
  nobody answers reports about themselves, and moderators act on players
  only (reports about staff, administrators' notes). The staff can't
  rename or delete an account in the middle of a game, a table, the queue
  or a tournament.
- A line break in the chat or a name turns into a space (the words were
  glued together); a few more characters that show nothing are left out.

### Server
- The weekly cup and the staff's tournaments were never recorded (the
  database refused a tournament without a host): they now count among the
  tournaments played and won, and on the trophy shelf.
- A weekly cup the staff called off opened again a minute later; it now
  stays off until next week's.
- A finished game whose saved copy couldn't be deleted for a moment came
  back when a player signed in again; the delete (and recording a
  tournament) is tried again. A server going away no longer rebuilds the
  games it has just handed over, and rebuilt games stay off the lounge's TV
  (whether they were private isn't saved).
- A knockout match neither player showed up for no longer sends the first
  of them through; a guest who leaves is taken off waiting lists too; a
  themed tournament called off for want of players is named (not "the
  cup"); events are read one reload at a time.
- A break that made balls of both groups counted those of the ball hit
  first as the breaker's (statistics, missions).

### Online
- A dropped connection came back badly: the game's messages went out
  before the session was resumed, the server refused them, and that answer
  made the game forget the saved session and close the match. Messages now
  wait for the session.
- A game of the global server could be taken for a local network game
  while hosting or joining one (its messages went to the wrong server).
- A shot made right at the buzzer no longer stops the clock's shot on
  screen, and a game that ends while a shot is on its way ends on screen
  (it could wait for ever).
- Space shoots only when it went down during this turn: Alt+Tab while
  charging no longer plays the shot.
- Reports filed in daily games reach the server. Invitations show over every
  game here, and a click on one no longer reaches the button under it.
- Online, a soft chime says the turn has come back.

### Tournaments (server)
- A running knockout was sent to the whole lounge four times a second; now
  only when it changes.
- A knockout could wait for ever (both players of a match gone, a player
  left on the end panel): matches are settled, players are brought back.
- Entry fees are taken once and given back once (each payment has its own
  reference); everything that can refuse a player is checked before coins
  are taken; a server that stops gives every fee back; the waiting list's
  fees never go into the prizes.
- Arenas: a game resigned or left within a few shots scores nothing, and two
  players score in their first two games against each other only. The
  weekly cup nobody signed up for is called off once. Muted players can't
  name a tournament. Players in a tournament can't start other games.

### The server
- Shots never wait for the database: they have a queue of their own (a slow
  write held every game up), and the ten-minute clean-ups run apart from
  the shot clocks.
- A prize, a refund or a match result the database refused for a moment is
  tried again (each stays single). A stopping server waits for everything
  it has to write.
- A client that stops reading is let go after 15 s (its connection no
  longer outlived it, uncounted).
- Casual games between the same two players earn no experience or cue
  progress past five a day (two accounts could farm each other).
- Stripe: a payment for something this server doesn't sell is answered and
  logged (it was refused, so Stripe kept sending it); a refunded welcome
  pack takes its items back.
- A daily game that can't be rebuilt is looked at again a day later, not at
  every sweep (it held the others up).

### At the table
- **Always show the head string** (Settings → Game, off by default): the
  line the break is played from stays on the cloth.
- Balls made in a row show by the player's picture with a small flame;
  three, five and seven in a row, and a table cleared in one visit, are
  cheered in gold.
- The shot clock's last ten seconds are on a thicker red bar, and the last
  five tick. The rating counts up on the end panel. The deciding shot can be
  watched again from the end panel (which steps aside meanwhile).
- After a win on the tour, "Next" goes straight to the next opponent;
  "Try again" after a loss, and after a drill.
- A new player's first game points out where to aim, set the power and
  shoot. Charging with Space says how to shoot and how to cancel.
- A new game against the computer asks before it replaces the one in
  progress (a tournament's game left that way counted as lost).
- The camera swings round the table instead of through it when the aim
  turns quickly.
- A time foul says what it does under the house rules: the turn passes.
- End-of-game numbers, profiles and missions count only a player's own
  balls (the opponent's balls and a losing 8 no longer count).

### Interface
- Dialogs keep Enter and Esc to themselves (Enter on a gift could start a
  game and replace the saved one); the Esc that leaves a text field doesn't
  also close the screen; a field that is gone no longer keeps the keyboard.
- AltGr types on Windows (@ # € [ ] could not be typed).
- Very small or minimised windows no longer stop the game. Tooltips can
  have several lines; banners and notices fit the window.
- The local network says why a join failed (with "Try again"), the host
  sees the address to join at (a click copies it), and the host's "back"
  asks twice when others are in the game.
- Medals say their metal in words; icons explain themselves when disabled.

### Sound and graphics
- The haze under the lamps and the halos round the bulbs no longer cut hard
  arcs and lines into each other (the halo under a shade was sliced off):
  see-through light is hidden by what is solid without hiding anything.
- A hard break no longer clips: the loudest few hits of a frame play within
  a shared headroom. The volume slider follows a curve like the ear's.
- GPUs without high precision in fragment shaders (some GLES 2 chips) can
  run the game; no texture of uneven size is drawn black there.

### Words
- The server's announcements (restarts, the weekly cup, a tournament's
  winner) and the weekly cups' names in the player's language; mutes and
  suspensions say how long in words that agree.
- Portuguese: pockets that read well after "caçapa", "saída" for the break
  everywhere, "Acerto" for potting success. Spanish: "computadora", "ver",
  "partida", rating, "Retruque", pockets named by the head and the foot,
  every country's name, Spanish keyboard keys. Singulars ("1 ball left").
- The rules say that the called pocket stands to the end of the game, and
  that an 8 knocked off the table on the break goes back on it.

### Under the hood
- The vendored miniquad lets a pipeline test depth without writing it
  (see vendor/README.md).
- The computer's decisions are pinned by tests for every ruleset; snapshots
  are checked when read; saved games and progress are flushed to the disk.

## 0.6.2 (protocol 17)

0.6.0, 0.6.1 and 0.6.2 play together (same protocol and rules). The server
pays the leagues' prizes, so it must be updated with the game. No
migrations.

### The profile
- A cleaner profile: on the left, where the player stands (the level and
  its bar, what the next level and the next league bring) and one row each
  of the medals and reactions won so far; the tabs run over the whole
  width. The reactions no longer run off the bottom of the panel.
- **Leagues and levels**: a page with every league (from which rating, and
  what reaching it pays) and every level that brings an item, with what is
  already won and what comes next. **Achievements**: every medal and
  reaction, on a page of their own.
- Reaching a league for the first time pays coins (50 for Amateur up to
  1000 for Legend, once per league) and, from Skilled up, an item of the
  league's own.

### The view from above
- It keeps its depth and its light. 0.6.1 looked from so high that the
  table read as a flat, pale drawing: it now looks from twice as high as
  before 0.6.1 (enough for the balls to stay round out to the corners), and
  the table is lit as it always was.
- Turning the camera out of the view from above no longer starts with a
  jump upwards.

### For those who build the game
- The workflow that woke a sleeping free server every ten minutes is gone:
  the server runs on a paid plan, which doesn't sleep.
- CI on Rust 1.100: a test helper only the Unix tests use is built on Unix
  only, an outbox is taken instead of drained into a new vector, and "this
  is a build output, never update it" finds Cargo's folder in the layout
  Cargo 1.100 builds in.

## 0.6.1 (protocol 17)

0.6.0 and 0.6.1 play together (same protocol and rules); the tour's last
game is checked against the tour's harder opponent and the server pays the
levels' rewards, so the server must be updated with the game. No migrations.

### One rank to show, one road to walk
- Other players are shown by their **league** (the name and colour of
  their rating's tier) in the lounge, at open tables, in tournaments, in
  the score panel and among recent opponents. The rating itself is on the
  profile and in the ranking.
- The **account level** is a road of rewards: every level reached pays 20
  coins, and levels 5, 10, 15, 20, 30, 40 and 50 each bring an item that
  can be had no other way (two chalks, three cloths, two cues). The main
  menu's card and one's own profile say what the next such level brings.
- The road is long on purpose: a level takes a good deal more experience
  than it did (level 5 is some thirty games away, level 10 over a hundred,
  level 20 about four hundred, level 50 a couple of thousand).

### Tournaments against the computer are hard
- In a tournament every opponent plays at their best, as on the tour's
  last stages.

### A hard economy
- Coins are scarce, so that what the shop sells is earned over weeks and
  months: 2 coins for a rated game and 4 more for winning it (they were 4
  and 6), at most 24 a day from games (50), a daily bonus of 5 (10), and
  missions that pay half of what they did. Prices are the same.

### Tournaments against the computer
- A game left half way is picked up from where it was (the button says
  "Continue"); one that was not kept (another game against the computer
  took its place) counts as lost. A game can no longer be started again
  when it is going badly.
- No rematch after a tournament's game: the result stands and the
  tournament goes on.
- The champion gets a gold medal and their name by the bracket.
- The opponents no longer say their opening line with their remarks turned
  off (in any game against the computer: the line was said before the
  setting was read).

### The tour is hard
- On the tour every opponent plays at their best: tighter aim and pace, a
  wider look at the table, more thought for the next shot, safeties when
  nothing is on. The server checks the final game against that player.

### The view and the aim
- The view from above looks from high up through a long lens, so the table
  is seen almost flat: balls stay round out to the corners (at the ends of
  the table they were drawn a sixth longer than wide, the ghost ball
  included), and the cue, the aiming line and the ghost ball line up
  whatever the cue's elevation.
- The ring round a cue ball being placed sits exactly round it.
- A softer light on the balls (a patch, not a hard white dot on the
  number) and no reflection on the balls of the score panel.
- The low end of the power bar is a little stronger; the top is the same.

### Achievements
- A golden break (a rated game won by making only the 8 on the break), six
  months and two, three and five years of account. "Shutout" is "Capote" in
  Portuguese.

### Pictures and sound
- Faces with a person's eyes (almond-shaped, the iris filling them), for
  the profile pictures and the computer's opponents: they had a doll's
  stare. Twelve pictures that are not faces: the 8-ball, the cue ball, a
  striped ball, the rack, crossed cues, the chalk, a trophy, a star, a
  crown, a table, a flame and a bolt.
- New sounds for the balls, the cue, the cushions and the pockets: struck
  resonances, not pure tones; a hard hit is brighter than a soft one, not
  only louder; a room without the ringing echo.

### Windows
- The game opens on the monitor the pointer is on, not always on the
  primary one.

## 0.6.0 (protocol 17)

The game now plays a **regional Brazilian rule** of eight-ball (the rule of
bars and pool halls, with penalty balls), not the official tournament rule,
with heavier-feeling balls (ruleset 4). Every shot and every foul plays out
differently, so older games must update to play online (the server must be
updated with the game). No migrations.

### The rules (ruleset 4)
- A foul passes the turn and gives the opponent balls: the lowest numbers
  of their group still on the table come off it, as if they had made them.
  One for hitting none of one's own balls (the 8 hit too soon included), one
  for a scratch, two for hitting one of the opponent's balls first, directly
  (one if the cue ball went to a cushion before it). They add up: a miss
  that scratches gives two, the opponent's ball hit directly and a scratch
  give three. The 8 never comes off; on an open table a
  foul costs the turn alone.
- Only a cue ball that went down or left the table is in hand. After any
  other foul it is played from where it lies.
- A ball of one's own knocked off the table comes back to it; one of the
  opponent's stays off, made for them.
- The 8 alone on the break wins; the 8 on the break with any other ball
  loses. At the end, the 8 loses when the cue ball or one of the opponent's
  balls goes down with it.
- A break that makes only the cue ball goes to the opponent: the balls are
  racked again and they break.
- No illegal break, and nothing has to reach a cushion after a contact.
- The rules are written in the game (three languages) and say plainly that
  this is a regional rule; the README says so too.

### The balls
- Heavier-feeling balls: a slower cloth (a rolling ball stops about a
  quarter sooner), a rack that soaks up more of the break, cushions that
  give back a little less, and a break struck at 18 mph for the cue ball
  (it was 23): the break no longer scatters the balls all over the table.

### Against the computer
- **Tournaments** with the fifteen opponents: a knockout (sixteen players,
  four rounds) or an arena (five rounds for everyone, on points, with a
  bonus for a run of wins). The player plays their own games; the others'
  are decided by their strength and a draw. A tournament is kept between
  sessions.
- Eight more **drills** (fifteen in all): the side pocket, the long cut, the
  thin cut, along the cushion, the stop shot, the combination, the
  cross-corner bank and the kick shot.

### At the table
- The cue's elevation can be set by dragging in its panel: an angle with
  the wrong sign made every drag come out level (only the keys and the
  wheel worked).
- The aiming line runs from the cue ball's centre to the ghost ball's: it
  lay on the cloth and passed beside the ghost ball from any view not
  straight above it. The ghost ball is still a see-through ball, drawn so
  that its far side no longer shows through in patches and a cushion or a
  pocket's casting no longer cuts it off (it looked squashed).
- With the cue ball in hand, four faint arrowheads round it say it can be
  moved.
- A setting to turn off the shadows of the balls and the cue (Settings →
  Graphics → Shadows).
- A shorter power bar.

### Accounts and the server
- The saved session is only ever sent to the server that gave it: pointing
  the game at another server no longer signs in there with it.
- IPv6 clients are counted by their /64 for the limits per address
  (connections, new accounts, wrong passwords).
- An account is used in one place at a time. Signing in again already
  closed the connection that was there; now the server also refuses
  anything a replaced connection still sends, and reads nothing more from
  a connection once it is told to go.

## 0.5.1 (protocol 16)

A pass over the graphics, from the balls to the menus. Nothing changes in
the rules, the physics or the protocol: 0.5.0 and 0.5.1 play together. No
migrations.

### The balls and the table
- No more pale disc on top of the balls. The cap of every ball (what faces
  up) was lit as the cloth is, as if from a wide fixture, and the rest of
  the ball from where it stands: a paler circle with a hard edge sat on each
  one, the ghost ball included. Only the table's own surfaces are lit that
  way now, and a ball is shaded as one round thing.
- A ball shows the light over the table as one clean dot in a faint glow.
  It is never drawn smaller than a pixel or two: seen from above it was a
  speck that flickered as the ball rolled and read as a scratch (and the
  three lamps side by side as a ragged dash).
- Only the balls are read from a sharper level of their texture; wood grain,
  floors and rugs, read sharper than their size on screen, shimmered as the
  camera moved.
- The castings (and the leathers) round the pockets are shaded as plates
  with eased edges: the light runs across them (they were cut-outs of one
  flat colour).
- Low quality is lit from the row the three lamps hang in. Its single lamp
  stood in the middle, as bright as all three: the ends of the castings and
  the facings of the side pockets burnt out to white.
- The rugs of the halls have smooth outlines (four samples a pixel, and the
  English club's at twice the size); the fine gold lines of the pool hall's
  rug no longer break up into dots seen from above.

### Aiming and the power bar
- The power bar no longer marks the last stroke, and it no longer starts a
  turn where the last one left it: every turn starts from the middle of the
  bar (the break from a firm stroke). Nothing tells what the last stroke was
  played with: judging the pace is part of the game.
- The bar's colours run into each other without steps and its ends are
  round, like the track it lies in.
- The aiming line, the lines of the full guide and every ring on the table
  have soft edges and are never thinner than a pixel, whatever the screen
  and the anti-aliasing (the line to the ghost ball was a stepped hairline).
- The rings round the pockets (calling the 8, a drill's pocket) are whole:
  they lay under the castings, which hid all but an arc over the cloth.
- Setting up a game (against the computer, two players) offers all four
  aiming guides; "No guide" was missing there, and with it chosen in the
  settings none was lit.

### The view
- Whose turn it is, and what they are on, is written in the top bar beside
  the score panel. The line used to lie across the top rail and hid the side
  pocket from above; a long line wraps onto two there.
- The 3D view of the industrial loft was black: the camera sat inside one
  of its steel beams. The camera stays below the beams of every hall, and
  nothing of a hall is drawn right against the lens (a beam, a duct or a
  lamp the camera has moved into no longer blacks out the view).
- From above, the table leaves room for the zoom bar at the side; in a
  narrow window the bar sat on the rail.
- In the score panel, a name is shown whole when it fits (in slightly
  smaller type if it must) in a wider box; "Computer" was cut short.
- Choosing an opponent, a nickname takes the lines that fit on its card
  instead of being cut after the first.

## 0.5.0 (protocol 16)

A new edition of the physics and the table (ruleset 3): the WPA's 8-foot
tournament table, measure for measure, played with a tournament table's
physics as measured. Every shot plays out differently, so games older than
this one (0.4.0 and before) must update to play online. Games already
recorded (replays, games saved during a restart, daily games in progress)
keep the rules, the physics and the table they started with, shot for shot.
No migrations.

### Tournament-table physics (ruleset 3)
- The physics of a tournament table as measured (Alciatore's and Marlow's
  figures for phenolic balls, worsted cloth and K-66 cushions): rolling
  resistance of 1% of g, side spin that dies at 10 rad/s², ball friction
  that falls with the rubbing speed (0.118 to 0.01: slow cuts throw more),
  balls that give back 95% of a soft hit and 85% of a break.
- Cushions give back less the harder they are hit: a ball rolling straight
  in rolls away at about 60% of its pace on a lag (it was 46% at every
  speed) and at under 40% at break speed.
- Strokes at real speeds. A 19 oz cue with a leather tip (the ball leaves at
  1.32 times the cue's speed); the power bar of an ordinary stroke tops out
  at a power shot (12 mph, 5.4 m/s for the ball: it was 21 mph on every
  stroke, so pots were played at break speed and the balls looked light),
  and only the break gets the whole cue (23 mph, 10.5 m/s). Half the bar is
  a stroke of medium pace (3 mph). The server enforces the same limits.
- A ball made falls in: it keeps the velocity it went over the lip with,
  drops under gravity, rolls over the lip or hits the liner at the back and
  goes down (it used to float over the hole and sink slowly).
- The table is the WPA's 8-foot tournament table, measure for measure
  (a test holds every figure): a 92" × 46" playing surface (it was the
  88" × 44" of a home table), 2 1/4" balls of 6 oz, cushion noses at 63.5%
  of a ball's height, 4 1/2" corner mouths with 142° facings and a 1 1/2"
  shelf, 5" side mouths with 104° facings and a 1/4" shelf. The side
  pockets' facings turn in towards the hole (they opened out behind two
  points that hooked towards the pocket, round a hole 170 mm across); the
  band of paths that drop is 86 mm straight into a side pocket and 66 mm
  into a corner.
- The balls shine like polished phenolic: each lamp is a small, crisp
  glint (a wide soft one read as a pale disc painted on top of the ball),
  and small balls are read from a sharper level of their texture, so the
  numbers stay crisp from the top view and on Low quality (which also
  gets rounder balls).
- In the score panel a ball's number sits in the middle of its ball,
  facing the viewer, larger and black on ivory (turned aside and tilted,
  it was squashed to a few pixels); the waiting player's rail is dimmed
  less.
- Spin and elevation checked against a real table, and kept by tests: the
  tip at the miscue limit gives a spin of 1.25 times the ball's speed, a
  rolling cue ball leaves a half-ball hit at 34° (the 30° rule), full side
  spin comes 25° off a cushion hit square, a slow cut throws 3°, a cue
  raised 5° swerves a few centimetres in a metre and a massé hooks round
  (`--example spin_stats` prints them all).
- The cloth is a deep green under the lamps, brightest in the middle and a
  little darker towards the cushions (it washed out to a pale pastel); the
  balls keep all their light, so they stand out and their numbers read.
- The aiming guide stops where the ball would: it is traced against every
  cushion, pocket facing and hole (it used the rectangle of the cushion
  lines, so the ghost ball hung in the mouth of a pocket or went wrong
  beside one), and nothing of it is drawn inside a ball when the contact is
  closer than a ball's width or the cue ball is touching what it is aimed
  at. Aimed into a pocket, the line runs to the lip with no ghost ball.
- A fourth aiming guide, "No guide": nothing on the table, in Settings and
  when setting up a game.
- Rulesets 1 and 2 play exactly as before (replays and games in progress);
  `cargo run --release -p snook_legend_core --example break_stats` prints
  the numbers of the break, the cushions and the cloth for each ruleset.

## 0.4.0 (protocol 15)

A new edition of the rules (ruleset 2): an 8-foot table with open side
pockets, balls that roll a little more freely, and a new rule. Every shot
plays out differently, so games older than this one (0.3.0 and before) must
update to play online. Games already recorded (replays, games saved during
a restart, daily games in progress) keep the rules, the physics and the
table they started with, shot for shot. Migrations 0013, 0014 and 0015 run
on start-up.

### The game is called Snook Legend
- The new name everywhere: the window, the menus, the installer and its
  shortcuts, the Microsoft Store, the server's messages and pages, the
  staff panel, the store art and the documents, and every technical name:
  the crates (`snook_legend_core`, `snook_legend_proto`,
  `snook_legend_server`), the executables (`snook-legend`,
  `snook-legend-server`), the release files (`snook-legend-<platform>`),
  the server image, the repository's address and every setting
  (`SNOOK_LEGEND_*`, which replace the old prefix: rename them on the host
  before deploying).
- The settings folder takes the new name (`Snook Legend`, `snook-legend` on
  Linux); the old one is moved there the first time the game starts, so the
  settings, the session, the tour and the saved games come along.
- Copies of 0.3.0 installed from GitHub can't find the new release files by
  themselves: they show the button that opens the releases page (the
  installer upgrades the old installation in place). The Microsoft Store
  updates its copies as usual.
- "SnookLegend" and "Snook_Legend" are reserved usernames.

### The table and the rules
- An 8-foot table (88" × 44"; it was 9-foot), with open, forgiving side
  pockets: a 6.1" mouth whose facings open out, a hole that starts right
  behind it and a liner behind the hole. Measured with the physics, they
  take paths about 118 mm wide from straight in to 35° (the 9-foot table's:
  80–96 mm), and no ball can come to rest in the throat.
- The balls roll a little more freely (the cloth takes 1.1% of g off a
  rolling ball, it was 1.25%) and the cushions give back a little more.
- New rule: making one of the opponent's balls passes the turn, even with
  one of your own on the same shot (not on the break). It is not a foul:
  no ball in hand. The game says why the turn passed.
- Every game records its ruleset; the server keeps it with every game, and
  the game draws each game on its own table.

### Easier to see
- The cloth is lit evenly: its edges get 87% of the middle's light (they
  got 54%), so every ball reads the same wherever it is.
- The balls have a soft sheen instead of a hard glint: their colours and
  numbers always read. Shadows (balls, contact, the cue) are lighter.
- Looking down on the table, nothing of the hall comes between the camera
  and the table (some halls showed cords, ropes and lamp parts across it).
- A new score panel at the top: each player's picture and name in a box,
  and a rail with the balls of their group in the order they went down
  (the first player's from the left, the second's from the right), drawn
  as spheres seen slightly from above. A ball just pocketed rolls in from
  the far end of the rail into its place. Whoever isn't at the table is
  dimmed a little; the shot clock runs along the active rail.
- Settings → Game → "Top view only": the table is always seen from
  above (no 3D view, no turning, no TV camera); zooming still works.

### Aiming by eye
- The elevation panel shows the cue and the cue ball from the side, with a
  curved arrow: drag the cue (or anywhere in the picture) to raise or
  lower it, the wheel nudges it, "Level" puts it back.
- No numbers for the power or the elevation any more (the percentage by
  the power bar and on the last-shot mark, the degrees of the elevation):
  the player judges them by eye.

### Rules and profile pictures
- The rules of the game, as this game plays them, in the three languages:
  the book on the main menu, or "Rules" in the pause menu.
- Thirty-two profile pictures to choose from (in the profile, or Settings
  → Game when playing offline), shown in the score panel, in the profile and
  in the lounge's list of players. The account keeps the choice (migration
  0015; protocol: `SetAvatar` and `PlayerInfo.avatar`).

### Against the computer
- Five more opponents, each with a portrait and a voice of their own: Tito
  the Gambler, Inês the Soft Touch, Diego the Showman, Amara the Tactician
  and Yuri the Machine.
- The tour has fifteen stages; the champion's cue needs all fifteen. A tour
  played before keeps every win, and an earned cue stays earned.
- Settings → Game → "Opponents' remarks" turns the opponents' speech
  bubbles off.

### Microsoft Store
- Ready, and off until the server is set up for it (see `docs/MSSTORE.md`):
  without it the Store version's Coins tab says coins come from playing, as
  before.
- Coin packs can be bought in the Microsoft Store version, through the
  Store's own purchase dialog and with prices in the player's currency (the
  Store's policy for games). VIP and the welcome pack are still not sold
  there.
- The server never takes the game's word for a purchase: it asks Microsoft
  which packs the player's Microsoft account bought, has Microsoft consume
  each one under a tracking id saved beforehand, and credits the coins only
  on Microsoft's confirmation, once. Answers lost on the way are asked again
  (on the next visit to the shop and every minute in the background) until
  Microsoft answers; a purchase paid while the game closed arrives the next
  time the shop opens.
- The staff panel lists each player's Store purchases (pending, done,
  refused or stuck, with Microsoft's order ids) and counts Store sales in
  the metrics.
- The privacy policy says what is kept about Store purchases.

### Fixes
- A game newer than the server no longer says "a new version of the game is
  out" (which sent players already up to date to the Store): it checks the
  server's version and says the server is still being updated, with a Retry
  button. Servers from this version on say so themselves.

### Releases
- The **Server image** workflow puts the new server live on Render by itself
  when the repository secret `RENDER_DEPLOY_HOOK` holds the service's Deploy
  Hook (see the deploy guide); without it, it reminds you to deploy by hand.

## 0.3.0 (protocol 14)

The game now plays on a regulation 9-foot tournament table with heavier,
more realistic balls and a new break rule: every shot plays out
differently, so older games (0.2.0 and before) must update to play online.

### The table and the balls
- A 9-foot tournament table to the WPA's specifications: a 100" × 50"
  playing surface (it was a 7-foot bar table), 4 1/2" corner and 5" side
  pockets, 2 1/4" balls of 6 oz, a 58" cue with a 13 mm tip.
- Heavier balls, as players asked ("they felt hollow"): measured values
  for a tournament table (the cloth takes the pace off a little sooner, the
  balls give back 93% of a hit, the cushions about 70%), and a power bar
  that keeps most of its length for ordinary shots: half the bar is a firm
  shot of about 2.4 m/s for the ball (it was almost 4), the top still a
  full break.
- The thinnest grazes at full power always count as contact (a rare case
  where two balls touched within one step of the simulation went unseen).

### Rules
- The break decides the groups: the first ball made on a legal break is
  the breaker's group (solids or stripes), and they shoot again. A foul on
  the break leaves the table open. (Until now the table stayed open after
  the break, as in WPA rules, so the breaker could pick the easier group.)

### Halls and tables
- Four new halls in the shop: the Mountain lodge (log walls, a fieldstone
  chimney with a fire, tall windows on snowy peaks, 6500 coins), the Neon
  arcade (a black mirror floor, neon tubes and signs, arcade cabinets and
  pinball machines, 7000), the Art deco lounge (a black and white marble
  floor, emerald silk, gilt pilasters, sunburst mirrors, velvet banquettes,
  8000) and the Skyline penthouse (glass walls on the city at night, pale
  marble, an oak slat wall, VIP). Each has its own lamp over the table and
  its own picture in the shop.
- Four new tables: Steel & oak (reclaimed oak on a braced black steel
  frame, a riveted apron, 5500), Tournament (charcoal rails, block legs,
  chrome, 7000), Victorian walnut (figured walnut on barley-twist legs,
  leather pockets, 8000) and Ivory deco (ivory lacquer and gold on fluted
  columns, 8500).
- Every hall is lit better: the table, the bar, sofas and chimneys cast
  shadows from every light of the room (soft ones on Ultra, which also
  bakes the light on a finer grid and darkens what is close to them),
  varnish, brass, glass, leather and polished floors catch the lamps as the
  camera moves (High and Ultra), and the lights glow with soft halos and a
  faint haze under the lamp over the table (High and Ultra).

### Graphics
- Ultra: polished floors (the pool hall's planks, the club's parquet, the
  loft's concrete, the new halls' marble and epoxy) reflect the room, the
  table and the balls, fading with height like a real floor; the textures
  of the halls and tables are drawn at twice the resolution.
- The balls, the rails and the castings reflect the colours of the hall
  around them (a sunset, neon, warm wood) instead of one brown room.
- Faster: the halls, the table, the balls and the cues stay on the
  graphics card instead of being copied and sent again every frame (the
  pool hall alone was 18 000 vertices a frame on Low, 39 000 on High,
  62 000 on Ultra, plus the table and the balls); the meshes made every
  frame (guides, shadows) no longer allocate per quad.
- The lamps over the table, their light and their haze span the table's
  length, whatever its size.
- The balls are a little less glossy: a softer highlight from the lamps
  and a fainter reflection of the room, closer to real resin balls.

### Lounge
- While "Play now" looks for an opponent, "Practise while you wait" opens a
  table to yourself at once: shoot as you like, a new rack when one is
  over; the game starts as soon as an opponent is found (the game against
  the computer is still offered after half a minute).
- Players in a game against the computer show it in the lounge ("playing
  the computer", a purple dot) and can still be invited: the invitation
  appears in the top right corner of their game, with Accept and Decline.
  Accepting starts the online game; the game against the computer stays
  saved ("Continue" in the menu). Older servers and games are unaffected
  (the server says its protocol in its welcome, and the game only tells a
  server that knows the message).
- Players who haven't touched their game for ten minutes (a minimised
  window included) show as "away" in the lounge, with a yellow dot.

### Profile
- An account level, separate from the rating, so playing always moves you
  forward: experience for every online game played to the end (60 for a
  win, 30 otherwise, plus 2 a ball), the daily challenge and each
  achievement. The level shows on the menu's player card (with its bar)
  and on the profile; a level up is announced. Existing accounts start
  from the games they already played (migration 0011).
- Cue statistics: the cue card counts the online games played with the cue,
  how many were won and the balls made with it (migration 0011; wins so far
  count as games).
- A trophy shelf on the profile (the Trophies tab): tournaments won (with
  how many played), the computer tour and the days topped in the daily
  challenge, newest first, with the date.
- Change your username in the account dialog, up to three times in all
  (the dialog shows how many changes are left). The same rules as a new
  account, never someone else's name in any letter case, and not during a
  game, at a table, in the queue or in a tournament. You stay signed in;
  friends, games and items follow the account (migration 0012).

### Against the computer
- The ladder is now the **Tour**: ten stages, one opponent each, the same
  exclusive cue for winning it all.
- Marta "the Wall" plays the part again on the bigger table: she goes for
  a pot only when it's on, and otherwise plays safe. (A careful player no
  longer counts on a missed pot to leave nothing, only on a safety played
  on purpose.)
- The opponents talk: a line now and then in a speech bubble under their
  card, about your shots and their own, in each one's voice (Bruno wants
  more power, Kenji quotes physics, Rosa "hardly ever plays"…), in every
  language of the game. They greet you at the start and always have a word
  at the end; in between they speak up only some of the time.
- New portraits for the ten opponents, each with a character of their own
  (Leo's crooked cap and plaster, Otto's moustache and flat cap, Rosa's
  wink, Viktor's medal and bow tie...), and a friendly robot for the
  classic levels.

### Daily challenge
- A new tab on the "Against the computer" screen: the same table for
  everyone, all day (UTC), with five to seven balls. Make as many as you can
  in one visit; a miss or a foul ends the attempt. Three attempts a day,
  the day's table (ties go to who made it first), 25 coins for clearing the
  table and a trophy for the day's winner. The position stays secret until
  an attempt starts, and the server plays the strokes again to count the
  balls; the full aiming guide is never shown (it's ranked).

### Chat at the table
- Quick phrases (a button by the chat and the reactions): "Nice shot!",
  "Good game!", "Rematch?" and more. Each player reads them in their own
  language, whatever the other one speaks (they travel in English, so
  older games see them in English).

### Drills
- A new tab on the "Against the computer" screen: seven short exercises on
  fixed positions, the same for everyone (a long straight pot, a cut, draw
  back into a circle, follow through, a bank, escaping a snooker, ten pots
  in a row). Each earns up to three stars and has its own table of the
  best scores; the stars and scores of every drill make the player's
  challenge rating, with its own table. The pocket to use and the circle
  to stop the cue ball in are marked on the cloth. The server plays the
  strokes again to count them; new stars give experience.

### Watching
- Spectators see how hard the shot being lined up is, once the shooter's
  aim settles: easy to very hard, the chance a good player makes it
  (24 slightly imperfect strokes simulated on a worker thread), the cut and
  the cushions.

### Fair play
- Leaving a game in progress (closing the game, or losing the connection
  for good) costs more than losing it: 10 extra rating points in a rated
  game, and a wait before the next online game (2 minutes, then 10, then
  30 for each game left within a day). The player who stayed now gets the
  winner's coins. Resigning is never punished.
- The full aiming guide can never show in a game against a person over the
  network, rated or casual (nor when watching): the guide drawn is checked
  at the table itself, whatever the setting says. Nor in the Tour, where
  a cue is at stake, the daily challenge or the drills: the full guide is
  for free games against the computer and practice.

### Cues
- Six new cues that look like nothing else: Phoenix (flames), Carrara
  marble, Dragon scale, Chrome, Galaxy (a starry sky) and the VIP Ruby
  throne (cut stones); the Crown, Obsidian gold and Legend cues got their
  own patterns too. The finest cues have a carbon shaft and carry their
  pattern on the butt sleeve.
- The shop finally shows each cue's pattern (it only showed its colours):
  a crisp picture of the cue, lit like varnished wood, the same pattern as
  at the table.

### Staff panel
- Metrics: retention after 30 days beside 1 and 7 (each over the last 30
  days of sign-ups that reached that day), a table of retention by week of
  sign-up, players and games by day (30 days) and games per day.

### At the table
- The power bar marks the power of your last shot (each player's own), to
  repeat it or to judge the next one from it; the mouse wheel over the bar
  sets the power one percent at a time.
- Free practice: take the last shot back (a button, or Z) to try it again.
- The players' cards show the balls each one has made (they showed the
  balls left), the group's name until the first one drops.
- Offline, the game's history opens from a small icon in the top bar (the
  "History" button over the table is gone).
- The tournament table at its regulation height (the rails' top at 30"),
  the cameras framing the bigger table, and the lamp over the table out of
  the way of a camera above it (its light stays).
- The power bar starts the break at 90%: four balls must reach a cushion,
  and half the bar no longer does it on the heavier table.

### Fixes
- Letting go of the power bar over the Shoot button no longer shoots (at
  full power, as the drag past the bar's end had set it). Every button now
  needs the press to start on it: a drag (the power, the spin, the aim)
  that ends over a button never clicks it.

### Releases
- The Microsoft Store workflow only builds the MSIX (the `msix` artifact);
  the package goes to the Store by hand in Partner Center.

## 0.2.0 (protocol 13)

The server still lets 0.1.2 to 0.1.4 in (protocols 11 and 12): they keep
playing as before and never receive the new messages. Migrations 0006 to
0010 run on start-up.

### Against the computer
- Ten opponents with names, portraits and styles (from Leo, still learning,
  to Marta, who lives on safeties, Lola, who banks everything, and Viktor at
  the top), built on a style layer over the AI's skill. Easy, Medium and
  Hard still exist underneath.
- The ladder: beat the ten one by one. The last win, checked by the server
  by replaying the game and every one of the computer's strokes, earns the
  "Ladder king" cue and a new achievement.
- Play the ghost: the server measures a player's habits from their online
  games (power, spin, safeties, banks, potting) and the computer plays in
  that style.
- The AI only uses the physics' portable maths, so its decisions are the
  same on every system (pinned by a test).

### Daily games
- Correspondence games: challenge anyone (online or not), rated or casual;
  each turn (a whole visit) has 72 hours. Live notices, desktop
  notifications and a badge on the menu; the opponent's strokes replay when
  you open the game. Up to 10 at once. Finished games go into the history
  and, when rated, count for ratings and achievements (not for coins or
  missions).

### Friends
- Recent players (the last 20 opponents) with add friend, invite and watch;
  "Add friend" on the result panel of an online game.
- A status message on the profile, shown in the lounge and the friends list.
- A "looking for a game" board on the Players tab: post a note, others
  challenge you from it; notes expire after 30 minutes.
- Gifts are kept: a "You received a gift!" dialog (also for gifts received
  while away) with "Use it now" and "Thank", an optional message with a
  gift, and a Gifts tab on your own profile.

### Shop
- Cues level up with the online games won with them (10 to 500 wins): bands,
  inlays and, at the top, a gold scroll with a soft glow, seen by everyone.
- Themed tables (beach bar, modern, English club; ebony & gold for VIP) and
  halls (beach bar, industrial loft, English club), drawn procedurally and
  scaled by the graphics preset. Each player sees their own.
- The rosewood cue is "Jacarandá" in Portuguese.

### Profile and moderation
- Analysis tab on the profile: potting by pocket, distance and kind of shot,
  safeties and breaks, and a heat map of the table (from games after this
  version).
- Reports carry the game between the two players (live or recent); the
  staff panel has a replay viewer for them and for any game.
- Staff metrics: players and games per hour for 48 hours, revenue per day,
  the queue and its waits, sampled every minute and kept across restarts.

### Settings and updates
- Every keyboard shortcut can be changed (Settings → Controls); the game's
  hints show your keys.
- Automatic updates on every platform: the new version downloads in the
  background, is checked against its SHA-256 and replaces the game safely
  (with a backup); "Restart now" from the menu, never during a game. Store
  builds are updated by the Store.
- "What's new" after an update (and from the version label).

### Interface
A review of every screen, in the three languages and from 1024×600 to
1920×1080 (see `docs/UX.md`).
- Panels and buttons have smooth gradients: the hard-edged sheen that cut
  through titles and labels is gone. Disabled controls look disabled.
- Settings: one layout on every tab (sections of rows: label and hint on
  the left, the control on the right), sized to its content; switches
  instead of scattered checkboxes; the server field under "Advanced";
  "Reset to defaults" asks once more.
- Main menu: a player card with an avatar and the tier in its colour,
  hover and pressed states, and a compact layout for large interface sizes.
- Lounge: a tidy sign-in form (Enter and Esc work), a header that fits its
  content, empty states with a way forward, toasts that are never cut, a
  tidier tournament form, details and bracket.
- Shop: unaffordable items read as such at a glance (price in red, how many
  coins are missing); one way to page; ball sets drawn properly.
- Profile and ranking: aligned columns, readable locked medals and
  reactions, your own row highlighted, loading and empty states.
- At the table: the pause menu and the list of controls are modal and fit
  their content; the result panel fits long names; no cue settings when no
  one here holds a cue (after the game, replays, the computer playing
  itself); the cue settings dim while it is not your turn.
- LAN: a searching state instead of "Looking for games......"; the country
  picker searches without accents and works with the keyboard.

### Fixes
- Esc opens the menu at the table: with a power set on the bar it reset the
  power instead.
- Esc in a shop, profile or account dialog closes the dialog, not the
  screen.
- A quick click on the country field no longer picks a country too.
- The overhead view with the bar around the table no longer rises into the
  ceiling beam above the table (the screen went black); it stays under the
  ceiling and widens its view instead. The same for the broadcast's view
  from above.
- The computer no longer picks a clearly worse shot just for variety (often
  the same pot at a speed that scratches, or that sends the 8 into another
  pocket), and on the 8 every level checks that the shot still works when
  struck a little imperfectly: it rarely sinks the 8 in a pocket other than
  the one it called.
- Watching a game: the TV camera is off by default (it can be turned on in
  the settings).

## 0.1.4 (protocol 12)

Only additions to the protocol: the server still lets 0.1.2 and 0.1.3 in
(they don't see deleted chat lines go, nor the new reactions).

### Lounge
- "Play now": after half a minute without an opponent, a game against the
  computer while the search goes on (not saved); the online game takes
  its place as soon as someone is found.
- Chat moderation: anyone can delete their own lines (shown as deleted by
  the author); administrators can delete any line (shown as deleted by an
  administrator) or remove it without a trace, and clear the chat (from
  the game or the panel; everyone is told who did). The text stays on the
  server for reports.
- Administrators' names are red everywhere, and only theirs (name styles
  and the Grandmaster tier moved away from red); their badge in the panel
  is red too.
- An administrator watching a game reads the players' chat (read only);
  other spectators still don't.

### Games and shop
- Cues on the table show their pattern (points, spiral, weave), so the
  opponent sees the cue you bought; a cue, chalk or effect changed during
  a game shows at once.
- VIP members have six reactions of their own while they are VIP.
- The shop shows clearly when a category has more items than fit, with
  buttons to the next and previous ones.
- Against the computer, the name is the signed-in account's (editable).

### Fixes
- Profile: a shop frame crowded the header (back button, crown, the line
  below).
- Replays: on small windows the History button sat on the playback bar.
- Profile: the hover hints of the achievements showed through the account
  dialog (and any dialog over the page); the page under a dialog (and
  under the e-mail offer) no longer reacts to the pointer.
- Microsoft Store: the icon no longer sits on a blue plate in the Start
  menu and the taskbar (the package has every size Windows asks for,
  plated and unplated, and the resource index that finds them).

## 0.1.3 (protocol 12)

The server still lets 0.1.2 in (protocol 11): it ignores what is new, so
nobody is locked out while the update (and the Store's review of it)
reaches everyone.

### Economy
- Coins come much more slowly: 4 per rated game and 6 more for a win (up
  to 50 a day), a daily bonus of 10, and smaller mission rewards (10–15 a
  day, 50–70 a week). Items cost 2.5 times as much (400 to 6250 coins).
  Coin packs, VIP and the welcome pack bring the same coins as before.

### Lounge
- **Missions**: three a day and two a week, the same for everyone (play,
  win, pot balls, pot with spin, three in a row, win without a foul). They
  count rated games played to the end and pay their coins once complete.
  A tab beside the lounge chat shows them with the time left.
- **The board**: notes the staff pin from the panel (for some days or
  until taken down), shown to everyone at once, with the tournaments open
  for registration. A dot on the tab marks a note not read yet. Notes are
  notices (gold), events (green) or rules (blue), can be pinned at the top
  and can carry a button (tournaments, shop, VIP, missions). The newest
  also show on the main menu.
- Notifications when a tournament starts in a few minutes or one you are
  in begins; Settings → Game turns system notifications off. On Windows
  the toast goes out under the game's Start menu entry, so it shows.

### Watching
- A television camera when watching a friend, the featured table or a
  replay: a wide shot as a turn begins, from behind the shooter while they
  aim, and the rolling balls from above or a high corner. Moving the camera
  pauses it until the view is reset; Settings → Game turns it off.
- **The crowd**: spectators click a player's card to cheer for them; a bar
  under the cards shows how the spectators are split.
- **Replays like a video**: pause, ½× to 4×, the shot before or after
  (← →) and a time line to drag to any moment, marked where balls were
  made and fouls committed.

### Games
- The game against the computer is kept after every shot: closing the
  game no longer loses it, and the menu offers to continue it.
- End of game: besides the numbers, the momentum (who was on top, shot by
  shot) and a map of where every shot was played from.

### Shop
- Give a month of VIP to a friend (a one-off payment, the `vip_gift`
  price in `SNOOKERED_STRIPE_PRICES`). Not in the Microsoft Store version.
- **The welcome pack**: once per account, the cue "First break" and the
  cloth "Lagoon" (sold nowhere else) and 600 coins (the `welcome_pack`
  price). Not in the Microsoft Store version.
- The Coins tab says first that coins are earned by playing (as the Store
  version does); buying them is only an extra.

### Updates
- When the server refuses an old version, the sign-in page and the menu
  offer the update: the Store's updates page for the Store version; the
  installed (.msi) game downloads the new installer, closes and runs it;
  other copies open the releases page. The saved session is kept.

### Interface
- Text fields: Tab and Shift+Tab move between fields; a click puts the
  caret where it lands; drag, Shift+arrows or a double click select;
  typing replaces the selection; Ctrl+C / Ctrl+X / Ctrl+V.

### Server and panel
- Administrators may change their own wallet (coins, items, VIP days).
- Payments made with Stripe's test keys are marked *test* and left out of
  the revenue; older ones can be marked by hand on the player's page.
- Migration 0005 (missions and the board).
- The README lists Spanish among the languages.

## 0.1.2 (protocol 11)

### The game is called Snookered
- New name everywhere: window, menus, lounge, notifications, the installer
  and its shortcuts, the server's messages and pages. Installing 0.1.2 over
  0.1.1 replaces it (now in `Program Files\Snookered`); the settings moved to
  a new folder, so sign in again once.

### Distribution
- The Microsoft Store version: an MSIX package built by the *Microsoft Store*
  workflow from release tags and sent to Partner Center as a draft
  submission (`docs/MSSTORE.md`). It sells nothing through the game's own
  payment pages (coins come from playing), has the C runtime built in, and
  opens the local network ports in the firewall.
- Pictures for the Store pages: screenshots in three languages
  (`store-screenshots/`) and the poster, box art, tile icons and hero art
  (`store-art/`, made by `tools/store_art.py`).

### Server
- The privacy policy, in Portuguese (`/privacidade`) and English
  (`/privacy`); `SNOOKERED_GAME_NAME`, `SNOOKERED_PRIVACY_CONTROLLER` and
  `SNOOKERED_PRIVACY_CONTACT` fill in its names.
- Environment variables are now `SNOOKERED_*`.
- A test keeps migrations already applied in production unchanged (the
  server refuses to start when one is edited).

### Fixes
- Lounge: the featured game's "watching · average" line ran into a long
  label ("EN LA TELE" in Spanish).

## 0.1.1 (protocol 11)

Everything in the 0.1.1 betas, and:

### Fixes
- Tournaments: a player who withdrew from a knockout while waiting for their
  next opponent stayed in it (held from other games, then pulled into the
  match). They now lose that match by walkover and are free at once.
- The ranking left out for good anyone whose temporary ban had ended.
- Staff panel sign-in: spaces around the name got around the per-account
  attempt limit; unknown names now take as long as wrong passwords.
- A free VIP month was recorded with the month's coins, which a reversal
  would have taken back without them ever being given.
- The settings file (it holds the session token) is created private from
  the start.
- Translations: texts starting with `#` were read as comments ("#3 in the
  world" stayed in English on the profile).
- Shop: a VIP perk that wrapped onto a third line overlapped the next one;
  long category names ran into their item count.
- Profile: the replay button covered the date of each game.

## 0.1.1-beta.4 (protocol 11)

### Game
- A detailed pool hall (panelling, damask paper, moldings, plank floor, rug,
  tin ceiling, bar with bottles and stools, racks, chalkboard, dartboard,
  posters, jukebox, neon) with baked lighting; turned table legs, rounded
  rail edge, apron lip; finer cloth on Ultra.
- No more freeze when cloth or balls change: textures are made on another
  thread and uploaded a few per frame.
- Shop: 98 items in 7 categories (new: frames, VIP name colours, winning
  effects), a clear confirmation (price, coins before/after), a success
  card, a rotating 3D preview on the table, gifts to friends.
- VIP crown in the ranking and profiles; profile rating chart, career
  numbers, abandon rate, replays, account dialog (recovery e-mail,
  password, deletion).
- Lounge: featured game ("on TV"), restart notice, invitation codes,
  tournaments with entry fees and the Arena format; system notifications.
- Translations moved to `lang/*.txt`; Spanish added.

### Server
- Accounts: change password, recovery e-mail (offered from checkout),
  deletion by anonymising (LGPD); matches no longer cascade on delete.
- Graceful drain on SIGTERM (`SNOOKERED_DRAIN_SECS`, `maxShutdownDelaySeconds`);
  games are saved after every stroke and continue on the next server.
- Replays (seed + strokes), per-match statistics, rating history.
- Coins for rated games (daily cap), daily bonus, anti-farm for repeated
  pairs; tournament fees and prizes (70/20, 10% house); weekly cup
  (`SNOOKERED_CUP`); VIP 16-player brackets and waiting-list priority.
- Staff panel: metrics (DAU, games, retention, funnel, revenue, queue
  wait), aim-assistance review queue, replays attached to reports.

## 0.1.1-beta.3

### Distribution
- Releases are built and published by dist: a Windows installer (`.msi`,
  with Start menu and desktop shortcuts and an uninstaller), portable
  `.zip`/`.tar.xz` packages for Windows, macOS (Apple Silicon and Intel) and
  Linux (x64 and ARM64), and one-line installers for the terminal (shell and
  PowerShell). The server image has its own workflow.
- A logo: a pool table seen from above with an "S" ball. It is the window
  icon, the executable's icon (with version details on Windows), the
  installer's icon and pictures, and it appears on the menu, in the lounge
  and while the game loads.

### Graphics
- The balls' numbers are drawn with the game's bold font, anti-aliased (a
  5x7 bitmap font looked blocky up close); 6 and 9 are underlined, as on
  real balls; the edges of the number circle and the stripes are smooth.
- The cue casts a soft shadow on the cloth and the balls from every lamp
  (High and Ultra).
- The leather around the pockets was lit from below (its faces pointed
  down) and looked dark; the pocket walls were lit from the outside.
- A development view (`SNOOKERED_DEBUG_NORMALS=1`, debug builds) paints red any
  face lit from behind; tests check the shapes the table is made of.

### Fixes
- Buying coins or VIP on Windows opened Stripe's "This link is incomplete"
  page: the game opened the address with Explorer, which drops everything
  after `#`, and Stripe needs that part. It is now opened with
  ShellExecute, which passes the whole address.
- Text never runs past the edge of the panel it is in: it is made a little
  smaller and, if that is not enough, cut with "…" ("Computador (Difícil)
  vence!" used to overflow the game-over panel).
- Installing a newer beta over an older one with the .msi kept the old
  game: Windows Installer ignores the fourth part of a version (the beta
  number), so it saw the same version and left the old snookered.exe, whose
  file version was also the same. Newer builds now replace older ones of the
  same version, and snookered.exe carries the beta number in its version.
- The bottom and right cushions were darker than the others: their faces
  were wound the other way, so the light hit them from behind.
- The side pockets showed a black trapezoid between the jaws: the cloth
  was cut away there, opening onto nothing. It now runs on to the hole and
  rolls over its edge, as in the corner pockets.
- Hover hints (tooltips, achievement cards, "Shoot!", the power hint) are
  drawn after everything else, so panels, buttons, chat and toasts drawn
  later no longer cover them ("Find an opponent…" used to hide under the
  rated/casual switch, achievement cards under the match history).
- A release built with an empty `SNOOKERED_SERVER` (the CI variable not set) no
  longer ships without a game server: it keeps the public one.

## 0.1.0

First public version.

### Play
- Eight-ball with real ball physics, a computer opponent (three levels), two
  players on one computer, and games on the local network (host from the
  game; others find it automatically or join by address).
- Offline setup screens: names, the computer's level, the aiming guide and an
  optional shot clock.
- Replay of the last shot, a slim zoom bar, the top view as one icon, and
  right-dragging to turn the view.
- Text fields: arrow keys, Home/End, Delete, Ctrl+arrows, Ctrl+A, click to
  place the caret, keys repeat when held.

### Online
- The shop: cloth colours, cues, chalk and ball sets. Some are free, some
  come with achievements, some are bought with coins, some are VIP. Coins
  and the monthly VIP are paid through Stripe (card, Pix, boleto) on its
  hosted page; the server keeps the only wallet, credits each payment once,
  takes refunds back, and logs every staff change. Players see each other's
  cue and chalk, and a crown by VIP names.
- Play now: a queue that pairs players by rating, widening with time.
- End-of-game numbers: potting success, balls made, best run, fouls, ball
  in hand.
- Friends see when the other was last online.
- Watch a friend's game: an eye next to them in Friends (or in the players
  list) opens it; the players see an eye with how many are watching.
  Spectators follow the friend into the rematch, can't read the players'
  chat and can send reactions, which reach the table small and faint.
- Reactions during a match: 4 for everyone; the other 16 come from hard
  rated-game achievements (100 games, 50 wins, 10 in a row, rating 2100…)
  and from winning a rated game during an event (Halloween, the holidays,
  New Year). They pop up discreetly by the sender's card.
- Progress is earned slowly: smaller rating changes (K 24 for the first 10
  games, then 14, 8 from 1900), everyone starts as a Rookie, feats (8-ball
  wins, runouts, shutouts) count only in rated games. Games against the
  computer never
  touch ratings, numbers, achievements or reactions.
- The 8-ball pocket is called by clicking a pocket on the table (no panel),
  and the call is binding for the rest of the game.
- With the chat folded, new messages show a pulsing dot and a short preview.
- No full aiming guide against people: the standard one (ghost ball) or the
  short line, in rated and casual games alike.
- The game points at the public server out of the box.
- Shots play instantly for both players (the shooter's is predicted locally;
  the server's result is authoritative).
- Fixed: a game won on the 8-ball was shown to the loser as a resignation.
- Accounts with a country flag; names coloured by rating tier; profiles with
  numbers, achievements (37, three of them events), the reactions unlocked
  and progress towards the next tier and match history; the world top 100.
- Open tables (public or private with a password), invitations, tournaments
  (4, 8 or 16 players, public or private), friends, blocking and reporting.
- The game's events appear in the match chat, quieter than what people write.
- The game signs in in the background; the menu shows who is signed in.

- After a foul on the break the cue ball always goes back behind the head
  string (it used to stay where it stopped, and no shot was allowed until
  the player moved it).
- Esc while charging a shot only cancels the shot (it used to open the menu
  as well).

### Server
- Before launch: limits on lounge actions and database requests, no ghost
  tables or queue places after signing in elsewhere, tournaments abandoned
  by their host are removed, VIP refunds and first payments handled in any
  order of events. See `docs/SECURITY.md` (14–20).
- PostgreSQL storage (Neon-ready) with migrations; in memory for LAN and tests.
- Hardened: hashed and renewed sessions, sign-in and sign-up throttling,
  flood and connection limits, bounded queues, simulation off the main lock,
  bans and mutes. See `docs/SECURITY.md`.
- Staff panel at `/admin` with an audit log. See `docs/ADMIN.md`.
- Docker image, Render blueprint, CI (PostgreSQL, image, dependency audit)
  and a release workflow.
