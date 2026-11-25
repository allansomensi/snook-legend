//! The online part of the client: the connection (kept in the background
//! so the menu can show who is signed in), signing in, the lounge (players,
//! open tables, tournaments, friends, chat) and routing match messages to
//! the table. The same code runs LAN games, against a server hosted by one
//! of the players.

use std::collections::VecDeque;

use macroquad::prelude::*;

mod gifts;
mod social;
pub use social::{STATUS_COLOR, sub_line};

use crate::i18n::{t, tw};
use snook_legend_proto::{
    ClientMsg, FriendInfo, LeaderboardEntry, MAX_CHAT_LEN, PerRules, PlayerInfo, PlayerStatus,
    Profile, Rules, ServerMsg, TableInfo, TournamentInfo, TournamentState, valid_password,
    valid_username,
};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use crate::config::Config;
use crate::game::Match;
use crate::icons::Icon;
use crate::net::{Connection, NetEvent};
use crate::ui::{
    ALERT, BORDER, CREAM, FeedLine, GOLD, MUTED, Ui, name_color, rrect, rrect_gradient,
    rrect_lines, shade,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    /// Not connected (no saved session, or the player left).
    Idle,
    /// Waiting for the socket to open.
    Connecting,
    /// The sign-in form.
    SignIn,
    /// Credentials sent, waiting for the answer.
    Authenticating,
    Lounge,
    /// The connection dropped while signed in; trying again.
    Reconnecting {
        attempt: u32,
        wait: f32,
    },
}

/// What the online screens ask of the rest of the game.
pub enum OnlineAction {
    None,
    /// Back to the main menu (the connection stays open).
    Leave,
    Profile(String),
    Ranking,
    Shop,
    /// The shop, on the VIP page.
    ShopVip,
    /// A game against the computer while "Play now" keeps looking.
    PracticeWhileWaiting,
    /// A table to oneself (free practice) while "Play now" keeps looking.
    FreeTableWhileWaiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Players,
    Tables,
    Tournaments,
    Friends,
    /// Correspondence ("daily") games.
    Daily,
}

#[derive(Default)]
struct TournamentForm {
    name: String,
    size: u32,
    rated: bool,
    password: String,
    /// Coins to enter (0: free).
    entry_fee: u32,
    format: snook_legend_proto::TournamentFormat,
    rules: Rules,
}

#[derive(Default)]
pub struct FriendsData {
    pub friends: Vec<FriendInfo>,
    pub incoming: Vec<String>,
    pub outgoing: Vec<String>,
    pub blocked: Vec<String>,
}

/// The player's shop state, as the server last sent it.
#[derive(Debug, Clone, Default)]
pub struct WalletView {
    pub coins: i64,
    pub vip_until: i64,
    pub owned: Vec<String>,
    pub equipped: Vec<(String, String)>,
    /// Online games won with each cue.
    pub cue_wins: Vec<(String, u32)>,
}

impl WalletView {
    pub fn is_vip(&self) -> bool {
        self.vip_until > crate::game::unix_now()
    }

    /// Online games won with the cue `code`.
    pub fn cue_wins(&self, code: &str) -> u32 {
        self.cue_wins.iter().find(|(c, _)| c == code).map_or(0, |(_, n)| *n)
    }
}

/// What the server sells for money (empty when it takes no payments).
#[derive(Debug, Clone, Default)]
pub struct ShopOffers {
    pub payments: bool,
    pub packs: Vec<snook_legend_proto::PackOffer>,
    pub vip_price: String,
    pub vip_coins: u32,
    /// The price of a VIP month given to a friend (empty: not on sale).
    pub vip_gift_price: String,
    /// The price of the welcome pack (empty: not on sale).
    pub welcome_price: String,
    /// Coin packs sold in the Microsoft Store (the Store build shows them).
    pub store_packs: Vec<snook_legend_proto::StorePack>,
    /// The Store ID of the month of VIP sold in the Microsoft Store (empty:
    /// not on sale there).
    pub store_vip: String,
}

pub struct Online {
    conn: Option<Connection>,
    /// The host pressed "back" once (leaving closes the game for everyone).
    leave_armed: Option<f64>,
    /// Hosting a game on the local network: where others join it.
    pub host_address: Option<String>,
    /// This connection has signed in (a new one hasn't yet, even while the
    /// player still counts as signed in, reconnecting).
    authed: bool,
    /// Messages sent while reconnecting: they go out once the session is
    /// back (the server refuses everything before that, and that refusal
    /// used to end the match and forget the saved session).
    held: std::cell::RefCell<Vec<ClientMsg>>,
    pub wallet: Option<WalletView>,
    pub offers: Option<ShopOffers>,
    /// The Microsoft Store build's coin packs (see msstore.rs).
    pub store_shop: crate::msstore::StoreShop,
    /// Set when the items in use changed (the table must be redrawn).
    pub looks_changed: bool,
    /// The cloth and ball set of the current online table (shop codes;
    /// empty: this player's own).
    pub table_look: [String; 2],
    /// This account's achievements (for the reactions they unlock).
    achievements: Vec<String>,
    /// Whose achievements those are: after signing in to another account
    /// they don't count for it until its own come.
    achievements_of: String,
    pub stage: Stage,
    pub server: String,
    /// A LAN game (guest name, nothing saved).
    pub lan: bool,
    username: String,
    /// The account the last session signed in as (a session that comes
    /// back as another one forgets everything of it).
    signed_in_as: String,
    /// The name this account asked to take (its session comes back under
    /// it: the same account, nothing forgotten).
    pub renaming_to: Option<String>,
    password: String,
    country: String,
    register: bool,
    /// Error shown on the sign-in form.
    pub error: Option<String>,
    /// Short messages at the bottom of the screen.
    toasts: VecDeque<(String, f32, bool)>,
    pub me: Option<PlayerInfo>,
    players: Vec<PlayerInfo>,
    chat: Vec<FeedLine>,
    /// The lounge chat's emoji tray, and this player's lines shown before
    /// the server passed them on (see [`FeedLine::pending`]), oldest first.
    emoji: crate::emoji_tray::EmojiTray,
    echoes: Vec<String>,
    chat_input: String,
    chat_scroll: usize,
    /// The world ranking of each of the rules, and whether it came.
    pub leaderboard: PerRules<Vec<LeaderboardEntry>>,
    pub leaderboard_loaded: PerRules<bool>,
    /// The rules the player plays (the game's choice, see
    /// [`Config::rules`]): the server is told when it changes.
    pub rules: Rules,
    /// The rules the server was last told (none: a new session).
    rules_sent: Option<Rules>,
    /// The invitation being set up (the game and the rules), before it goes.
    pub invite_dialog: Option<InviteDialog>,
    /// Invitations waiting for an answer, oldest first.
    incoming: Vec<Invite>,
    /// Where the invitation cards were last drawn: under them nothing else
    /// takes the pointer (see [`Online::invite_cards_cover`]).
    invite_area: Option<Rect>,
    /// An invitation just came: the game rings once.
    pub invite_sound: bool,
    /// The invitation sent and waiting: who to, rated, under which rules.
    outgoing: Option<(String, bool, Rules)>,
    motd: String,
    token: String,
    auto_resume: bool,
    pub rated_games: bool,
    pub tab: Tab,
    list_first: usize,
    tables: Vec<TableInfo>,
    table_private: bool,
    table_password: String,
    joining: Option<(u64, String)>,
    /// An invitation code being typed.
    code_input: String,
    tournaments: Vec<TournamentInfo>,
    selected_tournament: Option<u64>,
    tournament_form: Option<TournamentForm>,
    tournament_password: String,
    pub friends: FriendsData,
    friend_input: String,
    pub profile: Option<Profile>,
    unread_friend_requests: bool,
    /// Searching in "Play now": rated, under which rules, seconds waited
    /// and the rating range when the server last said so, and when that was.
    queue: Option<(bool, Rules, u32, u32, std::time::Instant)>,
    /// Correspondence ("daily") games and challenges.
    pub corr: crate::correspondence::Corr,
    /// Round trip to the server, measured with a ping every few seconds.
    pub rtt: Option<std::time::Duration>,
    ping_sent: Option<std::time::Instant>,
    ping_timer: f32,
    /// A purchase sent to the server and not answered yet (item code).
    pub buying: Option<String>,
    /// The last purchase the server confirmed: (item, given to).
    pub bought: Option<(String, Option<String>)>,
    /// The last thing the server refused while buying.
    pub shop_error: Option<String>,
    /// The account page, as the server last sent it.
    pub account: Option<AccountView>,
    /// The e-mail of a payment, offered as the recovery e-mail.
    pub email_offer: Option<String>,
    /// The account's age group and chat, as the server says (servers of
    /// [`snook_legend_proto::AGE_PROTOCOL`] or later; `None` before).
    pub age: Option<AgeState>,
    /// The dialog that turns the chat with other players on (the age group,
    /// then, under 18, a parent's or guardian's permission) or off.
    pub chat_dialog_open: bool,
    /// Its declaration is ticked.
    guardian_ticked: bool,
    /// What the player wrote with the chat off: it goes once the chat is
    /// on (see [`Online::send_text`]).
    pending_text: Option<ClientMsg>,
    /// Games that can be watched again, and how many are kept (0: all).
    pub replays: Option<(Vec<snook_legend_proto::HistoryEntry>, u32)>,
    /// A replay to watch (opened by the replays screen).
    pub replay: Option<Box<snook_legend_proto::ReplayInfo>>,
    /// The game on the lounge's TV.
    pub featured: Option<snook_legend_proto::FeaturedMatch>,
    /// A code friends can use to sit at this player's table.
    pub invite_code: Option<String>,
    /// The server restarts at this time (no new games until then).
    pub draining: Option<std::time::Instant>,
    /// Something happened that a player in another window should notice.
    pub attention: Option<String>,
    /// The last player who came online (not this one), and when
    /// ([`get_time`]): said for a moment by the count of players online
    /// over a game against the computer.
    arrived: Option<(String, f64)>,
    /// Counts the sessions signed in (a new one: what waits for the server
    /// is sent again).
    pub session_count: u32,
    /// The server's answers about the medals of the tournaments against the
    /// computer: the count, and the tournament kept (see `medals`).
    pub cup_medal_news: Vec<(snook_legend_proto::cup::Medals, Option<u64>)>,
    /// What the server was last told: (playing the computer, away, in
    /// focus mode).
    activity_sent: (bool, bool, bool),
    /// Nobody has touched the game for a while (set every frame).
    pub away: bool,
    /// What the right side of the lounge shows.
    side: Side,
    /// Today's and this week's missions.
    missions: Option<MissionsView>,
    /// A mission was finished since the missions were last looked at.
    mission_done: bool,
    /// The notes on the lounge's board, newest first.
    board: Vec<snook_legend_proto::BoardNote>,
    /// Scheduled tournaments the player was already told are about to start.
    told_starting: Vec<u64>,
    /// The server refused this version of the game: a newer one is out.
    pub outdated: bool,
    /// Asking the server which version it runs, after it refused this one:
    /// if it is older, it's the server that hasn't been updated yet.
    /// The connection open now got through (see the fallback in `update`).
    connected: bool,
    /// The public server's own domain didn't answer: this session uses its
    /// host's address ([`crate::config::LEGACY_SERVER`]).
    fallback: bool,
    version_check: Option<std::sync::mpsc::Receiver<Option<u32>>>,
    /// An administrator clicked "Clear the chat" once (at this time): a
    /// second click clears it.
    clear_armed: Option<f64>,
    /// The new version's installer is on its way: the game should close.
    pub quit_for_update: bool,
    /// The sign-in form has put the cursor in its first empty field (once
    /// each time it is shown, so a click elsewhere is not undone).
    form_focused: bool,
    /// Where toasts must stay above this frame: the lounge sets it (its chat
    /// field is at the bottom), [`Online::draw_toasts`] takes it.
    toast_floor: std::cell::Cell<Option<f32>>,
    /// Recent opponents and the "looking for a game" board.
    social: social::Social,
    /// Presents from friends.
    gifts: gifts::Gifts,
    /// The server's verdict on the ladder's reward (taken by the ladder).
    pub ladder_claim: Option<snook_legend_proto::ladder::ClaimResult>,
    /// The style asked for a ghost opponent, and the answer once it came.
    pub ghost: Option<GhostRequest>,
    /// Today's challenge (asked for by the challenge's tab).
    pub daily: Option<snook_legend_proto::daily::Info>,
    /// An attempt the server started: (day, seed), for the game to set up.
    pub daily_started: Option<(i64, u64)>,
    /// The server's answer to the attempt sent, and its coins.
    pub daily_result: Option<(
        Result<snook_legend_proto::daily::Outcome, snook_legend_proto::daily::Refused>,
        i64,
    )>,
    /// Why the last start was refused.
    pub daily_refused: Option<snook_legend_proto::daily::Refused>,
    /// The account level's experience (once the server said).
    pub xp: Option<i64>,
    /// The drills, their tables and the challenge rating.
    pub drills: Option<snook_legend_proto::drills::Info>,
    /// The server's count of the last drill session sent.
    pub drill_result: Option<(String, Result<u32, snook_legend_proto::drills::Refused>)>,
    /// A drill session the server started: (drill, session).
    pub drill_session: Option<(String, u64)>,
    /// Drill sessions asked for and not answered yet: only the answer to
    /// the last one is used (one left behind with Esc is not the new one).
    pub drill_starts: u32,
    /// The staff's events running and coming (double experience, a cup…).
    pub events: Vec<snook_legend_proto::events::EventInfo>,
    /// The tour's final was asked for (see `ladder_ticket`).
    pub awaiting_ticket: bool,
    /// The tour's final as the server dealt it: (rack, breaker).
    pub ladder_ticket: Option<(u64, usize)>,
    /// The server refused to deal it (asked too often).
    pub ticket_refused: bool,
    /// A daily attempt or a drill session an online game took the table
    /// from: the game still sends what it made (see main.rs).
    pub displaced: Option<Match>,
    /// Warnings from the staff, oldest first: each waits in a dialog until
    /// the player presses OK (see [`Online::draw_staff_warning`]).
    staff_warnings: Vec<String>,
    /// When the first of them came on screen (Enter dismisses it only
    /// after a moment: not a key pressed for something else).
    staff_shown: Option<f64>,
    /// The account password typed into the e-mail offer.
    offer_password: String,
}

/// How many players online play one of the rules (the menu and the
/// lounge show them).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuleCounts {
    /// Everyone online who plays them.
    pub players: usize,
    /// Of them, those in the lounge or in online games.
    pub online: usize,
    /// Of them, those playing the computer.
    pub vs_computer: usize,
    /// Of them, those looking for an opponent in "Play now".
    pub searching: usize,
}

impl RuleCounts {
    fn of(o: &Online, rules: Rules) -> RuleCounts {
        let mut c = RuleCounts::default();
        for p in o.players_of(rules) {
            c.players += 1;
            if p.vs_computer {
                c.vs_computer += 1;
            } else {
                c.online += 1;
            }
            if p.searching {
                c.searching += 1;
            }
        }
        c
    }
}

/// An invitation being set up: who to, and the game offered.
#[derive(Debug, Clone, PartialEq)]
pub struct InviteDialog {
    pub to: String,
    pub rated: bool,
    pub rules: Rules,
    /// A daily (correspondence) game rather than one played now.
    pub daily: bool,
}

/// A request for the style of a player's ghost.
#[derive(Debug, Clone)]
pub struct GhostRequest {
    pub username: String,
    pub sent: std::time::Instant,
    pub answer: Option<snook_legend_proto::ghost::GhostAnswer>,
}

/// How long to wait for a ghost's style before saying the server is not
/// answering (measuring a player takes a moment).
pub const GHOST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// The update button, for how this copy of the game is updated (and how
/// far the download is).
/// An invitation received.
#[derive(Debug, Clone)]
struct Invite {
    from: String,
    rated: bool,
    rules: Rules,
    /// When it came (`get_time`): its card slides in.
    at: f64,
}

/// Where invitations are answered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InviteSpot {
    /// Not at a table (the lounge, the menu, the shop…): accepted at once.
    Lounge,
    /// At an online table: declined only (the game must end first).
    OnlineMatch,
    /// Over a game against the computer (`saved`: it can be continued later).
    VsComputer { saved: bool },
}

pub fn update_label() -> String {
    crate::update::button_label()
}

/// A note's colour: gold notices, green events, blue rules.
pub fn note_color(kind: snook_legend_proto::NoteKind) -> Color {
    match kind {
        snook_legend_proto::NoteKind::Notice => GOLD,
        snook_legend_proto::NoteKind::Event => Color::new(0.38, 0.8, 0.5, 1.0),
        snook_legend_proto::NoteKind::Rule => Color::new(0.48, 0.72, 1.0, 1.0),
    }
}

fn note_button(a: snook_legend_proto::NoteAction) -> &'static str {
    use snook_legend_proto::NoteAction;
    match a {
        NoteAction::Tournaments => t("See the tournaments"),
        NoteAction::Shop => t("Open the shop"),
        NoteAction::Vip => t("See VIP"),
        NoteAction::Missions => t("See the missions"),
    }
}

/// How tall a note is drawn `w` wide.
pub fn note_height(ui: &Ui, n: &snook_legend_proto::BoardNote, w: f32) -> f32 {
    let lines = ui.wrap(&n.text, w - 44.0, 14.0).len() as f32;
    lines * 19.0 + 38.0 + if n.action.is_some() { 36.0 } else { 0.0 }
}

/// Draws a note in `card`; returns its button's action when clicked.
pub fn draw_note(
    ui: &mut Ui,
    n: &snook_legend_proto::BoardNote,
    card: Rect,
) -> Option<snook_legend_proto::NoteAction> {
    let color = note_color(n.kind);
    let tint = Color::new(color.r, color.g, color.b, if n.pinned { 0.13 } else { 0.07 });
    rrect(card, 8.0, tint);
    rrect_lines(card, 8.0, if n.pinned { 1.6 } else { 1.0 }, if n.pinned { color } else { BORDER });
    // A coloured edge on the left: the kind of note.
    rrect(Rect::new(card.x, card.y + 6.0, 4.0, card.h - 12.0), 2.0, color);
    if n.pinned {
        crate::icons::draw_filled(
            Icon::Star,
            vec2(card.x + card.w - 16.0, card.y + 14.0),
            14.0,
            color,
        );
    }
    let lines = ui.wrap(&n.text, card.w - 44.0, 14.0);
    for (i, l) in lines.iter().enumerate() {
        ui.text(l, card.x + 14.0, card.y + 10.0 + i as f32 * 19.0, 14.0, CREAM, false);
    }
    let by = if n.author.is_empty() {
        crate::profile::date(n.at)
    } else {
        format!("{} · {}", n.author, crate::profile::date(n.at))
    };
    let by_y = card.y + 14.0 + lines.len() as f32 * 19.0;
    ui.text(&by, card.x + 14.0, by_y, 12.0, MUTED, false);
    let a = n.action?;
    let label = note_button(a);
    let bw = ui.text_size(label, 14.0, true).x + 36.0;
    let b = Rect::new(card.x + 14.0, by_y + 22.0, bw.min(card.w - 28.0), 28.0);
    ui.button(b, label, true).then_some(a)
}

/// Under the format of a new arena tournament.
const ARENA_HINT: &str =
    "30 minutes: play as many games as you can. A win is 2 points, 4 from the third win in a row.";
/// Under a new tournament's entry fee.
const FEE_HINT: &str = "The fees make the prize: 70% to the champion, 20% to the runner-up.";

/// After this long without an opponent, "Play now" offers a game against
/// the computer while it keeps looking.
const PRACTICE_AFTER_SECS: u32 = 30;

/// How the server starts a warning from the staff.
const STAFF_WARNING: &str = "Warning from the staff: ";

/// The server's answer to a final of the tour asked for too often.
const TICKET_REFUSED: &str = "Slow down a little.";

/// The password field of the e-mail offer (its keyboard mark: see main.rs).
pub const EMAIL_OFFER_FIELD: u64 = 77;
/// The lounge chat's scroll bar.
const LOUNGE_LOG: u64 = 9_203;

/// "5 h", "3 days", "12 min": how long until something.
/// A notice of something earned, with the event's multiplier when one ran.
fn with_boost(text: String, boost: u32) -> String {
    if boost > 1 { format!("{text} (×{boost})") } else { text }
}

/// What an event is, for players: "Experience ×2", "Tournament"…
pub fn event_label(e: &snook_legend_proto::events::EventInfo) -> String {
    use snook_legend_proto::events::EventKind;
    match e.kind {
        EventKind::Xp => tw("Experience ×{n}", &[("n", &e.multiplier)]),
        EventKind::Coins => tw("Coins from games ×{n}", &[("n", &e.multiplier)]),
        EventKind::Tournament => t("Themed tournament").to_string(),
        EventKind::Unknown => t("Event").to_string(),
    }
}

/// When an event ends (running) or starts.
pub fn event_when(e: &snook_legend_proto::events::EventInfo, now: i64) -> String {
    if e.is_on(now) {
        tw("ends in {t}", &[("t", &time_left(e.ends_at - now))])
    } else {
        tw("starts in {t}", &[("t", &time_left(e.starts_at - now))])
    }
}

/// An event's card: what it is and when on the first line, the staff's
/// title under it. Its height for a card `w` wide.
pub fn event_height(ui: &Ui, e: &snook_legend_proto::events::EventInfo, w: f32) -> f32 {
    let lines = ui.wrap(&e.title, w - 24.0, 13.0).len().min(2) as f32;
    12.0 + 18.0 + lines * 17.0 + 8.0
}

pub fn draw_event(ui: &Ui, e: &snook_legend_proto::events::EventInfo, r: Rect, now: i64) {
    let live = e.is_on(now);
    rrect(r, 8.0, Color::new(0.91, 0.75, 0.38, if live { 0.16 } else { 0.08 }));
    rrect_lines(r, 8.0, 1.0, Color::new(0.91, 0.75, 0.38, if live { 0.7 } else { 0.35 }));
    let head = format!("{} · {}", event_label(e), event_when(e, now));
    let (head, size) = fit_text(ui, &head, 13.0, r.w - 24.0, true);
    ui.text(&head, r.x + 12.0, r.y + 10.0, size, GOLD, true);
    for (k, l) in ui.wrap(&e.title, r.w - 24.0, 13.0).iter().take(2).enumerate() {
        ui.text(l, r.x + 12.0, r.y + 30.0 + k as f32 * 17.0, 13.0, CREAM, false);
    }
}

pub(crate) fn time_left(secs: i64) -> String {
    let secs = secs.max(0);
    if secs >= 2 * 86_400 {
        tw("{n} days", &[("n", &(secs / 86_400))])
    } else if secs >= 3600 {
        tw("{n} h", &[("n", &(secs / 3600))])
    } else {
        tw("{n} min", &[("n", &(secs / 60).max(1))])
    }
}

/// The right side of the lounge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Chat,
    Missions,
    Board,
}

struct MissionsView {
    daily: Vec<snook_legend_proto::missions::MissionState>,
    weekly: Vec<snook_legend_proto::missions::MissionState>,
    day_ends: i64,
    week_ends: i64,
}

/// The account page.
#[derive(Debug, Clone, Default)]
pub struct AccountView {
    pub recovery_email: Option<String>,
    pub payment_email: Option<String>,
    /// Username changes left (see `snook_legend_proto::MAX_NAME_CHANGES`).
    pub name_changes_left: u32,
}

const MAX_RECONNECTS: u32 = 15;

/// What the server answers a saved session it no longer knows (expired, or
/// signed out everywhere): the only answer that makes the client forget it.
const SESSION_EXPIRED: &str = "Your session expired. Please log in again.";

impl Online {
    /// The global server. Connects (and signs in) at once if a session was saved.
    pub fn new(config: &Config) -> Online {
        let mut o = Online::empty(&config.server, false);
        o.username = config.username.clone();
        o.rules = config.rules();
        // Only the session this server gave (see `Config::session_token`).
        o.token = config.session_token().to_string();
        o.auto_resume = !o.token.is_empty();
        o.country = crate::flags::system_country();
        if o.auto_resume {
            o.connect();
        }
        o
    }

    /// A LAN server, joined as the guest `name`.
    pub fn lan(url: &str, name: &str) -> Online {
        let mut o = Online::empty(url, true);
        o.username = name.to_string();
        o.rated_games = false;
        o.connect();
        o
    }

    fn empty(server: &str, lan: bool) -> Online {
        Online {
            conn: None,
            leave_armed: None,
            host_address: None,
            authed: false,
            held: Default::default(),
            stage: Stage::Idle,
            server: server.to_string(),
            lan,
            username: String::new(),
            signed_in_as: String::new(),
            renaming_to: None,
            password: String::new(),
            country: String::new(),
            register: false,
            error: None,
            toasts: VecDeque::new(),
            me: None,
            players: Vec::new(),
            chat: Vec::new(),
            emoji: crate::emoji_tray::EmojiTray::new(crate::dev::flag("SNOOK_LEGEND_EMOJI_OPEN")),
            echoes: Vec::new(),
            // Development aid: SNOOK_LEGEND_CHAT_DRAFT="text" starts the field with it.
            chat_input: crate::dev::var("SNOOK_LEGEND_CHAT_DRAFT").unwrap_or_default(),
            chat_scroll: 0,
            leaderboard: PerRules::default(),
            leaderboard_loaded: PerRules::default(),
            rules: Rules::default(),
            rules_sent: None,
            invite_dialog: None,
            incoming: Vec::new(),
            invite_area: None,
            invite_sound: false,
            outgoing: None,
            activity_sent: (false, false, false),
            away: false,
            motd: String::new(),
            token: String::new(),
            auto_resume: false,
            rated_games: true,
            tab: Tab::Players,
            list_first: 0,
            tables: Vec::new(),
            table_private: false,
            table_password: String::new(),
            joining: None,
            code_input: String::new(),
            tournaments: Vec::new(),
            selected_tournament: None,
            tournament_form: None,
            tournament_password: String::new(),
            friends: FriendsData::default(),
            friend_input: String::new(),
            profile: None,
            unread_friend_requests: false,
            queue: None,
            corr: Default::default(),
            wallet: None,
            offers: None,
            store_shop: Default::default(),
            looks_changed: false,
            table_look: Default::default(),
            achievements: Vec::new(),
            achievements_of: String::new(),
            rtt: None,
            ping_sent: None,
            ping_timer: 0.0,
            buying: None,
            bought: None,
            shop_error: None,
            account: None,
            // Development aid (screenshots): SNOOK_LEGEND_EMAIL_OFFER=<e-mail> shows the offer.
            email_offer: crate::dev::var("SNOOK_LEGEND_EMAIL_OFFER"),
            age: None,
            chat_dialog_open: false,
            guardian_ticked: false,
            pending_text: None,
            replays: None,
            replay: None,
            featured: None,
            invite_code: None,
            draining: None,
            attention: None,
            arrived: None,
            session_count: 0,
            cup_medal_news: Vec::new(),
            // Development aid: SNOOK_LEGEND_LOUNGE_SIDE=missions|board.
            side: match crate::dev::var("SNOOK_LEGEND_LOUNGE_SIDE").as_deref() {
                Some("missions") => Side::Missions,
                Some("board") => Side::Board,
                _ => Side::Chat,
            },
            missions: None,
            mission_done: false,
            board: Vec::new(),
            told_starting: Vec::new(),
            outdated: false,
            version_check: None,
            connected: false,
            fallback: false,
            clear_armed: None,
            quit_for_update: false,
            form_focused: false,
            toast_floor: std::cell::Cell::new(None),
            social: social::Social {
                // Development aid (screenshots): SNOOK_LEGEND_RECENT=1 opens
                // the Friends tab on the people played recently.
                recent_view: crate::dev::flag("SNOOK_LEGEND_RECENT"),
                ..Default::default()
            },
            gifts: Default::default(),
            ladder_claim: None,
            ghost: None,
            daily: None,
            daily_started: None,
            daily_result: None,
            daily_refused: None,
            xp: None,
            drills: None,
            drill_result: None,
            drill_session: None,
            drill_starts: 0,
            events: Vec::new(),
            awaiting_ticket: false,
            ladder_ticket: None,
            ticket_refused: false,
            displaced: None,
            staff_warnings: Vec::new(),
            staff_shown: None,
            offer_password: String::new(),
        }
    }

