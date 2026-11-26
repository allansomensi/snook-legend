//! A closer look at an achievement or a reaction, from the profile's
//! Achievements pane: a click on a medal (or a reaction) opens a card with
//! it large, what it takes, how far along the player is, when it was won and
//! what comes with it. The arrows (and ← →) go through the others.

use macroquad::prelude::*;
use snook_legend_proto::{ACHIEVEMENTS, Achievement, Profile, REACTIONS, Reaction};

use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::paint::{draw_circle, draw_rectangle};
use crate::profile::{achievement_text, date, medal_color, medal_tier};
use crate::ranking::text_y;
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

/// What the card shows: an achievement or a reaction (their place in
/// [`ACHIEVEMENTS`] and [`REACTIONS`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    Achievement(usize),
    Reaction(usize),
}

const W: f32 = 440.0;
const PAD: f32 = 26.0;
const WIN: Color = Color::new(0.45, 0.85, 0.5, 1.0);

/// When `p` has `a` (Unix seconds; 0: when isn't known).
fn unlocked_at(p: &Profile, a: &Achievement) -> Option<i64> {
    p.achievements
        .iter()
        .find(|(code, _)| code == a.code)
        .map(|(_, at)| *at)
        .or(crate::dev::flag("SNOOK_LEGEND_ALL_MEDALS").then_some(0))
}

/// The codes of `p`'s achievements.
fn codes(p: &Profile) -> Vec<&str> {
    p.achievements.iter().map(|(c, _)| c.as_str()).collect()
}

/// Draws the open card over the profile; closes it (None) on Close, Esc or
/// a click beside it (not the click that opened it: `fresh`).
pub fn draw(ui: &mut Ui, detail: &mut Option<Detail>, p: &Profile, fresh: bool) {
    let Some(d) = *detail else { return };
    let sw = crate::ui::vw();
    let sh = crate::ui::vh();
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.6));
    let (count, at) = match d {
        Detail::Achievement(k) => (ACHIEVEMENTS.len(), k),
        Detail::Reaction(k) => (REACTIONS.len(), k),
    };
    let w = W.min(sw - 20.0);
    let inner = w - 2.0 * PAD;
    let h = match d {
        Detail::Achievement(k) => achievement_height(ui, p, &ACHIEVEMENTS[k.min(count - 1)], inner),
        Detail::Reaction(k) => reaction_height(ui, p, &REACTIONS[k.min(count - 1)], inner),
    };
    let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
    ui.block(Rect::new(0.0, 0.0, sw, sh));
    let mut go: Option<Option<Detail>> = None;
    let close_r = Rect::new(r.x + w - 12.0 - 32.0, r.y + 12.0, 32.0, 32.0);
    // Previous and next, at the panel's sides (and ← →).
    let step = |k: usize, by: isize| (k as isize + by).rem_euclid(count as isize) as usize;
    let with = |k: usize| match d {
        Detail::Achievement(_) => Detail::Achievement(k),
        Detail::Reaction(_) => Detail::Reaction(k),
    };
    let arrows = sw >= w + 2.0 * 52.0;
    let prev_r = Rect::new(r.x - 48.0, r.y + h / 2.0 - 22.0, 40.0, 44.0);
    let next_r = Rect::new(r.x + w + 8.0, r.y + h / 2.0 - 22.0, 40.0, 44.0);
    let typing = ui.wants_keyboard;
    if (arrows && ui.icon_button(prev_r, Icon::ChevronLeft, t("Previous"), true))
        || (!typing && is_key_pressed(KeyCode::Left))
    {
        go = Some(Some(with(step(at, -1))));
    }
    if (arrows && ui.icon_button(next_r, Icon::ChevronRight, t("Next"), true))
        || (!typing && is_key_pressed(KeyCode::Right))
    {
        go = Some(Some(with(step(at, 1))));
    }
    let content = Rect::new(r.x, r.y, w, h);
    ui.panel(content);
    let picked = match d {
        Detail::Achievement(k) => {
            draw_achievement(ui, p, &ACHIEVEMENTS[k.min(count - 1)], content, inner)
        }
        Detail::Reaction(k) => draw_reaction(ui, p, &REACTIONS[k.min(count - 1)], content, inner),
    };
    if let Some(picked) = picked {
        go = Some(Some(picked));
    }
    if ui.icon_button(close_r, Icon::Close, t("Close"), true) || ui.escape() {
        go = Some(None);
    }
    // A click beside the card closes it.
    if !fresh
        && is_mouse_button_pressed(MouseButton::Left)
        && !content.contains(ui.mouse())
        && !(arrows && (prev_r.contains(ui.mouse()) || next_r.contains(ui.mouse())))
    {
        go = Some(None);
    }
    if let Some(g) = go {
        *detail = g;
    }
}

