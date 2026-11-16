//! A player's profile: rating, numbers, achievements and recent games, with
//! the friend, block and report actions.

use macroquad::prelude::*;
use snookered_proto::{ACHIEVEMENTS, ClientMsg, Profile, Relation, ReportReason};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::online::Online;
use crate::ranking::{PAD, caps_y, empty_state, loading, text_y, unreachable};
use crate::ui::{ALERT, BORDER, CREAM, GOLD, MUTED, Ui, rrect, rrect_lines, tier_color};

pub enum ProfileAction {
    None,
    Back,
}

/// What the right-hand side of the profile shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Pane {
    #[default]
    Games,
    Rating,
    Numbers,
}

#[derive(Default)]
pub struct ProfileScreen {
    pub name: String,
    history_first: usize,
    report: Option<(usize, String)>,
    confirm_block: bool,
    pane: Pane,
    /// The account dialog (own profile).
    account: Option<AccountForm>,
    /// Asked for while signed in: the server knew who was looking (and so
    /// sent our relation: friends, requests, blocks).
    asked_signed_in: bool,
    /// The status message being written (own profile).
    status_edit: Option<String>,
}

/// The account dialog: recovery e-mail, password, deletion.
#[derive(Default)]
struct AccountForm {
    email: String,
    email_loaded: bool,
    old_password: String,
    new_password: String,
    delete_password: String,
    confirm_delete: bool,
}

/// Where the content starts under the header (from the panel's top).
const BODY_TOP: f32 = 220.0;
/// The achievement medals: cell and gap.
const MEDAL: f32 = 44.0;
const MEDAL_GAP: f32 = 8.0;
/// The reactions: cell and gap.
const REACTION: f32 = 30.0;
const REACTION_GAP: f32 = 4.0;
const WIN: Color = Color::new(0.45, 0.95, 0.55, 1.0);

/// "3 days", "2 months", "1 year".
fn age(created_at: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = ((now - created_at).max(0) / 86_400) as u32;
    if days < 1 {
        t("joined today").to_string()
    } else if days < 60 {
        tw("member for {n} days", &[("n", &days)])
    } else if days < 730 {
        tw("member for {n} months", &[("n", &(days / 30))])
    } else {
        tw("member for {n} years", &[("n", &(days / 365))])
    }
}

pub fn date(at: i64) -> String {
    // Days since the epoch → a civil date (proleptic Gregorian).
    let days = at.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    match crate::i18n::lang() {
        crate::i18n::Lang::Portuguese | crate::i18n::Lang::Spanish => format!("{d:02}/{m:02}/{y}"),
        crate::i18n::Lang::English => format!("{y}-{m:02}-{d:02}"),
    }
}

pub fn medal_color(level: u8) -> Color {
    match level {
        0 => Color::new(0.80, 0.55, 0.33, 1.0),
        1 => Color::new(0.80, 0.82, 0.86, 1.0),
        2 => GOLD,
        _ => Color::new(0.62, 0.90, 0.98, 1.0),
    }
}

/// A grid for `n` cells of `cell` (plus `gap`) in `width`: as many columns
/// as fit, then evened out so the rows come out the same length (the last
/// one no shorter than it must be). Returns (columns, rows).
fn balanced_grid(n: usize, cell: f32, gap: f32, width: f32) -> (usize, usize) {
    let most = ((width + gap) / (cell + gap)).floor().max(1.0) as usize;
    let rows = n.div_ceil(most).max(1);
    (n.div_ceil(rows).max(1), rows)
}

/// The height of the achievements and reactions column for `width`.
fn left_height(width: f32) -> f32 {
    let (_, medal_rows) = balanced_grid(ACHIEVEMENTS.len(), MEDAL, MEDAL_GAP, width);
    let n = snookered_proto::REACTIONS.len();
    let (_, reaction_rows) = balanced_grid(n, REACTION, REACTION_GAP, width);
    // Heading and medals; a gap, heading and reactions (as `achievements` draws them).
    let medals = 30.0 + medal_rows as f32 * (MEDAL + MEDAL_GAP) - MEDAL_GAP;
    let reactions = 24.0 + 30.0 + reaction_rows as f32 * (REACTION + REACTION_GAP) - REACTION_GAP;
    medals + reactions
}

/// Regular text of up to `size` that fits `width` (not under 11).
fn fit_regular(ui: &Ui, text: &str, size: f32, width: f32) -> f32 {
    let mut s = size;
    while s > 11.0 && ui.text_size(text, s, false).x > width {
        s -= 0.5;
    }
    s
}

/// A section heading (15 px bold gold) with its middle on `mid`.
fn heading(ui: &Ui, text: &str, x: f32, mid: f32) {
    ui.text(text, x, text_y(mid, 15.0), 15.0, GOLD, true);
}

impl ProfileScreen {
    pub fn open(&mut self, name: &str, online: &mut Online) {
        self.name = name.to_string();
        self.history_first = 0;
        self.report = None;
        self.confirm_block = false;
        self.account = None;
        self.status_edit = None;
        self.pane = Pane::Games;
        self.asked_signed_in = online.signed_in();
        online.request_profile(name);
    }