    pub fn send(&self, msg: ClientMsg) {
        // Reconnecting: what isn't part of signing in waits for the session
        // (the newest few hundred; pings and aim are of no use later).
        let signing_in = matches!(
            msg,
            ClientMsg::Hello { .. }
                | ClientMsg::Resume { .. }
                | ClientMsg::Guest { .. }
                | ClientMsg::Login { .. }
                | ClientMsg::Register { .. }
        );
        if self.me.is_some() && !self.authed && !signing_in {
            if !matches!(msg, ClientMsg::Ping | ClientMsg::Aim { .. }) {
                let mut held = self.held.borrow_mut();
                if held.len() >= 256 {
                    held.remove(0);
                }
                held.push(msg);
            }
            return;
        }
        if let Some(c) = &self.conn {
            c.send(msg);
        }
    }

    pub fn signed_in(&self) -> bool {
        self.me.is_some()
    }

    pub fn has_session(&self) -> bool {
        !self.token.is_empty()
    }

    /// The account of the saved session (its name, as last signed in).
    pub fn account_name(&self) -> &str {
        if self.signed_in_as.is_empty() { &self.username } else { &self.signed_in_as }
    }

    /// Opens the connection if it isn't open (for the ranking, a profile or the lounge).
    ///
    /// A player who signed in stays so until they sign out: the saved
    /// session is used whenever the connection opens again (after it
    /// dropped, the server restarted, or the account was taken by another
    /// place: then this place takes it back, one place at a time).
    pub fn connect(&mut self) {
        let resume = !self.lan && !self.token.is_empty() && !self.authed;
        if resume {
            self.auto_resume = true;
        }
        if self.conn.is_none() {
            self.authed = false;
            self.connected = false;
            self.conn = Some(Connection::open(&self.server, self.rules));
            if !matches!(self.stage, Stage::Reconnecting { .. }) {
                self.stage = Stage::Connecting;
            }
        } else if resume && matches!(self.stage, Stage::SignIn) {
            // Connected, at the sign-in form, with a session saved: use it.
            self.error = None;
            self.send(ClientMsg::Resume { token: self.token.clone() });
            self.stage = Stage::Authenticating;
        }
    }

    /// Carries out what the Store part of the shop asked for.
    fn store_effects(&mut self, effects: Vec<crate::msstore::Effect>) {
        for effect in effects {
            match effect {
                crate::msstore::Effect::Send(msg) => self.send(msg),
                crate::msstore::Effect::Toast(text, good) => self.toast(text, good),
            }
        }
    }

    pub fn toast(&mut self, text: impl Into<String>, good: bool) {
        self.toasts.push_back((text.into(), 5.0, good));
        while self.toasts.len() > 3 {
            self.toasts.pop_front();
        }
    }

    pub fn request_leaderboard(&mut self, rules: Rules) {
        self.connect();
        self.send(ClientMsg::Leaderboard { rules });
    }

    /// Players online who play `rules` (this one included).
    pub fn players_of(&self, rules: Rules) -> impl Iterator<Item = &PlayerInfo> {
        self.players.iter().filter(move |p| p.rules == rules)
    }

    /// Players online under each of the rules, and of those how many are
    /// in online games or the lounge, and how many play the computer (the
    /// menu's counts); `None` before the lounge's list came.
    pub fn counts(&self) -> Option<PerRules<RuleCounts>> {
        if !self.signed_in() || self.players.is_empty() {
            return None;
        }
        Some(PerRules::new(
            RuleCounts::of(self, Rules::Brazilian),
            RuleCounts::of(self, Rules::Official),
        ))
    }

    /// Whether this account has the achievement `code`.
    pub fn has_achievement(&self, code: &str) -> bool {
        self.me.as_ref().is_some_and(|me| me.username == self.achievements_of)
            && self.achievements.iter().any(|c| c == code)
    }

    /// The account signed in on the online server (not a guest, not on a LAN).
    pub fn account(&self) -> Option<&str> {
        self.me.as_ref().filter(|me| !me.guest && !self.lan).map(|me| me.username.as_str())
    }

    /// Signed in on a working connection (not reconnecting): what is sent
    /// now goes out at once (messages held while reconnecting are lost if
    /// the session doesn't come back).
    pub fn live(&self) -> bool {
        self.signed_in() && self.authed && self.conn.is_some()
    }

    /// Asks the server how `name` plays (for their ghost).
    pub fn request_ghost(&mut self, name: &str) {
        let name = name.trim().to_string();
        self.connect();
        self.send(ClientMsg::GhostStyle { username: name.clone() });
        self.ghost =
            Some(GhostRequest { username: name, sent: std::time::Instant::now(), answer: None });
    }

    pub fn request_profile(&mut self, name: &str) {
        self.connect();
        if self.profile.as_ref().is_none_or(|p| !p.player.username.eq_ignore_ascii_case(name)) {
            self.profile = None;
        }
        self.send(ClientMsg::Profile { username: name.to_string() });
    }

    /// Handles network traffic. Creates, updates or ends `current` when a match
    /// message arrives. Returns true if the config changed and should be saved.
    pub fn update(&mut self, dt: f32, current: &mut Option<Match>, config: &mut Config) -> bool {
        let mut save = false;
        for toast in &mut self.toasts {
            toast.1 -= dt;
        }
        self.toasts.retain(|t| t.1 > 0.0);
        self.tournaments_starting();
        let effects = self.store_shop.update(dt);
        self.store_effects(effects);
        if let Some(rx) = &self.version_check
            && let Ok(protocol) = rx.try_recv()
        {
            self.version_check = None;
            if protocol.is_some_and(|p| p < snook_legend_proto::PROTOCOL_VERSION) {
                // This game is newer than the server: updating it can't
                // help, waiting for the server can.
                self.outdated = false;
                self.error = Some(
                    t("The server is still being updated to the new version. Try again in a few minutes.")
                        .into(),
                );
            }
        }
        // Outgoing match messages.
        if let Some(m) = current.as_mut()
            && let Some(o) = m.online.as_mut()
        {
            // Keep messages while reconnecting; they go out once this
            // connection has signed in again.
            let chat_off = self.chat_off();
            let mut held = None;
            if let Some(c) = &self.conn
                && self.signed_in()
                && self.authed
            {
                let outbox: Vec<ClientMsg> = std::mem::take(&mut o.outbox);
                for msg in outbox {
                    // A correspondence game says which game it is.
                    let msg = match o.corr.as_mut() {
                        Some(table) => match crate::correspondence::outgoing(table, msg) {
                            Some(msg) => msg,
                            None => continue,
                        },
                        None => msg,
                    };
                    // A line to the opponent with the chat off waits for
                    // the dialog that turns it on.
                    if chat_off && matches!(msg, ClientMsg::MatchChat { .. }) {
                        held = Some(msg);
                        continue;
                    }
                    // A line to the opponent shows at once (not after the
                    // round trip to the server).
                    if let ClientMsg::MatchChat { text } = &msg
                        && o.corr.is_none()
                    {
                        let admin = self
                            .me
                            .as_ref()
                            .is_some_and(|me| me.role == snook_legend_proto::Role::Admin);
                        o.echo_chat(&m.names[o.seat], admin, text);
                    }
                    c.send(msg);
                }
            }
            if let Some(msg) = held {
                self.send_text(msg);
            }
        }
        if let Stage::Reconnecting { attempt, wait } = &mut self.stage {
            *wait -= dt;
            if *wait <= 0.0 && self.conn.is_none() {
                *attempt += 1;
                self.authed = false;
                self.connected = false;
                self.conn = Some(Connection::open(&self.server, self.rules));
            }
        }
        // Measure the round trip every few seconds while signed in.
        if self.signed_in() {
            self.ping_timer -= dt;
            if self.ping_timer <= 0.0 {
                self.ping_timer = 3.0;
                if self.ping_sent.is_none_or(|t| t.elapsed().as_secs() > 10) {
                    self.ping_sent = Some(std::time::Instant::now());
                    self.send(ClientMsg::Ping);
                }
            }
        }

        while let Some(event) = self.conn.as_mut().and_then(|c| c.poll()) {
            match event {
                NetEvent::Connected => {
                    self.connected = true;
                    self.authed = false;
                    if self.lan {
                        self.send(ClientMsg::Guest { name: self.username.clone() });
                        // Reconnecting stays so (its count of attempts and
                        // "Reconnected." once back), as on the global server.
                        if !matches!(self.stage, Stage::Reconnecting { .. }) {
                            self.stage = Stage::Authenticating;
                        }
                    } else if self.auto_resume && !self.token.is_empty() {
                        self.send(ClientMsg::Resume { token: self.token.clone() });
                        if !matches!(self.stage, Stage::Reconnecting { .. }) {
                            self.stage = Stage::Authenticating;
                        }
                    } else {
                        self.stage = Stage::SignIn;
                    }
                }
                NetEvent::Closed(reason) => {
                    // The public server's own domain didn't answer at all:
                    // the same server at its host's address, for this session.
                    if !self.connected
                        && !self.lan
                        && !self.fallback
                        && self.server.trim() == crate::config::OFFICIAL_SERVER
                    {
                        self.fallback = true;
                        self.server = crate::config::LEGACY_SERVER.to_string();
                        self.conn = Some(Connection::open(&self.server, self.rules));
                        continue;
                    }
                    self.conn = None;
                    self.authed = false;
                    self.queue = None;
                    // The server drops a session's invitations and table.
                    self.forget_invitations();
                    // Nothing asked on that connection will be answered: a
                    // purchase may have gone through or not (the wallet
                    // says, once back).
                    self.drill_starts = 0;
                    if self.buying.take().is_some() {
                        self.shop_error = Some(
                            t("The connection dropped: check your coins before trying again.")
                                .to_string(),
                        );
                    }
                    let was_in = self.signed_in();
                    let attempt = match self.stage {
                        Stage::Reconnecting { attempt, .. } => attempt,
                        _ => 0,
                    };
                    let can_resume = self.lan || !self.token.is_empty();
                    if was_in && can_resume && self.auto_resume && attempt < MAX_RECONNECTS {
                        if attempt == 0
                            && let Some(m) = current.as_mut()
                        {
                            m.notify(t("Connection lost. Reconnecting…"), true);
                        }
                        let wait = (1.0 + attempt as f32 * 1.5).min(8.0);
                        self.stage = Stage::Reconnecting { attempt, wait };
                    } else {
                        if let Some(m) = current.take_if(|m| m.is_online()) {
                            drop(m);
                        }
                        self.me = None;
                        self.held.borrow_mut().clear();
                        self.stage = if self.lan { Stage::Idle } else { Stage::SignIn };
                        if self.error.is_none() {
                            self.error = Some(crate::i18n::server_text(&reason));
                        }
                    }
                }
                NetEvent::Message(msg) => save |= self.handle(*msg, current, config),
            }
        }
        // The lounge shows who is playing the computer (they can still be
        // invited: the invitation shows over their game).
        // Away too, after a while untouched (a minimised window included).
        let vs_computer = current
            .as_ref()
            .is_some_and(|m| m.is_against_computer() || (m.practice && !m.free_play));
        // Focus mode, during a game played here: no invitations (the
        // server turns them away, and withdraws those waiting).
        let focus = config.focus_mode && current.as_ref().is_some_and(|m| m.is_played_offline());
        let activity = (vs_computer, self.away, focus);
        if self.signed_in() && activity != self.activity_sent {
            self.activity_sent = activity;
            self.send(ClientMsg::SetActivity { vs_computer, away: self.away, focus });
        }
        // The rules the player plays: the lounge shows them (a new session
        // says them in its hello).
        self.rules = config.rules();
        if self.signed_in() && self.rules_sent != Some(self.rules) {
            self.rules_sent = Some(self.rules);
            self.send(ClientMsg::SetRules { rules: self.rules });
            // Searching under the other rules: the search goes on under these.
            if let Some((rated, rules, ..)) = self.queue
                && rules != self.rules
            {
                self.send(ClientMsg::QueueJoin { rated, rules: self.rules });
            }
        }
        save
    }

