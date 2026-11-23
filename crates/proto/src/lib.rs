//! # snook_legend_proto
//!
//! Messages exchanged between the Snook Legend client and server over a
//! WebSocket, encoded as JSON (one message per text frame), plus the data
//! both sides must agree on: name rules, countries, rating tiers and the
//! achievement catalogue.
//!
//! Floats are serialized with exact round-tripping so game snapshots arrive
//! bit-identical on every machine.

mod achievements;
pub mod analysis;
pub mod correspondence;
mod countries;
pub mod cup;
pub mod daily;
pub mod drills;
pub mod events;
pub mod ghost;
pub mod ladder;
pub mod missions;
pub mod reactions;
pub mod replay;
pub mod shop;
pub mod xp;

pub use achievements::{ACHIEVEMENTS, Achievement, GOLDEN_BREAK, RUBY, Stat, Stats, month_day};
pub use countries::{COUNTRIES, FLAG_COLUMNS, FLAG_H, FLAG_W};
pub use reactions::{REACTIONS, Reaction, reaction};

use glam::DVec2;
use serde::{Deserialize, Serialize};
use snook_legend_core::{BreakOption, EightBallGame, GameOverReason, PocketId, Shot, ShotReport};
pub use snook_legend_core::{PerRules, Rules};

/// A coin pack on sale, with its price as shown to players.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackOffer {
    pub code: String,
    pub coins: u32,
    pub bonus: u32,
    pub price: String,
}

/// A coin pack sold in the Microsoft Store (the Store build of the game).
/// The price comes from the Store itself, in the player's currency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorePack {
    pub code: String,
    pub coins: u32,
    pub bonus: u32,
    /// The add-on's Store ID (12 characters, `9…`).
    pub store_id: String,
}

/// A note the staff pinned on the lounge's board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardNote {
    pub id: i64,
    pub text: String,
    pub author: String,
    /// Unix seconds.
    pub at: i64,
    /// Taken down at this time (Unix seconds), if set.
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub kind: NoteKind,
    /// Shown above the other notes.
    #[serde(default)]
    pub pinned: bool,
    /// A button under the note.
    #[serde(default)]
    pub action: Option<NoteAction>,
}

/// What a note on the board is about (its colour).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    #[default]
    Notice,
    Event,
    Rule,
}

impl NoteKind {
    pub fn code(self) -> &'static str {
        match self {
            NoteKind::Notice => "notice",
            NoteKind::Event => "event",
            NoteKind::Rule => "rule",
        }
    }

    pub fn from_code(code: &str) -> NoteKind {
        match code {
            "event" => NoteKind::Event,
            "rule" => NoteKind::Rule,
            _ => NoteKind::Notice,
        }
    }
}

/// Where a note's button takes the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteAction {
    /// The lounge's tournaments (e.g. to sign up for the cup).
    Tournaments,
    Shop,
    Vip,
    Missions,
}

impl NoteAction {
    pub const ALL: [NoteAction; 4] =
        [NoteAction::Tournaments, NoteAction::Shop, NoteAction::Vip, NoteAction::Missions];

    pub fn code(self) -> &'static str {
        match self {
            NoteAction::Tournaments => "tournaments",
            NoteAction::Shop => "shop",
            NoteAction::Vip => "vip",
            NoteAction::Missions => "missions",
        }
    }

    pub fn from_code(code: &str) -> Option<NoteAction> {
        NoteAction::ALL.into_iter().find(|a| a.code() == code)
    }
}

/// The `seat` of a reaction sent by a spectator.
pub const SPECTATOR_SEAT: usize = 2;

/// The game's name, as players see it (window, menus, notifications, the
/// server's messages and web pages).
pub const GAME_NAME: &str = "Snook Legend";

/// Bumped whenever messages change incompatibly.
pub const PROTOCOL_VERSION: u32 = 24;

/// The oldest version the server still lets in: version 24 changed the
/// rules a replay is played back with (on a Brazilian break handed over
/// with the balls scattered, the 8 hit first is a foul and the 8 made
/// loses; it used to win): an older game would show other tables than the
/// server's, so it is asked to update.
pub const OLDEST_PROTOCOL_VERSION: u32 = 24;

/// Username rules: 3–16 characters, letters, digits and underscores.
pub fn valid_username(name: &str) -> bool {
    (3..=16).contains(&name.chars().count())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// How many times a player may change their username, in all.
pub const MAX_NAME_CHANGES: u32 = 3;

/// The longest password, in bytes (the hashing limit).
pub const MAX_PASSWORD_BYTES: usize = 72;

/// Password rules: at least 8 characters (counted as characters, not
/// bytes: "çãõé" is four) and at most 72 bytes (the hashing limit), any
/// characters, not made only of digits (of any script) or of one repeated
/// character.
pub fn valid_password(password: &str) -> bool {
    let first = password.chars().next();
    password.chars().count() >= 8
        && password.len() <= MAX_PASSWORD_BYTES
        && !password.chars().all(|c| c.is_numeric())
        && !password.chars().all(|c| Some(c) == first)
}

/// A country code from [`COUNTRIES`] (or the empty string: not set).
pub fn valid_country(code: &str) -> bool {
    code.is_empty() || country_index(code).is_some()
}

/// The profile pictures a player can choose (the client draws each one;
/// the empty code is no picture: a plain silhouette).
pub const AVATARS: [&str; 64] = [
    // Things of the game (once there were faces too: see RETIRED_AVATARS).
    "pic_eight",
    "pic_cue_ball",
    "pic_stripe",
    "pic_rack",
    "pic_cues",
    "pic_chalk",
    "pic_trophy",
    "pic_star",
    "pic_crown",
    "pic_table",
    "pic_flame",
    "pic_bolt",
    "pic_medal",
    "pic_laurel",
    "pic_one",
    "pic_fifteen",
    "pic_fire_eight",
    "pic_spin",
    "pic_pocket",
    "pic_lamp",
    "pic_tip",
    "pic_diamond",
    "pic_shield",
    "pic_target",
    "pic_rocket",
    "pic_rosette",
    "pic_king_eight",
    "pic_cues_star",
    "pic_planet",
    "pic_comet",
    "pic_break",
    "pic_silver_cup",
    // More things of the game, in place of the faces. New pictures go at the
    // end: a code, once chosen, stays.
    "pic_two",
    "pic_three",
    "pic_four",
    "pic_five",
    "pic_six",
    "pic_seven",
    "pic_nine",
    "pic_ten",
    "pic_eleven",
    "pic_twelve",
    "pic_thirteen",
    "pic_fourteen",
    "pic_triangle",
    "pic_bridge",
    "pic_cue_rack",
    "pic_net",
    "pic_beads",
    "pic_stopwatch",
    "pic_bank",
    "pic_curve",
    "pic_jump",
    "pic_combo",
    "pic_ghost_ball",
    "pic_kiss",
    "pic_brush",
    "pic_case",
    "pic_gold_eight",
    "pic_wings",
    "pic_podium",
    "pic_strike",
    "pic_reds",
    "pic_fire_cue",
];

/// The faces the pictures once had: still accepted (an older client may
/// send one, an account may have one), each shown as one of the things of the
/// game instead.
pub const RETIRED_AVATARS: [(&str, &str); 32] = [
    ("sunny", "pic_two"),
    ("marco", "pic_three"),
    ("zuri", "pic_four"),
    ("kai", "pic_five"),
    ("mei", "pic_six"),
    ("omar", "pic_seven"),
    ("lena", "pic_nine"),
    ("theo", "pic_ten"),
    ("nia", "pic_eleven"),
    ("raul", "pic_twelve"),
    ("ana", "pic_thirteen"),
    ("joe", "pic_fourteen"),
    ("iris", "pic_triangle"),
    ("sam", "pic_bridge"),
    ("hugo", "pic_cue_rack"),
    ("lua", "pic_net"),
    ("duda", "pic_beads"),
    ("bento", "pic_stopwatch"),
    ("rex", "pic_bank"),
    ("yuki", "pic_curve"),
    ("malik", "pic_jump"),
    ("clara", "pic_combo"),
    ("dante", "pic_ghost_ball"),
    ("flor", "pic_kiss"),
    ("gus", "pic_brush"),
    ("tati", "pic_case"),
    ("nico", "pic_gold_eight"),
    ("jade", "pic_wings"),
    ("beto", "pic_podium"),
    ("vera", "pic_strike"),
    ("leon", "pic_reds"),
    ("kiara", "pic_fire_cue"),
];

/// A picture from [`AVATARS`] (or the empty string: none), or a retired
/// one (see [`RETIRED_AVATARS`]).
pub fn valid_avatar(code: &str) -> bool {
    code.is_empty() || AVATARS.contains(&code) || RETIRED_AVATARS.iter().any(|(r, _)| *r == code)
}

/// The picture shown for `code`: a retired face's thing of the game, or
/// the code itself.
pub fn shown_avatar(code: &str) -> &str {
    RETIRED_AVATARS.iter().find(|(r, _)| *r == code).map_or(code, |(_, shown)| shown)
}

/// Position of `code` in [`COUNTRIES`] (and in the flag atlas).
pub fn country_index(code: &str) -> Option<usize> {
    COUNTRIES.iter().position(|(c, _, _)| c.eq_ignore_ascii_case(code))
}

/// Maximum length of a chat message, in characters.
pub const MAX_CHAT_LEN: usize = 200;
/// Maximum length of a table or tournament name, in characters.
pub const MAX_NAME_LEN: usize = 32;
/// Private tables and tournaments: password length in characters.
pub const MAX_ROOM_PASSWORD: usize = 32;

/// Removes control and invisible formatting characters (bidirectional
/// overrides, zero-width joiners…) and surrounding whitespace, collapses
/// runs of spaces and truncates to `max` characters. The joiners and
/// selectors that are part of an emoji stay (❤️, 🏳️‍🌈, 1️⃣, England's
/// flag), and the cut never falls inside one: an emoji that doesn't fit
/// goes whole.
pub fn clean_text(text: &str, max: usize) -> String {
    let mut out: Vec<char> = Vec::new();
    let mut last_space = true;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        // A line break or a tab is a space ("hello\nthere" stays two words);
        // other control characters go.
        if c.is_control() && !c.is_whitespace() {
            continue;
        }
        if is_invisible(c) && !in_emoji(out.last().copied(), c, chars.peek().copied()) {
            continue;
        }
        // A flag's tags are a few letters and the tag that ends them: a
        // longer run is text nobody sees (it would pass under any filter).
        let is_tag = |c: &char| ('\u{E0020}'..='\u{E007E}').contains(c);
        if is_tag(&c) && out.iter().rev().take_while(|c| is_tag(c)).count() >= 6 {
            continue;
        }
        let space = c.is_whitespace();
        if space && last_space {
            continue;
        }
        out.push(if space { ' ' } else { c });
        last_space = space;
        // One more than fits shows where to cut.
        if out.len() > max {
            break;
        }
    }
    let mut end = out.len().min(max);
    while end > 0 && end < out.len() && joined(&out, end) {
        end -= 1;
    }
    out[..end].iter().collect::<String>().trim().to_string()
}

