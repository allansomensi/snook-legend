//! Playing the computer: a free game against any opponent (a personality
//! or a real player's ghost), or the ladder of ten.

use macroquad::prelude::*;
use snook_legend_core::personalities::PERSONALITIES;
use snook_legend_proto::ghost::GhostAnswer;
use snook_legend_proto::ladder;

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use crate::config::{AimGuide, Config};
use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::ladder::{Claim, Claimer, Progress};
use crate::online::{GHOST_TIMEOUT, Online};
use crate::opponents::{self, Opponent};
use crate::ranking::{HEADER_H, PAD, caps_y, screen_header_until, text_y};
use crate::ui::{ALERT, BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};
use crate::{MatchMode, MatchSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Free,
    Ladder,
    /// The daily challenge.
    Daily,
    /// Drills: short exercises with stars.
    Drills,
    /// A tournament against the computer's opponents.
    Cup,
}

/// The card picked on the free game's grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pick {
    Personality(usize),
    Ghost,
}

pub enum VersusAction {
    None,
    Back,
    Start(MatchSettings),
    /// To the sign-in form (for a ghost or the ladder's reward).
    SignIn,
    /// A session of this drill.
    Drill(String),
    /// The player's game of the tournament's round.
    StartCup(MatchSettings),
    /// The tournament's game that was left half way, picked up again.
    ResumeCup,
}

pub struct Versus {
    pub tab: Tab,
    pick: Pick,
    /// The rung shown on the ladder's right side.
    rung: Option<u8>,
    ghost_name: String,
    /// The ghost's name field was focused once already (on opening).
    ghost_focused: bool,
    /// A ghost to ask for once signed in.
    waiting: bool,
    /// Today's challenge was asked for (the day it was asked).
    daily_asked: Option<i64>,
    /// "Start" was pressed: waiting for the server.
    daily_starting: bool,
    /// The drill picked on the list.
    drill: usize,
    /// The drills were asked for (once per visit).
    drills_asked: bool,
    /// "Abandon the tournament" was pressed this frame.
    cup_abandon: bool,
    /// The seed of the game against the computer kept on disk (a
    /// tournament's game left half way is picked up from it).
    saved_seed: Option<u64>,
    /// A tournament's game was left and not kept: it was counted as lost.
    cup_forfeit: bool,
}

/// Text field ids.
const NAME_FIELD: u64 = 40;
const GHOST_FIELD: u64 = 41;
const GAP: f32 = 12.0;
const LABEL_H: f32 = 20.0;
const CONTROL_H: f32 = 34.0;
const GREEN_TEXT: Color = Color::new(0.45, 0.95, 0.55, 1.0);

impl Versus {
    pub fn new(tab: Tab, config: &Config) -> Versus {
        let index = PERSONALITIES
            .iter()
            .position(|p| p.code == opponents::chosen(&config.opponent).code)
            .unwrap_or(0);
        Versus {
            tab,
            pick: Pick::Personality(index),
            rung: None,
            ghost_name: String::new(),
            ghost_focused: false,
            waiting: false,
            daily_asked: None,
            daily_starting: false,
            drill: 0,
            drills_asked: false,
            cup_abandon: false,
            saved_seed: crate::saved::load().map(|s| s.seed),
            cup_forfeit: false,
        }
    }

    /// Opens on the ghost card, asking for `name`'s style.
    pub fn ghost_of(name: &str, config: &Config, online: &mut Online) -> Versus {
        let mut v = Versus::new(Tab::Free, config);
        v.pick = Pick::Ghost;
        v.ghost_name = name.to_string();
        v.ghost_focused = true;
        if online.signed_in() && !name.is_empty() {
            online.request_ghost(name);
        } else {
            v.waiting = !name.is_empty();
        }
        v
    }