    fn handle(&mut self, msg: ServerMsg, current: &mut Option<Match>, config: &mut Config) -> bool {
        let mut msg = msg;
        if self.chat_off() {
            let me = self.username.clone();
            hide_players_text(&mut msg, &me);
        }
        let mut save = false;
        // A server (or a game on the local network) that names a seat no
        // table has is not listened to: the game would stop on it.
        if !plausible(&msg) {
            return save;
        }
        // On the local network the host is anyone: it has no say in the
        // player's purchases, wallet or settings (a payment page from it
        // could be anything).
        if self.lan
            && matches!(
                msg,
                ServerMsg::PaymentPage { .. }
                    | ServerMsg::Wallet { .. }
                    | ServerMsg::ShopInfo { .. }
                    | ServerMsg::StoreTicket { .. }
                    | ServerMsg::StoreRedeemed { .. }
                    // Nor over the account of the official server.
                    | ServerMsg::AccountDeleted
                    | ServerMsg::Account { .. }
                    | ServerMsg::Age { .. }
                    // Nor over what only that server gives: coins, VIP, XP,
                    // achievements, presents, missions, medals, the daily
                    // challenge, the drills, the tour, its events.
                    | ServerMsg::ItemBought { .. }
                    | ServerMsg::VipGift { .. }
                    | ServerMsg::CoinsEarned { .. }
                    | ServerMsg::Xp { .. }
                    | ServerMsg::AchievementUnlocked { .. }
                    | ServerMsg::MyAchievements { .. }
                    | ServerMsg::NewGifts { .. }
                    | ServerMsg::Gifts { .. }
                    | ServerMsg::GiftThanked { .. }
                    | ServerMsg::Missions { .. }
                    | ServerMsg::CupMedals { .. }
                    | ServerMsg::Daily { .. }
                    | ServerMsg::DailyStarted { .. }
                    | ServerMsg::DailyResult { .. }
                    | ServerMsg::Drills { .. }
                    | ServerMsg::DrillStarted { .. }
                    | ServerMsg::DrillResult { .. }
                    | ServerMsg::LadderClaim { .. }
                    | ServerMsg::LadderTicket { .. }
                    | ServerMsg::GhostStyle { .. }
                    | ServerMsg::Events { .. }
                    | ServerMsg::Inbox { .. }
                    | ServerMsg::InboxNew { .. }
                    | ServerMsg::Collection { .. }
            )
        {
            return save;
        }
        match msg {
            ServerMsg::AuthOk { player, token } => {
                // Another account than the last one signed in (not the name
                // typed in the form, nor the new name this one asked for):
                // nothing of that one's stays.
                let renamed = self
                    .renaming_to
                    .take()
                    .is_some_and(|n| n.eq_ignore_ascii_case(&player.username));
                if renamed {
                    // Its achievements are its own under the new name too.
                    if self.achievements_of.eq_ignore_ascii_case(&self.signed_in_as) {
                        self.achievements_of = player.username.clone();
                    }
                } else if !self.signed_in_as.is_empty()
                    && !self.signed_in_as.eq_ignore_ascii_case(&player.username)
                {
                    self.forget_account();
                }
                self.signed_in_as = player.username.clone();
                // A new session: no invitation or code of the last one is
                // still valid, and the server says again if it restarts.
                self.forget_invitations();
                self.draining = None;
                self.outdated = false;
                self.authed = true;
                // The server says it next (a server that knows it).
                self.age = None;
                if let Some(m) = current.as_mut()
                    && matches!(self.stage, Stage::Reconnecting { .. })
                {
                    m.notify(t("Reconnected."), false);
                }
                self.username = player.username.clone();
                // The profile picture: the account's, or else the one chosen
                // here before signing in becomes the account's.
                let account_picture = player.avatar.clone();
                let guest = player.guest;
                // What the hello said.
                self.rules_sent = Some(player.rules);
                self.me = Some(player);
                if !account_picture.is_empty() && !self.lan {
                    crate::avatars::set_mine(&account_picture);
                    config.avatar = account_picture;
                    save = true;
                } else if !config.avatar.is_empty() && !guest && !self.lan {
                    self.send(ClientMsg::SetAvatar { avatar: config.avatar.clone() });
                }
                // A new session: the server learns again what this player
                // is doing (its welcome follows).
                self.activity_sent = (false, false, false);
                // A new session: the board comes again; the recent games may have changed.
                self.social.seeks.clear();
                self.social.recent_stale = true;
                self.auto_resume = true;
                self.session_count = self.session_count.wrapping_add(1);
                self.password.clear();
                self.error = None;
                self.stage = Stage::Lounge;
                if !self.lan {
                    self.token = token.clone();
                    config.username = self.username.clone();
                    config.token = token;
                    // (The server in the settings is the player's: one
                    // changed there while signed in stays changed.)
                    config.token_server = self.server.clone();
                    save = true;
                    self.send(ClientMsg::Friends);
                    self.send(ClientMsg::Leaderboard { rules: self.rules });
                    // The wallet: what the account owns (the tour's reward
                    // among it: see ladder::Progress::restore_from_reward).
                    self.send(ClientMsg::Shop);
                    // Development aid: SNOOK_LEGEND_AUTOINVITE=<name> invites that player.
                    if let Some(to) = crate::dev::var("SNOOK_LEGEND_AUTOINVITE") {
                        self.send(ClientMsg::Invite { to, rated: true, rules: self.rules });
                    }
                    // SNOOK_LEGEND_INVITE_DIALOG=<name> opens the invitation to them.
                    if let Some(to) = crate::dev::var("SNOOK_LEGEND_INVITE_DIALOG") {
                        self.open_invite(&to);
                    }
                    // SNOOK_LEGEND_AUTOQUEUE=1 joins "Play now".
                    if crate::dev::flag("SNOOK_LEGEND_AUTOQUEUE") {
                        self.send(ClientMsg::QueueJoin { rated: true, rules: self.rules });
                    }
                    // SNOOK_LEGEND_AUTOWATCH=<name> watches that friend's game.
                    if let Some(username) = crate::dev::var("SNOOK_LEGEND_AUTOWATCH") {
                        self.send(ClientMsg::Spectate { username });
                    }
                }
                // The tour final's deal asked on the lost session: asked
                // again (its answer would never come).
                if self.awaiting_ticket {
                    self.send(ClientMsg::LadderTicket);
                }
                // A friend's game being watched: the new session watches it
                // again (the server forgot the old session's watching).
                if let Some(m) = current.as_ref().filter(|m| m.is_spectating())
                    && let Some(o) = m.online.as_ref()
                {
                    let username = m.names[o.seat % 2].clone();
                    self.send(ClientMsg::Spectate { username });
                }
                // What was sent while reconnecting goes now, in order.
                let held = std::mem::take(&mut *self.held.borrow_mut());
                for msg in held {
                    self.send(msg);
                }
            }
            ServerMsg::AuthError { reason } => {
                // A message that went out before this connection signed in
                // (again): it says nothing of a sign-in (the Resume or the
                // Login answers that), whatever was asked.
                if reason == "Please log in first." {
                    return save;
                }
                // Back on the local network while the host still holds the
                // old connection (it lets go within its grace): try again.
                if self.lan
                    && reason == "Someone with that name is already here."
                    && let Stage::Reconnecting { attempt, .. } = self.stage
                    && attempt < MAX_RECONNECTS
                {
                    self.conn = None;
                    self.authed = false;
                    self.stage = Stage::Reconnecting { attempt, wait: 3.0 };
                    return save;
                }
                // This version is too old for the server: the saved session
                // is still good for the new one.
                self.outdated = reason.contains("out of date");
                if self.outdated {
                    self.version_check = Some(server_protocol(&self.server));
                }
                // Only a session the server no longer knows is forgotten: a
                // passing trouble (the server busy, too many tries, a ban
                // that ends) keeps it, to try again.
                if self.auto_resume && reason == SESSION_EXPIRED {
                    self.token.clear();
                    config.token.clear();
                    save = true;
                }
                self.auto_resume = false;
                self.me = None;
                self.held.borrow_mut().clear();
                if let Some(m) = current.take_if(|m| m.is_online()) {
                    drop(m);
                }
                self.stage = if self.lan { Stage::Idle } else { Stage::SignIn };
                self.error = Some(crate::i18n::server_text(&reason));
            }
            ServerMsg::Kicked { reason } => {
                // Signed in elsewhere, banned, or disconnected by the staff:
                // don't fight over the account by reconnecting.
                self.auto_resume = false;
                self.forget_invitations();
                self.me = None;
                self.held.borrow_mut().clear();
                self.authed = false;
                if let Some(m) = current.take_if(|m| m.is_online()) {
                    drop(m);
                }
                self.conn = None;
                self.queue = None;
                self.stage = if self.lan { Stage::Idle } else { Stage::SignIn };
                self.error = Some(crate::i18n::server_text(&reason));
            }
            ServerMsg::Welcome { motd, .. } => self.motd = crate::i18n::server_text(&motd),
            ServerMsg::Lounge { players } => self.players = players,
            ServerMsg::PlayerUpdated { player } => {
                if self.me.as_ref().is_some_and(|me| me.username == player.username) {
                    self.me = Some(player.clone());
                }
                // Someone at this table changed their cue, chalk or effect.
                if let Some(m) = current.as_mut().filter(|m| m.is_online())
                    && let Some(seat) = m.names.iter().position(|n| *n == player.username)
                {
                    m.cue_looks[seat] = (player.cue.clone(), player.chalk.clone());
                    m.cue_levels[seat] = player.cue_level;
                    m.effects[seat] = player.effect.clone();
                }
                for f in &mut self.friends.friends {
                    if f.player.username == player.username {
                        f.player = player.clone();
                        f.online = true;
                    }
                }
                match self.players.iter_mut().find(|p| p.username == player.username) {
                    Some(p) => *p = player,
                    None => {
                        // A friend just came online.
                        if self.friends.friends.iter().any(|f| f.player.username == player.username)
                        {
                            self.attention =
                                Some(tw("{name} is online.", &[("name", &player.username)]));
                        }
                        if self.me.as_ref().is_none_or(|me| me.username != player.username) {
                            self.arrived = Some((player.username.clone(), get_time()));
                        }
                        self.players.push(player)
                    }
                }
            }
            ServerMsg::PlayerLeft { username } => {
                self.players.retain(|p| p.username != username);
                self.incoming.retain(|i| i.from != username);
                for f in &mut self.friends.friends {
                    if f.player.username == username {
                        f.online = false;
                    }
                }
                if self.outgoing.as_ref().is_some_and(|(to, ..)| *to == username) {
                    self.outgoing = None;
                }
            }
            ServerMsg::ChatDeleted { id, by } => {
                if let Some(l) = self.chat.iter_mut().find(|l| l.id == id) {
                    l.deleted = Some(by);
                    l.text.clear();
                }
            }
            ServerMsg::ChatRemoved { id } => self.chat.retain(|l| l.id != id),
            ServerMsg::ChatCleared { by } => {
                self.chat.clear();
                self.chat_scroll = 0;
                let text = tw("The chat was cleared by {name}.", &[("name", &by)]);
                self.chat.push(FeedLine {
                    text,
                    kind: crate::ui::FeedKind::Event,
                    ..Default::default()
                });
            }
            ServerMsg::ChatHistory { lines } => {
                self.chat = lines.iter().map(FeedLine::chat).collect();
                self.chat_scroll = 0;
            }
            ServerMsg::Chat { line } => {
                // This player's own line, shown already: it takes the
                // server's (its number, to delete it) in its place.
                let mine =
                    self.me.as_ref().is_some_and(|m| m.username.eq_ignore_ascii_case(&line.from));
                if mine && let Some(e) = self.echoes.iter().position(|e| *e == line.text) {
                    self.echoes.remove(e);
                    if let Some(i) = self.chat.iter().position(|l| l.pending) {
                        self.chat[i] = FeedLine::chat(&line);
                        return save;
                    }
                }
                if !self.friends.blocked.iter().any(|b| b.eq_ignore_ascii_case(&line.from)) {
                    self.chat.push(FeedLine::chat(&line));
                    if self.chat.len() > 200 {
                        self.chat.remove(0);
                    }
                    if self.chat_scroll > 0 {
                        self.chat_scroll += 1;
                    }
                }
            }
            ServerMsg::Invited { from, rated, rules } => {
                if crate::dev::flag("SNOOK_LEGEND_AUTOPLAY") && current.is_none() {
                    // Development aid: accept at once.
                    self.send(ClientMsg::AnswerInvite { from, accept: true });
                    return save;
                }
                // In focus mode during a game nothing calls for attention:
                // the invitation waits for the game's end (the server turns
                // away the ones that come once it knows).
                let focused =
                    config.focus_mode && current.as_ref().is_some_and(Match::is_focus_game);
                if !focused {
                    self.attention = Some(tw("{name} invites you to play.", &[("name", &from)]));
                    self.invite_sound = true;
                }
                self.incoming.retain(|i| i.from != from);
                self.incoming.push(Invite { from, rated, rules, at: get_time() });
            }
            ServerMsg::InviteSent { to, rated, rules } => self.outgoing = Some((to, rated, rules)),
            ServerMsg::InviteCancelled { from } => self.incoming.retain(|i| i.from != from),
            ServerMsg::InviteDeclined { by } => {
                self.outgoing = None;
                self.toast(tw("{by} declined your invitation.", &[("by", &by)]), false);
            }
            ServerMsg::InviteFailed { reason } => {
                self.outgoing = None;
                self.toast(crate::i18n::server_text(&reason), false);
            }
            ServerMsg::Tables { tables } => self.tables = tables,
            ServerMsg::TableOpened { table } => {
                self.tables.retain(|t| t.id != table.id);
                self.tables.push(table);
            }
            ServerMsg::TableClosed { id } => self.tables.retain(|t| t.id != id),
            ServerMsg::Tournaments { tournaments } => self.tournaments = tournaments,
            ServerMsg::TournamentUpdated { tournament } => {
                // A tournament this player is in has just begun.
                let me = self.me.as_ref().map(|m| m.username.as_str()).unwrap_or("");
                let was = self.tournaments.iter().find(|t| t.id == tournament.id);
                if was.is_some_and(|t| t.state == TournamentState::Registering)
                    && tournament.state == TournamentState::Running
                    && tournament.players.iter().any(|p| p.username == me)
                {
                    self.attention = Some(tw(
                        "{name} has started!",
                        &[("name", &crate::i18n::tournament_name(&tournament.name))],
                    ));
                }
                match self.tournaments.iter_mut().find(|t| t.id == tournament.id) {
                    Some(t) => *t = tournament,
                    None => self.tournaments.push(tournament),
                }
            }
            ServerMsg::TournamentRemoved { id } => {
                self.tournaments.retain(|t| t.id != id);
                if self.selected_tournament == Some(id) {
                    self.selected_tournament = None;
                }
            }
            ServerMsg::Leaderboard { rules, entries } => {
                *self.leaderboard.get_mut(rules) = entries;
                *self.leaderboard_loaded.get_mut(rules) = true;
            }
            ServerMsg::Profile { profile } => self.profile = Some(*profile),
            ServerMsg::Friends { friends, incoming, outgoing, blocked } => {
                self.friends = FriendsData { friends, incoming, outgoing, blocked };
                if self.friends.incoming.is_empty() {
                    self.unread_friend_requests = false;
                }
            }
            ServerMsg::FriendRequestReceived { from } => {
                self.unread_friend_requests = true;
                self.toast(tw("{name} wants to be your friend.", &[("name", &from)]), true);
            }
            ServerMsg::AchievementUnlocked { code } => {
                let title = snook_legend_proto::ACHIEVEMENTS
                    .iter()
                    .find(|a| a.code == code)
                    .map(|a| t(a.title).to_string())
                    .unwrap_or(code.clone());
                self.toast(tw("Achievement unlocked: {title}", &[("title", &title)]), true);
                // Known at once (what it unlocks can be used straight away),
                // and what comes with it is shown.
                let mine = self.me.as_ref().is_some_and(|me| me.username == self.achievements_of);
                if mine && !self.achievements.contains(&code) {
                    self.achievements.push(code.clone());
                }
                self.earned(&code);
            }
            // A warning from the staff waits in a dialog until it is read
            // (a toast could be cut, pushed out by others or missed).
            ServerMsg::Notice { text } if text.starts_with(STAFF_WARNING) => {
                if self.staff_warnings.len() < 20 {
                    self.staff_warnings.push(text[STAFF_WARNING.len()..].trim().to_string());
                }
                self.attention = Some(crate::i18n::server_text(&text));
            }
            ServerMsg::Notice { text } => self.toast(crate::i18n::server_text(&text), true),
            ServerMsg::MatchStarted {
                match_id,
                seat,
                players,
                rated,
                game,
                shot_clock_secs,
                clock_left,
                tournament,
                spectating,
                table_look,
            } => {
                self.table_look = table_look;
                if !spectating && current.as_ref().is_none_or(|m| m.match_id() != match_id) {
                    let other = &players[1 - seat.min(1)].username;
                    self.attention =
                        Some(tw("Your game against {name} is starting.", &[("name", other)]));
                }
                // Watching doesn't end the player's invitation or search (the
                // server keeps both).
                if !spectating {
                    self.outgoing = None;
                    self.queue = None;
                    self.incoming.clear();
                }
                // Development aid: SNOOK_LEGEND_AUTOCHAT="text" says it at the start of a match.
                if let Some(text) = crate::dev::var("SNOOK_LEGEND_AUTOCHAT").filter(|_| !spectating)
                {
                    self.send(ClientMsg::MatchChat { text });
                }
                // SNOOK_LEGEND_AUTOREACT="code" sends that reaction.
                if let Some(code) = crate::dev::var("SNOOK_LEGEND_AUTOREACT") {
                    self.send(ClientMsg::React { code });
                }
                let next = Match::online(
                    match_id,
                    seat,
                    &players,
                    rated,
                    game,
                    shot_clock_secs,
                    clock_left,
                    tournament,
                    config.guide,
                    spectating,
                );
                match current.as_mut() {
                    // Same match after reconnecting, or a rematch with the same opponent.
                    // (Not a daily game's table, nor watching against playing.)
                    Some(m)
                        if m.is_online()
                            && !crate::correspondence::is_corr(m)
                            && m.is_spectating() == next.is_spectating()
                            && same_players(m, &next) =>
                    {
                        let same_game = m.match_id() == match_id;
                        m.continue_online(next, same_game)
                    }
                    _ => {
                        // A daily attempt or a drill session gives the table
                        // up: what it made is still sent (see main.rs).
                        if let Some(old) = current.replace(next)
                            && (old.daily.is_some() || old.drill.is_some())
                        {
                            self.displaced = Some(old);
                        }
                    }
                }
                if let Some(o) = current.as_mut().and_then(|m| m.online.as_mut()) {
                    o.unlocked = self.achievements.clone();
                }
                // A message sent just before the match (e.g. "this game is
                // casual"): only one that has just arrived, not whatever
                // toast happened to be showing ("+5 coins…").
                if self.toasts.back().is_some_and(|t| t.1 > 4.0)
                    && let (Some((text, ..)), Some(m)) = (self.toasts.pop_back(), current.as_mut())
                {
                    m.notify(text, true);
                }
            }
            // A stroke of a daily game the server didn't take (asked too fast,
            // a moment's trouble): the table shows it played, it isn't.
            ServerMsg::Error { message } if crate::correspondence::awaiting(current) => {
                crate::correspondence::refused(self, current);
                self.toast(crate::i18n::server_text(&message), false);
            }
            ServerMsg::Error { message } if self.buying.is_some() => {
                self.buying = None;
                self.shop_error = Some(crate::i18n::server_text(&message));
            }
            // A message an older server doesn't know (a newer game asking
            // for something new): nothing the player can do about it.
            ServerMsg::Error { message } if message == "Malformed message." => {}
            // Asked for the tour's final too often: it is played without
            // the server (the game says so). Only that refusal: another
            // error is about something else.
            ServerMsg::Error { message } if self.awaiting_ticket && message == TICKET_REFUSED => {
                self.awaiting_ticket = false;
                self.ticket_refused = true;
                self.toast(crate::i18n::server_text(&message), false);
            }
            ServerMsg::Error { message } => match current.as_mut() {
                Some(m) if m.is_online() => m.handle_server(ServerMsg::Error { message }),
                _ => {
                    // A lounge line refused: it leaves the chat, the notice says why.
                    let refused = [
                        "You are muted",
                        "You're sending messages too quickly",
                        "Please don't repeat",
                    ];
                    if refused.iter().any(|p| message.starts_with(p))
                        && let Some(text) = self.echoes.pop()
                        && let Some(i) = self.chat.iter().rposition(|l| l.pending && l.text == text)
                    {
                        self.chat.remove(i);
                    }
                    self.toast(crate::i18n::server_text(&message), false)
                }
            },
            ServerMsg::ItemBought { code, gift_to } => {
                self.buying = None;
                self.bought = Some((code, gift_to));
            }
            ServerMsg::CoinsEarned { amount, reason } => {
                let why = match reason.as_str() {
                    "daily" => t("daily bonus"),
                    "win" => t("for the win"),
                    "game" => t("for the game"),
                    "prize" => t("tournament prize"),
                    "refund" => t("entry fee back"),
                    "level" => t("new level"),
                    "league" => t("new league"),
                    "mission" => {
                        if self.side != Side::Missions {
                            self.mission_done = true;
                        }
                        t("mission complete")
                    }
                    _ => "",
                };
                let text = tw("+{n} coins · {why}", &[("n", &amount), ("why", &why)]);
                let boost = match reason.as_str() {
                    "game" | "win" => self.boost(snook_legend_proto::events::EventKind::Coins),
                    _ => 1,
                };
                self.toast(with_boost(text, boost), true);
            }
            ServerMsg::Age { minor, chat_allowed } => {
                self.age = Some(AgeState { minor, chat_allowed });
                if !self.chat_off() {
                    self.send_pending_text();
                } else {
                    // What came before the age (the lounge, the board, the
                    // friends, sent as the session began) is cleaned now.
                    let me = self.username.clone();
                    let other = |n: &str| !n.eq_ignore_ascii_case(&me);
                    for p in self.players.iter_mut().filter(|p| other(&p.username)) {
                        p.status_message.clear();
                    }
                    for f in self.friends.friends.iter_mut().filter(|f| other(&f.player.username)) {
                        f.player.status_message.clear();
                    }
                    for n in self.social.seeks.iter_mut().filter(|n| other(&n.username)) {
                        n.text.clear();
                    }
                }
            }
            ServerMsg::Account { recovery_email, payment_email, name_changes_left } => {
                // After a payment, without a recovery e-mail: offer the one paid with.
                if recovery_email.is_none() && !config.email_offer_declined {
                    self.email_offer = payment_email.clone();
                }
                self.account =
                    Some(AccountView { recovery_email, payment_email, name_changes_left });
            }
            ServerMsg::AccountDeleted => {
                self.forget_account();
                self.auto_resume = false;
                self.me = None;
                self.authed = false;
                self.held.borrow_mut().clear();
                self.conn = None;
                self.token.clear();
                config.token.clear();
                config.username.clear();
                save = true;
                self.stage = Stage::SignIn;
                self.error = Some(t("Your account was deleted.").to_string());
            }
            ServerMsg::Replays { games, limit } => self.replays = Some((games, limit)),
            ServerMsg::Replay { replay } => self.replay = Some(replay),
            ServerMsg::Featured { featured } => self.featured = featured,
            ServerMsg::Missions { daily, weekly, day_ends, week_ends } => {
                self.missions = Some(MissionsView { daily, weekly, day_ends, week_ends });
            }
            ServerMsg::Board { notes } => self.board = notes,
            ServerMsg::InviteCode { code } => self.invite_code = Some(code),
            ServerMsg::Draining { secs } => {
                self.draining =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(secs as u64));
            }
            ServerMsg::Wallet { coins, vip_until, owned, equipped, cue_wins } => {
                let wallet = WalletView { coins, vip_until, owned, equipped, cue_wins };
                // A cue that just reached a new level (a game was won).
                use snook_legend_proto::shop::{Category, cue_level};
                let cue = self.effective(&wallet, Category::Cue);
                if let Some(old) = &self.wallet {
                    let (before, now) = (old.cue_wins(cue), wallet.cue_wins(cue));
                    if now > before && cue_level(now) > cue_level(before) {
                        let name = snook_legend_proto::shop::item(cue).map_or(cue, |i| t(i.name));
                        let text = tw(
                            "Your cue {cue} reached level {n}!",
                            &[("cue", &name), ("n", &cue_level(now))],
                        );
                        self.toast(text, true);
                    }
                }
                let level = cue_level(wallet.cue_wins(cue));
                if config.cue_level != level {
                    config.cue_level = level;
                    self.looks_changed = true;
                    save = true;
                }
                // The items in use, as the server allows them now.
                for c in snook_legend_proto::shop::Category::ALL {
                    let code = self.effective(&wallet, c).to_string();
                    let slot = match c {
                        snook_legend_proto::shop::Category::Cloth => &mut config.cloth,
                        snook_legend_proto::shop::Category::Cue => &mut config.cue,
                        snook_legend_proto::shop::Category::Chalk => &mut config.chalk,
                        snook_legend_proto::shop::Category::Balls => &mut config.balls,
                        snook_legend_proto::shop::Category::Effect => &mut config.effect,
                        snook_legend_proto::shop::Category::Table => &mut config.table,
                        snook_legend_proto::shop::Category::Hall => &mut config.hall,
                        // Only seen online.
                        snook_legend_proto::shop::Category::Frame
                        | snook_legend_proto::shop::Category::Name
                        | snook_legend_proto::shop::Category::Reaction => {
                            continue;
                        }
                    };
                    if *slot != code {
                        *slot = code;
                        self.looks_changed = true;
                        save = true;
                    }
                }
                self.wallet = Some(wallet);
            }
            ServerMsg::ShopInfo {
                payments,
                packs,
                vip_price,
                vip_coins,
                vip_gift_price,
                welcome_price,
                store_packs,
                store_vip,
            } => {
                let ids = store_packs.iter().map(|p| p.store_id.clone()).collect();
                // (Asked even with the sales off: a purchase paid for in an
                // earlier version and never received still arrives.)
                let effects = self.store_shop.offers(ids, store_vip.clone(), self.signed_in());
                // The Store build without sales shows none of the server's
                // Store offers: coins are earned, VIP isn't sold.
                let (store_packs, store_vip) = if crate::channel::store_sales() {
                    (store_packs, store_vip)
                } else {
                    (Vec::new(), String::new())
                };
                self.offers = Some(ShopOffers {
                    payments,
                    packs,
                    vip_price,
                    vip_coins,
                    vip_gift_price,
                    welcome_price,
                    store_packs,
                    store_vip,
                });
                self.store_effects(effects);
            }
            // Only the Store build asks for these.
            ServerMsg::StoreTicket { ticket, user_id } => self.store_shop.ticket(ticket, user_id),
            ServerMsg::StoreRedeemed { coins, pending, error, vip_until } => {
                let effects = self.store_shop.redeemed(coins, pending, &error, vip_until);
                self.store_effects(effects);
            }
            ServerMsg::VipGift { from, to } => {
                let text = if self.me.as_ref().is_some_and(|me| me.username == to) {
                    tw("{from} gave you a month of VIP!", &[("from", &from)])
                } else {
                    tw("{name} received a month of VIP.", &[("name", &to)])
                };
                self.toast(text.clone(), true);
                self.attention = Some(text);
            }
            // The Store build asks for none, and opens none.
            ServerMsg::PaymentPage { .. } if !crate::channel::real_money_purchases() => {}
            ServerMsg::PaymentPage { url } => {
                if crate::util::open_url(&url) {
                    self.toast(t("The payment page opened in your browser."), true);
                } else {
                    self.toast(t("Could not open the browser."), false);
                }
            }
            ServerMsg::Queue { searching, rated, rules, waited_secs, range } => {
                self.queue = searching
                    .then(|| (rated, rules, waited_secs, range, std::time::Instant::now()));
            }
            ServerMsg::SpectateEnded { reason } => {
                if current.as_ref().is_some_and(|m| m.is_spectating()) {
                    *current = None;
                }
                self.toast(crate::i18n::server_text(&reason), false);
            }
            ServerMsg::LadderClaim { result } => self.ladder_claim = Some(result),
            ServerMsg::Daily { info } => self.daily = Some(info),
            ServerMsg::Drills { info } => self.drills = Some(info),
            ServerMsg::CupMedals { medals, kept } => self.cup_medal_news.push((medals, kept)),
            ServerMsg::DrillResult { code, result } => self.drill_result = Some((code, result)),
            // Only the answer to the last request: an older one's session
            // is no longer the server's.
            ServerMsg::DrillStarted { code, session } => {
                self.drill_starts = self.drill_starts.saturating_sub(1);
                if self.drill_starts == 0 {
                    self.drill_session = Some((code, session));
                }
            }
            ServerMsg::Events { events } => self.events = events,
            ServerMsg::LadderTicket { seed, breaker } if self.awaiting_ticket => {
                self.awaiting_ticket = false;
                self.ladder_ticket = Some((seed, breaker));
            }
            ServerMsg::LadderTicket { .. } => {}
            ServerMsg::Xp { xp, gained } => {
                use snook_legend_proto::xp::level_of;
                let before = self.xp.map(level_of);
                self.xp = Some(xp);
                if gained > 0 {
                    match before.filter(|&b| level_of(xp) > b) {
                        Some(_) => self.toast(
                            tw("Level {n}! (+{xp} XP)", &[("n", &level_of(xp)), ("xp", &gained)]),
                            true,
                        ),
                        None => {
                            let text = tw("+{xp} XP", &[("xp", &gained)]);
                            let boost = self.boost(snook_legend_proto::events::EventKind::Xp);
                            self.toast(with_boost(text, boost), false)
                        }
                    }
                }
            }
            ServerMsg::DailyStarted { day, seed } => {
                self.daily_refused = None;
                self.daily_started = Some((day, seed));
            }
            ServerMsg::DailyResult { result: Err(e), .. } if current.is_none() => {
                // Refused before an attempt began (none left, the day over).
                self.daily_refused = Some(e);
            }
            ServerMsg::DailyResult { result, coins, .. } => {
                if result.is_ok() && coins > 0 {
                    self.toast(tw("Table cleared! +{n} coins", &[("n", &coins)]), true);
                }
                self.daily_result = Some((result, coins));
            }
            ServerMsg::GhostStyle { username, answer } => {
                if let Some(g) =
                    self.ghost.as_mut().filter(|g| g.username.eq_ignore_ascii_case(&username))
                {
                    g.answer = Some(answer);
                }
            }
            ServerMsg::MyAchievements { codes } => {
                if let Some(o) = current.as_mut().and_then(|m| m.online.as_mut()) {
                    o.unlocked = codes.clone();
                }
                self.achievements = codes;
                self.achievements_of =
                    self.me.as_ref().map(|me| me.username.clone()).unwrap_or_default();
            }
            msg @ (ServerMsg::CorrList { .. }
            | ServerMsg::CorrState { .. }
            | ServerMsg::CorrShot { .. }
            | ServerMsg::CorrShotRejected { .. }
            | ServerMsg::CorrChose { .. }
            | ServerMsg::CorrNotice { .. }) => {
                crate::correspondence::handle(self, msg, current, config.guide);
            }
            ServerMsg::Pong => {
                if let Some(sent) = self.ping_sent.take() {
                    self.rtt = Some(sent.elapsed());
                }
            }
            ServerMsg::RecentOpponents { players } => self.social.recent = Some(players),
            ServerMsg::SeekBoard { mut notes } => {
                notes.sort_by_key(|n| std::cmp::Reverse((n.at, n.username.to_lowercase())));
                self.social.seeks = notes;
            }
            ServerMsg::SeekPosted { note } => self.social.posted(note),
            ServerMsg::SeekRemoved { username } => self.social.removed(&username),
            ServerMsg::NewGifts { gifts } => self.gifts_arrived(gifts, current.is_some()),
            ServerMsg::Gifts { gifts } => self.gifts.history = Some(gifts),
            ServerMsg::GiftThanked { by, code } => self.gift_thanked(&by, &code),
            other => {
                if let Some(m) = current.as_mut().filter(|m| m.is_online()) {
                    m.handle_server(other);
                }
            }
        }
        save
    }

    /// Called when the player leaves the table to go back to the lounge.
    /// The item used in `category` (what is equipped, if still allowed).
    pub fn effective(
        &self,
        w: &WalletView,
        category: snook_legend_proto::shop::Category,
    ) -> &'static str {
        use snook_legend_proto::shop;
        w.equipped
            .iter()
            .find(|(c, _)| c == category.code())
            .and_then(|(_, code)| shop::item(code))
            .filter(|i| {
                i.category == category && shop::can_use(i, &w.owned, &self.achievements, w.is_vip())
            })
            .map(|i| i.code)
            .unwrap_or_else(|| category.default_item())
    }

    /// Whether this player may use `item` now.
    pub fn can_use(&self, item: &snook_legend_proto::shop::Item) -> bool {
        // Borrowed (the shop asks for every item, every frame).
        let (owned, vip) = match &self.wallet {
            Some(w) => (&w.owned[..], w.is_vip()),
            None => (&[][..], false),
        };
        snook_legend_proto::shop::can_use(item, owned, &self.achievements, vip)
    }

    /// Starts watching a friend's game.
    pub fn spectate(&mut self, name: &str) {
        self.send(ClientMsg::Spectate { username: name.to_string() });
    }

    pub fn leave_match(&mut self, m: &Match) {
        // A game was played: who was played recently has changed.
        self.social.recent_stale = true;
        // A correspondence game waits for the next turn: back to the list.
        if crate::correspondence::is_corr(m) {
            self.tab = Tab::Daily;
            self.send(ClientMsg::CorrList);
            return;
        }
        if m.is_spectating() {
            self.send(ClientMsg::StopSpectating);
            return;
        }
        if m.game.game_over().is_none() {
            self.send(ClientMsg::Resign);
        }
        self.send(ClientMsg::LeaveMatch);
    }

    /// Invitations to and from this player, and the code of their table:
    /// the server drops them when the session ends.
    fn forget_invitations(&mut self) {
        self.incoming.clear();
        self.outgoing = None;
        self.invite_code = None;
    }

    /// What belongs to the account signed in: forgotten when another one
    /// signs in (or no one is): nothing of it is shown to the next one, or
    /// sent as theirs.
    fn forget_account(&mut self) {
        self.forget_invitations();
        self.account = None;
        self.email_offer = None;
        self.offer_password.clear();
        self.wallet = None;
        self.offers = None;
        self.staff_warnings.clear();
        self.staff_shown = None;
        self.profile = None;
        self.friends = FriendsData::default();
        self.unread_friend_requests = false;
        self.corr = Default::default();
        self.social = Default::default();
        self.gifts = Default::default();
        self.achievements.clear();
        self.achievements_of.clear();
        // What they wrote (and what waited for their chat to be on).
        self.pending_text = None;
        self.chat_input.clear();
        self.chat_dialog_open = false;
        self.buying = None;
        self.bought = None;
        self.shop_error = None;
        self.replays = None;
        self.replay = None;
        self.missions = None;
        self.mission_done = false;
        self.xp = None;
        self.daily = None;
        self.daily_started = None;
        self.daily_result = None;
        self.daily_refused = None;
        self.drills = None;
        self.drill_result = None;
        self.drill_session = None;
        self.ladder_claim = None;
        self.ladder_ticket = None;
        self.awaiting_ticket = false;
        self.ticket_refused = false;
        self.ghost = None;
        self.cup_medal_news.clear();
        self.signed_in_as.clear();
        self.renaming_to = None;
        // What was typed for the last one (a table's password among them).
        self.table_password.clear();
        self.tournament_password.clear();
        self.tournament_form = None;
        self.joining = None;
        self.code_input.clear();
        self.friend_input.clear();
        self.table_look = Default::default();
        // The next account checks its own Store purchases.
        self.store_shop.signed_out();
    }

    pub fn logout(&mut self, config: &mut Config) {
        self.send(ClientMsg::Logout);
        self.conn = None;
        self.queue = None;
        self.me = None;
        self.authed = false;
        self.held.borrow_mut().clear();
        self.token.clear();
        self.auto_resume = false;
        self.forget_account();
        self.draining = None;
        config.token.clear();
        config.save();
        self.stage = Stage::Idle;
        // (The server chosen in the settings meanwhile: signed out, it is
        // the one to sign in to; the same one stays on the address in use.)
        if !crate::config::same_server(&self.server, &config.server) {
            self.server = config.server.clone();
            self.fallback = false;
        }
        self.connect();
    }

    /// After a payment: "use this e-mail to recover your account?", with
    /// the account's password to confirm it. Returns true if the settings
    /// changed.
    pub fn draw_email_offer(&mut self, ui: &mut Ui, config: &mut Config) -> bool {
        let Some(email) = self.email_offer.clone() else { return false };
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let w = 480.0f32.min(sw - 32.0);
        let pad = 24.0;
        let inner = w - pad * 2.0;
        let text = tw(
            "Use {email} to recover your account if you ever forget your password?",
            &[("email", &email)],
        );
        let lines = ui.wrap(&text, inner, 15.0);
        let ask = ui.wrap(t("Type your password to confirm it's you."), inner, 13.0);
        let note = ui.wrap(t("You can change it any time in your profile (Account)."), inner, 13.0);
        // Sized to its text: title, question, the password, the note and the
        // buttons. The note goes first when the screen is short.
        let base = pad
            + 30.0
            + 16.0
            + lines.len() as f32 * 20.0
            + 12.0
            + ask.len() as f32 * 17.0
            + 4.0
            + 36.0
            + 24.0
            + 40.0
            + pad;
        let note_h = 8.0 + note.len() as f32 * 17.0;
        let show_note = base + note_h <= sh - 16.0;
        let h = if show_note { base + note_h } else { base };
        let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.55));
        ui.panel(r);
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        let mut y = r.y + pad;
        crate::icons::draw(Icon::Lock, vec2(r.x + pad + 11.0, y + 15.0), 22.0, GOLD);
        ui.text(t("Protect your purchases"), r.x + pad + 32.0, y + 2.0, 24.0, GOLD, true);
        y += 30.0 + 16.0;
        for line in &lines {
            ui.text(line, r.x + pad, y, 15.0, CREAM, false);
            y += 20.0;
        }
        y += 12.0;
        for line in &ask {
            ui.text(line, r.x + pad, y, 13.0, MUTED, false);
            y += 17.0;
        }
        y += 4.0;
        // The field has the keyboard while the offer is up (the screen
        // under it gets none: see main.rs).
        if ui.focus.is_none() {
            ui.focus = Some(EMAIL_OFFER_FIELD);
        }
        let field = Rect::new(r.x + pad, y, inner, 36.0);
        let enter = ui.password_input(EMAIL_OFFER_FIELD, field, &mut self.offer_password, 72);
        if self.offer_password.is_empty() && ui.focus != Some(EMAIL_OFFER_FIELD) {
            let ty = crate::ranking::text_y(y + 18.0, 14.0);
            ui.text(t("Password"), field.x + 10.0, ty, 14.0, MUTED, false);
        }
        y += 36.0;
        if show_note {
            y += 8.0;
            for line in &note {
                ui.text(line, r.x + pad, y, 13.0, MUTED, false);
                y += 17.0;
            }
        }
        // "Use it" bottom right; "No, thanks" (never ask again) bottom left.
        let by = r.y + r.h - pad - 40.0;
        let no = t("No, thanks");
        let yes = t("Use this e-mail");
        let half = (inner - 12.0) / 2.0;
        let no_w = crate::ui::clamp_soft(button_w(ui, no, 40.0), 130.0, half);
        let yes_w = crate::ui::clamp_soft(button_w(ui, yes, 40.0), 170.0, half);
        let mut changed = false;
        let close = |me: &mut Online, ui: &mut Ui| {
            me.email_offer = None;
            me.offer_password.clear();
            if ui.focus == Some(EMAIL_OFFER_FIELD) {
                ui.focus = None;
            }
        };
        if ui.button(Rect::new(r.x + pad, by, no_w, 40.0), no, true) {
            close(self, ui);
            config.email_offer_declined = true;
            changed = true;
            return changed;
        }
        // The server asks for the password in every case.
        let typed = !self.offer_password.is_empty();
        let yes_r = Rect::new(r.x + r.w - pad - yes_w, by, yes_w, 40.0);
        let clicked =
            if typed { ui.primary_button(yes_r, yes) } else { ui.button(yes_r, yes, false) };
        if (clicked || enter) && typed {
            let password = std::mem::take(&mut self.offer_password);
            self.send(ClientMsg::SetRecoveryEmail { email, password });
            close(self, ui);
        } else if is_key_pressed(KeyCode::Escape) || (!typed && ui.dismissed(r)) {
            // Not now (Esc, or a press around it before the password is
            // typed): asked again after the next payment.
            close(self, ui);
        }
        changed
    }

    /// A warning from the staff is waiting to be read.
    pub fn staff_warning_open(&self) -> bool {
        !self.staff_warnings.is_empty()
    }

    /// The oldest warning from the staff, over whatever page is up (not a
    /// game), until the player presses OK. Esc doesn't close it: it must
    /// be read.
    pub fn draw_staff_warning(&mut self, ui: &mut Ui) {
        let Some(text) = self.staff_warnings.first().cloned() else {
            self.staff_shown = None;
            return;
        };
        let now = get_time();
        let shown = *self.staff_shown.get_or_insert(now);
        let more = self.staff_warnings.len() - 1;
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let w = 500.0f32.min(sw - 32.0);
        let pad = 24.0;
        let inner = w - pad * 2.0;
        // Title, the warning (up to 600 characters: smaller type, then cut,
        // if the screen is short) and the button.
        let fixed = pad + 30.0 + 14.0 + 20.0 + 40.0 + pad;
        let room = sh - 16.0 - fixed;
        let mut size = 15.0;
        let mut lines = ui.wrap(&text, inner, size);
        if lines.len() as f32 * (size + 5.0) > room {
            size = 13.0;
            lines = ui.wrap(&text, inner, size);
        }
        let line_h = size + 5.0;
        let fit = ((room / line_h).floor() as usize).max(1);
        if lines.len() > fit {
            lines.truncate(fit);
            if let Some(last) = lines.last_mut() {
                *last = ui.ellipsize(&format!("{last} …"), inner, size);
            }
        }
        let h = fixed + lines.len() as f32 * line_h;
        let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.6));
        ui.panel(r);
        rrect_lines(r, 8.0, 2.0, ALERT);
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        let mut y = r.y + pad;
        let (title, title_size) = ui.fitted(t("Warning from the staff"), 22.0, inner, true);
        ui.text_centered(&title, sw / 2.0, y + 2.0, title_size, ALERT, true);
        y += 30.0 + 14.0;
        for line in &lines {
            ui.text(line, r.x + pad, y, size, CREAM, false);
            y += line_h;
        }
        let label =
            if more > 0 { tw("Next ({n} more)", &[("n", &more)]) } else { t("OK").to_string() };
        let bw = button_w(ui, &label, 40.0).max(140.0).min(inner);
        let b = Rect::new(sw / 2.0 - bw / 2.0, r.y + r.h - pad - 40.0, bw, 40.0);
        let enter = (is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter))
            && now - shown > 1.0;
        if ui.primary_button(b, &label) || enter {
            self.staff_warnings.remove(0);
            // The next one gets its own moment.
            self.staff_shown = None;
        }
    }

    /// Short messages (achievements, declined invitations…), on top of everything.
    pub fn draw_toasts(&self, ui: &Ui, bottom: f32) {
        // In the lounge they stay clear of the chat field at the bottom.
        let bottom = self.toast_floor.take().map_or(bottom, |floor| bottom.min(floor));
        let sw = crate::ui::vw();
        // Long messages wrap (up to three lines) rather than being cut.
        let max_text = (sw - 72.0).min(520.0);
        let mut toasts = Vec::new();
        let mut y = bottom;
        for (text, ttl, good) in self.toasts.iter().rev() {
            let mut lines = ui.wrap(text, max_text, 15.0);
            lines.truncate(3);
            let text_w = lines.iter().map(|l| ui.text_size(l, 15.0, true).x).fold(0.0, f32::max);
            let h = lines.len() as f32 * 20.0 + 16.0;
            y -= h + 8.0;
            let r = Rect::new((sw - text_w - 40.0) / 2.0, y, text_w + 40.0, h);
            toasts.push((r, lines, ttl.min(1.0), *good));
        }
        // Drawn over everything at the end of the frame: they float above the
        // panels and are not held to any panel's edge.
        ui.overlay(move |ui: &Ui| {
            for (r, lines, alpha, good) in toasts {
                let radius = r.h.min(34.0) / 2.0;
                crate::ui::soft_shadow(r, radius, alpha);
                rrect(r, radius, Color::new(0.05, 0.035, 0.025, 0.94 * alpha));
                let edge = if good { GOLD } else { ALERT };
                rrect_lines(r, radius, 1.2, Color::new(edge.r, edge.g, edge.b, 0.75 * alpha));
                let color = Color::new(CREAM.r, CREAM.g, CREAM.b, alpha);
                for (k, line) in lines.iter().enumerate() {
                    let y = r.y + 8.0 + k as f32 * 20.0 + 2.0;
                    ui.text_centered(line, r.x + r.w / 2.0, y, 15.0, color, true);
                }
            }
        });
    }

    /// The pointer is over the invitation cards (as last drawn): what is
    /// drawn before them this frame must not take the click.
    pub fn invite_cards_cover(&self, mouse: Vec2) -> bool {
        self.invite_area.is_some_and(|r| r.contains(mouse))
    }

    /// The invitations waiting, as cards stacked down from `top` in the top
    /// right corner, like the system's notifications: over any screen and
    /// any game, the newest sliding in. Drawn last, over everything. At an
    /// online table they can only be declined; over a game against the
    /// computer, accepting it gives the table to the online game.
    /// Where the next cards may go (under these).
    pub fn draw_invite_cards(&mut self, ui: &mut Ui, top: f32, spot: InviteSpot) -> f32 {
        self.invite_area = None;
        if self.incoming.is_empty() {
            return top;
        }
        let sw = crate::ui::vw();
        let w = 400.0f32.min(sw - 24.0);
        let x = sw - w - 12.0;
        let now = get_time();
        let pad = 14.0;
        let text_w = w - pad * 2.0 - 44.0 - 12.0;
        let mut y = top;
        let mut answer = None;
        let shown = self.incoming.len().min(3);
        for invite in self.incoming.iter().rev().take(shown) {
            // Slides in from the right edge, easing out.
            let kind = if invite.rated { t("a rated game") } else { t("a casual game") };
            let invites = tw("invites you to {kind}", &[("kind", &kind)]);
            let line = match spot {
                InviteSpot::OnlineMatch => format!("{invites} · {}", t("Finish this match first.")),
                InviteSpot::VsComputer { saved: true } => {
                    format!("{invites} · {}", t("this game is saved"))
                }
                _ => invites,
            };
            // Up to two lines, the card as tall as they need.
            let mut lines = ui.wrap(&line, text_w, 13.0);
            lines.truncate(2);
            // The rules of the game offered, in a pill under the text.
            let card_h = 112.0 + 28.0 + (lines.len().max(1) - 1) as f32 * 17.0;
            let k = ((now - invite.at) / 0.28).clamp(0.0, 1.0) as f32;
            let ease = 1.0 - (1.0 - k).powi(3);
            let r = Rect::new(x + (1.0 - ease) * (w + 24.0), y, w, card_h);
            crate::ui::soft_shadow(r, 10.0, 0.9);
            ui.panel(r);
            rrect_lines(r, 8.0, 1.6, GOLD);
            // The gold edge of a notification.
            rrect(Rect::new(r.x + 1.5, r.y + 10.0, 3.0, r.h - 20.0), 1.5, GOLD);
            let player = self.players.iter().find(|p| p.username == invite.from);
            let avatar = player.map(|p| p.avatar.clone()).unwrap_or_default();
            crate::avatars::draw(&avatar, vec2(r.x + pad + 22.0, r.y + pad + 22.0), 22.0);
            let text_x = r.x + pad + 44.0 + 12.0;
            let label = t("Invitation").to_uppercase();
            ui.text(&label, text_x, r.y + 11.0, 11.0, GOLD, true);
            let color = player.map(name_color).unwrap_or(CREAM);
            let country = player.map(|p| p.country.clone()).unwrap_or_default();
            ui.name_with_flag(&invite.from, &country, color, text_x, r.y + 27.0, 16.0, text_w);
            for (n, l) in lines.iter().enumerate() {
                ui.text(l, text_x, r.y + 50.0 + n as f32 * 17.0, 13.0, CREAM, false);
            }
            let pill_y = r.y + 50.0 + lines.len().max(1) as f32 * 17.0 + 5.0;
            let label = crate::i18n::rules_name(invite.rules);
            crate::rulesui::pill(ui, text_x, pill_y, 22.0, invite.rules, label, false);
            // The answers along the bottom, under the text.
            let by = r.y + r.h - pad - 32.0;
            let bw = (text_w - 8.0) / 2.0;
            let decline_r = Rect::new(text_x, by, bw, 32.0);
            let accept_r = Rect::new(text_x + bw + 8.0, by, bw, 32.0);
            if ui.button(decline_r, t("Decline"), true) {
                answer = Some((invite.from.clone(), false));
            }
            if spot == InviteSpot::OnlineMatch {
                ui.button(accept_r, t("Accept"), false);
            } else if ui.primary_button(accept_r, t("Accept")) {
                answer = Some((invite.from.clone(), true));
            }
            ui.block(r);
            y += card_h + 8.0;
        }
        let more = self.incoming.len() - shown;
        if more > 0 {
            let text = tw("{n} more invitations", &[("n", &more)]);
            let tw_ = ui.text_size(&text, 12.0, true).x + 24.0;
            let r = Rect::new(x + w - tw_, y, tw_, 24.0);
            rrect(r, 12.0, Color::new(0.05, 0.035, 0.025, 0.94));
            rrect_lines(r, 12.0, 1.0, crate::ui::BORDER);
            ui.text_centered(&text, r.x + r.w / 2.0, r.y + 5.0, 12.0, CREAM, true);
            ui.block(r);
            y += 24.0 + 8.0;
        }
        self.invite_area = Some(Rect::new(x, top, w, y - top));
        if let Some((from, accept)) = answer {
            self.incoming.retain(|i| i.from != from);
            self.send(ClientMsg::AnswerInvite { from, accept });
        }
        y
    }

    /// Opens the dialog that invites `to`: the game (rated or casual) and
    /// the rules, this player's by default.
    pub fn open_invite(&mut self, to: &str) {
        self.invite_dialog = Some(InviteDialog {
            to: to.to_string(),
            rated: self.rated_games && !self.lan,
            rules: self.rules,
            daily: false,
        });
    }

    /// The same dialog for a daily game (a turn whenever each can).
    pub fn open_daily_challenge(&mut self, to: &str) {
        self.open_invite(to);
        if let Some(d) = self.invite_dialog.as_mut() {
            d.daily = true;
        }
    }

    /// The invitation dialog, over the lounge (`main` draws it last, with
    /// the input of what is under it held).
    pub fn draw_invite_dialog(&mut self, ui: &mut Ui) {
        let Some(mut d) = self.invite_dialog.clone() else { return };
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let w = 560.0f32.min(sw - 24.0);
        let pad = 28.0;
        let inner = w - pad * 2.0;
        // Who: online in the lounge, or the profile shown (a daily game
        // can go to someone away).
        let them = self.players.iter().find(|p| p.username == d.to).cloned().or_else(|| {
            self.profile
                .as_ref()
                .map(|p| p.player.clone())
                .filter(|p| p.username.eq_ignore_ascii_case(&d.to))
        });
        // What the other player plays, when it isn't this player's (a
        // daily game says how it goes instead).
        let theirs = them.as_ref().map(|p| p.rules).filter(|r| *r != self.rules && !d.daily);
        let note = if d.daily {
            Some(
                t("Each of you plays a turn whenever you can: the game can last days.").to_string(),
            )
        } else {
            them.as_ref().filter(|p| p.rules != self.rules).map(|p| {
                tw(
                    "{name} plays the {theirs} now; yours are the {mine}.",
                    &[
                        ("name", &p.username),
                        ("theirs", &crate::i18n::rules_name_inline(p.rules)),
                        ("mine", &crate::i18n::rules_name_inline(self.rules)),
                    ],
                )
            })
        };
        let note_lines = note.as_ref().map_or(Vec::new(), |n| ui.wrap(n, inner - 24.0, 13.0));
        let note_h =
            if note_lines.is_empty() { 0.0 } else { 10.0 + note_lines.len() as f32 * 18.0 };
        let h = pad + 64.0 + 22.0 + 24.0 + 44.0 + 20.0 + 24.0 + 48.0 + note_h + 28.0 + 44.0 + pad;
        let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.55));
        crate::ui::soft_shadow(r, 10.0, 1.0);
        ui.panel(r);
        let x = r.x + pad;
        let mut y = r.y + pad;
        // Who: the picture, the name, and their standing under the rules
        // chosen.
        let avatar = them.as_ref().map(|p| p.avatar.clone()).unwrap_or_default();
        crate::avatars::draw(&avatar, vec2(x + 30.0, y + 30.0), 30.0);
        let title = if d.daily {
            tw("Daily game with {name}", &[("name", &d.to)])
        } else {
            tw("Invite {name}", &[("name", &d.to)])
        };
        let (title, size) = ui.fitted(&title, 24.0, inner - 76.0, true);
        ui.text(&title, x + 76.0, y + 4.0, size, GOLD, true);
        if let Some(p) = &them {
            let mut sx = x + 76.0;
            if crate::flags::draw(&p.country, sx, y + 38.0, 14.0) {
                sx += crate::flags::width(14.0) + 8.0;
            }
            let standing = if p.guest {
                t("guest").to_string()
            } else {
                format!("{} · {}", t(p.tier_in(d.rules).name()), p.rating_in(d.rules))
            };
            ui.text(&standing, sx, y + 37.0, 14.0, MUTED, false);
        }
        y += 64.0 + 22.0;
        // The game: rated or casual.
        ui.text(t("Game type"), x, y, 15.0, CREAM, true);
        y += 24.0;
        let seg = Rect::new(x, y, inner, 44.0);
        if self.lan {
            ui.text(t("LAN games don't change ratings."), x, y + 14.0, 14.0, MUTED, false);
        } else if let Some(i) = ui.segmented(seg, &[t("Rated"), t("Casual")], !d.rated as usize) {
            d.rated = i == 0;
        }
        y += 44.0 + 20.0;
        // The rules.
        ui.text(t("Rules"), x, y, 15.0, CREAM, true);
        y += 24.0;
        if let Some(rules) = crate::rulesui::switch(ui, Rect::new(x, y, inner, 48.0), d.rules, None)
        {
            d.rules = rules;
        }
        y += 48.0;
        if let Some(theirs) = theirs.filter(|_| !note_lines.is_empty()) {
            y += 10.0;
            crate::rulesui::mark(theirs, vec2(x + 8.0, y + 9.0), 15.0);
            let color = crate::rulesui::accent(theirs);
            for (k, line) in note_lines.iter().enumerate() {
                ui.text(line, x + 24.0, y + k as f32 * 18.0, 13.0, color, false);
            }
        } else if d.daily {
            y += 10.0;
            crate::icons::draw(Icon::Hourglass, vec2(x + 8.0, y + 9.0), 15.0, MUTED);
            for (k, line) in note_lines.iter().enumerate() {
                ui.text(line, x + 24.0, y + k as f32 * 18.0, 13.0, MUTED, false);
            }
        }
        // Cancel, and Invite (Enter).
        let by = r.y + r.h - pad - 44.0;
        let cw = (inner * 0.36).min(200.0);
        let cancel = Rect::new(x, by, cw, 44.0);
        let send = Rect::new(x + cw + 16.0, by, inner - cw - 16.0, 44.0);
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        let enter = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
        if ui.button(cancel, t("Cancel"), true)
            || is_key_pressed(KeyCode::Escape)
            || ui.dismissed(r)
        {
            self.invite_dialog = None;
            return;
        }
        let label = if d.daily { t("Send the challenge") } else { t("Invite") };
        if ui.primary_button(send, label) || enter {
            self.invite_dialog = None;
            let rated = d.rated && !self.lan;
            if d.daily {
                self.send(ClientMsg::CorrChallenge { to: d.to, rated, rules: d.rules });
            } else {
                self.send(ClientMsg::Invite { to: d.to, rated, rules: d.rules });
            }
            return;
        }
        self.invite_dialog = Some(d);
    }

    /// The cards are not shown this frame (focus mode, the settings).
    pub fn hide_invite_cards(&mut self) {
        self.invite_area = None;
    }

    /// The sign-in form, a waiting message or the lounge.
    pub fn draw(&mut self, ui: &mut Ui, config: &mut Config) -> OnlineAction {
        if self.stage != Stage::SignIn {
            self.form_focused = false;
        }
        match self.stage.clone() {
            Stage::Lounge => self.draw_lounge(ui, config),
            Stage::Reconnecting { attempt, .. } => {
                let text =
                    tw("Connection lost. Reconnecting (attempt {n})…", &[("n", &(attempt + 1))]);
                self.waiting(ui, &text)
            }
            // Joining a game on the local network failed: say why, and offer
            // to try again (it used to look like a join that never ended).
            Stage::Idle if self.lan && self.error.is_some() => self.draw_join_failed(ui),
            Stage::Connecting | Stage::Idle => {
                let text = if self.lan {
                    t("Joining the LAN game…").to_string()
                } else {
                    t("Connecting to the server…").to_string()
                };
                let hint = (!self.lan).then(|| {
                    t("The first connection of the day can take up to a minute while the server wakes up.")
                });
                self.waiting_hint(ui, &text, hint)
            }
            Stage::Authenticating => self.waiting(ui, t("Signing in…")),
            Stage::SignIn => self.draw_sign_in(ui, config),
        }
    }

    fn draw_join_failed(&mut self, ui: &mut Ui) -> OnlineAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let w = 480.0f32.min(sw - 32.0);
        let reason = self.error.clone().unwrap_or_default();
        let lines = balanced_wrap(ui, &reason, w - 48.0, 14.0);
        let h = 24.0 + 22.0 + 14.0 + lines.len() as f32 * 19.0 + 24.0 + 36.0 + 24.0;
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        let mut y = r.y + 24.0;
        ui.text_centered(t("Could not join the LAN game"), sw / 2.0, y, 17.0, GOLD, true);
        y += 22.0 + 14.0;
        for line in &lines {
            ui.text_centered(line, sw / 2.0, y, 14.0, CREAM, false);
            y += 19.0;
        }
        let by = r.y + r.h - 24.0 - 36.0;
        let bw = ((w - 48.0 - 12.0) / 2.0).min(180.0);
        let back = Rect::new(sw / 2.0 - bw - 6.0, by, bw, 36.0);
        let again = Rect::new(sw / 2.0 + 6.0, by, bw, 36.0);
        if ui.button(back, t("Back"), true) || ui.escape() {
            self.error = None;
            self.conn = None;
            return OnlineAction::Leave;
        }
        let enter = (is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter))
            && ui.focus.is_none();
        if ui.primary_button(again, t("Try again")) || enter {
            self.error = None;
            self.auto_resume = true;
            self.connect();
        }
        OnlineAction::None
    }

    fn waiting(&mut self, ui: &mut Ui, text: &str) -> OnlineAction {
        self.waiting_hint(ui, text, None)
    }

    fn waiting_hint(&mut self, ui: &mut Ui, text: &str, hint: Option<&str>) -> OnlineAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let w = 480.0f32.min(sw - 32.0);
        let hint = hint.map(|h| balanced_wrap(ui, h, w - 48.0, 13.0)).unwrap_or_default();
        // Sized to its content: message, spinner, hint and the button.
        let hint_h = if hint.is_empty() { 0.0 } else { hint.len() as f32 * 17.0 + 12.0 };
        let h = 24.0 + 20.0 + 16.0 + 36.0 + 16.0 + hint_h + 34.0 + 24.0;
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        let mut y = r.y + 24.0;
        ui.text_centered(text, sw / 2.0, y, 17.0, CREAM, true);
        y += 20.0 + 16.0;
        crate::ui::spinner(vec2(sw / 2.0, y + 18.0), 14.0, GOLD);
        y += 36.0 + 16.0;
        for line in &hint {
            ui.text_centered(line, sw / 2.0, y, 13.0, MUTED, false);
            y += 17.0;
        }
        let back = Rect::new(sw / 2.0 - 75.0, r.y + r.h - 24.0 - 34.0, 150.0, 34.0);
        if ui.button(back, t("Back"), true) || ui.escape() {
            if !self.signed_in() {
                self.conn = None;
                self.queue = None;
                self.stage = Stage::Idle;
            }
            return OnlineAction::Leave;
        }
        OnlineAction::None
    }

    fn draw_sign_in(&mut self, ui: &mut Ui, config: &mut Config) -> OnlineAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        // The country list is a dialog of its own: while it is open it takes
        // Esc and the keyboard.
        let picker_open = crate::countries::is_open();
        let pad = 24.0;
        let w = 460.0f32.min(sw - 32.0);
        let inner = w - pad * 2.0;
        let connected = self.conn.is_some();
        let outdated = self.outdated && self.error.is_some();
        // Over the button: why the last try failed, or that a new version is needed.
        let (message, message_color) = match &self.error {
            Some(_) if outdated => {
                let text = t("A new version of the game is out: update to play online.");
                (ui.wrap(text, inner, 14.0), GOLD)
            }
            Some(e) => (ui.wrap(e, inner, 14.0).into_iter().take(3).collect(), ALERT),
            None => (Vec::new(), ALERT),
        };
        let pass_hint = if self.register {
            ui.wrap(
                t("At least 8 characters. Use a password you don't use elsewhere."),
                inner,
                12.0,
            )
        } else {
            Vec::new()
        };
        let welcome = if !self.register && message.is_empty() {
            ui.wrap(t("New here? Choose “New account”: it takes ten seconds."), inner, 13.0)
        } else {
            Vec::new()
        };
        // The panel fits its content: header, choice, fields, message, button, hint.
        const FIELD: f32 = 20.0 + 36.0 + 16.0;
        let mut h = pad + 36.0 + 20.0 + 36.0 + 20.0 + FIELD * 2.0;
        if self.register {
            h += pass_hint.len() as f32 * 16.0 + 4.0 + FIELD;
            // The terms under the button.
            h += 14.0 + 17.0 + 20.0 - 4.0;
        }
        if !message.is_empty() {
            h += message.len() as f32 * 19.0 + 8.0;
        }
        h += 4.0 + 44.0;
        if !welcome.is_empty() {
            h += 14.0 + welcome.len() as f32 * 17.0 - 4.0;
        }
        h += pad;
        let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
        ui.panel(r);
        let x = r.x + pad;

        // Header: back, and the title with its icon.
        let mut y = r.y + pad;
        let back = ui.icon_button(Rect::new(x, y, 36.0, 36.0), Icon::Back, t("Back"), true);
        // Esc leaves the form, also from its fields (they have the keyboard
        // from the start), never from under a dialog.
        let modal = ui.focus.is_some_and(|f| f >= crate::ui::MODAL_FOCUS);
        if back || (is_key_pressed(KeyCode::Escape) && !picker_open && !modal) {
            return OnlineAction::Leave;
        }
        crate::icons::draw(Icon::Globe, vec2(x + 36.0 + 16.0 + 11.0, y + 18.0), 22.0, GOLD);
        ui.text(t("Play online"), x + 36.0 + 16.0 + 30.0, y + 6.0, 24.0, GOLD, true);
        y += 36.0 + 20.0;

        let tabs = [t("I have an account"), t("New account")];
        if let Some(i) = ui.segmented(Rect::new(x, y, inner, 36.0), &tabs, self.register as usize) {
            self.register = i == 1;
            self.error = None;
            self.form_focused = false;
        }
        y += 36.0 + 20.0;

        // The cursor starts in the first empty field (not on the frame of a
        // click: that click would take it away again).
        if !self.form_focused && !picker_open && !is_mouse_button_pressed(MouseButton::Left) {
            ui.focus = Some(if self.username.is_empty() { 20 } else { 21 });
            self.form_focused = true;
        }
        ui.text(t("Username"), x, y, 13.0, MUTED, true);
        let field = Rect::new(x, y + 20.0, inner, 36.0);
        let user_enter = ui.text_input(20, field, &mut self.username, 16);
        // Enter in the name goes on to the password while that is empty.
        let to_password = user_enter && self.password.is_empty();
        if to_password {
            ui.focus = Some(21);
        }
        y += FIELD;
        ui.text(t("Password"), x, y, 13.0, MUTED, true);
        let field = Rect::new(x, y + 20.0, inner, 36.0);
        let pass_enter = ui.password_input(21, field, &mut self.password, 72);
        // (The Enter that moved the cursor here is not a submission.)
        let submit = (user_enter || pass_enter) && !to_password;
        let mut just_opened = false;
        if self.register {
            y += 20.0 + 36.0 + 6.0;
            for line in &pass_hint {
                ui.text(line, x, y, 12.0, MUTED, false);
                y += 16.0;
            }
            y += 14.0;
            ui.text(t("Country"), x, y, 13.0, MUTED, true);
            let label = if self.country.is_empty() {
                t("Choose…").to_string()
            } else {
                crate::flags::name(&self.country).to_string()
            };
            let field = Rect::new(x, y + 20.0, inner, 36.0);
            if crate::countries::field(ui, field, &self.country, &label) {
                crate::countries::open();
                just_opened = true;
            }
        }
        y += FIELD + 4.0;
        if !message.is_empty() {
            for line in &message {
                ui.text_centered(line, sw / 2.0, y, 14.0, message_color, outdated);
                y += 19.0;
            }
            y += 8.0;
        }

        // The one action: sign in, create the account, try again or update.
        let update = update_label();
        let label = if outdated {
            update.as_str()
        } else if !connected {
            t("Retry")
        } else if self.register {
            t("Create account")
        } else {
            t("Sign in")
        };
        let clicked = ui.primary_button(Rect::new(x, y, inner, 44.0), label);
        y += 44.0 + 14.0;
        if self.register {
            let lead = t("Creating an account means accepting the");
            ui.text_centered(lead, sw / 2.0, y, 12.0, MUTED, false);
            y += 17.0;
            crate::legal::links(ui, &config.server, sw / 2.0, y, 13.0);
            y += 20.0;
        }
        for line in &welcome {
            ui.text_centered(line, sw / 2.0, y, 13.0, MUTED, false);
            y += 17.0;
        }
        if outdated {
            // Trying again can't help: the new version can.
            if clicked {
                self.quit_for_update = crate::update::start();
            }
        } else if clicked || submit {
            let name_ok = valid_username(self.username.trim());
            // The rules for a new password are only for new accounts: one
            // made under older rules (shorter in letters, but not in bytes)
            // still signs in.
            let pass_ok = if self.register {
                valid_password(&self.password)
            } else {
                !self.password.is_empty()
                    && self.password.len() <= snook_legend_proto::MAX_PASSWORD_BYTES
            };
            if !connected {
                if !crate::config::same_server(&self.server, &config.server) {
                    // Another server: the session of the last one stays home.
                    self.token.clear();
                    self.auto_resume = false;
                    self.server = config.server.clone();
                    self.fallback = false;
                }
                self.connect();
                self.error = None;
            } else if !name_ok {
                self.error = Some(t("Usernames have 3–16 letters, digits or underscores.").into());
            } else if !pass_ok && !self.register {
                self.error = Some(t("Wrong username or password.").into());
            } else if !pass_ok {
                self.error =
                    Some(t("Passwords need at least 8 characters (not only digits).").into());
            } else {
                let username = self.username.trim().to_string();
                let password = self.password.clone();
                self.send(if self.register {
                    ClientMsg::Register {
                        username,
                        password,
                        country: self.country.clone(),
                        // Asked when the player first writes to others.
                        minor: None,
                    }
                } else {
                    ClientMsg::Login { username, password }
                });
                self.auto_resume = false;
                self.error = None;
                self.stage = Stage::Authenticating;
            }
        }
        // Not on the frame it opens: a quick click (pressed and released in
        // one frame) would also pick the country under the pointer.
        if !just_opened && let Some(code) = crate::countries::draw_picker(ui) {
            self.country = code;
        }
        OnlineAction::None
    }

    fn draw_lounge(&mut self, ui: &mut Ui, config: &mut Config) -> OnlineAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        // Esc steps back: out of a form or a tournament, then (online) to the
        // menu. Not while typing (there Esc leaves the field), nor under a dialog.
        let esc = ui.escape()
            && self.email_offer.is_none()
            && !crate::countries::is_open()
            && !self.chat_dialog_open
            && self.invite_dialog.is_none();
        // A dark wash over the slowly turning table: the room is the backdrop.
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.42));
        let mut action = OnlineAction::None;
        if esc {
            if self.tournament_form.is_some() {
                self.tournament_form = None;
            } else if self.selected_tournament.is_some() {
                self.selected_tournament = None;
            } else if self.joining.is_some() {
                self.joining = None;
            } else if self.social.composing.is_some() {
                self.social.composing = None;
            } else if !self.lan {
                // (Leaving a LAN lounge closes the game: only on purpose.)
                action = OnlineAction::Leave;
            }
        }
        let margin = 14.0;
        // Inside the lounge's panels: tabs, lists and fields keep this inset.
        let pad = 12.0;

        // Header: back and the game's name on the left; who is here, the
        // player, the coins, the ranking and signing out on the right. Every
        // control is 36 high, centred, 8 apart.
        let header = Rect::new(margin, margin, sw - margin * 2.0, 56.0);
        ui.panel(header);
        let by = header.y + 10.0;
        let gap = 8.0;
        // The host leaving closes the game for everyone in it: a second click
        // (within a few seconds) to be sure.
        let others = self.players.len().saturating_sub(1);
        let hosting = self.lan && self.host_address.is_some() && others > 0;
        let armed = self.leave_armed.is_some_and(|at| get_time() - at < 4.0);
        let back_r = Rect::new(header.x + 10.0, by, 36.0, 36.0);
        let back_tip = if hosting && armed {
            tw("Click again to close the game: {n} will be disconnected", &[("n", &others)])
        } else {
            t("Main menu").to_string()
        };
        if ui.icon_button(back_r, Icon::Back, &back_tip, true) {
            if hosting && !armed {
                self.leave_armed = Some(get_time());
            } else {
                self.leave_armed = None;
                action = OnlineAction::Leave;
            }
        }
        if hosting && armed {
            crate::ui::rrect_lines(back_r, 6.0, 1.6, crate::ui::ALERT);
            ui.tooltip(&back_tip, back_r);
        }
        crate::icon::draw(header.x + 10.0 + 36.0 + 10.0, by, 36.0);
        // The wordmark, its capitals as tall as a 24-point title's.
        let title_x = header.x + 10.0 + 36.0 + 10.0 + 36.0 + 10.0;
        let title_w =
            crate::menu::draw_wordmark(title_x, header.y + 15.0 + 24.0 * 0.073, 24.0 * 0.727);
        // The host of a game on the local network sees where others join
        // (when they don't find it on their own: a firewall, isolated
        // Wi-Fi); a click copies it.
        let subtitle = match (&self.host_address, self.lan) {
            (Some(address), true) => {
                tw("LAN game · others join at {address}", &[("address", address)])
            }
            (None, true) => t("LAN game").to_string(),
            _ => t("lounge").to_string(),
        };
        // On the title's baseline.
        let sub_w = ui.text_size(&subtitle, 15.0, false).x;
        ui.text(&subtitle, title_x + title_w + 10.0, header.y + 22.0, 15.0, MUTED, false);
        if let Some(address) = self.host_address.clone().filter(|_| self.lan) {
            let r = Rect::new(title_x + title_w + 10.0, header.y + 16.0, sub_w, 24.0);
            if ui.hovering(r) {
                ui.tooltip(t("Click to copy"), r);
            }
            if ui.clicked(r) {
                macroquad::miniquad::window::clipboard_set(&address);
                self.toast(t("Copied."), true);
            }
        }
        // The rules this player plays, beside the title: a click switches
        // them ("Play now", tables and invitations follow).
        let wide = header.w > 1150.0;
        let label = if wide {
            crate::i18n::rules_name(self.rules)
        } else {
            crate::i18n::rules_short(self.rules)
        };
        let pill_x = title_x + title_w + 10.0 + sub_w + 16.0;
        let (pill_r, switch) =
            crate::rulesui::pill(ui, pill_x, by + 2.0, 32.0, self.rules, label, true);
        if ui.hovering(pill_r) {
            ui.tooltip(t("The rules of your games: click to switch"), pill_r);
        }
        if switch {
            config.set_rules(self.rules.other());
            self.rules = config.rules();
        }
        let title_end = pill_r.x + pill_r.w;
        // The ping, when asked for, beside the count.
        let ping = match self.rtt.filter(|_| config.show_ping) {
            Some(rtt) => tw("ping {ms} ms", &[("ms", &rtt.as_millis())]),
            None => String::new(),
        };
        let online_w = presence_width(ui, self.players.len(), 14.0, &ping);
        let mut right = header.x + header.w - 10.0;
        if !self.lan {
            // (Signing out is seldom wanted: it is in the account's dialog,
            // on one's own profile.)
            right -= 36.0;
            if ui.icon_button(
                Rect::new(right, by, 36.0, 36.0),
                Icon::Trophy,
                t("World ranking"),
                true,
            ) {
                action = OnlineAction::Ranking;
            }
            // The coins, which open the shop.
            let coins = self.wallet.as_ref().map(|w| w.coins).unwrap_or(0).to_string();
            let cw = ui.text_size(&coins, 15.0, true).x + 46.0;
            right -= gap + cw;
            let coins_r = Rect::new(right, by, cw, 36.0);
            if header_chip(ui, coins_r, true) {
                ui.tooltip(t("Shop"), coins_r);
                if ui.clicked(coins_r) {
                    action = OnlineAction::Shop;
                }
            }
            crate::icons::draw(Icon::Coin, vec2(coins_r.x + 18.0, coins_r.y + 18.0), 18.0, GOLD);
            ui.text(&coins, coins_r.x + 32.0, coins_r.y + 10.0, 15.0, CREAM, true);
        }
        // Who am I: as wide as the name needs, never into the title.
        if let Some(me) = self.me.clone() {
            let sub = if me.guest {
                t("guest").to_string()
            } else {
                format!("{} · {}", me.rating_in(self.rules), t(me.tier_in(self.rules).name()))
            };
            let content = name_width(ui, &me, 14.0).max(ui.text_size(&sub, 12.0, false).x);
            let room = right - gap - (title_end + 24.0 + online_w + 26.0);
            let card_w = (content + 24.0).min(room).clamp(110.0, 280.0);
            right -= gap + card_w;
            let card = Rect::new(right, by, card_w, 36.0);
            if header_chip(ui, card, !self.lan) {
                ui.tooltip(t("Your profile"), card);
                if ui.clicked(card) {
                    action = OnlineAction::Profile(me.username.clone());
                }
            }
            ui.player_name(&me, card.x + 12.0, card.y + 3.0, 14.0, card.w - 24.0);
            ui.text(&sub, card.x + 12.0, card.y + 20.0, 12.0, MUTED, false);
            // Presents waiting: a dot (the profile shows them).
            if self.gift_badge() {
                draw_circle(card.x + card.w - 7.0, card.y + 7.0, 4.5, ALERT);
            }
        }
        // How many are here: a green "live" dot, a person and the number.
        self.draw_presence(ui, right - 16.0, header.y + 28.0, 14.0, &ping, 0.0, MUTED);

        let body_y = header.y + header.h + 10.0;
        let body_h = sh - body_y - margin;
        let left_w = (sw * 0.46).clamp(330.0, 560.0);
        let left = Rect::new(margin, body_y, left_w, body_h);
        let right_panel =
            Rect::new(margin + left_w + 10.0, body_y, sw - left_w - margin * 2.0 - 10.0, body_h);

        // Left: tabs.
        ui.panel(left);
        let mut tabs = vec![
            (Tab::Players, t("Players")),
            (Tab::Tables, t("Tables")),
            (Tab::Tournaments, t("Tournaments")),
        ];
        if !self.lan {
            tabs.push((Tab::Friends, t("Friends")));
            tabs.push((Tab::Daily, t("Daily")));
        }
        // Each tab as wide as its label, all labels at one size.
        let me = self.me.as_ref().map(|m| m.username.as_str()).unwrap_or("");
        let waiting = self.corr.waiting(me);
        let labels: Vec<&str> = tabs.iter().map(|(_, l)| *l).collect();
        let counts: Vec<String> = tabs
            .iter()
            .map(|(tab, _)| match tab {
                Tab::Tables if !self.tables.is_empty() => self.tables.len().to_string(),
                _ => String::new(),
            })
            .collect();
        let counts: Vec<&str> = counts.iter().map(String::as_str).collect();
        let picked = tabs.iter().position(|(tab, _)| *tab == self.tab).unwrap_or(0);
        let row = Rect::new(left.x + pad, left.y + pad, left.w - pad * 2.0, 34.0);
        let (clicked, rects) = crate::tabrow::tabs(ui, row, &labels, &counts, picked, 6.0);
        if let Some(k) = clicked {
            let tab = tabs[k].0;
            self.tab = tab;
            self.list_first = 0;
            if tab == Tab::Friends {
                self.send(ClientMsg::Friends);
                self.unread_friend_requests = false;
                if self.social.recent_view {
                    self.request_recent();
                }
            }
            if tab == Tab::Daily {
                self.send(ClientMsg::CorrList);
            }
        }
        for ((tab, _), r) in tabs.iter().zip(&rects) {
            let badge = match tab {
                Tab::Friends => self.unread_friend_requests || !self.friends.incoming.is_empty(),
                Tab::Daily => waiting > 0,
                _ => false,
            };
            if badge {
                draw_circle(r.x + r.w - 9.0, r.y + 9.0, 4.5, ALERT);
            }
        }
        let reserve = if self.outgoing.is_some() { 48.0 + 12.0 } else { 0.0 };
        let top = left.y + pad + 34.0 + 14.0;
        let area =
            Rect::new(left.x + pad, top, left.w - pad * 2.0, left.y + left.h - pad - top - reserve);
        match self.tab {
            Tab::Players => {
                if let Some(a) = self.draw_players(ui, area) {
                    action = a;
                }
            }
            Tab::Tables => self.draw_tables(ui, area),
            Tab::Tournaments => self.draw_tournaments(ui, area),
            Tab::Friends => {
                if let Some(a) = self.draw_friends(ui, area) {
                    action = a;
                }
            }
            Tab::Daily => {
                if let Some(a) = crate::correspondence::draw_tab(self, ui, area) {
                    action = a;
                }
            }
        }

        // Outgoing invitation.
        if let Some((to, rated, rules)) = self.outgoing.clone() {
            let r = Rect::new(left.x + pad, left.y + left.h - pad - 48.0, left.w - pad * 2.0, 48.0);
            rrect(r, 8.0, Color::new(0.18, 0.12, 0.06, 0.95));
            rrect_lines(r, 8.0, 1.5, GOLD);
            let cancel = t("Cancel");
            let bw = button_w(ui, cancel, 32.0).max(90.0);
            crate::ui::spinner(vec2(r.x + 22.0, r.y + 24.0), 7.0, GOLD);
            let kind = if rated { t("rated") } else { t("casual") };
            let kind = format!("{kind} · {}", crate::i18n::rules_name_inline(rules));
            let text = tw("Waiting for {to} ({kind})…", &[("to", &to), ("kind", &kind)]);
            let (text, size) = fit_text(ui, &text, 15.0, r.w - 40.0 - bw - 24.0, false);
            ui.text(&text, r.x + 40.0, r.y + (r.h - size) / 2.0, size, CREAM, false);
            let b = Rect::new(r.x + r.w - 8.0 - bw, r.y + 8.0, bw, 32.0);
            if ui.button(b, cancel, true) {
                self.send(ClientMsg::CancelInvite { to });
                self.outgoing = None;
            }
        }

        // Right: the chat, the missions and the board.
        ui.panel(right_panel);
        let inner_x = right_panel.x + pad;
        let inner_w = right_panel.w - pad * 2.0;
        let bottom = right_panel.y + right_panel.h - pad;
        if self.lan {
            let y = right_panel.y + pad + (34.0 - 15.0) / 2.0;
            ui.text(t("Lounge chat"), inner_x + 2.0, y, 15.0, GOLD, true);
        } else {
            self.side_tabs(ui, right_panel, config);
        }
        let mut log_top = right_panel.y + pad + 34.0 + 14.0;
        if !self.motd.is_empty() {
            let lines = ui.wrap(&self.motd, inner_w - 20.0, 13.0);
            let lines = &lines[..lines.len().min(3)];
            let mh = lines.len() as f32 * 17.0 + 14.0;
            let mr = Rect::new(inner_x, log_top, inner_w, mh);
            rrect(mr, 6.0, Color::new(0.91, 0.75, 0.38, 0.08));
            for (k, l) in lines.iter().enumerate() {
                let color = Color::new(1.0, 0.9, 0.7, 0.9);
                ui.text(l, mr.x + 10.0, mr.y + 7.0 + k as f32 * 17.0, 13.0, color, false);
            }
            log_top += mh + 8.0;
        }
        // The server is about to restart (drawn under the events).
        // (Gone once the time is up: the next session says it again if the
        // server is still on its way out.)
        let restart =
            self.draining.filter(|until| *until > std::time::Instant::now()).map(|until| {
                let left = until.saturating_duration_since(std::time::Instant::now()).as_secs();
                let text = tw(
                    "The server restarts in {m}:{s} for an update. Games in progress go on.",
                    &[("m", &(left / 60)), ("s", &format!("{:02}", left % 60))],
                );
                ui.wrap(&text, inner_w - 20.0, 13.0)
            });
        // What goes under the events: the restart notice, the featured game
        // and the chat's field. An event card is left out when the chat (or
        // the missions, the board) would keep less than this.
        const MIN_AREA: f32 = 100.0;
        let chat = self.side == Side::Chat;
        let mut below = restart.as_ref().map_or(0.0, |l| l.len() as f32 * 17.0 + 14.0 + 8.0);
        if chat {
            below += 36.0 + 10.0;
            if self.featured.is_some() && !self.lan {
                below += 58.0 + 8.0;
            }
        }
        // The staff's events running and coming.
        let now = crate::game::unix_now();
        for e in self.events_now().iter().take(2) {
            let r = Rect::new(inner_x, log_top, inner_w, event_height(ui, e, inner_w));
            if bottom - (r.y + r.h + 8.0) - below < MIN_AREA {
                break;
            }
            draw_event(ui, e, r, now);
            log_top += r.h + 8.0;
        }
        if let Some(lines) = restart {
            let r = Rect::new(inner_x, log_top, inner_w, lines.len() as f32 * 17.0 + 14.0);
            rrect(r, 6.0, Color::new(1.0, 0.56, 0.44, 0.14));
            rrect_lines(r, 6.0, 1.0, ALERT);
            for (k, l) in lines.iter().enumerate() {
                ui.text(l, r.x + 10.0, r.y + 7.0 + k as f32 * 17.0, 13.0, CREAM, true);
            }
            log_top += r.h + 8.0;
        }
        if self.side != Side::Chat {
            let area = Rect::new(inner_x, log_top, inner_w, bottom - log_top);
            match self.side {
                Side::Missions => self.draw_missions(ui, area),
                _ => {
                    if let Some(a) = self.draw_board(ui, area) {
                        action = a;
                    }
                }
            }
            return action;
        }
        // The featured game: anyone can watch it. One of this player's own,
        // seen from the lounge, is a game they just left (the server's word
        // that the TV changed may still be on its way): not offered.
        let mine = |f: &snook_legend_proto::FeaturedMatch, me: &Option<PlayerInfo>| {
            me.as_ref().is_some_and(|m| f.players.iter().any(|p| p.username == m.username))
        };
        if let Some(f) = self.featured.clone().filter(|f| !mine(f, &self.me))
            && !self.lan
        {
            let r = Rect::new(inner_x, log_top, inner_w, 58.0);
            rrect(r, 8.0, Color::new(0.0, 0.0, 0.0, 0.3));
            rrect_lines(r, 8.0, 1.0, BORDER);
            let mine = self
                .me
                .as_ref()
                .is_some_and(|m| f.players.iter().any(|p| p.username == m.username));
            let watch = t("Watch");
            let bw = if mine { 0.0 } else { button_w(ui, watch, 32.0).max(84.0) };
            let live = 0.5 + 0.5 * (get_time() as f32 * 2.5).sin();
            draw_circle(r.x + 16.0, r.y + 16.0, 4.0, Color::new(1.0, 0.3, 0.25, 0.5 + 0.5 * live));
            let on_tv = t("ON TV");
            ui.text(on_tv, r.x + 26.0, r.y + 10.0, 11.0, ALERT, true);
            let avg = (f.players[0].rating_in(f.rules) + f.players[1].rating_in(f.rules)) / 2;
            let sub = tw("{n} watching · avg {r}", &[("n", &f.spectators), ("r", &avg)]);
            let sub = format!("{sub} · {}", crate::i18n::rules_name_inline(f.rules));
            // After the label, however long it is in this language ("EN LA TELE").
            let sub_x = r.x + 26.0 + ui.text_size(on_tv, 11.0, true).x + 10.0;
            let (sub, sub_size) = fit_text(ui, &sub, 12.0, r.x + r.w - bw - 24.0 - sub_x, false);
            ui.text(&sub, sub_x, r.y + 9.5, sub_size, MUTED, false);
            // "Ana vs Bruno", each name as long as it needs.
            // The versus mark, set like a scoreboard's ("VS", "X"), so it
            // reads as a separator and not a stray letter.
            let vs = t("vs").to_uppercase();
            let vs_w = ui.text_size(&vs, 12.0, true).x;
            let names_w = r.w - bw - 24.0 - 12.0 - vs_w - 20.0;
            let a = name_width(ui, &f.players[0], 14.0).min(names_w / 2.0);
            let used = ui.player_name(&f.players[0], r.x + 12.0, r.y + 31.0, 14.0, a);
            let vx = r.x + 12.0 + used + 10.0;
            ui.text(&vs, vx, r.y + 32.0, 12.0, GOLD, true);
            let bx = vx + vs_w + 10.0;
            ui.player_name(&f.players[1], bx, r.y + 31.0, 14.0, names_w - used);
            if !mine
                && ui.button(Rect::new(r.x + r.w - 12.0 - bw, r.y + 13.0, bw, 32.0), watch, true)
            {
                self.send(ClientMsg::WatchMatch { match_id: f.match_id });
            }
            log_top += r.h + 8.0;
        }
        // The field grows with what is typed (up to four lines).
        let input_h = 36.0;
        let field_w = inner_w - 2.0 * (input_h + 6.0);
        let field_h = ui.text_area_height(&self.chat_input, field_w, 4).max(input_h);
        let input = Rect::new(inner_x, bottom - field_h, field_w, field_h);
        let log = Rect::new(inner_x, log_top, inner_w, input.y - 10.0 - log_top);
        // Toasts float above the chat field, not over it.
        self.toast_floor.set(Some(input.y - 10.0));
        rrect(log, 6.0, Color::new(0.0, 0.0, 0.0, 0.25));
        let me = self.me.as_ref().map(|m| m.username.clone());
        let admin = self.me.as_ref().is_some_and(|m| m.role == snook_legend_proto::Role::Admin);
        if self.chat.is_empty() {
            ui.block(log);
            if self.chat_off() {
                // What players write shows once the chat is on.
                let (text, label) = if self.age.is_some_and(|a| a.minor == Some(true)) {
                    (
                        t("Chat off: under 18. A parent or guardian can turn it on."),
                        t("For a parent or guardian…"),
                    )
                } else {
                    (
                        t("The chat with other players is off until you turn it on."),
                        t("Turn the chat on"),
                    )
                };
                if empty_state(ui, log, Glyph::Chat, text, Some(label)) {
                    self.open_chat_dialog();
                }
            } else {
                empty_state(ui, log, Glyph::Chat, t("No messages yet."), None);
            }
        } else {
            let (scroll, act) =
                ui.chat_log(LOUNGE_LOG, log, &self.chat, me.as_deref(), admin, self.chat_scroll);
            self.chat_scroll = scroll;
            if let Some(crate::ui::ChatAction::Delete { id, silent }) = act {
                self.send(ClientMsg::DeleteChat { id, silent });
            }
        }
        // Administrators: clear the chat (a second click confirms).
        if admin && !self.lan {
            let now = get_time();
            let armed = self.clear_armed.is_some_and(|t| now - t < 4.0);
            let label = if armed { t("Click again to clear") } else { t("Clear the chat") };
            let w = ui.text_size(label, 13.0, true).x + 20.0;
            let b = Rect::new(log.x + log.w - w - 6.0, log.y + 6.0, w, 26.0);
            if ui.button(b, label, true) {
                if armed {
                    self.send(ClientMsg::ClearChat);
                    self.clear_armed = None;
                } else {
                    self.clear_armed = Some(now);
                }
            }
        }
        // Under 18 with the chat a parent or guardian turned on: where they
        // can turn it off again.
        if self.age.is_some_and(|a| a.minor == Some(true) && a.chat_allowed) && !self.lan {
            let label = t("For a parent or guardian…");
            let w = ui.text_size(label, 13.0, true).x + 20.0;
            let b = Rect::new(log.x + log.w - w - 6.0, log.y + 6.0, w, 26.0);
            if ui.button(b, label, true) {
                self.open_chat_dialog();
            }
        }
        let sent = ui.text_area(30, input, &mut self.chat_input, MAX_CHAT_LEN);
        crate::ui::chat_counter(ui, &self.chat_input, input);
        self.emoji.track(ui, 30);
        let emoji_r =
            Rect::new(input.x + input.w + 6.0, input.y + input.h - input_h, input_h, input_h);
        let chat_area = Rect::new(inner_x, log.y, inner_w, input.y + input.h - log.y);
        self.emoji.draw(ui, emoji_r, input, chat_area, 30, &mut self.chat_input, MAX_CHAT_LEN);
        let hint = if self.chat_off() {
            t("Write to turn the chat on…")
        } else {
            t("Say hello… (Enter to send)")
        };
        placeholder(ui, 30, input, &self.chat_input, hint);
        let send_r = Rect::new(
            input.x + input.w + 6.0 + input_h + 6.0,
            input.y + input.h - input_h,
            input_h,
            input_h,
        );
        let can_send = !self.chat_input.trim().is_empty();
        if (ui.icon_button(send_r, Icon::Play, t("Send"), can_send) || sent) && can_send {
            let text = snook_legend_proto::clean_chat(&self.chat_input);
            // With the chat off, it waits for the dialog (and comes back to
            // the field if the player doesn't turn the chat on).
            let now = self.send_text(ClientMsg::Chat { text: text.clone() });
            // Shown at once, dimmed until the server passes it on.
            if now && let Some(me) = self.me.as_ref() {
                let line = snook_legend_proto::ChatLine {
                    from: me.username.clone(),
                    text: text.clone(),
                    at: 0,
                    kind: snook_legend_proto::ChatKind::Player,
                    rating: me.rating_in(self.rules),
                    id: 0,
                    admin: me.role == snook_legend_proto::Role::Admin,
                    deleted: None,
                };
                let mut feed = FeedLine::chat(&line);
                feed.pending = true;
                self.echoes.push(text);
                self.chat.push(feed);
            }
            self.emoji.reset();
            self.chat_input.clear();
            self.chat_scroll = 0;
            ui.focus = if now { Some(30) } else { None };
        }
        action
    }

    /// Sends a message with text for other players: a line in a chat, a
    /// note on the board, a status message, a gift's message. With the
    /// chat off it waits instead, and the dialog that turns the chat on
    /// opens; it goes once the chat is on. Whether it went at once.
    pub fn send_text(&mut self, msg: ClientMsg) -> bool {
        if self.chat_off() && has_text(&msg) {
            self.pending_text = Some(msg);
            self.open_chat_dialog();
            false
        } else {
            self.send(msg);
            true
        }
    }

    /// Opens the dialog that turns the chat on (or, under 18 with it on,
    /// off).
    pub fn open_chat_dialog(&mut self) {
        self.chat_dialog_open = true;
        self.guardian_ticked = false;
    }

    /// The chat is on: what the player wrote while it was off goes now.
    fn send_pending_text(&mut self) {
        self.chat_dialog_open = false;
        let Some(msg) = self.pending_text.take() else { return };
        if let ClientMsg::GiftItem { code, .. } = &msg {
            self.buying = Some(code.clone());
        }
        if let ClientMsg::SetStatusMessage { text } = &msg {
            let me = self.username.clone();
            if let Some(p) =
                self.profile.as_mut().filter(|p| p.player.username.eq_ignore_ascii_case(&me))
            {
                p.player.status_message = text.clone();
            }
        }
        self.send(msg);
    }

    /// The player left the chat off: a gift or a note goes without its
    /// text, a chat line comes back to the lounge's field, a status
    /// message is dropped.
    fn drop_pending_text(&mut self) {
        self.chat_dialog_open = false;
        match self.pending_text.take() {
            Some(ClientMsg::GiftItem { to, code, .. }) => {
                self.buying = Some(code.clone());
                self.send(ClientMsg::GiftItem { to, code, message: String::new() });
            }
            Some(ClientMsg::PostSeek { rated, rules, .. }) => {
                self.send(ClientMsg::PostSeek { text: String::new(), rated, rules });
            }
            Some(ClientMsg::Chat { text }) if self.chat_input.is_empty() => {
                self.chat_input = text;
            }
            _ => {}
        }
    }

    /// The dialog that turns the chat with other players on: the age group
    /// first (it can't be changed from the game afterwards), then, under
    /// 18, a parent's or guardian's permission, with a declaration. With
    /// their permission given, they turn it off here. Over any screen.
    pub fn draw_chat_dialog(&mut self, ui: &mut Ui, config: &Config) {
        if !self.chat_dialog_open {
            return;
        }
        let Some(age) = self.age.filter(|_| self.signed_in() && !self.lan) else {
            // Signed out, or a server that doesn't ask: nothing to turn on.
            self.drop_pending_text();
            return;
        };
        if age.minor == Some(false) {
            self.send_pending_text();
            return;
        }
        // The dialog has the keyboard: no field under it takes the typing.
        if ui.focus.is_some_and(|f| f < crate::ui::MODAL_FOCUS) {
            ui.focus = None;
        }
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let w = 520.0f32.min(sw - 32.0);
        let pad = 24.0;
        let inner = w - pad * 2.0;
        let asks_age = age.minor.is_none();
        let allowed = age.chat_allowed;
        let (title, text) = if asks_age {
            (
                t("Chat with other players"),
                t(
                    "The chat with other players is off until you turn it on. To turn it on, tell us your age group: it can't be changed from the game afterwards.",
                ),
            )
        } else if allowed {
            (
                t("Chat for players under 18"),
                t(
                    "A parent or guardian turned the chat on for this account. They can turn it off here at any time.",
                ),
            )
        } else {
            (
                t("Chat for players under 18"),
                t(
                    "This account belongs to someone under 18. By law, the chat with other players (messages in the lounge and in games, notes, status and gift messages) stays off until their mother, father or legal guardian turns it on.",
                ),
            )
        };
        let lines = ui.wrap(text, inner, 15.0);
        let note = if asks_age {
            ui.wrap(
                t("Under 18: a parent or guardian must agree to the terms with you and turn the chat on."),
                inner,
                13.0,
            )
        } else {
            Vec::new()
        };
        let declaration = ui.wrap(
            t("I am this player's mother, father or legal guardian, I have read the terms of use and I allow them to chat with other players."),
            inner - 30.0,
            14.0,
        );
        let ticks = !asks_age && !allowed;
        let decl_h = if ticks { 14.0 + declaration.len() as f32 * 19.0 } else { 0.0 };
        let note_h = if note.is_empty() { 0.0 } else { 10.0 + note.len() as f32 * 17.0 };
        let buttons_h = if asks_age { 44.0 + 10.0 + 44.0 } else { 44.0 };
        let h = pad
            + 30.0
            + 14.0
            + lines.len() as f32 * 20.0
            + note_h
            + decl_h
            + 20.0
            + buttons_h
            + 14.0
            + 20.0
            + pad;
        let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.6));
        ui.panel(r);
        let mut y = r.y + pad;
        ui.text(title, r.x + pad, y, 22.0, GOLD, true);
        y += 30.0 + 14.0;
        for line in &lines {
            ui.text(line, r.x + pad, y, 15.0, CREAM, false);
            y += 20.0;
        }
        if !note.is_empty() {
            y += 10.0;
            for line in &note {
                ui.text(line, r.x + pad, y, 13.0, MUTED, false);
                y += 17.0;
            }
        }
        if ticks {
            y += 14.0;
            // The tick box, with the declaration beside it on several lines.
            let tick = Rect::new(r.x + pad, y, inner, declaration.len() as f32 * 19.0);
            let b = Rect::new(r.x + pad, y + 1.0, 18.0, 18.0);
            rrect(b, 4.0, Color::new(0.06, 0.04, 0.03, 1.0));
            let hover = ui.hovering(tick);
            rrect_lines(b, 4.0, 1.2, if hover { GOLD } else { crate::ui::BORDER });
            if self.guardian_ticked {
                crate::paint::draw_line(b.x + 4.0, b.y + 9.5, b.x + 7.5, b.y + 13.0, 2.2, GOLD);
                crate::paint::draw_line(b.x + 7.5, b.y + 13.0, b.x + 14.0, b.y + 5.0, 2.2, GOLD);
            }
            for (k, line) in declaration.iter().enumerate() {
                ui.text(line, r.x + pad + 30.0, y + k as f32 * 19.0, 14.0, CREAM, false);
            }
            if ui.clicked(tick) {
                self.guardian_ticked = !self.guardian_ticked;
            }
            y += declaration.len() as f32 * 19.0;
        }
        y += 20.0;
        let half = (inner - 10.0) / 2.0;
        let later = if allowed { t("Close") } else { t("Not now") };
        let close = if asks_age {
            let adult = ui.primary_button(Rect::new(r.x + pad, y, half, 44.0), t("18 or older"));
            let minor =
                ui.primary_button(Rect::new(r.x + pad + half + 10.0, y, half, 44.0), t("Under 18"));
            y += 44.0 + 10.0;
            let close = ui.button(Rect::new(r.x + pad, y, inner, 44.0), later, true);
            if adult || minor {
                self.send(ClientMsg::SetAge { minor });
                // Shown at once; the server's answer confirms it. 18 or
                // older, what was written goes at once (after the answer,
                // on the same connection); under 18, the permission next.
                if let Some(a) = self.age.as_mut() {
                    a.minor = Some(minor);
                }
                if adult {
                    self.send_pending_text();
                }
            }
            close
        } else {
            let close = ui.button(Rect::new(r.x + pad, y, half, 44.0), later, true);
            let go = Rect::new(r.x + pad + half + 10.0, y, half, 44.0);
            let act = if allowed {
                ui.button(go, t("Turn the chat off"), true)
            } else if self.guardian_ticked {
                ui.primary_button(go, t("Turn the chat on"))
            } else {
                ui.button(go, t("Turn the chat on"), false)
            };
            if act && (allowed || self.guardian_ticked) {
                self.send(ClientMsg::AllowChat { allowed: !allowed });
                if let Some(a) = self.age.as_mut() {
                    a.chat_allowed = !allowed;
                }
                if allowed {
                    self.chat_dialog_open = false;
                } else {
                    self.send_pending_text();
                }
            }
            close
        };
        y += 44.0 + 14.0;
        crate::legal::links(ui, &config.server, sw / 2.0, y, 13.0);
        // Nothing under the dialog takes a click.
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        // A press on the dimmed screen around it is "Not now" (or Close).
        if self.chat_dialog_open && (close || ui.escape() || ui.dismissed(r)) {
            self.drop_pending_text();
        }
    }

    /// A scheduled tournament (the weekly cup) starts within ten minutes:
    /// tell the player once, whether they signed up or not.
    fn tournaments_starting(&mut self) {
        if !self.signed_in() || self.lan {
            return;
        }
        let now = crate::game::unix_now();
        let me = self.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
        let soon: Vec<(u64, String, bool)> = self
            .tournaments
            .iter()
            .filter(|t| t.state == TournamentState::Registering && !t.private)
            .filter(|t| t.starts_at.is_some_and(|at| at > now && at - now <= 600))
            .filter(|t| !self.told_starting.contains(&t.id))
            .map(|t| {
                let name = crate::i18n::tournament_name(&t.name);
                (t.id, name, t.players.iter().any(|p| p.username == me))
            })
            .collect();
        for (id, name, signed_up) in soon {
            self.told_starting.push(id);
            let text = if signed_up {
                tw("{name} starts in a few minutes: get ready!", &[("name", &name)])
            } else if self
                .tournaments
                .iter()
                .any(|t| t.id == id && t.players.len() < t.size as usize)
            {
                tw("{name} starts in a few minutes: there is still room.", &[("name", &name)])
            } else {
                continue;
            };
            self.toast(text.clone(), true);
            self.attention = Some(text);
        }
    }

    /// The tabs over the right side of the lounge.
    fn side_tabs(&mut self, ui: &mut Ui, panel: Rect, config: &mut Config) {
        let tabs =
            [(Side::Chat, t("Chat")), (Side::Missions, t("Missions")), (Side::Board, t("Board"))];
        let w = (panel.w - 24.0 - 12.0) / 3.0;
        let newest = self.board.iter().map(|n| n.id).max().unwrap_or(0);
        // Notes that arrive while the board is open are seen.
        if self.side == Side::Board && newest > config.board_seen {
            config.board_seen = newest;
            config.save();
        }
        for (k, (side, label)) in tabs.into_iter().enumerate() {
            let r = Rect::new(panel.x + 12.0 + k as f32 * (w + 6.0), panel.y + 12.0, w, 34.0);
            if ui.tab(r, label, self.side == side) {
                self.side = side;
                match side {
                    Side::Missions => {
                        self.mission_done = false;
                        self.send(ClientMsg::Missions);
                    }
                    Side::Board if newest > config.board_seen => {
                        config.board_seen = newest;
                        config.save();
                    }
                    _ => {}
                }
            }
            let badge = match side {
                Side::Missions => self.mission_done,
                Side::Board => newest > config.board_seen,
                Side::Chat => false,
            };
            if badge {
                draw_circle(r.x + r.w - 9.0, r.y + 9.0, 4.5, ALERT);
            }
        }
    }

    /// Today's and this week's missions with the player's progress.
    fn draw_missions(&mut self, ui: &mut Ui, area: Rect) {
        let Some(v) = &self.missions else {
            loading(ui, area);
            return;
        };
        let now = crate::game::unix_now();
        let mut y = area.y + 2.0;
        let sections = [
            (
                t("Today"),
                tw("new missions in {t}", &[("t", &time_left(v.day_ends - now))]),
                &v.daily,
            ),
            (t("This week"), tw("ends in {t}", &[("t", &time_left(v.week_ends - now))]), &v.weekly),
        ];
        for (title, when, list) in sections {
            ui.text(title, area.x + 2.0, y, 15.0, GOLD, true);
            let tw_ = ui.text_size(title, 15.0, true).x;
            // On the heading's baseline.
            ui.text(&when, area.x + 2.0 + tw_ + 10.0, y + 2.5, 12.0, MUTED, false);
            y += 26.0;
            for state in list {
                let Some(m) = snook_legend_proto::missions::mission(&state.code) else { continue };
                let card = Rect::new(area.x, y, area.w, 58.0);
                let done = state.progress >= m.target;
                rrect(card, 8.0, Color::new(1.0, 1.0, 1.0, if done { 0.07 } else { 0.035 }));
                rrect_lines(card, 8.0, 1.0, if done { GOLD } else { BORDER });
                // The reward, on the right and centred.
                let reward = m.reward.to_string();
                let rw = ui.text_size(&reward, 15.0, true).x;
                let rx = card.x + card.w - rw - 16.0;
                let icon = if done { Icon::Check } else { Icon::Coin };
                crate::icons::draw(icon, vec2(rx - 15.0, card.y + card.h / 2.0), 18.0, GOLD);
                let color = if done { GOLD } else { CREAM };
                ui.text(&reward, rx, card.y + (card.h - 15.0) / 2.0, 15.0, color, true);
                let text_w = rx - 30.0 - (card.x + 14.0);
                let text = tw(m.text, &[("n", &m.target)]);
                let (text, size) = fit_text(ui, &text, 14.0, text_w, true);
                ui.text(&text, card.x + 14.0, card.y + 10.0, size, CREAM, true);
                // Progress: the bar and the count after it.
                let count = format!("{}/{}", state.progress.min(m.target), m.target);
                let count_w = ui.text_size(&count, 12.0, false).x;
                let bar = Rect::new(card.x + 14.0, card.y + 38.0, text_w - count_w - 10.0, 7.0);
                rrect(bar, 3.5, Color::new(0.0, 0.0, 0.0, 0.4));
                let k = (state.progress as f32 / m.target.max(1) as f32).clamp(0.0, 1.0);
                if k > 0.0 {
                    let fill = Rect::new(bar.x, bar.y, (bar.w * k).max(7.0), bar.h);
                    rrect(fill, 3.5, if done { GOLD } else { Color::new(0.35, 0.75, 0.5, 1.0) });
                }
                ui.text(&count, bar.x + bar.w + 10.0, bar.y - 3.0, 12.0, MUTED, false);
                y += card.h + 8.0;
            }
            y += 10.0;
        }
        let note = t(
            "Missions count rated games played to the end. The coins arrive as soon as one is complete.",
        );
        for (i, line) in ui.wrap(note, area.w - 4.0, 12.0).iter().enumerate() {
            if y + i as f32 * 16.0 > area.y + area.h - 14.0 {
                break;
            }
            ui.text(line, area.x + 2.0, y + i as f32 * 16.0, 12.0, MUTED, false);
        }
    }

    /// Follows a note's button: lounge pages here, the rest for the caller.
    pub fn note_action(&mut self, a: snook_legend_proto::NoteAction) -> Option<OnlineAction> {
        use snook_legend_proto::NoteAction;
        match a {
            NoteAction::Tournaments => {
                self.tab = Tab::Tournaments;
                None
            }
            NoteAction::Missions => {
                self.side = Side::Missions;
                self.mission_done = false;
                self.send(ClientMsg::Missions);
                None
            }
            NoteAction::Shop => Some(OnlineAction::Shop),
            NoteAction::Vip => Some(OnlineAction::ShopVip),
        }
    }

    /// The notes on the board that are up now, pinned first.
    /// The events to show: running, or starting within two days; the
    /// running ones first, at most three.
    pub fn events_now(&self) -> Vec<snook_legend_proto::events::EventInfo> {
        let now = crate::game::unix_now();
        let mut list: Vec<_> = self
            .events
            .iter()
            .filter(|e| e.ends_at > now && e.starts_at - now <= 2 * 86_400)
            .filter(|e| e.kind != snook_legend_proto::events::EventKind::Unknown)
            .cloned()
            .collect();
        list.sort_by_key(|e| (!e.is_on(now), e.starts_at));
        list.truncate(3);
        list
    }

    /// The boost of `kind` running now (1: none).
    pub fn boost(&self, kind: snook_legend_proto::events::EventKind) -> u32 {
        snook_legend_proto::events::boost(&self.events, kind, crate::game::unix_now())
    }

    pub fn board_now(&self) -> Vec<snook_legend_proto::BoardNote> {
        let now = crate::game::unix_now();
        let mut notes: Vec<_> =
            self.board.iter().filter(|n| n.expires_at.is_none_or(|e| e > now)).cloned().collect();
        notes.sort_by_key(|n| !n.pinned);
        notes
    }

    /// The staff's notes and the tournaments open for registration.
    fn draw_board(&mut self, ui: &mut Ui, area: Rect) -> Option<OnlineAction> {
        let mut action = None;
        let now = crate::game::unix_now();
        let bottom = area.y + area.h;
        let mut y = area.y + 2.0;
        let notes = self.board_now();
        let open: Vec<TournamentInfo> = self
            .tournaments
            .iter()
            .filter(|t| t.state == TournamentState::Registering && !t.private)
            .cloned()
            .collect();
        if notes.is_empty() && open.is_empty() {
            let text = t("Nothing on the board right now.");
            empty_state(ui, area, Glyph::Icon(Icon::Star), text, None);
            return None;
        }
        for n in &notes {
            let h = note_height(ui, n, area.w);
            if y + h > bottom {
                break;
            }
            if let Some(a) = draw_note(ui, n, Rect::new(area.x, y, area.w, h)) {
                action = self.note_action(a);
            }
            y += h + 8.0;
        }
        if open.is_empty() || y + 60.0 > bottom {
            return action;
        }
        y += 6.0;
        ui.text(t("Open tournaments"), area.x + 2.0, y, 15.0, GOLD, true);
        y += 26.0;
        let see = t("See");
        let bw = button_w(ui, see, 30.0).max(80.0);
        for tour in open {
            if y + 52.0 > bottom {
                break;
            }
            let card = Rect::new(area.x, y, area.w, 52.0);
            let hover = ui.hovering(card);
            rrect(card, 8.0, Color::new(1.0, 1.0, 1.0, if hover { 0.07 } else { 0.035 }));
            rrect_lines(card, 8.0, 1.0, BORDER);
            crate::icons::draw(Icon::Trophy, vec2(card.x + 22.0, card.y + 26.0), 20.0, GOLD);
            let shown = crate::i18n::tournament_name(&tour.name);
            let (name, size) = fit_text(ui, &shown, 14.0, card.w - 40.0 - bw - 24.0, true);
            ui.text(&name, card.x + 42.0, card.y + 9.0, size, CREAM, true);
            let mut sub =
                tw("{n}/{size} players", &[("n", &tour.players.len()), ("size", &tour.size)]);
            if let Some(at) = tour.starts_at.filter(|&a| a > now) {
                sub = format!("{sub} · {}", tw("starts in {t}", &[("t", &time_left(at - now))]));
            }
            if tour.entry_fee > 0 {
                sub = format!("{sub} · {}", tw("entry {n} coins", &[("n", &tour.entry_fee)]));
            }
            let (sub, size) = fit_text(ui, &sub, 12.0, card.w - 40.0 - bw - 24.0, false);
            ui.text(&sub, card.x + 42.0, card.y + 29.0, size, MUTED, false);
            let b = Rect::new(card.x + card.w - bw - 11.0, card.y + 11.0, bw, 30.0);
            if ui.button(b, see, true) || (hover && ui.clicked(card)) {
                self.tab = Tab::Tournaments;
                self.selected_tournament = Some(tour.id);
            }
            y += card.h + 8.0;
        }
        action
    }

    pub(crate) fn rated_toggle(&mut self, ui: &mut Ui, r: Rect) {
        if self.lan {
            let y = r.y + (r.h - 13.0) / 2.0;
            ui.text(t("LAN games don't change ratings."), r.x + 2.0, y, 13.0, MUTED, false);
            return;
        }
        let options = [t("Rated"), t("Casual")];
        if let Some(i) = ui.segmented(r, &options, if self.rated_games { 0 } else { 1 }) {
            self.rated_games = i == 0;
        }
    }

    fn status_text(p: &PlayerInfo) -> String {
        match p.status {
            PlayerStatus::Playing => t("playing").to_string(),
            PlayerStatus::Waiting => t("at a table").to_string(),
            // Playing in focus mode: no invitations until it is over.
            PlayerStatus::Available if p.focus => {
                if p.guest {
                    t("in focus mode").to_string()
                } else {
                    format!("{} · {}", t(p.tier().name()), t("in focus mode"))
                }
            }
            // An invitation waits for them.
            PlayerStatus::Available if p.away => t("away").to_string(),
            // Still free for an invitation: it shows over their game.
            PlayerStatus::Available if p.vs_computer => {
                if p.guest {
                    t("playing the computer").to_string()
                } else {
                    // Others are shown by their league (the rating itself
                    // is on the profile and in the ranking).
                    format!("{} · {}", t(p.tier().name()), t("playing the computer"))
                }
            }
            PlayerStatus::Available => {
                if p.guest {
                    t("guest").to_string()
                } else {
                    t(p.tier().name()).to_string()
                }
            }
        }
    }

    fn status_dot(p: &PlayerInfo, x: f32, y: f32) {
        let color = match p.status {
            PlayerStatus::Playing => Color::new(0.9, 0.6, 0.2, 1.0),
            PlayerStatus::Waiting => Color::new(0.45, 0.7, 1.0, 1.0),
            PlayerStatus::Available if p.focus => Color::new(0.85, 0.45, 0.45, 1.0),
            PlayerStatus::Available if p.away => Color::new(0.85, 0.75, 0.3, 1.0),
            PlayerStatus::Available if p.vs_computer => Color::new(0.75, 0.55, 0.95, 1.0),
            PlayerStatus::Available => Color::new(0.35, 0.85, 0.45, 1.0),
        };
        draw_circle(x, y, 4.5, color);
    }

    /// "Play now": one click to be paired with someone of a similar rating.
    /// Seconds "Play now" has been looking for an opponent.
    pub fn queue_secs(&self) -> Option<u32> {
        self.queue.map(|(_, _, waited, _, at)| waited + at.elapsed().as_secs() as u32)
    }

    /// While a game against the computer fills the wait: the search, under
    /// the top bar, with a way to stop it.
    pub fn draw_queue_banner(&mut self, ui: &mut Ui) {
        let Some(secs) = self.queue_secs() else { return };
        let sw = crate::ui::vw();
        let text = tw(
            "Looking for an opponent… {t} · the game starts as soon as one is found",
            &[("t", &format!("{}:{:02}", secs / 60, secs % 60))],
        );
        let w = (ui.text_size(&text, 13.0, false).x + 130.0).min(sw - 20.0);
        let r = Rect::new((sw - w) / 2.0, 92.0, w, 34.0);
        rrect(r, 8.0, Color::new(0.06, 0.2, 0.12, 0.92));
        rrect_lines(r, 8.0, 1.2, Color::new(0.35, 0.8, 0.5, 0.8));
        let text = ui.ellipsize(&text, w - 120.0, 13.0);
        ui.text(&text, r.x + 12.0, r.y + 10.0, 13.0, CREAM, false);
        ui.block(r);
        if ui.button(Rect::new(r.x + r.w - 96.0, r.y + 4.0, 90.0, 26.0), t("Stop"), true) {
            self.send(ClientMsg::QueueLeave);
            self.queue = None;
        }
    }

    fn draw_play_now(&mut self, ui: &mut Ui, r: Rect) {
        match self.queue {
            None => {
                if ui.primary_button(r, t("Play now")) {
                    let rated = self.rated_games && !self.lan;
                    self.send(ClientMsg::QueueJoin { rated, rules: self.rules });
                }
                // How many are looking for an opponent under these rules.
                let waiting = self.players_of(self.rules).filter(|p| p.searching).count();
                if waiting > 0 && r.w > 260.0 {
                    let label = tw("{n} waiting", &[("n", &waiting)]);
                    let lw = ui.text_size(&label, 13.0, true).x;
                    let cy = r.y + r.h / 2.0;
                    let lx = r.x + r.w - 14.0 - lw;
                    crate::icons::draw(Icon::Person, vec2(lx - 11.0, cy), 13.0, CREAM);
                    ui.text(&label, lx, crate::ranking::text_y(cy, 13.0), 13.0, CREAM, true);
                }
                if ui.hovering(r) {
                    let tip = tw(
                        "Find an opponent of your level automatically ({rules})",
                        &[("rules", &crate::i18n::rules_name_inline(self.rules))],
                    );
                    ui.tooltip(&tip, r);
                }
            }
            Some((rated, rules, waited, range, at)) => {
                rrect(r, 8.0, Color::new(0.10, 0.30, 0.18, 0.55));
                rrect_lines(r, 8.0, 1.5, Color::new(0.35, 0.8, 0.5, 0.8));
                // A small spinner.
                let c = vec2(r.x + 22.0, r.y + r.h / 2.0);
                let a = get_time() as f32 * 5.0;
                for k in 0..8 {
                    let ang = a + k as f32 * std::f32::consts::TAU / 8.0;
                    let p = c + Vec2::from_angle(ang) * 8.0;
                    draw_circle(p.x, p.y, 1.8, Color::new(0.6, 1.0, 0.7, (k as f32 + 1.0) / 8.0));
                }
                let secs = waited + at.elapsed().as_secs() as u32;
                let title = tw(
                    "Looking for an opponent… {t}",
                    &[("t", &format!("{}:{:02}", secs / 60, secs % 60))],
                );
                let mode = if rated { t("rated") } else { t("casual") };
                let sub = if range == u32::MAX {
                    format!("{} · {mode}", t("any rating"))
                } else {
                    tw("rating ±{n} · {mode}", &[("n", &range), ("mode", &mode)])
                };
                let sub = format!("{sub} · {}", crate::i18n::rules_name_inline(rules));
                let cw = 100.0f32.min(r.w * 0.3);
                let (title, size) = ui.fitted(&title, 14.0, r.w - cw - 56.0, true);
                ui.text(&title, r.x + 40.0, r.y + 7.0, size, CREAM, true);
                ui.text(&sub, r.x + 40.0, r.y + r.h - 19.0, 12.0, MUTED, false);
                if ui.button(
                    Rect::new(r.x + r.w - cw - 8.0, r.y + 7.0, cw, r.h - 14.0),
                    t("Cancel"),
                    true,
                ) {
                    self.send(ClientMsg::QueueLeave);
                    self.queue = None;
                }
            }
        }
    }

    fn draw_players(&mut self, ui: &mut Ui, area: Rect) -> Option<OnlineAction> {
        self.draw_play_now(ui, Rect::new(area.x, area.y, area.w, 44.0));
        let mut y = area.y + 44.0 + 12.0;
        let mut action = None;
        // Nobody yet after half a minute: a game against the computer while
        // the search goes on.
        // Searching: a table to practise on at once (the game starts as
        // soon as an opponent is found); after half a minute, the computer
        // too.
        if self.queue_secs().is_some() && !self.lan {
            let computer = self.queue_secs().is_some_and(|s| s >= PRACTICE_AFTER_SECS);
            let half = if computer { (area.w - 8.0) / 2.0 } else { area.w };
            let b = Rect::new(area.x, y, half, 36.0);
            let (label, _) = ui.fitted(t("Practise while you wait"), 15.0, half - 16.0, true);
            if ui.button(b, &label, true) {
                action = Some(OnlineAction::FreeTableWhileWaiting);
            }
            if computer {
                let b = Rect::new(area.x + half + 8.0, y, half, 36.0);
                let (label, _) =
                    ui.fitted(t("Play the computer while you wait"), 15.0, half - 16.0, true);
                if ui.button(b, &label, true) {
                    action = Some(OnlineAction::PracticeWhileWaiting);
                }
            }
            y += 36.0 + 12.0;
        }
        self.rated_toggle(ui, Rect::new(area.x, y, area.w.min(280.0), 32.0));
        y += 32.0 + 12.0;
        y += self.draw_seek_bar(ui, Rect::new(area.x, y, area.w, 40.0));
        let me = self.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
        let mut players: Vec<PlayerInfo> =
            self.players.iter().filter(|p| p.username != me).cloned().collect();
        // Those looking for a game first (newest note first), then who is
        // free, those of this player's rules before the others.
        let now = crate::game::unix_now();
        let note_at = |p: &PlayerInfo| self.social.note_of(&p.username, now).map(|n| n.at);
        let mine = self.rules;
        players.sort_by(|a, b| {
            note_at(b)
                .cmp(&note_at(a))
                .then(
                    (a.status != PlayerStatus::Available)
                        .cmp(&(b.status != PlayerStatus::Available)),
                )
                .then((a.rules != mine).cmp(&(b.rules != mine)))
                .then(b.rating().cmp(&a.rating()))
        });
        let list = Rect::new(area.x, y, area.w, area.y + area.h - y);
        if players.is_empty() {
            let text = t("Nobody else is here right now.");
            let cta = Some(t("Open a table"));
            if empty_state(ui, list, Glyph::Icon(Icon::People), text, cta) {
                self.tab = Tab::Tables;
                self.list_first = 0;
            }
            return action;
        }
        let row_h = 48.0;
        let visible = (list.h / row_h).floor().max(1.0) as usize;
        self.list_first = ui.scroll_list(list, self.list_first, players.len(), visible);
        let row_w = list.w - if players.len() > visible { 12.0 } else { 0.0 };
        let invite_label = t("Invite");
        let watch_label = t("Watch");
        let challenge_label = t("Challenge");
        let bw = button_w(ui, invite_label, 30.0)
            .max(button_w(ui, watch_label, 30.0))
            .max(button_w(ui, challenge_label, 30.0))
            .max(96.0);
        let staff = self.me.as_ref().is_some_and(|m| m.role.is_staff());
        let mut invite = None;
        let mut open = None;
        let mut watch = None;
        let mut daily = None;
        let mut take_down = None;
        for (k, p) in players.iter().skip(self.list_first).take(visible).enumerate() {
            let r = Rect::new(list.x, list.y + k as f32 * row_h, row_w, row_h - 6.0);
            let note = self.social.note_of(&p.username, now).cloned();
            let button = Rect::new(r.x + r.w - bw - 6.0, r.y + 6.0, bw, r.h - 12.0);
            // The staff can take a note down (×, before the button).
            let x_r = Rect::new(button.x - 8.0 - 28.0, r.y + 7.0, 28.0, r.h - 14.0);
            let staff_x = staff && note.is_some();
            // A daily game (played a turn at a time), for players with an account.
            let corr = (!p.guest && !self.lan).then(|| {
                let right = if staff_x { x_r.x } else { button.x };
                Rect::new(right - 6.0 - 30.0, r.y + 6.0, 30.0, r.h - 12.0)
            });
            // The row (but its buttons) opens the player's profile.
            let hover = row_card(ui, r, !p.guest);
            if note.is_some() {
                // Looking for a game: a gold edge.
                rrect(Rect::new(r.x + 1.0, r.y + 6.0, 3.0, r.h - 12.0), 1.5, GOLD);
            }
            // The picture, with the status dot on its edge.
            let pic_r = ((r.h - 12.0) / 2.0).min(17.0);
            let pic = vec2(r.x + 10.0 + pic_r, r.y + r.h / 2.0);
            crate::avatars::draw(&p.avatar, pic, pic_r);
            let dot = pic + vec2(pic_r * 0.72, pic_r * 0.72);
            draw_circle(dot.x, dot.y, 6.5, Color::new(0.08, 0.05, 0.03, 1.0));
            Self::status_dot(p, dot.x, dot.y);
            let nx = pic.x + pic_r + 10.0;
            let right = corr.map_or(if staff_x { x_r.x } else { button.x }, |c| c.x);
            let name_w = right - 12.0 - nx;
            ui.player_name(p, nx, r.y + 5.0, 15.0, name_w);
            match &note {
                Some(n) => {
                    Self::rating_after_name(ui, p, nx, r.y + 5.0, name_w);
                    Self::note_line(ui, n, mine, nx, r.y + 23.0, name_w);
                }
                None => {
                    // A player of the other rules says so at the end.
                    let other = (p.rules != mine)
                        .then(|| social::other_rules(ui, p.rules, 0.0, 0.0, false));
                    let room = name_w - other.unwrap_or(0.0);
                    let status = Self::status_text(p);
                    let used = sub_line(ui, &status, &p.status_message, nx, r.y + 23.0, room);
                    if other.is_some() {
                        social::other_rules(ui, p.rules, nx + used, r.y + 23.0, true);
                    }
                }
            }
            if hover
                && !p.guest
                && ui.clicked(r)
                && !button.contains(ui.mouse())
                && !(staff_x && x_r.contains(ui.mouse()))
                && !corr.is_some_and(|c| c.contains(ui.mouse()))
            {
                open = Some(p.username.clone());
            }
            if staff_x && ui.icon_button_ex(x_r, Icon::Close, t("Take the note down"), true, true) {
                take_down = Some(p.username.clone());
            }
            if let Some(c) = corr
                && ui.icon_button_ex(c, Icon::Hourglass, t("Challenge to a daily game"), true, true)
            {
                daily = Some(p.username.clone());
            }
            // Friends' games (anyone's on a local network) can be watched.
            let friend =
                self.lan || self.friends.friends.iter().any(|f| f.player.username == p.username);
            if let Some(n) = note.as_ref().filter(|_| p.status != PlayerStatus::Playing) {
                // A challenge is an invitation, rated as the note says.
                let can =
                    p.status == PlayerStatus::Available && !p.focus && self.outgoing.is_none();
                let hit = if can {
                    ui.primary_button(button, challenge_label)
                } else {
                    ui.button(button, challenge_label, false)
                };
                if hit {
                    let (to, rated, rules) = (p.username.clone(), n.rated, n.rules);
                    self.send(ClientMsg::Invite { to, rated, rules });
                }
            } else if p.status == PlayerStatus::Playing && friend {
                if ui.button(button, watch_label, true) {
                    watch = Some(p.username.clone());
                }
            } else {
                // Not one in focus mode: it would be turned away.
                let can =
                    p.status == PlayerStatus::Available && !p.focus && self.outgoing.is_none();
                if ui.button(button, invite_label, can) {
                    invite = Some(p.username.clone());
                }
                if p.focus && ui.hovering(button) {
                    ui.tooltip(
                        t("In focus mode: no invitations until their game is over."),
                        button,
                    );
                }
            }
        }
        if let Some(to) = invite {
            self.open_invite(&to);
        }
        if let Some(name) = watch {
            self.spectate(&name);
        }
        if let Some(to) = daily {
            crate::correspondence::challenge(self, &to);
        }
        if let Some(username) = take_down {
            self.send(ClientMsg::RemoveSeek { username });
        }
        open.map(OnlineAction::Profile).or(action)
    }

    fn draw_tables(&mut self, ui: &mut Ui, area: Rect) {
        let me = self.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
        let mine = self.tables.iter().find(|t| t.host.username == me).cloned();
        let mut y = area.y;
        match mine {
            Some(table) => {
                let r = Rect::new(area.x, y, area.w, 64.0);
                rrect(r, 8.0, Color::new(0.10, 0.30, 0.18, 0.55));
                rrect_lines(r, 8.0, 1.5, Color::new(0.35, 0.8, 0.5, 0.8));
                let pulse = 0.5 + 0.5 * (get_time() as f32 * 3.0).sin();
                let dot = Color::new(0.35, 0.95, 0.5, 0.4 + 0.6 * pulse);
                draw_circle(r.x + 20.0, r.y + 21.0, 5.0, dot);
                let close = t("Close");
                let bw = button_w(ui, close, 32.0).max(96.0);
                let text_w = r.w - 34.0 - bw - 24.0;
                let title = t("Your table is open");
                let (title, size) = fit_text(ui, title, 16.0, text_w, true);
                ui.text(&title, r.x + 34.0, r.y + 13.0, size, CREAM, true);
                let kind = match (table.rated, table.private) {
                    (true, true) => t("rated · private"),
                    (true, false) => t("rated · public"),
                    (false, true) => t("casual · private"),
                    (false, false) => t("casual · public"),
                };
                let rules = crate::i18n::rules_name_inline(table.rules);
                let sub = format!("{kind} · {rules} — {}", t("waiting for an opponent…"));
                let (sub, size) = fit_text(ui, &sub, 13.0, text_w, false);
                ui.text(&sub, r.x + 34.0, r.y + 36.0, size, MUTED, false);
                if ui.button(Rect::new(r.x + r.w - 12.0 - bw, r.y + 16.0, bw, 32.0), close, true) {
                    self.send(ClientMsg::CloseTable);
                }
                y += r.h + 12.0;
            }
            None => {
                // How the table is played, then the two ways to open one.
                // (On a local network there is no rating to choose: the
                // Players tab says so.)
                let seg_w = if self.lan { 0.0 } else { (area.w * 0.55).min(280.0) };
                if !self.lan {
                    self.rated_toggle(ui, Rect::new(area.x, y, seg_w, 32.0));
                }
                let cx = if self.lan { area.x + 2.0 } else { area.x + seg_w + 20.0 };
                ui.checkbox(cx, y + 7.0, t("Private"), &mut self.table_private);
                y += 32.0 + 12.0;
                if self.table_private {
                    let label = t("Password");
                    let lw = ui.text_size(label, 13.0, true).x;
                    ui.text(label, area.x + 2.0, y + 11.5, 13.0, MUTED, true);
                    let field = Rect::new(area.x + lw + 14.0, y, area.w - lw - 14.0, 36.0);
                    ui.text_input(31, field, &mut self.table_password, 32);
                    let hint = t("for your friends to sit down");
                    placeholder(ui, 31, field, &self.table_password, hint);
                    y += 36.0 + 12.0;
                }
                let can = !self.table_private || !self.table_password.trim().is_empty();
                let open_label = t("Open a table");
                let code_label = t("Invite with a code");
                // Side by side when both fit, else one above the other.
                let half = (area.w - 8.0) / 2.0;
                let both = button_w(ui, open_label, 36.0).max(button_w(ui, code_label, 36.0));
                let (open_r, code_r) = if self.lan {
                    (Rect::new(area.x, y, area.w, 36.0), None)
                } else if both <= half {
                    (
                        Rect::new(area.x, y, half, 36.0),
                        Some(Rect::new(area.x + half + 8.0, y, half, 36.0)),
                    )
                } else {
                    let open_r = Rect::new(area.x, y, area.w, 36.0);
                    y += 36.0 + 8.0;
                    (open_r, Some(Rect::new(area.x, y, area.w, 36.0)))
                };
                let open = if can {
                    ui.primary_button(open_r, open_label)
                } else {
                    ui.button(open_r, open_label, false)
                };
                if open {
                    // A new table: the code of an earlier one is no more.
                    self.invite_code = None;
                    let password = self.table_private.then(|| self.table_password.clone());
                    self.send(ClientMsg::OpenTable {
                        rated: self.rated_games && !self.lan,
                        rules: self.rules,
                        password,
                    });
                }
                // A private table and a short code for someone who isn't in
                // the lounge yet.
                if let Some(code_r) = code_r
                    && ui.button(code_r, code_label, true)
                {
                    let (rated, rules) = (self.rated_games, self.rules);
                    self.invite_code = None;
                    self.send(ClientMsg::CreateInviteCode { rated, rules });
                }
                y += 36.0 + 12.0;
                if !self.lan {
                    // Someone sent a code: sit at their table.
                    let join = t("Join");
                    let jw = button_w(ui, join, 36.0).max(96.0);
                    let field = Rect::new(area.x, y, area.w - jw - 8.0, 36.0);
                    let enter = ui.text_input(37, field, &mut self.code_input, 24);
                    placeholder(ui, 37, field, &self.code_input, t("Got a code? Type it here"));
                    let ok = self.code_input.trim().len() >= 6;
                    let jr = Rect::new(field.x + field.w + 8.0, y, jw, 36.0);
                    if (ui.button(jr, join, ok) || enter) && ok {
                        let code = std::mem::take(&mut self.code_input);
                        self.send(ClientMsg::JoinByCode { code });
                    }
                    y += 36.0 + 12.0;
                }
            }
        }
        // The code of this player's table, to send to a friend.
        if let Some(code) =
            self.invite_code.clone().filter(|_| self.tables.iter().any(|t| t.host.username == me))
        {
            let r = Rect::new(area.x, y, area.w, 52.0);
            rrect(r, 8.0, Color::new(0.91, 0.75, 0.38, 0.12));
            rrect_lines(r, 8.0, 1.0, Color::new(0.91, 0.75, 0.38, 0.4));
            ui.text(&code, r.x + 14.0, r.y + 13.0, 24.0, GOLD, true);
            let link = format!("snook-legend://join/{code}");
            let copy = t("Copy");
            let bw = button_w(ui, copy, 32.0).max(90.0);
            let cw = ui.text_size(&code, 24.0, true).x;
            let room = r.w - 14.0 - cw - 16.0 - bw - 24.0;
            if room > 80.0 {
                let (link, size) = fit_text(ui, &link, 12.0, room, false);
                ui.text(&link, r.x + 30.0 + cw, r.y + 20.0, size, MUTED, false);
            }
            if ui.button(Rect::new(r.x + r.w - 10.0 - bw, r.y + 10.0, bw, 32.0), copy, true) {
                macroquad::miniquad::window::clipboard_set(&format!(
                    "{} {link}",
                    t("Join my table in Snook Legend:")
                ));
                self.toast(t("Copied: send it to a friend."), true);
            }
            y += r.h + 12.0;
        }
        draw_line(area.x, y, area.x + area.w, y, 1.0, BORDER);
        y += 12.0;
        let others: Vec<TableInfo> =
            self.tables.iter().filter(|t| t.host.username != me).cloned().collect();
        let list = Rect::new(area.x, y, area.w, area.y + area.h - y);
        if others.is_empty() {
            let text = if self.tables.len() > others.len() {
                t("No other open tables right now.")
            } else {
                t("No open tables. Open one and others can sit down.")
            };
            empty_state(ui, list, Glyph::Icon(Icon::Table), text, None);
            return;
        }
        let row_h = 52.0;
        let visible = (list.h / row_h).floor().max(1.0) as usize;
        self.list_first = ui.scroll_list(list, self.list_first, others.len(), visible);
        let row_w = list.w - if others.len() > visible { 12.0 } else { 0.0 };
        let sit = t("Sit down");
        let join_label = t("Join");
        let bw = button_w(ui, sit, 32.0).max(button_w(ui, join_label, 32.0)).max(96.0);
        let mut join = None;
        for (k, table) in others.iter().skip(self.list_first).take(visible).enumerate() {
            let r = Rect::new(list.x, list.y + k as f32 * row_h, row_w, row_h - 6.0);
            row_card(ui, r, false);
            crate::icons::draw(Icon::Table, vec2(r.x + 22.0, r.y + r.h / 2.0), 22.0, CREAM);
            let button = Rect::new(r.x + r.w - bw - 7.0, r.y + 7.0, bw, r.h - 14.0);
            let joining = self.joining.as_ref().is_some_and(|(id, _)| *id == table.id);
            // While typing a private table's password the field takes the
            // room of the details.
            let field_w = if joining { 160.0f32.min(r.w * 0.4) } else { 0.0 };
            let text_right = button.x - 12.0 - if joining { field_w + 8.0 } else { 0.0 };
            let lock_w = if table.private && !joining { 24.0 } else { 0.0 };
            let name_w = text_right - lock_w - (r.x + 44.0);
            ui.player_name(&table.host, r.x + 44.0, r.y + 6.0, 15.0, name_w);
            let mut px = r.x + 44.0;
            let (label, color) = if table.rated {
                (t("rated"), GOLD)
            } else {
                (t("casual"), Color::new(0.6, 0.8, 1.0, 1.0))
            };
            px += ui.pill(label, px, r.y + 25.0, color) + 8.0;
            px += crate::rulesui::tag(ui, table.rules, px, r.y + 33.5, 12.0) + 10.0;
            let tier = t(table.host.tier_in(table.rules).name());
            if px + ui.text_size(tier, 12.0, false).x < text_right - lock_w {
                ui.text(tier, px, r.y + 26.0, 12.0, MUTED, false);
            }
            if table.private && !joining {
                let c = vec2(text_right - 8.0, r.y + r.h / 2.0);
                crate::icons::draw(Icon::Lock, c, 16.0, MUTED);
            }
            if !joining {
                if ui.button(button, sit, true) {
                    if table.private {
                        self.joining = Some((table.id, String::new()));
                        ui.focus = Some(32);
                    } else {
                        join = Some((table.id, None));
                    }
                }
            } else {
                let ir = Rect::new(button.x - 8.0 - field_w, button.y, field_w, button.h);
                let (_, pw) = self.joining.as_mut().expect("joining");
                let enter = ui.password_input(32, ir, pw, 32);
                placeholder(ui, 32, ir, pw, t("password"));
                let pw = pw.clone();
                if ui.primary_button(button, join_label) || enter {
                    join = Some((table.id, Some(pw)));
                    self.joining = None;
                }
            }
        }
        if let Some((id, password)) = join {
            self.send(ClientMsg::JoinTable { id, password });
        }
    }

    /// A new tournament: the form, labels in one column and controls in the other.
    fn draw_tournament_form(&mut self, ui: &mut Ui, area: Rect) {
        let lan = self.lan;
        let Some(form) = &mut self.tournament_form else { return };
        let labels = [
            t("Name"),
            t("Format"),
            t("Rules"),
            t("Players"),
            t("Entry"),
            t("Games"),
            t("Password"),
        ];
        let lw = labels.iter().map(|l| ui.text_size(l, 13.0, true).x).fold(70.0, f32::max) + 16.0;
        let cx = area.x + lw;
        let cw = area.w - lw;
        let seg_w = cw.min(280.0);
        let arena = form.format == snook_legend_proto::TournamentFormat::Arena;
        let note = t("The tournament starts when it is full (the host can start it earlier).");
        let note = ui.wrap(note, area.w - 4.0, 12.0);
        // When it would not fit (a large interface size), the rows close up
        // and the footnote goes.
        let hint_h = |text: &str| ui.wrap(text, cw, 12.0).len() as f32 * 16.0 + 2.0;
        // Name, format, rules, players (not in an arena), entry and games
        // (online), password.
        let rows = 5.0 - arena as u8 as f32 + if lan { 0.0 } else { 2.0 };
        let mut needed = 34.0 + 16.0 + rows * 42.0 + note.len() as f32 * 16.0 + 12.0 + 36.0;
        if arena {
            needed += hint_h(t(ARENA_HINT));
        }
        if !lan && form.entry_fee > 0 {
            needed += hint_h(t(FEE_HINT));
        }
        let compact = needed > area.h;
        let gap = if compact { 6.0 } else { 10.0 };
        let mut y = area.y;
        let mut cancel =
            ui.icon_button(Rect::new(area.x, y, 34.0, 34.0), Icon::Back, t("Cancel"), true);
        crate::icons::draw(Icon::Trophy, vec2(area.x + 34.0 + 12.0 + 10.0, y + 17.0), 20.0, GOLD);
        ui.text(t("New tournament"), area.x + 34.0 + 12.0 + 26.0, y + 8.0, 18.0, GOLD, true);
        y += 34.0 + if compact { 10.0 } else { 16.0 };
        let label = |ui: &Ui, text: &str, y: f32| {
            ui.text(text, area.x + 2.0, y + (32.0 - 13.0) / 2.0, 13.0, MUTED, true);
        };
        // A hint under a control, in its column.
        let hint = |ui: &Ui, text: &str, y: &mut f32| {
            for line in ui.wrap(text, cw, 12.0) {
                ui.text(&line, cx + 2.0, *y, 12.0, MUTED, false);
                *y += 16.0;
            }
            *y += 6.0;
        };
        label(ui, t("Name"), y);
        let name_r = Rect::new(cx, y, cw, 32.0);
        let mut enter = ui.text_input(33, name_r, &mut form.name, snook_legend_proto::MAX_NAME_LEN);
        y += 32.0 + gap;
        label(ui, t("Format"), y);
        let formats = [t("Knockout"), t("Arena")];
        if let Some(i) = ui.segmented(Rect::new(cx, y, seg_w, 32.0), &formats, arena as usize) {
            form.format = if i == 1 {
                snook_legend_proto::TournamentFormat::Arena
            } else {
                snook_legend_proto::TournamentFormat::Knockout
            };
        }
        y += 32.0 + gap;
        if arena {
            y -= 4.0;
            hint(ui, t(ARENA_HINT), &mut y);
        }
        label(ui, t("Rules"), y);
        if let Some(rules) =
            crate::rulesui::switch(ui, Rect::new(cx, y, seg_w, 32.0), form.rules, None)
        {
            form.rules = rules;
        }
        y += 32.0 + gap;
        if !arena {
            label(ui, t("Players"), y);
            let sizes = [4u32, 8, 16];
            let labels = ["4", "8", "16 (VIP)"];
            let sel = sizes.iter().position(|s| *s == form.size).unwrap_or(1);
            if let Some(i) = ui.segmented(Rect::new(cx, y, seg_w, 32.0), &labels, sel) {
                form.size = sizes[i];
            }
            y += 32.0 + gap;
        }
        if !lan {
            label(ui, t("Entry"), y);
            let fees = [0u32, 50, 100, 250, 500];
            let labels: Vec<String> = fees
                .iter()
                .map(|f| if *f == 0 { t("Free").to_string() } else { f.to_string() })
                .collect();
            let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
            let sel = fees.iter().position(|f| *f == form.entry_fee).unwrap_or(0);
            if let Some(i) = ui.segmented(Rect::new(cx, y, cw, 32.0), &labels, sel) {
                form.entry_fee = fees[i];
            }
            y += 32.0 + gap;
            if form.entry_fee > 0 {
                y -= 4.0;
                hint(ui, t(FEE_HINT), &mut y);
            }
            label(ui, t("Games"), y);
            let opts = [t("Rated"), t("Casual")];
            let r = Rect::new(cx, y, seg_w, 32.0);
            if let Some(i) = ui.segmented(r, &opts, if form.rated { 0 } else { 1 }) {
                form.rated = i == 0;
            }
            y += 32.0 + gap;
        }
        label(ui, t("Password"), y);
        let pass_r = Rect::new(cx, y, cw, 32.0);
        enter |= ui.text_input(34, pass_r, &mut form.password, 32);
        placeholder(ui, 34, pass_r, &form.password, t("optional: only friends with it can join"));
        y += 32.0 + gap;
        if !compact {
            for line in &note {
                ui.text(line, area.x + 2.0, y, 12.0, MUTED, false);
                y += 16.0;
            }
            y += 12.0;
        } else {
            y += 4.0;
        }
        // Cancel on the left, Create on the right.
        let ok = form.name.trim().chars().count() >= 3;
        let create_l = t("Create");
        let cancel_l = t("Cancel");
        let bw = button_w(ui, create_l, 36.0).max(button_w(ui, cancel_l, 36.0)).max(140.0);
        cancel |= ui.button(Rect::new(area.x, y, bw, 36.0), cancel_l, true);
        let create_r = Rect::new(area.x + area.w - bw, y, bw, 36.0);
        let create = if ok {
            ui.primary_button(create_r, create_l) || enter
        } else {
            ui.button(create_r, create_l, false)
        };
        if create {
            let msg = ClientMsg::CreateTournament {
                name: form.name.trim().to_string(),
                size: form.size,
                rated: form.rated && !lan,
                rules: form.rules,
                password: (!form.password.trim().is_empty()).then(|| form.password.clone()),
                entry_fee: form.entry_fee,
                format: form.format,
            };
            self.send(msg);
            self.tournament_form = None;
        } else if cancel {
            self.tournament_form = None;
        }
    }

    fn draw_tournaments(&mut self, ui: &mut Ui, area: Rect) {
        if self.tournament_form.is_some() {
            return self.draw_tournament_form(ui, area);
        }
        // Development aid (screenshots): open the first running tournament.
        if self.selected_tournament.is_none() && crate::dev::flag("SNOOK_LEGEND_OPEN_TOURNAMENT") {
            self.selected_tournament =
                self.tournaments.iter().find(|t| t.state == TournamentState::Running).map(|t| t.id);
        }
        if let Some(id) = self.selected_tournament {
            match self.tournaments.iter().find(|t| t.id == id).cloned() {
                Some(tour) => return self.draw_tournament(ui, area, &tour),
                None => self.selected_tournament = None,
            }
        }
        if ui.button(Rect::new(area.x, area.y, area.w, 36.0), t("Create a tournament"), true) {
            self.tournament_form = Some(TournamentForm {
                size: 8,
                rated: !self.lan,
                rules: self.rules,
                ..Default::default()
            });
            ui.focus = Some(33);
        }
        let list = Rect::new(area.x, area.y + 36.0 + 12.0, area.w, area.h - 36.0 - 12.0);
        if self.tournaments.is_empty() {
            let text = t("No tournaments right now. Create one!");
            empty_state(ui, list, Glyph::Icon(Icon::Trophy), text, None);
            return;
        }
        let row_h = 58.0;
        let visible = (list.h / row_h).floor().max(1.0) as usize;
        let mut order: Vec<&TournamentInfo> = self.tournaments.iter().collect();
        order.sort_by_key(|t| {
            (t.state != TournamentState::Registering, t.state == TournamentState::Finished, t.id)
        });
        self.list_first = ui.scroll_list(list, self.list_first, order.len(), visible);
        let row_w = list.w - if order.len() > visible { 12.0 } else { 0.0 };
        let mut open = None;
        for (k, tour) in order.iter().skip(self.list_first).take(visible).enumerate() {
            let r = Rect::new(list.x, list.y + k as f32 * row_h, row_w, row_h - 6.0);
            let hover = row_card(ui, r, true);
            crate::icons::draw(Icon::Trophy, vec2(r.x + 22.0, r.y + r.h / 2.0), 22.0, GOLD);
            // On the right: the state, and a star (rated) and a lock (password).
            let (label, color) = state_label(tour.state);
            let pw = ui.text_size(label, 11.0, true).x + 12.0;
            let mut ix = r.x + r.w - 12.0 - pw;
            ui.pill(label, ix, r.y + (r.h - 17.0) / 2.0, color);
            ix -= 4.0;
            if tour.rated {
                ix -= 16.0;
                crate::icons::draw(Icon::Star, vec2(ix + 7.0, r.y + r.h / 2.0), 14.0, GOLD);
            }
            if tour.private {
                ix -= 20.0;
                crate::icons::draw(Icon::Lock, vec2(ix + 7.0, r.y + r.h / 2.0), 14.0, MUTED);
            }
            // The rules its games are played under.
            ix -= 8.0 + crate::rulesui::tag_w(ui, tour.rules, 12.0);
            crate::rulesui::tag(ui, tour.rules, ix, r.y + r.h / 2.0, 12.0);
            let text_w = ix - 12.0 - (r.x + 44.0);
            let shown = crate::i18n::tournament_name(&tour.name);
            let (name, size) = fit_text(ui, &shown, 15.0, text_w, true);
            ui.text(&name, r.x + 44.0, r.y + 8.0, size, CREAM, true);
            let mut sub = tw(
                "by {host} · {n}/{size} players",
                &[("host", &tour.host), ("n", &tour.players.len()), ("size", &tour.size)],
            );
            if tour.entry_fee > 0 {
                sub = format!("{sub} · {}", tw("entry {n} coins", &[("n", &tour.entry_fee)]));
            }
            let (sub, size) = fit_text(ui, &sub, 12.0, text_w, false);
            ui.text(&sub, r.x + 44.0, r.y + 29.0, size, MUTED, false);
            if hover && ui.clicked(r) {
                open = Some(tour.id);
            }
        }
        if let Some(id) = open {
            self.selected_tournament = Some(id);
            self.tournament_password.clear();
        }
    }

    fn draw_tournament(&mut self, ui: &mut Ui, area: Rect, tour: &TournamentInfo) {
        let me = self.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
        if ui.icon_button(
            Rect::new(area.x, area.y, 34.0, 34.0),
            Icon::Back,
            t("All tournaments"),
            true,
        ) {
            self.selected_tournament = None;
            return;
        }
        let (label, color) = state_label(tour.state);
        let pw = ui.text_size(label, 11.0, true).x + 12.0;
        ui.pill(label, area.x + area.w - pw, area.y + (34.0 - 17.0) / 2.0, color);
        let shown = crate::i18n::tournament_name(&tour.name);
        let (name, size) = fit_text(ui, &shown, 20.0, area.w - 46.0 - pw - 12.0, true);
        ui.text(&name, area.x + 46.0, area.y + (34.0 - size) / 2.0, size, GOLD, true);
        let mut y = area.y + 34.0 + 12.0;
        let kind = if tour.rated { t("rated games") } else { t("casual games") };
        let kind = format!("{kind} · {}", crate::i18n::rules_name_inline(tour.rules));
        let arena = tour.format == snook_legend_proto::TournamentFormat::Arena;
        let line = if arena {
            tw(
                "Arena · host {host} · {n} players · {kind}",
                &[("host", &tour.host), ("n", &tour.players.len()), ("kind", &kind)],
            )
        } else {
            tw(
                "Host {host} · {n}/{size} players · {kind}",
                &[
                    ("host", &tour.host),
                    ("n", &tour.players.len()),
                    ("size", &tour.size),
                    ("kind", &kind),
                ],
            )
        };
        for l in ui.wrap(&line, area.w - 4.0, 13.0).iter().take(2) {
            ui.text(l, area.x + 2.0, y, 13.0, MUTED, false);
            y += 18.0;
        }
        // Entry, prizes and times.
        let mut facts = Vec::new();
        if tour.entry_fee > 0 {
            facts.push(tw("Entry {n} coins", &[("n", &tour.entry_fee)]));
        }
        // A themed tournament's prizes are put up by the house, entry or not.
        if tour.prizes[0] > 0 {
            facts.push(tw("Prizes {a} + {b}", &[("a", &tour.prizes[0]), ("b", &tour.prizes[1])]));
        }
        let now = crate::game::unix_now();
        if let Some(start) = tour.starts_at.filter(|_| tour.state == TournamentState::Registering) {
            let mins = ((start - now).max(0) + 59) / 60;
            facts.push(tw("starts in {n} min", &[("n", &mins)]));
        }
        if let Some(end) = tour.ends_at.filter(|_| tour.state == TournamentState::Running) {
            let left = (end - now).max(0);
            facts.push(tw(
                "{m}:{s} left",
                &[("m", &(left / 60)), ("s", &format!("{:02}", left % 60))],
            ));
        }
        if !facts.is_empty() {
            let (facts, size) = fit_text(ui, &facts.join("  ·  "), 13.0, area.w - 4.0, true);
            ui.text(&facts, area.x + 2.0, y, size, GOLD, true);
            y += 18.0;
        }
        y += 10.0;
        let joined = tour.players.iter().any(|p| p.username == me);
        let is_host = tour.host == me;
        // A line of explanation over a "Withdraw" button.
        let withdraw = |ui: &mut Ui, text: &str, y: &mut f32| {
            for l in ui.wrap(text, area.w - 4.0, 13.0) {
                ui.text(&l, area.x + 2.0, *y, 13.0, GOLD, false);
                *y += 18.0;
            }
            *y += 6.0;
            let label = t("Withdraw");
            let w = button_w(ui, label, 32.0).max(140.0);
            let clicked = ui.button(Rect::new(area.x, *y, w, 32.0), label, true);
            *y += 32.0 + 14.0;
            clicked
        };
        if tour.state == TournamentState::Registering {
            let bw = (area.w - 8.0) / 2.0;
            if joined {
                let leave = if is_host { t("Cancel tournament") } else { t("Leave") };
                if ui.button(Rect::new(area.x, y, bw, 36.0), leave, true) {
                    self.send(ClientMsg::LeaveTournament { id: tour.id });
                }
                if is_host {
                    let start = Rect::new(area.x + bw + 8.0, y, bw, 36.0);
                    // At least two players: before that it can't start.
                    if tour.players.len() >= 2 {
                        if ui.primary_button(start, t("Start now")) {
                            self.send(ClientMsg::StartTournament { id: tour.id });
                        }
                    } else {
                        ui.button(start, t("Start now"), false);
                    }
                }
            } else {
                let full = tour.players.len() >= tour.size as usize;
                if tour.private {
                    let field = Rect::new(area.x, y, bw, 36.0);
                    let enter = ui.password_input(35, field, &mut self.tournament_password, 32);
                    placeholder(ui, 35, field, &self.tournament_password, t("password"));
                    if enter {
                        let password = Some(self.tournament_password.clone());
                        self.send(ClientMsg::JoinTournament { id: tour.id, password });
                    }
                }
                let jr = if tour.private {
                    Rect::new(area.x + bw + 8.0, y, bw, 36.0)
                } else {
                    Rect::new(area.x, y, area.w, 36.0)
                };
                let label = match (full, tour.entry_fee) {
                    (true, _) => t("Join the waiting list").to_string(),
                    (false, 0) => t("Join").to_string(),
                    (false, fee) => tw("Join · {n} coins", &[("n", &fee)]),
                };
                if ui.primary_button(jr, &label) {
                    let password = tour.private.then(|| self.tournament_password.clone());
                    self.send(ClientMsg::JoinTournament { id: tour.id, password });
                }
            }
            y += 36.0 + 14.0;
        } else if tour.state == TournamentState::Running && arena {
            // An arena can be joined while it runs.
            if !joined {
                let label = match tour.entry_fee {
                    0 => t("Join").to_string(),
                    fee => tw("Join · {n} coins", &[("n", &fee)]),
                };
                // (A private one asks for its password here as well.)
                let bw = (area.w - 8.0) / 2.0;
                let mut enter = false;
                if tour.private {
                    let field = Rect::new(area.x, y, bw, 36.0);
                    enter = ui.password_input(35, field, &mut self.tournament_password, 32);
                    placeholder(ui, 35, field, &self.tournament_password, t("password"));
                }
                let jr = if tour.private {
                    Rect::new(area.x + bw + 8.0, y, bw, 36.0)
                } else {
                    Rect::new(area.x, y, area.w, 36.0)
                };
                if ui.primary_button(jr, &label) || enter {
                    let password = tour.private.then(|| self.tournament_password.clone());
                    self.send(ClientMsg::JoinTournament { id: tour.id, password });
                }
                y += 36.0 + 14.0;
            } else if withdraw(
                ui,
                t("You are paired as soon as you and an opponent are free."),
                &mut y,
            ) {
                self.send(ClientMsg::LeaveTournament { id: tour.id });
            }
        } else if tour.state == TournamentState::Running && joined {
            let alive = tour.champion.is_none()
                && !tour.rounds.iter().flatten().any(|m| {
                    m.winner.is_some_and(|w| m.players[1 - w].as_deref() == Some(me.as_str()))
                });
            if alive
                && withdraw(
                    ui,
                    t("Your next match starts by itself when both players are free."),
                    &mut y,
                )
            {
                self.send(ClientMsg::LeaveTournament { id: tour.id });
            }
        }
        if let Some(champion) = &tour.champion {
            let r = Rect::new(area.x, y, area.w, 44.0);
            rrect(r, 8.0, Color::new(0.91, 0.75, 0.38, 0.14));
            rrect_lines(r, 8.0, 1.0, Color::new(0.91, 0.75, 0.38, 0.4));
            crate::icons::draw(Icon::Trophy, vec2(r.x + 24.0, r.y + 22.0), 22.0, GOLD);
            let text = tw("Champion: {name}", &[("name", champion)]);
            ui.text(&text, r.x + 46.0, r.y + 13.0, 16.0, GOLD, true);
            y += r.h + 14.0;
        }
        draw_line(area.x, y, area.x + area.w, y, 1.0, BORDER);
        y += 12.0;
        let rest = Rect::new(area.x, y, area.w, area.y + area.h - y);
        if arena && !tour.standings.is_empty() {
            // Points, best first; a flame for a winning streak.
            for (k, s) in tour.standings.iter().enumerate() {
                let ry = rest.y + k as f32 * 28.0;
                if ry + 24.0 > rest.y + rest.h {
                    break;
                }
                let color = if s.username == me { GOLD } else { CREAM };
                let pts = tw("{n} pts · {g} games", &[("n", &s.points), ("g", &s.games)]);
                let pw = ui.text_size(&pts, 13.0, false).x;
                ui.text(&format!("{}.", k + 1), rest.x + 2.0, ry + 2.0, 13.0, MUTED, false);
                let streak = if s.streak >= 2 { format!("×{}", s.streak) } else { String::new() };
                let sw_ =
                    if streak.is_empty() { 0.0 } else { ui.text_size(&streak, 12.0, true).x + 8.0 };
                let room = rest.w - 30.0 - pw - 16.0 - sw_;
                let (name, size) = fit_text(ui, &s.username, 15.0, room, true);
                ui.text(&name, rest.x + 30.0, ry, size, color, true);
                if !streak.is_empty() {
                    let nw = ui.text_size(&name, size, true).x;
                    ui.text(&streak, rest.x + 38.0 + nw, ry + 2.0, 12.0, ALERT, true);
                }
                ui.text(&pts, rest.x + rest.w - pw - 2.0, ry + 2.0, 13.0, MUTED, false);
            }
        } else if tour.rounds.is_empty() {
            // Sign-up list.
            for (k, p) in tour.players.iter().enumerate() {
                let ry = rest.y + k as f32 * 28.0;
                if ry + 24.0 > rest.y + rest.h {
                    break;
                }
                let rating = t(p.tier().name()).to_string();
                let rw = ui.text_size(&rating, 13.0, false).x;
                ui.text(&format!("{}.", k + 1), rest.x + 2.0, ry + 2.0, 13.0, MUTED, false);
                ui.player_name(p, rest.x + 30.0, ry, 15.0, rest.w - 30.0 - rw - 16.0);
                ui.text(&rating, rest.x + rest.w - rw - 2.0, ry + 2.0, 13.0, MUTED, false);
            }
        } else {
            draw_bracket(ui, rest, tour, &me);
        }
    }

    fn draw_friends(&mut self, ui: &mut Ui, area: Rect) -> Option<OnlineAction> {
        let area = self.friends_switch(ui, area);
        if self.social.recent_view {
            return self.draw_recent(ui, area);
        }
        let mut y = area.y;
        let add = t("Add");
        let aw = button_w(ui, add, 34.0).max(110.0);
        let input = Rect::new(area.x, y, area.w - aw - 8.0, 34.0);
        let enter = ui.text_input(36, input, &mut self.friend_input, 16);
        placeholder(ui, 36, input, &self.friend_input, t("Add a friend by username"));
        let ok = valid_username(self.friend_input.trim());
        if (ui.button(Rect::new(input.x + input.w + 8.0, y, aw, 34.0), add, ok) || enter) && ok {
            self.send(ClientMsg::FriendRequest { username: self.friend_input.trim().to_string() });
            self.friend_input.clear();
        }
        y += 34.0 + 12.0;
        let mut action = None;
        let mut answer = None;
        let accept = t("Accept");
        let decline = t("Decline");
        let bw = button_w(ui, accept, 30.0).max(button_w(ui, decline, 30.0)).max(90.0);
        for from in self.friends.incoming.clone() {
            let r = Rect::new(area.x, y, area.w, 44.0);
            rrect(r, 7.0, Color::new(0.91, 0.75, 0.38, 0.12));
            rrect_lines(r, 7.0, 1.0, Color::new(0.91, 0.75, 0.38, 0.35));
            let text = tw("{name} wants to be friends", &[("name", &from)]);
            let room = r.w - 14.0 - bw * 2.0 - 8.0 - 20.0;
            let (text, size) = fit_text(ui, &text, 14.0, room, false);
            ui.text(&text, r.x + 14.0, r.y + (r.h - size) / 2.0, size, CREAM, false);
            let by = r.y + 7.0;
            let decline_r = Rect::new(r.x + r.w - 7.0 - bw, by, bw, 30.0);
            if ui.primary_button(Rect::new(decline_r.x - 8.0 - bw, by, bw, 30.0), accept) {
                answer = Some((from.clone(), true));
            }
            if ui.button(decline_r, decline, true) {
                answer = Some((from.clone(), false));
            }
            y += r.h + 8.0;
        }
        if let Some((username, accept)) = answer {
            self.send(ClientMsg::FriendAnswer { username, accept });
        }
        if !self.friends.incoming.is_empty() {
            y += 4.0;
        }
        let list = Rect::new(area.x, y, area.w, area.y + area.h - y);
        let total = self.friends.friends.len() + self.friends.outgoing.len();
        if total == 0 {
            if self.friends.incoming.is_empty() {
                let text = t("No friends yet. Add someone you played with!");
                empty_state(ui, list, Glyph::Icon(Icon::People), text, None);
            }
            return action;
        }
        let row_h = 48.0;
        let visible = (list.h / row_h).floor().max(1.0) as usize;
        self.list_first = ui.scroll_list(list, self.list_first, total, visible);
        let row_w = list.w - if total > visible { 12.0 } else { 0.0 };
        let invite_label = t("Invite");
        let watch_label = t("Watch");
        let bw = button_w(ui, invite_label, 30.0).max(button_w(ui, watch_label, 30.0)).max(96.0);
        let mut invite = None;
        let mut remove = None;
        let mut watch = None;
        let mut daily = None;
        let friends = self.friends.friends.clone();
        let outgoing = self.friends.outgoing.clone();
        let rows = friends.iter().map(Some).chain(outgoing.iter().map(|_| None));
        for (k, (friend, i)) in rows.zip(0..).skip(self.list_first).take(visible).enumerate() {
            let r = Rect::new(list.x, list.y + k as f32 * row_h, row_w, row_h - 6.0);
            // Remove (×) at the far right; the main button before it, and a
            // daily game (friends who are away too) before that.
            let x_r = Rect::new(r.x + r.w - 6.0 - 30.0, r.y + 6.0, 30.0, r.h - 12.0);
            let button = Rect::new(x_r.x - 8.0 - bw, r.y + 6.0, bw, r.h - 12.0);
            let corr = Rect::new(button.x - 6.0 - 30.0, r.y + 6.0, 30.0, r.h - 12.0);
            match friend {
                Some(f) => {
                    let hover = row_card(ui, r, true);
                    if f.online {
                        Self::status_dot(&f.player, r.x + 14.0, r.y + r.h / 2.0);
                    } else {
                        let grey = Color::new(0.4, 0.4, 0.4, 1.0);
                        draw_circle(r.x + 14.0, r.y + r.h / 2.0, 4.5, grey);
                    }
                    let name_w = corr.x - 12.0 - (r.x + 28.0);
                    ui.player_name(&f.player, r.x + 28.0, r.y + 5.0, 15.0, name_w);
                    let sub = if f.online {
                        Self::status_text(&f.player)
                    } else {
                        last_seen(f.last_seen)
                    };
                    let status = &f.player.status_message;
                    sub_line(ui, &sub, status, r.x + 28.0, r.y + 23.0, name_w);
                    if hover
                        && ui.clicked(r)
                        && !button.contains(ui.mouse())
                        && !x_r.contains(ui.mouse())
                        && !corr.contains(ui.mouse())
                    {
                        action = Some(OnlineAction::Profile(f.player.username.clone()));
                    }
                    let tip = t("Challenge to a daily game");
                    if ui.icon_button_ex(corr, Icon::Hourglass, tip, true, true) {
                        daily = Some(f.player.username.clone());
                    }
                    if f.online && f.player.status == PlayerStatus::Playing {
                        // Watch their game.
                        if ui.button(button, watch_label, true) {
                            watch = Some(f.player.username.clone());
                        }
                    } else {
                        let can = f.online
                            && f.player.status == PlayerStatus::Available
                            && !f.player.focus
                            && self.outgoing.is_none();
                        if ui.button(button, invite_label, can) {
                            invite = Some(f.player.username.clone());
                        }
                    }
                    if ui.icon_button_ex(x_r, Icon::Close, t("Remove friend"), true, true) {
                        remove = Some(f.player.username.clone());
                    }
                }
                None => {
                    row_card(ui, r, false);
                    // Not a friend yet: a hollow dot.
                    let c = vec2(r.x + 14.0, r.y + r.h / 2.0);
                    crate::paint::draw_circle_lines(c.x, c.y, 4.5, 1.2, MUTED);
                    let name = &outgoing[i - friends.len()];
                    let (shown, size) =
                        fit_text(ui, name, 15.0, button.x - 12.0 - (r.x + 28.0), true);
                    ui.text(&shown, r.x + 28.0, r.y + 5.0, size, MUTED, true);
                    ui.text(t("request sent"), r.x + 28.0, r.y + 23.0, 12.0, MUTED, false);
                    if ui.icon_button_ex(x_r, Icon::Close, t("Cancel request"), true, true) {
                        remove = Some(name.clone());
                    }
                }
            }
        }
        if let Some(to) = invite {
            self.open_invite(&to);
        }
        if let Some(username) = remove {
            self.send(ClientMsg::FriendRemove { username });
        }
        if let Some(name) = watch {
            self.spectate(&name);
        }
        if let Some(to) = daily {
            crate::correspondence::challenge(self, &to);
        }
        action
    }
}

