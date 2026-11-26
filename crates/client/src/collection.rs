//! The collection: everything the player has — cues (the ones that can't
//! be bought too), cloths, chalks, ball sets, tables, halls, frames, name
//! colours, winning effects and reactions — by category, each with its
//! rarity and how and when it was got ("Champion of the Winter Cup 2026 ·
//! 12 Dec 2026", "Bought for 4000 coins", "Gift from Ana"). Items can be put
//! on and looked at closely on the table from here; what is still missing
//! shows how it is had.
//!
//! The shop sells; the collection keeps. It is drawn inside the shop's
//! screen (see [`crate::shop::ShopScreen`]), so the showroom serves both.

use macroquad::prelude::*;
use snook_legend_proto::ClientMsg;
use snook_legend_proto::collection::Origin;
#[cfg(test)]
use snook_legend_proto::collection::OwnedItem;
use snook_legend_proto::reactions::{ITEM_PREFIX, REACTIONS, Reaction};
use snook_legend_proto::shop::{self, Category, ITEMS, Item, Price, Rarity};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle, draw_triangle};

use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::online::Online;
use crate::ranking::{empty_state, text_y};
use crate::shop::{CardLayout, Kind, SideNote, button, has_3d_preview, side_tab};
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, motion, rrect, rrect_gradient, rrect_lines};

/// What the collection asks of the screen around it.
pub enum CollectionAction {
    None,
    /// Look at an item closely on the table (the showroom).
    Preview(&'static str),
    /// To the shop, on an item that is sold there (or its first page).
    Shop(Option<&'static str>),
}

/// How the cards are ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sort {
    /// The rarest first (then by name).
    Rarity,
    /// The latest got first (then what is missing).
    Newest,
    /// By name.
    Name,
}

impl Sort {
    fn name(self) -> &'static str {
        match self {
            Sort::Rarity => t("Rarest first"),
            Sort::Newest => t("Newest first"),
            Sort::Name => t("By name"),
        }
    }

    fn next(self) -> Sort {
        match self {
            Sort::Rarity => Sort::Newest,
            Sort::Newest => Sort::Name,
            Sort::Name => Sort::Rarity,
        }
    }
}

/// A card of the collection: a shop item, or a reaction (most reactions
/// are not shop items).
#[derive(Clone, Copy)]
enum Entry {
    Item(&'static Item),
    Reaction(&'static Reaction),
}

impl Entry {
    fn category(self) -> Category {
        match self {
            Entry::Item(i) => i.category,
            Entry::Reaction(_) => Category::Reaction,
        }
    }

    /// The code the server's collection lists it under.
    fn code(self) -> String {
        match self {
            Entry::Item(i) => i.code.to_string(),
            Entry::Reaction(r) => format!("{ITEM_PREFIX}{}", r.code),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Entry::Item(i) => t(i.name),
            Entry::Reaction(r) => match r.item() {
                Some(i) => t(i.name),
                None => reaction_name(r),
            },
        }
    }

    fn rarity(self) -> Rarity {
        match self {
            Entry::Item(i) => shop::rarity(i),
            Entry::Reaction(r) => shop::reaction_rarity(r),
        }
    }
}

/// Every card of a category: the shop's items, or every reaction.
fn entries(c: Category) -> Vec<Entry> {
    if c == Category::Reaction {
        REACTIONS.iter().map(Entry::Reaction).collect()
    } else {
        ITEMS.iter().filter(|i| i.category == c).map(Entry::Item).collect()
    }
}

/// A reaction's name: its item's, or its achievement's.
fn reaction_name(r: &Reaction) -> &'static str {
    match r.achievement() {
        Some(a) => t(a.title),
        None if r.is_vip() => "VIP",
        None => t("Reaction"),
    }
}

/// The colour of a rarity.
pub fn rarity_color(r: Rarity) -> Color {
    let c = r.color();
    Color::from_rgba(c[0], c[1], c[2], 255)
}

pub fn rarity_name(r: Rarity) -> &'static str {
    t(r.name())
}

