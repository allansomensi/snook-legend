//! Lounge state: who is online, chat, invitations, open tables, match rooms
//! and tournaments.
//!
//! Everything here is in memory and synchronous, behind one mutex: methods
//! are short and never wait for I/O. Work that does (the database, shot
//! simulation) is handed to the effect worker as an [`Effect`]; its results
//! come back through methods such as [`Lounge::commit_shot`].

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use glam::DVec2;
use rand::Rng;
use snook_legend_core::rating::{self, EloConfig, Rating};
use snook_legend_core::stats::ShotStats;
use snook_legend_core::{
    BallId, BreakOption, EightBallGame, GameOverReason, PerRules, PocketId, Rules, Shot, ShotReport,
};
use snook_legend_proto::replay::{Replay, Stroke, shot_difficulty};
use snook_legend_proto::{
    AimState, ChatKind, ChatLine, ClientMsg, EIGHT_CALL_PROTOCOL, FeaturedMatch, MAX_ROOM_PASSWORD,
    MatchResult, PROTOCOL_VERSION, PlayerInfo, PlayerStatus, SPECTATOR_SEAT, ServerMsg, TableInfo,
    TournamentInfo, TournamentState, clean_chat, clean_text, is_quick_phrase,
};
use tokio::sync::Notify;
use tokio::sync::mpsc::{Sender, UnboundedSender, error::TrySendError};

use crate::limits::Bucket;
use crate::now;
use crate::store::{LiveMatch, MatchRecord, TournamentRecord, User};

mod doubles;
mod match_social;
mod moderation;
mod social;
mod tournaments;

pub use moderation::{INBOX_PROTOCOL, refusal_text};

pub use match_social::{PHRASES_ONLY, phrases_only};
pub use social::SEEK_LIFETIME;
pub use tournaments::{
    ARENA_MINUTES, Cup, HOUSE_CUT, Joined, KNOCKOUT_SIZES, MAX_HOUSE_PRIZE, OfficialStart,
    ScheduledTournament,
};

pub fn tournaments_max_fee() -> u32 {
    tournaments::MAX_ENTRY_FEE
}
use tournaments::Tournament;

/// Tunable server settings.
#[derive(Debug, Clone)]
pub struct Settings {
    pub shot_clock: Duration,
    /// How long a disconnected player has to come back before forfeiting.
    pub reconnect_grace: Duration,
    pub chat_history: usize,
    pub motd: String,
    /// LAN server: guests without accounts, no ratings.
    pub lan: bool,
    /// Simultaneous connections from one address.
    pub max_connections_per_ip: usize,
    /// A tournament player who is not online (or not free) this long after
    /// their match is ready loses it.
    pub tournament_no_show: Duration,
    /// A knockout player still looking at the end of their last game when
    /// the next one is ready is taken back to the lounge after this long
    /// (the bracket doesn't wait on a screen nobody may be looking at).
    pub tournament_pause: Duration,
    pub max_open_tournaments: usize,
    /// The weekly cup the server organises (none if not configured).
    pub cup: Option<Cup>,
    /// How long a note stays on the "looking for a game" board.
    pub seek_lifetime: Duration,
    /// The share of the entry fees the house keeps (percent): the rest
    /// goes to the prizes. Coins that leave the economy.
    pub house_cut: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            shot_clock: Duration::from_secs(30),
            reconnect_grace: Duration::from_secs(60),
            chat_history: 60,
            motd: format!("Welcome to {}! Be kind, play fair.", snook_legend_proto::GAME_NAME),
            lan: false,
            max_connections_per_ip: 12,
            tournament_no_show: Duration::from_secs(120),
            tournament_pause: Duration::from_secs(15),
            max_open_tournaments: 20,
            cup: None,
            seek_lifetime: SEEK_LIFETIME,
            house_cut: tournaments::HOUSE_CUT,
        }
    }
}

/// Slow work for the effect worker (see `lib.rs`).
#[derive(Debug)]
pub enum Effect {
    /// Simulate a shot and give the outcome to [`Lounge::commit_shot`].
    Simulate {
        room: u64,
        turn: u32,
        game: Box<EightBallGame>,
        shot: Shot,
        called: Option<PocketId>,
        /// The shooter's game can't call the 8 (older than
        /// [`EIGHT_CALL_PROTOCOL`]): the 8 counts as called in the pocket
        /// it goes into ([`EightBallGame::call_the_eight_where_it_went`]).
        eight_anywhere: bool,
    },
    /// Keep an official tournament once over (the website shows it), and
    /// tell its players how they did (see `crate::official`).
    RecordOfficial(Box<crate::official::Finished>),
    /// Store a finished match (players are notified of achievements).
    RecordMatch {
        record: MatchRecord,
        keys: [String; 2],
        /// The same two players played too many games today (rated or
        /// casual): no coins, experience or progress for their cues.
        farmed: bool,
    },
    RecordTournament(TournamentRecord),
    /// Save a game in progress (after every stroke).
    SaveLive(LiveMatch),
    /// A game handed to the next server: its copy is written as it is now,
    /// and only then are its players' connections closed (they come back
    /// on the next server, which plays it from that copy, not an older one).
    HandOff {
        live: LiveMatch,
        kills: Vec<Arc<Notify>>,
    },
    /// The game ended (or was handed over): forget the saved copy.
    DeleteLive(i64),
    /// Add a player's shots by difficulty band: (band, attempts, made).
    ShotStats {
        user_id: i64,
        bands: Vec<(usize, u32, u32)>,
    },
    /// Store a finished doubles game (each player's record, coins and
    /// experience follow).
    RecordDoubles {
        record: Box<crate::store::DoublesRecord>,
        /// The teams' players (lower-case names).
        keys: [[String; 2]; 2],
    },
    /// How long each shot of a finished game was thought over (tenths of
    /// a second, one per stroke; None for what isn't a shot).
    ShotTimes {
        room: i64,
        times: Vec<Option<u32>>,
    },
    /// Give (or give back) coins: a tournament prize or refund.
    Pay {
        user_id: i64,
        name: String,
        coins: i64,
        reason: String,
        reference: String,
    },
}

/// A restarted server gives the players of a saved game this long to come back.
/// What a player under 18 is told when they write while their chat is off.
pub const CHAT_OFF_FOR_MINORS: &str = "The chat is off for players under 18. A parent or guardian can turn it on below the lounge's chat.";

/// What a player who hasn't turned the chat on is told when they write (the
/// game asks for the age group first, so this is rare).
pub const CHAT_OFF: &str = "The chat with other players is off until you turn it on.";

/// Why `user`'s chat is off (see [`User::chat_restricted`]).
pub fn chat_off_reason(user: &User) -> &'static str {
    if user.minor == Some(true) { CHAT_OFF_FOR_MINORS } else { CHAT_OFF }
}

const RESTORE_GRACE: Duration = Duration::from_secs(180);
/// Games between the same two players in a day: from the third rated one
/// the rating changes are halved, from the fifth they stop; from the fifth,
/// rated or casual, nothing is earned (no farming).
const PAIR_WINDOW: Duration = Duration::from_secs(24 * 3600);
/// Letters of invitation codes (no 0/O, 1/I/L).
const CODE_LETTERS: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const CODE_LIFETIME: Duration = Duration::from_secs(30 * 60);

/// Extra time after a shot's simulated duration before the next shot clock
/// starts (the players watch the balls stop and read the result).
const SHOT_MARGIN: Duration = Duration::from_millis(1500);
/// The shot clocks a player can let run out in a row before they are taken
/// to have left the game.
const IDLE_TURNS: u8 = 3;
/// A player back at their turn after their connection dropped has at least
/// this long to shoot (the shot clock waited for them meanwhile).
const RECONNECT_CLOCK: Duration = Duration::from_secs(15);
/// The games of 0.12 and later open a match with its introduction (the two
/// players face each other, a few seconds): the first shot clock waits for it.
const MATCH_INTRO: Duration = Duration::from_secs(3);
/// A player who signs in again while their old connection still seemed
/// open gets the table's chat lines of this many seconds again.
const RECENT_CHAT_SECS: i64 = 60;
/// Aim and cue-ball updates are relayed at most this often.
const AIM_RELAY: Duration = Duration::from_millis(33);
const MATCH_CHAT_KEEP: usize = 40;

/// What an invitation to a player in focus mode is told (see
/// [`ClientMsg::SetActivity`]).
const FOCUSED: &str =
    "That player is in focus mode and isn't taking invitations until their game is over.";
/// How long a player's rate limits outlive their connection.
const LIMITS_KEPT: Duration = Duration::from_secs(600);
/// A session's rate limits: chat, invitations, lounge actions, reactions
/// and the "looking for a game" board.
type Limits = [Bucket; 5];

fn fresh_limits() -> Limits {
    [
        Bucket::new(5.0, 0.8),
        Bucket::new(6.0, 0.2),
        Bucket::new(ACTION_BURST, ACTION_RATE),
        Bucket::new(3.0, 0.5),
        Bucket::new(social::SEEK_BURST, social::SEEK_RATE),
    ]
}

/// Opening and closing tables, tournaments and the queue are announced to
/// the whole lounge: a dozen at once, then one a second (a person clicking
/// never gets near it; a script can't flood everyone through it).
const ACTION_BURST: f64 = 12.0;
const ACTION_RATE: f64 = 1.0;
/// A tournament still taking names is dropped when its host has been gone
/// this long (it would hold one of the few tournament slots forever).
const HOSTLESS_TOURNAMENT: Duration = Duration::from_secs(300);
/// A player's tournament still taking names after this long is dropped
/// (its fees given back): nobody came, and it holds one of the few slots.
const STALE_REGISTRATION: Duration = Duration::from_secs(3600);

pub struct Session {
    pub conn_id: u64,
    pub user: User,
    pub guest: bool,
    pub ip: String,
    tx: Sender<ServerMsg>,
    kill: Arc<Notify>,
    pub room: Option<u64>,
    pub table: Option<u64>,
    /// Users this player blocked (ids).
    pub blocked: HashSet<i64>,
    chat_bucket: Bucket,
    last_chat: String,
    repeats: u32,
    invite_bucket: Bucket,
    /// Lounge actions everyone hears about (tables, tournaments, the queue).
    action_bucket: Bucket,
    last_aim: Option<Instant>,
    /// Achievement codes (they unlock reactions).
    achievements: Vec<String>,
    reaction_bucket: Bucket,
    /// The player whose games this one is watching.
    watching: Option<String>,
    /// Watching through the lounge's TV (not a friend's game): the watcher
    /// follows that player into public games only.
    watching_tv: bool,
    /// VIP badge, cue and chalk (from the wallet).
    pub look: Look,
    /// Notes on the "looking for a game" board.
    seek_bucket: Bucket,
    /// Playing a game against the computer (the game says so).
    pub vs_computer: bool,
    /// Away from the game (the game says so after a while untouched).
    pub away: bool,
    /// Playing in focus mode (the game says so): no invitations.
    pub focus: bool,
    /// The rules this player plays (their choice in the game): the queue
    /// pairs them with players of the same rules, their tables and
    /// invitations are played under them.
    pub rules: Rules,
    /// What exactly they play outside online games (the game says so).
    pub pastime: snook_legend_proto::activity::Pastime,
    /// Their friends (lower-case names): told what this player is doing.
    pub friends: HashSet<String>,
    /// The protocol of the player's game (its hello): one older than
    /// [`EIGHT_CALL_PROTOCOL`] can't call the 8 under the official rules,
    /// and the 8 counts as called in the pocket it goes into.
    pub protocol: u32,
}

impl Session {
    fn limits(&self) -> Limits {
        [
            self.chat_bucket.clone(),
            self.invite_bucket.clone(),
            self.action_bucket.clone(),
            self.reaction_bucket.clone(),
            self.seek_bucket.clone(),
        ]
    }

    /// The player's rating under the rules they play now (none for a guest).
    fn shown_rating(&self) -> i32 {
        if self.guest { 0 } else { self.user.records.get(self.rules).rating }
    }
}

struct Room {
    id: u64,
    /// Lower-case usernames.
    keys: [String; 2],
    /// The players' accounts when the match started (kept for disconnected players).
    users: [User; 2],
    guests: [bool; 2],
    rated: bool,
    game: EightBallGame,
    deadline: Instant,
    disconnected: [Option<Instant>; 2],
    finished: bool,
    rematch: [bool; 2],
    present: [bool; 2],
    /// How the game ended (told again to who comes back to the table, or
    /// starts watching it, after the end).
    result: Option<MatchResult>,
    breaker: usize,
    /// The shot being simulated (no other shot or aim is accepted meanwhile).
    busy: Option<u32>,
    /// The shot clocks each seat let run out in a row (a stroke or a choice
    /// of theirs starts over): at [`IDLE_TURNS`] they left the game.
    idle: [u8; 2],
    /// The last shot played was a break (a win on it is a golden break).
    last_was_break: bool,
    shot_at: Instant,
    /// Turns each seat has had (for "break and run").
    turns: [u32; 2],
    /// (tournament, round, index)
    tournament: Option<(u64, usize, usize)>,
    chat: VecDeque<ChatLine>,
    /// Reactions from spectators, all together (a crowd must stay quiet).
    crowd: Bucket,
    /// Spectator -> the seat they cheer for.
    cheers: HashMap<String, usize>,
    /// The whole game so far (saved after every stroke; the replay).
    replay: Replay,
    /// Each seat's numbers this game.
    stats: [ShotStats; 2],
    /// Each seat's measurable pots: (difficulty band, made).
    bands: [Vec<(usize, bool)>; 2],
    /// The cloth and ball set of this table.
    table_look: [String; 2],
    /// Rebuilt after a restart: the players have until then to come back.
    grace_until: Option<Instant>,
    /// When the game ended.
    ended: Option<Instant>,
    /// Started at a table with a password: not on the lounge's TV.
    private: bool,
    /// A seat that resigned while a shot was being played: the resignation
    /// comes after the shot (see [`Lounge::commit_shot`]).
    resigning: Option<usize>,
    /// Seats that left as they resigned (that resignation waiting for the
    /// shot): back in the lounge as soon as the game is over.
    leaving: [bool; 2],
    /// How long each shot was thought over: (its stroke's index, tenths of
    /// a second from the shot clock's start). Not kept across a restart.
    think: Vec<(usize, u32)>,
    /// A rated game: what each seat stands to win and lose (see
    /// [`ServerMsg::MatchStarted`]).
    stakes: Option<[[i32; 2]; 2]>,
    /// The spectators' chat (the players never see it).
    spectator_chat: VecDeque<ChatLine>,
    /// Lines said at the table so far (`chat` keeps the last few).
    chat_total: u64,
    /// For a seat whose connection dropped: `chat_total` then (the lines
    /// after it are sent again when they come back).
    chat_missed: [Option<u64>; 2],
    /// A doubles game: the teams' second players and who holds the cue
    /// (see `doubles.rs`); each seat is a team.
    doubles: Option<Box<doubles::Doubles>>,
}

struct Table {
    id: u64,
    host: String,
    rated: bool,
    rules: Rules,
    password: Option<String>,
}

/// Extra rating lost for leaving a rated game in progress.
pub const ABANDON_RATING: i32 = 10;
/// Games left within this long count towards a longer wait.
const ABANDON_WINDOW: Duration = Duration::from_secs(24 * 3600);

/// "You left a game in progress: you can play online again in N min."
fn cooldown_message(left: Duration) -> String {
    let minutes = left.as_secs().div_ceil(60).max(1);
    format!("You left a game in progress: you can play online again in {minutes} min.")
}

#[derive(Default)]
pub struct Lounge {
    pub settings: Settings,
    /// Games finished here, and when: their saved copy may still be on its
    /// way out of the database, and must not be played again from it.
    finished_rooms: HashMap<u64, Instant>,
    sessions: HashMap<String, Session>,
    /// (from, to) -> the game offered, lower-case names.
    invites: HashMap<(String, String), Offer>,
    rooms: HashMap<u64, Room>,
    tables: BTreeMap<u64, Table>,
    tournaments: BTreeMap<u64, Tournament>,
    next_id: u64,
    chat: VecDeque<ChatLine>,
    /// Player -> who is watching their games (lower-case names).
    watchers: HashMap<String, HashSet<String>>,
    /// "Play now": who is waiting for an opponent.
    queue: Vec<Queued>,
    /// Pairs the queue matched recently (not matched again right away).
    recent_pairs: VecDeque<(String, String, Instant)>,
    effects: Option<UnboundedSender<Effect>>,
    /// Shots to simulate, on a queue of their own (see [`Lounge::simulate_on`]).
    shots: Option<UnboundedSender<Effect>>,
    /// Effects sent and not done yet (the worker counts them down).
    pending: Arc<AtomicUsize>,
    next_guest_id: i64,
    /// The server is restarting: no new games; running ones are handed to
    /// the next server at this time (or when a player drops).
    draining: Option<Instant>,
    /// Invitation codes -> (table, created).
    codes: HashMap<String, (u64, Instant)>,
    /// The game shown on the lounge's "TV".
    featured: Option<FeaturedMatch>,
    last_featured: Option<Instant>,
    /// A game started, ended or lost a player: the TV is picked again as
    /// soon as the message (or tick) that did it is handled, not on the
    /// next round (a game just left was still offered to watch).
    featured_dirty: bool,
    /// Games between two players (sorted keys) recently, rated and casual
    /// apart.
    pair_games: HashMap<(String, String, bool), VecDeque<Instant>>,
    /// Who broke the last game under the Brazilian rules between two
    /// players (sorted keys), and when it began: the other breaks the next.
    last_break: HashMap<(String, String), (String, Instant)>,
    /// When each player last left games in progress (the last day's).
    abandoned: HashMap<String, VecDeque<Instant>>,
    /// Waits in the "Play now" queue (seconds), for the staff panel.
    pub queue_waits: VecDeque<u32>,
    /// The notes on the board (the store has them; this is what players see).
    board: Vec<snook_legend_proto::BoardNote>,
    /// The last lounge chat line's number.
    chat_id: u64,
    /// Lounge chat lines deleted (shown as such), and by whom. The text is
    /// kept for reports.
    chat_deleted: HashMap<u64, snook_legend_proto::Deleted>,
    /// Lounge chat lines taken away without a trace (or cleared).
    chat_removed: HashSet<u64>,
    /// The "looking for a game" board.
    seeks: social::Seeks,
    /// (player, table or tournament) -> wrong passwords given, since when.
    password_misses: HashMap<(String, u64), (u32, Instant)>,
    /// Results of each account (id) still waiting for the effect worker: the
    /// numbers it stored aren't the newest while another is on its way.
    records_pending: HashMap<i64, u32>,
    /// The rate limits of players who left a moment ago, since when:
    /// signing in again doesn't fill them up (see [`LIMITS_KEPT`]).
    left_limits: HashMap<String, (Limits, Instant)>,
    /// Who wrote each lounge chat line (its number -> the account's id):
    /// hidden from those who blocked them, online or not.
    chat_authors: HashMap<u64, i64>,
    /// The staff's themed tournaments, and the tournament each one became.
    scheduled_tournaments: Vec<tournaments::ScheduledTournament>,
    event_tournaments: HashMap<i64, u64>,
    /// The start of a weekly cup the staff called off: it is not opened
    /// again (the next week's is).
    cup_called_off: Option<i64>,
    /// Doubles: pairs of partners, their searches and tables.
    doubles: doubles::Lobby,
    /// The banned words, the pinned line, why players were muted.
    moderation: moderation::Moderation,
}

