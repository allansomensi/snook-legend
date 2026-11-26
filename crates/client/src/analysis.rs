//! The shot analysis, on the player's own profile (a VIP feature): how they
//! pot pocket by pocket on a small table seen from above, where the object
//! balls were (potted or missed) as a heat map, by kind of shot (the cut
//! angles with their league's mark) and by distance, their safeties and
//! breaks, how it went week after week, and how they compare with the
//! players of their league, each with how many shots it rests on. A
//! player who isn't VIP sees the same page with made-up numbers, dimmed,
//! and what it is.
//!
//! It explains the player's game; nothing here helps at the table.

use macroquad::prelude::*;
use snook_legend_core::{PocketId, Table};
use snook_legend_proto::advanced::{AdvancedStats, CUT_KINDS, Sums, Week, week_start};
use snook_legend_proto::analysis::{
    GRID_H, GRID_W, MIN_ATTEMPTS, POCKETS, ShotAnalysis, ShotKind, Tally,
};

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
/// Ahead of the league, and behind it.
const GOOD: Color = Color::new(0.45, 0.88, 0.5, 1.0);
const BAD: Color = Color::new(1.0, 0.5, 0.42, 1.0);

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

/// The share potted (None without shots).
fn share(t: Tally) -> Option<f32> {
    (t.attempts > 0).then(|| t.made.min(t.attempts) as f32 / t.attempts as f32)
}

/// The page in `area`, from what the server sent (None: on its way);
/// `guest`: a guest's profile (local network). True when the player asks
/// to become VIP.
pub fn draw(
    ui: &mut Ui,
    stats: Option<&AdvancedStats>,
    guest: bool,
    area: Rect,
    heat: &mut Heat,
) -> bool {
    if guest {
        empty_state(ui, area, Icon::Star, t("Only online accounts have a shot analysis."));
        return false;
    }
    let Some(stats) = stats else {
        crate::online::loading(ui, area);
        return false;
    };
    if !stats.vip {
        // The page with made-up numbers, dimmed and still, under the pitch.
        let saved = crate::paint::opacity();
        crate::paint::set_opacity(saved * 0.28);
        let input = ui.suspend_input();
        page(ui, &sample_analysis(), &sample(), area, heat);
        ui.resume_input(input);
        crate::paint::set_opacity(saved);
        return pitch(ui, area);
    }
    let none = ShotAnalysis::default();
    let a = stats.analysis.as_deref().unwrap_or(&none);
    if !a.is_enough() {
        let more = MIN_ATTEMPTS.saturating_sub(a.attempts().attempts);
        let text = tw(
            "Play online to see how you pot: the analysis appears after {n} more shots at a pocket.",
            &[("n", &more)],
        );
        empty_state(ui, area, Icon::Star, &text);
        return false;
    }
    page(ui, a, stats, area, heat);
    false
}

/// The numbers on top, then the table, the kinds of shot and the rest.
fn page(ui: &mut Ui, a: &ShotAnalysis, s: &AdvancedStats, area: Rect, heat: &mut Heat) {
    // What it all rests on, and the league's on the right.
    let pots = a.attempts();
    let games =
        if a.games == 1 { t("1 game").to_string() } else { tw("{n} games", &[("n", &a.games)]) };
    let summary = tw(
        "{pct}% of {n} shots at a pocket potted · {games}",
        &[("pct", &pots.rate().to_string()), ("n", &pots.attempts.to_string()), ("games", &games)],
    );
    let league = share(s.league.overall).map(|v| {
        let tier =
            s.tier.map_or_else(|| t("Rookie").to_string(), |tier| t(tier.name()).to_string());
        tw(
            "Your league: {pct}% ({tier}, {n} players)",
            &[("pct", &((v * 100.0).round() as i32)), ("tier", &tier), ("n", &s.league_players)],
        )
    });
    let league_w =
        league.as_ref().map_or(0.0, |l| ui.text_size(l, 13.0, false).x.min(area.w * 0.45));
    let summary = ui.ellipsize(&summary, area.w - league_w - 16.0, 13.0);
    ui.text(&summary, area.x, area.y, 13.0, MUTED, false);
    if let Some(l) = &league {
        let l = ui.ellipsize(l, league_w, 13.0);
        let w = ui.text_size(&l, 13.0, false).x;
        ui.text(&l, area.x + area.w - w, area.y, 13.0, CREAM, false);
    }

    let top = area.y + 26.0;
    let body = Rect::new(area.x, top, area.w, area.y + area.h - top);
    let gap = 26.0;
    let table_w = (body.w * 0.42).floor();
    let kinds_w = ((body.w - table_w - 2.0 * gap) * 0.53).floor();
    let rest_w = body.w - table_w - kinds_w - 2.0 * gap;
    table_column(ui, a, Rect::new(body.x, body.y, table_w, body.h), heat);
    kinds_column(ui, a, s, Rect::new(body.x + table_w + gap, body.y, kinds_w, body.h));
    side_column(
        ui,
        a,
        s,
        Rect::new(body.x + table_w + kinds_w + 2.0 * gap, body.y, rest_w, body.h),
    );
}