/// An achievement's numbers: how far `p` has come (shown while locked and
/// measured by a number).
fn progress(p: &Profile, a: &Achievement) -> Option<(i64, i64)> {
    use snook_legend_proto::Stat;
    if a.secret || a.stat.is_event() || a.stat == Stat::Awarded || a.goal <= 1 {
        return None;
    }
    Some((p.stats.get(a.stat).clamp(0, a.goal), a.goal))
}

/// What comes with an achievement: the reactions it unlocks and the items
/// it gives.
fn rewards(
    a: &Achievement,
) -> (Vec<&'static Reaction>, Vec<&'static snook_legend_proto::shop::Item>) {
    let reactions = REACTIONS.iter().filter(|r| r.unlock == Some(a.code)).collect();
    let items = snook_legend_proto::shop::ITEMS
        .iter()
        .filter(|i| i.price == snook_legend_proto::shop::Price::Achievement(a.code))
        .collect();
    (reactions, items)
}

const MEDAL: f32 = 150.0;
const TILE: f32 = 72.0;
/// The name under a reward's tile.
const CAPTION: f32 = 22.0;

/// The achievement card's height.
fn achievement_height(ui: &Ui, p: &Profile, a: &Achievement, inner: f32) -> f32 {
    let hidden = a.secret && unlocked_at(p, a).is_none();
    let desc = if hidden {
        t("A secret achievement: play to find it.").to_string()
    } else {
        achievement_text(a)
    };
    let lines = ui.wrap(&desc, inner, 14.0).len() as f32;
    let mut h = PAD + MEDAL + 12.0 + 20.0 + 30.0 + 22.0 + 22.0;
    h += 24.0 + lines * 19.0;
    if unlocked_at(p, a).is_none() && progress(p, a).is_some() {
        h += 22.0 + 24.0 + 12.0;
    }
    let (reactions, items) = rewards(a);
    if !hidden && !(reactions.is_empty() && items.is_empty()) {
        h += 22.0 + 24.0 + TILE + CAPTION;
    }
    h + PAD
}

/// A small heading in capitals, with a rule after it.
fn section(ui: &Ui, title: &str, x: f32, y: f32, w: f32) {
    let caps = title.to_uppercase();
    ui.text(&caps, x, y, 11.0, MUTED, true);
    let tw_ = ui.text_size(&caps, 11.0, true).x;
    let ly = y + 7.0;
    draw_rectangle(x + tw_ + 10.0, ly, (w - tw_ - 10.0).max(0.0), 1.0, BORDER);
}

/// A pill in `color`, centred on `cx`; its width.
fn pill(ui: &Ui, text: &str, cx: f32, y: f32, color: Color) -> f32 {
    let w = ui.text_size(text, 11.0, true).x + 18.0;
    let r = Rect::new(cx - w / 2.0, y, w, 20.0);
    rrect(r, 10.0, Color::new(color.r, color.g, color.b, 0.16));
    rrect_lines(r, 10.0, 1.0, Color::new(color.r, color.g, color.b, 0.5));
    ui.text_centered(text, cx, text_y(y + 10.0, 11.0), 11.0, color, true);
    w
}

