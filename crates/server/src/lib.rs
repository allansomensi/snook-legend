//! # snook_legend_server
//!
//! The Snook Legend game server: accounts, the lounge (who's online, chat,
//! invitations, open tables), authoritative eight-ball matches with a shot
//! clock and reconnection, tournaments, friends, achievements, Elo ratings,
//! the global leaderboard and a web panel for the staff.
//!
//! Endpoints:
//! - `GET /ws`: the game (WebSocket, JSON messages from [`snook_legend_proto`]);
//! - `GET /health`: `ok`, for monitoring;
//! - `GET /api/status`, `GET /api/leaderboard[?rules=official]`: public JSON;
//! - `/admin`: the staff panel (see [`admin`]).

pub mod account;
pub mod admin;
pub mod advanced;
pub mod analysis;
pub mod analytics;
pub mod auction;
pub mod collection;
pub mod computer;
pub mod correspondence;
pub mod cup_medals;
pub mod daily;
pub mod dm;
pub mod drills;
pub mod events;
pub mod feats;
pub mod inbox;
pub mod lan;
pub mod limits;
pub mod lounge;
pub mod maintenance;
pub mod metrics;
pub mod missions;
pub mod moderation;
pub mod msstore;
pub mod official;
pub mod ops;
pub mod pages;
pub mod payments;
pub mod prefs;
pub mod promotions;
pub mod public_api;
pub mod replay_view;
pub mod reports;
pub mod settings;
pub mod shop;
pub mod social;
pub mod store;
pub mod time_attack;
pub mod tournament;
pub mod wordfilter;

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use argon2::Argon2;
use argon2::password_hash::{
    PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng,
};
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use sha2::{Digest, Sha256};
use snook_legend_core::rating::EloConfig;
use snook_legend_core::{GameOverReason, PerRules, Rules};
use snook_legend_proto::{
    ACHIEVEMENTS, ClientMsg, FriendInfo, HistoryEntry, LeaderboardEntry, MAX_PASSWORD_BYTES,
    OLDEST_PROTOCOL_VERSION, PROTOCOL_VERSION, PlayerStatus, Profile, Relation, Role, ServerMsg,
    clean_text, parse_client, valid_avatar, valid_country, valid_password, valid_username,
};
use tokio::net::TcpListener;
use tokio::sync::{Notify, Semaphore, mpsc};

use crate::limits::{AuthLimiter, Bucket};
use crate::lounge::{Effect, Lounge, Settings, key, player_info};
use crate::store::{FriendOutcome, FriendState, NewReport, NewUser, SessionKind, Store, User};

/// Unix time in seconds.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Whether `ip` is one of the proxies (`None`: a private network).
pub fn trusted_proxy(ip: IpAddr, trusted: Option<&[(IpAddr, u8)]>) -> bool {
    let ip = ip.to_canonical();
    match trusted {
        Some(list) => list.iter().any(|(net, bits)| in_network(ip, *net, *bits)),
        None => match ip {
            IpAddr::V4(v4) => {
                v4.is_private() || v4.is_loopback() || v4.is_link_local() || {
                    // Shared address space (100.64.0.0/10), used by hosts' proxies.
                    let o = v4.octets();
                    o[0] == 100 && (64..128).contains(&o[1])
                }
            }
            IpAddr::V6(v6) => {
                v6.is_loopback()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
            }
        },
    }
}

/// `ip` lies in `net`/`bits` (any address when `bits` is 0).
fn in_network(ip: IpAddr, net: IpAddr, bits: u8) -> bool {
    if bits == 0 {
        return true;
    }
    match (ip, net.to_canonical()) {
        (IpAddr::V4(a), IpAddr::V4(n)) => {
            let bits = bits.min(32) as u32;
            let mask = if bits == 0 { 0 } else { u32::MAX << (32 - bits) };
            u32::from(a) & mask == u32::from(n) & mask
        }
        (IpAddr::V6(a), IpAddr::V6(n)) => {
            let bits = bits.min(128) as u32;
            let mask = if bits == 0 { 0 } else { u128::MAX << (128 - bits) };
            u128::from(a) & mask == u128::from(n) & mask
        }
        _ => false,
    }
}

/// A list of networks ("10.0.0.0/8, 2001:db8::/32, 192.0.2.7"); `*`: any
/// address. Entries that don't read are left out.
pub fn parse_networks(text: &str) -> Vec<(IpAddr, u8)> {
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| {
            if s == "*" {
                return Some((IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED), 0));
            }
            let (addr, bits) = s.split_once('/').unwrap_or((s, ""));
            let ip: IpAddr = addr.parse().ok()?;
            let max = if ip.to_canonical().is_ipv4() { 32 } else { 128 };
            let bits =
                if bits.is_empty() { max } else { bits.parse().ok().filter(|b| *b <= max)? };
            Some((ip.to_canonical(), bits))
        })
        .collect()
}

/// How a game-over reason is stored.
pub fn reason_code(r: GameOverReason) -> &'static str {
    match r {
        GameOverReason::PocketedEight => "pocketed_eight",
        GameOverReason::EightPocketedEarly => "eight_pocketed_early",
        GameOverReason::EightInWrongPocket => "eight_in_wrong_pocket",
        GameOverReason::EightPocketedOnFoul => "eight_pocketed_on_foul",
        GameOverReason::EightOffTable => "eight_off_table",
        GameOverReason::EightWithAnotherBall => "eight_with_another_ball",
        GameOverReason::Resignation => "resignation",
        GameOverReason::Abandoned => "abandoned",
    }
}

pub fn parse_reason(code: &str) -> GameOverReason {
    match code {
        "pocketed_eight" | "PocketedEight" => GameOverReason::PocketedEight,
        "eight_pocketed_early" | "EightPocketedEarly" => GameOverReason::EightPocketedEarly,
        "eight_in_wrong_pocket" | "EightInWrongPocket" => GameOverReason::EightInWrongPocket,
        "eight_pocketed_on_foul" | "EightPocketedOnFoul" => GameOverReason::EightPocketedOnFoul,
        "eight_off_table" | "EightOffTable" => GameOverReason::EightOffTable,
        "eight_with_another_ball" | "EightWithAnotherBall" => GameOverReason::EightWithAnotherBall,
        "abandoned" | "Abandoned" => GameOverReason::Abandoned,
        _ => GameOverReason::Resignation,
    }
}

/// Server configuration (usually from environment variables, see `main.rs`).
#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    /// PostgreSQL connection string; `None` keeps everything in memory.
    pub database_url: Option<String>,
    pub database_pool: u32,
    pub settings: Settings,
    /// Usernames that are made administrators when they sign in.
    pub admins: Vec<String>,
    /// Headers with the client's address when behind a proxy, tried in
    /// order (`cf-connecting-ip,true-client-ip,x-forwarded-for`…).
    pub client_ip_header: Option<String>,
    /// With `x-forwarded-for`: how many proxies append to it (the address is
    /// taken that many entries from the right).
    pub proxy_hops: usize,
    /// Who may tell the client's address in those headers: the proxies'
    /// networks (`None`: private networks only, where a host's proxy
    /// connects from; anyone else's header is ignored).
    pub trusted_proxies: Option<Vec<(IpAddr, u8)>>,
    /// Real-money payments (none when not configured).
    pub stripe: Option<payments::StripeConfig>,
    /// Coin packs sold in the Microsoft Store (none when not configured).
    pub msstore: Option<msstore::MsStoreConfig>,
    /// The game's website, where the terms of use and the privacy policy
    /// are: this server's old addresses for them and its root lead there
    /// (see [`pages`]).
    pub site_url: String,
    /// The staff portal's origin (`https://snooklegend.app`): the portal is
    /// part of the website (its `/admin` pages, in the website's repository)
    /// and calls the staff API (`/admin/api`) from there. Only that origin
    /// may call it from a browser (CORS), and `/admin` here leads to the
    /// portal (`{origin}/admin/`).
    /// `None`: no browser may call the staff API from another origin.
    pub admin_origin: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            bind: "0.0.0.0:7878".parse().expect("address"),
            database_url: None,
            database_pool: 5,
            settings: Settings::default(),
            admins: Vec::new(),
            client_ip_header: None,
            proxy_hops: 1,
            trusted_proxies: None,
            stripe: None,
            msstore: None,
            site_url: pages::SITE_URL.into(),
            admin_origin: None,
        }
    }
}

/// A ranking as read from the store, and when.
type CachedBoard = (Instant, Vec<LeaderboardEntry>);

pub struct Server {
    pub store: Store,
    lounge: Mutex<Lounge>,
    next_conn: AtomicU64,
    auth: Mutex<AuthLimiter>,
    /// The effect worker's queue, and the shots to simulate (a queue of
    /// their own: a slow database never holds a shot up); taken when the
    /// server starts.
    effects: Mutex<Option<(mpsc::UnboundedReceiver<Effect>, mpsc::UnboundedReceiver<Effect>)>>,
    /// Effects sent to the worker and not done yet (a stopping server
    /// waits for them).
    effects_pending: Arc<AtomicUsize>,
    pub config: Config,
    pub started: Instant,
    /// Password hashing is slow and memory-hungry: a few at a time.
    hashing: Semaphore,
    /// The world ranking of each of the rules, with when it was read.
    leaderboard: Mutex<PerRules<Option<CachedBoard>>>,
    connections: Mutex<HashMap<IpAddr, usize>>,
    /// The connection-level limits of accounts whose connection closed a
    /// moment ago, since when: signing in again doesn't fill them up.
    kept_limits: Mutex<HashMap<i64, (ConnLimits, Instant)>>,
    pub stripe: Option<payments::Stripe>,
    /// Microsoft Store purchases (see [`msstore`]).
    pub msstore: Option<msstore::MsStore>,
    /// Correspondence games being changed (see [`correspondence`]).
    pub(crate) corr: correspondence::State,
    /// The ladder's reward and ghosts' styles (see [`computer`]).
    pub computer: computer::Computer,
    /// The staff's events not over yet (see [`events`]).
    pub events: events::Events,
    /// Live settings, promotions, maintenance, auctions (see [`ops`]).
    pub ops: ops::Ops,
    /// The leagues' numbers for the advanced statistics (see [`advanced`]).
    pub(crate) leagues: advanced::LeagueCache,
}

/// Drops connections that send nothing (clients ping every ~15 s).
const IDLE_TIMEOUT: Duration = Duration::from_secs(75);
/// Room for the longest message: a whole game sent to claim the ladder's
/// reward (a few hundred strokes).
const MAX_MESSAGE_BYTES: usize = 128 * 1024;
const OUTGOING_QUEUE: usize = 512;
/// A message the client doesn't take in this long ends the connection.
const SEND_TIMEOUT: Duration = Duration::from_secs(15);
/// Game sessions last this long; they are renewed when used after a day.
pub(crate) const SESSION_LIFETIME: i64 = 60 * 60 * 24 * 30;
const SESSION_RENEW_AFTER: i64 = 60 * 60 * 24;
/// Names nobody may take, in the game's three languages; also with digits
/// or underscores around them ("Admin_1", "_mod").
const RESERVED_NAMES: &[&str] = &[
    "admin",
    "administrator",
    "administrador",
    // The game's name, then and now.
    "snook-legend",
    "snooklegend",
    "snook_legend",
    "moderator",
    "moderador",
    "mod",
    "system",
    "sistema",
    "staff",
    "equipe",
    "equipo",
    "support",
    "suporte",
    "soporte",
    "official",
    "oficial",
    "guest",
    "convidado",
    "invitado",
];
/// How long a password check waits for its turn (two at a time) before the
/// server says it is busy, rather than queueing without end.
const HASH_WAIT: Duration = Duration::from_secs(15);

