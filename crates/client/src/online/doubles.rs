//! Doubles in the lounge: a partner invited among friends, the pair's
//! "Play now" and tables, and the line-up of a doubles game (who plays with
//! whom, and who holds the cue: see `snook_legend_proto::doubles`).
//!
//! A doubles game is played as any online game, a team per seat: the game
//! lets this player hold the cue only on their own team's visits that are
//! theirs ([`apply_lineup`]); their partner's visits they watch, as they
//! watch the other team's.

use macroquad::prelude::*;
use snook_legend_core::Rules;
use snook_legend_proto::doubles::{Lineup, Pair, Table};
use snook_legend_proto::{ClientMsg, LeaderboardEntry, PerRules, ServerMsg};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use super::{Glyph, Online, button_w, empty_state, fit_text, row_card};
use crate::game::{Controller, Match};
use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

/// The player's doubles as the server last told them.
#[derive(Default)]
pub struct DoublesView {
    pub pair: Option<Pair>,
    /// The doubles tables waiting for a pair.
    pub tables: Vec<Table>,
    /// Friends asking this player to be their partner, oldest first.
    pub invites: Vec<String>,
    /// The friend this player asked, waiting for the answer.
    pub sent: Option<String>,
    /// The doubles ranking of each of the rules, and whether it came.
    pub leaderboard: PerRules<Vec<LeaderboardEntry>>,
    pub leaderboard_loaded: PerRules<bool>,
    /// The line-up of the doubles game being played (or watched): it comes
    /// just before the game itself.
    pub lineup: Option<Lineup>,
    /// The first row of the friends' list shown.
    first: crate::ui::Scroll,
}

/// Puts a doubles game's line-up on its table: the teams' names, and the
/// cue in this player's hands only on their own visits.
pub fn apply_lineup(m: &mut Match, lineup: &Lineup, me: &str) {
    if m.match_id() != lineup.match_id {
        return;
    }
    let Some(o) = m.online.as_mut() else { return };
    let watching = o.spectator;
    let seat = o.seat.min(1);
    o.doubles = Some(lineup.clone());
    o.me = Some(me.to_string());
    // Someone of the four left the table after the game: no rematch.
    if lineup.teams.iter().flatten().any(|p| !p.connected) && o.result.is_some() {
        o.opponent_left = true;
    }
    m.names = [lineup.team_name(0), lineup.team_name(1)];
    if !watching {
        let mine = lineup.shooter_of(seat).username.eq_ignore_ascii_case(me);
        m.controllers[seat] = if mine { Controller::Human } else { Controller::Remote };
        m.controllers[1 - seat] = Controller::Remote;
    }
}

impl Online {
    /// Doubles messages from the server: None when handled here.
    pub(super) fn doubles_message(
        &mut self,
        msg: ServerMsg,
        current: &mut Option<Match>,
    ) -> Option<ServerMsg> {
        match msg {
            ServerMsg::DoublesInvited { from } => {
                if !self.doubles.invites.iter().any(|f| f.eq_ignore_ascii_case(&from)) {
                    self.doubles.invites.push(from.clone());
                }
                self.invite_sound = true;
                self.attention =
                    Some(tw("{name} asks you to be their doubles partner.", &[("name", &from)]));
                self.toast(
                    tw("{name} asks you to be their doubles partner.", &[("name", &from)]),
                    true,
                );
            }
            ServerMsg::DoublesInviteSent { to } => self.doubles.sent = Some(to),
            ServerMsg::DoublesPair { pair } => {
                if let Some(p) = &pair
                    && self
                        .doubles
                        .pair
                        .as_ref()
                        .is_none_or(|old| old.partner.username != p.partner.username)
                {
                    let name = p.partner.username.clone();
                    self.toast(tw("{name} is your doubles partner.", &[("name", &name)]), true);
                }
                if pair.is_some() {
                    self.doubles.sent = None;
                    self.doubles.invites.clear();
                }
                self.doubles.pair = pair;
            }
            ServerMsg::DoublesTables { tables } => self.doubles.tables = tables,
            ServerMsg::DoublesLeaderboard { rules, entries } => {
                *self.doubles.leaderboard.get_mut(rules) = entries;
                *self.doubles.leaderboard_loaded.get_mut(rules) = true;
            }
            ServerMsg::DoublesLineup { lineup } => {
                let me = self.username.clone();
                if let Some(m) = current.as_mut() {
                    apply_lineup(m, &lineup, &me);
                }
                self.doubles.lineup = Some(lineup);
            }
            // The answer to an invitation to be partners comes as any
            // invitation's: the one waiting is over.
            ServerMsg::InviteDeclined { ref by }
                if self.doubles.sent.as_ref().is_some_and(|s| s.eq_ignore_ascii_case(by)) =>
            {
                self.doubles.sent = None;
                return Some(msg);
            }
            ServerMsg::InviteCancelled { ref from } => {
                self.doubles.invites.retain(|f| !f.eq_ignore_ascii_case(from));
                return Some(msg);
            }
            other => return Some(other),
        }
        None
    }

