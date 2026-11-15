//! The shop: cloth, cues, chalk, ball sets, frames, name styles and winning
//! effects (for coins, with achievements or for VIP members), coin packs and
//! the VIP subscription. Items can be tried on the 3D table before buying,
//! and given to friends.
//!
//! The server is the authority: this screen only asks it to buy, use or
//! open a payment page, and shows what it answers.

use macroquad::prelude::*;
use snookered_proto::ClientMsg;
use snookered_proto::shop::{Category, ITEMS, Item, Look, Price, VIP_GIFT};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_circle_lines, draw_line, draw_rectangle};

use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::online::Online;
use crate::ranking::{HEADER_H, PAD, empty_state, screen_header, text_y};
use crate::ui::{ALERT, BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_gradient, rrect_lines, shade};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Items(Category),
    Coins,
    Vip,
}

/// What the 3D preview's bar asks for.
pub enum PreviewAction {
    None,
    Close,
    Buy,
}

pub enum ShopAction {
    None,
    Back,
    /// Try an item on the 3D table.
    Preview(&'static str),
}

/// A purchase waiting for the player to confirm it.
#[derive(Debug, Clone)]
struct Confirm {
    code: &'static str,
    /// Bought as a present for this friend.
    gift_to: Option<String>,
}

pub struct ShopScreen {
    tab: Tab,
    /// The page of items shown.
    page: usize,
    /// An item to bring into view (back from trying it on the table).
    focus: Option<&'static str>,
    /// When the mouse wheel last turned a page (a trackpad sends a stream
    /// of small turns: one page per gesture).
    wheel_at: f64,
    asked: bool,
    confirm: Option<Confirm>,
    /// Choosing the friend who gets this item.
    gift_pick: Option<&'static str>,
    /// The first friend shown in that list.
    gift_first: usize,
    /// The purchase that just went through: (item, given to).
    done: Option<(&'static str, Option<String>)>,
}

impl Default for ShopScreen {
    fn default() -> Self {
        ShopScreen {
            tab: Tab::Items(Category::Cloth),
            page: 0,
            focus: None,
            wheel_at: 0.0,
            asked: false,
            confirm: None,
            gift_pick: None,
            gift_first: 0,
            done: None,
        }
    }
}

/// Width of the list of categories.
const SIDE_W: f32 = 216.0;
/// Space between the item cards.
const GAP: f32 = 12.0;
const CARD_MIN_W: f32 = 176.0;
const CARD_MIN_H: f32 = 200.0;
const CARD_MAX_H: f32 = 264.0;
/// Room under the cards for the page buttons.
const FOOTER: f32 = 48.0;
const GOOD: Color = Color::new(0.45, 0.95, 0.55, 1.0);
/// Text on a disabled button (as the toolkit draws it).
const DIM: Color = Color::new(MUTED.r, MUTED.g, MUTED.b, 0.6);

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], 255)
}

pub fn category_name(c: Category) -> &'static str {
    match c {
        Category::Cloth => t("Cloth"),
        Category::Cue => t("Cues"),
        Category::Chalk => t("Chalk"),
        Category::Balls => t("Balls"),
        Category::Frame => t("Frames"),
        Category::Name => t("Name colour"),
        Category::Effect => t("Winning effects"),
    }
}

/// Items that can be tried on the 3D table.
pub fn has_3d_preview(item: &Item) -> bool {
    matches!(item.category, Category::Cloth | Category::Balls | Category::Cue | Category::Chalk)
}

/// A frame of two colours (and a moving shine) around `r`.
pub fn frame(r: Rect, a: [u8; 3], b: [u8; 3], shine: bool, time: f32) {
    let (ca, cb) = (rgb(a), rgb(b));
    rrect(r, 12.0, cb);
    rrect(Rect::new(r.x + 1.5, r.y + 1.5, r.w - 3.0, r.h * 0.55), 11.0, ca);
    if shine {
        let x = r.x + ((time * 0.35).fract() * 1.6 - 0.3) * r.w;
        let w = r.w * 0.12;
        if x > r.x && x + w < r.x + r.w {
            draw_rectangle(x, r.y + 2.0, w, 2.5, Color::new(1.0, 1.0, 1.0, 0.7));
            draw_rectangle(x, r.y + r.h - 4.5, w, 2.5, Color::new(1.0, 1.0, 1.0, 0.5));
        }
    }
}

fn draw_styled_name_preview(ui: &Ui, label: &str, c: Vec2, style: Option<([u8; 3], [u8; 3])>) {
    let w = ui.text_size(label, 22.0, true).x;
    match style {
        Some((a, b)) => {
            ui.styled_text(label, c.x - w / 2.0, c.y - 11.0, 22.0, rgb(a), rgb(b), true)
        }
        None => {
            let color = crate::ui::tier_color(snookered_proto::Tier::Expert);
            ui.text(label, c.x - w / 2.0, c.y - 11.0, 22.0, color, true)
        }
    }
}

/// A striped ball: ivory, with a band of `color` round its middle.
fn striped_ball(c: Vec2, rad: f32, color: Color, ivory: Color) {
    draw_circle(c.x, c.y, rad, ivory);
    // The band is the part of the disc within ±55% of the radius of its
    // middle, built from thin slices that follow the edge.
    let half = rad * 0.55;
    let steps = 8;
    let at = |k: usize| {
        let y = -half + 2.0 * half * k as f32 / steps as f32;
        (y, (rad * rad - y * y).max(0.0).sqrt())
    };
    for k in 0..steps {
        let ((y0, w0), (y1, w1)) = (at(k), at(k + 1));
        let (a, b) = (vec2(c.x - w0, c.y + y0), vec2(c.x + w0, c.y + y0));
        let (d, e) = (vec2(c.x - w1, c.y + y1), vec2(c.x + w1, c.y + y1));
        crate::paint::draw_triangle(a, b, e, color);
        crate::paint::draw_triangle(a, e, d, color);
    }
}

