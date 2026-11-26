//! Presents from friends: the "You received a gift!" dialog (one after
//! another, until seen), thanking the giver, and the history of presents
//! received and given (on one's own profile).

use macroquad::prelude::*;
use snook_legend_proto::shop::{self, VIP_GIFT};
use snook_legend_proto::{ClientMsg, GiftInfo};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use super::Online;
use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::ranking::{empty_state, loading, text_y};
use crate::ui::{BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines};

/// What the game keeps about presents.
#[derive(Default)]
pub(super) struct Gifts {
    /// Received and not seen yet, oldest first (the dialog shows the first).
    pub(super) new: Vec<GiftInfo>,
    /// The player put the dialog off ("Later"): it waits for the profile.
    pub(super) later: bool,
    /// Presents thanked for in this session.
    pub(super) thanked: Vec<i64>,
    /// The history as the server last sent it (None: not yet).
    pub(super) history: Option<Vec<GiftInfo>>,
    /// The history was asked for and has not come yet.
    asked: bool,
    /// The present the dialog shows, and since when (a key or a click meant
    /// for something else just then doesn't close it unseen).
    shown: Option<(i64, f64)>,
    /// Items earned and not shown yet (a level reached, an achievement):
    /// the item's code and the achievement it came with, oldest first.
    pub(super) rewards: Vec<(&'static str, &'static str)>,
    /// Since when the first of them is on screen.
    reward_shown: Option<f64>,
}

impl Gifts {
    /// Presents arrived (after signing in, or one now).
    pub(super) fn arrived(&mut self, gifts: Vec<GiftInfo>) {
        for g in gifts {
            if !self.new.iter().any(|n| n.id == g.id) {
                self.new.push(g);
            }
        }
        self.later = false;
        // The history is out of date.
        self.history = None;
        self.asked = false;
    }

    /// The first present was seen: on to the next.
    fn done(&mut self) -> Option<i64> {
        (!self.new.is_empty()).then(|| self.new.remove(0).id)
    }
}

/// What a present is, in a few words: the item's name, or "A month of VIP".
fn gift_name(code: &str) -> String {
    if code == VIP_GIFT {
        return t("A month of VIP").to_string();
    }
    shop::item(code).map(|i| t(i.name).to_string()).unwrap_or_else(|| code.to_string())
}

/// The present's picture, `age` seconds after it was first shown: it comes
/// out of its box (see `reveal`), the shop's picture, or a crown for VIP.
fn picture(ui: &Ui, code: &str, r: Rect, age: f32) {
    crate::reveal::item(ui, code, r, age);
}

impl Online {
    /// Presents are waiting to be seen (a dot on the profile's buttons).
    pub fn gift_badge(&self) -> bool {
        !self.gifts.new.is_empty()
    }

    /// The gift dialog is on screen (the screen under it ignores Esc).
    pub fn gift_dialog_open(&self) -> bool {
        // Development aid (screenshots): SNOOK_LEGEND_QUIET_REWARDS keeps
        // the rewards and presents off the screens they would cover.
        if crate::dev::flag("SNOOK_LEGEND_QUIET_REWARDS") {
            return false;
        }
        ((!self.gifts.new.is_empty() && !self.gifts.later) || !self.gifts.rewards.is_empty())
            && self.signed_in()
    }

    /// An achievement was unlocked: the items that come with it are shown
    /// to the player (they only turned up in the shop, unannounced).
    pub(super) fn earned(&mut self, achievement: &str) {
        for item in shop::ITEMS {
            if let shop::Price::Achievement(code) = item.price
                && code == achievement
                && !self.gifts.rewards.iter().any(|(i, _)| *i == item.code)
            {
                self.gifts.rewards.push((item.code, code));
            }
        }
    }

