//! Main menu: who is signed in, the four ways to play, and a row of icons.

use macroquad::prelude::*;

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_circle_lines, draw_line, draw_rectangle};

use snook_legend_core::Rules;

use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::online::{Online, Stage};
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

pub enum MenuAction {
    None,
    Online,
    VsComputer,
    /// Picks up the saved game against the computer.
    ContinueGame,
    /// Get the new version (the server refused this one, or the updater
    /// can't install it here: the releases page).
    Update,
    /// Start the new version the updater installed.
    Restart,
    /// The notes of this version (the version label).
    WhatsNew,
    /// The rules of the game (the book open on these rules).
    Rules(Rules),
    /// The player switched the rules of their games.
    SetRules(Rules),
    /// The button under a note on the board.
    Note(snook_legend_proto::NoteAction),
    /// The lounge's daily games (some are waiting for the player).
    Daily,
    TwoPlayers,
    Lan,
    Ranking,
    Profile,
    Shop,
    /// Everything the player has (see `collection`).
    Collection,
    Settings,
    Quit,
    /// The inbox (signed in; else the sign-in first).
    Inbox,
}

#[derive(Default)]
pub struct Menu {
    /// Hover animation of each item (0..1).
    hover: [f32; 4],
    /// And of each place along the bottom.
    dock_hover: [f32; 8],
    shade: Option<Texture2D>,
    /// Where the menu's column (cards and the row of places) ends on the
    /// right, as last drawn: notices go to the right of it.
    pub column_right: f32,
    /// And where the game's name ends (what pops up at the top keeps
    /// clear of it).
    pub title_right: f32,
}

/// The game's name, drawn by `tools/gen_wordmark.py` (gold lit from above,
/// a fine dark edge and a soft shadow), in a 2048 x 256 texture: where the
/// picture is in it, and where its capitals are in the picture.
const WORDMARK: Rect = Rect { x: 0.0, y: 0.0, w: 2046.0, h: 227.0 };
const WORDMARK_CAP_TOP: f32 = 10.7;
const WORDMARK_CAP_H: f32 = 188.6;

thread_local! {
    static WORDMARK_TEX: std::cell::OnceCell<Texture2D> = const { std::cell::OnceCell::new() };
}

fn wordmark() -> Texture2D {
    WORDMARK_TEX.with(|w| {
        w.get_or_init(|| {
            let tex =
                Texture2D::from_file_with_format(include_bytes!("../assets/wordmark.png"), None);
            // Mipmapped: drawn at a fraction of its size (its sides are
            // powers of two, so every driver can).
            let id = tex.raw_miniquad_id();
            let ctx = unsafe { get_internal_gl() }.quad_context;
            ctx.texture_generate_mipmaps(id);
            ctx.texture_set_min_filter(
                id,
                FilterMode::Linear,
                macroquad::miniquad::MipmapFilterMode::Linear,
            );
            ctx.texture_set_mag_filter(id, FilterMode::Linear);
            tex
        })
        .clone()
    })
}

/// Draws the wordmark with its capitals `cap` tall, their top-left corner
/// at (`x`, `y`); returns how wide it is.
pub(crate) fn draw_wordmark(x: f32, y: f32, cap: f32) -> f32 {
    let k = cap / WORDMARK_CAP_H;
    let (w, h) = (WORDMARK.w * k, WORDMARK.h * k);
    draw_texture_ex(
        &wordmark(),
        x,
        y - WORDMARK_CAP_TOP * k,
        Color::new(1.0, 1.0, 1.0, crate::paint::alpha()),
        DrawTextureParams {
            dest_size: Some(vec2(w, h)),
            source: Some(WORDMARK),
            ..Default::default()
        },
    );
    w
}

/// A smooth left-to-right darkening (a texture, so there is no banding).
fn shade_texture() -> Texture2D {
    const W: u16 = 256;
    let mut rgba = Vec::with_capacity(W as usize * 4);
    for x in 0..W {
        let k = x as f32 / (W - 1) as f32;
        let a = 0.82 * (1.0 - k * k * (3.0 - 2.0 * k));
        rgba.extend_from_slice(&[0, 0, 0, (a * 255.0) as u8]);
    }
    let tex = Texture2D::from_rgba8(W, 1, &rgba);
    tex.set_filter(FilterMode::Linear);
    tex
}

/// Sizes of the menu, roomy or (when the window is short for the interface
/// size) compact.
struct Metrics {
    emblem: f32,
    title: f32,
    card_h: f32,
    gap: f32,
    icon: f32,
    /// The rules' switch, and the space over and under it.
    switch_h: f32,
}

impl Metrics {
    fn for_height(sh: f32) -> Metrics {
        if sh >= 640.0 {
            Metrics {
                emblem: 92.0,
                title: 64.0,
                card_h: 66.0,
                gap: 12.0,
                icon: 56.0,
                switch_h: 46.0,
            }
        } else {
            Metrics {
                emblem: 72.0,
                title: 50.0,
                card_h: 58.0,
                gap: 8.0,
                icon: 48.0,
                switch_h: 38.0,
            }
        }
    }