/// A small picture of an item.
fn preview(ui: &Ui, item: &Item, r: Rect) {
    let c = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
    match item.look {
        Look::Cloth(col) => {
            // A corner of a table: rail, cushion and cloth.
            rrect(r, 8.0, Color::new(0.20, 0.09, 0.05, 1.0));
            let cloth = Rect::new(r.x + 10.0, r.y + 10.0, r.w - 20.0, r.h - 20.0);
            rrect(cloth, 5.0, rgb(col));
            let shade = Color::new(0.0, 0.0, 0.0, 0.18);
            draw_rectangle(cloth.x, cloth.y, cloth.w, 6.0, shade);
            draw_circle(c.x + cloth.w * 0.18, c.y + 4.0, 9.0, Color::new(0.96, 0.95, 0.9, 1.0));
            draw_circle(c.x - cloth.w * 0.2, c.y - 6.0, 9.0, Color::new(0.85, 0.12, 0.1, 1.0));
        }
        Look::Cue { butt, accent, ring, .. } => {
            let (x0, x1) = (r.x + 10.0, r.x + r.w - 10.0);
            let y = c.y;
            let lerp = |k: f32| x0 + (x1 - x0) * k;
            let part = |a: f32, b: f32, w0: f32, w1: f32, col: Color| {
                let (xa, xb) = (lerp(a), lerp(b));
                crate::paint::draw_triangle(
                    vec2(xa, y - w0),
                    vec2(xb, y - w1),
                    vec2(xb, y + w1),
                    col,
                );
                crate::paint::draw_triangle(
                    vec2(xa, y - w0),
                    vec2(xb, y + w1),
                    vec2(xa, y + w0),
                    col,
                );
            };
            part(0.0, 0.08, 6.0, 5.6, rgb(butt));
            part(0.08, 0.26, 5.6, 5.0, rgb(accent));
            part(0.26, 0.27, 5.0, 4.9, rgb(ring));
            part(0.27, 0.45, 4.9, 4.4, rgb(butt));
            part(0.45, 0.46, 4.4, 4.3, rgb(ring));
            part(0.46, 0.97, 4.3, 2.4, Color::new(0.87, 0.75, 0.53, 1.0));
            part(0.97, 0.99, 2.4, 2.3, Color::new(0.95, 0.94, 0.9, 1.0));
            part(0.99, 1.0, 2.3, 2.2, Color::new(0.16, 0.32, 0.6, 1.0));
        }
        Look::Chalk(col) => {
            let s = r.h.min(r.w) * 0.42;
            let top = Color::new(
                col[0] as f32 / 255.0 * 1.2,
                col[1] as f32 / 255.0 * 1.2,
                col[2] as f32 / 255.0 * 1.2,
                1.0,
            );
            draw_rectangle(c.x - s / 2.0, c.y - s / 2.0 + 6.0, s, s * 0.8, rgb(col));
            draw_rectangle(c.x - s / 2.0, c.y - s / 2.0, s, 8.0, top);
            draw_circle(c.x, c.y - s / 2.0 + 4.0, s * 0.22, Color::new(0.0, 0.0, 0.0, 0.25));
        }
        Look::Balls { colors, eight, ivory, .. } => {
            // A rack of fifteen, apex up: solids, stripes and the 8 in the
            // middle of the third row.
            const RACK: [&[usize]; 5] =
                [&[1], &[9, 2], &[10, 8, 3], &[11, 4, 12, 5], &[13, 6, 14, 7, 15]];
            let rad = (r.w / 11.5).min(r.h / 10.0);
            let (dx, dy) = (rad * 2.04, rad * 1.77);
            for (i, row) in RACK.iter().enumerate() {
                for (j, &n) in row.iter().enumerate() {
                    let at = vec2(
                        c.x + (j as f32 - (row.len() - 1) as f32 / 2.0) * dx,
                        c.y + (i as f32 - 2.0) * dy,
                    );
                    match n {
                        8 => draw_circle(at.x, at.y, rad, rgb(eight)),
                        1..=7 => draw_circle(at.x, at.y, rad, rgb(colors[n - 1])),
                        _ => striped_ball(at, rad, rgb(colors[n - 9]), rgb(ivory)),
                    }
                    draw_circle(at.x, at.y, rad * 0.42, rgb(ivory));
                    // A touch of shine.
                    let shine = Color::new(1.0, 1.0, 1.0, 0.35);
                    draw_circle(at.x - rad * 0.4, at.y - rad * 0.45, rad * 0.2, shine);
                }
            }
        }
        Look::Frame { a, b, shine } => {
            // A name card inside the frame.
            rrect(r, 8.0, Color::new(0.0, 0.0, 0.0, 0.25));
            let card = Rect::new(c.x - 70.0, c.y - 30.0, 140.0, 60.0);
            frame(card, a, b, shine, get_time() as f32);
            rrect(
                Rect::new(card.x + 5.0, card.y + 5.0, card.w - 10.0, card.h - 10.0),
                8.0,
                Color::new(0.08, 0.05, 0.03, 1.0),
            );
            crate::icons::draw(Icon::Person, vec2(card.x + 26.0, c.y), 26.0, rgb(a));
        }
        Look::Name(style) => {
            rrect(r, 8.0, Color::new(0.0, 0.0, 0.0, 0.25));
            let label = crate::i18n::t("Your name");
            draw_styled_name_preview(ui, label, c, style);
        }
        Look::Effect { kind, color } => {
            rrect(r, 8.0, Color::new(0.03, 0.08, 0.05, 1.0));
            crate::effects::draw_preview(kind, color, r, get_time() as f32);
        }
    }
}

/// A coin and an amount, with the text's middle on `mid`. Returns the width.
fn coin_amount(ui: &Ui, n: i64, x: f32, mid: f32, size: f32, color: Color) -> f32 {
    crate::icons::draw(Icon::Coin, vec2(x + size * 0.5, mid), size, GOLD);
    let text = n.to_string();
    ui.text(&text, x + size * 1.25, text_y(mid, size), size, color, true);
    size * 1.25 + ui.text_size(&text, size, true).x
}

/// An icon (if any) and a label, centred together in `r` (on a button
/// drawn without a label of its own). The icon goes when there is no room
/// for both: the label matters more.
fn icon_label(ui: &Ui, r: Rect, icon: Option<Icon>, label: &str, size: f32, color: Color) {
    let room = r.w - 14.0;
    let icon = icon.filter(|_| ui.text_size(label, size, true).x + 20.0 <= room);
    let icon_w = if icon.is_some() { 20.0 } else { 0.0 };
    let (label, size) = ui.fitted(label, size, room - icon_w, true);
    let w = icon_w + ui.text_size(&label, size, true).x;
    let x = r.x + (r.w - w) / 2.0;
    let mid = r.y + r.h / 2.0;
    if let Some(icon) = icon {
        let color = if icon == Icon::Coin { GOLD } else { color };
        crate::icons::draw(icon, vec2(x + 7.0, mid), 14.0, color);
    }
    ui.text(&label, x + icon_w, text_y(mid, size), size, color, true);
}

/// How a button looks: the call to action, an ordinary one, one that
/// can't be pressed, or one for what can't be had yet (too few coins).
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Primary,
    Normal,
    Disabled,
    Short,
}

/// A button with an icon by its label (see [`icon_label`]). A `Short` one
/// is quiet like a disabled one, so it reads as out of reach, but it still
/// answers the pointer (it explains what is missing). True when clicked.
fn button(ui: &mut Ui, r: Rect, look: Kind, icon: Option<Icon>, label: &str, size: f32) -> bool {
    let hover = ui.hovering(r);
    let (hit, color) = match look {
        Kind::Primary => (ui.primary_button(r, ""), WHITE),
        Kind::Normal => (ui.button(r, "", true), CREAM),
        Kind::Disabled => (ui.button(r, "", false), DIM),
        Kind::Short => {
            ui.block(r);
            rrect(r, 6.0, Color::new(0.11, 0.08, 0.06, 0.9));
            let edge = if hover { GOLD } else { BORDER };
            let alpha = if hover { 0.7 } else { 0.45 };
            rrect_lines(r, 6.0, 1.0, Color::new(edge.r, edge.g, edge.b, alpha));
            (hover && ui.clicked(r), if hover { MUTED } else { DIM })
        }
    };
    icon_label(ui, r, icon, label, size, color);
    hit
}

/// A chevron pointing left or right, centred on `c`.
fn chevron(c: Vec2, s: f32, left: bool, color: Color) {
    let d = if left { 1.0 } else { -1.0 };
    let tip = vec2(c.x - d * s * 0.22, c.y);
    for dy in [-1.0, 1.0] {
        draw_line(tip.x, tip.y, c.x + d * s * 0.22, c.y + dy * s * 0.42, 2.2, color);
    }
}

/// What the list of categories shows at the right end of a row.
enum SideNote<'a> {
    Count(&'a str),
    Icon(Icon),
}

