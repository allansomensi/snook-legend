//! The players a note on the lounge's board mentions (`@name`, filled in by
//! the server as in the chat): drawn in gold, a click opens their profile,
//! and a player mentioned in a note just pinned is told so.

use macroquad::prelude::*;
use snook_legend_proto::BoardNote;

use super::Online;
use crate::i18n::{t, tw};
use crate::ui::{GOLD, Ui};

/// A new note older than this (seconds) is not news: what a player finds
/// on the board when they sign in.
const NEWS: i64 = 120;

/// Draws a note's wrapped `lines` from (`x`, `y`), `step` apart, its
/// mentions in gold; the player whose name was clicked.
pub fn draw_lines(
    ui: &mut Ui,
    note: &BoardNote,
    lines: &[String],
    x: f32,
    y: f32,
    step: f32,
    color: Color,
) -> Option<String> {
    const SIZE: f32 = 14.0;
    let mut clicked = None;
    for (i, l) in lines.iter().enumerate() {
        let ly = y + i as f32 * step;
        if note.mentions.is_empty() {
            ui.text(l, x, ly, SIZE, color, false);
            continue;
        }
        let mut lx = x;
        for (span, mention) in crate::ui::mention_spans(l, &note.mentions) {
            let w = ui.text_size(&span, SIZE, mention).x;
            ui.text(&span, lx, ly, SIZE, if mention { GOLD } else { color }, mention);
            let hit = Rect::new(lx, ly - 1.0, w, step);
            if mention && ui.hovering(hit) {
                crate::paint::draw_line(lx, ly + SIZE + 1.0, lx + w, ly + SIZE + 1.0, 1.0, GOLD);
                if ui.clicked(hit) {
                    clicked = Some(span.trim_start_matches('@').to_string());
                }
            }
            lx += w;
        }
    }
    clicked
}

/// Whether `note` mentions `me`.
pub fn mentions(note: &BoardNote, me: &str) -> bool {
    note.mentions.iter().any(|m| m.eq_ignore_ascii_case(me))
}

impl Online {
    /// The board came again: a note just pinned that mentions this player
    /// is told as a chat mention is (a soft sound, a notice).
    pub(super) fn board_mentions(&mut self, notes: &[BoardNote]) {
        let Some(me) = self.me.as_ref().map(|m| m.username.clone()) else { return };
        let now = crate::game::unix_now();
        let new = notes.iter().find(|n| {
            mentions(n, &me)
                && now - n.at <= NEWS
                && !n.author.eq_ignore_ascii_case(&me)
                && !self.board.iter().any(|o| o.id == n.id)
        });
        let Some(n) = new else { return };
        let from = if n.author.is_empty() { t("Board").to_string() } else { n.author.clone() };
        let text = tw("{name} mentioned you", &[("name", &from)]);
        self.attention = Some(format!("{text}: {}", n.text));
        self.toast(text, true);
        self.invite_sound = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_mentions_by_username() {
        let note = BoardNote {
            id: 1,
            text: "Well done @Bia".into(),
            author: "boss".into(),
            at: 0,
            expires_at: None,
            kind: Default::default(),
            pinned: false,
            action: None,
            mentions: vec!["bia".into()],
        };
        assert!(mentions(&note, "BIA"));
        assert!(!mentions(&note, "boss"));
    }
}
