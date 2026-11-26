# UI/UX review (v0.1.0)

What was reviewed in the game before the release, what changed and why.

## Start and menus

| Before | Problem | Now |
|---|---|---|
| The menu asked for the player's name, the second player's name and the level, all at once. | Too much at once; most of it irrelevant to what the player wants to do. | Four large choices (online, computer, two players, local network) and a row of icons (ranking, profile, settings, quit). Names, level, aiming guide and shot clock are asked on a setup screen for the chosen mode only. |
| Online status was invisible until entering the lounge. | No sense of identity or progress. | The player's card (flag, name in their tier colour, rating and tier) is always top right; the game signs in in the background when the session is saved. |
| The first connection to a sleeping server looked frozen. | Seemed broken. | A clear "the server is waking up" note with a spinner. |

## Lounge

- Tabs for **players**, **open tables**, **tournaments** and **friends**;
  rated/casual is one switch that applies to invitations and tables.
- Names are coloured by rating tier everywhere (lounge, chat, ranking,
  brackets, profiles), with the country's flag: status reads at a glance.
- Status dots (free / at a table / playing), "invite" disabled when it can't
  work, a red dot on Friends when a request arrives.
- Private tables and tournaments with a password: the password is asked
  inline, not in a dialog.
- Toasts (achievements, declined invitations, notices) stack at the bottom
  and never cover controls.

## At the table

- **Camera**: the "3D view / Top view / Reset camera" buttons became one
  icon (a camera with a back arrow: top view and default zoom, `C`). Only
  the right button turns the view (and goes 3D); the left button is only
  for aiming, so the camera never moves by accident.
- **Zoom bar**: a slim bar on the right edge, almost invisible until the
  pointer comes near, with + and −.
- **Replay**: a small icon next to the camera icon (and `R`) replays the last
  shot without touching the game; a discreet "REPLAY" tag shows it is one.
- **Chat**: online, what happens in the game (pocketed balls, fouls, ball in
  hand…) appears in the same chat, smaller, dimmer and without a name, with
  fouls in a muted red: easy to tell from what people write.
- **Calling the 8**: clicking a pocket on the table calls it (the pockets
  glow and the status line says so). The panel that used to cover the left
  of the table is gone. Under the Brazilian rules the call is binding for
  the rest of the game; under the official rules each shot at the 8 is
  called the same way, afresh. No shot at the 8 until a pocket is called.
- **Folded chat**: a pulsing dot on the Chat button and the newest message
  for five seconds above it, so nothing is missed without opening it.
- **Reactions**: a small button by the chat opens a grid of pictures (locked
  ones in grey, with what unlocks them). A reaction pops up by the sender's
  card for under three seconds and stays visible while aiming.
- **Aiming guide online**: never the full guide against a person (it plays
  the cut for you): the standard one (ghost ball) or the short line, rated
  or not.
- **Shot clock** offline too (optional), on the player cards.
- The deciding shot is always shown before the result.

## Responsiveness

- Your own shot starts the instant you press the button (no round trip); the
  opponent's starts as soon as the server announces it; the opponent's cue
  glides instead of jumping 10 times a second.

## Watching friends

- An eye replaces "Invite" next to a friend who is playing. The game opens
  as for a player, but the bottom bar only says who is being watched, the
  reactions button and "Stop watching".
- Players see an eye and the number of spectators by their cards (hover for
  "3 watching"). Reactions from spectators are smaller and fainter than the
  players' own and drift down from the eye, a few at most.

## Review before launch

- Panels are almost opaque: the table behind no longer muddies the text.
- Settings use the same segmented controls as the setup screens (the old
  radio rows overlapped: "Off" ran into "2×").
- An empty lounge offers "Open a table" right there.
- The profile shows how far the next tier is and the reactions collected.
- The menu's left side is darker, so the buttons read over the table.
- Pocket castings are polished brass instead of flat cream plates.

## Text fields

Every field has a caret: arrows, Home/End, Delete, Ctrl+arrows (by word),
Ctrl+Backspace, Ctrl+A, Ctrl+C/V, and a click places the caret. Held keys
repeat. The LAN name field no longer refills itself when emptied.

## Accessibility and polish

- Interface scale follows the window, with a manual size in settings.
- Disabled buttons look disabled; every icon has a tooltip.
- English, Brazilian Portuguese and Spanish for every text, including server messages,
  achievements, countries (249) and match history.
- Gradients are textures (no banding); rounded rectangles no longer allocate
  every frame.