fn heading(ui: &Ui, text: &str, x: f32, y: f32, w: f32) {
    let (text, size) = ui.fitted(text, 14.0, w, true);
    ui.text(&text, x, y, size, GOLD, true);
}

/// A labelled bar: the label on the left, the rate and its sample on the
/// right; `mark`, the league's rate, as a line across the bar; `what`, what
/// the rate means when it isn't "potted" (for the tooltip).
fn bar_row(ui: &mut Ui, label: &str, t: Tally, r: Rect, mark: Option<f32>, what: Option<&str>) {
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
    if let Some(v) = mark {
        let x = track.x + track.w * v.clamp(0.0, 1.0);
        draw_line(x, track.y - 3.0, x, track.y + track.h + 3.0, 2.0, CREAM);
    }
    let pw = ui.text_size(&pct, 13.0, true).x;
    let px = track.x + track.w + 6.0 + num_w - pw;
    ui.text(&pct, px, text_y(mid, 13.0), 13.0, if dim { MUTED } else { CREAM }, true);
    let nw = ui.text_size(&n, 11.0, false).x;
    ui.text(&n, r.x + r.w - nw, text_y(mid, 11.0), 11.0, MUTED, false);
    if ui.hovering(r) && !dim {
        let tip = match what {
            Some(what) => format!("{pct} {what}"),
            None => tw(
                "{made} of {n} potted ({pct})",
                &[("made", &t.made.to_string()), ("n", &t.attempts.to_string()), ("pct", &pct)],
            ),
        };
        ui.tooltip(&tip, r);
    }
}

/// Success by kind of shot (the cut angles with the league's mark), then
/// the safeties and the breaks.
fn kinds_column(ui: &mut Ui, a: &ShotAnalysis, s: &AdvancedStats, r: Rect) {
    heading(ui, t("By kind of shot"), r.x, r.y, r.w);
    // The sample column's heading, over the counts.
    let shots = t("shots");
    let sw = ui.text_size(shots, 10.5, false).x;
    ui.text(shots, r.x + r.w - sw, r.y + 3.0, 10.5, MUTED, false);
    let kinds: Vec<ShotKind> =
        ShotKind::ALL.into_iter().filter(|k| *k != ShotKind::Break).collect();
    let note_h = 18.0;
    let rows = kinds.len() + 2;
    let row_h = ((r.h - 26.0 - note_h) / rows as f32).clamp(18.0, 28.0);
    let row = |k: usize| Rect::new(r.x, r.y + 26.0 + k as f32 * row_h, r.w, row_h);
    let mut marked = false;
    for (k, kind) in kinds.iter().enumerate() {
        let mark = CUT_KINDS.iter().position(|c| c == kind).and_then(|i| share(s.league.cuts[i]));
        marked |= mark.is_some();
        bar_row(ui, t(kind.name()), a.kinds[kind.index()], row(k), mark, None);
    }
    let safeties = t("left the opponent nothing");
    bar_row(ui, t("Safeties"), a.safeties, row(kinds.len()), None, Some(safeties));
    let breaks = a.kinds[ShotKind::Break.index()];
    bar_row(ui, t("Breaks"), breaks, row(kinds.len() + 1), None, Some(t("made a ball")));
    if marked {
        let note = t("The white mark: your league.");
        let (note, size) = ui.fitted(note, 11.0, r.w, false);
        ui.text(&note, r.x, r.y + 26.0 + rows as f32 * row_h + 4.0, size, MUTED, false);
    }
}