    pub fn draw(&mut self, ui: &mut Ui, online: &mut Online) -> ProfileAction {
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.5));
        let w = (sw - 40.0).min(960.0);
        let half = (w - 2.0 * PAD - 24.0) / 2.0;
        // Opened before signing in (e.g. straight from a link): ask again
        // once signed in, for the friend and block buttons.
        if !self.asked_signed_in && online.signed_in() {
            self.asked_signed_in = true;
            online.send(ClientMsg::Profile { username: self.name.clone() });
        }
        let loaded =
            online.profile.clone().filter(|p| p.player.username.eq_ignore_ascii_case(&self.name));
        // The panel fits its content: the header, then the achievements and
        // reactions (the recent games scroll in the same height). Until the
        // profile comes, just room for the wait.
        let h = if loaded.is_some() { BODY_TOP + left_height(half) + PAD } else { 240.0 };
        let h = h.min(sh - 40.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        let mut action = ProfileAction::None;
        // A dialog over the page (account, report, country): Esc is its own.
        let dialog = self.account.is_some() || self.report.is_some() || crate::countries::is_open();
        if ui.icon_button(Rect::new(r.x + PAD, r.y + 20.0, 36.0, 36.0), Icon::Back, t("Back"), true)
            || (is_key_pressed(KeyCode::Escape)
                && ui.focus.is_none()
                && !dialog
                && self.status_edit.is_none())
        {
            action = ProfileAction::Back;
        }
        let Some(mut p) = loaded else {
            let area = Rect::new(r.x, r.y + 40.0, r.w, r.h - 60.0);
            match unreachable(online) {
                Some(error) => empty_state(ui, area, Icon::Network, &error),
                None => loading(ui, area, t("Loading the profile…")),
            }
            return action;
        };
        // Asked before signing in: the server didn't know who was looking.
        if online.me.as_ref().is_some_and(|me| me.username.eq_ignore_ascii_case(&p.player.username))
        {
            p.relation = Relation::Me;
        }
        // While a dialog is open, the page under it doesn't react to the
        // pointer (no hover hints through it).
        let saved = dialog.then(|| ui.suspend_input());
        self.header(ui, online, &p, r);
        let body = Rect::new(r.x + PAD, r.y + BODY_TOP, r.w - 2.0 * PAD, r.h - BODY_TOP - PAD);
        self.achievements(ui, &p, Rect::new(body.x, body.y, half, body.h));
        let right = Rect::new(body.x + half + 24.0, body.y, half, body.h);
        let panes = [t("Games"), t("Rating"), t("Numbers")];
        // As narrow as the labels allow (12 px in a 28 px control), so the
        // heading beside it keeps its room.
        let cell = panes.iter().map(|l| ui.text_size(l, 12.0, true).x + 24.0).fold(0.0, f32::max);
        let seg_w = cell * 3.0;
        let seg = Rect::new(right.x + right.w - seg_w, right.y - 4.0, seg_w, 28.0);
        if let Some(k) = ui.segmented(seg, &panes, self.pane as usize) {
            self.pane = [Pane::Games, Pane::Rating, Pane::Numbers][k];
        }
        let title = match self.pane {
            Pane::Games => t("Recent games"),
            Pane::Rating => t("Rating"),
            Pane::Numbers => t("Numbers"),
        };
        let (title, _) = ui.fitted(title, 15.0, seg.x - 12.0 - right.x, true);
        heading(ui, &title, right.x, right.y + 10.0);
        let content = Rect::new(right.x, right.y + 34.0, right.w, right.h - 34.0);
        match self.pane {
            Pane::Games => self.history(ui, online, &p, content),
            Pane::Rating => rating_chart(ui, &p, content),
            Pane::Numbers => numbers(ui, &p, content),
        }
        if let Some(saved) = saved {
            ui.resume_input(saved);
        }
        if let Some(code) = crate::countries::draw_picker(ui) {
            online.send(ClientMsg::SetCountry { country: code });
            online.request_profile(&p.player.username);
        }
        self.report_dialog(ui, online, &p);
        self.account_dialog(ui, online);
        action
    }

    fn header(&mut self, ui: &mut Ui, online: &mut Online, p: &Profile, r: Rect) {
        let player = &p.player;
        let framed = snookered_proto::shop::item(&player.frame)
            .map(|i| i.look)
            .filter(|_| player.frame != "frame_none");
        // With a frame, everything moves in a little: the frame keeps clear
        // of the back button.
        let pad = if framed.is_some() { 14.0 } else { 0.0 };
        let left = r.x + PAD + 36.0 + 16.0;
        let x = left + pad;
        // The name's middle lines up with the back button's.
        let mid = r.y + 38.0;
        let flag_h = 24.0;
        let flag_w =
            if player.country.is_empty() { 0.0 } else { crate::flags::width(flag_h) + 12.0 };
        let name_w = ui.text_size(&player.username, 30.0, true).x;
        let crown_w = if player.vip { 30.0 } else { 0.0 };
        let dot_w = if p.online { 18.0 } else { 0.0 };
        if let Some(snookered_proto::shop::Look::Frame { a, b, shine }) = framed {
            // The frame from the shop, around the flag, the name, the crown
            // and the online dot.
            let w = flag_w + name_w + crown_w + dot_w + 2.0 * pad;
            let fr = Rect::new(x - pad, mid - 24.0, w, 48.0);
            crate::shop::frame(fr, a, b, shine, get_time() as f32);
            rrect(
                Rect::new(fr.x + 3.0, fr.y + 3.0, fr.w - 6.0, fr.h - 6.0),
                10.0,
                Color::new(0.09, 0.055, 0.035, 1.0),
            );
        }
        crate::flags::draw(&player.country, x, mid - flag_h / 2.0, flag_h);
        let nx = x + flag_w;
        crate::ui::draw_name(ui, player, nx, caps_y(mid, 30.0), 30.0);
        let mut after = nx + name_w + 4.0;
        if player.vip {
            crate::icons::draw(Icon::Crown, vec2(after + 14.0, mid), 22.0, GOLD);
            after += crown_w;
        }
        if p.online {
            draw_circle(after + 9.0, mid, 5.0, Color::new(0.35, 0.85, 0.45, 1.0));
        }
        // The status message, between the name and the rating.
        let status_x = after + dot_w + 18.0 + pad;
        let status_end = if player.guest {
            r.x + r.w - PAD
        } else {
            let rw = ui.text_size(&player.rating.to_string(), 40.0, true).x;
            r.x + r.w - PAD - rw - 24.0
        };
        self.status_message(ui, online, p, status_x, status_end, mid);

        // The tier and a line about the player, under the name. The pill's
        // text and the line share a baseline.
        let tier = player.tier();
        let info_mid = r.y + 76.0;
        let mut info_x = left;
        if !player.guest {
            info_x += tier_pill(ui, t(tier.name()), info_x, info_mid, tier_color(tier)) + 10.0;
        }
        let mut parts = Vec::new();
        if let Some(rank) = p.rank {
            parts.push(tw("#{n} in the world", &[("n", &rank)]));
        }
        if !player.country.is_empty() {
            parts.push(crate::flags::name(&player.country).to_string());
        }
        if !player.guest {
            parts.push(age(p.created_at));
        }
        // Friends see when the other was last around.
        if !p.online && p.last_seen > 0 {
            parts.push(crate::online::last_seen(p.last_seen));
        }
        // The pill's 12 px text sits on `info_mid + 0.365 × 12` (see
        // `tier_pill`); this 13 px line sits on the same baseline.
        let baseline = info_mid + 0.365 * 12.0;
        ui.text(&parts.join("  ·  "), info_x, baseline - 0.8 * 13.0, 13.0, MUTED, false);

        // The rating, large, on the right, with the peak under it.
        if !player.guest {
            let rating = player.rating.to_string();
            let rw = ui.text_size(&rating, 40.0, true).x;
            let rx = r.x + r.w - PAD;
            ui.text(&rating, rx - rw, caps_y(mid, 40.0), 40.0, tier_color(tier), true);
            let peak = tw("peak {n}", &[("n", &p.stats.peak_rating)]);
            let pw = ui.text_size(&peak, 12.0, false).x;
            ui.text(&peak, rx - pw, text_y(info_mid, 12.0), 12.0, MUTED, false);
        }

        // Numbers.
        let s = &p.stats;
        let pct = (s.wins * 100).checked_div(s.games).unwrap_or(0);
        let cards = [
            (s.games.to_string(), t("rated games")),
            (s.wins.to_string(), t("wins")),
            (format!("{pct}%"), t("win rate")),
            (s.eight_wins.to_string(), t("won on the 8")),
            (s.best_streak.to_string(), t("best streak")),
            (s.tournaments_won.to_string(), t("tournaments won")),
        ];
        let gap = 12.0;
        let cw = (r.w - 2.0 * PAD - 5.0 * gap) / 6.0;
        for (k, (n, label)) in cards.iter().enumerate() {
            let c = Rect::new(r.x + PAD + k as f32 * (cw + gap), r.y + 100.0, cw, 58.0);
            rrect(c, 8.0, Color::new(1.0, 1.0, 1.0, 0.04));
            ui.text_centered(n, c.x + c.w / 2.0, c.y + 8.0, 22.0, CREAM, true);
            let size = fit_regular(ui, label, 12.0, c.w - 10.0);
            ui.text_centered(label, c.x + c.w / 2.0, c.y + 37.0, size, MUTED, false);
        }

        // Progress towards the next tier, and the actions, on one row.
        let row_mid = r.y + 186.0;
        if !player.guest {
            let next =
                snookered_proto::Tier::ALL.into_iter().find(|t| t.min_rating() > player.rating);
            let bx = r.x + PAD;
            let bw = (r.w * 0.36).min(320.0);
            match next {
                Some(next) => {
                    let from = if tier == snookered_proto::Tier::Rookie {
                        snookered_core::rating::EloConfig::default()
                            .initial_rating
                            .min(player.rating)
                    } else {
                        tier.min_rating()
                    };
                    let span = (next.min_rating() - from).max(1) as f32;
                    let k = ((player.rating - from) as f32 / span).clamp(0.0, 1.0);
                    let label = tw(
                        "{n} points to {tier}",
                        &[("n", &(next.min_rating() - player.rating)), ("tier", &t(next.name()))],
                    );
                    ui.text(&label, bx, row_mid - 17.0, 12.0, MUTED, false);
                    let bar = Rect::new(bx, row_mid + 4.0, bw, 6.0);
                    rrect(bar, 3.0, Color::new(1.0, 1.0, 1.0, 0.08));
                    if k > 0.0 {
                        rrect(
                            Rect::new(bar.x, bar.y, (bar.w * k).max(6.0), bar.h),
                            3.0,
                            tier_color(next),
                        );
                    }
                }
                None => {
                    ui.text(t("Top tier"), bx, text_y(row_mid, 13.0), 13.0, tier_color(tier), true)
                }
            }
        }

        let line_y = r.y + BODY_TOP - 14.0;
        draw_line(r.x + PAD, line_y, r.x + r.w - PAD, line_y, 1.0, BORDER);
        // Actions, from the right: the main one is green.
        let mut bx = r.x + r.w - PAD;
        let by = row_mid - 16.0;
        let mut button = |ui: &mut Ui, label: &str, enabled: bool, primary: bool| -> bool {
            let w = ui.text_size(label, 14.0, true).x + 32.0;
            bx -= w;
            let b = Rect::new(bx, by, w, 32.0);
            let hit = if primary && enabled {
                ui.primary_button(b, label)
            } else {
                ui.button(b, label, enabled)
            };
            bx -= 8.0;
            hit
        };
        let name = player.username.clone();
        if player.guest || online.lan {
            return;
        }
        match p.relation {
            Relation::Me => {
                if button(ui, t("Account"), true, false) {
                    self.account = Some(AccountForm::default());
                    online.send(ClientMsg::Account);
                }
                if button(ui, t("Change country"), true, false) {
                    crate::countries::open_at(&player.country);
                }
            }
            _ if online.me.is_none() => {}
            relation => {
                if relation != Relation::Blocked && button(ui, t("Report"), true, false) {
                    self.report = Some((0, String::new()));
                }
                let block_label = if relation == Relation::Blocked {
                    t("Unblock")
                } else if self.confirm_block {
                    t("Click again to block")
                } else {
                    t("Block")
                };
                if button(ui, block_label, true, false) {
                    if relation == Relation::Blocked {
                        online.send(ClientMsg::Unblock { username: name.clone() });
                        online.request_profile(&name);
                    } else if self.confirm_block {
                        online.send(ClientMsg::Block { username: name.clone() });
                        online.request_profile(&name);
                        self.confirm_block = false;
                    } else {
                        self.confirm_block = true;
                    }
                }
                let friend = match relation {
                    Relation::Friend => Some(t("Remove friend")),
                    Relation::RequestSent => Some(t("Request sent")),
                    Relation::RequestReceived => Some(t("Accept friend")),
                    Relation::None => Some(t("Add friend")),
                    _ => None,
                };
                if let Some(label) = friend {
                    let enabled = relation != Relation::RequestSent;
                    // Making friends is the way on; ending it is not.
                    let primary = matches!(relation, Relation::None | Relation::RequestReceived);
                    if button(ui, label, enabled, primary) {
                        let msg = match relation {
                            Relation::Friend => ClientMsg::FriendRemove { username: name.clone() },
                            Relation::RequestReceived => {
                                ClientMsg::FriendAnswer { username: name.clone(), accept: true }
                            }
                            _ => ClientMsg::FriendRequest { username: name.clone() },
                        };
                        online.send(msg);
                        online.request_profile(&name);
                    }
                }
            }
        }
    }

    /// The status message on the name's line (`x` to `end`, centred on
    /// `mid`): written by its owner here (Enter saves, Esc cancels), and
    /// cleared by the staff.
    fn status_message(
        &mut self,
        ui: &mut Ui,
        online: &mut Online,
        p: &Profile,
        x: f32,
        end: f32,
        mid: f32,
    ) {
        const FIELD: u64 = 62;
        let width = end - x;
        if width < 90.0 || online.lan || p.player.guest {
            return;
        }
        let own = p.relation == Relation::Me;
        let text = p.player.status_message.clone();
        if let Some(draft) = self.status_edit.as_mut().filter(|_| own) {
            let w = width.min(440.0);
            let field = Rect::new(x, mid - 16.0, w - 2.0 * 34.0, 32.0);
            let enter = ui.text_input(FIELD, field, draft, snookered_proto::MAX_STATUS_LEN);
            if draft.is_empty() {
                // Shown while typing too: the field was opened for it.
                let hint = ui.ellipsize(t("A line about you"), field.w - 24.0, 14.0);
                ui.text(&hint, field.x + 12.0, text_y(mid, 14.0), 14.0, MUTED, false);
            }
            let draft = draft.clone();
            let save = Rect::new(field.x + field.w + 4.0, mid - 15.0, 30.0, 30.0);
            let cancel = Rect::new(save.x + 34.0, mid - 15.0, 30.0, 30.0);
            if ui.icon_button(save, Icon::Check, t("Save (Enter)"), true) || enter {
                let clean = snookered_proto::clean_status(&draft);
                online.send(ClientMsg::SetStatusMessage { text: clean.clone() });
                // Shown at once; the lounge hears it from the server.
                if let Some(shown) = online.profile.as_mut() {
                    shown.player.status_message = clean;
                }
                self.status_edit = None;
                ui.focus = None;
            } else if ui.icon_button(cancel, Icon::Close, t("Cancel (Esc)"), true)
                || (is_key_pressed(KeyCode::Escape) && ui.focus.is_none())
            {
                self.status_edit = None;
            }
            return;
        }
        let size = 15.0;
        let ty = text_y(mid, size);
        let edit_w = if own { 30.0 } else { 0.0 };
        let staff = online.me.as_ref().map(|m| m.role).unwrap_or_default();
        // The staff may clear what players wrote (only administrators other staff's).
        let may_clear = !own
            && online.me.is_some()
            && staff.is_staff()
            && (!p.player.role.is_staff() || staff == snookered_proto::Role::Admin);
        if text.is_empty() {
            if !own {
                return;
            }
            // Nothing yet: an invitation to write one.
            let label = t("Add a status message");
            let lw = ui.text_size(label, 13.0, false).x;
            let b = Rect::new(x, mid - 14.0, (lw + 36.0).min(width), 28.0);
            let hover = ui.hovering(b);
            ui.block(b);
            if hover {
                rrect(b, 7.0, Color::new(1.0, 1.0, 1.0, 0.06));
            }
            rrect_lines(b, 7.0, 1.0, if hover { GOLD } else { BORDER });
            let color = if hover { GOLD } else { MUTED };
            crate::icons::draw(Icon::Pencil, vec2(b.x + 14.0, mid), 13.0, color);
            let label = ui.ellipsize(label, b.w - 34.0, 13.0);
            ui.text(&label, b.x + 26.0, text_y(mid, 13.0), 13.0, color, false);
            if hover && ui.clicked(b) {
                self.status_edit = Some(String::new());
                ui.focus = Some(FIELD);
            }
            return;
        }
        let clear_w = if may_clear { 30.0 } else { 0.0 };
        let room = width - edit_w - clear_w - 8.0;
        let shown = format!("“{text}”");
        let cut = ui.ellipsize(&shown, room, size);
        ui.text(&cut, x, ty, size, crate::online::STATUS_COLOR, false);
        let tw_ = ui.text_size(&cut, size, false).x;
        let area = Rect::new(x, mid - 12.0, tw_, 24.0);
        if cut != shown && ui.hovering(area) {
            ui.tooltip(&text, area);
        }
        let bx = x + tw_ + 8.0;
        if own {
            let b = Rect::new(bx, mid - 13.0, 26.0, 26.0);
            if ui.icon_button_ex(b, Icon::Pencil, t("Change your status message"), true, true) {
                self.status_edit = Some(text.clone());
                ui.focus = Some(FIELD);
            }
        } else if may_clear {
            let b = Rect::new(bx, mid - 13.0, 26.0, 26.0);
            if ui.icon_button_ex(b, Icon::Close, t("Clear the status message"), true, true) {
                online.send(ClientMsg::ClearStatusMessage { username: p.player.username.clone() });
                if let Some(shown) = online.profile.as_mut() {
                    shown.player.status_message.clear();
                }
            }
        }
    }

    fn achievements(&self, ui: &mut Ui, p: &Profile, area: Rect) {
        let got =
            ACHIEVEMENTS.iter().filter(|a| p.achievements.iter().any(|(c, _)| c == a.code)).count();
        heading(
            ui,
            &tw("Achievements {n}/{total}", &[("n", &got), ("total", &ACHIEVEMENTS.len())]),
            area.x,
            area.y + 10.0,
        );
        let (cols, rows) = balanced_grid(ACHIEVEMENTS.len(), MEDAL, MEDAL_GAP, area.w);
        let mut tip = None;
        let top = area.y + 30.0;
        for (k, a) in ACHIEVEMENTS.iter().enumerate() {
            let (col, row) = (k % cols, k / cols);
            let c = Rect::new(
                area.x + col as f32 * (MEDAL + MEDAL_GAP),
                top + row as f32 * (MEDAL + MEDAL_GAP),
                MEDAL,
                MEDAL,
            );
            let unlocked = p.achievements.iter().find(|(code, _)| code == a.code);
            let color = medal_color(a.level);
            let hover = ui.hovering(c);
            let centre = vec2(c.x + MEDAL / 2.0, c.y + MEDAL / 2.0);
            if unlocked.is_some() {
                rrect(c, 8.0, Color::new(color.r, color.g, color.b, 0.12));
                rrect_lines(c, 8.0, 1.2, Color::new(color.r, color.g, color.b, 0.7));
                crate::icons::draw(Icon::Medal, centre, MEDAL * 0.62, color);
            } else {
                // Still to win: a greyed medal (its colour hints at how hard
                // it is) and a small lock.
                rrect(c, 8.0, Color::new(1.0, 1.0, 1.0, 0.035));
                rrect_lines(c, 8.0, 1.0, Color::new(1.0, 1.0, 1.0, 0.07));
                let grey = |v: f32| v * 0.35 + 0.5 * 0.65;
                let ghost = Color::new(grey(color.r), grey(color.g), grey(color.b), 0.45);
                crate::icons::draw(Icon::Medal, centre, MEDAL * 0.55, ghost);
                crate::icons::draw(
                    Icon::Lock,
                    vec2(c.x + c.w - 9.0, c.y + c.h - 9.0),
                    10.0,
                    Color::new(MUTED.r, MUTED.g, MUTED.b, 0.8),
                );
            }
            if hover {
                rrect_lines(c, 8.0, 1.5, GOLD);
            }
            ui.block(c);
            if hover {
                tip = Some((a, unlocked.map(|(_, at)| *at), c));
            }
        }
        if let Some((a, at, c)) = tip {
            let level = a.level;
            let title = t(a.title);
            let desc = t(a.description);
            let when = at
                .map(|at| tw("Unlocked {date}", &[("date", &date(at))]))
                .unwrap_or_else(|| t("Locked").to_string());
            // On top of the whole screen: the history next to it must not cover it.
            ui.overlay(move |ui| {
                let w = ui
                    .text_size(title, 14.0, true)
                    .x
                    .max(ui.text_size(desc, 13.0, false).x)
                    .max(ui.text_size(&when, 12.0, false).x)
                    + 28.0;
                let below = c.y + c.h + 76.0 < crate::ui::vh();
                let tr = Rect::new(
                    (c.x + c.w / 2.0 - w / 2.0).clamp(4.0, (crate::ui::vw() - w - 4.0).max(4.0)),
                    if below { c.y + c.h + 6.0 } else { c.y - 76.0 },
                    w,
                    70.0,
                );
                rrect(tr, 8.0, Color::new(0.0, 0.0, 0.0, 0.92));
                rrect_lines(tr, 8.0, 1.0, medal_color(level));
                ui.text(title, tr.x + 14.0, tr.y + 10.0, 14.0, medal_color(level), true);
                ui.text(desc, tr.x + 14.0, tr.y + 30.0, 13.0, CREAM, false);
                ui.text(&when, tr.x + 14.0, tr.y + 50.0, 12.0, MUTED, false);
            });
        }

        // The reactions this player has unlocked, under the medals.
        let ry = top + rows as f32 * (MEDAL + MEDAL_GAP) - MEDAL_GAP + 24.0;
        let codes: Vec<&str> = p.achievements.iter().map(|(c, _)| c.as_str()).collect();
        let vip = p.player.vip;
        let reactions = snookered_proto::REACTIONS;
        let open = reactions.iter().filter(|r| r.available(&codes, vip)).count();
        heading(
            ui,
            &tw("Reactions {n}/{total}", &[("n", &open), ("total", &reactions.len())]),
            area.x,
            ry + 10.0,
        );
        let (cols, _) = balanced_grid(reactions.len(), REACTION, REACTION_GAP, area.w);
        let mut tip = None;
        for (k, r) in reactions.iter().enumerate() {
            let c = Rect::new(
                area.x + (k % cols) as f32 * (REACTION + REACTION_GAP),
                ry + 30.0 + (k / cols) as f32 * (REACTION + REACTION_GAP),
                REACTION,
                REACTION,
            );
            let centre = vec2(c.x + c.w / 2.0, c.y + c.h / 2.0);
            let hover = ui.hovering(c);
            if hover {
                rrect(c, 6.0, Color::new(1.0, 1.0, 1.0, 0.08));
            }
            if r.available(&codes, vip) {
                crate::reactions::draw(r.code, centre, 24.0, crate::paint::opacity());
            } else {
                crate::reactions::draw_locked(r.code, centre, 22.0);
                crate::icons::draw(
                    Icon::Lock,
                    vec2(c.x + c.w - 6.0, c.y + c.h - 6.0),
                    9.0,
                    Color::new(MUTED.r, MUTED.g, MUTED.b, 0.8),
                );
                if hover {
                    let how = if r.is_vip() {
                        t("For VIP members").to_string()
                    } else {
                        r.unlock
                            .and_then(|a| ACHIEVEMENTS.iter().find(|x| x.code == a))
                            .map(|a| {
                                tw(
                                    "{title}: {how}",
                                    &[("title", &t(a.title)), ("how", &t(a.description))],
                                )
                            })
                            .unwrap_or_default()
                    };
                    tip = Some((how, c));
                }
            }
            ui.block(c);
        }
        if let Some((text, c)) = tip
            && !text.is_empty()
        {
            ui.tooltip(&text, c);
        }
    }

    fn history(&mut self, ui: &mut Ui, online: &mut Online, p: &Profile, list: Rect) {
        if p.history.is_empty() {
            empty_state(ui, list, Icon::Table, t("No games yet."));
            return;
        }
        let row_h = 46.0;
        let visible = (list.h / row_h).floor().max(1.0) as usize;
        self.history_first = ui.scroll_list(list, self.history_first, p.history.len(), visible);
        let scrolls = p.history.len() > visible;
        for (k, h) in p.history.iter().skip(self.history_first).take(visible).enumerate() {
            let w = list.w - if scrolls { 12.0 } else { 0.0 };
            let r = Rect::new(list.x, list.y + k as f32 * row_h, w, row_h - 6.0);
            rrect(r, 6.0, Color::new(1.0, 1.0, 1.0, 0.035));
            let mid = r.y + r.h / 2.0;
            let (mark, color) = if h.won { (t("W"), WIN) } else { (t("L"), ALERT) };
            let badge = Rect::new(r.x + 8.0, mid - 12.0, 24.0, 24.0);
            rrect(badge, 5.0, Color::new(color.r, color.g, color.b, 0.18));
            ui.text_centered(mark, badge.x + 12.0, text_y(mid, 14.0), 14.0, color, true);
            // The rating change and the date, right-aligned; the replay
            // button (own games that kept their strokes) left of both.
            let delta = if h.rated {
                format!("{}{}", if h.delta >= 0 { "+" } else { "" }, h.delta)
            } else {
                t("casual").to_string()
            };
            let dc = if !h.rated {
                MUTED
            } else if h.delta >= 0 {
                WIN
            } else {
                ALERT
            };
            let d = date(h.at);
            let dw = ui.text_size(&delta, 15.0, true).x;
            let ddw = ui.text_size(&d, 12.0, false).x;
            let rx = r.x + r.w - 12.0;
            ui.text(&delta, rx - dw, mid - 17.0, 15.0, dc, true);
            ui.text(&d, rx - ddw, mid + 3.0, 12.0, MUTED, false);
            let mut text_right = rx - dw.max(ddw) - 12.0;
            if p.relation == Relation::Me && h.replay && h.match_id > 0 {
                let br = Rect::new(text_right - 30.0, mid - 15.0, 30.0, 30.0);
                if ui.icon_button(br, Icon::Replay, t("Watch again"), true) {
                    online.send(ClientMsg::Replay { match_id: h.match_id });
                }
                text_right = br.x - 10.0;
            }
            let x = r.x + 42.0;
            let room = text_right - x;
            let opp_color = crate::ui::name_color_of(h.opponent_admin, h.opponent_rating);
            ui.name_with_flag(
                &h.opponent,
                &h.opponent_country,
                opp_color,
                x,
                mid - 17.0,
                14.0,
                room,
            );
            let how = crate::i18n::history_reason(h.reason, h.won);
            let sub = if h.tournament {
                format!("{how} · {}", t("tournament"))
            } else {
                how.to_string()
            };
            let sub = ui.ellipsize(&sub, room, 12.0);
            ui.text(&sub, x, mid + 3.0, 12.0, MUTED, false);
        }
    }

    /// Recovery e-mail, password and deleting the account.
    fn account_dialog(&mut self, ui: &mut Ui, online: &mut Online) {
        let Some(form) = self.account.as_mut() else { return };
        if !form.email_loaded
            && let Some(a) = &online.account
        {
            form.email = a.recovery_email.clone().or(a.payment_email.clone()).unwrap_or_default();
            form.email_loaded = true;
        }
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.55));
        let w = 500.0f32.min(sw - 20.0);
        let fw = w - 2.0 * PAD;
        let wrap = |ui: &Ui, text: &str| ui.wrap(text, fw, 13.0);
        let email_note =
            wrap(ui, t("If you forget your password, we confirm it's you by this e-mail."));
        let delete_note = wrap(
            ui,
            t(
                "Your name, e-mail, friends and items are erased for good. Your opponents keep the games you played (without your name).",
            ),
        );
        // The height follows the text (it wraps differently per language):
        // the title, the three sections as laid out below, and Close.
        let lines = (email_note.len() + delete_note.len()) as f32 * 17.0;
        let h = (70.0 + 89.0 + 171.0 + 64.0 + 88.0 + lines).min(sh - 20.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        let x = r.x + PAD;
        crate::icons::draw(Icon::Person, vec2(x + 12.0, r.y + 24.0 + 13.0), 24.0, GOLD);
        ui.text(t("Your account"), x + 34.0, text_y(r.y + 24.0 + 13.0, 22.0), 22.0, GOLD, true);
        let mut y = r.y + 24.0 + 26.0 + 20.0;
        // Recovery e-mail.
        ui.text(t("Recovery e-mail"), x, y, 15.0, GOLD, true);
        y += 24.0;
        for line in &email_note {
            ui.text(line, x, y, 13.0, MUTED, false);
            y += 17.0;
        }
        y += 6.0;
        let saved = online.account.as_ref().and_then(|a| a.recovery_email.clone());
        ui.text_input(71, Rect::new(x, y, fw - 120.0, 34.0), &mut form.email, 120);
        if ui.button(
            Rect::new(x + fw - 110.0, y, 110.0, 34.0),
            t("Save"),
            saved.as_deref() != Some(form.email.trim()),
        ) {
            online.send(ClientMsg::SetRecoveryEmail { email: form.email.trim().to_string() });
        }
        y += 34.0 + 12.0;
        draw_line(x, y, x + fw, y, 1.0, BORDER);
        y += 13.0;
        // Password.
        ui.text(t("Change password"), x, y, 15.0, GOLD, true);
        y += 24.0 + 6.0;
        let label_w = 120.0;
        ui.text(t("Current"), x, text_y(y + 17.0, 13.0), 13.0, MUTED, true);
        let field = Rect::new(x + label_w, y, fw - label_w, 34.0);
        ui.password_input(72, field, &mut form.old_password, 72);
        y += 38.0;
        ui.text(t("New"), x, text_y(y + 17.0, 13.0), 13.0, MUTED, true);
        let field = Rect::new(x + label_w, y, fw - label_w, 34.0);
        ui.password_input(73, field, &mut form.new_password, 72);
        y += 38.0 + 6.0;
        let ok =
            snookered_proto::valid_password(&form.new_password) && !form.old_password.is_empty();
        let bw = ui.text_size(t("Change password"), 15.0, true).x + 32.0;
        if ui.button(Rect::new(x + fw - bw, y, bw, 34.0), t("Change password"), ok) {
            online.send(ClientMsg::ChangePassword {
                old: std::mem::take(&mut form.old_password),
                new: std::mem::take(&mut form.new_password),
            });
        }
        y += 34.0 + 12.0;
        draw_line(x, y, x + fw, y, 1.0, BORDER);
        y += 13.0;
        // Deleting the account.
        ui.text(t("Delete account"), x, y, 15.0, ALERT, true);
        y += 24.0;
        for line in &delete_note {
            ui.text(line, x, y, 13.0, MUTED, false);
            y += 17.0;
        }
        y += 6.0;
        let label =
            if form.confirm_delete { t("Click again to delete") } else { t("Delete account") };
        // Wide enough for both labels: the field beside it doesn't jump.
        let both = [t("Delete account"), t("Click again to delete")];
        let bw = both.iter().map(|l| ui.text_size(l, 15.0, true).x).fold(0.0, f32::max) + 32.0;
        let field = Rect::new(x, y, fw - bw - 10.0, 34.0);
        ui.password_input(74, field, &mut form.delete_password, 72);
        if form.delete_password.is_empty() && ui.focus != Some(74) {
            ui.text(t("Password"), x + 10.0, text_y(y + 17.0, 14.0), 14.0, MUTED, false);
        }
        if ui.button(Rect::new(x + fw - bw, y, bw, 34.0), label, !form.delete_password.is_empty()) {
            if form.confirm_delete {
                online.send(ClientMsg::DeleteAccount {
                    password: std::mem::take(&mut form.delete_password),
                });
                form.confirm_delete = false;
            } else {
                form.confirm_delete = true;
            }
        }
        if ui.button(Rect::new(x, y + 34.0 + 24.0, 140.0, 40.0), t("Close"), true)
            || (is_key_pressed(KeyCode::Escape) && ui.focus.is_none())
        {
            self.account = None;
            ui.focus = None;
        }
    }

    fn report_dialog(&mut self, ui: &mut Ui, online: &mut Online, p: &Profile) {
        let Some((reason, details)) = &mut self.report else { return };
        let sw = crate::ui::vw();
        let sh = crate::ui::vh();
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.5));
        let w = 440.0f32.min(sw - 20.0);
        let n = ReportReason::ALL.len() as f32;
        let h = 24.0 + 26.0 + 20.0 + n * 28.0 + 8.0 + 34.0 + 24.0 + 40.0 + 24.0;
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        let x = r.x + PAD;
        let title = tw("Report {name}", &[("name", &p.player.username)]);
        ui.text(&title, x, text_y(r.y + 24.0 + 13.0, 22.0), 22.0, GOLD, true);
        let mut y = r.y + 24.0 + 26.0 + 20.0;
        for (k, rr) in ReportReason::ALL.iter().enumerate() {
            if ui.radio(x, y, t(rr.name()), *reason == k) {
                *reason = k;
            }
            y += 28.0;
        }
        y += 8.0;
        let field = Rect::new(x, y, w - 2.0 * PAD, 34.0);
        ui.text_input(61, field, details, 300);
        if details.is_empty() && ui.focus != Some(61) {
            let hint = t("What happened? (optional)");
            ui.text(hint, x + 10.0, text_y(y + 17.0, 14.0), 14.0, MUTED, false);
        }
        // Cancel on the left, send on the right.
        let by = r.y + h - PAD - 40.0;
        let mut close = false;
        let send_w = ui.text_size(t("Send report"), 15.0, true).x + 40.0;
        if ui.primary_button(Rect::new(r.x + w - PAD - send_w, by, send_w, 40.0), t("Send report"))
        {
            online.send(ClientMsg::Report {
                username: p.player.username.clone(),
                reason: ReportReason::ALL[*reason],
                details: details.clone(),
            });
            close = true;
        }
        if ui.button(Rect::new(x, by, 120.0, 40.0), t("Cancel"), true)
            || (is_key_pressed(KeyCode::Escape) && ui.focus.is_none())
        {
            close = true;
        }
        if close {
            self.report = None;
            ui.focus = None;
        }
    }
}