fn draw_achievement(
    ui: &mut Ui,
    p: &Profile,
    a: &Achievement,
    r: Rect,
    inner: f32,
) -> Option<Detail> {
    let at = unlocked_at(p, a);
    let hidden = a.secret && at.is_none();
    let metal = medal_color(a.level);
    let cx = r.x + r.w / 2.0;
    let x = r.x + PAD;
    // The medal, large, in a glow of its metal when won.
    let mc = vec2(cx, r.y + PAD + MEDAL / 2.0);
    if at.is_some() {
        for i in 0..5 {
            let g = MEDAL * (0.34 + 0.06 * i as f32);
            draw_circle(mc.x, mc.y + 6.0, g, Color::new(metal.r, metal.g, metal.b, 0.04));
        }
    }
    crate::profile::medal_art(mc, MEDAL, a.level, at.is_none());
    if at.is_none() {
        let lock = vec2(mc.x + MEDAL * 0.3, mc.y + MEDAL * 0.34);
        draw_circle(lock.x, lock.y, 15.0, Color::new(0.08, 0.06, 0.04, 0.95));
        crate::paint::draw_circle_lines(lock.x, lock.y, 15.0, 1.0, BORDER);
        crate::icons::draw(Icon::Lock, lock, 15.0, MUTED);
    }
    let mut y = r.y + PAD + MEDAL + 12.0;
    // The metal (and a secret's tag).
    let tier = medal_tier(a.level).to_uppercase();
    if a.secret {
        let secret = t("A secret achievement");
        let gap = 8.0;
        let tw1 = ui.text_size(&tier, 11.0, true).x + 18.0;
        let tw2 = ui.text_size(secret, 11.0, true).x + 18.0;
        let left = cx - (tw1 + gap + tw2) / 2.0;
        pill(ui, &tier, left + tw1 / 2.0, y, metal);
        pill(ui, secret, left + tw1 + gap + tw2 / 2.0, y, Color::new(0.75, 0.55, 0.95, 1.0));
    } else {
        pill(ui, &tier, cx, y, metal);
    }
    y += 20.0 + 10.0;
    // The title.
    let title = if hidden { "???" } else { t(a.title) };
    let (title, size) = ui.fitted(title, 22.0, inner, true);
    ui.text_centered(&title, cx, y, size, if at.is_some() { metal } else { CREAM }, true);
    y += 30.0;
    // Won, and when.
    let (status, color) = match at {
        Some(at) if at > 0 => (tw("Unlocked {date}", &[("date", &date(at))]), WIN),
        Some(_) => (t("Yours").to_string(), WIN),
        None => (t("Locked").to_string(), MUTED),
    };
    ui.text_centered(&status, cx, y, 13.0, color, at.is_some());
    y += 22.0 + 22.0;
    // What it takes.
    section(ui, t("How to win it"), x, y, inner);
    y += 24.0;
    let desc = if hidden {
        t("A secret achievement: play to find it.").to_string()
    } else {
        achievement_text(a)
    };
    for line in ui.wrap(&desc, inner, 14.0) {
        ui.text(&line, x, y, 14.0, CREAM, false);
        y += 19.0;
    }
    // How far along (still to win, and counted by a number).
    if at.is_none()
        && let Some((have, goal)) = progress(p, a)
    {
        y += 22.0;
        section(ui, t("Progress"), x, y, inner);
        y += 24.0;
        let bar = Rect::new(x, y + 2.0, inner - 96.0, 8.0);
        rrect(bar, 4.0, Color::new(1.0, 1.0, 1.0, 0.08));
        let k = have as f32 / goal.max(1) as f32;
        if k > 0.0 {
            rrect(Rect::new(bar.x, bar.y, (bar.w * k).max(8.0), bar.h), 4.0, metal);
        }
        let label = tw("{n} of {total}", &[("n", &have), ("total", &goal)]);
        let lw = ui.text_size(&label, 13.0, true).x;
        ui.text(&label, x + inner - lw, y - 2.0, 13.0, CREAM, true);
        y += 12.0;
    }
    // What comes with it.
    let (reactions, items) = rewards(a);
    let mut picked = None;
    if !hidden && !(reactions.is_empty() && items.is_empty()) {
        y += 22.0;
        section(ui, t("Comes with it"), x, y, inner);
        y += 24.0;
        let n = reactions.len() + items.len();
        let gap = 12.0;
        let tile = TILE.min((inner - gap * (n as f32 - 1.0)) / n as f32);
        let mut tx = cx - (n as f32 * tile + (n as f32 - 1.0) * gap) / 2.0;
        let codes = codes(p);
        for reaction in &reactions {
            let tr = Rect::new(tx, y, tile, tile);
            let hover = ui.hovering(tr);
            let lit = ui.hover_anim(tr, hover);
            rrect(tr, 10.0, Color::new(1.0, 1.0, 1.0, 0.05 + 0.04 * lit));
            rrect_lines(tr, 10.0, 1.0, crate::ui::mix(BORDER, GOLD, lit));
            let c = vec2(tr.x + tile / 2.0, tr.y + tile / 2.0);
            if reaction.available(&codes, &p.reactions, p.player.vip) {
                crate::reactions::draw(reaction.code, c, tile * 0.5, crate::paint::alpha());
            } else {
                crate::reactions::draw_locked(reaction.code, c, tile * 0.46);
            }
            caption(ui, t("Reaction"), tr, gap);
            if hover && ui.clicked(tr) {
                picked =
                    REACTIONS.iter().position(|r| r.code == reaction.code).map(Detail::Reaction);
            }
            tx += tile + gap;
        }
        for item in &items {
            let tr = Rect::new(tx, y, tile, tile);
            rrect(tr, 10.0, Color::new(1.0, 1.0, 1.0, 0.05));
            rrect_lines(tr, 10.0, 1.0, BORDER);
            // Whatever the item draws stays in its tile.
            crate::ui::motion::push_clip(tr);
            crate::shop::preview(
                ui,
                item,
                Rect::new(tr.x + 6.0, tr.y + 6.0, tile - 12.0, tile - 12.0),
            );
            crate::ui::motion::pop_clip();
            caption(ui, t(item.name), tr, gap);
            if ui.hovering(tr) {
                ui.tooltip(t(item.name), tr);
            }
            tx += tile + gap;
        }
    }
    picked
}