struct Queued {
    key: String,
    rated: bool,
    rules: Rules,
    since: Instant,
    last_status: Instant,
}

/// The game an invitation, a table, a tournament or the queue offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer {
    pub rated: bool,
    pub rules: Rules,
}

/// How a new game starts: its rack, who breaks, under which rules.
#[derive(Debug, Clone, Copy)]
struct Game {
    seed: u64,
    breaker: usize,
    rules: Rules,
}

/// How far apart two ratings may be after waiting `secs`: close at first,
/// wider every second, anyone after two minutes.
pub fn queue_range(secs: f32) -> u32 {
    if secs >= 120.0 { u32::MAX } else { (50.0 + secs * 6.0).min(800.0) as u32 }
}

pub fn key(name: &str) -> String {
    name.to_lowercase()
}

fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn clean_password(p: Option<String>) -> Option<String> {
    p.map(|p| clean_text(&p, MAX_ROOM_PASSWORD)).filter(|p| !p.is_empty())
}

/// The password a private table or tournament is opened with: none for
/// none (or a blank one), an error for one that is nothing but characters
/// the cleaning removes (it would silently open a public one).
fn room_password(p: Option<String>) -> Result<Option<String>, ()> {
    match p {
        Some(raw) if !raw.trim().is_empty() => clean_password(Some(raw)).map(Some).ok_or(()),
        _ => Ok(None),
    }
}

const UNUSABLE_PASSWORD: &str = "That password can't be used. Choose another one.";

/// Wrong passwords one player may give for a private table or tournament
/// before being refused for a while: guessing one is not a game.
const PASSWORD_MISSES: u32 = 5;
const PASSWORD_LOCKOUT: Duration = Duration::from_secs(600);
const TOO_MANY_PASSWORDS: &str = "Too many wrong passwords. Try again in a few minutes.";

/// What everyone sees of a player who plays `rules`.
pub fn player_info(user: &User, guest: bool, status: PlayerStatus, rules: Rules) -> PlayerInfo {
    PlayerInfo {
        username: user.username.clone(),
        rules,
        standings: user.standings(),
        status,
        country: user.country.clone(),
        role: user.role,
        guest,
        vip: false,
        cue: String::new(),
        chalk: String::new(),
        frame: String::new(),
        name_style: String::new(),
        effect: String::new(),
        cue_level: 0,
        status_message: user.status_message.clone(),
        avatar: user.avatar.clone(),
        vs_computer: false,
        away: false,
        focus: false,
        searching: false,
        level: if guest { 0 } else { snook_legend_proto::xp::level_of(user.xp) },
    }
}

/// What others see of a player's shop items.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Look {
    pub vip: bool,
    pub cue: String,
    /// The cue's level (online wins with it).
    pub cue_level: u8,
    pub chalk: String,
    /// The table they bring (as the host, or the higher rated player).
    pub cloth: String,
    pub balls: String,
    pub frame: String,
    pub name_style: String,
    pub effect: String,
    /// The reactions bought in the shop (their codes).
    pub reactions: Vec<String>,
}

pub fn dressed(mut info: PlayerInfo, look: &Look) -> PlayerInfo {
    info.vip = look.vip;
    info.cue = look.cue.clone();
    info.cue_level = look.cue_level;
    info.chalk = look.chalk.clone();
    info.frame = look.frame.clone();
    info.name_style = look.name_style.clone();
    info.effect = look.effect.clone();
    info
}

/// What the admin panel shows about the live server.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LiveRoom {
    pub id: u64,
    pub players: [String; 2],
    pub rated: bool,
    pub rules: Rules,
    pub shots: u32,
    pub finished: bool,
    pub tournament: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LiveSnapshot {
    pub players: Vec<PlayerInfo>,
    pub rooms: Vec<LiveRoom>,
    pub tables: usize,
    pub tournaments: Vec<TournamentInfo>,
}

impl Lounge {
    pub fn new(settings: Settings, effects: UnboundedSender<Effect>) -> Self {
        Lounge {
            settings,
            // Ids from different runs of the server never meet (saved games
            // keep theirs across a restart).
            next_id: (now().max(0) as u64) << 16,
            effects: Some(effects),
            next_guest_id: -1,
            ..Default::default()
        }
    }

    /// Shots are simulated from a queue of their own: a slow database never
    /// holds a shot up behind results and saved games.
    pub fn simulate_on(&mut self, shots: UnboundedSender<Effect>) {
        self.shots = Some(shots);
    }

    /// The count of effects sent and not done yet, shared with the worker
    /// (which counts each one down when it is done).
    pub fn pending_effects(&self) -> Arc<AtomicUsize> {
        self.pending.clone()
    }

    fn effect(&self, e: Effect) {
        let shot = matches!(e, Effect::Simulate { .. });
        if shot && let Some(tx) = &self.shots {
            let _ = tx.send(e);
            return;
        }
        if let Some(tx) = &self.effects {
            self.pending.fetch_add(1, Ordering::SeqCst);
            if tx.send(e).is_err() {
                self.pending.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Sends to one player. A player whose connection can't keep up is
    /// disconnected rather than buffering without limit.
    fn send(&self, k: &str, msg: ServerMsg) {
        if let Some(s) = self.sessions.get(k) {
            match s.tx.try_send(msg) {
                Ok(()) | Err(TrySendError::Closed(_)) => {}
                Err(TrySendError::Full(_)) => {
                    tracing::warn!("{k}: outgoing queue full, closing the connection");
                    s.kill.notify_one();
                }
            }
        }
    }

    fn broadcast(&self, msg: &ServerMsg) {
        for k in self.sessions.keys() {
            self.send(k, msg.clone());
        }
    }

    /// A game over is not played any more: its players are free in the
    /// lounge at once, also while they still look at its result.
    fn status(&self, k: &str) -> PlayerStatus {
        match self.sessions.get(k) {
            Some(s) if s.room.is_some_and(|id| self.live_room(id)) => PlayerStatus::Playing,
            Some(s) if s.table.is_some() => PlayerStatus::Waiting,
            _ => PlayerStatus::Available,
        }
    }

    /// Room `id` has a game going on (not over).
    fn live_room(&self, id: u64) -> bool {
        self.rooms.get(&id).is_some_and(|r| !r.finished)
    }

    fn info(&self, k: &str) -> Option<PlayerInfo> {
        self.sessions.get(k).map(|s| {
            let mut info = dressed(player_info(&s.user, s.guest, self.status(k), s.rules), &s.look);
            // An online game comes first (the game against the computer is
            // left behind when one starts).
            let online = s.room.is_some_and(|id| self.live_room(id));
            info.vs_computer = s.vs_computer && !online;
            info.away = s.away && !online;
            info.focus = s.focus && !online;
            info.searching = self.queue.iter().any(|q| q.key == k);
            info
        })
    }

    fn announce(&self, k: &str) {
        if let Some(player) = self.info(k) {
            self.broadcast(&ServerMsg::PlayerUpdated { player });
        }
        // Their friends see what they do in more detail.
        self.tell_friends(k);
    }

    fn display(&self, k: &str) -> String {
        self.sessions.get(k).map(|s| s.user.username.clone()).unwrap_or_else(|| k.to_string())
    }

    fn error(&self, k: &str, message: impl Into<String>) {
        self.send(k, ServerMsg::Error { message: message.into() });
    }

    pub fn online_count(&self) -> usize {
        self.sessions.len()
    }

    /// Players online who play each of the rules.
    pub fn online_by_rules(&self) -> PerRules<usize> {
        let mut counts = PerRules::<usize>::default();
        for s in self.sessions.values() {
            *counts.get_mut(s.rules) += 1;
        }
        counts
    }

    /// Players online from each country (the empty code: not given).
    pub fn online_countries(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for s in self.sessions.values() {
            *counts.entry(s.user.country.clone()).or_default() += 1;
        }
        counts
    }

    pub fn is_online(&self, name: &str) -> bool {
        self.sessions.contains_key(&key(name))
    }

    pub fn player(&self, name: &str) -> Option<PlayerInfo> {
        self.info(&key(name))
    }

    pub fn session_user(&self, name: &str) -> Option<&User> {
        self.sessions.get(&key(name)).map(|s| &s.user)
    }

    /// Distinct client addresses online (if this is 1 with many players, the
    /// proxy header is not configured).
    pub fn distinct_addresses(&self) -> usize {
        self.sessions.values().map(|s| s.ip.as_str()).collect::<HashSet<_>>().len()
    }

    /// A unique id for a LAN guest (negative, never stored).
    pub fn guest_id(&mut self) -> i64 {
        self.next_guest_id -= 1;
        self.next_guest_id
    }

    /// A user finished signing in on connection `conn_id`, to play `rules`.
    #[allow(clippy::too_many_arguments)]
    pub fn join(
        &mut self,
        conn_id: u64,
        user: User,
        guest: bool,
        rules: Rules,
        tx: Sender<ServerMsg>,
        kill: Arc<Notify>,
        token: String,
        ip: String,
        blocked: HashSet<i64>,
    ) {
        let k = key(&user.username);
        // Signing in again (or from somewhere else) keeps the limits.
        let limits = match self.sessions.get(&k) {
            Some(s) => s.limits(),
            None => self.left_limits.remove(&k).map_or_else(fresh_limits, |(l, _)| l),
        };
        let [chat_bucket, invite_bucket, action_bucket, reaction_bucket, seek_bucket] = limits;
        if self.sessions.contains_key(&k) {
            // Signed in again from somewhere else: whatever the old connection
            // had going in the lounge (an open table, a place in the queue,
            // invitations, a game being watched) ends with it. A match in
            // progress carries over below.
            self.close_table_of(&k);
            self.drop_invites_of(&k);
            self.stop_watching(&k);
            self.queue.retain(|q| q.key != k);
        }
        if let Some(old) = self.sessions.remove(&k) {
            let _ = old
                .tx
                .try_send(ServerMsg::Kicked { reason: "You signed in from another place.".into() });
            old.kill.notify_one();
        }
        // A game going on first (one just finished could also be waiting).
        let mine = |r: &&Room| r.players_present().iter().any(|(p, at)| *p == k && *at);
        let room = self.rooms.values().filter(mine).min_by_key(|r| r.finished).map(|r| r.id);
        let info = player_info(
            &user,
            guest,
            if room.is_some_and(|id| self.live_room(id)) {
                PlayerStatus::Playing
            } else {
                PlayerStatus::Available
            },
            rules,
        );
        let _ = tx.try_send(ServerMsg::AuthOk { player: info, token });
        let _ = tx.try_send(ServerMsg::Welcome {
            motd: self.settings.motd.clone(),
            lan: self.settings.lan,
        });
        self.sessions.insert(
            k.clone(),
            Session {
                conn_id,
                user,
                guest,
                ip,
                tx,
                kill,
                room,
                table: None,
                blocked,
                chat_bucket,
                last_chat: String::new(),
                repeats: 0,
                invite_bucket,
                action_bucket,
                last_aim: None,
                achievements: Vec::new(),
                reaction_bucket,
                watching: None,
                watching_tv: false,
                look: Look::default(),
                seek_bucket,
                vs_computer: false,
                away: false,
                focus: false,
                rules,
                pastime: Default::default(),
                friends: HashSet::new(),
                protocol: PROTOCOL_VERSION,
            },
        );
        let players: Vec<PlayerInfo> = self.sessions.keys().filter_map(|k| self.info(k)).collect();
        self.send(&k, ServerMsg::Lounge { players });
        self.send_chat_history(&k);
        let tables = self.tables.values().filter_map(|t| self.table_info(t)).collect();
        self.send(&k, ServerMsg::Tables { tables });
        let tournaments = self.tournaments.values().map(|t| self.tournament_info(t)).collect();
        self.send(&k, ServerMsg::Tournaments { tournaments });
        self.send(&k, ServerMsg::Featured { featured: self.featured.clone() });
        if !self.settings.lan {
            self.send(&k, ServerMsg::Board { notes: self.board.clone() });
            self.send_seek_board(&k);
        }
        if let Some(until) = self.draining {
            let secs = until.saturating_duration_since(Instant::now()).as_secs() as u32;
            self.send(&k, ServerMsg::Draining { secs });
        }
        self.announce(&k);
        // Back to a running match.
        if let Some(id) = room {
            self.back_to_room(&k, id);
        }
    }

    /// `k` is back at the game in room `id` (signed in again, or found
    /// there when it was rebuilt): the table is sent to them and the
    /// opponent is told.
    fn back_to_room(&mut self, k: &str, id: u64) {
        if self.rooms.get(&id).is_some_and(|r| r.doubles.is_some()) {
            return self.doubles_back(k, id);
        }
        let clock = self.settings.shot_clock.as_secs() as u32;
        let grace = self.settings.reconnect_grace;
        let Some(room) = self.rooms.get_mut(&id) else { return };
        let seat = room.seat_of(k);
        // (Signed in again from another place without dropping: the
        // opponent never saw them go.)
        let was_away = room.disconnected[seat].take().is_some();
        let missed = room.chat_missed[seat].take();
        let mut clock_left = None;
        if room.grace_until.is_some() && room.disconnected.iter().all(Option::is_none) {
            // Both are back after a restart: the clock starts now.
            room.grace_until = None;
            self.featured_dirty = true;
            room.deadline = Instant::now() + self.settings.shot_clock;
        } else if was_away
            && !room.finished
            && room.busy.is_none()
            && room.game.current_player() == seat
        {
            // Back at their turn: the shot clock waited for them (see
            // `tick_inner`), and gives them a fair moment at least.
            let least = Instant::now() + RECONNECT_CLOCK.min(self.settings.shot_clock);
            room.deadline = room.deadline.max(least);
            clock_left =
                Some(room.deadline.saturating_duration_since(Instant::now()).as_secs_f32());
        }
        let opponent = room.keys[1 - seat].clone();
        let msg = room.started_msg(seat, &self.sessions, clock, false);
        let result = room.result.clone();
        // The opponent dropped too and hasn't come back: the table says so
        // (with the time they have left), not that they are there.
        let opponent_away =
            (!room.finished).then(|| room.disconnected[1 - seat]).flatten().map(|since| match room
                .grace_until
            {
                Some(until) => until.saturating_duration_since(Instant::now()),
                None => grace.saturating_sub(since.elapsed()),
            });
        // What the opponent said while this player was away (the lines
        // still kept), and whether they asked for a rematch.
        let lines: Vec<ChatLine> = match missed {
            Some(mark) => {
                let n = room.chat_total.saturating_sub(mark).min(room.chat.len() as u64) as usize;
                room.chat.iter().skip(room.chat.len() - n).cloned().collect()
            }
            // Signed in again before the server saw the old connection go
            // (the usual way a game reconnects): what was sent to it in its
            // last moments may never have arrived. The last minute's lines
            // come again (the game leaves out those it has).
            None => {
                let since = now() - RECENT_CHAT_SECS;
                room.chat.iter().filter(|l| l.at >= since).cloned().collect()
            }
        };
        let rematch_offered = room.finished && room.rematch[1 - seat];
        self.send(k, msg);
        // The game ended while they were away: its end, too.
        if let Some(result) = result {
            self.send(k, ServerMsg::MatchEnded { result });
        }
        if let Some(left) = opponent_away {
            let grace_secs = left.as_secs().max(1) as u32;
            self.send(
                k,
                ServerMsg::OpponentConnection { connected: false, grace_secs, clock_left: None },
            );
        }
        let blocked = self.sessions.get(k).map(|s| s.blocked.clone()).unwrap_or_default();
        let author = |l: &ChatLine| l.from.eq_ignore_ascii_case(&self.display(&opponent));
        let opponent_id = self.rooms.get(&id).map(|r| r.users[1 - seat].id);
        if !opponent_id.is_some_and(|o| blocked.contains(&o)) {
            for line in lines.into_iter().filter(author) {
                self.send(k, ServerMsg::MatchChat { line });
            }
        }
        if rematch_offered {
            self.send(k, ServerMsg::RematchOffered);
        }
        // Who is watching, and who they cheer for (a new table shows none).
        let count = self.spectators(id).len() as u32;
        if count > 0 {
            self.send(k, ServerMsg::Spectators { count });
            self.send(k, ServerMsg::Support { counts: self.support(id) });
        }
        // (An opponent who left the table for another isn't told.)
        if was_away {
            let msg = ServerMsg::OpponentConnection { connected: true, grace_secs: 0, clock_left };
            if self.sessions.get(&opponent).is_some_and(|s| s.room == Some(id)) {
                self.send(&opponent, msg);
            }
        }
    }

    /// Connection `conn_id` closed.
    pub fn leave(&mut self, username: &str, conn_id: u64) {
        self.leave_conn(username, conn_id);
        self.flush_featured();
    }

    fn leave_conn(&mut self, username: &str, conn_id: u64) {
        let k = key(username);
        if self.sessions.get(&k).is_none_or(|s| s.conn_id != conn_id) {
            return; // A newer connection replaced this one.
        }
        self.close_table_of(&k);
        self.drop_invites_of(&k);
        self.stop_watching(&k);
        self.queue.retain(|q| q.key != k);
        self.drop_seek(&k);
        self.doubles_left(&k);
        let session = self.sessions.remove(&k).expect("checked");
        self.left_limits.insert(k.clone(), (session.limits(), Instant::now()));
        if let Some(id) = session.room
            && self.rooms.get(&id).is_some_and(|r| r.doubles.is_some())
        {
            if self.rooms.get(&id).is_some_and(|r| r.finished) {
                self.end_watching_of(&k, id);
            }
            self.doubles_dropped(&k, id);
        } else if let Some(id) = session.room
            && let Some(room) = self.rooms.get_mut(&id)
        {
            let seat = room.seat_of(&k);
            let opponent = room.keys[1 - seat].clone();
            if room.finished {
                room.present[seat] = false;
                if self.sessions.get(&opponent).is_some_and(|s| s.room == Some(id)) {
                    self.send(&opponent, ServerMsg::OpponentLeft);
                }
                self.end_watching_of(&k, id);
                self.close_room_if_empty(id);
            } else if self.draining.is_some()
                && room.busy.is_none()
                && !room.guests.iter().any(|g| *g)
            {
                // This server is going away: the game moves to the next one
                // (the opponent reconnects there too). A guest's game can't
                // move: it goes on here, as after any lost connection.
                self.hand_off(id);
            } else {
                room.disconnected[seat] = Some(Instant::now());
                room.chat_missed[seat] = Some(room.chat_total);
                let grace = self.settings.reconnect_grace.as_secs() as u32;
                self.send(
                    &opponent,
                    ServerMsg::OpponentConnection {
                        connected: false,
                        grace_secs: grace,
                        clock_left: None,
                    },
                );
            }
        }
        // A guest who leaves a tournament before its start (or a waiting
        // list) is taken off it: they can't come back as the same player.
        if session.guest {
            let waiting: Vec<u64> = self
                .tournaments
                .values()
                .filter(|t| t.has_entered(&k))
                .filter(|t| t.state == TournamentState::Registering || t.waitlist.contains(&k))
                .map(|t| t.id)
                .collect();
            for id in waiting {
                self.leave_tournament(&k, id);
            }
        }
        self.broadcast(&ServerMsg::PlayerLeft { username: session.user.username });
    }

    /// Whether connection `conn_id` is still the one `name` is signed in
    /// on. An account is used from one place at a time: signing in again
    /// replaces the session, and the connection it replaced may do nothing
    /// more in the account's name.
    pub fn is_current(&self, name: &str, conn_id: u64) -> bool {
        self.sessions.get(&key(name)).is_some_and(|s| s.conn_id == conn_id)
    }

    /// Disconnects a player (signed in elsewhere, banned…).
    pub fn kick(&mut self, name: &str, reason: &str) -> bool {
        let k = key(name);
        match self.sessions.get(&k) {
            Some(s) => {
                let _ = s.tx.try_send(ServerMsg::Kicked { reason: reason.to_string() });
                s.kill.notify_one();
                true
            }
            None => false,
        }
    }

    /// Why `name` can't change their username right now, if they can't: the
    /// game, table, queue or tournament they are in knows them by it.
    /// `name` is about to stop being an account's (renamed or deleted): the
    /// invitations to and from them end now, so none is accepted while the
    /// change is being made (a game would start under the old name).
    pub fn drop_invites_named(&mut self, name: &str) {
        let k = key(name);
        self.drop_invites_of(&k);
    }

    pub fn rename_blocker(&self, name: &str) -> Option<&'static str> {
        let k = key(name);
        let s = self.sessions.get(&k)?;
        if s.room.is_some() || self.rooms.values().any(|r| !r.finished && r.has(&k)) {
            Some("Finish your game before changing your username.")
        } else if s.table.is_some() || self.queue.iter().any(|q| q.key == k) {
            Some("Leave your table or the queue before changing your username.")
        } else if self
            .tournaments
            .values()
            .any(|t| t.state != TournamentState::Finished && t.has_entered(&k))
        {
            Some("Leave your tournament before changing your username.")
        } else {
            None
        }
    }

    /// Whether `name` has a game in progress (also while away from it), a
    /// table, a place in the queue or a tournament: the staff don't rename
    /// or delete an account then (whoever took the name freed could take
    /// the seat).
    pub fn is_busy(&self, name: &str) -> bool {
        let k = key(name);
        self.rooms.values().any(|r| !r.finished && r.has(&k))
            || self.sessions.get(&k).is_some_and(|s| s.table.is_some() || s.room.is_some())
            || self.queue.iter().any(|q| q.key == k)
            || self
                .tournaments
                .values()
                .any(|t| t.state != TournamentState::Finished && t.has_entered(&k))
    }

    /// The online player `old` changed their username to `user.username`:
    /// their session starts again under the new name on the same
    /// connection (everyone sees the old name leave and the new one come).
    /// Only when [`Lounge::rename_blocker`] has nothing against it.
    pub fn renamed(&mut self, old: &str, conn: u64, user: User, token: String) {
        let (old_k, new_k) = (key(old), key(&user.username));
        // This connection's session (not another device that signed in
        // meanwhile, still under the old name: that name is no longer the
        // account's, and someone else may take it).
        let Some(s) = self.sessions.get(&old_k) else { return };
        if s.conn_id != conn {
            self.kick(old, "Your username was changed. Sign in again.");
            return;
        }
        let (conn_id, tx, kill, ip) = (s.conn_id, s.tx.clone(), s.kill.clone(), s.ip.clone());
        let (blocked, look, achievements) =
            (s.blocked.clone(), s.look.clone(), s.achievements.clone());
        let (guest, rules, protocol) = (s.guest, s.rules, s.protocol);
        let friends = s.friends.clone();
        // Their friends know them by the new name.
        for other in self.sessions.values_mut() {
            if other.friends.remove(&old_k) {
                other.friends.insert(new_k.clone());
            }
        }
        // A new name doesn't wipe the slate: games left and games against
        // the same opponent still count.
        if old_k != new_k {
            if let Some(list) = self.abandoned.remove(&old_k) {
                self.abandoned.insert(new_k.clone(), list);
            }
            let swap = |n: &String| if *n == old_k { new_k.clone() } else { n.clone() };
            self.pair_games = std::mem::take(&mut self.pair_games)
                .into_iter()
                .map(|((a, b, rated), list)| {
                    let mut pair = [swap(&a), swap(&b)];
                    pair.sort();
                    let [a, b] = pair;
                    ((a, b, rated), list)
                })
                .collect();
            self.last_break = std::mem::take(&mut self.last_break)
                .into_iter()
                .map(|((a, b), (last, at))| {
                    let mut pair = [swap(&a), swap(&b)];
                    pair.sort();
                    let [a, b] = pair;
                    ((a, b), (swap(&last), at))
                })
                .collect();
            for (a, b, _) in &mut self.recent_pairs {
                (*a, *b) = (swap(a), swap(b));
            }
        }
        let name = user.username.clone();
        self.leave(old, conn_id);
        if let Some(limits) = self.left_limits.remove(&old_k) {
            self.left_limits.insert(new_k.clone(), limits);
        }
        self.join(conn_id, user, guest, rules, tx, kill, token, ip, blocked);
        self.set_friends(&name, friends);
        self.set_protocol(&name, protocol);
        self.set_achievements(&name, achievements);
        self.set_look(&name, look);
    }

    /// Replaces the cached account of an online player (after a change by
    /// the staff, a new country, a finished match…).
    pub fn refresh_user(&mut self, user: User) {
        let k = key(&user.username);
        if let Some(s) = self.sessions.get_mut(&k) {
            s.user = user;
            self.announce(&k);
        }
    }

    /// The effect worker wrote a result of these players (or gave up).
    pub fn record_written(&mut self, players: [i64; 2]) {
        for id in players {
            if let Some(n) = self.records_pending.get_mut(&id) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    self.records_pending.remove(&id);
                }
            }
        }
    }