/// "seen 5 min ago", "seen yesterday"… (friends only).
pub fn last_seen(at: i64) -> String {
    if at <= 0 {
        return t("offline").to_string();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let s = (now - at).max(0);
    let n = |v: i64| v.to_string();
    match s {
        0..60 => t("seen just now").to_string(),
        60..3600 => tw("seen {n} min ago", &[("n", &n(s / 60))]),
        3600..86_400 => tw("seen {n} h ago", &[("n", &n(s / 3600))]),
        86_400..172_800 => t("seen yesterday").to_string(),
        172_800..2_592_000 => tw("seen {n} days ago", &[("n", &n(s / 86_400))]),
        _ => tw("seen {n} months ago", &[("n", &n(s / 2_592_000))]),
    }
}

fn state_label(state: TournamentState) -> (&'static str, Color) {
    match state {
        TournamentState::Registering => (t("open"), Color::new(0.4, 0.9, 0.5, 1.0)),
        TournamentState::Running => (t("playing"), Color::new(1.0, 0.7, 0.3, 1.0)),
        TournamentState::Finished => (t("finished"), MUTED),
    }
}

/// The bracket: one column per round, each match a small card.
fn draw_bracket(ui: &mut Ui, area: Rect, tour: &TournamentInfo, me: &str) {
    ui.block(area);
    let rounds = tour.rounds.len();
    if rounds == 0 {
        return;
    }
    // Columns share the width (cards no wider than reads well), with a gap
    // between them for the connectors.
    let gap = 24.0;
    let col_w = ((area.w + gap) / rounds as f32).min(250.0);
    let card_w = col_w - gap;
    let card_h = 46.0f32;
    let line = Color::new(0.45, 0.31, 0.16, 0.8);
    let profile = |name: &str| tour.players.iter().find(|p| p.username == name);
    // Compact: no more than ~2.4 cards' height per first-round match.
    let first = tour.rounds[0].len().max(1) as f32;
    let height = (area.h - 26.0).min(first * card_h * 2.4);
    let centre = |round: usize, i: usize| {
        let slot_h = height / tour.rounds[round].len().max(1) as f32;
        area.y + 26.0 + slot_h * (i as f32 + 0.5)
    };
    for (r, round) in tour.rounds.iter().enumerate() {
        let x = area.x + r as f32 * col_w;
        let title = if r + 1 == rounds {
            t("Final").to_string()
        } else if r + 2 == rounds {
            t("Semi-finals").to_string()
        } else {
            tw("Round {n}", &[("n", &(r + 1))])
        };
        ui.text(&title, x + 2.0, area.y, 13.0, MUTED, true);
        for (i, m) in round.iter().enumerate() {
            let cy = centre(r, i);
            let card = Rect::new(x, cy - card_h / 2.0, card_w, card_h);
            // Elbow to the match the winner goes on to.
            if r + 1 < rounds {
                let ny = centre(r + 1, i / 2);
                let mx = card.x + card.w + gap / 2.0;
                draw_line(card.x + card.w, cy, mx, cy, 1.2, line);
                draw_line(mx, cy, mx, ny, 1.2, line);
                draw_line(mx, ny, x + col_w, ny, 1.2, line);
            }
            let involved = m.players.iter().any(|p| p.as_deref() == Some(me));
            rrect(card, 6.0, Color::new(0.0, 0.0, 0.0, if involved { 0.5 } else { 0.32 }));
            let edge = if m.live {
                Color::new(1.0, 0.7, 0.3, 0.9)
            } else if involved {
                Color::new(GOLD.r, GOLD.g, GOLD.b, 0.7)
            } else {
                BORDER
            };
            rrect_lines(card, 6.0, 1.0, edge);
            draw_line(
                card.x + 6.0,
                cy,
                card.x + card.w - 6.0,
                cy,
                1.0,
                Color::new(1.0, 1.0, 1.0, 0.06),
            );
            for seat in 0..2 {
                let y = card.y + 4.0 + seat as f32 * 21.0;
                let won = m.winner == Some(seat);
                let lost = m.winner.is_some() && !won;
                match &m.players[seat] {
                    Some(name) => {
                        let color = match profile(name) {
                            Some(p) if !lost => name_color(p),
                            _ if lost => Color::new(0.5, 0.46, 0.42, 1.0),
                            _ => CREAM,
                        };
                        let country = profile(name).map(|p| p.country.clone()).unwrap_or_default();
                        ui.name_with_flag(
                            name,
                            &country,
                            color,
                            card.x + 8.0,
                            y + 1.0,
                            13.0,
                            card.w - 30.0,
                        );
                        if won {
                            let c = vec2(card.x + card.w - 12.0, y + 9.0);
                            crate::icons::draw(Icon::Check, c, 12.0, GOLD);
                        }
                    }
                    None => ui.text("—", card.x + 8.0, y + 1.0, 13.0, MUTED, false),
                }
            }
            if m.live {
                let pulse = 0.5 + 0.5 * (get_time() as f32 * 4.0).sin();
                draw_circle(
                    card.x + card.w - 12.0,
                    card.y + card_h / 2.0,
                    3.5,
                    Color::new(1.0, 0.7, 0.3, pulse),
                );
            }
        }
    }
}

fn same_players(a: &Match, b: &Match) -> bool {
    let mut x = a.names.clone();
    let mut y = b.names.clone();
    x.sort();
    y.sort();
    x == y
}

/// Draws the round trip to the server, small, in a corner (during matches).
pub fn draw_ping(ui: &Ui, rtt: Option<std::time::Duration>, x_right: f32, y: f32) {
    let Some(rtt) = rtt else { return };
    let ms = rtt.as_millis();
    let color = if ms < 80 {
        Color::new(0.45, 0.9, 0.5, 0.85)
    } else if ms < 200 {
        Color::new(1.0, 0.8, 0.35, 0.9)
    } else {
        Color::new(1.0, 0.45, 0.4, 0.95)
    };
    let text = format!("{ms} ms");
    let w = ui.text_size(&text, 12.0, true).x;
    draw_circle(x_right - w - 10.0, y + 7.0, 3.5, color);
    ui.text(&text, x_right - w, y, 12.0, color, true);
}

/// How long the name of a player who just came online stays by the count.
const ARRIVAL_SECS: f64 = 10.0;

impl Online {
    /// How many are online on the server (the lounge's count, this player
    /// included), while signed in on a working connection to it.
    pub fn online_count(&self) -> Option<usize> {
        (self.live() && !self.lan).then_some(self.players.len())
    }

    /// Over a game that isn't online (the computer, a drill): for a moment
    /// after someone comes online, who, in gold, right-aligned at `x_right`
    /// (someone to invite may have come; not when `quiet`: in focus mode).
    /// How many are online is in the rules' badge at the top. Returns
    /// whether it was drawn.
    pub fn draw_arrival(&self, ui: &Ui, x_right: f32, y: f32, quiet: bool) -> bool {
        if self.online_count().is_none() || quiet {
            return false;
        }
        let now = get_time();
        let Some((name, age)) = self
            .arrived
            .as_ref()
            .filter(|(name, at)| {
                now - at < ARRIVAL_SECS && self.players.iter().any(|p| p.username == *name)
            })
            .map(|(name, at)| (name.clone(), now - at))
        else {
            return false;
        };
        // The dot breathes beside the name for the moment it shows.
        let pulse = (age as f32 * 5.0).sin().abs();
        draw_circle(x_right - 5.0, y + 7.0, 4.0 + pulse * 1.5, Color::new(0.35, 0.85, 0.45, 0.95));
        let came = tw("{name} came online", &[("name", &name)]);
        let w = ui.text_size(&came, 12.0, true).x;
        ui.text(&came, x_right - 16.0 - w, y + 1.0, 12.0, GOLD, true);
        true
    }

    /// The players online, compactly: a green dot, a person and the number
    /// (then `extra`, muted), right-aligned at `x_right` and centred on
    /// `cy`. Hovering says "Users online" and who they are. Its rect.
    #[allow(clippy::too_many_arguments)]
    fn draw_presence(
        &self,
        ui: &Ui,
        x_right: f32,
        cy: f32,
        size: f32,
        extra: &str,
        pulse: f32,
        color: Color,
    ) -> Rect {
        let n = self.players.len();
        let w = presence_width(ui, n, size, extra);
        let x = x_right - w;
        let icon = size + 2.0;
        draw_circle(x + 4.0, cy, 3.5 + 1.5 * pulse, Color::new(0.35, 0.85, 0.45, 0.95));
        crate::icons::draw(Icon::Person, vec2(x + 13.0 + icon / 2.0, cy), icon, color);
        let number = n.to_string();
        let nx = x + 13.0 + icon + 3.0;
        ui.text(&number, nx, crate::ranking::text_y(cy, size), size, color, true);
        if !extra.is_empty() {
            let ex = nx + ui.text_size(&number, size, true).x + 8.0;
            ui.text(extra, ex, crate::ranking::text_y(cy, size - 2.0), size - 2.0, MUTED, false);
        }
        let r = Rect::new(x - 4.0, cy - size / 2.0 - 6.0, w + 8.0, size + 12.0);
        if ui.hovering(r) {
            let me = self.me.as_ref().map(|me| me.username.as_str());
            let others: Vec<&str> = self
                .players
                .iter()
                .map(|p| p.username.as_str())
                .filter(|name| Some(*name) != me)
                .collect();
            let who = if others.is_empty() {
                t("Nobody else is online right now.").to_string()
            } else {
                let mut names = others.iter().take(12).copied().collect::<Vec<_>>().join(", ");
                if others.len() > 12 {
                    names.push_str(", …");
                }
                names
            };
            ui.tooltip(&format!("{}\n{who}", t("Users online")), r);
        }
        r
    }
}

/// How wide [`Online::draw_presence`] draws `n` at `size`, with `extra`.
fn presence_width(ui: &Ui, n: usize, size: f32, extra: &str) -> f32 {
    let number = ui.text_size(&n.to_string(), size, true).x;
    let extra = if extra.is_empty() { 0.0 } else { 8.0 + ui.text_size(extra, size - 2.0, false).x };
    13.0 + size + 2.0 + 3.0 + number + extra
}

/// How wide a button `h` high must be for `label` (the toolkit's label size,
/// with room on both sides).
pub(crate) fn button_w(ui: &Ui, label: &str, h: f32) -> f32 {
    ui.text_size(label, (h * 0.45).clamp(13.0, 22.0), true).x + 32.0
}

/// `text` at `size` if it fits `width`; else smaller, down to 11 px; cut
/// with "…" only if even that is too long.
pub(crate) fn fit_text(ui: &Ui, text: &str, size: f32, width: f32, bold: bool) -> (String, f32) {
    let min = size.min(11.0);
    let mut s = size;
    while s > min && ui.text_size(text, s, bold).x > width {
        s = (s - 0.5).max(min);
    }
    if ui.text_size(text, s, bold).x <= width {
        return (text.to_string(), s);
    }
    let mut cut: String = text.to_string();
    while !cut.is_empty() && ui.text_size(&format!("{cut}…"), s, bold).x > width {
        cut.pop();
    }
    (format!("{}…", cut.trim_end()), s)
}

/// How wide [`Ui::player_name`] draws `p` at `size`: flag, name and crown.
fn name_width(ui: &Ui, p: &PlayerInfo, size: f32) -> f32 {
    let flag = if crate::flags::name(&p.country).is_empty() {
        0.0
    } else {
        crate::flags::width((size * 0.78).round()) + 6.0
    };
    let crown = if p.vip { size + 4.0 } else { 0.0 };
    flag + ui.text_size(&p.username, size, true).x + crown
}

/// Muted text in an empty field nobody is typing in, where the typed text
/// will be.
pub(crate) fn placeholder(ui: &Ui, id: u64, field: Rect, value: &str, text: &str) {
    if value.is_empty() && ui.focus != Some(id) {
        let (text, size) = fit_text(ui, text, 14.0, field.w - 18.0, false);
        ui.text(&text, field.x + 9.0, field.y + (field.h - size) / 2.0, size, MUTED, false);
    }
}

/// A chip in the lounge's header (the player, the coins): lighter with a
/// gold edge under the pointer when it can be clicked. Returns that hover.
fn header_chip(ui: &mut Ui, r: Rect, active: bool) -> bool {
    let hover = active && ui.hovering(r);
    ui.block(r);
    let bg = Color::new(0.20, 0.14, 0.09, if hover { 0.95 } else { 0.7 });
    rrect_gradient(r, 8.0, shade(bg, 0.06), shade(bg, -0.2));
    rrect_lines(r, 8.0, 1.2, if hover { GOLD } else { BORDER });
    hover
}

/// A list row's card. Rows that open something get lighter under the
/// pointer. Returns whether the pointer is over it.
pub(crate) fn row_card(ui: &Ui, r: Rect, clickable: bool) -> bool {
    let hover = ui.hovering(r);
    let lit = hover && clickable;
    rrect(r, 7.0, Color::new(1.0, 1.0, 1.0, if lit { 0.075 } else { 0.035 }));
    if lit {
        rrect_lines(r, 7.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.35));
    }
    hover
}

