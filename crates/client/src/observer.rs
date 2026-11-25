//! The pro observer mode, for spectators: the camera they choose (from
//! above, the 3D view, following the cue ball, the broadcast that directs
//! itself), each shot's power and spin as it is played, and how many balls
//! each player has left.
//!
//! It shows what was played (from the shots the server passes on), never
//! where to aim.

use macroquad::prelude::*;

use snook_legend_core::{BallId, Group};

use crate::camera::{CameraMode, CameraRig};
use crate::game::{Match, speed_to_power};
use crate::i18n::t;
use crate::paint::draw_circle;
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

/// The cameras a spectator can choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Camera {
    /// Cuts between angles like a television broadcast.
    Broadcast,
    /// From above, the whole table.
    Top,
    /// The 3D view, moved by the viewer.
    Free,
    /// Behind the cue ball, turning with the shot.
    CueBall,
}

impl Camera {
    pub const ALL: [Camera; 4] = [Camera::Broadcast, Camera::Top, Camera::Free, Camera::CueBall];

    pub fn label(self) -> &'static str {
        match self {
            Camera::Broadcast => "Broadcast",
            Camera::Top => "Top",
            Camera::Free => "3D",
            Camera::CueBall => "Cue ball",
        }
    }
}

/// What the spectator chose.
#[derive(Debug, Default)]
pub struct Observer {
    /// The camera chosen (None: the usual one, from the settings).
    pub camera: Option<Camera>,
    /// Chosen this frame: the caller sets the camera up for it.
    pub changed: bool,
    /// The panel with the shot's numbers is shown.
    pub panel: bool,
}

impl Observer {
    /// Whether the camera follows the cue ball (the rig's `follow_cue`).
    pub fn follow_cue(&self) -> Option<bool> {
        self.camera.map(|c| c == Camera::CueBall)
    }

    /// Whether the broadcast directs the camera.
    pub fn broadcast(&self) -> Option<bool> {
        self.camera.map(|c| c == Camera::Broadcast)
    }

    /// Sets `rig` up for the camera just chosen.
    pub fn apply(&mut self, rig: &mut CameraRig) {
        if !std::mem::take(&mut self.changed) {
            return;
        }
        let Some(camera) = self.camera else { return };
        rig.follow_cue = camera == Camera::CueBall;
        rig.reset();
        rig.tv = camera == Camera::Broadcast;
        rig.mode = if camera == Camera::Top { CameraMode::Top } else { CameraMode::Aim };
    }

    /// The camera buttons, in `r` (the spectator's bar).
    pub fn draw_cameras(&mut self, ui: &mut Ui, r: Rect, broadcast_default: bool) {
        let shown =
            self.camera.unwrap_or(if broadcast_default { Camera::Broadcast } else { Camera::Top });
        let labels: Vec<&str> = Camera::ALL.iter().map(|c| t(c.label())).collect();
        let selected = Camera::ALL.iter().position(|c| *c == shown).unwrap_or(0);
        if let Some(k) = ui.segmented(r, &labels, selected) {
            self.camera = Some(Camera::ALL[k]);
            self.changed = true;
        }
        if ui.hovering(r) {
            ui.tooltip(t("Camera"), r);
        }
    }