/// A row of the list of categories: the label on the left, a count or an
/// icon on the right, a gold bar by the one selected. True when clicked.
fn side_tab(ui: &mut Ui, r: Rect, label: &str, note: SideNote, selected: bool) -> bool {
    ui.block(r);
    let hover = ui.hovering(r);
    if selected {
        let bg = Color::new(0.38, 0.25, 0.12, 1.0);
        rrect_gradient(r, 6.0, shade(bg, 0.06), shade(bg, -0.15));
        rrect(Rect::new(r.x + 4.0, r.y + 8.0, 3.0, r.h - 16.0), 1.5, GOLD);
    } else if hover {
        rrect(r, 6.0, Color::new(1.0, 1.0, 1.0, 0.07));
    }
    let mid = r.y + r.h / 2.0;
    let right = r.x + r.w - 12.0;
    let note_w = match note {
        SideNote::Count(n) => {
            let w = ui.text_size(n, 12.0, false).x;
            ui.text(n, right - w, text_y(mid, 12.0), 12.0, MUTED, false);
            w
        }
        SideNote::Icon(icon) => {
            crate::icons::draw(
                icon,
                vec2(right - 8.0, mid),
                16.0,
                if selected { GOLD } else { MUTED },
            );
            16.0
        }
    };
    let x = r.x + 16.0;
    let room = right - note_w - 10.0 - x;
    // Shrinks a little before it would cut the label short.
    let size = ui.fit(15.0, label, room).max(12.0);
    let label = if ui.text_size(label, size, true).x > room {
        ui.fitted(label, size, room, true).0
    } else {
        label.to_string()
    };
    let color = if selected { GOLD } else { CREAM };
    ui.text(&label, x, text_y(mid, size), size, color, true);
    hover && ui.clicked(r) && !selected
}

/// Items the shop lists: the welcome pack's only where it is sold (or
/// for those who have them).
fn listed(item: &Item, online: &Online) -> bool {
    item.price != Price::Pack || crate::channel::real_money_purchases() || online.can_use(item)
}

/// The achievement an item comes with: (title, description).
fn achievement_of(code: &str) -> Option<(&'static str, &'static str)> {
    snookered_proto::ACHIEVEMENTS
        .iter()
        .find(|x| x.code == code)
        .map(|x| (t(x.title), t(x.description)))
}

/// The line under an item's name: what it costs, or how it is had. Too few
/// coins shows in red, with how many are missing.
fn status_line(ui: &Ui, item: &Item, usable: bool, coins: Option<i64>, x: f32, mid: f32, w: f32) {
    let small = |icon: Icon, color: Color, text: &str, text_color: Color| {
        crate::icons::draw(icon, vec2(x + 7.0, mid), 14.0, color);
        let (text, size) = ui.fitted(text, 13.0, w - 20.0, false);
        ui.text(&text, x + 20.0, text_y(mid, size), size, text_color, false);
    };
    match item.price {
        Price::Free => ui.text(t("free"), x, text_y(mid, 13.0), 13.0, MUTED, false),
        Price::Vip => small(Icon::Crown, GOLD, "VIP", GOLD),
        Price::Achievement(a) => {
            let title = achievement_of(a).map(|(title, _)| title).unwrap_or(a);
            let text = if usable { title } else { t("achievement") };
            small(Icon::Medal, if usable { GOLD } else { MUTED }, text, MUTED);
        }
        Price::Coins(_) | Price::Pack if usable => small(Icon::Check, GOOD, t("yours"), MUTED),
        Price::Pack => small(Icon::Gift, MUTED, t("welcome pack"), MUTED),
        Price::Coins(n) => {
            let short = coins.map(|c| n as i64 - c).filter(|s| *s > 0);
            let color = if short.is_some() { ALERT } else { CREAM };
            let used = coin_amount(ui, n as i64, x, mid, 14.0, color);
            if let Some(s) = short {
                let note = tw("{n} short", &[("n", &s)]);
                let (note, size) = ui.fitted(&note, 12.0, w - used - 10.0, false);
                ui.text(&note, x + used + 8.0, text_y(mid, size), size, ALERT, false);
            }
        }
    }
}

impl ShopScreen {
    pub fn open(&mut self, online: &mut Online) {
        self.confirm = None;
        self.gift_pick = None;
        self.done = None;
        online.bought = None;
        online.shop_error = None;
        self.asked = online.signed_in();
        if online.signed_in() {
            online.send(ClientMsg::Shop);
            online.send(ClientMsg::Friends);
        }
    }

    /// Opens on the tab of an item (back from the 3D preview), at its page.
    pub fn show_item(&mut self, code: &str) {
        if let Some(item) = snookered_proto::shop::item(code) {
            self.tab = Tab::Items(item.category);
            self.page = 0;
            self.focus = Some(item.code);
        }
    }

    /// Asks to buy `code` (the confirmation dialog), e.g. from the 3D preview.
    pub fn ask_to_buy(&mut self, code: &'static str) {
        self.confirm = Some(Confirm { code, gift_to: None });
    }

    /// Opens on a tab (`shop-coins`, `shop-vip`, `shop-balls`…; also `--screen`).
    pub fn show_tab(&mut self, name: &str) {
        self.tab = match name {
            "shop-coins" => Tab::Coins,
            "shop-vip" => Tab::Vip,
            "shop-balls" => Tab::Items(Category::Balls),
            "shop-cues" => Tab::Items(Category::Cue),
            "shop-chalk" => Tab::Items(Category::Chalk),
            "shop-frames" => Tab::Items(Category::Frame),
            "shop-names" => Tab::Items(Category::Name),
            "shop-effects" => Tab::Items(Category::Effect),
            _ => Tab::Items(Category::Cloth),
        };
        self.page = 0;
        if name == "shop-confirm" {
            self.confirm = Some(Confirm { code: "cloth_purple", gift_to: None });
        }
        if name == "shop-gift" {
            self.confirm = Some(Confirm { code: "cloth_purple", gift_to: Some("Ana".into()) });
        }
        if name == "shop-pick" {
            self.gift_pick = Some("cloth_purple");
        }
        if name == "shop-done" {
            self.done = Some(("balls_neon", None));
        }
    }

    pub fn draw(&mut self, ui: &mut Ui, online: &mut Online) -> ShopAction {
        // Signed in after the screen opened: ask for the wallet now.
        if online.signed_in() && (online.wallet.is_none() || online.offers.is_none()) && !self.asked
        {
            self.asked = true;
            online.send(ClientMsg::Shop);
            online.send(ClientMsg::Friends);
        }
        // The server confirmed a purchase: show it.
        if let Some((code, gift)) = online.bought.take()
            && let Some(item) = snookered_proto::shop::item(&code)
        {
            self.confirm = None;
            self.done = Some((item.code, gift));
        }
        let modal = self.confirm.is_some() || self.gift_pick.is_some() || self.done.is_some();
        let saved = modal.then(|| ui.suspend_input());
        let mut action = self.page(ui, online, modal);
        if let Some(saved) = saved {
            ui.resume_input(saved);
            draw_rectangle(
                0.0,
                0.0,
                crate::ui::vw(),
                crate::ui::vh(),
                Color::new(0.0, 0.0, 0.0, 0.55),
            );
            if self.done.is_some() {
                self.done_dialog(ui);
            } else if self.gift_pick.is_some() {
                self.gift_dialog(ui, online);
            } else if self.confirm.is_some()
                && let Some(a) = self.confirm_dialog(ui, online)
            {
                action = a;
            }
        }
        action
    }