/// Waiting for the server: a spinner in the middle of `area`.
pub(crate) fn loading(ui: &Ui, area: Rect) {
    let c = vec2(area.x + area.w / 2.0, area.y + (area.h * 0.4).min(160.0));
    crate::ui::spinner(c, 14.0, GOLD);
    ui.text_centered(t("Loading…"), c.x, c.y + 28.0, 13.0, MUTED, false);
}

/// What an empty state shows over its line of text.
#[derive(Clone, Copy)]
pub(crate) enum Glyph {
    Icon(Icon),
    /// A speech bubble (the chat).
    Chat,
}

/// A speech bubble `s` wide centred on `c`, drawn in outline.
/// A speech bubble with three dots, `s` across, centred on `c`.
pub(crate) fn chat_glyph(c: Vec2, s: f32, color: Color) {
    let body = Rect::new(c.x - s * 0.5, c.y - s * 0.42, s, s * 0.68);
    rrect_lines(body, s * 0.2, 2.0, color);
    // The tail, bottom left.
    let a = vec2(body.x + s * 0.24, body.y + body.h - 1.0);
    let tip = vec2(body.x + s * 0.14, body.y + body.h + s * 0.22);
    draw_line(a.x, a.y, tip.x, tip.y, 2.0, color);
    draw_line(tip.x, tip.y, a.x + s * 0.22, a.y, 2.0, color);
    for k in -1..=1 {
        draw_circle(c.x + k as f32 * s * 0.22, body.y + body.h / 2.0, 2.0, color);
    }
}