    /// The shot's numbers and the balls left, on the right side of the
    /// table (spectators only).
    pub fn draw_panel(&mut self, ui: &mut Ui, m: &Match, top: f32) {
        if !m.is_spectating() {
            return;
        }
        let sw = crate::ui::vw();
        let w = 190.0;
        let toggle = Rect::new(sw - 12.0 - 30.0, top, 30.0, 30.0);
        let tip =
            if self.panel { t("Hide the shot's numbers") } else { t("Show the shot's numbers") };
        if ui.icon_button_ex(toggle, crate::icons::Icon::Sliders, tip, true, !self.panel) {
            self.panel = !self.panel;
        }
        if !self.panel {
            return;
        }
        let r = Rect::new(sw - 12.0 - w, top + 36.0, w, 214.0);
        ui.panel(r);
        ui.text(t("Last shot"), r.x + 12.0, r.y + 10.0, 13.0, GOLD, true);
        match &m.last_shot {
            Some(last) => {
                let max = last.start.max_cue_speed();
                let power = speed_to_power(last.shot.speed, max);
                // Power.
                ui.text(t("Power"), r.x + 12.0, r.y + 34.0, 12.0, MUTED, false);
                let bar = Rect::new(r.x + 12.0, r.y + 52.0, r.w - 24.0, 8.0);
                rrect(bar, 4.0, Color::new(1.0, 1.0, 1.0, 0.08));
                rrect(Rect::new(bar.x, bar.y, bar.w * power.clamp(0.0, 1.0), bar.h), 4.0, GOLD);
                let pct = format!("{}%", (power * 100.0).round() as i32);
                let pw = ui.text_size(&pct, 12.0, true).x;
                ui.text(&pct, r.x + r.w - 12.0 - pw, r.y + 34.0, 12.0, CREAM, true);
                // Spin: where the cue struck the ball.
                ui.text(t("Spin"), r.x + 12.0, r.y + 70.0, 12.0, MUTED, false);
                let c = vec2(r.x + 46.0, r.y + 122.0);
                draw_circle(c.x, c.y, 30.0, Color::new(0.95, 0.94, 0.9, 1.0));
                crate::paint::draw_line(
                    c.x - 30.0,
                    c.y,
                    c.x + 30.0,
                    c.y,
                    1.0,
                    Color::new(0.0, 0.0, 0.0, 0.2),
                );
                crate::paint::draw_line(
                    c.x,
                    c.y - 30.0,
                    c.x,
                    c.y + 30.0,
                    1.0,
                    Color::new(0.0, 0.0, 0.0, 0.2),
                );
                let tip = last.shot.tip;
                let dot = vec2(c.x + tip.x as f32 * 24.0, c.y - tip.y as f32 * 24.0);
                draw_circle(dot.x, dot.y, 5.0, Color::new(0.8, 0.15, 0.12, 1.0));
                let side = spin_words(tip.x, tip.y);
                let tx = r.x + 88.0;
                for (k, line) in side.iter().enumerate() {
                    ui.text(line, tx, r.y + 100.0 + k as f32 * 18.0, 12.0, CREAM, false);
                }
                let elevation = format!("{}°", last.shot.elevation.to_degrees().round() as i32);
                ui.text(t("Cue"), tx, r.y + 140.0, 12.0, MUTED, false);
                ui.text(&elevation, tx + 40.0, r.y + 140.0, 12.0, CREAM, true);
            }
            None => {
                ui.text(t("No shot yet."), r.x + 12.0, r.y + 40.0, 12.0, MUTED, false);
            }
        }
        // The balls each player has left.
        let y = r.y + 164.0;
        ui.text(t("Balls left"), r.x + 12.0, y, 12.0, MUTED, false);
        for seat in 0..2 {
            let left = balls_left(m, seat);
            let row = y + 18.0 + seat as f32 * 16.0;
            let (name, size) = ui.fitted(&m.names[seat], 11.0, 64.0, true);
            ui.text(&name, r.x + 12.0, row, size, CREAM, true);
            for k in 0..7 {
                let cx = r.x + 88.0 + k as f32 * 13.0;
                let filled = k < left;
                let color = if filled { GOLD } else { Color::new(1.0, 1.0, 1.0, 0.12) };
                draw_circle(cx, row + 6.0, 5.0, color);
            }
        }
        rrect_lines(r, 12.0, 1.0, BORDER);
    }
}

/// The balls of `seat`'s group still on the table (7 each on an open table;
/// the 8 is the last).
pub fn balls_left(m: &Match, seat: usize) -> usize {
    let game = &m.game;
    match game.group_of(seat) {
        Some(g) => game.remaining(g).len(),
        None => {
            // Open table: each group's balls still on the table, the more.
            let on = |g: Group| game.remaining(g).len();
            on(Group::Solids).max(on(Group::Stripes)).min(7)
        }
    }
    .min(if game.ball(BallId::EIGHT).in_play() { 7 } else { 0 })
}

/// The spin in words ("Top spin", "Left side"…).
fn spin_words(x: f64, y: f64) -> Vec<&'static str> {
    let mut out = Vec::new();
    if y > 0.15 {
        out.push(t("Top spin"));
    } else if y < -0.15 {
        out.push(t("Back spin"));
    }
    if x > 0.15 {
        out.push(t("Right side"));
    } else if x < -0.15 {
        out.push(t("Left side"));
    }
    if out.is_empty() {
        out.push(t("Centre ball"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spin_reads_as_words() {
        assert_eq!(spin_words(0.0, 0.0), ["Centre ball"]);
        assert_eq!(spin_words(0.5, -0.5), ["Back spin", "Right side"]);
        assert_eq!(spin_words(-0.3, 0.6), ["Top spin", "Left side"]);
    }
}