    /// The label over the switch, the switch and the gap under it.
    fn rules_block(&self) -> f32 {
        20.0 + self.switch_h + self.gap + 6.0
    }
}

use crate::ui::mix;

/// The surface of a card of the menu: a soft shadow, a body lit from above
/// with a faint light along its top edge, and an edge that turns gold as
/// the pointer comes (`a`, 0..1). The first card is green.
fn card_surface(card: Rect, radius: f32, primary: bool, a: f32, pressed: bool) {
    crate::ui::soft_shadow(card, radius, if pressed { 0.4 } else { 0.7 + 0.5 * a });
    let (top, bottom) = if primary {
        (
            Color::new(0.15 + 0.05 * a, 0.46 + 0.06 * a, 0.28 + 0.04 * a, 0.98),
            Color::new(0.07, 0.29, 0.16, 0.98),
        )
    } else {
        (
            Color::new(0.13 + 0.07 * a, 0.095 + 0.05 * a, 0.07 + 0.03 * a, 0.94),
            Color::new(0.07, 0.05, 0.037, 0.94),
        )
    };
    let k = if pressed { -0.15 } else { 0.0 };
    crate::ui::rrect_gradient(card, radius, crate::ui::shade(top, k), crate::ui::shade(bottom, k));
    draw_line(
        card.x + radius,
        card.y + 1.0,
        card.x + card.w - radius,
        card.y + 1.0,
        1.0,
        Color::new(1.0, 1.0, 1.0, if primary { 0.18 } else { 0.06 }),
    );
    let edge = if primary {
        Color::new(0.5, 0.9, 0.65, 0.3 + 0.45 * a)
    } else {
        Color::new(GOLD.r, GOLD.g, GOLD.b, 0.1 + 0.6 * a)
    };
    rrect_lines(card, radius, 1.0, edge);
}

/// Room kept for the player card at the top right.
const CARD_W: f32 = 280.0;
const EDGE: f32 = 20.0;

impl Menu {
    /// `saved`: object balls left in the saved game against the computer;
    /// `rules`: the rules of the player's games.
    pub fn draw(
        &mut self,
        ui: &mut Ui,
        online: &Online,
        saved: Option<usize>,
        rules: Rules,
    ) -> MenuAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let dt = get_frame_time();
        let m = Metrics::for_height(sh);
        let mut action = MenuAction::None;
        // Darker on the left, where the menu is, so the table stays visible.
        let shade = self.shade.get_or_insert_with(shade_texture);
        draw_texture_ex(
            shade,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams { dest_size: Some(vec2(sw * 0.7, sh)), ..Default::default() },
        );

        let left = (sw * 0.06).clamp(24.0, 90.0);
        let top = (sh * 0.09).clamp(20.0, 80.0);
        // The emblem, then the name (smaller if it would run under the
        // player card).
        crate::icon::draw(left - 4.0, top + 2.0, m.emblem);
        let tx = left + m.emblem + 10.0;
        // The wordmark: as tall as the title's capitals, smaller if it would
        // run under the player card.
        let room = sw - player_card_w(sw) - EDGE - 24.0 - tx;
        let cap = (m.title * 0.73).min(room * WORDMARK_CAP_H / WORDMARK.w).max(m.title * 0.4);
        self.title_right = tx + draw_wordmark(tx, top + (m.title * 0.8 - cap) - 2.0, cap);
        let tagline = top + m.emblem * 0.8;
        ui.text(
            t("eight-ball pool"),
            tx + 4.0,
            tagline,
            if m.emblem > 80.0 { 17.0 } else { 15.0 },
            CREAM,
            false,
        );