impl Server {
    pub async fn new(config: Config) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let store = match &config.database_url {
            #[cfg(feature = "postgres")]
            Some(url) => Store::postgres(url, config.database_pool).await?,
            #[cfg(not(feature = "postgres"))]
            Some(_) => return Err("this build has no database support".into()),
            None => Store::memory(),
        };
        Ok(Self::with_store(config, store).await)
    }

    /// A server on an existing store.
    pub async fn with_store(config: Config, store: Store) -> Arc<Self> {
        let _ = store.purge_sessions(now()).await;
        let _ = store.purge_missions(now()).await;
        let (tx, rx) = mpsc::unbounded_channel();
        let (shots_tx, shots_rx) = mpsc::unbounded_channel();
        let stripe = config.stripe.clone().map(payments::Stripe::new);
        let msstore = config.msstore.clone().map(msstore::MsStore::new);
        let mut lounge = Lounge::new(config.settings.clone(), tx);
        lounge.simulate_on(shots_tx);
        let effects_pending = lounge.pending_effects();
        if let Ok(notes) = board_notes(&store).await {
            lounge.set_board(notes);
        }
        let ops = ops::Ops::new(&config);
        let server = Arc::new(Server {
            stripe,
            msstore,
            store,
            lounge: Mutex::new(lounge),
            next_conn: AtomicU64::new(1),
            auth: Mutex::new(AuthLimiter::default()),
            effects: Mutex::new(Some((rx, shots_rx))),
            effects_pending,
            config,
            started: Instant::now(),
            hashing: Semaphore::new(2),
            leaderboard: Mutex::new(PerRules::default()),
            connections: Mutex::new(HashMap::new()),
            kept_limits: Mutex::new(HashMap::new()),
            corr: correspondence::State::default(),
            computer: computer::Computer::default(),
            events: events::Events::default(),
            ops,
            leagues: advanced::LeagueCache::default(),
        });
        // What the staff set in the portal is in force from the start.
        ops::init(&server).await;
        server
    }

    /// Reads the board again and shows it to everyone.
    pub async fn refresh_board(&self) {
        match board_notes(&self.store).await {
            Ok(notes) => self.with(|l| l.set_board(notes)),
            Err(e) => tracing::error!("the board: {e}"),
        }
    }

    /// Runs `f` with the lounge locked.
    pub fn with<R>(&self, f: impl FnOnce(&mut Lounge) -> R) -> R {
        let mut lounge = self.lounge.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut lounge)
    }

    fn auth(&self) -> std::sync::MutexGuard<'_, AuthLimiter> {
        self.auth.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn is_admin_name(&self, name: &str) -> bool {
        self.config.admins.iter().any(|a| a.eq_ignore_ascii_case(name))
    }

    /// Names nobody may take, by registering or by changing theirs: the
    /// reserved ones and the configured administrators' (signing in with
    /// one makes an administrator, so such a name only ever belongs to the
    /// account that has it already).
    pub(crate) fn is_reserved_name(&self, name: &str) -> bool {
        is_reserved_word(name) || self.is_admin_name(name)
    }

    /// The client's address: from the first configured proxy header that is
    /// present (e.g. `cf-connecting-ip,x-forwarded-for`), else the peer.
    /// The address a connection is counted under. An IPv6 address stands
    /// for its /64: that is what one home or one phone is given, and its
    /// owner can pick any of its 2^64 addresses, so limits "per address"
    /// (connections, new accounts, wrong passwords) kept by the full
    /// address limited nothing.
    pub fn client_ip(&self, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
        counted_address(self.claimed_ip(peer, headers))
    }

    fn claimed_ip(&self, peer: SocketAddr, headers: &HeaderMap) -> IpAddr {
        let Some(names) = &self.config.client_ip_header else { return peer.ip() };
        // A header is only believed from the proxy: anyone reaching the
        // server directly could otherwise name any address and slip past
        // every limit per address.
        if !trusted_proxy(peer.ip(), self.config.trusted_proxies.as_deref()) {
            return peer.ip();
        }
        for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            // Every line of the header, in order (a proxy may add its own
            // line instead of adding to the one it was sent: the client's
            // line alone would be the client's word for its address).
            let lines: Vec<&str> =
                headers.get_all(name).iter().filter_map(|v| v.to_str().ok()).collect();
            if lines.is_empty() {
                continue;
            }
            let value = lines.join(",");
            let parts: Vec<&str> = value.split(',').map(str::trim).collect();
            let pick = if name.eq_ignore_ascii_case("x-forwarded-for") {
                parts.len().checked_sub(self.config.proxy_hops.max(1)).and_then(|i| parts.get(i))
            } else {
                parts.first()
            };
            if let Some(ip) = pick.and_then(|p| p.parse().ok()) {
                return ip;
            }
        }
        peer.ip()
    }

    /// The top 100 of the world ranking under `rules` (read again at most
    /// every 15 seconds).
    pub async fn leaderboard(&self, rules: Rules) -> Vec<LeaderboardEntry> {
        if let Some((at, list)) =
            self.leaderboard.lock().unwrap_or_else(|e| e.into_inner()).get(rules).as_ref()
            && at.elapsed() < Duration::from_secs(15)
        {
            return list.clone();
        }
        match self.store.leaderboard(rules, 100, now()).await {
            Ok(list) => {
                *self.leaderboard.lock().unwrap_or_else(|e| e.into_inner()).get_mut(rules) =
                    Some((Instant::now(), list.clone()));
                list
            }
            Err(e) => {
                tracing::error!("leaderboard query failed: {e}");
                Vec::new()
            }
        }
    }

    /// The doubles ranking of `rules` (its first hundred).
    pub async fn doubles_leaderboard(&self, rules: Rules) -> Vec<LeaderboardEntry> {
        match self.store.doubles_leaderboard(rules, 100, now()).await {
            Ok(list) => list,
            Err(e) => {
                tracing::error!("doubles leaderboard query failed: {e}");
                Vec::new()
            }
        }
    }

    pub(crate) fn invalidate_leaderboard(&self) {
        *self.leaderboard.lock().unwrap_or_else(|e| e.into_inner()) = PerRules::default();
    }

    /// None when it could not be done (the server is too busy).
    pub async fn hash_password(&self, password: &str) -> Option<String> {
        let _permit = tokio::time::timeout(HASH_WAIT, self.hashing.acquire()).await.ok()?.ok()?;
        let password = password.to_string();
        tokio::task::spawn_blocking(move || {
            let salt = SaltString::generate(&mut OsRng);
            Argon2::default().hash_password(password.as_bytes(), &salt).ok().map(|h| h.to_string())
        })
        .await
        .ok()
        .flatten()
    }

    /// Whether `password` is the one of `hash`; None when it could not be
    /// checked (the server is too busy).
    pub async fn verify_password(&self, password: &str, hash: &str) -> Option<bool> {
        let Ok(Ok(_permit)) = tokio::time::timeout(HASH_WAIT, self.hashing.acquire()).await else {
            return None;
        };
        let (password, hash) = (password.to_string(), hash.to_string());
        let ok = tokio::task::spawn_blocking(move || {
            PasswordHash::new(&hash)
                .is_ok_and(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
        })
        .await
        .unwrap_or(false);
        Some(ok)
    }

    /// Unlocks every achievement `user` has earned and tells them.
    pub async fn check_achievements(&self, user: &User) {
        self.check_achievements_after(user, false).await
    }

    /// Unlocks an achievement the server awards for a feat it saw (not one
    /// that comes from the account's numbers), and tells the player.
    pub async fn award_achievement(&self, user: &User, code: &str) {
        match self.store.add_achievements(user.id, &[code], now()).await {
            Ok(new) => {
                let all = self.store.achievements(user.id).await.unwrap_or_default();
                let codes: Vec<String> = all.into_iter().map(|(code, _)| code).collect();
                let xp = new.len() as u32 * snook_legend_proto::xp::ACHIEVEMENT;
                inbox::achievements(self, user.id, &new).await;
                self.with(|l| {
                    for code in new {
                        l.notify(&user.username, ServerMsg::AchievementUnlocked { code });
                    }
                    l.set_achievements(&user.username, codes);
                });
                account::award_xp(self, user, xp).await;
            }
            Err(e) => tracing::error!("achievement {code} for {}: {e}", user.username),
        }
    }

    /// Like [`Server::check_achievements`]; `rated_win` when the player just
    /// won a rated game (what event achievements ask for).
    pub async fn check_achievements_after(&self, user: &User, rated_win: bool) {
        let friends = self.store.friend_count(user.id).await.unwrap_or(0);
        let mut stats = user.stats(now());
        stats.friends = friends;
        if rated_win {
            stats.today = snook_legend_proto::month_day(now());
        }
        let earned: Vec<&str> = stats.earned().collect();
        match self.store.add_achievements(user.id, &earned, now()).await {
            Ok(new) => {
                let all = self.store.achievements(user.id).await.unwrap_or_default();
                let codes: Vec<String> = all.into_iter().map(|(code, _)| code).collect();
                let xp = new.len() as u32 * snook_legend_proto::xp::ACHIEVEMENT;
                inbox::achievements(self, user.id, &new).await;
                self.with(|l| {
                    for code in new {
                        l.notify(&user.username, ServerMsg::AchievementUnlocked { code });
                    }
                    l.set_achievements(&user.username, codes);
                });
                account::award_xp(self, user, xp).await;
            }
            Err(e) => tracing::error!("achievements for {}: {e}", user.username),
        }
    }
}

pub fn router(server: Arc<Server>) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/ws", get(ws_handler))
        .route("/api/status", get(api_status))
        .route("/api/leaderboard", get(api_leaderboard))
        .merge(public_api::router())
        .route("/stripe/webhook", axum::routing::post(shop::webhook))
        .route("/shop/thanks", get(shop::thanks))
        .route("/shop/cancelled", get(shop::cancelled))
        .merge(pages::router())
        .merge(admin::router(server.clone()))
        .layer(axum::middleware::from_fn_with_state(server.clone(), hosts))
        .with_state(server)
}

/// `/admin` was the staff portal's address on this server: it now leads to
/// the portal, on the website (or is not found when none is set).
async fn hosts(
    State(server): State<Arc<Server>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let path = req.uri().path();
    if path == "/admin" || path == "/admin/" {
        return match server.config.admin_origin.as_deref() {
            Some(origin) => {
                axum::response::Redirect::permanent(&format!("{origin}/admin/")).into_response()
            }
            None => axum::http::StatusCode::NOT_FOUND.into_response(),
        };
    }
    next.run(req).await
}

fn public_json(value: serde_json::Value) -> Response {
    let mut res = axum::Json(value).into_response();
    let h = res.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=15"));
    res
}

async fn api_status(State(server): State<Arc<Server>>) -> Response {
    let (online, by_rules) = server.with(|l| (l.online_count(), l.online_by_rules()));
    public_json(serde_json::json!({
        "online": online,
        "online_by_rules": by_rules,
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": PROTOCOL_VERSION,
    }))
}

#[derive(serde::Deserialize)]
struct LeaderboardQuery {
    /// "brazilian" (the default) or "official".
    #[serde(default)]
    rules: Option<String>,
    /// "singles" (the default) or "doubles".
    #[serde(default)]
    mode: Option<String>,
}

/// The ranking under the rules asked (`?rules=official`; the Brazilian
/// rules' by default), of singles or of doubles (`?mode=doubles`).
async fn api_leaderboard(
    State(server): State<Arc<Server>>,
    axum::extract::Query(q): axum::extract::Query<LeaderboardQuery>,
) -> Response {
    let Some(rules) = q.rules.as_deref().map_or(Some(Rules::default()), Rules::parse) else {
        return (axum::http::StatusCode::BAD_REQUEST, "rules: brazilian or official")
            .into_response();
    };
    let entries = match q.mode.as_deref() {
        None | Some("singles") => server.leaderboard(rules).await,
        Some("doubles") => server.doubles_leaderboard(rules).await,
        Some(_) => {
            return (axum::http::StatusCode::BAD_REQUEST, "mode: singles or doubles")
                .into_response();
        }
    };
    public_json(serde_json::to_value(entries).unwrap_or_default())
}

/// Runs the server on an already-bound listener until the process stops.
pub async fn serve(listener: TcpListener, server: Arc<Server>) -> std::io::Result<()> {
    use axum::serve::ListenerExt;
    spawn_background(server.clone());
    // No Nagle delay: small game messages go out at once.
    let listener = listener.tap_io(|tcp| {
        let _ = tcp.set_nodelay(true);
    });
    axum::serve(listener, router(server).into_make_service_with_connect_info::<SocketAddr>()).await
}

/// Stops gracefully: no new games, a notice in the lounge, and up to `secs`
/// for the games in progress to end. Whatever is still being played then is
/// handed to the next server (it is saved after every stroke; the players'
/// games reconnect and the new server rebuilds the table).
pub async fn drain(server: Arc<Server>, secs: u64) {
    server.with(|l| l.start_drain(secs));
    tracing::info!(secs, "draining: waiting for {} games", server.with(|l| l.running_games()));
    let deadline = Instant::now() + Duration::from_secs(secs + 10);
    while server.with(|l| l.running_games()) > 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    // What the effect worker still has to write (results, prizes, refunds,
    // saved games) is written before the process ends: long enough for a
    // result tried again through a database's trouble (see `retrying_long`).
    let deadline = Instant::now() + Duration::from_secs(60);
    while server.effects_pending.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tracing::info!("drained");
}