    /// Sends the ghost request that waited for the sign-in.
    pub fn send_waiting(&mut self, online: &mut Online) {
        if std::mem::take(&mut self.waiting) {
            online.request_ghost(&self.ghost_name);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        ui: &mut Ui,
        config: &mut Config,
        online: &mut Online,
        progress: &Progress,
        claimer: &mut Claimer,
        cup: &mut Option<crate::cup::Cup>,
        medals: &crate::medals::MedalBook,
    ) -> VersusAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.5));
        let w = (sw - 40.0).min(1000.0);
        let h = (sh - 40.0).min(660.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        let mut action = VersusAction::None;
        let subtitle = match self.tab {
            Tab::Free => t("Each opponent has a style of their own").to_string(),
            Tab::Ladder => tw(
                "Tour: {n} of {total} beaten",
                &[("n", &progress.beaten), ("total", &ladder::RUNGS.len())],
            ),
            Tab::Daily => t("The same table for everyone, all day").to_string(),
            Tab::Drills => t("Exercises with stars").to_string(),
            Tab::Cup => t("Sixteen players: knockout or arena").to_string(),
        };
        // The tabs at the top right; the title keeps clear of them.
        let labels =
            [t("Free game"), t("Tour"), t("Tournament"), t("Daily challenge"), t("Drills")];
        // The tabs keep to the right, clear of the title: each as wide as
        // its label, all labels at one size.
        let tab_gap = 6.0;
        let natural: f32 = labels.iter().map(|l| ui.text_size(l, 15.0, true).x + 28.0).sum::<f32>()
            + tab_gap * (labels.len() - 1) as f32;
        let row_w = natural.min(r.w * 0.56);
        let tabs_x = r.x + r.w - PAD - row_w;
        if screen_header_until(
            ui,
            r,
            Icon::Robot,
            t("Against the computer"),
            Some(&subtitle),
            tabs_x - 12.0,
        ) || ui.escape()
        {
            action = VersusAction::Back;
        }
        let order = [Tab::Free, Tab::Ladder, Tab::Cup, Tab::Daily, Tab::Drills];
        let picked = order.iter().position(|t| *t == self.tab).unwrap_or(0);
        let row = Rect::new(tabs_x, r.y + 20.0, row_w, 36.0);
        if let (Some(k), _) = crate::tabrow::tabs(ui, row, &labels, &[], picked, tab_gap) {
            self.tab = order[k];
            ui.focus = None;
        }
        let body =
            Rect::new(r.x + PAD, r.y + HEADER_H + 8.0, r.w - 2.0 * PAD, r.h - HEADER_H - 8.0 - PAD);
        draw_line(r.x + PAD - 8.0, body.y - 6.0, r.x + r.w - PAD + 8.0, body.y - 6.0, 1.0, BORDER);
        let inner = match self.tab {
            Tab::Free => self.free(ui, config, online, body),
            Tab::Ladder => self.ladder(ui, config, online, progress, claimer, body),
            Tab::Daily => self.daily(ui, online, body),
            Tab::Drills => self.drills(ui, online, body),
            Tab::Cup => {
                let account = crate::medals::account_of(online, config);
                let shown = medals.shown(account.as_deref());
                let waiting = medals.unsynced(account.as_deref());
                self.cup(ui, config, cup, body, (shown, waiting, account.is_some()))
            }
        };
        if !matches!(inner, VersusAction::None) {
            action = inner;
        }
        action
    }

    // -----------------------------------------------------------------------
    // Free game.

    fn free(
        &mut self,
        ui: &mut Ui,
        config: &mut Config,
        online: &mut Online,
        body: Rect,
    ) -> VersusAction {
        let mut action = VersusAction::None;
        let grid_w = (body.w * 0.58).floor();
        let cols = if grid_w / 3.0 >= 165.0 { 3 } else { 2 };
        let count = PERSONALITIES.len() + 1;
        let rows = count.div_ceil(cols);
        let card_w = (grid_w - (cols - 1) as f32 * 10.0) / cols as f32;
        let card_h = ((body.h - (rows - 1) as f32 * 10.0) / rows as f32).min(100.0);
        let mut tip: Option<(String, Rect)> = None;
        for k in 0..count {
            let cell = Rect::new(
                body.x + (k % cols) as f32 * (card_w + 10.0),
                body.y + (k / cols) as f32 * (card_h + 10.0),
                card_w,
                card_h,
            );
            let pick = if k < PERSONALITIES.len() { Pick::Personality(k) } else { Pick::Ghost };
            let selected = self.pick == pick;
            let hover = ui.hovering(cell);
            card_frame(cell, selected, hover);
            ui.block(cell);
            let pr = (card_h * 0.3).min(25.0);
            let pc = vec2(
                cell.x + 12.0 + pr,
                cell.y + card_h / 2.0 - if card_h > 80.0 { 6.0 } else { 0.0 },
            );
            let tx = cell.x + 12.0 + 2.0 * pr + 12.0;
            let room = cell.x + cell.w - 10.0 - tx;
            let (name, epithet, strength) = match pick {
                Pick::Personality(i) => {
                    let p = &PERSONALITIES[i];
                    opponents::draw_person(p.code, pc, pr);
                    (p.name.to_string(), t(p.epithet).to_string(), Some(p.strength))
                }
                Pick::Ghost => {
                    let label = match &online.ghost {
                        Some(g) if !g.username.is_empty() => g.username.clone(),
                        _ => "?".to_string(),
                    };
                    opponents::draw_ghost(&label, pc, pr);
                    (t("Ghost").to_string(), t("Plays like a real player").to_string(), None)
                }
            };
            let name_color = if selected { GOLD } else { CREAM };
            let (name, size) = ui.fitted(&name, 16.0, room, true);
            ui.text(&name, tx, cell.y + 10.0, size, name_color, true);
            let lines = ui.wrap(&epithet, room, 12.0);
            // As many lines as fit above the strength pips (a nickname cut
            // short with room to spare under it read as a mistake).
            let max_lines = (((card_h - 17.0 - 31.0) / 15.0).floor() as usize).clamp(1, 3);
            for (n, line) in lines.iter().take(max_lines).enumerate() {
                let line = if n + 1 == max_lines && lines.len() > max_lines {
                    ui.ellipsize(&format!("{line}…"), room, 12.0)
                } else {
                    line.clone()
                };
                ui.text(&line, tx, cell.y + 31.0 + n as f32 * 15.0, 12.0, MUTED, false);
            }
            let pip_y = cell.y + card_h - 13.0;
            match strength {
                Some(s) => {
                    opponents::draw_strength(tx, pip_y, s, 7.0);
                    if hover {
                        let pips = Rect::new(tx, pip_y - 6.0, opponents::strength_width(7.0), 12.0);
                        if ui.hovering(pips) {
                            tip = Some((opponents::strength_label(s), pips));
                        }
                    }
                }
                None => {
                    crate::icons::draw(Icon::Globe, vec2(tx + 6.0, pip_y), 12.0, MUTED);
                    let note = ui.ellipsize(t("Online"), room - 18.0, 11.0);
                    ui.text(&note, tx + 16.0, text_y(pip_y, 11.0), 11.0, MUTED, false);
                }
            }
            if hover && ui.clicked(cell) && !selected {
                self.pick = pick;
                ui.focus = None;
                if let Pick::Personality(i) = pick {
                    config.opponent = PERSONALITIES[i].code.to_string();
                }
            }
        }

        // The details and the options on the right.
        let dx = body.x + grid_w + 24.0;
        let dw = body.x + body.w - dx;
        draw_line(
            dx - 12.0,
            body.y,
            dx - 12.0,
            body.y + body.h,
            1.0,
            Color::new(BORDER.r, BORDER.g, BORDER.b, 0.6),
        );
        let play_r = Rect::new(dx, body.y + body.h - 42.0, dw, 42.0);
        // The options, bottom up: name, aiming guide, time per shot.
        let field = LABEL_H + CONTROL_H;
        let options_h = 3.0 * field + 2.0 * GAP;
        let options_y = play_r.y - 16.0 - options_h;
        let mut enter = self.options(ui, config, Rect::new(dx, options_y, dw, options_h));
        let top = Rect::new(dx, body.y, dw, options_y - 14.0 - body.y);
        let opponent = match self.pick {
            Pick::Personality(i) => {
                let p = &PERSONALITIES[i];
                let o = Opponent::personality(p.code);
                details(ui, &o, p.name, t(p.epithet), t(p.description), p.strength, top);
                Some(o)
            }
            Pick::Ghost => {
                let (o, a, e) = self.ghost_panel(ui, online, top);
                enter &= !e;
                if let Some(a) = a {
                    action = a;
                }
                o
            }
        };
        let ready = opponent.is_some();
        if ready {
            if ui.primary_button(play_r, t("Play")) || (enter && ui.focus != Some(GHOST_FIELD)) {
                let o = opponent.expect("ready");
                action = VersusAction::Start(settings(config, o, None));
            }
        } else {
            ui.button(play_r, t("Play"), false);
        }
        if let Some((text, r)) = tip {
            ui.tooltip(&text, r);
        }
        action
    }

    /// Name, aiming guide, time per shot. True when Enter was pressed.
    fn options(&mut self, ui: &mut Ui, config: &mut Config, area: Rect) -> bool {
        let label = |ui: &Ui, text: &str, y: f32| ui.text(text, area.x, y, 13.0, MUTED, true);
        // Enter from a field of this form, or with no field (nor a dialog
        // over the screen) holding the keyboard.
        let mut enter = (is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter))
            && ui.focus.is_none();
        let mut y = area.y;
        label(ui, t("Your name"), y);
        let field = Rect::new(area.x, y + LABEL_H, area.w, CONTROL_H);
        enter |= ui.text_input(NAME_FIELD, field, &mut config.player_name, 20);
        if config.player_name.is_empty() && ui.focus != Some(NAME_FIELD) {
            placeholder(ui, field, t("Player"));
        }
        y += LABEL_H + CONTROL_H + GAP;
        label(ui, t("Aiming guide"), y);
        // Every guide, "No guide" too (with it chosen, nothing was lit here).
        let guides: Vec<&str> = AimGuide::ALL.iter().map(|g| t(g.name())).collect();
        let sel = AimGuide::ALL.iter().position(|g| *g == config.guide).unwrap_or(1);
        let seg = Rect::new(area.x, y + LABEL_H, area.w, CONTROL_H);
        if let Some(i) = ui.segmented(seg, &guides, sel) {
            config.guide = AimGuide::ALL[i];
        }
        if ui.hovering(seg) {
            let hint = match config.guide {
                AimGuide::Full => t(
                    "Shows where the object ball and the cue ball go (only in free games and practice).",
                ),
                AimGuide::Standard => t("Shows the contact point; judging the cut is up to you."),
                AimGuide::Short => t("Only a short line from the cue ball."),
                AimGuide::Off => t("No lines on the table: aim by eye, along the cue."),
            };
            ui.tooltip(hint, seg);
        }
        y += LABEL_H + CONTROL_H + GAP;
        label(ui, t("Time per shot"), y);
        let clocks = [t("No limit"), "30 s", "60 s"];
        let sel = match config.shot_clock {
            30 => 1,
            60 => 2,
            _ => 0,
        };
        let seg = Rect::new(area.x, y + LABEL_H, area.w, CONTROL_H);
        if let Some(i) = ui.segmented(seg, &clocks, sel) {
            config.shot_clock = [0, 30, 60][i];
        }
        if ui.hovering(seg) {
            ui.tooltip(t("When time runs out, the turn passes."), seg);
        }
        enter
    }

    /// The ghost card's side: who, and the state of their style. Returns
    /// the opponent once the style is in, an action, and whether Enter was
    /// used by the name field.
    fn ghost_panel(
        &mut self,
        ui: &mut Ui,
        online: &mut Online,
        area: Rect,
    ) -> (Option<Opponent>, Option<VersusAction>, bool) {
        let mut action = None;
        let mut used_enter = false;
        let (x, w) = (area.x, area.w);
        let mut y = area.y;
        // Title.
        let title = t("A player's ghost");
        ui.text(title, x, y + 2.0, 20.0, GOLD, true);
        y += 30.0;
        for line in ui.wrap(
            t("The computer plays in the style of a real player, measured on their online games."),
            w,
            13.0,
        ) {
            ui.text(&line, x, y, 13.0, MUTED, false);
            y += 17.0;
        }
        y += 8.0;
        if !online.signed_in() {
            for line in ui.wrap(t("Sign in to play the ghost of a real player."), w, 14.0) {
                ui.text(&line, x, y, 14.0, CREAM, false);
                y += 19.0;
            }
            if ui.button(Rect::new(x, y + 6.0, w.min(200.0), 36.0), t("Sign in"), true) {
                action = Some(VersusAction::SignIn);
            }
            return (None, action, false);
        }
        // Who: a name, or a friend.
        let load_w = 110.0f32.max(ui.text_size(t("Load"), 15.0, true).x + 24.0);
        let field = Rect::new(x, y, w - load_w - 8.0, CONTROL_H);
        if !self.ghost_focused {
            self.ghost_focused = true;
            ui.focus = Some(GHOST_FIELD);
        }
        let entered = ui.text_input(GHOST_FIELD, field, &mut self.ghost_name, 16);
        if self.ghost_name.is_empty() && ui.focus != Some(GHOST_FIELD) {
            placeholder(ui, field, t("Username"));
        }
        let name_ok = snook_legend_proto::valid_username(self.ghost_name.trim());
        let load = ui.button(Rect::new(x + w - load_w, y, load_w, CONTROL_H), t("Load"), name_ok);
        if (load || entered) && name_ok {
            used_enter = entered;
            online.request_ghost(&self.ghost_name);
            ui.focus = None;
        }
        y += CONTROL_H + 8.0;
        // Friends, as chips (as many as fit on one line).
        let friends: Vec<String> =
            online.friends.friends.iter().map(|f| f.player.username.clone()).collect();
        if !friends.is_empty() {
            let mut cx = x;
            let label = t("Friends:");
            ui.text(label, cx, text_y(y + 11.0, 12.0), 12.0, MUTED, false);
            cx += ui.text_size(label, 12.0, false).x + 8.0;
            for name in friends {
                let cw = ui.text_size(&name, 12.0, true).x + 16.0;
                if cx + cw > x + w {
                    break;
                }
                let chip = Rect::new(cx, y, cw, 22.0);
                let hover = ui.hovering(chip);
                rrect(chip, 11.0, Color::new(1.0, 1.0, 1.0, if hover { 0.12 } else { 0.05 }));
                rrect_lines(chip, 11.0, 1.0, if hover { GOLD } else { BORDER });
                ui.text(&name, cx + 8.0, text_y(y + 11.0, 12.0), 12.0, CREAM, true);
                ui.block(chip);
                if hover && ui.clicked(chip) {
                    self.ghost_name = name.clone();
                    online.request_ghost(&name);
                }
                cx += cw + 6.0;
            }
            y += 30.0;
        }
        y += 6.0;
        // The state of the request.
        let status = Rect::new(x, y, w, (area.y + area.h - y).max(40.0));
        let Some(g) = online.ghost.clone() else {
            let hint = t("Type a username or pick a friend, then Load.");
            for (k, line) in ui.wrap(hint, w, 13.0).iter().enumerate() {
                ui.text(line, x, y + k as f32 * 17.0, 13.0, MUTED, false);
            }
            return (None, action, used_enter);
        };
        let line = |ui: &Ui, text: &str, color: Color, y: f32| {
            let mut yy = y;
            for l in ui.wrap(text, w, 14.0) {
                ui.text(&l, x, yy, 14.0, color, false);
                yy += 19.0;
            }
        };
        match g.answer {
            None if g.sent.elapsed() > GHOST_TIMEOUT
                || crate::ranking::unreachable(online).is_some() =>
            {
                line(ui, t("The server can't be reached right now. Try again later."), ALERT, y);
            }
            None => {
                crate::ui::spinner(vec2(x + 11.0, y + 11.0), 9.0, GOLD);
                let text = tw("Measuring how {name} plays…", &[("name", &g.username)]);
                ui.text(
                    &ui.ellipsize(&text, w - 30.0, 14.0),
                    x + 28.0,
                    text_y(y + 11.0, 14.0),
                    14.0,
                    MUTED,
                    false,
                );
            }
            Some(GhostAnswer::Ready { style }) => {
                let o = Opponent::Ghost { name: g.username.clone(), style };
                let pr = 22.0;
                opponents::draw_portrait(&o, vec2(x + pr, y + pr), pr);
                let tx = x + 2.0 * pr + 12.0;
                let name = ui.ellipsize(&o.display_name(), x + w - tx, 16.0);
                ui.text(&name, tx, y + 2.0, 16.0, CREAM, true);
                opponents::draw_strength(tx, y + 32.0, opponents::ghost_strength(&style), 7.0);
                let s = style.sanitized();
                let facts = tw(
                    "Pots {pots}% · safeties {safe}% · banks {banks}% · {shots} shots",
                    &[
                        ("pots", &((s.pot_rate * 100.0).round() as i32)),
                        ("safe", &((s.safety_rate * 100.0).round() as i32)),
                        ("banks", &((s.bank_rate * 100.0).round() as i32)),
                        ("shots", &s.shots),
                    ],
                );
                let mut yy = y + 2.0 * pr + 10.0;
                for l in ui.wrap(&facts, w, 13.0) {
                    if yy + 15.0 > status.y + status.h {
                        break;
                    }
                    ui.text(&l, x, yy, 13.0, MUTED, false);
                    yy += 17.0;
                }
                if status.y + status.h - yy >= 8.0 + TRAITS_H {
                    traits(ui, &o, x, yy + 8.0, w);
                }
                return (Some(o), action, used_enter);
            }
            Some(GhostAnswer::NotEnough { shots, needed }) => line(
                ui,
                &tw(
                    "{name} has only {shots} recorded shots; a ghost needs {needed}. More online games will do it.",
                    &[("name", &g.username), ("shots", &shots), ("needed", &needed)],
                ),
                CREAM,
                y,
            ),
            Some(GhostAnswer::NoSuchPlayer) => line(
                ui,
                &tw("There is no player called {name}.", &[("name", &g.username)]),
                ALERT,
                y,
            ),
            Some(GhostAnswer::Busy) => {
                line(ui, t("The server is busy. Try again in a moment."), ALERT, y)
            }
            Some(GhostAnswer::TooSoon) => {
                line(ui, t("Wait a moment before asking again."), ALERT, y)
            }
            Some(GhostAnswer::Unknown) => {
                line(ui, t("Something went wrong on the server. Try again later."), ALERT, y)
            }
        }
        (None, action, used_enter)
    }

    // -----------------------------------------------------------------------
    // The ladder.

    fn ladder(
        &mut self,
        ui: &mut Ui,
        config: &mut Config,
        online: &mut Online,
        progress: &Progress,
        claimer: &mut Claimer,
        body: Rect,
    ) -> VersusAction {
        let mut action = VersusAction::None;
        let n = ladder::RUNGS.len();
        let selected = self.rung.filter(|k| progress.unlocked(*k)).unwrap_or(progress.next());
        let list_w = (body.w * 0.55).floor();
        let gap = 6.0;
        let row_h = ((body.h - (n - 1) as f32 * gap) / n as f32).min(50.0);
        let list_h = n as f32 * row_h + (n - 1) as f32 * gap;
        let top = body.y + (body.h - list_h) / 2.0;
        let mut tip: Option<(String, Rect)> = None;
        // The last rung at the top: the ladder is climbed.
        for k in (0..n).rev() {
            let p = ladder::rung(k).expect("rung");
            let row = Rect::new(body.x, top + (n - 1 - k) as f32 * (row_h + gap), list_w, row_h);
            let rung = k as u8;
            let unlocked = progress.unlocked(rung);
            let beaten = progress.is_beaten(rung);
            let is_next = rung == progress.next() && !progress.complete();
            let is_sel = rung == selected;
            let hover = unlocked && ui.hovering(row);
            card_frame(row, is_sel, hover);
            ui.block(row);
            let mid = row.y + row_h / 2.0;
            let fade = if unlocked { 1.0 } else { 0.45 };
            let col = |c: Color| Color::new(c.r, c.g, c.b, c.a * fade);
            // The rung's number.
            let num = (k + 1).to_string();
            let nr = (row_h * 0.3).min(13.0);
            let ncx = row.x + 10.0 + nr;
            let disc = if beaten {
                Color::new(0.16, 0.42, 0.24, 1.0)
            } else if is_next {
                GOLD
            } else {
                Color::new(1.0, 1.0, 1.0, 0.08)
            };
            draw_circle(ncx, mid, nr, disc);
            let ncol = if is_next { Color::new(0.1, 0.07, 0.04, 1.0) } else { col(CREAM) };
            ui.text_centered(&num, ncx, text_y(mid, 12.0), 12.0, ncol, true);
            // Portrait, name and epithet.
            let pr = (row_h * 0.38).min(18.0);
            let pcx = ncx + nr + 10.0 + pr;
            crate::paint::set_opacity(fade);
            opponents::draw_person(p.code, vec2(pcx, mid), pr);
            crate::paint::set_opacity(1.0);
            let tx = pcx + pr + 10.0;
            // The state on the right.
            let (state, state_col, icon) = if beaten {
                (t("Beaten"), GREEN_TEXT, Some(Icon::Check))
            } else if is_next {
                (t("Next"), GOLD, Some(Icon::Play))
            } else {
                (t("Locked"), MUTED, Some(Icon::Lock))
            };
            let sw_ = ui.text_size(state, 12.0, true).x;
            let sx = row.x + row.w - 10.0 - sw_;
            ui.text(state, sx, text_y(mid, 12.0), 12.0, col(state_col), true);
            if let Some(icon) = icon {
                crate::icons::draw(icon, vec2(sx - 10.0, mid), 11.0, col(state_col));
            }
            let pips_w = opponents::strength_width(5.0);
            let px = sx - 22.0 - pips_w;
            opponents::draw_strength(px, mid, p.strength, 5.0);
            if ui.hovering(Rect::new(px, mid - 5.0, pips_w, 10.0)) {
                tip = Some((
                    opponents::strength_label(p.strength),
                    Rect::new(px, mid - 5.0, pips_w, 10.0),
                ));
            }
            let room = px - 12.0 - tx;
            let name_size = if row_h >= 40.0 { 15.0 } else { 13.0 };
            let (name, ns) = ui.fitted(p.name, name_size, room, true);
            let name_w = ui.text_size(&name, ns, true).x;
            if row_h >= 40.0 {
                ui.text(
                    &name,
                    tx,
                    mid - ns - 1.0,
                    ns,
                    col(if is_sel { GOLD } else { CREAM }),
                    true,
                );
                let ep = ui.ellipsize(t(p.epithet), room, 12.0);
                ui.text(&ep, tx, mid + 3.0, 12.0, col(MUTED), false);
            } else {
                ui.text(
                    &name,
                    tx,
                    text_y(mid, ns),
                    ns,
                    col(if is_sel { GOLD } else { CREAM }),
                    true,
                );
                let ep = ui.ellipsize(t(p.epithet), room - name_w - 8.0, 12.0);
                ui.text(&ep, tx + name_w + 8.0, text_y(mid, 12.0), 12.0, col(MUTED), false);
            }
            if hover && ui.clicked(row) {
                self.rung = Some(rung);
            }
            if !unlocked && ui.hovering(row) {
                tip = Some((t("Win the previous stage to unlock this one.").to_string(), row));
            }
        }

        // The selected opponent and the reward on the right.
        let dx = body.x + list_w + 24.0;
        let dw = body.x + body.w - dx;
        draw_line(
            dx - 12.0,
            body.y,
            dx - 12.0,
            body.y + body.h,
            1.0,
            Color::new(BORDER.r, BORDER.g, BORDER.b, 0.6),
        );
        let reward_h = 150.0f32.min(body.h * 0.34);
        let reward = Rect::new(dx, body.y + body.h - reward_h, dw, reward_h);
        let play_r = Rect::new(dx, reward.y - 18.0 - 42.0, dw, 42.0);
        let p = ladder::rung(selected as usize).expect("rung");
        let rung_line =
            tw("Stage {n} of {total}", &[("n", &(selected + 1)), ("total", &ladder::RUNGS.len())]);
        let area = Rect::new(dx, body.y, dw, play_r.y - 14.0 - body.y);
        let o = Opponent::personality(p.code);
        ui.text(&rung_line, dx, area.y, 12.0, MUTED, true);
        details(
            ui,
            &o,
            p.name,
            t(p.epithet),
            t(p.description),
            p.strength,
            Rect::new(area.x, area.y + 20.0, area.w, area.h - 20.0),
        );
        let label = tw("Challenge {name}", &[("name", &p.name)]);
        let enter = (is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter))
            && ui.focus.is_none();
        if ui.primary_button(play_r, &label) || enter {
            action = VersusAction::Start(settings(config, o, Some(selected)));
        }
        if let Some(a) = self.reward(ui, online, progress, claimer, reward) {
            action = a;
        }
        if let Some((text, r)) = tip {
            ui.tooltip(&text, r);
        }
        action
    }

    /// The reward box: the item and where its claim stands.
    fn reward(
        &mut self,
        ui: &mut Ui,
        online: &mut Online,
        progress: &Progress,
        claimer: &mut Claimer,
        r: Rect,
    ) -> Option<VersusAction> {
        let mut action = None;
        rrect(r, 8.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.07));
        rrect_lines(r, 8.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.45));
        let item = snook_legend_proto::shop::item(ladder::REWARD_ITEM)?;
        let pad = 12.0;
        let x = r.x + pad;
        let w = r.w - 2.0 * pad;
        ui.text(t("Reward"), x, r.y + 10.0, 12.0, MUTED, true);
        crate::icons::draw(Icon::Trophy, vec2(x + w - 8.0, r.y + 16.0), 16.0, GOLD);
        let name = tw("{item} cue · exclusive", &[("item", &t(item.name))]);
        let name = ui.ellipsize(&name, w - 24.0, 15.0);
        ui.text(&name, x, r.y + 28.0, 15.0, GOLD, true);
        crate::shop::preview(ui, item, Rect::new(x - 4.0, r.y + 48.0, w.min(260.0), 22.0));
        let mut y = r.y + 78.0;
        let line_room = r.y + r.h - y;
        let say = |ui: &Ui, text: &str, color: Color, y: &mut f32| {
            for l in ui.wrap(text, w, 13.0) {
                if *y + 15.0 > r.y + r.h - 4.0 {
                    break;
                }
                ui.text(&l, x, *y, 13.0, color, false);
                *y += 17.0;
            }
        };
        // Signed in, the account's own achievements say whether it has the
        // reward; signed out, what this computer last heard (any account).
        let account = online.account();
        let granted = match account {
            Some(_) => {
                claimer.state == Claim::Granted || online.has_achievement(ladder::ACHIEVEMENT)
            }
            None => !online.signed_in() && progress.claimed && progress.pending.is_none(),
        };
        let button_w = 130.0f32.min(w);
        let button = |y: f32| Rect::new(x, (y + 2.0).min(r.y + r.h - 34.0), button_w, 30.0);
        let _ = line_room;
        if granted {
            crate::icons::draw(Icon::Check, vec2(x + 7.0, y + 8.0), 14.0, GREEN_TEXT);
            let text = t("Yours: find it in the shop, under Cues.");
            let l = ui.ellipsize(text, w - 20.0, 13.0);
            ui.text(&l, x + 20.0, y, 13.0, GREEN_TEXT, false);
            return action;
        }
        if progress.pending.is_none() {
            let text = if progress.complete() {
                t("Win the last stage again while signed in to claim it.")
            } else {
                t("Beat all fifteen opponents to earn it. It is sold nowhere.")
            };
            // Why the last claim was refused (the game can't be sent again).
            if let Claim::Failed(f) = claimer.state {
                say(ui, f.text(), ALERT, &mut y);
            }
            say(ui, text, CREAM, &mut y);
            return action;
        }
        // A won final game, waiting for the server.
        if !online.signed_in() {
            say(ui, t("You won the tour! Sign in to claim your reward."), CREAM, &mut y);
            if ui.button(button(y), t("Sign in"), true) {
                action = Some(VersusAction::SignIn);
            }
            return action;
        }
        // Only the account the final was dealt to can claim with it.
        if account.is_none_or(|a| progress.pending_for(a).is_none()) {
            let text = if progress.pending_account.is_empty() {
                t("You won the tour! Sign in to claim your reward.").to_string()
            } else {
                tw(
                    "The final you won was dealt to {name}: sign in as {name} to claim the reward.",
                    &[("name", &progress.pending_account)],
                )
            };
            say(ui, &text, CREAM, &mut y);
            return action;
        }
        match claimer.state {
            Claim::Claiming(_) | Claim::Idle => {
                crate::ui::spinner(vec2(x + 9.0, y + 8.0), 7.0, GOLD);
                let l = ui.ellipsize(t("The server is checking your final game…"), w - 24.0, 13.0);
                ui.text(&l, x + 24.0, y, 13.0, MUTED, false);
            }
            Claim::Failed(f) => {
                say(ui, f.text(), ALERT, &mut y);
                if f.retryable() && ui.button(button(y), t("Try again"), true) {
                    claimer.retry(progress, online, get_time());
                }
            }
            Claim::Granted => {}
        }
        action
    }
}

