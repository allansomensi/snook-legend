//! The shop: cloth, cues, chalk, ball sets, tables, halls, frames, name
//! styles and winning effects (for coins, with achievements or for VIP
//! members), coin packs and the VIP subscription. Items can be tried on the
//! 3D table before buying, and given to friends. Cues show their level
//! (online wins with them).
//!
//! The server is the authority: this screen only asks it to buy, use or
//! open a payment page, and shows what it answers.

use macroquad::prelude::*;
use snook_legend_proto::ClientMsg;
use snook_legend_proto::shop::{Category, ITEMS, Item, Look, Price, VIP_GIFT};

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
    /// Turn the dark stage off, so the hall shows around the table.
    ShowHall,
    /// Signed out, a buy or use button was pressed: to the sign-in form.
    SignIn,
}

/// A purchase waiting for the player to confirm it.
#[derive(Debug, Clone)]
struct Confirm {
    code: &'static str,
    /// Bought as a present for this friend.
    gift_to: Option<String>,
}

pub struct ShopScreen {
    /// Signed out, "Sign in" (or an item's button) was pressed.
    sign_in: bool,
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
    /// The dark stage is on (the hall doesn't show around the table).
    pub dark_stage: bool,
    /// A few words to go with a gift.
    gift_message: String,
}

impl Default for ShopScreen {
    fn default() -> Self {
        ShopScreen {
            sign_in: false,
            tab: Tab::Items(Category::Cloth),
            page: 0,
            focus: None,
            wheel_at: 0.0,
            asked: false,
            confirm: None,
            gift_pick: None,
            gift_first: 0,
            done: None,
            dark_stage: false,
            gift_message: String::new(),
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
        Category::Table => t("Tables"),
        Category::Hall => t("Halls"),
        Category::Reaction => t("Reactions"),
    }
}

/// Items that can be tried on the 3D table.
pub fn has_3d_preview(item: &Item) -> bool {
    matches!(
        item.category,
        Category::Cloth
            | Category::Balls
            | Category::Cue
            | Category::Chalk
            | Category::Table
            | Category::Hall
    )
}

/// "Level 3 · 57 wins", the progress to the next level (0..1) and what the
/// next level needs.
pub fn cue_level_text(wins: u32) -> (String, f32, String) {
    use snook_legend_proto::shop::{cue_level, cue_level_progress, next_cue_level_at};
    let level = cue_level(wins);
    let line = if wins == 1 {
        tw("Level {n} · 1 win", &[("n", &level)])
    } else {
        tw("Level {n} · {w} wins", &[("n", &level), ("w", &wins)])
    };
    let tip = match next_cue_level_at(wins) {
        Some(at) if at - wins == 1 => tw(
            "1 more online win with this cue for level {n}. Levels only add marks to the cue.",
            &[("n", &(level + 1))],
        ),
        Some(at) => tw(
            "{left} more online wins with this cue for level {n}. Levels only add marks to the cue.",
            &[("left", &(at - wins)), ("n", &(level + 1))],
        ),
        None => t("The top level: this cue has every mark.").to_string(),
    };
    (line, cue_level_progress(wins), tip)
}

/// A cue's picture with the marks of its level (the profile shows it).
pub fn cue_picture(ui: &Ui, code: &str, r: Rect, level: u8) {
    if let Some(item) = snook_legend_proto::shop::item(code) {
        preview(ui, item, r);
        cue_marks(r, level);
    }
}

/// How a cue's picture fits `r`: the part of the cue shown (from the butt;
/// a big picture shows the butt half, where the patterns are) and how much
/// thicker it is drawn.
fn cue_zoom(r: Rect) -> (f32, f32) {
    if r.h >= 60.0 { (0.56, (r.h / 38.0).clamp(1.0, 3.2)) } else { (1.0, 1.0) }
}

/// Where a point `k` along the cue (0: the butt, 1: the tip) is drawn.
fn cue_x(r: Rect, k: f32) -> f32 {
    let (x0, x1) = (r.x + 10.0, r.x + r.w - 10.0);
    x0 + (x1 - x0) * k / cue_zoom(r).0
}

/// The marks of a cue's level on its picture (see [`preview`]): bands by
/// the joint, then inlays, then gold that glows.
fn cue_marks(r: Rect, level: u8) {
    if level == 0 {
        return;
    }
    let y = r.y + r.h / 2.0;
    let at = |k: f32| cue_x(r, k);
    let z = cue_zoom(r).1;
    let silver = Color::new(0.86, 0.88, 0.92, 1.0);
    let gold = Color::new(0.95, 0.78, 0.38, 1.0);
    let metal = if level >= 4 { gold } else { silver };
    if level >= 6 {
        draw_circle(at(0.43), y, 9.0 * z, Color::new(1.0, 0.8, 0.4, 0.22));
        draw_circle(at(0.43), y, 6.0 * z, Color::new(1.0, 0.85, 0.5, 0.25));
    }
    let bands = match level {
        1 => 1,
        2..=4 => 2,
        _ => 3,
    };
    for k in 0..bands {
        let x = at(0.44 - k as f32 * 0.018);
        draw_line(x, y - 4.6 * z, x, y + 4.6 * z, 1.6 * z, metal);
    }
    if level >= 3 {
        let x = at(0.44 - bands as f32 * 0.018 - 0.02);
        let d = 2.2 * z;
        crate::paint::draw_triangle(vec2(x - d, y), vec2(x, y - d), vec2(x + d, y), WHITE);
        crate::paint::draw_triangle(vec2(x - d, y), vec2(x, y + d), vec2(x + d, y), WHITE);
    }
    if level >= 4 {
        draw_line(at(0.1), y - 5.6 * z, at(0.1), y + 5.6 * z, 1.6 * z, gold);
    }
    if level >= 5 {
        draw_line(at(0.14), y - 5.4 * z, at(0.14), y + 5.4 * z, 1.6 * z, gold);
    }
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
            let color = crate::ui::tier_color(snook_legend_proto::Tier::Expert);
            ui.text(label, c.x - w / 2.0, c.y - 11.0, 22.0, color, true)
        }
    }
}