/// A rounded tag in the tier's colour; `mid` is its vertical middle.
/// Returns its width.
fn tier_pill(ui: &Ui, text: &str, x: f32, mid: f32, color: Color) -> f32 {
    let size = 12.0;
    let w = ui.text_size(text, size, true).x + 18.0;
    let r = Rect::new(x, mid - 10.0, w, 20.0);
    rrect(r, 10.0, Color::new(color.r, color.g, color.b, 0.16));
    rrect_lines(r, 10.0, 1.0, Color::new(color.r, color.g, color.b, 0.6));
    // Capitals centred: the middle of the cap height on the pill's middle.
    ui.text(text, x + 9.0, mid - 0.435 * size, size, color, true);
    w
}

/// The rating after each rated game, as a line.
fn rating_chart(ui: &mut Ui, p: &Profile, area: Rect) {
    let pts = &p.rating_history;
    if pts.len() < 2 {
        let text = if p.relation == Relation::Me {
            t("Play rated games to see your rating here.")
        } else {
            t("No rated games yet.")
        };
        empty_state(ui, area, Icon::Trophy, text);
        return;
    }
    let chart = Rect::new(area.x + 44.0, area.y + 4.0, area.w - 48.0, area.h - 28.0);
    let lo = pts.iter().map(|p| p.1).min().unwrap_or(0) - 15;
    let hi = pts.iter().map(|p| p.1).max().unwrap_or(0) + 15;
    let span = (hi - lo).max(1) as f32;
    let at = |k: usize, r: i32| {
        vec2(
            chart.x + chart.w * k as f32 / (pts.len() - 1) as f32,
            chart.y + chart.h * (1.0 - (r - lo) as f32 / span),
        )
    };
    rrect(chart, 6.0, Color::new(1.0, 1.0, 1.0, 0.03));
    // Tier lines that fall inside the chart.
    for tier in snookered_proto::Tier::ALL {
        let m = tier.min_rating();
        if m > lo && m < hi {
            let y = chart.y + chart.h * (1.0 - (m - lo) as f32 / span);
            draw_line(chart.x, y, chart.x + chart.w, y, 1.0, Color::new(1.0, 1.0, 1.0, 0.08));
            ui.text(t(tier.name()), chart.x + 6.0, y - 15.0, 11.0, tier_color(tier), false);
        }
    }
    // The scale, right-aligned against the chart.
    for (y, label) in [(chart.y, hi), (chart.y + chart.h - 12.0, lo)] {
        let label = label.to_string();
        let lw = ui.text_size(&label, 11.0, false).x;
        ui.text(&label, chart.x - 8.0 - lw, y, 11.0, MUTED, false);
    }
    for k in 1..pts.len() {
        let (a, b) = (at(k - 1, pts[k - 1].1), at(k, pts[k].1));
        let color = if pts[k].1 >= pts[k - 1].1 { WIN } else { ALERT };
        draw_line(a.x, a.y, b.x, b.y, 2.0, color);
    }
    let last = at(pts.len() - 1, pts[pts.len() - 1].1);
    draw_circle(last.x, last.y, 4.0, GOLD);
    let first = date(pts[0].0);
    let now = date(pts[pts.len() - 1].0);
    ui.text(&first, chart.x, chart.y + chart.h + 8.0, 11.0, MUTED, false);
    let nw = ui.text_size(&now, 11.0, false).x;
    ui.text(&now, chart.x + chart.w - nw, chart.y + chart.h + 8.0, 11.0, MUTED, false);
}

