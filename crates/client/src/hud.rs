//! In-game interface: scoreboard, shot panel, notices, help, game over and pause.

use macroquad::prelude::*;

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_circle_lines, draw_line, draw_rectangle, draw_triangle};

use crate::i18n::{t, tw};
use snook_legend_core::{BallInHand, Group, Rules};

use crate::game::{Controller, Match, Phase};
use crate::icons::Icon;
use crate::keys::{self, Action};
use crate::ui::{ALERT, BORDER, CREAM, FeedKind, FeedLine, GOLD, MUTED, Ui};
use snook_legend_proto::Relation;

pub struct Hud {
    /// Watching: the shot assessment on screen (kept while it fades out),
    /// how much it shows (0 to 1), how bright (dimmed while the aim moves)
    /// and the chance its bar shows (it slides to a new one).
    assess_shown: Option<snook_legend_core::assess::ShotAssessment>,
    assess_fade: f32,
    assess_bright: f32,
    assess_chance: f32,
    /// The game whose rating change is being counted up on the end panel,
    /// and since when.
    result_shown: Option<((u64, u32), f64)>,
    /// The game whose result panel the player put away (to see the table
    /// and use the chat): a slim bar at the top stands in for it.
    result_hidden: Option<(u64, u32)>,
    /// The pause menu is open (online, the game keeps running behind it).
    pub paused: bool,
    /// The balls made, in order, for the score panel.
    made: crate::scoreboard::Made,
    /// What the panel under the game-over panel shows: 0 numbers,
    /// 1 momentum, 2 the shot map.
    stats_view: usize,
    pub show_help: bool,
    chat_input: String,
    chat_open: bool,
    chat_scroll: usize,
    /// Set by the caller: the open chat's height as the player left it
    /// (0: the size that fits the window). Changed by its top edge.
    pub chat_height: f32,
    /// The chat's top edge is being dragged.
    chat_resizing: bool,
    /// The height the player dragged the chat to, for the caller to keep.
    pub chat_resized: Option<f32>,
    /// The pointer shows the up-down arrows (over the chat's top edge).
    resize_cursor: bool,
    /// The quick phrases are open (by the chat).
    quick_open: bool,
    confirm_leave: bool,
    /// Identifies the turn the banner was shown for.
    banner_key: Option<(u32, u32, usize)>,
    /// (title, subtitle, age in seconds).
    banner: Option<(String, String, f32)>,
    /// Set by the caller: the camera is in the overhead view.
    pub top_view: bool,
    /// The camera stays above the table (a setting): no 3D help lines.
    pub top_only: bool,
    /// Set by the caller: multiplier for fine aiming.
    pub aim_sensitivity: f32,
    /// Last pointer x while dragging the fine-aim strip.
    fine_last: Option<f32>,
    /// The power bar is being dragged.
    stroking: bool,
    /// The cue-setting panel that is open above the bottom bar.
    popover: Option<Pop>,
    /// The turn the power bar was last set to its starting place for.
    power_key: Option<(u32, u32, u32)>,
    panel_key: Option<(u32, u32, u32)>,
    /// Camera zoom, 0 (far) to 1 (close): set by the caller, changed by the zoom bar.
    pub zoom: f32,
    /// Set by the caller: the opponent can be reported (signed in online,
    /// not on a local network).
    pub can_report: bool,
    /// Players online under each of the rules (signed in, the lounge's
    /// list come): the rules' badge says how many play these.
    pub rule_players: Option<snook_legend_core::PerRules<usize>>,
    /// Reporting the opponent.
    report: Option<crate::report::ReportDialog>,
    /// The rules of the game, opened from the pause menu (or the badge).
    rules: Option<crate::whatsnew::WhatsNew>,
    /// The rules were opened from the badge in the top bar: closing them
    /// goes back to the game, not to the pause menu.
    rules_from_badge: bool,
    zoom_drag: bool,
    /// The reaction picker is open.
    reactions_open: bool,
    /// The reactions button the picker was opened from: a spectator sees
    /// two (by the chat and in the bottom bar), and only that one draws the
    /// picker and closes it.
    reactions_from: Option<Rect>,
    /// The chat's emoji tray.
    emoji: crate::emoji_tray::EmojiTray,
    /// How much the open chat lets the table show through (0 solid, 1
    /// glass): glass while nobody uses it.
    chat_glass: f32,
    /// Offline: the game history panel is open (it starts folded).
    history_open: bool,
    /// Where the history's icon in the top bar was drawn (pressing it is
    /// not a press outside the history).
    history_toggle: Rect,
    /// Set by the caller: the camera is already in its default place.
    pub camera_default: bool,
    /// Set by the caller, online: how the player relates to the opponent
    /// (None: no friends here, e.g. a local network). Offers "Add friend"
    /// after the game.
    pub opponent_relation: Option<Relation>,
    /// Set by the caller: the opponent's chat and reactions are muted
    /// ([chat, reactions]).
    pub opponent_muted: [bool; 2],
    /// The player muted or unmuted the opponent from the game: for the
    /// caller to keep (remembered for the next games against them).
    pub mute_request: Option<[bool; 2]>,
    /// The mute menu is open (by the chat).
    mute_open: bool,
    /// Watching: the chat shows the spectators' own (else the game's).
    spectator_tab: bool,
    /// The chat kept to the quick phrases: their list in the field's place.
    phrases_open: bool,
    /// Watching: the camera chosen and the shot's numbers (see `observer`).
    pub observer: crate::observer::Observer,
    /// Set by the caller: watching uses the broadcast camera by default.
    pub tv_default: bool,
    /// What happens at the table also in an online game's chat (Settings →
    /// Game); always in the history, from the top bar.
    pub events_in_chat: bool,
    /// The match's introduction is playing (the table's keys wait).
    intro_on: bool,
}

/// What the player asked for through the HUD.
pub enum HudAction {
    None,
    MainMenu,
    /// Online: back to the lounge.
    Lounge,
    Settings,
    /// Back to the overhead view of the whole table.
    ResetCamera,
    /// The zoom bar moved (0 far … 1 close).
    Zoom(f32),
    /// Online, after the game: ask the opponent (or accept their request)
    /// to be friends.
    AddFriend(String),
    /// The tour: on to the stage after this one (its number, from 0).
    NextStage(u8),
    /// The same drill once more.
    DrillAgain(&'static str),
    /// Another attempt against the clock.
    TimeAttackAgain,
}

impl Drop for Hud {
    /// Left with the pointer on the chat's edge: back to the arrow.
    fn drop(&mut self) {
        self.set_resize_cursor(false);
    }
}

impl Default for Hud {
    fn default() -> Self {
        Hud {
            result_shown: None,
            result_hidden: None,
            // Development aid (screenshots): SNOOK_LEGEND_PAUSED opens the
            // pause menu.
            paused: crate::dev::flag("SNOOK_LEGEND_PAUSED"),
            made: Default::default(),
            // Development aid: SNOOK_LEGEND_STATS_VIEW=1|2 opens the charts.
            stats_view: crate::dev::var("SNOOK_LEGEND_STATS_VIEW")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            show_help: false,
            // Development aid: SNOOK_LEGEND_CHAT_DRAFT="text" starts the field with it.
            chat_input: crate::dev::var("SNOOK_LEGEND_CHAT_DRAFT").unwrap_or_default(),
            // Development aid: SNOOK_LEGEND_CHAT_CLOSED starts with the chat folded.
            // Folded at the start (a dot and a preview announce new messages).
            // Development aid: SNOOK_LEGEND_CHAT_OPEN opens it.
            chat_open: crate::dev::flag("SNOOK_LEGEND_CHAT_OPEN"),
            chat_scroll: 0,
            chat_height: 0.0,
            chat_resizing: false,
            chat_resized: None,
            resize_cursor: false,
            quick_open: false,
            confirm_leave: false,
            banner_key: None,
            banner: None,
            top_view: false,
            top_only: false,
            aim_sensitivity: 1.0,
            fine_last: None,
            stroking: false,
            popover: None,
            power_key: None,
            panel_key: None,
            zoom: 0.5,
            zoom_drag: false,
            // Development aid: SNOOK_LEGEND_OPEN_REACTIONS opens the picker.
            reactions_open: crate::dev::flag("SNOOK_LEGEND_OPEN_REACTIONS"),
            reactions_from: None,
            // Development aid: SNOOK_LEGEND_EMOJI_OPEN opens the emoji tray.
            emoji: crate::emoji_tray::EmojiTray::new(crate::dev::flag("SNOOK_LEGEND_EMOJI_OPEN")),
            chat_glass: 0.0,
            assess_shown: None,
            assess_fade: 0.0,
            assess_bright: 1.0,
            assess_chance: 0.0,
            camera_default: true,
            opponent_relation: None,
            history_open: false,
            history_toggle: Rect::new(0.0, 0.0, 0.0, 0.0),
            can_report: false,
            rule_players: None,
            report: None,
            rules: None,
            rules_from_badge: false,
            opponent_muted: [false, false],
            mute_request: None,
            mute_open: false,
            spectator_tab: true,
            phrases_open: false,
            observer: Default::default(),
            tv_default: false,
            events_in_chat: false,
            intro_on: false,
        }
    }
}

impl Hud {
    /// The chat's top edge is being dragged (the caller leaves its height
    /// alone meanwhile).
    pub fn resizing_chat(&self) -> bool {
        self.chat_resizing
    }

    /// The pointer as up-down arrows over the chat's top edge.
    fn set_resize_cursor(&mut self, on: bool) {
        self.resize_cursor = on;
        if on {
            crate::ui::motion::want_cursor(macroquad::miniquad::CursorIcon::NSResize);
        }
    }

    /// A menu or a dialog is over the table: the table's keys (aim, spin,
    /// power, the replay) wait, not only its pointer.
    pub fn modal_open(&self) -> bool {
        self.paused || self.report.is_some() || self.rules.is_some() || self.intro_on
    }

    /// The controls are only listed from the menu: never on the table.
    pub fn new(_show_help: bool) -> Self {
        Hud::default()
    }

    /// The opponent's name, when this player can report them from here.
    fn reportable(&self, m: &Match) -> Option<String> {
        let o = m.online.as_ref()?;
        (self.can_report && !o.spectator).then(|| m.names[1 - o.seat].clone())
    }
}

const CHAT_FIELD: u64 = 40;
/// The match chat's top edge (dragged to resize it) and its scroll bar.
const CHAT_RESIZE: u64 = 9_200;
const CHAT_LOG: u64 = 9_201;
/// The game history's scroll bar (offline).
const HISTORY_LOG: u64 = 9_202;

/// The status line in the top bar, from `x`, at most `width` wide: what is
/// in brackets at its end ("… to shoot (stripes)") goes on a second, quieter
/// line; a long line without them wraps onto two.
/// The official rules in a game recorded with every shot called (until
/// 0.11.1): the button that declares a safety (no pocket called), at the
/// right end of the top bar on the player's turn (the status beside it
/// says which pocket is called). Its width, to keep the status clear of it
/// (0 when not shown). With only the 8 called there is none: a shot at the
/// 8 that calls nothing could only lose with it.
fn draw_call(ui: &mut Ui, m: &mut Match) -> f32 {
    if !m.game.calls_shots() || !m.game.calls() || !m.is_human_turn() || m.replay.is_some() {
        return 0.0;
    }
    let sw = crate::ui::vw();
    let label = t("Safety");
    let bw = ui.text_size(label, 13.0, true).x + 26.0;
    let b = Rect::new(sw - 14.0 - bw, 12.0, bw, 30.0);
    let declared = m.call_locked && m.called_pocket.is_none();
    let pressed = if declared { ui.primary_button(b, label) } else { ui.button(b, label, true) };
    if pressed {
        if declared {
            m.call_from_aim();
        } else {
            m.set_call(None);
        }
    }
    if ui.hovering(b) {
        let tip = if declared {
            t("A safety: nothing called. Click again to call the pocket you aim at.")
        } else {
            t(
                "A safety calls nothing: whatever goes in stays down, and the turn passes. To call another pocket, click it on the table.",
            )
        };
        ui.tooltip(tip, b);
    }
    bw + 12.0
}

fn draw_status(ui: &Ui, status: &str, x: f32, width: f32) {
    if width < 80.0 {
        return;
    }
    let (head, detail) = match status.strip_suffix(')').and_then(|s| s.rsplit_once(" (")) {
        Some((head, detail)) => (head, Some(detail)),
        None => (status, None),
    };
    let mut lines: Vec<(String, f32, Color, bool)> = Vec::new();
    let mut wrapped = |text: &str, size: f32, color: Color, bold: bool, most: usize| {
        let mut parts = ui.wrap(text, width, size);
        if parts.len() > most {
            // What does not fit ends in an ellipsis.
            let rest = parts.split_off(most - 1).join(" ");
            parts.push(ui.ellipsize(&rest, width, size));
        }
        lines.extend(parts.into_iter().map(|line| (line, size, color, bold)));
    };
    match detail {
        Some(detail) => {
            wrapped(head, 14.0, CREAM, false, 1);
            // "stripes" → "Stripes": a line of its own starts with a capital.
            let mut chars = detail.chars();
            let detail = match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            };
            wrapped(&detail, 12.0, MUTED, false, 2);
        }
        None => wrapped(head, 14.0, CREAM, false, 2),
    }
    let height: f32 = lines.iter().map(|l| l.1 + 4.0).sum::<f32>() - 4.0;
    let mut y = (TOP_BAR - height) / 2.0 - 1.0;
    for (line, size, color, bold) in &lines {
        ui.text(line, x, y, *size, *color, *bold);
        y += size + 4.0;
    }
}

/// A short status centred at `cx` from `y`: wrapped to two lines at most
/// in `width`, the second cut to fit (a long translation stays inside its
/// panel).
fn status_lines(ui: &Ui, text: &str, cx: f32, y: f32, width: f32, color: Color) {
    let mut lines = ui.wrap(text, width, 13.0);
    if lines.len() > 2 {
        let rest = lines[1..].join(" ");
        lines.truncate(1);
        lines.push(ui.ellipsize(&rest, width, 13.0));
    }
    // One line where it always was; two start a little higher.
    let top = if lines.len() > 1 { y - 7.0 } else { y };
    for (k, line) in lines.iter().enumerate() {
        ui.text_centered(line, cx, top + k as f32 * 16.0, 13.0, color, false);
    }
}

fn status_line(m: &Match) -> Option<String> {
    if let Some(waiting) = crate::correspondence::status(m) {
        return Some(waiting);
    }
    let name = &m.names[m.game.current_player()];
    match &m.phase {
        Phase::Turn { placing_cue_ball, computer } => {
            // The official rules: a choice the break left comes first.
            if m.game.pending_choice().is_some() {
                return Some(if m.current_controller() == Controller::Human {
                    t("Choose how play goes on").to_string()
                } else {
                    tw("{name} is choosing how play goes on…", &[("name", name)])
                });
            }
            if computer.is_some() {
                return Some(tw("{name} is thinking…", &[("name", name)]));
            }
            if m.current_controller() == Controller::Remote {
                return Some(if m.game.ball_in_hand().is_some() {
                    tw("{name} has ball in hand…", &[("name", name)])
                } else {
                    tw("{name} is aiming…", &[("name", name)])
                });
            }
            if *placing_cue_ball || m.game.ball_in_hand().is_some() {
                let zone = if m.game.ball_in_hand() == Some(BallInHand::Kitchen) {
                    t(" behind the line")
                } else {
                    ""
                };
                return Some(tw(
                    "Ball in hand: drag the cue ball{zone} to move it",
                    &[("zone", &zone)],
                ));
            }
            if m.game.is_break() {
                return Some(tw("{name} to break", &[("name", name)]));
            }
            if let Some(pocket) = m.game.called_pocket_of(m.game.current_player())
                && m.game.needs_called_pocket()
            {
                return Some(tw(
                    "{name} is on the 8: called pocket {pocket}",
                    &[("name", name), ("pocket", &crate::i18n::pocket(pocket))],
                ));
            }
            if m.game.needs_called_pocket() && m.called_pocket.is_none() {
                return Some(tw(
                    "{name} is on the 8: click a pocket to call it",
                    &[("name", name)],
                ));
            }
            // The official rules: each shot at the 8 calls its pocket, as
            // the Brazilian rules' first one does: clicked by the player,
            // then named.
            if m.game.calls_the_eight() {
                return Some(match m.called_pocket {
                    Some(pocket) => tw(
                        "{name} is on the 8: called pocket {pocket}",
                        &[("name", name), ("pocket", &crate::i18n::pocket(pocket))],
                    ),
                    None => tw("{name} is on the 8: click a pocket to call it", &[("name", name)]),
                });
            }
            // A drill: which try, and how many worked.
            if let Some(run) = &m.drill {
                let d = run.drill();
                return Some(tw(
                    "{drill}: try {k} of {n} · {made} made",
                    &[
                        ("drill", &t(d.name)),
                        ("k", &(run.results.len() + 1)),
                        ("n", &d.tries),
                        ("made", &run.score()),
                    ],
                ));
            }
            // Against the clock: what is left.
            if m.time_attack.is_some() {
                return Some(tw(
                    "Against the clock: {n} balls left",
                    &[("n", &m.game.balls_left())],
                ));
            }
            // The daily challenge: how far the visit is.
            if let Some(run) = &m.daily {
                return Some(tw(
                    "Daily challenge: {n} of {total} balls made",
                    &[("n", &run.made), ("total", &run.balls)],
                ));
            }
            let target = match (m.game.group_of(m.game.current_player()), m.game.rules()) {
                (Some(Group::Solids), _) => t("solids"),
                (Some(Group::Stripes), _) => t("stripes"),
                // The official rules with calls (a game recorded until
                // 0.11.1): the ball made in the called pocket.
                (None, Rules::Official) if m.game.calls() => {
                    t("open table: the called ball you make sets your group")
                }
                // Otherwise the first ball made decides.
                (None, _) => t("open table: the next ball you make sets your group"),
            };
            // The official rules: the pocket the player called (clicked),
            // after the group. Not the one the aim calls: it would say where
            // the aim sends the ball.
            let target = if m.game.calls_shots() && m.call_locked {
                let call = match m.called_pocket {
                    Some(p) => tw("called: {pocket}", &[("pocket", &crate::i18n::pocket(p))]),
                    None => t("no call: a safety").to_string(),
                };
                format!("{target} · {call}")
            } else {
                target.to_string()
            };
            Some(tw("{name} to shoot ({target})", &[("name", name), ("target", &target)]))
        }
        _ => None,
    }
}

impl Hud {
    pub fn draw(&mut self, ui: &mut Ui, m: &mut Match) -> HudAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let mut action = HudAction::None;

        let escape_used = std::mem::take(&mut m.escape_used);
        // The match's introduction plays over everything: its keys skip it.
        let intro_on = m.intro_on();
        self.intro_on = intro_on;
        if !ui.wants_keyboard && !intro_on {
            use crate::gamepad::{self as pad, Button};
            // A controller: Start opens and closes the menu, B steps back
            // out of what is open (at the table B is the game's).
            let open = self.paused || self.reactions_open || self.quick_open;
            let esc = is_key_pressed(KeyCode::Escape)
                || pad::pressed(Button::Start)
                || (open && pad::pressed(Button::East));
            if self.report.is_some() || self.rules.is_some() {
                // The dialog has Esc.
            } else if esc && self.reactions_open {
                self.reactions_open = false;
            } else if esc && self.quick_open {
                self.quick_open = false;
            } else if esc && self.paused && self.show_help {
                // The list of controls closes back to the menu (as its Back).
                self.show_help = false;
            } else if esc && !escape_used {
                self.paused = !self.paused;
                self.confirm_leave = false;
            }
            if (keys::pressed(Action::Help) || pad::pressed(Button::Back))
                && self.report.is_none()
                && self.rules.is_none()
            {
                // The list of controls, in the menu.
                self.show_help = !(self.paused && self.show_help);
                self.paused = self.show_help;
            }
            if !self.paused {
                self.show_help = false;
            }
        }

        // The pause menu and the list of controls are modal: nothing behind
        // them reacts to the pointer (they are drawn last, with input back).
        // Nor the keyboard: the keys of what is under them (the break's
        // choice, taking a shot back, the replay's arrows) wait.
        let modal = (self.paused || self.report.is_some()).then(|| {
            let keyboard = std::mem::replace(&mut ui.wants_keyboard, true);
            (ui.suspend_input(), keyboard)
        });
        // Nothing under the introduction takes the pointer either.
        let intro_input = intro_on.then(|| ui.suspend_input());