    /// [`Self::refresh_user`] with the numbers a written result left, unless
    /// another of their results is still on its way (the session already
    /// counts it: the stored numbers are older).
    pub fn refresh_recorded(&mut self, user: User) {
        if !self.records_pending.contains_key(&user.id) {
            self.refresh_user(user);
        }
    }

    pub fn set_blocked(&mut self, name: &str, blocked: HashSet<i64>) {
        if let Some(s) = self.sessions.get_mut(&key(name)) {
            s.blocked = blocked;
        }
    }

    /// Sends a message to a player if they are online.
    /// A player's experience changed: everyone sees the new level.
    pub fn set_xp(&mut self, name: &str, xp: i64) {
        let k = key(name);
        let Some(s) = self.sessions.get_mut(&k) else { return };
        let before = snook_legend_proto::xp::level_of(s.user.xp);
        s.user.xp = xp;
        if snook_legend_proto::xp::level_of(xp) != before {
            self.announce(&k);
        }
    }

    pub fn notify(&self, name: &str, msg: ServerMsg) {
        self.send(&key(name), msg);
    }

    /// A message from the staff in the lounge chat.
    pub fn announce_text(&mut self, text: &str) {
        self.announce_line(text, Vec::new());
    }

    /// An announcement that mentions players (their usernames).
    fn announce_line(&mut self, text: &str, mentions: Vec<String>) {
        let text = clean_chat(text);
        if text.is_empty() {
            return;
        }
        let line = ChatLine {
            from: snook_legend_proto::GAME_NAME.into(),
            text,
            at: now(),
            kind: ChatKind::Announcement,
            mentions,
            ..Default::default()
        };
        let line = self.push_chat(line);
        self.broadcast(&ServerMsg::Chat { line });
    }

    /// New notes on the board: everyone sees them at once.
    pub fn set_board(&mut self, notes: Vec<snook_legend_proto::BoardNote>) {
        self.board = notes;
        self.broadcast(&ServerMsg::Board { notes: self.board.clone() });
    }

    pub fn set_motd(&mut self, motd: &str) {
        self.settings.motd = clean_text(motd, 300);
    }

    /// Adds a line to the lounge chat (numbered); returns it.
    fn push_chat(&mut self, mut line: ChatLine) -> ChatLine {
        self.chat_id += 1;
        line.id = self.chat_id;
        self.chat.push_back(line.clone());
        while self.chat.len() > self.settings.chat_history {
            if let Some(old) = self.chat.pop_front() {
                self.chat_deleted.remove(&old.id);
                self.chat_removed.remove(&old.id);
                self.chat_authors.remove(&old.id);
            }
        }
        line
    }

    /// A lounge chat line as players see it: none if taken away, without
    /// its text if deleted.
    fn shown(&self, l: &ChatLine) -> Option<ChatLine> {
        if self.chat_removed.contains(&l.id) {
            return None;
        }
        let mut l = l.clone();
        if let Some(&by) = self.chat_deleted.get(&l.id) {
            l.text = String::new();
            l.deleted = Some(by);
        }
        Some(l)
    }

    /// Deletes lounge chat line `id`: the writer their own, an administrator
    /// any (`silent`: without a trace).
    fn delete_chat(&mut self, k: &str, id: u64, silent: bool) {
        let Some(s) = self.sessions.get(k) else { return };
        let admin = s.user.role == snook_legend_proto::Role::Admin;
        if !self.chat.iter().any(|l| l.id == id && l.kind == ChatKind::Player) {
            return;
        }
        // The writer by account (not by a name someone else may have now).
        let own = self.chat_authors.get(&id).is_some_and(|a| *a == s.user.id);
        if self.chat_removed.contains(&id) || !(own || admin) || (silent && !admin) {
            return;
        }
        // Deleted once (asking again would tell the whole lounge again); an
        // administrator may still take a deleted line away for good.
        if !silent && self.chat_deleted.contains_key(&id) {
            return;
        }
        tracing::info!(by = k, line = id, silent, "chat line deleted");
        if silent {
            self.chat_deleted.remove(&id);
            self.chat_removed.insert(id);
            self.broadcast(&ServerMsg::ChatRemoved { id });
        } else {
            let by = if own {
                snook_legend_proto::Deleted::Author
            } else {
                snook_legend_proto::Deleted::Admin
            };
            self.chat_deleted.insert(id, by);
            self.broadcast(&ServerMsg::ChatDeleted { id, by });
        }
        self.unpin_deleted(Some(id));
    }

    /// The lounge chat as the staff see it: every line still kept, with its
    /// text also when deleted, and how it was deleted ("author", "staff",
    /// or "removed" when it was taken away without a trace), newest last.
    pub fn staff_chat(&self) -> Vec<serde_json::Value> {
        self.chat
            .iter()
            .filter(|l| l.id != 0)
            .map(|l| {
                let deleted = if self.chat_removed.contains(&l.id) {
                    Some("removed")
                } else {
                    match self.chat_deleted.get(&l.id) {
                        Some(snook_legend_proto::Deleted::Author) => Some("author"),
                        Some(snook_legend_proto::Deleted::Admin) => Some("staff"),
                        None => None,
                    }
                };
                serde_json::json!({
                    "id": l.id, "from": l.from, "text": l.text, "at": l.at,
                    "kind": l.kind, "rating": l.rating, "admin": l.admin, "deleted": deleted,
                    "mentions": l.mentions,
                })
            })
            .collect()
    }

    /// A message to everyone in the lounge.
    pub fn tell_everyone(&self, msg: &ServerMsg) {
        self.broadcast(msg);
    }

    /// The staff delete lounge chat line `id` from the panel: shown as
    /// deleted by the staff, or (`silent`) taken away without a trace.
    /// False if there is no such player line, or it is gone already.
    pub fn staff_delete_chat(&mut self, id: u64, silent: bool) -> bool {
        let found = self.chat.iter().any(|l| l.id == id && l.kind == ChatKind::Player);
        if !found || self.chat_removed.contains(&id) {
            return false;
        }
        if silent {
            self.chat_deleted.remove(&id);
            self.chat_removed.insert(id);
            self.broadcast(&ServerMsg::ChatRemoved { id });
        } else {
            if self.chat_deleted.contains_key(&id) {
                return false;
            }
            let by = snook_legend_proto::Deleted::Admin;
            self.chat_deleted.insert(id, by);
            self.broadcast(&ServerMsg::ChatDeleted { id, by });
        }
        self.unpin_deleted(Some(id));
        true
    }

    /// An administrator clears the lounge chat (the lines stay for reports).
    pub fn clear_chat(&mut self, by: &str) {
        let ids: Vec<u64> = self.chat.iter().map(|l| l.id).collect();
        self.chat_removed.extend(ids);
        self.chat_deleted.clear();
        self.broadcast(&ServerMsg::ChatCleared { by: by.to_string() });
        let line = ChatLine {
            from: snook_legend_proto::GAME_NAME.into(),
            text: format!("The chat was cleared by {by}."),
            at: now(),
            kind: ChatKind::System,
            ..Default::default()
        };
        self.push_chat(line);
        self.unpin_deleted(None);
    }

    /// Recent chat lines involving `target`, attached to a report.
    pub fn report_context(&self, reporter: &str, target: &str) -> String {
        let (r, t) = (key(reporter), key(target));
        let mut lines: Vec<String> = self
            .chat
            .iter()
            .filter(|l| key(&l.from) == t)
            .rev()
            .take(10)
            .map(|l| format!("[lounge] {}: {}", l.from, l.text))
            .collect();
        lines.reverse();
        if let Some(room) = self.rooms.values().find(|room| room.has(&r) && room.has(&t)) {
            lines.extend(room.chat.iter().map(|l| format!("[match] {}: {}", l.from, l.text)));
        }
        lines.join("\n")
    }

    pub fn live(&self) -> LiveSnapshot {
        let mut players: Vec<PlayerInfo> =
            self.sessions.keys().filter_map(|k| self.info(k)).collect();
        players.sort_by_key(|p| p.username.to_lowercase());
        let mut rooms: Vec<LiveRoom> = self
            .rooms
            .values()
            .map(|r| LiveRoom {
                id: r.id,
                players: [r.users[0].username.clone(), r.users[1].username.clone()],
                rated: r.rated,
                rules: r.game.rules(),
                shots: r.game.shots_taken(),
                finished: r.finished,
                tournament: r.tournament.is_some(),
            })
            .collect();
        rooms.sort_by_key(|r| r.id);
        LiveSnapshot {
            players,
            rooms,
            tables: self.tables.len(),
            tournaments: self.tournaments.values().map(|t| self.tournament_info(t)).collect(),
        }
    }

    /// `k` began a game in focus mode: the invitations waiting for them
    /// are withdrawn, and whoever sent them is told why.
    fn turn_invites_away(&mut self, k: &str) {
        let waiting: Vec<String> =
            self.invites.keys().filter(|(_, to)| to == k).map(|(from, _)| from.clone()).collect();
        for from in waiting {
            self.invites.remove(&(from.clone(), k.to_string()));
            self.send(k, ServerMsg::InviteCancelled { from: self.display(&from) });
            self.send(&from, ServerMsg::InviteFailed { reason: FOCUSED.into() });
        }
    }

    fn drop_invites_of(&mut self, k: &str) {
        let affected: Vec<(String, String)> =
            self.invites.keys().filter(|(f, t)| f == k || t == k).cloned().collect();
        for (from, to) in affected {
            self.invites.remove(&(from.clone(), to.clone()));
            if from == k {
                self.send(&to, ServerMsg::InviteCancelled { from: self.display(&from) });
            } else {
                self.send(&from, ServerMsg::InviteDeclined { by: self.display(&to) });
            }
        }
    }

    fn close_room_if_empty(&mut self, id: u64) {
        if self.rooms.get(&id).is_some_and(|r| r.players_present().iter().all(|(_, at)| !at)) {
            self.rooms.remove(&id);
            self.featured_dirty = true;
        }
    }

    /// Handles a message from a signed-in player.
    pub fn handle(&mut self, username: &str, msg: ClientMsg) {
        self.handle_msg(username, msg);
        self.flush_featured();
    }

    /// Picks the TV's game again if something changed it.
    fn flush_featured(&mut self) {
        if std::mem::take(&mut self.featured_dirty) {
            self.update_featured();
        }
    }