impl Versus {
    // -----------------------------------------------------------------------
    // The daily challenge.

    fn daily(&mut self, ui: &mut Ui, online: &mut Online, body: Rect) -> VersusAction {
        use snook_legend_proto::daily;
        let mut action = VersusAction::None;
        let today = daily::day_of(crate::util::unix_now());
        if online.signed_in() && self.daily_asked != Some(today) {
            self.daily_asked = Some(today);
            online.send(snook_legend_proto::ClientMsg::Daily);
        }
        if online.daily_refused.is_some() {
            self.daily_starting = false;
        }
        let left_w = (body.w * 0.46).floor();
        let left = Rect::new(body.x, body.y, left_w, body.h);
        let right = Rect::new(body.x + left_w + 24.0, body.y, body.w - left_w - 24.0, body.h);
        let x = left.x + 4.0;
        let w = left.w - 8.0;
        let mut y = left.y + 6.0;
        let say = |ui: &Ui, text: &str, size: f32, color: Color, y: &mut f32| {
            for l in ui.wrap(text, w, size) {
                ui.text(&l, x, *y, size, color, false);
                *y += size + 5.0;
            }
        };
        ui.text(t("Daily challenge"), x, y, 20.0, GOLD, true);
        y += 32.0;
        say(
            ui,
            t("Make as many balls as you can in one visit: a miss or a foul ends the attempt."),
            14.0,
            CREAM,
            &mut y,
        );
        y += 4.0;
        say(
            ui,
            &tw(
                "{n} attempts a day. Clear the table for {coins} coins; the day's winner gets a trophy.",
                &[("n", &daily::ATTEMPTS), ("coins", &daily::CLEAR_COINS)],
            ),
            13.0,
            MUTED,
            &mut y,
        );
        // Until the next table (UTC midnight).
        let left_secs = 86_400 - crate::util::unix_now().rem_euclid(86_400);
        let next = tw(
            "A new table in {h} h {m} min",
            &[("h", &(left_secs / 3600)), ("m", &(left_secs % 3600 / 60))],
        );
        say(ui, &next, 13.0, MUTED, &mut y);
        y += 10.0;
        if !online.signed_in() {
            say(
                ui,
                t("Sign in to take part: the server keeps the day's table."),
                14.0,
                CREAM,
                &mut y,
            );
            if ui.button(Rect::new(x, y + 4.0, 140.0, 34.0), t("Sign in"), true) {
                action = VersusAction::SignIn;
            }
        } else if let Some(info) = online.daily.clone().filter(|i| i.day == today) {
            let stat = |ui: &Ui, label: &str, value: &str, y: &mut f32| {
                ui.text(label, x, *y, 13.0, MUTED, false);
                let vw = ui.text_size(value, 15.0, true).x;
                ui.text(value, x + w - vw, *y - 1.0, 15.0, CREAM, true);
                *y += 24.0;
            };
            stat(ui, t("Balls on today's table"), &info.balls.to_string(), &mut y);
            let left = format!("{} / {}", info.attempts_left, daily::ATTEMPTS);
            stat(ui, t("Attempts left"), &left, &mut y);
            let best = match (info.best, info.rank) {
                (Some(b), Some(r)) => tw("{n} balls · #{rank}", &[("n", &b), ("rank", &r)]),
                (Some(b), None) => tw("{n} balls", &[("n", &b)]),
                _ => t("not yet").to_string(),
            };
            stat(ui, t("Your best today"), &best, &mut y);
            y += 8.0;
            if let Some(refused) = online.daily_refused {
                let text = match refused {
                    daily::Refused::NoAttemptsLeft => {
                        t("No attempts left today: come back tomorrow.")
                    }
                    _ => t("The attempt could not be counted."),
                };
                say(ui, text, 13.0, ALERT, &mut y);
            }
            let b = Rect::new(x, y + 4.0, w.min(240.0), 40.0);
            if self.daily_starting {
                crate::ui::spinner(vec2(b.x + 12.0, b.y + 20.0), 8.0, GOLD);
                ui.text(t("Setting up the table…"), b.x + 30.0, b.y + 12.0, 14.0, MUTED, false);
            } else if info.attempts_left > 0 {
                if ui.primary_button(b, t("Start an attempt")) {
                    self.daily_starting = true;
                    online.daily_refused = None;
                    online.send(snook_legend_proto::ClientMsg::DailyStart);
                }
                y = b.y + b.h + 8.0;
                say(
                    ui,
                    t("Starting uses an attempt, even if you leave the table."),
                    12.0,
                    MUTED,
                    &mut y,
                );
            } else {
                say(ui, t("No attempts left today: come back tomorrow."), 14.0, CREAM, &mut y);
            }
            self.daily_board(ui, &info, online, right);
        } else {
            crate::ui::spinner(vec2(x + 10.0, y + 8.0), 8.0, GOLD);
            ui.text(t("Loading today's table…"), x + 26.0, y, 14.0, MUTED, false);
        }
        action
    }