        if !ui.wants_keyboard && !self.modal_open() && keys::pressed(Action::Replay) {
            if m.replay.is_some() {
                m.stop_replay();
            } else {
                m.start_replay();
            }
        }
        // Top bar: menu and camera on the left, the players in the middle.
        // All of it is the interface's: a click on it (between its buttons,
        // on the names) never aims the cue under it.
        draw_bar(Rect::new(0.0, 0.0, sw, TOP_BAR), true);
        ui.block(Rect::new(0.0, 0.0, sw, TOP_BAR));
        // The score panel centred, but never under the buttons on the left.
        // (Three icons offline and in daily games: the history's too.)
        let icons = if !m.is_online() || crate::correspondence::is_corr(m) { 3.0 } else { 2.0 };
        // The rules of the game, in a badge after the buttons (not in a
        // drill or the daily challenge, which set their own tables).
        let badge = (m.drill.is_none() && m.daily.is_none()).then(|| {
            let rules = m.game.rules();
            let online = self.rule_players.map(|p| *p.get(rules));
            (rules, online, crate::rulesui::badge_w(ui, 30.0, rules, online))
        });
        let badge_w = badge.map_or(0.0, |(_, _, w)| w + 12.0);
        let buttons_w =
            ui.text_size(t("Menu"), 13.0, true).x + 22.0 + icons * 36.0 + 30.0 + badge_w;
        let board =
            crate::scoreboard::Board::layout(sw, buttons_w, 4.0, crate::correspondence::is_corr(m));
        let now = get_time();
        self.made.update(m, now);
        crate::scoreboard::draw(ui, m, &self.made, &board, now);
        // The shot clock, where it reads at a glance; the last seconds of
        // this player's turn over the table.
        crate::scoreboard::clock_tag(ui, m, &board);
        crate::scoreboard::clock_warning(ui, m, &board);
        // Doubles: the four players, and who holds the cue.
        crate::online::doubles::draw_lineup(ui, m, TOP_BAR);
        let (x0, total) = (board.all.x, board.all.w);
        // Free practice: take the last shot back (button, or Z).
        if m.free_play && m.last_shot.is_some() && matches!(m.phase, Phase::Turn { .. }) {
            let label = t("Take back the shot (Z)");
            let bw = ui.text_size(label, 13.0, true).x + 24.0;
            let b = Rect::new(12.0, TOP_BAR + 8.0, bw, 30.0);
            let key = !ui.wants_keyboard && is_key_pressed(KeyCode::Z);
            if (ui.button(b, label, true) || key) && m.take_back() {
                m.notify(t("Shot taken back."), false);
            }
        }
        // What the computer opponent says: a bubble under its card.
        if let Some((text, age)) = m.banter.clone() {
            let a = board.avatars[1];
            let anchor = vec2(a.x + a.w / 2.0, TOP_BAR - 4.0);
            ui.overlay(move |ui| draw_banter(ui, &text, age, anchor));
        }
        // How many are watching: an eye and the number, by the cards.
        let eye = vec2(x0 + total + 22.0, TOP_BAR / 2.0);
        if let Some(o) = &m.online
            && (o.spectators > 0 || o.spectator)
        {
            let n = o.spectators.to_string();
            let tip =
                Rect::new(eye.x - 14.0, eye.y - 14.0, 28.0 + ui.text_size(&n, 13.0, true).x, 28.0);
            crate::icons::draw(Icon::Eye, eye, 20.0, MUTED);
            ui.text(&n, eye.x + 14.0, eye.y - 8.0, 13.0, MUTED, true);
            if ui.hovering(tip) {
                ui.tooltip(&tw("{n} watching", &[("n", &n)]), tip);
            }
        }
        self.draw_support(ui, m, &board);
        draw_reactions(m, [x0 + 34.0, x0 + total - 34.0], eye);
        // Online: rated or casual, always in view (under the top bar, left).
        if m.is_online() && m.replay.is_none() {
            crate::table_social::rated_badge(ui, m, 12.0, TOP_BAR + 8.0);
        }
        let mut y = TOP_BAR + 8.0;
        // Whose turn it is and what they are on: in the top bar, beside the
        // score panel. Nothing lies over the table (a line across the top
        // rail hid the side pocket from above).
        // The official rules: the safety button at the end of the bar.
        let call_w = if m.is_spectating() { 0.0 } else { draw_call(ui, m) };
        if let Some(status) = status_line(m).filter(|_| m.replay.is_none()) {
            let watched = m.online.as_ref().is_some_and(|o| o.spectators > 0 || o.spectator);
            let left = x0 + total + if watched { 72.0 } else { 18.0 };
            draw_status(ui, &status, left, sw - 14.0 - call_w - left);
        }
        // Only important messages float on the table; everything is in the log.
        for n in m.notices.iter().filter(|n| n.important) {
            let alpha = n.ttl.min(1.0);
            let base = if n.cheer { GOLD } else { ALERT };
            let color = Color::new(base.r, base.g, base.b, alpha);
            // Never wider than the window (smaller type, then cut short).
            let (text, size) = ui.fitted(&n.text, 15.0, sw - 44.0, n.cheer);
            let Some(detail) = &n.detail else {
                let w = ui.text_size(&text, size, n.cheer).x + 20.0;
                crate::ui::rrect(
                    Rect::new((sw - w) / 2.0, y, w, 22.0),
                    11.0,
                    Color::new(0.0, 0.0, 0.0, 0.45 * alpha),
                );
                ui.text_centered(&text, sw / 2.0, y + 3.0, size, color, n.cheer);
                y += 25.0;
                continue;
            };
            // What it means under the rules, under it: a card of its own.
            let max = (sw - 44.0).min(720.0);
            let mut lines = ui.wrap(detail, max - 28.0, 13.0);
            lines.truncate(2);
            let tw_ = ui.text_size(&text, size, true).x;
            let w = lines
                .iter()
                .map(|l| ui.text_size(l, 13.0, false).x)
                .fold(tw_, f32::max)
                .min(max - 28.0)
                + 28.0;
            let h = 10.0 + 20.0 + lines.len() as f32 * 17.0 + 8.0;
            let card = Rect::new((sw - w) / 2.0, y, w, h);
            crate::ui::rrect(card, 9.0, Color::new(0.06, 0.04, 0.03, 0.82 * alpha));
            crate::ui::rrect_lines(card, 9.0, 1.0, Color::new(base.r, base.g, base.b, 0.5 * alpha));
            ui.text_centered(&text, sw / 2.0, y + 8.0, size, color, true);
            for (k, line) in lines.iter().enumerate() {
                let c = Color::new(CREAM.r, CREAM.g, CREAM.b, alpha);
                ui.text_centered(line, sw / 2.0, y + 30.0 + k as f32 * 17.0, 13.0, c, false);
            }
            y += h + 4.0;
        }
        // The official rules: the choice a break left.
        if m.replay.is_none() && !m.is_spectating() {
            self.draw_choice(ui, m, y + 6.0);
        }

        self.draw_turn_banner(ui, m);
        // The opponent lost their connection: the time they have left.
        crate::table_social::reconnection(ui, m);

        // Cue panel (spin, elevation, fine aim) and the power stroke bar:
        // everything can be played with the mouse alone.
        if m.is_spectating() {
            if self.draw_spectator_bar(ui, m) {
                action = HudAction::Lounge;
            }
            self.draw_shot_assessment(ui, m);
            // The pro observer's numbers: the last shot, the balls left.
            self.observer.draw_panel(ui, m, TOP_BAR + 8.0);
        } else {
            self.draw_bottom_bar(ui, m);
            // A correspondence game: its replay, or the turn sent.
            let bar = Rect::new(0.0, sh - BOTTOM_BAR, sw, BOTTOM_BAR);
            if let crate::correspondence::BarAction::Back =
                crate::correspondence::draw_bar(ui, m, bar)
            {
                action = HudAction::Lounge;
            }
        }
        // A recorded game's controls, in the bottom bar (over it: drawn after).
        draw_video_bar(ui, m);

        // Menu, the overhead view and the replay, in the top bar.
        if !self.paused {
            let label = if m.is_spectating() { t("Leave") } else { t("Menu") };
            let w = ui.text_size(t("Menu"), 13.0, true).x + 22.0;
            if ui.button(Rect::new(10.0, 12.0, w, 30.0), label, true) {
                // A spectator has nothing to pause: the menu just leaves.
                if m.is_spectating() {
                    action = HudAction::Lounge;
                } else {
                    self.paused = true;
                }
            }
            let mut x = 10.0 + w + 8.0;
            let tip = keys::tip(t("Reset the camera: top view, no zoom"), Action::ResetCamera);
            if ui.icon_button(
                Rect::new(x, 12.0, 30.0, 30.0),
                Icon::CameraReset,
                &tip,
                !self.camera_default,
            ) {
                action = HudAction::ResetCamera;
            }
            x += 36.0;
            let can_replay = m.last_shot.is_some() && !m.shot_in_progress();
            let tip = if m.replay.is_some() {
                keys::tip(t("Stop the replay"), Action::Replay)
            } else {
                keys::tip(t("Watch the last shot again"), Action::Replay)
            };
            if ui.icon_button(
                Rect::new(x, 12.0, 30.0, 30.0),
                Icon::Replay,
                &tip,
                can_replay || m.replay.is_some(),
            ) {
                if m.replay.is_some() {
                    m.stop_replay();
                } else {
                    m.start_replay();
                }
            }
            // The game's history, out of the way up here (online it takes
            // the chat's place while it is open).
            x += 36.0;
            let tip = if self.history_open { t("Hide the history") } else { t("Game history") };
            let r = Rect::new(x, 12.0, 30.0, 30.0);
            self.history_toggle = r;
            if ui.icon_button_ex(r, Icon::List, tip, true, self.history_open) {
                self.history_open = !self.history_open;
                self.chat_scroll = 0;
            }
            if let Some((rules, online, _)) = badge {
                x += 36.0 + 6.0;
                let r = crate::rulesui::badge(ui, x, 12.0, 30.0, rules, online);
                if ui.hovering(r) {
                    let mut tip = crate::i18n::rules_name(rules).to_string();
                    if let Some(n) = online {
                        tip = format!("{tip} · {}", tw("{n} online", &[("n", &n)]));
                    }
                    let tip = format!("{tip}. {}", t("The rules of this game: click to read them"));
                    ui.tooltip(&tip, r);
                    if ui.clicked(r) {
                        self.rules = Some(crate::whatsnew::WhatsNew::rules(rules));
                        self.rules_from_badge = true;
                        self.paused = true;
                    }
                }
            }
            if let Some(z) = self.zoom_bar(ui) {
                action = HudAction::Zoom(z);
            }
        }
        if m.replay.is_some() {
            let label = t("REPLAY");
            let w = ui.text_size(label, 14.0, true).x + 40.0;
            let r = Rect::new((sw - w) / 2.0, TOP_BAR + 8.0, w, 26.0);
            let pulse = 0.6 + 0.4 * (get_time() as f32 * 3.0).sin();
            crate::ui::rrect(r, 13.0, Color::new(0.0, 0.0, 0.0, 0.7));
            draw_circle(r.x + 14.0, r.y + 13.0, 4.5, Color::new(0.95, 0.3, 0.25, pulse));
            ui.text(label, r.x + 26.0, r.y + 6.0, 14.0, CREAM, true);
        }
        // The elevation and spin panels open in the same corner: while one
        // is open, the chat (or history) steps aside.
        if self.popover.is_none() {
            // Spectators have a chat of their own (the players never see
            // it); an administrator watching also gets the players' (only
            // they are sent it), read only.
            // A correspondence game has no one to chat with: its history.
            let corr = crate::correspondence::is_corr(m);
            if m.is_online() && !corr && !self.history_open {
                self.draw_chat(ui, m);
            } else {
                self.draw_history(ui, m);
            }
        }

        // A drill session is over.
        if let (Phase::GameOver, Some(run)) = (&m.phase, &m.drill) {
            let d = run.drill();
            let w = 440.0f32.min(sw - 20.0);
            let r = Rect::new((sw - w) / 2.0, sh / 2.0 - 120.0, w, 240.0);
            ui.panel(r);
            let score = match &run.result {
                Some(Ok(n)) => *n,
                _ => run.score(),
            };
            let stars = snook_legend_proto::drills::stars(d, score);
            crate::ui::rrect_lines(r, 8.0, 2.0, if stars == 3 { GOLD } else { BORDER });
            ui.text_centered(t(d.name), sw / 2.0, r.y + 16.0, 26.0, GOLD, true);
            // The stars: gold for each earned.
            for k in 0..3 {
                let c = vec2(sw / 2.0 + (k as f32 - 1.0) * 46.0, r.y + 78.0);
                if (k as u32) < stars {
                    crate::icons::draw_filled(crate::icons::Icon::Star, c, 34.0, GOLD);
                } else {
                    let faint = Color::new(1.0, 1.0, 1.0, 0.22);
                    crate::icons::draw(crate::icons::Icon::Star, c, 34.0, faint);
                }
            }
            let line = tw("{n} of {total}", &[("n", &score), ("total", &d.tries)]);
            ui.text_centered(&line, sw / 2.0, r.y + 108.0, 20.0, CREAM, true);
            let (status, color) = match &run.result {
                None if run.sent => (t("Sending to the server…").to_string(), MUTED),
                // Started signed in, over while signed out: sent once back.
                None if run.session != 0 => {
                    (t("Will be sent when you're back online.").to_string(), MUTED)
                }
                None => (t("Not kept: sessions count when started signed in.").to_string(), MUTED),
                Some(Ok(_)) => (t("Kept in your account.").to_string(), CREAM),
                Some(Err(_)) => (t("The session could not be counted.").to_string(), ALERT),
            };
            // Two lines at most between the score and the buttons.
            status_lines(ui, &status, sw / 2.0, r.y + 140.0, w - 40.0, color);
            let bw = ((w - 60.0) / 2.0).min(200.0);
            let back = Rect::new(sw / 2.0 - bw - 6.0, r.y + 176.0, bw, 40.0);
            let again = Rect::new(sw / 2.0 + 6.0, r.y + 176.0, bw, 40.0);
            if ui.button(back, t("Back to the drills"), true) {
                action = HudAction::MainMenu;
            }
            if ui.primary_button(again, t("Try again")) {
                action = HudAction::DrillAgain(run.code);
            }
        }

        // Against the clock: the running clock, and the time once clear.
        crate::time_attack::draw_clock(ui, m);
        if let Some(a) = crate::time_attack::draw_end(ui, m) {
            action = a;
        }

        // The daily challenge's visit is over.
        if let (Phase::GameOver, Some(run)) = (&m.phase, &m.daily) {
            let w = 440.0f32.min(sw - 20.0);
            let r = Rect::new((sw - w) / 2.0, sh / 2.0 - 110.0, w, 220.0);
            ui.panel(r);
            let cleared = run.made >= run.balls;
            crate::ui::rrect_lines(r, 8.0, 2.0, if cleared { GOLD } else { BORDER });
            let title = if cleared { t("Table cleared!") } else { t("Daily challenge") };
            ui.text_centered(title, sw / 2.0, r.y + 16.0, 28.0, GOLD, true);
            let made = tw("{n} of {total} balls", &[("n", &run.made), ("total", &run.balls)]);
            ui.text_centered(&made, sw / 2.0, r.y + 60.0, 22.0, CREAM, true);
            let (line, color) = match &run.result {
                None if run.sent => (t("Sending your attempt to the server…").to_string(), MUTED),
                // Over while signed out: sent once back.
                None => (t("Will be sent when you're back online.").to_string(), MUTED),
                Some((Ok(o), coins)) if *coins > 0 => (
                    tw("Counted: {n} balls · +{coins} coins", &[("n", &o.balls), ("coins", coins)]),
                    GOLD,
                ),
                Some((Ok(o), _)) => (tw("Counted: {n} balls", &[("n", &o.balls)]), CREAM),
                Some((Err(_), _)) => (t("The attempt could not be counted.").to_string(), ALERT),
            };
            let spin = run.result.is_none() && run.sent;
            let room = w - 40.0 - if spin { 40.0 } else { 0.0 };
            let (line, size) = ui.fitted(&line, 14.0, room, false);
            if spin {
                crate::ui::spinner(
                    vec2(sw / 2.0 - ui.text_size(&line, size, false).x / 2.0 - 16.0, r.y + 108.0),
                    7.0,
                    GOLD,
                );
            }
            ui.text_centered(&line, sw / 2.0, r.y + 100.0, size, color, false);
            let b = Rect::new(sw / 2.0 - 110.0, r.y + 160.0, 220.0, 40.0);
            if ui.primary_button(b, t("See today's table")) {
                action = HudAction::MainMenu;
            }
        }