/// Wraps `text` into as few lines as `width` allows, but with lines of
/// even length (no lone word on the last line).
fn balanced_wrap(ui: &Ui, text: &str, width: f32, size: f32) -> Vec<String> {
    let lines = ui.wrap(text, width, size);
    if lines.len() < 2 {
        return lines;
    }
    let total = ui.text_size(text, size, false).x;
    let mut w = (total / lines.len() as f32).max(width * 0.5);
    while w < width {
        let even = ui.wrap(text, w, size);
        if even.len() == lines.len() {
            return even;
        }
        w += 12.0;
    }
    lines
}

/// An empty list: a muted glyph, one line of muted text and, when there is
/// an obvious next step, a button for it; centred in `area` (a little above
/// the middle, where the eye expects it). True when the button is clicked.
pub(crate) fn empty_state(
    ui: &mut Ui,
    area: Rect,
    glyph: Glyph,
    text: &str,
    action: Option<&str>,
) -> bool {
    let lines = balanced_wrap(ui, text, (area.w - 32.0).min(400.0), 14.0);
    let button_h = if action.is_some() { 16.0 + 36.0 } else { 0.0 };
    let h = 60.0 + 14.0 + lines.len() as f32 * 20.0 + button_h;
    let top = (area.y + (area.h - h) * 0.4).max(area.y + 8.0);
    let cx = area.x + area.w / 2.0;
    let c = vec2(cx, top + 30.0);
    draw_circle(c.x, c.y, 30.0, Color::new(1.0, 1.0, 1.0, 0.045));
    let color = Color::new(MUTED.r, MUTED.g, MUTED.b, 0.8);
    match glyph {
        Glyph::Icon(icon) => crate::icons::draw(icon, c, 30.0, color),
        Glyph::Chat => chat_glyph(c, 30.0, color),
    }
    let mut y = top + 60.0 + 14.0;
    for line in &lines {
        ui.text_centered(line, cx, y, 14.0, MUTED, false);
        y += 20.0;
    }
    let Some(label) = action else { return false };
    let w = button_w(ui, label, 36.0).max(180.0).min(area.w - 16.0);
    ui.button(Rect::new(cx - w / 2.0, y + 16.0, w, 36.0), label, true)
}