    /// "Congratulations!": the first item earned and not seen yet.
    fn draw_reward_dialog(&mut self, ui: &mut Ui) {
        let (code, achievement) = self.gifts.rewards[0];
        let more = self.gifts.rewards.len() - 1;
        let Some(item) = shop::item(code) else {
            self.gifts.rewards.remove(0);
            return;
        };
        let (sw, sh) = (crate::ui::vw(), crate::ui::vh());
        let w = 460.0f32.min(sw - 32.0);
        let pad = 24.0;
        let inner = w - pad * 2.0;
        let usable = self.can_use(item)
            && self.wallet.as_ref().is_some_and(|w| self.effective(w, item.category) != item.code);
        let pic_h = (sh * 0.2).clamp(80.0, 130.0);
        let h = pad + 32.0 + 10.0 + 22.0 + 14.0 + pic_h + 12.0 + 26.0 + 20.0 + 18.0 + 38.0 + pad;
        let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
        let _modal = ui.modal_scope(crate::ui::motion::id("gifts-122"), 0.6);
        ui.panel(r);
        rrect_lines(r, 8.0, 2.0, GOLD);
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        let cx = r.x + w / 2.0;
        let mut y = r.y + pad;
        let title = t("Congratulations!");
        let (title, size) = ui.fitted(title, 24.0, inner - 34.0, true);
        let tw_ = ui.text_size(&title, size, true).x + 34.0;
        crate::icons::draw(Icon::Trophy, vec2(cx - tw_ / 2.0 + 12.0, y + 16.0), 24.0, GOLD);
        ui.text(&title, cx - tw_ / 2.0 + 34.0, text_y(y + 16.0, size), size, GOLD, true);
        y += 32.0 + 10.0;
        // What it came with: "You earned a new item: Level 10".
        let reason = snook_legend_proto::ACHIEVEMENTS
            .iter()
            .find(|a| a.code == achievement)
            .map(|a| t(a.title).to_string());
        let line = match reason {
            Some(title) => tw("You earned a new item: {title}", &[("title", &title)]),
            None => t("You earned a new item!").to_string(),
        };
        let (line, size) = ui.fitted(&line, 15.0, inner, false);
        ui.text_centered(&line, cx, y + 2.0, size, MUTED, false);
        y += 22.0 + 14.0;
        let age = self.gifts.reward_shown.map_or(0.0, |at| (get_time() - at) as f32);
        picture(ui, code, Rect::new(r.x + pad + 20.0, y, inner - 40.0, pic_h), age);
        y += pic_h + 12.0;
        let name = gift_name(code);
        let (name, size) = ui.fitted(&name, 20.0, inner, true);
        ui.text_centered(&name, cx, y, size, CREAM, true);
        y += 26.0;
        ui.text_centered(crate::shop::category_name(item.category), cx, y, 13.0, MUTED, false);
        y += 20.0 + 18.0;
        let gap = 12.0;
        let half = (inner - gap) / 2.0;
        let mut seen = false;
        let ok_r = if usable {
            if ui.button(Rect::new(r.x + pad, y, half, 38.0), t("Use it now"), true) {
                self.send(ClientMsg::Equip { code: code.to_string() });
                seen = true;
            }
            Rect::new(r.x + pad + half + gap, y, half, 38.0)
        } else {
            Rect::new(r.x + pad, y, inner, 38.0)
        };
        let ok_label =
            if more > 0 { tw("Next ({n} more)", &[("n", &more)]) } else { t("OK").to_string() };
        let ok = ui.primary_button(ok_r, &ok_label);
        let now = get_time();
        let since = *self.gifts.reward_shown.get_or_insert(now);
        // (As for a present: a key or a press meant for what was there
        // before isn't taken for OK.)
        let settled = now - since > 1.0;
        let enter = ui.enter();
        let away = ui.dismissed(r);
        if ok || seen || (settled && (enter || crate::ui::esc_pressed() || away)) {
            self.gifts.rewards.remove(0);
            self.gifts.reward_shown = None;
        }
    }

    /// Shows the presents put off with "Later" again (the own profile).
    pub fn show_gifts_again(&mut self) {
        self.gifts.later = false;
    }

    /// Asks for the history of presents (the profile's Gifts pane).
    pub fn request_gifts(&mut self) {
        self.gifts.asked = true;
        self.send(ClientMsg::Gifts);
    }

    fn thank(&mut self, id: i64) {
        if !self.gifts.thanked.contains(&id) {
            self.gifts.thanked.push(id);
            self.send(ClientMsg::ThankGift { id });
        }
    }

    fn thanked(&self, g: &GiftInfo) -> bool {
        g.thanked || self.gifts.thanked.contains(&g.id)
    }