        // Game over.
        // (While the last shot is watched again, the panel steps aside.)
        let mut replay_now = false;
        let result_key = (m.match_id(), m.games_played);
        if let (Phase::GameOver, Some(over), true, None) =
            (&m.phase, m.game.game_over(), m.is_online(), &m.replay)
            // Development aid (screenshots): SNOOK_LEGEND_RESULT_HIDDEN puts it away.
            && (self.result_hidden == Some(result_key) || crate::dev::flag("SNOOK_LEGEND_RESULT_HIDDEN"))
        {
            // Put away: a bar at the top, with what the panel's buttons do.
            if let Some(a) = self.result_bar(ui, m, over) {
                action = a;
            }
        } else if let (Phase::GameOver, Some(over), true, None) =
            (&m.phase, m.game.game_over(), m.is_online(), &m.replay)
        {
            let o = m.online.as_ref().expect("online");
            let me = o.seat;
            let won = over.winner == me && !o.spectator;
            let title = if won {
                // (The opponent left, or didn't come back in time.)
                if m.won_by_abandonment() { t("Victory by abandonment") } else { t("You win!") }
                    .to_string()
            } else {
                tw("{name} wins", &[("name", &m.names[over.winner])])
            };
            // "Add friend" by Rematch and Back to lounge while the opponent
            // isn't one yet: three buttons, as wide as the longest label.
            let friend = match self.opponent_relation.filter(|_| !o.spectator) {
                Some(Relation::None) => Some((t("Add friend"), true)),
                Some(Relation::RequestReceived) => Some((t("Accept friend"), true)),
                Some(Relation::RequestSent) => Some((t("Request sent"), false)),
                _ => None,
            };
            let label_w = |l: &str| ui.text_size(l, 17.0, true).x + 32.0;
            let bw = match friend {
                Some((f, _)) => [f, t("Rematch"), t("Accept rematch"), t("Back to lounge")]
                    .iter()
                    .map(|l| label_w(l))
                    .fold(150.0, f32::max),
                None => 180.0,
            };
            let row_w = if friend.is_some() { bw * 3.0 + 20.0 } else { 370.0 };
            // Room on both sides of the centred title for the Report button.
            let report_w = if self.reportable(m).is_some() {
                2.0 * (ui.text_size(t("Report"), 12.0, true).x + 30.0)
            } else {
                0.0
            };
            let w = crate::ui::clamp_soft(
                (ui.text_size(&title, 28.0, true).x + 64.0 + report_w).max(row_w + 40.0),
                440.0,
                sw - 20.0,
            );
            let bw = bw.min((w - 60.0) / if friend.is_some() { 3.0 } else { 2.0 });
            let r = Rect::new((sw - w) / 2.0, sh / 2.0 - 110.0 - STATS_H / 2.0, w, 220.0);
            ui.panel(r);
            replay_now |= replay_button(ui, m, r);
            // Put away, to see the table and use the chat.
            let hide = Rect::new(r.x + 10.0, r.y + 10.0, 30.0, 30.0);
            let tip = t("Hide the result to see the table and the chat");
            if ui.icon_button_ex(hide, Icon::Minus, tip, true, true) {
                self.result_hidden = Some(result_key);
            }
            let view = &mut self.stats_view;
            let stats_w = 440.0f32.min(w);
            let stats = Rect::new((sw - stats_w) / 2.0, r.y + r.h + 8.0, stats_w, STATS_H - 8.0);
            draw_match_stats(ui, m, stats, view);
            crate::ui::rrect_lines(r, 8.0, 2.0, if won { GOLD } else { BORDER });
            ui.text_centered(&title, sw / 2.0, r.y + 16.0, 28.0, GOLD, true);
            if let Some(name) = self.reportable(m) {
                let label = t("Report");
                let bw = ui.text_size(label, 12.0, true).x + 20.0;
                let br = Rect::new(r.x + r.w - bw - 10.0, r.y + 10.0, bw, 24.0);
                if ui.button(br, label, true) {
                    self.report = Some(crate::report::ReportDialog::at_table(&name));
                }
                if ui.hovering(br) {
                    ui.tooltip(&tw("Report {name}", &[("name", &name)]), br);
                }
            }
            let how = crate::correspondence::result_line(m, over).unwrap_or_else(|| {
                crate::i18n::result_line(
                    over.reason,
                    &m.names[over.winner],
                    &m.names[1 - over.winner],
                )
            });
            ui.text_centered(&how, sw / 2.0, r.y + 56.0, 15.0, CREAM, false);
            let score = format!("{}  {} – {}  {}", m.names[0], m.wins[0], m.wins[1], m.names[1]);
            ui.text_centered(&score, sw / 2.0, r.y + 82.0, 15.0, CREAM, true);
            match &o.result {
                Some(res) if res.rated => {
                    // The rating counts from where it was to where it is,
                    // quickly at first and settling (once per game).
                    let key = (m.match_id(), m.games_played);
                    let now = get_time();
                    let since = match self.result_shown {
                        Some((k, at)) if k == key => at,
                        _ => {
                            self.result_shown = Some((key, now));
                            now
                        }
                    };
                    let k = ((now - since) / 1.1).clamp(0.0, 1.0) as f32;
                    let eased = 1.0 - (1.0 - k).powi(3);
                    let full = res.rating_delta[me];
                    let d = (full as f32 * eased).round() as i32;
                    let shown = res.new_ratings[me] - full + d;
                    let delta = format!("{}{}", if d >= 0 { "+" } else { "" }, d);
                    let text = tw("Rating {r} ({d})", &[("r", &shown), ("d", &delta)]);
                    let d = full;
                    let color = if d >= 0 { Color::new(0.45, 1.0, 0.55, 1.0) } else { ALERT };
                    ui.text_centered(&text, sw / 2.0, r.y + 108.0, 18.0, color, true);
                }
                Some(_) => ui.text_centered(
                    t("Casual game: ratings unchanged"),
                    sw / 2.0,
                    r.y + 110.0,
                    14.0,
                    MUTED,
                    false,
                ),
                None => ui.text_centered(
                    t("Saving the result…"),
                    sw / 2.0,
                    r.y + 110.0,
                    14.0,
                    MUTED,
                    false,
                ),
            }
            let note = if o.spectator {
                String::new()
            } else if o.tournament.is_some() {
                t("Tournament game: the next round starts from the lounge.").to_string()
            } else if o.opponent_left {
                t("Your opponent went back to the lounge.").to_string()
            } else if o.rematch_requested {
                tw("Waiting for {name}…", &[("name", &m.names[1 - me])])
            } else if o.rematch_offered {
                tw("{name} wants a rematch!", &[("name", &m.names[1 - me])])
            } else {
                String::new()
            };
            ui.text_centered(&note, sw / 2.0, r.y + 138.0, 14.0, GOLD, false);
            if o.spectator {
                ui.text_centered(
                    t("If they play again, you keep watching."),
                    sw / 2.0,
                    r.y + 138.0,
                    14.0,
                    MUTED,
                    false,
                );
                if ui.button(
                    Rect::new(sw / 2.0 - 100.0, r.y + 164.0, 200.0, 38.0),
                    t("Stop watching"),
                    true,
                ) {
                    action = HudAction::Lounge;
                }
            }
            if crate::correspondence::is_corr(m) {
                // A daily game: no rematch here, the list has the rest.
                let b = Rect::new(sw / 2.0 - 120.0, r.y + 164.0, 240.0, 38.0);
                if ui.primary_button(b, t("Back to daily games")) {
                    action = HudAction::Lounge;
                }
            } else if !o.spectator {
                let can_rematch =
                    !o.opponent_left && !o.rematch_requested && o.tournament.is_none();
                let label = if o.rematch_offered { t("Accept rematch") } else { t("Rematch") };
                let n = if friend.is_some() { 3.0 } else { 2.0 };
                let x0 = sw / 2.0 - (bw * n + 10.0 * (n - 1.0)) / 2.0;
                let at = |k: f32| Rect::new(x0 + k * (bw + 10.0), r.y + 164.0, bw, 38.0);
                let rr = at(0.0);
                if can_rematch {
                    if ui.primary_button(rr, label) {
                        m.rematch();
                    }
                } else {
                    ui.button(rr, t("Rematch"), false);
                }
                if let Some((label, enabled)) = friend
                    && ui.button(at(1.0), label, enabled)
                {
                    action = HudAction::AddFriend(m.names[1 - me].clone());
                }
                if ui.button(at(n - 1.0), t("Back to lounge"), true) {
                    action = HudAction::Lounge;
                }
            }
        } else if let (Phase::GameOver, Some(over), None) =
            (&m.phase, m.game.game_over(), &m.replay)
            && m.drill.is_none()
            && m.daily.is_none()
        {
            let all_computers = m.controllers.iter().all(|c| matches!(c, Controller::Computer(_)));
            let title = tw("{name} wins!", &[("name", &m.names[over.winner])]);
            // Wide enough for the title with a margin (long computer names).
            let w =
                crate::ui::clamp_soft(ui.text_size(&title, 28.0, true).x + 100.0, 420.0, sw - 20.0);
            let r = Rect::new((sw - w) / 2.0, sh / 2.0 - 95.0 - STATS_H / 2.0, w, 190.0);
            ui.panel(r);
            replay_now |= replay_button(ui, m, r);
            let view = &mut self.stats_view;
            let stats_w = 420.0f32.min(w);
            let stats = Rect::new((sw - stats_w) / 2.0, r.y + r.h + 8.0, stats_w, STATS_H - 8.0);
            draw_match_stats(ui, m, stats, view);
            // The player beat the computer: the title in gold with a trophy
            // and a glow of gold along the top; lost to it: plainer, no
            // trophy (two players at one computer: always the winner's gold).
            let human = |seat: usize| matches!(m.controllers[seat], Controller::Human);
            let lost = !human(over.winner) && human(1 - over.winner);
            if !lost {
                crate::ui::rrect_gradient(
                    Rect::new(r.x + 1.0, r.y + 1.0, r.w - 2.0, 56.0),
                    crate::ui::PANEL_RADIUS - 1.0,
                    Color::new(GOLD.r, GOLD.g, GOLD.b, 0.12),
                    Color::new(GOLD.r, GOLD.g, GOLD.b, 0.0),
                );
                let tw_ = ui.text_size(&title, 28.0, true).x;
                crate::icons::draw(
                    Icon::Trophy,
                    vec2(sw / 2.0 - tw_ / 2.0 - 22.0, r.y + 33.0),
                    24.0,
                    GOLD,
                );
            }
            let color = if lost { CREAM } else { GOLD };
            ui.text_centered(
                &title,
                sw / 2.0 + if lost { 0.0 } else { 14.0 },
                r.y + 18.0,
                28.0,
                color,
                true,
            );
            ui.text_centered(
                &crate::i18n::result_line(
                    over.reason,
                    &m.names[over.winner],
                    &m.names[1 - over.winner],
                ),
                sw / 2.0,
                r.y + 60.0,
                15.0,
                CREAM,
                false,
            );
            let score = format!("{}  {} – {}  {}", m.names[0], m.wins[0], m.wins[1], m.names[1]);
            ui.text_centered(&score, sw / 2.0, r.y + 88.0, 16.0, CREAM, true);
            // (A stored game being watched ends there.)
            if all_computers && !m.is_replay() {
                ui.text_centered(
                    t("Next game starts in a moment…"),
                    sw / 2.0,
                    r.y + 130.0,
                    14.0,
                    MUTED,
                    false,
                );
            } else if m.cup {
                // A tournament's game: the result stands, on with it.
                let b = Rect::new(sw / 2.0 - 130.0, r.y + 128.0, 260.0, 38.0);
                if ui.primary_button(b, t("Back to the tournament")) {
                    action = HudAction::MainMenu;
                }
            } else if m.is_replay() {
                // A stored game watched to its end: from the start again (one
                // that can be gone through), or back (no rematch: it was
                // someone's game, already played).
                let bx = sw / 2.0 - 180.0;
                let again = Rect::new(bx, r.y + 128.0, 170.0, 38.0);
                if m.video.is_some() && ui.primary_button(again, t("Watch again")) {
                    m.seek(0);
                }
                if ui.button(Rect::new(bx + 190.0, r.y + 128.0, 170.0, 38.0), t("Main menu"), true)
                {
                    action = HudAction::MainMenu;
                }
            } else {
                let bx = sw / 2.0 - 180.0;
                // On the tour: after a win, straight on to the next
                // opponent; after a loss, the same one again.
                let tour = m.ladder.as_ref().map(|run| run.rung);
                let won = over.winner == snook_legend_proto::ladder::PLAYER_SEAT;
                let next = tour.filter(|_| won).and_then(|k| {
                    snook_legend_proto::ladder::rung(k as usize + 1).map(|p| (k + 1, p))
                });
                let b = Rect::new(bx, r.y + 128.0, 170.0, 38.0);
                if let Some((k, p)) = next {
                    let label = tw("Next: {name}", &[("name", &p.name)]);
                    let (label, _) = ui.fitted(&label, 17.0, b.w - 16.0, true);
                    if ui.primary_button(b, &label) {
                        action = HudAction::NextStage(k);
                    }
                } else if ui.primary_button(
                    b,
                    if tour.is_some() && !won { t("Try again") } else { t("Rematch") },
                ) {
                    // The final again: dealt anew by the server (the
                    // game's loop asks it).
                    let last = snook_legend_proto::ladder::RUNGS.len() as u8 - 1;
                    match tour {
                        Some(k) if k == last => action = HudAction::NextStage(k),
                        _ => m.rematch(),
                    }
                }
                if ui.button(Rect::new(bx + 190.0, r.y + 128.0, 170.0, 38.0), t("Main menu"), true)
                {
                    action = HudAction::MainMenu;
                }
            }
        }

        if replay_now {
            m.start_replay();
        }
        // The two players face each other as the match starts (skippable).
        if let Some(saved) = intro_input {
            ui.resume_input(saved);
            let slot = m.intro_mut();
            if slot.as_mut().is_some_and(|intro| !intro.draw(ui, get_frame_time().min(0.1))) {
                *slot = None;
            }
        }
        if let Some((saved, keyboard)) = modal {
            ui.resume_input(saved);
            ui.wants_keyboard = keyboard;
        }
        // The report dialog, over everything (its input is its own).
        if let Some(dialog) = self.report.as_mut() {
            match dialog.draw(ui) {
                crate::report::ReportOutcome::Open => {}
                crate::report::ReportOutcome::Cancelled => self.report = None,
                crate::report::ReportOutcome::Send(msg) => {
                    if let Some(o) = m.online.as_mut() {
                        o.outbox.push(msg);
                    }
                    self.report = None;
                }
            }
        }
        // The rules, from the pause menu (back to it when closed) or the
        // rules' badge (back to the game).
        if let Some(rules) = self.rules.as_mut()
            && (!self.paused || rules.draw(ui))
        {
            self.rules = None;
            if std::mem::take(&mut self.rules_from_badge) {
                self.paused = false;
            }
        }
        if self.show_help && self.paused {
            let _modal = ui.modal_scope(crate::ui::motion::id("match-help"), 0.5);
            // A controller in use: its buttons; else the keys and the mouse.
            let pad = crate::gamepad::connected() && crate::gamepad::active();
            let lines =
                if pad { pad_controls_list(self.top_only) } else { controls_list(self.top_only) };
            const ROW: f32 = 26.0;
            const PAD: f32 = 24.0;
            let label_w =
                lines.iter().map(|(a, _)| ui.text_size(a, 14.0, true).x).fold(0.0, f32::max);
            let value_w =
                lines.iter().map(|(_, b)| ui.text_size(b, 14.0, false).x).fold(0.0, f32::max);
            let w = crate::ui::clamp_soft(label_w + value_w + 2.0 * PAD + 40.0, 320.0, sw - 20.0);
            let h = PAD + 30.0 + 12.0 + lines.len() as f32 * ROW + 12.0 + 20.0 + 16.0 + 38.0 + PAD;
            let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
            ui.panel(r);
            ui.text_centered(t("Controls"), sw / 2.0, r.y + PAD - 2.0, 22.0, GOLD, true);
            let top = r.y + PAD + 30.0;
            draw_line(r.x + PAD, top, r.x + r.w - PAD, top, 1.0, BORDER);
            for (k, (a, b)) in lines.iter().enumerate() {
                let yy = top + 12.0 + k as f32 * ROW;
                if k % 2 == 0 {
                    crate::ui::rrect(
                        Rect::new(r.x + PAD - 8.0, yy, r.w - 2.0 * PAD + 16.0, ROW),
                        5.0,
                        Color::new(1.0, 0.9, 0.7, 0.035),
                    );
                }
                ui.text(a, r.x + PAD, yy + 5.0, 14.0, CREAM, true);
                let x = r.x + PAD + label_w + 32.0;
                let room = r.x + r.w - PAD - x;
                ui.text(&ui.ellipsize(b, room, 14.0), x, yy + 5.0, 14.0, MUTED, false);
            }
            let note = if pad {
                t("The keyboard and the mouse work as well.")
            } else {
                t("Keys can be changed in Settings → Controls.")
            };
            let note_y = top + 12.0 + lines.len() as f32 * ROW + 12.0;
            let note = ui.ellipsize(note, r.w - 2.0 * PAD, 12.0);
            ui.text_centered(&note, sw / 2.0, note_y, 12.0, MUTED, false);
            let back = Rect::new(sw / 2.0 - 80.0, r.y + r.h - PAD - 38.0, 160.0, 38.0);
            if ui.button(back, t("Back"), true) || ui.dismissed(r) {
                self.show_help = false;
            }
        }

        // Pause menu (online the match keeps running behind it).
        if self.paused && !self.show_help && self.rules.is_none() {
            let online = m.is_online();
            // A correspondence game waits: leaving loses nothing.
            let corr = crate::correspondence::is_corr(m);
            let _modal = ui.modal_scope(crate::ui::motion::id("match-pause"), 0.5);
            const PAD: f32 = 28.0;
            const GAP: f32 = 10.0;
            const BH: f32 = 38.0;
            let w = 340.0;
            let header = if online && !corr {
                86.0
            } else if online {
                66.0
            } else {
                48.0
            };
            // Resume, settings, controls, resign, then the way out set apart.
            let leave_h = if self.confirm_leave { 24.0 + BH } else { BH };
            let reportable = self.reportable(m);
            let rows = if reportable.is_some() { 5.0 } else { 4.0 };
            let h = PAD + header + rows * (BH + GAP) + 10.0 + leave_h + PAD;
            let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
            ui.panel(r);
            ui.text_centered(
                if online { t("Menu") } else { t("Paused") },
                sw / 2.0,
                r.y + PAD - 4.0,
                24.0,
                GOLD,
                true,
            );
            if online {
                let note = if corr {
                    t("Your game is saved: come back whenever you like.")
                } else {
                    t("The game goes on while this is open.")
                };
                ui.text_centered(note, sw / 2.0, r.y + PAD + 30.0, 13.0, MUTED, false);
                if !corr {
                    // Closing the game mid-match is leaving it (resigning isn't).
                    let warn = t(
                        "Closing the game now is leaving it: extra rating lost and a wait to play again.",
                    );
                    let (warn, size) = ui.fitted(warn, 12.0, w - 24.0, false);
                    ui.text_centered(&warn, sw / 2.0, r.y + PAD + 50.0, size, ALERT, false);
                }
            }
            let bx = r.x + PAD;
            let bw = r.w - 2.0 * PAD;
            let mut y = r.y + PAD + header;
            if ui.primary_button(Rect::new(bx, y, bw, BH), t("Resume")) {
                self.paused = false;
                self.confirm_leave = false;
            }
            y += BH + GAP;
            if ui.button(Rect::new(bx, y, bw, BH), t("Settings"), true) {
                action = HudAction::Settings;
            }
            y += BH + GAP;
            // The controls and the rules, side by side.
            let half = (bw - GAP) / 2.0;
            if ui.button(Rect::new(bx, y, half, BH), t("Controls"), true) {
                self.show_help = true;
            }
            if ui.button(Rect::new(bx + half + GAP, y, half, BH), t("Rules"), true) {
                self.rules = Some(crate::whatsnew::WhatsNew::rules(m.game.rules()));
            }
            y += BH + GAP;
            let my_turn = m.current_controller() == Controller::Human;
            // Not in a drill or the daily challenge: they end with their
            // own panel (and are left from the menu).
            let can_resign = m.game.game_over().is_none()
                && (online || my_turn)
                && m.drill.is_none()
                && m.daily.is_none();
            if ui.button(Rect::new(bx, y, bw, BH), t("Resign this game"), can_resign) {
                m.resign();
                self.paused = false;
            }
            y += BH + GAP;
            if let Some(name) = reportable {
                let label = tw("Report {name}", &[("name", &name)]);
                let (label, _) = ui.fitted(&label, 15.0, bw - 24.0, true);
                if ui.button(Rect::new(bx, y, bw, BH), &label, true) {
                    self.report = Some(crate::report::ReportDialog::at_table(&name));
                    self.paused = false;
                }
                y += BH + GAP;
            }
            draw_line(
                bx,
                y + 4.0,
                bx + bw,
                y + 4.0,
                1.0,
                Color::new(BORDER.r, BORDER.g, BORDER.b, 0.6),
            );
            y += 10.0;
            let leave = if corr {
                t("Back to daily games")
            } else if online {
                t("Leave to the lounge")
            } else {
                t("Main menu")
            };
            if self.confirm_leave {
                ui.text_centered(
                    t("Leaving now counts as a loss."),
                    sw / 2.0,
                    y,
                    13.0,
                    ALERT,
                    false,
                );
                y += 24.0;
                let half = (bw - GAP) / 2.0;
                if ui.button(Rect::new(bx, y, half, BH), t("Leave"), true) {
                    self.paused = false;
                    action = if online { HudAction::Lounge } else { HudAction::MainMenu };
                }
                if ui.button(Rect::new(bx + half + GAP, y, half, BH), t("Stay"), true) {
                    self.confirm_leave = false;
                }
            } else if ui.button(Rect::new(bx, y, bw, BH), leave, true) {
                if online && !corr && m.game.game_over().is_none() {
                    self.confirm_leave = true;
                } else {
                    self.paused = false;
                    action = if online { HudAction::Lounge } else { HudAction::MainMenu };
                }
            }
            // A press on the dimmed game around it: back to the game.
            if self.paused && ui.dismissed(r) {
                self.paused = false;
                self.confirm_leave = false;
            }
        }
        action
    }
}

/// The chat (online) or the game history (offline): smaller on small
/// windows, so it never hides much of the table.
fn chat_w() -> f32 {
    (crate::ui::vw() * 0.3).clamp(240.0, 360.0)
}

fn chat_h() -> f32 {
    ((crate::ui::vh() - TOP_BAR - BOTTOM_BAR) * 0.42).clamp(140.0, 250.0)
}

/// The shortest the chat is dragged to.
const CHAT_MIN_H: f32 = 140.0;

/// The tallest the chat is dragged to: up to the top bar.
fn chat_max_h() -> f32 {
    crate::ui::vh() - TOP_BAR - BOTTOM_BAR - 16.0
}

/// The highest a list opened from a button goes: under the top bar.
const POPOVER_TOP: f32 = TOP_BAR + 4.0;

/// Where a list `w` × `h` opens from button `b`: above it when it fits
/// (under the top bar), else below it, else on the side with more room.
/// Not over the button while there is room on a side (and the click that
/// opens a list is never taken as a click on it).
fn popover_rect(b: Rect, w: f32, h: f32) -> Rect {
    let (vw, vh) = (crate::ui::vw(), crate::ui::vh());
    let above = b.y - 6.0 - POPOVER_TOP;
    let below = vh - 8.0 - (b.y + b.h + 6.0);
    let y = if h <= above || (h > below && above >= below) {
        crate::ui::clamp_soft(b.y - h - 6.0, POPOVER_TOP, vh)
    } else {
        b.y + b.h + 6.0
    };
    let x = crate::ui::clamp_soft(b.x, 8.0, vw - w - 8.0);
    Rect::new(x, y, w, h)
}

/// The room a list opened from button `b` has, above or below it (see
/// [`popover_rect`]).
fn popover_room(b: Rect) -> f32 {
    let above = b.y - 6.0 - POPOVER_TOP;
    let below = crate::ui::vh() - 8.0 - (b.y + b.h + 6.0);
    above.max(below)
}