/// The ticker (shot clocks, tournaments), the effect worker and housekeeping.
pub fn spawn_background(server: Arc<Server>) {
    metrics::spawn(server.clone());
    let ticker = server.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        let mut last_sweep = Instant::now();
        loop {
            interval.tick().await;
            ticker.with(|l| l.tick());
            if last_sweep.elapsed() > Duration::from_secs(600) {
                last_sweep = Instant::now();
                ticker.auth().sweep();
                // On a task of their own: the shot clocks and tournaments
                // don't stop while the database works.
                let s = ticker.clone();
                tokio::spawn(async move {
                    let _ = s.store.purge_sessions(now()).await;
                    let _ = s.store.purge_missions(now()).await;
                    // A year of activity is more than the metrics read.
                    let _ = s.store.purge_old(store::day_of(now()) - 400, now()).await;
                    // Saved games nobody came back to within a day.
                    let _ = s.store.purge_live(now() - 86_400).await;
                });
            }
        }
    });
    // The banned words as the staff left them.
    let words = server.clone();
    tokio::spawn(async move { moderation::reload_words(&words).await });
    // Correspondence games: turns that ran out (also right after a restart).
    if !server.config.settings.lan {
        tokio::spawn(correspondence::run(server.clone()));
    }
    // The staff's events: boosts and themed tournaments.
    if !server.config.settings.lan {
        tokio::spawn(events::run(server.clone()));
    }
    // Promotions, maintenance, auctions, reminders of tournaments.
    if !server.config.settings.lan {
        tokio::spawn(ops::run(server.clone()));
    }
    // The week against the clock's prizes.
    if !server.config.settings.lan {
        tokio::spawn(time_attack::run(server.clone()));
    }
    // Microsoft Store purchases whose answer was lost on the way.
    if server.msstore.is_some() {
        tokio::spawn(msstore::run(server.clone()));
    }
    let queues = server.effects.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some((mut effects, mut shots)) = queues {
        // Each shot is simulated on a thread of its own as soon as it comes,
        // whatever the database is doing.
        let s = server.clone();
        tokio::spawn(async move {
            while let Some(shot) = shots.recv().await {
                run_effect(&s, shot).await;
            }
        });
        tokio::spawn(async move {
            while let Some(effect) = effects.recv().await {
                run_effect(&server, effect).await;
                server.effects_pending.fetch_sub(1, Ordering::SeqCst);
            }
        });
    }
}

async fn run_effect(server: &Arc<Server>, effect: Effect) {
    match effect {
        Effect::Simulate { room, turn, game, shot, called, eight_anywhere } => {
            let server = server.clone();
            tokio::spawn(async move {
                let outcome = tokio::task::spawn_blocking(move || {
                    let mut game = *game;
                    let mut pending =
                        game.prepare_shot(&shot, called).map_err(|e| e.to_string())?;
                    pending.simulation.run_to_rest();
                    if eight_anywhere {
                        game.call_the_eight_where_it_went(&mut pending);
                    }
                    let duration = pending.simulation.time();
                    let report = game.finish_shot(pending);
                    Ok((game, report, duration))
                })
                .await
                .unwrap_or_else(|e| Err(format!("simulation panicked: {e}")));
                server.with(|l| l.commit_shot(room, turn, outcome));
            });
        }
        Effect::RecordMatch { record, keys, farmed } => {
            // A failed attempt writes nothing (one transaction): the match
            // is tried again, for minutes, rather than lost to the
            // database's trouble; one stored already (an answer lost on the
            // way) is not stored again.
            let failed = std::sync::atomic::AtomicBool::new(false);
            let write = || {
                let attempt = server.store.record_match(&record);
                let failed = &failed;
                async move {
                    let r = attempt.await;
                    if r.is_err() {
                        failed.store(true, Ordering::SeqCst);
                    }
                    r
                }
            };
            let users = match retrying_long("recording a match", write).await {
                Ok(Some((users, _))) => Some(users),
                // Stored by a try whose answer was lost on the way (one
                // failed): what follows it hasn't been done, and is now.
                Ok(None) if failed.load(Ordering::SeqCst) => {
                    let (a, b) = tokio::join!(
                        server.store.user_by_id(record.players[0]),
                        server.store.user_by_id(record.players[1])
                    );
                    Some([a.ok().flatten(), b.ok().flatten()])
                }
                Ok(None) => {
                    tracing::warn!("the match {keys:?} was recorded already");
                    None
                }
                Err(e) => {
                    tracing::error!("could not record the match {keys:?}: {e}");
                    None
                }
            };
            server.with(|l| l.record_written(record.players));
            match users {
                None => {}
                Some(users) => {
                    server.invalidate_leaderboard();
                    // The shot analysis replays the game: off the effect queue.
                    let (s, r) = (server.clone(), record.clone());
                    tokio::spawn(async move {
                        analysis::after_match(&s, &r).await;
                        feats::after_match(&s, &r).await;
                    });
                    // Coins for a rated game played (not two friends farming
                    // each other); left half way, only the one who stayed
                    // is paid.
                    let pays =
                        record.rated && !farmed && record.shots >= account::MIN_SHOTS_FOR_COINS;
                    // A game played online counts for each player's cue (a
                    // win for the winner's), casual too (not a walkover, nor
                    // two friends farming each other).
                    let cue_game = !farmed
                        && record.shots >= account::MIN_SHOTS_FOR_COINS
                        && !server.with(|l| l.settings.lan);
                    for (seat, user) in users.into_iter().enumerate() {
                        let Some(user) = user else { continue };
                        server.with(|l| l.refresh_recorded(user.clone()));
                        if cue_game {
                            let won = record.winner == seat;
                            let potted = record.stats[seat].balls;
                            shop::count_cue_game(server, &user, won, potted).await;
                            // Experience for every game played to the end
                            // (not for the one who left it).
                            if record.abandoned != Some(seat) {
                                let xp = snook_legend_proto::xp::for_game(won, potted);
                                account::award_xp(server, &user, xp).await;
                            }
                        }
                        let rated_win = record.rated && record.counts && record.winner == seat;
                        server.check_achievements_after(&user, rated_win).await;
                        if record.rated {
                            let after = record.ratings_after[seat];
                            account::league_rewards(
                                server,
                                &user,
                                after - record.delta[seat],
                                after,
                            )
                            .await;
                        }
                        if rated_win && record.break_win {
                            server.award_achievement(&user, snook_legend_proto::GOLDEN_BREAK).await;
                        }
                        if pays && record.abandoned != Some(seat) {
                            account::game_coins(server, &user, record.winner == seat).await;
                            let won = record.winner == seat;
                            missions::after_game(server, &user, won, &record.stats[seat]).await;
                            shop::push_wallet(server, &user).await;
                        }
                    }
                }
            }
        }
        Effect::RecordDoubles { record, keys } => {
            match retrying_long("recording a doubles game", || server.store.record_doubles(&record))
                .await
            {
                Ok(true) => {}
                Ok(false) => {
                    return tracing::warn!("the doubles game {keys:?} was recorded already");
                }
                Err(e) => {
                    return tracing::error!("could not record the doubles game {keys:?}: {e}");
                }
            }
            // A game played (not left after a few shots) earns experience;
            // a rated one coins too, as in singles. The team that left it
            // gets neither.
            let played = record.shots >= account::MIN_SHOTS_FOR_COINS;
            let left = (record.reason == GameOverReason::Abandoned).then_some(1 - record.winner);
            for (team, ids) in record.players.iter().enumerate() {
                if !played || left == Some(team) {
                    continue;
                }
                for id in ids {
                    let Ok(Some(user)) = server.store.user_by_id(*id).await else { continue };
                    let won = record.winner == team;
                    account::award_xp(server, &user, snook_legend_proto::xp::for_game(won, 0))
                        .await;
                    if record.rated {
                        account::game_coins(server, &user, won).await;
                        shop::push_wallet(server, &user).await;
                    }
                }
            }
        }
        Effect::SaveLive(m) => {
            if let Err(e) = server.store.save_live(&m).await {
                tracing::warn!(room = m.id, "could not save the game in progress: {e}");
            }
        }
        Effect::HandOff { live, kills } => {
            if let Err(e) = retrying("handing a game over", || server.store.save_live(&live)).await
            {
                tracing::error!(room = live.id, "could not save the game handed over: {e}");
            }
            for kill in kills {
                kill.notify_one();
            }
        }
        Effect::DeleteLive(id) => {
            // A finished game left saved would be rebuilt when one of its
            // players signs in again.
            if let Err(e) = retrying("deleting a saved game", || server.store.delete_live(id)).await
            {
                tracing::error!(room = id, "could not delete the saved game: {e}");
            }
        }
        Effect::ShotStats { user_id, bands } => {
            if let Err(e) = server.store.add_shot_stats(user_id, &bands).await {
                tracing::warn!("shot statistics of {user_id}: {e}");
            }
        }
        Effect::ShotTimes { room, times } => {
            if let Err(e) = server.store.save_shot_times(room, &times, now()).await {
                tracing::warn!(room, "how long the shots took: {e}");
            }
        }
        Effect::Pay { user_id, name, coins, reason, reference } => {
            pay(server, user_id, &name, coins, &reason, &reference).await;
        }
        Effect::RecordOfficial(f) => official::finished(server, &f).await,
        Effect::RecordTournament(t) => {
            if let Err(e) =
                retrying("recording a tournament", || server.store.record_tournament(&t)).await
            {
                tracing::error!("could not record the tournament {}: {e}", t.name);
                return;
            }
            for id in &t.players {
                if let Ok(Some(user)) = server.store.user_by_id(*id).await {
                    server.with(|l| l.refresh_user(user.clone()));
                    server.check_achievements(&user).await;
                }
            }
        }
    }
}

async fn ws_handler(
    State(server): State<Arc<Server>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let ip = server.client_ip(peer, &headers);
    {
        let mut conns = server.connections.lock().unwrap_or_else(|e| e.into_inner());
        let n = conns.entry(ip).or_default();
        // (As set now: see `settings`.)
        let max = server.with(|l| l.settings.max_connections_per_ip);
        if *n >= max {
            return (axum::http::StatusCode::TOO_MANY_REQUESTS, "too many connections")
                .into_response();
        }
        *n += 1;
    }
    let guard = ConnGuard { server: server.clone(), ip };
    ws.max_message_size(MAX_MESSAGE_BYTES).max_frame_size(MAX_MESSAGE_BYTES).on_upgrade(
        move |socket| async move {
            handle_socket(server, socket, ip).await;
            drop(guard);
        },
    )
}

/// Counts open connections per address.
struct ConnGuard {
    server: Arc<Server>,
    ip: IpAddr,
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        let mut conns = self.server.connections.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = conns.get_mut(&self.ip) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                conns.remove(&self.ip);
            }
        }
    }
}

pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex(&bytes)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Tokens are stored hashed: a leaked database can't be used to sign in.
pub fn token_hash(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

enum Auth {
    Ok {
        user: Box<User>,
        token: String,
        guest: bool,
    },
    Failed(String),
    /// Turned away without an answer (the connection closes): a saved
    /// session signing in too often. Told why, the game would forget the
    /// session; this way it tries again later by itself.
    Dropped,
}

const TOO_MANY_SIGN_INS: &str = "Too many sign-ins. Wait a minute and try again.";

fn ban_message(user: &User) -> String {
    let reason = user.ban_reason.as_deref().filter(|r| !r.is_empty()).unwrap_or("not given");
    match user.banned_until {
        Some(t) if t >= i64::MAX / 2 => format!("This account is banned. Reason: {reason}."),
        Some(t) => {
            let days = ((t - now()) as f64 / 86_400.0).ceil().max(1.0) as i64;
            format!("This account is banned for {days} more day(s). Reason: {reason}.")
        }
        None => String::new(),
    }
}

async fn new_session(server: &Server, user: &User) -> Option<String> {
    let token = new_token();
    let t = now();
    match server
        .store
        .create_session(&token_hash(&token), user.id, SessionKind::Game, t, t + SESSION_LIFETIME)
        .await
    {
        Ok(()) => Some(token),
        Err(e) => {
            tracing::error!("could not store a session: {e}");
            None
        }
    }
}

/// Makes configured administrators admins (bootstrap for a new server).
async fn promote_if_admin(server: &Server, mut user: User) -> User {
    if server.is_admin_name(&user.username)
        && user.role != Role::Admin
        && server.store.set_role(user.id, Role::Admin).await.is_ok()
    {
        user.role = Role::Admin;
        tracing::info!("{} is now an administrator", user.username);
    }
    user
}

/// Checked against when the account doesn't exist, so a wrong name takes as
/// long as a wrong password (timing tells nothing).
pub(crate) const DUMMY_HASH: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

async fn authenticate(server: &Arc<Server>, msg: ClientMsg, ip: &str) -> Auth {
    let fail = |s: &str| Auth::Failed(s.to_string());
    match msg {
        ClientMsg::Register { username, password, country, minor } => {
            if server.config.settings.lan {
                return fail("This is a LAN server: join as a guest.");
            }
            if !server.auth().try_sign_in(ip) {
                return fail(TOO_MANY_SIGN_INS);
            }
            let username = username.trim().to_string();
            if !valid_username(&username) {
                return fail("Usernames have 3–16 letters, digits or underscores.");
            }
            // An administrator's name too: whoever signs in with it is made
            // one, so it may only be the name of an account that exists
            // already (made before the name was listed). Otherwise anyone
            // could register it once its account was renamed or deleted.
            if server.is_reserved_name(&username) {
                return fail("That username is taken.");
            }
            if let Some(why) = moderation::name_refusal(server, &username) {
                return fail(&why);
            }
            if !valid_password(&password) {
                return fail("Passwords need at least 8 characters (not only digits).");
            }
            let country = country.trim().to_uppercase();
            let country = if valid_country(&country) { country } else { String::new() };
            if !server.auth().try_register(ip) {
                return fail("Too many new accounts from this network. Try again later.");
            }
            let Some(hash) = server.hash_password(&password).await else {
                server.auth().not_registered(ip);
                return fail("Could not create the account.");
            };
            let new = NewUser {
                username: &username,
                password_hash: &hash,
                country: &country,
                rating: EloConfig::default().initial_rating,
                role: Role::Player,
                ip,
                minor,
            };
            match server.store.create_user(&new).await {
                Ok(Some(user)) => {
                    tracing::info!("new account: {}", user.username);
                    match new_session(server, &user).await {
                        Some(token) => Auth::Ok { user: Box::new(user), token, guest: false },
                        None => fail("Could not create the account."),
                    }
                }
                Ok(None) => {
                    server.auth().not_registered(ip);
                    fail("That username is taken.")
                }
                Err(e) => {
                    server.auth().not_registered(ip);
                    tracing::error!("register failed: {e}");
                    fail("Could not create the account.")
                }
            }
        }
        ClientMsg::Login { username, password } => {
            if server.config.settings.lan {
                return fail("This is a LAN server: join as a guest.");
            }
            if !server.auth().try_sign_in(ip) {
                return fail(TOO_MANY_SIGN_INS);
            }
            let username = username.trim().to_string();
            // A name no account can have, or a password longer than any
            // password can be, is turned away before it costs anything (a
            // lookup, a hash, room in the limits' memory), like any wrong
            // password: the rules are no secret.
            if !valid_username(&username) || password.len() > MAX_PASSWORD_BYTES {
                return fail("Wrong username or password.");
            }
            // Counted as a failure until the password proves right.
            if !server.auth().try_password(ip, &username) {
                return fail("Too many failed attempts. Wait a few minutes and try again.");
            }
            let user = match server.store.user_by_name(&username).await {
                Ok(u) => u,
                Err(e) => {
                    server.auth().not_tried(ip, &username);
                    tracing::error!("login lookup failed: {e}");
                    return fail("The server is busy. Try again in a moment.");
                }
            };
            // Hash even for unknown names, so timing reveals nothing.
            let ok = match &user {
                Some(u) => server.verify_password(&password, &u.password_hash).await,
                None => server.verify_password(&password, DUMMY_HASH).await.map(|_| false),
            };
            let Some(ok) = ok else {
                server.auth().not_tried(ip, &username);
                return fail("The server is busy. Try again in a moment.");
            };
            match user {
                Some(user) if ok => {
                    server.auth().succeeded(ip, &username);
                    if user.is_banned(now()) {
                        return Auth::Failed(ban_message(&user));
                    }
                    if !server.auth().try_sign_in_as(user.id) {
                        return fail(TOO_MANY_SIGN_INS);
                    }
                    let user = promote_if_admin(server, user).await;
                    match new_session(server, &user).await {
                        Some(token) => Auth::Ok { user: Box::new(user), token, guest: false },
                        None => fail("The server is busy. Try again in a moment."),
                    }
                }
                _ => fail("Wrong username or password."),
            }
        }
        ClientMsg::Resume { token } => {
            if server.config.settings.lan {
                return fail("This is a LAN server: join as a guest.");
            }
            if !server.auth().try_sign_in(ip) {
                return Auth::Dropped;
            }
            let hash = token_hash(&token);
            match server.store.session_user(&hash, SessionKind::Game, now()).await {
                Ok(Some((user, created))) => {
                    if user.deleted_at.is_some() {
                        return fail("Your session expired. Please log in again.");
                    }
                    if user.is_banned(now()) {
                        return Auth::Failed(ban_message(&user));
                    }
                    if !server.auth().try_sign_in_as(user.id) {
                        return Auth::Dropped;
                    }
                    let user = promote_if_admin(server, user).await;
                    // Renew sessions in use, so active players stay signed in.
                    if now() - created > SESSION_RENEW_AFTER
                        && let Some(fresh) = new_session(server, &user).await
                    {
                        let _ = server.store.delete_session(&hash).await;
                        return Auth::Ok { user: Box::new(user), token: fresh, guest: false };
                    }
                    Auth::Ok { user: Box::new(user), token, guest: false }
                }
                Ok(None) => fail("Your session expired. Please log in again."),
                Err(e) => {
                    tracing::error!("resume failed: {e}");
                    fail("The server is busy. Try again in a moment.")
                }
            }
        }
        ClientMsg::Guest { name } => {
            if !server.config.settings.lan {
                return fail("Guests can only join LAN games. Create an account.");
            }
            let name = name.trim().to_string();
            if !valid_username(&name) {
                return fail("Names have 3–16 letters, digits or underscores.");
            }
            if server.with(|l| l.is_online(&name)) {
                return fail("Someone with that name is already here.");
            }
            let id = server.with(|l| l.guest_id());
            let t = now();
            let rating = EloConfig::default().initial_rating;
            let user = User {
                id,
                username: name,
                password_hash: String::new(),
                country: String::new(),
                role: Role::Player,
                records: PerRules::both(store::Record::new(rating)),
                eight_wins: 0,
                casual_games: 0,
                streak: 0,
                best_streak: 0,
                runouts: 0,
                shutouts: 0,
                tournaments_played: 0,
                tournaments_won: 0,
                created_at: t,
                last_seen: t,
                banned_until: None,
                ban_reason: None,
                muted_until: None,
                recovery_email: None,
                deleted_at: None,
                abandons: 0,
                status_message: String::new(),
                xp: 0,
                name_changes: 0,
                avatar: String::new(),
                minor: None,
                chat_allowed_at: None,
                chat_phrases_only: false,
            };
            Auth::Ok { user: Box::new(user), token: String::new(), guest: true }
        }
        _ => fail("Please log in first."),
    }
}

async fn handle_socket(server: Arc<Server>, socket: WebSocket, ip: IpAddr) {
    let conn_id = server.next_conn.fetch_add(1, Ordering::Relaxed);
    let ip_text = ip.to_string();
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::channel::<ServerMsg>(OUTGOING_QUEUE);
    let kill = Arc::new(Notify::new());

    let writer_kill = kill.clone();
    let mut writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let close = matches!(msg, ServerMsg::Kicked { .. });
            // A client that doesn't take a message in this long has stopped
            // reading: the connection goes (its socket with it).
            let text = Message::Text(msg.to_json().into());
            if !matches!(tokio::time::timeout(SEND_TIMEOUT, sink.send(text)).await, Ok(Ok(()))) {
                break;
            }
            if close {
                let _ = tokio::time::timeout(SEND_TIMEOUT, sink.send(Message::Close(None))).await;
                break;
            }
        }
        // Nothing more goes out: the reading side stops too.
        writer_kill.notify_one();
    });

    // A connection that can't keep up is closed (the game reconnects and
    // gets everything afresh), as the lounge does with its own messages: an
    // answer dropped on the floor left the game waiting for it.
    let send = |msg: ServerMsg| {
        if let Err(mpsc::error::TrySendError::Full(_)) = tx.try_send(msg) {
            tracing::warn!("{ip}: outgoing queue full, closing the connection");
            kill.notify_one();
        }
    };
    let mut hello = false;
    // The rules the player plays (the game says so in its hello), and its
    // protocol (what it understands).
    let mut rules = Rules::default();
    // The protocol of the player's game (its hello).
    let mut protocol = PROTOCOL_VERSION;
    let mut user: Option<(User, bool)> = None;
    let mut failures = 0;
    let mut token_in_use: Option<String> = None;
    // ~40 messages a second sustained (aim updates are ~20), short bursts
    // more (as set now: see `settings`).
    let mut flood = {
        let live = &server.ops.settings;
        let burst = live.int(settings::MESSAGE_BURST).max(1) as f64;
        Bucket::new(burst, live.int(settings::MESSAGES_PER_SEC).max(1) as f64)
    };
    let ConnLimits {
        shop: mut shop_bucket,
        store: mut store_bucket,
        query: mut query_bucket,
        status: mut status_bucket,
        dm: mut dm_bucket,
        settings: mut settings_bucket,
    } = ConnLimits::fresh();
    loop {
        let next = tokio::select! {
            // Checked first: a connection that was told to go (signed in
            // elsewhere, banned) reads nothing more, even with messages
            // already waiting.
            biased;
            _ = kill.notified() => break,
            m = tokio::time::timeout(IDLE_TIMEOUT, stream.next()) => m,
        };
        let next = match next {
            Ok(Some(Ok(m))) => m,
            _ => break,
        };
        // Every frame counts, whatever its kind (pings and binary frames too).
        if !flood.take() {
            tracing::warn!("{ip_text}: message flood, closing");
            send(ServerMsg::Kicked { reason: "Too many messages.".into() });
            break;
        }
        let text = match next {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };
        let Ok(msg) = parse_client(text.as_str()) else {
            send(ServerMsg::Error { message: "Malformed message.".into() });
            continue;
        };
        if !hello {
            match msg {
                ClientMsg::Hello { version: v, rules: r }
                    if (OLDEST_PROTOCOL_VERSION..=PROTOCOL_VERSION).contains(&v) =>
                {
                    hello = true;
                    rules = r;
                    protocol = v;
                }
                // A game newer than the server: the server hasn't been
                // updated yet (updating the game can't help).
                ClientMsg::Hello { version: v, .. } if v > PROTOCOL_VERSION => {
                    send(ServerMsg::AuthError {
                        reason: "The server is still being updated to the new version. Try again in a few minutes."
                            .into(),
                    });
                    break;
                }
                ClientMsg::Hello { .. } => {
                    send(ServerMsg::AuthError {
                        reason:
                            "This version of Snook Legend is out of date. Please update the game."
                                .into(),
                    });
                    break;
                }
                _ => break,
            }
            continue;
        }
        if is_query_msg(&msg) && !query_bucket.take() {
            send(ServerMsg::Error { message: "Slow down a little.".into() });
            continue;
        }
        let Some((me, guest)) = user.clone() else {
            match msg {
                msg @ (ClientMsg::Register { .. }
                | ClientMsg::Login { .. }
                | ClientMsg::Resume { .. }
                | ClientMsg::Guest { .. }) => match authenticate(&server, msg, &ip_text).await {
                    Auth::Ok { user: row, token, guest } => {
                        let row = *row;
                        if !guest {
                            restore_live(&server, &row).await;
                        }
                        let blocked: HashSet<i64> = if guest {
                            HashSet::new()
                        } else {
                            server
                                .store
                                .blocked(row.id)
                                .await
                                .unwrap_or_default()
                                .iter()
                                .map(|u| u.id)
                                .collect()
                        };
                        server.with(|l| {
                            l.join(
                                conn_id,
                                row.clone(),
                                guest,
                                rules,
                                tx.clone(),
                                kill.clone(),
                                token.clone(),
                                ip_text.clone(),
                                blocked,
                            );
                            l.set_protocol(&row.username, protocol);
                        });
                        if !guest && let Ok(records) = server.store.doubles_records(row.id).await {
                            server.with(|l| l.set_doubles_records(&row.username, records));
                        }
                        if !guest {
                            token_in_use = Some(token);
                            send(age_msg(&row));
                            let _ = server.store.touch(row.id, now()).await;
                            let s = server.clone();
                            let r = row.clone();
                            tokio::spawn(async move {
                                s.check_achievements(&r).await;
                                account::signed_in(&s, &r).await;
                                ops::signed_in(&s, &r).await;
                                shop::push_wallet(&s, &r).await;
                                social::signed_in(&s, &r).await;
                                if !s.config.settings.lan {
                                    correspondence::signed_in(&s, &r).await;
                                    prefs::signed_in(&s, &r).await;
                                    dm::signed_in(&s, &r).await;
                                }
                            });
                            if let Ok(friends) = server.store.friends(row.id).await {
                                // The lounge tells them what their friends
                                // do (and their friends what they do).
                                let names = friends
                                    .iter()
                                    .filter(|(_, st)| *st == FriendState::Friends)
                                    .map(|(u, _)| u.username.clone());
                                server.with(|l| {
                                    l.set_friends(&row.username, names);
                                    l.tell(&row.username);
                                });
                                if friends.iter().any(|(_, st)| *st == FriendState::Incoming) {
                                    send(friends_msg(&server, row.id, friends).await);
                                }
                            }
                        }
                        // The account's limits as its last connection left them.
                        if let Some(kept) = server.take_limits(row.id) {
                            ConnLimits {
                                shop: shop_bucket,
                                store: store_bucket,
                                query: query_bucket,
                                status: status_bucket,
                                dm: dm_bucket,
                                settings: settings_bucket,
                            } = kept;
                        }
                        user = Some((row, guest));
                    }
                    Auth::Failed(reason) => {
                        failures += 1;
                        send(ServerMsg::AuthError { reason });
                        if failures >= 5 {
                            break;
                        }
                    }
                    Auth::Dropped => break,
                },
                ClientMsg::Ping => send(ServerMsg::Pong),
                ClientMsg::Leaderboard { rules } => {
                    send(ServerMsg::Leaderboard { rules, entries: server.leaderboard(rules).await })
                }
                ClientMsg::DoublesLeaderboard { rules } => send(ServerMsg::DoublesLeaderboard {
                    rules,
                    entries: server.doubles_leaderboard(rules).await,
                }),
                ClientMsg::Profile { username } => {
                    // (Counted per address, as sign-ins are: a profile is a
                    // dozen queries, and a new connection has a full bucket.)
                    if !server.auth().try_sign_in(&ip_text) {
                        send(ServerMsg::Error { message: "Slow down a little.".into() });
                    } else if let Some(mut msg) = profile_msg(&server, None, &username).await {
                        analysis::attach(&server, &mut msg).await;
                        send(msg);
                    }
                }
                _ => send(ServerMsg::AuthError { reason: "Please log in first.".into() }),
            }
            continue;
        };
        // One place at a time: once the account has signed in on another
        // connection, this one acts for nobody (whatever the game on the
        // other end does with the notice it was sent).
        if !server.with(|l| l.is_current(&me.username, conn_id)) {
            send(ServerMsg::Kicked { reason: "You signed in from another place.".into() });
            break;
        }
        match msg {
            ClientMsg::Logout => {
                if let Some(t) = token_in_use.take() {
                    let _ = server.store.delete_session(&token_hash(&t)).await;
                }
                break;
            }
            ClientMsg::Leaderboard { rules } => {
                send(ServerMsg::Leaderboard { rules, entries: server.leaderboard(rules).await })
            }
            ClientMsg::DoublesLeaderboard { rules } => send(ServerMsg::DoublesLeaderboard {
                rules,
                entries: server.doubles_leaderboard(rules).await,
            }),
            ClientMsg::Profile { username } => {
                let viewer = (!guest).then_some(&me);
                match profile_msg(&server, viewer, &username).await {
                    Some(mut msg) => {
                        analysis::attach(&server, &mut msg).await;
                        send(msg)
                    }
                    None => send(ServerMsg::Error { message: "No such player.".into() }),
                }
            }
            // Friends only (anyone on a local network, where there are none).
            ClientMsg::Spectate { username } => {
                let allowed = server.config.settings.lan
                    || (!guest
                        && server.store.friends(me.id).await.is_ok_and(|list| {
                            list.iter().any(|(u, st)| {
                                *st == FriendState::Friends
                                    && u.username.eq_ignore_ascii_case(&username)
                            })
                        }));
                if allowed {
                    server.with(|l| l.spectate(&lounge::key(&me.username), &username));
                } else {
                    send(ServerMsg::Error { message: "You can only watch your friends.".into() });
                }
            }
            msg if guest && is_account_msg(&msg) => {
                send(ServerMsg::Error { message: "Guests don't have this.".into() })
            }
            msg @ (ClientMsg::Account
            | ClientMsg::ChangePassword { .. }
            | ClientMsg::SetRecoveryEmail { .. }
            | ClientMsg::DeleteAccount { .. }) => {
                let deleting = matches!(msg, ClientMsg::DeleteAccount { .. });
                if account::handle(&server, &me, token_in_use.as_deref(), &ip_text, msg, &send)
                    .await
                {
                    if deleting {
                        // Signed in from another place while it was being
                        // deleted: that connection goes too, with any
                        // session made meanwhile (it would stay online
                        // under a name that is free again).
                        server.with(|l| {
                            l.leave(&me.username, conn_id);
                            l.kick(&me.username, "This account was deleted.");
                        });
                        let _ = server.store.delete_user_sessions(me.id).await;
                    }
                    break;
                }
            }
            ClientMsg::ChangeUsername { username } => {
                if let Some(fresh) = account::change_username(&server, &me, &username, &send).await
                {
                    // The lounge knows players by name: sign in again under
                    // the new one, on this same connection.
                    let token = token_in_use.clone().unwrap_or_default();
                    server.with(|l| l.renamed(&me.username, conn_id, fresh.clone(), token));
                    user = Some((fresh, guest));
                }
            }
            ClientMsg::SetAge { minor } => {
                match server.store.set_minor(me.id, minor).await {
                    Ok(true) => {
                        let mut fresh = me.clone();
                        fresh.minor = Some(minor);
                        server
                            .with(|l| l.set_age(&me.username, fresh.minor, fresh.chat_allowed_at));
                        tracing::info!(player = %me.username, minor, "age group given");
                        send(age_msg(&fresh));
                        user = Some((fresh, guest));
                    }
                    // Given before: it stays (the game is told what it is).
                    Ok(false) => send(age_msg(&me)),
                    Err(e) => {
                        tracing::error!("age of {}: {e}", me.username);
                        send(ServerMsg::Error {
                            message: "Something went wrong. Please try again.".into(),
                        });
                    }
                }
            }
            ClientMsg::AllowChat { allowed, phrases_only } => {
                if me.minor != Some(true) {
                    send(age_msg(&me));
                } else {
                    let at = allowed.then(now);
                    // The quick phrases only: as the parent or guardian
                    // chose (a game before 0.12 doesn't say: it stays).
                    let phrases = phrases_only.unwrap_or(me.chat_phrases_only);
                    let kept = if phrases == me.chat_phrases_only {
                        Ok(())
                    } else {
                        server.store.set_phrases_only(me.id, phrases).await
                    };
                    match kept.and(server.store.set_chat_allowed(me.id, at).await) {
                        Ok(()) => {
                            let mut fresh = me.clone();
                            fresh.chat_allowed_at = at;
                            fresh.chat_phrases_only = phrases;
                            server.with(|l| {
                                l.set_phrases_only(&me.username, phrases);
                                l.set_age(&me.username, fresh.minor, at)
                            });
                            // Chat turned off: what they wrote for others
                            // to read (the status on their card, a note on
                            // the board) goes as well.
                            if !allowed {
                                let _ = server.store.set_status_message(me.id, "").await;
                                fresh.status_message.clear();
                                server.with(|l| l.silenced(&me.username));
                            }
                            tracing::info!(player = %me.username, allowed, phrases, "chat for a minor");
                            send(age_msg(&fresh));
                            user = Some((fresh, guest));
                        }
                        Err(e) => {
                            tracing::error!("chat consent of {}: {e}", me.username);
                            send(ServerMsg::Error {
                                message: "Something went wrong. Please try again.".into(),
                            });
                        }
                    }
                }
            }
            ClientMsg::Missions => send(missions::message(&server, &me).await),
            msg if correspondence::is_msg(&msg) => {
                correspondence::handle(&server, &me, protocol, msg, &send).await
            }
            msg if daily::is_msg(&msg) => daily::handle(&server, &me, msg, &send).await,
            msg if time_attack::is_msg(&msg) => time_attack::handle(&server, &me, msg, &send).await,
            msg if drills::is_msg(&msg) => drills::handle(&server, &me, msg, &send).await,
            msg if cup_medals::is_msg(&msg) => cup_medals::handle(&server, &me, msg, &send).await,
            msg if inbox::is_msg(&msg) => inbox::handle(&server, &me, msg, &send).await,
            msg if moderation::is_msg(&msg) => moderation::handle(&server, &me, msg, &send).await,
            // The mentions of players who aren't online are looked up first.
            ClientMsg::Chat { text } => moderation::chat(&server, &me, text).await,
            msg if collection::is_msg(&msg) => collection::handle(&server, &me, msg, &send).await,
            msg if ops::is_msg(&msg) => ops::handle(&server, &me, msg, &send).await,
            msg if dm::is_msg(&msg) => dm::handle(&server, &me, msg, &send, &mut dm_bucket).await,
            msg if prefs::is_msg(&msg) => {
                if settings_bucket.take() {
                    prefs::handle(&server, &me, msg, &send).await
                } else {
                    send(ServerMsg::Error { message: "Slow down a little.".into() });
                }
            }
            ClientMsg::AdvancedStats { rules } => {
                advanced::handle(&server, &me, rules, &send).await
            }
            ClientMsg::LadderTicket => send(computer::ticket(&server, &me).await),
            // Seconds of simulation each: answered on their own tasks.
            ClientMsg::LadderClaim { replay } => {
                computer::spawn_claim(server.clone(), me.clone(), replay, tx.clone())
            }
            ClientMsg::GhostStyle { username } => {
                computer::spawn_ghost(server.clone(), me.clone(), username, tx.clone())
            }
            // Someone else's note on the "looking for a game" board: staff
            // only (told so before the database is asked anything).
            ClientMsg::RemoveSeek { username }
                if !username.is_empty() && !username.eq_ignore_ascii_case(&me.username) =>
            {
                if !me.role.is_staff() {
                    send(ServerMsg::Error { message: "You can't do that.".into() });
                    continue;
                }
                if !query_bucket.take() {
                    send(ServerMsg::Error { message: "Slow down a little.".into() });
                    continue;
                }
                social::handle(&server, &me, ClientMsg::RemoveSeek { username }, &send).await;
            }
            msg @ (ClientMsg::SetStatusMessage { .. }
            | ClientMsg::ClearStatusMessage { .. }
            | ClientMsg::RecentOpponents
            | ClientMsg::Gifts
            | ClientMsg::GiftsSeen { .. }
            | ClientMsg::ThankGift { .. }) => {
                // A few status changes a minute at most.
                if matches!(msg, ClientMsg::SetStatusMessage { .. }) && !status_bucket.take() {
                    send(ServerMsg::Error { message: "Slow down a little.".into() });
                    continue;
                }
                if let Some(fresh) = social::handle(&server, &me, msg, &send).await {
                    user = Some((fresh, guest));
                }
            }
            ClientMsg::Replays => {
                let vip =
                    shop::is_vip(&server.store.wallet(me.id).await.unwrap_or_default(), now());
                let limit = if vip { 0 } else { FREE_REPLAYS };
                let games = server
                    .store
                    .history(me.id, if vip { 200 } else { FREE_REPLAYS })
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|h| h.replay)
                    .map(history_entry)
                    .collect();
                send(ServerMsg::Replays { games, limit });
            }
            ClientMsg::Replay { match_id } => match replay_for(&server, &me, match_id).await {
                Some(replay) => send(ServerMsg::Replay { replay: Box::new(replay) }),
                None => send(ServerMsg::Error { message: "This replay is not available.".into() }),
            },
            msg @ (ClientMsg::Shop
            | ClientMsg::BuyItem { .. }
            | ClientMsg::GiftItem { .. }
            | ClientMsg::Equip { .. }
            | ClientMsg::BuyCoins { .. }
            | ClientMsg::SubscribeVip
            | ClientMsg::ManageVip
            | ClientMsg::GiftVip { .. }
            | ClientMsg::BuyWelcomePack) => {
                // Opening payment pages costs a call to the provider: a few
                // per minute at most.
                if shop_bucket.take() {
                    shop::handle(&server, &me, msg, &send).await;
                } else {
                    send(ServerMsg::Error { message: "Slow down a little.".into() });
                }
            }
            msg @ (ClientMsg::StoreTicket | ClientMsg::StoreRedeem { .. }) => {
                // Each one costs calls to Microsoft: a few per minute at most
                // (a purchase and the looks after it fit). The answer is one
                // the Store's buttons understand, so they don't keep waiting.
                if store_bucket.take() {
                    msstore::handle(&server, &me, msg, &send).await;
                } else {
                    send(ServerMsg::StoreRedeemed {
                        coins: 0,
                        pending: false,
                        error: "Slow down a little.".into(),
                        vip_until: 0,
                    });
                }
            }
            ClientMsg::SetAvatar { avatar } => {
                // A retired face (from an older client) is kept as the
                // thing of the game shown in its place.
                let avatar = snook_legend_proto::shown_avatar(avatar.trim()).to_string();
                if valid_avatar(&avatar)
                    && avatar != me.avatar
                    && server.store.set_avatar(me.id, &avatar).await.is_ok()
                {
                    let mut fresh = me.clone();
                    fresh.avatar = avatar;
                    server.with(|l| {
                        if let Some(u) = l.session_user(&fresh.username) {
                            let mut u = u.clone();
                            u.avatar = fresh.avatar.clone();
                            l.refresh_user(u);
                        }
                    });
                    user = Some((fresh, guest));
                }
            }
            ClientMsg::SetCountry { country } => {
                let country = country.trim().to_uppercase();
                if valid_country(&country)
                    && country != me.country
                    && server.store.set_country(me.id, &country).await.is_ok()
                {
                    use snook_legend_proto::moderation::ChangeKind;
                    moderation::record_change(
                        &server,
                        me.id,
                        ChangeKind::Country,
                        &me.country,
                        &country,
                        None,
                    )
                    .await;
                    let mut fresh = me.clone();
                    fresh.country = country;
                    server.with(|l| {
                        if let Some(u) = l.session_user(&fresh.username) {
                            let mut u = u.clone();
                            u.country = fresh.country.clone();
                            l.refresh_user(u);
                        }
                    });
                    user = Some((fresh, guest));
                    server.invalidate_leaderboard();
                }
            }
            ClientMsg::Friends => {
                if let Ok(list) = server.store.friends(me.id).await {
                    send(friends_msg(&server, me.id, list).await);
                }
            }
            ClientMsg::FriendRequest { username } => {
                friend_request(&server, &me, &username, &send).await
            }
            ClientMsg::FriendAnswer { username, accept } => {
                if let Ok(Some(other)) = server.store.user_by_name(&username).await {
                    // The same limit as for asking.
                    let full = accept
                        && server.store.friends(me.id).await.unwrap_or_default().len()
                            >= MAX_FRIENDS as usize;
                    if full {
                        send(ServerMsg::Error { message: "Your friend list is full.".into() });
                        continue;
                    }
                    if accept {
                        if server.store.friend_accept(me.id, other.id).await.unwrap_or(false) {
                            server.check_achievements(&me).await;
                            server.check_achievements(&other).await;
                            refresh_friends(&server, &other).await;
                        }
                    } else {
                        let _ = server.store.friend_remove(me.id, other.id).await;
                        server.with(|l| l.unfriended(&me.username, &other.username));
                    }
                    refresh_friends(&server, &me).await;
                }
            }
            ClientMsg::FriendRemove { username } => {
                if let Ok(Some(other)) = server.store.user_by_name(&username).await {
                    let _ = server.store.friend_remove(me.id, other.id).await;
                    server.with(|l| l.unfriended(&me.username, &other.username));
                    refresh_friends(&server, &me).await;
                    refresh_friends(&server, &other).await;
                }
            }
            ClientMsg::Block { username } => {
                if let Ok(Some(other)) = server.store.user_by_name(&username).await
                    && other.id != me.id
                {
                    let _ = server.store.block(me.id, other.id).await;
                    let _ = server.store.friend_remove(me.id, other.id).await;
                    update_blocked(&server, &me).await;
                    server.with(|l| l.blocked_player(&me.username, &other.username));
                    refresh_friends(&server, &me).await;
                    refresh_friends(&server, &other).await;
                }
            }
            ClientMsg::Unblock { username } => {
                if let Ok(Some(other)) = server.store.user_by_name(&username).await {
                    let _ = server.store.unblock(me.id, other.id).await;
                    update_blocked(&server, &me).await;
                    refresh_friends(&server, &me).await;
                }
            }
            ClientMsg::Report { username, reason, details } => {
                report(&server, &me, &username, reason, &details, &send).await
            }
            // Tournaments with an entry fee: everything that can be checked
            // is before the coins are taken; each payment has a ledger
            // reference of its own, which its refund names (paying again
            // after leaving is a new payment, refunded on its own).
            ClientMsg::JoinTournament { id, password } if !guest => {
                let key = lounge::key(&me.username);
                // Announced to everyone, like the lounge's own actions.
                if !server.with(|l| l.take_action(&key)) {
                    continue;
                }
                let Some(fee) = server.with(|l| l.tournament_entry(&key, id, password.as_deref()))
                else {
                    continue;
                };
                let mut paid_with = None;
                if fee > 0 {
                    let reference = format!("tour:{id}:{}:{}", me.id, now_millis());
                    if !take_fee(&server, &me, fee, &reference, &send).await {
                        continue;
                    }
                    paid_with = Some(reference);
                }
                let joined =
                    server.with(|l| l.join_tournament(&key, id, password, paid_with.clone()));
                if let Some(reference) = paid_with {
                    if joined == lounge::Joined::No {
                        let reference = format!("refund:{reference}");
                        pay(&server, me.id, &me.username, fee as i64, "refund", &reference).await;
                    }
                    shop::push_wallet(&server, &me).await;
                }
            }
            ClientMsg::CreateTournament {
                name,
                size,
                rated,
                rules,
                password,
                entry_fee,
                format,
            } if !guest && entry_fee > 0 => {
                let key = lounge::key(&me.username);
                let fee = entry_fee.min(lounge::tournaments_max_fee());
                let may = server.with(|l| {
                    l.take_action(&key)
                        && l.may_create_tournament(
                            &key,
                            &name,
                            size,
                            password.as_deref(),
                            fee,
                            format,
                        )
                });
                if !may {
                    continue;
                }
                let reference = format!("tour:new:{}:{}", me.id, now_millis());
                if !take_fee(&server, &me, fee, &reference, &send).await {
                    continue;
                }
                let paid = Some(reference.clone());
                let created = server.with(|l| {
                    let offer = lounge::Offer { rated, rules };
                    l.create_tournament(&key, &name, size, offer, password, fee, format, paid)
                });
                if created.is_none() {
                    let reference = format!("refund:{reference}");
                    pay(&server, me.id, &me.username, fee as i64, "refund", &reference).await;
                }
                shop::push_wallet(&server, &me).await;
            }
            other => {
                let name = me.username.clone();
                server.with(|l| l.handle(&name, other));
            }
        }
    }

    if let Some((u, guest)) = user {
        let limits = ConnLimits {
            shop: shop_bucket,
            store: store_bucket,
            query: query_bucket,
            status: status_bucket,
            dm: dm_bucket,
            settings: settings_bucket,
        };
        server.keep_limits(u.id, limits);
        server.with(|l| l.leave(&u.username, conn_id));
        if !guest {
            let _ = server.store.touch(u.id, now()).await;
        }
    }
    drop(tx);
    // The last messages (a notice to go) have a moment to leave; then the
    // writer ends, holding the socket no longer (a writer left behind kept
    // a connection open that the limit per address no longer counted).
    if tokio::time::timeout(Duration::from_secs(2), &mut writer).await.is_err() {
        writer.abort();
    }
}