    // -----------------------------------------------------------------------
    // The tournament against the computer's opponents.

    fn cup(
        &mut self,
        ui: &mut Ui,
        config: &Config,
        cup: &mut Option<crate::cup::Cup>,
        body: Rect,
        medals: (snook_legend_proto::cup::Medals, usize, bool),
    ) -> VersusAction {
        use crate::cup::{Cup, Format};
        let mut action = VersusAction::None;
        // The player's medals, at the foot of the side (not while a
        // tournament is being played).
        let shelf_h = 104.0;
        // The tournament's game was left and is no longer kept (another
        // game against the computer took its place): it counts as lost. A
        // game is never played again from the start.
        if let Some(c) = cup.as_mut()
            && let Some(seed) = c.playing
            && self.saved_seed != Some(seed)
        {
            c.report(false);
            c.save();
            self.cup_forfeit = true;
        }
        let side_w = (body.w * 0.30).clamp(200.0, 290.0);
        let side = Rect::new(body.x + body.w - side_w, body.y, side_w, body.h);
        let board = Rect::new(body.x, body.y, body.w - side_w - 18.0, body.h);
        let x = side.x;
        let w = side.w;
        let mut y = side.y + 2.0;
        let say = |ui: &mut Ui, text: &str, size: f32, color: Color, bold: bool, y: &mut f32| {
            for l in ui.wrap(text, w, size) {
                ui.text(&l, x, *y, size, color, bold);
                *y += size + 5.0;
            }
        };
        // What starting one of each would be.
        let mut start: Option<Format> = None;
        let mut new_buttons = |ui: &mut Ui, y: f32| {
            let knock = Rect::new(x, y, w, 40.0);
            if ui.primary_button(knock, t("New knockout")) {
                start = Some(Format::Knockout);
            }
            let arena = Rect::new(x, y + 48.0, w, 40.0);
            if ui.button(arena, t("New arena"), true) {
                start = Some(Format::Arena);
            }
        };
        match cup.as_ref() {
            None => {
                say(ui, t("Tournament"), 20.0, GOLD, true, &mut y);
                y += 4.0;
                say(
                    ui,
                    t("Knockout: sixteen players, four rounds. Lose a game and you are out."),
                    14.0,
                    CREAM,
                    false,
                    &mut y,
                );
                y += 6.0;
                say(
                    ui,
                    t(
                        "Arena: five rounds for everyone. A win is 2 points, 3 from the third win in a row. The most points wins.",
                    ),
                    14.0,
                    CREAM,
                    false,
                    &mut y,
                );
                y += 6.0;
                say(
                    ui,
                    t(
                        "The games between the other players are decided by their strength and a draw.",
                    ),
                    13.0,
                    MUTED,
                    false,
                    &mut y,
                );
                new_buttons(ui, y + 12.0);
                medal_shelf(ui, Rect::new(x, side.y + side.h - shelf_h, w, shelf_h), medals);
                // The sixteen, as a preview of who is in.
                self.cup_field(ui, config, board);
            }
            Some(c) => {
                let round = c.rounds.len();
                let title = match c.format {
                    Format::Knockout => t("Knockout"),
                    Format::Arena => t("Arena"),
                };
                say(ui, title, 20.0, GOLD, true, &mut y);
                if c.finished() {
                    let champion = c.champion().unwrap_or(0);
                    let name = cup_name(c, champion, config);
                    let line = if c.is_player(champion) {
                        t("You won the tournament!").to_string()
                    } else {
                        tw("{name} won the tournament.", &[("name", &name)])
                    };
                    y += 4.0;
                    say(ui, &line, 15.0, CREAM, true, &mut y);
                    if let Some(place) = c.player_place().filter(|p| *p > 1) {
                        let line = tw("You finished in place {n}.", &[("n", &place)]);
                        say(ui, &line, 14.0, MUTED, false, &mut y);
                    }
                    new_buttons(ui, y + 14.0);
                    medal_shelf(ui, Rect::new(x, side.y + side.h - shelf_h, w, shelf_h), medals);
                    // The player's medal of this one (gold for the champion,
                    // silver, bronze), and their name.
                    if let Some(medal) = c.medal() {
                        let top = y + 14.0 + 96.0 + 18.0;
                        let room = side.y + side.h - shelf_h - 10.0 - top;
                        if room > 90.0 {
                            let color = medal_color(medal.place);
                            let size = (room * 0.5).clamp(44.0, 110.0);
                            let cx = x + w / 2.0;
                            let cy = top + size / 2.0 + 4.0;
                            let glow = 0.5 + 0.5 * (get_time() as f32 * 2.0).sin();
                            draw_circle(
                                cx,
                                cy,
                                size * (0.62 + 0.04 * glow),
                                Color::new(color.r, color.g, color.b, 0.10 + 0.06 * glow),
                            );
                            crate::icons::draw(Icon::Medal, vec2(cx, cy), size, color);
                            let me = cup_name(c, c.player(), config);
                            let (me, ns) = ui.fitted(&me, 20.0, w, true);
                            ui.text_centered(&me, cx, cy + size / 2.0 + 14.0, ns, color, true);
                            let title = match medal.place {
                                1 => t("Champion"),
                                2 => t("Silver medal"),
                                _ => t("Bronze medal"),
                            };
                            ui.text_centered(
                                title,
                                cx,
                                cy + size / 2.0 + 14.0 + ns + 6.0,
                                13.0,
                                CREAM,
                                true,
                            );
                        }
                    }
                } else {
                    if self.cup_forfeit {
                        say(
                            ui,
                            t("You left your game: it counts as a loss."),
                            13.0,
                            ALERT,
                            false,
                            &mut y,
                        );
                    }
                    let stage = match c.format {
                        Format::Knockout => t(match round {
                            0 => "Round of 16",
                            1 => "Quarter-finals",
                            2 => "Semi-finals",
                            _ => "Final",
                        })
                        .to_string(),
                        Format::Arena => tw(
                            "Round {n} of {total}",
                            &[("n", &(round + 1)), ("total", &c.total_rounds())],
                        ),
                    };
                    say(ui, &stage, 13.0, MUTED, true, &mut y);
                    y += 6.0;
                    if let Some(p) = c.next_opponent().and_then(|o| c.personality(o)) {
                        let o = Opponent::personality(p.code);
                        let play_r = Rect::new(x, side.y + side.h - 42.0 - 34.0, w, 42.0);
                        details(
                            ui,
                            &o,
                            p.name,
                            t(p.epithet),
                            t(p.description),
                            p.strength,
                            Rect::new(x, y, w, play_r.y - 12.0 - y),
                        );
                        // A game left half way goes on from where it was.
                        let resume = c.playing.is_some();
                        let label = if resume {
                            tw("Continue against {name}", &[("name", &p.name)])
                        } else {
                            tw("Play {name}", &[("name", &p.name)])
                        };
                        let (label, _) = ui.fitted(&label, 18.0, w - 16.0, true);
                        let enter = (is_key_pressed(KeyCode::Enter)
                            || is_key_pressed(KeyCode::KpEnter))
                            && ui.focus.is_none();
                        if ui.primary_button(play_r, &label) || enter {
                            action = if resume {
                                VersusAction::ResumeCup
                            } else {
                                VersusAction::StartCup(settings(config, o, None))
                            };
                        }
                        let give_up = Rect::new(x, play_r.y + 48.0, w, 26.0);
                        if ui.button(give_up, t("Abandon the tournament"), true) {
                            start = Some(c.format);
                            self.cup_abandon = true;
                        }
                    }
                }
                match c.format {
                    Format::Knockout => cup_bracket(ui, c, config, board),
                    Format::Arena => cup_table(ui, c, config, board),
                }
            }
        }
        if let Some(format) = start {
            if std::mem::take(&mut self.cup_abandon) {
                // Abandoned: back to choosing one.
                *cup = None;
                Cup::discard();
            } else {
                let fresh = Cup::new(format, crate::game::clock_seed());
                fresh.save();
                *cup = Some(fresh);
            }
        }
        action
    }

