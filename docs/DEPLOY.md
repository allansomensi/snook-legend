# Running the Snook Legend server

The server is a single executable (`snook-legend-server`) that keeps the lounge and
running matches in memory and stores accounts, ratings, history,
achievements, friends, reports and the staff audit log in PostgreSQL. It also
serves the staff API at `/admin/api`, for the staff portal (part of the
website, in its repository `snooklegend.app`, at `snooklegend.app/admin`).

A step-by-step guide for **Render + Neon**, in Portuguese, is in
[DEPLOY.pt-BR.md](DEPLOY.pt-BR.md). The official server runs on Render's
**Starter** plan (always on; `render.yaml` asks for it); the free plans of
Render and Neon are fine for testing. This page is the reference.

## Configuration

| Variable | Default | Meaning |
|---|---|---|
| `PORT` / `SNOOK_LEGEND_BIND` | `0.0.0.0:7878` | port to listen on (`PORT`, as set by hosts like Render) or a full address |
| `DATABASE_URL` | — | PostgreSQL connection string (required unless `SNOOK_LEGEND_IN_MEMORY=1`); migrations run at start |
| `SNOOK_LEGEND_DB_POOL` | `5` | database connections |
| `SNOOK_LEGEND_IN_MEMORY` | — | `1`: keep everything in memory (development only) |
| `SNOOK_LEGEND_ADMINS` | — | comma-separated usernames that become administrators when they sign in |
| `SNOOK_LEGEND_CLIENT_IP_HEADER` | — | headers with the client's real address behind a proxy, tried in order, e.g. `cf-connecting-ip,true-client-ip,x-forwarded-for` |
| `SNOOK_LEGEND_TRUSTED_PROXIES` | private networks | the proxies whose address headers are believed (networks such as `203.0.113.0/24, 2001:db8::/32`; `*`: any). A connection from anywhere else is counted by its own address, whatever its headers say, so nobody can dodge the limits per address by sending a header. The default fits hosts whose proxy connects from a private network (Render, Fly, a reverse proxy on the same machine) |
| `SNOOK_LEGEND_PROXY_HOPS` | `1` | with `x-forwarded-for`: proxies that append to it (the address is taken that many entries from the right) |
| `SNOOK_LEGEND_MOTD` | a welcome line | message shown in the lounge (the first value: the portal's *Settings* change it live) |
| `SNOOK_LEGEND_SHOT_CLOCK` | `30` | seconds per shot (10–120; the first value: see the portal's *Settings*) |
| `SNOOK_LEGEND_STRIPE_*` | — | real-money purchases through Stripe (see `crates/server/src/payments.rs`) |
| `SNOOK_LEGEND_MSSTORE_*` | — | coin packs bought in the Microsoft Store (see [MSSTORE.md](MSSTORE.md)) |
| `SNOOK_LEGEND_SITE_URL` | `https://snooklegend.app` | the game's website, with the terms of use and the privacy policy: the server's old addresses for them (`/termos`, `/privacy`…) and `/` lead there, permanently |
| `SNOOK_LEGEND_ADMIN_ORIGIN` | — | the origin of the website, where the staff portal is (`https://snooklegend.app`): the only site a browser may call the staff API (`/admin/api`) from (CORS, no credentials: the portal's session is a bearer token), and `/admin` here leads to its `/admin/`. Without it, no browser may call the staff API from another site, and `/admin` is not found. (`SNOOK_LEGEND_ADMIN_HOST`, before 0.10.3, is no longer used.) |
| `RUST_LOG` | `info` | log filter |

Endpoints: `GET /ws` (the game), `GET /health` (`ok`), `GET /api/status`
(the version, the protocol and the players online, also by rules) and
`GET /api/leaderboard?rules=brazilian|official&mode=singles|doubles` (the top players under those
rules, Brazilian when left out; `mode=doubles`: by the doubles rating; public JSON with CORS, for a website),
`/admin/api/…` (the staff API; `/admin` leads to the portal). The terms of use and the privacy policy are on the game's website
(the `snooklegend.app` repository): the server's old addresses for them
(`/termos`, `/terms`, `/terminos`, `/privacidade`, `/privacy`, `/privacidad`)
and `/` redirect there. The public server's addresses: the game at
`wss://api.snooklegend.app/ws`, the website on `https://snooklegend.app`, the
staff portal on `https://snooklegend.app/admin/` (section 11 of the Portuguese guide sets
them up).

## Docker

```bash
docker build -t snook-legend-server .
```

```bash
docker run -p 7878:7878 -e DATABASE_URL=postgres://user:pass@host/db snook-legend-server
```

The image is distroless (no shell, unprivileged user, ~50 MB). The Server
image workflow publishes it as `ghcr.io/<owner>/snook-legend-server:<tag>` when a
release tag is pushed (see [RELEASING.md](RELEASING.md)).

## Without Docker (a VPS with systemd)

```bash
cargo build --release -p snook_legend_server
```

```bash
sudo cp target/release/snook-legend-server /usr/local/bin/
```

`/etc/systemd/system/snook-legend.service`:

```ini
[Unit]
Description=Snook Legend game server
After=network.target postgresql.service

[Service]
DynamicUser=yes
Environment=SNOOK_LEGEND_BIND=127.0.0.1:7878
Environment=DATABASE_URL=postgres://snook-legend:secret@localhost/snook-legend
Environment=SNOOK_LEGEND_CLIENT_IP_HEADER=x-forwarded-for
ExecStart=/usr/local/bin/snook-legend-server
Restart=always
RestartSec=2
NoNewPrivileges=true
ProtectSystem=strict

[Install]
WantedBy=multi-user.target
```

Put it behind TLS (Caddy: `snooklegend.example.com { reverse_proxy 127.0.0.1:7878 }`);
the game's address is then `wss://snooklegend.example.com/ws`.

## Point the game at the server

```bash
SNOOK_LEGEND_SERVER=wss://snook-legend.example.com/ws cargo build --release -p snook-legend
```

Players can also change it under **Settings → Game → Game server**.

## Updating

A restart keeps every account and result; players reconnect by themselves.
On SIGTERM the server stops starting games, lets the games in progress go
on for `SNOOK_LEGEND_DRAIN_SECS` and saves what is left, which the next
server picks up when the players reconnect. A maintenance scheduled in
the staff portal does the same at its time and then ends the process
cleanly (exit code 0): the host must start it again by itself (Render and
Docker's `restart: always` do; with systemd, `Restart=always`).

Since 0.12 the environment variables of the lounge (`SNOOK_LEGEND_MOTD`,
`SNOOK_LEGEND_SHOT_CLOCK`, `SNOOK_LEGEND_CUP`) only give the first values:
the portal's *Settings* page changes them (and connection limits, the
tournaments' house cut, the economy's numbers) while the server runs, and
what it sets is kept in the database. When messages
change incompatibly, `PROTOCOL_VERSION` in `crates/proto` is bumped and older
games are told to update.

Update the server together with (or before) each release: a game newer than
the server can't play online until it is. On Render, the **Server image**
workflow redeploys by itself when the repository secret `RENDER_DEPLOY_HOOK`
holds the service's Deploy Hook URL; otherwise use **Manual Deploy → Deploy
latest reference**. `GET /api/status` shows the version running.

## Rules enforced by the server

- Usernames: 3–16 letters, digits or `_` (case-insensitive, unique, a few
  reserved). Passwords: 8–72 characters, not only digits; Argon2id.
- Sessions: random 256-bit tokens, stored hashed (SHA-256); valid 30 days,
  renewed when used after a day; staff panel sessions last 8 hours.
- Sign-in throttling: 8 failures per account or 20 per address in 15 minutes;
  5 new accounts per address per hour; 5 bad sign-ins close the connection.
- An account is live in one place at a time: signing in again closes the
  connection that was there.
- An IPv6 address is counted as its /64 for every limit "per address".
- Connections: 12 per address; 40 messages a second per connection (bursts of
  120) or the connection is closed; 16 KiB per message; outgoing queues are
  bounded (a client that stops reading is disconnected).
- Chat: 200 characters, control and invisible characters removed, about one
  message a second with bursts of 5, no repeating the same line; blocked
  players' messages aren't delivered; muted players can't chat.
- Rated games: Elo with K = 24 for the first 10 games, 14 after that, 8 from
  1900.
- Shot clock (default 30 s): when it runs out the shot is played at full
  power where the player was aiming.
- A player who disconnects has 60 s to come back; after that the game is lost.
- Every shot is validated and simulated by the server; clients only send the
  stroke.
