<p align="center"><img src="crates/client/assets/icon/snook-legend-256.png" width="128" alt="Snook Legend"></p>

# Snook Legend

A lightweight 3D eight-ball pool game for the desktop, written in Rust. It plays
eight-ball under **two rules**, chosen in the game: a **regional Brazilian rule**
(bar rules with penalty balls) and the **official rules** of the World
Pool-Billiard Association (WPA), each with its own rating and ranking.

Snook Legend is built around what made the old online pool rooms special: a realistic
cue (spin, draw, follow, jump and massé shots), a lounge where you meet people and
chat, invitations, open tables and tournaments, friends, achievements, and a
world ranking that keeps everyone coming back.

| Menu | 3D view |
|---|---|
| ![Menu](docs/screenshots/menu.png) | ![3D view](docs/screenshots/3d-view.png) |

| Lounge | Profile |
|---|---|
| ![Lounge](docs/screenshots/lounge.png) | ![Profile](docs/screenshots/profile.png) |

## Download

- **Windows**: get it from the
  [Microsoft Store](https://apps.microsoft.com/search?query=Snook%20Legend)
  (free; the Store installs and updates it).

Or get the latest version from the
[Releases page](https://github.com/allansomensi/snook-legend/releases/latest):

- **Windows**: `snook-legend-x86_64-pc-windows-msvc.msi` installs the game with a
  Start menu (and optional desktop) shortcut; the `.zip` is a portable copy.
- **macOS** (Apple Silicon or Intel) and **Linux** (x64 or ARM64): the
  `.tar.xz` for your computer, or install from a terminal:

  ```bash
  curl --proto '=https' --tlsv1.2 -LsSf https://github.com/allansomensi/snook-legend/releases/latest/download/snook-legend-installer.sh | sh
  ```

- **Windows from PowerShell**:

  ```powershell
  powershell -ExecutionPolicy Bypass -c "irm https://github.com/allansomensi/snook-legend/releases/latest/download/snook-legend-installer.ps1 | iex"
  ```

On Linux the game needs the usual desktop libraries (ALSA, X11, OpenGL),
which every desktop installation has. How releases are made is described in
[docs/RELEASING.md](docs/RELEASING.md).

## Features

- **Real ball physics**: sliding → rolling transition, side spin (English) that
  changes cushion rebounds, draw and follow, squirt, throw, ball-to-ball friction,
  and balls that can leave the slate for **jump shots**. Contacts are found
  continuously (a 0.02 mm cut at full power still counts), and the simulation
  gives the **same bits on Windows, macOS and Linux**, so the server and the
  players always agree (pinned by a fingerprint test in CI).
- **Full cue control**: aim, power, the exact point where the tip hits the cue ball,
  cue elevation, and fine aim right on the bottom bar.
- **Two rules, chosen once and switched at any time**: the first start asks
  how you like to play (a card for each rules, with how many play them now);
  the main menu, the lounge, the screen against the computer, the two-players
  setup and Settings → Game switch them. Every game says its rules (on
  invitations, tables, "Play now", tournaments, daily games and replays, and
  at the table a badge that opens them), and each rules has its **own rating,
  tier, peak and world ranking**; the level, coins and achievements are
  shared. The rules are in the game (Menu → Rules), a tab for each, in three
  languages. Every rule has its own test, and random games of both are
  checked after every shot.
- **The Brazilian rule** (the rule of bars and pool halls, with penalty
  balls): a foul passes the turn and gives the opponent balls
  (the lowest numbers of their group come off the table: one for a miss, a
  scratch or the cue ball off the table, two for hitting the opponent's ball first, one if the cue ball
  went to a cushion before it, and they add up; with fewer balls left than
  the foul gives, the 8 pays the rest and the foul loses; the shot clock
  running out gives a ball too, with the cue ball back on the head spot), the cue ball is in hand
  on the break only (one that goes down or leaves the table comes back to
  the head spot), a break that loses the cue ball with no ball made, or
  with balls of both groups, passes the break to the opponent (from the head
  spot, not in hand), as does one that hits no ball (cue ball in hand
  behind the head string), the break alternates (one each), the shot clock
  on the break passes it too, the first ball made decides the groups (one of the
  opponent's with it passes the turn), a ball of one's own knocked off the
  table comes back and one of the opponent's stays off, the 8 alone on the
  break wins and the 8 with any other ball loses, a called pocket for the 8
  that is binding once made.
- **The official rules (WPA)**, as in international tournaments: every shot
  but the break is called (the called pocket follows your aim, a click calls
  another, and **Safety** calls nothing), a legal break makes a ball or drives
  four balls to a cushion, the break never decides the groups, the choices
  the break can leave (play the table as it lies, rack and break again, the
  other player breaks, the cue ball in hand behind the head string; the 8 made
  on the break is spotted), **ball in hand anywhere** after any foul (no
  penalty balls), the cue ball in hand behind the head string must cross it
  before hitting a ball behind it, and the 8 lost on a foul, too soon, in a
  pocket not called or off the table. The shot clock running out is a foul.
- **Computer opponents**: fifteen named opponents with their own portraits and
  styles (cautious, all-out attack, bank artist, positional…) over three skill
  levels. They play with the same physics and either rules (calling their
  pockets and making the break's choices under the official ones): they simulate candidate
  shots and score the outcome. **The tour** takes you through all fifteen,
  each tighter than the last, and its final win against the champion (checked
  by the server, stroke by stroke) earns an exclusive cue. **Tournaments**
  against them (a knockout or an arena of sixteen) give gold, silver and
  bronze medals, kept in the account and shown with the tournaments.
  **Ghosts** play in the measured style of a real player. Games against
  the computer never touch ratings or coins.
- **Daily challenge and drills**: the same position for everyone each day,
  with the day's ranking, and **24 drills** (straight pots, cuts, draw and
  follow, banks, the plant, the carom, snooker escapes, runs of ten and
  fifteen…) with up to three stars each, kept in the account.
- **Daily games**: correspondence games with up to 72 hours per turn, live
  notices and desktop notifications, several at once, rated or casual.
- **Online lounge**: accounts with a country flag, players online with names
  coloured by rating tier, lounge chat, **Play now** (a queue that pairs
  players by rating, widening with time), invitations to **rated** or
  **casual** games, **open tables** (public or **private with a password**),
  **tournaments** (4, 8 or 16 players, public or private, with a live
  bracket), **friends** (with "last seen"), blocking and reporting, chat with
  the opponent (with the game's events in the same chat), rematches, a shot
  clock and reconnection after a dropped connection.
- **Watching friends**: an eye next to a friend who is playing opens their
  game; players see how many are watching. Spectators follow into the
  rematch and can send small, faint reactions.
- **Reactions** during a match: 4 for everyone, 16 more unlocked by hard
  achievements and events (Halloween, the holidays, New Year).
- **Instant shots online**: your shot plays the moment you take it (local
  prediction of the deterministic physics), the opponent's as soon as the
  server announces it; the server's result is authoritative.
- **Profiles and progress**: a rating card for each rules (rating, tier and
  how far the next one is, peak, world rank, rating chart), numbers, match
  history with the rules and rating changes, **50 achievements**, an **analysis** of how you pot (by pocket, distance and kind of shot, with a heat map), the
  reactions collected and the **world top 100**. Ratings move slowly (Elo,
  K 24 for the first 10 games, then 14, 8 from 1900).
- **End-of-game numbers**: potting success, balls made, best run, fouls and
  how often each player had ball in hand.
- **Social**: recent opponents, a status message on the profile, a
  "looking for a game" board in the lounge, and gifts that arrive with a
  message and a thank-you.
- **The shop**: cloth colours, cues, chalk and ball sets, shown on the table
  (opponents see your cue and chalk), each one first looked at closely in a
  **showroom**: alone on a dark stage, turning until you take the camera
  (the right button turns it as at the table, the wheel zooms to a point;
  ready views of a cue's tip and joint or a table's pockets, a ball of a
  set floating close up, tables and cloths in any hall). Some are free,
  some come with
  achievements, some cost coins, some are for **VIP** members. Coins and the
  monthly VIP are paid through Stripe (card, Pix, boleto) on its hosted page;
  in the Microsoft Store version, coin packs and a month of VIP at a time
  (nothing renews by itself) are bought in the Store itself.
  The server keeps the only wallet, confirms every purchase with the payment
  provider and credits it exactly once.
- **Cues that level up** with the online games you win with them, and
  **themed tables and halls** (beach bar, industrial loft, English club,
  mountain lodge, neon arcade, art deco lounge, skyline penthouse…).
- **Configurable shortcuts** and **automatic updates** (checked against their
  SHA-256, installed on restart) with a "What's new" window.
- **Local network**: host a game from the game itself (no server needed);
  others find it automatically or join by address.
- **Replay** of the last shot, a discreet **zoom bar**, a read-only game
  history offline, and a staff **portal** (Next.js, part of the
  website at `snooklegend.app/admin`, over the server's staff API: moderation, wallets,
  payments, metrics, audit log).
- **Aiming guides**: the full guide (where both balls go) is for offline
  practice; against people, the standard guide (ghost ball) or a short line.
  The head string can be kept on the table at all times (Settings → Game).
- **A cosy pool hall** around the table (cue racks, scoreboard, posters, wall
  lights), or a dark stage with only the table.
- **Graphics presets** (Low / Medium / High / Ultra; on the first run the game
  measures the frame rate and lowers the preset if needed). High and Ultra add
  soft shadows from every lamp, contact shading, glossy halls with shadows
  from the furniture and halos round the lights; Ultra adds reflections in
  polished floors and sharper textures. Plus anti-aliasing (off, 2×,
  4×, 8×) and full screen, so it runs on old laptops and looks sharp on gaming PCs.
- **Small and fast**: a single executable of about 20 MB that runs on OpenGL 2.1 / GLES 2, so it works
  on practically any PC, including old integrated GPUs. Textures and sounds
  (modal synthesis) are generated at start-up; the only bundled files are the
  fonts, the flag atlas and the reaction pictures. The interface scales down
  to small windows without anything spilling out.

## Terms of use and privacy

Playing online follows the [terms of use](https://snooklegend.app/pt/termos/)
([English](https://snooklegend.app/terms/),
[español](https://snooklegend.app/es/terminos/)) and the
[privacy policy](https://snooklegend.app/pt/privacidade/)
([English](https://snooklegend.app/privacy/),
[español](https://snooklegend.app/es/privacidad/)), on the game's website
(the [snooklegend.app](https://github.com/allansomensi/snooklegend.app)
repository); the game links to
them when signing up, in the shop and in Settings → Game → About. The chat
with other players (lounge, games, notes, status and gift messages) starts
off for everyone, as Brazilian law asks of games that children and
adolescents are likely to play (Lei 15.211/2025): the first time a player
writes, the game asks their age group, and 18 or older turns it on; under 18,
only a parent or guardian turns it on, with a declaration. Nobody is asked
anything who never writes to others. There are no loot boxes: every item is sold
at a price shown before buying. A server run by someone else publishes the
same pages with its own provider details (see [docs/DEPLOY.md](docs/DEPLOY.md)).

## Controls

Everything can be played with the mouse alone; the keyboard is optional.
The interface is available in 27 languages (Settings → Game; the system's language
is picked on the first start): English, Spanish, Portuguese (Brazil and Portugal),
French, German, Italian, Dutch, Swedish, Polish, Czech, Hungarian, Romanian, Greek,
Russian, Ukrainian, Turkish, Indonesian, Filipino, Vietnamese, Thai, Hindi, Bengali,
Arabic, Chinese (Simplified), Japanese and Korean. Arabic, Devanagari, Bengali and
Thai are shaped as a browser would (rustybuzz), and Arabic reads right to left.
A translation is one file, `crates/client/lang/<code>.txt` (plus the rules and
"what's new" in `crates/client/rules/` and `crates/client/whatsnew/`).

| Action | Mouse | Keyboard |
|---|---|---|
| Aim | drag on the table (the cue points at the mouse) or drag the cue stick; click a ball or spot | A / D, ← / → (speeds up while held) |
| Fine aim | the arrows on the bottom bar (one small step per click) or drag the dial between them; Shift + drag on the table | Shift + A / D |
| Quick aim | click a ball or any spot on the cloth | |
| Shoot | set the power on the bar, then press the green button with the cue and ball (bottom right) | hold **Space**, release |
| Spin | click/drag on the spin ball | ↑ / ↓ (follow/draw), Q / E (English) |
| Cue elevation | elevation slider | W / S |
| Ball in hand | drag the cue ball (any time while it is in hand; aiming and power work too) | B toggles, Enter confirms |
| Call a pocket | the official rules: the pocket the aimed ball is heading for is called (a ring on it); click another to call it, or **Safety** in the top bar to call nothing. The Brazilian rule: click a pocket for the 8-ball (they glow when a call is needed; binding once made) | |
| A choice after an official break | the buttons on the table | 1, 2, 3 |
| Camera | drag with the right button to turn the table (goes 3D; the left button never moves the camera); wheel or the zoom bar to zoom; the camera icon goes back to the top view with no zoom | Tab 3D/top, C reset |
| Replay of the last shot | the replay icon (top left) | R |
| Menu / list of controls | **Menu** button (the controls are listed there, never on the table) | Esc / H |
| Full screen | | F11 |

Settings are saved in `%APPDATA%\Snook Legend\settings.ini` (Windows),
`~/Library/Application Support/Snook Legend` (macOS) or `~/.config/snook-legend` (Linux). A folder from before 0.4.0 is moved
there the first time the game starts.

## Building

Requires a recent stable Rust toolchain (see `rust-version` in `Cargo.toml`).

On Linux, install the system libraries for windowing and sound:

```bash
sudo apt-get install libasound2-dev libx11-dev libxi-dev libgl1-mesa-dev
```

Run the game:

```bash
cargo run --release -p snook-legend
```

Run a local server (in memory) and play online against yourself (start the
game twice, with different `HOME` folders so the sessions don't collide):

```bash
SNOOK_LEGEND_IN_MEMORY=1 cargo run --release -p snook_legend_server
```

With PostgreSQL instead (what production uses):

```bash
DATABASE_URL=postgres://user:pass@localhost/snook-legend cargo run --release -p snook_legend_server
```

The game connects to the public server (`wss://snook-server.onrender.com/ws`) by
default; for a local server, enter `ws://127.0.0.1:7878/ws` under Settings → Game
server. To build a release that points at another server, set `SNOOK_LEGEND_SERVER` at
build time:

```bash
SNOOK_LEGEND_SERVER=wss://snooklegend.example.com/ws cargo build --release -p snook-legend
```

Putting the server on the internet is described in [docs/DEPLOY.md](docs/DEPLOY.md)
(step by step for Render + Neon, in Portuguese: [docs/DEPLOY.pt-BR.md](docs/DEPLOY.pt-BR.md),
including the Stripe setup for the shop in section 9).
The staff panel is described in [docs/ADMIN.md](docs/ADMIN.md), the protocol in
[docs/ONLINE.md](docs/ONLINE.md), and the reviews behind this release in
[docs/SECURITY.md](docs/SECURITY.md) and [docs/UX.md](docs/UX.md).

Useful flags (for development):

```bash
cargo run --release -p snook-legend -- --vs hard        # straight into a game
cargo run --release -p snook-legend -- --demo           # computer vs computer
cargo run --release -p snook-legend -- --window 900x600 # start with this window size
cargo run --release -p snook-legend -- --screenshot shot.png --frames 200
cargo run --release -p snook-legend -- --demo --fps     # print frames per second
cargo run --release -p snook-legend -- --online         # open the lounge directly
cargo run --release -p snook-legend -- --screen shop    # or lan, setup, ranking, profile:<name>, …
cargo run --release -p snook-legend -- --screen showroom:cue_tiger   # an item in the shop's showroom
```

Development builds also read a few switches from the environment to drive
automated matches and screenshots (`SNOOK_LEGEND_AUTOPLAY`, `SNOOK_LEGEND_AUTOINVITE`,
`SNOOK_LEGEND_PROFILE`…, see `crates/client/src/dev.rs`); release builds ignore them.

Run the tests (the PostgreSQL store test runs when
`SNOOK_LEGEND_TEST_DATABASE_URL` points at an empty database):

```bash
cargo test --workspace --release
```

## Project layout

```
crates/
  core/     snook_legend_core: physics, table geometry, 8-ball rules, AI, Elo rating
            (no engine dependencies; shared by the client and the server)
  proto/    snook_legend_proto: the client/server messages (JSON over WebSocket),
            countries, rating tiers, achievements, reactions and the shop
            catalogue
  server/   snook_legend_server: accounts, lounge, Play now, tables, tournaments,
            friends, spectators, authoritative matches, ratings,
            achievements, wallets and Stripe payments, the staff panel
            (Axum; PostgreSQL, or in memory for LAN games and tests)
  client/   snook-legend: the desktop client built on macroquad (3D scene with a
            custom lighting shader, input, HUD, sound, online and LAN screens);
            assets/icon/ holds the emblem's files (made by src/icon.rs) and
            wix/ the Windows installer
vendor/
  miniquad/ miniquad with two platform fixes (see vendor/README.md)
tools/      gen_countries.py, gen_reactions.py, gen_emoji.py, gen_avatars.py:
            regenerate the country list, the flag atlas, the reaction atlas,
            the emoji atlas (emoji in chat and other text) and the profile
            pictures' atlas (drawn as SVG, rasterised with Playwright's Chromium)
Dockerfile, render.yaml: the server image and the Render blueprint
dist-workspace.toml: packaging and releases with dist (docs/RELEASING.md)
```

Online games are **authoritative**: the client only sends the stroke (aim, power,
spin, elevation); the server simulates the shot with the same deterministic physics
and sends both players the resulting table, so nobody can cheat by editing their
client.

## Roadmap

1. ~~Physics, rules, AI, rating~~
2. ~~3D client with offline modes~~
3. ~~Game server: accounts, lounge with chat, invitations, authoritative matches~~
4. ~~Rated games and the global leaderboard~~
5. ~~Friends, private tables, tournaments, achievements, profiles, LAN, staff panel~~
6. ~~Spectating friends, Play now, reactions, the shop and VIP~~
7. ~~Installers and packages for every platform (MSI, shell, PowerShell; dist)~~
8. Store releases: ~~Microsoft Store~~; Steam, itch.io and signed installers
   for the other downloads
9. Replays of whole matches

## License

[MIT](LICENSE). The bundled DejaVu and Noto fonts keep their own licenses (below).

## Credits

Fonts: DejaVu Sans (Bitstream Vera license, see https://dejavu-fonts.github.io/License.html).
Chinese, Japanese, Korean, Arabic, Devanagari, Bengali and Thai: subsets of
[Noto](https://notofonts.github.io/) Sans (SIL Open Font License 1.1, see
`crates/client/assets/noto/OFL.txt`; made by `tools/gen_fonts.py`).

Flags: [flag-icons](https://github.com/lipis/flag-icons) (MIT, see
`crates/client/assets/flag-icons-LICENSE.txt`). Country names: ISO 3166 data
and translations from the Debian [iso-codes](https://salsa.debian.org/iso-codes-team/iso-codes)
project (via pycountry, LGPL-2.1).

Reactions and the emoji in chat: [Twemoji](https://github.com/jdecked/twemoji)
graphics (CC-BY 4.0, see `crates/client/assets/twemoji-LICENSE.txt`).