        // The four ways to play.
        let resume_line = saved.map(|n| {
            if n == 1 {
                t("Game in progress: 1 ball left").to_string()
            } else {
                tw("Game in progress: {n} balls left", &[("n", &n)])
            }
        });
        // Daily games waiting for the player (their turn, or a challenge).
        let me = online.me.as_ref().map(|m| m.username.as_str()).unwrap_or("");
        let waiting = if online.signed_in() { online.corr.waiting(me) } else { 0 };
        let waiting_line = match waiting {
            0 => None,
            1 => Some(t("A daily game is waiting for you").to_string()),
            n => Some(tw("{n} daily games are waiting for you", &[("n", &n)])),
        };
        let items = [
            (
                Icon::Globe,
                t("Play online"),
                waiting_line
                    .as_deref()
                    .unwrap_or(t("Rated games, tables, tournaments and friends")),
            ),
            (
                Icon::Robot,
                t("Against the computer"),
                resume_line
                    .as_deref()
                    .unwrap_or(t("Fifteen rivals, a tour and ghosts of real players")),
            ),
            (Icon::People, t("Two players"), t("Take turns on this computer")),
            (Icon::Network, t("Local network"), t("Play with friends on the same network")),
        ];
        // Wide enough for every line (and the "Continue" button beside the
        // saved game's, or the count of players), as far as the window
        // allows.
        let counts = online.counts();
        let count_room = if counts.is_some() { 64.0 } else { 0.0 };
        let resume_w = 112.0;
        let daily_w = (ui.text_size(t("Daily games"), 14.0, true).x + 40.0).max(resume_w);
        let needed = items
            .iter()
            .enumerate()
            .map(|(k, (_, title, sub))| {
                let text = ui.text_size(title, 20.0, true).x.max(ui.text_size(sub, 13.0, false).x);
                let button = match k {
                    0 if waiting > 0 => daily_w + 12.0,
                    1 if saved.is_some() => resume_w + 12.0,
                    0 | 1 => count_room,
                    _ => 0.0,
                };
                // (The chevron's room when there is no button.)
                let chevron = if button > 0.0 && button != count_room { 0.0 } else { 26.0 };
                70.0 + text + button + chevron + 24.0
            })
            .fold(0.0, f32::max);
        let w = (sw * 0.36).clamp(300.0, 440.0).max(needed).min(sw - left - EDGE);
        self.column_right = left + w;
        let h = m.card_h;
        let block = m.rules_block() + 4.0 * h + 3.0 * m.gap;
        let below = 24.0 + m.icon + EDGE;
        let mut y = ((sh - block) / 2.0 + 40.0).min(sh - below - block).max(top + m.emblem + 16.0);