    fn handle_msg(&mut self, username: &str, msg: ClientMsg) {
        let k = key(username);
        let announced = matches!(
            msg,
            ClientMsg::OpenTable { .. }
                | ClientMsg::CloseTable
                | ClientMsg::JoinTable { .. }
                | ClientMsg::QueueJoin { .. }
                | ClientMsg::QueueLeave
                | ClientMsg::SetRules { .. }
                | ClientMsg::Tournaments
                | ClientMsg::CreateTournament { .. }
                | ClientMsg::JoinTournament { .. }
                | ClientMsg::LeaveTournament { .. }
                | ClientMsg::StartTournament { .. }
                | ClientMsg::CreateInviteCode { .. }
                | ClientMsg::JoinByCode { .. }
                | ClientMsg::PostSeek { .. }
                | ClientMsg::RemoveSeek { .. }
                | ClientMsg::WatchMatch { .. }
                | ClientMsg::DoublesQueue { .. }
                | ClientMsg::DoublesQueueLeave
                | ClientMsg::DoublesOpenTable { .. }
                | ClientMsg::DoublesJoinTable { .. }
                | ClientMsg::DoublesCloseTable
        );
        if announced && !self.take_action(&k) {
            return;
        }
        if self.handle_doubles(&k, &msg) {
            return;
        }
        match msg {
            ClientMsg::Chat { text } => self.chat_mentioning(&k, &text, &[]),
            ClientMsg::DeleteChat { id, silent } => self.delete_chat(&k, id, silent),
            ClientMsg::ClearChat => {
                if let Some(s) =
                    self.sessions.get(&k).filter(|s| s.user.role == snook_legend_proto::Role::Admin)
                {
                    let by = s.user.username.clone();
                    tracing::info!(by = %by, "lounge chat cleared");
                    self.clear_chat(&by);
                }
            }
            ClientMsg::MatchChat { text } => self.match_chat(&k, &text),
            ClientMsg::SpectatorChat { text } => self.spectator_chat(&k, &text),
            ClientMsg::React { code } => self.react(&k, &code),
            ClientMsg::Cheer { seat } => self.cheer(&k, seat),
            ClientMsg::StopSpectating => self.stop_watching(&k),
            ClientMsg::QueueJoin { rated, rules } => self.queue_join(&k, rated, rules),
            ClientMsg::QueueLeave => self.queue_leave(&k, true),
            ClientMsg::Invite { to, rated, rules } => {
                self.invite(&k, &key(&to), Offer { rated, rules })
            }
            ClientMsg::CancelInvite { to } => {
                let to = key(&to);
                if self.invites.remove(&(k.clone(), to.clone())).is_some() {
                    self.send(&to, ServerMsg::InviteCancelled { from: self.display(&k) });
                }
            }
            ClientMsg::AnswerInvite { from, accept } => self.answer(&k, &key(&from), accept),
            ClientMsg::OpenTable { rated, rules, password } => {
                self.open_table(&k, Offer { rated, rules }, password)
            }
            ClientMsg::JoinTable { id, password } => self.join_table(&k, id, password),
            ClientMsg::CloseTable => {
                self.close_table_of(&k);
                self.announce(&k);
            }
            ClientMsg::PlaceCueBall { position } => self.place_cue_ball(&k, position),
            ClientMsg::Aim { aim } => self.aim(&k, aim),
            ClientMsg::Shoot { turn, shot, called_pocket, cue_ball } => {
                self.shoot(&k, turn, shot, called_pocket, cue_ball)
            }
            ClientMsg::Choose { turn, option } => self.choose(&k, turn, option),
            ClientMsg::SetRules { rules } => self.set_rules(&k, rules),
            ClientMsg::Resign => self.resign(&k),
            ClientMsg::SetActivity { vs_computer, away, focus, pastime } => {
                // Everyone hears about it, so it shares the lounge actions'
                // limit; past it the change is kept but goes unannounced
                // (the next one tells everyone), without an error: the game
                // sends it by itself.
                let mut focused = false;
                if let Some(s) = self.sessions.get_mut(&k)
                    && (s.vs_computer, s.away, s.focus, s.pastime)
                        != (vs_computer, away, focus, pastime)
                {
                    focused = focus && !s.focus;
                    s.vs_computer = vs_computer;
                    s.away = away;
                    s.focus = focus;
                    s.pastime = pastime;
                    if s.action_bucket.take() {
                        self.announce(&k);
                    }
                }
                // A game in focus mode began: the invitations waiting for
                // this player are turned away (they wouldn't be seen).
                if focused {
                    self.turn_invites_away(&k);
                }
            }
            ClientMsg::Rematch => self.rematch(&k),
            ClientMsg::LeaveMatch => self.leave_match(&k),
            ClientMsg::Tournaments => {
                let tournaments =
                    self.tournaments.values().map(|t| self.tournament_info(t)).collect();
                self.send(&k, ServerMsg::Tournaments { tournaments });
            }
            ClientMsg::CreateTournament {
                name,
                size,
                rated,
                rules,
                password,
                entry_fee,
                format,
            } => {
                // Tournaments with a fee come through the connection (it takes the coins).
                let offer = Offer { rated, rules };
                self.create_tournament(&k, &name, size, offer, password, entry_fee, format, None);
            }
            ClientMsg::WatchMatch { match_id } => self.watch_match(&k, match_id),
            ClientMsg::CreateInviteCode { rated, rules } => {
                self.create_code(&k, Offer { rated, rules })
            }
            ClientMsg::JoinByCode { code } => self.join_by_code(&k, &code),
            ClientMsg::JoinTournament { id, password } => {
                self.join_tournament(&k, id, password, None);
            }
            ClientMsg::LeaveTournament { id } => self.leave_tournament(&k, id),
            ClientMsg::StartTournament { id } => self.start_tournament(&k, id),
            ClientMsg::PostSeek { text, rated, rules } => {
                self.post_seek(&k, &text, Offer { rated, rules })
            }
            // Someone else's note: the staff, through the connection.
            ClientMsg::RemoveSeek { username } => {
                if username.is_empty() || key(&username) == k {
                    self.drop_seek(&k);
                }
            }
            ClientMsg::Ping => self.send(&k, ServerMsg::Pong),
            // Handled by the connection (they need the database).
            ClientMsg::Hello { .. }
            | ClientMsg::Register { .. }
            | ClientMsg::Login { .. }
            | ClientMsg::Resume { .. }
            | ClientMsg::Guest { .. }
            | ClientMsg::Logout
            | ClientMsg::Leaderboard { .. }
            | ClientMsg::Profile { .. }
            | ClientMsg::SetCountry { .. }
            | ClientMsg::SetAvatar { .. }
            | ClientMsg::Friends
            | ClientMsg::Spectate { .. }
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
            | ClientMsg::GiftItem { .. }
            | ClientMsg::Missions
            | ClientMsg::Replays
            | ClientMsg::Replay { .. }
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
            | ClientMsg::PinChat { .. }
            | ClientMsg::UnpinChat
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
            | ClientMsg::CupMedal { .. } => {}
            ClientMsg::SetStatusMessage { .. }
            | ClientMsg::ClearStatusMessage { .. }
            | ClientMsg::RecentOpponents
            | ClientMsg::Gifts
            | ClientMsg::GiftsSeen { .. }
            | ClientMsg::ThankGift { .. } => {}
            // Answered by the server around the lounge (lib.rs).
            ClientMsg::SaveSettings { .. }
            | ClientMsg::DmSend { .. }
            | ClientMsg::DmHistory { .. }
            | ClientMsg::DmRead { .. }
            | ClientMsg::DmThreads
            | ClientMsg::AdvancedStats { .. }
            | ClientMsg::DoublesLeaderboard { .. }
            | ClientMsg::TimeAttack
            | ClientMsg::TimeAttackStart
            | ClientMsg::TimeAttackSubmit { .. } => {}
            // Handled above (`handle_doubles`).
            ClientMsg::DoublesInvite { .. }
            | ClientMsg::DoublesAnswer { .. }
            | ClientMsg::DoublesLeave
            | ClientMsg::DoublesQueue { .. }
            | ClientMsg::DoublesQueueLeave
            | ClientMsg::DoublesOpenTable { .. }
            | ClientMsg::DoublesJoinTable { .. }
            | ClientMsg::DoublesCloseTable => {}
        }
    }

    /// Takes one of `name`'s lounge actions, those everyone hears about
    /// (see [`ACTION_BURST`]); false, and they are asked to slow down, when
    /// none is left.
    pub fn take_action(&mut self, name: &str) -> bool {
        let k = key(name);
        if self.sessions.get_mut(&k).is_some_and(|s| s.action_bucket.take()) {
            return true;
        }
        self.error(&k, "Slow down a little.");
        false
    }

    /// Whether `k` gave too many wrong passwords for the private table or
    /// tournament `id` lately.
    fn password_locked(&self, k: &str, id: u64) -> bool {
        self.password_misses
            .get(&(k.to_string(), id))
            .is_some_and(|(n, since)| *n >= PASSWORD_MISSES && since.elapsed() < PASSWORD_LOCKOUT)
    }

    /// `k` gave a wrong password for `id`.
    fn password_missed(&mut self, k: &str, id: u64) {
        let e = self.password_misses.entry((k.to_string(), id)).or_insert((0, Instant::now()));
        if e.1.elapsed() >= PASSWORD_LOCKOUT {
            *e = (0, Instant::now());
        }
        e.0 += 1;
    }

    /// Checks the chat limits; the text to send, or None (with an error sent).
    fn chat_allowed(
        &mut self,
        k: &str,
        text: &str,
        place: snook_legend_proto::moderation::TextPlace,
    ) -> Option<String> {
        let text = clean_chat(text);
        if text.is_empty() {
            return None;
        }
        // Muted (why and until when), or a word the chat doesn't allow.
        if let Some(refusal) = self.text_refusal(k, &text) {
            self.refuse(k, place, refusal);
            return None;
        }
        let s = self.sessions.get_mut(k)?;
        if !self.settings.lan && s.user.chat_restricted() {
            let reason = chat_off_reason(&s.user);
            self.error(k, reason);
            return None;
        }
        if !self.settings.lan && match_social::phrases_only(&s.user) && !is_quick_phrase(&text) {
            self.error(k, PHRASES_ONLY);
            return None;
        }
        if !s.chat_bucket.take() {
            self.error(k, "You're sending messages too quickly.");
            return None;
        }
        if text.eq_ignore_ascii_case(&s.last_chat) {
            s.repeats += 1;
            if s.repeats >= 2 {
                self.error(k, "Please don't repeat the same message.");
                return None;
            }
        } else {
            s.repeats = 0;
            s.last_chat = text.clone();
        }
        Some(text)
    }

    /// A line in the lounge chat; the mentions of players who aren't online
    /// are among `known` (looked up in the store).
    fn chat_mentioning(&mut self, k: &str, text: &str, known: &[String]) {
        use snook_legend_proto::moderation::TextPlace;
        let Some(text) = self.chat_allowed(k, text, TextPlace::Chat) else { return };
        let mentions = self.mentions_in(&text, known);
        let s = &self.sessions[k];
        let sender = s.user.id;
        let line = ChatLine {
            from: s.user.username.clone(),
            text,
            at: now(),
            kind: ChatKind::Player,
            rating: s.shown_rating(),
            admin: s.user.role == snook_legend_proto::Role::Admin,
            mentions,
            ..Default::default()
        };
        let line = self.push_chat(line);
        self.chat_authors.insert(line.id, sender);
        let msg = ServerMsg::Chat { line };
        for (other, session) in &self.sessions {
            if !session.blocked.contains(&sender) && !self.chat_off(session) {
                self.send(other, msg.clone());
            }
        }
    }

    fn match_chat(&mut self, k: &str, text: &str) {
        if self.room_of(k).is_none() {
            return;
        }
        let place = snook_legend_proto::moderation::TextPlace::MatchChat;
        let Some(text) = self.chat_allowed(k, text, place) else { return };
        let s = &self.sessions[k];
        let line = ChatLine {
            from: s.user.username.clone(),
            text,
            at: now(),
            kind: ChatKind::Player,
            rating: s.shown_rating(),
            admin: s.user.role == snook_legend_proto::Role::Admin,
            ..Default::default()
        };
        let sender = s.user.id;
        let Some(id) = s.room else { return };
        let Some(room) = self.rooms.get_mut(&id) else { return };
        room.chat.push_back(line.clone());
        room.chat_total += 1;
        while room.chat.len() > MATCH_CHAT_KEEP {
            room.chat.pop_front();
        }
        for p in self.members(id) {
            let blocked = self
                .sessions
                .get(&p)
                .is_some_and(|s| s.blocked.contains(&sender) || self.chat_off(s));
            if !blocked {
                self.send(&p, ServerMsg::MatchChat { line: line.clone() });
            }
        }
        // The players' chat stays between them, except for administrators
        // watching the game.
        for w in self.admin_spectators(id) {
            self.send(&w, ServerMsg::MatchChat { line: line.clone() });
        }
    }

    /// Administrators watching room `id`.
    fn admin_spectators(&self, id: u64) -> Vec<String> {
        self.spectators(id)
            .into_iter()
            .filter(|w| {
                self.sessions.get(w).is_some_and(|s| s.user.role == snook_legend_proto::Role::Admin)
            })
            .collect()
    }

    /// `name`'s age group or the chat a parent or guardian allowed changed
    /// (see [`User::chat_restricted`]). The chat turned on or off, the
    /// lounge's chat comes again, with or without what players wrote.
    pub fn set_age(&mut self, name: &str, minor: Option<bool>, chat_allowed_at: Option<i64>) {
        let k = key(name);
        let Some(s) = self.sessions.get_mut(&k) else { return };
        let was = s.user.chat_restricted();
        s.user.minor = minor;
        s.user.chat_allowed_at = chat_allowed_at;
        if s.user.chat_restricted() != was {
            self.send_chat_history(&k);
        }
    }

    /// The chat with other players is off for this session (see
    /// [`User::chat_restricted`]). Not on a LAN server: its guests are the
    /// players of a local network, and have no account to keep an answer.
    fn chat_off(&self, s: &Session) -> bool {
        !self.settings.lan && s.user.chat_restricted()
    }

    /// The lounge's chat as the player at `k` sees it: without the lines of
    /// those they blocked, and, with their chat off, the server's notices
    /// only.
    fn send_chat_history(&self, k: &str) {
        let Some(s) = self.sessions.get(k) else { return };
        let off = self.chat_off(s);
        let lines = self
            .chat
            .iter()
            .filter(|l| !(off && l.kind == ChatKind::Player))
            .filter(|l| !self.chat_authors.get(&l.id).is_some_and(|a| s.blocked.contains(a)))
            .filter_map(|l| self.shown(l))
            .collect();
        self.send(k, ServerMsg::ChatHistory { lines });
        self.send_pinned(k);
    }

    /// The protocol of `name`'s game, from its hello (see
    /// [`Session::protocol`]): what it understands (the inbox, see
    /// [`INBOX_PROTOCOL`]) and whether it calls the 8.
    pub fn set_protocol(&mut self, name: &str, protocol: u32) {
        if let Some(s) = self.sessions.get_mut(&key(name)) {
            s.protocol = protocol;
        }
        self.doubles_hello(&key(name));
    }

    /// Sets what others see of `name`'s shop items (and tells everyone).
    pub fn set_look(&mut self, name: &str, look: Look) {
        let k = key(name);
        let Some(s) = self.sessions.get_mut(&k) else { return };
        if s.look != look {
            s.look = look;
            self.announce(&k);
        }
    }

    /// Records what `name` has unlocked and tells them (reactions follow).
    pub fn set_achievements(&mut self, name: &str, codes: Vec<String>) {
        let k = key(name);
        let Some(s) = self.sessions.get_mut(&k) else { return };
        s.achievements = codes.clone();
        self.send(&k, ServerMsg::MyAchievements { codes });
    }

    /// A reaction to everyone at the table (it must be unlocked; a few a
    /// second at most; muted players and blocked senders are not shown).
    fn react(&mut self, k: &str, code: &str) {
        let Some(r) = snook_legend_proto::reaction(code) else { return };
        let Some(s) = self.sessions.get_mut(k) else { return };
        if !r.available(&s.achievements, &s.look.reactions, s.look.vip) || !s.reaction_bucket.take()
        {
            return;
        }
        if s.user.is_muted(now()) {
            return;
        }
        let sender = s.user.id;
        let (id, seat) = match (s.room, s.watching.clone()) {
            (Some(id), _) => match self.rooms.get(&id) {
                Some(room) => (id, room.seat_of(k)),
                None => return,
            },
            // A spectator: through the room's crowd limit (also while the
            // friend watched is reconnecting).
            (None, Some(_)) => {
                let Some(id) = self.watched_room(k) else { return };
                let Some(room) = self.rooms.get_mut(&id) else { return };
                if !room.crowd.take() {
                    return;
                }
                (id, SPECTATOR_SEAT)
            }
            (None, None) => return,
        };
        let msg = ServerMsg::Reaction { seat, code: r.code.to_string() };
        for p in self.members(id).into_iter().chain(self.spectators(id)) {
            let blocked = self.sessions.get(&p).is_some_and(|s| s.blocked.contains(&sender));
            if !blocked {
                self.send(&p, msg.clone());
            }
        }
    }

    fn queue_join(&mut self, k: &str, rated: bool, rules: Rules) {
        if self.refuse_if_draining(k) {
            return;
        }
        if self.refuse_if_cooling_down(k) {
            return;
        }
        let Some(s) = self.sessions.get(k) else { return };
        if s.room.is_some() {
            return self.error(k, "Finish your game first.");
        }
        if self.in_tournament(k).is_some() {
            return self.error(k, "Players in a tournament can't start other games.");
        }
        let rated = rated && !s.guest && !self.settings.lan;
        let was_searching = self.queue.iter().any(|q| q.key == k);
        self.queue.retain(|q| q.key != k);
        self.close_table_of(k);
        self.stop_watching(k);
        let now = Instant::now();
        self.queue.push(Queued { key: k.to_string(), rated, rules, since: now, last_status: now });
        self.send(
            k,
            ServerMsg::Queue {
                searching: true,
                rated,
                rules,
                waited_secs: 0,
                range: queue_range(0.0),
            },
        );
        if !was_searching {
            self.announce(k);
        }
        self.match_queue();
    }

    /// Takes `k` out of the queue (`tell`: they asked, so confirm it).
    fn queue_leave(&mut self, k: &str, tell: bool) {
        let stopped = |rated, rules| ServerMsg::Queue {
            searching: false,
            rated,
            rules,
            waited_secs: 0,
            range: 0,
        };
        match self.queue.iter().position(|q| q.key == k) {
            Some(at) => {
                let q = self.queue.remove(at);
                self.send(k, stopped(q.rated, q.rules));
                self.announce(k);
            }
            None if tell => {
                let rules = self.sessions.get(k).map(|s| s.rules).unwrap_or_default();
                self.send(k, stopped(false, rules));
            }
            None => {}
        }
    }