/// A reward's name under its tile (as wide as the tile and the gaps).
fn caption(ui: &Ui, text: &str, tile: Rect, gap: f32) {
    let text = ui.ellipsize(text, tile.w + gap, 11.5);
    ui.text_centered(&text, tile.x + tile.w / 2.0, tile.y + tile.h + 5.0, 11.5, MUTED, false);
}

/// How a reaction is won, in words: the achievement's own text when the
/// card of the achievement is shown under it (its name is there).
fn how(r: &Reaction, have: bool) -> String {
    match source(r, have) {
        Some((_, a)) => achievement_text(a),
        None => crate::reactions::how(r, have),
    }
}

/// The reaction card's height.
fn reaction_height(ui: &Ui, p: &Profile, r: &Reaction, inner: f32) -> f32 {
    let have = r.available(&codes(p), &p.reactions, p.player.vip);
    let lines = ui.wrap(&how(r, have), inner, 14.0).len() as f32;
    let mut h = PAD + REACTION + 16.0 + 20.0 + 14.0 + 22.0 + 22.0 + 24.0 + lines * 19.0;
    if source(r, have).is_some() {
        h += 18.0 + 56.0;
    }
    h + PAD
}

const REACTION: f32 = 120.0;

/// The achievement a reaction comes with, when it is one to show (a secret
/// stays one until the reaction is had).
fn source(r: &Reaction, have: bool) -> Option<(usize, &'static Achievement)> {
    let code = r.unlock?;
    let k = ACHIEVEMENTS.iter().position(|a| a.code == code)?;
    let a = &ACHIEVEMENTS[k];
    (!a.secret || have).then_some((k, a))
}

/// How rare a reaction is (the shop's colours).
fn rarity(r: &Reaction) -> Option<snook_legend_proto::shop::Rarity> {
    if let Some(item) = snook_legend_proto::shop::item(r.code) {
        return Some(snook_legend_proto::shop::rarity(item));
    }
    match r.unlock {
        Some(a) if ACHIEVEMENTS.iter().any(|x| x.code == a) => {
            Some(snook_legend_proto::shop::achievement_rarity(a))
        }
        _ => None,
    }
}