    /// The first unseen present, over whatever screen is up (not during a
    /// game). Returns true while it is shown.
    pub fn draw_gift_dialog(&mut self, ui: &mut Ui) -> bool {
        if !self.gift_dialog_open() {
            return false;
        }
        // What was earned first; then the presents.
        if !self.gifts.rewards.is_empty() {
            self.draw_reward_dialog(ui);
            return true;
        }
        let g = self.gifts.new[0].clone();
        let more = self.gifts.new.len() - 1;
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        let w = 460.0f32.min(sw - 32.0);
        let pad = 24.0;
        let inner = w - pad * 2.0;
        let message = if g.message.is_empty() {
            Vec::new()
        } else {
            ui.wrap(&format!("“{}”", g.message), inner - 24.0, 15.0)
        };
        // Usable at once: an item it can wear now (not what it wears already).
        let usable = shop::item(&g.code).is_some_and(|item| {
            self.can_use(item)
                && self
                    .wallet
                    .as_ref()
                    .is_some_and(|w| self.effective(w, item.category) != item.code)
        });
        let pic_h = (sh * 0.2).clamp(80.0, 130.0);
        // Sized to what it shows: title, from, picture, name and kind, the
        // message, the buttons (two rows).
        let msg_h =
            if message.is_empty() { 0.0 } else { 12.0 + message.len() as f32 * 20.0 + 12.0 };
        let h = pad
            + 32.0
            + 10.0
            + 22.0
            + 14.0
            + pic_h
            + 12.0
            + 26.0
            + 20.0
            + msg_h
            + 18.0
            + 38.0
            + 10.0
            + 38.0
            + pad;
        let r = Rect::new((sw - w) / 2.0, ((sh - h) / 2.0).max(8.0), w, h);
        let _modal = ui.modal_scope(crate::ui::motion::id("gifts-257"), 0.6);
        ui.panel(r);
        rrect_lines(r, 8.0, 2.0, GOLD);
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        let cx = r.x + w / 2.0;
        let mut y = r.y + pad;
        // A gift icon and the title, centred together.
        let title = t("You received a gift!");
        let (title, size) = ui.fitted(title, 24.0, inner - 34.0, true);
        let tw_ = ui.text_size(&title, size, true).x + 34.0;
        crate::icons::draw(Icon::Gift, vec2(cx - tw_ / 2.0 + 12.0, y + 16.0), 24.0, GOLD);
        ui.text(&title, cx - tw_ / 2.0 + 34.0, text_y(y + 16.0, size), size, GOLD, true);
        y += 32.0 + 10.0;
        // From whom: "From [flag] Bruno".
        let from_label = t("From");
        let lw = ui.text_size(from_label, 15.0, false).x;
        let giver = if g.from.is_empty() { t("someone").to_string() } else { g.from.clone() };
        let flag_w = if crate::flags::name(&g.from_country).is_empty() || g.from.is_empty() {
            0.0
        } else {
            crate::flags::width(12.0) + 6.0
        };
        let nw = ui.text_size(&giver, 15.0, true).x.min(inner - lw - 8.0 - flag_w);
        let x0 = cx - (lw + 8.0 + flag_w + nw) / 2.0;
        ui.text(from_label, x0, y + 2.0, 15.0, MUTED, false);
        ui.name_with_flag(
            &giver,
            &g.from_country,
            CREAM,
            x0 + lw + 8.0,
            y + 2.0,
            15.0,
            nw + flag_w,
        );
        y += 22.0 + 14.0;
        let age = match self.gifts.shown {
            Some((id, at)) if id == g.id => (get_time() - at) as f32,
            _ => 0.0,
        };
        picture(ui, &g.code, Rect::new(r.x + pad + 20.0, y, inner - 40.0, pic_h), age);
        y += pic_h + 12.0;
        let name = gift_name(&g.code);
        let (name, size) = ui.fitted(&name, 20.0, inner, true);
        ui.text_centered(&name, cx, y, size, CREAM, true);
        y += 26.0;
        let kind = match shop::item(&g.code) {
            Some(item) => crate::shop::category_name(item.category).to_string(),
            None => t("VIP").to_string(),
        };
        ui.text_centered(&kind, cx, y, 13.0, MUTED, false);
        y += 20.0;
        if !message.is_empty() {
            let box_ = Rect::new(r.x + pad, y + 6.0, inner, message.len() as f32 * 20.0 + 16.0);
            rrect(box_, 8.0, Color::new(1.0, 1.0, 1.0, 0.04));
            rrect_lines(box_, 8.0, 1.0, Color::new(BORDER.r, BORDER.g, BORDER.b, 0.7));
            for (k, line) in message.iter().enumerate() {
                let ly = box_.y + 8.0 + k as f32 * 20.0;
                ui.text_centered(line, cx, ly, 15.0, crate::online::STATUS_COLOR, false);
            }
            y += msg_h;
        }
        y += 18.0;
        // Thank (and use it now) on the first row; later and OK on the second.
        let gap = 12.0;
        let half = (inner - gap) / 2.0;
        let thanked = self.thanked(&g);
        let thank_label = if thanked { t("Thanked") } else { t("Thank") };
        let can_thank = !thanked && !g.from.is_empty();
        let thank_r = if usable {
            Rect::new(r.x + pad, y, half, 38.0)
        } else {
            Rect::new(r.x + pad, y, inner, 38.0)
        };
        if ui.button(thank_r, thank_label, can_thank) {
            self.thank(g.id);
        }
        let mut seen = false;
        if usable {
            let use_r = Rect::new(r.x + pad + half + gap, y, half, 38.0);
            if ui.button(use_r, t("Use it now"), true) {
                self.send(ClientMsg::Equip { code: g.code.clone() });
                seen = true;
            }
        }
        y += 38.0 + 10.0;
        let later = ui.button(Rect::new(r.x + pad, y, half, 38.0), t("Later"), true);
        let ok_label =
            if more > 0 { tw("Next ({n} more)", &[("n", &more)]) } else { t("OK").to_string() };
        let ok = ui.primary_button(Rect::new(r.x + pad + half + gap, y, half, 38.0), &ok_label);
        let now = get_time();
        let since = match self.gifts.shown {
            Some((id, at)) if id == g.id => at,
            _ => {
                self.gifts.shown = Some((g.id, now));
                now
            }
        };
        // Enter, Esc or a press around it only after a moment: one meant
        // for what was there before (the chat's line) isn't taken for OK.
        let settled = now - since > 1.0;
        let enter = ui.enter();
        // A press on the dimmed screen around it is OK, as Esc is.
        let away = ui.dismissed(r);
        if ok || seen || (settled && (enter || crate::ui::esc_pressed() || away)) {
            if let Some(id) = self.gifts.done() {
                self.send(ClientMsg::GiftsSeen { ids: vec![id] });
            }
        } else if later {
            self.gifts.later = true;
        }
        true
    }

