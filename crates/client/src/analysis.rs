//! The profile's shot analysis: success per pocket on a small table seen
//! from above, where the object balls were (potted or missed) as a heat map,
//! and bars by distance and by kind of shot, each with how many shots it is
//! based on.

use macroquad::prelude::*;
use snook_legend_core::{PocketId, Table};
use snook_legend_proto::analysis::{GRID_H, GRID_W, ShotAnalysis, ShotKind, Tally};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::ranking::{empty_state, text_y};
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

/// Potted (warm gold) and missed (coral) read well on the dark cloth.
const POTTED: Color = Color::new(1.0, 0.80, 0.30, 1.0);
const MISSED: Color = Color::new(1.0, 0.45, 0.38, 1.0);
const CLOTH: Color = Color::new(0.05, 0.22, 0.15, 1.0);
const CUSHION: Color = Color::new(0.035, 0.16, 0.11, 1.0);
const RAIL: Color = Color::new(0.27, 0.16, 0.09, 1.0);
/// The heat map's colour at its strongest (a share of the busiest cell).
const HEAT_ALPHA: f32 = 0.85;

/// What the heat map shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Heat {
    #[default]
    Potted,
    Missed,
}

/// "61%", or a dash without shots.
fn rate(t: Tally) -> String {
    if t.attempts == 0 { "—".to_string() } else { format!("{}%", t.rate()) }
}

/// The analysis in `area`; `name` is the profile's owner, `own` whether
/// it is the viewer and `guest` whether it is a guest (local network).
pub fn draw(
    ui: &mut Ui,
    analysis: Option<&ShotAnalysis>,
    own: bool,
    guest: bool,
    name: &str,
    area: Rect,
    heat: &mut Heat,
) {
    let Some(a) = analysis else {
        let text = if guest {
            t("Only online accounts have a shot analysis.")
        } else {
            t("The shot analysis isn't available on this server yet.")
        };
        empty_state(ui, area, Icon::Star, text);
        return;
    };
    let pots = a.attempts();
    if !a.is_enough() {
        let more = snook_legend_proto::analysis::MIN_ATTEMPTS - pots.attempts;
        let text = if own {
            tw(
                "Play online to see how you pot: the analysis appears after {n} more shots at a pocket.",
                &[("n", &more)],
            )
        } else {
            tw("{name} hasn't played enough online games for an analysis yet.", &[("name", &name)])
        };
        empty_state(ui, area, Icon::Star, &text);
        return;
    }
    // The numbers it all rests on.
    let games =
        if a.games == 1 { t("1 game").to_string() } else { tw("{n} games", &[("n", &a.games)]) };
    let summary = tw(
        "{pct}% of {n} shots at a pocket potted · {games}",
        &[("pct", &pots.rate().to_string()), ("n", &pots.attempts.to_string()), ("games", &games)],
    );
    let summary = ui.ellipsize(&summary, area.w, 13.0);
    ui.text(&summary, area.x, area.y, 13.0, MUTED, false);

    let top = area.y + 26.0;
    let body = Rect::new(area.x, top, area.w, area.y + area.h - top);
    let gap = 26.0;
    let table_w = (body.w * 0.42).floor();
    let kinds_w = ((body.w - table_w - 2.0 * gap) * 0.53).floor();
    let rest_w = body.w - table_w - kinds_w - 2.0 * gap;
    table_column(ui, a, Rect::new(body.x, body.y, table_w, body.h), heat);
    kinds_column(ui, a, Rect::new(body.x + table_w + gap, body.y, kinds_w, body.h));
    side_column(ui, a, Rect::new(body.x + table_w + kinds_w + 2.0 * gap, body.y, rest_w, body.h));
}

fn heading(ui: &Ui, text: &str, x: f32, y: f32, w: f32) {
    let (text, size) = ui.fitted(text, 14.0, w, true);
    ui.text(&text, x, y, size, GOLD, true);
}