impl Hud {
    /// A slim vertical zoom control on the right edge: almost invisible
    /// until the pointer comes near.
    fn zoom_bar(&mut self, ui: &mut Ui) -> Option<f32> {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let h = (sh * 0.28).clamp(120.0, 220.0);
        let track = Rect::new(sw - 26.0, (sh - h) / 2.0, 6.0, h);
        let zone = Rect::new(track.x - 16.0, track.y - 34.0, 40.0, h + 68.0);
        let near = ui.hovering(zone) || self.zoom_drag;
        let alpha = if near { 0.95 } else { 0.35 };
        let mut out = None;
        // + and − buttons.
        let plus = Rect::new(track.x - 9.0, track.y - 30.0, 24.0, 24.0);
        let minus = Rect::new(track.x - 9.0, track.y + h + 6.0, 24.0, 24.0);
        if near {
            if ui.icon_button_ex(plus, Icon::Plus, t("Zoom in"), true, true) {
                out = Some((self.zoom + 0.12).min(1.0));
            }
            if ui.icon_button_ex(minus, Icon::Minus, t("Zoom out"), true, true) {
                out = Some((self.zoom - 0.12).max(0.0));
            }
        } else {
            crate::icons::draw(
                Icon::Plus,
                vec2(plus.x + 12.0, plus.y + 12.0),
                10.0,
                Color::new(1.0, 1.0, 1.0, 0.3),
            );
            crate::icons::draw(
                Icon::Minus,
                vec2(minus.x + 12.0, minus.y + 12.0),
                10.0,
                Color::new(1.0, 1.0, 1.0, 0.3),
            );
        }
        let drag = ui.drag_area(95, Rect::new(track.x - 10.0, track.y, 26.0, h));
        self.zoom_drag = matches!(drag, crate::ui::Drag::Held(_));
        if let crate::ui::Drag::Held(p) | crate::ui::Drag::Released(p) = drag {
            out = Some((1.0 - (p.y - track.y) / h).clamp(0.0, 1.0));
        }
        crate::ui::rrect(track, 3.0, Color::new(0.0, 0.0, 0.0, 0.45 * alpha));
        let fill = h * self.zoom;
        crate::ui::rrect(
            Rect::new(track.x, track.y + h - fill, track.w, fill),
            3.0,
            Color::new(GOLD.r, GOLD.g, GOLD.b, 0.55 * alpha),
        );
        let ky = track.y + h - fill;
        draw_circle(
            track.x + 3.0,
            ky,
            if near { 8.0 } else { 5.5 },
            Color::new(0.93, 0.9, 0.84, alpha),
        );
        if let Some(z) = out {
            self.zoom = z;
        }
        out
    }

    /// The chat with the opponent (online matches), and the reactions.
    fn draw_chat(&mut self, ui: &mut Ui, m: &mut Match) {
        let sh = crate::ui::vh();
        let dt = get_frame_time();
        // The game is over: opening the chat puts the result away (the bar
        // at the top brings it back).
        let over = matches!(m.phase, Phase::GameOver).then(|| (m.match_id(), m.games_played));
        let Some(o) = m.online.as_mut() else { return };
        if !self.chat_open {
            self.set_resize_cursor(false);
            self.chat_resizing = false;
            // A bubble button as small as the tools beside it; what's new
            // shows as a number on its corner.
            // The bubble and the word: the chat is used most, so it is the
            // widest of the three.
            let label = t("Chat");
            let label_w = ui.text_size(label, 13.0, true).x;
            let button = Rect::new(12.0, sh - BOTTOM_BAR - 40.0, 40.0 + label_w + 12.0, 32.0);
            ui.block(button);
            let hover = ui.hovering(button);
            // (Watching: the spectators' lines count too.)
            let count = o.unread + if o.spectator { o.spectator_unread } else { 0 };
            let unread = count > 0;
            crate::ui::rrect(
                button,
                7.0,
                if hover {
                    Color::new(0.38, 0.25, 0.12, 0.95)
                } else {
                    Color::new(0.07, 0.045, 0.03, 0.85)
                },
            );
            if hover || unread {
                crate::ui::rrect_lines(button, 7.0, 1.2, if hover { GOLD } else { BORDER });
            }
            let ink = if unread || hover { GOLD } else { CREAM };
            crate::online::chat_glyph(
                vec2(button.x + 20.0, button.y + button.h / 2.0 - 1.0),
                17.0,
                ink,
            );
            ui.text(
                label,
                button.x + 36.0,
                crate::ranking::text_y(button.y + button.h / 2.0, 13.0),
                13.0,
                ink,
                true,
            );
            if ui.activated(button, hover) {
                self.chat_open = true;
                if over.is_some() {
                    self.result_hidden = over;
                }
            }
            if unread {
                // The number of new lines, on the corner, with a soft glow
                // that breathes while they wait.
                let label = if count > 9 { "9+".to_string() } else { count.to_string() };
                let lw = ui.text_size(&label, 10.0, true).x;
                let bw = (lw + 9.0).max(16.0);
                let badge = Rect::new(button.x + button.w - bw * 0.6, button.y - 6.0, bw, 16.0);
                let pulse = 0.5 + 0.5 * (get_time() as f32 * 4.0).sin();
                crate::ui::rrect(
                    Rect::new(badge.x - 2.0, badge.y - 2.0, badge.w + 4.0, badge.h + 4.0),
                    10.0,
                    Color::new(GOLD.r, GOLD.g, GOLD.b, 0.18 + 0.17 * pulse),
                );
                crate::ui::rrect(badge, 8.0, GOLD);
                ui.text_centered(
                    &label,
                    badge.x + badge.w / 2.0,
                    badge.y + 3.0,
                    10.0,
                    Color::new(0.12, 0.08, 0.04, 1.0),
                    true,
                );
            }
            // The newest message, for a few seconds, above the button: it
            // stays while the pointer is on it, and a click opens the chat.
            let mut open_chat = false;
            if let Some((line, age)) = &mut o.preview {
                let who = line.from.as_ref().map(|(n, _)| format!("{n}: ")).unwrap_or_default();
                let max_w = 300.0;
                let who_w = ui.text_size(&who, 13.0, true).x;
                let text = if who_w + ui.text_size(&line.text, 13.0, false).x > max_w {
                    let room = max_w - who_w;
                    format!("{}…", ui.start_that_fits(&line.text, 13.0, room, false))
                } else {
                    line.text.clone()
                };
                let w = who_w + ui.text_size(&text, 13.0, false).x + 20.0;
                let r = Rect::new(button.x, button.y - 34.0, w, 26.0);
                let held = ui.hovering(r);
                if held {
                    // Read at leisure: it waits, fully shown.
                    *age = age.clamp(0.2, PREVIEW_TIME - 0.6);
                } else {
                    *age += dt;
                }
                let alpha = (*age / 0.2).min(1.0) * ((PREVIEW_TIME - *age) / 0.6).clamp(0.0, 1.0);
                if alpha > 0.05 {
                    ui.block(r);
                }
                let c = line.from.as_ref().map_or(GOLD, |(n, _)| crate::ui::author_color(n));
                // A bubble in the chat's colours (the author's own).
                crate::ui::rrect(r, 13.0, Color::new(0.06, 0.045, 0.035, 0.9 * alpha));
                crate::ui::rrect_lines(
                    r,
                    13.0,
                    1.0,
                    if held {
                        Color::new(GOLD.r, GOLD.g, GOLD.b, alpha)
                    } else {
                        Color::new(BORDER.r, BORDER.g, BORDER.b, 0.8 * alpha)
                    },
                );
                ui.text(&who, r.x + 10.0, r.y + 6.0, 13.0, Color::new(c.r, c.g, c.b, alpha), true);
                ui.text(
                    &text,
                    r.x + 10.0 + who_w,
                    r.y + 6.0,
                    13.0,
                    Color::new(CREAM.r, CREAM.g, CREAM.b, alpha),
                    false,
                );
                open_chat = held && ui.released_in(r);
                if *age > PREVIEW_TIME {
                    o.preview = None;
                }
            }
            if open_chat {
                self.chat_open = true;
                if over.is_some() {
                    self.result_hidden = over;
                }
            }
            let rb = Rect::new(button.x + button.w + 6.0, button.y, 32.0, 32.0);
            self.reaction_button(ui, m, rb, true);
            let qb = Rect::new(rb.x + rb.w + 6.0, button.y, 32.0, 32.0);
            self.quick_phrases(ui, m, qb);
            return;
        }
        o.unread = 0;
        o.preview = None;
        // A list open by the chat (quick phrases, reactions) closes first:
        // the press that closes it leaves the chat open.
        let list_open = self.quick_open || self.reactions_open || self.emoji.open;
        // As tall as the player made it (its top edge is dragged), or the
        // size that fits the window.
        let bottom = sh - BOTTOM_BAR - 8.0;
        let height = |h: f32| {
            if h > 0.0 { crate::ui::clamp_soft(h, CHAT_MIN_H, chat_max_h()) } else { chat_h() }
        };
        let mut r =
            Rect::new(12.0, bottom - height(self.chat_height), chat_w(), height(self.chat_height));
        // The top edge, left of the header's tools: dragged up for a taller
        // chat (more of the talk in view), down for a shorter one. Kept for
        // the next games.
        let grip = Rect::new(r.x + 6.0, r.y - 5.0, r.w - 6.0 - 3.0 * 28.0 - 8.0, 10.0);
        let drag = ui.drag_area(CHAT_RESIZE, grip);
        match drag {
            crate::ui::Drag::Held(p) => {
                self.chat_resizing = true;
                self.chat_height = height((bottom - p.y).round());
                r = Rect::new(r.x, bottom - self.chat_height, r.w, self.chat_height);
            }
            crate::ui::Drag::Released(_) if self.chat_resizing => {
                self.chat_resizing = false;
                self.chat_resized = Some(self.chat_height);
            }
            _ => self.chat_resizing = false,
        }
        let on_grip = self.chat_resizing || matches!(drag, crate::ui::Drag::Hover);
        self.set_resize_cursor(on_grip);
        // Glass while nobody uses it (the table shows through), solid under
        // the pointer, while typing or with one of its lists open.
        let busy = ui.hovering(r) || ui.focus == Some(CHAT_FIELD) || list_open;
        let target = if busy { 0.0 } else { 1.0 };
        self.chat_glass += (target - self.chat_glass) * (1.0 - (-dt * 10.0).exp());
        ui.glass_panel(r, self.chat_glass);
        // The grip: a short bar in the middle of the top edge.
        let grip_c = if on_grip { GOLD } else { Color::new(BORDER.r, BORDER.g, BORDER.b, 0.9) };
        crate::ui::rrect(Rect::new(r.x + r.w / 2.0 - 16.0, r.y + 3.0, 32.0, 3.0), 1.5, grip_c);
        if matches!(drag, crate::ui::Drag::Hover) && !self.chat_resizing {
            ui.tooltip(t("Drag to make the chat taller or shorter"), grip);
        }
        let watching = o.spectator;
        // Watching: the spectators' own chat, and the game's (with the
        // players' chat for an administrator), on two tabs.
        let spectators = watching && self.spectator_tab;
        if spectators {
            o.spectator_unread = 0;
        }
        let players_chat = o.feed.iter().any(|f| f.kind == FeedKind::Player);
        let head_mid = r.y + 15.0;
        if watching {
            let game = if players_chat { t("Players (administrator)") } else { t("Game") };
            let labels = [t("Spectators"), game];
            let tabs = Rect::new(r.x + 6.0, head_mid - 12.0, (r.w - 80.0).min(300.0), 24.0);
            if let Some(k) = ui.segmented(tabs, &labels, if spectators { 0 } else { 1 }) {
                self.spectator_tab = k == 0;
            }
        } else {
            // Muted: said in the title (nothing new comes).
            let title = if self.opponent_muted[0] { t("Chat (muted)") } else { t("Chat") };
            let (title, title_size) = ui.fitted(title, 13.0, r.w - 150.0, true);
            ui.text(
                &title,
                r.x + 10.0,
                crate::ranking::text_y(head_mid, title_size),
                title_size,
                if self.opponent_muted[0] { MUTED } else { GOLD },
                true,
            );
        }
        // The header's tools, right to left: hide, reactions, quick
        // phrases, muting the opponent.
        let hide = Rect::new(r.x + r.w - 28.0, head_mid - 11.0, 22.0, 22.0);
        if ui.icon_button_ex(hide, Icon::Minus, t("Hide"), true, true) {
            self.chat_open = false;
        }
        let reactions = Rect::new(hide.x - 28.0, head_mid - 12.0, 24.0, 24.0);
        let quick = Rect::new(reactions.x - 28.0, head_mid - 12.0, 24.0, 24.0);
        let mute = Rect::new(quick.x - 28.0, head_mid - 12.0, 24.0, 24.0);
        if !watching {
            let name = m.names[1 - o.seat.min(1)].clone();
            let muted = self.opponent_muted;
            if let Some(next) =
                crate::table_social::mute_menu(ui, mute, &name, muted, &mut self.mute_open)
            {
                self.mute_request = Some(next);
                self.opponent_muted = next;
            }
        }
        let Some(o) = m.online.as_mut() else { return };
        crate::paint::draw_line(
            r.x + 8.0,
            r.y + 30.0,
            r.x + r.w - 8.0,
            r.y + 30.0,
            1.0,
            Color::new(BORDER.r, BORDER.g, BORDER.b, 0.5),
        );
        // The field grows with what is typed (up to four lines): the log
        // above it gives the room.
        // The field, and the emoji button at its right.
        let field_w = r.w - 12.0 - crate::emoji_tray::BUTTON - 4.0;
        // The chat kept to the quick phrases: their picker, one line high.
        let phrases = o.phrases_only && !watching;
        let field_h =
            if phrases { 34.0 } else { ui.text_area_height(&self.chat_input, field_w, 4) };
        let input_h = if watching && !spectators { 0.0 } else { field_h + 10.0 };
        let log = Rect::new(r.x + 2.0, r.y + 32.0, r.w - 4.0, r.h - 32.0 - input_h);
        let me = o.me.clone().unwrap_or_else(|| m.names[o.seat].clone());
        let me = (!watching).then_some(me.as_str());
        // The players' chat without what happens at the table (it is in the
        // history), unless the player wants it all together. (Watching, the
        // game's tab is the game itself.)
        let chat_only: Vec<FeedLine>;
        let lines = if spectators {
            &o.spectator_chat
        } else if watching || self.events_in_chat {
            &o.feed
        } else {
            chat_only = o
                .feed
                .iter()
                .filter(|l| !matches!(l.kind, FeedKind::Event | FeedKind::Alert))
                .cloned()
                .collect();
            &chat_only
        };
        if spectators && lines.is_empty() {
            let text = t("Say something to the others watching (the players don't see it).");
            let lines = ui.wrap(text, log.w - 24.0, 13.0);
            for (k, line) in lines.iter().enumerate() {
                let y = log.y + 12.0 + k as f32 * 18.0;
                ui.text_centered(line, log.x + log.w / 2.0, y, 13.0, MUTED, false);
            }
        } else {
            self.chat_scroll = ui.chat_log(CHAT_LOG, log, lines, me, false, self.chat_scroll).0;
        }
        if watching && !spectators {
            // (A spectator's reactions are in the bar below.)
            if !list_open && ui.pressed_outside(&[r, grip]) {
                self.chat_open = false;
            }
            return;
        }
        if phrases {
            let field = Rect::new(r.x + 6.0, r.y + r.h - 6.0 - field_h, r.w - 12.0, field_h);
            if let Some(p) = crate::dm::phrase_picker(ui, field, &mut self.phrases_open) {
                m.send_chat(p);
                self.chat_scroll = 0;
            }
            self.reaction_button(ui, m, reactions, true);
            let mut keep = vec![r, reactions, grip, mute];
            keep.push(field);
            if !list_open && !self.phrases_open && !self.mute_open && ui.pressed_outside(&keep) {
                self.chat_open = false;
            }
            return;
        }
        let input = Rect::new(r.x + 6.0, r.y + r.h - 6.0 - field_h, field_w, field_h);
        let sent =
            ui.text_area(CHAT_FIELD, input, &mut self.chat_input, snook_legend_proto::MAX_CHAT_LEN);
        self.emoji.track(ui, CHAT_FIELD);
        let button = crate::emoji_tray::BUTTON;
        let emoji_button =
            Rect::new(input.x + input.w + 4.0, input.y + input.h - button - 1.0, button, button);
        let (opened, picker) = self.emoji.draw(
            ui,
            emoji_button,
            input,
            r,
            CHAT_FIELD,
            &mut self.chat_input,
            snook_legend_proto::MAX_CHAT_LEN,
        );
        if opened {
            self.quick_open = false;
            self.reactions_open = false;
        }
        if self.chat_input.is_empty() && ui.focus != Some(CHAT_FIELD) {
            ui.text(
                t("Click to chat, Enter to send"),
                input.x + 8.0,
                input.y + (input.h - 13.0) / 2.0,
                13.0,
                MUTED,
                false,
            );
        }
        crate::ui::chat_counter(ui, &self.chat_input, input);
        if sent {
            let text = std::mem::take(&mut self.chat_input);
            if spectators {
                m.send_spectator_chat(&text);
            } else {
                m.send_chat(&text);
            }
            self.chat_scroll = 0;
            self.emoji.reset();
            ui.focus = None;
        }
        self.reaction_button(ui, m, reactions, true);
        self.quick_phrases(ui, m, quick);
        // A press anywhere else hides the chat (what was typed stays). The
        // press still reaches the table: a click on a ball hides it and aims.
        let mut keep = vec![r, reactions, quick, grip, mute];
        keep.extend(picker);
        if !list_open && !self.mute_open && ui.pressed_outside(&keep) {
            self.chat_open = false;
        }
    }

    /// The result of an online game, put away: a slim bar at the top with
    /// the title and what the panel's buttons do (show it again, the
    /// rematch, back to the lounge), the table and the chat free under it.
    fn result_bar(
        &mut self,
        ui: &mut Ui,
        m: &mut Match,
        over: snook_legend_core::GameOver,
    ) -> Option<HudAction> {
        let sw = crate::ui::vw();
        let o = m.online.as_ref()?;
        let me = o.seat;
        let won = over.winner == me && !o.spectator;
        let title = if won {
            if m.won_by_abandonment() { t("Victory by abandonment") } else { t("You win!") }
                .to_string()
        } else {
            tw("{name} wins", &[("name", &m.names[over.winner])])
        };
        let corr = crate::correspondence::is_corr(m);
        let rematch = (!o.spectator && !corr && o.tournament.is_none()).then(|| {
            let can = !o.opponent_left && !o.rematch_requested;
            (if o.rematch_offered { t("Accept rematch") } else { t("Rematch") }, can)
        });
        let offered = o.rematch_offered && !o.rematch_requested;
        let back = if corr {
            t("Back to daily games")
        } else if o.spectator {
            t("Stop watching")
        } else {
            t("Back to lounge")
        };
        let show = t("Show the result");
        const H: f32 = 34.0;
        let bw = |l: &str| crate::online::button_w(ui, l, H - 6.0);
        let (show_w, back_w) = (bw(show), bw(back));
        let rematch_w = rematch.map_or(0.0, |r| bw(r.0) + 8.0);
        let title_w = ui.text_size(&title, 16.0, true).x;
        let w = (16.0 + title_w + 16.0 + show_w + back_w + rematch_w + 16.0).min(sw - 16.0);
        let r = Rect::new((sw - w) / 2.0, TOP_BAR + 8.0, w, H + 12.0);
        ui.panel(r);
        crate::ui::rrect_lines(r, 8.0, 1.5, if won || offered { GOLD } else { BORDER });
        let mid = r.y + r.h / 2.0;
        let (title, size) = ui.fitted(&title, 16.0, title_w.min(r.w * 0.35), true);
        ui.text(&title, r.x + 16.0, crate::ranking::text_y(mid, size), size, GOLD, true);
        let mut x = r.x + r.w - 8.0;
        let mut at = |w: f32| {
            x -= w;
            let b = Rect::new(x, r.y + 6.0, w, H);
            x -= 8.0;
            b
        };
        let mut action = None;
        if ui.button(at(back_w), back, true) {
            action = Some(HudAction::Lounge);
        }
        if let Some((label, can)) = rematch {
            let b = at(rematch_w - 8.0);
            let pressed =
                if offered { ui.primary_button(b, label) } else { ui.button(b, label, can) };
            if pressed && can {
                m.rematch();
            }
        }
        if ui.button(at(show_w), show, true) {
            self.result_hidden = None;
        }
        action
    }