    /// The sixteen who play, before a tournament is drawn.
    fn cup_field(&self, ui: &mut Ui, config: &Config, board: Rect) {
        let cols = 4;
        let rows = 4;
        let cw = (board.w - (cols - 1) as f32 * 10.0) / cols as f32;
        let ch = ((board.h - (rows - 1) as f32 * 10.0) / rows as f32).min(120.0);
        for k in 0..16 {
            let cell = Rect::new(
                board.x + (k % cols) as f32 * (cw + 10.0),
                board.y + (k / cols) as f32 * (ch + 10.0),
                cw,
                ch,
            );
            card_frame(cell, k == 0, false);
            let pr = (ch * 0.26).min(24.0);
            let c = vec2(cell.x + cell.w / 2.0, cell.y + ch * 0.38);
            let name = if k == 0 {
                // The player: a gold disc with their initial.
                let name = crate::name_or(&config.player_name, "Player");
                let initial: String = name.chars().take(1).flat_map(char::to_uppercase).collect();
                draw_circle(c.x, c.y, pr, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.25));
                ui.text_centered(&initial, c.x, text_y(c.y, pr), pr, GOLD, true);
                name
            } else {
                let p = &PERSONALITIES[k - 1];
                opponents::draw_person(p.code, c, pr);
                p.name.to_string()
            };
            let (name, size) = ui.fitted(&name, 13.0, cell.w - 10.0, true);
            ui.text_centered(&name, c.x, cell.y + ch * 0.38 + pr + 8.0, size, CREAM, true);
        }
    }

    fn drills(&mut self, ui: &mut Ui, online: &mut Online, body: Rect) -> VersusAction {
        use snook_legend_proto::drills::{self, DRILLS};
        let mut action = VersusAction::None;
        if online.signed_in() && !self.drills_asked {
            self.drills_asked = true;
            online.send(snook_legend_proto::ClientMsg::Drills);
        }
        let info = online.drills.clone();
        let best_of = |code: &str| {
            info.as_ref()
                .and_then(|i| i.drills.iter().find(|d| d.code == code))
                .and_then(|d| d.best)
        };
        let list_w = (body.w * 0.56).floor();
        // The stars earned, over the drills, with a bar to fill.
        let head = Rect::new(body.x, body.y, list_w, 46.0);
        rrect(head, 8.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.08));
        let stars: u32 =
            DRILLS.iter().map(|d| best_of(d.code).map_or(0, |b| drills::stars(d, b))).sum();
        let max = drills::max_stars();
        crate::icons::draw(Icon::Star, vec2(head.x + 22.0, head.y + head.h / 2.0), 20.0, GOLD);
        let label = tw("{s} / {max} stars", &[("s", &stars), ("max", &max)]);
        ui.text(&label, head.x + 40.0, text_y(head.y + head.h / 2.0, 16.0), 16.0, CREAM, true);
        let lw = ui.text_size(&label, 16.0, true).x;
        let bar_x = head.x + 40.0 + lw + 18.0;
        let bar =
            Rect::new(bar_x, head.y + head.h / 2.0 - 4.0, head.x + head.w - 14.0 - bar_x, 8.0);
        if bar.w > 30.0 {
            rrect(bar, 4.0, Color::new(1.0, 1.0, 1.0, 0.08));
            let filled = bar.w * stars as f32 / max.max(1) as f32;
            if filled > 0.0 {
                rrect(Rect::new(bar.x, bar.y, filled.max(8.0), bar.h), 4.0, GOLD);
            }
        }
        // The drills, in columns.
        let top = head.y + head.h + 10.0;
        let cols = if list_w > 760.0 { 3 } else { 2 };
        let rows = DRILLS.len().div_ceil(cols);
        let col_gap = 8.0;
        let col_w = (list_w - (cols - 1) as f32 * col_gap) / cols as f32;
        let row_h = ((body.y + body.h - top) / rows as f32).min(58.0);
        // Many drills: the rows close up and their stars shrink.
        let compact = row_h < 40.0;
        let (row_gap, star, star_step) =
            if compact { (3.0, 12.0, 15.0) } else { (6.0, 15.0, 19.0) };
        for (k, d) in DRILLS.iter().enumerate() {
            let (col, row) = (k / rows, k % rows);
            let cell = Rect::new(
                body.x + col as f32 * (col_w + col_gap),
                top + row as f32 * row_h,
                col_w,
                row_h - row_gap,
            );
            let hover = ui.hovering(cell);
            card_frame(cell, self.drill == k, hover);
            ui.block(cell);
            if ui.clicked(cell) {
                self.drill = k;
            }
            let mid = cell.y + cell.h / 2.0;
            let size = if compact { 13.0 } else { 14.0 };
            let stars_w = 3.0 * star_step + 6.0;
            let (name, size) = ui.fitted(t(d.name), size, cell.w - 24.0 - stars_w, true);
            ui.text(&name, cell.x + 12.0, text_y(mid, size), size, CREAM, true);
            let earned = best_of(d.code).map_or(0, |b| drills::stars(d, b));
            for s in 0..3 {
                let c = vec2(cell.x + cell.w - 12.0 - (2 - s) as f32 * star_step - star / 2.0, mid);
                let color = if s < earned { GOLD } else { Color::new(1.0, 1.0, 1.0, 0.15) };
                crate::icons::draw(Icon::Star, c, star, color);
            }
        }
        // The drill picked: what it asks, the player's best, its position.
        let d = &DRILLS[self.drill.min(DRILLS.len() - 1)];
        let right = Rect::new(body.x + list_w + 24.0, body.y, body.w - list_w - 24.0, body.h);
        let x = right.x + 4.0;
        let w = right.w - 8.0;
        let mut y = right.y + 4.0;
        let (title, title_size) = ui.fitted(t(d.name), 20.0, w, true);
        ui.text(&title, x, y, title_size, GOLD, true);
        y += 30.0;
        for l in ui.wrap(t(d.description), w, 14.0) {
            ui.text(&l, x, y, 14.0, CREAM, false);
            y += 19.0;
        }
        y += 4.0;
        let need = tw(
            "Stars at {a}, {b} and {c}",
            &[("a", &d.stars[0]), ("b", &d.stars[1]), ("c", &d.stars[2])],
        );
        ui.text(&need, x, y, 13.0, MUTED, false);
        y += 20.0;
        let best = match best_of(d.code) {
            Some(b) => tw("Your best: {n} of {total}", &[("n", &b), ("total", &d.tries)]),
            None => t("Not tried yet.").to_string(),
        };
        ui.text(&best, x, y, 14.0, CREAM, true);
        y += 26.0;
        let b = Rect::new(x, y, w.min(220.0), 40.0);
        if ui.primary_button(b, t("Start the drill")) {
            action = VersusAction::Drill(d.code.to_string());
        }
        y += 52.0;
        if !online.signed_in() {
            for l in ui.wrap(t("Sign in to keep your stars in your account."), w, 13.0) {
                ui.text(&l, x, y, 13.0, MUTED, false);
                y += 18.0;
            }
            y += 8.0;
        }
        // The position, from above.
        let area = Rect::new(right.x, y, right.w, right.y + right.h - y);
        if area.h > 80.0 {
            drill_sketch(ui, d, area);
        }
        action
    }

    /// The day's leaders.
    fn daily_board(
        &self,
        ui: &mut Ui,
        info: &snook_legend_proto::daily::Info,
        online: &Online,
        r: Rect,
    ) {
        rrect(r, 8.0, Color::new(1.0, 1.0, 1.0, 0.03));
        let x = r.x + 14.0;
        let w = r.w - 28.0;
        ui.text(t("Today's table"), x, r.y + 12.0, 16.0, GOLD, true);
        let players = if info.players == 1 {
            t("1 player").to_string()
        } else {
            tw("{n} players", &[("n", &info.players)])
        };
        let pw = ui.text_size(&players, 12.0, false).x;
        ui.text(&players, x + w - pw, r.y + 15.0, 12.0, MUTED, false);
        if info.board.is_empty() {
            let text = t("Nobody has played today's table yet. Be the first!");
            let mut y = r.y + 52.0;
            for l in ui.wrap(text, w, 14.0) {
                ui.text(&l, x, y, 14.0, MUTED, false);
                y += 19.0;
            }
            return;
        }
        let row_h = 28.0;
        let rows = (((r.h - 52.0) / row_h).floor().max(1.0)) as usize;
        let me = online.me.as_ref().map(|m| m.username.as_str()).unwrap_or("");
        for (k, e) in info.board.iter().take(rows).enumerate() {
            let y = r.y + 44.0 + k as f32 * row_h;
            let mine = e.username.eq_ignore_ascii_case(me);
            if mine {
                rrect(
                    Rect::new(r.x + 6.0, y - 2.0, r.w - 12.0, row_h - 2.0),
                    6.0,
                    Color::new(GOLD.r, GOLD.g, GOLD.b, 0.12),
                );
            }
            let place = format!("{}.", k + 1);
            let color = if k == 0 { GOLD } else { CREAM };
            ui.text(&place, x, y + 4.0, 14.0, color, true);
            let balls = tw("{n} balls", &[("n", &e.balls)]);
            let bw = ui.text_size(&balls, 14.0, true).x;
            ui.text(&balls, x + w - bw, y + 4.0, 14.0, color, true);
            ui.name_with_flag(
                &e.username,
                &e.country,
                CREAM,
                x + 34.0,
                y + 4.0,
                14.0,
                w - 34.0 - bw - 12.0,
            );
        }
    }
}

