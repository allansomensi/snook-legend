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
    /// Get the new version (the server refused this one).
    Update,
    /// The button under a note on the board.
    Note(snookered_proto::NoteAction),
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
        let items = [
            (Icon::Globe, t("Play online"), t("Rated games, tables, tournaments and friends")),
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
        let needed = items
            .iter()
            .enumerate()
            .map(|(k, (_, title, sub))| {
                let text = ui.text_size(title, 20.0, true).x.max(ui.text_size(sub, 13.0, false).x);
                let button = if k == 1 && saved.is_some() { resume_w + 12.0 } else { 0.0 };
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
            let resume = (k == 1 && saved.is_some()).then(|| {
                let bw = resume_w.min(w * 0.3);
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
            ui.text(
                &ui.ellipsize(sub, text_room, 13.0),
                card.x + 70.0,
                card.y + lines + 26.0,
                13.0,
                Color::new(0.85, 0.8, 0.72, 0.85),
                false,
            );
            // An arrow slides in on hover: this card leads somewhere.
            if resume.is_none() && a > 0.02 {
                let c = vec2(card.x + card.w - 22.0 - (1.0 - a) * 6.0, card.y + h / 2.0);
                let col = Color::new(GOLD.r, GOLD.g, GOLD.b, a);
                draw_line(c.x - 4.0, c.y - 7.0, c.x + 3.0, c.y, 2.2, col);
                draw_line(c.x + 3.0, c.y, c.x - 4.0, c.y + 7.0, 2.2, col);
            }
            // The saved game: a button of its own inside the card.
            if let Some(b) = resume
                && ui.primary_button(b, t("Continue"))
            {
                action = MenuAction::ContinueGame;
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
        }

        // Who is signed in, top right, and the board under it.
        let card = self.player_card(ui, online, &mut action);
        self.board(ui, online, left + w, card.y + card.h + 24.0, &mut action);

        let version = concat!("v", env!("CARGO_PKG_VERSION"));
        let vw = ui.text_size(version, 12.0, false).x;
        ui.text(version, sw - vw - EDGE, sh - EDGE - 12.0, 12.0, MUTED, false);
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
        action: &mut MenuAction,
    ) {
        let notes = online.board_now();
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
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
            if y + h > sh - 60.0 {
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
                let sub = ui.ellipsize(crate::online::update_label(), text_w, 12.0);
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

fn player_card_w(sw: f32) -> f32 {
    CARD_W.min(sw * 0.4)
}