    /// Pairs whoever can be paired: the closest ratings first, within what
    /// both have waited for.
    fn match_queue(&mut self) {
        let now = Instant::now();
        self.recent_pairs.retain(|(_, _, t)| now.duration_since(*t) < Duration::from_secs(120));
        // Drop whoever can no longer play (left, started a game elsewhere,
        // went into a tournament).
        let gone: Vec<String> = self
            .queue
            .iter()
            .filter(|q| {
                self.sessions.get(&q.key).is_none_or(|s| s.room.is_some())
                    || self.in_tournament(&q.key).is_some()
            })
            .map(|q| q.key.clone())
            .collect();
        for k in gone {
            self.queue_leave(&k, false);
        }
        loop {
            let mut best: Option<(usize, usize, i32)> = None;
            for i in 0..self.queue.len() {
                for j in (i + 1)..self.queue.len() {
                    let (a, b) = (&self.queue[i], &self.queue[j]);
                    if (a.rated, a.rules) != (b.rated, b.rules)
                        || self.blocks(&a.key, &b.key)
                        || self.blocks(&b.key, &a.key)
                    {
                        continue;
                    }
                    let waited = |q: &Queued| now.duration_since(q.since).as_secs_f32();
                    // Players who often leave games wait longer for the next.
                    if waited(a) < self.abandon_penalty(&a.key)
                        || waited(b) < self.abandon_penalty(&b.key)
                    {
                        continue;
                    }
                    // Not the same opponent twice in a row, unless nobody else comes.
                    let recent = self.recent_pairs.iter().any(|(x, y, _)| {
                        (x == &a.key && y == &b.key) || (x == &b.key && y == &a.key)
                    });
                    if recent && waited(a).min(waited(b)) < 45.0 {
                        continue;
                    }
                    let ra = self.sessions[&a.key].user.records.get(a.rules).rating;
                    let rb = self.sessions[&b.key].user.records.get(b.rules).rating;
                    let gap = (ra - rb).unsigned_abs();
                    let allowed = queue_range(waited(a)).min(queue_range(waited(b)));
                    if gap <= allowed && best.is_none_or(|(_, _, g)| (gap as i32) < g) {
                        best = Some((i, j, gap as i32));
                    }
                }
            }
            let Some((i, j, _)) = best else { break };
            let b = self.queue.remove(j);
            let a = self.queue.remove(i);
            for q in [&a, &b] {
                self.queue_waits.push_back(now.duration_since(q.since).as_secs() as u32);
            }
            while self.queue_waits.len() > 500 {
                self.queue_waits.pop_front();
            }
            self.recent_pairs.push_back((a.key.clone(), b.key.clone(), now));
            let found = ServerMsg::Queue {
                searching: false,
                rated: a.rated,
                rules: a.rules,
                waited_secs: 0,
                range: 0,
            };
            for k in [&a.key, &b.key] {
                self.send(k, found.clone());
            }
            let offer = Offer { rated: a.rated, rules: a.rules };
            self.start_room([a.key, b.key], offer, None, None, false);
        }
        // Keep the waiting players informed (every few seconds).
        let updates: Vec<(String, ServerMsg)> = self
            .queue
            .iter_mut()
            .filter(|q| now.duration_since(q.last_status) >= Duration::from_secs(3))
            .map(|q| {
                q.last_status = now;
                let secs = now.duration_since(q.since).as_secs() as u32;
                let msg = ServerMsg::Queue {
                    searching: true,
                    rated: q.rated,
                    rules: q.rules,
                    waited_secs: secs,
                    range: queue_range(secs as f32),
                };
                (q.key.clone(), msg)
            })
            .collect();
        for (k, msg) in updates {
            self.send(&k, msg);
        }
    }

    /// Everyone watching room `id` (not its players).
    /// The players still at room `id`: both while it is played; once it is
    /// over, those who haven't gone on (a player back in the lounge, or in
    /// another game, hears nothing more from it).
    fn members(&self, id: u64) -> Vec<String> {
        let Some(room) = self.rooms.get(&id) else { return Vec::new() };
        // (One who is offline is still theirs: their friends watching them
        // hear how it ended.)
        room.all_keys()
            .into_iter()
            .filter(|k| !room.finished || self.sessions.get(k).is_none_or(|s| s.room == Some(id)))
            .collect()
    }