/// The settings for a game against `o` (`rung`: a ladder game).
pub(crate) fn settings(config: &Config, o: Opponent, rung: Option<u8>) -> MatchSettings {
    let mut s = MatchSettings::from_config(MatchMode::VsComputer(o), config);
    s.ladder = rung;
    s
}

/// An entrant's name in a tournament: the player's own, or the opponent's.
fn cup_name(c: &crate::cup::Cup, entrant: u8, config: &Config) -> String {
    match c.personality(entrant) {
        Some(p) => p.name.to_string(),
        None => crate::name_or(&config.player_name, "Player"),
    }
}

/// A knockout's bracket: a column for each round and one for the champion.
fn cup_bracket(ui: &mut Ui, c: &crate::cup::Cup, config: &Config, board: Rect) {
    let cols = 5;
    let col_w = board.w / cols as f32;
    let cell_w = col_w - 14.0;
    let line = Color::new(1.0, 1.0, 1.0, 0.16);
    for round in 0..cols {
        let alive = if round <= c.rounds.len() { c.alive(round) } else { Vec::new() };
        let slots = 16usize >> round;
        let slot_h = board.h / slots as f32;
        let cell_h = (slot_h - 4.0).min(30.0);
        let x = board.x + round as f32 * col_w;
        for k in 0..slots {
            let mid = board.y + (k as f32 + 0.5) * slot_h;
            let cell = Rect::new(x, mid - cell_h / 2.0, cell_w, cell_h);
            // The line on to the next round.
            if round + 1 < cols {
                let next_mid = board.y + ((k / 2) as f32 + 0.5) * slot_h * 2.0;
                let jx = x + cell_w + 7.0;
                draw_line(x + cell_w, mid, jx, mid, 1.0, line);
                draw_line(jx, mid, jx, next_mid, 1.0, line);
                draw_line(jx, next_mid, x + col_w, next_mid, 1.0, line);
            }
            let Some(&entrant) = alive.get(k) else {
                rrect(cell, 6.0, Color::new(1.0, 1.0, 1.0, 0.025));
                continue;
            };
            let me = c.is_player(entrant);
            // Went through this round, went out in it, or still to play.
            let through =
                c.rounds.get(round).map(|games| games.iter().any(|g| g.winner == entrant));
            let champion = round == cols - 1;
            let (fill, text) = match through {
                _ if champion => (Color::new(GOLD.r, GOLD.g, GOLD.b, 0.22), GOLD),
                Some(false) => (Color::new(1.0, 1.0, 1.0, 0.03), MUTED),
                _ if me => (Color::new(GOLD.r, GOLD.g, GOLD.b, 0.16), GOLD),
                _ => (Color::new(1.0, 1.0, 1.0, 0.07), CREAM),
            };
            rrect(cell, 6.0, fill);
            if me || champion {
                rrect_lines(cell, 6.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.7));
            }
            let mut tx = cell.x + 8.0;
            if cell_h >= 20.0
                && let Some(p) = c.personality(entrant)
            {
                let pr = (cell_h * 0.38).min(11.0);
                opponents::draw_person(p.code, vec2(tx + pr, mid), pr);
                tx += 2.0 * pr + 6.0;
            }
            let size = if cell_h >= 20.0 { 13.0 } else { 11.0 };
            let (name, size) =
                ui.fitted(&cup_name(c, entrant, config), size, cell.x + cell.w - 6.0 - tx, me);
            ui.text(&name, tx, text_y(mid, size), size, text, me || champion);
        }
    }
}