    /// The game just set up from `MatchStarted`: its line-up, if it is a
    /// doubles game (the line-up came first).
    pub(super) fn attach_lineup(&self, current: &mut Option<Match>) {
        if let (Some(m), Some(lineup)) = (current.as_mut(), self.doubles.lineup.as_ref()) {
            apply_lineup(m, lineup, &self.username);
        }
    }

    /// The Doubles tab of the lounge.
    pub(super) fn draw_doubles(&mut self, ui: &mut Ui, area: Rect) {
        let mut y = area.y;
        // Friends asking to be partners.
        for from in self.doubles.invites.clone() {
            let r = Rect::new(area.x, y, area.w, 52.0);
            rrect(r, 8.0, Color::new(0.18, 0.12, 0.06, 0.95));
            rrect_lines(r, 8.0, 1.5, GOLD);
            let (yes, no) = (t("Accept"), t("Decline"));
            let bw = button_w(ui, yes, 32.0).max(button_w(ui, no, 32.0)).max(84.0);
            let text = tw("{name} asks you to be their doubles partner.", &[("name", &from)]);
            let (text, size) = fit_text(ui, &text, 14.0, r.w - 2.0 * bw - 40.0, false);
            ui.text(&text, r.x + 14.0, r.y + (r.h - size) / 2.0, size, CREAM, false);
            let b2 = Rect::new(r.x + r.w - bw - 10.0, r.y + 10.0, bw, 32.0);
            let b1 = Rect::new(b2.x - bw - 8.0, r.y + 10.0, bw, 32.0);
            if ui.primary_button(b1, yes) {
                self.send(ClientMsg::DoublesAnswer { from: from.clone(), accept: true });
                self.doubles.invites.retain(|f| f != &from);
            }
            if ui.button(b2, no, true) {
                self.send(ClientMsg::DoublesAnswer { from: from.clone(), accept: false });
                self.doubles.invites.retain(|f| f != &from);
            }
            y += r.h + 10.0;
        }
        match self.doubles.pair.clone() {
            None => y = self.draw_no_partner(ui, area, y),
            Some(pair) => y = self.draw_pair(ui, area, y, &pair),
        }
        draw_line(area.x, y, area.x + area.w, y, 1.0, BORDER);
        y += 12.0;
        self.draw_doubles_tables(ui, Rect::new(area.x, y, area.w, area.y + area.h - y));
    }