    fn page(&mut self, ui: &mut Ui, online: &mut Online, modal: bool) -> ShopAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.5));
        let w = (sw - 40.0).min(1060.0);
        let h = (sh - 40.0).min(680.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        let mut action = ShopAction::None;
        // Esc belongs to a dialog while one is open.
        if screen_header(ui, r, Icon::Shop, t("Shop"), None)
            || (!modal && is_key_pressed(KeyCode::Escape) && ui.focus.is_none())
        {
            action = ShopAction::Back;
        }
        let signed_in = online.signed_in();
        let wallet = online.wallet.clone().unwrap_or_default();
        // The balance (and VIP), top right: a way to the coins (and VIP) tab.
        if signed_in {
            let right = r.x + r.w - PAD;
            let chip =
                |ui: &mut Ui, right: f32, icon: Icon, text: &str, color: Color, tip: &str| {
                    let tw = ui.text_size(text, 17.0, true).x;
                    let c = Rect::new(right - (tw + 52.0), r.y + 20.0, tw + 52.0, 36.0);
                    ui.block(c);
                    let hover = ui.hovering(c);
                    rrect(c, 18.0, Color::new(0.0, 0.0, 0.0, if hover { 0.4 } else { 0.25 }));
                    rrect_lines(c, 18.0, 1.0, if hover { GOLD } else { BORDER });
                    crate::icons::draw(icon, vec2(c.x + 25.0, c.y + 18.0), 20.0, GOLD);
                    ui.text(text, c.x + 40.0, text_y(c.y + 18.0, 17.0), 17.0, color, true);
                    if hover {
                        ui.tooltip(tip, c);
                    }
                    (c.x, hover && ui.clicked(c))
                };
            let coins = wallet.coins.to_string();
            let color = if wallet.coins < 0 { ALERT } else { CREAM };
            let (x, hit) = chip(ui, right, Icon::Coin, &coins, color, t("Your coins"));
            if hit {
                self.tab = Tab::Coins;
            }
            if wallet.is_vip() {
                let until =
                    tw("You are VIP until {d}.", &[("d", &crate::profile::date(wallet.vip_until))]);
                let (_, hit) = chip(ui, x - 8.0, Icon::Crown, "VIP", GOLD, &until);
                if hit {
                    self.tab = Tab::Vip;
                }
            }
        }
        let line_y = r.y + HEADER_H - 4.0;
        draw_line(r.x + PAD, line_y, r.x + r.w - PAD, line_y, 1.0, BORDER);
        // The categories, down the left side.
        let top = r.y + HEADER_H + 12.0;
        let side = Rect::new(r.x + PAD - 4.0, top, SIDE_W, r.y + r.h - PAD - top);
        let mut y = side.y;
        for c in Category::ALL {
            let tab = Tab::Items(c);
            let owned = ITEMS
                .iter()
                .filter(|i| i.category == c && (i.price == Price::Free || online.can_use(i)))
                .count();
            let total = ITEMS.iter().filter(|i| i.category == c && listed(i, online)).count();
            let tr = Rect::new(side.x, y, side.w, 36.0);
            let count = if signed_in { format!("{owned}/{total}") } else { total.to_string() };
            if side_tab(ui, tr, category_name(c), SideNote::Count(&count), self.tab == tab) {
                self.tab = tab;
                self.page = 0;
            }
            y += 40.0;
        }
        y += 8.0;
        draw_line(side.x + 12.0, y - 6.0, side.x + side.w - 12.0, y - 6.0, 1.0, BORDER);
        y += 4.0;
        for (tab, label, icon) in
            [(Tab::Coins, t("Coins"), Icon::Coin), (Tab::Vip, "VIP", Icon::Crown)]
        {
            let tr = Rect::new(side.x, y, side.w, 36.0);
            if side_tab(ui, tr, label, SideNote::Icon(icon), self.tab == tab) {
                self.tab = tab;
                self.page = 0;
            }
            y += 40.0;
        }
        let bx = side.x + side.w + 20.0;
        let mut body = Rect::new(bx, top, r.x + r.w - PAD - bx, r.y + r.h - PAD + 4.0 - top);
        if !signed_in {
            let note = t("Sign in to buy and use items. Here is what there is:");
            ui.text(note, body.x, body.y + 2.0, 14.0, MUTED, false);
            body = Rect::new(body.x, body.y + 28.0, body.w, body.h - 28.0);
        }
        match self.tab {
            Tab::Items(c) => {
                if let Some(a) = self.items(ui, online, c, body, modal) {
                    action = a;
                }
            }
            Tab::Coins => self.coins(ui, online, body),
            Tab::Vip => self.vip(ui, online, body),
        }
        action
    }

    fn items(
        &mut self,
        ui: &mut Ui,
        online: &mut Online,
        category: Category,
        area: Rect,
        modal: bool,
    ) -> Option<ShopAction> {
        let items: Vec<&'static Item> =
            ITEMS.iter().filter(|i| i.category == category && listed(i, online)).collect();
        let signed_in = online.signed_in();
        let wallet = online.wallet.clone().unwrap_or_default();
        let coins = (signed_in && online.wallet.is_some()).then_some(wallet.coins);
        let in_use = online.effective(&wallet, category);
        let can_give = signed_in && !online.lan;
        // As many columns as fit; the cards share the height of the area
        // (minus the page buttons), so a page never ends in a gap.
        let cols = ((area.w + GAP) / (CARD_MIN_W + GAP)).floor().max(1.0) as usize;
        let cw = (area.w - (cols - 1) as f32 * GAP) / cols as f32;
        let grid = Rect::new(area.x, area.y, area.w, area.h - FOOTER);
        let rows = ((grid.h + GAP) / (CARD_MIN_H + GAP)).floor().max(1.0) as usize;
        let ch = ((grid.h + GAP) / rows as f32 - GAP).min(CARD_MAX_H);
        let per_page = rows * cols;
        let pages = items.len().div_ceil(per_page).max(1);
        if let Some(code) = self.focus.take()
            && let Some(i) = items.iter().position(|i| i.code == code)
        {
            self.page = i / per_page;
        }
        self.page = self.page.min(pages - 1);
        if pages > 1 {
            // One way through: pages, with the arrows, the wheel or the keys.
            let mut turn = 0i32;
            let from = self.page * per_page + 1;
            let to = ((self.page + 1) * per_page).min(items.len());
            let count =
                tw("Items {a}–{b} of {n}", &[("a", &from), ("b", &to), ("n", &items.len())]);
            let cx = area.x + area.w / 2.0;
            let mid = area.y + area.h - 18.0;
            let half = ui.text_size(&count, 14.0, false).x / 2.0;
            ui.text_centered(&count, cx, text_y(mid, 14.0), 14.0, MUTED, false);
            let prev = Rect::new(cx - half - 16.0 - 40.0, mid - 17.0, 40.0, 34.0);
            let next = Rect::new(cx + half + 16.0, mid - 17.0, 40.0, 34.0);
            for (b, left) in [(prev, true), (next, false)] {
                let enabled = if left { self.page > 0 } else { self.page + 1 < pages };
                if ui.button(b, "", enabled) {
                    turn = if left { -1 } else { 1 };
                }
                let color = if !enabled {
                    DIM
                } else if ui.hovering(b) {
                    GOLD
                } else {
                    CREAM
                };
                chevron(vec2(b.x + b.w / 2.0, b.y + b.h / 2.0), 16.0, left, color);
            }
            if ui.hovering(grid) {
                let (_, wheel) = mouse_wheel();
                let now = get_time();
                if wheel != 0.0 && now - self.wheel_at > 0.3 {
                    self.wheel_at = now;
                    turn = if wheel < 0.0 { 1 } else { -1 };
                }
            }
            if !modal && ui.focus.is_none() {
                if is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::PageDown) {
                    turn = 1;
                } else if is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::PageUp) {
                    turn = -1;
                }
            }
            self.page = (self.page as i32 + turn).clamp(0, pages as i32 - 1) as usize;
        }
        let mut tip = None;
        let mut action = None;
        for (k, item) in items.iter().enumerate().skip(self.page * per_page).take(per_page) {
            let k = k - self.page * per_page;
            let c = Rect::new(
                area.x + (k % cols) as f32 * (cw + GAP),
                area.y + (k / cols) as f32 * (ch + GAP),
                cw,
                ch,
            );
            let usable = signed_in && online.can_use(item) || item.price == Price::Free;
            let equipped = signed_in && in_use == item.code;
            rrect(c, 10.0, Color::new(1.0, 1.0, 1.0, if equipped { 0.07 } else { 0.035 }));
            if equipped {
                rrect_lines(c, 10.0, 2.0, GOLD);
            } else {
                rrect_lines(c, 10.0, 1.0, BORDER);
            }
            let inner = c.w - 24.0;
            let pic = Rect::new(c.x + 12.0, c.y + 12.0, inner, (c.h - 120.0).max(40.0));
            preview(ui, item, pic);
            let name_y = pic.y + pic.h + 10.0;
            let size = ui.fit(15.0, t(item.name), inner).max(12.0);
            let (name, size) = ui.fitted(t(item.name), size, inner, true);
            ui.text(&name, c.x + 12.0, name_y, size, CREAM, true);
            status_line(ui, item, usable, coins, c.x + 12.0, name_y + 29.0, inner);
            // The actions, along the bottom: the main one, then try it on
            // the table and give it to a friend.
            let b = Rect::new(c.x + 12.0, c.y + c.h - 46.0, inner, 34.0);
            let mut right = b.x + b.w;
            if can_give && item.coins().is_some() {
                right -= 34.0;
                let g = Rect::new(right, b.y, 34.0, 34.0);
                if ui.icon_button(g, Icon::Gift, t("Give to a friend"), true) {
                    self.gift_pick = Some(item.code);
                    self.gift_first = 0;
                    online.send(ClientMsg::Friends);
                }
                right -= 4.0;
            }
            if has_3d_preview(item) {
                right -= 34.0;
                let e = Rect::new(right, b.y, 34.0, 34.0);
                if ui.icon_button(e, Icon::Eye, t("Try it on the table"), true) {
                    action = Some(ShopAction::Preview(item.code));
                }
                right -= 4.0;
            }
            let b = Rect::new(b.x, b.y, right - b.x, b.h);
            let size = 14.0;
            if equipped {
                button(ui, b, Kind::Disabled, Some(Icon::Check), t("In use"), size);
            } else if usable {
                let kind = if signed_in { Kind::Normal } else { Kind::Disabled };
                if button(ui, b, kind, None, t("Use"), size) {
                    online.send(ClientMsg::Equip { code: item.code.to_string() });
                }
            } else {
                match item.price {
                    Price::Coins(n) => {
                        // Too few coins: a quiet button (the price above is
                        // in red); it still opens the dialog that says how
                        // many are missing. Signed out, or the coins not
                        // known yet: nothing to press.
                        let kind = match coins {
                            None => Kind::Disabled,
                            Some(c) if c >= n as i64 => Kind::Primary,
                            Some(_) => Kind::Short,
                        };
                        if kind == Kind::Short && ui.hovering(b) {
                            let text = t("Not enough coins: get more in the Coins tab.");
                            tip = Some((text.to_string(), b));
                        }
                        if button(
                            ui,
                            b,
                            kind,
                            (kind == Kind::Short).then_some(Icon::Lock),
                            t("Buy"),
                            size,
                        ) {
                            self.confirm = Some(Confirm { code: item.code, gift_to: None });
                            online.shop_error = None;
                        }
                    }
                    Price::Vip => {
                        if button(ui, b, Kind::Normal, Some(Icon::Crown), t("VIP only"), size) {
                            self.tab = Tab::Vip;
                        }
                    }
                    // Sold with the welcome pack, on the coins tab.
                    Price::Pack => {
                        if button(ui, b, Kind::Normal, None, t("Buy"), size) {
                            self.tab = Tab::Coins;
                        }
                    }
                    Price::Achievement(a) => {
                        let (title, desc) = achievement_of(a).unwrap_or((a, ""));
                        button(ui, b, Kind::Disabled, Some(Icon::Lock), title, size);
                        let status = Rect::new(c.x, name_y + 20.0, c.w, 20.0);
                        if ui.hovering(b) || ui.hovering(status) {
                            tip = Some((
                                tw("{title}: {how}", &[("title", &title), ("how", &desc)]),
                                b,
                            ));
                        }
                    }
                    Price::Free => {}
                }
            }
        }
        if let Some((text, r)) = tip {
            ui.tooltip(&text, r);
        }
        action
    }

    /// "Buy this?": the item, what it costs, the coins before and after.
    fn confirm_dialog(&mut self, ui: &mut Ui, online: &mut Online) -> Option<ShopAction> {
        let confirm = self.confirm.clone()?;
        let item = snookered_proto::shop::item(confirm.code)?;
        let price = item.coins().unwrap_or(0) as i64;
        let coins = online.wallet.as_ref().map(|w| w.coins).unwrap_or(0);
        let after = coins - price;
        let gift = confirm.gift_to.is_some();
        let busy = online.buying.is_some();
        let try_on = !gift && has_3d_preview(item) && !busy;
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let w = 480.0f32.min(sw - 40.0);
        let note = if gift {
            t("Your friend gets it right away.")
        } else {
            t("It goes on right away. Items only change the look, never the game.")
        };
        let note_lines = if after < 0 { Vec::new() } else { ui.wrap(note, w - 64.0, 13.0) };
        // The height follows what is shown: the title, the picture, the
        // name and category, the try-on button, the friend, the two lines
        // of coins, the note (or what is missing), an error, the buttons.
        let mut h = 24.0 + 28.0 + 16.0 + 120.0 + 14.0 + 26.0 + 22.0;
        h += if try_on { 44.0 } else { 0.0 };
        h += if gift { 28.0 } else { 0.0 };
        h += 14.0 + 2.0 * 30.0;
        h += if after < 0 { 28.0 } else { 6.0 + note_lines.len() as f32 * 17.0 };
        h += if online.shop_error.is_some() { 26.0 } else { 0.0 };
        h += 20.0 + 40.0 + 24.0;
        let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(10.0), w, h);
        ui.panel(r);
        let cx = r.x + w / 2.0;
        let title = if gift { t("Give as a gift") } else { t("Confirm purchase") };
        let icon = if gift { Icon::Gift } else { Icon::Shop };
        let tw_ = ui.text_size(title, 22.0, true).x;
        crate::icons::draw(icon, vec2(cx - tw_ / 2.0 - 4.0, r.y + 24.0 + 14.0), 22.0, GOLD);
        ui.text(title, cx - tw_ / 2.0 + 14.0, text_y(r.y + 24.0 + 14.0, 22.0), 22.0, GOLD, true);
        let mut y = r.y + 24.0 + 28.0 + 16.0;
        let pic = Rect::new(r.x + 32.0, y, w - 64.0, 120.0);
        preview(ui, item, pic);
        y += 120.0 + 14.0;
        ui.text_centered(t(item.name), cx, y, 20.0, CREAM, true);
        y += 26.0;
        ui.text_centered(category_name(item.category), cx, y, 13.0, MUTED, false);
        y += 22.0;
        let mut action = None;
        if try_on {
            let label = t("Try it on the table");
            let bw = ui.text_size(label, 13.0, true).x + 52.0;
            let b = Rect::new(cx - bw / 2.0, y + 4.0, bw, 30.0);
            if ui.button(b, "", true) {
                self.confirm = None;
                action = Some(ShopAction::Preview(item.code));
            }
            let color = if ui.hovering(b) { GOLD } else { CREAM };
            crate::icons::draw(Icon::Eye, vec2(b.x + 20.0, b.y + 15.0), 16.0, color);
            ui.text(label, b.x + 34.0, text_y(b.y + 15.0, 13.0), 13.0, color, true);
            y += 44.0;
        }
        if let Some(friend) = &confirm.gift_to {
            let line = tw("For {name}", &[("name", friend)]);
            ui.text_centered(&line, cx, y + 4.0, 15.0, CREAM, true);
            y += 28.0;
        }
        draw_line(r.x + 32.0, y + 6.0, r.x + w - 32.0, y + 6.0, 1.0, BORDER);
        y += 14.0;
        // The price, and the coins before and after: label left, amount right.
        let (lx, rx) = (r.x + 48.0, r.x + w - 48.0);
        let amount = |ui: &Ui, text: &str, mid: f32, color: Color| {
            let tw = ui.text_size(text, 17.0, true).x;
            crate::icons::draw(Icon::Coin, vec2(rx - tw - 13.0, mid), 17.0, GOLD);
            ui.text(text, rx - tw, text_y(mid, 17.0), 17.0, color, true);
        };
        let mid = y + 15.0;
        ui.text(t("Price"), lx, text_y(mid, 15.0), 15.0, MUTED, false);
        amount(ui, &price.to_string(), mid, CREAM);
        let mid = y + 45.0;
        ui.text(t("Your coins"), lx, text_y(mid, 15.0), 15.0, MUTED, false);
        if after < 0 {
            amount(ui, &coins.to_string(), mid, ALERT);
        } else {
            amount(ui, &format!("{coins}  →  {after}"), mid, CREAM);
        }
        y += 60.0;
        if after < 0 {
            let short = tw("You need {n} more coins.", &[("n", &-after)]);
            ui.text_centered(&short, cx, y + 6.0, 14.0, ALERT, true);
            y += 28.0;
        } else {
            y += 6.0;
            for line in &note_lines {
                ui.text_centered(line, cx, y, 13.0, MUTED, false);
                y += 17.0;
            }
        }
        if let Some(e) = &online.shop_error {
            ui.text_centered(e, cx, y + 6.0, 14.0, ALERT, true);
        }
        // Cancel on the left, the way on at the right.
        let by = r.y + h - 24.0 - 40.0;
        let half = (w - 48.0 - 12.0) / 2.0;
        let cancel = ui.button(Rect::new(r.x + 24.0, by, half, 40.0), t("Cancel"), !busy);
        let ok_rect = Rect::new(r.x + 24.0 + half + 12.0, by, half, 40.0);
        if busy {
            ui.button(ok_rect, t("Buying…"), false);
        } else if after < 0 {
            if ui.primary_button(ok_rect, t("Get coins")) {
                self.confirm = None;
                self.tab = Tab::Coins;
            }
        } else {
            let label = if gift { t("Give it") } else { t("Buy now") };
            if ui.primary_button(ok_rect, label) || is_key_pressed(KeyCode::Enter) {
                online.shop_error = None;
                online.buying = Some(item.code.to_string());
                match &confirm.gift_to {
                    Some(to) => online
                        .send(ClientMsg::GiftItem { to: to.clone(), code: item.code.to_string() }),
                    None => online.send(ClientMsg::BuyItem { code: item.code.to_string() }),
                }
            }
        }
        if (cancel || is_key_pressed(KeyCode::Escape)) && !busy {
            self.confirm = None;
            online.shop_error = None;
        }
        action
    }

    /// The bar under the 3D preview: the item, its price, buy or go back.
    pub fn preview_bar(
        &mut self,
        ui: &mut Ui,
        online: &mut Online,
        code: &'static str,
    ) -> PreviewAction {
        let Some(item) = snookered_proto::shop::item(code) else { return PreviewAction::Close };
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let w = 640.0f32.min(sw - 30.0);
        let r = Rect::new((sw - w) / 2.0, sh - 108.0, w, 88.0);
        ui.panel(r);
        let usable = online.signed_in() && online.can_use(item) || item.price == Price::Free;
        let mut action = PreviewAction::None;
        let b = Rect::new(r.x + r.w - 24.0 - 150.0, r.y + 24.0, 150.0, 40.0);
        let mut text_right = b.x - 16.0;
        match item.price {
            Price::Coins(n) if !usable && online.signed_in() => {
                let coins = online.wallet.as_ref().map(|w| w.coins).unwrap_or(0);
                let label = tw("Buy · {n}", &[("n", &n)]);
                // Too few coins: the dialog says how many are missing.
                let (kind, icon) = if coins >= n as i64 {
                    (Kind::Primary, Icon::Coin)
                } else {
                    (Kind::Short, Icon::Lock)
                };
                if button(ui, b, kind, Some(icon), &label, 15.0) {
                    action = PreviewAction::Buy;
                }
            }
            _ if usable && online.signed_in() => {
                let wallet = online.wallet.clone().unwrap_or_default();
                let in_use = online.effective(&wallet, item.category) == item.code;
                if in_use {
                    button(ui, b, Kind::Disabled, Some(Icon::Check), t("In use"), 15.0);
                } else if ui.primary_button(b, t("Use")) {
                    online.send(ClientMsg::Equip { code: item.code.to_string() });
                }
            }
            _ => text_right = r.x + r.w - 24.0,
        }
        let label = t("Back to the shop");
        let back_w = ui.text_size(label, 15.0, true).x + 52.0;
        let back = Rect::new(r.x + 24.0, r.y + 24.0, back_w, 40.0);
        if button(ui, back, Kind::Normal, Some(Icon::Back), label, 15.0)
            || is_key_pressed(KeyCode::Escape)
        {
            action = PreviewAction::Close;
        }
        // The item, between the two buttons.
        let x = back.x + back.w + 20.0;
        let room = text_right - x;
        let (name, size) = ui.fitted(t(item.name), 20.0, room, true);
        ui.text(&name, x, r.y + 18.0, size, GOLD, true);
        let sub = format!("{} · {}", category_name(item.category), t("turning around the table"));
        let (sub, size) = ui.fitted(&sub, 13.0, room, false);
        ui.text(&sub, x, r.y + 50.0, size, MUTED, false);
        action
    }

    /// The purchase went through.
    fn done_dialog(&mut self, ui: &mut Ui) {
        let Some((code, gift)) = self.done.clone() else { return };
        let Some(item) = snookered_proto::shop::item(code) else {
            self.done = None;
            return;
        };
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let w = 440.0f32.min(sw - 40.0);
        let text = match &gift {
            Some(friend) => {
                tw("{name} received {item}.", &[("name", friend), ("item", &t(item.name))])
            }
            None => tw("{item} is yours and already in use.", &[("item", &t(item.name))]),
        };
        let lines = ui.wrap(&text, w - 56.0, 15.0);
        // The tick and the title, the picture, the text, the button.
        let pic_h = 130.0;
        let head = 24.0 + 52.0 + 16.0 + 26.0 + 16.0;
        let h = head + pic_h + 16.0 + lines.len() as f32 * 20.0 + 24.0 + 40.0 + 24.0;
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        let cx = r.x + w / 2.0;
        let mut y = r.y + 24.0;
        draw_circle(cx, y + 26.0, 26.0, GOLD);
        crate::icons::draw(Icon::Check, vec2(cx, y + 26.0), 28.0, Color::new(0.1, 0.07, 0.04, 1.0));
        y += 52.0 + 16.0;
        let title = if gift.is_some() { t("Gift sent!") } else { t("Purchase complete!") };
        ui.text_centered(title, cx, y, 22.0, GOLD, true);
        y += 26.0 + 16.0;
        let pic = Rect::new(r.x + 48.0, y, w - 96.0, pic_h);
        preview(ui, item, pic);
        y += pic_h + 16.0;
        for line in &lines {
            ui.text_centered(line, cx, y, 15.0, CREAM, false);
            y += 20.0;
        }
        let b = Rect::new(cx - 100.0, r.y + h - 24.0 - 40.0, 200.0, 40.0);
        if ui.primary_button(b, t("Great!"))
            || is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::Enter)
        {
            self.done = None;
        }
    }

    /// Choosing which friend gets a present.
    fn gift_dialog(&mut self, ui: &mut Ui, online: &mut Online) {
        let Some(code) = self.gift_pick else { return };
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let friends = online.friends.friends.clone();
        let row_h = 44.0;
        let w = 440.0f32.min(sw - 40.0);
        // Room for the friends (or a few rows' worth for the empty state).
        let rows = if friends.is_empty() { 4 } else { friends.len() };
        let list_h = (rows as f32 * row_h).min(sh - 40.0 - 200.0);
        let h = 24.0 + 52.0 + 16.0 + list_h + 20.0 + 40.0 + 24.0;
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        crate::icons::draw(Icon::Gift, vec2(r.x + 24.0 + 12.0, r.y + 24.0 + 12.0), 24.0, GOLD);
        ui.text(t("Give to a friend"), r.x + 24.0 + 34.0, r.y + 22.0, 22.0, GOLD, true);
        let vip = code == VIP_GIFT;
        let what = if vip {
            format!("{} VIP · {}", snookered_proto::GAME_NAME, t("one month"))
        } else {
            snookered_proto::shop::item(code).map(|i| t(i.name).to_string()).unwrap_or_default()
        };
        ui.text(&what, r.x + 24.0 + 34.0, r.y + 52.0, 14.0, MUTED, false);
        let line_y = r.y + 24.0 + 52.0 + 8.0;
        draw_line(r.x + 24.0, line_y, r.x + w - 24.0, line_y, 1.0, BORDER);
        let list = Rect::new(r.x + 16.0, line_y + 8.0, w - 32.0, list_h);
        if friends.is_empty() {
            empty_state(ui, list, Icon::People, t("Add friends to give them items."));
        }
        let visible = (list.h / row_h).floor().max(1.0) as usize;
        self.gift_first = ui.scroll_list(list, self.gift_first, friends.len(), visible);
        let scrolls = friends.len() > visible;
        for (k, f) in friends.iter().enumerate().skip(self.gift_first).take(visible) {
            let k = k - self.gift_first;
            let row_w = list.w - if scrolls { 12.0 } else { 0.0 };
            let row = Rect::new(list.x, list.y + k as f32 * row_h, row_w, row_h - 4.0);
            // A friend who is VIP already can't be given VIP.
            let taken = vip && f.player.vip;
            let hover = !taken && ui.hovering(row);
            rrect(row, 6.0, Color::new(1.0, 1.0, 1.0, if hover { 0.09 } else { 0.035 }));
            if hover {
                rrect_lines(row, 6.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.5));
            }
            let mid = row.y + row.h / 2.0;
            let note = if taken { t("already VIP") } else { "" };
            let nw = if taken { ui.text_size(note, 13.0, false).x + 16.0 } else { 0.0 };
            ui.player_name(&f.player, row.x + 12.0, text_y(mid, 15.0), 15.0, row.w - 24.0 - nw);
            if taken {
                ui.text(note, row.x + row.w - nw + 4.0, text_y(mid, 13.0), 13.0, MUTED, false);
            } else if hover {
                crate::icons::draw(Icon::Gift, vec2(row.x + row.w - 22.0, mid), 16.0, GOLD);
            }
            ui.block(row);
            if hover && ui.clicked(row) {
                self.gift_pick = None;
                online.shop_error = None;
                if vip {
                    // The payment page shows the price and asks for confirmation.
                    online.send(ClientMsg::GiftVip { to: f.player.username.clone() });
                } else {
                    self.confirm = Some(Confirm { code, gift_to: Some(f.player.username.clone()) });
                }
            }
        }
        let b = Rect::new(r.x + 24.0, r.y + h - 24.0 - 40.0, 140.0, 40.0);
        if ui.button(b, t("Cancel"), true) || is_key_pressed(KeyCode::Escape) {
            self.gift_pick = None;
        }
    }

    fn coins(&mut self, ui: &mut Ui, online: &mut Online, area: Rect) {
        let offers = online.offers.clone().unwrap_or_default();
        let mut y = area.y;
        // What there is: the balance, large.
        if online.signed_in() {
            let coins = online.wallet.as_ref().map(|w| w.coins).unwrap_or(0);
            let card = Rect::new(area.x, y, area.w, 76.0);
            rrect(card, 10.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.07));
            rrect_lines(card, 10.0, 1.0, BORDER);
            let mid = card.y + card.h / 2.0;
            for i in 0..3 {
                let at = vec2(card.x + 30.0 + i as f32 * 8.0, mid + 4.0 - i as f32 * 5.0);
                crate::icons::draw(Icon::Coin, at, 26.0, GOLD);
            }
            ui.text(t("Your coins"), card.x + 80.0, card.y + 14.0, 13.0, MUTED, true);
            let color = if coins < 0 { ALERT } else { CREAM };
            ui.text(&coins.to_string(), card.x + 80.0, card.y + 32.0, 28.0, color, true);
            y += card.h + 24.0;
        }
        // Every version earns coins the same way; buying them is only extra.
        let store = !crate::channel::real_money_purchases();
        let how = if store {
            t("In this version of the game, coins are earned by playing:")
        } else {
            t("Coins are earned by playing:")
        };
        for line in ui.wrap(how, area.w, 15.0) {
            ui.text(&line, area.x, y, 15.0, CREAM, false);
            y += 21.0;
        }
        y += 4.0;
        let ways = t("rated games, and a bonus every day you play.");
        for (i, line) in ui.wrap(ways, area.w - 26.0, 15.0).iter().enumerate() {
            if i == 0 {
                crate::icons::draw(Icon::Star, vec2(area.x + 8.0, y + 8.0), 12.0, GOLD);
            }
            ui.text(line, area.x + 24.0, y, 15.0, CREAM, false);
            y += 21.0;
        }
        if store {
            return;
        }
        let below = y + 14.0;
        if !offers.payments || offers.packs.is_empty() {
            ui.text(
                t("Coins can't be bought on this server yet."),
                area.x,
                below,
                14.0,
                MUTED,
                false,
            );
            return;
        }
        ui.text(t("If you like, you can also buy them:"), area.x, below, 15.0, MUTED, false);
        let mut top = below + 32.0;
        // The welcome pack, once per account.
        let owned = online.wallet.as_ref().map(|w| w.owned.clone()).unwrap_or_default();
        if !offers.welcome_price.is_empty() && !snookered_proto::shop::has_welcome_pack(&owned) {
            let card = Rect::new(area.x, top, area.w, 74.0);
            rrect(card, 10.0, Color::new(0.91, 0.75, 0.38, 0.10));
            rrect_lines(card, 10.0, 1.6, GOLD);
            crate::icons::draw(Icon::Gift, vec2(card.x + 30.0, card.y + 37.0), 30.0, GOLD);
            ui.text(t("Welcome pack"), card.x + 58.0, card.y + 14.0, 17.0, GOLD, true);
            let names: Vec<String> = snookered_proto::shop::WELCOME_ITEMS
                .iter()
                .filter_map(|c| snookered_proto::shop::item(c))
                .map(|i| t(i.name).to_string())
                .collect();
            let what = tw(
                "Cue “{cue}”, cloth “{cloth}” and {n} coins · only once, sold nowhere else",
                &[
                    ("cue", &names.first().cloned().unwrap_or_default()),
                    ("cloth", &names.get(1).cloned().unwrap_or_default()),
                    ("n", &snookered_proto::shop::WELCOME_COINS),
                ],
            );
            let b = Rect::new(card.x + card.w - 150.0, card.y + 18.0, 136.0, 38.0);
            let what = ui.ellipsize(&what, b.x - 16.0 - (card.x + 58.0), 13.0);
            ui.text(&what, card.x + 58.0, card.y + 42.0, 13.0, CREAM, false);
            if online.signed_in() {
                if ui.primary_button(b, &offers.welcome_price) {
                    online.send(ClientMsg::BuyWelcomePack);
                }
            } else {
                ui.button(b, &offers.welcome_price, false);
            }
            top += card.h + GAP;
        }
        let area = Rect::new(area.x, top, area.w, area.h - (top - area.y));
        let signed_in = online.signed_in();
        let n = offers.packs.len();
        let cols = n.min(((area.w + GAP) / (180.0 + GAP)).floor().max(1.0) as usize).max(1);
        let cw = (area.w - (cols - 1) as f32 * GAP) / cols as f32;
        let card_h = 168.0;
        for (k, p) in offers.packs.iter().enumerate() {
            let c = Rect::new(
                area.x + (k % cols) as f32 * (cw + GAP),
                area.y + (k / cols) as f32 * (card_h + GAP),
                cw,
                card_h,
            );
            rrect(c, 10.0, Color::new(1.0, 1.0, 1.0, 0.035));
            rrect_lines(c, 10.0, 1.0, BORDER);
            // A small pile of coins, bigger for bigger packs.
            let pile = (k + 2).min(5);
            for i in 0..pile {
                let x = c.x + c.w / 2.0 + (i as f32 - (pile - 1) as f32 / 2.0) * 10.0;
                crate::icons::draw(Icon::Coin, vec2(x, c.y + 32.0 - i as f32 * 3.0), 26.0, GOLD);
            }
            let total = (p.coins + p.bonus).to_string();
            ui.text_centered(&total, c.x + c.w / 2.0, c.y + 56.0, 24.0, CREAM, true);
            ui.text_centered(t("coins"), c.x + c.w / 2.0, c.y + 84.0, 13.0, MUTED, false);
            if p.bonus > 0 {
                ui.text_centered(
                    &tw("+{n} bonus", &[("n", &p.bonus)]),
                    c.x + c.w / 2.0,
                    c.y + 102.0,
                    13.0,
                    GOLD,
                    true,
                );
            }
            let b = Rect::new(c.x + 12.0, c.y + c.h - 46.0, c.w - 24.0, 34.0);
            if signed_in {
                if ui.primary_button(b, &p.price) {
                    online.send(ClientMsg::BuyCoins { pack: p.code.clone() });
                }
            } else {
                ui.button(b, &p.price, false);
            }
        }
        let rows = n.div_ceil(cols) as f32;
        let y = area.y + rows * (card_h + GAP) + 4.0;
        for (i, line) in [
            t("Payment opens in your browser (card, Pix…), on Stripe's secure page."),
            t("Coins arrive in a moment; they are only used for items in the game."),
        ]
        .iter()
        .enumerate()
        {
            ui.text(line, area.x, y + i as f32 * 20.0, 13.0, MUTED, false);
        }
    }

    fn vip(&mut self, ui: &mut Ui, online: &mut Online, area: Rect) {
        let offers = online.offers.clone().unwrap_or_default();
        let wallet = online.wallet.clone().unwrap_or_default();
        let exclusives: Vec<String> =
            ITEMS.iter().filter(|i| i.price == Price::Vip).map(|i| t(i.name).to_string()).collect();
        let perks = [
            tw("{n} coins every month", &[("n", &offers.vip_coins)]),
            t("A crown by your name, everywhere (lounge, profile, ranking)").to_string(),
            t("Name colours for the lounge and exclusive frames, cloth, cues, balls and effects")
                .to_string(),
            tw("Exclusive items: {list}", &[("list", &exclusives.join(", "))]),
            tw(
                "{n} reactions of your own to send during games",
                &[("n", &snookered_proto::REACTIONS.iter().filter(|r| r.is_vip()).count())],
            ),
            t("Every replay kept (free players keep their last 10 games)").to_string(),
            t("16-player tournaments, and first place on the waiting list of a full one")
                .to_string(),
            t("Nothing that wins games: VIP only changes the look").to_string(),
            t("Cancel whenever you want: VIP lasts until the end of the paid month").to_string(),
        ];
        // The perks on a card edged in gold, under the crown and the name.
        let pad = 20.0;
        let text_w = area.w - 2.0 * pad - 24.0;
        let wrapped: Vec<Vec<String>> = perks.iter().map(|p| ui.wrap(p, text_w, 15.0)).collect();
        let list_h: f32 = wrapped.iter().map(|l| l.len() as f32 * 20.0 + 6.0).sum();
        let card = Rect::new(area.x, area.y, area.w, pad + 30.0 + 14.0 + list_h + pad - 6.0);
        rrect_gradient(
            card,
            10.0,
            Color::new(GOLD.r, GOLD.g, GOLD.b, 0.10),
            Color::new(GOLD.r, GOLD.g, GOLD.b, 0.03),
        );
        rrect_lines(card, 10.0, 1.2, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.55));
        let (x, mid) = (card.x + pad, card.y + pad + 15.0);
        crate::icons::draw(Icon::Crown, vec2(x + 14.0, mid), 28.0, GOLD);
        let title = format!("{} VIP", snookered_proto::GAME_NAME);
        ui.text(&title, x + 38.0, text_y(mid, 22.0), 22.0, GOLD, true);
        let mut y = card.y + pad + 30.0 + 14.0;
        for lines in &wrapped {
            // The star sits on the middle of the first line.
            crate::icons::draw(Icon::Star, vec2(x + 8.0, y + 8.0), 12.0, GOLD);
            for (i, line) in lines.iter().enumerate() {
                ui.text(line, x + 24.0, y + i as f32 * 20.0, 15.0, CREAM, false);
            }
            y += lines.len() as f32 * 20.0 + 6.0;
        }
        let mut y = card.y + card.h + 20.0;
        let bw = 300.0f32.min(area.w);
        if wallet.is_vip() {
            let until = crate::profile::date(wallet.vip_until);
            ui.text(&tw("You are VIP until {d}.", &[("d", &until)]), area.x, y, 15.0, GOLD, true);
            y += 28.0;
            // The subscription is managed on the payment provider's pages.
            if crate::channel::real_money_purchases() {
                let b = Rect::new(area.x, y, bw, 40.0);
                if ui.button(b, t("Manage subscription"), offers.payments) {
                    online.send(ClientMsg::ManageVip);
                }
                y += 52.0;
            }
        } else if !crate::channel::real_money_purchases() {
            ui.text(
                t("VIP isn't sold in this version of the game."),
                area.x,
                y,
                14.0,
                MUTED,
                false,
            );
            y += 28.0;
        } else if offers.payments && !offers.vip_price.is_empty() {
            let b = Rect::new(area.x, y, bw, 42.0);
            if ui.primary_button(b, &tw("Become VIP · {p}", &[("p", &offers.vip_price)])) {
                online.send(ClientMsg::SubscribeVip);
            }
            y += 54.0;
        } else {
            ui.text(t("VIP can't be bought on this server yet."), area.x, y, 14.0, MUTED, false);
            y += 28.0;
        }
        // A month for a friend: one payment, no subscription.
        if crate::channel::real_money_purchases()
            && offers.payments
            && !offers.vip_gift_price.is_empty()
        {
            let g = Rect::new(area.x, y, bw, 40.0);
            let label = tw("Give a month to a friend · {p}", &[("p", &offers.vip_gift_price)]);
            if ui.button(g, &label, online.signed_in()) {
                self.gift_pick = Some(VIP_GIFT);
                self.gift_first = 0;
                online.send(ClientMsg::Friends);
            }
        }
    }
}