/// An arena's table.
fn cup_table(ui: &mut Ui, c: &crate::cup::Cup, config: &Config, board: Rect) {
    let table = c.standings();
    let row_h = (board.h / (table.len() as f32 + 1.0)).min(34.0);
    let cols = [board.x + 8.0, board.x + 44.0, board.x + board.w - 150.0, board.x + board.w - 62.0];
    let head_y = text_y(board.y + row_h / 2.0, 12.0);
    ui.text("#", cols[0], head_y, 12.0, MUTED, true);
    ui.text(t("Player"), cols[1], head_y, 12.0, MUTED, true);
    ui.text(t("W - L"), cols[2], head_y, 12.0, MUTED, true);
    ui.text(t("Points"), cols[3], head_y, 12.0, MUTED, true);
    for (k, s) in table.iter().enumerate() {
        let row = Rect::new(board.x, board.y + (k as f32 + 1.0) * row_h, board.w, row_h - 3.0);
        let mid = row.y + row.h / 2.0;
        let me = c.is_player(s.entrant);
        let leader = k == 0 && (s.wins + s.losses) > 0;
        if me {
            rrect(row, 6.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.14));
            rrect_lines(row, 6.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.7));
        } else if k % 2 == 0 {
            rrect(row, 6.0, Color::new(1.0, 1.0, 1.0, 0.03));
        }
        let color = if me || leader { GOLD } else { CREAM };
        let size = if row.h >= 22.0 { 14.0 } else { 12.0 };
        ui.text(&(k + 1).to_string(), cols[0], text_y(mid, size), size, MUTED, true);
        let mut tx = cols[1];
        if let Some(p) = c.personality(s.entrant) {
            let pr = (row.h * 0.4).min(12.0);
            opponents::draw_person(p.code, vec2(tx + pr, mid), pr);
            tx += 2.0 * pr + 8.0;
        }
        let (name, size) =
            ui.fitted(&cup_name(c, s.entrant, config), size, cols[2] - 12.0 - tx, me);
        ui.text(&name, tx, text_y(mid, size), size, color, me);
        let record = format!("{} - {}", s.wins, s.losses);
        ui.text(&record, cols[2], text_y(mid, size), size, CREAM, false);
        ui.text(&s.points.to_string(), cols[3], text_y(mid, size), size, color, true);
    }
}

