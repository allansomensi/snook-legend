//! Setting up a game of two players on one computer: the names, the
//! aiming guide and the shot clock. (Games against the computer are set up
//! on [`crate::versus`].)

use macroquad::prelude::*;

use crate::config::{AimGuide, Config};
use crate::i18n::t;
use crate::ui::{GOLD, MUTED, Ui};
use crate::{MatchMode, MatchSettings};

pub enum SetupAction {
    None,
    Back,
    Start(MatchSettings),
}

#[derive(Default)]
pub struct Setup;

/// Panel width and inner padding.
const WIDTH: f32 = 520.0;
const PAD: f32 = 28.0;
/// Title, from the panel's top to the first field.
const HEADER_H: f32 = 74.0;
/// A field: its label, then the control.
const LABEL_H: f32 = 20.0;
const CONTROL_H: f32 = 36.0;
const FIELD_GAP: f32 = 18.0;
/// From the last field to the panel's bottom (the buttons).
const FOOTER_H: f32 = 28.0 + 42.0 + 24.0;
const HINT: f32 = 12.0;

impl Setup {
    pub fn new() -> Setup {
        Setup
    }

    pub fn draw(&mut self, ui: &mut Ui, config: &mut Config) -> SetupAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
        let w = WIDTH.min(sw - 20.0);
        let inner = w - 2.0 * PAD;

        // The hints under the choices, which set how tall the panel is.
        let guide_hint = match config.guide {
            AimGuide::Full => t(
                "Shows where the object ball and the cue ball go (only in free games and practice).",
            ),
            AimGuide::Standard => t("Shows the contact point; judging the cut is up to you."),
            AimGuide::Short => t("Only a short line from the cue ball."),
            AimGuide::Off => t("No lines on the table: aim by eye, along the cue."),
        };
        let clock_hint = t("When time runs out, the opponent gets ball in hand.");
        let guide_lines = ui.wrap(guide_hint, inner, HINT);
        let clock_lines = ui.wrap(clock_hint, inner, HINT);
        let field_h = |hint: &[String]| {
            LABEL_H
                + CONTROL_H
                + if hint.is_empty() { 0.0 } else { 6.0 + hint.len() as f32 * (HINT + 3.0) - 3.0 }
        };
        let body = field_h(&[]) + field_h(&guide_lines) + field_h(&clock_lines) + 2.0 * FIELD_GAP;
        let h = HEADER_H + body + FOOTER_H;
        let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
        ui.panel(r);
        let x = r.x + PAD;
        let (icon, title) = (crate::icons::Icon::People, t("Two players"));
        crate::icons::draw(icon, vec2(x + 13.0, r.y + 35.0), 26.0, GOLD);
        ui.text(title, x + 38.0, r.y + 22.0, 24.0, GOLD, true);
        let mut y = r.y + HEADER_H;
        let label = |ui: &Ui, text: &str, x: f32, y: f32| ui.text(text, x, y, 13.0, MUTED, true);
        let hint = |ui: &Ui, lines: &[String], y: f32| {
            for (k, line) in lines.iter().enumerate() {
                ui.text(line, x, y + 6.0 + k as f32 * (HINT + 3.0), HINT, MUTED, false);
            }
        };
        // Enter starts the game, from a name field or anywhere else.
        let mut enter = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);

        // Names.
        let half = (inner - 12.0) / 2.0;
        label(ui, t("Player 1"), x, y);
        let first = Rect::new(x, y + LABEL_H, half, CONTROL_H);
        enter |= ui.text_input(10, first, &mut config.player_name, 20);
        if config.player_name.is_empty() && ui.focus != Some(10) {
            placeholder(ui, first, t("Player"));
        }
        label(ui, t("Player 2"), x + half + 12.0, y);
        let second = Rect::new(x + half + 12.0, y + LABEL_H, half, CONTROL_H);
        enter |= ui.text_input(11, second, &mut config.second_player_name, 20);
        if config.second_player_name.is_empty() && ui.focus != Some(11) {
            placeholder(ui, second, t("Player 2"));
        }
        y += field_h(&[]) + FIELD_GAP;

        // Aiming help.
        label(ui, t("Aiming guide"), x, y);
        let guides = [t("Full"), t("Standard"), t("Short")];
        let sel = AimGuide::ALL.iter().position(|g| *g == config.guide).unwrap_or(1);
        if let Some(i) = ui.segmented(Rect::new(x, y + LABEL_H, inner, CONTROL_H), &guides, sel) {
            config.guide = AimGuide::ALL[i];
        }
        hint(ui, &guide_lines, y + LABEL_H + CONTROL_H);
        y += field_h(&guide_lines) + FIELD_GAP;

        // Shot clock.
        label(ui, t("Time per shot"), x, y);
        let clocks = [t("No limit"), "30 s", "60 s"];
        let sel = match config.shot_clock {
            30 => 1,
            60 => 2,
            _ => 0,
        };
        if let Some(i) = ui.segmented(Rect::new(x, y + LABEL_H, inner, CONTROL_H), &clocks, sel) {
            config.shot_clock = [0, 30, 60][i];
        }
        hint(ui, &clock_lines, y + LABEL_H + CONTROL_H);

        let by = r.y + r.h - 24.0 - 42.0;
        let mut action = SetupAction::None;
        if ui.button(Rect::new(x, by, 150.0, 42.0), t("Back"), true)
            || (is_key_pressed(KeyCode::Escape) && ui.focus.is_none())
        {
            action = SetupAction::Back;
        }
        if ui.primary_button(Rect::new(x + inner - 220.0, by, 220.0, 42.0), t("Play")) || enter {
            action = SetupAction::Start(MatchSettings::from_config(MatchMode::HotSeat, config));
        }
        action
    }
}

/// Placeholders are fainter than typed text, so an empty field reads as empty.
const PLACEHOLDER: Color = Color::new(MUTED.r, MUTED.g, MUTED.b, 0.6);

/// Grey text in an empty field, where its text would be.
fn placeholder(ui: &Ui, field: Rect, text: &str) {
    let size = (field.h * 0.5).clamp(12.0, 18.0);
    ui.text(text, field.x + 8.0, field.y + (field.h - size) / 2.0, size, PLACEHOLDER, false);
}