fn draw_reaction(ui: &mut Ui, p: &Profile, r: &Reaction, card: Rect, inner: f32) -> Option<Detail> {
    let codes = codes(p);
    let have = r.available(&codes, &p.reactions, p.player.vip);
    let cx = card.x + card.w / 2.0;
    let x = card.x + PAD;
    // The reaction, large (moving, if it is one that moves), on a soft disc.
    let c = vec2(cx, card.y + PAD + REACTION / 2.0);
    let tint = rarity(r).map_or(GOLD, crate::collection::rarity_color);
    for i in 0..4 {
        let g = REACTION * (0.36 + 0.07 * i as f32);
        let a = if have { 0.05 } else { 0.025 };
        draw_circle(c.x, c.y, g, Color::new(tint.r, tint.g, tint.b, a));
    }
    if have {
        crate::reactions::draw(r.code, c, REACTION * 0.72, crate::paint::alpha());
    } else {
        crate::reactions::draw_locked(r.code, c, REACTION * 0.66);
        let lock = vec2(c.x + REACTION * 0.32, c.y + REACTION * 0.32);
        draw_circle(lock.x, lock.y, 15.0, Color::new(0.08, 0.06, 0.04, 0.95));
        crate::paint::draw_circle_lines(lock.x, lock.y, 15.0, 1.0, BORDER);
        crate::icons::draw(Icon::Lock, lock, 15.0, MUTED);
    }
    let mut y = card.y + PAD + REACTION + 16.0;
    // Its rarity, and whether it moves.
    let mut tags: Vec<(String, Color)> = Vec::new();
    if let Some(rar) = rarity(r) {
        tags.push((crate::collection::rarity_name(rar).to_uppercase(), tint));
    }
    if crate::reactions::motion(r.code).is_some() {
        tags.push((t("Animated").to_uppercase(), Color::new(0.55, 0.8, 1.0, 1.0)));
    }
    if !tags.is_empty() {
        let gap = 8.0;
        let widths: Vec<f32> =
            tags.iter().map(|(s, _)| ui.text_size(s, 11.0, true).x + 18.0).collect();
        let total: f32 = widths.iter().sum::<f32>() + gap * (tags.len() as f32 - 1.0);
        let mut left = cx - total / 2.0;
        for ((s, col), w) in tags.iter().zip(&widths) {
            pill(ui, s, left + w / 2.0, y, *col);
            left += w + gap;
        }
    }
    y += 20.0 + 14.0;
    // Had or not.
    let (status, color) = if have { (t("Yours"), WIN) } else { (t("Locked"), MUTED) };
    ui.text_centered(status, cx, y, 15.0, color, true);
    y += 22.0 + 22.0;
    // How it is won.
    section(ui, t("How to win it"), x, y, inner);
    y += 24.0;
    for line in ui.wrap(&how(r, have), inner, 14.0) {
        ui.text(&line, x, y, 14.0, CREAM, false);
        y += 19.0;
    }
    // The achievement it comes with: a click opens it.
    let mut picked = None;
    if let Some((k, a)) = source(r, have) {
        y += 18.0;
        let b = Rect::new(x, y, inner, 56.0);
        let hover = ui.hovering(b);
        let lit = ui.hover_anim(b, hover);
        let metal = medal_color(a.level);
        rrect(b, 10.0, Color::new(metal.r, metal.g, metal.b, 0.06 + 0.06 * lit));
        rrect_lines(b, 10.0, 1.0, crate::ui::mix(BORDER, metal, lit));
        let won = unlocked_at(p, a).is_some();
        crate::profile::medal_art(vec2(b.x + 30.0, b.y + 28.0), 44.0, a.level, !won);
        let tx = b.x + 60.0;
        let room = b.w - 60.0 - 34.0;
        let (title, size) = ui.fitted(t(a.title), 15.0, room, true);
        ui.text(&title, tx, b.y + 10.0, size, CREAM, true);
        let (sub, ss) = ui.fitted(t("See the achievement"), 12.0, room, false);
        ui.text(&sub, tx, b.y + 31.0, ss, if hover { GOLD } else { MUTED }, false);
        crate::icons::draw(Icon::ChevronRight, vec2(b.x + b.w - 20.0, b.y + 28.0), 14.0, MUTED);
        ui.block(b);
        if hover && ui.clicked(b) {
            picked = Some(Detail::Achievement(k));
        }
    }
    picked
}
