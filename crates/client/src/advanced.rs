//! The advanced statistics (a VIP feature), in the player's own profile:
//! success pocket by pocket on a drawing of the table, by cut angle, week
//! by week, and against the players of their league. A player who isn't
//! VIP sees the same screen with made-up numbers, dimmed, and what it is.
//!
//! They explain the player's game; nothing here helps at the table.

use macroquad::prelude::*;

use snook_legend_proto::advanced::{AdvancedStats, CUTS, Sums, Week, week_start};
use snook_legend_proto::analysis::{POCKETS, Tally};

use crate::i18n::{t, tw};
use crate::paint::{draw_circle, draw_line};
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

/// The cut-angle buckets' labels.
const CUT_LABELS: [&str; CUTS] = ["Straight", "Cut 8–25°", "Cut 25–45°", "Thin cut 45°+"];
/// Where each pocket is on the drawing ((0, 0) top left; the order of
/// `PocketId`).
const POCKET_AT: [(f32, f32); POCKETS] =
    [(0.0, 0.0), (0.0, 1.0), (0.5, 0.0), (0.5, 1.0), (1.0, 0.0), (1.0, 1.0)];
const GOOD: Color = Color::new(0.45, 0.88, 0.5, 1.0);
const BAD: Color = Color::new(1.0, 0.5, 0.42, 1.0);

fn rate(t: Tally) -> Option<f32> {
    (t.attempts > 0).then(|| t.made.min(t.attempts) as f32 / t.attempts as f32)
}

fn pct(t: Tally) -> String {
    rate(t).map_or("—".to_string(), |r| format!("{}%", (r * 100.0).round() as i32))
}

/// From red (0%) through gold to green (100%).
fn heat(k: f32) -> Color {
    let k = k.clamp(0.0, 1.0);
    if k < 0.5 {
        crate::ui::mix(BAD, GOLD, k * 2.0)
    } else {
        crate::ui::mix(GOLD, GOOD, (k - 0.5) * 2.0)
    }
}

/// Numbers to show behind the preview (made up: never anyone's).
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

/// Draws the screen in `area`; true when the player asks to become VIP.
pub fn draw(ui: &mut Ui, stats: Option<&AdvancedStats>, area: Rect) -> bool {
    let Some(stats) = stats else {
        crate::online::loading(ui, area);
        return false;
    };
    let preview = !stats.vip;
    let shown = if preview { sample() } else { stats.clone() };
    let saved = crate::paint::opacity();
    if preview {
        crate::paint::set_opacity(saved * 0.28);
    }
    let gap = 16.0;
    let half_w = (area.w - gap) / 2.0;
    let half_h = (area.h - gap) / 2.0;
    let cell = |k: usize| {
        Rect::new(
            area.x + (k % 2) as f32 * (half_w + gap),
            area.y + (k / 2) as f32 * (half_h + gap),
            half_w,
            half_h,
        )
    };
    pockets(ui, &shown, cell(0));
    cuts(ui, &shown, cell(1));
    weeks(ui, &shown, cell(2));
    league(ui, &shown, cell(3));
    crate::paint::set_opacity(saved);
    if preview {
        return pitch(ui, area);
    }
    if stats.games == 0 && stats.mine.overall.attempts == 0 {
        let note = t("Play online: your games are added up here as they end.");
        let w = ui.text_size(note, 14.0, true).x + 40.0;
        let r = Rect::new(area.x + (area.w - w) / 2.0, area.y + area.h / 2.0 - 20.0, w, 40.0);
        rrect(r, 10.0, Color::new(0.04, 0.03, 0.02, 0.92));
        ui.text_centered(note, r.x + r.w / 2.0, r.y + 12.0, 14.0, CREAM, true);
    }
    false
}

/// The card over the preview: what the screen is, and the way to it.
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
    crate::icons::draw(crate::icons::Icon::Crown, vec2(r.x + 30.0, r.y + 30.0), 22.0, GOLD);
    ui.text(t("Advanced statistics"), r.x + 50.0, r.y + 20.0, 18.0, GOLD, true);
    for (k, line) in lines.iter().enumerate() {
        ui.text(line, r.x + 20.0, r.y + 56.0 + k as f32 * 19.0, 14.0, CREAM, false);
    }
    let b = Rect::new(r.x + (r.w - 200.0) / 2.0, r.y + r.h - 52.0, 200.0, 38.0);
    ui.primary_button(b, t("Become VIP"))
}