/// A labelled bar: the label on the left, the rate and its sample on the right.
fn bar_row(ui: &mut Ui, label: &str, t: Tally, r: Rect) {
    let mid = r.y + r.h / 2.0;
    let pct = rate(t);
    let n = format!("{}", t.attempts);
    let num_w = 46.0;
    let n_w = ui.text_size("0000", 11.0, false).x + 4.0;
    let label_w = if label.is_empty() { 0.0 } else { (r.w * 0.42).min(150.0) };
    let (label_t, size) = ui.fitted(label, 12.5, label_w - 6.0, false);
    let dim = t.attempts == 0;
    ui.text(&label_t, r.x, text_y(mid, size), size, if dim { MUTED } else { CREAM }, false);
    let track = Rect::new(r.x + label_w, mid - 4.0, r.w - label_w - num_w - n_w - 8.0, 8.0);
    rrect(track, 4.0, Color::new(1.0, 1.0, 1.0, 0.06));
    if t.attempts > 0 && t.made > 0 {
        let k = t.made as f32 / t.attempts as f32;
        rrect(Rect::new(track.x, track.y, (track.w * k).max(8.0), track.h), 4.0, GOLD);
    }
    let pw = ui.text_size(&pct, 13.0, true).x;
    let px = track.x + track.w + 6.0 + num_w - pw;
    ui.text(&pct, px, text_y(mid, 13.0), 13.0, if dim { MUTED } else { CREAM }, true);
    let nw = ui.text_size(&n, 11.0, false).x;
    ui.text(&n, r.x + r.w - nw, text_y(mid, 11.0), 11.0, MUTED, false);
    if ui.hovering(r) && !dim {
        let tip = tw(
            "{made} of {n} potted ({pct})",
            &[("made", &t.made.to_string()), ("n", &t.attempts.to_string()), ("pct", &pct)],
        );
        ui.tooltip(&tip, r);
    }
}

/// Success by kind of shot (the break is beside, with the safeties).
fn kinds_column(ui: &mut Ui, a: &ShotAnalysis, r: Rect) {
    heading(ui, t("By kind of shot"), r.x, r.y, r.w);
    // The sample column's heading, over the counts.
    let shots = t("shots");
    let sw = ui.text_size(shots, 10.5, false).x;
    ui.text(shots, r.x + r.w - sw, r.y + 3.0, 10.5, MUTED, false);
    let kinds: Vec<ShotKind> =
        ShotKind::ALL.into_iter().filter(|k| *k != ShotKind::Break).collect();
    let row_h = ((r.h - 26.0) / kinds.len() as f32).clamp(18.0, 28.0);
    for (k, kind) in kinds.iter().enumerate() {
        let row = Rect::new(r.x, r.y + 26.0 + k as f32 * row_h, r.w, row_h);
        bar_row(ui, t(kind.name()), a.kinds[kind.index()], row);
    }
}

/// Distance bands, then safeties and breaks.
fn side_column(ui: &mut Ui, a: &ShotAnalysis, r: Rect) {
    heading(ui, t("By distance"), r.x, r.y, r.w);
    let labels = [t("Short (under 0.75 m)"), t("Medium (to 1.5 m)"), t("Long (over 1.5 m)")];
    let mut y = r.y + 26.0;
    let row_h = 23.0;
    for (k, label) in labels.iter().enumerate() {
        // The label on its own line: the column is narrow.
        let tl = a.distances[k];
        let (label, size) = ui.fitted(label, 12.0, r.w, false);
        ui.text(&label, r.x, y, size, CREAM, false);
        let bar = Rect::new(r.x, y + 15.0, r.w, row_h - 6.0);
        bar_row(ui, "", tl, bar);
        y += 15.0 + row_h;
    }
    y += 6.0;
    let cards = [
        (t("Safeties"), a.safeties, t("left the opponent nothing")),
        (t("Breaks"), a.kinds[ShotKind::Break.index()], t("made a ball")),
    ];
    for (title, tally, what) in cards.iter() {
        let line = if tally.attempts == 0 {
            "—".to_string()
        } else {
            format!("{} {what}", rate(*tally))
        };
        let mut lines = ui.wrap(&line, r.w - 20.0, 12.0);
        lines.truncate(2);
        let c = Rect::new(r.x, y, r.w, 34.0 + lines.len() as f32 * 15.0);
        rrect(c, 8.0, Color::new(1.0, 1.0, 1.0, 0.04));
        let top = c.y + 18.0;
        let n = tally.attempts.to_string();
        ui.text(&n, c.x + 10.0, text_y(top, 20.0), 20.0, CREAM, true);
        let nw = ui.text_size(&n, 20.0, true).x;
        let (title, size) = ui.fitted(title, 12.0, c.w - nw - 26.0, true);
        ui.text(&title, c.x + 18.0 + nw, text_y(top, size), size, MUTED, true);
        for (k, l) in lines.iter().enumerate() {
            let l = ui.ellipsize(l, c.w - 20.0, 12.0);
            ui.text(&l, c.x + 10.0, c.y + 30.0 + k as f32 * 15.0, 12.0, CREAM, false);
        }
        y += c.h + 8.0;
    }
}

/// Where on the screen a point of the table is.
struct Board {
    cloth: Rect,
    length: f32,
    width: f32,
}

impl Board {
    fn at(&self, p: ::glam::DVec2) -> Vec2 {
        vec2(
            self.cloth.x + (p.x as f32 / self.length + 0.5) * self.cloth.w,
            // +y up, as the top view shows it.
            self.cloth.y + (0.5 - p.y as f32 / self.width) * self.cloth.h,
        )
    }
}