# UI/UX review (after 0.1.4)

Every screen was captured in English, Portuguese and Spanish at 1024×600,
1280×760 and 1920×1080 (and at large interface sizes), then fixed until
nothing overlapped, clipped or was cut short.

## Toolkit

- Panels, buttons, tabs and toggles are shaded with per-vertex gradients;
  the old gloss was a hard-edged band that ran through titles and labels.
- Disabled buttons are flat and quiet; pressed buttons sink a pixel.
- One spinner for everything that waits; panels are opaque, so a dialog
  over a panel never lets the text behind show through.

## Screens

| Screen | Before | Now |
|---|---|---|
| Settings | Fixed height (the Sound tab was one slider in a tall empty panel), a different layout per tab, "Português (Br…", ragged checkbox columns, a dev-looking server field. | Sections of rows on every tab, sized to the content; switches; short language names; the server under "Advanced". |
| Sign in | "Entrar" three times (title, switch, button), a gap above a small "Back". | "Play online" with a back arrow; "I have an account / New account"; one green action; Enter and Esc. |
| Lounge | Plain text for empty lists, a player card twice as wide as its content, toasts cut to "Voc…". | Centred empty states with the next step, a header that fits, toasts that wrap. |
| Shop | "Buy · 1000" with 10 coins; a big green "More items" plus a scroll bar; gift/try-on icons over the picture. | Price with how much is missing, in red; one pager; the buttons under the picture. |
| Profile, ranking | Misaligned tier pill, emoji touching, columns crammed on the right. | Shared baselines, even grids, spread columns, your own row highlighted. |
| At the table | The pause menu let the HUD behind react; the cue settings showed after the game and in replays. | Modal pause and help; the bottom bar only offers what can be used. |

## Keyboard

Esc steps back everywhere (closing a dialog before its screen), Enter
submits forms and starts a game from the setup screen, and the country
list works with the arrows and Enter.

# UI/UX review (0.12.0)

## Screenshots of every screen

`tools/screens.py` renders the screens offscreen (Xvfb and Mesa's software
renderer) against a local server full of made-up players
(`crates/server/examples/ux_seed.rs`: the account `Ana` with VIP, coins,
items, achievements of every metal, friends, messages, an inbox and a
history; others in the lounge, an open table and a tournament):

```sh
export CARGO_TARGET_DIR=target   # any target dir; a debug build is needed
cargo build -p snook-legend
cargo build -p snook_legend_server --example ux_seed
python3 tools/screens.py --out /tmp/shots                     # everything
python3 tools/screens.py --only menu,profile-achievements --langs en,ar --sizes 1024x600
```

Pictures land in `<out>/<size>/<lang>/<screen>.png` (default languages
en, pt-BR, de, ar; sizes 1280×720, 1920×1080, 1024×600). The list of
screens is at the top of the script; it relies on development switches
that release builds ignore (`--screen inbox|doubles|clock|toasts|dm:<name>|
ranking-friends|ranking-doubles|auctions|credits`, `SNOOK_LEGEND_OPEN`,
`SNOOK_LEGEND_INTRO`, besides the older ones).

The medals alone: `cargo test -p snook-legend export_medals -- --ignored`
(sheets at 32, 64, 128 and 288 px in `$SNOOK_LEGEND_MEDALS_OUT`).

## Changes

| Where | Before | Now |
|---|---|---|
| Achievement medals (profile, collection of tournament medals, the new achievement card) | Flat circles and triangles: a disc, a star, a plain V ribbon. | Rendered like jewellery (`medal_art.rs`): a struck disc with a rounded milled rim, a guilloché field and a ring of beads, an embossed faceted star (a cut diamond in platinum, a ruby in gold), lit by a studio environment with specular highlights, ambient occlusion and a soft drop shadow; a striped grosgrain ribbon with a satin sheen and the ring that holds it. Cached per size, with fine detail only where it reads. |
| Achievement unlocked | A one-line toast. | A card at the top with the medal (it settles with a small overshoot), the metal, the title and what it was for; a click puts it away. |
| Tournament medals (against the computer) | A medal icon in a flat colour. | The same gold, silver and bronze medals. |
| Settings → Game | "Show reactions" (General) and "Hide the opponent's reactions" (Chat and reactions): two switches for one thing. | One "Reactions" choice in Chat and reactions: All / Hide opponent / Off. |
| Profile → Achievements | On short windows the reactions ran out of the panel. | Medals and reactions shrink together to fit the panel at every size. |
| Main menu | Crashed with seven places along the bottom (hover state for five). | Fixed. |