        // The rules of the player's games: one switch for everything (online,
        // the computer, two players), with how many play each right now.
        let label = t("RULES OF YOUR GAMES");
        ui.text(label, left + 2.0, y, 12.0, MUTED, true);
        let info = Rect::new(left + ui.text_size(label, 12.0, true).x + 8.0, y - 3.0, 18.0, 18.0);
        crate::paint::draw_circle_lines(info.x + 9.0, info.y + 9.0, 7.0, 1.3, MUTED);
        ui.text_centered("i", info.x + 9.0, info.y + 2.5, 11.0, MUTED, true);
        if ui.hovering(info) {
            ui.tooltip(t("What changes between the rules: click to read them"), info);
            if ui.clicked(info) {
                action = MenuAction::Rules(rules);
            }
        }
        y += 20.0;
        let players = counts.map(|c| c.map(|_, n| n.players));
        if let Some(chosen) =
            crate::rulesui::switch(ui, Rect::new(left, y, w, m.switch_h), rules, players)
        {
            action = MenuAction::SetRules(chosen);
        }
        y += m.switch_h + m.gap + 6.0;
        // How many play this way under these rules (online, or the computer).
        let mine = counts.map(|c| *c.get(rules));
        for (k, (icon, title, sub)) in items.into_iter().enumerate() {
            let r = Rect::new(left, y, w, h);
            let hover = ui.hovering(r);
            let target = if hover { 1.0 } else { 0.0 };
            self.hover[k] += (target - self.hover[k]) * (1.0 - (-dt * 14.0).exp());
            let a = self.hover[k];
            // A button of its own inside the card: the saved game, or the
            // daily games waiting.
            let side = match k {
                0 if waiting > 0 => Some(daily_w),
                1 if saved.is_some() => Some(resume_w),
                _ => None,
            };
            let resume = side.map(|bw| {
                let bw = bw.min(w * 0.4);
                Rect::new(r.x + r.w - bw - 16.0 + a * 8.0, r.y + (h - 36.0) / 2.0, bw, 36.0)
            });
            let on_resume = resume.is_some_and(|b| ui.hovering(b));
            // Pressed: the card sinks back a little.
            let pressed = hover && !on_resume && is_mouse_button_down(MouseButton::Left);
            let shift = a * 8.0 - if pressed { 3.0 } else { 0.0 };
            let card = Rect::new(r.x + shift, r.y + if pressed { 1.0 } else { 0.0 }, r.w, r.h);
            ui.block(r);
            let primary = k == 0;
            card_surface(card, 12.0, primary, a, pressed);
            // The icon in a disc of its own.
            let ic = vec2(card.x + 38.0, card.y + h / 2.0);
            let disc = (h * 0.34).min(22.0);
            let disc_color = if primary {
                Color::new(1.0, 1.0, 1.0, 0.12 + 0.06 * a)
            } else {
                Color::new(GOLD.r, GOLD.g, GOLD.b, 0.10 + 0.08 * a)
            };
            draw_circle(ic.x, ic.y, disc, disc_color);
            let icon_color = if primary { WHITE } else { mix(CREAM, GOLD, a) };
            crate::icons::draw(icon, ic, disc * 1.05, icon_color);
            // Players here now under these rules, on the right.
            let count = match (k, mine) {
                (0, Some(c)) => Some(c.online),
                (1, Some(c)) => Some(c.vs_computer),
                _ => None,
            }
            .filter(|_| resume.is_none())
            .map(|n| n.to_string());
            let count_w = count.as_ref().map_or(0.0, |n| ui.text_size(n, 17.0, true).x + 40.0);
            let chevron_w = if resume.is_none() { 26.0 } else { 0.0 };
            let text_room =
                card.w - 70.0 - 16.0 - resume.map_or(0.0, |b| b.w + 12.0) - count_w - chevron_w;
            let (title, title_size) = ui.fitted(title, 20.0, text_room, true);
            let lines = h / 2.0 - 20.0;
            ui.text(
                &title,
                card.x + 70.0,
                card.y + lines + (20.0 - title_size) / 2.0,
                title_size,
                if primary { WHITE } else { CREAM },
                true,
            );
            let sub_color =
                if k == 0 && waiting > 0 { GOLD } else { Color::new(0.85, 0.8, 0.72, 0.85) };
            ui.text(
                &ui.ellipsize(sub, text_room, 13.0),
                card.x + 70.0,
                card.y + lines + 26.0,
                13.0,
                sub_color,
                false,
            );
            if let Some(n) = &count {
                // Left of the chevron.
                let right = card.x + card.w - 44.0;
                let nw = ui.text_size(n, 17.0, true).x;
                let cy = card.y + h / 2.0;
                let nx = right - nw;
                crate::icons::draw(Icon::Person, vec2(nx - 11.0, cy), 15.0, CREAM);
                draw_circle(nx - 26.0, cy, 3.5, Color::new(0.35, 0.85, 0.45, 0.95));
                ui.text(n, nx, crate::ranking::text_y(cy, 17.0), 17.0, CREAM, true);
                let tip = if k == 0 {
                    t("Players online under these rules (the lounge and online games)")
                } else {
                    t("Players against the computer under these rules")
                };
                if ui.hovering(Rect::new(nx - 34.0, cy - 14.0, nw + 38.0, 28.0)) {
                    ui.tooltip(tip, Rect::new(nx - 34.0, cy - 14.0, nw + 38.0, 28.0));
                }
            }
            // A chevron: this card leads somewhere (it brightens and nudges
            // on hover).
            if resume.is_none() {
                let c = vec2(card.x + card.w - 24.0 + a * 3.0, card.y + h / 2.0);
                let col = if primary { WHITE } else { mix(MUTED, GOLD, a) };
                crate::icons::draw(
                    Icon::ChevronRight,
                    c,
                    16.0,
                    Color { a: 0.45 + 0.55 * a, ..col },
                );
            }
            // The saved game (or the daily games): a button of its own.
            if let Some(b) = resume {
                let label = if k == 0 { t("Daily games") } else { t("Continue") };
                if k == 0 {
                    if ui.button(b, label, true) {
                        action = MenuAction::Daily;
                    }
                    // How many, in a red badge on its corner.
                    let n = waiting.to_string();
                    let bw = (ui.text_size(&n, 11.0, true).x + 10.0).max(18.0);
                    let badge = Rect::new(b.x + b.w - bw + 6.0, b.y - 6.0, bw, 18.0);
                    rrect(badge, 9.0, crate::ui::ALERT);
                    ui.text_centered(&n, badge.x + bw / 2.0, badge.y + 3.0, 11.0, WHITE, true);
                } else if ui.primary_button(b, label) {
                    action = MenuAction::ContinueGame;
                }
            }
            if hover && ui.clicked(r) && !on_resume {
                ui.note_click();
                action = match k {
                    0 => MenuAction::Online,
                    1 => MenuAction::VsComputer,
                    2 => MenuAction::TwoPlayers,
                    _ => MenuAction::Lan,
                };
            }
            y += h + m.gap;
        }