    /// No partner yet: what doubles are, and the friends online to ask.
    fn draw_no_partner(&mut self, ui: &mut Ui, area: Rect, mut y: f32) -> f32 {
        let say = |ui: &Ui, text: &str, size: f32, color: Color, y: &mut f32| {
            for l in ui.wrap(text, area.w - 4.0, size) {
                ui.text(&l, area.x + 2.0, *y, size, color, false);
                *y += size + 5.0;
            }
        };
        say(
            ui,
            t(
                "Two against two. Your team's visits to the table go to each of you in turn: you keep the cue while you pot, and your partner takes the next visit.",
            ),
            14.0,
            CREAM,
            &mut y,
        );
        y += 6.0;
        if let Some(to) = self.doubles.sent.clone() {
            let r = Rect::new(area.x, y, area.w, 44.0);
            rrect(r, 8.0, Color::new(1.0, 1.0, 1.0, 0.04));
            crate::ui::spinner(vec2(r.x + 18.0, r.y + 22.0), 7.0, GOLD);
            let cancel = t("Cancel");
            let bw = button_w(ui, cancel, 30.0).max(84.0);
            let text = tw("Waiting for {name}'s answer…", &[("name", &to)]);
            let (text, size) = fit_text(ui, &text, 14.0, r.w - bw - 60.0, false);
            ui.text(&text, r.x + 34.0, r.y + (r.h - size) / 2.0, size, CREAM, false);
            if ui.button(Rect::new(r.x + r.w - bw - 7.0, r.y + 7.0, bw, 30.0), cancel, true) {
                self.send(ClientMsg::DoublesLeave);
                self.doubles.sent = None;
            }
            return y + r.h + 12.0;
        }
        ui.text(t("Ask a friend to be your partner"), area.x + 2.0, y, 13.0, MUTED, true);
        y += 22.0;
        let me = self.username.clone();
        let friends: Vec<_> = self
            .friends
            .friends
            .iter()
            .filter(|f| f.online && !f.player.username.eq_ignore_ascii_case(&me))
            .cloned()
            .collect();
        let list_h = (area.y + area.h - y - 150.0).max(52.0);
        let list = Rect::new(area.x, y, area.w, list_h);
        if friends.is_empty() {
            let text = if self.friends.friends.is_empty() {
                t("Add friends in the Friends tab: you play doubles with a friend.")
            } else {
                t("None of your friends is online right now.")
            };
            say(ui, text, 13.0, MUTED, &mut y);
            return y + 8.0;
        }
        let row_h = 46.0;
        let view = ui.list_begin(list, &mut self.doubles.first, friends.len(), row_h);
        let ask = t("Invite");
        let bw = button_w(ui, ask, 30.0).max(84.0);
        for k in view.rows() {
            let f = &friends[k];
            let r = Rect::new(list.x, view.y(k), list.w - 12.0, row_h - 6.0);
            row_card(ui, r, false);
            ui.player_name(&f.player, r.x + 12.0, r.y + 11.0, 15.0, r.w - bw - 36.0);
            let b = Rect::new(r.x + r.w - bw - 6.0, r.y + 5.0, bw, r.h - 10.0);
            if ui.button(b, ask, true) {
                self.send(ClientMsg::DoublesInvite { to: f.player.username.clone() });
            }
        }
        ui.scroll_end(view, &mut self.doubles.first);
        list.y + (friends.len() as f32 * row_h).min(list.h) + 8.0
    }