/// A drill's position seen from above, in `area`: the table, the balls,
/// the pocket to play (gold) and the circle for the cue ball (green).
fn drill_sketch(ui: &Ui, d: &snook_legend_proto::drills::Drill, area: Rect) {
    use snook_legend_proto::drills::Goal;
    const RAIL: Color = Color::new(0.27, 0.16, 0.09, 1.0);
    const CUSHION: Color = Color::new(0.06, 0.30, 0.18, 1.0);
    const CLOTH: Color = Color::new(0.09, 0.40, 0.24, 1.0);
    let table = snook_legend_core::Table::default();
    let (len, wid) = (table.spec.length as f32, table.spec.width as f32);
    let rail_k = 0.075;
    let aspect = (len + 2.0 * rail_k * len) / (wid + 2.0 * rail_k * len);
    let fw = area.w.min(area.h * aspect);
    let fh = fw / aspect;
    let frame = Rect::new(area.x + (area.w - fw) / 2.0, area.y, fw, fh);
    let rail = rail_k * len / (len + 2.0 * rail_k * len) * fw;
    let cloth =
        Rect::new(frame.x + rail, frame.y + rail, frame.w - 2.0 * rail, frame.h - 2.0 * rail);
    // +y up, as the top view shows it.
    let at = |p: ::glam::DVec2| {
        vec2(
            cloth.x + (p.x as f32 / len + 0.5) * cloth.w,
            cloth.y + (0.5 - p.y as f32 / wid) * cloth.h,
        )
    };
    rrect(frame, rail * 0.8, RAIL);
    rrect_lines(frame, rail * 0.8, 1.0, Color::new(0.0, 0.0, 0.0, 0.5));
    let cushion = rail * 0.35;
    draw_rectangle(
        cloth.x - cushion,
        cloth.y - cushion,
        cloth.w + 2.0 * cushion,
        cloth.h + 2.0 * cushion,
        CUSHION,
    );
    draw_rectangle(cloth.x, cloth.y, cloth.w, cloth.h, CLOTH);
    let hx = cloth.x + cloth.w * 0.25;
    draw_line(hx, cloth.y, hx, cloth.y + cloth.h, 1.0, Color::new(1.0, 1.0, 1.0, 0.10));
    let target = match d.goal {
        Goal::Pot { pocket, .. } | Goal::Bank { pocket, .. } | Goal::Position { pocket, .. } => {
            Some(pocket)
        }
        _ => None,
    };
    let pocket_r = (rail * 0.42).max(4.0);
    for p in &table.pockets {
        let c = at(p.hole_center);
        draw_circle(c.x, c.y, pocket_r, Color::new(0.02, 0.01, 0.0, 1.0));
        if Some(p.id) == target {
            crate::paint::draw_circle_lines(c.x, c.y, pocket_r + 3.0, 2.0, GOLD);
        }
    }
    let scale = cloth.w / len;
    if let Goal::Position { zone, radius, .. } = d.goal {
        let c = at(zone);
        let r = radius as f32 * scale;
        draw_circle(c.x, c.y, r, Color::new(0.35, 1.0, 0.5, 0.12));
        crate::paint::draw_circle_lines(c.x, c.y, r, 1.5, Color::new(0.35, 1.0, 0.5, 0.8));
    }
    let palette = crate::textures::Palette::of("balls_classic");
    let ball_r = (table.spec.ball_radius as f32 * scale).max(6.0);
    let game = snook_legend_proto::drills::game(d);
    let mut balls: Vec<_> = game.balls().iter().filter(|b| b.in_play()).collect();
    // The cue ball last, over the others.
    balls.sort_by_key(|b| b.id == snook_legend_core::BallId::CUE);
    for b in balls {
        crate::showroom::ball_chip(ui, b.id, &palette, at(b.xy()), ball_r, false);
    }
}

/// A card's background: picked, under the pointer, or plain.
fn card_frame(r: Rect, selected: bool, hover: bool) {
    if selected {
        rrect(r, 8.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.13));
        rrect_lines(r, 8.0, 1.5, GOLD);
    } else if hover {
        rrect(r, 8.0, Color::new(1.0, 1.0, 1.0, 0.07));
        rrect_lines(r, 8.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.6));
    } else {
        rrect(r, 8.0, Color::new(1.0, 1.0, 1.0, 0.03));
        rrect_lines(r, 8.0, 1.0, Color::new(1.0, 1.0, 1.0, 0.07));
    }
}

/// A big portrait, the name, epithet, strength and description, in `area`.
fn details(
    ui: &Ui,
    o: &Opponent,
    name: &str,
    epithet: &str,
    description: &str,
    strength: u8,
    area: Rect,
) {
    let pr = 34.0f32.min(area.h * 0.3).max(16.0);
    opponents::draw_portrait(o, vec2(area.x + pr, area.y + pr), pr);
    let tx = area.x + 2.0 * pr + 14.0;
    let room = area.x + area.w - tx;
    let (n, size) = ui.fitted(name, 24.0, room, true);
    ui.text(&n, tx, caps_y(area.y + pr - 12.0, size), size, CREAM, true);
    let ep = ui.wrap(epithet, room, 14.0);
    let mut y = area.y + pr + 4.0;
    for l in ep.iter().take(2) {
        ui.text(l, tx, y, 14.0, GOLD, false);
        y += 18.0;
    }
    opponents::draw_strength(tx, y + 8.0, strength, 8.0);
    let label = opponents::strength_label(strength);
    let lx = tx + opponents::strength_width(8.0) + 8.0;
    ui.text(
        &ui.ellipsize(&label, area.x + area.w - lx, 12.0),
        lx,
        text_y(y + 8.0, 12.0),
        12.0,
        MUTED,
        false,
    );
    let mut y = (y + 22.0).max(area.y + 2.0 * pr + 12.0);
    let lines = ui.wrap(description, area.w, 14.0);
    let fit = (((area.y + area.h - y) / 19.0).floor().max(0.0)) as usize;
    for (k, l) in lines.iter().enumerate().take(fit) {
        let l = if k + 1 == fit && lines.len() > fit {
            ui.ellipsize(&format!("{l}…"), area.w, 14.0)
        } else {
            l.clone()
        };
        ui.text(&l, area.x, y, 14.0, CREAM, false);
        y += 19.0;
    }
    // How they play, in six levels (when there is room).
    if area.y + area.h - y >= 12.0 + TRAITS_H {
        traits(ui, o, area.x, y + 12.0, area.w);
    }
}

/// The height [`traits`] takes.
const TRAITS_H: f32 = 3.0 * 22.0;

/// How an opponent plays, as six levels in two columns.
fn traits(ui: &Ui, o: &Opponent, x: f32, y: f32, w: f32) {
    let col_w = (w - 20.0) / 2.0;
    let bar_w = (col_w * 0.42).min(80.0);
    for (i, (label, level)) in opponents::traits(&o.player()).into_iter().enumerate() {
        let cx = x + (i / 3) as f32 * (col_w + 20.0);
        let cy = y + (i % 3) as f32 * 22.0 + 8.0;
        let (label, size) = ui.fitted(t(label), 12.0, col_w - bar_w - 8.0, false);
        ui.text(&label, cx, text_y(cy, size), size, MUTED, false);
        opponents::draw_level(cx + col_w - bar_w, cy, bar_w, level);
    }
}

/// Placeholders are fainter than typed text, so an empty field reads as empty.
fn placeholder(ui: &Ui, field: Rect, text: &str) {
    let color = Color::new(MUTED.r, MUTED.g, MUTED.b, 0.6);
    let size = (field.h * 0.5).clamp(12.0, 18.0);
    ui.text(text, field.x + 8.0, field.y + (field.h - size) / 2.0, size, color, false);
}

/// The colour of a medal: gold, silver, bronze (place 1, 2, 3).
fn medal_color(place: u8) -> Color {
    match place {
        1 => GOLD,
        2 => Color::new(0.80, 0.82, 0.86, 1.0),
        _ => Color::new(0.82, 0.52, 0.30, 1.0),
    }
}

/// The player's medals of the tournaments against the computer, by kind:
/// gold, silver and bronze. `medals`: the count, how many wait to be kept
/// by the server, and whether there is an account to keep them in.
fn medal_shelf(ui: &mut Ui, r: Rect, medals: (snook_legend_proto::cup::Medals, usize, bool)) {
    let (m, waiting, account) = medals;
    rrect(r, 8.0, Color::new(0.0, 0.0, 0.0, 0.22));
    rrect_lines(r, 8.0, 1.0, Color::new(BORDER.r, BORDER.g, BORDER.b, 0.6));
    let x = r.x + 12.0;
    ui.text(t("Your medals"), x, r.y + 8.0, 12.0, MUTED, true);
    let rows = [(t("Knockout"), m.knockout), (t("Arena"), m.arena)];
    let label_w = rows.iter().map(|(l, _)| ui.text_size(l, 13.0, true).x).fold(0.0, f32::max);
    let cell = ((r.w - 24.0 - label_w - 10.0) / 3.0).max(36.0);
    for (k, (label, counts)) in rows.iter().enumerate() {
        let cy = r.y + 38.0 + k as f32 * 26.0;
        ui.text(label, x, cy - 7.5, 13.0, CREAM, true);
        for (i, n) in counts.iter().enumerate() {
            let cx = x + label_w + 10.0 + cell * i as f32 + 10.0;
            let color = medal_color(i as u8 + 1);
            let faded = if *n == 0 { Color::new(color.r, color.g, color.b, 0.35) } else { color };
            crate::icons::draw(Icon::Medal, vec2(cx, cy), 18.0, faded);
            let text = n.to_string();
            ui.text(&text, cx + 13.0, cy - 7.5, 14.0, if *n == 0 { MUTED } else { CREAM }, true);
        }
    }
    if waiting > 0 {
        let note = if account {
            t("Kept in your account the next time you're online.")
        } else {
            t("Sign in to keep them in your account.")
        };
        let (note, size) = ui.fitted(note, 11.0, r.w - 24.0, false);
        ui.text(&note, x, r.y + r.h - 18.0, size, MUTED, false);
    }
}