/// Pictographs: the characters emoji are made of (roughly Unicode's
/// Extended_Pictographic, with the skin tones).
fn is_pictograph(c: char) -> bool {
    matches!(c,
        '\u{A9}' | '\u{AE}' | '\u{203C}' | '\u{2049}' | '\u{2122}' | '\u{2139}'
        | '\u{2194}'..='\u{2199}' | '\u{21A9}' | '\u{21AA}' | '\u{231A}' | '\u{231B}'
        | '\u{2328}' | '\u{23CF}' | '\u{23E9}'..='\u{23FA}' | '\u{24C2}' | '\u{25AA}'
        | '\u{25AB}' | '\u{25B6}' | '\u{25C0}' | '\u{25FB}'..='\u{25FE}' | '\u{2600}'..='\u{27BF}'
        | '\u{2934}' | '\u{2935}' | '\u{2B05}'..='\u{2B07}' | '\u{2B1B}' | '\u{2B1C}' | '\u{2B50}'
        | '\u{2B55}' | '\u{3030}' | '\u{303D}' | '\u{3297}' | '\u{3299}' | '\u{1F000}'..='\u{1FAFF}')
}

/// Whether `c`, an invisible character after `prev` (kept already) and
/// before `next`, is part of an emoji: U+FE0F after a pictograph (❤️) or a
/// keycap's character (1️⃣), a joiner between two pictographs (🏳️‍🌈, 🤷‍♂️;
/// never between letters), the tags of a black flag (England's).
fn in_emoji(prev: Option<char>, c: char, next: Option<char>) -> bool {
    let Some(p) = prev else { return false };
    match c {
        '\u{FE0F}' => {
            is_pictograph(p) || (matches!(p, '#' | '*' | '0'..='9') && next == Some('\u{20E3}'))
        }
        '\u{200D}' => (is_pictograph(p) || p == '\u{FE0F}') && next.is_some_and(is_pictograph),
        '\u{E0020}'..='\u{E007F}' => p == '\u{1F3F4}' || ('\u{E0020}'..='\u{E007E}').contains(&p),
        _ => false,
    }
}

/// Whether characters `k - 1` and `k` of `s` belong to the same emoji (a
/// cut between them would leave half of it).
fn joined(s: &[char], k: usize) -> bool {
    let (a, b) = (s[k - 1], s[k]);
    let regional = |c: char| ('\u{1F1E6}'..='\u{1F1FF}').contains(&c);
    if regional(a) && regional(b) {
        // Flags pair the letters from the first of a run.
        return s[..k].iter().rev().take_while(|&&c| regional(c)).count() % 2 == 1;
    }
    a == '\u{200D}'
        || matches!(b, '\u{FE0F}' | '\u{200D}' | '\u{20E3}' | '\u{1F3FB}'..='\u{1F3FF}'
            | '\u{E0020}'..='\u{E007F}')
}

/// Characters that take no room or show nothing: format marks (Unicode's
/// "Cf"), variation selectors, tags, and the blank "fillers" some scripts
/// have (a name or a line made only of them looks empty).
fn is_invisible(c: char) -> bool {
    matches!(c,
        '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}'
        | '\u{FEFF}' | '\u{00AD}' | '\u{034F}'
        // The Arabic letter mark, Mongolian variation selectors and vowel
        // separator.
        | '\u{061C}' | '\u{180B}'..='\u{180F}'
        // Hangul fillers and the braille blank.
        | '\u{115F}' | '\u{1160}' | '\u{3164}' | '\u{FFA0}' | '\u{2800}'
        // Khmer's inherent vowels, shorthand format controls.
        | '\u{17B4}' | '\u{17B5}' | '\u{1BCA0}'..='\u{1BCA3}'
        // Variation selectors, interlinear annotations, musical formatting.
        | '\u{FE00}'..='\u{FE0F}' | '\u{FFF9}'..='\u{FFFB}' | '\u{1D159}'
        | '\u{1D173}'..='\u{1D17A}'
        // Egyptian hieroglyph format controls.
        | '\u{13430}'..='\u{1343F}'
        // Tags and the supplementary variation selectors.
        | '\u{E0000}'..='\u{E007F}' | '\u{E0100}'..='\u{E01EF}')
}

/// Cleans a chat message (see [`clean_text`]).
pub fn clean_chat(text: &str) -> String {
    clean_text(text, MAX_CHAT_LEN)
}

/// Quick phrases for the chat at the table, sent as they are written here
/// (in English); each game shows them in its player's language, so two
/// players without a language in common still understand each other.
pub const QUICK_CHAT: [&str; 14] = [
    "Good luck, have fun!",
    "Nice shot!",
    "Great safety!",
    "Unlucky!",
    "Oops!",
    "Wow!",
    "That was close!",
    "Take your time.",
    "Sorry!",
    "Thanks!",
    "Well played!",
    "Good game!",
    "Rematch?",
    "I have to go, thanks for the game!",
];

/// Maximum length of a status message ("looking for a practice partner"),
/// in characters.
pub const MAX_STATUS_LEN: usize = 60;
/// Maximum length of a note on the "looking for a game" board.
pub const MAX_SEEK_LEN: usize = 60;
/// Maximum length of the message that goes with a gift.
pub const MAX_GIFT_MESSAGE: usize = 80;

/// Cleans a status message (see [`clean_text`]).
pub fn clean_status(text: &str) -> String {
    clean_text(text, MAX_STATUS_LEN)
}

/// Cleans a note for the "looking for a game" board (see [`clean_text`]).
pub fn clean_seek(text: &str) -> String {
    clean_text(text, MAX_SEEK_LEN)
}

/// Rating tiers: shown as the colour of a player's name everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Rookie,
    Amateur,
    Skilled,
    Expert,
    Master,
    Grandmaster,
    Legend,
}

impl Tier {
    pub const ALL: [Tier; 7] = [
        Tier::Rookie,
        Tier::Amateur,
        Tier::Skilled,
        Tier::Expert,
        Tier::Master,
        Tier::Grandmaster,
        Tier::Legend,
    ];

    /// The tier for a rating.
    pub fn of(rating: i32) -> Tier {
        Tier::ALL.into_iter().rev().find(|t| rating >= t.min_rating()).unwrap_or(Tier::Rookie)
    }

    pub fn min_rating(self) -> i32 {
        match self {
            Tier::Rookie => i32::MIN,
            // Everyone starts at 1200, a Rookie: every tier is earned.
            Tier::Amateur => 1250,
            Tier::Skilled => 1300,
            Tier::Expert => 1500,
            Tier::Master => 1700,
            Tier::Grandmaster => 1900,
            Tier::Legend => 2100,
        }
    }

    /// Coins for reaching this league for the first time (none for the
    /// one everybody starts in).
    pub fn prize(self) -> u32 {
        match self {
            Tier::Rookie => 0,
            Tier::Amateur => 50,
            Tier::Skilled => 100,
            Tier::Expert => 200,
            Tier::Master => 400,
            Tier::Grandmaster => 700,
            Tier::Legend => 1000,
        }
    }

    /// The achievement that comes with reaching this league (and carries
    /// the league's item in the shop).
    pub fn achievement(self) -> Option<&'static str> {
        match self {
            Tier::Rookie | Tier::Amateur => None,
            Tier::Skilled => Some("rating_1300"),
            Tier::Expert => Some("rating_1500"),
            Tier::Master => Some("rating_1700"),
            Tier::Grandmaster => Some("rating_1900"),
            Tier::Legend => Some("rating_2100"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tier::Rookie => "Rookie",
            Tier::Amateur => "Amateur",
            Tier::Skilled => "Skilled",
            Tier::Expert => "Expert",
            Tier::Master => "Master",
            Tier::Grandmaster => "Grandmaster",
            Tier::Legend => "Legend",
        }
    }
}