        // Along the bottom, lined up with the cards: where else to go, each
        // with its name; quitting apart, at the end.
        let dock = [
            (Icon::Trophy, t("Ranking"), Some(t("World ranking")), MenuAction::Ranking),
            (Icon::Person, t("Profile"), None, MenuAction::Profile),
            // (Drawn as an envelope: see below.)
            (Icon::List, t("Inbox"), None, MenuAction::Inbox),
            (Icon::Shop, t("Shop"), None, MenuAction::Shop),
            (Icon::Star, t("Collection"), Some(t("Everything you have")), MenuAction::Collection),
            (Icon::Book, t("Rules"), Some(t("Rules of the game")), MenuAction::Rules(rules)),
            (Icon::Gear, t("Settings"), None, MenuAction::Settings),
        ];
        let by = y - m.gap + 22.0;
        let dh = m.icon;
        let gap = 8.0;
        // Quitting says so too (an icon alone read as "turn off").
        let quit_w = (ui.text_size(t("Quit"), 12.0, true).x + 22.0).max(dh * 0.8).round();
        let room = w - quit_w - 14.0 - gap * (dock.len() - 1) as f32;
        // Each as wide as its name (at least square); the names smaller if
        // they don't fit, and left out if even that isn't enough.
        let natural = |size: f32| -> Vec<f32> {
            dock.iter().map(|d| (ui.text_size(d.1, size, true).x + 22.0).max(dh)).collect()
        };
        let mut label = 12.0;
        let mut widths = natural(label);
        while widths.iter().sum::<f32>() > room && label > 9.6 {
            label -= 0.5;
            widths = natural(label);
        }
        let show_labels = widths.iter().sum::<f32>() <= room + 0.5;
        if show_labels {
            // The room left shared out: the row ends where the cards do.
            let extra = (room - widths.iter().sum::<f32>()) / widths.len() as f32;
            widths.iter_mut().for_each(|w| *w += extra);
        } else {
            widths = vec![(room / dock.len() as f32).min(dh); dock.len()];
        }
        let mut x = left;
        for (k, ((icon, name, tip, act), iw)) in dock.into_iter().zip(widths).enumerate() {
            let r = Rect::new(x, by, iw, dh);
            x += iw + gap;
            let hover = ui.hovering(r);
            self.dock_hover[k] +=
                (if hover { 1.0 } else { 0.0 } - self.dock_hover[k]) * (1.0 - (-dt * 14.0).exp());
            let a = self.dock_hover[k];
            let pressed = hover && is_mouse_button_down(MouseButton::Left);
            ui.block(r);
            card_surface(r, 10.0, false, a, pressed);
            let color = mix(CREAM, GOLD, a);
            // The inbox's envelope (the icon font has none).
            let envelope = matches!(act, MenuAction::Inbox);
            let draw_icon = |c: Vec2, s: f32| {
                if envelope {
                    crate::online::draw_envelope(c, s, color);
                } else {
                    crate::icons::draw(icon, c, s, color);
                }
            };
            if show_labels {
                draw_icon(vec2(r.x + r.w / 2.0, r.y + dh * 0.38), 21.0);
                ui.text_centered(
                    name,
                    r.x + r.w / 2.0,
                    r.y + dh * 0.64,
                    label,
                    mix(MUTED, CREAM, a),
                    true,
                );
            } else {
                draw_icon(vec2(r.x + r.w / 2.0, r.y + dh / 2.0), 22.0);
            }
            if hover {
                if let Some(tip) = tip.or((!show_labels).then_some(name)) {
                    ui.tooltip(tip, r);
                }
                if ui.clicked(r) {
                    ui.note_click();
                    action = act;
                }
            }
            // Presents waiting: a dot on the profile.
            if k == 1 && online.gift_badge() {
                draw_circle(r.x + r.w - 8.0, r.y + 8.0, 4.5, crate::ui::ALERT);
            }
            // What isn't read yet in the inbox.
            if envelope {
                crate::online::draw_badge(ui, r, online.inbox_unread());
            }
        }
        // Quitting: plain, apart from the rest.
        let q = Rect::new(left + w - quit_w, by, quit_w, dh);
        let hover = ui.hovering(q);
        ui.block(q);
        if hover {
            rrect(q, 10.0, Color::new(1.0, 1.0, 1.0, 0.05));
        }
        rrect_lines(q, 10.0, 1.0, Color::new(1.0, 1.0, 1.0, if hover { 0.14 } else { 0.07 }));
        let quit_color = if hover { Color::new(0.95, 0.55, 0.45, 1.0) } else { MUTED };
        if show_labels {
            crate::icons::draw(
                Icon::DoorOpen,
                vec2(q.x + q.w / 2.0, q.y + dh * 0.38),
                20.0,
                quit_color,
            );
            ui.text_centered(t("Quit"), q.x + q.w / 2.0, q.y + dh * 0.64, label, quit_color, true);
        } else {
            crate::icons::draw(
                Icon::DoorOpen,
                vec2(q.x + q.w / 2.0, q.y + dh / 2.0),
                20.0,
                quit_color,
            );
        }
        if hover {
            if !show_labels {
                ui.tooltip(t("Quit"), q);
            }
            if ui.clicked(q) {
                ui.note_click();
                action = MenuAction::Quit;
            }
        }

        // Who is signed in, top right, and the board under it.
        let card = self.player_card(ui, online, rules, &mut action);
        // A new version, bottom right (never in the way of a game).
        let notice_top = update_notice(ui, left + w, &mut action);
        let bottom = notice_top.unwrap_or(sh - 60.0);
        self.board(ui, online, left + w, card.y + card.h + 24.0, bottom, &mut action);