    /// The quick phrases: a button, and the list it opens above it. A phrase
    /// goes out in English and reads in each player's own language.
    fn quick_phrases(&mut self, ui: &mut Ui, m: &mut Match, b: Rect) {
        if m.is_spectating() {
            return;
        }
        let label = t("Quick phrases");
        // The click that opens the list is not a click on it: in a small
        // window the list could open under the pointer and send a phrase.
        let toggled = ui.icon_button_ex(b, Icon::Pencil, label, true, true);
        if toggled {
            self.quick_open = !self.quick_open;
            self.reactions_open = false;
            self.emoji.open = false;
        }
        if !self.quick_open {
            return;
        }
        let phrases = snook_legend_proto::QUICK_CHAT;
        let n = phrases.len() as f32;
        // Rows a little closer when the window is short, never over the button.
        let row_h = ((popover_room(b) - 34.0) / n).clamp(22.0, 30.0);
        let w = phrases
            .iter()
            .map(|p| ui.text_size(t(p), 13.0, false).x + 24.0)
            .fold(200.0, f32::max)
            .min(crate::ui::vw() - 16.0);
        let h = n * row_h + 34.0;
        let r = popover_rect(b, w, h);
        // Grows out of its button.
        let anchor = vec2(b.x + b.w / 2.0, b.y + b.h / 2.0);
        let _pop = ui.popover_scope(crate::ui::motion::id("quick-phrases"), anchor);
        ui.panel(r);
        ui.text(label, r.x + 10.0, r.y + 8.0, 13.0, GOLD, true);
        let mut chosen = None;
        for (k, p) in phrases.iter().enumerate() {
            let row = Rect::new(r.x + 4.0, r.y + 28.0 + k as f32 * row_h, r.w - 8.0, row_h - 2.0);
            let hover = ui.hovering(row);
            let lit = ui.hover_anim(row, hover);
            if lit > 0.01 {
                crate::ui::rrect(row, 5.0, Color::new(1.0, 1.0, 1.0, 0.08 * lit));
            }
            let (text, size) = ui.fitted(t(p), 13.0, row.w - 16.0, false);
            ui.text(
                &text,
                row.x + 8.0,
                crate::ranking::text_y(row.y + row.h / 2.0, size),
                size,
                CREAM,
                false,
            );
            if !toggled && ui.clicked(row) {
                chosen = Some(*p);
            }
        }
        if let Some(p) = chosen {
            m.send_chat(p);
            self.quick_open = false;
        } else if ui.pressed_outside(&[r, b]) {
            self.quick_open = false;
        }
    }

    /// What happened in the game so far, opened from its icon in the top
    /// bar: offline in the chat's place (no one to write to), online over
    /// it while it is open (the chat keeps to what the players say).
    fn draw_history(&mut self, ui: &mut Ui, m: &mut Match) {
        // Above the replay's controls when they would be in the way.
        let bar = video_bar_rect(m);
        let bottom = if m.video.is_some() && bar.x < 12.0 + chat_w() + 8.0 {
            bar.y - 4.0
        } else {
            crate::ui::vh() - BOTTOM_BAR
        };
        // Opened from its icon in the top bar.
        if !self.history_open {
            return;
        }
        let r = Rect::new(12.0, bottom - chat_h() - 8.0, chat_w(), chat_h());
        ui.panel(r);
        ui.text(t("Game history"), r.x + 10.0, r.y + 7.0, 14.0, GOLD, true);
        let hide = Rect::new(r.x + r.w - 30.0, r.y + 4.0, 24.0, 24.0);
        // Hidden by its button, its icon, or a press anywhere else.
        if ui.icon_button_ex(hide, Icon::Minus, t("Hide"), true, true)
            || ui.pressed_outside(&[r, self.history_toggle])
        {
            self.history_open = false;
        }
        let lines: Vec<FeedLine> = m
            .log
            .iter()
            .map(|(text, important)| FeedLine {
                from: None,
                text: text.clone(),
                kind: if *important { FeedKind::Alert } else { FeedKind::Event },
                ..Default::default()
            })
            .collect();
        let log = Rect::new(r.x + 6.0, r.y + 28.0, r.w - 12.0, r.h - 34.0);
        if lines.is_empty() {
            ui.text(t("Nothing yet."), log.x + 6.0, log.y + 6.0, 13.0, MUTED, false);
        } else {
            self.chat_scroll =
                ui.chat_log(HISTORY_LOG, log, &lines, None, false, self.chat_scroll).0;
        }
    }

    /// The reactions button and, when open, the picker above it.
    /// `flat`: in a toolbar (the chat's header), edged only when lit.
    fn reaction_button(&mut self, ui: &mut Ui, m: &mut Match, b: Rect, flat: bool) {
        let Some(o) = m.online.as_ref() else { return };
        ui.block(b);
        let hover = ui.hovering(b);
        crate::ui::rrect(
            b,
            7.0,
            if self.reactions_open || hover {
                Color::new(0.38, 0.25, 0.12, 0.95)
            } else {
                Color::new(0.07, 0.045, 0.03, 0.85)
            },
        );
        if !flat || self.reactions_open || hover {
            crate::ui::rrect_lines(
                b,
                7.0,
                1.2,
                if self.reactions_open || hover { GOLD } else { BORDER },
            );
        }
        crate::reactions::draw(
            "thumbs_up",
            vec2(b.x + b.w / 2.0, b.y + b.h / 2.0),
            (b.h * 0.62).min(20.0),
            crate::paint::alpha() * if hover || self.reactions_open { 1.0 } else { 0.8 },
        );
        if hover && !self.reactions_open {
            ui.tooltip(t("Reactions"), b);
        }
        let clicked_button = ui.activated(b, hover);
        // Open from another button (or opened from none yet): this one
        // claims it only when clicked; the other draws the list.
        let mine = self.reactions_from.is_none_or(|from| from == b);
        if !self.reactions_open || !mine {
            if clicked_button {
                self.reactions_open = true;
                self.emoji.open = false;
                self.reactions_from = Some(b);
            }
            return;
        }
        self.reactions_from = Some(b);
        // As many columns as it takes to stay above the bottom bar and
        // under the top one (a tall list ran over both), cells a little
        // smaller on a narrow window.
        let n = snook_legend_proto::REACTIONS.len();
        let room = popover_room(b).max(200.0);
        let mut cols = 7usize;
        let mut cell = 40.0f32;
        while (n.div_ceil(cols) as f32 * cell + 40.0 > room) && cols < 18 {
            cols += 1;
        }
        let wide = crate::ui::vw() - 24.0;
        if cols as f32 * cell + 16.0 > wide {
            cell = ((wide - 16.0) / cols as f32).max(28.0);
        }

        let rows = n.div_ceil(cols);
        let w = cols as f32 * cell + 16.0;
        let h = rows as f32 * cell + 40.0;
        // Above the button, or below it when the chat reaches the top.
        let p = popover_rect(b, w, h);
        // Grows out of its button.
        let anchor = vec2(b.x + b.w / 2.0, b.y + b.h / 2.0);
        let _pop = ui.popover_scope(crate::ui::motion::id("reactions-grid"), anchor);
        ui.panel(p);
        let open = snook_legend_proto::REACTIONS
            .iter()
            .filter(|r| r.available(&o.unlocked, &o.bought, o.vip))
            .count();
        ui.text(
            &tw("Reactions {n}/{total}", &[("n", &open), ("total", &n)]),
            p.x + 10.0,
            p.y + 9.0,
            13.0,
            GOLD,
            true,
        );
        let mut chosen = None;
        let mut tip: Option<(String, Rect)> = None;
        // The ones this player has first, then those still to win.
        let mut order: Vec<&snook_legend_proto::Reaction> =
            snook_legend_proto::REACTIONS.iter().collect();
        order.sort_by_key(|r| !r.available(&o.unlocked, &o.bought, o.vip));
        for (k, r) in order.into_iter().enumerate() {
            let c = Rect::new(
                p.x + 8.0 + (k % cols) as f32 * cell,
                p.y + 32.0 + (k / cols) as f32 * cell,
                cell,
                cell,
            );
            let center = vec2(c.x + c.w / 2.0, c.y + c.h / 2.0);
            let unlocked = r.available(&o.unlocked, &o.bought, o.vip);
            let hover = ui.hovering(c);
            if unlocked {
                if hover {
                    crate::ui::rrect(c, 8.0, Color::new(1.0, 1.0, 1.0, 0.10));
                }
                crate::reactions::draw(
                    r.code,
                    center,
                    if hover { 32.0 } else { 28.0 },
                    crate::paint::alpha(),
                );
                if ui.activated(c, hover) {
                    chosen = Some(r.code);
                }
            } else {
                crate::reactions::draw_locked(r.code, center, 26.0);
                crate::icons::draw(
                    crate::icons::Icon::Lock,
                    vec2(c.x + c.w - 10.0, c.y + c.h - 10.0),
                    11.0,
                    MUTED,
                );
            }
            // Had or not, it says how it is won.
            if hover {
                tip = Some((crate::reactions::how(r, unlocked), c));
            }
        }
        if let Some((text, c)) = tip {
            ui.tooltip(&text, c);
        }
        ui.block(p);
        if let Some(code) = chosen {
            m.send_reaction(code);
            self.reactions_open = false;
        } else if (is_mouse_button_released(MouseButton::Left)
            && !ui.hovering(p)
            && !clicked_button)
            || clicked_button
        {
            self.reactions_open = false;
        }
    }
}

impl Hud {
    /// The crowd: a spectator clicks a player's card to cheer for them; a
    /// thin bar under the cards shows how the spectators are split.
    fn draw_support(&mut self, ui: &mut Ui, m: &mut Match, board: &crate::scoreboard::Board) {
        let (x0, total) = (board.all.x, board.all.w);
        let Some(o) = m.online.as_ref() else { return };
        let (support, spectator, cheering) = (o.support, o.spectator, o.cheering);
        if spectator {
            for (seat, color) in SEAT_COLORS.into_iter().enumerate() {
                let card = board.seat(seat);
                if cheering == Some(seat) {
                    crate::icons::draw_filled(
                        Icon::Star,
                        vec2(card.x + card.w - 12.0, card.y + 10.0),
                        12.0,
                        color,
                    );
                }
                if ui.hovering(card) {
                    let tip = tw("Cheer for {name}", &[("name", &m.names[seat])]);
                    ui.tooltip(&tip, card);
                    if ui.clicked(card) {
                        m.cheer(seat);
                    }
                }
            }
        }
        let n = support[0] + support[1];
        if n == 0 {
            return;
        }
        let bar = Rect::new(x0, TOP_BAR - 4.0, total, 4.0);
        let k = support[0] as f32 / n as f32;
        draw_rectangle(bar.x, bar.y, bar.w * k, bar.h, SEAT_COLORS[0]);
        draw_rectangle(bar.x + bar.w * k, bar.y, bar.w * (1.0 - k), bar.h, SEAT_COLORS[1]);
        let tip = Rect::new(bar.x, bar.y - 4.0, bar.w, bar.h + 8.0);
        if ui.hovering(tip) {
            let text = tw(
                "Crowd: {a} for {p0}, {b} for {p1}",
                &[("a", &support[0]), ("p0", &m.names[0]), ("b", &support[1]), ("p1", &m.names[1])],
            );
            ui.tooltip(&text, tip);
        }
    }
}

#[derive(Clone, Copy)]
enum Media {
    Previous,
    Play,
    Pause,
    Next,
}

/// A round media button drawn from shapes (no font glyphs needed).
fn media_button(ui: &mut Ui, r: Rect, kind: Media, tip: &str) -> bool {
    let clicked = ui.button(r, "", true);
    let c = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
    let s = r.h * 0.22;
    let tri = |dir: f32, x: f32| {
        draw_triangle(
            vec2(x - dir * s * 0.8, c.y - s),
            vec2(x - dir * s * 0.8, c.y + s),
            vec2(x + dir * s * 0.9, c.y),
            CREAM,
        );
    };
    match kind {
        Media::Play => tri(1.0, c.x + s * 0.1),
        Media::Pause => {
            draw_rectangle(c.x - s * 0.8, c.y - s, s * 0.55, s * 2.0, CREAM);
            draw_rectangle(c.x + s * 0.25, c.y - s, s * 0.55, s * 2.0, CREAM);
        }
        Media::Previous => {
            draw_rectangle(c.x - s * 1.2, c.y - s, s * 0.35, s * 2.0, CREAM);
            tri(-1.0, c.x + s * 0.2);
        }
        Media::Next => {
            tri(1.0, c.x - s * 0.2);
            draw_rectangle(c.x + s * 0.85, c.y - s, s * 0.35, s * 2.0, CREAM);
        }
    }
    if ui.hovering(r) {
        ui.tooltip(tip, r);
    }
    clicked
}

/// Where the replay's controls go.
fn video_bar_rect(m: &Match) -> Rect {
    let sw = crate::ui::vw();
    let w = (sw - 24.0).min(680.0);
    // In the bottom bar (no cue to set up while a recorded game plays), not
    // over the table; above it when the spectator's bar has it.
    let y = if m.is_spectating() {
        crate::ui::vh() - BOTTOM_BAR - 64.0
    } else {
        crate::ui::vh() - BOTTOM_BAR + (BOTTOM_BAR - 52.0) / 2.0
    };
    Rect::new((sw - w) / 2.0, y, w, 52.0)
}

/// The list of controls with a game controller (its maker's button names).
pub(crate) fn pad_controls_list(top_only: bool) -> Vec<(&'static str, String)> {
    use crate::gamepad::{Button, label};
    let b = |x: Button| label(x);
    let mut camera = vec![tw(
        "{rb} + right stick: turn · {lt} / {rt}: zoom",
        &[
            ("rb", &b(Button::RightShoulder)),
            ("lt", &b(Button::LeftTrigger)),
            ("rt", &b(Button::RightTrigger)),
        ],
    )];
    if !top_only {
        camera.insert(0, tw("{y}: 3D / top view", &[("y", &b(Button::North))]));
    }
    vec![
        (
            t("Aim"),
            tw(
                "Left stick · hold {lb}: fine · d-pad ← →: a hair",
                &[("lb", &b(Button::LeftShoulder))],
            ),
        ),
        (
            t("Shoot"),
            tw(
                "Hold {a}, let go to shoot · {b}: cancel",
                &[("a", &b(Button::South)), ("b", &b(Button::East))],
            ),
        ),
        (t("Spin"), t("Right stick").to_string()),
        (t("Elevation"), t("D-pad ↑ ↓").to_string()),
        (
            t("Cue ball in hand"),
            tw(
                "{x}: pick up · left stick: move · {a}: put down",
                &[("x", &b(Button::West)), ("a", &b(Button::South))],
            ),
        ),
        (t("Calling the 8"), tw("{x}: the next pocket", &[("x", &b(Button::West))])),
        (t("Camera"), camera.join(" · ")),
        (
            t("Menu"),
            tw(
                "{start}: menu · {back}: controls",
                &[("start", &b(Button::Start)), ("back", &b(Button::Back))],
            ),
        ),
    ]
}

/// The list of controls (H), with the keys the player has chosen (a line
/// that would only name keys is left out when its action has none).
fn controls_list(top_only: bool) -> Vec<(&'static str, String)> {
    let k = keys::short;
    // Parts joined with " · ", leaving out the empty ones.
    let join = |parts: &[&str]| {
        parts.iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(" · ")
    };
    // Two keys ("Q / E"), or the one there is.
    let pair = |a: Action, b: Action| {
        [k(a), k(b)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" / ")
    };
    let with_key = |text: &str, action: Action| match k(action) {
        "" => String::new(),
        key => tw(text, &[("key", &key)]),
    };
    let with_keys = |text: &str, keys: String| {
        if keys.is_empty() { String::new() } else { tw(text, &[("keys", &keys)]) }
    };
    let aim_keys = pair(Action::AimLeft, Action::AimRight);
    let spin_keys = [k(Action::SpinUp), k(Action::SpinDown)]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let side_spin = pair(Action::SpinLeft, Action::SpinRight);
    vec![
        (t("Aim"), t("Drag the cue · click a spot").to_string()),
        (t("Fine aim"), join(&[&with_key("{key} + drag", Action::FineAim), &aim_keys])),
        (t("Quick aim"), t("Click a ball or a spot").to_string()),
        (
            t("Shoot"),
            join(&[
                t("Set the power, then press the green button"),
                &with_key("hold {key} and release", Action::Shoot),
            ]),
        ),
        (t("Spin"), join(&[t("Spin ball"), &spin_keys, &side_spin])),
        (
            t("Elevation"),
            with_keys("{keys} (jump & massé)", pair(Action::ElevationUp, Action::ElevationDown)),
        ),
        (
            t("Camera"),
            if top_only {
                join(&[t("Wheel"), k(Action::ResetCamera)])
            } else {
                join(&[t("Right-drag · Wheel"), k(Action::ResetCamera)])
            },
        ),
        (
            t("View"),
            join(&[
                &if top_only {
                    String::new()
                } else {
                    with_key("{key}: 3D / top view", Action::ToggleView)
                },
                &with_keys("{keys}: zoom", pair(Action::ZoomIn, Action::ZoomOut)),
            ]),
        ),
        (t("Replay"), with_key("{key}: watch the last shot again", Action::Replay)),
        (t("Ball in hand"), join(&[t("Drag the cue ball"), k(Action::BallInHand)])),
        (t("Full screen"), k(Action::Fullscreen).to_string()),
        (t("Menu / help"), join(&["Esc", k(Action::Help)])),
    ]
    .into_iter()
    .filter(|(_, keys)| !keys.is_empty())
    .collect()
}

/// Watching a stored game: play and pause, the shot before and after, the
/// speed, and a line to drag to any moment of the game.
fn draw_video_bar(ui: &mut Ui, m: &mut Match) {
    let Some(v) = m.video.as_ref() else { return };
    let (total, paused, speed) = (v.states.len() - 1, v.paused, v.speed);
    if total == 0 {
        return;
    }
    let marks: Vec<Option<crate::game::ShotMark>> = v.marks.clone();
    let r = video_bar_rect(m);
    ui.panel(r);
    // The shot on screen: the one being played, or the next to come.
    let pos = m.video_position();
    let shown = pos.max(1);
    let mut x = r.x + 10.0;
    let b = |x: f32| Rect::new(x, r.y + 9.0, 34.0, 34.0);
    let keys = !ui.wants_keyboard;
    if media_button(ui, b(x), Media::Previous, t("Previous shot (←)"))
        || (keys && is_key_pressed(KeyCode::Left))
    {
        // The shot before this one (a clock run out or a choice in between
        // is played on the way: going back to it would only come here again).
        let current = shown - 1;
        let previous = m.video.as_ref().and_then(|v| {
            (0..current).rev().find(|&j| {
                matches!(v.strokes.get(j), Some(snook_legend_proto::replay::Stroke::Shot { .. }))
            })
        });
        m.seek(previous.unwrap_or(0));
    }
    x += 40.0;
    let (kind, tip) =
        if paused { (Media::Play, t("Play (Space)")) } else { (Media::Pause, t("Pause (Space)")) };
    if (media_button(ui, b(x), kind, tip) || (keys && is_key_pressed(KeyCode::Space)))
        && let Some(v) = m.video.as_mut()
    {
        v.paused = !v.paused;
    }
    x += 40.0;
    if media_button(ui, b(x), Media::Next, t("Next shot (→)"))
        || (keys && is_key_pressed(KeyCode::Right))
    {
        m.seek(shown.min(total));
    }
    x += 44.0;
    let label = if speed < 1.0 { "½×".to_string() } else { format!("{speed}×") };
    if ui.button(Rect::new(x, r.y + 11.0, 46.0, 30.0), &label, true)
        && let Some(v) = m.video.as_mut()
    {
        v.next_speed();
    }
    if ui.hovering(Rect::new(x, r.y + 11.0, 46.0, 30.0)) {
        ui.tooltip(t("Speed"), Rect::new(x, r.y + 11.0, 46.0, 30.0));
    }
    x += 58.0;
    // The time line: one step per stroke, a tick where balls were made.
    let count = tw("Shot {n} of {total}", &[("n", &shown.min(total)), ("total", &total)]);
    let cw = ui.text_size(&count, 13.0, true).x;
    let line = Rect::new(x, r.y + 14.0, r.x + r.w - x - cw - 24.0, 24.0);
    for (i, mark) in marks.iter().enumerate() {
        let Some(mark) = mark.filter(|k| k.made > 0 || k.foul) else { continue };
        let tx = line.x + line.w * (i as f32 + 0.5) / total as f32;
        let color = if mark.foul { ALERT } else { SEAT_COLORS[mark.shooter] };
        draw_line(tx, line.y + 1.0, tx, line.y + 6.0, 2.0, color);
    }
    let mut value = (shown - 1) as f32;
    if ui.slider(9_100, line, &mut value, 0.0, (total - 1).max(1) as f32) {
        let k = value.round() as usize;
        if k + 1 != shown {
            m.seek(k);
        }
    }
    ui.text(&count, r.x + r.w - cw - 12.0, r.y + 18.0, 13.0, CREAM, true);
}

