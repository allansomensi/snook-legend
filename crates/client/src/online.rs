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
use snookered_proto::{
    ClientMsg, FriendInfo, LeaderboardEntry, MAX_CHAT_LEN, PlayerInfo, PlayerStatus, Profile,
    ServerMsg, TableInfo, TournamentInfo, TournamentState, valid_password, valid_username,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Players,
    Tables,
    Tournaments,
    Friends,
}

#[derive(Default)]
struct TournamentForm {
    name: String,
    size: u32,
    rated: bool,
    password: String,
    /// Coins to enter (0: free).
    entry_fee: u32,
    format: snookered_proto::TournamentFormat,
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
}

impl WalletView {
    pub fn is_vip(&self) -> bool {
        self.vip_until > crate::game::unix_now()
    }
}

/// What the server sells for money (empty when it takes no payments).
#[derive(Debug, Clone, Default)]
pub struct ShopOffers {
    pub payments: bool,
    pub packs: Vec<snookered_proto::PackOffer>,
    pub vip_price: String,
    pub vip_coins: u32,
    /// The price of a VIP month given to a friend (empty: not on sale).
    pub vip_gift_price: String,
    /// The price of the welcome pack (empty: not on sale).
    pub welcome_price: String,
}

pub struct Online {
    conn: Option<Connection>,
    pub wallet: Option<WalletView>,
    pub offers: Option<ShopOffers>,
    /// Set when the items in use changed (the table must be redrawn).
    pub looks_changed: bool,
    /// The cloth and ball set of the current online table (shop codes;
    /// empty: this player's own).
    pub table_look: [String; 2],
    /// This account's achievements (for the reactions they unlock).
    achievements: Vec<String>,
    pub stage: Stage,
    pub server: String,
    /// A LAN game (guest name, nothing saved).
    pub lan: bool,
    username: String,
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
    chat_input: String,
    chat_scroll: usize,
    pub leaderboard: Vec<LeaderboardEntry>,
    pub leaderboard_loaded: bool,
    leaderboard_age: f32,
    incoming: Vec<(String, bool)>,
    outgoing: Option<(String, bool)>,
    motd: String,
    token: String,
    auto_resume: bool,
    rated_games: bool,
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
    /// Searching in "Play now": rated, seconds waited and the rating range
    /// when the server last said so, and when that was.
    queue: Option<(bool, u32, u32, std::time::Instant)>,
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
    /// Games that can be watched again, and how many are kept (0: all).
    pub replays: Option<(Vec<snookered_proto::HistoryEntry>, u32)>,
    /// A replay to watch (opened by the replays screen).
    pub replay: Option<Box<snookered_proto::ReplayInfo>>,
    /// The game on the lounge's TV.
    pub featured: Option<snookered_proto::FeaturedMatch>,
    /// A code friends can use to sit at this player's table.
    pub invite_code: Option<String>,
    /// The server restarts at this time (no new games until then).
    pub draining: Option<std::time::Instant>,
    /// Something happened that a player in another window should notice.
    pub attention: Option<String>,
    /// What the right side of the lounge shows.
    side: Side,
    /// Today's and this week's missions.
    missions: Option<MissionsView>,
    /// A mission was finished since the missions were last looked at.
    mission_done: bool,
    /// The notes on the lounge's board, newest first.
    board: Vec<snookered_proto::BoardNote>,
    /// Scheduled tournaments the player was already told are about to start.
    told_starting: Vec<u64>,
    /// The server refused this version of the game: a newer one is out.
    pub outdated: bool,
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
}

/// The update button, for how this copy of the game is updated.
pub fn update_label() -> &'static str {
    match crate::update::how() {
        crate::update::How::Store => t("Update in the Microsoft Store"),
        crate::update::How::Installer => t("Update now"),
        crate::update::How::Page => t("Download the new version"),
    }
}

/// A note's colour: gold notices, green events, blue rules.
pub fn note_color(kind: snookered_proto::NoteKind) -> Color {
    match kind {
        snookered_proto::NoteKind::Notice => GOLD,
        snookered_proto::NoteKind::Event => Color::new(0.38, 0.8, 0.5, 1.0),
        snookered_proto::NoteKind::Rule => Color::new(0.48, 0.72, 1.0, 1.0),
    }
}

fn note_button(a: snookered_proto::NoteAction) -> &'static str {
    use snookered_proto::NoteAction;
    match a {
        NoteAction::Tournaments => t("See the tournaments"),
        NoteAction::Shop => t("Open the shop"),
        NoteAction::Vip => t("See VIP"),
        NoteAction::Missions => t("See the missions"),
    }
}

/// How tall a note is drawn `w` wide.
pub fn note_height(ui: &Ui, n: &snookered_proto::BoardNote, w: f32) -> f32 {
    let lines = ui.wrap(&n.text, w - 44.0, 14.0).len() as f32;
    lines * 19.0 + 38.0 + if n.action.is_some() { 36.0 } else { 0.0 }
}