/// A connection's own limits, kept for its account a while after it closes
/// (see [`Server::keep_limits`]).
#[derive(Clone)]
struct ConnLimits {
    /// Shop actions (some open payment pages at the provider).
    shop: Bucket,
    store: Bucket,
    /// Requests that read or write the database (profiles, friends, blocks,
    /// reports…): plenty for a person, far too few to wear the database down.
    query: Bucket,
    /// Status messages (everyone in the lounge sees each change).
    status: Bucket,
    /// Direct messages (see [`dm::bucket`]).
    dm: Bucket,
    /// The account's settings (the game sends them a moment after a change).
    settings: Bucket,
}

impl ConnLimits {
    fn fresh() -> ConnLimits {
        ConnLimits {
            shop: Bucket::new(6.0, 0.1),
            store: Bucket::new(12.0, 0.2),
            query: Bucket::new(30.0, 2.0),
            status: Bucket::new(4.0, 1.0 / 15.0),
            dm: dm::bucket(),
            settings: Bucket::new(6.0, 0.1),
        }
    }
}

/// How long an account's connection limits outlive its connection.
const LIMITS_KEPT: Duration = Duration::from_secs(600);

impl Server {
    fn keep_limits(&self, id: i64, limits: ConnLimits) {
        let mut kept = self.kept_limits.lock().unwrap_or_else(|e| e.into_inner());
        kept.retain(|_, (_, at)| at.elapsed() < LIMITS_KEPT);
        kept.insert(id, (limits, Instant::now()));
    }