/// An amount of coins with spaces between the thousands ("15 000").
pub fn coins_text(n: i64) -> String {
    let digits = n.abs().to_string();
    let mut out = String::new();
    for (k, ch) in digits.chars().enumerate() {
        if k > 0 && (digits.len() - k).is_multiple_of(3) {
            out.push('\u{2009}');
        }
        out.push(ch);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// How an item was got, in the player's words.
pub fn origin_text(origin: &Origin) -> String {
    match origin {
        Origin::Bought { price } if *price > 0 => {
            tw("Bought for {n} coins", &[("n", &coins_text(*price))])
        }
        Origin::Bought { .. } => t("Bought in the shop").to_string(),
        Origin::Gift { from } => tw("Gift from {name}", &[("name", from)]),
        Origin::Staff { reason } if reason.is_empty() => t("A present from the staff").to_string(),
        Origin::Staff { reason } => tw("From the staff: {reason}", &[("reason", reason)]),
        Origin::Tournament { name, place: 1 } => tw("Champion of {name}", &[("name", name)]),
        Origin::Tournament { name, place: 2 } => tw("Runner-up of {name}", &[("name", name)]),
        Origin::Tournament { name, place } => {
            tw("Place {n} in {name}", &[("n", place), ("name", name)])
        }
        Origin::Achievement { code } => {
            let title = snook_legend_proto::ACHIEVEMENTS
                .iter()
                .find(|a| a.code == code)
                .map_or(code.as_str(), |a| t(a.title));
            tw("Achievement: {title}", &[("title", &title)])
        }
        Origin::Auction { price } => {
            tw("Won at auction for {n} coins", &[("n", &coins_text(*price))])
        }
        Origin::VipMonth { month } => {
            let shown = match month.split_once('-') {
                Some((y, m)) => format!("{m}/{y}"),
                None => month.clone(),
            };
            tw("VIP's item of the month · {month}", &[("month", &shown)])
        }
        Origin::Event { name } => tw("Prize of {name}", &[("name", name)]),
        Origin::Season { name } => tw("Prize of {name}", &[("name", name)]),
        Origin::WelcomePack => t("Welcome pack").to_string(),
        Origin::Unknown => t("Yours").to_string(),
    }
}

/// Items of the season's champion.
const CHAMPION: &[&str] = &[
    "cue_season_champion",
    "table_champion",
    "frame_season",
    "balls_champion",
    "effect_champion",
    "reaction_season_medal",
    "chalk_champion",
];
/// Items of the official tournaments.
const TOURNAMENT: &[&str] = &["hall_arena", "cloth_arena", "frame_pro"];

/// How an item only won is won.
pub fn earned_hint(code: &str) -> &'static str {
    if code.starts_with("vip_month_") {
        t("VIP's item of the month: VIP members get it that month")
    } else if CHAMPION.contains(&code) {
        t("Only the season's champion wins it")
    } else if TOURNAMENT.contains(&code) {
        t("Won in the official tournaments")
    } else {
        t("Only won: events, cups and seasons")
    }
}

/// How an item still missing is had.
fn how_to_get(e: Entry, online: &Online) -> String {
    match e {
        Entry::Reaction(r) if r.item().is_none() => crate::reactions::how(r, false),
        Entry::Reaction(r) => how_item(r.item().expect("checked"), online),
        Entry::Item(i) => how_item(i, online),
    }
}

fn how_item(i: &Item, online: &Online) -> String {
    match i.price {
        Price::Free => t("Every player has it").to_string(),
        Price::Coins(n) => tw("In the shop, for {n} coins", &[("n", &coins_text(n as i64))]),
        Price::Vip => t("For VIP members").to_string(),
        Price::Pack => t("In the welcome pack").to_string(),
        Price::Earned => earned_hint(i.code).to_string(),
        Price::Achievement(a) => {
            match snook_legend_proto::ACHIEVEMENTS.iter().find(|x| x.code == a) {
                Some(x) if x.secret && !online.has_achievement(a) => {
                    t("A secret achievement").to_string()
                }
                Some(x) => tw(
                    "{title}: {how}",
                    &[("title", &t(x.title)), ("how", &crate::profile::achievement_text(x))],
                ),
                None => a.to_string(),
            }
        }
    }
}

/// Whether the player has this card's item (owned, won, or VIP's while
/// VIP).
fn has(e: Entry, online: &Online) -> bool {
    match e {
        Entry::Item(i) => i.price == Price::Free || online.can_use(i),
        Entry::Reaction(r) => {
            let (bought, vip) = match &online.wallet {
                Some(w) => (snook_legend_proto::reactions::bought(&w.owned), w.is_vip()),
                None => (Vec::new(), false),
            };
            let unlocked = r.achievement().is_some_and(|a| online.has_achievement(a.code));
            r.unlock.is_none() || unlocked || r.available::<&str, _>(&[], &bought, vip)
        }
    }
}

/// How the player has an item they have: the server's record (with the
/// date), or what its price says.
fn held(e: Entry, online: &Online) -> (String, i64) {
    let code = e.code();
    if let Some(o) = online.collection.as_ref().and_then(|c| c.iter().find(|o| o.code == code)) {
        return (origin_text(&o.origin), o.acquired_at);
    }
    let price = match e {
        Entry::Item(i) => Some(i.price),
        Entry::Reaction(r) => r.item().map(|i| i.price),
    };
    let text = match (price, e) {
        (Some(Price::Free), _) => t("Every player has it").to_string(),
        (Some(Price::Vip), _) => t("Yours while you are VIP").to_string(),
        (None, Entry::Reaction(r)) if r.is_vip() => t("Yours while you are VIP").to_string(),
        (None, Entry::Reaction(r)) if r.unlock.is_none() => t("Every player has it").to_string(),
        (None, Entry::Reaction(r)) => crate::reactions::how(r, true),
        (Some(Price::Achievement(a)), _) => {
            origin_text(&Origin::Achievement { code: a.to_string() })
        }
        (Some(Price::Pack), _) => t("Welcome pack").to_string(),
        _ => t("Yours").to_string(),
    };
    (text, 0)
}

/// A card's frame in the colour of its item's rarity: a wash of the colour
/// at the top, its edge, a pill with the rarity's name, and for the
/// legendary a light running round it.
pub fn rarity_card(ui: &Ui, c: Rect, rarity: Rarity, selected: bool, time: f32) {
    let rc = rarity_color(rarity);
    let tint = |a: f32| Color::new(rc.r, rc.g, rc.b, a);
    rrect(c, 10.0, Color::new(1.0, 1.0, 1.0, if selected { 0.07 } else { 0.035 }));
    if rarity > Rarity::Common {
        rrect_gradient(c, 10.0, tint(0.16), tint(0.01));
    }
    if rarity == Rarity::Legendary {
        // A soft glow round the card, breathing.
        let k = 0.5 + 0.5 * (time * 2.0).sin();
        for (grow, a) in [(4.0, 0.06), (2.0, 0.1)] {
            let g = Rect::new(c.x - grow, c.y - grow, c.w + grow * 2.0, c.h + grow * 2.0);
            rrect_lines(g, 10.0 + grow, 2.0, tint(a * (0.6 + 0.4 * k)));
        }
    }
    let edge = match rarity {
        Rarity::Common => BORDER,
        _ => tint(0.8),
    };
    rrect_lines(c, 10.0, if rarity >= Rarity::Epic { 1.6 } else { 1.0 }, edge);
    if rarity == Rarity::Legendary {
        // A spark running along the top and bottom edges.
        let f = (time * 0.35).fract();
        for (y, dir) in [(c.y + 0.5, 1.0f32), (c.y + c.h - 0.5, -1.0)] {
            let x0 = if dir > 0.0 {
                c.x + 10.0 + f * (c.w - 40.0)
            } else {
                c.x + c.w - 30.0 - f * (c.w - 40.0)
            };
            for k in 0..6 {
                let a = 0.9 * (1.0 - k as f32 / 6.0);
                let x = x0 - dir * k as f32 * 3.0;
                draw_line(x, y, x + 3.0, y, 2.0, Color::new(1.0, 0.95, 0.8, a));
            }
        }
    }
    if selected {
        rrect_lines(c, 10.0, 2.0, GOLD);
    }
    // The rarity's name, top right.
    if rarity > Rarity::Common {
        let name = rarity_name(rarity);
        let w = ui.text_size(name, 10.5, true).x + 14.0;
        let pill = Rect::new(c.x + c.w - w - 8.0, c.y + 8.0, w, 17.0);
        rrect(pill, 8.5, Color::new(0.0, 0.0, 0.0, 0.55));
        rrect_lines(pill, 8.5, 1.0, tint(0.7));
        ui.text_centered(name, pill.x + w / 2.0, pill.y + 3.0, 10.5, rc, true);
    }
}

/// A sheen sweeping over a legendary item's picture (a cue's shimmer).
pub fn shimmer(r: Rect, time: f32) {
    let f = (time * 0.45).fract() * 1.8 - 0.4;
    let x = r.x + f * r.w;
    let w = r.w * 0.08;
    let skew = r.h * 0.5;
    for (k, a) in [(0.0f32, 0.18f32), (0.35, 0.1), (0.7, 0.05)] {
        let dx = k * w;
        let quad = [
            vec2(x + dx, r.y),
            vec2(x + dx + w * 0.5, r.y),
            vec2(x + dx + w * 0.5 - skew, r.y + r.h),
            vec2(x + dx - skew, r.y + r.h),
        ];
        if quad.iter().all(|p| p.x >= r.x && p.x <= r.x + r.w) {
            let c = Color::new(1.0, 0.97, 0.88, a);
            draw_triangle(quad[0], quad[1], quad[2], c);
            draw_triangle(quad[0], quad[2], quad[3], c);
        }
    }
}

/// The rarity of a cue that stands out (epic or legendary), if it does.
pub fn rare_cue(code: &str) -> Option<Rarity> {
    shop::item(code).map(shop::rarity).filter(|r| *r >= Rarity::Epic)
}

/// A glow and a small gem on a player's card when their cue is epic or
/// legendary (the opponent's card at the table, the match's intro):
/// `r` is the card. Returns the gem's rect for a tooltip.
pub fn rare_cue_badge(r: Rect, rarity: Rarity, time: f32) -> Rect {
    let rc = rarity_color(rarity);
    let k = 0.5 + 0.5 * (time * 2.4).sin();
    let strong = if rarity == Rarity::Legendary { 1.0 } else { 0.6 };
    for (grow, a) in [(3.0, 0.10), (1.5, 0.18)] {
        let g = Rect::new(r.x - grow, r.y - grow, r.w + grow * 2.0, r.h + grow * 2.0);
        rrect_lines(g, 9.0 + grow, 1.5, Color::new(rc.r, rc.g, rc.b, a * strong * (0.6 + 0.4 * k)));
    }
    // A cut gem in the bottom left corner.
    let c = vec2(r.x + 9.0, r.y + r.h - 9.0);
    let s = 5.5;
    let top = Color::new(rc.r * 0.6 + 0.4, rc.g * 0.6 + 0.4, rc.b * 0.6 + 0.4, 1.0);
    draw_triangle(
        vec2(c.x - s, c.y - s * 0.2),
        vec2(c.x + s, c.y - s * 0.2),
        vec2(c.x, c.y + s),
        rc,
    );
    draw_triangle(
        vec2(c.x - s, c.y - s * 0.2),
        vec2(c.x + s, c.y - s * 0.2),
        vec2(c.x, c.y - s * 0.9),
        top,
    );
    let glint = Color::new(1.0, 1.0, 1.0, 0.5 + 0.5 * k);
    draw_line(c.x - s * 0.4, c.y - s * 0.5, c.x - s * 0.1, c.y - s * 0.5, 1.0, glint);
    Rect::new(c.x - s - 2.0, c.y - s - 2.0, s * 2.0 + 4.0, s * 2.0 + 4.0)
}

/// "Legendary cue: Emperor", for the badge's tooltip.
pub fn rare_cue_tip(code: &str, rarity: Rarity) -> String {
    let name = shop::item(code).map_or(code, |i| t(i.name));
    let what = match rarity {
        Rarity::Legendary => t("Legendary cue"),
        _ => t("Epic cue"),
    };
    format!("{what}: {name}")
}

const SIDE_W: f32 = 216.0;
const CARD_MIN_W: f32 = 176.0;
const CARD_MIN_H: f32 = 178.0;
const CARD_MAX_H: f32 = 262.0;
const FILTERS: f32 = 44.0;

pub struct CollectionScreen {
    category: Category,
    rarity: Option<Rarity>,
    sort: Sort,
    /// Shows what is still missing too (greyed, with how it is had): off
    /// at first, the collection shows what the player has.
    missing: bool,
    /// Where the grid of cards is scrolled to.
    grid: crate::ui::Scroll,
    /// An item to bring into view (back from the showroom).
    focus: Option<String>,
}

impl Default for CollectionScreen {
    fn default() -> Self {
        CollectionScreen {
            category: Category::Cue,
            rarity: None,
            sort: Sort::Rarity,
            missing: false,
            grid: Default::default(),
            focus: None,
        }
    }
}

impl CollectionScreen {
    /// Opened: asks the server for the collection (with its origins).
    pub fn open(&mut self, online: &mut Online) {
        if online.signed_in() {
            online.send(ClientMsg::Collection);
            online.send(ClientMsg::Shop);
        }
    }

    /// Back from the showroom: on that item.
    pub fn show_item(&mut self, code: &str) {
        if let Some(item) = shop::item(code) {
            self.category = item.category;
            self.grid.reset();
            self.focus = Some(code.to_string());
        }
    }

    /// The left column and the cards, in `body` (under the header).
    pub fn draw(
        &mut self,
        ui: &mut Ui,
        online: &mut Online,
        area: Rect,
        modal: bool,
    ) -> CollectionAction {
        let mut action = CollectionAction::None;
        let signed_in = online.signed_in();
        // The categories, down the left side, with what is had of each.
        let side = Rect::new(area.x - 4.0, area.y, SIDE_W, area.h);
        let rows = Category::ALL.len() as f32 + 2.5;
        let pitch = ((side.h - 12.0) / rows).clamp(31.0, 40.0);
        let row_h = pitch - 4.0;
        let mut y = side.y;
        let (mut had_all, mut total_all) = (0, 0);
        for c in Category::ALL {
            let list = entries(c);
            let had = list.iter().filter(|e| has(**e, online)).count();
            had_all += had;
            total_all += list.len();
            let tr = Rect::new(side.x, y, side.w, row_h);
            let count = format!("{had}/{}", list.len());
            if side_tab(
                ui,
                tr,
                crate::shop::category_name(c),
                SideNote::Count(&count),
                self.category == c,
            ) {
                self.category = c;
                self.grid.reset();
            }
            y += pitch;
        }
        // The whole collection: how much of it there is.
        draw_line(side.x + 12.0, y - 1.0, side.x + side.w - 12.0, y - 1.0, 1.0, BORDER);
        let share = had_all as f32 / total_all.max(1) as f32;
        let bar = Rect::new(side.x + 14.0, y + 30.0, side.w - 28.0, 5.0);
        let line = tw("{n} of {total} items", &[("n", &had_all), ("total", &total_all)]);
        let (line, size) = ui.fitted(&line, 13.0, side.w - 28.0, true);
        ui.text(&line, side.x + 14.0, y + 10.0, size, CREAM, true);
        rrect(bar, 2.5, Color::new(1.0, 1.0, 1.0, 0.08));
        rrect(Rect::new(bar.x, bar.y, bar.w * share, bar.h), 2.5, GOLD);

        let bx = side.x + side.w + 20.0;
        let mut body = Rect::new(bx, area.y, area.x + area.w - bx, area.h);
        if !signed_in {
            // Signed out: everything there is to have, and how.
            let note = Rect::new(body.x, body.y, body.w, 36.0);
            rrect(note, 8.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.08));
            crate::icons::draw(Icon::Person, vec2(note.x + 18.0, note.y + 18.0), 16.0, GOLD);
            let text = t("Sign in to see your collection.");
            let (text, size) = ui.fitted(text, 14.0, note.w - 44.0, false);
            ui.text(&text, note.x + 34.0, text_y(note.y + 18.0, size), size, CREAM, false);
            body = Rect::new(body.x, body.y + 46.0, body.w, body.h - 46.0);
        }
        // The filters: rarity, missing items, the order.
        let mut x = body.x;
        let fy = body.y;
        let chip =
            |ui: &mut Ui, x: f32, label: &str, color: Option<Color>, on: bool| -> (f32, bool) {
                let w =
                    ui.text_size(label, 12.5, true).x + if color.is_some() { 30.0 } else { 20.0 };
                let r = Rect::new(x, fy, w, 30.0);
                ui.block(r);
                let hover = ui.hovering(r);
                let fill = if on {
                    0.14
                } else if hover {
                    0.08
                } else {
                    0.04
                };
                rrect(r, 15.0, Color::new(1.0, 1.0, 1.0, fill));
                let edge = match (on, color) {
                    (true, Some(c)) => c,
                    (true, None) => GOLD,
                    _ => BORDER,
                };
                rrect_lines(r, 15.0, if on { 1.5 } else { 1.0 }, edge);
                let mut tx = r.x + 10.0;
                if let Some(c) = color {
                    draw_circle(tx + 5.0, r.y + 15.0, 4.5, c);
                    tx += 14.0;
                }
                ui.text(
                    label,
                    tx,
                    text_y(r.y + 15.0, 12.5),
                    12.5,
                    if on { CREAM } else { MUTED },
                    true,
                );
                (w, hover && ui.clicked(r))
            };
        let (w, hit) = chip(ui, x, t("All"), None, self.rarity.is_none());
        if hit && !modal {
            self.rarity = None;
            self.grid.reset();
        }
        x += w + 6.0;
        for r in Rarity::ALL {
            let (w, hit) =
                chip(ui, x, rarity_name(r), Some(rarity_color(r)), self.rarity == Some(r));
            if hit && !modal {
                self.rarity = if self.rarity == Some(r) { None } else { Some(r) };
                self.grid.reset();
            }
            x += w + 6.0;
        }
        // On the right: the order, and whether missing items show.
        let sort_label = self.sort.name();
        let sw = ui.text_size(sort_label, 12.5, true).x + 40.0;
        let sort_r = Rect::new(body.x + body.w - sw, fy, sw, 30.0);
        if ui.button(sort_r, "", true) && !modal {
            self.sort = self.sort.next();
            self.grid.reset();
        }
        crate::icons::draw(Icon::List, vec2(sort_r.x + 14.0, sort_r.y + 15.0), 14.0, MUTED);
        ui.text(sort_label, sort_r.x + 26.0, text_y(sort_r.y + 15.0, 12.5), 12.5, CREAM, true);
        let miss_label = t("Missing too");
        let mw = ui.text_size(miss_label, 12.5, true).x + 20.0;
        if x + mw + 8.0 < sort_r.x {
            let (_, hit) = chip(ui, sort_r.x - mw - 8.0, miss_label, None, self.missing);
            if hit && !modal {
                self.missing = !self.missing;
                self.grid.reset();
            }
        }

        // The cards.
        let area = Rect::new(body.x, body.y + FILTERS, body.w, body.h - FILTERS);
        let mut list: Vec<(Entry, bool)> = entries(self.category)
            .into_iter()
            .map(|e| (e, has(e, online)))
            .filter(|(e, had)| {
                (*had || self.missing || !signed_in) && self.rarity.is_none_or(|r| e.rarity() == r)
            })
            .collect();
        let when = |e: Entry| -> i64 {
            let code = e.code();
            online
                .collection
                .as_ref()
                .and_then(|c| c.iter().find(|o| o.code == code))
                .map_or(0, |o| o.acquired_at)
        };
        match self.sort {
            Sort::Rarity => list.sort_by(|a, b| {
                b.1.cmp(&a.1).then(b.0.rarity().cmp(&a.0.rarity())).then(a.0.name().cmp(b.0.name()))
            }),
            Sort::Newest => list.sort_by(|a, b| b.1.cmp(&a.1).then(when(b.0).cmp(&when(a.0)))),
            Sort::Name => list.sort_by(|a, b| a.0.name().cmp(b.0.name())),
        }
        if list.is_empty() {
            let text = if self.missing {
                t("Nothing of this rarity here.")
            } else {
                t("None yet: they are waiting to be won.")
            };
            empty_state(ui, area, Icon::Chest, text);
            return action;
        }
        // Everything of the category (as filtered) in one grid that scrolls
        // down; another category rises in (from below for one further down).
        let layout = CardLayout::new(area, list.len(), CARD_MIN_W, CARD_MIN_H, CARD_MAX_H);
        if let Some(code) = self.focus.take()
            && let Some(i) = list.iter().position(|(e, _)| e.code() == code)
        {
            layout.show(&mut self.grid, i);
        }
        let wallet = online.wallet.clone().unwrap_or_default();
        let time = get_time() as f32;
        let mut tip = None;
        let index = Category::ALL.iter().position(|c| *c == self.category).unwrap_or(0);
        let fx = ui.switch_down(motion::id("collection-tab"), index as i64, 14.0);
        let grid = layout.begin(ui, &mut self.grid);
        for (k, c) in grid.cards() {
            let (e, had) = list[k];
            let item = match e {
                Entry::Item(i) => Some(i),
                Entry::Reaction(r) => r.item(),
            };
            let equipped = signed_in
                && had
                && item.is_some_and(|i| {
                    i.category != Category::Reaction
                        && online.effective(&wallet, i.category) == i.code
                });
            let rarity = e.rarity();
            rarity_card(ui, c, rarity, equipped, time);
            let inner = c.w - 24.0;
            let pic = Rect::new(c.x + 12.0, c.y + 30.0, inner, (c.h - 136.0).max(36.0));
            match e {
                Entry::Item(i) => crate::shop::preview(ui, i, pic),
                Entry::Reaction(r) => {
                    rrect(pic, 8.0, Color::new(0.0, 0.0, 0.0, 0.25));
                    let s = pic.w.min(pic.h) * 0.62;
                    let center = vec2(pic.x + pic.w / 2.0, pic.y + pic.h / 2.0);
                    draw_circle(center.x, center.y, s * 0.62, Color::new(1.0, 0.85, 0.5, 0.06));
                    if had {
                        crate::reactions::draw(r.code, center, s, crate::paint::alpha());
                    } else {
                        crate::reactions::draw_locked(r.code, center, s);
                    }
                }
            }
            if !had {
                // Still to get: dimmed, with a lock.
                rrect(pic, 8.0, Color::new(0.04, 0.03, 0.02, 0.55));
                crate::icons::draw(
                    Icon::Lock,
                    vec2(pic.x + pic.w / 2.0, pic.y + pic.h / 2.0),
                    22.0,
                    MUTED,
                );
            }
            // The name, and how (and when) it was got.
            let name_y = pic.y + pic.h + 10.0;
            let secret = match e {
                Entry::Item(Item { price: Price::Achievement(a), .. }) => {
                    !had && snook_legend_proto::ACHIEVEMENTS
                        .iter()
                        .any(|x| x.code == *a && x.secret)
                }
                Entry::Reaction(r) => !had && r.achievement().is_some_and(|a| a.secret),
                _ => false,
            };
            let name = if secret { "???" } else { e.name() };
            let (name, size) = ui.fitted(name, 15.0, inner, true);
            ui.text(&name, c.x + 12.0, name_y, size, if had { CREAM } else { MUTED }, true);
            let (line, at) = if had { held(e, online) } else { (how_to_get(e, online), 0) };
            // As many lines as there is room for above the buttons (the
            // rest under the pointer), the date after them.
            let lines = ui.wrap(&line, inner, 12.0);
            let mut ly = name_y + 22.0;
            let room = ((c.y + c.h - 50.0 - ly) / 15.0).floor().max(1.0) as usize;
            let dated = at > 0 && room > lines.len().min(2);
            let shown = room.min(2).min(lines.len());
            for (k, l) in lines.iter().take(shown).enumerate() {
                let cut = k + 1 == shown && lines.len() > shown;
                let l = if cut {
                    ui.fitted(&format!("{l}…"), 12.0, inner, false).0
                } else {
                    l.clone()
                };
                ui.text(&l, c.x + 12.0, ly, 12.0, if had { GOLD } else { MUTED }, false);
                ly += 15.0;
            }
            if dated {
                ui.text(&crate::profile::date(at), c.x + 12.0, ly, 11.5, MUTED, false);
            }
            let info = Rect::new(c.x, name_y, c.w, 54.0);
            if ui.hovering(info) && (lines.len() > shown || (at > 0 && !dated)) {
                tip = Some((line.clone(), info));
            }
            // The actions along the bottom.
            let b = Rect::new(c.x + 12.0, c.y + c.h - 46.0, inner, 34.0);
            let mut right = b.x + b.w;
            if let Some(i) = item.filter(|i| has_3d_preview(i)) {
                right -= 34.0;
                let eye = Rect::new(right, b.y, 34.0, 34.0);
                if ui.icon_button(eye, Icon::Eye, t("Look at it closely"), true) && !modal {
                    action = CollectionAction::Preview(i.code);
                }
                right -= 4.0;
            }
            let b = Rect::new(b.x, b.y, right - b.x, b.h);
            if equipped {
                button(ui, b, Kind::Disabled, Some(Icon::Check), t("In use"), 14.0);
            } else if had && e.category() == Category::Reaction {
                button(ui, b, Kind::Disabled, Some(Icon::Check), t("In your reactions"), 13.0);
            } else if had {
                if button(ui, b, Kind::Normal, None, t("Use"), 14.0)
                    && !modal
                    && let Some(i) = item
                {
                    online.send(ClientMsg::Equip { code: i.code.to_string() });
                }
            } else {
                match item.map(|i| i.price) {
                    Some(Price::Coins(_) | Price::Vip | Price::Pack) => {
                        if button(ui, b, Kind::Normal, Some(Icon::Shop), t("In the shop"), 13.0)
                            && !modal
                        {
                            action = CollectionAction::Shop(item.map(|i| i.code));
                        }
                    }
                    _ => {
                        button(ui, b, Kind::Disabled, Some(Icon::Lock), t("Not yet yours"), 13.0);
                    }
                }
            }
        }
        grid.end(ui, &mut self.grid);
        ui.pop_fx(fx);
        if let Some((text, r)) = tip {
            ui.tooltip(&text, r);
        }
        action
    }
}