/// Under 18 without the chat: what other players wrote (status messages,
/// notes on the board, gift messages) is left out; the server already keeps
/// their chat lines away.
fn hide_players_text(msg: &mut ServerMsg, me: &str) {
    let other = |name: &str| !name.eq_ignore_ascii_case(me);
    let player = |p: &mut snook_legend_proto::PlayerInfo| {
        if other(&p.username) {
            p.status_message.clear();
        }
    };
    match msg {
        ServerMsg::Lounge { players } => players.iter_mut().for_each(player),
        ServerMsg::PlayerUpdated { player: p } => player(p),
        ServerMsg::Profile { profile } => player(&mut profile.player),
        ServerMsg::RecentOpponents { players } => {
            players.iter_mut().for_each(|r| player(&mut r.player))
        }
        ServerMsg::SeekBoard { notes } => {
            notes.iter_mut().filter(|n| other(&n.username)).for_each(|n| n.text.clear())
        }
        ServerMsg::SeekPosted { note } if other(&note.username) => note.text.clear(),
        ServerMsg::NewGifts { gifts } | ServerMsg::Gifts { gifts } => {
            gifts.iter_mut().filter(|g| other(&g.from)).for_each(|g| g.message.clear())
        }
        ServerMsg::Friends { friends, .. } => {
            friends.iter_mut().for_each(|f| player(&mut f.player))
        }
        // A tournament's name its host wrote (not the server's cups).
        ServerMsg::Tournaments { tournaments } => tournaments.iter_mut().for_each(|t| {
            if !t.host.is_empty() && other(&t.host) {
                t.name = tw("{name}'s tournament", &[("name", &t.host)]);
            }
        }),
        ServerMsg::TournamentUpdated { tournament: t } if !t.host.is_empty() && other(&t.host) => {
            t.name = tw("{name}'s tournament", &[("name", &t.host)]);
        }
        _ => {}
    }
}