    fn take_limits(&self, id: i64) -> Option<ConnLimits> {
        let mut kept = self.kept_limits.lock().unwrap_or_else(|e| e.into_inner());
        kept.remove(&id).filter(|(_, at)| at.elapsed() < LIMITS_KEPT).map(|(l, _)| l)
    }
}

/// Messages answered from the database.
fn is_query_msg(msg: &ClientMsg) -> bool {
    matches!(
        msg,
        ClientMsg::Profile { .. }
            | ClientMsg::Spectate { .. }
            | ClientMsg::SetCountry { .. }
            | ClientMsg::SetAvatar { .. }
            | ClientMsg::Friends
            | ClientMsg::FriendRequest { .. }
            | ClientMsg::FriendAnswer { .. }
            | ClientMsg::FriendRemove { .. }
            | ClientMsg::Block { .. }
            | ClientMsg::Unblock { .. }
            | ClientMsg::Report { .. }
            | ClientMsg::Account
            | ClientMsg::ChangePassword { .. }
            | ClientMsg::SetRecoveryEmail { .. }
            | ClientMsg::DeleteAccount { .. }
            | ClientMsg::ChangeUsername { .. }
            | ClientMsg::SetAge { .. }
            | ClientMsg::AllowChat { .. }
            | ClientMsg::Replays
            | ClientMsg::Replay { .. }
            | ClientMsg::Missions
            | ClientMsg::CorrList
            | ClientMsg::CorrChallenge { .. }
            | ClientMsg::CorrAnswer { .. }
            | ClientMsg::CorrCancel { .. }
            | ClientMsg::CorrOpen { .. }
            | ClientMsg::CorrShoot { .. }
            | ClientMsg::CorrChoose { .. }
            | ClientMsg::CorrResign { .. }
            | ClientMsg::LadderClaim { .. }
            | ClientMsg::GhostStyle { .. }
            | ClientMsg::Inbox
            | ClientMsg::InboxRead { .. }
            | ClientMsg::InboxDelete { .. }
            | ClientMsg::Collection
            | ClientMsg::Auctions
            | ClientMsg::Bid { .. }
            | ClientMsg::SetCredits { .. }
            | ClientMsg::Supporters
            | ClientMsg::Daily
            | ClientMsg::DailyStart
            | ClientMsg::DailySubmit { .. }
            | ClientMsg::Drills
            | ClientMsg::DrillSubmit { .. }
            | ClientMsg::DrillStart { .. }
            | ClientMsg::LadderTicket
            | ClientMsg::CupMedals
            | ClientMsg::CupMedal { .. }
            | ClientMsg::SetStatusMessage { .. }
            | ClientMsg::ClearStatusMessage { .. }
            | ClientMsg::RecentOpponents
            | ClientMsg::Gifts
            | ClientMsg::GiftsSeen { .. }
            | ClientMsg::ThankGift { .. }
            | ClientMsg::DmHistory { .. }
            | ClientMsg::DmRead { .. }
            | ClientMsg::DmThreads
            | ClientMsg::AdvancedStats { .. }
            | ClientMsg::DoublesLeaderboard { .. }
            | ClientMsg::TimeAttack
            | ClientMsg::TimeAttackStart
            | ClientMsg::TimeAttackSubmit { .. }
    )
}

/// The player's age group, as their game is told it.
fn age_msg(user: &User) -> ServerMsg {
    ServerMsg::Age {
        minor: user.minor,
        chat_allowed: user.chat_allowed_at.is_some(),
        phrases_only: lounge::phrases_only(user),
    }
}

