//! Main menu: who is signed in, the four ways to play, and a row of icons.

use macroquad::prelude::*;

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_circle_lines, draw_line, draw_rectangle};

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
    /// The button under a note on the board.
    Note(snookered_proto::NoteAction),
    /// The lounge's daily games (some are waiting for the player).
    Daily,
    TwoPlayers,
    Lan,
    Ranking,
    Profile,
    Shop,
    Settings,
    Quit,
}

#[derive(Default)]
pub struct Menu {
    /// Hover animation of each item (0..1).
    hover: [f32; 4],
    shade: Option<Texture2D>,
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
}

impl Metrics {
    fn for_height(sh: f32) -> Metrics {
        if sh >= 600.0 {
            Metrics { emblem: 92.0, title: 64.0, card_h: 66.0, gap: 12.0, icon: 46.0 }
        } else {
            Metrics { emblem: 72.0, title: 50.0, card_h: 58.0, gap: 8.0, icon: 42.0 }
        }
    }
}

/// Room kept for the player card at the top right.
const CARD_W: f32 = 280.0;
const EDGE: f32 = 20.0;

impl Menu {
    /// `saved`: object balls left in the saved game against the computer.
    pub fn draw(&mut self, ui: &mut Ui, online: &Online, saved: Option<usize>) -> MenuAction {
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
        let name = snookered_proto::GAME_NAME.to_uppercase();
        let room = sw - player_card_w(sw) - EDGE - 24.0 - tx;
        let size = ui.fit(m.title, &name, room).max(m.title * 0.6);
        let ty = top + (m.title - size) * 0.5;
        ui.text(&name, tx + 3.0, ty + 3.0, size, Color::new(0.0, 0.0, 0.0, 0.6), true);
        ui.text(&name, tx, ty, size, GOLD, true);
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
        let resume_line = saved.map(|n| tw("Game in progress: {n} balls left", &[("n", &n)]));
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
                    .unwrap_or(t("Ten rivals, a ladder and ghosts of real players")),
            ),
            (Icon::People, t("Two players"), t("Take turns on this computer")),
            (Icon::Network, t("Local network"), t("Play with friends on the same network")),
        ];
        // Wide enough for every line (and the "Continue" button beside the
        // saved game's), as far as the window allows.
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
                    _ => 0.0,
                };
                70.0 + text + button + 24.0
            })
            .fold(0.0, f32::max);
        let w = (sw * 0.36).clamp(300.0, 440.0).max(needed).min(sw - left - EDGE);
        let h = m.card_h;
        let block = 4.0 * h + 3.0 * m.gap;
        let below = 24.0 + m.icon + EDGE;
        let mut y = ((sh - block) / 2.0 + 40.0).min(sh - below - block).max(top + m.emblem + 16.0);
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
            let bg = if primary {
                Color::new(0.10 + 0.06 * a, 0.36 + 0.1 * a, 0.20 + 0.06 * a, 0.98)
            } else {
                Color::new(0.10 + 0.12 * a, 0.065 + 0.08 * a, 0.04 + 0.04 * a, 0.97)
            };
            let bg = if pressed { crate::ui::shade(bg, -0.12) } else { bg };
            rrect(
                Rect::new(card.x, card.y + 3.0, card.w, card.h),
                10.0,
                Color::new(0.0, 0.0, 0.0, if pressed { 0.2 } else { 0.35 }),
            );
            crate::ui::rrect_gradient(
                card,
                10.0,
                crate::ui::shade(bg, 0.08),
                crate::ui::shade(bg, -0.22),
            );
            let edge = if hover {
                GOLD
            } else if primary {
                Color::new(0.35, 0.75, 0.5, 1.0)
            } else {
                BORDER
            };
            rrect_lines(card, 10.0, 1.4, edge);
            crate::icons::draw(
                icon,
                vec2(card.x + 36.0, card.y + h / 2.0),
                28.0,
                if hover { GOLD } else { CREAM },
            );
            let text_room = card.w - 70.0 - 16.0 - resume.map_or(0.0, |b| b.w + 12.0);
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
            // An arrow slides in on hover: this card leads somewhere.
            if resume.is_none() && a > 0.02 {
                let c = vec2(card.x + card.w - 22.0 - (1.0 - a) * 6.0, card.y + h / 2.0);
                let col = Color::new(GOLD.r, GOLD.g, GOLD.b, a);
                draw_line(c.x - 4.0, c.y - 7.0, c.x + 3.0, c.y, 2.2, col);
                draw_line(c.x + 3.0, c.y, c.x - 4.0, c.y + 7.0, 2.2, col);
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
                action = match k {
                    0 => MenuAction::Online,
                    1 => MenuAction::VsComputer,
                    2 => MenuAction::TwoPlayers,
                    _ => MenuAction::Lan,
                };
            }
            y += h + m.gap;
        }

        // Icons along the bottom, lined up with the cards.
        let icons = [
            (Icon::Trophy, t("World ranking"), MenuAction::Ranking),
            (Icon::Person, t("Profile"), MenuAction::Profile),
            (Icon::Shop, t("Shop"), MenuAction::Shop),
            (Icon::Gear, t("Settings"), MenuAction::Settings),
            (Icon::Power, t("Quit"), MenuAction::Quit),
        ];
        let by = y - m.gap + 24.0;
        for (k, (icon, tip, act)) in icons.into_iter().enumerate() {
            let r = Rect::new(left + k as f32 * (m.icon + 12.0), by, m.icon, m.icon);
            if ui.icon_button(r, icon, tip, true) {
                action = act;
            }
            // Presents waiting: a dot on the profile.
            if k == 1 && online.gift_badge() {
                draw_circle(r.x + r.w - 7.0, r.y + 7.0, 4.5, crate::ui::ALERT);
            }
        }

        // Who is signed in, top right, and the board under it.
        let card = self.player_card(ui, online, &mut action);
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
        let sw = crate::ui::vw();
        let w = 320.0f32.min(sw * 0.3);
        let x = sw - w - EDGE;
        if notes.is_empty() || x < menu_right + 24.0 {
            return;
        }
        let mut y = top;
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
    fn player_card(&self, ui: &mut Ui, online: &Online, action: &mut MenuAction) -> Rect {
        let sw = crate::ui::vw();
        let w = player_card_w(sw);
        let r = Rect::new(sw - w - EDGE, EDGE, w, 64.0);
        let hover = ui.hovering(r);
        let pressed = hover && is_mouse_button_down(MouseButton::Left);
        let bg = Color::new(0.05, 0.035, 0.025, if hover { 0.94 } else { 0.8 });
        crate::ui::rrect_gradient(
            r,
            10.0,
            crate::ui::shade(bg, if pressed { 0.0 } else { 0.06 }),
            bg,
        );
        rrect_lines(r, 10.0, 1.2, if hover { GOLD } else { BORDER });
        ui.block(r);
        // The avatar: a disc with the person in it.
        let avatar = vec2(r.x + 34.0, r.y + 32.0);
        let ring = match &online.me {
            Some(me) => crate::ui::name_color(me),
            None if online.outdated => GOLD,
            None => BORDER,
        };
        draw_circle(avatar.x, avatar.y, 21.0, Color::new(0.13, 0.09, 0.06, 1.0));
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
                crate::icons::draw(Icon::Person, avatar, 24.0, crate::ui::name_color(me));
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
                    let (level, into, span) = snookered_proto::xp::progress(xp);
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
                        let tip = tw(
                            "Level {n} · {into} / {span} XP to the next",
                            &[("n", &level), ("into", &into), ("span", &span)],
                        );
                        ui.tooltip(&tip, r);
                    }
                }
                let rating = me.rating.to_string();
                let rw = ui.text_size(&rating, 13.0, true).x;
                ui.text(&rating, text_x, r.y + 37.0, 13.0, CREAM, true);
                ui.text(
                    &format!("·  {}", t(me.tier().name())),
                    text_x + rw + 6.0,
                    r.y + 37.0,
                    13.0,
                    crate::ui::tier_color(me.tier()),
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