        // The version: a click shows what's new in it.
        let version = concat!("v", env!("CARGO_PKG_VERSION"));
        let size = ui.text_size(version, 12.0, false);
        let at = vec2(sw - size.x - EDGE, sh - EDGE - 12.0);
        let hit = Rect::new(at.x - 6.0, at.y - 5.0, size.x + 12.0, 24.0);
        let hover = ui.hovering(hit);
        ui.block(hit);
        ui.text(version, at.x, at.y, 12.0, if hover { GOLD } else { MUTED }, false);
        if hover {
            draw_line(at.x, at.y + 15.0, at.x + size.x, at.y + 15.0, 1.0, GOLD);
            ui.tooltip(t("What's new"), hit);
            if ui.clicked(hit) {
                action = MenuAction::WhatsNew;
            }
        }
        action
    }

    /// The newest notes on the lounge's board (pinned first), for players
    /// who don't go to the lounge, beside the menu (which ends at
    /// `menu_right`) when the window leaves room.
    fn board(
        &self,
        ui: &mut Ui,
        online: &Online,
        menu_right: f32,
        top: f32,
        bottom: f32,
        action: &mut MenuAction,
    ) {
        let notes = online.board_now();
        let events = online.events_now();
        let sw = crate::ui::vw();
        let w = 320.0f32.min(sw * 0.3);
        let x = sw - w - EDGE;
        if (notes.is_empty() && events.is_empty()) || x < menu_right + 24.0 {
            return;
        }
        let mut y = top;
        // The staff's events first: double experience, a themed cup…
        if !events.is_empty() {
            ui.text(t("Events"), x + 2.0, y, 15.0, GOLD, true);
            y += 24.0;
            let now = crate::game::unix_now();
            for e in &events {
                let h = crate::online::event_height(ui, e, w);
                if y + h > bottom {
                    return;
                }
                let card = Rect::new(x, y, w, h);
                rrect(card, 8.0, Color::new(0.05, 0.035, 0.025, 0.86));
                crate::online::draw_event(ui, e, card, now);
                ui.block(card);
                y += h + 8.0;
            }
            y += 10.0;
        }
        if notes.is_empty() {
            return;
        }
        ui.text(t("Board"), x + 2.0, y, 15.0, GOLD, true);
        y += 24.0;
        for n in notes.iter().take(3) {
            let h = crate::online::note_height(ui, n, w);
            if y + h > bottom {
                break;
            }
            let card = Rect::new(x, y, w, h);
            rrect(card, 8.0, Color::new(0.05, 0.035, 0.025, 0.86));
            if let Some(a) = crate::online::draw_note(ui, n, card) {
                *action = MenuAction::Note(a);
            }
            ui.block(card);
            y += h + 8.0;
        }
    }

    /// The player card: who is signed in (or how to), at the top right.
    /// Returns where it is.
    fn player_card(
        &self,
        ui: &mut Ui,
        online: &Online,
        rules: Rules,
        action: &mut MenuAction,
    ) -> Rect {
        let sw = crate::ui::vw();
        let w = player_card_w(sw);
        let r = Rect::new(sw - w - EDGE, EDGE, w, 64.0);
        let hover = ui.hovering(r);
        let pressed = hover && is_mouse_button_down(MouseButton::Left);
        card_surface(r, 12.0, false, if hover { 1.0 } else { 0.0 }, pressed);
        ui.block(r);
        // The avatar: a disc with the person in it.
        let avatar = vec2(r.x + 34.0, r.y + 32.0);
        let ring = match &online.me {
            Some(me) => crate::ui::name_color(me),
            None if online.outdated => GOLD,
            None => BORDER,
        };
        draw_circle(avatar.x, avatar.y, 21.0, Color::new(0.13, 0.09, 0.06, 1.0));
        // The picture of the account signed in (none signed in: the plain
        // person, as before).
        let picture = match &online.me {
            Some(me) if !me.avatar.is_empty() => me.avatar.clone(),
            Some(me) if !me.guest => crate::avatars::mine(),
            _ => String::new(),
        };
        let has_picture = snook_legend_proto::valid_avatar(&picture) && !picture.is_empty();
        if has_picture {
            crate::avatars::draw(&picture, avatar, 20.0);
        }
        draw_circle_lines(avatar.x, avatar.y, 21.0, 1.5, ring);
        let text_x = r.x + 66.0;
        let text_w = r.w - 66.0 - 34.0;
        let busy = match online.stage {
            _ if online.me.is_some() || online.outdated => None,
            Stage::Connecting | Stage::Authenticating if online.has_session() => {
                Some((t("Signing in"), t("connecting to the server")))
            }
            Stage::Reconnecting { .. } => Some((t("Reconnecting"), t("the connection dropped"))),
            _ => None,
        };
        match &online.me {
            Some(me) => {
                if !has_picture {
                    crate::icons::draw(Icon::Person, avatar, 24.0, crate::ui::name_color(me));
                }
                // Online: a green dot on the avatar.
                draw_circle(
                    avatar.x + 15.0,
                    avatar.y + 15.0,
                    6.0,
                    Color::new(0.05, 0.035, 0.025, 1.0),
                );
                draw_circle(
                    avatar.x + 15.0,
                    avatar.y + 15.0,
                    4.5,
                    Color::new(0.35, 0.85, 0.45, 1.0),
                );
                ui.player_name(me, text_x, r.y + 13.0, 17.0, text_w);
                // The account level: a badge on the avatar and its bar along
                // the bottom of the card.
                if let Some(xp) = online.xp.filter(|_| !me.guest) {
                    let (level, into, span) = snook_legend_proto::xp::progress(xp);
                    let badge = vec2(avatar.x - 15.0, avatar.y + 15.0);
                    draw_circle(badge.x, badge.y, 9.5, Color::new(0.05, 0.035, 0.025, 1.0));
                    draw_circle(badge.x, badge.y, 8.0, GOLD);
                    let n = level.to_string();
                    let size = if level >= 10 { 9.0 } else { 11.0 };
                    ui.text_centered(
                        &n,
                        badge.x,
                        badge.y - size / 2.0 - 1.0,
                        size,
                        Color::new(0.1, 0.06, 0.02, 1.0),
                        true,
                    );
                    let bar = Rect::new(r.x + 10.0, r.y + r.h - 6.0, r.w - 20.0, 3.0);
                    crate::ui::rrect(bar, 1.5, Color::new(1.0, 1.0, 1.0, 0.08));
                    let k = if span == 0 { 1.0 } else { into as f32 / span as f32 };
                    crate::ui::rrect(Rect::new(bar.x, bar.y, bar.w * k, bar.h), 1.5, GOLD);
                    if hover {
                        let mut tip = tw(
                            "Level {n} · {into} / {span} XP to the next",
                            &[("n", &level), ("into", &into), ("span", &span)],
                        );
                        // What the level is for: coins at each one, an item
                        // of its own at some.
                        if let Some(next) = crate::profile::next_reward(level) {
                            tip.push('\n');
                            tip.push_str(&next);
                        }
                        ui.tooltip(&tip, r);
                    }
                }
                // The rating and the league under the rules of the
                // player's games (their mark says which).
                crate::rulesui::mark(rules, vec2(text_x + 8.0, r.y + 44.0), 15.0);
                let rx = text_x + 22.0;
                let rating = me.rating_in(rules).to_string();
                let rw = ui.text_size(&rating, 13.0, true).x;
                ui.text(&rating, rx, r.y + 37.0, 13.0, CREAM, true);
                let tier = me.tier_in(rules);
                ui.text(
                    &format!("·  {}", t(tier.name())),
                    rx + rw + 6.0,
                    r.y + 37.0,
                    13.0,
                    crate::ui::tier_color(tier),
                    false,
                );
            }
            None if online.outdated => {
                // This version is too old for the server.
                crate::icons::draw(Icon::Refresh, avatar, 22.0, GOLD);
                ui.text(t("New version out"), text_x, r.y + 13.0, 17.0, GOLD, true);
                let sub = ui.ellipsize(&crate::online::update_label(), text_w, 12.0);
                ui.text(&sub, text_x, r.y + 37.0, 12.0, CREAM, false);
            }
            // Signed in, the connection closed (it dropped, or the account
            // was used elsewhere): still this player, a click connects.
            None if busy.is_none() && online.has_session() && !online.account_name().is_empty() => {
                if !has_picture {
                    crate::icons::draw(Icon::Person, avatar, 24.0, CREAM);
                }
                // A grey dot on the avatar: not connected.
                draw_circle(
                    avatar.x + 15.0,
                    avatar.y + 15.0,
                    6.0,
                    Color::new(0.05, 0.035, 0.025, 1.0),
                );
                draw_circle(
                    avatar.x + 15.0,
                    avatar.y + 15.0,
                    4.5,
                    Color::new(0.55, 0.52, 0.48, 1.0),
                );
                let name = ui.ellipsize(online.account_name(), text_w, 17.0);
                ui.text(&name, text_x, r.y + 13.0, 17.0, CREAM, true);
                let sub = ui.ellipsize(t("offline: click to connect"), text_w, 12.0);
                ui.text(&sub, text_x, r.y + 37.0, 12.0, MUTED, false);
            }
            None => {
                crate::icons::draw(Icon::Person, avatar, 24.0, MUTED);
                let (title, sub) = busy.unwrap_or((t("Sign in"), t("to play online")));
                ui.text(title, text_x, r.y + 13.0, 17.0, CREAM, true);
                if busy.is_some() {
                    // Dots that keep moving: something is happening.
                    let tw = ui.text_size(title, 17.0, true).x;
                    let n = ((get_time() * 2.5) as usize) % 4;
                    ui.text(&".".repeat(n), text_x + tw + 1.0, r.y + 13.0, 17.0, CREAM, true);
                    crate::ui::spinner(vec2(r.x + r.w - 22.0, r.y + 32.0), 7.0, GOLD);
                }
                let sub = ui.ellipsize(sub, text_w, 12.0);
                ui.text(&sub, text_x, r.y + 37.0, 12.0, MUTED, false);
            }
        }
        // A chevron: the card opens something (the spinner stands there
        // while signing in).
        if busy.is_none() {
            let c = vec2(r.x + r.w - 20.0, r.y + 32.0);
            let col = if hover { GOLD } else { Color::new(MUTED.r, MUTED.g, MUTED.b, 0.7) };
            draw_line(c.x - 3.0, c.y - 6.0, c.x + 3.0, c.y, 2.0, col);
            draw_line(c.x + 3.0, c.y, c.x - 3.0, c.y + 6.0, 2.0, col);
        }
        if hover && ui.clicked(r) {
            *action = if online.me.is_some() {
                MenuAction::Profile
            } else if online.outdated {
                MenuAction::Update
            } else {
                MenuAction::Online
            };
        }
        if hover {
            let tip = if online.me.is_some() {
                t("Your profile")
            } else if online.outdated {
                t("Get the new version to play online")
            } else {
                t("Sign in or create an account")
            };
            ui.tooltip(tip, r);
        }
        r
    }
}