fn is_account_msg(msg: &ClientMsg) -> bool {
    matches!(
        msg,
        ClientMsg::LadderClaim { .. }
            | ClientMsg::GhostStyle { .. }
            | ClientMsg::Inbox
            | ClientMsg::InboxRead { .. }
            | ClientMsg::InboxDelete { .. }
            | ClientMsg::Collection
            | ClientMsg::Auctions
            | ClientMsg::Bid { .. }
            | ClientMsg::SetCredits { .. }
            | ClientMsg::Supporters
            | ClientMsg::Daily
            | ClientMsg::DailyStart
            | ClientMsg::DailySubmit { .. }
            | ClientMsg::Drills
            | ClientMsg::DrillSubmit { .. }
            | ClientMsg::DrillStart { .. }
            | ClientMsg::LadderTicket
            | ClientMsg::CupMedals
            | ClientMsg::CupMedal { .. }
            | ClientMsg::SetCountry { .. }
            | ClientMsg::SetAvatar { .. }
            | ClientMsg::Friends
            | ClientMsg::FriendRequest { .. }
            | ClientMsg::FriendAnswer { .. }
            | ClientMsg::FriendRemove { .. }
            | ClientMsg::Block { .. }
            | ClientMsg::Unblock { .. }
            | ClientMsg::Report { .. }
            | ClientMsg::Shop
            | ClientMsg::BuyItem { .. }
            | ClientMsg::Equip { .. }
            | ClientMsg::BuyCoins { .. }
            | ClientMsg::SubscribeVip
            | ClientMsg::ManageVip
            | ClientMsg::GiftVip { .. }
            | ClientMsg::BuyWelcomePack
            | ClientMsg::StoreTicket
            | ClientMsg::StoreRedeem { .. }
            | ClientMsg::GiftItem { .. }
            | ClientMsg::Account
            | ClientMsg::ChangePassword { .. }
            | ClientMsg::SetRecoveryEmail { .. }
            | ClientMsg::DeleteAccount { .. }
            | ClientMsg::ChangeUsername { .. }
            | ClientMsg::SetAge { .. }
            | ClientMsg::AllowChat { .. }
            | ClientMsg::Replays
            | ClientMsg::Missions
            | ClientMsg::Replay { .. }
            | ClientMsg::CreateInviteCode { .. }
            | ClientMsg::CorrList
            | ClientMsg::CorrChallenge { .. }
            | ClientMsg::CorrAnswer { .. }
            | ClientMsg::CorrCancel { .. }
            | ClientMsg::CorrOpen { .. }
            | ClientMsg::CorrShoot { .. }
            | ClientMsg::CorrResign { .. }
            | ClientMsg::SetStatusMessage { .. }
            | ClientMsg::ClearStatusMessage { .. }
            | ClientMsg::RecentOpponents
            | ClientMsg::Gifts
            | ClientMsg::GiftsSeen { .. }
            | ClientMsg::ThankGift { .. }
            | ClientMsg::SaveSettings { .. }
            | ClientMsg::DmSend { .. }
            | ClientMsg::DmHistory { .. }
            | ClientMsg::DmRead { .. }
            | ClientMsg::DmThreads
            | ClientMsg::AdvancedStats { .. }
            | ClientMsg::TimeAttack
            | ClientMsg::TimeAttackStart
            | ClientMsg::TimeAttackSubmit { .. }
    )
}

/// The notes on the board as players see them, with the players each
/// mentions (`@name`) looked up.
async fn board_notes(store: &Store) -> store::StoreResult<Vec<snook_legend_proto::BoardNote>> {
    let mut notes = Vec::new();
    for n in store.board_notes(now()).await? {
        let mentions = moderation::known_mentions_in(store, &n.text).await;
        notes.push(snook_legend_proto::BoardNote { mentions, ..board_note(n) });
    }
    Ok(notes)
}

fn board_note(n: crate::store::BoardNote) -> snook_legend_proto::BoardNote {
    snook_legend_proto::BoardNote {
        mentions: Vec::new(),
        id: n.id,
        text: n.text,
        author: n.author,
        at: n.created_at,
        expires_at: n.expires_at,
        kind: snook_legend_proto::NoteKind::from_code(&n.kind),
        pinned: n.pinned,
        action: n.action.as_deref().and_then(snook_legend_proto::NoteAction::from_code),
    }
}

/// The most friends (and requests) a player has.
const MAX_FRIENDS: u32 = 300;

/// Free players keep the replays of their last games; VIP members all of them.
pub const FREE_REPLAYS: u32 = 10;

fn history_entry(h: crate::store::HistoryRow) -> HistoryEntry {
    HistoryEntry {
        opponent: h.opponent,
        opponent_country: h.opponent_country,
        opponent_rating: h.opponent_rating,
        opponent_admin: h.opponent_admin,
        won: h.won,
        reason: parse_reason(&h.reason),
        rated: h.rated,
        rules: h.rules,
        delta: h.delta,
        rating_after: h.rating_after,
        tournament: h.tournament,
        at: h.at,
        match_id: h.match_id,
        replay: h.replay,
        correspondence: h.correspondence,
    }
}

/// A stored game's replay, for a player of that game (within their replay
/// allowance) or staff.
async fn replay_for(
    server: &Server,
    me: &User,
    match_id: i64,
) -> Option<snook_legend_proto::ReplayInfo> {
    let r = server.store.replay(match_id).await.ok().flatten()?;
    if !me.role.is_staff() {
        let mine = r.players.iter().any(|p| p.eq_ignore_ascii_case(&me.username));
        if !mine {
            return None;
        }
        let vip = shop::is_vip(&server.store.wallet(me.id).await.unwrap_or_default(), now());
        if !vip {
            let recent = server.store.history(me.id, FREE_REPLAYS).await.ok()?;
            if !recent.iter().any(|h| h.match_id == match_id) {
                return None;
            }
        }
    }
    Some(snook_legend_proto::ReplayInfo {
        match_id: r.id,
        players: r.players,
        countries: r.countries,
        ratings: r.ratings,
        winner: r.winner,
        rated: r.rated,
        at: r.at,
        replay: r.replay,
    })
}

async fn friends_msg(server: &Server, user_id: i64, list: Vec<(User, FriendState)>) -> ServerMsg {
    let blocked =
        server.store.blocked(user_id).await.unwrap_or_default().into_iter().map(|u| u.username);
    let mut friends = Vec::new();
    let mut incoming = Vec::new();
    let mut outgoing = Vec::new();
    server.with(|l| {
        for (u, state) in list {
            match state {
                FriendState::Friends => {
                    let online = l.player(&u.username);
                    friends.push(FriendInfo {
                        online: online.is_some(),
                        last_seen: u.last_seen,
                        activity: l.activity(&lounge::key(&u.username)).unwrap_or_default(),
                        player: online.unwrap_or_else(|| {
                            player_info(&u, false, PlayerStatus::Available, Rules::default())
                        }),
                    })
                }
                FriendState::Incoming => incoming.push(u.username),
                FriendState::Outgoing => outgoing.push(u.username),
            }
        }
    });
    friends.sort_by(|a, b| b.online.cmp(&a.online).then(a.player.username.cmp(&b.player.username)));
    ServerMsg::Friends { friends, incoming, outgoing, blocked: blocked.collect() }
}

/// Sends a fresh friend list to `user` if they are online.
async fn refresh_friends(server: &Server, user: &User) {
    if !server.with(|l| l.is_online(&user.username)) {
        return;
    }
    if let Ok(list) = server.store.friends(user.id).await {
        let names: Vec<String> = list
            .iter()
            .filter(|(_, st)| *st == FriendState::Friends)
            .map(|(u, _)| u.username.clone())
            .collect();
        let msg = friends_msg(server, user.id, list).await;
        server.with(|l| {
            l.set_friends(&user.username, names);
            l.notify(&user.username, msg);
            // A new friend learns at once what this player is doing.
            l.tell(&user.username);
        });
    }
}

async fn update_blocked(server: &Server, user: &User) {
    let ids =
        server.store.blocked(user.id).await.unwrap_or_default().iter().map(|u| u.id).collect();
    server.with(|l| l.set_blocked(&user.username, ids));
}

async fn friend_request(server: &Server, me: &User, name: &str, send: &impl Fn(ServerMsg)) {
    let other = match server.store.user_by_name(name.trim()).await {
        Ok(Some(o)) if o.id != me.id => o,
        _ => return send(ServerMsg::Error { message: "No such player.".into() }),
    };
    let blocked_by_them =
        server.store.blocked(other.id).await.unwrap_or_default().iter().any(|u| u.id == me.id);
    if blocked_by_them {
        // Don't reveal the block: it looks like a sent request.
        return send(ServerMsg::Notice { text: "Friend request sent.".into() });
    }
    let current = server.store.friends(me.id).await.unwrap_or_default();
    if current.len() >= MAX_FRIENDS as usize {
        return send(ServerMsg::Error { message: "Your friend list is full.".into() });
    }
    match server.store.friend_request(me.id, other.id, now()).await {
        Ok(FriendOutcome::Sent) => {
            server.with(|l| {
                l.notify(
                    &other.username,
                    ServerMsg::FriendRequestReceived { from: me.username.clone() },
                )
            });
            send(ServerMsg::Notice { text: "Friend request sent.".into() });
            refresh_friends(server, &other).await;
        }
        Ok(FriendOutcome::Accepted) => {
            server.check_achievements(me).await;
            server.check_achievements(&other).await;
            refresh_friends(server, &other).await;
        }
        Ok(FriendOutcome::AlreadyFriends | FriendOutcome::AlreadySent) => {}
        Err(e) => tracing::error!("friend request: {e}"),
    }
    refresh_friends(server, me).await;
}

async fn report(
    server: &Server,
    me: &User,
    name: &str,
    reason: snook_legend_proto::ReportReason,
    details: &str,
    send: &impl Fn(ServerMsg),
) {
    let Ok(Some(target)) = server.store.user_by_name(name).await else {
        return send(ServerMsg::Error { message: "No such player.".into() });
    };
    if target.id == me.id {
        return;
    }
    let key_ = format!("report:{}", me.id);
    if !server.auth().try_register(&key_) {
        return send(ServerMsg::Error {
            message: "You've sent many reports. Please wait before sending more.".into(),
        });
    }
    let context = server.with(|l| l.report_context(&me.username, &target.username));
    // The game between the two goes with the report: the staff can watch
    // it again instead of taking anyone's word for it.
    let at = now();
    let (match_id, live_id) = reports::game_for(server, me, &target, at).await;
    let r = NewReport {
        reporter: me.id,
        target: target.id,
        reason: reason.name().to_string(),
        details: clean_text(details, 500),
        context,
        at,
    };
    match server.store.create_report_with_game(&r, match_id, live_id).await {
        Ok(_) => {
            let attached = match_id.is_some() || live_id.is_some();
            let text = if attached {
                "Thank you. The moderators will look into it, with your game attached."
            } else {
                "Thank you. The moderators will look into it."
            };
            send(ServerMsg::Notice { text: text.into() })
        }
        Err(e) => tracing::error!("report: {e}"),
    }
}

async fn profile_msg(server: &Server, viewer: Option<&User>, name: &str) -> Option<ServerMsg> {
    // LAN guests have no stored profile: build one from the session.
    let user = match server.store.user_by_name(name.trim()).await.ok().flatten() {
        Some(u) => u,
        None => {
            let player = server.with(|l| l.player(name))?;
            return Some(ServerMsg::Profile {
                profile: Box::new(Profile {
                    player,
                    online: true,
                    stats: Default::default(),
                    created_at: now(),
                    ranks: PerRules::default(),
                    peaks: PerRules::both(EloConfig::default().initial_rating),
                    achievements: Vec::new(),
                    history: Vec::new(),
                    relation: Relation::None,
                    last_seen: 0,
                    rating_history: PerRules::default(),
                    career: Default::default(),
                    abandon_rate: 0,
                    analysis: None,
                    cue_wins: Vec::new(),
                    cue_stats: Vec::new(),
                    trophies: Vec::new(),
                    reactions: Vec::new(),
                    renames: Vec::new(),
                }),
            });
        }
    };
    if user.deleted_at.is_some() {
        return None;
    }
    let (achievements, history, brazilian_rank, official_rank, friends) = tokio::join!(
        server.store.achievements(user.id),
        server.store.history(user.id, 25),
        server.store.rank_of(user.id, Rules::Brazilian),
        server.store.rank_of(user.id, Rules::Official),
        server.store.friend_count(user.id),
    );
    let (brazilian_history, official_history, career, wallet, trophies) = tokio::join!(
        server.store.rating_history(user.id, Rules::Brazilian, 100),
        server.store.rating_history(user.id, Rules::Official, 100),
        server.store.career(user.id),
        server.store.wallet(user.id),
        trophies_of(server, user.id),
    );
    let wallet = wallet.unwrap_or_default();
    let renames = moderation::renames(server, user.id).await;
    let played = user.stats(now()).games + user.casual_games;
    let abandon_rate = (user.abandons * 100).checked_div(played).unwrap_or(0).min(100);
    let mut stats = user.stats(now());
    stats.friends = friends.unwrap_or(0);
    let relation = match viewer {
        None => Relation::None,
        Some(v) if v.id == user.id => Relation::Me,
        Some(v) => {
            let blocked = server
                .store
                .blocked(v.id)
                .await
                .unwrap_or_default()
                .iter()
                .any(|b| b.id == user.id);
            if blocked {
                Relation::Blocked
            } else {
                match server
                    .store
                    .friends(v.id)
                    .await
                    .unwrap_or_default()
                    .iter()
                    .find(|(u, _)| u.id == user.id)
                    .map(|(_, s)| *s)
                {
                    Some(FriendState::Friends) => Relation::Friend,
                    Some(FriendState::Outgoing) => Relation::RequestSent,
                    Some(FriendState::Incoming) => Relation::RequestReceived,
                    None => Relation::None,
                }
            }
        }
    };
    let online = server.with(|l| l.player(&user.username));
    let known: HashSet<&str> = ACHIEVEMENTS.iter().map(|a| a.code).collect();
    Some(ServerMsg::Profile {
        profile: Box::new(Profile {
            player: online.clone().unwrap_or_else(|| {
                // Offline: dress the card from the wallet (VIP crown, frame…).
                let achievements: Vec<String> = achievements
                    .as_ref()
                    .map(|a| a.iter().map(|(c, _)| c.clone()).collect())
                    .unwrap_or_default();
                lounge::dressed(
                    player_info(&user, false, PlayerStatus::Available, Rules::default()),
                    &shop::look_of(&wallet, &achievements, now()),
                )
            }),
            online: online.is_some(),
            stats,
            created_at: user.created_at,
            ranks: PerRules::new(brazilian_rank.ok().flatten(), official_rank.ok().flatten()),
            peaks: user.records.map(|_, r| r.peak_rating),
            achievements: achievements
                .clone()
                .unwrap_or_default()
                .into_iter()
                .filter(|(c, _)| known.contains(c.as_str()))
                .collect(),
            history: history.unwrap_or_default().into_iter().map(history_entry).collect(),
            relation,
            last_seen: if relation == Relation::Friend { user.last_seen } else { 0 },
            rating_history: PerRules::new(
                brazilian_history.unwrap_or_default(),
                official_history.unwrap_or_default(),
            ),
            career: career.unwrap_or_default(),
            abandon_rate,
            analysis: None,
            reactions: snook_legend_proto::reactions::bought(&wallet.owned),
            cue_wins: {
                let mut cues: Vec<(String, u32)> =
                    wallet.cue_wins.iter().map(|(c, n)| (c.clone(), *n)).collect();
                cues.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                cues
            },
            cue_stats: wallet
                .cue_games
                .iter()
                .map(|(code, &(games, potted))| snook_legend_proto::CueStats {
                    code: code.clone(),
                    games,
                    wins: wallet.cue_wins.get(code).copied().unwrap_or(0),
                    potted,
                })
                .collect(),
            trophies,
            renames,
        }),
    })
}