/// The table: per-pocket success at each pocket, the heat map on the cloth.
fn table_column(ui: &mut Ui, a: &ShotAnalysis, r: Rect, heat: &mut Heat) {
    heading(ui, t("Where and in which pocket"), r.x, r.y, r.w);
    let table = Table::default();
    let (len, wid) = (table.spec.length as f32, table.spec.width as f32);
    // Pills above and below the table for the pockets; the switch and the
    // colour scale under it.
    let pill_h = 20.0;
    let legend_h = 26.0;
    let avail =
        Rect::new(r.x, r.y + 24.0 + pill_h, r.w, r.h - 24.0 - 2.0 * pill_h - legend_h - 10.0);
    let rail_k = 0.075;
    let aspect = (len + 2.0 * rail_k * len) / (wid + 2.0 * rail_k * len);
    let fw = avail.w.min(avail.h * aspect);
    let fh = fw / aspect;
    let frame = Rect::new(avail.x + (avail.w - fw) / 2.0, avail.y + (avail.h - fh) / 2.0, fw, fh);
    let rail = rail_k * len / (len + 2.0 * rail_k * len) * fw;
    let cloth =
        Rect::new(frame.x + rail, frame.y + rail, frame.w - 2.0 * rail, frame.h - 2.0 * rail);
    let board = Board { cloth, length: len, width: wid };
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
    // The head string, faintly.
    let hx = cloth.x + cloth.w * 0.25;
    draw_line(hx, cloth.y, hx, cloth.y + cloth.h, 1.0, Color::new(1.0, 1.0, 1.0, 0.08));
    // The heat map.
    let value = |t: &Tally| match heat {
        Heat::Potted => t.made,
        Heat::Missed => t.missed(),
    };
    let color = if *heat == Heat::Potted { POTTED } else { MISSED };
    let busiest = a.heat.iter().map(value).max().unwrap_or(0).max(1) as f32;
    let (cw, ch) = (cloth.w / GRID_W as f32, cloth.h / GRID_H as f32);
    let mut tip = None;
    for row in 0..GRID_H {
        for col in 0..GRID_W {
            let Some(cell) = a.heat.get(row * GRID_W + col) else { continue };
            // Row 0 is the -y side: the bottom of the drawing.
            let c = Rect::new(
                cloth.x + col as f32 * cw,
                cloth.y + (GRID_H - 1 - row) as f32 * ch,
                cw,
                ch,
            );
            let v = value(cell);
            if v > 0 {
                // Square root: a few busy cells don't wash out the rest.
                let k = (v as f32 / busiest).sqrt();
                let inset = 1.0;
                rrect(
                    Rect::new(c.x + inset, c.y + inset, c.w - 2.0 * inset, c.h - 2.0 * inset),
                    3.0,
                    heat_color(color, k),
                );
            }
            if ui.hovering(c) && cell.attempts > 0 {
                tip = Some((*cell, c));
            }
        }
    }
    ui.block(cloth);
    // Pockets, and how often they were potted into.
    for p in &table.pockets {
        let at = board.at(p.hole_center);
        let radius = (rail * 0.42).max(4.0);
        draw_circle(at.x, at.y, radius, Color::new(0.02, 0.01, 0.0, 1.0));
        draw_circle_lines_c(at, radius, Color::new(0.75, 0.62, 0.36, 0.9));
        let tally = a.pockets[p.id.index()];
        let label = rate(tally);
        let n = tally.attempts.to_string();
        let lw = ui.text_size(&label, 12.0, true).x;
        let nw = ui.text_size(&n, 10.5, false).x;
        let w = lw + nw + 22.0;
        let above = p.mouth_center.y > 0.0;
        let y = if above { frame.y - pill_h - 2.0 } else { frame.y + frame.h + 2.0 };
        let x = if p.id.is_side() {
            at.x - w / 2.0
        } else if p.mouth_center.x < 0.0 {
            frame.x
        } else {
            frame.x + frame.w - w
        };
        let pr = Rect::new(x, y, w, pill_h);
        rrect(pr, pill_h / 2.0, Color::new(0.0, 0.0, 0.0, 0.35));
        rrect_lines(pr, pill_h / 2.0, 1.0, Color::new(BORDER.r, BORDER.g, BORDER.b, 0.8));
        let mid = pr.y + pr.h / 2.0;
        ui.text(
            &label,
            pr.x + 9.0,
            text_y(mid, 12.0),
            12.0,
            if tally.attempts > 0 { CREAM } else { MUTED },
            true,
        );
        ui.text(&n, pr.x + 13.0 + lw, text_y(mid, 10.5), 10.5, MUTED, false);
        if ui.hovering(pr) {
            let tip = tw(
                "{pocket}: {made} of {n} potted",
                &[
                    ("pocket", &t(pocket_name(p.id)).to_string()),
                    ("made", &tally.made.to_string()),
                    ("n", &tally.attempts.to_string()),
                ],
            );
            ui.tooltip(&tip, pr);
        }
    }
    // Under the table: what the colours show (a switch), and the scale.
    let ly = frame.y + frame.h + pill_h + 10.0;
    let options = [t("Potted"), t("Missed")];
    let cell_w = options.iter().map(|l| ui.text_size(l, 12.0, true).x + 18.0).fold(0.0, f32::max);
    let seg = Rect::new(r.x, ly, cell_w * 2.0, 26.0);
    if let Some(k) = ui.segmented(seg, &options, *heat as usize) {
        *heat = if k == 0 { Heat::Potted } else { Heat::Missed };
    }
    if ui.hovering(seg) {
        let what = if *heat == Heat::Potted {
            t("Where the potted balls were")
        } else {
            t("Where the missed balls were")
        };
        ui.tooltip(what, seg);
    }
    let steps = 5;
    let sw = 12.0;
    let scale_w = steps as f32 * (sw + 2.0);
    let mid = ly + 13.0;
    let few = t("few");
    let many = t("many");
    let many_w = ui.text_size(many, 11.0, false).x;
    let sx = r.x + r.w - many_w - 6.0 - scale_w;
    let few_w = ui.text_size(few, 11.0, false).x;
    // The words only where they fit beside the switch.
    let words = sx - few_w - 6.0 > seg.x + seg.w + 10.0;
    if words {
        ui.text(few, sx - few_w - 6.0, text_y(mid, 11.0), 11.0, MUTED, false);
    }
    for k in 0..steps {
        let kk = (k + 1) as f32 / steps as f32;
        rrect(
            Rect::new(sx + k as f32 * (sw + 2.0), mid - 6.0, sw, 12.0),
            2.0,
            heat_color(color, kk),
        );
    }
    if words {
        ui.text(many, sx + scale_w + 4.0, text_y(mid, 11.0), 11.0, MUTED, false);
    }
    let scale = Rect::new(sx, mid - 8.0, scale_w, 16.0);
    if ui.hovering(scale) {
        ui.tooltip(&format!("{few} → {many}"), scale);
    }
    if let Some((cell, c)) = tip {
        rrect_lines(c, 3.0, 1.5, GOLD);
        let text = tw(
            "{made} potted · {missed} missed ({pct})",
            &[
                ("made", &cell.made.to_string()),
                ("missed", &cell.missed().to_string()),
                ("pct", &rate(cell)),
            ],
        );
        ui.tooltip(&text, c);
    }
}