fn title(ui: &Ui, text: &str, r: Rect) {
    ui.text(text, r.x, r.y, 14.0, GOLD, true);
}

/// Success in each pocket, on a drawing of the table.
fn pockets(ui: &mut Ui, s: &AdvancedStats, r: Rect) {
    title(ui, t("By pocket"), r);
    let room = Rect::new(r.x + 18.0, r.y + 34.0, r.w - 36.0, r.h - 52.0);
    // A table twice as long as wide, as large as fits.
    let w = room.w.min(room.h * 2.0);
    let h = w / 2.0;
    let table = Rect::new(room.x + (room.w - w) / 2.0, room.y + (room.h - h) / 2.0, w, h);
    rrect(table, 8.0, Color::new(0.33, 0.2, 0.1, 1.0));
    let cloth = Rect::new(table.x + 8.0, table.y + 8.0, table.w - 16.0, table.h - 16.0);
    rrect(cloth, 4.0, Color::new(0.08, 0.36, 0.2, 1.0));
    for (k, (u, v)) in POCKET_AT.iter().enumerate() {
        let c = vec2(cloth.x + u * cloth.w, cloth.y + v * cloth.h);
        let t_ = s.mine.pockets[k];
        let color = rate(t_).map_or(Color::new(0.3, 0.3, 0.3, 1.0), heat);
        draw_circle(c.x, c.y, 15.0, Color::new(color.r, color.g, color.b, 0.35));
        draw_circle(c.x, c.y, 9.0, color);
        let label = pct(t_);
        // Inside the table, towards its middle.
        let ly = if *v < 0.5 { c.y + 16.0 } else { c.y - 30.0 };
        ui.text_centered(
            &label,
            c.x.clamp(cloth.x + 18.0, cloth.x + cloth.w - 18.0),
            ly,
            13.0,
            CREAM,
            true,
        );
        let n = format!("{}/{}", t_.made, t_.attempts);
        let ny = if *v < 0.5 { ly + 15.0 } else { ly - 13.0 };
        ui.text_centered(
            &n,
            c.x.clamp(cloth.x + 18.0, cloth.x + cloth.w - 18.0),
            ny,
            10.0,
            MUTED,
            false,
        );
    }
}

/// Success by cut angle, with the league's as a mark on each bar.
fn cuts(ui: &mut Ui, s: &AdvancedStats, r: Rect) {
    title(ui, t("By cut angle"), r);
    let rows = Rect::new(r.x, r.y + 34.0, r.w, r.h - 40.0);
    let row_h = (rows.h / CUTS as f32).min(40.0);
    let label_w = CUT_LABELS.iter().map(|l| ui.text_size(t(l), 12.0, false).x).fold(80.0, f32::max);
    for (k, label) in CUT_LABELS.iter().enumerate() {
        let y = rows.y + k as f32 * row_h;
        ui.text(t(label), r.x, y + 6.0, 12.0, CREAM, false);
        let bar = Rect::new(r.x + label_w + 12.0, y + 8.0, r.w - label_w - 12.0 - 48.0, 10.0);
        rrect(bar, 5.0, Color::new(1.0, 1.0, 1.0, 0.07));
        let mine = s.mine.cuts[k];
        if let Some(v) = rate(mine) {
            rrect(Rect::new(bar.x, bar.y, (bar.w * v).max(4.0), bar.h), 5.0, heat(v));
        }
        if let Some(v) = rate(s.league.cuts[k]) {
            let x = bar.x + bar.w * v;
            draw_line(x, bar.y - 3.0, x, bar.y + bar.h + 3.0, 2.0, CREAM);
        }
        ui.text(&pct(mine), bar.x + bar.w + 8.0, y + 5.0, 12.0, CREAM, true);
    }
    let note = t("The white mark: your league.");
    ui.text(note, r.x, r.y + r.h - 14.0, 11.0, MUTED, false);
}