/// Distance bands, the weeks, and where the player is furthest ahead of
/// their league and behind it.
fn side_column(ui: &mut Ui, a: &ShotAnalysis, s: &AdvancedStats, r: Rect) {
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
        bar_row(ui, "", tl, bar, None, None);
        y += 15.0 + row_h;
    }
    // The league at the bottom, the weeks between.
    let lines = 2.0 * 17.0;
    let league_top = r.y + r.h - 20.0 - lines;
    y += 8.0;
    heading(ui, t("Week by week"), r.x, y, r.w);
    let chart =
        Rect::new(r.x + 30.0, y + 24.0, r.w - 30.0, (league_top - 22.0 - y - 24.0).max(30.0));
    weeks(ui, s, chart);
    heading(ui, t("Against your league"), r.x, league_top, r.w);
    ahead_and_behind(ui, s, r.x, league_top + 20.0, r.w);
}

/// Success week by week (the last weeks with games), in `chart`, with the
/// rates on its left and the first and last weeks under it.
fn weeks(ui: &mut Ui, s: &AdvancedStats, chart: Rect) {
    rrect(chart, 6.0, Color::new(0.0, 0.0, 0.0, 0.2));
    for k in 0..=2 {
        let y = chart.y + chart.h * k as f32 / 2.0;
        draw_line(chart.x, y, chart.x + chart.w, y, 1.0, Color::new(1.0, 1.0, 1.0, 0.05));
        let label = format!("{}%", 100 - k * 50);
        ui.text(&label, chart.x - 30.0, y - 6.0, 10.0, MUTED, false);
    }
    let points: Vec<(usize, f32)> =
        s.weeks.iter().enumerate().filter_map(|(k, w)| share(w.pots).map(|v| (k, v))).collect();
    if points.is_empty() {
        let note = t("No games yet.");
        let mid = chart.y + chart.h / 2.0;
        ui.text_centered(note, chart.x + chart.w / 2.0, mid - 7.0, 13.0, MUTED, false);
        return;
    }
    let n = s.weeks.len().max(2) as f32 - 1.0;
    let at = |k: usize, v: f32| {
        let x = if s.weeks.len() < 2 {
            chart.x + chart.w / 2.0
        } else {
            chart.x + chart.w * k as f32 / n
        };
        vec2(x, chart.y + chart.h * (1.0 - v))
    };
    for pair in points.windows(2) {
        let (a, b) = (at(pair[0].0, pair[0].1), at(pair[1].0, pair[1].1));
        draw_line(a.x, a.y, b.x, b.y, 2.0, GOLD);
    }
    for &(k, v) in &points {
        let p = at(k, v);
        draw_circle(p.x, p.y, 3.5, GOLD);
    }
    let date = |w: &Week| crate::profile::date(week_start(w.week));
    if let (Some(first), Some(last)) = (s.weeks.first(), s.weeks.last()) {
        ui.text(&date(first), chart.x, chart.y + chart.h + 4.0, 10.0, MUTED, false);
        let d = date(last);
        let w = ui.text_size(&d, 10.0, false).x;
        ui.text(&d, chart.x + chart.w - w, chart.y + chart.h + 4.0, 10.0, MUTED, false);
    }
}

/// The pockets where the player is furthest ahead of their league and
/// furthest behind it, a line each.
fn ahead_and_behind(ui: &mut Ui, s: &AdvancedStats, x: f32, y: f32, w: f32) {
    let mut diffs: Vec<(usize, f32)> = (0..POCKETS)
        .filter_map(|k| Some((k, share(s.mine.pockets[k])? - share(s.league.pockets[k])?)))
        .collect();
    if diffs.is_empty() {
        let (text, size) = ui.fitted(t("No games yet."), 12.0, w, false);
        ui.text(&text, x, y, size, MUTED, false);
        return;
    }
    diffs.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut shown = vec![diffs[0]];
    if diffs.len() > 1 {
        shown.push(diffs[diffs.len() - 1]);
    }
    for (line, (k, d)) in shown.into_iter().enumerate() {
        let pocket = t(pocket_name(PocketId::ALL[k]));
        let points = (d * 100.0).round() as i32;
        let (text, color) = if points >= 0 {
            (tw("{pocket}: {n} points above", &[("pocket", &pocket), ("n", &points)]), GOOD)
        } else {
            (tw("{pocket}: {n} points below", &[("pocket", &pocket), ("n", &-points)]), BAD)
        };
        let (text, size) = ui.fitted(&text, 12.0, w, false);
        ui.text(&text, x, y + line as f32 * 17.0, size, color, false);
    }
}