    /// With a partner: who, and the two ways to find another pair.
    fn draw_pair(&mut self, ui: &mut Ui, area: Rect, mut y: f32, pair: &Pair) -> f32 {
        let r = Rect::new(area.x, y, area.w, 58.0);
        rrect(r, 8.0, Color::new(0.10, 0.30, 0.18, 0.55));
        rrect_lines(r, 8.0, 1.5, Color::new(0.35, 0.8, 0.5, 0.8));
        crate::icons::draw(Icon::People, vec2(r.x + 22.0, r.y + r.h / 2.0), 22.0, CREAM);
        let leave = t("Leave the pair");
        let bw = button_w(ui, leave, 32.0).max(96.0);
        ui.text(t("Your partner"), r.x + 44.0, r.y + 9.0, 12.0, MUTED, false);
        ui.player_name(&pair.partner, r.x + 44.0, r.y + 26.0, 16.0, r.w - bw - 70.0);
        if ui.button(Rect::new(r.x + r.w - bw - 12.0, r.y + 13.0, bw, 32.0), leave, true) {
            self.send(ClientMsg::DoublesLeave);
        }
        y += r.h + 12.0;
        let rules = self.rules;
        if let Some(search) = pair.searching {
            let r = Rect::new(area.x, y, area.w, 48.0);
            rrect(r, 8.0, Color::new(0.18, 0.12, 0.06, 0.95));
            rrect_lines(r, 8.0, 1.5, GOLD);
            crate::ui::spinner(vec2(r.x + 22.0, r.y + 24.0), 7.0, GOLD);
            let cancel = t("Cancel");
            let bw = button_w(ui, cancel, 32.0).max(90.0);
            let text = tw("Looking for another pair… {secs} s", &[("secs", &search.waited_secs)]);
            let (text, size) = fit_text(ui, &text, 15.0, r.w - bw - 70.0, false);
            ui.text(&text, r.x + 40.0, r.y + (r.h - size) / 2.0, size, CREAM, false);
            if ui.button(Rect::new(r.x + r.w - bw - 8.0, r.y + 8.0, bw, 32.0), cancel, true) {
                self.send(ClientMsg::DoublesQueueLeave);
            }
            return y + r.h + 12.0;
        }
        if pair.table.is_some() {
            let r = Rect::new(area.x, y, area.w, 48.0);
            rrect(r, 8.0, Color::new(0.10, 0.30, 0.18, 0.55));
            let close = t("Close");
            let bw = button_w(ui, close, 32.0).max(90.0);
            let text = t("Your doubles table is open: waiting for a pair…");
            let (text, size) = fit_text(ui, text, 15.0, r.w - bw - 40.0, false);
            ui.text(&text, r.x + 14.0, r.y + (r.h - size) / 2.0, size, CREAM, false);
            if ui.button(Rect::new(r.x + r.w - bw - 8.0, r.y + 8.0, bw, 32.0), close, true) {
                self.send(ClientMsg::DoublesCloseTable);
            }
            return y + r.h + 12.0;
        }
        if !self.lan {
            let seg_w = (area.w * 0.55).min(280.0);
            self.rated_toggle(ui, Rect::new(area.x, y, seg_w, 32.0));
            let cx = area.x + seg_w + 14.0;
            crate::rulesui::tag(ui, rules, cx, y + 16.0, 13.0);
            y += 32.0 + 12.0;
        }
        let rated = self.rated_games && !self.lan;
        let half = (area.w - 8.0) / 2.0;
        if ui.primary_button(Rect::new(area.x, y, half, 36.0), t("Play now")) {
            self.send(ClientMsg::DoublesQueue { rated, rules });
        }
        if ui.button(Rect::new(area.x + half + 8.0, y, half, 36.0), t("Open a doubles table"), true)
        {
            self.send(ClientMsg::DoublesOpenTable { rated, rules });
        }
        y + 36.0 + 12.0
    }

    /// The other pairs' tables.
    fn draw_doubles_tables(&mut self, ui: &mut Ui, list: Rect) {
        let mine = self.doubles.pair.as_ref().and_then(|p| p.table);
        let others: Vec<Table> =
            self.doubles.tables.iter().filter(|t| Some(t.id) != mine).cloned().collect();
        if others.is_empty() {
            let text = t(
                "No doubles tables open. With a partner, open one and another pair can sit down.",
            );
            empty_state(ui, list, Glyph::Icon(Icon::Table), text, None);
            return;
        }
        let paired = self.doubles.pair.is_some();
        let sit = t("Sit down");
        let bw = button_w(ui, sit, 32.0).max(96.0);
        let row_h = 52.0;
        let visible = (list.h / row_h).floor().max(1.0) as usize;
        for (k, table) in others.iter().take(visible).enumerate() {
            let r = Rect::new(list.x, list.y + k as f32 * row_h, list.w, row_h - 6.0);
            row_card(ui, r, false);
            crate::icons::draw(Icon::Table, vec2(r.x + 22.0, r.y + r.h / 2.0), 22.0, CREAM);
            let names = format!("{} & {}", table.team[0].username, table.team[1].username);
            let (names, size) = fit_text(ui, &names, 15.0, r.w - bw - 70.0, true);
            ui.text(&names, r.x + 44.0, r.y + 6.0, size, CREAM, true);
            let mut px = r.x + 44.0;
            let (label, color) = if table.rated {
                (t("rated"), GOLD)
            } else {
                (t("casual"), Color::new(0.6, 0.8, 1.0, 1.0))
            };
            px += ui.pill(label, px, r.y + 25.0, color) + 8.0;
            px += crate::rulesui::tag(ui, table.rules, px, r.y + 33.5, 12.0) + 10.0;
            ui.text(&table.rating.to_string(), px, r.y + 26.0, 12.0, MUTED, false);
            let b = Rect::new(r.x + r.w - bw - 7.0, r.y + 7.0, bw, r.h - 14.0);
            if ui.button(b, sit, paired) {
                self.send(ClientMsg::DoublesJoinTable { id: table.id });
            }
            if !paired && ui.hovering(b) {
                ui.tooltip(t("Invite a friend to be your partner first."), b);
            }
        }
    }