/// Account roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    #[default]
    Player,
    Moderator,
    Admin,
}

impl Role {
    pub fn from_i16(v: i16) -> Role {
        match v {
            2 => Role::Admin,
            1 => Role::Moderator,
            _ => Role::Player,
        }
    }

    pub fn as_i16(self) -> i16 {
        self as i16
    }

    pub fn is_staff(self) -> bool {
        self >= Role::Moderator
    }
}

/// What a player is doing in the lounge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerStatus {
    #[default]
    Available,
    /// Seated at a table, waiting for an opponent.
    Waiting,
    Playing,
}

/// A player's record under one of the rules: their rating and their rated
/// games.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Standing {
    pub rating: i32,
    pub games: u32,
    pub wins: u32,
}

impl Default for Standing {
    /// No rated game yet: the starting rating.
    fn default() -> Self {
        Standing {
            rating: snook_legend_core::rating::EloConfig::default().initial_rating,
            games: 0,
            wins: 0,
        }
    }
}

impl Standing {
    pub fn tier(&self) -> Tier {
        Tier::of(self.rating)
    }

    /// Wins over rated games, 0..=100.
    pub fn win_rate(&self) -> u32 {
        (self.wins * 100).checked_div(self.games).unwrap_or(0)
    }
}

/// Public information about a player.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub username: String,
    /// The rules they play: the ones they chose in their game (online games,
    /// "Play now", tables and games against the computer follow them).
    pub rules: Rules,
    /// Their rating and rated games under each of the rules.
    pub standings: PerRules<Standing>,
    pub status: PlayerStatus,
    /// ISO 3166-1 alpha-2 code, or empty.
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub role: Role,
    /// A guest on a LAN server (no account, no rating).
    #[serde(default)]
    pub guest: bool,
    /// A VIP member (a small badge by the name).
    #[serde(default)]
    pub vip: bool,
    /// The cue and chalk they play with (shop codes; empty: the defaults),
    /// so the opponent sees them too.
    #[serde(default)]
    pub cue: String,
    #[serde(default)]
    pub chalk: String,
    /// Avatar frame and name style (shop codes; empty: none).
    #[serde(default)]
    pub frame: String,
    #[serde(default)]
    pub name_style: String,
    /// What happens when they win on the 8 (shop code; empty: nothing).
    #[serde(default)]
    pub effect: String,
    /// The level of their cue (online wins with it; marks on the cue).
    #[serde(default)]
    pub cue_level: u8,
    /// A short line the player wrote about themself (empty: none). See
    /// [`MAX_STATUS_LEN`].
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status_message: String,
    /// Playing a game against the computer (still in the lounge, and can
    /// still be invited: the invitation shows over that game).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub vs_computer: bool,
    /// Nobody has touched their game for a while (an invitation may wait).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub away: bool,
    /// Playing in focus mode (a game outside the online ones, with the
    /// game's focus mode on): invitations are turned away until it ends.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub focus: bool,
    /// In the "Play now" queue, looking for an opponent of their rules.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub searching: bool,
    /// The account level (see [`xp`]; 0: not known, e.g. a guest).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub level: u32,
    /// Their profile picture (a code from [`AVATARS`]; empty: none).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub avatar: String,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl PlayerInfo {
    /// Their record under `rules`.
    pub fn standing(&self, rules: Rules) -> Standing {
        *self.standings.get(rules)
    }

    /// Their rating under `rules`.
    pub fn rating_in(&self, rules: Rules) -> i32 {
        self.standings.get(rules).rating
    }

    /// Their rating under the rules they play (see [`Self::rules`]).
    pub fn rating(&self) -> i32 {
        self.rating_in(self.rules)
    }

    /// Their tier under `rules` (Rookie for a guest).
    pub fn tier_in(&self, rules: Rules) -> Tier {
        if self.guest { Tier::Rookie } else { Tier::of(self.rating_in(rules)) }
    }

    /// Their tier under the rules they play.
    pub fn tier(&self) -> Tier {
        self.tier_in(self.rules)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub rank: u32,
    pub username: String,
    pub rating: i32,
    pub games: u32,
    pub wins: u32,
    #[serde(default)]
    pub country: String,
    /// A VIP member (a crown by the name).
    #[serde(default)]
    pub vip: bool,
    /// An administrator (the name is red).
    #[serde(default)]
    pub admin: bool,
}

/// Who wrote a chat line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatKind {
    #[default]
    Player,
    /// An announcement from the staff (shown highlighted).
    Announcement,
    /// Something that happened in the game (shown dimmed, without a name).
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatLine {
    pub from: String,
    pub text: String,
    /// Unix time in seconds.
    pub at: i64,
    #[serde(default)]
    pub kind: ChatKind,
    /// The writer's rating (for the name colour); 0 when unknown.
    #[serde(default)]
    pub rating: i32,
    /// The line's number in the lounge chat (0: not a lounge chat line).
    #[serde(default)]
    pub id: u64,
    /// Written by an administrator (the name is red).
    #[serde(default)]
    pub admin: bool,
    /// Deleted (its text is gone): by whom.
    #[serde(default)]
    pub deleted: Option<Deleted>,
}

/// Who deleted a chat line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Deleted {
    /// The one who wrote it.
    Author,
    /// An administrator.
    Admin,
}

/// The aim of the player at the table, streamed so the opponent sees the cue move.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AimState {
    pub yaw: f64,
    pub elevation: f64,
    pub tip: DVec2,
    pub power: f32,
    /// The pocket called for the 8-ball, if any.
    #[serde(default)]
    pub called_pocket: Option<PocketId>,
}

impl AimState {
    /// Within what a cue can do: the angle in one turn, the elevation from
    /// level to upright, the tip on the ball, the power on the bar. What a
    /// player's game sends is passed on like this (a modified game could
    /// send absurd numbers to the opponent's).
    pub fn tidied(self) -> AimState {
        let tip = if self.tip.length() > 1.0 {
            // Scaled first: a huge tip's length would overflow.
            let m = self.tip.x.abs().max(self.tip.y.abs());
            (self.tip / m).normalize_or_zero()
        } else {
            self.tip
        };
        AimState {
            yaw: self.yaw.rem_euclid(std::f64::consts::TAU),
            elevation: self.elevation.clamp(0.0, std::f64::consts::FRAC_PI_2),
            tip,
            power: self.power.clamp(0.0, 1.0),
            called_pocket: self.called_pocket,
        }
    }

    pub fn is_finite(&self) -> bool {
        self.yaw.is_finite()
            && self.elevation.is_finite()
            && self.tip.is_finite()
            && self.power.is_finite()
    }
}

/// A table in the lounge that someone opened and is waiting at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableInfo {
    pub id: u64,
    pub host: PlayerInfo,
    pub rated: bool,
    pub rules: Rules,
    /// Joining needs a password.
    pub private: bool,
}

/// One match of a tournament bracket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BracketMatch {
    /// Usernames; `None` is a bye (or a player not decided yet).
    pub players: [Option<String>; 2],
    pub winner: Option<usize>,
    /// Being played right now.
    #[serde(default)]
    pub live: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentState {
    Registering,
    Running,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TournamentInfo {
    pub id: u64,
    pub name: String,
    pub host: String,
    /// Maximum number of players (4, 8 or 16).
    pub size: u32,
    pub rated: bool,
    pub rules: Rules,
    pub private: bool,
    pub state: TournamentState,
    pub players: Vec<PlayerInfo>,
    /// Rounds of the bracket, first round first (empty while registering).
    pub rounds: Vec<Vec<BracketMatch>>,
    pub champion: Option<String>,
    /// Coins to enter (0: free). The prizes come from the pot.
    #[serde(default)]
    pub entry_fee: u32,
    /// Prizes in coins for the champion and the runner-up.
    #[serde(default)]
    pub prizes: [u32; 2],
    /// Organised by the server (e.g. the Saturday cup), starting at this
    /// Unix time.
    #[serde(default)]
    pub starts_at: Option<i64>,
    #[serde(default)]
    pub format: TournamentFormat,
    /// Arena: points so far, best first (username, points, games, on a streak).
    #[serde(default)]
    pub standings: Vec<ArenaStanding>,
    /// Arena: Unix time the arena closes.
    #[serde(default)]
    pub ends_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentFormat {
    /// A bracket: lose once and you are out.
    #[default]
    Knockout,
    /// Play as many games as you can until the time is up: a win is 2
    /// points, 4 on a winning streak (from the third win in a row).
    Arena,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArenaStanding {
    pub username: String,
    pub points: u32,
    pub games: u32,
    /// Wins in a row right now (points double from the third).
    pub streak: u32,
}

/// A finished game in a player's history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub opponent: String,
    pub opponent_country: String,
    pub opponent_rating: i32,
    /// The opponent is an administrator (the name is red).
    #[serde(default)]
    pub opponent_admin: bool,
    pub won: bool,
    pub reason: GameOverReason,
    pub rated: bool,
    pub rules: Rules,
    /// Rating change for this player (zero for casual games).
    pub delta: i32,
    /// This player's rating after the game.
    pub rating_after: i32,
    pub tournament: bool,
    /// Unix time in seconds.
    pub at: i64,
    /// The stored match (for its replay); 0 if unknown.
    #[serde(default)]
    pub match_id: i64,
    /// The replay can be watched (seed and strokes were kept).
    #[serde(default)]
    pub replay: bool,
    /// A correspondence game (played a turn at a time).
    #[serde(default)]
    pub correspondence: bool,
}

/// Lifetime numbers from every game with stored statistics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct CareerNumbers {
    pub games: u32,
    pub shots: u32,
    pub potting_shots: u32,
    pub balls: u32,
    pub fouls: u32,
    /// Most balls in one visit, in any game.
    pub best_run: u32,
}

impl CareerNumbers {
    /// Potting success, 0..=100.
    pub fn pot_rate(&self) -> u32 {
        (self.potting_shots * 100).checked_div(self.shots).unwrap_or(0)
    }
    /// Balls per game, times ten (one decimal).
    pub fn balls_per_game_x10(&self) -> u32 {
        (self.balls * 10).checked_div(self.games).unwrap_or(0)
    }
}

/// A game that can be watched again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayInfo {
    pub match_id: i64,
    pub players: [String; 2],
    pub countries: [String; 2],
    pub ratings: [i32; 2],
    pub winner: usize,
    pub rated: bool,
    pub at: i64,
    pub replay: replay::Replay,
}