    fn spectators(&self, id: u64) -> Vec<String> {
        let Some(room) = self.rooms.get(&id) else { return Vec::new() };
        let mut out: Vec<String> = self
            .members(id)
            .iter()
            .filter_map(|k| self.watchers.get(k))
            .flatten()
            .filter(|w| !room.has(w))
            .cloned()
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Sends to both players and everyone watching.
    fn send_room(&self, id: u64, msg: &ServerMsg) {
        for k in self.members(id).into_iter().chain(self.spectators(id)) {
            self.send(&k, msg.clone());
        }
    }

    /// Tells everyone at the table how many are watching, and who they
    /// cheer for.
    fn announce_spectators(&self, id: u64) {
        let count = self.spectators(id).len() as u32;
        self.send_room(id, &ServerMsg::Spectators { count });
        self.send_room(id, &ServerMsg::Support { counts: self.support(id) });
    }

    /// How many of those watching room `id` cheer for each seat.
    fn support(&self, id: u64) -> [u32; 2] {
        let mut counts = [0, 0];
        let Some(room) = self.rooms.get(&id) else { return counts };
        for w in self.spectators(id) {
            if let Some(&seat) = room.cheers.get(&w) {
                counts[seat % 2] += 1;
            }
        }
        counts
    }

    /// A spectator picks a side (or changes their mind).
    fn cheer(&mut self, k: &str, seat: usize) {
        if seat > 1 {
            return;
        }
        let Some(s) = self.sessions.get_mut(k) else { return };
        if !s.reaction_bucket.take() {
            return;
        }
        if s.watching.is_none() {
            return;
        }
        let Some(id) = self.watched_room(k) else { return };
        let Some(room) = self.rooms.get_mut(&id) else { return };
        if room.has(k) || room.cheers.get(k) == Some(&seat) {
            return;
        }
        room.cheers.insert(k.to_string(), seat);
        self.send_room(id, &ServerMsg::Support { counts: self.support(id) });
    }

    fn send_spectator_start(&self, w: &str, id: u64, seat: usize) {
        if let Some(room) = self.rooms.get(&id) {
            let clock = self.settings.shot_clock.as_secs() as u32;
            self.send_lineup(w, id);
            self.send(w, room.started_msg(seat, &self.sessions, clock, true));
            if let Some(result) = room.result.clone() {
                self.send(w, ServerMsg::MatchEnded { result });
            }
            // An administrator also gets what the players said so far.
            if self.sessions.get(w).is_some_and(|s| s.user.role == snook_legend_proto::Role::Admin)
            {
                for line in &room.chat {
                    self.send(w, ServerMsg::MatchChat { line: line.clone() });
                }
            }
            // What the other spectators said so far.
            self.send_spectator_chat(w, id);
        }
    }

    /// `k` starts watching `friend`'s games (the caller checked they may).
    pub fn spectate(&mut self, k: &str, friend: &str) {
        let friend = key(friend);
        let Some(me) = self.sessions.get(k) else { return };
        if me.room.is_some() {
            return self.error(k, "Finish your game first.");
        }
        if friend == k {
            return;
        }
        // (A friend reconnecting is still at their table: watching it goes
        // on, as for whoever was watching already.)
        let room = self
            .sessions
            .get(&friend)
            .and_then(|f| f.room)
            .and_then(|id| self.rooms.get(&id))
            .or_else(|| self.rooms.values().find(|r| !r.finished && r.has(&friend)));
        let Some(room) = room else {
            return self.error(k, "Your friend isn't playing right now.");
        };
        // Neither player may have blocked whoever watches.
        if room.all_keys().iter().any(|p| self.blocks(p, k)) {
            return self.error(k, "You can't watch this game.");
        }
        let (id, seat) = (room.id, room.seat_of(&friend));
        self.stop_watching(k);
        self.watchers.entry(friend.clone()).or_default().insert(k.to_string());
        if let Some(s) = self.sessions.get_mut(k) {
            s.watching = Some(friend.clone());
            s.watching_tv = false;
        }
        self.send_spectator_start(k, id, seat);
        self.announce_spectators(id);
        self.tell_friends(k);
    }

    /// `w` stops watching, told why (if they were watching).
    fn end_spectating(&mut self, w: &str, reason: &str) {
        if self.sessions.get(w).is_some_and(|s| s.watching.is_some()) {
            self.stop_watching(w);
            self.send(w, ServerMsg::SpectateEnded { reason: reason.into() });
        }
    }

    /// `name` blocked `other`: invitations between them are withdrawn (each
    /// is told as if it was declined or cancelled), and `other` no longer
    /// watches `name`'s game.
    pub fn blocked_player(&mut self, name: &str, other: &str) {
        let (a, b) = (key(name), key(other));
        if self.invites.remove(&(a.clone(), b.clone())).is_some() {
            self.send(&b, ServerMsg::InviteCancelled { from: self.display(&a) });
        }
        if self.invites.remove(&(b.clone(), a.clone())).is_some() {
            self.send(&b, ServerMsg::InviteDeclined { by: self.display(&a) });
        }
        let watched = self.sessions.get(&b).and_then(|s| s.watching.clone());
        let room = watched.and_then(|p| self.sessions.get(&p)).and_then(|s| s.room);
        if room.and_then(|id| self.rooms.get(&id)).is_some_and(|r| r.has(&a)) {
            self.end_spectating(&b, "You can't watch this game.");
        }
    }

    /// Two players are no longer friends: neither goes on watching the
    /// other's games (watching is for friends; the lounge's featured game
    /// is everyone's).
    pub fn unfriended(&mut self, a: &str, b: &str) {
        let (a, b) = (key(a), key(b));
        for (w, p) in [(&a, &b), (&b, &a)] {
            let watches = self
                .sessions
                .get(w)
                .is_some_and(|s| !s.watching_tv && s.watching.as_deref() == Some(p.as_str()));
            if watches {
                self.end_spectating(w, "You can't watch this game.");
            }
        }
    }

    /// A minor's chat was turned off: their status message and their note
    /// on the board are no longer shown.
    pub fn silenced(&mut self, name: &str) {
        let k = key(name);
        self.drop_seek(&k);
        if let Some(u) = self.session_user(name) {
            let mut u = u.clone();
            u.status_message.clear();
            self.refresh_user(u);
        }
    }

    /// `k` stops watching (if they were).
    pub fn stop_watching(&mut self, k: &str) {
        let Some(friend) = self.sessions.get_mut(k).and_then(|s| {
            s.watching_tv = false;
            s.watching.take()
        }) else {
            return;
        };
        if let Some(set) = self.watchers.get_mut(&friend) {
            set.remove(k);
            if set.is_empty() {
                self.watchers.remove(&friend);
            }
        }
        if let Some(id) = self.sessions.get(&friend).and_then(|f| f.room) {
            self.announce_spectators(id);
        }
        self.tell_friends(k);
    }

    /// `k` left their game: whoever was watching them goes back too.
    fn end_watching_of(&mut self, k: &str, room: u64) {
        let Some(set) = self.watchers.remove(k) else { return };
        for w in set {
            if let Some(s) = self.sessions.get_mut(&w) {
                s.watching = None;
            }
            self.send(
                &w,
                ServerMsg::SpectateEnded { reason: "Your friend left the table.".into() },
            );
            self.tell_friends(&w);
        }
        self.announce_spectators(room);
    }

    fn room_of(&self, k: &str) -> Option<&Room> {
        self.sessions.get(k).and_then(|s| s.room).and_then(|id| self.rooms.get(&id))
    }

    fn available(&self, k: &str) -> bool {
        self.sessions.get(k).is_some_and(|s| s.room.is_none())
    }

    /// Free for an invitation: in no game, or only looking at the result
    /// of one that is over (the lounge shows them free).
    fn invitable(&self, k: &str) -> bool {
        self.sessions.get(k).is_some_and(|s| s.room.is_none_or(|id| !self.live_room(id)))
    }

    /// In a tournament that is still going (and not yet knocked out of it).
    fn in_tournament(&self, k: &str) -> Option<u64> {
        self.tournaments
            .values()
            .find(|t| match t.state {
                TournamentState::Registering => t.players.iter().any(|p| p == k),
                TournamentState::Running => t.alive(k),
                TournamentState::Finished => false,
            })
            .map(|t| t.id)
    }

    /// Still playing in a running tournament: no other games until knocked out.
    fn tournament_busy(&self, k: &str) -> bool {
        self.tournaments.values().any(|t| t.state == TournamentState::Running && t.alive(k))
    }

    fn is_guest(&self, k: &str) -> bool {
        self.sessions.get(k).is_none_or(|s| s.guest)
    }

    fn blocks(&self, who: &str, other: &str) -> bool {
        match (self.sessions.get(who), self.sessions.get(other)) {
            (Some(a), Some(b)) => a.blocked.contains(&b.user.id),
            _ => false,
        }
    }

    fn invite(&mut self, from: &str, to: &str, offer: Offer) {
        let fail = |l: &Self, reason: &str| {
            l.send(from, ServerMsg::InviteFailed { reason: reason.into() })
        };
        if self.draining.is_some() {
            return fail(self, "The server is restarting: new games start in a moment.");
        }
        if from == to {
            return fail(self, "You can't invite yourself.");
        }
        if let Some(left) = self.abandon_cooldown(from) {
            return fail(self, &cooldown_message(left));
        }
        if !self.sessions.contains_key(to) {
            return fail(self, "That player is not online.");
        }
        if !self.invitable(from) || !self.invitable(to) {
            return fail(self, "That player is in a game.");
        }
        if self.tournament_busy(from) || self.tournament_busy(to) {
            return fail(self, "Players in a tournament can't start other games.");
        }
        if self.blocks(to, from) {
            return fail(self, "That player is not accepting invitations.");
        }
        if self.sessions.get(to).is_some_and(|s| s.focus) {
            return fail(self, FOCUSED);
        }
        if self.invites.contains_key(&(from.to_string(), to.to_string())) {
            return;
        }
        if !self.sessions.get_mut(from).is_some_and(|s| s.invite_bucket.take()) {
            return fail(self, "You're sending invitations too quickly.");
        }
        let offer =
            Offer { rated: offer.rated && !self.is_guest(from) && !self.is_guest(to), ..offer };
        if let Some(back) = self.invites.get(&(to.to_string(), from.to_string())).copied() {
            // They already invited us: show their invitation again.
            return self.send(
                from,
                ServerMsg::Invited { from: self.display(to), rated: back.rated, rules: back.rules },
            );
        }
        self.invites.insert((from.to_string(), to.to_string()), offer);
        let Offer { rated, rules } = offer;
        self.send(to, ServerMsg::Invited { from: self.display(from), rated, rules });
        self.send(from, ServerMsg::InviteSent { to: self.display(to), rated, rules });
    }

    fn answer(&mut self, me: &str, from: &str, accept: bool) {
        let Some(offer) = self.invites.remove(&(from.to_string(), me.to_string())) else {
            return self.send(
                me,
                ServerMsg::InviteFailed { reason: "That invitation is no longer valid.".into() },
            );
        };
        if !accept {
            self.send(from, ServerMsg::InviteDeclined { by: self.display(me) });
            return;
        }
        // A block since the invitation: it is no longer good.
        if self.blocks(from, me) || self.blocks(me, from) {
            return self.send(
                me,
                ServerMsg::InviteFailed { reason: "That invitation is no longer valid.".into() },
            );
        }
        if let Some(left) = self.abandon_cooldown(me) {
            self.send(from, ServerMsg::InviteDeclined { by: self.display(me) });
            return self.send(me, ServerMsg::InviteFailed { reason: cooldown_message(left) });
        }
        // Either may have gone into a tournament since the invitation.
        if self.tournament_busy(from) || self.tournament_busy(me) {
            self.send(from, ServerMsg::InviteDeclined { by: self.display(me) });
            return self.send(
                me,
                ServerMsg::InviteFailed {
                    reason: "Players in a tournament can't start other games.".into(),
                },
            );
        }
        if !self.invitable(from) || !self.invitable(me) {
            return self.send(
                me,
                ServerMsg::InviteFailed { reason: "That player is no longer available.".into() },
            );
        }
        // Either still at the result of a game over: it is left for this one.
        self.leave_finished(from);
        self.leave_finished(me);
        self.start_room(
            [from.to_string(), me.to_string()],
            offer,
            None,
            Some(from.to_string()),
            false,
        );
    }

    fn table_info(&self, t: &Table) -> Option<TableInfo> {
        Some(TableInfo {
            id: t.id,
            host: self.info(&t.host)?,
            rated: t.rated,
            rules: t.rules,
            private: t.password.is_some(),
        })
    }

    fn open_table(&mut self, k: &str, offer: Offer, password: Option<String>) {
        if self.refuse_if_draining(k) || self.refuse_if_cooling_down(k) {
            return;
        }
        if !self.available(k) {
            return self.error(k, "Finish your game first.");
        }
        if self.tournament_busy(k) {
            return self.error(k, "Players in a tournament can't start other games.");
        }
        let Ok(password) = room_password(password) else {
            return self.error(k, UNUSABLE_PASSWORD);
        };
        self.close_table_of(k);
        let id = self.new_id();
        let table = Table {
            id,
            host: k.to_string(),
            rated: offer.rated && !self.is_guest(k),
            rules: offer.rules,
            password,
        };
        if let Some(s) = self.sessions.get_mut(k) {
            s.table = Some(id);
        }
        self.announce(k);
        if let Some(info) = self.table_info(&table) {
            self.broadcast(&ServerMsg::TableOpened { table: info });
        }
        self.tables.insert(id, table);
    }

    fn close_table_of(&mut self, k: &str) {
        let Some(id) = self.sessions.get_mut(k).and_then(|s| s.table.take()) else { return };
        self.tables.remove(&id);
        self.broadcast(&ServerMsg::TableClosed { id });
    }

    fn join_table(&mut self, k: &str, id: u64, password: Option<String>) {
        if self.refuse_if_draining(k) || self.refuse_if_cooling_down(k) {
            return;
        }
        let Some(t) = self.tables.get(&id) else {
            return self.error(k, "That table is no longer open.");
        };
        let host = t.host.clone();
        if host == k {
            return;
        }
        if let Some(expected) = t.password.clone() {
            if self.password_locked(k, id) {
                return self.error(k, TOO_MANY_PASSWORDS);
            }
            if !ct_eq(&expected, &clean_password(password).unwrap_or_default()) {
                self.password_missed(k, id);
                return self.error(k, "Wrong table password.");
            }
        }
        if !self.available(k) {
            return self.error(k, "Finish your game first.");
        }
        if self.tournament_busy(k) || self.tournament_busy(&host) {
            return self.error(k, "Players in a tournament can't start other games.");
        }
        if self.blocks(&host, k) {
            return self.error(k, "You can't join this table.");
        }
        let offer = Offer { rated: t.rated && !self.is_guest(k), rules: t.rules };
        // A table with a password is private (off the lounge's TV); one an
        // invitation code made is an invitation, like one sent by name.
        let by_code = self.codes.values().any(|(table, _)| *table == id);
        let private = t.password.is_some() && !by_code;
        self.close_table_of(k);
        self.close_table_of(&host);
        self.codes.retain(|_, (t, _)| *t != id);
        self.start_room([host.clone(), k.to_string()], offer, None, Some(host), private);
    }

    /// `host`: whose table it is (their cloth and balls); else the higher
    /// rated player's. `private`: a table with a password.
    fn start_room(
        &mut self,
        mut keys: [String; 2],
        offer: Offer,
        tournament: Option<(u64, usize, usize)>,
        host: Option<String>,
        private: bool,
    ) {
        if !keys.iter().all(|k| self.sessions.contains_key(k)) {
            return;
        }
        if self.draining.is_some() {
            for k in &keys {
                self.error(k, "The server is restarting: new games start in a moment.");
            }
            return;
        }
        if tournament.is_none() && rand::thread_rng().gen_bool(0.5) {
            keys.swap(0, 1);
        }
        for k in &keys {
            self.drop_invites_of(k);
            self.close_table_of(k);
            self.queue_leave(k, false);
        }
        let users = [self.sessions[&keys[0]].user.clone(), self.sessions[&keys[1]].user.clone()];
        let guests = [self.sessions[&keys[0]].guest, self.sessions[&keys[1]].guest];
        let Offer { rated, rules } = offer;
        let rated = rated && !guests[0] && !guests[1];
        let host = host.unwrap_or_else(|| {
            let rating = |u: &User| u.records.get(rules).rating;
            if rating(&users[0]) >= rating(&users[1]) { keys[0].clone() } else { keys[1].clone() }
        });
        let id = self.new_id();
        let look = self.table_look_of(&host);
        let seed: u64 = rand::thread_rng().r#gen();
        let breaker = self.first_breaker(&keys, rules);
        let game = Game { seed, breaker, rules };
        self.create_room(id, keys, users, guests, rated, game, tournament, look, private, None);
    }

    /// Who breaks a new game between these two: under the Brazilian rules
    /// one break each, game after game (the one whose name sorts first
    /// breaks the first of the day); the first seat otherwise.
    pub(crate) fn first_breaker(&mut self, keys: &[String; 2], rules: Rules) -> usize {
        if rules != Rules::Brazilian {
            return 0;
        }
        // (Not by counting their games: a game under the other rules, or
        // one that leaves the day's count as another enters it, would give
        // the same player two breaks running.)
        let (a, b, _) = pair_key(keys, false);
        match self.last_break.get(&(a, b)) {
            Some((last, at)) if at.elapsed() < PAIR_WINDOW => usize::from(*last == keys[0]),
            _ => usize::from(keys[0] > keys[1]),
        }
    }

    /// The cloth and ball set `host` brings to the table.
    fn table_look_of(&self, host: &str) -> [String; 2] {
        self.sessions
            .get(host)
            .map(|s| [s.look.cloth.clone(), s.look.balls.clone()])
            .unwrap_or_default()
    }

    #[allow(clippy::too_many_arguments)]
    fn create_room(
        &mut self,
        id: u64,
        keys: [String; 2],
        users: [User; 2],
        guests: [bool; 2],
        rated: bool,
        game: Game,
        tournament: Option<(u64, usize, usize)>,
        table_look: [String; 2],
        private: bool,
        doubles: Option<Box<doubles::Doubles>>,
    ) {
        let Game { seed, breaker, rules } = game;
        if rules == Rules::Brazilian && doubles.is_none() {
            let (a, b, _) = pair_key(&keys, false);
            self.last_break.insert((a, b), (keys[breaker % 2].clone(), Instant::now()));
        }
        let mut turns = [0, 0];
        turns[breaker] = 1;
        let stakes = match &doubles {
            Some(d) => rated.then(|| Self::doubles_stakes(d)),
            None => rated.then(|| self.stakes_for(&keys, &users, rules)),
        };
        let room = Room {
            id,
            keys: keys.clone(),
            users,
            guests,
            rated,
            game: EightBallGame::new(seed, breaker, rules),
            // (The players see the match's introduction first.)
            deadline: Instant::now() + MATCH_INTRO + self.settings.shot_clock,
            disconnected: [None, None],
            finished: false,
            rematch: [false, false],
            present: [true, true],
            result: None,
            breaker,
            busy: None,
            idle: [0, 0],
            last_was_break: false,
            shot_at: Instant::now(),
            turns,
            tournament,
            chat: VecDeque::new(),
            crowd: Bucket::new(4.0, 0.6),
            cheers: HashMap::new(),
            replay: Replay::new(seed, breaker, rules),
            stats: Default::default(),
            bands: Default::default(),
            table_look,
            grace_until: None,
            ended: None,
            private,
            resigning: None,
            leaving: [false, false],
            think: Vec::new(),
            stakes,
            spectator_chat: VecDeque::new(),
            chat_total: 0,
            chat_missed: [None, None],
            doubles,
        };
        let clock = self.settings.shot_clock.as_secs() as u32;
        let everyone = room.all_keys();
        for k in &everyone {
            self.stop_watching(k);
            // Found a game: off the "looking for a game" board.
            self.drop_seek(k);
            if let Some(s) = self.sessions.get_mut(k) {
                s.room = Some(id);
                // The game against the computer was left for this one.
                s.vs_computer = false;
                s.away = false;
                s.focus = false;
            }
        }
        self.rooms.insert(id, room);
        let room = &self.rooms[&id];
        for k in &everyone {
            // (A doubles player hears who plays with whom first.)
            self.send_lineup(k, id);
            let msg = room.started_msg(room.seat_of(k), &self.sessions, clock, false);
            self.send(k, msg);
        }
        self.featured_dirty = true;
        for k in &everyone {
            self.announce(k);
        }
        // Friends watching either player follow them to the new game; who
        // watched on the lounge's TV only to a game the TV could show, and
        // nobody to a game whose players blocked them.
        for k in &everyone {
            let seat = self.rooms[&id].seat_of(k);
            for w in self.watchers.get(k).cloned().unwrap_or_default() {
                let tv = self.sessions.get(&w).is_some_and(|s| s.watching_tv);
                if (tv && private) || everyone.iter().any(|p| self.blocks(p, &w)) {
                    self.end_spectating(&w, "You can't watch this game.");
                    continue;
                }
                self.send_spectator_start(&w, id, seat);
            }
        }
        self.announce_spectators(id);
        self.save_live(id);
        tracing::info!(room = id, rated, "match started: {everyone:?}");
    }

    /// Saves a game in progress (games with accounts only).
    fn save_live(&self, id: u64) {
        let Some(room) = self.rooms.get(&id) else { return };
        // (Doubles games aren't saved: they go on here to the end.)
        if self.settings.lan
            || room.guests.iter().any(|g| *g)
            || room.finished
            || room.doubles.is_some()
        {
            return;
        }
        self.effect(Effect::SaveLive(LiveMatch {
            id: id as i64,
            players: [room.users[0].id, room.users[1].id],
            rated: room.rated,
            replay: room.replay.clone(),
            updated_at: now(),
        }));
    }

    /// The running room of `k` and their seat, if it is their turn.
    fn my_turn(&self, k: &str) -> Option<(u64, usize)> {
        let id = self.sessions.get(k)?.room?;
        let room = self.rooms.get(&id)?;
        let seat = room.seat_of(k);
        // (In doubles, the team's player whose visit it is.)
        let holds = room.doubles.is_none() || room.holds_cue(k);
        (!room.finished && room.busy.is_none() && room.game.current_player() == seat && holds)
            .then_some((id, seat))
    }

    /// True if `k` may relay another aim update now (drops floods).
    fn aim_allowed(&mut self, k: &str) -> bool {
        let Some(s) = self.sessions.get_mut(k) else { return false };
        let now = Instant::now();
        if s.last_aim.is_some_and(|t| now.duration_since(t) < AIM_RELAY) {
            return false;
        }
        s.last_aim = Some(now);
        true
    }

    fn place_cue_ball(&mut self, k: &str, p: DVec2) {
        if !p.is_finite() {
            return;
        }
        let Some((id, seat)) = self.my_turn(k) else { return };
        // The final position comes with the shot, so dropping some is harmless.
        if !self.aim_allowed(k) {
            return;
        }
        let room = self.rooms.get_mut(&id).expect("room");
        if room.game.ball_in_hand().is_some() && room.game.place_cue_ball(p) {
            let _ = seat;
            for other in room.all_keys().into_iter().filter(|o| o != k) {
                self.send(&other, ServerMsg::OpponentCueBall { position: p });
            }
            for w in self.spectators(id) {
                self.send(&w, ServerMsg::OpponentCueBall { position: p });
            }
        }
    }

    fn aim(&mut self, k: &str, aim: AimState) {
        if !aim.is_finite() {
            return;
        }
        let Some((id, seat)) = self.my_turn(k) else { return };
        // What goes on is an aim the cue can take (as a shot is cleaned
        // before it is played), up to the elevation this table allows.
        let max_elevation = self.rooms.get(&id).map_or(0.0, |r| r.game.params().max_elevation);
        let aim = aim.tidied();
        let aim = AimState { elevation: aim.elevation.min(max_elevation), ..aim };
        if !self.aim_allowed(k) {
            return;
        }
        let _ = seat;
        for other in self.rooms[&id].all_keys().into_iter().filter(|o| o != k) {
            self.send(&other, ServerMsg::OpponentAim { aim });
        }
        for w in self.spectators(id) {
            self.send(&w, ServerMsg::OpponentAim { aim });
        }
    }

    fn shoot(
        &mut self,
        k: &str,
        turn: u32,
        shot: Shot,
        called: Option<PocketId>,
        cue_ball: Option<DVec2>,
    ) {
        let reject = |l: &Self, reason: &str| {
            if let Some(room) = l.room_of(k) {
                l.send(
                    k,
                    ServerMsg::ShotRejected {
                        turn,
                        reason: reason.into(),
                        game: room.game.clone(),
                    },
                );
            }
        };
        let Some((id, seat)) = self.my_turn(k) else {
            return reject(self, "It's not your turn.");
        };
        if self.rooms[&id].game.shots_taken() != turn {
            return reject(self, "That shot was for an earlier turn.");
        }
        let finite = shot.yaw.is_finite()
            && shot.elevation.is_finite()
            && shot.tip.is_finite()
            && shot.speed.is_finite()
            && cue_ball.is_none_or(|p| p.is_finite());
        if !finite {
            return reject(self, "Invalid shot.");
        }
        // A game older than the 8's call plays the 8 in any pocket.
        let eight_anywhere = self.sessions.get(k).is_some_and(|s| s.protocol < EIGHT_CALL_PROTOCOL);
        if let Err(e) = self.start_shot(id, seat, shot, called, cue_ball, false, eight_anywhere) {
            reject(self, &e);
        }
    }

    /// Accepts a shot: tells both players straight away and has it simulated.
    #[allow(clippy::too_many_arguments)]
    fn start_shot(
        &mut self,
        id: u64,
        seat: usize,
        shot: Shot,
        called: Option<PocketId>,
        cue_ball: Option<DVec2>,
        timeout: bool,
        eight_anywhere: bool,
    ) -> Result<(), String> {
        let room = self.rooms.get_mut(&id).ok_or("No such game.")?;
        // A shot at the 8 names its pocket, under the official rules as
        // under the Brazilian ones (refused before anything moves); but for
        // a game that can't (see `eight_anywhere`).
        if !eight_anywhere {
            room.game.check_eight_call(called).map_err(|e| format!("Can't shoot: {e}."))?;
        }
        if let Some(p) = cue_ball
            && room.game.ball_in_hand().is_some()
            && room.game.ball(BallId::CUE).xy() != p
            && !room.game.place_cue_ball(p)
        {
            return Err("The cue ball can't be placed there.".into());
        }
        // Validate before telling anyone (cheap: the simulation isn't run).
        room.game.prepare_shot(&shot, called).map_err(|e| format!("Can't shoot: {e}."))?;
        let turn = room.game.shots_taken();
        let cue_ball = room.game.ball_in_hand().map(|_| room.game.ball(BallId::CUE).xy());
        // How long it was thought over: since the shot clock started.
        let clock = self.settings.shot_clock;
        let left = room.deadline.saturating_duration_since(Instant::now());
        let think = clock.saturating_sub(left).as_millis().div_ceil(100) as u32;
        room.think.push((room.replay.strokes.len(), think));
        room.busy = Some(turn);
        room.shot_at = Instant::now();
        room.idle[seat % 2] = 0;
        room.replay.strokes.push(Stroke::Shot { shot, called, cue_ball, timeout });
        let msg =
            ServerMsg::ShotTaken { seat, turn, shot, called_pocket: called, cue_ball, timeout };
        let game = Box::new(room.game.clone());
        self.send_room(id, &msg);
        self.effect(Effect::Simulate { room: id, turn, game, shot, called, eight_anywhere });
        Ok(())
    }

    /// The outcome of a simulated shot (from the effect worker).
    pub fn commit_shot(
        &mut self,
        id: u64,
        turn: u32,
        outcome: Result<(EightBallGame, ShotReport, f64), String>,
    ) {
        let clock = self.settings.shot_clock;
        let Some(room) = self.rooms.get_mut(&id) else { return };
        if room.busy != Some(turn) || room.finished {
            return;
        }
        room.busy = None;
        let (game, report, duration) = match outcome {
            Ok(o) => o,
            Err(e) => {
                tracing::error!(room = id, "shot simulation failed: {e}");
                room.replay.strokes.pop();
                let strokes = room.replay.strokes.len();
                room.think.retain(|(stroke, _)| *stroke < strokes);
                let msg = ServerMsg::ShotRejected {
                    turn,
                    reason: "The server could not play that shot.".into(),
                    game: room.game.clone(),
                };
                room.deadline = Instant::now() + clock;
                self.send_room(id, &msg);
                self.resign_waiting(id);
                return;
            }
        };
        let shooter = room.game.current_player();
        // The numbers for the end-of-game panel, the profile and the review
        // of aim assistance (measured on the table as it was).
        let in_hand = room.game.ball_in_hand().is_some();
        room.stats[shooter].add(&report, in_hand);
        room.last_was_break = report.was_break;
        if let Some(Stroke::Shot { shot, .. }) = room.replay.strokes.last() {
            room.stats[shooter].add_spin(shot, &report);
            if let Some(band) = shot_difficulty(&room.game, shot, &report) {
                room.bands[shooter].push(band);
            }
        }
        // The call the shot was played with, as recorded: the 8's pocket,
        // for a game older than the 8's call (see `eight_anywhere`), so the
        // game replays the same. (Under the official rules a call the
        // shot didn't need was dropped: the stroke keeps it, to no effect.)
        if room.game.calls_the_eight()
            && let Some(Stroke::Shot { called, .. }) = room.replay.strokes.last_mut()
        {
            *called = report.called;
        }
        room.game = game;
        let next = room.game.current_player();
        if next != shooter {
            room.turns[next] += 1;
        }
        room.deadline = room.shot_at + Duration::from_secs_f64(duration) + SHOT_MARGIN + clock;
        let clock_left = room.deadline.saturating_duration_since(Instant::now()).as_secs_f32();
        let msg = ServerMsg::ShotResult { turn, end: room.game.clone(), report, clock_left };
        let over = room.game.game_over().is_some();
        self.send_room(id, &msg);
        self.doubles_changed(id);
        if over {
            self.finish(id);
        } else if !self.resign_waiting(id) {
            self.save_live(id);
        }
    }

    /// A resignation that waited for the shot being played is made now;
    /// false if there was none.
    fn resign_waiting(&mut self, id: u64) -> bool {
        let Some(room) = self.rooms.get_mut(&id) else { return false };
        let Some(seat) = room.resigning.take() else { return false };
        if room.finished {
            return false;
        }
        room.game.resign(seat);
        room.replay.strokes.push(Stroke::Resign { seat });
        self.finish(id);
        true
    }

    /// The shot clock ran out: a foul under both rules (see
    /// [`EightBallGame::time_foul`]). A choice after the break that isn't
    /// made in time takes its first option.
    fn timeout_shot(&mut self, id: u64) {
        let Some(room) = self.rooms.get_mut(&id) else { return };
        let seat = room.game.current_player();
        // A player who lets the clock run out again and again isn't playing
        // (a break or an open table owes nothing, and the official rules
        // never end on the clock): they left the game, and it (a
        // tournament's too) doesn't wait for them forever.
        room.idle[seat % 2] = room.idle[seat % 2].saturating_add(1);
        if room.idle[seat % 2] >= IDLE_TURNS {
            tracing::info!(room = id, seat, "the shot clock ran out {IDLE_TURNS} times in a row");
            room.game.abandon(seat);
            room.replay.strokes.push(Stroke::Abandon { seat });
            self.finish(id);
            return;
        }
        if let Some(choice) = room.game.pending_choice() {
            let turn = room.game.shots_taken();
            return self.make_choice(id, seat, turn, choice.options()[0]);
        }
        self.time_foul(id, seat);
    }

    /// `seat` ran out of time (see [`EightBallGame::time_foul`]): under
    /// the Brazilian rules the ball it gives can be the 8, which ends the
    /// game.
    fn time_foul(&mut self, id: u64, seat: usize) {
        let clock = self.settings.shot_clock;
        let Some(room) = self.rooms.get_mut(&id) else { return };
        room.game.time_foul();
        room.replay.strokes.push(Stroke::TimeFoul);
        room.stats[seat % 2].end_visit();
        let over = room.game.game_over().is_some();
        if !over {
            room.turns[room.game.current_player()] += 1;
        }
        room.deadline = Instant::now() + clock;
        let msg =
            ServerMsg::TimeFoul { seat, game: room.game.clone(), clock_left: clock.as_secs_f32() };
        self.send_room(id, &msg);
        self.doubles_changed(id);
        if over {
            self.finish(id);
        } else {
            self.save_live(id);
        }
    }

    /// `k` answers the choice the break left them (the official rules).
    fn choose(&mut self, k: &str, turn: u32, option: BreakOption) {
        let Some((id, seat)) = self.my_turn(k) else { return };
        let game = &self.rooms[&id].game;
        if game.shots_taken() != turn {
            return;
        }
        match game.pending_choice() {
            Some(choice) if choice.options().contains(&option) => {
                if let Some(room) = self.rooms.get_mut(&id) {
                    room.idle[seat % 2] = 0;
                }
                self.make_choice(id, seat, turn, option)
            }
            _ => self.error(k, "That choice can't be made now."),
        }
    }

    /// Plays `seat`'s answer to the choice the break left (a valid one).
    fn make_choice(&mut self, id: u64, seat: usize, turn: u32, option: BreakOption) {
        let clock = self.settings.shot_clock;
        let Some(room) = self.rooms.get_mut(&id) else { return };
        if room.game.choose(option).is_err() {
            return;
        }
        room.replay.strokes.push(Stroke::Choice { option });
        let next = room.game.current_player();
        if next != seat {
            room.turns[next] += 1;
        }
        room.deadline = Instant::now() + clock;
        let msg = ServerMsg::Chose {
            seat,
            turn,
            option,
            game: room.game.clone(),
            clock_left: clock.as_secs_f32(),
        };
        self.send_room(id, &msg);
        self.doubles_changed(id);
        self.save_live(id);
    }

    /// `k` now plays `rules` (their choice in the game): everyone sees it.
    fn set_rules(&mut self, k: &str, rules: Rules) {
        let Some(s) = self.sessions.get_mut(k) else { return };
        if s.rules != rules {
            s.rules = rules;
            self.announce(k);
        }
    }

    fn resign(&mut self, k: &str) {
        let Some(id) = self.sessions.get(k).and_then(|s| s.room) else { return };
        self.resign_in(id, k);
    }

    /// `k` resigns the game in room `id` (if they play in it).
    fn resign_in(&mut self, id: u64, k: &str) {
        let Some(room) = self.rooms.get_mut(&id) else { return };
        if room.finished || !room.has(k) {
            return;
        }
        let seat = room.seat_of(k);
        // The shot being played is played first: the replay holds what the
        // table saw, and the shooter keeps what it brings.
        if room.busy.is_some() {
            room.resigning = Some(seat);
            return;
        }
        room.game.resign(seat);
        room.replay.strokes.push(Stroke::Resign { seat });
        self.finish(id);
    }

    /// Settles ratings and notifies both players.
    fn finish(&mut self, id: u64) {
        let Some(room) = self.rooms.get(&id) else { return };
        let Some(over) = room.game.game_over() else { return };
        if room.finished {
            return;
        }
        if room.doubles.is_some() {
            return self.finish_doubles(id);
        }
        // The freshest numbers: from the session if the player is online.
        let current = |seat: usize| -> User {
            self.sessions
                .get(&room.keys[seat])
                .map(|s| s.user.clone())
                .unwrap_or_else(|| room.users[seat].clone())
        };
        let users = [current(0), current(1)];
        let rated = room.rated;
        let rules = room.game.rules();
        let rating = |seat: usize| users[seat].records.get(rules).rating;
        let mut delta = [0, 0];
        let mut new_ratings = [rating(0), rating(1)];
        let keys = room.keys.clone();
        // The same two players again and again, rated or casual: less and
        // less at stake, and past a few games nothing earned.
        let repeats = self.count_pair_game(&keys, rated);
        if rated {
            let (w, l) = (over.winner, 1 - over.winner);
            let change = rating::apply_result(
                &EloConfig::default(),
                users[w].rating(rules),
                users[l].rating(rules),
            );
            let (rw, rl): (Rating, Rating) = (change.winner, change.loser);
            delta[w] = change.winner_delta;
            delta[l] = change.loser_delta;
            new_ratings[w] = rw.rating;
            new_ratings[l] = rl.rating;
            // The same two players again and again: less and less at stake.
            let scale = match repeats {
                0..=2 => 1.0,
                3..=4 => 0.5,
                _ => 0.0,
            };
            if scale < 1.0 {
                for seat in 0..2 {
                    delta[seat] = (delta[seat] as f64 * scale).round() as i32;
                    new_ratings[seat] = rating(seat) + delta[seat];
                }
            }
        }
        let loser = 1 - over.winner;
        // Leaving a game in progress costs more than losing it: extra
        // rating (rated games), and a while without online games.
        if over.reason == GameOverReason::Abandoned {
            if rated {
                delta[loser] -= ABANDON_RATING;
                new_ratings[loser] = rating(loser) + delta[loser];
            }
            let key = keys[loser].clone();
            self.record_abandon(&key);
        }
        let room = &self.rooms[&id];
        let eight_win = over.reason == GameOverReason::PocketedEight;
        let farmed = repeats > 4;
        // A game resigned or left after a few shots (as short as a game
        // that earns no coins) was arranged or never played: no arena
        // points, and no win or streak.
        let played =
            !matches!(over.reason, GameOverReason::Resignation | GameOverReason::Abandoned)
                || room.game.shots_taken() >= crate::account::MIN_SHOTS_FOR_COINS;
        let shutout = eight_win
            && room.game.group_of(loser).is_some_and(|g| room.game.remaining(g).len() == 7);
        let record = MatchRecord {
            players: [users[0].id, users[1].id],
            winner: over.winner,
            reason: over.reason,
            rated,
            rules,
            delta,
            ratings_after: new_ratings,
            shots: room.game.shots_taken(),
            tournament: room.tournament.map(|t| t.0 as i64),
            eight_win,
            runout: eight_win && room.turns[loser] == 0 && room.breaker == over.winner,
            shutout,
            // Won with the 8 alone on a break (not later on an open table).
            break_win: eight_win && room.last_was_break,
            ended_at: now(),
            replay: Some(room.replay.clone()),
            stats: room.stats,
            abandoned: (over.reason == GameOverReason::Abandoned).then_some(loser),
            room: Some(id as i64),
            counts: !farmed && played,
        };
        let guests = room.guests;
        let tournament = room.tournament;
        let result = MatchResult {
            winner: over.winner,
            reason: over.reason,
            rated,
            rating_delta: delta,
            new_ratings,
        };
        let room = self.rooms.get_mut(&id).expect("room");
        room.finished = true;
        room.result = Some(result.clone());
        room.ended = Some(Instant::now());
        self.finished_rooms.insert(id, Instant::now());
        self.featured_dirty = true;
        room.rematch = [false, false];
        // Show the new numbers at once; the database catches up.
        for (seat, k) in keys.iter().enumerate() {
            if let Some(s) = self.sessions.get_mut(k) {
                crate::store::apply_match_numbers(&mut s.user, &record, seat);
            }
        }
        for w in self.spectators(id) {
            self.send(&w, ServerMsg::MatchEnded { result: result.clone() });
        }
        for k in &keys {
            self.send(k, ServerMsg::MatchEnded { result: result.clone() });
            self.announce(k);
        }
        if !guests[0] && !guests[1] {
            let room = &self.rooms[&id];
            for seat in 0..2 {
                let mut bands: BTreeMap<usize, (u32, u32)> = BTreeMap::new();
                for &(band, made) in &room.bands[seat] {
                    let e = bands.entry(band).or_default();
                    e.0 += 1;
                    e.1 += made as u32;
                }
                if !bands.is_empty() {
                    self.effect(Effect::ShotStats {
                        user_id: room.users[seat].id,
                        bands: bands.into_iter().map(|(b, (a, m))| (b, a, m)).collect(),
                    });
                }
            }
            // Recorded with its saved copy deleted in the same step.
            for id in record.players {
                *self.records_pending.entry(id).or_default() += 1;
            }
            self.effect(Effect::RecordMatch { record, keys: keys.clone(), farmed });
            let room = &self.rooms[&id];
            let times = moderation::shot_times(&room.think, room.replay.strokes.len());
            self.effect(Effect::ShotTimes { room: id as i64, times });
        }
        tracing::info!(room = id, "match finished: {:?}", over);
        if let Some((tid, round, index)) = tournament {
            self.tournament_game_over(tid, round, index, &keys, over.winner, played);
        }
        // Who left as they resigned (during a shot) is back in the lounge.
        let leaving = self.rooms.get(&id).map_or([false, false], |r| r.leaving);
        for (seat, k) in keys.iter().enumerate() {
            if leaving[seat] && self.sessions.get(k).is_some_and(|s| s.room == Some(id)) {
                self.leave_finished(k);
                if let Some(tid) = self.in_tournament(k) {
                    self.tournament_progress(tid);
                }
            }
        }
    }

    fn rematch(&mut self, k: &str) {
        let Some(id) = self.sessions.get(k).and_then(|s| s.room) else { return };
        let Some(room) = self.rooms.get_mut(&id) else { return };
        if !room.finished {
            return;
        }
        if room.tournament.is_some() {
            return self.error(k, "Tournament games have no rematch.");
        }
        if room.doubles.is_some() {
            return self.doubles_rematch(k, id);
        }
        let seat = room.seat_of(k);
        let opponent = room.keys[1 - seat].clone();
        // Gone, or on to another game (or back in the lounge).
        let there = self.sessions.get(&opponent).is_some_and(|s| s.room == Some(id));
        if !room.present[1 - seat] || !there {
            return self.send(k, ServerMsg::OpponentLeft);
        }
        // What every other way of starting a game asks too, before the
        // room is touched.
        if self.refuse_if_draining(k) || self.refuse_if_cooling_down(k) {
            return;
        }
        if self.abandon_cooldown(&opponent).is_some() {
            return self.error(k, "Your opponent can't start another game yet.");
        }
        if self.tournament_busy(k) || self.tournament_busy(&opponent) {
            return self.error(k, "Players in a tournament can't start other games.");
        }
        let Some(room) = self.rooms.get_mut(&id) else { return };
        let offered = room.rematch[seat];
        room.rematch[seat] = true;
        if room.rematch[1 - seat] {
            let keys = room.keys.clone();
            let rated = room.rated;
            // The same rules, a new rack, the other player breaks.
            let game = Game {
                seed: rand::thread_rng().r#gen(),
                breaker: 1 - room.breaker,
                rules: room.game.rules(),
            };
            let guests = room.guests;
            let chat = room.chat.clone();
            let table_look = room.table_look.clone();
            let private = room.private;
            self.rooms.remove(&id);
            self.featured_dirty = true;
            for p in &keys {
                if let Some(s) = self.sessions.get_mut(p) {
                    s.room = None;
                }
            }
            let users =
                [self.sessions[&keys[0]].user.clone(), self.sessions[&keys[1]].user.clone()];
            let new_id = self.new_id();
            self.create_room(
                new_id, keys, users, guests, rated, game, None, table_look, private, None,
            );
            if let Some(r) = self.rooms.get_mut(&new_id) {
                r.chat = chat;
            }
        } else if !offered {
            // Offered once (asking again changes nothing).
            self.send(&opponent, ServerMsg::RematchOffered);
        }
    }

    fn leave_match(&mut self, k: &str) {
        let Some(id) = self.sessions.get(k).and_then(|s| s.room) else { return };
        let Some(room) = self.rooms.get_mut(&id) else { return };
        if !room.finished {
            // Resigned as the shot being played rolls: the resignation is
            // made after it, and the leaving with it.
            let seat = room.seat_of(k);
            if room.resigning == Some(seat) {
                room.leaving[seat] = true;
                return;
            }
            return self.error(k, "Resign first to leave a game in progress.");
        }
        self.leave_finished(k);
        if let Some(tid) = self.in_tournament(k) {
            self.tournament_progress(tid);
        }
    }

    /// Takes `k` away from the finished game they are still at (the
    /// opponent and whoever watches them are told).
    fn leave_finished(&mut self, k: &str) {
        let Some(id) = self.sessions.get(k).and_then(|s| s.room) else { return };
        let Some(room) = self.rooms.get_mut(&id).filter(|r| r.finished) else { return };
        if room.doubles.is_some() {
            if let Some(s) = self.sessions.get_mut(k) {
                s.room = None;
            }
            self.doubles_leave_finished(k, id);
            self.end_watching_of(k, id);
            self.close_room_if_empty(id);
            self.announce(k);
            return;
        }
        let seat = room.seat_of(k);
        room.present[seat] = false;
        let opponent = room.keys[1 - seat].clone();
        if let Some(s) = self.sessions.get_mut(k) {
            s.room = None;
        }
        if self.sessions.get(&opponent).is_some_and(|s| s.room == Some(id)) {
            self.send(&opponent, ServerMsg::OpponentLeft);
        }
        self.end_watching_of(k, id);
        self.close_room_if_empty(id);
        self.announce(k);
    }

    /// Called periodically: shot clocks, abandoned games, tournaments.
    pub fn tick(&mut self) {
        self.tick_inner();
        // What the tick ended (a clock, a player who didn't come back).
        self.flush_featured();
    }

    fn tick_inner(&mut self) {
        if !self.queue.is_empty() {
            self.match_queue();
        }
        self.match_doubles();
        self.expire_seeks();
        let nowi = Instant::now();
        // A saved game is gone from the database within a day at most.
        self.finished_rooms.retain(|_, at| nowi.duration_since(*at) < Duration::from_secs(86_400));
        self.last_break.retain(|_, (_, at)| nowi.duration_since(*at) < PAIR_WINDOW);
        let grace = self.settings.reconnect_grace;
        let mut finished = Vec::new();
        let mut timeouts = Vec::new();
        let mut dropped = Vec::new();
        for room in self.rooms.values_mut().filter(|r| !r.finished) {
            // A shot being played settles first (the next tick sees the rest).
            if room.grace_until.is_some_and(|t| nowi < t) || room.busy.is_some() {
                continue;
            }
            let gone = |s: usize| {
                room.disconnected[s].is_some_and(|t| nowi - t > grace)
                    || (room.grace_until.is_some() && room.disconnected[s].is_some())
            };
            if gone(0) && gone(1) {
                // Nobody came back: no result.
                dropped.push(room.id);
                continue;
            }
            if let Some(seat) = (0..2).find(|s| gone(*s)) {
                room.game.abandon(seat);
                room.replay.strokes.push(Stroke::Abandon { seat });
                finished.push((room.id, seat));
                continue;
            }
            // (The shot clock of a player whose connection dropped waits for
            // them: the time to come back is the reconnection grace.)
            let shooter_away = room.disconnected[room.game.current_player() % 2].is_some();
            if room.busy.is_none() && nowi > room.deadline && !shooter_away {
                timeouts.push(room.id);
            }
        }
        for id in dropped {
            if let Some(room) = self.rooms.remove(&id) {
                self.featured_dirty = true;
                tracing::info!(room = id, "game dropped: both players gone");
                // (Its saved copy, not deleted yet, isn't rebuilt.)
                self.finished_rooms.insert(id, Instant::now());
                for k in &room.all_keys() {
                    if let Some(s) = self.sessions.get_mut(k)
                        && s.room == Some(id)
                    {
                        s.room = None;
                    }
                }
                self.effect(Effect::DeleteLive(id as i64));
                for k in &room.all_keys() {
                    self.end_watching_of(k, id);
                }
                // A knockout can't wait for a game that is gone: its match
                // is settled like one its players didn't come to.
                if let Some((tid, round, index)) = room.tournament {
                    self.tournament_room_gone(tid, round, index, id);
                }
            }
        }
        for id in timeouts {
            self.timeout_shot(id);
        }
        // The server is going away: hand the remaining games over (not one
        // just ended, whose result is recorded below), and at once a game a
        // player left while a shot was being played (they reconnect to the
        // next server: it has to be there).
        if let Some(deadline) = self.draining {
            let running: Vec<u64> = self
                .rooms
                .values()
                .filter(|r| !r.finished && r.busy.is_none() && r.game.game_over().is_none())
                // (A doubles game isn't handed over: it is played to its end.)
                .filter(|r| r.doubles.is_none())
                .filter(|r| {
                    nowi >= deadline
                        || (r.disconnected.iter().any(Option::is_some)
                            && !r.guests.iter().any(|g| *g))
                })
                .map(|r| r.id)
                .collect();
            for id in running {
                self.hand_off(id);
            }
        }
        if self.featured_dirty
            || self.last_featured.is_none_or(|t| nowi.duration_since(t) > Duration::from_secs(4))
        {
            self.featured_dirty = false;
            self.last_featured = Some(nowi);
            self.update_featured();
        }
        self.codes.retain(|_, (_, at)| at.elapsed() < CODE_LIFETIME);
        self.password_misses.retain(|_, (_, since)| since.elapsed() < PASSWORD_LOCKOUT);
        self.left_limits.retain(|_, (_, at)| at.elapsed() < LIMITS_KEPT);
        for (id, seat) in finished {
            self.finish(id);
            // The player who left is gone: close their side of the room (not
            // the other's, who may be back in a moment for the result).
            if let Some(room) = self.rooms.get_mut(&id) {
                room.present[seat] = false;
                let mut gone = vec![room.keys[seat].clone()];
                if let Some(d) = room.doubles.as_mut() {
                    d.present[seat] = [false, false];
                    gone.push(d.partners[seat].clone());
                }
                for k in gone {
                    self.end_watching_of(&k, id);
                }
            }
            self.close_room_if_empty(id);
        }
        // Finished rooms nobody is looking at any more.
        let stale: Vec<u64> = self
            .rooms
            .values()
            .filter(|r| {
                r.finished
                    && r.players_present().iter().all(|(k, at)| {
                        !at || self.sessions.get(k).is_none_or(|x| x.room != Some(r.id))
                    })
            })
            .map(|r| r.id)
            .collect();
        for id in stale {
            self.rooms.remove(&id);
            self.featured_dirty = true;
        }
        self.tournaments_tick(nowi);
    }
}

/// Restarts, the featured table, invitation codes, game limits.
impl Lounge {
    /// Refuses a new game while the server is restarting (with a message).
    fn refuse_if_draining(&self, k: &str) -> bool {
        if self.draining.is_some() {
            self.error(k, "The server is restarting: new games start in a moment.");
        }
        self.draining.is_some()
    }