/// The items listed in the server's collection that this game knows (a
/// newer server may send codes it doesn't).
#[cfg(test)]
fn known(items: &[OwnedItem]) -> usize {
    items
        .iter()
        .filter(|o| {
            shop::item(&o.code).is_some()
                || o.code
                    .strip_prefix(ITEM_PREFIX)
                    .is_some_and(|r| snook_legend_proto::reaction(r).is_some())
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_are_worded() {
        assert_eq!(coins_text(15_000), "15\u{2009}000");
        assert_eq!(coins_text(600), "600");
        assert_eq!(coins_text(1_234_567), "1\u{2009}234\u{2009}567");
        let o = Origin::Tournament { name: "Winter Cup 2026".into(), place: 1 };
        assert!(origin_text(&o).contains("Winter Cup 2026"));
        assert!(origin_text(&Origin::Gift { from: "Ana".into() }).contains("Ana"));
        assert!(origin_text(&Origin::VipMonth { month: "2026-12".into() }).contains("12/2026"));
    }

    #[test]
    fn every_category_has_cards_and_codes_are_found() {
        for c in Category::ALL {
            assert!(!entries(c).is_empty(), "{c:?}");
        }
        assert_eq!(entries(Category::Reaction).len(), REACTIONS.len());
        let items = vec![
            OwnedItem { code: "reaction_lion".into(), origin: Origin::Unknown, acquired_at: 0 },
            OwnedItem { code: "cue_emperor".into(), origin: Origin::Unknown, acquired_at: 0 },
            OwnedItem {
                code: "cue_from_the_future".into(),
                origin: Origin::Unknown,
                acquired_at: 0,
            },
        ];
        assert_eq!(known(&items), 2);
        assert_eq!(rare_cue("cue_emperor"), Some(Rarity::Legendary));
        assert_eq!(rare_cue("cue_classic"), None);
    }
}