/// A heat cell's colour: dim (`k` near 0) to the full `color` (`k` = 1).
fn heat_color(color: Color, k: f32) -> Color {
    // Darker to full colour (mixing with the green cloth would muddy it).
    let k = 0.32 + 0.68 * k.clamp(0.0, 1.0);
    Color::new(color.r * k, color.g * k, color.b * k, HEAT_ALPHA + (1.0 - HEAT_ALPHA) * k)
}

fn draw_circle_lines_c(c: Vec2, r: f32, color: Color) {
    crate::paint::draw_circle_lines(c.x, c.y, r, 1.2, color);
}

/// The pocket's name, for people (translated by the caller).
pub fn pocket_name(p: PocketId) -> &'static str {
    match p {
        PocketId::HeadLeft => "Top left corner",
        PocketId::HeadRight => "Bottom left corner",
        PocketId::SideLeft => "Top side pocket",
        PocketId::SideRight => "Bottom side pocket",
        PocketId::FootLeft => "Top right corner",
        PocketId::FootRight => "Bottom right corner",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_read_well() {
        assert_eq!(rate(Tally::default()), "—");
        assert_eq!(rate(Tally { attempts: 3, made: 2 }), "66%");
    }

    #[test]
    fn the_board_puts_pockets_at_the_corners() {
        let cloth = Rect::new(10.0, 20.0, 200.0, 100.0);
        let table = Table::default();
        let board =
            Board { cloth, length: table.spec.length as f32, width: table.spec.width as f32 };
        // HeadLeft is at -x, +y: the top left of the drawing.
        let hl = board.at(table.pocket(PocketId::HeadLeft).mouth_center);
        let fr = board.at(table.pocket(PocketId::FootRight).mouth_center);
        assert!(hl.x < 30.0 && hl.y < 40.0, "{hl}");
        assert!(fr.x > 190.0 && fr.y > 100.0, "{fr}");
        let side = board.at(table.pocket(PocketId::SideRight).mouth_center);
        assert!((side.x - 110.0).abs() < 0.5 && side.y > 100.0, "{side}");
    }
}