    /// Notes that `k` left a game in progress (for [`Lounge::abandon_cooldown`]).
    fn record_abandon(&mut self, k: &str) {
        let now = Instant::now();
        let list = self.abandoned.entry(k.to_string()).or_default();
        while list.front().is_some_and(|t| now.duration_since(*t) > ABANDON_WINDOW) {
            list.pop_front();
        }
        list.push_back(now);
        if self.abandoned.len() > 1000 {
            self.abandoned
                .retain(|_, l| l.back().is_some_and(|t| now.duration_since(*t) <= ABANDON_WINDOW));
        }
    }

    /// How long `k` must still wait before starting another online game,
    /// after leaving one in progress: longer each time in a day.
    pub fn abandon_cooldown(&self, k: &str) -> Option<Duration> {
        let list = self.abandoned.get(k)?;
        let now = Instant::now();
        let recent: Vec<&Instant> =
            list.iter().filter(|t| now.duration_since(**t) <= ABANDON_WINDOW).collect();
        let last = **recent.last()?;
        let wait = match recent.len() {
            1 => Duration::from_secs(2 * 60),
            2 => Duration::from_secs(10 * 60),
            _ => Duration::from_secs(30 * 60),
        };
        wait.checked_sub(now.duration_since(last)).filter(|d| !d.is_zero())
    }

    /// Refuses a new game to a player still waiting after leaving one (with
    /// a message saying how long).
    fn refuse_if_cooling_down(&self, k: &str) -> bool {
        let Some(left) = self.abandon_cooldown(k) else { return false };
        self.error(k, cooldown_message(left));
        true
    }

    /// Seconds a player who often leaves games waits in the queue before
    /// being paired (none for most players).
    fn abandon_penalty(&self, k: &str) -> f32 {
        let Some(s) = self.sessions.get(k) else { return 0.0 };
        let u = &s.user;
        let played = (u.rated_games() + u.casual_games).max(1);
        if u.abandons < 3 {
            return 0.0;
        }
        let rate = u.abandons as f32 / played as f32;
        // 10% of games left: 30 s; 30% or more: the full 90 s.
        (rate * 300.0).min(90.0)
    }

    /// How many games of a kind (rated or casual) these two played in the
    /// last day.
    pub(crate) fn pair_games_today(&mut self, keys: &[String; 2], rated: bool) -> usize {
        let now = Instant::now();
        let Some(list) = self.pair_games.get_mut(&pair_key(keys, rated)) else { return 0 };
        while list.front().is_some_and(|t| now.duration_since(*t) > PAIR_WINDOW) {
            list.pop_front();
        }
        list.len()
    }