/// The account's age group and chat (see [`snook_legend_proto::ServerMsg::Age`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgeState {
    /// `None`: not given yet (the chat is off; asked when the player first
    /// writes to others).
    pub minor: Option<bool>,
    pub chat_allowed: bool,
}

impl Online {
    /// The chat with other players is off: the player hasn't said they are
    /// 18 or older, nor, under 18, has a parent or guardian turned it on.
    /// The server passes on no messages either way (what others write is
    /// not shown), and what the player writes waits for the dialog that
    /// turns it on. Never on a LAN server, nor with a server that doesn't
    /// say (older than [`snook_legend_proto::AGE_PROTOCOL`]).
    pub fn chat_off(&self) -> bool {
        !self.lan && self.age.is_some_and(|a| a.minor != Some(false) && !a.chat_allowed)
    }
}

/// `msg` carries text for other players (see [`Online::send_text`]).
fn has_text(msg: &ClientMsg) -> bool {
    match msg {
        ClientMsg::Chat { text } | ClientMsg::MatchChat { text } => !text.is_empty(),
        ClientMsg::SetStatusMessage { text } | ClientMsg::PostSeek { text, .. } => !text.is_empty(),
        ClientMsg::GiftItem { message, .. } => !message.is_empty(),
        _ => false,
    }
}

/// The address of the server's public status for its game address
/// (`wss://host/ws` → `https://host/api/status`).
fn status_url(server: &str) -> Option<String> {
    let (scheme, rest) = if let Some(rest) = server.strip_prefix("wss://") {
        ("https", rest)
    } else {
        ("http", server.strip_prefix("ws://")?)
    };
    let host = rest.split('/').next().filter(|h| !h.is_empty())?;
    Some(format!("{scheme}://{host}/api/status"))
}

/// The protocol the server at `server` speaks, asked in the background
/// (`None` when it can't be found out).
fn server_protocol(server: &str) -> std::sync::mpsc::Receiver<Option<u32>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let url = status_url(server);
    std::thread::spawn(move || {
        let protocol = url.and_then(|url| {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(20)))
                .build()
                .into();
            let text = agent.get(&url).call().ok()?.body_mut().read_to_string().ok()?;
            let status: serde_json::Value = serde_json::from_str(&text).ok()?;
            u32::try_from(status.get("protocol")?.as_u64()?).ok()
        });
        let _ = tx.send(protocol);
    });
    rx
}

/// Whether a message's seats and players are ones a table of two has.
fn plausible(msg: &ServerMsg) -> bool {
    match msg {
        ServerMsg::MatchStarted { seat, .. }
        | ServerMsg::ShotTaken { seat, .. }
        | ServerMsg::TimeFoul { seat, .. } => *seat < 2,
        ServerMsg::Reaction { seat, .. } => *seat <= snook_legend_proto::SPECTATOR_SEAT,
        ServerMsg::MatchEnded { result } => result.winner < 2,
        ServerMsg::ShotResult { report, .. } => {
            report.shooter < 2 && report.game_over.is_none_or(|g| g.winner < 2)
        }
        // A bracket's winner is one of its match's two players.
        ServerMsg::Tournaments { tournaments } => tournaments.iter().all(sound_bracket),
        ServerMsg::TournamentUpdated { tournament } => sound_bracket(tournament),
        _ => true,
    }
}

fn sound_bracket(t: &snook_legend_proto::TournamentInfo) -> bool {
    t.rounds.iter().flatten().all(|m| m.winner.is_none_or(|w| w < 2))
}

#[cfg(test)]
mod tests {
    use super::status_url;

    #[test]
    fn the_status_is_next_to_the_game_address() {
        assert_eq!(
            status_url("wss://snook.example.com/ws").as_deref(),
            Some("https://snook.example.com/api/status")
        );
        assert_eq!(
            status_url("ws://127.0.0.1:7878/ws").as_deref(),
            Some("http://127.0.0.1:7878/api/status")
        );
        assert_eq!(status_url("https://example.com"), None);
        assert_eq!(status_url("wss:///ws"), None);
    }
}