/// The game with the highest ratings being played right now (anyone may watch).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeaturedMatch {
    pub match_id: u64,
    pub players: [PlayerInfo; 2],
    pub rated: bool,
    pub rules: Rules,
    pub spectators: u32,
}

/// How the viewer relates to a profile's owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    #[default]
    None,
    /// The viewer's own profile.
    Me,
    Friend,
    /// The viewer asked to be friends.
    RequestSent,
    /// The owner asked the viewer to be friends.
    RequestReceived,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub player: PlayerInfo,
    pub online: bool,
    pub stats: Stats,
    /// Unix time in seconds.
    pub created_at: i64,
    /// Rank in the world ranking of each of the rules (None while they have
    /// no rated game there).
    pub ranks: PerRules<Option<u32>>,
    /// The highest rating they reached under each of the rules.
    pub peaks: PerRules<i32>,
    /// Unlocked achievement codes with the unlock time.
    pub achievements: Vec<(String, i64)>,
    pub history: Vec<HistoryEntry>,
    pub relation: Relation,
    /// When they were last online: only for friends (0 otherwise).
    #[serde(default)]
    pub last_seen: i64,
    /// Rating after each rated game under each of the rules, oldest first
    /// (Unix time, rating).
    #[serde(default)]
    pub rating_history: PerRules<Vec<(i64, i32)>>,
    #[serde(default)]
    pub career: CareerNumbers,
    /// Share of rated and casual games left unfinished, 0..=100.
    #[serde(default)]
    pub abandon_rate: u32,
    /// How this player pots (pockets, distances, kinds of shot, the table).
    /// Only sent to games of protocol 13 or later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis: Option<Box<analysis::ShotAnalysis>>,
    /// Online games won with each cue (cue code, wins), most wins first.
    #[serde(default)]
    pub cue_wins: Vec<(String, u32)>,
    /// What each cue has done online (only cues played with since these
    /// were counted).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cue_stats: Vec<CueStats>,
    /// What they won (tournaments, the ladder…), newest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trophies: Vec<Trophy>,
    /// The reactions they bought in the shop (codes; see
    /// [`reactions::bought`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<String>,
}

/// Something a player won, for the profile's trophy shelf.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trophy {
    pub kind: TrophyKind,
    /// The tournament's name (empty for the others).
    #[serde(default)]
    pub name: String,
    /// How many took part (a tournament's players, a day's challengers).
    #[serde(default)]
    pub players: u32,
    /// When (Unix seconds).
    pub at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrophyKind {
    /// Won a tournament.
    Tournament,
    /// Beat the whole computer ladder.
    Ladder,
    /// First in a day's challenge.
    DailyChallenge,
    /// A kind this version doesn't know yet.
    #[serde(other)]
    Other,
}

/// One cue's online record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CueStats {
    /// Shop code.
    pub code: String,
    pub games: u32,
    pub wins: u32,
    /// Object balls made with it.
    pub potted: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FriendInfo {
    pub player: PlayerInfo,
    pub online: bool,
    /// When they were last online (Unix seconds; 0: unknown). Only friends
    /// get this.
    #[serde(default)]
    pub last_seen: i64,
}

/// A report goes with the game being played between the two players or,
/// failing that, their latest game that ended this long ago at most (seconds).
pub const REPORT_GAME_WINDOW_SECS: i64 = 24 * 60 * 60;
/// Someone this player played online recently (the latest game against them).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentOpponent {
    /// As they are now (rating, country, status message).
    pub player: PlayerInfo,
    pub online: bool,
    /// When the latest game against them ended (Unix seconds).
    pub at: i64,
    /// This player won that game.
    pub won: bool,
    pub rated: bool,
    /// Games against them (among the latest few hundred).
    #[serde(default)]
    pub games: u32,
}

/// A present between friends (received or given).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GiftInfo {
    pub id: i64,
    /// The giver (empty if the account is gone) and their country.
    pub from: String,
    #[serde(default)]
    pub from_country: String,
    pub to: String,
    #[serde(default)]
    pub to_country: String,
    /// An item code (see [`shop::item`]), or [`shop::VIP_GIFT`]: a month of VIP.
    pub code: String,
    /// A few words from the giver (may be empty).
    #[serde(default)]
    pub message: String,
    /// Unix seconds.
    pub at: i64,
    /// The receiver has seen it.
    #[serde(default)]
    pub seen: bool,
    /// The receiver said thank you.
    #[serde(default)]
    pub thanked: bool,
}

/// A note on the lounge's "looking for a game" board: one per player, up
/// until they start a game, leave or it expires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeekNote {
    pub username: String,
    /// What they are after (may be empty: just "looking for a game").
    pub text: String,
    pub rated: bool,
    pub rules: Rules,
    /// Posted at (Unix seconds).
    pub at: i64,
    /// Taken down at (Unix seconds).
    pub expires_at: i64,
}

/// What a player may report another for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportReason {
    Harassment,
    Cheating,
    OffensiveName,
    Spam,
    Other,
}

impl ReportReason {
    pub const ALL: [ReportReason; 5] = [
        ReportReason::Harassment,
        ReportReason::Cheating,
        ReportReason::OffensiveName,
        ReportReason::Spam,
        ReportReason::Other,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ReportReason::Harassment => "Harassment",
            ReportReason::Cheating => "Cheating or abuse",
            ReportReason::OffensiveName => "Offensive name",
            ReportReason::Spam => "Spam",
            ReportReason::Other => "Other",
        }
    }
}