/// A player's trophies, newest first: tournaments won, the ladder and the
/// days they topped the daily challenge.
async fn trophies_of(server: &Server, user_id: i64) -> Vec<snook_legend_proto::Trophy> {
    use snook_legend_proto::{Trophy, TrophyKind};
    let (tournaments, ladder, days) = tokio::join!(
        server.store.tournaments_won(user_id, 50),
        server.store.ladder_claim(user_id),
        server.store.daily_wins(user_id, snook_legend_proto::daily::day_of(now())),
    );
    let mut trophies: Vec<Trophy> = tournaments
        .unwrap_or_default()
        .into_iter()
        .map(|(name, players, at)| Trophy { kind: TrophyKind::Tournament, name, players, at })
        .collect();
    if let Ok(Some(at)) = ladder {
        trophies.push(Trophy { kind: TrophyKind::Ladder, name: String::new(), players: 0, at });
    }
    // A day topped: dated at the day's end.
    trophies.extend(days.unwrap_or_default().into_iter().map(|(day, players)| Trophy {
        kind: TrophyKind::DailyChallenge,
        name: String::new(),
        players,
        at: (day + 1) * 86_400 - 1,
    }));
    trophies.sort_by_key(|t| std::cmp::Reverse(t.at));
    trophies
}

/// Takes a tournament's entry fee (the player is told when it can't be).
async fn take_fee(
    server: &Server,
    me: &User,
    fee: u32,
    reference: &str,
    send: &impl Fn(ServerMsg),
) -> bool {
    match server.store.spend(me.id, fee as i64, "tournament entry", Some(reference), now()).await {
        Ok(true) => true,
        Ok(false) => {
            send(ServerMsg::Error { message: "Not enough coins.".into() });
            false
        }
        Err(e) => {
            tracing::error!("tournament fee of {}: {e}", me.username);
            send(ServerMsg::Error { message: "Something went wrong. Please try again.".into() });
            false
        }
    }
}

/// How many times a write that pays or records something is tried.
const WRITE_TRIES: u32 = 4;

/// Runs a database write again after a failure (a busy pool, a dropped
/// connection), a few times with growing waits, rather than losing what it
/// pays or records to a moment's trouble. Only for writes that are safe to
/// repeat.
/// [`retrying`] for longer (about 45 s): for a write that can't be lost,
/// such as a match's result.
async fn retrying_long<T, F, Fut>(what: &str, mut write: F) -> store::StoreResult<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = store::StoreResult<T>>,
{
    let mut wait = Duration::from_millis(500);
    let mut tries = 1;
    loop {
        match write().await {
            Err(e) if tries < 8 => {
                tracing::warn!("{what} (try {tries}): {e}; trying again");
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(Duration::from_secs(15));
                tries += 1;
            }
            done => return done,
        }
    }
}

pub(crate) async fn retrying<T, F, Fut>(what: &str, mut write: F) -> store::StoreResult<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = store::StoreResult<T>>,
{
    let mut wait = Duration::from_millis(500);
    let mut tries = 1;
    loop {
        match write().await {
            Err(e) if tries < WRITE_TRIES => {
                tracing::warn!("{what} (try {tries}): {e}; trying again");
                tokio::time::sleep(wait).await;
                wait *= 2;
                tries += 1;
            }
            done => return done,
        }
    }
}

/// Gives (or gives back) coins once (the ledger `reference` keeps it
/// single, also when tried again), and tells the player.
pub(crate) async fn pay(
    server: &Server,
    user_id: i64,
    name: &str,
    coins: i64,
    reason: &str,
    reference: &str,
) {
    let credit = || server.store.credit(user_id, coins, reason, Some(reference), now());
    match retrying("paying", credit).await {
        Ok(true) => {
            let amount = coins.max(0) as u32;
            server.with(|l| {
                l.notify(name, ServerMsg::CoinsEarned { amount, reason: reason.to_string() })
            });
            if let Ok(Some(user)) = server.store.user_by_id(user_id).await {
                shop::push_wallet(server, &user).await;
            }
        }
        Ok(false) => {}
        Err(e) => tracing::error!("paying {coins} coins to {name} ({reference}): {e}"),
    }
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// A player signing in whose game was saved by a server that went away:
/// rebuild it here (the lounge then puts them back at the table).
async fn restore_live(server: &Arc<Server>, user: &User) {
    // A server going away hands its games over and rebuilds none (a player
    // who reconnects to it before the next server is up would otherwise get
    // the game back here, to be handed over again and again).
    if server.with(|l| l.is_draining()) {
        return;
    }
    let Ok(Some(live)) = server.store.live_match_of(user.id).await else { return };
    if server.with(|l| l.has_room(live.id as u64)) {
        return;
    }
    // Finished here already (its result recorded), the copy not deleted
    // yet: it is not played again.
    if server.with(|l| l.was_finished(live.id as u64)) {
        let _ = server.store.delete_live(live.id).await;
        return;
    }
    let other = if live.players[0] == user.id { live.players[1] } else { live.players[0] };
    let (a, b) = tokio::join!(
        server.store.user_by_id(live.players[0]),
        server.store.user_by_id(live.players[1])
    );
    let (a, b) = match (a, b) {
        (Ok(Some(a)), Ok(Some(b))) => (a, b),
        // A player gone for good: the game can't go on.
        (Ok(None), _) | (_, Ok(None)) => {
            let _ = server.store.delete_live(live.id).await;
            return;
        }
        // The database didn't answer: the game is kept for the next try.
        _ => return,
    };
    let replay = live.replay.clone();
    let rebuilt = tokio::task::spawn_blocking(move || lounge::Rebuilt::from(&replay)).await;
    match rebuilt {
        Ok(Ok(rebuilt)) => {
            if server.with(|l| l.restore(&live, [a, b], rebuilt)) {
                tracing::info!(room = live.id, other, "saved game rebuilt");
            }
        }
        Ok(Err(e)) => {
            tracing::warn!(room = live.id, "saved game could not be rebuilt: {e}");
            let _ = server.store.delete_live(live.id).await;
        }
        Err(e) => tracing::error!("rebuilding a game panicked: {e}"),
    }
}

/// The address limits are kept under: an IPv4 address as it is (also when
/// written as an IPv6 one), an IPv6 address as its /64.
pub fn counted_address(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(a) => match a.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                let s = a.segments();
                IpAddr::V6(std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
            }
        },
        v4 => v4,
    }
}

/// A reserved word, maybe with digits or underscores around it ("Admin_1",
/// "_mod", "suporte2"), in any case.
fn is_reserved_word(name: &str) -> bool {
    let bare = name.trim_matches(|c: char| c.is_ascii_digit() || c == '_');
    RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(name) || r.eq_ignore_ascii_case(bare))
}

/// Lower-case key of a username (re-exported for the admin panel).
pub fn user_key(name: &str) -> String {
    key(name)
}

#[cfg(test)]
mod tests {
    #[test]
    fn address_headers_are_believed_from_proxies_only() {
        use super::{parse_networks, trusted_proxy};
        let ip = |s: &str| s.parse::<std::net::IpAddr>().unwrap();
        // By default: private networks (where a host's proxy connects from).
        for proxy in [
            "10.1.2.3",
            "172.16.0.9",
            "192.168.1.1",
            "127.0.0.1",
            "100.70.0.1",
            "::1",
            "fd00::1",
            "::ffff:10.0.0.5",
        ] {
            assert!(trusted_proxy(ip(proxy), None), "{proxy}");
        }
        for direct in ["8.8.8.8", "100.128.0.1", "2001:db8::1", "172.32.0.1"] {
            assert!(!trusted_proxy(ip(direct), None), "{direct}");
        }
        // A list: only those networks; "*" is anyone.
        let list = parse_networks("203.0.113.0/24, 2001:db8::/32, nonsense, 10.0.0.0/99");
        assert_eq!(list.len(), 2);
        assert!(trusted_proxy(ip("203.0.113.77"), Some(&list)));
        assert!(trusted_proxy(ip("2001:db8:1::5"), Some(&list)));
        assert!(!trusted_proxy(ip("10.0.0.1"), Some(&list)), "a list replaces the default");
        assert!(trusted_proxy(ip("8.8.8.8"), Some(&parse_networks("*"))));
        assert!(trusted_proxy(ip("::ffff:203.0.113.1"), Some(&list)), "a mapped address too");
    }

    use super::*;

    /// The stand-in hash must be a real one: a hash that doesn't parse is
    /// rejected at once, and the timing would give unknown names away.
    #[test]
    fn dummy_hash_is_a_real_argon2_hash() {
        let hash = PasswordHash::new(DUMMY_HASH).expect("parses");
        assert!(Argon2::default().verify_password(b"guess", &hash).is_err());
    }

    /// A write that fails for a moment is tried again until it is done.
    #[tokio::test]
    async fn writes_are_tried_again() {
        let tries = std::sync::atomic::AtomicU32::new(0);
        let tries = &tries;
        let done = retrying("a test", || async move {
            match tries.fetch_add(1, Ordering::SeqCst) {
                0 | 1 => Err(store::StoreError("busy".into())),
                n => Ok(n),
            }
        })
        .await;
        assert_eq!(done.unwrap(), 2);
    }

    /// The reserved words in any case, and dressed up with digits or
    /// underscores; names that only contain one are free.
    #[test]
    fn reserved_names() {
        for name in
            ["admin", "ADMIN", "Admin_1", "_mod_", "moderador", "Suporte2", "staff", "1oficial"]
        {
            assert!(is_reserved_word(name), "{name}");
        }
        for name in ["adminx", "modesto", "my_staff", "ana_99", "system_x"] {
            assert!(!is_reserved_word(name), "{name}");
        }
    }

    /// Every address of an IPv6 /64 counts as one (its owner picks among
    /// them freely); IPv4 addresses stay apart.
    #[test]
    fn an_ipv6_network_counts_as_one_address() {
        let ip = |s: &str| counted_address(s.parse().unwrap());
        assert_eq!(ip("2001:db8:1:2:aaaa:bbbb:cccc:dddd"), ip("2001:db8:1:2::1"));
        assert_ne!(ip("2001:db8:1:2::1"), ip("2001:db8:1:3::1"));
        assert_eq!(ip("::ffff:203.0.113.7"), ip("203.0.113.7"));
        assert_ne!(ip("203.0.113.7"), ip("203.0.113.8"));
    }
}
