//! Reporting a player (from a profile or at the table): a reason, an
//! optional note, and a line saying which game goes with the report (the
//! server attaches the game being played between the two or their latest
//! one, so the moderators can watch it).

use macroquad::prelude::*;
use snookered_proto::{ClientMsg, Profile, REPORT_GAME_WINDOW_SECS, ReportReason};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::ranking::{PAD, text_y};
use crate::ui::{BORDER, GOLD, MUTED, Ui, rrect, rrect_lines};

const DETAILS_FIELD: u64 = 61;

pub struct ReportDialog {
    pub name: String,
    reason: usize,
    details: String,
    /// Which game goes with the report (or that none does).
    note: String,
    /// A game goes with it (the note is highlighted).
    attached: bool,
}

pub enum ReportOutcome {
    Open,
    Cancelled,
    Send(ClientMsg),
}

impl ReportDialog {
    /// Reporting the opponent at the table: this game goes with it.
    pub fn at_table(name: &str) -> ReportDialog {
        ReportDialog::new(
            name,
            t("This game goes with the report, so the moderators can watch it.").to_string(),
            true,
        )
    }

    /// Reporting from a profile: the latest game between the two, if recent.
    pub fn from_profile(p: &Profile, me: Option<&str>, now: i64) -> ReportDialog {
        let name = p.player.username.as_str();
        let recent = me.and_then(|me| {
            p.history.iter().find(|h| {
                h.opponent.eq_ignore_ascii_case(me) && h.at >= now - REPORT_GAME_WINDOW_SECS
            })
        });
        match recent {
            Some(h) => ReportDialog::new(
                name,
                tw(
                    "Your game with {name} ({date}) goes with the report, so the moderators can watch it.",
                    &[("name", &name), ("date", &crate::profile::date(h.at))],
                ),
                true,
            ),
            None => ReportDialog::new(
                name,
                tw(
                    "You haven't played {name} in the last day: the moderators see the recent chat.",
                    &[("name", &name)],
                ),
                false,
            ),
        }
    }

    fn new(name: &str, note: String, attached: bool) -> ReportDialog {
        ReportDialog { name: name.to_string(), reason: 0, details: String::new(), note, attached }
    }

    pub fn draw(&mut self, ui: &mut Ui) -> ReportOutcome {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.5));
        let w = 460.0f32.min(sw - 20.0);
        let inner = w - 2.0 * PAD;
        let lines = ui.wrap(&self.note, inner - 34.0, 13.0);
        let note_h = (lines.len() as f32 * 17.0 + 16.0).max(40.0);
        let n = ReportReason::ALL.len() as f32;
        let h = 24.0 + 26.0 + 20.0 + n * 28.0 + 8.0 + 34.0 + 12.0 + note_h + 20.0 + 40.0 + 24.0;
        let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
        ui.panel(r);
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        let x = r.x + PAD;
        let title = tw("Report {name}", &[("name", &self.name)]);
        let (title, size) = ui.fitted(&title, 22.0, inner, true);
        ui.text(&title, x, text_y(r.y + 24.0 + 13.0, size), size, GOLD, true);
        let mut y = r.y + 24.0 + 26.0 + 20.0;
        for (k, rr) in ReportReason::ALL.iter().enumerate() {
            if ui.radio(x, y, t(rr.name()), self.reason == k) {
                self.reason = k;
            }
            y += 28.0;
        }
        y += 8.0;
        let field = Rect::new(x, y, inner, 34.0);
        ui.text_input(DETAILS_FIELD, field, &mut self.details, 300);
        if self.details.is_empty() && ui.focus != Some(DETAILS_FIELD) {
            let hint = t("What happened? (optional)");
            ui.text(hint, x + 10.0, text_y(y + 17.0, 14.0), 14.0, MUTED, false);
        }
        y += 34.0 + 12.0;
        // Which game goes with it.
        let box_r = Rect::new(x, y, inner, note_h);
        let tint = if self.attached { GOLD } else { MUTED };
        rrect(box_r, 8.0, Color::new(tint.r, tint.g, tint.b, 0.07));
        rrect_lines(box_r, 8.0, 1.0, Color::new(BORDER.r, BORDER.g, BORDER.b, 0.7));
        let icon = if self.attached { Icon::Replay } else { Icon::People };
        crate::icons::draw(icon, vec2(x + 18.0, y + note_h / 2.0), 16.0, tint);
        let text_top = y + (note_h - lines.len() as f32 * 17.0) / 2.0;
        for (k, line) in lines.iter().enumerate() {
            ui.text(line, x + 34.0, text_top + k as f32 * 17.0, 13.0, crate::ui::CREAM, false);
        }
        // Cancel on the left, send on the right.
        let by = r.y + h - PAD - 40.0;
        let send_w = ui.text_size(t("Send report"), 15.0, true).x + 40.0;
        if ui.primary_button(Rect::new(r.x + w - PAD - send_w, by, send_w, 40.0), t("Send report"))
            || (is_key_pressed(KeyCode::Enter) && ui.focus == Some(DETAILS_FIELD))
        {
            ui.focus = None;
            return ReportOutcome::Send(ClientMsg::Report {
                username: self.name.clone(),
                reason: ReportReason::ALL[self.reason],
                details: self.details.clone(),
            });
        }
        if ui.button(Rect::new(x, by, 120.0, 40.0), t("Cancel"), true)
            || (is_key_pressed(KeyCode::Escape) && ui.focus.is_none())
        {
            ui.focus = None;
            return ReportOutcome::Cancelled;
        }
        ReportOutcome::Open
    }
}