# UI/UX review (0.12.1)

## Screenshots: every entry works

`tools/screens.py` now runs all its screens (65 of them) in every
language and size. What was wrong with it, and what changed:

| Before | Now |
|---|---|
| The documented `cargo build -p snook-legend -p snook_legend_server --example ux_seed` built the examples only: no game binary, every screen failed. | Two commands (above). |
| One server for all workers: Ana signed in on two screens at once signed herself out of the first, so most online screens showed the sign-in page. | A seeded server per worker (`--jobs`), on ports from `--port` (two runs at once need different ones). |
| Signing in adopted the account's language: online screens were in English whatever `--langs` said. | The settings written for the shot are newer than the account's, so they stay. |
| Ana's pinned message from the staff and the items her achievements brought covered every online screen. | Kept off them (`SNOOK_LEGEND_QUIET_INBOX`, `SNOOK_LEGEND_QUIET_REWARDS`), each on its own screen: `staff-message` (with the achievement card waiting behind it) and `reward`. |
| `auctions` and `credits` opened on empty pages. | The seed has auctions (with bids, one over) and supporters. |
| The seed's players were let go after 75 s idle: the lounge emptied during a run. | They ping the server. |
| `match-chat` and `match-reactions` were games against the computer, which have neither. | Real rated online games (and `match-online`) against seed players who take any invitation, greet the table and resign when the window closes. |
| `match-intro` held the intro at one moment only. | `match-intro` (all in), `match-intro-in`, `match-intro-vs`, `match-intro-out`: `SNOOK_LEGEND_INTRO=0.5s` holds it at that moment. |
| Hover states and tooltips depended on where Xvfb left the pointer. | The pointer is parked off the window for screenshots (`SNOOK_LEGEND_POINTER` keeps it). |
| `settings-advanced` showed the Graphics tab. | The Game tab, scrolled to Advanced. |

## A promotional video

`tools/video.py` makes a short video of the game for social media
(1920×1080, 1080×1080 or 1080×1920, 60 fps, H.264, with the game's own
sounds), in English or Brazilian Portuguese: a
title card, a game against Viktor from its introduction, the break and a
run of pots under the broadcast camera, the main menu, and an end card
with where to get the game.

```sh
cargo build -p snook-legend
python3 tools/video.py --out /tmp/promo                  # 1920x1080
python3 tools/video.py --out /tmp/promo --format square  # 1080x1080
python3 tools/video.py --out /tmp/promo --lang pt-BR --format square,vertical
```

`--lang` sets the game's language (the match's introduction, Viktor's
words, the menu, the ranking) and the cards'; the files of a language
other than English carry its code (`snooklegend-promo-pt-vertical.mp4`).
The vertical frame (Reels, Stories, a WhatsApp status) is recorded at
1080×1920 with the interface at its largest size; the broadcast camera,
in a window taller than wide, widens its lens to see across as much of
the table as in a square one.

The game records itself. With `SNOOK_LEGEND_RECORD=<file.mp4>` each frame
moves the game's clock on by exactly 1/60 s, however long it took to draw
(Mesa's software renderer draws a few a second), and goes to `ffmpeg`
through a pipe, with no picture per frame on the disk; the sounds the
game plays are mixed at their frames into `<file>.wav`.
`SNOOK_LEGEND_RECORD_FRAMES=<n>` frames are recorded (600 by default),
after `SNOOK_LEGEND_RECORD_FROM=<n>` frames played first, then the game
quits. The clock is miniquad's (`date::set_fixed`, a hook in
`vendor/miniquad`), so everything that reads `get_time` or
`get_frame_time` follows it. `SNOOK_LEGEND_AUTOPLAY=1` lets the computer
play the person's seat in a game against the computer too (it already did
online). Like every development switch, release builds ignore them.

The cards are `tools/video/cards.html` (the wordmark, Inter, the store
art), drawn frame by frame by Playwright's Chromium
(`tools/video/cards.js`), laid out for the frame's shape, their words in
the page by language. The cut and its crossfades are a list at the
top of the script, timed for its default seed: another seed plays another
game and needs other cut points.

## Changes