    /// Counts a game of a kind (rated or casual) between these two; returns
    /// how many they played in the last day before this one.
    pub(crate) fn count_pair_game(&mut self, keys: &[String; 2], rated: bool) -> usize {
        let before = self.pair_games_today(keys, rated);
        let now = Instant::now();
        self.pair_games.entry(pair_key(keys, rated)).or_default().push_back(now);
        if self.pair_games.len() > 10_000 {
            self.pair_games
                .retain(|_, l| l.back().is_some_and(|t| now.duration_since(*t) < PAIR_WINDOW));
        }
        before
    }

    /// The server got the signal to stop: no new games; games in progress
    /// go on for up to `secs` and are then handed to the next server.
    pub fn start_drain(&mut self, secs: u64) {
        if self.draining.is_some() {
            return;
        }
        self.draining = Some(Instant::now() + Duration::from_secs(secs));
        let minutes = secs.div_ceil(60).max(1);
        self.announce_text(&format!(
            "The server restarts in {minutes} min for an update. Games in progress go on; new games start right after."
        ));
        self.broadcast(&ServerMsg::Draining { secs: secs as u32 });
        let queued: Vec<String> = self.queue.iter().map(|q| q.key.clone()).collect();
        for k in queued {
            self.queue_leave(&k, true);
        }
        let hosts: Vec<String> = self.tables.values().map(|t| t.host.clone()).collect();
        for h in hosts {
            self.close_table_of(&h);
            self.announce(&h);
        }
        // Invitations end, and those who sent or got them are told (their
        // dialogs would wait for an answer that won't come); the board too.
        let inviters: Vec<String> = self.invites.keys().map(|(from, _)| from.clone()).collect();
        for k in inviters {
            self.drop_invites_of(&k);
        }
        let seekers: Vec<String> = self.sessions.keys().cloned().collect();
        for k in seekers {
            self.drop_seek(&k);
        }
        // Tournaments live in this server's memory alone: whatever is still
        // open or being played is called off, every entry fee given back
        // (the games already on go on, as games of their own).
        let open: Vec<u64> = self
            .tournaments
            .values()
            .filter(|t| t.state != TournamentState::Finished)
            .map(|t| t.id)
            .collect();
        for id in open {
            let players: Vec<String> = self
                .tournaments
                .get(&id)
                .map(|t| t.players.iter().chain(&t.waitlist).cloned().collect())
                .unwrap_or_default();
            for k in players {
                let text = "The server is restarting: your tournament was called off.";
                self.send(&k, ServerMsg::Notice { text: text.into() });
            }
            self.cancel_tournament(id);
        }
    }

    pub fn has_room(&self, id: u64) -> bool {
        self.rooms.contains_key(&id)
    }

    /// The game `id` was finished here (its saved copy is stale).
    pub fn was_finished(&self, id: u64) -> bool {
        self.finished_rooms.contains_key(&id)
    }

    pub fn is_draining(&self) -> bool {
        self.draining.is_some()
    }

    /// Games not finished yet.
    pub fn running_games(&self) -> usize {
        self.rooms.values().filter(|r| !r.finished).count()
    }

    /// The game being played between two players, if any (its room id).
    pub fn live_game_between(&self, a: &str, b: &str) -> Option<u64> {
        let (a, b) = (key(a), key(b));
        self.rooms.values().find(|r| !r.finished && r.has(&a) && r.has(&b)).map(|r| r.id)
    }

    /// A game still in this server (playing or just finished): the players,
    /// whether it is rated, finished, and the strokes so far.
    pub fn room_replay(&self, id: u64) -> Option<([String; 2], bool, bool, Replay)> {
        self.rooms.get(&id).map(|r| {
            (
                [r.users[0].username.clone(), r.users[1].username.clone()],
                r.rated,
                r.finished,
                r.replay.clone(),
            )
        })
    }

    /// The accounts online (not guests), for the hourly activity.
    pub fn online_user_ids(&self) -> Vec<i64> {
        self.sessions.values().filter(|s| !s.guest && s.user.id > 0).map(|s| s.user.id).collect()
    }

    /// Who waits in the "Play now" queue: the game they want and how long
    /// they have waited (seconds).
    pub fn queue_now(&self) -> Vec<(Offer, u32)> {
        self.queue
            .iter()
            .map(|q| (Offer { rated: q.rated, rules: q.rules }, q.since.elapsed().as_secs() as u32))
            .collect()
    }

    /// Hands a game in progress to the next server: it stays saved, both
    /// players are disconnected (their game reconnects to the new server,
    /// which rebuilds the table) and the room is closed here.
    fn hand_off(&mut self, id: u64) {
        let Some(room) = self.rooms.remove(&id) else { return };
        self.featured_dirty = true;
        if room.finished || room.guests.iter().any(|g| *g) {
            self.rooms.insert(id, room);
            self.featured_dirty = true;
            return;
        }
        tracing::info!(room = id, "game handed to the next server");
        let mut kills = Vec::new();
        for k in &room.keys {
            if let Some(set) = self.watchers.remove(k) {
                for w in set {
                    if let Some(s) = self.sessions.get_mut(&w) {
                        s.watching = None;
                    }
                    self.send(
                        &w,
                        ServerMsg::SpectateEnded { reason: "The server is restarting.".into() },
                    );
                }
            }
            if let Some(s) = self.sessions.get_mut(k)
                && s.room == Some(id)
            {
                s.room = None;
                // Closing the connection (once the copy is written): the
                // game reconnects by itself.
                kills.push(s.kill.clone());
            }
        }
        let live = LiveMatch {
            id: id as i64,
            players: [room.users[0].id, room.users[1].id],
            rated: room.rated,
            replay: room.replay.clone(),
            updated_at: now(),
        };
        self.effect(Effect::HandOff { live, kills });
    }

    /// Rebuilds a saved game after a restart (or a hand-over). The players
    /// are away until they reconnect. False if it is already here.
    pub fn restore(&mut self, live: &LiveMatch, users: [User; 2], rebuilt: Rebuilt) -> bool {
        let id = live.id as u64;
        let keys = [key(&users[0].username), key(&users[1].username)];
        // A game finished here meanwhile (while its copy was being rebuilt)
        // isn't played again.
        if self.rooms.contains_key(&id)
            || self.finished_rooms.contains_key(&id)
            || self.rooms.values().any(|r| !r.finished && keys.iter().any(|k| r.has(k)))
        {
            return false;
        }
        let nowi = Instant::now();
        // (The break after this game is the other player's.)
        if live.replay.rules == Rules::Brazilian {
            let (a, b, _) = pair_key(&keys, false);
            self.last_break.insert((a, b), (keys[live.replay.breaker % 2].clone(), nowi));
        }
        let stakes = live.rated.then(|| self.stakes_for(&keys, &users, live.replay.rules));
        let room = Room {
            id,
            keys: keys.clone(),
            users,
            guests: [false, false],
            rated: live.rated,
            game: rebuilt.game,
            deadline: nowi + RESTORE_GRACE + self.settings.shot_clock,
            disconnected: [Some(nowi), Some(nowi)],
            finished: false,
            rematch: [false, false],
            present: [true, true],
            result: None,
            breaker: live.replay.breaker,
            busy: None,
            idle: [0, 0],
            last_was_break: false,
            shot_at: nowi,
            turns: rebuilt.turns,
            tournament: None,
            chat: VecDeque::new(),
            crowd: Bucket::new(4.0, 0.6),
            cheers: HashMap::new(),
            replay: live.replay.clone(),
            stats: rebuilt.stats,
            bands: rebuilt.bands,
            table_look: Default::default(),
            grace_until: Some(nowi + RESTORE_GRACE),
            ended: None,
            // Whether it was started at a table with a password isn't
            // saved: kept off the lounge's TV to be safe.
            private: true,
            resigning: None,
            leaving: [false, false],
            think: Vec::new(),
            stakes,
            spectator_chat: VecDeque::new(),
            chat_total: 0,
            chat_missed: [None, None],
            doubles: None,
        };
        self.rooms.insert(id, room);
        self.featured_dirty = true;
        self.next_id = self.next_id.max(id + 1);
        // A player online here already is back at the table at once (else
        // they would sit in the lounge, told nothing, and lose the game for
        // not coming back).
        for k in &keys {
            if self.sessions.contains_key(k) {
                // Whatever they had going in the lounge ends (a table someone
                // could sit down at, the queue, invitations, a game being
                // watched): they are at this table now.
                self.close_table_of(k);
                self.drop_invites_of(k);
                self.stop_watching(k);
                self.queue.retain(|q| q.key != *k);
                self.drop_seek(k);
                self.leave_finished(k);
                if let Some(s) = self.sessions.get_mut(k) {
                    s.room = Some(id);
                }
                self.back_to_room(k, id);
                self.announce(k);
            }
        }
        tracing::info!(room = id, "game restored: {} vs {}", keys[0], keys[1]);
        true
    }

    /// Picks the game on the lounge's "TV": the highest rated game being
    /// played (tells everyone when it changes).
    fn update_featured(&mut self) {
        let best = self
            .rooms
            .values()
            .filter(|r| {
                !r.finished && r.grace_until.is_none() && !r.private && !r.guests.iter().any(|g| *g)
            })
            .max_by_key(|r| (r.rating_of(0) + r.rating_of(1), r.id));
        let featured = best.map(|r| FeaturedMatch {
            match_id: r.id,
            players: [r.player(0, &self.sessions), r.player(1, &self.sessions)],
            rated: r.rated,
            rules: r.game.rules(),
            spectators: self.spectators(r.id).len() as u32,
        });
        if featured != self.featured {
            self.featured = featured.clone();
            self.broadcast(&ServerMsg::Featured { featured });
        }
    }

    /// `k` watches the game on the lounge's TV (the featured one: not any
    /// game whose number they guess, nor a private one).
    fn watch_match(&mut self, k: &str, id: u64) {
        let Some(room) = self.rooms.get(&id).filter(|r| !r.finished) else {
            return self.error(k, "That game is over.");
        };
        if self.featured.as_ref().is_none_or(|f| f.match_id != id) {
            return self.error(k, "You can't watch this game.");
        }
        if room.has(k) {
            return;
        }
        if self.sessions.get(k).is_none_or(|s| s.room.is_some()) {
            return self.error(k, "Finish your game first.");
        }
        let target = room.keys[0].clone();
        if room.all_keys().iter().any(|p| self.blocks(p, k)) {
            return self.error(k, "You can't watch this game.");
        }
        self.stop_watching(k);
        self.watchers.entry(target.clone()).or_default().insert(k.to_string());
        if let Some(s) = self.sessions.get_mut(k) {
            s.watching = Some(target);
            s.watching_tv = true;
        }
        self.send_spectator_start(k, id, 0);
        self.announce_spectators(id);
        self.tell_friends(k);
    }

    /// Opens a private table whose password is a short code friends can use.
    fn create_code(&mut self, k: &str, offer: Offer) {
        if self.refuse_if_draining(k) {
            return;
        }
        let mut rng = rand::thread_rng();
        let code: String = loop {
            let c: String = (0..6)
                .map(|_| CODE_LETTERS[rng.gen_range(0..CODE_LETTERS.len())] as char)
                .collect();
            if !self.codes.contains_key(&c) {
                break c;
            }
        };
        self.open_table(k, offer, Some(code.clone()));
        let Some(table) = self.sessions.get(k).and_then(|s| s.table) else { return };
        self.codes.insert(code.clone(), (table, Instant::now()));
        self.send(k, ServerMsg::InviteCode { code });
    }

    /// Sits at the table of an invitation code.
    fn join_by_code(&mut self, k: &str, code: &str) {
        if self.refuse_if_cooling_down(k) {
            return;
        }
        let code: String = code
            .trim()
            .trim_start_matches("snook-legend://join/")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_uppercase();
        let Some((table, _)) = self.codes.get(&code).copied() else {
            return self.error(k, "That code is not valid (or the table was closed).");
        };
        self.join_table(k, table, Some(code));
    }
}

/// The key two players' games are counted under (names sorted).
fn pair_key(keys: &[String; 2], rated: bool) -> (String, String, bool) {
    let mut pair = [keys[0].clone(), keys[1].clone()];
    pair.sort();
    let [a, b] = pair;
    (a, b, rated)
}

/// A saved game replayed up to now (done away from the lounge: it simulates
/// every shot).
pub struct Rebuilt {
    pub game: EightBallGame,
    pub stats: [ShotStats; 2],
    pub bands: [Vec<(usize, bool)>; 2],
    pub turns: [u32; 2],
}

impl Rebuilt {
    pub fn from(replay: &Replay) -> Result<Rebuilt, String> {
        let mut game = replay.start();
        let mut stats = [ShotStats::default(); 2];
        let mut bands: [Vec<(usize, bool)>; 2] = Default::default();
        let mut turns = [0, 0];
        turns[replay.breaker % 2] = 1;
        for stroke in &replay.strokes {
            // Measured as the live game does: with the cue ball where it was
            // placed (in hand).
            let mut before = game.clone();
            if let Stroke::Shot { cue_ball, .. } = stroke {
                let _ = snook_legend_proto::replay::place_cue_ball(&mut before, *cue_ball);
            }
            let shooter = game.current_player();
            let in_hand = game.ball_in_hand().is_some();
            let report = snook_legend_proto::replay::apply(&mut game, stroke)?;
            if matches!(stroke, Stroke::TimeFoul) {
                stats[shooter % 2].end_visit();
            }
            if let (Some(report), Stroke::Shot { shot, .. }) = (report, stroke) {
                stats[shooter].add(&report, in_hand);
                stats[shooter].add_spin(shot, &report);
                if let Some(b) = shot_difficulty(&before, shot, &report) {
                    bands[shooter].push(b);
                }
            }
            let next = game.current_player();
            if next != shooter {
                turns[next] += 1;
            }
        }
        if game.game_over().is_some() {
            return Err("the game is over".into());
        }
        Ok(Rebuilt { game, stats, bands, turns })
    }
}

impl Room {
    fn seat_of(&self, k: &str) -> usize {
        match &self.doubles {
            Some(d) if self.keys[0] == k || d.partners[0] == k => 0,
            Some(_) => 1,
            None if self.keys[0] == k => 0,
            None => 1,
        }
    }

    /// The player at `seat`'s rating under the rules of this game.
    fn rating_of(&self, seat: usize) -> i32 {
        self.users[seat].records.get(self.game.rules()).rating
    }

    /// What everyone sees of the player at `seat` (playing the rules of this
    /// game; dressed when online).
    fn player(&self, seat: usize, sessions: &HashMap<String, Session>) -> PlayerInfo {
        let rules = self.game.rules();
        match sessions.get(&self.keys[seat]) {
            Some(x) => {
                dressed(player_info(&x.user, x.guest, PlayerStatus::Playing, rules), &x.look)
            }
            None => player_info(&self.users[seat], self.guests[seat], PlayerStatus::Playing, rules),
        }
    }

    fn started_msg(
        &self,
        seat: usize,
        sessions: &HashMap<String, Session>,
        clock: u32,
        spectating: bool,
    ) -> ServerMsg {
        let player = |s: usize| self.player(s, sessions);
        let left = self.deadline.saturating_duration_since(Instant::now()).as_secs_f32();
        ServerMsg::MatchStarted {
            match_id: self.id,
            seat,
            players: [player(0), player(1)],
            rated: self.rated,
            game: self.game.clone(),
            shot_clock_secs: clock,
            clock_left: left,
            tournament: self.tournament.map(|t| t.0),
            spectating,
            table_look: self.table_look.clone(),
            stakes: self.stakes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shots go to a queue of their own; everything else to the worker's,
    /// counted until it is done.
    #[test]
    fn shots_do_not_wait_behind_the_database() {
        let (tx, mut effects) = tokio::sync::mpsc::unbounded_channel();
        let (shots_tx, mut shots) = tokio::sync::mpsc::unbounded_channel();
        let mut l = Lounge::new(Settings::default(), tx);
        l.simulate_on(shots_tx);
        let game = Box::new(EightBallGame::new(1, 0, Rules::Official));
        let shot = Shot { yaw: 0.0, elevation: 0.0, tip: DVec2::ZERO, speed: 1.0 };
        l.effect(Effect::DeleteLive(1));
        l.effect(Effect::Simulate {
            room: 1,
            turn: 0,
            game,
            shot,
            called: None,
            eight_anywhere: false,
        });
        assert!(matches!(shots.try_recv(), Ok(Effect::Simulate { .. })));
        assert!(matches!(effects.try_recv(), Ok(Effect::DeleteLive(1))));
        assert!(effects.try_recv().is_err());
        assert_eq!(l.pending_effects().load(Ordering::SeqCst), 1);
    }

    /// Games between the same two players are counted rated and casual
    /// apart, whichever way round their names come.
    #[test]
    fn pair_games_are_counted_by_kind() {
        let (tx, _effects) = tokio::sync::mpsc::unbounded_channel();
        let mut l = Lounge::new(Settings::default(), tx);
        let ab = ["ana".to_string(), "bob".to_string()];
        let ba = ["bob".to_string(), "ana".to_string()];
        for n in 0..5 {
            assert_eq!(l.count_pair_game(&ab, false), n);
        }
        assert_eq!(l.count_pair_game(&ba, false), 5, "farmed from the sixth");
        assert_eq!(l.pair_games_today(&ab, true), 0);
        assert_eq!(l.count_pair_game(&ba, true), 0);
        assert_eq!(l.pair_games_today(&ab, true), 1);
    }

    /// The Brazilian break goes by who broke last, whatever else the two
    /// played in between and whichever way round their names come.
    #[test]
    fn the_break_follows_the_last_one() {
        let (tx, _effects) = tokio::sync::mpsc::unbounded_channel();
        let mut l = Lounge::new(Settings::default(), tx);
        let ab = ["ana".to_string(), "bob".to_string()];
        let ba = ["bob".to_string(), "ana".to_string()];
        assert_eq!(l.first_breaker(&ab, Rules::Brazilian), 0, "the first name, the first game");
        assert_eq!(l.first_breaker(&ba, Rules::Brazilian), 1);
        l.last_break.insert(("ana".into(), "bob".into()), ("ana".into(), Instant::now()));
        // A game under the other rules in between changes nothing.
        l.count_pair_game(&ab, true);
        assert_eq!(l.first_breaker(&ab, Rules::Brazilian), 1, "bob's turn");
        assert_eq!(l.first_breaker(&ba, Rules::Brazilian), 0);
        assert_eq!(l.first_breaker(&ab, Rules::Official), 0);
    }
}