/// Messages from the client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Must be the first message. `rules`: the rules the player plays (see
    /// [`ClientMsg::SetRules`]), so the lounge shows them from the start.
    Hello {
        version: u32,
        #[serde(default)]
        rules: Rules,
        /// The Microsoft Store build says whether it sells coin packs and
        /// VIP as Store add-ons (its build decides: the feature
        /// `store-purchases`); other builds say nothing. The staff portal
        /// shows what the Store's game does.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        store_sales: Option<bool>,
    },
    Register {
        username: String,
        password: String,
        #[serde(default)]
        country: String,
        /// Under 18 (`Some(true)`) or 18 or older; `None` when not asked
        /// (the game asks when the player first writes to others).
        #[serde(default)]
        minor: Option<bool>,
    },
    Login {
        username: String,
        password: String,
    },
    /// Log in again with a token from an earlier `AuthOk`.
    Resume {
        token: String,
    },
    /// Join a LAN server without an account (only where the server allows it).
    Guest {
        name: String,
    },
    Logout,
    /// Lounge chat.
    Chat {
        text: String,
    },
    /// Deletes a lounge chat line: one's own, or any (administrators).
    /// `silent` (administrators): it disappears without a trace.
    DeleteChat {
        id: u64,
        #[serde(default)]
        silent: bool,
    },
    /// Clears the lounge chat (administrators).
    ClearChat,
    /// Chat with the opponent during a match.
    MatchChat {
        text: String,
    },
    /// A reaction (see [`REACTIONS`]) during a match, as a player or a
    /// spectator.
    React {
        code: String,
    },
    /// A spectator cheers for the player in `seat` (0 or 1).
    Cheer {
        seat: usize,
    },
    /// The shop: the offers and this player's wallet.
    Shop,
    /// Buys a cosmetic item with coins.
    BuyItem {
        code: String,
    },
    /// Uses an item (one per category).
    Equip {
        code: String,
    },
    /// Starts paying for a coin pack: the server answers with a payment page.
    BuyCoins {
        pack: String,
    },
    /// Starts the VIP subscription (a payment page).
    SubscribeVip,
    /// Starts paying for the welcome pack (once per account).
    BuyWelcomePack,
    /// Opens the page that manages (or cancels) the subscription.
    ManageVip,
    /// Starts paying for a month of VIP for a friend (a payment page).
    GiftVip {
        to: String,
    },
    /// The Microsoft Store build, after a purchase (or to finish earlier
    /// ones): asks for the ticket the Store needs to name the player to the
    /// server (answered with [`ServerMsg::StoreTicket`]).
    StoreTicket,
    /// The player's Microsoft Store ID key (made by the Store from the
    /// ticket): the server finds the coin packs bought and not yet received,
    /// has Microsoft consume them and credits the coins (answered with
    /// [`ServerMsg::StoreRedeemed`]).
    StoreRedeem {
        key: String,
    },
    /// "Play now": wait for an opponent of a similar rating under these
    /// rules.
    QueueJoin {
        rated: bool,
        rules: Rules,
    },
    QueueLeave,
    /// Watch the games of this friend (follows them from game to game).
    Spectate {
        username: String,
    },
    StopSpectating,
    Invite {
        to: String,
        rated: bool,
        rules: Rules,
    },
    CancelInvite {
        to: String,
    },
    AnswerInvite {
        from: String,
        accept: bool,
    },
    /// Opens a table in the lounge and waits there for an opponent.
    OpenTable {
        rated: bool,
        rules: Rules,
        /// Set for a private table (friends join with the password).
        #[serde(default)]
        password: Option<String>,
    },
    JoinTable {
        id: u64,
        #[serde(default)]
        password: Option<String>,
    },
    CloseTable,
    /// Moves the cue ball while it is in hand (shown to the opponent).
    PlaceCueBall {
        position: DVec2,
    },
    /// Current aim (send at most ~20 times per second).
    Aim {
        aim: AimState,
    },
    /// Takes the shot. `cue_ball` is the final position when the ball was in
    /// hand; `turn` is the game's `shots_taken()` the shot was aimed on.
    Shoot {
        turn: u32,
        shot: Shot,
        called_pocket: Option<PocketId>,
        cue_ball: Option<DVec2>,
    },
    /// The answer to the break's choice of the official rules (see
    /// [`snook_legend_core::EightBallGame::pending_choice`]); `turn` is the
    /// game's `shots_taken()` it was made on.
    Choose {
        turn: u32,
        option: BreakOption,
    },
    Resign,
    /// Ask the same opponent for another game after a match ends.
    Rematch,
    /// Go back to the lounge after a match ends.
    LeaveMatch,
    /// The world ranking of these rules.
    Leaderboard {
        rules: Rules,
    },
    /// The rules this player plays now (their choice in the game): shown to
    /// the others in the lounge, and counted in the numbers of players
    /// online under each.
    SetRules {
        rules: Rules,
    },
    Profile {
        username: String,
    },
    SetCountry {
        country: String,
    },
    /// Choose a profile picture (a code from [`AVATARS`]; empty: none).
    SetAvatar {
        avatar: String,
    },
    Friends,
    FriendRequest {
        username: String,
    },
    FriendAnswer {
        username: String,
        accept: bool,
    },
    FriendRemove {
        username: String,
    },
    /// Hides a player's chat and refuses their invitations.
    Block {
        username: String,
    },
    Unblock {
        username: String,
    },
    Report {
        username: String,
        reason: ReportReason,
        #[serde(default)]
        details: String,
    },
    Tournaments,
    CreateTournament {
        name: String,
        size: u32,
        rated: bool,
        rules: Rules,
        #[serde(default)]
        password: Option<String>,
        /// Coins to enter (0: free).
        #[serde(default)]
        entry_fee: u32,
        #[serde(default)]
        format: TournamentFormat,
    },
    JoinTournament {
        id: u64,
        #[serde(default)]
        password: Option<String>,
    },
    LeaveTournament {
        id: u64,
    },
    /// The host starts the tournament early (with byes for the empty seats).
    StartTournament {
        id: u64,
    },
    /// The account page: recovery e-mail and the e-mail of the last payment.
    Account,
    ChangePassword {
        old: String,
        new: String,
    },
    /// Sets (or clears, when empty) the e-mail used to recover the account.
    SetRecoveryEmail {
        email: String,
        /// The account's password: a session alone (a stolen one) can't
        /// change where the account is recovered from.
        #[serde(default)]
        password: String,
    },
    /// Changes the player's username (at most [`MAX_NAME_CHANGES`] times).
    /// Refused during a game, at a table, in the queue or in a tournament.
    ChangeUsername {
        username: String,
    },
    /// Deletes the account for good (it is anonymised: the opponents keep
    /// their history).
    DeleteAccount {
        password: String,
    },
    /// Gives a friend an item, paid with this player's coins.
    GiftItem {
        to: String,
        code: String,
        /// A few words for the friend (see [`MAX_GIFT_MESSAGE`]).
        #[serde(default)]
        message: String,
    },
    /// The presents this player received and gave.
    Gifts,
    /// The player saw these presents (they are not shown again).
    GiftsSeen {
        ids: Vec<i64>,
    },
    /// Thanks the giver of a present (once per present).
    ThankGift {
        id: i64,
    },
    /// Today's and this week's missions, with this player's progress.
    Missions,
    /// This player's games that can be watched again.
    Replays,
    /// The strokes of a stored game.
    Replay {
        match_id: i64,
    },
    /// Watches any game in progress (the featured table).
    WatchMatch {
        match_id: u64,
    },
    /// A code (and link) a friend can use to sit at this player's table.
    CreateInviteCode {
        rated: bool,
        rules: Rules,
    },
    JoinByCode {
        code: String,
    },
    /// Correspondence games: this player's games and challenges.
    CorrList,
    /// Challenges a player (online or not) to a correspondence game.
    CorrChallenge {
        to: String,
        rated: bool,
        rules: Rules,
    },
    /// Accepts or declines a challenge sent to this player.
    CorrAnswer {
        id: i64,
        accept: bool,
    },
    /// Withdraws a challenge this player sent.
    CorrCancel {
        id: i64,
    },
    /// The whole game, to look at it or play a turn.
    CorrOpen {
        id: i64,
    },
    /// A shot of this player's visit (like [`ClientMsg::Shoot`]).
    CorrShoot {
        id: i64,
        turn: u32,
        shot: Shot,
        called_pocket: Option<PocketId>,
        cue_ball: Option<DVec2>,
    },
    /// The answer to a break's choice in a correspondence game (like
    /// [`ClientMsg::Choose`]).
    CorrChoose {
        id: i64,
        turn: u32,
        option: BreakOption,
    },
    CorrResign {
        id: i64,
    },
    /// The age group of an account that has none yet, given to turn the
    /// chat on; it can't be changed afterwards from the game.
    SetAge {
        minor: bool,
    },
    /// A parent or guardian turns the chat of an account under 18 on or off
    /// (see [`ServerMsg::Age`]); right after [`ClientMsg::SetAge`] for an
    /// account that had no age group.
    AllowChat {
        allowed: bool,
    },
    /// Sets this player's status message (empty clears it).
    SetStatusMessage {
        text: String,
    },
    /// Clears another player's status message (staff).
    ClearStatusMessage {
        username: String,
    },
    /// What this player is doing outside online games, shown by their name
    /// in the lounge: playing the computer, or away from the game.
    SetActivity {
        #[serde(default)]
        vs_computer: bool,
        #[serde(default)]
        away: bool,
        /// Playing in focus mode: no invitations until the game is over
        /// (those waiting are withdrawn). Protocol 19 and later.
        #[serde(default)]
        focus: bool,
    },
    /// The people this player played recently.
    RecentOpponents,
    /// Puts (or replaces) this player's note on the "looking for a game"
    /// board.
    PostSeek {
        #[serde(default)]
        text: String,
        rated: bool,
        rules: Rules,
    },
    /// Takes a note off the board: this player's own (empty `username`), or
    /// anyone's (staff).
    RemoveSeek {
        #[serde(default)]
        username: String,
    },
    /// Claims the ladder's reward with the recorded game won against its
    /// last opponent (the server plays it again to check it).
    LadderClaim {
        replay: replay::Replay,
    },
    /// Today's challenge: the table, attempts left and the leaders.
    Daily,
    /// Starts an attempt at today's challenge (one of [`daily::ATTEMPTS`]):
    /// the server sends the position's seed.
    DailyStart,
    /// The strokes of the attempt under way (the server plays them again).
    DailySubmit {
        day: i64,
        strokes: Vec<Shot>,
    },
    /// The drills: this player's bests, the tables and the challenge rating.
    Drills,
    /// A drill session's strokes (the server plays them again).
    DrillSubmit {
        code: String,
        /// The session the server started ([`ServerMsg::DrillStarted`]).
        #[serde(default)]
        session: u64,
        strokes: Vec<Shot>,
    },
    /// A drill session about to be played for the tables: the server sets
    /// where each try's cue ball lies ([`ServerMsg::DrillStarted`]).
    DrillStart {
        code: String,
    },
    /// The tour's final is about to be played for the reward: the server
    /// deals its rack and who breaks ([`ServerMsg::LadderTicket`]).
    LadderTicket,
    /// The medals this account won in tournaments against the computer
    /// ([`ServerMsg::CupMedals`]).
    CupMedals,
    /// A medal won in a tournament against the computer, for this account
    /// (kept once per tournament; answered with [`ServerMsg::CupMedals`]).
    CupMedal {
        medal: cup::Medal,
    },
    /// How a player plays (aggregated numbers), for a computer opponent in
    /// their style.
    GhostStyle {
        username: String,
    },
    Ping,
}

/// Why a match ended, from the server's point of view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchResult {
    pub winner: usize,
    pub reason: GameOverReason,
    pub rated: bool,
    /// Rating change for seats 0 and 1 (zero for casual games).
    pub rating_delta: [i32; 2],
    pub new_ratings: [i32; 2],
}