/// Success week by week (the last weeks with games).
fn weeks(ui: &mut Ui, s: &AdvancedStats, r: Rect) {
    title(ui, t("Week by week"), r);
    let chart = Rect::new(r.x + 34.0, r.y + 34.0, r.w - 44.0, r.h - 60.0);
    rrect(chart, 6.0, Color::new(0.0, 0.0, 0.0, 0.2));
    for k in 0..=4 {
        let y = chart.y + chart.h * k as f32 / 4.0;
        draw_line(chart.x, y, chart.x + chart.w, y, 1.0, Color::new(1.0, 1.0, 1.0, 0.05));
        let label = format!("{}%", 100 - k * 25);
        ui.text(&label, r.x, y - 6.0, 10.0, MUTED, false);
    }
    let points: Vec<(usize, f32)> =
        s.weeks.iter().enumerate().filter_map(|(k, w)| rate(w.pots).map(|v| (k, v))).collect();
    if points.is_empty() {
        let note = t("No games yet.");
        ui.text_centered(
            note,
            chart.x + chart.w / 2.0,
            chart.y + chart.h / 2.0 - 7.0,
            13.0,
            MUTED,
            false,
        );
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
    // The first and last weeks under the chart.
    let date = |w: &Week| crate::profile::date(week_start(w.week));
    if let (Some(first), Some(last)) = (s.weeks.first(), s.weeks.last()) {
        ui.text(&date(first), chart.x, chart.y + chart.h + 6.0, 10.0, MUTED, false);
        let d = date(last);
        let w = ui.text_size(&d, 10.0, false).x;
        ui.text(&d, chart.x + chart.w - w, chart.y + chart.h + 6.0, 10.0, MUTED, false);
    }
}

/// The player against the players of their league.
fn league(ui: &mut Ui, s: &AdvancedStats, r: Rect) {
    title(ui, t("Against your league"), r);
    let tier = s.tier.map_or_else(|| t("Rookie").to_string(), |tier| t(tier.name()).to_string());
    let sub = tw("{tier}: {n} players", &[("tier", &tier), ("n", &s.league_players)]);
    ui.text(&sub, r.x, r.y + 20.0, 12.0, MUTED, false);
    let mine = rate(s.mine.overall);
    let theirs = rate(s.league.overall);
    let y = r.y + 46.0;
    let big = |v: Option<f32>| {
        v.map_or("—".to_string(), |v| format!("{}%", (v * 100.0).round() as i32))
    };
    let col = r.w / 2.0;
    ui.text_centered(t("You"), r.x + col / 2.0, y, 12.0, MUTED, true);
    ui.text_centered(&big(mine), r.x + col / 2.0, y + 16.0, 30.0, GOLD, true);
    ui.text_centered(t("League"), r.x + col * 1.5, y, 12.0, MUTED, true);
    ui.text_centered(&big(theirs), r.x + col * 1.5, y + 16.0, 30.0, CREAM, true);
    draw_line(r.x + col, y, r.x + col, y + 48.0, 1.0, BORDER);
    // Where the player is furthest ahead and behind, pocket by pocket.
    let mut diffs: Vec<(usize, f32)> = (0..POCKETS)
        .filter_map(|k| Some((k, rate(s.mine.pockets[k])? - rate(s.league.pockets[k])?)))
        .collect();
    diffs.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut line_y = y + 66.0;
    let mut say = |k: usize, d: f32| {
        let pocket = t(crate::analysis::pocket_name(pocket_id(k)));
        let points = (d * 100.0).round() as i32;
        let (text, color) = if points >= 0 {
            (tw("{pocket}: {n} points above", &[("pocket", &pocket), ("n", &points)]), GOOD)
        } else {
            (tw("{pocket}: {n} points below", &[("pocket", &pocket), ("n", &-points)]), BAD)
        };
        let (text, size) = ui.fitted(&text, 12.0, r.w, false);
        ui.text(&text, r.x, line_y, size, color, false);
        line_y += 18.0;
    };
    if let Some(&(k, d)) = diffs.first() {
        say(k, d);
    }
    if diffs.len() > 1
        && let Some(&(k, d)) = diffs.last()
    {
        say(k, d);
    }
}

fn pocket_id(k: usize) -> snook_legend_core::PocketId {
    use snook_legend_core::PocketId::*;
    [HeadLeft, HeadRight, SideLeft, SideRight, FootLeft, FootRight][k.min(POCKETS - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_and_colours() {
        assert_eq!(pct(Tally { attempts: 0, made: 0 }), "—");
        assert_eq!(pct(Tally { attempts: 3, made: 2 }), "67%");
        assert_eq!(heat(0.0), BAD);
        assert_eq!(heat(1.0), GOOD);
        assert_eq!(sample().mine.pockets.len(), POCKETS);
        assert_eq!(pocket_id(4), snook_legend_core::PocketId::FootLeft);
    }
}
