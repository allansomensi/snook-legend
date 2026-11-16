//! Playing the computer: a free game against any opponent (a personality
//! or a real player's ghost), or the ladder of ten.

use macroquad::prelude::*;
use snookered_core::personalities::PERSONALITIES;
use snookered_proto::ghost::GhostAnswer;
use snookered_proto::ladder;

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use crate::config::{AimGuide, Config};
use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::ladder::{Claim, Claimer, Progress};
use crate::online::{GHOST_TIMEOUT, Online};
use crate::opponents::{self, Opponent};
use crate::ranking::{HEADER_H, PAD, caps_y, screen_header, text_y};
use crate::ui::{ALERT, BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};
use crate::{MatchMode, MatchSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Free,
    Ladder,
    /// The daily challenge.
    Daily,
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

    pub fn draw(
        &mut self,
        ui: &mut Ui,
        config: &mut Config,
        online: &mut Online,
        progress: &Progress,
        claimer: &mut Claimer,
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
                "Ladder: {n} of {total} beaten",
                &[("n", &progress.beaten), ("total", &ladder::RUNGS.len())],
            ),
            Tab::Daily => t("The same table for everyone, all day").to_string(),
        };
        // The tabs at the top right; the title keeps clear of them.
        let labels = [t("Free game"), t("Ladder"), t("Daily challenge")];
        let widest = labels.iter().map(|l| ui.text_size(l, 15.0, true).x).fold(0.0, f32::max);
        let tab_w = (widest + 28.0).clamp(120.0, 150.0);
        let tabs_x = r.x + r.w - PAD - 3.0 * tab_w - 16.0;
        if screen_header(ui, r, Icon::Robot, t("Against the computer"), Some(&subtitle))
            || (is_key_pressed(KeyCode::Escape) && ui.focus.is_none())
        {
            action = VersusAction::Back;
        }
        for (k, (tab, label)) in
            [(Tab::Free, labels[0]), (Tab::Ladder, labels[1]), (Tab::Daily, labels[2])]
                .into_iter()
                .enumerate()
        {
            let tr = Rect::new(tabs_x + k as f32 * (tab_w + 8.0), r.y + 20.0, tab_w, 36.0);
            if ui.tab(tr, label, self.tab == tab) {
                self.tab = tab;
                ui.focus = None;
            }
        }
        let body =
            Rect::new(r.x + PAD, r.y + HEADER_H + 8.0, r.w - 2.0 * PAD, r.h - HEADER_H - 8.0 - PAD);
        draw_line(r.x + PAD - 8.0, body.y - 6.0, r.x + r.w - PAD + 8.0, body.y - 6.0, 1.0, BORDER);
        let inner = match self.tab {
            Tab::Free => self.free(ui, config, online, body),
            Tab::Ladder => self.ladder(ui, config, online, progress, claimer, body),
            Tab::Daily => self.daily(ui, online, body),
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
            let max_lines = if card_h >= 96.0 {
                3
            } else if card_h >= 84.0 {
                2
            } else {
                1
            };
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
        let mut enter = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
        let mut y = area.y;
        label(ui, t("Your name"), y);
        let field = Rect::new(area.x, y + LABEL_H, area.w, CONTROL_H);
        enter |= ui.text_input(NAME_FIELD, field, &mut config.player_name, 20);
        if config.player_name.is_empty() && ui.focus != Some(NAME_FIELD) {
            placeholder(ui, field, t("Player"));
        }
        y += LABEL_H + CONTROL_H + GAP;
        label(ui, t("Aiming guide"), y);
        let guides = [t("Full"), t("Standard"), t("Short")];
        let sel = AimGuide::ALL.iter().position(|g| *g == config.guide).unwrap_or(1);
        let seg = Rect::new(area.x, y + LABEL_H, area.w, CONTROL_H);
        if let Some(i) = ui.segmented(seg, &guides, sel) {
            config.guide = AimGuide::ALL[i];
        }
        if ui.hovering(seg) {
            let hint = match config.guide {
                AimGuide::Full => t(
                    "Shows where the object ball and the cue ball go (never online or in rated games).",
                ),
                AimGuide::Standard => t("Shows the contact point; judging the cut is up to you."),
                AimGuide::Short => t("Only a short line from the cue ball."),
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
            ui.tooltip(t("When time runs out, the opponent gets ball in hand."), seg);
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
        let name_ok = snookered_proto::valid_username(self.ghost_name.trim());
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
            let beaten = rung < progress.beaten;
            let is_next = rung == progress.beaten && !progress.complete();
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
                tip = Some((t("Beat the rung below to unlock it.").to_string(), row));
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
            tw("Rung {n} of {total}", &[("n", &(selected + 1)), ("total", &ladder::RUNGS.len())]);
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
        let item = snookered_proto::shop::item(ladder::REWARD_ITEM)?;
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
        let granted =
            claimer.state == Claim::Granted || online.has_achievement(ladder::ACHIEVEMENT);
        let button_w = 130.0f32.min(w);
        let button = |y: f32| Rect::new(x, (y + 2.0).min(r.y + r.h - 34.0), button_w, 30.0);
        let _ = line_room;
        if granted || (progress.claimed && progress.pending.is_none()) {
            crate::icons::draw(Icon::Check, vec2(x + 7.0, y + 8.0), 14.0, GREEN_TEXT);
            let text = t("Yours: find it in the shop, under Cues.");
            let l = ui.ellipsize(text, w - 20.0, 13.0);
            ui.text(&l, x + 20.0, y, 13.0, GREEN_TEXT, false);
            return action;
        }
        if progress.pending.is_none() {
            let text = if progress.complete() {
                t("Beat the last rung again while signed in to claim it.")
            } else {
                t("Beat all ten opponents to earn it. It is sold nowhere.")
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
            say(ui, t("You beat the ladder! Sign in to claim your reward."), CREAM, &mut y);
            if ui.button(button(y), t("Sign in"), true) {
                action = Some(VersusAction::SignIn);
            }
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
        use snookered_proto::daily;
        let mut action = VersusAction::None;
        let today = daily::day_of(crate::util::unix_now());
        if online.signed_in() && self.daily_asked != Some(today) {
            self.daily_asked = Some(today);
            online.send(snookered_proto::ClientMsg::Daily);
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
                    online.send(snookered_proto::ClientMsg::DailyStart);
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

    /// The day's leaders.
    fn daily_board(
        &self,
        ui: &mut Ui,
        info: &snookered_proto::daily::Info,
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
fn settings(config: &Config, o: Opponent, rung: Option<u8>) -> MatchSettings {
    let mut s = MatchSettings::from_config(MatchMode::VsComputer(o), config);
    s.ladder = rung;
    s
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