const PREVIEW_TIME: f32 = 8.0;
/// Height of the numbers panel under the game-over panel (and the gap).
const STATS_H: f32 = 196.0;

/// The game in numbers, per player (potting success, balls made, best run,
/// fouls, ball in hand), how the lead went back and forth, and where each
/// shot was played from.
fn draw_match_stats(ui: &mut Ui, m: &Match, r: Rect, view: &mut usize) {
    ui.panel(r);
    let options = [t("Numbers"), t("Momentum"), t("Shot map")];
    let seg = Rect::new(r.x + 10.0, r.y + 8.0, r.w - 20.0, 26.0);
    if let Some(i) = ui.segmented(seg, &options, *view) {
        *view = i;
    }
    let body = Rect::new(r.x + 10.0, r.y + 42.0, r.w - 20.0, r.h - 50.0);
    match *view {
        1 => draw_momentum(ui, m, body),
        2 => draw_shot_map(ui, m, body),
        _ => draw_numbers(ui, m, body),
    }
}

/// The two players' colours on the charts.
const SEAT_COLORS: [Color; 2] =
    [Color::new(0.91, 0.75, 0.38, 1.0), Color::new(0.45, 0.72, 1.0, 1.0)];

/// A player's name for a narrow column: "Computer (Medium)" loses the
/// level rather than end in "…".
fn short_name(ui: &Ui, name: &str, size: f32, room: f32) -> (String, f32) {
    let short = match name.split_once(" (") {
        Some((base, _)) if ui.text_size(name, size - 1.0, true).x > room => base,
        _ => name,
    };
    ui.fitted(short, size, room, true)
}

fn draw_numbers(ui: &mut Ui, m: &Match, r: Rect) {
    let s = &m.stats;
    let rows = [
        (t("Potting success"), format!("{}%", s[0].pot_rate()), format!("{}%", s[1].pot_rate())),
        (t("Balls made"), s[0].balls.to_string(), s[1].balls.to_string()),
        (t("Best run"), s[0].best_run.to_string(), s[1].best_run.to_string()),
        (t("Fouls"), s[0].fouls.to_string(), s[1].fouls.to_string()),
        (
            t("Ball in hand"),
            format!("{}%", s[0].in_hand_rate()),
            format!("{}%", s[1].in_hand_rate()),
        ),
    ];
    let label_x = r.x + 6.0;
    let col = [r.x + r.w * 0.6, r.x + r.w * 0.85];
    for (k, name) in m.names.iter().enumerate() {
        let (name, size) = short_name(ui, name, 13.0, r.w * 0.25);
        ui.text_centered(&name, col[k], r.y, size, GOLD, true);
    }
    for (i, (label, a, b)) in rows.iter().enumerate() {
        let y = r.y + 22.0 + i as f32 * 21.0;
        if i % 2 == 0 {
            crate::ui::rrect(
                Rect::new(r.x - 2.0, y - 3.0, r.w + 4.0, 20.0),
                4.0,
                Color::new(1.0, 1.0, 1.0, 0.035),
            );
        }
        let (label, size) = ui.fitted(label, 13.0, r.w * 0.45, false);
        ui.text(&label, label_x, y, size, MUTED, false);
        ui.text_centered(a, col[0], y, 13.0, CREAM, true);
        ui.text_centered(b, col[1], y, 13.0, CREAM, true);
    }
}

/// Who was on top, shot by shot: each ball made moves the line towards its
/// player, each foul away from the one who made it.
pub fn momentum(timeline: &[crate::game::ShotMark]) -> Vec<f32> {
    let mut v = 0.0;
    let mut out = vec![0.0];
    for s in timeline {
        let gain = s.made as f32 - if s.foul { 1.0 } else { 0.0 };
        v += if s.shooter == 0 { gain } else { -gain };
        out.push(v);
    }
    out
}

fn chart_legend(ui: &mut Ui, m: &Match, r: Rect) {
    let mut x = r.x + r.w;
    for seat in [1, 0] {
        let (name, size) = ui.fitted(&m.names[seat], 12.0, r.w * 0.3, false);
        let w = ui.text_size(&name, size, false).x;
        x -= w;
        ui.text(&name, x, r.y, size, MUTED, false);
        draw_circle(x - 8.0, r.y + 7.0, 4.0, SEAT_COLORS[seat]);
        x -= 22.0;
    }
}

fn draw_momentum(ui: &mut Ui, m: &Match, r: Rect) {
    let values = momentum(&m.timeline);
    if values.len() < 2 {
        ui.text_centered(
            t("No shots yet."),
            r.x + r.w / 2.0,
            r.y + r.h / 2.0 - 8.0,
            13.0,
            MUTED,
            false,
        );
        return;
    }
    chart_legend(ui, m, r);
    let plot = Rect::new(r.x + 4.0, r.y + 20.0, r.w - 8.0, r.h - 24.0);
    crate::ui::rrect(plot, 6.0, Color::new(0.0, 0.0, 0.0, 0.25));
    let mid = plot.y + plot.h / 2.0;
    let top = values.iter().fold(3.0f32, |a, v| a.max(v.abs()));
    let half = plot.h / 2.0 - 6.0;
    let step = plot.w / (values.len() - 1) as f32;
    let at = |i: usize, v: f32| vec2(plot.x + i as f32 * step, mid - v / top * half);
    for i in 1..values.len() {
        let (a, b) = (at(i - 1, values[i - 1]), at(i, values[i]));
        // The area between the line and the middle, in the leader's colour.
        let lead = values[i - 1] + values[i];
        let mut c = SEAT_COLORS[if lead >= 0.0 { 0 } else { 1 }];
        c.a = 0.28;
        let (a0, b0) = (vec2(a.x, mid), vec2(b.x, mid));
        draw_triangle(a0, a, b, c);
        draw_triangle(a0, b, b0, c);
    }
    draw_line(plot.x, mid, plot.x + plot.w, mid, 1.0, Color::new(1.0, 1.0, 1.0, 0.2));
    for i in 1..values.len() {
        let (a, b) = (at(i - 1, values[i - 1]), at(i, values[i]));
        let lead = values[i];
        let c = SEAT_COLORS[if lead >= 0.0 { 0 } else { 1 }];
        draw_line(a.x, a.y, b.x, b.y, 2.0, c);
    }
    // Fouls: a small mark on the line.
    for (i, s) in m.timeline.iter().enumerate() {
        if s.foul {
            let p = at(i + 1, values[i + 1]);
            draw_circle(p.x, p.y, 3.0, ALERT);
        }
    }
}

fn draw_shot_map(ui: &mut Ui, m: &Match, r: Rect) {
    if m.timeline.is_empty() {
        ui.text_centered(
            t("No shots yet."),
            r.x + r.w / 2.0,
            r.y + r.h / 2.0 - 8.0,
            13.0,
            MUTED,
            false,
        );
        return;
    }
    let spec = &m.game.table().spec;
    let (len, wid) = (spec.length as f32, spec.width as f32);
    // The table, as large as fits beside the legend.
    let avail = Rect::new(r.x, r.y, r.w - 120.0, r.h);
    let scale = (avail.w / (len + 0.16)).min(avail.h / (wid + 0.16));
    let (tw_, th) = (len * scale, wid * scale);
    let cloth = Rect::new(avail.x + (avail.w - tw_) / 2.0, avail.y + (avail.h - th) / 2.0, tw_, th);
    let rail = 0.06 * scale;
    crate::ui::rrect(
        Rect::new(cloth.x - rail, cloth.y - rail, cloth.w + 2.0 * rail, cloth.h + 2.0 * rail),
        6.0,
        Color::new(0.30, 0.17, 0.09, 1.0),
    );
    draw_rectangle(cloth.x, cloth.y, cloth.w, cloth.h, Color::new(0.08, 0.36, 0.22, 1.0));
    let to_screen = |x: f64, y: f64| {
        vec2(cloth.x + (x as f32 / len + 0.5) * cloth.w, cloth.y + (0.5 - y as f32 / wid) * cloth.h)
    };
    for p in &m.game.table().pockets {
        let c = to_screen(p.mouth_center.x, p.mouth_center.y);
        draw_circle(c.x, c.y, 0.055 * scale, Color::new(0.02, 0.02, 0.02, 1.0));
    }
    // Soft blobs where shots were played from (the heat), then the shots.
    for s in &m.timeline {
        let p = to_screen(s.cue.0, s.cue.1);
        let mut c = SEAT_COLORS[s.shooter];
        c.a = 0.16;
        draw_circle(p.x, p.y, 0.09 * scale, c);
    }
    for s in &m.timeline {
        let p = to_screen(s.cue.0, s.cue.1);
        let c = SEAT_COLORS[s.shooter];
        if s.foul {
            draw_line(p.x - 3.0, p.y - 3.0, p.x + 3.0, p.y + 3.0, 1.5, ALERT);
            draw_line(p.x - 3.0, p.y + 3.0, p.x + 3.0, p.y - 3.0, 1.5, ALERT);
        } else if s.made > 0 {
            draw_circle(p.x, p.y, 3.5, c);
        } else {
            draw_circle_lines(p.x, p.y, 3.0, 1.2, c);
        }
    }
    // The legend.
    let lx = r.x + r.w - 112.0;
    let mut y = r.y + 4.0;
    for (seat, color) in SEAT_COLORS.into_iter().enumerate() {
        let made = m.timeline.iter().filter(|s| s.shooter == seat && s.made > 0).count();
        let shots = m.timeline.iter().filter(|s| s.shooter == seat).count();
        draw_circle(lx + 5.0, y + 7.0, 4.0, color);
        let (name, size) = short_name(ui, &m.names[seat], 12.0, 96.0);
        ui.text(&name, lx + 14.0, y, size, CREAM, true);
        ui.text(&format!("{made}/{shots}"), lx + 14.0, y + 15.0, 12.0, MUTED, false);
        y += 38.0;
    }
    draw_circle(lx + 5.0, y + 7.0, 3.5, CREAM);
    ui.text(t("made"), lx + 14.0, y, 12.0, MUTED, false);
    draw_circle_lines(lx + 5.0, y + 25.0, 3.0, 1.2, CREAM);
    ui.text(t("missed"), lx + 14.0, y + 18.0, 12.0, MUTED, false);
    let fy = y + 43.0;
    draw_line(lx + 2.0, fy - 3.0, lx + 8.0, fy + 3.0, 1.5, ALERT);
    draw_line(lx + 2.0, fy + 3.0, lx + 8.0, fy - 3.0, 1.5, ALERT);
    ui.text(t("foul"), lx + 14.0, fy - 7.0, 12.0, MUTED, false);
}

const REACTION_TIME: f32 = 2.8;

/// Reactions float down from the sender's card and fade (drawn even while
/// the HUD fades for aiming: they are meant to be seen).
fn draw_reactions(m: &mut Match, xs: [f32; 2], eye: Vec2) {
    let dt = get_frame_time();
    let Some(o) = m.online.as_mut() else { return };
    // The setting "Reactions" (set to off) off: no one's are shown.
    if !crate::perks::reactions_shown() {
        o.reactions.clear();
        return;
    }
    let mut crowd = 0;
    for (seat, code, age) in &mut o.reactions {
        *age += dt;
        let t = *age;
        if *seat >= 2 {
            // From the spectators: small, faint, drifting down from the eye,
            // side by side, so the players hardly notice them.
            let alpha = 0.75 * (t / 0.2).min(1.0) * ((REACTION_TIME - t) / 0.8).clamp(0.0, 1.0);
            let at = vec2(
                eye.x + 4.0 + crowd as f32 * 20.0,
                TOP_BAR + 14.0 + 18.0 * (t / REACTION_TIME),
            );
            crate::reactions::draw(code, at, 18.0, alpha);
            crowd += 1;
            continue;
        }
        let pop = if t < 0.25 {
            0.6 + 0.4 * (t / 0.25) + 0.25 * (t / 0.25 * std::f32::consts::PI).sin()
        } else {
            1.0
        };
        let alpha = (t / 0.12).min(1.0) * ((REACTION_TIME - t) / 0.7).clamp(0.0, 1.0);
        let y = TOP_BAR + 30.0 + 14.0 * (t / REACTION_TIME).sqrt();
        let size = 38.0 * pop;
        macroquad::shapes::draw_circle(
            xs[*seat],
            y,
            size * 0.62,
            Color::new(0.0, 0.0, 0.0, 0.35 * alpha),
        );
        crate::reactions::draw(code, vec2(xs[*seat], y), size, alpha);
    }
    o.reactions.retain(|(.., age)| *age < REACTION_TIME);
}

const BANNER_TIME: f32 = 1.9;

impl Hud {
    /// A large t("Your turn") style message whenever the turn changes.
    /// The official rules: what the break left to decide, with the answers
    /// as buttons, for the player who decides (from `top` down).
    fn draw_choice(&mut self, ui: &mut Ui, m: &mut Match, top: f32) {
        let Some(choice) = m.game.pending_choice() else { return };
        if !matches!(m.phase, Phase::Turn { .. }) || m.current_controller() != Controller::Human {
            return;
        }
        let sw = crate::ui::vw();
        let me = m.game.current_player();
        let other = m.names[1 - me].clone();
        let options = choice.options();
        let w = (190.0 * options.len() as f32 + 40.0 + 10.0 * (options.len() - 1) as f32)
            .min(sw - 24.0);
        let title = crate::i18n::choice_title(choice);
        let lead = t("How does play go on? Choose one:");
        let r = Rect::new((sw - w) / 2.0, top, w, 24.0 + 24.0 + 20.0 + 12.0 + 66.0 + 18.0);
        crate::ui::soft_shadow(r, 10.0, 0.9);
        ui.panel(r);
        crate::ui::rrect_lines(r, 8.0, 1.6, crate::rulesui::OFFICIAL);
        ui.text_centered(title, sw / 2.0, r.y + 16.0, 19.0, GOLD, true);
        ui.text_centered(lead, sw / 2.0, r.y + 44.0, 14.0, CREAM, false);
        let sent = m.online.as_ref().and_then(|o| o.choice_sent);
        let bw = (w - 40.0 - 10.0 * (options.len() - 1) as f32) / options.len() as f32;
        let by = r.y + 24.0 + 24.0 + 20.0 + 12.0;
        let mut chosen = None;
        // The keys 1 to 4 choose too.
        let keys = [KeyCode::Key1, KeyCode::Key2, KeyCode::Key3, KeyCode::Key4];
        for (k, option) in options.iter().copied().enumerate() {
            let b = Rect::new(r.x + 20.0 + k as f32 * (bw + 10.0), by, bw, 66.0);
            let (label, what) = crate::i18n::break_option(choice, option, &other);
            let label = format!("{}. {label}", k + 1);
            let enabled = sent.is_none();
            let key =
                enabled && !ui.wants_keyboard && keys.get(k).is_some_and(|c| is_key_pressed(*c));
            if self.choice_button(ui, b, &label, &what, k == 0, enabled, sent == Some(option))
                || key
            {
                chosen = Some(option);
            }
        }
        ui.block(r);
        if let Some(option) = chosen {
            m.choose(option);
        }
    }

    /// One answer to the break's choice: its label and what it does.
    #[allow(clippy::too_many_arguments)]
    fn choice_button(
        &self,
        ui: &mut Ui,
        b: Rect,
        label: &str,
        what: &str,
        first: bool,
        enabled: bool,
        sending: bool,
    ) -> bool {
        let hover = enabled && ui.hovering(b);
        let base = if first {
            Color::new(0.10, 0.38, 0.22, 1.0)
        } else {
            Color::new(0.20, 0.13, 0.08, 1.0)
        };
        let fill = if hover { crate::ui::shade(base, 0.12) } else { base };
        crate::ui::rrect_gradient(
            b,
            8.0,
            crate::ui::shade(fill, 0.06),
            crate::ui::shade(fill, -0.12),
        );
        let edge = if hover {
            GOLD
        } else if first {
            Color::new(0.35, 0.75, 0.5, 1.0)
        } else {
            BORDER
        };
        crate::ui::rrect_lines(b, 8.0, 1.2, edge);
        let (label, size) = ui.fitted(label, 15.0, b.w - 16.0, true);
        ui.text_centered(&label, b.x + b.w / 2.0, b.y + 10.0, size, WHITE, true);
        let lines = ui.wrap(what, b.w - 16.0, 12.0);
        for (k, line) in lines.iter().take(2).enumerate() {
            let c = Color::new(0.88, 0.84, 0.76, 0.9);
            ui.text_centered(line, b.x + b.w / 2.0, b.y + 32.0 + k as f32 * 15.0, 12.0, c, false);
        }
        if sending {
            crate::ui::spinner(vec2(b.x + b.w - 14.0, b.y + 14.0), 6.0, GOLD);
        }
        enabled && ui.clicked(b)
    }

    fn draw_turn_banner(&mut self, ui: &mut Ui, m: &Match) {
        let dt = get_frame_time();
        let all_computers = m.controllers.iter().all(|c| matches!(c, Controller::Computer(_)));
        if let Phase::Turn { .. } = m.phase
            && !all_computers
        {
            let player = m.game.current_player();
            let key = (m.games_played, m.game.shots_taken(), player);
            if self.banner_key != Some(key) {
                let first = self.banner_key.is_none_or(|k| k.0 != key.0 || k.2 != key.2);
                self.banner_key = Some(key);
                // Only announce a change of player (or a new game), not every shot.
                if first {
                    self.banner = Some((turn_title(m), turn_subtitle(m), 0.0));
                }
            }
        }
        let Some((title, subtitle, age)) = &mut self.banner else { return };
        *age += dt;
        if *age > BANNER_TIME || self.paused {
            self.banner = None;
            return;
        }
        let t = *age;
        let alpha = (t / 0.18).min(1.0) * ((BANNER_TIME - t) / 0.45).min(1.0);
        let slide = (1.0 - (t / 0.25).min(1.0)).powi(2) * 30.0;
        let sw = crate::ui::vw();
        let y = crate::ui::vh() * 0.21 + slide;
        // Never wider than the window: long names and translations get
        // smaller type, then are cut short.
        let room = (sw - 24.0 - 80.0).max(120.0);
        let (title, size) = ui.fitted(title, 40.0, room, true);
        let (subtitle, sub_size) = ui.fitted(subtitle, 17.0, room, false);
        let (title, subtitle) = (&title, &subtitle);
        let w =
            ui.text_size(title, size, true).x.max(ui.text_size(subtitle, sub_size, false).x) + 80.0;
        let r = Rect::new((sw - w) / 2.0, y - 14.0, w, 92.0);
        crate::ui::rrect(r, 14.0, Color::new(0.0, 0.0, 0.0, 0.55 * alpha));
        let gold = Color::new(GOLD.r, GOLD.g, GOLD.b, alpha);
        draw_line(
            r.x + 30.0,
            r.y + 64.0,
            r.x + r.w - 30.0,
            r.y + 64.0,
            1.0,
            Color::new(GOLD.r, GOLD.g, GOLD.b, 0.4 * alpha),
        );
        ui.text_centered(
            title,
            sw / 2.0 + 2.0,
            y + 2.0,
            size,
            Color::new(0.0, 0.0, 0.0, 0.5 * alpha),
            true,
        );
        ui.text_centered(title, sw / 2.0, y, size, gold, true);
        ui.text_centered(
            subtitle,
            sw / 2.0,
            y + 56.0,
            sub_size,
            Color::new(CREAM.r, CREAM.g, CREAM.b, alpha),
            false,
        );
    }
}

/// On the end panel's corner: the deciding shot, once more (true when
/// pressed).
fn replay_button(ui: &mut Ui, m: &Match, panel: Rect) -> bool {
    if m.last_shot.is_none() {
        return false;
    }
    let b = Rect::new(panel.x + panel.w - 40.0, panel.y + 10.0, 30.0, 30.0);
    let tip = keys::tip(t("Watch the deciding shot again"), Action::Replay);
    ui.icon_button_ex(b, Icon::Replay, &tip, true, true)
}