| Where | Before | Now |
|---|---|---|
| Match intro | The "VS" (46 px, 62 at its bounce) spilled out of its 68 px circle; on narrow windows the badge sat over the cards; "You" touched the picture; a VIP's crown pushed the name off centre; skipping cut it dead. | A ringed medallion that always fits the gap, the word fitted inside it in every language. A sequence: the table dims, the cards slide in from their sides a beat apart with a hint of overshoot, the VS lands from large with a bounce and a ring of light, the stakes and the rules rise in; going (or skipped) the cards part a little as everything fades. Names shrink to fit before they are shortened. |
| Achievement card | Drew over the staff's message and any other dialog; its medal shrank as it went. | Waits, unseen and with its clock stopped, while a dialog is up; the medal fades with the card. On the main menu it comes down beside the title, under the player's card, never over the game's name. |
| Diamond medal | The stone read as a pale grey-blue disc at mid sizes. | A round brilliant's real layout (table, star facets, kites, girdle facets) in strong light and dark, a little saturated fire, a dark girdle against the setting and a glint never thinner than a pixel or two: it sparkles at 32 px too. |
| Main menu | Toasts covered the row of places. | They centre in the room right of the menu's column. |
| Two players | A Back button at the bottom, unlike every other setup screen. | The back arrow by the title; Play as the single, full-width action. |
| Daily challenge, Against the clock | "Nobody has played yet" in a line of small text at the top of a tall, empty panel; buttons narrower than the column of numbers above them. | The empty state of every other list (centred, with its icon); full-width actions. |
| Segmented controls (settings, setup) | Equal cells: a long option ("Gegner ausblenden", "Hide opponent") shrank to a whisper beside "All". | Cells as wide as their labels when they don't all fit, one type size for the whole control. |
| Settings, notes, rules | Rows cut in half at the scroll's edges. | The list fades into the panel at the edge that has more. |
| Profile → Rating | League names sat under the rating-now chip and the rules tag. | Names at the left, the chart under the tag. |
| Profile → Numbers | "No games yet." to players with games (the pane counts shots). | "No shots yet." |
| Shop → VIP | "0 coins every month" before the server said how many. | The line waits for the number. |
| Chalk (shop, collection, rewards) | A flat green square. | A cube in its wrapper, lit from the top left, the dish of the tip worn into it, with its shadow. |
| At the table: reactions | 101 reactions, 7 across, ran over the bottom bar and its buttons. | As many columns as needed to fit above the bar; the player's own first, those still to win after. |
| At the table: a recorded game | Its controls floated over the table's bottom cushion while the bottom bar sat empty. | In the bottom bar. |
| Game over | "Computer wins!" in the same celebratory gold as a win. | A win: gold, a trophy and a glow; a loss: plain. |

## Motion: scrolling, transitions, feedback

Navigating felt hard: lists jumped by whole rows, screens and dialogs
appeared in one frame, selections snapped from one tab to another. Every
screen was gone through for it; the fixes are in the toolkit
(`ui/motion.rs`, `ui/scroll.rs`), so every screen has them.

