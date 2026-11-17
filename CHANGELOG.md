# Changelog

## Unreleased (protocol 17)

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