/// A small picture of an item.
pub fn preview(ui: &Ui, item: &Item, r: Rect) {
    let c = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
    match item.look {
        // A corner of a table in this cloth, under the lamp.
        Look::Cloth(col) => crate::swatch::cloth(r, col),
        Look::Cue { .. } => {
            // The cue, drawn once into a picture (crisp patterns, a
            // varnished cylinder's light): the whole cue, or its butt half
            // in a big picture.
            let (span, z) = cue_zoom(r);
            if let Some(tex) = crate::cues::picture(item.code, span) {
                let h = 12.6 * z;
                draw_texture_ex(
                    &tex,
                    cue_x(r, 0.0),
                    c.y - h / 2.0,
                    Color::new(1.0, 1.0, 1.0, crate::paint::alpha()),
                    DrawTextureParams {
                        dest_size: Some(vec2(cue_x(r, span) - cue_x(r, 0.0), h)),
                        ..Default::default()
                    },
                );
            }
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
            // A rack of fifteen on the classic cloth, apex up: solids,
            // stripes and the 8 in the middle of the third row.
            let felt = crate::swatch::table_corner(r, [24, 96, 48]);
            let c = vec2(felt.x + felt.w / 2.0, felt.y + felt.h / 2.0);
            const RACK: [&[usize]; 5] =
                [&[1], &[9, 2], &[10, 8, 3], &[11, 4, 12, 5], &[13, 6, 14, 7, 15]];
            let rad = (felt.w / 11.5).min(felt.h / 10.4);
            let (dx, dy) = (rad * 2.04, rad * 1.77);
            let ivory = rgb(ivory);
            for (i, row) in RACK.iter().enumerate() {
                for (j, &n) in row.iter().enumerate() {
                    let at = vec2(
                        c.x + (j as f32 - (row.len() - 1) as f32 / 2.0) * dx,
                        c.y + (i as f32 - 2.0) * dy,
                    );
                    let (base, stripe) = match n {
                        8 => (rgb(eight), None),
                        1..=7 => (rgb(colors[n - 1]), None),
                        _ => (ivory, Some(rgb(colors[n - 9]))),
                    };
                    crate::swatch::ball(at, rad, base, stripe, ivory, true);
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
        Look::Table(style) => crate::theme_art::table(r, style),
        Look::Hall(style) => crate::theme_art::hall(r, style),
        Look::Reaction(code) => {
            // The picture, large, on a soft spotlight.
            rrect(r, 8.0, Color::new(0.0, 0.0, 0.0, 0.25));
            let s = r.w.min(r.h) * 0.62;
            draw_circle(c.x, c.y, s * 0.62, Color::new(1.0, 0.85, 0.5, 0.06));
            crate::reactions::draw(code, c, s, crate::paint::alpha());
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
    // Where the server says it doesn't sell the pack, its items would lead
    // to nothing.
    let pack_sold = crate::channel::real_money_purchases()
        && online.offers.as_ref().is_none_or(|o| !o.welcome_price.is_empty());
    // The free reactions aren't sold (everyone has them).
    let free_reaction = item.category == Category::Reaction && item.price == Price::Free;
    !free_reaction && (item.price != Price::Pack || pack_sold || online.can_use(item))
}

/// The achievement an item comes with: (title, description).
fn achievement_of(code: &str) -> Option<(&'static str, &'static str)> {
    snook_legend_proto::ACHIEVEMENTS
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
        Price::Coins(_) | Price::Pack | Price::Earned if usable => {
            small(Icon::Check, GOOD, t("yours"), MUTED)
        }
        // TODO(0.12 collection): say how it is won.
        Price::Earned => small(Icon::Lock, MUTED, t("achievement"), MUTED),
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
        // A purchase whose answer never came doesn't hold the next one.
        online.buying = None;
        self.asked = online.signed_in();
        if online.signed_in() {
            online.send(ClientMsg::Shop);
            online.send(ClientMsg::Friends);
        }
    }

    /// Opens on the tab of an item (back from the 3D preview), at its page.
    pub fn show_item(&mut self, code: &str) {
        if let Some(item) = snook_legend_proto::shop::item(code) {
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
            "shop-tables" => Tab::Items(Category::Table),
            "shop-halls" => Tab::Items(Category::Hall),
            "shop-reactions" => Tab::Items(Category::Reaction),
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
        if std::mem::take(&mut self.sign_in) {
            return ShopAction::SignIn;
        }
        // Signed in after the screen opened: ask for the wallet now.
        if online.signed_in() && (online.wallet.is_none() || online.offers.is_none()) && !self.asked
        {
            self.asked = true;
            online.send(ClientMsg::Shop);
            online.send(ClientMsg::Friends);
        }
        // The server confirmed a purchase: show it.
        if let Some((code, gift)) = online.bought.take()
            && let Some(item) = snook_legend_proto::shop::item(&code)
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
        if screen_header(ui, r, Icon::Shop, t("Shop"), None) || (!modal && ui.escape()) {
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
        // Rows shrink a little when the window is short (eleven of them).
        let rows = Category::ALL.len() as f32 + 2.0;
        let pitch = ((side.h - 12.0) / rows).clamp(31.0, 40.0);
        let row_h = pitch - 4.0;
        let mut y = side.y;
        for c in Category::ALL {
            let tab = Tab::Items(c);
            let owned = ITEMS
                .iter()
                .filter(|i| i.category == c && listed(i, online))
                .filter(|i| i.price == Price::Free || online.can_use(i))
                .count();
            let total = ITEMS.iter().filter(|i| i.category == c && listed(i, online)).count();
            let tr = Rect::new(side.x, y, side.w, row_h);
            let count = if signed_in { format!("{owned}/{total}") } else { total.to_string() };
            if side_tab(ui, tr, category_name(c), SideNote::Count(&count), self.tab == tab) {
                self.tab = tab;
                self.page = 0;
            }
            y += pitch;
        }
        draw_line(side.x + 12.0, y - 1.0, side.x + side.w - 12.0, y - 1.0, 1.0, BORDER);
        y += 4.0;
        // The Store build that sells nothing keeps VIP's page for those who
        // are VIP (bought before): to others it would show what can't be had.
        let vip_page = !crate::channel::STORE
            || crate::channel::store_sales()
            || online.wallet.as_ref().is_some_and(|w| w.is_vip());
        for (tab, label, icon) in
            [(Tab::Coins, t("Coins"), Icon::Coin), (Tab::Vip, "VIP", Icon::Crown)]
        {
            if tab == Tab::Vip && !vip_page {
                continue;
            }
            let tr = Rect::new(side.x, y, side.w, row_h);
            if side_tab(ui, tr, label, SideNote::Icon(icon), self.tab == tab) {
                self.tab = tab;
                self.page = 0;
            }
            y += pitch;
        }
        let bx = side.x + side.w + 20.0;
        let mut body = Rect::new(bx, top, r.x + r.w - PAD - bx, r.y + r.h - PAD + 4.0 - top);
        if !signed_in {
            // Once, above the items: what signing in is for, and the way to it
            // (the items' buttons lead there too).
            let banner = Rect::new(body.x, body.y, body.w, 46.0);
            rrect(banner, crate::ui::BUTTON_RADIUS, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.08));
            rrect_lines(
                banner,
                crate::ui::BUTTON_RADIUS,
                1.0,
                Color::new(GOLD.r, GOLD.g, GOLD.b, 0.25),
            );
            crate::icons::draw(Icon::Person, vec2(banner.x + 22.0, banner.y + 23.0), 18.0, GOLD);
            let label = t("Sign in");
            let bw = ui.text_size(label, 14.0, true).x + 36.0;
            let b = Rect::new(banner.x + banner.w - bw - 7.0, banner.y + 7.0, bw, 32.0);
            let note = t("Sign in to buy and use items. Here is what there is:");
            let (note, size) = ui.fitted(note, 14.0, b.x - banner.x - 52.0, false);
            ui.text(&note, banner.x + 40.0, text_y(banner.y + 23.0, size), size, CREAM, false);
            if ui.primary_button(b, label) {
                self.sign_in = true;
            }
            body = Rect::new(body.x, body.y + 58.0, body.w, body.h - 58.0);
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
        let mut area = area;
        let mut action = None;
        if category == Category::Hall && self.dark_stage {
            // The dark stage hides the hall: say so, with the way out.
            let note = Rect::new(area.x, area.y, area.w, 44.0);
            rrect(note, 8.0, Color::new(1.0, 1.0, 1.0, 0.05));
            rrect_lines(note, 8.0, 1.0, BORDER);
            let label = t("Show the hall");
            let bw = (ui.text_size(label, 14.0, true).x + 28.0).min(note.w * 0.45);
            let b = Rect::new(note.x + note.w - bw - 6.0, note.y + 6.0, bw, 32.0);
            let text = t("The dark room hides the hall around the table.");
            crate::icons::draw(Icon::Eye, vec2(note.x + 18.0, note.y + 22.0), 16.0, GOLD);
            let room = b.x - note.x - 44.0;
            let (text, size) = ui.fitted(text, 14.0, room, false);
            ui.text(&text, note.x + 34.0, text_y(note.y + 22.0, size), size, CREAM, false);
            if button(ui, b, Kind::Normal, None, label, 14.0) && !modal {
                action = Some(ShopAction::ShowHall);
            }
            area = Rect::new(area.x, area.y + 54.0, area.w, area.h - 54.0);
        }
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
            // A cue's level, under its picture (what it has won online).
            let level_info = (item.category == Category::Cue && signed_in)
                .then(|| wallet.cue_wins(item.code))
                .filter(|&w| w > 0 || usable);
            match level_info {
                Some(wins) => {
                    let art = Rect::new(pic.x, pic.y, pic.w, pic.h - 18.0);
                    preview(ui, item, art);
                    cue_marks(art, snook_legend_proto::shop::cue_level(wins));
                    let (line, progress, how) = cue_level_text(wins);
                    let mid = pic.y + pic.h - 12.0;
                    let (line, size) = ui.fitted(&line, 12.0, pic.w, false);
                    ui.text(&line, pic.x, text_y(mid, size), size, MUTED, false);
                    let bar = Rect::new(pic.x, pic.y + pic.h - 3.0, pic.w, 3.0);
                    rrect(bar, 1.5, Color::new(1.0, 1.0, 1.0, 0.1));
                    rrect(Rect::new(bar.x, bar.y, bar.w * progress, bar.h), 1.5, GOLD);
                    let zone = Rect::new(pic.x, pic.y + pic.h - 20.0, pic.w, 20.0);
                    if ui.hovering(zone) {
                        tip = Some((how, zone));
                    }
                }
                None => preview(ui, item, pic),
            }
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
                if ui.icon_button(e, Icon::Eye, t("Look at it closely"), true) {
                    action = Some(ShopAction::Preview(item.code));
                }
                right -= 4.0;
            }
            let b = Rect::new(b.x, b.y, right - b.x, b.h);
            let size = 14.0;
            if equipped {
                button(ui, b, Kind::Disabled, Some(Icon::Check), t("In use"), size);
            } else if usable && item.category == Category::Reaction {
                // A reaction is kept, not put on: it is in the match's picker.
                button(ui, b, Kind::Disabled, Some(Icon::Check), t("In your reactions"), size);
            } else if !signed_in && matches!(item.price, Price::Coins(_) | Price::Free) {
                // Signed out: the button leads to signing in (rather than
                // a row of dead grey buttons).
                let label = if usable { t("Use") } else { t("Buy") };
                if button(ui, b, Kind::Normal, None, label, size) {
                    self.sign_in = true;
                }
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
                    Price::Free | Price::Earned => {}
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
        let item = snook_legend_proto::shop::item(confirm.code)?;
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
        // The friend's name and the message field.
        h += if gift { 28.0 + 46.0 } else { 0.0 };
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
            let label = t("Look at it closely");
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
            // A few words with it (optional; Enter gives it).
            const MESSAGE: u64 = GIFT_MESSAGE;
            let field = Rect::new(r.x + 32.0, y + 2.0, w - 64.0, 34.0);
            let max = snook_legend_proto::MAX_GIFT_MESSAGE;
            ui.text_input(MESSAGE, field, &mut self.gift_message, max);
            if self.gift_message.is_empty() && ui.focus != Some(MESSAGE) {
                let hint = t("Add a message (optional)");
                let hint = ui.ellipsize(hint, field.w - 18.0, 14.0);
                ui.text(&hint, field.x + 9.0, text_y(field.y + 17.0, 14.0), 14.0, MUTED, false);
            }
            y += 46.0;
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
            if ui.primary_button(ok_rect, label) || key_for_shop(ui, KeyCode::Enter) {
                online.shop_error = None;
                online.buying = Some(item.code.to_string());
                match &confirm.gift_to {
                    Some(to) => {
                        let gift = ClientMsg::GiftItem {
                            to: to.clone(),
                            code: item.code.to_string(),
                            message: self.gift_message.trim().to_string(),
                        };
                        // With the chat off, a message waits for the dialog
                        // that turns it on (and the gift with it).
                        if !online.send_text(gift) {
                            online.buying = None;
                        }
                    }
                    None => online.send(ClientMsg::BuyItem { code: item.code.to_string() }),
                }
            }
        }
        // A press on the dimmed screen around it cancels too.
        if !busy && (cancel || key_for_shop(ui, KeyCode::Escape) || ui.dismissed(r)) {
            self.confirm = None;
            online.shop_error = None;
        }
        action
    }

    /// The showroom's corner for the item (`r`): what it costs, or how it
    /// is had, and the button to buy it or to use it.
    pub fn showroom_offer(
        &mut self,
        ui: &mut Ui,
        online: &mut Online,
        code: &'static str,
        r: Rect,
    ) -> PreviewAction {
        let Some(item) = snook_legend_proto::shop::item(code) else { return PreviewAction::Close };
        let usable = online.signed_in() && online.can_use(item) || item.price == Price::Free;
        let mut action = PreviewAction::None;
        let b = Rect::new(r.x + r.w - 12.0 - 156.0, r.y + 12.0, 156.0, r.h - 24.0);
        match item.price {
            Price::Coins(n) if !usable && online.signed_in() => {
                let coins = online.wallet.as_ref().map(|w| w.coins).unwrap_or(0);
                // Too few coins: the dialog says how many are missing.
                let (kind, icon) = if coins >= n as i64 {
                    (Kind::Primary, Icon::Coin)
                } else {
                    (Kind::Short, Icon::Lock)
                };
                if button(ui, b, kind, Some(icon), t("Buy"), 15.0) {
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
            // VIP members' own: the VIP tab says how.
            Price::Vip => {
                button(ui, b, Kind::Disabled, Some(Icon::Crown), t("VIP only"), 13.0);
            }
            // Earned with an achievement: which one, and how.
            Price::Achievement(a) => {
                let (title, desc) = achievement_of(a).unwrap_or((a, ""));
                button(ui, b, Kind::Disabled, Some(Icon::Medal), title, 13.0);
                if ui.hovering(b) {
                    ui.tooltip(&tw("{title}: {how}", &[("title", &title), ("how", &desc)]), b);
                }
            }
            Price::Free => {
                button(ui, b, Kind::Disabled, Some(Icon::Lock), t("Sign in to use"), 13.0);
            }
            Price::Pack => {
                button(ui, b, Kind::Disabled, Some(Icon::Gift), t("welcome pack"), 13.0);
            }
            Price::Earned => {
                button(ui, b, Kind::Disabled, Some(Icon::Lock), t("achievement"), 13.0);
            }
            Price::Coins(_) => {
                button(ui, b, Kind::Disabled, Some(Icon::Lock), t("Sign in to buy"), 13.0);
            }
        }
        // The price, or how it is had, on the left.
        let coins = online.wallet.as_ref().map(|w| w.coins).filter(|_| online.signed_in());
        let x = r.x + 16.0;
        let w = b.x - 12.0 - x;
        ui.text(t("Price"), x, r.y + 12.0, 11.0, MUTED, false);
        status_line(ui, item, usable, coins, x, r.y + r.h - 22.0, w);
        action
    }

    /// The purchase went through.
    fn done_dialog(&mut self, ui: &mut Ui) {
        let Some((code, gift)) = self.done.clone() else { return };
        let Some(item) = snook_legend_proto::shop::item(code) else {
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
            || key_for_shop(ui, KeyCode::Escape)
            || key_for_shop(ui, KeyCode::Enter)
            || ui.dismissed(r)
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
            format!("{} VIP · {}", snook_legend_proto::GAME_NAME, t("one month"))
        } else {
            snook_legend_proto::shop::item(code).map(|i| t(i.name).to_string()).unwrap_or_default()
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
                    self.gift_message.clear();
                }
            }
        }
        let b = Rect::new(r.x + 24.0, r.y + h - 24.0 - 40.0, 140.0, 40.0);
        // The message written so far stays (it is the shop's).
        if ui.button(b, t("Cancel"), true) || key_for_shop(ui, KeyCode::Escape) || ui.dismissed(r) {
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
        let how = if store && offers.store_packs.is_empty() {
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
            self.store_coins(ui, online, &offers, area, y + 14.0);
            return;
        }
        let below = y + 14.0;
        if !offers.payments || offers.packs.is_empty() {
            // The server says what it sells to players signed in only.
            let text = if online.signed_in() {
                t("Coins can't be bought on this server yet.")
            } else {
                t("Sign in to buy coins.")
            };
            ui.text(text, area.x, below, 14.0, MUTED, false);
            return;
        }
        ui.text(t("If you like, you can also buy them:"), area.x, below, 15.0, MUTED, false);
        let mut top = below + 32.0;
        // The welcome pack, once per account.
        let owned = online.wallet.as_ref().map(|w| w.owned.clone()).unwrap_or_default();
        if !offers.welcome_price.is_empty() && !snook_legend_proto::shop::has_welcome_pack(&owned) {
            let card = Rect::new(area.x, top, area.w, 74.0);
            rrect(card, 10.0, Color::new(0.91, 0.75, 0.38, 0.10));
            rrect_lines(card, 10.0, 1.6, GOLD);
            crate::icons::draw(Icon::Gift, vec2(card.x + 30.0, card.y + 37.0), 30.0, GOLD);
            ui.text(t("Welcome pack"), card.x + 58.0, card.y + 14.0, 17.0, GOLD, true);
            let names: Vec<String> = snook_legend_proto::shop::WELCOME_ITEMS
                .iter()
                .filter_map(|c| snook_legend_proto::shop::item(c))
                .map(|i| t(i.name).to_string())
                .collect();
            let what = tw(
                "Cue “{cue}”, cloth “{cloth}” and {n} coins · only once, sold nowhere else",
                &[
                    ("cue", &names.first().cloned().unwrap_or_default()),
                    ("cloth", &names.get(1).cloned().unwrap_or_default()),
                    ("n", &snook_legend_proto::shop::WELCOME_COINS),
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
        let cards: Vec<(u32, u32, String)> =
            offers.packs.iter().map(|p| (p.coins, p.bonus, p.price.clone())).collect();
        let (clicked, y) = pack_cards(ui, area, &cards, online.signed_in());
        if let Some(k) = clicked {
            online.send(ClientMsg::BuyCoins { pack: offers.packs[k].code.clone() });
        }
        let mut lines = vec![
            t("Payment opens in your browser (card, Pix…), on Stripe's secure page."),
            t("Coins arrive in a moment; they are only used for items in the game."),
            t("You have 7 days to change your mind and get your money back."),
        ];
        if online.age.is_some_and(|a| a.minor == Some(true)) {
            lines.push(t("Under 18: buy only with a parent's or guardian's permission."));
        }
        for (i, line) in lines.iter().enumerate() {
            ui.text(line, area.x, y + i as f32 * 20.0, 13.0, MUTED, false);
        }
        let server = online.server.clone();
        crate::legal::links_at(ui, &server, area.x, y + lines.len() as f32 * 20.0 + 4.0, 13.0);
    }

    /// The Microsoft Store build: coin packs bought in the Store (prices
    /// from the Store, in the player's currency), from `below` down.
    fn store_coins(
        &mut self,
        ui: &mut Ui,
        online: &mut Online,
        offers: &crate::online::ShopOffers,
        area: Rect,
        below: f32,
    ) {
        if offers.store_packs.is_empty() {
            return;
        }
        let Some(prices) = online.store_shop.prices.clone() else {
            ui.text(
                t("Loading prices from the Microsoft Store…"),
                area.x,
                below,
                14.0,
                MUTED,
                false,
            );
            return;
        };
        let on_sale: Vec<(&snook_legend_proto::StorePack, String)> = offers
            .store_packs
            .iter()
            .filter_map(|p| Some((p, prices.get(&p.store_id)?.clone())))
            .collect();
        if on_sale.is_empty() {
            ui.text(
                t("Coins can't be bought in this version yet."),
                area.x,
                below,
                14.0,
                MUTED,
                false,
            );
            return;
        }
        ui.text(t("If you like, you can also buy them:"), area.x, below, 15.0, MUTED, false);
        let top = below + 32.0;
        let area = Rect::new(area.x, top, area.w, area.h - (top - area.y));
        let cards: Vec<(u32, u32, String)> =
            on_sale.iter().map(|(p, price)| (p.coins, p.bonus, price.clone())).collect();
        // One purchase at a time: the buttons wait for the Store and the server.
        let waiting = online.store_shop.waiting();
        let (clicked, y) = pack_cards(ui, area, &cards, online.signed_in() && !waiting);
        if let Some(k) = clicked {
            online.store_shop.buy(&on_sale[k].0.store_id);
        }
        let status = match &online.store_shop.busy {
            Some(crate::msstore::Busy::Buying(_)) => {
                Some(t("Finish the purchase in the Microsoft Store window…"))
            }
            _ if waiting => Some(t("Confirming your purchase…")),
            _ => None,
        };
        let mut lines = vec![
            t("Paid in the Microsoft Store, with your Microsoft account."),
            t("Coins arrive in a moment; they are only used for items in the game."),
        ];
        if let Some(status) = status {
            lines.insert(0, status);
        }
        for (i, line) in lines.iter().enumerate() {
            let color = if status.is_some() && i == 0 { GOLD } else { MUTED };
            ui.text(line, area.x, y + i as f32 * 20.0, 13.0, color, status.is_some() && i == 0);
        }
    }

    /// The Microsoft Store build: a month of VIP bought in the Store (its
    /// price from the Store, in the player's currency), at the top left of
    /// `at` (its width for the button). `vip`: the player is VIP already, and
    /// the month comes after the time left. Returns the height below.
    fn store_vip(
        &mut self,
        ui: &mut Ui,
        online: &mut Online,
        offers: &crate::online::ShopOffers,
        at: Rect,
        vip: bool,
    ) -> f32 {
        let (x, mut y) = (at.x, at.y);
        let not_sold = |ui: &mut Ui, y: f32| {
            if !vip {
                ui.text(t("VIP isn't sold in this version of the game."), x, y, 14.0, MUTED, false);
                return y + 28.0;
            }
            y
        };
        if offers.store_vip.is_empty() {
            return not_sold(ui, y);
        }
        let Some(prices) = online.store_shop.prices.clone() else {
            ui.text(t("Loading prices from the Microsoft Store…"), x, y, 14.0, MUTED, false);
            return y + 28.0;
        };
        // Not published yet, or not sold in the player's market.
        let Some(price) = prices.get(&offers.store_vip).cloned() else {
            return not_sold(ui, y);
        };
        let label = if vip {
            tw("Add a month · {p}", &[("p", &price)])
        } else {
            tw("A month of VIP · {p}", &[("p", &price)])
        };
        let b = Rect::new(x, y, at.w, 42.0);
        // One purchase at a time: the button waits for the Store and the server.
        let waiting = online.store_shop.waiting();
        let clicked = if online.signed_in() && !waiting {
            if vip { ui.button(b, &label, true) } else { ui.primary_button(b, &label) }
        } else {
            ui.button(b, &label, false)
        };
        if clicked {
            online.store_shop.buy(&offers.store_vip);
        }
        y += 50.0;
        let status = match &online.store_shop.busy {
            Some(crate::msstore::Busy::Buying(_)) => {
                Some(t("Finish the purchase in the Microsoft Store window…"))
            }
            _ if waiting => Some(t("Confirming your purchase…")),
            _ => None,
        };
        let mut lines = vec![t("Paid in the Microsoft Store, with your Microsoft account.")];
        if vip {
            lines.push(t("The new month starts when this one ends."));
        }
        if let Some(status) = status {
            lines.insert(0, status);
        }
        for (i, line) in lines.iter().enumerate() {
            let first = status.is_some() && i == 0;
            ui.text(line, x, y + i as f32 * 20.0, 13.0, if first { GOLD } else { MUTED }, first);
        }
        y + lines.len() as f32 * 20.0 + 10.0
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
                &[("n", &snook_legend_proto::REACTIONS.iter().filter(|r| r.is_vip()).count())],
            ),
            t("Every replay kept (free players keep their last 10 games)").to_string(),
            t("16-player tournaments, and first place on the waiting list of a full one")
                .to_string(),
            t("Nothing that wins games: VIP only changes the look").to_string(),
            // The Store build sells a month at a time; elsewhere it is a
            // subscription.
            if crate::channel::real_money_purchases() {
                t("Cancel whenever you want: VIP lasts until the end of the paid month")
            } else {
                t("A month at a time: nothing renews by itself")
            }
            .to_string(),
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
        let title = format!("{} VIP", snook_legend_proto::GAME_NAME);
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
            // The subscription is managed on the payment provider's pages;
            // in the Store build, another month is added after this one.
            if crate::channel::real_money_purchases() {
                let b = Rect::new(area.x, y, bw, 40.0);
                if ui.button(b, t("Manage subscription"), offers.payments) {
                    online.send(ClientMsg::ManageVip);
                }
                y += 52.0;
            } else {
                y = self.store_vip(ui, online, &offers, Rect::new(area.x, y, bw, 0.0), true);
            }
        } else if !crate::channel::real_money_purchases() {
            y = self.store_vip(ui, online, &offers, Rect::new(area.x, y, bw, 0.0), false);
        } else if offers.payments && !offers.vip_price.is_empty() {
            let b = Rect::new(area.x, y, bw, 42.0);
            if ui.primary_button(b, &tw("Become VIP · {p}", &[("p", &offers.vip_price)])) {
                online.send(ClientMsg::SubscribeVip);
            }
            y += 50.0;
            // What subscribing means, and the terms it follows.
            let note = t("Renews every month until you cancel. 7 days to change your mind.");
            ui.text(note, area.x, y, 13.0, MUTED, false);
            let server = online.server.clone();
            crate::legal::links_at(ui, &server, area.x, y + 20.0, 13.0);
            y += 50.0;
        } else {
            let text = if online.signed_in() {
                t("VIP can't be bought on this server yet.")
            } else {
                t("Sign in to become VIP.")
            };
            ui.text(text, area.x, y, 14.0, MUTED, false);
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

/// Cards for coin packs (coins, bonus, price) in rows across `area`, with a
/// button each (pressable when `enabled`). Returns the card pressed and the
/// height below the cards.
fn pack_cards(
    ui: &mut Ui,
    area: Rect,
    packs: &[(u32, u32, String)],
    enabled: bool,
) -> (Option<usize>, f32) {
    let n = packs.len();
    let cols = n.min(((area.w + GAP) / (180.0 + GAP)).floor().max(1.0) as usize).max(1);
    let cw = (area.w - (cols - 1) as f32 * GAP) / cols as f32;
    let card_h = 168.0;
    let mut clicked = None;
    for (k, (coins, bonus, price)) in packs.iter().enumerate() {
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
        let total = (coins + bonus).to_string();
        ui.text_centered(&total, c.x + c.w / 2.0, c.y + 56.0, 24.0, CREAM, true);
        ui.text_centered(t("coins"), c.x + c.w / 2.0, c.y + 84.0, 13.0, MUTED, false);
        if *bonus > 0 {
            ui.text_centered(
                &tw("+{n} bonus", &[("n", bonus)]),
                c.x + c.w / 2.0,
                c.y + 102.0,
                13.0,
                GOLD,
                true,
            );
        }
        let b = Rect::new(c.x + 12.0, c.y + c.h - 46.0, c.w - 24.0, 34.0);
        if enabled {
            if ui.primary_button(b, price) {
                clicked = Some(k);
            }
        } else {
            ui.button(b, price, false);
        }
    }
    let rows = n.div_ceil(cols) as f32;
    (clicked, area.y + rows * (card_h + GAP) + 4.0)
}

/// The field of the words that go with a present.
const GIFT_MESSAGE: u64 = 64;

/// A key pressed for the shop's own dialog: not one meant for a dialog
/// drawn over it (a present arriving, a warning from the staff), which
/// holds the keyboard meanwhile.
fn key_for_shop(ui: &Ui, key: KeyCode) -> bool {
    is_key_pressed(key) && ui.focus.is_none_or(|f| f == GIFT_MESSAGE)
}