| Where | Before | Now |
|---|---|---|
| Lists (lounge, ranking, profile, inbox, gifts, friends, daily games, LAN, countries, languages, pictures, supporters) | Scrolled by whole rows, three a notch: rows popped in and out; a touchpad's small moves became whole jumps; the bar only showed where it was. The supporters' credits never scrolled (two arguments swapped). | By the pixel, eased toward where the wheel sends it (a notch is 96 units, notches add up; there in about 0.2 s, the same at any frame rate; a touchpad's small steps scroll a little). Rows cut by the edges are clipped and fade into the panel. Dragged with the pointer, a flick carries on and slows down, past an end it resists and springs back; a press that moves is a drag, not a click. The bar's thumb is dragged (a press on the track goes there), widens under the pointer, moves with the list. Arrows, Page Up/Down, Home and End for the list under the pointer (or the only one on screen). The ranking's own row slides into its pinned place at the bottom. |
| Settings, notes, rules | 48 units a notch, at once. | The same smooth scrolling. |
| Chats (lounge, table, messages) | Three rows a notch, at once; new lines pushed the others up in one frame; reading back, a new line moved what was being read. | Eased; a new line slides up into place; reading back, the view stays put. Its edges clipped and faded. |
| Screens | Faded in over 0.2 s; the darkening behind the pages faded in again on every page, so the table flashed between two pages. | Fade in rising 14 units and growing from 98.5 % (0.2 s, eased out); the darkening stays from one page to the next (it fades in from the main menu only). |
| Dialogs (purchases, gifts, account, settings, what's new, pickers, invitations, pause…) | Appeared in one frame, dimming included. | The dimming fades in (0.15 s); the dialog fades in growing from 95 % and rising 10 units (0.18 s, eased out). |
| Popovers (emoji, phrases and reactions at the table, spin and elevation) | Appeared in one frame. | Grow out of their button (0.13 s). |
| Tabs | The selection and its underline jumped. | The underline glides to the tab chosen; colours cross-fade; the content slides in 16–18 units from the tab's side (lounge, profile, settings, inbox, ranking, against the computer). The shop's and the collection's categories: the plate glides down the list, the page rises in. |
| Segmented controls, the rules' switch, switches | The selection and the knob jumped. | The selection's plate and the knob slide; colours follow (the rules' switch turns from brown to blue on the way). |
| Shop and collection pages | The next page replaced the last in one frame; a touchpad's stream of small turns turned pages at random. | One grid that scrolls down (see below). |
| Panels that change size (profile panes, settings tabs) | Jumped to the new height. | Grow or shrink to it. |
| Buttons, rows | Only some lit up eased; pressing moved the label a pixel. | Light eases in and out everywhere in the toolkit (buttons, icon buttons, tabs, toggles, rows of the lounge, ranking, inbox, the shop's categories); pressed, a button sinks (a little smaller and darker), eased; a press that began elsewhere doesn't press it. |
| Toasts | Popped in; the stack jumped. | Rise in and fade in; those above glide up. |
| Pointer | Always the arrow. | A hand over what can be clicked, the text cursor over fields. |
| Back | A profile opened from the ranking (or the inbox), the shop opened from the inbox, went back to the main menu: the place in the list was lost. | Back (and Esc) goes back there, the list scrolled where it was. |

Settings → Game → **Reduce motion** (follows the account): what slides,
grows or glides is there at once, scrolling included; colours still ease.

For code: `Ui::anim`, `glide`, `glide_rect`, `appear` (a small store keyed
by a hash, forgotten three frames after the widget was last drawn);
`Scroll` with `list_begin`/`scroll_begin`/`scroll_end`; `push_fx`/`pop_fx`
and the scoped `modal_scope`, `switch_scope`, `popover_scope`;
`switch_in`, `switch_down`, `come_in`, `take_wheel`, `selection_glide`; `motion::push_clip`/`pop_clip` (scissor,
nested, following what moves); `motion::want_cursor`.

`tools/screens.py --input "<script>" --shots <frames> [--frames <last>]`
takes a screen frame by frame at a fixed 60 frames a second
(`SNOOK_LEGEND_INPUT`, `SNOOK_LEGEND_DT`, `SNOOK_LEGEND_SHOTS`), to see
what moves: `--input "180:move 640 450;182:wheel -1"` scrolls the
ranking, `"170:move 618 334;172:down;173:up"` opens a purchase.

What moves is drawn through the batcher's model matrix, never a camera
change: a camera change flushes what was drawn, and an extra flush in a
frame let the bundled miniquad's texture cache (its `delete_texture`
leaves the cache as it was) draw the bold font's atlas black at some
sizes.

## Down, not sideways; going out; the keyboard

"Instead of going sideways I want it to go down": the shop's and the
collection's cards turned pages sideways (‹ ›, "Items 1–8 of 23"). Every
screen was looked at for paging: those two were the only lists or grids
that paged; the medal and reaction close-up (previous and next of a single
card) and the showroom's settings stay sideways, as carousels of one thing.

| Where | Before | Now |
|---|---|---|
| Shop and collection | Pages of cards, turned with ‹ ›, the wheel or ← →; "Items 1–8 of 23" under them. | All of a category (as filtered) in one grid that scrolls down with the lists' smooth scrolling (wheel, touchpad, drag and flick, the bar, the keys); the rows at the edges clipped and faded; a quarter of the next row shows when there is more. Back at the top when the category or a filter changes; on the item when coming back from the 3D preview. The counter is gone (the category list counts the items). |
| Dialogs closing | Gone in one frame, the dimming too. | The last look of the dialog fades out shrinking a little (0.14 s, speeding up) while the dimming lifts (0.15 s). Every dialog drawn through `modal_scope`/`dialog_scope` (about twenty five) has it. What is under it works at once. |
| Hand-drawn cards and rows (shop and collection cards, filter chips, history rows, medals, reactions, gift picker, pickers, LAN games, ladder and drills cards, rules badges and cards…) | Lit at once under the pointer. | Ease in and out like the toolkit's widgets. |
| The mouse's back button | Dropped by the bundled miniquad. | Back, like Esc, on every screen and dialog; not at the table during a match. |
| Keyboard | Tab moved between text fields only. | Tab and Shift+Tab go through the buttons, tabs, switches, rows and fields of the screen (or of the dialog on top) in the order they are drawn; Enter or Space presses the one focused; a row of tabs or a segmented control is one stop (its chosen tab) and the arrows go along it, choosing; a focused row of a list is scrolled into view; an icon button focused shows its hint. A gold ring shows the focus, gliding from one to the next, only while the keyboard is used (the pointer hides it). Off at the table during a match (Tab, Space and the arrows are the game's). |

How a dialog goes out in immediate mode: a closed dialog's code is not run
any more (often its state is gone the same frame), so each open dialog's
panel is copied from the screen at the end of every frame into a texture
of its own (`glCopyTexSubImage2D`, a few hundred pixels, on the graphics
card; `ui/ghost.rs`); the frame a dialog is no longer drawn, that picture
goes out where it was. Where the dialog is: the panels it draws. The extra
flush this takes is safe now that the bundled miniquad forgets a deleted
texture in its cache (vendor/README.md). Reduce motion: closed at once.

How the focus works in immediate mode (`ui/focus.rs`): what can be pressed
is noted as it is drawn (`released_in`, `clicked`, `activated`, which every
widget uses; text fields too), keyed by where it is (in a scrolled list,
where it is in the list, so it keeps the focus as the list scrolls); the
keys of a frame act on the last frame's list. A dialog starts a new list;
what is drawn with the input suspended (a page under a dialog) takes no
focus. Enter pressed on a focused button is not the screen's Enter too
(`Ui::enter`).

For scripts: `SNOOK_LEGEND_INPUT` also presses keys (`key tab`,
`key shift+tab`, `key enter`, `key space`, `key escape`, the arrows) and the
mouse's back button (`back`).

## Messages, the history, the analysis, the cards

| Where | Before | Now |
|---|---|---|
| Inbox | Notifications only; private messages between friends, from the friends list in the lounge. | A "Messages" tab first, with the unread count: the conversations on the left (name, the newest line, "You: …" for one's own, the date, a count), the one picked on the right (its name, "Profile", the messages and the field); "New message" in the header (a name, "Write"). Narrow: the list, or the conversation (Esc goes back to the list). It opens on the messages when some are unread. The menu's and the lounge's inbox counts include them. |
| Another player's profile | Report, block, friend, daily game. | The same, and a speech bubble that opens the conversation in the inbox (with the unread count on it). |
| Who can write | Friends only. | Friends; and adults to adults who keep "Messages from anyone" on (Settings → Game), three messages until the other answers, ten new conversations a day. Minors: friends only, both ways. The server's refusals are toasts. |
| Online game chat | The players' lines and everything that happened at the table, mixed. | The players' lines only; the history's icon in the top bar (as offline) opens the game's events in the chat's place. Settings → Game → "Game events in the chat" (off) mixes them again. |
| Profile, analysis | "Analysis" (anyone's profile) and "Advanced" (one's own, VIP) showing the same pockets and cut angles twice. | One "Analysis" tab on one's own profile, VIP: the table and heat map, kinds of shot with the league's mark on the cut angles, safeties and breaks, distances, the weeks and the pockets furthest ahead of the league and behind it, the league's rate on top. Not VIP: the page dimmed with made-up numbers under the way to VIP. |
| Players' cards (the score panel, the match intro) | A gem and a glow for an epic or legendary cue, the gem on the start of the name. | No cue on the cards. |
| Main menu, the row at the bottom | Seven icons and Quit; with that many the names didn't fit and were left out, icons alone. | Five, each with its name: Ranking, Profile, Inbox, Shop (the collection is in its header; "Shop and collection" on hover), Settings, and Quit apart. The rules moved to Settings → Game → About (with the terms, the privacy policy and the credits); the "i" by the rules switch still opens them. In a narrow window the names get smaller and closer, then Quit loses its word, before the names go. |

## Still to do

- Arabic is shaped and ordered right, but laid out left to right (labels
  on the left, controls on the right, chevrons pointing right): a mirrored
  layout for right-to-left languages is a larger piece of work.
- Sliders take no keyboard focus (they are dragged); the arrows move the
  focus only along rows of tabs, not up and down a list.
- A dialog going out on macOS with the Metal renderer (not the one the game
  uses) would close at once.