/// A pointer for a first game: a label with its step number over a small
/// arrow bobbing down to `at` (kept inside the window).
fn coach_mark(ui: &mut Ui, at: Vec2, text: &str, step: Option<u8>, time: f32) {
    let label = match step {
        Some(n) => format!("{n} · {text}"),
        None => text.to_string(),
    };
    let bob = (time * 3.2).sin() * 3.0;
    let tip = vec2(at.x, at.y + bob);
    let w = ui.text_size(&label, 13.0, true).x + 22.0;
    let sw = crate::ui::vw();
    let x = crate::ui::clamp_soft(tip.x - w / 2.0, 8.0, sw - w - 8.0);
    let r = Rect::new(x, tip.y - 40.0, w, 24.0);
    let glow = 0.75 + 0.25 * (time * 3.2).cos();
    crate::ui::rrect(r, 12.0, Color::new(0.08, 0.06, 0.03, 0.92));
    crate::ui::rrect_lines(r, 12.0, 1.4, Color::new(GOLD.r, GOLD.g, GOLD.b, glow));
    ui.text(&label, r.x + 11.0, r.y + 5.0, 13.0, GOLD, true);
    if step.is_some() {
        draw_triangle(
            vec2(tip.x - 7.0, tip.y - 15.0),
            vec2(tip.x + 7.0, tip.y - 15.0),
            vec2(tip.x, tip.y - 5.0),
            Color::new(GOLD.r, GOLD.g, GOLD.b, glow),
        );
    }
}

fn turn_title(m: &Match) -> String {
    let player = m.game.current_player();
    let name = &m.names[player];
    let mine = match &m.online {
        Some(o) => o.seat == player && !o.spectator,
        None => {
            m.controllers[player] == Controller::Human
                && m.controllers.iter().filter(|c| **c == Controller::Human).count() == 1
        }
    };
    if mine { t("Your turn").into() } else { tw("{name}'s turn", &[("name", name)]) }
}

fn turn_subtitle(m: &Match) -> String {
    let player = m.game.current_player();
    if m.game.is_break() {
        return t("Break the rack").into();
    }
    // (The official rules call the 8 on each shot at it.)
    if m.game.must_call_pocket() {
        return match m.game.called_pocket_of(player) {
            Some(p) => tw("On the 8-ball: {pocket} pocket", &[("pocket", &crate::i18n::pocket(p))]),
            None => t("On the 8-ball: call a pocket").into(),
        };
    }
    let group = match m.game.group_of(player) {
        Some(Group::Solids) => t("Solids (1–7)"),
        Some(Group::Stripes) => t("Stripes (9–15)"),
        None => t("Open table"),
    };
    match m.game.ball_in_hand() {
        Some(_) => tw("Ball in hand  ·  {group}", &[("group", &group)]),
        None => group.into(),
    }
}

impl Hud {
    /// The bottom bar: game log, the power bar and the cue settings.
    /// Watching a friend: who, the reactions and a way out. Returns true to
    /// stop watching.
    fn draw_spectator_bar(&mut self, ui: &mut Ui, m: &mut Match) -> bool {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let bar = Rect::new(0.0, sh - BOTTOM_BAR, sw, BOTTOM_BAR);
        draw_bar(bar, false);
        ui.block(bar);
        let Some(o) = m.online.as_ref() else { return false };
        let friend = m.names[o.seat].clone();
        let cy = bar.y + bar.h / 2.0;
        crate::icons::draw(Icon::Eye, vec2(30.0, cy), 22.0, GOLD);
        ui.text(&tw("Watching {name}", &[("name", &friend)]), 50.0, cy - 17.0, 16.0, CREAM, true);
        ui.text(t("Your reactions reach the table quietly."), 50.0, cy + 4.0, 12.0, MUTED, false);
        let text_w = ui
            .text_size(&tw("Watching {name}", &[("name", &friend)]), 16.0, true)
            .x
            .max(ui.text_size(t("Your reactions reach the table quietly."), 12.0, false).x);
        self.reaction_button(ui, m, Rect::new(50.0 + text_w + 18.0, cy - 18.0, 36.0, 36.0), false);
        let w = ui.text_size(t("Stop watching"), 13.0, true).x + 32.0;
        // The camera, as the spectator chooses it (between the reactions
        // and the way out, when there is room).
        let from = 50.0 + text_w + 18.0 + 36.0 + 18.0;
        let room = sw - w - 14.0 - 18.0 - from;
        if room >= 240.0 {
            let cams = Rect::new(from, cy - 15.0, room.min(380.0), 30.0);
            let tv = self.tv_default;
            self.observer.draw_cameras(ui, cams, tv);
        }
        ui.button(Rect::new(sw - w - 14.0, cy - 18.0, w, 36.0), t("Stop watching"), true)
    }

    fn draw_bottom_bar(&mut self, ui: &mut Ui, m: &mut Match) {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let bar = Rect::new(0.0, sh - BOTTOM_BAR, sw, BOTTOM_BAR);
        draw_bar(bar, false);
        ui.block(bar);
        // The game is over (the result panel has the way on), or no one here
        // holds a cue (a recorded game, the computer against itself): no
        // cue to set up.
        let anyone_here = m.controllers.contains(&Controller::Human);
        if matches!(m.phase, Phase::GameOver) || !anyone_here {
            self.popover = None;
            return;
        }
        let my_turn = m.is_human_turn() && !self.paused;

        // Cue settings on the left, the power bar in the middle and the shoot
        // button on the right: the bar reads evenly from edge to edge.
        let gap = 8.0;
        let bh = bar.h - 18.0;
        let shoot = Rect::new(sw - 10.0 - bh * 1.15, bar.y + 9.0, bh * 1.15, bh);
        // Narrow windows shrink the buttons before anything overlaps.
        let bw = (sw * 0.075).clamp(62.0, 84.0);
        let pops = [Pop::Elevation, Pop::Spin];
        let bx0 = 10.0;
        let mut anchors = [0.0f32; 2];
        // Not this player's turn: the cue settings stay in view, dimmed and
        // quiet, so nothing jumps when the turn comes back.
        let opacity = crate::paint::opacity();
        let idle = (!my_turn).then(|| {
            crate::paint::set_opacity(opacity * 0.45);
            ui.suspend_input()
        });
        let mut toggles = [Rect::new(0.0, 0.0, 0.0, 0.0); 2];
        for (k, pop) in pops.into_iter().enumerate() {
            let r = Rect::new(bx0 + k as f32 * (bw + gap), bar.y + 9.0, bw, bh);
            toggles[k] = r;
            anchors[k] = r.x + r.w / 2.0;
            let open = self.popover == Some(pop);
            if ui.toggle(r, open) {
                self.popover = if open { None } else { Some(pop) };
            }
            draw_pop_icon(ui, m, pop, r);
        }
        // Fine aim right on the bar: one click on an arrow, one tiny step.
        let fw = (sw * 0.17).clamp(150.0, 210.0);
        let fine = Rect::new(bx0 + pops.len() as f32 * (bw + gap), bar.y + 9.0, fw, bh);
        self.fine_aim_inline(ui, m, fine, my_turn);
        if let Some(saved) = idle {
            ui.resume_input(saved);
            crate::paint::set_opacity(opacity);
        }
        let left_end = fine.x + fine.w + gap;
        // The power bar, centred in the space between.
        let room = (shoot.x - 24.0) - (left_end + 24.0);
        // Shorter than the room there is: a long bar made a small change
        // of pace a long drag.
        let tw = (room * 0.78).clamp(120.0, 400.0);
        let px0 = left_end + 24.0 + (room - tw) / 2.0;
        let track = Rect::new(px0, bar.y + bar.h / 2.0 - 9.0, tw, 18.0);

        // Every turn (a clock run out or a choice passes one too).
        let key = (m.games_played, m.game.shots_taken(), m.turns_begun);
        // Any shot (by anyone) closes the open cue-setting panel.
        if self.panel_key != Some(key) {
            self.panel_key = Some(key);
            self.popover = None;
        }
        if my_turn && self.power_key != Some(key) {
            self.power_key = Some(key);
            // Every turn starts from the same place, the middle of the bar
            // (the break from a firm stroke). Nothing on the bar tells what the last stroke was
            // played with: judging the pace is part of the game.
            let power = if m.game.is_break() { BREAK_POWER } else { START_POWER };
            m.aim.power = power;
            m.aim.charging = power > 0.02;
        }
        let can_shoot = my_turn && !m.is_placing_cue_ball();
        if shoot_button(ui, shoot, can_shoot && m.aim.power > 0.02) {
            if m.game.must_call_pocket() && m.called_pocket.is_none() {
                m.notify(t("Call a pocket for the 8-ball first: click a pocket."), true);
            } else if m.shoot() {
                self.popover = None;
            }
        }
        if can_shoot {
            self.stroke(ui, m, track);
        } else {
            self.stroking = false;
            draw_power_track(track, if m.aim.charging { m.aim.power } else { 0.0 }, false, false);
        }
        // Charging with the keyboard: what letting go does, and the way out.
        if can_shoot && m.aim.charging && keys::down(Action::Shoot) {
            let key = keys::current().short(Action::Shoot);
            let hint = crate::i18n::tw("Let go of {key} to shoot · Esc cancels", &[("key", &key)]);
            let w = ui.text_size(&hint, 13.0, false).x + 20.0;
            let r = Rect::new(track.x + (track.w - w) / 2.0, track.y - 40.0, w, 24.0);
            crate::ui::rrect(r, 12.0, Color::new(0.0, 0.0, 0.0, 0.85));
            ui.text(&hint, r.x + 10.0, r.y + 5.0, 13.0, CREAM, false);
        }
        // A first game: where to aim, then the power, then the shot.
        if m.coach && can_shoot && self.popover.is_none() {
            let time = get_time() as f32;
            coach_mark(
                ui,
                vec2(sw / 2.0, bar.y - 70.0),
                t("Drag on the table to aim, or click a ball"),
                None,
                time,
            );
            coach_mark(
                ui,
                vec2(track.x + track.w / 2.0, track.y - 12.0),
                t("Set the power"),
                Some(1),
                time,
            );
            coach_mark(
                ui,
                vec2(shoot.x + shoot.w / 2.0, shoot.y - 8.0),
                t("Shoot"),
                Some(2),
                time + 0.4,
            );
        }

        // Development aid: SNOOK_LEGEND_ELEVATION=<degrees> keeps the
        // elevation panel open at that elevation.
        if my_turn && let Some(deg) = crate::dev::var("SNOOK_LEGEND_ELEVATION") {
            self.popover = Some(Pop::Elevation);
            m.aim.elevation = deg.parse::<f64>().unwrap_or(0.0).to_radians();
        }
        // The open panel.
        if let Some(pop) = self.popover {
            // Closed as soon as the shot is taken (button, Space or online),
            // and when the turn passes.
            if !my_turn {
                self.popover = None;
                return;
            }
            let w = 250.0;
            let h = match pop {
                Pop::Spin => 186.0,
                Pop::Elevation => 158.0,
            };
            let k = match pop {
                Pop::Elevation => 0,
                Pop::Spin => 1,
            };
            let x = crate::ui::clamp_soft(anchors[k] - w / 2.0, 10.0, sw - w - 10.0);
            let r = Rect::new(x, bar.y - h - 8.0, w, h);
            // Grows out of its button in the bar.
            let key = crate::ui::motion::mix(crate::ui::motion::id("hud-pop"), k as u64);
            let fx = ui.popover_in(key, vec2(anchors[k], bar.y));
            ui.panel(r);
            match pop {
                Pop::Spin => self.spin_panel(ui, m, r),
                Pop::Elevation => elevation_panel(ui, m, r),
            }
            ui.pop_fx(fx);
            // A press anywhere else closes it (the setting stays; the press
            // goes on to the table or the bar).
            if ui.pressed_outside(&[r, toggles[0], toggles[1]]) {
                self.popover = None;
            }
        }
    }

    /// The power bar: drag (or click) to set the power. Shooting is a separate
    /// button, so the player can set everything up first.
    fn stroke(&mut self, ui: &mut Ui, m: &mut Match, track: Rect) {
        let hit = Rect::new(track.x - 10.0, track.y - 14.0, track.w + 20.0, track.h + 28.0);
        let drag = ui.drag_area(90, hit);
        if let crate::ui::Drag::Held(p) | crate::ui::Drag::Released(p) = drag {
            let power = ((p.x - track.x) / track.w).clamp(0.0, 1.0);
            m.aim.power = power;
            m.aim.charging = power > 0.02;
        }
        let hot = matches!(drag, crate::ui::Drag::Held(_) | crate::ui::Drag::Hover);
        // The wheel over the bar: one percent a notch, for fine control.
        if matches!(drag, crate::ui::Drag::Hover) {
            let wheel = mouse_wheel().1;
            if wheel != 0.0 {
                let power = (m.aim.power + wheel.signum() * 0.01).clamp(0.0, 1.0);
                m.aim.power = power;
                m.aim.charging = power > 0.02;
            }
        }
        draw_power_track(track, m.aim.power, hot, true);
        if hot && !matches!(drag, crate::ui::Drag::Held(_)) {
            ui.overlay(move |ui| {
                let hint = t("Drag to set the power");
                let w = ui.text_size(hint, 13.0, false).x + 20.0;
                let r = Rect::new(track.x + (track.w - w) / 2.0, track.y - 40.0, w, 24.0);
                crate::ui::rrect(r, 12.0, Color::new(0.0, 0.0, 0.0, 0.85));
                ui.text(hint, r.x + 10.0, r.y + 5.0, 13.0, CREAM, false);
            });
        }
    }

    fn spin_panel(&mut self, ui: &mut Ui, m: &mut Match, r: Rect) {
        ui.text(t("Cue ball contact"), r.x + 12.0, r.y + 8.0, 14.0, GOLD, true);
        let mut tip = (m.aim.tip.x as f32, m.aim.tip.y as f32);
        ui.spin_ball(1, vec2(r.x + r.w / 2.0, r.y + 92.0), 56.0, &mut tip);
        m.aim.tip = ::glam::DVec2::new(tip.0 as f64, tip.1 as f64);
        let label = match (tip.1 > 0.15, tip.1 < -0.15) {
            (true, _) => t("follow"),
            (_, true) => t("draw"),
            _ => t("stun"),
        };
        let side = if tip.0 > 0.15 {
            t(" · right english")
        } else if tip.0 < -0.15 {
            t(" · left english")
        } else {
            ""
        };
        if ui.button(Rect::new(r.x + 12.0, r.y + r.h - 30.0, 62.0, 22.0), t("Center"), true) {
            m.aim.tip = ::glam::DVec2::ZERO;
        }
        ui.text(&format!("{label}{side}"), r.x + 82.0, r.y + r.h - 27.0, 13.0, MUTED, false);
    }

    /// Fine aim, built into the bar: the arrows turn the cue by one tiny
    /// step per click (holding doesn't repeat); the dial in between can be
    /// dragged for slightly bigger moves.
    fn fine_aim_inline(&mut self, ui: &mut Ui, m: &mut Match, r: Rect, enabled: bool) {
        crate::ui::rrect(r, 8.0, Color::new(0.12, 0.08, 0.05, 1.0));
        crate::ui::rrect_lines(r, 8.0, 1.2, crate::ui::BORDER);
        let label = t("Fine aim");
        let size = ui.fit(12.0, label, r.w - 16.0);
        ui.text_centered(label, r.x + r.w / 2.0, r.y + r.h - size - 6.0, size, CREAM, true);
        let row = Rect::new(r.x + 6.0, r.y + 6.0, r.w - 12.0, (r.h - size - 16.0).max(24.0));
        let aw = row.h.min(34.0);
        let step = 0.05f64.to_radians() * self.aim_sensitivity as f64;
        let left = Rect::new(row.x, row.y, aw, row.h);
        let right = Rect::new(row.x + row.w - aw, row.y, aw, row.h);
        if ui.button(left, "◀", enabled) {
            m.aim.yaw += step;
        }
        if ui.button(right, "▶", enabled) {
            m.aim.yaw -= step;
        }
        let dial = Rect::new(left.x + aw + 4.0, row.y, row.w - 2.0 * aw - 8.0, row.h);
        let drag = if enabled { ui.drag_area(82, dial) } else { crate::ui::Drag::Idle };
        match drag {
            crate::ui::Drag::Held(p) => {
                if let Some(last) = self.fine_last {
                    m.aim.yaw -= ((p.x - last) as f64) * 0.00025 * self.aim_sensitivity as f64;
                }
                self.fine_last = Some(p.x);
            }
            _ => self.fine_last = None,
        }
        crate::ui::rrect(dial, 6.0, Color::new(0.05, 0.035, 0.025, 1.0));
        let spacing = 8.0;
        let offset = ((m.aim.yaw * 4000.0) as f32).rem_euclid(spacing);
        let mut tx = dial.x + offset;
        let a = if enabled { 0.8 } else { 0.3 };
        while tx < dial.x + dial.w {
            let edge = ((tx - dial.x) / dial.w - 0.5).abs() * 2.0;
            let alpha = a * (1.0 - edge * edge);
            draw_line(
                tx,
                dial.y + 5.0,
                tx,
                dial.y + dial.h - 5.0,
                1.5,
                Color::new(0.9, 0.8, 0.6, alpha),
            );
            tx += spacing;
        }
        let cx = dial.x + dial.w / 2.0;
        draw_line(
            cx,
            dial.y + 2.0,
            cx,
            dial.y + dial.h - 2.0,
            2.0,
            Color::new(GOLD.r, GOLD.g, GOLD.b, a),
        );
        let hot = matches!(drag, crate::ui::Drag::Held(_) | crate::ui::Drag::Hover);
        crate::ui::rrect_lines(dial, 6.0, 1.2, if hot { GOLD } else { crate::ui::BORDER });
        if hot {
            ui.tooltip(t("Fine aim: click the arrows, or drag here"), r);
        }
        m.aim.yaw = m.aim.yaw.rem_euclid(std::f64::consts::TAU);
    }
}

