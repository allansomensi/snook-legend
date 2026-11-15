//! Local network games: host one (the game runs a small server) or join
//! one found on the network, or by address.

use macroquad::prelude::*;
use snookered_proto::valid_username;
use snookered_server::lan::{Discovery, LanHost};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use crate::config::Config;
use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::ui::{ALERT, BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

pub enum LanAction {
    None,
    Back,
    /// Host a game as `name`.
    Host(String),
    /// Join the server at this address as `name`.
    Join(String, String),
}

pub struct LanScreen {
    discovery: Discovery,
    address: String,
    error: Option<String>,
    first: usize,
    /// The name field was filled in once (emptying it must not refill it).
    named: bool,
}

/// Panel width and inner padding.
const WIDTH: f32 = 560.0;
const PAD: f32 = 28.0;
/// Back button and title, from the panel's top to the first field.
const HEADER_H: f32 = 78.0;
/// A field's label, then its control.
const LABEL_H: f32 = 20.0;
const CONTROL_H: f32 = 36.0;
const HINT: f32 = 12.0;
/// A game in the list, and how many show without scrolling.
const ROW_H: f32 = 48.0;
const ROWS: usize = 3;

impl LanScreen {
    pub fn new() -> LanScreen {
        LanScreen {
            discovery: Discovery::new(),
            address: String::new(),
            error: None,
            first: 0,
            named: false,
        }
    }

    /// Starts hosting; returns the address to join.
    pub fn host(&mut self, name: &str) -> Option<(LanHost, String)> {
        match LanHost::start(name) {
            Ok(host) => {
                let url = format!("ws://127.0.0.1:{}/ws", host.port);
                Some((host, url))
            }
            Err(e) => {
                self.error = Some(tw("Could not host a game: {e}", &[("e", &e)]));
                None
            }
        }
    }

    pub fn draw(&mut self, ui: &mut Ui, config: &mut Config, hosting: Option<u16>) -> LanAction {
        self.discovery.poll();
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
        if !self.named && config.lan_name.is_empty() {
            config.lan_name = if config.username.is_empty() {
                config
                    .player_name
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .take(16)
                    .collect()
            } else {
                config.username.clone()
            };
            if config.lan_name.chars().count() < 3 {
                config.lan_name = format!("Player{}", crate::game::clock_seed() % 900 + 100);
            }
        }
        self.named = true;

        let w = WIDTH.min(sw - 20.0);
        let inner = w - 2.0 * PAD;
        // The note at the bottom: what went wrong, or the firewall tip.
        let (note, note_color) = match &self.error {
            Some(e) => (e.as_str(), ALERT),
            None => (t("The first time, allow Snookered through the firewall when asked."), MUTED),
        };
        let note_lines = ui.wrap(note, inner, HINT);
        let hosting_line = hosting.map(|port| {
            let ip = local_ip().unwrap_or_else(|| "?".into());
            tw("Others can join at {ip}:{port}", &[("ip", &ip), ("port", &port)])
        });
        // Everything but the list, which gets what the window leaves (up to
        // a few rows, at least one).
        let fixed = HEADER_H
            + LABEL_H + CONTROL_H + 6.0 + HINT   // name and its rule
            + 20.0 + 44.0                        // host button
            + if hosting_line.is_some() { 26.0 } else { 0.0 }
            + 28.0 + 15.0 + 12.0                 // games heading
            + 20.0 + LABEL_H + CONTROL_H         // by address
            + 12.0 + note_lines.len() as f32 * (HINT + 3.0)
            - 3.0
            + 26.0;
        let list_h = (ROWS as f32 * ROW_H + 12.0).min(sh - 16.0 - fixed).max(ROW_H + 12.0);
        let h = fixed + list_h;
        let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
        ui.panel(r);
        let mut action = LanAction::None;
        let x = r.x + PAD;

        // Header: back, then the title.
        if ui.icon_button(Rect::new(x, r.y + 20.0, 36.0, 36.0), Icon::Back, t("Back"), true)
            || (is_key_pressed(KeyCode::Escape) && ui.focus.is_none())
        {
            action = LanAction::Back;
        }
        crate::icons::draw(Icon::Network, vec2(x + 62.0, r.y + 38.0), 24.0, GOLD);
        ui.text(t("Local network"), x + 84.0, r.y + 25.0, 24.0, GOLD, true);
        let mut y = r.y + HEADER_H;

        // The name the others will see.
        ui.text(t("Your name"), x, y, 13.0, MUTED, true);
        ui.text_input(50, Rect::new(x, y + LABEL_H, inner, CONTROL_H), &mut config.lan_name, 16);
        let name_ok = valid_username(config.lan_name.trim());
        // The rule is always there, in red when the name breaks it.
        ui.text(
            t("3–16 letters, digits or underscores."),
            x,
            y + LABEL_H + CONTROL_H + 6.0,
            HINT,
            if name_ok { MUTED } else { ALERT },
            false,
        );
        y += LABEL_H + CONTROL_H + 6.0 + HINT + 20.0;

        // Host.
        let host_r = Rect::new(x, y, inner, 44.0);
        match &hosting_line {
            None => {
                let clicked = if name_ok {
                    ui.primary_button(host_r, t("Host a game on this computer"))
                } else {
                    ui.button(host_r, t("Host a game on this computer"), false)
                };
                if clicked {
                    action = LanAction::Host(config.lan_name.trim().to_string());
                }
            }
            Some(line) => {
                ui.button(host_r, t("You are hosting a game"), false);
                ui.text_centered(line, host_r.x + host_r.w / 2.0, y + 52.0, 13.0, CREAM, false);
                y += 26.0;
            }
        }
        y += 44.0 + 28.0;

        // Games found.
        ui.text(t("Games on your network"), x, y, 15.0, GOLD, true);
        let listening = self.discovery.listening();
        if listening {
            spinner(vec2(x + inner - 10.0, y + 8.0), 6.0);
        }
        y += 15.0 + 12.0;
        let list = Rect::new(x, y, inner, list_h);
        rrect(list, 8.0, Color::new(0.0, 0.0, 0.0, 0.22));
        rrect_lines(list, 8.0, 1.0, Color::new(BORDER.r, BORDER.g, BORDER.b, 0.55));
        let games = self.discovery.games.clone();
        if games.is_empty() {
            // Empty: a muted icon and one line, in the middle of the box.
            let c = vec2(list.x + list.w / 2.0, list.y + list.h / 2.0);
            let tall = list.h > 80.0;
            if tall {
                crate::icons::draw(Icon::Network, vec2(c.x, c.y - 16.0), 26.0, MUTED);
            }
            let ty = if tall { c.y + 6.0 } else { c.y - 7.0 };
            if listening {
                // The dots move; the words stay put.
                let text = t("Looking for games");
                let tw = ui.text_size(text, 14.0, false).x;
                let dots_w = ui.text_size("...", 14.0, false).x;
                let tx = c.x - (tw + dots_w) / 2.0;
                ui.text(text, tx, ty, 14.0, MUTED, false);
                let n = ((get_time() * 2.0) as usize) % 4;
                ui.text(&".".repeat(n), tx + tw, ty, 14.0, MUTED, false);
            } else {
                let lines = ui.wrap(
                    t("Can't listen for games here: join by address below."),
                    list.w - 32.0,
                    14.0,
                );
                for (k, line) in lines.iter().enumerate() {
                    ui.text_centered(line, c.x, ty + k as f32 * 18.0, 14.0, MUTED, false);
                }
            }
        }
        let visible = ((list.h - 12.0) / ROW_H).floor().max(1.0) as usize;
        self.first = ui.scroll_list(list, self.first, games.len(), visible);
        let scrolls = games.len() > visible;
        for (k, g) in games.iter().skip(self.first).take(visible).enumerate() {
            let row = Rect::new(
                list.x + 6.0,
                list.y + 6.0 + k as f32 * ROW_H,
                list.w - if scrolls { 20.0 } else { 12.0 },
                ROW_H - 4.0,
            );
            let hover = ui.hovering(row);
            rrect(row, 6.0, Color::new(1.0, 0.9, 0.7, if hover { 0.08 } else { 0.04 }));
            crate::icons::draw(Icon::Table, vec2(row.x + 22.0, row.y + row.h / 2.0), 20.0, CREAM);
            let button = Rect::new(row.x + row.w - 96.0, row.y + (row.h - 32.0) / 2.0, 88.0, 32.0);
            let name = ui.ellipsize(
                &tw("{host}'s game", &[("host", &g.host)]),
                button.x - row.x - 56.0,
                15.0,
            );
            ui.text(&name, row.x + 44.0, row.y + 6.0, 15.0, CREAM, true);
            ui.text(&g.address.to_string(), row.x + 44.0, row.y + 25.0, 12.0, MUTED, false);
            if ui.button(button, t("Join"), name_ok) {
                action = LanAction::Join(g.url(), config.lan_name.trim().to_string());
            }
        }
        y = list.y + list.h + 20.0;

        // By address.
        ui.text(t("Or join by address"), x, y, 13.0, MUTED, true);
        let input = Rect::new(x, y + LABEL_H, inner - 112.0, CONTROL_H);
        let enter = ui.text_input(51, input, &mut self.address, 60);
        if self.address.is_empty() && ui.focus != Some(51) {
            let size = (CONTROL_H * 0.5).clamp(12.0, 18.0);
            let ty = input.y + (CONTROL_H - size) / 2.0;
            ui.text(
                "192.168.0.10",
                input.x + 8.0,
                ty,
                size,
                Color::new(MUTED.r, MUTED.g, MUTED.b, 0.6),
                false,
            );
        }
        let can = name_ok && !self.address.trim().is_empty();
        let join = Rect::new(x + inner - 100.0, y + LABEL_H, 100.0, CONTROL_H);
        if (ui.button(join, t("Join"), can) || enter) && can {
            action =
                LanAction::Join(address_url(&self.address), config.lan_name.trim().to_string());
        }
        y += LABEL_H + CONTROL_H + 12.0;
        for (k, line) in note_lines.iter().enumerate() {
            ui.text(line, x, y + k as f32 * (HINT + 3.0), HINT, note_color, false);
        }
        action
    }
}

/// A small spinner: dots around a circle, the brightest one turning.
fn spinner(c: Vec2, radius: f32) {
    let a = get_time() as f32 * 5.0;
    for k in 0..8 {
        let ang = a + k as f32 * std::f32::consts::TAU / 8.0;
        let p = c + Vec2::from_angle(ang) * radius;
        draw_circle(p.x, p.y, 1.5, Color::new(GOLD.r, GOLD.g, GOLD.b, (k as f32 + 1.0) / 8.0));
    }
}

/// "192.168.0.10" or "192.168.0.10:47879" → a WebSocket URL.
fn address_url(address: &str) -> String {
    let a = address.trim().trim_start_matches("ws://").trim_end_matches("/ws");
    if a.contains(':') {
        format!("ws://{a}/ws")
    } else {
        format!("ws://{a}:{}/ws", snookered_proto::LAN_GAME_PORT)
    }
}

/// This computer's address on the local network.
fn local_ip() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    // No packet is sent: connecting a UDP socket only picks the route.
    sock.connect("10.255.255.255:1").ok()?;
    Some(sock.local_addr().ok()?.ip().to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn addresses_become_urls() {
        assert_eq!(super::address_url("10.0.0.2"), "ws://10.0.0.2:47879/ws");
        assert_eq!(super::address_url(" 10.0.0.2:9000 "), "ws://10.0.0.2:9000/ws");
    }
}