/// The card over the preview: what the page is, and the way to it.
fn pitch(ui: &mut Ui, area: Rect) -> bool {
    let w = 420.0f32.min(area.w - 20.0);
    let text = t(
        "See how you pot pocket by pocket and by cut angle, how you progress week after week, and how you compare with the players of your league.",
    );
    let lines = ui.wrap(text, w - 40.0, 14.0);
    let h = 70.0 + lines.len() as f32 * 19.0 + 64.0;
    let r = Rect::new(area.x + (area.w - w) / 2.0, area.y + (area.h - h) / 2.0, w, h);
    ui.panel(r);
    rrect_lines(r, 12.0, 1.5, GOLD);
    crate::icons::draw(Icon::Crown, vec2(r.x + 30.0, r.y + 30.0), 22.0, GOLD);
    let (title, size) = ui.fitted(t("Shot analysis"), 18.0, r.w - 70.0, true);
    ui.text(&title, r.x + 50.0, r.y + 20.0, size, GOLD, true);
    for (k, line) in lines.iter().enumerate() {
        ui.text(line, r.x + 20.0, r.y + 56.0 + k as f32 * 19.0, 14.0, CREAM, false);
    }
    let b = Rect::new(r.x + (r.w - 200.0) / 2.0, r.y + r.h - 52.0, 200.0, 38.0);
    ui.primary_button(b, t("Become VIP"))
}

/// The league's numbers and the weeks behind the preview (made up: never
/// anyone's).
fn sample() -> AdvancedStats {
    let t = |a: u32, m: u32| Tally { attempts: a, made: m };
    let mine = Sums {
        pockets: [t(40, 26), t(38, 22), t(20, 9), t(22, 12), t(41, 30), t(36, 21)],
        cuts: [t(60, 48), t(70, 44), t(40, 18), t(20, 5)],
        overall: t(197, 120),
    };
    let league = Sums {
        pockets: [t(40, 22), t(40, 22), t(20, 8), t(20, 8), t(40, 22), t(40, 22)],
        cuts: [t(60, 44), t(70, 40), t(40, 16), t(20, 4)],
        overall: t(200, 104),
    };
    AdvancedStats {
        vip: false,
        games: 42,
        mine,
        league,
        league_players: 120,
        weeks: (0..8)
            .map(|k| Week { week: 2900 + k, pots: t(40, 20 + (k as u32 * 3) % 9), games: 5 })
            .collect(),
        ..Default::default()
    }
}

/// A shot analysis to show behind the preview (made up, as [`sample`]).
fn sample_analysis() -> ShotAnalysis {
    let s = sample();
    let t = |a: u32, m: u32| Tally { attempts: a, made: m };
    let mut a = ShotAnalysis { games: s.games, pockets: s.mine.pockets, ..Default::default() };
    for (k, kind) in CUT_KINDS.iter().enumerate() {
        a.kinds[kind.index()] = s.mine.cuts[k];
    }
    for (kind, tally) in [
        (ShotKind::Bank, t(18, 7)),
        (ShotKind::Kick, t(9, 3)),
        (ShotKind::Combination, t(6, 2)),
        (ShotKind::Jump, t(4, 1)),
        (ShotKind::Masse, t(3, 1)),
        (ShotKind::Break, t(21, 13)),
    ] {
        a.kinds[kind.index()] = tally;
    }
    a.distances = [t(80, 62), t(78, 44), t(39, 14)];
    a.safeties = t(25, 15);
    // Busier around the pockets and the spots, as real games are.
    for (k, cell) in a.heat.iter_mut().enumerate() {
        let (col, row) = ((k % GRID_W) as f32, (k / GRID_W) as f32);
        let wave = ((col * 0.9).sin() + (row * 1.3).cos() + 2.0) * 3.0;
        let attempts = wave.round() as u32;
        *cell = t(attempts, attempts * 3 / 5);
    }
    a
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