/// The cue's elevation, seen from the side: the cue ball on the cloth and
/// the cue behind it, with a curved arrow around the ball. Dragging (the
/// cue, or anywhere in the picture) raises or lowers the cue; the wheel
/// nudges it. No numbers: the player judges by eye.
fn elevation_panel(ui: &mut Ui, m: &mut Match, r: Rect) {
    let max = m.game.params().max_elevation as f32;
    ui.text(t("Elevation"), r.x + 12.0, r.y + 8.0, 14.0, GOLD, true);
    let keys = [keys::short(Action::ElevationUp), keys::short(Action::ElevationDown)]
        .into_iter()
        .filter(|k| !k.is_empty())
        .collect::<Vec<_>>()
        .join(" / ");
    let hint = if keys.is_empty() {
        t("Drag the cue: jump & massé").to_string()
    } else {
        tw("Drag the cue · {keys}", &[("keys", &keys)])
    };
    let hint = ui.ellipsize(&hint, r.w - 24.0, 12.0);
    ui.text(&hint, r.x + 12.0, r.y + 28.0, 12.0, MUTED, false);

    // The picture.
    let scene = Rect::new(r.x + 8.0, r.y + 48.0, r.w - 16.0, r.h - 56.0);
    let ball_r = 15.0;
    let floor = scene.y + scene.h - 10.0;
    let ball = vec2(scene.x + scene.w - 46.0, floor - ball_r);
    let drag = ui.drag_area(2, scene);
    if let crate::ui::Drag::Held(p) | crate::ui::Drag::Released(p) = drag {
        // The cue points from the pointer to the ball: the pointer above
        // the ball's centre (screen y grows downwards) raises the butt.
        let d = ball - p;
        let a = if d.x <= 1.0 { max } else { (d.y / d.x).atan().max(0.0) };
        // Below the cloth line, or the pointer in front of the ball: level.
        let a = if p.y > ball.y && d.x > 0.0 { 0.0 } else { a };
        m.aim.elevation = (a.clamp(0.0, max)) as f64;
    }
    let hot = !matches!(drag, crate::ui::Drag::Idle);
    if matches!(drag, crate::ui::Drag::Hover) {
        let wheel = mouse_wheel().1;
        if wheel != 0.0 {
            let a = m.aim.elevation as f32 + wheel.signum() * 1f32.to_radians();
            m.aim.elevation = a.clamp(0.0, max) as f64;
        }
    }
    let a = (m.aim.elevation as f32).clamp(0.0, max);

    // The cloth and the cushion's shadow line.
    crate::ui::rrect(
        Rect::new(scene.x, floor, scene.w, 6.0),
        2.0,
        Color::new(0.16, 0.45, 0.24, 1.0),
    );
    draw_line(scene.x, floor, scene.x + scene.w, floor, 1.0, Color::new(0.35, 0.7, 0.42, 1.0));

    // The curved arrow: from level up to the steepest the cue goes.
    let arc_r = ball_r + 34.0;
    let gold = Color::new(GOLD.r, GOLD.g, GOLD.b, if hot { 0.9 } else { 0.55 });
    let steps = 28;
    let point = |ang: f32| ball + vec2(-ang.cos(), -ang.sin()) * arc_r;
    for k in 0..steps {
        let a0 = 0.04 + (max - 0.04) * k as f32 / steps as f32;
        let a1 = 0.04 + (max - 0.04) * (k + 1) as f32 / steps as f32;
        let (p0, p1) = (point(a0), point(a1));
        draw_line(p0.x, p0.y, p1.x, p1.y, 2.0, gold);
    }
    // Arrowheads at both ends (it goes both ways).
    for (ang, dir) in [(max, 1.0f32), (0.04, -1.0)] {
        let tip = point(ang);
        // Along the arc, the way it is travelling at this end.
        let along = vec2(ang.sin(), -ang.cos()) * dir;
        let side = vec2(-along.y, along.x);
        let back = tip - along * 8.0;
        crate::paint::draw_triangle(tip + along * 2.0, back + side * 5.0, back - side * 5.0, gold);
    }

    // The cue, from just behind the ball, raised by `a` (its butt runs out
    // of the picture).
    crate::ui::motion::push_clip(scene);
    let dir = vec2(-a.cos(), -a.sin());
    let tip = ball + dir * (ball_r + 4.0);
    let length = (scene.w - 30.0).min(200.0);
    let butt = tip + dir * length;
    let normal = vec2(-dir.y, dir.x);
    // Shadow on the cloth.
    let shadow = Color::new(0.0, 0.0, 0.0, 0.25);
    draw_line(tip.x, floor + 2.0, butt.x.max(scene.x), floor + 2.0, 3.0, shadow);
    // The shaft tapers from the butt to the tip: a quad in two triangles.
    let (wt, wb) = (2.0, 4.5);
    let wood = Color::new(0.86, 0.70, 0.46, 1.0);
    let dark = Color::new(0.30, 0.17, 0.09, 1.0);
    let ferrule = tip + dir * 7.0;
    let joint = tip + dir * (length * 0.62);
    let quad = |a: Vec2, b: Vec2, wa: f32, wb: f32, c: Color| {
        crate::paint::draw_triangle(a + normal * wa, a - normal * wa, b + normal * wb, c);
        crate::paint::draw_triangle(a - normal * wa, b - normal * wb, b + normal * wb, c);
    };
    let w_at = |p: Vec2| wt + (wb - wt) * (p - tip).length() / length;
    quad(ferrule, joint, w_at(ferrule), w_at(joint), wood);
    quad(joint, butt, w_at(joint), wb, dark);
    quad(tip, ferrule, wt, w_at(ferrule), Color::new(0.96, 0.95, 0.9, 1.0));
    // The chalked tip.
    draw_circle(tip.x, tip.y, wt + 0.6, Color::new(0.25, 0.45, 0.8, 1.0));
    // A soft highlight along the shaft.
    let hl = Color::new(1.0, 1.0, 1.0, 0.25);
    let h0 = ferrule + normal * 1.0;
    let h1 = joint + normal * (w_at(joint) * 0.5);
    draw_line(h0.x, h0.y, h1.x, h1.y, 1.0, hl);
    crate::ui::motion::pop_clip();

    // The cue ball, over the cue's end.
    draw_circle(ball.x + 2.0, floor - 1.0, ball_r * 0.9, Color::new(0.0, 0.0, 0.0, 0.25));
    draw_circle(ball.x, ball.y, ball_r, Color::new(0.95, 0.93, 0.88, 1.0));
    draw_circle(ball.x - 5.0, ball.y - 5.0, 4.5, Color::new(1.0, 1.0, 1.0, 0.75));
    draw_circle_lines(ball.x, ball.y, ball_r, 1.0, Color::new(0.45, 0.4, 0.33, 1.0));

    // Back to level.
    let level = Rect::new(r.x + r.w - 70.0, r.y + 6.0, 58.0, 20.0);
    if ui.button(level, t("Level"), a > 0.001) {
        m.aim.elevation = 0.0;
    }
    if hot {
        ui.tooltip(t("Drag to raise or lower the cue"), scene);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pop {
    Spin,
    Elevation,
}

/// A small picture on each cue-setting button showing the current value.
fn draw_pop_icon(ui: &mut Ui, m: &Match, pop: Pop, r: Rect) {
    let c = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0 - 7.0);
    let label = match pop {
        Pop::Spin => {
            let rad = 15.0;
            draw_circle(c.x, c.y, rad, Color::new(0.95, 0.93, 0.88, 1.0));
            draw_circle_lines(c.x, c.y, rad, 1.0, Color::new(0.4, 0.33, 0.25, 1.0));
            let d = c + vec2(m.aim.tip.x as f32, -m.aim.tip.y as f32) * rad * 0.85;
            draw_circle(d.x, d.y, 3.5, Color::new(0.8, 0.15, 0.15, 1.0));
            t("Spin")
        }
        Pop::Elevation => {
            let a = m.aim.elevation as f32;
            let base = c + vec2(14.0, 9.0);
            draw_line(
                c.x - 22.0,
                base.y + 3.0,
                c.x + 22.0,
                base.y + 3.0,
                1.5,
                Color::new(0.4, 0.7, 0.4, 1.0),
            );
            draw_circle(base.x, base.y - 3.0, 5.0, Color::new(0.95, 0.93, 0.88, 1.0));
            let tip = base + vec2(-7.0, -3.0);
            let butt = tip + vec2(-a.cos(), -a.sin()) * 30.0;
            draw_line(tip.x, tip.y, butt.x, butt.y, 2.5, Color::new(0.85, 0.7, 0.45, 1.0));
            t("Elevation")
        }
    };
    let size = ui.fit(12.0, label, r.w - 8.0);
    ui.text_centered(label, r.x + r.w / 2.0, r.y + r.h - size - 6.0, size, CREAM, true);
}

fn draw_power_track(track: Rect, p: f32, hot: bool, enabled: bool) {
    crate::ui::rrect(track, 9.0, Color::new(0.04, 0.03, 0.02, 1.0));
    // Ramp from green to red underneath, faint; bright where filled. Fine
    // slices, so the colours run into each other without steps, and the
    // strip's ends are round like the track it lies in.
    let inner = Rect::new(track.x + 4.0, track.y + 5.0, track.w - 8.0, track.h - 10.0);
    let round = inner.h / 2.0;
    let slices = (inner.w / 2.0).ceil().max(1.0) as usize;
    for k in 0..slices {
        let t0 = k as f32 / slices as f32;
        let t1 = (k + 1) as f32 / slices as f32;
        let mid = (t0 + t1) / 2.0;
        let color = if mid < 0.5 {
            Color::new(0.25 + 1.3 * mid, 0.85, 0.3, 1.0)
        } else {
            Color::new(0.95, 0.85 - 1.2 * (mid - 0.5), 0.25, 1.0)
        };
        let a = if t0 < p {
            1.0
        } else if enabled {
            0.22
        } else {
            0.1
        };
        let x0 = inner.x + t0 * inner.w;
        let x1 = inner.x + t1.min(if t0 < p { p.max(t0) } else { t1 }) * inner.w;
        // How far this slice is from the nearer end, for the round ends.
        let from_end = (mid * inner.w).min((1.0 - mid) * inner.w);
        let half = if from_end < round {
            (round * round - (round - from_end) * (round - from_end)).max(0.0).sqrt()
        } else {
            round
        };
        draw_rectangle(
            x0,
            inner.y + round - half,
            (x1 - x0).max(0.0),
            2.0 * half,
            Color::new(color.r, color.g, color.b, a),
        );
    }
    // The outline goes under the knob.
    crate::ui::rrect_lines(track, 9.0, 1.2, if hot { GOLD } else { crate::ui::BORDER });
    // The knob.
    let kx = track.x + 4.0 + p * (track.w - 8.0);
    let ky = track.y + track.h / 2.0;
    draw_circle(kx, ky + 1.5, 13.0, Color::new(0.0, 0.0, 0.0, 0.35));
    draw_circle(
        kx,
        ky,
        12.0,
        if enabled { Color::new(0.93, 0.92, 0.88, 1.0) } else { Color::new(0.5, 0.48, 0.45, 1.0) },
    );
    draw_circle(kx - 3.5, ky - 3.5, 4.0, Color::new(1.0, 1.0, 1.0, 0.7));
    // No number: the player judges the power by eye.
}

/// A full-width bar at the top or bottom of the screen.
fn draw_bar(r: Rect, top: bool) {
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.045, 0.03, 0.02, 0.94));
    let edge = if top { r.y + r.h - 1.0 } else { r.y };
    draw_line(r.x, edge, r.x + r.w, edge, 1.5, Color::new(0.45, 0.31, 0.16, 0.9));
}

const TOP_BAR: f32 = crate::scoreboard::AVATAR + 8.0;
const BOTTOM_BAR: f32 = 78.0;
/// Where the power bar starts on the break.
const BREAK_POWER: f32 = 0.9;
/// Where it starts on every other turn.
const START_POWER: f32 = 0.5;

/// The game log inside the bottom bar: the last few things that happened.
/// The shoot button: a cue striking a ball, on green. Unmistakable, no text.
fn shoot_button(ui: &mut Ui, r: Rect, enabled: bool) -> bool {
    ui.block(r);
    let hover = enabled && r.contains(ui.mouse());
    let down = hover && is_mouse_button_down(MouseButton::Left);
    let bg = if !enabled {
        Color::new(0.11, 0.14, 0.11, 0.9)
    } else if down {
        Color::new(0.10, 0.42, 0.20, 1.0)
    } else if hover {
        Color::new(0.20, 0.66, 0.34, 1.0)
    } else {
        Color::new(0.13, 0.52, 0.27, 1.0)
    };
    if enabled {
        let pulse = 0.5 + 0.5 * (get_time() as f32 * 3.0).sin();
        crate::ui::rrect(
            Rect::new(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0),
            12.0,
            Color::new(0.3, 0.9, 0.45, 0.10 + 0.14 * pulse),
        );
    }
    if enabled {
        crate::ui::rrect_gradient(r, 10.0, crate::ui::shade(bg, 0.10), crate::ui::shade(bg, -0.22));
    } else {
        crate::ui::rrect(r, 10.0, bg);
    }
    let edge =
        if hover { GOLD } else { Color::new(0.35, 0.75, 0.5, if enabled { 1.0 } else { 0.3 }) };
    crate::ui::rrect_lines(r, 10.0, 1.5, edge);

    // The picture: a ball on the right, the cue coming in from the lower left
    // with its tip touching the ball, and a few impact lines.
    let a = if enabled { 1.0 } else { 0.35 };
    let s = r.h.min(r.w);
    let ball = vec2(r.x + r.w * 0.64, r.y + r.h * 0.42);
    let br = s * 0.17;
    let dir = vec2(1.0, -0.55).normalize();
    let tip = ball - dir * (br + 1.5);
    let butt = tip - dir * s * 0.62;
    // Shaft (light), wrap (dark) and the blue tip.
    draw_line(butt.x, butt.y, tip.x, tip.y, s * 0.075, Color::new(0.86, 0.74, 0.52, a));
    let wrap = butt + dir * s * 0.16;
    draw_line(butt.x, butt.y, wrap.x, wrap.y, s * 0.085, Color::new(0.20, 0.12, 0.08, a));
    let ferrule = tip - dir * s * 0.05;
    draw_line(ferrule.x, ferrule.y, tip.x, tip.y, s * 0.075, Color::new(0.35, 0.55, 0.9, a));
    // Impact lines on the far side of the ball.
    for k in [-1.0f32, 0.0, 1.0] {
        let n = vec2(-dir.y, dir.x);
        let from = ball + dir * (br + 3.0) + n * k * br * 0.7;
        let to = from + dir * s * 0.12;
        draw_line(from.x, from.y, to.x, to.y, 1.6, Color::new(1.0, 1.0, 1.0, 0.75 * a));
    }
    draw_circle(ball.x, ball.y + 1.5, br, Color::new(0.0, 0.0, 0.0, 0.3 * a));
    draw_circle(ball.x, ball.y, br, Color::new(0.97, 0.96, 0.92, a));
    draw_circle(
        ball.x - br * 0.35,
        ball.y - br * 0.35,
        br * 0.32,
        Color::new(1.0, 1.0, 1.0, 0.85 * a),
    );
    // Pressed on the button too: a power drag let go over it is no shot.
    let clicked = enabled && ui.activated(r, hover);
    if hover {
        ui.overlay(move |ui| {
            let label = t("Shoot!");
            let w = ui.text_size(label, 13.0, true).x + 20.0;
            let tr = Rect::new(
                (r.x + r.w / 2.0 - w / 2.0).min(crate::ui::vw() - w - 6.0),
                r.y - 32.0,
                w,
                24.0,
            );
            crate::ui::rrect(tr, 12.0, Color::new(0.0, 0.0, 0.0, 0.85));
            ui.text(label, tr.x + 10.0, tr.y + 5.0, 13.0, CREAM, true);
        });
    }
    clicked
}

impl Hud {
    /// Watching: how hard the shot being lined up is, above the bottom bar
    /// (once the shooter's aim settles). It fades in and out and stays put
    /// while the aim moves (dimmed until the new answer comes): the aim
    /// arrives over the network and never quite stops, and an answer that
    /// went off at every twitch made the panel blink.
    fn draw_shot_assessment(&mut self, ui: &mut Ui, m: &Match) {
        let dt = get_frame_time().min(0.1);
        let latest = m.online.as_ref().and_then(|o| o.assessor.latest());
        let ease = |rate: f32| 1.0 - (-dt * rate).exp();
        match latest {
            Some((a, fresh)) => {
                if self.assess_shown.is_none() || self.assess_fade <= 0.0 {
                    // Appears with its chance (nothing to slide from).
                    self.assess_chance = a.chance as f32;
                }
                self.assess_shown = Some(a);
                self.assess_fade = (self.assess_fade + dt / 0.2).min(1.0);
                let bright = if fresh { 1.0 } else { 0.55 };
                self.assess_bright += (bright - self.assess_bright) * ease(10.0);
            }
            None => {
                self.assess_fade = (self.assess_fade - dt / 0.25).max(0.0);
                if self.assess_fade <= 0.0 {
                    self.assess_shown = None;
                }
            }
        }
        let Some(a) = self.assess_shown else { return };
        self.assess_chance += (a.chance as f32 - self.assess_chance) * ease(8.0);
        // Eased in and out.
        let f = self.assess_fade;
        let alpha = f * f * (3.0 - 2.0 * f);
        if alpha <= 0.0 {
            return;
        }
        let opacity = crate::paint::opacity();
        crate::paint::set_opacity(opacity * alpha);

        let pct = (a.chance * 100.0).round() as u32;
        let (word, color) = match pct {
            80.. => (t("Easy shot"), Color::new(0.45, 0.95, 0.55, 1.0)),
            50..=79 => (t("Fair shot"), Color::new(0.95, 0.85, 0.35, 1.0)),
            20..=49 => (t("Hard shot"), Color::new(1.0, 0.6, 0.25, 1.0)),
            _ => (t("Very hard shot"), Color::new(1.0, 0.4, 0.35, 1.0)),
        };
        let headline = if a.pot.is_some() {
            format!("{word} · {}", tw("~{n}% chance", &[("n", &pct)]))
        } else {
            format!("{} · {}", t("No ball goes down as aimed"), tw("~{n}% chance", &[("n", &pct)]))
        };
        let mut details = Vec::new();
        match a.cut_deg {
            Some(cut) if cut < 3.0 => details.push(t("straight in").to_string()),
            Some(cut) => details.push(tw("{n}° cut", &[("n", &(cut.round() as u32))])),
            None => {}
        }
        match a.cushions {
            0 => {}
            1 => details.push(t("off one cushion").to_string()),
            n => details.push(tw("off {n} cushions", &[("n", &n)])),
        }
        let details = details.join(" · ");
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let w = crate::ui::clamp_soft(
            ui.text_size(&headline, 14.0, true).x.max(ui.text_size(&details, 12.0, false).x) + 40.0,
            220.0,
            sw - 24.0,
        );
        let h = if details.is_empty() { 34.0 } else { 52.0 };
        // Rises a few pixels as it fades in.
        let lift = (1.0 - alpha) * 6.0;
        let r = Rect::new((sw - w) / 2.0, sh - BOTTOM_BAR - h - 10.0 + lift, w, h);
        crate::ui::rrect(r, 8.0, Color::new(0.05, 0.06, 0.05, 0.82));
        // The text dims while a new answer is worked out; the panel stays.
        let b = self.assess_bright;
        let dim = |c: Color| Color::new(c.r, c.g, c.b, c.a * b);
        // The chance as a thin bar along the bottom edge.
        let bar = Rect::new(r.x + 10.0, r.y + r.h - 5.0, r.w - 20.0, 3.0);
        crate::ui::rrect(bar, 1.5, Color::new(1.0, 1.0, 1.0, 0.12));
        let filled = bar.w * self.assess_chance.clamp(0.0, 1.0);
        if filled > 0.5 {
            crate::ui::rrect(Rect::new(bar.x, bar.y, filled, bar.h), 1.5, dim(color));
        }
        let (headline, size) = ui.fitted(&headline, 14.0, r.w - 20.0, true);
        ui.text_centered(&headline, r.x + r.w / 2.0, r.y + 8.0, size, dim(color), true);
        if !details.is_empty() {
            let (details, size) = ui.fitted(&details, 12.0, r.w - 20.0, false);
            ui.text_centered(&details, r.x + r.w / 2.0, r.y + 28.0, size, dim(CREAM), false);
        }
        crate::paint::set_opacity(opacity);
    }
}

/// A speech bubble with `text`, its tail up at `anchor` (under the
/// speaker's card), fading in and out over its life.
fn draw_banter(ui: &Ui, text: &str, age: f32, anchor: Vec2) {
    let life = crate::game::BANTER_TIME;
    let alpha = (age / 0.25).min(1.0) * ((life - age) / 0.5).clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let size = 14.0;
    let max_w = 300.0f32.min(crate::ui::vw() - 24.0);
    let lines = ui.wrap(text, max_w - 28.0, size);
    let w = lines.iter().map(|l| ui.text_size(l, size, false).x).fold(0.0, f32::max) + 28.0;
    let h = lines.len() as f32 * (size + 5.0) + 18.0;
    // Pops in: a little bounce at the start.
    let pop = if age < 0.25 { 0.9 + 0.1 * (age / 0.25) } else { 1.0 };
    let top = anchor.y + 16.0 + (1.0 - pop) * 10.0;
    let x = crate::ui::clamp_soft(anchor.x - w / 2.0, 12.0, crate::ui::vw() - w - 12.0);
    let r = Rect::new(x, top, w, h);
    let paper = Color::new(0.98, 0.96, 0.9, 0.97 * alpha);
    let edge = Color::new(0.25, 0.18, 0.1, 0.9 * alpha);
    // Shadow, tail, body.
    crate::ui::rrect(
        Rect::new(r.x + 2.0, r.y + 3.0, r.w, r.h),
        12.0,
        Color::new(0.0, 0.0, 0.0, 0.3 * alpha),
    );
    let tail_x = crate::ui::clamp_soft(anchor.x, r.x + 18.0, r.x + r.w - 18.0);
    crate::paint::draw_triangle(
        vec2(tail_x - 9.0, r.y + 1.0),
        vec2(tail_x + 9.0, r.y + 1.0),
        vec2(tail_x, r.y - 11.0),
        edge,
    );
    crate::ui::rrect(r, 12.0, paper);
    crate::ui::rrect_lines(r, 12.0, 1.5, edge);
    crate::paint::draw_triangle(
        vec2(tail_x - 7.0, r.y + 2.0),
        vec2(tail_x + 7.0, r.y + 2.0),
        vec2(tail_x, r.y - 8.5),
        paper,
    );
    let ink = Color::new(0.16, 0.11, 0.07, alpha);
    for (k, l) in lines.iter().enumerate() {
        ui.text_centered(l, r.x + r.w / 2.0, r.y + 9.0 + k as f32 * (size + 5.0), size, ink, false);
    }
}

#[cfg(test)]
mod chart_tests {
    use super::momentum;
    use crate::game::ShotMark;

    #[test]
    fn momentum_follows_the_balls_and_the_fouls() {
        let mark = |shooter, made, foul| ShotMark { shooter, cue: (0.0, 0.0), made, foul };
        let v =
            momentum(&[mark(0, 2, false), mark(0, 0, false), mark(1, 1, false), mark(1, 0, true)]);
        assert_eq!(v, vec![0.0, 2.0, 2.0, 1.0, 2.0]);
    }
}
