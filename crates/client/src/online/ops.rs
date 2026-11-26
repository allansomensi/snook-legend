//! What the staff run from the portal, as the player meets it: sales in the
//! shop, auctions of rare items, a maintenance announced ahead (a banner
//! with a countdown on the main menu and in the lounge), the daily bonus's
//! streak and the supporters' credits.

use macroquad::prelude::*;
use snook_legend_proto::ops::{AuctionInfo, AuctionState, Promotion, Supporter};
use snook_legend_proto::shop::{self, Item};
use snook_legend_proto::{ClientMsg, ServerMsg};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use super::Online;
use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::ranking::{empty_state, loading, text_y};
use crate::ui::{ALERT, BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

/// What the game keeps of the portal's doings.
#[derive(Default)]
pub struct OpsView {
    /// The shop's promotions running and coming.
    pub promotions: Vec<Promotion>,
    /// The auctions open, coming and lately over.
    pub auctions: Vec<AuctionInfo>,
    /// A maintenance planned: (when, minutes, the staff's message).
    pub maintenance: Option<(i64, u32, String)>,
    /// The supporters' credits (None: not asked yet).
    pub supporters: Option<Vec<Supporter>>,
    /// The auctions' and the credits' pages are open.
    pub auctions_open: bool,
    pub supporters_open: bool,
    /// The auction picked to bid on, and the amount typed.
    selected: Option<i64>,
    bid: String,
    /// How far the supporters' list is scrolled.
    first: crate::ui::Scroll,
}

impl OpsView {
    /// Another account signs in: its auctions and credits aren't this one's.
    pub fn forget_account(&mut self) {
        self.auctions.clear();
        self.supporters = None;
        self.auctions_open = false;
        self.supporters_open = false;
        self.selected = None;
        self.bid.clear();
    }

    /// Percent off `item` now (0: none).
    pub fn discount(&self, item: &Item) -> u32 {
        snook_legend_proto::ops::discount(&self.promotions, item, crate::game::unix_now())
    }

    /// What `item` costs now in coins (None: not sold for coins).
    pub fn price(&self, item: &Item) -> Option<u32> {
        snook_legend_proto::ops::price_now(&self.promotions, item, crate::game::unix_now())
    }

    /// A running promotion features `item` (shown first, highlighted).
    pub fn featured(&self, item: &Item) -> bool {
        snook_legend_proto::ops::featured(&self.promotions, item, crate::game::unix_now())
    }

    /// The promotions running now.
    pub fn running(&self) -> Vec<&Promotion> {
        let now = crate::game::unix_now();
        self.promotions.iter().filter(|p| p.is_on(now)).collect()
    }
}

/// A number with its thousands apart ("3,400").
pub fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// How long until something, in the largest units that say it ("1 d 4 h",
/// "3 h 20 min", "12 min", "45 s").
pub fn time_left(secs: i64) -> String {
    let s = secs.max(0);
    let (d, h, m) = (s / 86_400, s % 86_400 / 3600, s % 3600 / 60);
    if d > 0 {
        tw("{d} d {h} h", &[("d", &d), ("h", &h)])
    } else if h > 0 {
        tw("{h} h {m} min", &[("h", &h), ("m", &m)])
    } else if m > 0 {
        tw("{m} min", &[("m", &m)])
    } else {
        tw("{s} s", &[("s", &s)])
    }
}

impl Online {
    /// A message about promotions, auctions, maintenance, the credits or
    /// the daily streak.
    pub(super) fn ops_message(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::Promotions { list } => self.ops.promotions = list,
            ServerMsg::Auctions { list } => self.ops.auctions = list,
            ServerMsg::AuctionUpdated { auction } => {
                match self.ops.auctions.iter_mut().find(|a| a.id == auction.id) {
                    Some(a) => *a = auction,
                    None => self.ops.auctions.push(auction),
                }
                self.ops.auctions.retain(|a| a.state != AuctionState::Cancelled);
            }
            ServerMsg::Maintenance { at, minutes, message } => {
                self.ops.maintenance = (at > 0).then_some((at, minutes, message));
            }
            ServerMsg::Supporters { list } => self.ops.supporters = Some(list),
            ServerMsg::DailyStreak { day, coins, tomorrow } => {
                let text = tw(
                    "Day {d} in a row: +{n} coins. Tomorrow: +{m}.",
                    &[("d", &day), ("n", &coins), ("m", &tomorrow)],
                );
                self.toast(text, true);
            }
            _ => {}
        }
    }

    /// Opens the auctions (asking the server for them).
    pub fn open_auctions(&mut self) {
        self.send(ClientMsg::Auctions);
        self.ops.auctions_open = true;
        self.ops.selected = None;
    }

    /// Opens the supporters' credits.
    pub fn open_supporters(&mut self) {
        self.send(ClientMsg::Supporters);
        self.ops.supporters_open = true;
        self.ops.first.reset();
    }

    /// A page of this module is open (it takes the input).
    pub fn ops_page_open(&self) -> bool {
        self.ops.auctions_open || self.ops.supporters_open
    }

    /// The maintenance banner (on the main menu and in the lounge: `home`),
    /// and the auctions' or the credits' page when open.
    pub fn draw_ops(&mut self, ui: &mut Ui, home: bool) {
        if home {
            self.draw_maintenance(ui);
        }
        if self.ops.auctions_open {
            self.draw_auctions(ui);
        } else if self.ops.supporters_open {
            self.draw_supporters(ui);
        }
    }

    /// How many auctions are open (the shop's tab says so).
    pub fn open_auctions_count(&self) -> usize {
        self.ops.auctions.iter().filter(|a| a.state == AuctionState::Open).count()
    }

    /// "The server stops for maintenance in 25 min": a banner across the
    /// top, red in the last five minutes.
    fn draw_maintenance(&self, ui: &mut Ui) {
        let Some((at, minutes, message)) = &self.ops.maintenance else { return };
        let left = at - crate::game::unix_now();
        if left < -60 {
            return;
        }
        let mut text = if left > 0 {
            tw(
                "Maintenance in {t}: the server stops for about {m} min.",
                &[("t", &time_left(left)), ("m", minutes)],
            )
        } else {
            t("Maintenance: the server is stopping. Games in progress go on.").to_string()
        };
        if !message.is_empty() {
            text = format!("{text} {message}");
        }
        let sw = crate::ui::vw();
        let w = (sw - 32.0).min(640.0);
        let lines = ui.wrap(&text, w - 52.0, 14.0);
        let h = 16.0 + lines.len().min(3) as f32 * 19.0;
        let r = Rect::new((sw - w) / 2.0, 56.0, w, h);
        let urgent = left < 300;
        let tint = if urgent { ALERT } else { GOLD };
        rrect(r, 10.0, Color::new(0.08, 0.05, 0.03, 0.92));
        rrect_lines(r, 10.0, 1.5, tint);
        crate::icons::draw(Icon::Hourglass, vec2(r.x + 20.0, r.y + h / 2.0), 16.0, tint);
        let mut y = r.y + 8.0;
        for line in lines.iter().take(3) {
            ui.text(line, r.x + 38.0, y, 14.0, CREAM, false);
            y += 19.0;
        }
        ui.block(r);
    }

    fn draw_auctions(&mut self, ui: &mut Ui) {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let _modal = ui.modal_scope(crate::ui::motion::id("ops-209"), 0.6);
        let w = (sw - 32.0).min(760.0);
        let h = (sh - 32.0).min(600.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        let pad = 22.0;
        let x = r.x + pad;
        let inner = w - 2.0 * pad;
        crate::icons::draw(Icon::Hourglass, vec2(x + 12.0, r.y + 34.0), 22.0, GOLD);
        ui.text(t("Auctions"), x + 32.0, text_y(r.y + 34.0, 22.0), 22.0, GOLD, true);
        let close = Rect::new(r.x + r.w - pad - 34.0, r.y + 17.0, 34.0, 34.0);
        if ui.icon_button(close, Icon::Close, t("Close"), true) || ui.escape() {
            self.ops.auctions_open = false;
            return;
        }
        let me = self.username.clone();
        let coins = self.wallet.as_ref().map(|w| w.coins);
        if let Some(c) = coins {
            let text = tw("Your coins: {n}", &[("n", &thousands(c))]);
            let tw_ = ui.text_size(&text, 14.0, true).x;
            ui.text(&text, close.x - 16.0 - tw_, text_y(r.y + 34.0, 14.0), 14.0, CREAM, true);
        }
        let note = t(
            "Rare items for the highest bid. Your bid is held; if someone bids more, your coins come back at once. A bid in the last two minutes gives two more.",
        );
        let mut y = r.y + 62.0;
        for line in ui.wrap(note, inner, 13.0).iter().take(3) {
            ui.text(line, x, y, 13.0, MUTED, false);
            y += 17.0;
        }
        y += 8.0;
        let footer = 58.0;
        let list = Rect::new(x, y, inner, r.y + r.h - pad - footer - y);
        let now = crate::game::unix_now();
        if self.ops.auctions.is_empty() {
            empty_state(ui, list, Icon::Hourglass, t("No auctions right now. Come back soon!"));
        }
        let row_h = 86.0;
        let rows = ((list.h + 8.0) / (row_h + 8.0)).floor().max(1.0) as usize;
        let mut ry = list.y;
        let auctions = self.ops.auctions.clone();
        for a in auctions.iter().take(rows) {
            let row = Rect::new(list.x, ry, list.w, row_h);
            let mine = a.leader.eq_ignore_ascii_case(&me) && !me.is_empty();
            let selected = self.ops.selected == Some(a.id);
            rrect(row, 10.0, Color::new(1.0, 1.0, 1.0, if selected { 0.07 } else { 0.035 }));
            rrect_lines(
                row,
                10.0,
                if mine || selected { 2.0 } else { 1.0 },
                if mine || selected { GOLD } else { BORDER },
            );
            let item = shop::item(&a.code);
            let pic = Rect::new(row.x + 10.0, row.y + 10.0, 96.0, row_h - 20.0);
            if let Some(item) = item {
                crate::shop::preview(ui, item, pic);
            }
            let tx = pic.x + pic.w + 14.0;
            let name = item.map_or(a.code.as_str(), |i| t(i.name));
            let (name, size) = ui.fitted(name, 16.0, row.w * 0.45, true);
            // The name in its rarity's colour.
            let color = item.map_or(CREAM, |i| {
                let [cr, cg, cb] = shop::rarity(i).color();
                Color::from_rgba(cr, cg, cb, 255)
            });
            ui.text(&name, tx, row.y + 16.0, size, color, true);
            let bid_line = if a.bid > 0 {
                tw("Current bid: {n} coins", &[("n", &thousands(a.bid))])
            } else {
                tw("No bids yet · from {n} coins", &[("n", &thousands(a.min_bid))])
            };
            ui.text(&bid_line, tx, row.y + 52.0, 13.0, GOLD, true);
            let who = match a.state {
                AuctionState::Ended if a.leader.is_empty() => t("Ended without bids").to_string(),
                AuctionState::Ended if mine => t("You won it!").to_string(),
                AuctionState::Ended => tw("Won by {name}", &[("name", &a.leader)]),
                AuctionState::Coming => tw("Opens in {t}", &[("t", &time_left(a.starts_at - now))]),
                _ if mine => tw("You lead · ends in {t}", &[("t", &time_left(a.ends_at - now))]),
                _ if a.leader.is_empty() => {
                    tw("Ends in {t}", &[("t", &time_left(a.ends_at - now))])
                }
                _ => tw(
                    "{name} leads · ends in {t}",
                    &[("name", &a.leader), ("t", &time_left(a.ends_at - now))],
                ),
            };
            let (who, size) = ui.fitted(&who, 13.0, row.w - (tx - row.x) - 150.0, false);
            ui.text(&who, tx, row.y + 68.0, size, MUTED, false);
            let open = a.state == AuctionState::Open && now < a.ends_at;
            let b = Rect::new(row.x + row.w - 140.0, row.y + (row_h - 36.0) / 2.0, 128.0, 36.0);
            let owned = self.wallet.as_ref().is_some_and(|w| w.owned.contains(&a.code));
            if owned && !mine {
                ui.button(b, t("Yours already"), false);
            } else if open && ui.button(b, if mine { t("Raise") } else { t("Bid") }, true) {
                self.ops.selected = Some(a.id);
                self.ops.bid = a.next_min().to_string();
            }
            ry += row_h + 8.0;
        }
        // The bid being made, along the bottom.
        let Some(a) =
            self.ops.selected.and_then(|id| self.ops.auctions.iter().find(|a| a.id == id)).cloned()
        else {
            return;
        };
        let fy = r.y + r.h - pad - 40.0;
        let least = a.next_min();
        let label = tw("Your bid (at least {n}):", &[("n", &thousands(least))]);
        let lw = ui.text_size(&label, 14.0, true).x;
        ui.text(&label, x, text_y(fy + 20.0, 14.0), 14.0, CREAM, true);
        let field = Rect::new(x + lw + 12.0, fy, 140.0, 40.0);
        ui.text_input(91, field, &mut self.ops.bid, 9);
        self.ops.bid.retain(|c| c.is_ascii_digit());
        let amount: i64 = self.ops.bid.parse().unwrap_or(0);
        let enough = coins.is_none_or(|c| {
            let back = if a.leader.eq_ignore_ascii_case(&me) { a.bid } else { 0 };
            c + back >= amount
        });
        if !enough {
            ui.text(
                t("Not enough coins."),
                field.x + field.w + 12.0,
                text_y(fy + 20.0, 13.0),
                13.0,
                ALERT,
                false,
            );
        }
        let b = Rect::new(r.x + r.w - pad - 150.0, fy, 150.0, 40.0);
        let ok = amount >= least && enough;
        let pressed = if ok {
            ui.primary_button(b, t("Place bid"))
        } else {
            ui.button(b, t("Place bid"), false)
        };
        if pressed {
            self.send(ClientMsg::Bid { id: a.id, amount });
            self.ops.selected = None;
            self.ops.bid.clear();
        }
    }

    fn draw_supporters(&mut self, ui: &mut Ui) {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let _modal = ui.modal_scope(crate::ui::motion::id("ops-355"), 0.6);
        let w = (sw - 32.0).min(560.0);
        let h = (sh - 32.0).min(600.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        let pad = 22.0;
        let x = r.x + pad;
        let inner = w - 2.0 * pad;
        crate::icons::draw(Icon::Star, vec2(x + 12.0, r.y + 34.0), 22.0, GOLD);
        ui.text(t("Supporters"), x + 32.0, text_y(r.y + 34.0, 22.0), 22.0, GOLD, true);
        let close = Rect::new(r.x + r.w - pad - 34.0, r.y + 17.0, 34.0, 34.0);
        if ui.icon_button(close, Icon::Close, t("Close"), true) || ui.escape() {
            self.ops.supporters_open = false;
            return;
        }
        let mut y = r.y + 62.0;
        let note = t("Thank you to the VIP members who keep the game going, now and before.");
        for line in ui.wrap(note, inner, 13.0).iter().take(2) {
            ui.text(line, x, y, 13.0, MUTED, false);
            y += 17.0;
        }
        y += 8.0;
        // The player's own choice, at the bottom (VIP now or before).
        let account = self.account.clone();
        let footer = if account.as_ref().is_some_and(|a| a.supporter) { 44.0 } else { 0.0 };
        let list = Rect::new(x, y, inner, r.y + r.h - pad - footer - y);
        match &self.ops.supporters {
            None => loading(ui, list, t("Loading…")),
            Some(l) if l.is_empty() => {
                empty_state(ui, list, Icon::Star, t("Nobody in the credits yet."))
            }
            Some(l) => {
                let row_h = 30.0;
                let view = ui.list_begin(list, &mut self.ops.first, l.len(), row_h);
                // Clear of the scroll bar.
                let inner = inner - if view.scrolls() { 14.0 } else { 0.0 };
                for k in view.rows() {
                    let s = &l[k];
                    let mid = view.y(k) + row_h / 2.0;
                    let mut nx = x + 4.0;
                    if s.vip {
                        crate::icons::draw(Icon::Crown, vec2(nx + 8.0, mid), 15.0, GOLD);
                    }
                    nx += 24.0;
                    ui.text(&s.username, nx, text_y(mid, 15.0), 15.0, CREAM, true);
                    let nw = ui.text_size(&s.username, 15.0, true).x;
                    if !s.country.is_empty() {
                        ui.text(&s.country, nx + nw + 10.0, text_y(mid, 12.0), 12.0, MUTED, false);
                    }
                    let since = tw(
                        "since {d} · {n} months",
                        &[("d", &crate::profile::date(s.since)), ("n", &s.months)],
                    );
                    let w_ = ui.text_size(&since, 12.0, false).x;
                    ui.text(&since, x + inner - w_, text_y(mid, 12.0), 12.0, MUTED, false);
                }
                ui.scroll_end(view, &mut self.ops.first);
            }
        }
        if let Some(a) = account.filter(|a| a.supporter) {
            let fy = r.y + r.h - pad - 26.0;
            let mut show = a.in_credits;
            if ui.checkbox(x, fy, t("Show my name in the supporters' credits"), &mut show) {
                self.send(ClientMsg::SetCredits { show });
                if let Some(acc) = self.account.as_mut() {
                    acc.in_credits = show;
                }
                // The list follows.
                self.send(ClientMsg::Supporters);
            }
        }
    }
}

/// The running promotions, as a banner over the shop's items ("Weekend
/// sale: ends in 1 d 4 h"). Returns the height used (0: none).
pub fn promotion_banner(ui: &Ui, online: &Online, area: Rect) -> f32 {
    let running = online.ops.running();
    let Some(p) = running.iter().min_by_key(|p| p.ends_at) else { return 0.0 };
    let now = crate::game::unix_now();
    let left = time_left(p.ends_at - now);
    let mut text = tw("{title}: ends in {t}", &[("title", &p.title), ("t", &left)]);
    if p.percent > 0 {
        text = format!("-{}% · {text}", p.percent);
    }
    if running.len() > 1 {
        text = format!("{text} (+{})", running.len() - 1);
    }
    let banner = Rect::new(area.x, area.y, area.w, 40.0);
    rrect(banner, 8.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.12));
    rrect_lines(banner, 8.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.5));
    crate::icons::draw(Icon::Star, vec2(banner.x + 20.0, banner.y + 20.0), 16.0, GOLD);
    let (text, size) = ui.fitted(&text, 14.0, banner.w - 50.0, true);
    ui.text(&text, banner.x + 38.0, text_y(banner.y + 20.0, size), size, CREAM, true);
    50.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_times_read_well() {
        assert_eq!(thousands(3400), "3,400");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(thousands(-1200), "-1,200");
    }
}