    /// The doubles ranking of `rules` (asked for once per visit).
    pub fn request_doubles_leaderboard(&mut self, rules: Rules) {
        *self.doubles.leaderboard_loaded.get_mut(rules) = false;
        self.send(ClientMsg::DoublesLeaderboard { rules });
    }
}

/// The teams' colours (as the seats' on the charts).
const TEAM_COLORS: [Color; 2] =
    [Color::new(0.91, 0.75, 0.38, 1.0), Color::new(0.45, 0.72, 1.0, 1.0)];

/// A doubles game's four players under the top bar: each team in its
/// colour, the player holding the cue lit, and who of the other team takes
/// its next visit marked; once a rated game is over, each one's change.
pub fn draw_lineup(ui: &mut Ui, m: &Match, top: f32) {
    let Some(lineup) = m.online.as_ref().and_then(|o| o.doubles.as_ref()) else { return };
    let at_table = m.game.current_player();
    let size = 13.0;
    let name_w = lineup
        .teams
        .iter()
        .flatten()
        .map(|p| ui.text_size(&p.username, size, true).x)
        .fold(60.0, f32::max)
        .min(150.0);
    let w = 34.0 + name_w + 74.0;
    let r = Rect::new(12.0, top + 46.0, w, 2.0 * 46.0 + 4.0);
    rrect(r, 8.0, Color::new(0.0, 0.0, 0.0, 0.5));
    for (team, &color) in TEAM_COLORS.iter().enumerate() {
        let y0 = r.y + 6.0 + team as f32 * 46.0;
        rrect(Rect::new(r.x + 6.0, y0 + 2.0, 4.0, 36.0), 2.0, color);
        for place in 0..2 {
            let p = &lineup.teams[team][place];
            let y = y0 + place as f32 * 20.0;
            let shooter = lineup.shooter[team] == place;
            let holds = shooter && team == at_table && m.game.game_over().is_none();
            if holds {
                let pulse = 0.6 + 0.4 * (get_time() as f32 * 4.0).sin();
                draw_circle(
                    r.x + 20.0,
                    y + 10.0,
                    4.5,
                    Color::new(color.r, color.g, color.b, pulse),
                );
            }
            let c = if !p.connected {
                MUTED
            } else if holds {
                CREAM
            } else {
                Color::new(CREAM.r, CREAM.g, CREAM.b, 0.75)
            };
            let (name, s) = fit_text(ui, &p.username, size, name_w, holds);
            ui.text(&name, r.x + 30.0, y + 3.0, s, c, holds);
            let tag = if !p.connected {
                t("away")
            } else if holds {
                t("at the table")
            } else if shooter && team != at_table && m.game.game_over().is_none() {
                t("next")
            } else {
                ""
            };
            if !tag.is_empty() {
                let (tag, s) = fit_text(ui, tag, 11.0, 70.0, false);
                ui.text(&tag, r.x + 34.0 + name_w, y + 5.0, s, MUTED, false);
            }
        }
    }
    if let Some(deltas) = lineup.deltas {
        let mut y = r.y + r.h + 6.0;
        for (team, d) in deltas.iter().enumerate() {
            for (place, delta) in d.iter().enumerate() {
                let p = &lineup.teams[team][place];
                let text = format!("{} {:+} → {}", p.username, delta, p.rating);
                let (text, s) = fit_text(ui, &text, 12.0, w, false);
                ui.text(&text, r.x, y, s, TEAM_COLORS[team], false);
                y += 16.0;
            }
        }
    }
}