/// The updater's news at the bottom right: a card with "Restart now" (or
/// the releases page) once a new version is there, a quiet line while it
/// downloads. Returns the top of what it drew. `menu_right`: where the
/// menu's cards end (the card goes above them if it would cover them).
fn update_notice(ui: &mut Ui, menu_right: f32, action: &mut MenuAction) -> Option<f32> {
    use crate::update::Status;
    let sw = crate::ui::vw();
    let sh = crate::ui::vh();
    let status = crate::update::status();
    let line_y = sh - EDGE - 12.0;
    let version_w = ui.text_size(concat!("v", env!("CARGO_PKG_VERSION")), 12.0, false).x;
    match &status {
        Status::Downloading { version, done, total } => {
            let p = crate::update::percent(*done, *total);
            let text = tw("Downloading version {v}… {p}%", &[("v", version), ("p", &p)]);
            let tw_ = ui.text_size(&text, 12.0, false).x;
            let x = sw - EDGE - version_w - 18.0 - tw_;
            ui.text(&text, x, line_y, 12.0, MUTED, false);
            let bar = Rect::new(x, line_y + 17.0, tw_, 3.0);
            rrect(bar, 1.5, Color::new(1.0, 1.0, 1.0, 0.08));
            rrect(Rect::new(bar.x, bar.y, bar.w * p as f32 / 100.0, bar.h), 1.5, GOLD);
            Some(line_y - 12.0)
        }
        Status::Installing { version } => {
            let text = tw("Installing version {v}…", &[("v", version)]);
            let x = sw - EDGE - version_w - 18.0 - ui.text_size(&text, 12.0, false).x;
            ui.text(&text, x, line_y, 12.0, MUTED, false);
            Some(line_y - 12.0)
        }
        Status::Ready { version } | Status::Available { version } => {
            let ready = matches!(status, Status::Ready { .. });
            let (title, sub, button) = if ready {
                (
                    tw("Version {v} is ready", &[("v", version)]),
                    t("Restart to play it; nothing is lost."),
                    t("Restart now"),
                )
            } else {
                (
                    tw("Version {v} is out", &[("v", version)]),
                    t("Get it from the releases page."),
                    t("Download"),
                )
            };
            let bw = ui.text_size(button, 15.0, true).x.max(90.0) + 28.0;
            let text_w =
                ui.text_size(&title, 14.0, true).x.max(ui.text_size(sub, 12.0, false).x).min(260.0);
            // Beside the menu's cards, never over them.
            let w = (46.0 + text_w + 14.0 + bw + 12.0)
                .min((sw - EDGE - menu_right - 12.0).max(280.0))
                .min(sw - 2.0 * EDGE);
            let h = 56.0;
            let r = Rect::new(sw - EDGE - w, line_y - 10.0 - h, w, h);
            crate::ui::rrect_gradient(
                r,
                10.0,
                Color::new(0.09, 0.065, 0.045, 0.96),
                Color::new(0.05, 0.035, 0.025, 0.96),
            );
            rrect_lines(r, 10.0, 1.2, if ready { GOLD } else { BORDER });
            ui.block(r);
            crate::icons::draw(Icon::Refresh, vec2(r.x + 24.0, r.y + h / 2.0), 20.0, GOLD);
            let room = r.w - 46.0 - 14.0 - bw - 12.0;
            let (title, size) = ui.fitted(&title, 14.0, room, true);
            ui.text(&title, r.x + 46.0, r.y + 11.0, size, CREAM, true);
            ui.text(&ui.ellipsize(sub, room, 12.0), r.x + 46.0, r.y + 31.0, 12.0, MUTED, false);
            let b = Rect::new(r.x + r.w - 12.0 - bw, r.y + (h - 34.0) / 2.0, bw, 34.0);
            let clicked =
                if ready { ui.primary_button(b, button) } else { ui.button(b, button, true) };
            if clicked {
                *action = if ready { MenuAction::Restart } else { MenuAction::Update };
            }
            Some(r.y)
        }
        _ => None,
    }
}

fn player_card_w(sw: f32) -> f32 {
    CARD_W.min(sw * 0.4)
}