/// Messages from the server.
// Game snapshots make some variants large; messages are short-lived, so boxing
// them would only add noise.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Welcome {
        motd: String,
        /// The server runs on a local network (guests, no ratings).
        #[serde(default)]
        lan: bool,
    },
    AuthOk {
        player: PlayerInfo,
        token: String,
    },
    AuthError {
        reason: String,
    },
    /// Everyone in the lounge (sent after login).
    Lounge {
        players: Vec<PlayerInfo>,
    },
    /// A player came online or changed status/rating.
    PlayerUpdated {
        player: PlayerInfo,
    },
    PlayerLeft {
        username: String,
    },
    ChatHistory {
        lines: Vec<ChatLine>,
    },
    /// A lounge chat line was deleted: show that it was, by whom.
    ChatDeleted {
        id: u64,
        by: Deleted,
    },
    /// A lounge chat line was taken away without a trace.
    ChatRemoved {
        id: u64,
    },
    /// An administrator (`by`) cleared the lounge chat.
    ChatCleared {
        by: String,
    },
    Chat {
        line: ChatLine,
    },
    Invited {
        from: String,
        rated: bool,
        rules: Rules,
    },
    InviteSent {
        to: String,
        rated: bool,
        rules: Rules,
    },
    InviteCancelled {
        from: String,
    },
    InviteDeclined {
        by: String,
    },
    /// An invitation could not be sent or accepted.
    InviteFailed {
        reason: String,
    },
    /// All open tables (after login).
    Tables {
        tables: Vec<TableInfo>,
    },
    TableOpened {
        table: TableInfo,
    },
    TableClosed {
        id: u64,
    },
    /// A match began (or the player reconnected to one).
    MatchStarted {
        match_id: u64,
        seat: usize,
        players: [PlayerInfo; 2],
        rated: bool,
        game: EightBallGame,
        shot_clock_secs: u32,
        /// Seconds left on the current shot clock.
        clock_left: f32,
        /// Part of this tournament.
        #[serde(default)]
        tournament: Option<u64>,
        /// Sent to a spectator: `seat` is the friend being watched.
        #[serde(default)]
        spectating: bool,
        /// The cloth and ball set of the table (shop codes; empty: each
        /// player's own): the host's, or the higher rated player's.
        #[serde(default)]
        table_look: [String; 2],
    },
    OpponentAim {
        aim: AimState,
    },
    OpponentCueBall {
        position: DVec2,
    },
    /// A shot was accepted: animate it from the current table (with the cue
    /// ball at `cue_ball` if it was in hand). Sent before the server has
    /// simulated it, so both players see it straight away.
    ShotTaken {
        seat: usize,
        turn: u32,
        shot: Shot,
        called_pocket: Option<PocketId>,
        cue_ball: Option<DVec2>,
        /// Kept for older games: the server no longer plays a shot when the
        /// clock runs out (it is a foul, see `TimeFoul`), so it is false.
        #[serde(default)]
        timeout: bool,
    },
    /// The authoritative outcome of shot `turn`: adopt `end` once the
    /// animation is over.
    ShotResult {
        turn: u32,
        end: EightBallGame,
        report: ShotReport,
        /// Seconds left on the next shot clock (it starts after the animation).
        clock_left: f32,
    },
    /// The player deciding how play goes on after the break chose `option`
    /// (the official rules): adopt `game`.
    Chose {
        seat: usize,
        turn: u32,
        option: BreakOption,
        game: EightBallGame,
        /// Seconds left on the next shot clock.
        clock_left: f32,
    },
    /// A shot was refused: go back to this table.
    ShotRejected {
        turn: u32,
        reason: String,
        game: EightBallGame,
    },
    /// The shot clock expired for `seat`.
    TimeFoul {
        seat: usize,
        game: EightBallGame,
        clock_left: f32,
    },
    MatchChat {
        line: ChatLine,
    },
    /// A reaction at the table: `seat` 0 or 1 from a player,
    /// [`SPECTATOR_SEAT`] from someone watching.
    Reaction {
        seat: usize,
        code: String,
    },
    /// What can be bought for real money. `payments` is false when the
    /// server takes no payments (then coin packs and VIP are not offered).
    ShopInfo {
        payments: bool,
        packs: Vec<PackOffer>,
        /// The VIP price as shown to players (e.g. "R$ 14,90/mês").
        vip_price: String,
        vip_coins: u32,
        /// The price of a VIP month given to a friend (empty: not on sale).
        #[serde(default)]
        vip_gift_price: String,
        /// The price of the welcome pack (empty: not on sale).
        #[serde(default)]
        welcome_price: String,
        /// Coin packs sold in the Microsoft Store (only the Store build
        /// shows them; empty when the server doesn't take Store purchases).
        #[serde(default)]
        store_packs: Vec<StorePack>,
        /// The Store ID of the add-on that sells a month of VIP in the
        /// Microsoft Store ([`shop::VIP_MONTH`]; empty: not on sale there).
        #[serde(default)]
        store_vip: String,
    },
    /// This player's coins, VIP time, items and what they use (after
    /// signing in and whenever it changes).
    Wallet {
        coins: i64,
        vip_until: i64,
        owned: Vec<String>,
        /// (category, item) pairs.
        equipped: Vec<(String, String)>,
        /// Online games won with each cue (cue code, wins).
        #[serde(default)]
        cue_wins: Vec<(String, u32)>,
    },
    /// A payment page to open in the browser.
    PaymentPage {
        url: String,
    },
    /// What the Microsoft Store needs to make the player's Store ID key: a
    /// short-lived ticket from the server's Microsoft account, and the
    /// anonymous id the key will carry (it binds the key to this player).
    StoreTicket {
        ticket: String,
        user_id: String,
    },
    /// The Store purchases were checked: `coins` were credited now (0: none
    /// new). `pending`: a purchase is still waiting for Microsoft's answer
    /// (its coins arrive by themselves once it comes). `error`: why the
    /// check could not be made (empty when it was). `vip_until`: a month of
    /// VIP was among them, and VIP lasts until then (0: none).
    StoreRedeemed {
        coins: i64,
        #[serde(default)]
        pending: bool,
        #[serde(default)]
        error: String,
        #[serde(default)]
        vip_until: i64,
    },
    /// The player's place in the "Play now" queue (sent when it changes and
    /// every few seconds while searching). `range` is how far from their
    /// rating an opponent may be by now.
    Queue {
        searching: bool,
        rated: bool,
        rules: Rules,
        waited_secs: u32,
        range: u32,
    },
    /// How many people are watching this match.
    Spectators {
        count: u32,
    },
    /// How many spectators cheer for each seat (sent when it changes).
    Support {
        counts: [u32; 2],
    },
    /// The friend being watched went back to the lounge (or left).
    SpectateEnded {
        reason: String,
    },
    /// Every achievement this account has (after signing in and whenever
    /// one is unlocked): the game uses it to offer the reactions they grant.
    MyAchievements {
        codes: Vec<String>,
    },
    /// The opponent lost (or regained) their connection.
    OpponentConnection {
        connected: bool,
        grace_secs: u32,
    },
    MatchEnded {
        result: MatchResult,
    },
    RematchOffered,
    /// The opponent left after the game; the match room is closed.
    OpponentLeft,
    Leaderboard {
        rules: Rules,
        entries: Vec<LeaderboardEntry>,
    },
    Profile {
        profile: Box<Profile>,
    },
    Friends {
        friends: Vec<FriendInfo>,
        /// Usernames that asked to be friends.
        incoming: Vec<String>,
        /// Usernames this player asked.
        outgoing: Vec<String>,
        blocked: Vec<String>,
    },
    FriendRequestReceived {
        from: String,
    },
    AchievementUnlocked {
        code: String,
    },
    Tournaments {
        tournaments: Vec<TournamentInfo>,
    },
    TournamentUpdated {
        tournament: TournamentInfo,
    },
    TournamentRemoved {
        id: u64,
    },
    /// A message for the player (e.g. "your report was received").
    Notice {
        text: String,
    },
    /// Something the player did was rejected.
    Error {
        message: String,
    },
    /// The connection is about to be closed by the server (signed in
    /// elsewhere, banned, server restarting…). Don't reconnect automatically.
    Kicked {
        reason: String,
    },
    /// The player's age group and whether they may write to and read from
    /// other players (sent after signing in, and after [`ClientMsg::SetAge`]
    /// or [`ClientMsg::AllowChat`]). The chat with other players is off
    /// until the age group is given (18 or older) or, under 18, a parent or
    /// guardian turns it on.
    Age {
        /// `None`: not given yet (the chat is off; the game asks when the
        /// player first writes).
        minor: Option<bool>,
        /// Under 18, a parent or guardian turned the chat on.
        chat_allowed: bool,
    },
    /// The account page.
    Account {
        recovery_email: Option<String>,
        /// The e-mail given at the last payment, offered as the recovery
        /// e-mail when none is set.
        payment_email: Option<String>,
        /// Username changes the player has left (see [`MAX_NAME_CHANGES`]).
        #[serde(default)]
        name_changes_left: u32,
    },
    /// The account was deleted: forget the saved session.
    AccountDeleted,
    /// An item was bought (for this player, or as a gift for `gift_to`).
    ItemBought {
        code: String,
        #[serde(default)]
        gift_to: Option<String>,
    },
    /// A friend gave this player a month of VIP (`from` is the giver), or
    /// this player's gift reached `to`.
    VipGift {
        from: String,
        to: String,
    },
    /// Coins for playing (a finished game, the daily bonus, a prize).
    CoinsEarned {
        amount: u32,
        reason: String,
    },
    /// The notes on the lounge's board, newest first (after signing in and
    /// whenever they change).
    Board {
        notes: Vec<BoardNote>,
    },
    /// Today's and this week's missions and how far this player is (after
    /// signing in, when asked and whenever progress is made).
    Missions {
        daily: Vec<missions::MissionState>,
        weekly: Vec<missions::MissionState>,
        /// When today's and this week's missions end (Unix seconds).
        day_ends: i64,
        week_ends: i64,
    },
    Replays {
        games: Vec<HistoryEntry>,
        /// How many of the latest games are kept for this player (0: all).
        limit: u32,
    },
    Replay {
        replay: Box<ReplayInfo>,
    },
    /// The featured game in the lounge (sent when it changes).
    Featured {
        featured: Option<FeaturedMatch>,
    },
    InviteCode {
        code: String,
    },
    /// The server is restarting: no new games start; games in progress go on.
    Draining {
        /// Seconds until the restart.
        secs: u32,
    },
    /// This player's correspondence games (active ones and the latest
    /// finished) and challenges (sent after signing in, when asked and
    /// whenever they change). Never sent to games older than protocol 13.
    CorrList {
        games: Vec<correspondence::CorrGame>,
        challenges: Vec<correspondence::CorrChallenge>,
    },
    /// A correspondence game in full (the answer to `corr_open`): rebuild the
    /// table from `replay`; `seen` strokes were already shown to this player.
    CorrState {
        game: correspondence::CorrGame,
        replay: replay::Replay,
        seen: u32,
    },
    /// The authoritative outcome of this player's shot `turn` (like
    /// [`ServerMsg::ShotResult`]).
    CorrShot {
        id: i64,
        turn: u32,
        end: EightBallGame,
        report: ShotReport,
    },
    /// The answer to a break's choice in a correspondence game (like
    /// [`ServerMsg::Chose`]).
    CorrChose {
        id: i64,
        turn: u32,
        option: BreakOption,
        end: EightBallGame,
    },
    /// A correspondence shot was refused: back to this table.
    CorrShotRejected {
        id: i64,
        turn: u32,
        reason: String,
        game: EightBallGame,
    },
    /// Something happened in a correspondence game of this player.
    CorrNotice {
        id: i64,
        /// The other player.
        opponent: String,
        event: correspondence::CorrEvent,
    },
    /// The people this player played recently, latest first.
    RecentOpponents {
        players: Vec<RecentOpponent>,
    },
    /// Every note on the "looking for a game" board, newest first (after
    /// signing in).
    SeekBoard {
        notes: Vec<SeekNote>,
    },
    /// A note was put on the board (or replaced the player's earlier one).
    SeekPosted {
        note: SeekNote,
    },
    /// A player's note was taken off the board.
    SeekRemoved {
        username: String,
    },
    /// Presents this player has not seen yet, oldest first (after signing
    /// in, and when one arrives).
    NewGifts {
        gifts: Vec<GiftInfo>,
    },
    /// The presents this player received and gave, newest first.
    Gifts {
        gifts: Vec<GiftInfo>,
    },
    /// `by` thanked this player for a present (`code`).
    GiftThanked {
        by: String,
        code: String,
    },
    /// The answer to a claim of the ladder's reward.
    LadderClaim {
        result: ladder::ClaimResult,
    },
    /// The events running and coming (see [`events`]): sent when signing
    /// in and whenever the list changes. Empty: none.
    Events {
        events: Vec<events::EventInfo>,
    },
    /// The tour's final, dealt by the server (the answer to
    /// [`ClientMsg::LadderTicket`]): only a final played from this rack,
    /// with this breaker, earns the reward.
    LadderTicket {
        seed: u64,
        breaker: usize,
    },
    /// Today's challenge (the answer to [`ClientMsg::Daily`], and after an
    /// attempt).
    Daily {
        info: daily::Info,
    },
    /// An attempt started: the day's position comes from this seed (see
    /// [`daily::game`]).
    DailyStarted {
        day: i64,
        seed: u64,
    },
    /// The drills as this player sees them.
    Drills {
        info: drills::Info,
    },
    /// This account's medals of the tournaments against the computer, and
    /// the tournament whose medal was just kept (it can be forgotten).
    CupMedals {
        medals: cup::Medals,
        #[serde(default)]
        kept: Option<u64>,
    },
    /// A drill session counted (its score), or refused.
    DrillResult {
        code: String,
        result: Result<u32, drills::Refused>,
    },
    /// A drill session started (the answer to [`ClientMsg::DrillStart`]):
    /// the tries' cue balls lie where `session` puts them.
    DrillStarted {
        code: String,
        session: u64,
    },
    /// Experience earned (`gained`) and the total now (see [`xp`]).
    Xp {
        xp: i64,
        gained: u32,
    },
    /// An attempt was counted (with the coins for clearing the table, if
    /// any) or refused.
    DailyResult {
        day: i64,
        result: Result<daily::Outcome, daily::Refused>,
        #[serde(default)]
        coins: i64,
    },
    /// The answer to a request for a player's style.
    GhostStyle {
        username: String,
        answer: ghost::GhostAnswer,
    },
    Pong,
}