/// Lifetime numbers from the games with statistics, and games left unfinished.
fn numbers(ui: &mut Ui, p: &Profile, area: Rect) {
    let c = &p.career;
    if c.games == 0 && p.abandon_rate == 0 {
        empty_state(ui, area, Icon::Star, t("No games yet."));
        return;
    }
    let per_game = c.balls_per_game_x10();
    let foul_rate = (c.fouls * 100).checked_div(c.shots).unwrap_or(0);
    let cards = [
        (format!("{}%", c.pot_rate()), t("potting success")),
        (format!("{}.{}", per_game / 10, per_game % 10), t("balls per game")),
        (c.best_run.to_string(), t("best run ever")),
        (format!("{foul_rate}%"), t("shots with a foul")),
        (c.games.to_string(), t("games counted")),
        (format!("{}%", p.abandon_rate), t("games left unfinished")),
    ];
    let (cols, gap) = (2, 12.0);
    let cw = (area.w - gap) / cols as f32;
    let ch = ((area.h - 2.0 * gap) / 3.0).min(72.0);
    for (k, (n, label)) in cards.iter().enumerate() {
        let r = Rect::new(
            area.x + (k % cols) as f32 * (cw + gap),
            area.y + (k / cols) as f32 * (ch + gap),
            cw,
            ch,
        );
        rrect(r, 8.0, Color::new(1.0, 1.0, 1.0, 0.04));
        let color = if k == 5 && p.abandon_rate >= 15 { ALERT } else { CREAM };
        let mid = r.y + r.h / 2.0;
        ui.text_centered(n, r.x + r.w / 2.0, mid - 22.0, 24.0, color, true);
        let size = fit_regular(ui, label, 13.0, r.w - 12.0);
        ui.text_centered(label, r.x + r.w / 2.0, mid + 8.0, size, MUTED, false);
    }
}