    /// The presents received and given, newest first, with thanks for the
    /// received ones (the own profile's Gifts pane).
    pub fn draw_gift_history(&mut self, ui: &mut Ui, list: Rect, first: &mut crate::ui::Scroll) {
        let Some(gifts) = self.gifts.history.clone() else {
            if !self.gifts.asked {
                self.request_gifts();
            }
            loading(ui, list, t("Loading…"));
            return;
        };
        if gifts.is_empty() {
            empty_state(
                ui,
                list,
                Icon::Gift,
                t("No gifts yet. Friends can give you items from the shop."),
            );
            return;
        }
        let me = self.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
        let row_h = 46.0;
        let view = ui.list_begin(list, first, gifts.len(), row_h);
        let scrolls = view.scrolls();
        let mut thank = None;
        for k in view.rows() {
            let g = &gifts[k];
            let w = list.w - if scrolls { 12.0 } else { 0.0 };
            let r = Rect::new(list.x, view.y(k), w, row_h - 6.0);
            let received = g.to.eq_ignore_ascii_case(&me);
            let unseen = received && self.gifts.new.iter().any(|n| n.id == g.id);
            rrect(r, 6.0, Color::new(1.0, 1.0, 1.0, if unseen { 0.07 } else { 0.035 }));
            if unseen {
                rrect_lines(r, 6.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.5));
            }
            let mid = r.y + r.h / 2.0;
            // Received (a gift, gold) or given (an arrow out, muted).
            let badge = Rect::new(r.x + 8.0, mid - 12.0, 24.0, 24.0);
            let color = if received { GOLD } else { MUTED };
            rrect(badge, 5.0, Color::new(color.r, color.g, color.b, 0.16));
            let icon = if received { Icon::Gift } else { Icon::Play };
            crate::icons::draw(icon, vec2(badge.x + 12.0, mid), 14.0, color);
            // The date on the right; Thank left of it (received, not yet).
            let d = crate::profile::date(g.at);
            let dw = ui.text_size(&d, 12.0, false).x;
            let rx = r.x + r.w - 12.0;
            ui.text(&d, rx - dw, text_y(mid, 12.0), 12.0, MUTED, false);
            let mut right = rx - dw - 12.0;
            if received && !g.from.is_empty() {
                if self.thanked(g) {
                    let c = vec2(right - 8.0, mid);
                    crate::icons::draw(Icon::Check, c, 13.0, MUTED);
                    let tr = Rect::new(right - 18.0, mid - 10.0, 20.0, 20.0);
                    if ui.hovering(tr) {
                        ui.tooltip(t("Thanked"), tr);
                    }
                    right -= 26.0;
                } else {
                    let label = t("Thank");
                    let bw = ui.text_size(label, 13.0, true).x + 24.0;
                    let b = Rect::new(right - bw, mid - 14.0, bw, 28.0);
                    if ui.button(b, label, true) {
                        thank = Some(g.id);
                    }
                    right = b.x - 10.0;
                }
            } else if !received && g.thanked {
                // They said thank you.
                crate::icons::draw(Icon::Check, vec2(right - 8.0, mid), 13.0, GOLD);
                let tr = Rect::new(right - 18.0, mid - 10.0, 20.0, 20.0);
                if ui.hovering(tr) {
                    ui.tooltip(t("They thanked you"), tr);
                }
                right -= 26.0;
            }
            let x = r.x + 42.0;
            let room = right - x;
            let (who, country) =
                if received { (&g.from, &g.from_country) } else { (&g.to, &g.to_country) };
            let who = if who.is_empty() { t("someone").to_string() } else { who.clone() };
            let label = if received { t("From") } else { t("To") };
            let lw = ui.text_size(label, 13.0, false).x;
            ui.text(label, x, r.y + 5.0, 13.0, MUTED, false);
            ui.name_with_flag(&who, country, CREAM, x + lw + 6.0, r.y + 4.0, 14.0, room - lw - 6.0);
            let mut sub = gift_name(&g.code);
            if !g.message.is_empty() {
                sub = format!("{sub}  ·  “{}”", g.message);
            }
            let sub = ui.ellipsize(&sub, room, 12.0);
            ui.text(&sub, x, r.y + 23.0, 12.0, MUTED, false);
        }
        ui.scroll_end(view, first);
        if let Some(id) = thank {
            self.thank(id);
        }
    }

    /// Presents arrived while signed in (after signing in, or now).
    pub(super) fn gifts_arrived(&mut self, gifts: Vec<GiftInfo>, in_match: bool) {
        if in_match {
            // Not over the table: the dialog waits for the lounge.
            if let Some(g) = gifts.last() {
                let text = tw(
                    "{from} gave you {item}! It's waiting for you in the lounge.",
                    &[("from", &g.from), ("item", &gift_name(&g.code))],
                );
                self.toast(text.clone(), true);
                self.attention = Some(text);
            }
        } else if let Some(g) = gifts.last() {
            self.attention = Some(tw(
                "{from} gave you {item}!",
                &[("from", &g.from), ("item", &gift_name(&g.code))],
            ));
        }
        self.gifts.arrived(gifts);
    }

    /// The giver hears the thanks.
    pub(super) fn gift_thanked(&mut self, by: &str, code: &str) {
        let text =
            tw("{name} thanked you for {item}.", &[("name", &by), ("item", &gift_name(code))]);
        self.toast(text.clone(), true);
        self.attention = Some(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gift(id: i64) -> GiftInfo {
        GiftInfo {
            id,
            from: "ana".into(),
            from_country: "BR".into(),
            to: "bia".into(),
            to_country: "PT".into(),
            code: VIP_GIFT.into(),
            message: String::new(),
            at: id,
            seen: false,
            thanked: false,
        }
    }

    #[test]
    fn presents_queue_once_each_and_come_back_after_later() {
        let mut g = Gifts::default();
        g.arrived(vec![gift(1), gift(2)]);
        g.later = true;
        // Sent again after signing in: no duplicates, and shown again.
        g.arrived(vec![gift(1), gift(2), gift(3)]);
        assert!(!g.later);
        assert_eq!(g.new.iter().map(|x| x.id).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(g.done(), Some(1));
        assert_eq!(g.done(), Some(2));
        assert_eq!(g.done(), Some(3));
        assert_eq!(g.done(), None);
    }
}