/// Draws a note in `card`; returns its button's action when clicked.
pub fn draw_note(
    ui: &mut Ui,
    n: &snookered_proto::BoardNote,
    card: Rect,
) -> Option<snookered_proto::NoteAction> {
    let color = note_color(n.kind);
    let tint = Color::new(color.r, color.g, color.b, if n.pinned { 0.13 } else { 0.07 });
    rrect(card, 8.0, tint);
    rrect_lines(card, 8.0, if n.pinned { 1.6 } else { 1.0 }, if n.pinned { color } else { BORDER });
    // A coloured edge on the left: the kind of note.
    rrect(Rect::new(card.x, card.y + 6.0, 4.0, card.h - 12.0), 2.0, color);
    if n.pinned {
        crate::icons::draw(Icon::Star, vec2(card.x + card.w - 16.0, card.y + 14.0), 14.0, color);
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

/// "5 h", "3 days", "12 min": how long until something.
fn time_left(secs: i64) -> String {
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
    daily: Vec<snookered_proto::missions::MissionState>,
    weekly: Vec<snookered_proto::missions::MissionState>,
    day_ends: i64,
    week_ends: i64,
}

/// The account page.
#[derive(Debug, Clone, Default)]
pub struct AccountView {
    pub recovery_email: Option<String>,
    pub payment_email: Option<String>,
}

const MAX_RECONNECTS: u32 = 15;

impl Online {
    /// The global server. Connects (and signs in) at once if a session was saved.
    pub fn new(config: &Config) -> Online {
        let mut o = Online::empty(&config.server, false);
        o.username = config.username.clone();
        o.token = config.token.clone();
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
            stage: Stage::Idle,
            server: server.to_string(),
            lan,
            username: String::new(),
            password: String::new(),
            country: String::new(),
            register: false,
            error: None,
            toasts: VecDeque::new(),
            me: None,
            players: Vec::new(),
            chat: Vec::new(),
            chat_input: String::new(),
            chat_scroll: 0,
            leaderboard: Vec::new(),
            leaderboard_loaded: false,
            leaderboard_age: f32::MAX,
            incoming: Vec::new(),
            outgoing: None,
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
            wallet: None,
            offers: None,
            looks_changed: false,
            table_look: Default::default(),
            achievements: Vec::new(),
            rtt: None,
            ping_sent: None,
            ping_timer: 0.0,
            buying: None,
            bought: None,
            shop_error: None,
            account: None,
            // Development aid (screenshots): SNOOKERED_EMAIL_OFFER=<e-mail> shows the offer.
            email_offer: crate::dev::var("SNOOKERED_EMAIL_OFFER"),
            replays: None,
            replay: None,
            featured: None,
            invite_code: None,
            draining: None,
            attention: None,
            // Development aid: SNOOKERED_LOUNGE_SIDE=missions|board.
            side: match crate::dev::var("SNOOKERED_LOUNGE_SIDE").as_deref() {
                Some("missions") => Side::Missions,
                Some("board") => Side::Board,
                _ => Side::Chat,
            },
            missions: None,
            mission_done: false,
            board: Vec::new(),
            told_starting: Vec::new(),
            outdated: false,
            clear_armed: None,
            quit_for_update: false,
            form_focused: false,
            toast_floor: std::cell::Cell::new(None),
            social: social::Social {
                // Development aid (screenshots): SNOOKERED_RECENT=1 opens
                // the Friends tab on the people played recently.
                recent_view: crate::dev::flag("SNOOKERED_RECENT"),
                ..Default::default()
            },
            gifts: Default::default(),
        }
    }

    pub fn send(&self, msg: ClientMsg) {
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

    /// Opens the connection if it isn't open (for the ranking, a profile or the lounge).
    pub fn connect(&mut self) {
        if self.conn.is_none() {
            self.conn = Some(Connection::open(&self.server));
            if !matches!(self.stage, Stage::Reconnecting { .. }) {
                self.stage = Stage::Connecting;
            }
        }
    }

    /// Number of invitations waiting for an answer (shown during matches).
    pub fn pending_invites(&self) -> usize {
        self.incoming.len()
    }

    pub fn toast(&mut self, text: impl Into<String>, good: bool) {
        self.toasts.push_back((text.into(), 5.0, good));
        while self.toasts.len() > 3 {
            self.toasts.pop_front();
        }
    }

    pub fn request_leaderboard(&mut self) {
        self.connect();
        self.send(ClientMsg::Leaderboard);
        self.leaderboard_age = 0.0;
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
        // Outgoing match messages.
        if let Some(m) = current.as_mut()
            && let Some(o) = m.online.as_mut()
        {
            // Keep messages while reconnecting; they go out once signed in.
            if let Some(c) = &self.conn
                && self.signed_in()
            {
                for msg in o.outbox.drain(..) {
                    c.send(msg);
                }
            }
        }
        if let Stage::Reconnecting { attempt, wait } = &mut self.stage {
            *wait -= dt;
            if *wait <= 0.0 && self.conn.is_none() {
                *attempt += 1;
                self.conn = Some(Connection::open(&self.server));
            }
        }
        self.leaderboard_age += dt;
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
                    if self.lan {
                        self.send(ClientMsg::Guest { name: self.username.clone() });
                        self.stage = Stage::Authenticating;
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
                    self.conn = None;
                    self.queue = None;
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
                        self.stage = if self.lan { Stage::Idle } else { Stage::SignIn };
                        if self.error.is_none() {
                            self.error = Some(crate::i18n::server_text(&reason));
                        }
                    }
                }
                NetEvent::Message(msg) => save |= self.handle(*msg, current, config),
            }
        }
        save
    }

    fn handle(&mut self, msg: ServerMsg, current: &mut Option<Match>, config: &mut Config) -> bool {
        let mut save = false;
        match msg {
            ServerMsg::AuthOk { player, token } => {
                self.outdated = false;
                if let Some(m) = current.as_mut()
                    && matches!(self.stage, Stage::Reconnecting { .. })
                {
                    m.notify(t("Reconnected."), false);
                }
                self.username = player.username.clone();
                self.me = Some(player);
                // A new session: the board comes again; the recent games may have changed.
                self.social.seeks.clear();
                self.social.recent_stale = true;
                self.auto_resume = true;
                self.password.clear();
                self.error = None;
                self.stage = Stage::Lounge;
                if !self.lan {
                    self.token = token.clone();
                    config.username = self.username.clone();
                    config.token = token;
                    config.server = self.server.clone();
                    save = true;
                    self.send(ClientMsg::Friends);
                    self.send(ClientMsg::Leaderboard);
                    // Development aid: SNOOKERED_AUTOINVITE=<name> invites that player.
                    if let Some(to) = crate::dev::var("SNOOKERED_AUTOINVITE") {
                        self.send(ClientMsg::Invite { to, rated: true });
                    }
                    // SNOOKERED_AUTOQUEUE=1 joins "Play now".
                    if crate::dev::flag("SNOOKERED_AUTOQUEUE") {
                        self.send(ClientMsg::QueueJoin { rated: true });
                    }
                    // SNOOKERED_AUTOWATCH=<name> watches that friend's game.
                    if let Some(username) = crate::dev::var("SNOOKERED_AUTOWATCH") {
                        self.send(ClientMsg::Spectate { username });
                    }
                }
            }
            ServerMsg::AuthError { reason } => {
                // This version is too old for the server: the saved session
                // is still good for the new one.
                self.outdated = reason.contains("out of date");
                if self.auto_resume && !self.token.is_empty() && !self.outdated {
                    // The saved session expired (or the account was banned).
                    self.token.clear();
                    config.token.clear();
                    save = true;
                }
                self.auto_resume = false;
                self.me = None;
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
                self.me = None;
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
                        self.players.push(player)
                    }
                }
            }
            ServerMsg::PlayerLeft { username } => {
                self.players.retain(|p| p.username != username);
                self.incoming.retain(|(from, _)| *from != username);
                for f in &mut self.friends.friends {
                    if f.player.username == username {
                        f.online = false;
                    }
                }
                if self.outgoing.as_ref().is_some_and(|(to, _)| *to == username) {
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
            ServerMsg::Invited { from, rated } => {
                if crate::dev::flag("SNOOKERED_AUTOPLAY") && current.is_none() {
                    // Development aid: accept at once.
                    self.send(ClientMsg::AnswerInvite { from, accept: true });
                    return save;
                }
                self.attention = Some(tw("{name} invites you to play.", &[("name", &from)]));
                self.incoming.retain(|(f, _)| *f != from);
                self.incoming.push((from, rated));
            }
            ServerMsg::InviteSent { to, rated } => self.outgoing = Some((to, rated)),
            ServerMsg::InviteCancelled { from } => self.incoming.retain(|(f, _)| *f != from),
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
                    self.attention = Some(tw("{name} has started!", &[("name", &tournament.name)]));
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
            ServerMsg::Leaderboard { entries } => {
                self.leaderboard = entries;
                self.leaderboard_loaded = true;
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
                let title = snookered_proto::ACHIEVEMENTS
                    .iter()
                    .find(|a| a.code == code)
                    .map(|a| t(a.title).to_string())
                    .unwrap_or(code);
                self.toast(tw("Achievement unlocked: {title}", &[("title", &title)]), true);
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
                self.outgoing = None;
                self.queue = None;
                if !spectating {
                    self.incoming.clear();
                }
                // Development aid: SNOOKERED_AUTOCHAT="text" says it at the start of a match.
                if let Some(text) = crate::dev::var("SNOOKERED_AUTOCHAT").filter(|_| !spectating) {
                    self.send(ClientMsg::MatchChat { text });
                }
                // SNOOKERED_AUTOREACT="code" sends that reaction.
                if let Some(code) = crate::dev::var("SNOOKERED_AUTOREACT") {
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
                    Some(m) if m.is_online() && same_players(m, &next) => {
                        let same_game = m.match_id() == match_id;
                        m.continue_online(next, same_game)
                    }
                    _ => *current = Some(next),
                }
                if let Some(o) = current.as_mut().and_then(|m| m.online.as_mut()) {
                    o.unlocked = self.achievements.clone();
                }
                // A message sent just before the match (e.g. "this game is casual").
                if let (Some((text, ..)), Some(m)) = (self.toasts.pop_back(), current.as_mut()) {
                    m.notify(text, true);
                }
            }
            ServerMsg::Error { message } if self.buying.is_some() => {
                self.buying = None;
                self.shop_error = Some(crate::i18n::server_text(&message));
            }
            ServerMsg::Error { message } => match current.as_mut() {
                Some(m) if m.is_online() => m.handle_server(ServerMsg::Error { message }),
                _ => self.toast(crate::i18n::server_text(&message), false),
            },
            ServerMsg::ItemBought { code, gift_to } => {
                self.buying = None;
                self.bought = Some((code, gift_to));
            }
            ServerMsg::GiftReceived { from, code } => {
                let name = snookered_proto::shop::item(&code).map(|i| t(i.name)).unwrap_or(&code);
                self.toast(
                    tw("{from} gave you {item}!", &[("from", &from), ("item", &name)]),
                    true,
                );
                self.attention =
                    Some(tw("{from} gave you {item}!", &[("from", &from), ("item", &name)]));
            }
            ServerMsg::CoinsEarned { amount, reason } => {
                let why = match reason.as_str() {
                    "daily" => t("daily bonus"),
                    "win" => t("for the win"),
                    "game" => t("for the game"),
                    "prize" => t("tournament prize"),
                    "refund" => t("entry fee back"),
                    "mission" => {
                        if self.side != Side::Missions {
                            self.mission_done = true;
                        }
                        t("mission complete")
                    }
                    _ => "",
                };
                self.toast(tw("+{n} coins · {why}", &[("n", &amount), ("why", &why)]), true);
            }
            ServerMsg::Account { recovery_email, payment_email } => {
                // After a payment, without a recovery e-mail: offer the one paid with.
                if recovery_email.is_none() && !config.email_offer_declined {
                    self.email_offer = payment_email.clone();
                }
                self.account = Some(AccountView { recovery_email, payment_email });
            }
            ServerMsg::AccountDeleted => {
                self.auto_resume = false;
                self.me = None;
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
            ServerMsg::Wallet { coins, vip_until, owned, equipped } => {
                let wallet = WalletView { coins, vip_until, owned, equipped };
                // The items in use, as the server allows them now.
                for c in snookered_proto::shop::Category::ALL {
                    let code = self.effective(&wallet, c).to_string();
                    let slot = match c {
                        snookered_proto::shop::Category::Cloth => &mut config.cloth,
                        snookered_proto::shop::Category::Cue => &mut config.cue,
                        snookered_proto::shop::Category::Chalk => &mut config.chalk,
                        snookered_proto::shop::Category::Balls => &mut config.balls,
                        snookered_proto::shop::Category::Effect => &mut config.effect,
                        // Only seen online.
                        snookered_proto::shop::Category::Frame
                        | snookered_proto::shop::Category::Name => {
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
            } => {
                self.offers = Some(ShopOffers {
                    payments,
                    packs,
                    vip_price,
                    vip_coins,
                    vip_gift_price,
                    welcome_price,
                });
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
                    self.toast(t("The payment page opened in your browser."), false);
                } else {
                    self.toast(t("Could not open the browser."), true);
                }
            }
            ServerMsg::Queue { searching, rated, waited_secs, range } => {
                self.queue =
                    searching.then(|| (rated, waited_secs, range, std::time::Instant::now()));
            }
            ServerMsg::SpectateEnded { reason } => {
                if current.as_ref().is_some_and(|m| m.is_spectating()) {
                    *current = None;
                }
                self.toast(crate::i18n::server_text(&reason), false);
            }
            ServerMsg::MyAchievements { codes } => {
                if let Some(o) = current.as_mut().and_then(|m| m.online.as_mut()) {
                    o.unlocked = codes.clone();
                }
                self.achievements = codes;
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
        category: snookered_proto::shop::Category,
    ) -> &'static str {
        use snookered_proto::shop;
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
    pub fn can_use(&self, item: &snookered_proto::shop::Item) -> bool {
        let w = self.wallet.clone().unwrap_or_default();
        snookered_proto::shop::can_use(item, &w.owned, &self.achievements, w.is_vip())
    }

    /// Starts watching a friend's game.
    pub fn spectate(&mut self, name: &str) {
        self.send(ClientMsg::Spectate { username: name.to_string() });
    }

    pub fn leave_match(&mut self, m: &Match) {
        // A game was played: who was played recently has changed.
        self.social.recent_stale = true;
        if m.is_spectating() {
            self.send(ClientMsg::StopSpectating);
            return;
        }
        if m.game.game_over().is_none() {
            self.send(ClientMsg::Resign);
        }
        self.send(ClientMsg::LeaveMatch);
    }

    pub fn logout(&mut self, config: &mut Config) {
        self.send(ClientMsg::Logout);
        self.conn = None;
        self.queue = None;
        self.me = None;
        self.token.clear();
        self.auto_resume = false;
        self.friends = FriendsData::default();
        self.social = Default::default();
        self.gifts = Default::default();
        config.token.clear();
        config.save();
        self.stage = Stage::Idle;
        self.connect();
    }

    /// After a payment: "use this e-mail to recover your account?". Returns
    /// true if the settings changed.
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
        let note = ui.wrap(t("You can change it any time in your profile (Account)."), inner, 13.0);
        // Sized to its text: title, question, note and the buttons.
        let h = pad
            + 30.0
            + 16.0
            + lines.len() as f32 * 20.0
            + 8.0
            + note.len() as f32 * 17.0
            + 24.0
            + 40.0
            + pad;
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
        y += 8.0;
        for line in &note {
            ui.text(line, r.x + pad, y, 13.0, MUTED, false);
            y += 17.0;
        }
        // "Use it" bottom right; "No, thanks" (never ask again) bottom left.
        let by = r.y + r.h - pad - 40.0;
        let no = t("No, thanks");
        let yes = t("Use this e-mail");
        let half = (inner - 12.0) / 2.0;
        let no_w = button_w(ui, no, 40.0).clamp(130.0, half);
        let yes_w = button_w(ui, yes, 40.0).clamp(170.0, half);
        let mut changed = false;
        if ui.button(Rect::new(r.x + pad, by, no_w, 40.0), no, true) {
            self.email_offer = None;
            config.email_offer_declined = true;
            changed = true;
        }
        if ui.primary_button(Rect::new(r.x + r.w - pad - yes_w, by, yes_w, 40.0), yes) {
            self.send(ClientMsg::SetRecoveryEmail { email });
            self.email_offer = None;
        } else if is_key_pressed(KeyCode::Escape) {
            // Not now: asked again after the next payment.
            self.email_offer = None;
        }
        changed
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

    /// Invitations received at the table or in the lounge: answered from a banner.
    pub fn draw_invites(&mut self, ui: &mut Ui, in_match: bool) {
        let sw = crate::ui::vw();
        let w = 520.0f32.min(sw - 32.0);
        let y = if in_match { 64.0 } else { 74.0 };
        self.draw_invites_at(ui, in_match, Rect::new((sw - w) / 2.0, y, w, 0.0));
    }

    /// The invitations stacked down from the top of `at` (its width).
    fn draw_invites_at(&mut self, ui: &mut Ui, in_match: bool, at: Rect) {
        let w = at.w;
        let mut y = at.y;
        let mut answer = None;
        for (from, rated) in self.incoming.iter().take(3) {
            let r = Rect::new(at.x, y, w, 64.0);
            ui.panel(r);
            rrect_lines(r, 8.0, 1.6, GOLD);
            let kind = if *rated { t("a rated game") } else { t("a casual game") };
            let player = self.players.iter().find(|p| &p.username == from);
            let color = player.map(name_color).unwrap_or(CREAM);
            let country = player.map(|p| p.country.clone()).unwrap_or_default();
            // The answers on the right, centred; who and what on the left.
            let accept = t("Accept");
            let decline = t("Decline");
            let bw = button_w(ui, accept, 32.0).max(button_w(ui, decline, 32.0)).max(96.0);
            let buttons = if in_match { bw } else { bw * 2.0 + 8.0 };
            let text_w = w - 32.0 - buttons - 16.0;
            ui.name_with_flag(from, &country, color, r.x + 16.0, r.y + 12.0, 16.0, text_w);
            let line2 = if in_match {
                format!(
                    "{} · {}",
                    tw("invites you to {kind}", &[("kind", &kind)]),
                    t("Finish this match first.")
                )
            } else {
                tw("invites you to {kind}", &[("kind", &kind)])
            };
            let (line2, size) = fit_text(ui, &line2, 13.0, text_w, false);
            ui.text(&line2, r.x + 16.0, r.y + 36.0, size, CREAM, false);
            let by = r.y + (r.h - 32.0) / 2.0;
            let decline_r = Rect::new(r.x + w - 16.0 - bw, by, bw, 32.0);
            if !in_match
                && ui.primary_button(Rect::new(decline_r.x - 8.0 - bw, by, bw, 32.0), accept)
            {
                answer = Some((from.clone(), true));
            }
            if ui.button(decline_r, decline, true) {
                answer = Some((from.clone(), false));
            }
            y += r.h + 8.0;
        }
        if let Some((from, accept)) = answer {
            self.incoming.retain(|(f, _)| *f != from);
            self.send(ClientMsg::AnswerInvite { from, accept });
        }
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
        if ui.button(back, t("Back"), true) || is_key_pressed(KeyCode::Escape) {
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
        if back || (is_key_pressed(KeyCode::Escape) && !picker_open) {
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
        let label = if outdated {
            update_label()
        } else if !connected {
            t("Retry")
        } else if self.register {
            t("Create account")
        } else {
            t("Sign in")
        };
        let clicked = ui.primary_button(Rect::new(x, y, inner, 44.0), label);
        y += 44.0 + 14.0;
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
            let pass_ok = valid_password(&self.password);
            if !connected {
                self.server = config.server.clone();
                self.connect();
                self.error = None;
            } else if !name_ok {
                self.error = Some(t("Usernames have 3–16 letters, digits or underscores.").into());
            } else if !pass_ok {
                self.error =
                    Some(t("Passwords need at least 8 characters (not only digits).").into());
            } else {
                let username = self.username.trim().to_string();
                let password = self.password.clone();
                self.send(if self.register {
                    ClientMsg::Register { username, password, country: self.country.clone() }
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
        let esc = is_key_pressed(KeyCode::Escape)
            && ui.focus.is_none()
            && self.email_offer.is_none()
            && !crate::countries::is_open();
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
        if ui.icon_button(
            Rect::new(header.x + 10.0, by, 36.0, 36.0),
            Icon::Back,
            t("Main menu"),
            true,
        ) {
            action = OnlineAction::Leave;
        }
        crate::icon::draw(header.x + 10.0 + 36.0 + 10.0, by, 36.0);
        let name = snookered_proto::GAME_NAME.to_uppercase();
        let title_x = header.x + 10.0 + 36.0 + 10.0 + 36.0 + 10.0;
        ui.text(&name, title_x, header.y + 15.0, 24.0, GOLD, true);
        let title_w = ui.text_size(&name, 24.0, true).x;
        let subtitle = if self.lan { t("LAN game") } else { t("lounge") };
        // On the title's baseline.
        ui.text(subtitle, title_x + title_w + 10.0, header.y + 22.0, 15.0, MUTED, false);
        let title_end = title_x + title_w + 10.0 + ui.text_size(subtitle, 15.0, false).x;
        let online_text = match self.rtt.filter(|_| config.show_ping) {
            Some(rtt) => tw(
                "{n} online · ping {ms} ms",
                &[("n", &self.players.len()), ("ms", &rtt.as_millis())],
            ),
            None => tw("{n} online", &[("n", &self.players.len())]),
        };
        let online_w = ui.text_size(&online_text, 13.0, false).x;
        let mut right = header.x + header.w - 10.0;
        if !self.lan {
            right -= 36.0;
            if ui.icon_button(Rect::new(right, by, 36.0, 36.0), Icon::Power, t("Sign out"), true) {
                self.logout(config);
                return OnlineAction::Leave;
            }
            right -= gap + 36.0;
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
                format!("{} · {}", me.rating, t(me.tier().name()))
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
        // How many are here, with a green "live" dot.
        let ox = right - 16.0 - online_w;
        draw_circle(ox - 10.0, header.y + 28.0, 4.0, Color::new(0.35, 0.85, 0.45, 1.0));
        ui.text(&online_text, ox, header.y + 21.5, 13.0, MUTED, false);

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
        }
        let n = tabs.len() as f32;
        let tab_w = (left.w - pad * 2.0 - (n - 1.0) * 6.0) / n;
        for (k, (tab, label)) in tabs.into_iter().enumerate() {
            let r = Rect::new(left.x + pad + k as f32 * (tab_w + 6.0), left.y + pad, tab_w, 34.0);
            let count = match tab {
                Tab::Tables if !self.tables.is_empty() => self.tables.len().to_string(),
                _ => String::new(),
            };
            if ui.tab_with_count(r, label, &count, self.tab == tab) {
                self.tab = tab;
                self.list_first = 0;
                if tab == Tab::Friends {
                    self.send(ClientMsg::Friends);
                    self.unread_friend_requests = false;
                    if self.social.recent_view {
                        self.request_recent();
                    }
                }
            }
            let badge = match tab {
                Tab::Friends => self.unread_friend_requests || !self.friends.incoming.is_empty(),
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
        }

        // Outgoing invitation.
        if let Some((to, rated)) = self.outgoing.clone() {
            let r = Rect::new(left.x + pad, left.y + left.h - pad - 48.0, left.w - pad * 2.0, 48.0);
            rrect(r, 8.0, Color::new(0.18, 0.12, 0.06, 0.95));
            rrect_lines(r, 8.0, 1.5, GOLD);
            let cancel = t("Cancel");
            let bw = button_w(ui, cancel, 32.0).max(90.0);
            crate::ui::spinner(vec2(r.x + 22.0, r.y + 24.0), 7.0, GOLD);
            let kind = if rated { t("rated") } else { t("casual") };
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
        // Invitations float over the top of this side, under its tabs: the
        // tabs and the lists on the left stay in reach.
        let invites_at = Rect::new(inner_x, log_top, inner_w, 0.0);
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
        // The server is about to restart.
        if let Some(until) = self.draining {
            let left = until.saturating_duration_since(std::time::Instant::now()).as_secs();
            let text = tw(
                "The server restarts in {m}:{s} for an update. Games in progress go on.",
                &[("m", &(left / 60)), ("s", &format!("{:02}", left % 60))],
            );
            let lines = ui.wrap(&text, inner_w - 20.0, 13.0);
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
            self.draw_invites_at(ui, false, invites_at);
            return action;
        }
        // The featured game: anyone can watch it.
        if let Some(f) = self.featured.clone()
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
            let avg = (f.players[0].rating + f.players[1].rating) / 2;
            let sub = tw("{n} watching · avg {r}", &[("n", &f.spectators), ("r", &avg)]);
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
        let input_h = 36.0;
        let input = Rect::new(inner_x, bottom - input_h, inner_w - input_h - 8.0, input_h);
        let log = Rect::new(inner_x, log_top, inner_w, input.y - 10.0 - log_top);
        // Toasts float above the chat field, not over it.
        self.toast_floor.set(Some(input.y - 10.0));
        rrect(log, 6.0, Color::new(0.0, 0.0, 0.0, 0.25));
        let me = self.me.as_ref().map(|m| m.username.clone());
        let admin = self.me.as_ref().is_some_and(|m| m.role == snookered_proto::Role::Admin);
        if self.chat.is_empty() {
            ui.block(log);
            empty_state(ui, log, Glyph::Chat, t("No messages yet."), None);
        } else {
            let (scroll, act) =
                ui.chat_log(log, &self.chat, me.as_deref(), admin, self.chat_scroll);
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
        let sent = ui.text_input(30, input, &mut self.chat_input, MAX_CHAT_LEN);
        placeholder(ui, 30, input, &self.chat_input, t("Say hello… (Enter to send)"));
        let send_r = Rect::new(input.x + input.w + 8.0, input.y, input_h, input_h);
        let can_send = !self.chat_input.trim().is_empty();
        if (ui.icon_button(send_r, Icon::Play, t("Send"), can_send) || sent) && can_send {
            let text = snookered_proto::clean_chat(&self.chat_input);
            self.send(ClientMsg::Chat { text });
            self.chat_input.clear();
            self.chat_scroll = 0;
            ui.focus = Some(30);
        }

        self.draw_invites_at(ui, false, invites_at);
        action
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
            .map(|t| (t.id, t.name.clone(), t.players.iter().any(|p| p.username == me)))
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
                let Some(m) = snookered_proto::missions::mission(&state.code) else { continue };
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
    pub fn note_action(&mut self, a: snookered_proto::NoteAction) -> Option<OnlineAction> {
        use snookered_proto::NoteAction;
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
    pub fn board_now(&self) -> Vec<snookered_proto::BoardNote> {
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
            let (name, size) = fit_text(ui, &tour.name, 14.0, card.w - 40.0 - bw - 24.0, true);
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

    fn rated_toggle(&mut self, ui: &mut Ui, r: Rect) {
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
            PlayerStatus::Available => {
                if p.guest {
                    t("guest").to_string()
                } else {
                    format!("{} · {}", p.rating, t(p.tier().name()))
                }
            }
        }
    }

    fn status_dot(p: &PlayerInfo, x: f32, y: f32) {
        let color = match p.status {
            PlayerStatus::Playing => Color::new(0.9, 0.6, 0.2, 1.0),
            PlayerStatus::Waiting => Color::new(0.45, 0.7, 1.0, 1.0),
            PlayerStatus::Available => Color::new(0.35, 0.85, 0.45, 1.0),
        };
        draw_circle(x, y, 4.5, color);
    }

    /// "Play now": one click to be paired with someone of a similar rating.
    /// Seconds "Play now" has been looking for an opponent.
    pub fn queue_secs(&self) -> Option<u32> {
        self.queue.map(|(_, waited, _, at)| waited + at.elapsed().as_secs() as u32)
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
                    self.send(ClientMsg::QueueJoin { rated: self.rated_games && !self.lan });
                }
                if ui.hovering(r) {
                    ui.tooltip(t("Find an opponent of your level automatically"), r);
                }
            }
            Some((rated, waited, range, at)) => {
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
                let sub = if range == u32::MAX {
                    t("any rating").to_string()
                } else {
                    tw(
                        "rating ±{n} · {mode}",
                        &[("n", &range), ("mode", &if rated { t("rated") } else { t("casual") })],
                    )
                };
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
        if self.queue_secs().is_some_and(|s| s >= PRACTICE_AFTER_SECS) && !self.lan {
            let b = Rect::new(area.x, y, area.w, 36.0);
            if ui.button(b, t("Play the computer while you wait"), true) {
                action = Some(OnlineAction::PracticeWhileWaiting);
            }
            y += 36.0 + 12.0;
        }
        self.rated_toggle(ui, Rect::new(area.x, y, area.w.min(280.0), 32.0));
        y += 32.0 + 12.0;
        y += self.draw_seek_bar(ui, Rect::new(area.x, y, area.w, 40.0));
        let me = self.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
        let mut players: Vec<PlayerInfo> =
            self.players.iter().filter(|p| p.username != me).cloned().collect();
        // Those looking for a game first (newest note first), then who is free.
        let now = crate::game::unix_now();
        let note_at = |p: &PlayerInfo| self.social.note_of(&p.username, now).map(|n| n.at);
        players.sort_by(|a, b| {
            note_at(b)
                .cmp(&note_at(a))
                .then(
                    (a.status != PlayerStatus::Available)
                        .cmp(&(b.status != PlayerStatus::Available)),
                )
                .then(b.rating.cmp(&a.rating))
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
        let mut take_down = None;
        for (k, p) in players.iter().skip(self.list_first).take(visible).enumerate() {
            let r = Rect::new(list.x, list.y + k as f32 * row_h, row_w, row_h - 6.0);
            let note = self.social.note_of(&p.username, now).cloned();
            let button = Rect::new(r.x + r.w - bw - 6.0, r.y + 6.0, bw, r.h - 12.0);
            // The staff can take a note down (×, before the button).
            let x_r = Rect::new(button.x - 8.0 - 28.0, r.y + 7.0, 28.0, r.h - 14.0);
            let staff_x = staff && note.is_some();
            // The row (but its buttons) opens the player's profile.
            let hover = row_card(ui, r, !p.guest);
            if note.is_some() {
                // Looking for a game: a gold edge.
                rrect(Rect::new(r.x + 1.0, r.y + 6.0, 3.0, r.h - 12.0), 1.5, GOLD);
            }
            Self::status_dot(p, r.x + 14.0, r.y + r.h / 2.0);
            let right = if staff_x { x_r.x } else { button.x };
            let name_w = right - 12.0 - (r.x + 28.0);
            ui.player_name(p, r.x + 28.0, r.y + 5.0, 15.0, name_w);
            match &note {
                Some(n) => {
                    Self::rating_after_name(ui, p, r.x + 28.0, r.y + 5.0, name_w);
                    Self::note_line(ui, n, r.x + 28.0, r.y + 23.0, name_w);
                }
                None => sub_line(
                    ui,
                    &Self::status_text(p),
                    &p.status_message,
                    r.x + 28.0,
                    r.y + 23.0,
                    name_w,
                ),
            }
            if hover
                && !p.guest
                && ui.clicked(r)
                && !button.contains(ui.mouse())
                && !(staff_x && x_r.contains(ui.mouse()))
            {
                open = Some(p.username.clone());
            }
            if staff_x && ui.icon_button_ex(x_r, Icon::Close, t("Take the note down"), true, true) {
                take_down = Some(p.username.clone());
            }
            // Friends' games (anyone's on a local network) can be watched.
            let friend =
                self.lan || self.friends.friends.iter().any(|f| f.player.username == p.username);
            if let Some(n) = note.as_ref().filter(|_| p.status != PlayerStatus::Playing) {
                // A challenge is an invitation, rated as the note says.
                let can = p.status == PlayerStatus::Available && self.outgoing.is_none();
                let hit = if can {
                    ui.primary_button(button, challenge_label)
                } else {
                    ui.button(button, challenge_label, false)
                };
                if hit {
                    self.send(ClientMsg::Invite { to: p.username.clone(), rated: n.rated });
                }
            } else if p.status == PlayerStatus::Playing && friend {
                if ui.button(button, watch_label, true) {
                    watch = Some(p.username.clone());
                }
            } else {
                let can = p.status == PlayerStatus::Available && self.outgoing.is_none();
                if ui.button(button, invite_label, can) {
                    invite = Some(p.username.clone());
                }
            }
        }
        if let Some(to) = invite {
            self.send(ClientMsg::Invite { to, rated: self.rated_games && !self.lan });
        }
        if let Some(name) = watch {
            self.spectate(&name);
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
                let sub = format!("{kind} — {}", t("waiting for an opponent…"));
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
                    let password = self.table_private.then(|| self.table_password.clone());
                    self.send(ClientMsg::OpenTable {
                        rated: self.rated_games && !self.lan,
                        password,
                    });
                }
                // A private table and a short code for someone who isn't in
                // the lounge yet.
                if let Some(code_r) = code_r
                    && ui.button(code_r, code_label, true)
                {
                    self.send(ClientMsg::CreateInviteCode { rated: self.rated_games });
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
            let link = format!("snookered://join/{code}");
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
                    t("Join my table in Snookered:")
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
            ui.text(&table.host.rating.to_string(), px, r.y + 26.0, 12.0, MUTED, false);
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
        let labels = [t("Name"), t("Format"), t("Players"), t("Entry"), t("Games"), t("Password")];
        let lw = labels.iter().map(|l| ui.text_size(l, 13.0, true).x).fold(70.0, f32::max) + 16.0;
        let cx = area.x + lw;
        let cw = area.w - lw;
        let seg_w = cw.min(280.0);
        let arena = form.format == snookered_proto::TournamentFormat::Arena;
        let note = t("The tournament starts when it is full (the host can start it earlier).");
        let note = ui.wrap(note, area.w - 4.0, 12.0);
        // When it would not fit (a large interface size), the rows close up
        // and the footnote goes.
        let hint_h = |text: &str| ui.wrap(text, cw, 12.0).len() as f32 * 16.0 + 2.0;
        // Name, format, players (not in an arena), entry and games (online), password.
        let rows = 4.0 - arena as u8 as f32 + if lan { 0.0 } else { 2.0 };
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
        let mut enter = ui.text_input(33, name_r, &mut form.name, snookered_proto::MAX_NAME_LEN);
        y += 32.0 + gap;
        label(ui, t("Format"), y);
        let formats = [t("Knockout"), t("Arena")];
        if let Some(i) = ui.segmented(Rect::new(cx, y, seg_w, 32.0), &formats, arena as usize) {
            form.format = if i == 1 {
                snookered_proto::TournamentFormat::Arena
            } else {
                snookered_proto::TournamentFormat::Knockout
            };
        }
        y += 32.0 + gap;
        if arena {
            y -= 4.0;
            hint(ui, t(ARENA_HINT), &mut y);
        } else {
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
        if self.selected_tournament.is_none() && crate::dev::flag("SNOOKERED_OPEN_TOURNAMENT") {
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
            self.tournament_form =
                Some(TournamentForm { size: 8, rated: !self.lan, ..Default::default() });
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
            let text_w = ix - 12.0 - (r.x + 44.0);
            let (name, size) = fit_text(ui, &tour.name, 15.0, text_w, true);
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
        let (name, size) = fit_text(ui, &tour.name, 20.0, area.w - 46.0 - pw - 12.0, true);
        ui.text(&name, area.x + 46.0, area.y + (34.0 - size) / 2.0, size, GOLD, true);
        let mut y = area.y + 34.0 + 12.0;
        let kind = if tour.rated { t("rated games") } else { t("casual games") };
        let arena = tour.format == snookered_proto::TournamentFormat::Arena;
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
                if ui.primary_button(Rect::new(area.x, y, area.w, 36.0), &label) {
                    self.send(ClientMsg::JoinTournament { id: tour.id, password: None });
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
                let rating = p.rating.to_string();
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
        let friends = self.friends.friends.clone();
        let outgoing = self.friends.outgoing.clone();
        let rows = friends.iter().map(Some).chain(outgoing.iter().map(|_| None));
        for (k, (friend, i)) in rows.zip(0..).skip(self.list_first).take(visible).enumerate() {
            let r = Rect::new(list.x, list.y + k as f32 * row_h, row_w, row_h - 6.0);
            // Remove (×) at the far right; the main button before it.
            let x_r = Rect::new(r.x + r.w - 6.0 - 30.0, r.y + 6.0, 30.0, r.h - 12.0);
            let button = Rect::new(x_r.x - 8.0 - bw, r.y + 6.0, bw, r.h - 12.0);
            match friend {
                Some(f) => {
                    let hover = row_card(ui, r, true);
                    if f.online {
                        Self::status_dot(&f.player, r.x + 14.0, r.y + r.h / 2.0);
                    } else {
                        let grey = Color::new(0.4, 0.4, 0.4, 1.0);
                        draw_circle(r.x + 14.0, r.y + r.h / 2.0, 4.5, grey);
                    }
                    let name_w = button.x - 12.0 - (r.x + 28.0);
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
                    {
                        action = Some(OnlineAction::Profile(f.player.username.clone()));
                    }
                    if f.online && f.player.status == PlayerStatus::Playing {
                        // Watch their game.
                        if ui.button(button, watch_label, true) {
                            watch = Some(f.player.username.clone());
                        }
                    } else {
                        let can = f.online
                            && f.player.status == PlayerStatus::Available
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
            self.send(ClientMsg::Invite { to, rated: self.rated_games });
        }
        if let Some(username) = remove {
            self.send(ClientMsg::FriendRemove { username });
        }
        if let Some(name) = watch {
            self.spectate(&name);
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

/// How wide a button `h` high must be for `label` (the toolkit's label size,
/// with room on both sides).
fn button_w(ui: &Ui, label: &str, h: f32) -> f32 {
    ui.text_size(label, (h * 0.45).clamp(13.0, 22.0), true).x + 32.0
}

/// `text` at `size` if it fits `width`; else smaller, down to 11 px; cut
/// with "…" only if even that is too long.
fn fit_text(ui: &Ui, text: &str, size: f32, width: f32, bold: bool) -> (String, f32) {
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
fn placeholder(ui: &Ui, id: u64, field: Rect, value: &str, text: &str) {
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
fn row_card(ui: &Ui, r: Rect, clickable: bool) -> bool {
    let hover = ui.hovering(r);
    let lit = hover && clickable;
    rrect(r, 7.0, Color::new(1.0, 1.0, 1.0, if lit { 0.075 } else { 0.035 }));
    if lit {
        rrect_lines(r, 7.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.35));
    }
    hover
}

/// Waiting for the server: a spinner in the middle of `area`.
fn loading(ui: &Ui, area: Rect) {
    let c = vec2(area.x + area.w / 2.0, area.y + (area.h * 0.4).min(160.0));
    crate::ui::spinner(c, 14.0, GOLD);
    ui.text_centered(t("Loading…"), c.x, c.y + 28.0, 13.0, MUTED, false);
}

/// What an empty state shows over its line of text.
#[derive(Clone, Copy)]
enum Glyph {
    Icon(Icon),
    /// A speech bubble (the chat).
    Chat,
}

/// A speech bubble `s` wide centred on `c`, drawn in outline.
fn chat_glyph(c: Vec2, s: f32, color: Color) {
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
fn empty_state(ui: &mut Ui, area: Rect, glyph: Glyph, text: &str, action: Option<&str>) -> bool {
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