impl ClientMsg {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("client messages serialize")
    }
}

impl ServerMsg {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("server messages serialize")
    }
}

pub fn parse_client(text: &str) -> Result<ClientMsg, serde_json::Error> {
    serde_json::from_str(text)
}

pub fn parse_server(text: &str) -> Result<ServerMsg, serde_json::Error> {
    serde_json::from_str(text)
}

/// The LAN discovery beacon: a LAN server broadcasts this (as UTF-8) on
/// [`LAN_DISCOVERY_PORT`] about once a second.
pub const LAN_DISCOVERY_PORT: u16 = 47878;
/// Default port of a LAN server.
pub const LAN_GAME_PORT: u16 = 47879;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LanBeacon {
    /// Always "snook-legend".
    pub game: String,
    pub version: u32,
    /// Name of the host.
    pub host: String,
    pub port: u16,
    pub players: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Microsoft Store messages: a server from before them (no
    /// `store_packs`) is understood, and their JSON is what the server reads.
    #[test]
    fn store_messages_are_compatible() {
        let old =
            r#"{"type":"shop_info","payments":true,"packs":[],"vip_price":"","vip_coins":400}"#;
        match parse_server(old).unwrap() {
            ServerMsg::ShopInfo { store_packs, store_vip, .. } => {
                assert!(store_packs.is_empty());
                assert!(store_vip.is_empty());
            }
            other => panic!("{other:?}"),
        }
        let redeemed = parse_server(r#"{"type":"store_redeemed","coins":500}"#).unwrap();
        assert_eq!(
            redeemed,
            ServerMsg::StoreRedeemed {
                coins: 500,
                pending: false,
                error: String::new(),
                vip_until: 0
            }
        );
        // A game from before the VIP month reads the new messages too
        // (fields it doesn't know are skipped).
        #[derive(serde::Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum Before {
            ShopInfo { store_packs: Vec<StorePack> },
            StoreRedeemed { coins: i64 },
        }
        let info = ServerMsg::ShopInfo {
            payments: false,
            packs: Vec::new(),
            vip_price: String::new(),
            vip_coins: 400,
            vip_gift_price: String::new(),
            welcome_price: String::new(),
            store_packs: Vec::new(),
            store_vip: "9VVVVVVVVVVV".into(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(matches!(
            serde_json::from_str(&json).unwrap(),
            Before::ShopInfo { store_packs } if store_packs.is_empty()
        ));
        let vip = ServerMsg::StoreRedeemed {
            coins: 400,
            pending: false,
            error: String::new(),
            vip_until: 1_800_000_000,
        };
        let json = serde_json::to_string(&vip).unwrap();
        assert!(matches!(
            serde_json::from_str(&json).unwrap(),
            Before::StoreRedeemed { coins: 400 }
        ));
        assert_eq!(ClientMsg::StoreTicket.to_json(), r#"{"type":"store_ticket"}"#);
        let key = ClientMsg::StoreRedeem { key: "k".into() };
        assert_eq!(serde_json::from_str::<ClientMsg>(&key.to_json()).unwrap(), key);
    }

    #[test]
    fn names_and_passwords() {
        assert!(valid_username("Ana_99"));
        assert!(!valid_username("ab"));
        assert!(!valid_username("with space"));
        assert!(!valid_username("ação"));
        assert!(valid_password("secret12"));
        assert!(!valid_password("short1"));
        assert!(!valid_password("12345678"));
        assert!(!valid_password("aaaaaaaa"));
        // Characters, not bytes: four accented letters or three CJK
        // characters are too short; digits of any script are only digits.
        assert!(!valid_password("çãõé"));
        assert!(!valid_password("日本語"));
        assert!(!valid_password("１２３４５６７８"));
        assert!(valid_password("çãõéçãõé"));
        assert!(!valid_password(&"é".repeat(37)), "74 bytes");
    }

    #[test]
    fn text_is_cleaned() {
        assert_eq!(clean_chat("  hi\u{7}there \n"), "hithere");
        assert_eq!(clean_chat("a\u{202E}b\u{200B}c   d"), "abc d");
        assert_eq!(clean_chat(&"x".repeat(500)).chars().count(), MAX_CHAT_LEN);
        // Blank-looking fillers, tags and selectors leave nothing behind: a
        // line or a name made of them is empty, not invisible.
        assert_eq!(clean_chat("\u{3164}\u{3164}\u{115F}\u{2800}\u{FFA0}"), "");
        assert_eq!(clean_chat("o\u{E0041}\u{061C}k\u{FE0F}"), "ok");
        assert_eq!(clean_chat("good\ngame\r\n\tall"), "good game all");
        assert_eq!(clean_chat("\u{17B4}\u{17B5}\u{1BCA0}\u{1BCA3}"), "");
        assert_eq!(clean_chat("\u{206A}\u{206B}\u{206F}\u{180F}\u{1D159}\u{13430}"), "");
        assert_eq!(clean_text("Copa da Ana", MAX_NAME_LEN), "Copa da Ana");
    }

    #[test]
    fn emoji_survive_cleaning() {
        // Their selectors, joiners and tags stay…
        for t in [
            "eu \u{2764}\u{FE0F} sinuca",
            "\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}",
            "\u{1F937}\u{1F3FD}\u{200D}\u{2642}\u{FE0F}",
            "\u{2764}\u{FE0F}\u{200D}\u{1F525}",
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
            "1\u{FE0F}\u{20E3} #\u{FE0F}\u{20E3}",
            "\u{1F1E7}\u{1F1F7} \u{1F44D}\u{1F3FF} \u{1F3B1}",
            "\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}",
        ] {
            assert_eq!(clean_chat(t), t);
        }
        // …but nowhere else.
        assert_eq!(clean_chat("a\u{200D}b"), "ab");
        assert_eq!(clean_chat("adm\u{200D}in\u{FE0F}"), "admin");
        assert_eq!(clean_chat("\u{200D}\u{1F602}\u{200D}"), "\u{1F602}");
        assert_eq!(clean_chat("\u{1F602}\u{200D}a"), "\u{1F602}a");
        assert_eq!(clean_chat("1\u{FE0F}"), "1");
        assert_eq!(clean_chat("\u{FE0F}\u{200D}\u{FE0F}"), "");
        assert_eq!(clean_chat("\u{2764}\u{FE0F}\u{FE0F}"), "\u{2764}\u{FE0F}");
        assert_eq!(clean_chat("o\u{E0067}k\u{1F3F4}\u{E007F}\u{E0067}"), "ok\u{1F3F4}\u{E007F}");
        // The cut never splits an emoji: one that doesn't fit goes whole.
        let x = |n: usize| "x".repeat(n);
        let flag = "\u{1F1E7}\u{1F1F7}";
        assert_eq!(clean_chat(&format!("{}{flag}", x(198))), format!("{}{flag}", x(198)));
        assert_eq!(clean_chat(&format!("{}{flag}", x(199))), x(199));
        assert_eq!(clean_chat(&format!("{}{flag}{flag}", x(197))), format!("{}{flag}", x(197)));
        let fire = "\u{2764}\u{FE0F}\u{200D}\u{1F525}";
        assert_eq!(clean_chat(&format!("{}{fire}", x(196))), format!("{}{fire}", x(196)));
        assert_eq!(clean_chat(&format!("{}{fire}", x(197))), x(197));
        assert_eq!(clean_chat(&format!("{}\u{1F44D}\u{1F3FD}", x(199))), x(199));
        assert_eq!(clean_status(&"\u{1F602}".repeat(100)).chars().count(), MAX_STATUS_LEN);
        assert_eq!(clean_status(&flag.repeat(100)).chars().count(), MAX_STATUS_LEN);
        assert_eq!(
            clean_status(&format!("a{}", flag.repeat(100))).chars().count(),
            MAX_STATUS_LEN - 1
        );
    }

    /// An aim passed on to the opponent is one a cue can take.
    #[test]
    fn aims_are_tidied() {
        let wild = AimState {
            yaw: -7.0,
            elevation: 1e300,
            tip: glam::DVec2::new(1e300, 1e300),
            power: 9.0,
            called_pocket: None,
        };
        let a = wild.tidied();
        assert!((0.0..std::f64::consts::TAU).contains(&a.yaw));
        assert_eq!(a.elevation, std::f64::consts::FRAC_PI_2);
        assert!((a.tip.length() - 1.0).abs() < 1e-9);
        assert_eq!(a.power, 1.0);
        let calm = AimState {
            yaw: 1.0,
            elevation: 0.1,
            tip: glam::DVec2::new(0.2, -0.3),
            power: 0.5,
            called_pocket: None,
        };
        assert_eq!(calm.tidied(), calm);
    }

    #[test]
    fn countries_and_tiers() {
        assert!(valid_country("BR"));
        assert!(valid_country("br"));
        assert!(valid_country(""));
        assert!(!valid_country("XX"));
        assert!(COUNTRIES.len() > 200);
        assert_eq!(Tier::of(1200), Tier::Rookie);
        assert_eq!(Tier::of(1250), Tier::Amateur);
        assert_eq!(Tier::of(900), Tier::Rookie);
        assert_eq!(Tier::of(2500), Tier::Legend);
        assert_eq!(Tier::of(1500), Tier::Expert);
    }

    /// Cue levels (version 13) travel in fields older games ignore, and
    /// messages from older servers still parse.
    #[test]
    fn cue_levels_are_compatible() {
        let wallet = ServerMsg::Wallet {
            coins: 5,
            vip_until: 0,
            owned: vec!["table_club".into()],
            equipped: vec![("table".into(), "table_club".into())],
            cue_wins: vec![("cue_classic".into(), 57)],
        };
        assert_eq!(parse_server(&wallet.to_json()).unwrap(), wallet);
        // A version 12 game reads the wallet without the new field.
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        #[allow(dead_code)]
        enum Old {
            Wallet {
                coins: i64,
                vip_until: i64,
                owned: Vec<String>,
                equipped: Vec<(String, String)>,
            },
        }
        assert!(serde_json::from_str::<Old>(&wallet.to_json()).is_ok());
        // A player card without a cue level: level 0.
        let card = r#"{"username":"a","rules":"brazilian","standings":{"brazilian":{"rating":1200,"games":0,"wins":0},"official":{"rating":1200,"games":0,"wins":0}},"status":"available","cue":"cue_maple"}"#;
        let p: PlayerInfo = serde_json::from_str(card).unwrap();
        assert_eq!((p.cue.as_str(), p.cue_level), ("cue_maple", 0));
        let old = r#"{"type":"wallet","coins":1,"vip_until":0,"owned":[],"equipped":[]}"#;
        assert!(
            matches!(parse_server(old), Ok(ServerMsg::Wallet { cue_wins, .. }) if cue_wins.is_empty())
        );
    }

    #[test]
    fn messages_round_trip() {
        let game = EightBallGame::new(3, 0, snook_legend_core::Rules::Brazilian);
        let p = |name: &str| PlayerInfo {
            username: name.into(),
            rules: Rules::Official,
            standings: PerRules::new(
                Standing { rating: 1300, games: 4, wins: 3 },
                Standing::default(),
            ),
            status: PlayerStatus::Playing,
            country: "BR".into(),
            ..Default::default()
        };
        let msg = ServerMsg::MatchStarted {
            match_id: 7,
            seat: 1,
            players: [p("a"), p("b")],
            rated: true,
            game,
            shot_clock_secs: 60,
            clock_left: 60.0,
            tournament: None,
            spectating: false,
            table_look: Default::default(),
        };
        assert_eq!(parse_server(&msg.to_json()).unwrap(), msg);
        let c = ClientMsg::Shoot {
            turn: 3,
            shot: Shot::new(0.123456789, 3.3),
            called_pocket: Some(PocketId::FootLeft),
            cue_ball: None,
        };
        assert_eq!(parse_client(&c.to_json()).unwrap(), c);
        assert!(c.to_json().contains("\"type\":\"shoot\""));
    }

    #[test]
    fn status_and_seek_text_are_cleaned() {
        let long = "a".repeat(200);
        assert_eq!(clean_status(&long).chars().count(), MAX_STATUS_LEN);
        assert_eq!(clean_status("  treino\u{202E} \n à noite "), "treino à noite");
        assert_eq!(clean_seek(&long).chars().count(), MAX_SEEK_LEN);
        assert_eq!(clean_seek("\u{200B}"), "");
    }

    #[test]
    fn social_messages_round_trip() {
        let c = parse_client(r#"{"type":"post_seek","rated":true,"rules":"official"}"#).unwrap();
        assert_eq!(
            c,
            ClientMsg::PostSeek { text: String::new(), rated: true, rules: Rules::Official }
        );
        let c = parse_client(r#"{"type":"remove_seek"}"#).unwrap();
        assert_eq!(c, ClientMsg::RemoveSeek { username: String::new() });
        let note = SeekNote {
            username: "ana".into(),
            text: "treino".into(),
            rated: false,
            rules: Rules::Brazilian,
            at: 1,
            expires_at: 2,
        };
        let msg = ServerMsg::SeekPosted { note };
        assert_eq!(parse_server(&msg.to_json()).unwrap(), msg);
    }

    /// A player's record under each of the rules: the rating, tier and
    /// numbers shown are those of the rules asked for, or of the ones they
    /// play.
    #[test]
    fn a_player_has_a_standing_under_each_of_the_rules() {
        let mut p: PlayerInfo = serde_json::from_str(
            r#"{"username":"ana","rules":"official","status":"available",
                "standings":{"brazilian":{"rating":1450,"games":30,"wins":20},
                             "official":{"rating":1200,"games":0,"wins":0}}}"#,
        )
        .unwrap();
        assert_eq!((p.rating(), p.tier()), (1200, Tier::Rookie));
        assert_eq!(
            (p.rating_in(Rules::Brazilian), p.tier_in(Rules::Brazilian)),
            (1450, Tier::Skilled)
        );
        assert_eq!(p.standing(Rules::Brazilian).win_rate(), 66);
        p.rules = Rules::Brazilian;
        assert_eq!(p.rating(), 1450);
        p.guest = true;
        assert_eq!(p.tier(), Tier::Rookie);
        assert_eq!(Standing::default().rating, 1200);
    }
}
