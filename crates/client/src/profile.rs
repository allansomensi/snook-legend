//! A player's profile: rating, numbers, achievements and recent games, with
//! the friend, block and report actions.

use macroquad::prelude::*;
use snook_legend_proto::{ACHIEVEMENTS, ClientMsg, Profile, Relation, Rules};

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
    /// Play the computer in this player's style.
    Ghost(String),
    /// Sign out (one's own profile), to sign in to another account.
    SignOut,
    /// The shop's VIP page (from the advanced statistics' preview).
    ShopVip,
}

/// What the right-hand side of the profile shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Pane {
    #[default]
    Games,
    Rating,
    Numbers,
    /// Tournaments won, the ladder, daily challenges topped.
    Trophies,
    /// The shot analysis (over the whole width).
    Analysis,
    /// Every achievement and every reaction (over the whole width).
    Achievements,
    /// The leagues and the levels, with what each brings (over the whole
    /// width).
    Journey,
    /// Presents received and given (own profile).
    Gifts,
    /// The names and countries the account had (when it changed any).
    Renames,
    /// The advanced statistics (own profile; VIP, a preview otherwise).
    Advanced,
}

#[derive(Default)]
pub struct ProfileScreen {
    pub name: String,
    history_first: usize,
    report: Option<crate::report::ReportDialog>,
    confirm_block: bool,
    pane: Pane,
    /// The account dialog (own profile).
    account: Option<AccountForm>,
    /// Asked for while signed in: the server knew who was looking (and so
    /// sent our relation: friends, requests, blocks).
    asked_signed_in: bool,
    /// What the analysis's heat map shows.
    heat: crate::analysis::Heat,
    /// The status message being written (own profile).
    status_edit: Option<String>,
    /// The first present shown in the Gifts pane.
    gifts_first: usize,
    /// The first trophy shown.
    trophies_first: usize,
    /// The rules whose rating, rank, league and chart are shown (a rating
    /// card each, in the header).
    rules: Rules,
    /// "Sign out" was clicked (the screen hands it on as its action).
    sign_out: bool,
    /// The profiles this one was reached from (an opponent's name clicked
    /// in the games): Back goes to the last of them.
    trail: Vec<String>,
    /// A profile to open next (a name clicked this frame).
    follow: Option<String>,
    /// The advanced statistics were asked for, under these rules.
    advanced_asked: Option<Rules>,
}

/// The account dialog: username, recovery e-mail, password, deletion.
#[derive(Default)]
struct AccountForm {
    new_name: String,
    /// The name "Change" was clicked once for (a second click sends it).
    confirm_rename: Option<String>,
    /// The name asked for: when the session comes back under it, the page
    /// follows.
    renaming: Option<String>,
    email: String,
    email_loaded: bool,
    /// The password, to change the recovery e-mail.
    email_password: String,
    old_password: String,
    new_password: String,
    delete_password: String,
    confirm_delete: bool,
    /// How far the dialog is scrolled up (when it is taller than the window).
    scroll: f32,
    /// The field last scrolled into view (only a new one is).
    shown_field: Option<u64>,
}

/// Where the content starts under the header (from the panel's top).
const BODY_TOP: f32 = 290.0;
/// The body's height for the shot analysis (at least).
const ANALYSIS_H: f32 = 352.0;
/// The body's height: the summary on the left, a pane on the right.
const BODY_H: f32 = 400.0;
/// The row of tabs over the body, and the gap under it.
const TABS_H: f32 = 34.0;
/// The achievement medals: cell and gap.
const MEDAL: f32 = 44.0;
const MEDAL_GAP: f32 = 8.0;
/// The reactions: cell and gap.
const REACTION: f32 = 30.0;
const REACTION_GAP: f32 = 4.0;
const WIN: Color = Color::new(0.45, 0.95, 0.55, 1.0);

/// Unix seconds.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// "3 days", "2 months", "1 year".
fn age(created_at: i64) -> String {
    let now = now();
    let days = ((now - created_at).max(0) / 86_400) as u32;
    if days < 1 {
        t("joined today").to_string()
    } else if days < 60 {
        if days == 1 {
            t("member for 1 day").to_string()
        } else {
            tw("member for {n} days", &[("n", &days)])
        }
    } else if days < 730 {
        tw("member for {n} months", &[("n", &(days / 30))])
    } else {
        tw("member for {n} years", &[("n", &(days / 365))])
    }
}

/// The names and countries the account had, newest first.
fn renames(ui: &mut Ui, p: &Profile, list: Rect) {
    use snook_legend_proto::moderation::ChangeKind;
    let row_h = 46.0;
    let visible = (list.h / row_h).floor().max(1.0) as usize;
    for (k, c) in p.renames.iter().take(visible).enumerate() {
        let r = Rect::new(list.x, list.y + k as f32 * row_h, list.w, row_h - 6.0);
        rrect(r, 6.0, Color::new(1.0, 1.0, 1.0, 0.035));
        let mid = r.y + r.h / 2.0;
        let (icon, what, old, new) = match c.kind {
            ChangeKind::Country => {
                let name = |code: &str| {
                    let n = crate::flags::name(code);
                    if n.is_empty() { code.to_string() } else { n.to_string() }
                };
                (Icon::Globe, t("Country"), name(&c.old), name(&c.new))
            }
            _ => (Icon::Pencil, t("Username"), c.old.clone(), c.new.clone()),
        };
        let badge = Rect::new(r.x + 8.0, mid - 13.0, 26.0, 26.0);
        rrect(badge, 6.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.14));
        crate::icons::draw(icon, vec2(badge.x + 13.0, mid), 14.0, GOLD);
        let d = date(c.at);
        let dw = ui.text_size(&d, 12.0, false).x;
        ui.text(&d, r.x + r.w - 10.0 - dw, mid - 6.0, 12.0, MUTED, false);
        let x = r.x + 44.0;
        let room = r.w - 44.0 - dw - 20.0;
        ui.text(what, x, r.y + 4.0, 12.0, MUTED, false);
        let line = ui.ellipsize(&format!("{old} → {new}"), room, 14.0);
        ui.text(&line, x, r.y + 20.0, 14.0, CREAM, true);
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
    crate::i18n::lang().date(y, m, d)
}

/// The medal's metal, in words (its colour alone says it only to some).
pub fn medal_tier(level: u8) -> &'static str {
    t(match level {
        0 => "Bronze",
        1 => "Silver",
        2 => "Gold",
        3 => "Diamond",
        _ => "Ruby",
    })
}

/// An achievement's description, with the rules it can only be won under
/// in front ("(Brazilian rules) Win a rated game…").
pub fn achievement_text(a: &snook_legend_proto::Achievement) -> String {
    match a.rules() {
        Some(r) => format!("({}) {}", crate::i18n::rules_name(r), t(a.description)),
        None => t(a.description).to_string(),
    }
}

pub fn medal_color(level: u8) -> Color {
    match level {
        0 => Color::new(0.80, 0.55, 0.33, 1.0),
        1 => Color::new(0.80, 0.82, 0.86, 1.0),
        2 => GOLD,
        3 => Color::new(0.62, 0.90, 0.98, 1.0),
        _ => Color::new(0.93, 0.20, 0.33, 1.0),
    }
}

/// An achievement's medal (see [`crate::medal_art`]): `size` is the whole
/// medal's height, centred on `c`. `locked`: greyed and faint.
pub fn medal_art(c: Vec2, size: f32, level: u8, locked: bool) {
    crate::medal_art::draw(c, size, level, locked);
}

/// An achievement's cell: a soft glow of its metal behind the medal (none
/// while locked, then a small lock), lit gold under the pointer.
fn medal_cell(c: Rect, level: u8, unlocked: bool, hover: bool) {
    let metal = medal_color(level);
    rrect(c, 9.0, Color::new(1.0, 1.0, 1.0, if hover { 0.07 } else { 0.035 }));
    let centre = vec2(c.x + c.w / 2.0, c.y + c.h / 2.0);
    if unlocked {
        draw_circle(
            centre.x,
            centre.y + c.h * 0.1,
            c.w * 0.36,
            Color::new(metal.r, metal.g, metal.b, 0.1),
        );
        rrect_lines(c, 9.0, 1.0, Color::new(metal.r, metal.g, metal.b, 0.35));
    }
    medal_art(centre, c.h * 0.9, level, !unlocked);
    if !unlocked {
        crate::icons::draw(
            Icon::Lock,
            vec2(c.x + c.w - 8.5, c.y + c.h - 8.5),
            9.0,
            Color::new(MUTED.r, MUTED.g, MUTED.b, 0.9),
        );
    }
    if hover {
        rrect_lines(c, 9.0, 1.5, GOLD);
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
    left_height_at(width, 1.0)
}

/// [`left_height`] with the cells `scale` times their size.
fn left_height_at(width: f32, scale: f32) -> f32 {
    let (medal, medal_gap) = (MEDAL * scale, MEDAL_GAP * scale);
    let (reaction, reaction_gap) = (REACTION * scale, REACTION_GAP * scale);
    let (_, medal_rows) = balanced_grid(ACHIEVEMENTS.len(), medal, medal_gap, width);
    let n = snook_legend_proto::REACTIONS.len();
    let (_, reaction_rows) = balanced_grid(n, reaction, reaction_gap, width);
    // Heading and medals; a gap, heading and reactions (as `achievements` draws them).
    let medals = 30.0 + medal_rows as f32 * (medal + medal_gap) - medal_gap;
    let reactions = 24.0 + 30.0 + reaction_rows as f32 * (reaction + reaction_gap) - reaction_gap;
    medals + reactions
}

/// The size of the achievements' and reactions' cells (a share of their
/// full size) that fits them all in `area`: smaller on short windows, so
/// nothing runs out of the profile's panel.
fn achievements_scale(area: Rect) -> f32 {
    let mut scale = 1.0;
    while scale > 0.5 && left_height_at(area.w, scale) > area.h {
        scale -= 0.04;
    }
    scale
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
        self.trail.clear();
        self.show(name, online);
    }

    /// Goes on to `name`'s profile from this one (Back returns here).
    fn go_to(&mut self, name: &str, online: &mut Online) {
        let mut trail = std::mem::take(&mut self.trail);
        trail.push(self.name.clone());
        self.show(name, online);
        self.trail = trail;
    }

    fn show(&mut self, name: &str, online: &mut Online) {
        self.name = name.to_string();
        self.follow = None;
        // The rules of the viewer's games first.
        self.rules = online.rules;
        self.history_first = 0;
        self.report = None;
        self.confirm_block = false;
        self.account = None;
        // Development aid: SNOOK_LEGEND_PROFILE_PANE=analysis opens the analysis.
        self.pane = match crate::dev::var("SNOOK_LEGEND_PROFILE_PANE").as_deref() {
            Some("analysis") => Pane::Analysis,
            Some("numbers") => Pane::Numbers,
            Some("rating") => Pane::Rating,
            Some("achievements") => Pane::Achievements,
            Some("journey") => Pane::Journey,
            _ => Pane::Games,
        };
        self.status_edit = None;
        self.gifts_first = 0;
        self.trophies_first = 0;
        self.asked_signed_in = online.signed_in();
        online.request_profile(name);
        // Presents put off with "Later": they are shown again here.
        if online.me.as_ref().is_some_and(|me| me.username.eq_ignore_ascii_case(name)) {
            online.show_gifts_again();
        }
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
        // The username was changed in the account dialog: the page follows
        // the new name (the server signs this session in again under it).
        if let Some(form) = self.account.as_mut()
            && let Some(new) = form.renaming.clone()
            && online.me.as_ref().is_some_and(|me| me.username == new)
        {
            form.renaming = None;
            form.new_name.clear();
            if let Some(p) = online.profile.as_mut()
                && p.player.username.eq_ignore_ascii_case(&self.name)
            {
                p.player.username = new.clone();
            }
            self.name = new.clone();
            online.request_profile(&new);
        }
        let loaded =
            online.profile.clone().filter(|p| p.player.username.eq_ignore_ascii_case(&self.name));
        // The panel fits its content: the header, then the achievements and
        // reactions (the recent games scroll in the same height). Until the
        // profile comes, just room for the wait.
        let wide = matches!(
            self.pane,
            Pane::Analysis | Pane::Achievements | Pane::Journey | Pane::Advanced
        );
        // Tall enough for what the pane shows (every medal, for one).
        let body_h = TABS_H
            + match self.pane {
                Pane::Analysis | Pane::Advanced => ANALYSIS_H,
                Pane::Achievements => left_height(w - 2.0 * PAD),
                _ => BODY_H - TABS_H,
            }
            .max(BODY_H - TABS_H);
        let h = if loaded.is_some() { BODY_TOP + body_h + PAD } else { 240.0 };
        let h = h.min(sh - 40.0);
        let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
        ui.panel(r);
        let mut action = ProfileAction::None;
        // A dialog over the page (account, report, country): Esc is its own.
        let dialog = self.account.is_some()
            || self.report.is_some()
            || online.invite_dialog.is_some()
            || crate::countries::is_open()
            || crate::avatars::is_open();
        if ui.icon_button(Rect::new(r.x + PAD, r.y + 20.0, 36.0, 36.0), Icon::Back, t("Back"), true)
            || (ui.escape() && !dialog && self.status_edit.is_none())
        {
            // Back through the profiles followed, then out.
            match self.trail.pop() {
                Some(previous) => {
                    let trail = std::mem::take(&mut self.trail);
                    self.show(&previous, online);
                    self.trail = trail;
                    return ProfileAction::None;
                }
                None => action = ProfileAction::Back,
            }
        }
        let Some(mut p) = loaded else {
            let area = Rect::new(r.x, r.y + 40.0, r.w, r.h - 60.0);
            match unreachable(online) {
                Some(error) => {
                    if crate::ranking::offline_state(ui, area, &error) {
                        online.error = None;
                        online.request_profile(&self.name.clone());
                    }
                }
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
        // Their ghost: the computer in their style (signed in only: the
        // server measures it).
        if online.signed_in() && !online.lan && !p.player.guest {
            let label = if p.relation == Relation::Me {
                t("Play your ghost")
            } else {
                t("Play their ghost")
            };
            let bw = ghost_width(ui, p.relation == Relation::Me);
            let b = Rect::new(r.x + r.w - PAD - bw, r.y + 20.0, bw, 36.0);
            if ui.button(b, "", true) {
                action = ProfileAction::Ghost(p.player.username.clone());
            }
            let mid = b.y + b.h / 2.0;
            crate::opponents::draw_ghost(&p.player.username, vec2(b.x + 26.0, mid), 11.0);
            ui.text(label, b.x + 44.0, text_y(mid, 14.0), 14.0, CREAM, true);
        }
        let body = Rect::new(r.x + PAD, r.y + BODY_TOP, r.w - 2.0 * PAD, r.h - BODY_TOP - PAD);
        // Under the tabs: a summary on the left (the level, the next
        // prizes, the medals and reactions won so far) and the pane on the
        // right, or a pane over the whole width.
        let under = Rect::new(body.x, body.y + TABS_H, body.w, body.h - TABS_H);
        if !wide
            && let Some(pane) =
                self.summary(ui, online, &p, Rect::new(under.x, under.y, half, under.h))
        {
            self.pane = pane;
        }
        let right = Rect::new(body.x + half + 24.0, body.y, half, body.h);
        // One's own profile also keeps the presents.
        let own = p.relation == Relation::Me && !online.lan && !p.player.guest;
        let mut panes = vec![(Pane::Games, t("Games")), (Pane::Rating, t("Rating"))];
        panes.push((Pane::Numbers, t("Numbers")));
        panes.push((Pane::Trophies, t("Trophies")));
        panes.push((Pane::Analysis, t("Analysis")));
        panes.push((Pane::Achievements, t("Achievements")));
        panes.push((Pane::Journey, t("Leagues and levels")));
        if own {
            panes.push((Pane::Gifts, t("Gifts")));
            panes.push((Pane::Advanced, t("Advanced")));
        } else if matches!(self.pane, Pane::Gifts | Pane::Advanced) {
            self.pane = Pane::Games;
        }
        if !p.renames.is_empty() {
            panes.push((Pane::Renames, t("Names and countries")));
        } else if self.pane == Pane::Renames {
            self.pane = Pane::Games;
        }
        let labels: Vec<&str> = panes.iter().map(|(_, l)| *l).collect();
        // The switch runs over the whole body and names the pane itself.
        let seg = Rect::new(body.x, body.y - 4.0, body.w, 28.0);
        let selected = panes.iter().position(|(pane, _)| *pane == self.pane).unwrap_or(0);
        if let Some(k) = crate::tabrow::segmented(ui, seg, &labels, selected) {
            self.pane = panes[k].0;
            // (Asked again each time the pane opens: the numbers move.)
            if self.pane == Pane::Advanced {
                self.advanced_asked = None;
                online.advanced = None;
            }
            if self.pane == Pane::Gifts {
                online.request_gifts();
            }
        }
        if own && online.gift_badge() {
            // Presents waiting: a dot on Gifts.
            draw_circle(seg.x + seg.w - 8.0, seg.y + 7.0, 4.0, ALERT);
        }
        let content = Rect::new(right.x, under.y, right.w, under.h);
        match self.pane {
            Pane::Games => self.history(ui, online, &p, content),
            Pane::Rating => rating_chart(ui, &p, self.rules, content),
            Pane::Numbers => numbers(ui, &p, content),
            Pane::Trophies => self.trophies(ui, &p, content),
            Pane::Analysis => crate::analysis::draw(
                ui,
                p.analysis.as_deref(),
                p.relation == Relation::Me,
                p.player.guest,
                &p.player.username,
                Rect::new(body.x, content.y, body.w, content.h),
                &mut self.heat,
            ),
            Pane::Gifts => online.draw_gift_history(ui, content, &mut self.gifts_first),
            Pane::Advanced => {
                // Asked for the rules shown (again when they change).
                let rules = self.rules;
                if online.advanced.as_ref().is_none_or(|a| a.rules != rules)
                    && self.advanced_asked != Some(rules)
                {
                    self.advanced_asked = Some(rules);
                    online.send(ClientMsg::AdvancedStats { rules });
                }
                let stats = online.advanced.as_deref().filter(|a| a.rules == rules);
                let area = Rect::new(body.x, content.y, body.w, content.h);
                if crate::advanced::draw(ui, stats, area) {
                    action = ProfileAction::ShopVip;
                }
            }
            Pane::Achievements => self.achievements(ui, &p, under),
            Pane::Journey => journey(ui, online, &p, self.rules, under),
            Pane::Renames => renames(ui, &p, content),
        }
        if let Some(saved) = saved {
            ui.resume_input(saved);
        }
        if let Some(code) = crate::countries::draw_picker(ui) {
            online.send(ClientMsg::SetCountry { country: code });
            online.request_profile(&p.player.username);
        }
        // A new picture: the game sends it to the server (see main); shown
        // here at once.
        if let Some(code) = crate::avatars::draw_picker(ui) {
            if let Some(prof) = online.profile.as_mut() {
                prof.player.avatar = code.clone();
            }
            if let Some(me) = online.me.as_mut() {
                me.avatar = code;
            }
        }
        self.report_dialog(ui, online);
        self.account_dialog(ui, online);
        if let Some(name) = self.follow.take()
            && matches!(action, ProfileAction::None)
        {
            self.go_to(&name, online);
        }
        if std::mem::take(&mut self.sign_out) {
            action = ProfileAction::SignOut;
        }
        action
    }

    /// A rating card in the header: the rules, the rating and league, the
    /// record under them and the way to the next league, lit when `shown`;
    /// whether it was clicked.
    fn rating_card(&self, ui: &mut Ui, p: &Profile, rules: Rules, shown: bool, c: Rect) -> bool {
        use snook_legend_proto::Tier;
        let standing = p.player.standing(rules);
        let a = crate::rulesui::accent(rules);
        let hover = ui.hovering(c);
        // Both cards alike; the one shown is lit in its rules' colour.
        rrect(c, 9.0, Color::new(1.0, 1.0, 1.0, if hover && !shown { 0.06 } else { 0.04 }));
        if shown {
            rrect(c, 9.0, Color::new(a.r, a.g, a.b, 0.08));
        }
        let edge = if shown {
            a
        } else {
            Color::new(BORDER.r, BORDER.g, BORDER.b, if hover { 0.9 } else { 0.5 })
        };
        rrect_lines(c, 9.0, if shown { 1.6 } else { 1.0 }, edge);
        let x = c.x + 16.0;
        // The rules.
        crate::rulesui::mark(rules, vec2(x + 9.0, c.y + 20.0), 18.0);
        let name = crate::i18n::rules_name(rules);
        ui.text(name, x + 26.0, text_y(c.y + 20.0, 14.0), 14.0, a, true);
        // The rating, and the league.
        let tier = p.player.tier_in(rules);
        let rating = standing.rating.to_string();
        ui.text(&rating, x, c.y + 36.0, 32.0, tier_color(tier), true);
        let rw = ui.text_size(&rating, 32.0, true).x;
        ui.text(t(tier.name()), x + rw + 10.0, c.y + 50.0, 14.0, MUTED, false);
        // The record, on the right.
        let right = c.x + c.w - 16.0;
        let record = if standing.games == 0 {
            t("no rated games yet").to_string()
        } else {
            tw(
                "{games} games · {pct}% won",
                &[("games", &standing.games), ("pct", &standing.win_rate())],
            )
        };
        let (record, size) = ui.fitted(&record, 13.0, c.w * 0.42, false);
        let w = ui.text_size(&record, size, false).x;
        let color = if standing.games == 0 { MUTED } else { CREAM };
        ui.text(&record, right - w, c.y + 14.0, size, color, false);
        // The peak says something once it was played for.
        if standing.games > 0 {
            let peak = tw("peak {n}", &[("n", p.peaks.get(rules))]);
            let pw = ui.text_size(&peak, 12.0, false).x;
            ui.text(&peak, right - pw, c.y + 33.0, 12.0, MUTED, false);
        }
        // The way to the next league.
        let next = Tier::ALL.into_iter().find(|t| t.min_rating() > standing.rating);
        let bar = Rect::new(x, c.y + c.h - 14.0, c.w - 32.0, 4.0);
        rrect(bar, 2.0, Color::new(1.0, 1.0, 1.0, 0.08));
        match next {
            Some(next) => {
                let from = if tier == Tier::Rookie {
                    snook_legend_core::rating::EloConfig::default()
                        .initial_rating
                        .min(standing.rating)
                } else {
                    tier.min_rating()
                };
                let span = (next.min_rating() - from).max(1) as f32;
                let k = ((standing.rating - from) as f32 / span).clamp(0.0, 1.0);
                if k > 0.0 {
                    rrect(
                        Rect::new(bar.x, bar.y, (bar.w * k).max(4.0), bar.h),
                        2.0,
                        tier_color(next),
                    );
                }
                let to = tw(
                    "{n} points to {tier}",
                    &[("n", &(next.min_rating() - standing.rating)), ("tier", &t(next.name()))],
                );
                let tw_ = ui.text_size(&to, 12.0, false).x;
                ui.text(&to, right - tw_, c.y + 54.0, 12.0, MUTED, false);
            }
            None => {
                rrect(bar, 2.0, tier_color(tier));
                let top = t("Top tier");
                let tw_ = ui.text_size(top, 12.0, false).x;
                ui.text(top, right - tw_, c.y + 54.0, 12.0, tier_color(tier), false);
            }
        }
        if hover {
            // The one shown says what the two ratings mean; the other how
            // to show it.
            let tip = if shown {
                t("Each of the rules has its own rating; the level, coins and achievements are shared.")
                    .to_string()
            } else {
                tw(
                    "Show the {rules}: the rank, the league and the chart",
                    &[("rules", &crate::i18n::rules_name_inline(rules))],
                )
            };
            ui.tooltip(&tip, c);
        }
        hover && !shown && ui.clicked(c)
    }

    fn header(&mut self, ui: &mut Ui, online: &mut Online, p: &Profile, r: Rect) {
        let player = &p.player;
        let framed = snook_legend_proto::shop::item(&player.frame)
            .map(|i| i.look)
            .filter(|_| player.frame != "frame_none");
        // With a frame, everything moves in a little: the frame keeps clear
        // of the back button.
        let pad = if framed.is_some() { 14.0 } else { 0.0 };
        // The picture, by the name and the line under it; one's own can be
        // changed with a click.
        let pic_r = 31.0;
        let pic = vec2(r.x + PAD + 36.0 + 16.0 + pic_r, r.y + 57.0);
        crate::avatars::draw(&player.avatar, pic, pic_r);
        let own = p.relation == Relation::Me && !online.lan && !player.guest;
        if own {
            let hit = Rect::new(pic.x - pic_r, pic.y - pic_r, 2.0 * pic_r, 2.0 * pic_r);
            let hover = ui.hovering(hit);
            // A pencil badge says it can be changed.
            let b = pic + vec2(pic_r * 0.72, pic_r * 0.72);
            draw_circle(
                b.x,
                b.y,
                10.0,
                if hover { GOLD } else { Color::new(0.2, 0.14, 0.09, 1.0) },
            );
            draw_circle_lines(b.x, b.y, 10.0, 1.2, GOLD);
            crate::icons::draw(Icon::Pencil, b, 11.0, if hover { BLACK } else { GOLD });
            if hover {
                draw_circle_lines(pic.x, pic.y, pic_r + 1.5, 2.0, GOLD);
                ui.tooltip(t("Change your picture"), hit);
                if ui.clicked(hit) {
                    crate::avatars::open();
                }
            }
        }
        let left = r.x + PAD + 36.0 + 16.0 + 2.0 * pic_r + 16.0;
        let x = left + pad;
        // The name's middle lines up with the back button's.
        let mid = r.y + 38.0;
        let flag_h = 24.0;
        // One's own flag is where the country is changed (a placeholder
        // stands in for none yet).
        let flag_w = if player.country.is_empty() && !own {
            0.0
        } else {
            crate::flags::width(flag_h) + 12.0
        };
        let name_w = ui.text_size(&player.username, 30.0, true).x;
        let crown_w = if player.vip { 30.0 } else { 0.0 };
        let dot_w = if p.online { 18.0 } else { 0.0 };
        if let Some(snook_legend_proto::shop::Look::Frame { a, b, shine }) = framed {
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
        let flag = Rect::new(x, mid - flag_h / 2.0, crate::flags::width(flag_h), flag_h);
        let drawn = crate::flags::draw(&player.country, flag.x, flag.y, flag.h);
        if own {
            country_flag(ui, flag, drawn, &player.country);
        }
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
        // A rating card for each of the rules, under the name: the one shown
        // (its rank, league and chart) is lit; a click shows the other.
        let rules = self.rules;
        if !player.guest {
            let gap = 12.0;
            let cw = (r.w - 2.0 * PAD - gap) / 2.0;
            for (k, rr) in Rules::ALL.into_iter().enumerate() {
                let c = Rect::new(r.x + PAD + k as f32 * (cw + gap), r.y + 100.0, cw, 92.0);
                if self.rating_card(ui, p, rr, rr == rules, c) {
                    self.rules = rr;
                }
            }
        }

        // The status message, between the name and the ghost's button (at
        // the top right, signed in).
        let status_x = after + dot_w + 18.0 + pad;
        let ghost_w = if online.signed_in() && !online.lan && !player.guest {
            ghost_width(ui, p.relation == Relation::Me) + 16.0
        } else {
            0.0
        };
        let account_w = if own { ACCOUNT_W + 8.0 } else { 0.0 };
        let status_end = r.x + r.w - PAD - ghost_w - account_w;
        self.status_message(ui, online, p, status_x, status_end, mid);

        // The tier and a line about the player, under the name. The pill's
        // text and the line share a baseline.
        let tier = player.tier_in(rules);
        let info_mid = r.y + 76.0;
        let mut info_x = left;
        let _ = tier;
        if !player.guest {
            // The account level (experience, not skill); the league is on
            // the rating cards.
            if player.level > 0 {
                let level = tw("Level {n}", &[("n", &player.level)]);
                info_x += tier_pill(ui, &level, info_x, info_mid, GOLD) + 10.0;
            }
        }
        let mut parts = Vec::new();
        if let Some(rank) = p.ranks.get(rules) {
            parts.push(tw("#{n} in the world", &[("n", rank)]));
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

        // Numbers.
        let s = &p.stats;
        let pct = match (s.wins * 100).checked_div(s.games) {
            Some(pct) => format!("{pct}%"),
            None => "–".to_string(),
        };
        let cards = [
            (s.games.to_string(), t("rated games")),
            (s.wins.to_string(), t("wins")),
            (pct, t("win rate")),
            (s.eight_wins.to_string(), t("won on the 8")),
            (s.best_streak.to_string(), t("best streak")),
            (s.tournaments_won.to_string(), t("tournaments won")),
        ];
        let gap = 12.0;
        let cw = (r.w - 2.0 * PAD - 5.0 * gap) / 6.0;
        for (k, (n, label)) in cards.iter().enumerate() {
            let c = Rect::new(r.x + PAD + k as f32 * (cw + gap), r.y + 204.0, cw, 58.0);
            rrect(c, 8.0, Color::new(1.0, 1.0, 1.0, 0.04));
            ui.text_centered(n, c.x + c.w / 2.0, c.y + 8.0, 22.0, CREAM, true);
            let size = fit_regular(ui, label, 12.0, c.w - 10.0);
            ui.text_centered(label, c.x + c.w / 2.0, c.y + 37.0, size, MUTED, false);
        }

        // A line between the header and the tabs.
        let line_y = r.y + BODY_TOP - 14.0;
        draw_line(r.x + PAD, line_y, r.x + r.w - PAD, line_y, 1.0, BORDER);

        // The actions: a row of icons at the top right, under the ghost's
        // button, each named by its hint. Making friends, the way on, is
        // green; a block waiting for its second click is red.
        let size = 32.0;
        let row_mid = r.y + 77.0;
        let mut bx = r.x + r.w - PAD;
        #[derive(Clone, Copy, PartialEq)]
        enum Look {
            Plain,
            Primary,
            Armed,
        }
        let mut icon_action = |ui: &mut Ui, icon: Icon, tip: &str, enabled: bool, look: Look| {
            bx -= size;
            let b = Rect::new(bx, row_mid - size / 2.0, size, size);
            bx -= 6.0;
            match look {
                Look::Plain => ui.icon_button_ex(b, icon, tip, enabled, false),
                Look::Primary | Look::Armed => {
                    ui.block(b);
                    let hover = ui.hovering(b);
                    let (fill, edge) = if look == Look::Primary {
                        (
                            Color::new(0.16, 0.42, 0.24, if hover { 1.0 } else { 0.92 }),
                            Color::new(0.45, 0.85, 0.55, if hover { 1.0 } else { 0.7 }),
                        )
                    } else {
                        (
                            Color::new(0.5, 0.14, 0.1, if hover { 1.0 } else { 0.9 }),
                            Color::new(ALERT.r, ALERT.g, ALERT.b, 1.0),
                        )
                    };
                    rrect(b, 7.0, fill);
                    rrect_lines(b, 7.0, 1.2, edge);
                    crate::icons::draw(icon, vec2(b.x + b.w / 2.0, row_mid), size * 0.52, CREAM);
                    if hover {
                        ui.tooltip(tip, b);
                    }
                    hover && ui.released_in(b)
                }
            }
        };

        let name = player.username.clone();
        let actions = !(player.guest || online.lan);
        match p.relation {
            _ if !actions => {}
            Relation::Me => {
                // On the name's line, before the ghost's button.
                let b = Rect::new(
                    r.x + r.w - PAD - ghost_w - ACCOUNT_W,
                    r.y + 20.0,
                    ACCOUNT_W,
                    ACCOUNT_W,
                );
                if ui.icon_button(b, Icon::Gear, t("Account"), true) {
                    self.account = Some(AccountForm::default());
                    online.send(ClientMsg::Account);
                }
            }
            _ if online.me.is_none() => {}
            relation => {
                if relation != Relation::Blocked
                    && icon_action(ui, Icon::Flag, t("Report"), true, Look::Plain)
                {
                    let me = online.me.as_ref().map(|m| m.username.as_str());
                    self.report = Some(crate::report::ReportDialog::from_profile(p, me, now()));
                }
                let (block_tip, block_look) = if relation == Relation::Blocked {
                    (t("Unblock"), Look::Plain)
                } else if self.confirm_block {
                    (t("Click again to block"), Look::Armed)
                } else {
                    (t("Block"), Look::Plain)
                };
                if icon_action(ui, Icon::Block, block_tip, true, block_look) {
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
                    Relation::Friend => Some((Icon::RemoveFriend, t("Remove friend"), Look::Plain)),
                    Relation::RequestSent => {
                        Some((Icon::AddFriend, t("Request sent"), Look::Plain))
                    }
                    Relation::RequestReceived => {
                        Some((Icon::AddFriend, t("Accept friend"), Look::Primary))
                    }
                    Relation::None => Some((Icon::AddFriend, t("Add friend"), Look::Primary)),
                    _ => None,
                };
                if let Some((icon, tip, look)) = friend {
                    let enabled = relation != Relation::RequestSent;
                    let look = if enabled { look } else { Look::Plain };
                    if icon_action(ui, icon, tip, enabled, look) {
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
                // A daily game: one turn at a time, even while they are away.
                if relation != Relation::Blocked
                    && icon_action(
                        ui,
                        Icon::Hourglass,
                        t("Challenge to a daily game"),
                        true,
                        Look::Plain,
                    )
                {
                    crate::correspondence::challenge(online, &name);
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
            let enter = ui.text_input(FIELD, field, draft, snook_legend_proto::MAX_STATUS_LEN);
            if draft.is_empty() {
                // Shown while typing too: the field was opened for it.
                let hint = ui.ellipsize(t("A line about you"), field.w - 24.0, 14.0);
                ui.text(&hint, field.x + 12.0, text_y(mid, 14.0), 14.0, MUTED, false);
            }
            let draft = draft.clone();
            let save = Rect::new(field.x + field.w + 4.0, mid - 15.0, 30.0, 30.0);
            let cancel = Rect::new(save.x + 34.0, mid - 15.0, 30.0, 30.0);
            if ui.icon_button(save, Icon::Check, t("Save (Enter)"), true) || enter {
                let clean = snook_legend_proto::clean_status(&draft);
                // Shown at once (with the chat off, once it is on); the
                // lounge hears it from the server.
                if online.send_text(ClientMsg::SetStatusMessage { text: clean.clone() })
                    && let Some(shown) = online.profile.as_mut()
                {
                    shown.player.status_message = clean;
                }
                self.status_edit = None;
                ui.focus = None;
            } else if ui.icon_button(cancel, Icon::Close, t("Cancel (Esc)"), true) || ui.escape() {
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
            && (!p.player.role.is_staff() || staff == snook_legend_proto::Role::Admin);
        if text.is_empty() {
            if !own {
                return;
            }
            // Nothing yet: a quiet invitation to write one (it lights up
            // under the pointer, like the flag and the picture).
            let label = t("Add a status message");
            let lw = ui.text_size(label, 14.0, false).x;
            let b = Rect::new(x - 8.0, mid - 14.0, (lw + 40.0).min(width), 28.0);
            let hover = ui.hovering(b);
            ui.block(b);
            if hover {
                rrect(b, 7.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.08));
            }
            let color = if hover { GOLD } else { Color::new(MUTED.r, MUTED.g, MUTED.b, 0.75) };
            crate::icons::draw(Icon::Pencil, vec2(b.x + 16.0, mid), 13.0, color);
            let label = ui.ellipsize(label, b.w - 36.0, 14.0);
            ui.text(&label, b.x + 30.0, text_y(mid, 14.0), 14.0, color, false);
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

    /// The left of the profile: where the player stands (the level and what
    /// the next level and the next league bring) and what they have won so
    /// far (their medals and reactions, the newest first, one row of each).
    /// The whole lists are in the panes: returns the pane a click asks for.
    fn summary(&mut self, ui: &mut Ui, online: &Online, p: &Profile, area: Rect) -> Option<Pane> {
        let player = &p.player;
        let mut go = None;
        let mut y = area.y;
        // Each card only where it fits whole (a large interface in a small
        // window leaves room for fewer): the rest is in the panes.
        let bottom = area.y + area.h;
        if !player.guest {
            // The level: its bar (one's own: the XP is known), and what the
            // next one with an item brings.
            let own = p.relation == Relation::Me;
            let xp = online.xp.filter(|_| own);
            let card_h = if xp.is_some() { 84.0 } else { 66.0 };
            if y + card_h > bottom {
                return go;
            }
            let card = Rect::new(area.x, y, area.w, card_h);
            rrect(card, 8.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.07));
            let hover = ui.hovering(card);
            if hover {
                rrect_lines(card, 8.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.6));
            }
            let level = tw("Level {n}", &[("n", &player.level.max(1))]);
            ui.text(&level, card.x + 14.0, card.y + 10.0, 20.0, GOLD, true);
            if let Some(xp) = xp {
                let bar = Rect::new(card.x + 14.0, card.y + 40.0, card.w - 28.0, 6.0);
                rrect(bar, 3.0, Color::new(1.0, 1.0, 1.0, 0.08));
                let (_, into, span) = snook_legend_proto::xp::progress(xp);
                let k = if span == 0 { 1.0 } else { into as f32 / span as f32 };
                if k > 0.0 {
                    rrect(Rect::new(bar.x, bar.y, (bar.w * k).max(6.0), bar.h), 3.0, GOLD);
                }
                let text = format!("{into} / {span} XP");
                let tw_ = ui.text_size(&text, 12.0, false).x;
                ui.text(&text, card.x + card.w - 14.0 - tw_, card.y + 16.0, 12.0, MUTED, false);
            }
            let next = next_reward(player.level.max(1))
                .unwrap_or_else(|| t("Every prize of the levels is won.").to_string());
            let (next, size) = ui.fitted(&next, 13.0, card.w - 28.0, false);
            ui.text(&next, card.x + 14.0, card.y + card.h - 28.0, size, CREAM, false);
            ui.block(card);
            if ui.clicked(card) {
                go = Some(Pane::Journey);
            }
            y += card.h + 10.0;
            if y + 58.0 > bottom {
                return go;
            }
            // The next league and its prize (under the rules shown).
            let tier = player.tier_in(self.rules);
            let rating = player.rating_in(self.rules);
            let next_tier =
                snook_legend_proto::Tier::ALL.into_iter().find(|t| t.min_rating() > rating);
            let card = Rect::new(area.x, y, area.w, 58.0);
            let color = tier_color(next_tier.unwrap_or(tier));
            rrect(card, 8.0, Color::new(color.r, color.g, color.b, 0.07));
            if ui.hovering(card) {
                rrect_lines(card, 8.0, 1.0, Color::new(color.r, color.g, color.b, 0.6));
            }
            let (title, line) = match next_tier {
                Some(n) => (tw("Next league: {tier}", &[("tier", &t(n.name()))]), league_prize(n)),
                None => (t(tier.name()).to_string(), t("The top league.").to_string()),
            };
            ui.text(&title, card.x + 14.0, card.y + 9.0, 15.0, color, true);
            let (line, size) = ui.fitted(&line, 13.0, card.w - 28.0, false);
            ui.text(&line, card.x + 14.0, card.y + 32.0, size, CREAM, false);
            ui.block(card);
            if ui.clicked(card) {
                go = Some(Pane::Journey);
            }
            y += card.h + 14.0;
        }
        // The medals won, the newest first: one row (the rest is in the pane).
        let mut won: Vec<(&snook_legend_proto::Achievement, i64)> = ACHIEVEMENTS
            .iter()
            .filter_map(|a| {
                p.achievements.iter().find(|(c, _)| c == a.code).map(|(_, at)| (a, *at))
            })
            .collect();
        won.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
        if y + 22.0 + MEDAL > bottom {
            return go;
        }
        let row = Rect::new(area.x, y, area.w, 22.0 + MEDAL);
        heading(
            ui,
            &tw("Achievements {n}/{total}", &[("n", &won.len()), ("total", &ACHIEVEMENTS.len())]),
            area.x,
            y + 10.0,
        );
        let fit = ((area.w + MEDAL_GAP) / (MEDAL + MEDAL_GAP)).floor().max(1.0) as usize;
        if won.is_empty() {
            ui.text(
                t("None yet: the first is one game away."),
                area.x,
                y + 34.0,
                13.0,
                MUTED,
                false,
            );
        }
        for (k, (a, _)) in won.iter().take(fit).enumerate() {
            let c = Rect::new(area.x + k as f32 * (MEDAL + MEDAL_GAP), y + 26.0, MEDAL, MEDAL);
            medal_cell(c, a.level, true, ui.hovering(c));
            if ui.hovering(c) {
                let tip =
                    format!("{} ({}): {}", t(a.title), medal_tier(a.level), achievement_text(a));
                ui.tooltip(&tip, c);
            }
        }
        ui.block(row);
        if ui.clicked(row) {
            go = Some(Pane::Achievements);
        }
        y += row.h + 16.0;
        // The reactions they can send: one row.
        let codes: Vec<&str> = p.achievements.iter().map(|(c, _)| c.as_str()).collect();
        let reactions = snook_legend_proto::REACTIONS;
        let open: Vec<_> =
            reactions.iter().filter(|r| r.available(&codes, &p.reactions, player.vip)).collect();
        if y + 30.0 + REACTION <= area.y + area.h {
            heading(
                ui,
                &tw("Reactions {n}/{total}", &[("n", &open.len()), ("total", &reactions.len())]),
                area.x,
                y + 10.0,
            );
            let fit =
                ((area.w + REACTION_GAP) / (REACTION + REACTION_GAP)).floor().max(1.0) as usize;
            for (k, r) in open.iter().take(fit).enumerate() {
                let c = vec2(
                    area.x + k as f32 * (REACTION + REACTION_GAP) + REACTION / 2.0,
                    y + 28.0 + REACTION / 2.0,
                );
                crate::reactions::draw(r.code, c, 24.0, crate::paint::alpha());
            }
            let row = Rect::new(area.x, y, area.w, 28.0 + REACTION);
            ui.block(row);
            if ui.clicked(row) {
                go = Some(Pane::Achievements);
            }
        }
        go
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
        // Every cell shrinks alike when the window is too short for them.
        let scale = achievements_scale(area);
        let (medal, medal_gap) = (MEDAL * scale, MEDAL_GAP * scale);
        let (reaction, reaction_gap) = (REACTION * scale, REACTION_GAP * scale);
        let (cols, rows) = balanced_grid(ACHIEVEMENTS.len(), medal, medal_gap, area.w);
        let mut tip = None;
        let top = area.y + 30.0;
        for (k, a) in ACHIEVEMENTS.iter().enumerate() {
            let (col, row) = (k % cols, k / cols);
            let c = Rect::new(
                area.x + col as f32 * (medal + medal_gap),
                top + row as f32 * (medal + medal_gap),
                medal,
                medal,
            );
            // Development aid: SNOOK_LEGEND_ALL_MEDALS shows every medal won.
            let unlocked = p
                .achievements
                .iter()
                .find(|(code, _)| code == a.code)
                .map(|(_, at)| *at)
                .or(crate::dev::flag("SNOOK_LEGEND_ALL_MEDALS").then_some(0));
            let hover = ui.hovering(c);
            // Still to win: greyed, with a small lock.
            medal_cell(c, a.level, unlocked.is_some(), hover);
            ui.block(c);
            if hover {
                tip = Some((a, unlocked, c));
            }
        }
        if let Some((a, at, c)) = tip {
            let level = a.level;
            // A secret stays one until it is won.
            let hidden = a.secret && at.is_none();
            let title = if hidden { "???" } else { t(a.title) };
            let desc = if hidden {
                t("A secret achievement: play to find it.").to_string()
            } else {
                achievement_text(a)
            };
            let when = at
                .map(|at| tw("Unlocked {date}", &[("date", &date(at))]))
                .unwrap_or_else(|| t("Locked").to_string());
            let when = format!("{} · {when}", medal_tier(level));
            // On top of the whole screen: the history next to it must not cover it.
            ui.overlay(move |ui| {
                let w = ui
                    .text_size(title, 14.0, true)
                    .x
                    .max(ui.text_size(&desc, 13.0, false).x)
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
                ui.text(&desc, tr.x + 14.0, tr.y + 30.0, 13.0, CREAM, false);
                ui.text(&when, tr.x + 14.0, tr.y + 50.0, 12.0, MUTED, false);
            });
        }

        // The reactions this player has unlocked, under the medals.
        let ry = top + rows as f32 * (medal + medal_gap) - medal_gap + 24.0;
        let codes: Vec<&str> = p.achievements.iter().map(|(c, _)| c.as_str()).collect();
        let vip = p.player.vip;
        let reactions = snook_legend_proto::REACTIONS;
        let open = reactions.iter().filter(|r| r.available(&codes, &p.reactions, vip)).count();
        heading(
            ui,
            &tw("Reactions {n}/{total}", &[("n", &open), ("total", &reactions.len())]),
            area.x,
            ry + 10.0,
        );
        let (cols, _) = balanced_grid(reactions.len(), reaction, reaction_gap, area.w);
        let mut tip = None;
        for (k, r) in reactions.iter().enumerate() {
            let c = Rect::new(
                area.x + (k % cols) as f32 * (reaction + reaction_gap),
                ry + 30.0 + (k / cols) as f32 * (reaction + reaction_gap),
                reaction,
                reaction,
            );
            let centre = vec2(c.x + c.w / 2.0, c.y + c.h / 2.0);
            let hover = ui.hovering(c);
            if hover {
                rrect(c, 6.0, Color::new(1.0, 1.0, 1.0, 0.08));
            }
            let have = r.available(&codes, &p.reactions, vip);
            if have {
                crate::reactions::draw(r.code, centre, 24.0 * scale, crate::paint::alpha());
            } else {
                crate::reactions::draw_locked(r.code, centre, 22.0 * scale);
                crate::icons::draw(
                    Icon::Lock,
                    vec2(c.x + c.w - 6.0, c.y + c.h - 6.0),
                    9.0,
                    Color::new(MUTED.r, MUTED.g, MUTED.b, 0.8),
                );
            }
            // Had or not, it says how it is won.
            if hover {
                tip = Some((crate::reactions::how(r, have), c));
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
            // (A doubles game's id is negative: see `HistoryEntry::doubles`.)
            if p.relation == Relation::Me && h.replay && h.match_id != 0 {
                let br = Rect::new(text_right - 30.0, mid - 15.0, 30.0, 30.0);
                if ui.icon_button(br, Icon::Replay, t("Watch again"), true) {
                    online.send(ClientMsg::Replay { match_id: h.match_id });
                }
                text_right = br.x - 10.0;
            }
            let x = r.x + 42.0;
            let room = text_right - x;
            let opp_color = crate::ui::name_color_of(h.opponent_admin, h.opponent_rating);
            // The opponent's name leads to their profile.
            let name_w = ui.name_with_flag(
                &h.opponent,
                &h.opponent_country,
                opp_color,
                x,
                mid - 17.0,
                14.0,
                room,
            );
            // (A doubles game's two opponents aren't one profile.)
            let linked = !h.opponent.is_empty()
                && h.doubles.is_none()
                && !h.opponent.starts_with("deleted-")
                && !h.opponent.eq_ignore_ascii_case(&p.player.username);
            let hit = Rect::new(x - 3.0, mid - 19.0, name_w + 6.0, 20.0);
            if linked && ui.hovering(hit) {
                ui.block(hit);
                let ul = mid - 17.0 + 16.0;
                draw_line(
                    x + name_w - ui.text_size(&h.opponent, 14.0, true).x.min(name_w),
                    ul,
                    x + name_w,
                    ul,
                    1.0,
                    opp_color,
                );
                ui.tooltip(t("Open the profile"), hit);
                if ui.clicked(hit) {
                    self.follow = Some(h.opponent.clone());
                }
            }
            // A daily game lost on time was not left: it ran out.
            let how = match (h.correspondence, h.reason, h.won) {
                (true, snook_legend_core::GameOverReason::Abandoned, true) => {
                    t("the opponent ran out of time")
                }
                (true, snook_legend_core::GameOverReason::Abandoned, false) => t("ran out of time"),
                _ => crate::i18n::history_reason(h.reason, h.won),
            };
            let sub = if let Some(d) = &h.doubles {
                let with = crate::i18n::tw("with {name}", &[("name", &d.partner)]);
                format!("{} · {with} · {how}", t("Doubles"))
            } else if h.tournament {
                format!("{how} · {}", t("tournament"))
            } else if h.correspondence {
                format!("{how} · {}", t("daily game"))
            } else {
                how.to_string()
            };
            // The rules the game was played under, after the line.
            let tag_w = crate::rulesui::tag_w(ui, h.rules, 12.0) + 10.0;
            let sub = ui.ellipsize(&sub, room - tag_w, 12.0);
            ui.text(&sub, x, mid + 3.0, 12.0, MUTED, false);
            let tx = x + ui.text_size(&sub, 12.0, false).x + 10.0;
            crate::rulesui::tag(ui, h.rules, tx, mid + 10.0, 12.0);
        }
    }

    /// The trophy shelf: what they won, newest first, with the date.
    fn trophies(&mut self, ui: &mut Ui, p: &Profile, list: Rect) {
        use snook_legend_proto::TrophyKind;
        let shelf: Vec<_> = p.trophies.iter().filter(|t| t.kind != TrophyKind::Other).collect();
        if shelf.is_empty() {
            empty_state(ui, list, Icon::Trophy, t("No trophies yet."));
            return;
        }
        let row_h = 52.0;
        let visible = (list.h / row_h).floor().max(1.0) as usize;
        self.trophies_first = ui.scroll_list(list, self.trophies_first, shelf.len(), visible);
        let scrolls = shelf.len() > visible;
        for (k, trophy) in shelf.iter().skip(self.trophies_first).take(visible).enumerate() {
            let w = list.w - if scrolls { 12.0 } else { 0.0 };
            let r = Rect::new(list.x, list.y + k as f32 * row_h, w, row_h - 6.0);
            rrect(r, 6.0, Color::new(1.0, 1.0, 1.0, 0.035));
            let mid = r.y + r.h / 2.0;
            let (icon, color, title, sub) = match trophy.kind {
                TrophyKind::Tournament => (
                    Icon::Trophy,
                    GOLD,
                    crate::i18n::tournament_name(&trophy.name),
                    tw("Tournament won · {n} players", &[("n", &trophy.players)]),
                ),
                TrophyKind::Ladder => (
                    Icon::Crown,
                    Color::new(0.75, 0.55, 0.95, 1.0),
                    t("The computer tour").to_string(),
                    t("All fifteen opponents beaten").to_string(),
                ),
                _ => (
                    Icon::Star,
                    Color::new(0.45, 0.75, 1.0, 1.0),
                    t("Daily challenge").to_string(),
                    tw("First of {n} players", &[("n", &trophy.players)]),
                ),
            };
            let badge = Rect::new(r.x + 8.0, mid - 15.0, 30.0, 30.0);
            rrect(badge, 6.0, Color::new(color.r, color.g, color.b, 0.18));
            crate::icons::draw(icon, vec2(badge.x + 15.0, mid), 16.0, color);
            let d = date(trophy.at);
            let dw = ui.text_size(&d, 12.0, false).x;
            let rx = r.x + r.w - 12.0;
            ui.text(&d, rx - dw, text_y(mid, 12.0), 12.0, MUTED, false);
            let x = badge.x + badge.w + 12.0;
            let room = rx - dw - 12.0 - x;
            let (title, size) = ui.fitted(&title, 14.0, room, true);
            ui.text(&title, x, mid - 17.0, size, CREAM, true);
            let sub = ui.ellipsize(&sub, room, 12.0);
            ui.text(&sub, x, mid + 3.0, 12.0, MUTED, false);
        }
    }

    /// Username, recovery e-mail, password and deleting the account.
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
        // the title, the four sections as laid out below, and Close.
        let lines = (email_note.len() + delete_note.len()) as f32 * 17.0;
        // A supporter (VIP now or before) chooses to be in the credits.
        let supporter = online.account.as_ref().is_some_and(|a| a.supporter);
        let credits_h = if supporter { 34.0 } else { 0.0 };
        let h = 70.0 + 144.0 + 89.0 + 171.0 + 64.0 + 88.0 + lines + credits_h;
        // Taller than the window (a large interface size): the dialog
        // scrolls with the wheel, so every control stays in reach.
        let room = sh - 20.0;
        let max_scroll = (h - room).max(0.0);
        let wheel = mouse_wheel().1;
        if wheel != 0.0 {
            form.scroll -= wheel.signum() * 48.0;
        }
        form.scroll = form.scroll.clamp(0.0, max_scroll);
        let top = if max_scroll > 0.0 { 10.0 - form.scroll } else { (sh - h) / 2.0 };
        let r = Rect::new((sw - w) / 2.0, top, w, h);
        ui.panel(r);
        ui.block(Rect::new(0.0, 0.0, sw, sh));
        if max_scroll > 0.0 {
            // A scroll bar at the window's edge: there is more.
            let track = Rect::new(r.x + r.w + 4.0, 10.0, 5.0, room);
            let track = if track.x + track.w > sw { Rect { x: sw - 7.0, ..track } } else { track };
            rrect(track, 2.5, Color::new(1.0, 1.0, 1.0, 0.08));
            let bar_h = (room * room / h).max(24.0);
            let bar_y = track.y + (room - bar_h) * form.scroll / max_scroll;
            rrect(
                Rect::new(track.x, bar_y, 5.0, bar_h),
                2.5,
                Color::new(GOLD.r, GOLD.g, GOLD.b, 0.7),
            );
        }
        // A field getting the keyboard (Tab moves it) is scrolled into view.
        let mut focused: Option<(u64, f32)> = None;
        let x = r.x + PAD;
        crate::icons::draw(Icon::Person, vec2(x + 12.0, r.y + 24.0 + 13.0), 24.0, GOLD);
        ui.text(t("Your account"), x + 34.0, text_y(r.y + 24.0 + 13.0, 22.0), 22.0, GOLD, true);
        let mut y = r.y + 24.0 + 26.0 + 20.0;
        // Username: a few changes in all.
        ui.text(t("Username"), x, y, 15.0, GOLD, true);
        y += 24.0;
        let left = online.account.as_ref().map(|a| a.name_changes_left);
        // Once the free changes are used, another costs coins (if the
        // server sells them).
        let price = online.account.as_ref().map_or(0, |a| a.name_change_price);
        let can_change = left.is_some_and(|n| n > 0 || price > 0);
        let note = match left {
            Some(0) if price > 0 => tw(
                "Your free changes are used: another costs {n} coins.",
                &[("n", &crate::online::ops::thousands(price as i64))],
            ),
            Some(0) => t("You can't change your username again.").to_string(),
            Some(n) => tw(
                "{n} of {total} name changes left",
                &[("n", &n), ("total", &snook_legend_proto::MAX_NAME_CHANGES)],
            ),
            None => String::new(),
        };
        ui.text(&note, x, y, 13.0, MUTED, false);
        y += 17.0 + 6.0;
        let current = online.me.as_ref().map(|me| me.username.clone()).unwrap_or_default();
        let wanted = form.new_name.trim().to_string();
        if form.confirm_rename.as_ref().is_some_and(|c| *c != wanted) {
            form.confirm_rename = None;
        }
        let label =
            if form.confirm_rename.is_some() { t("Click again to confirm") } else { t("Change") };
        // Wide enough for both labels: the field beside it doesn't jump.
        let both = [t("Change"), t("Click again to confirm")];
        let bw = both.iter().map(|l| ui.text_size(l, 15.0, true).x).fold(0.0, f32::max) + 32.0;
        let field = Rect::new(x, y, fw - bw - 10.0, 34.0);
        if can_change {
            if ui.focus == Some(75) {
                focused = Some((75, y));
            }
            ui.text_input(75, field, &mut form.new_name, 16);
            if form.new_name.is_empty() && ui.focus != Some(75) {
                ui.text(t("New username"), x + 10.0, text_y(y + 17.0, 14.0), 14.0, MUTED, false);
            }
        }
        let ok = can_change && snook_legend_proto::valid_username(&wanted) && wanted != current;
        if ui.button(Rect::new(x + fw - bw, y, bw, 34.0), label, ok) {
            if form.confirm_rename.is_some() {
                online.send(ClientMsg::ChangeUsername { username: wanted.clone() });
                // The session comes back under the new name: the same account.
                online.renaming_to = Some(wanted.clone());
                form.confirm_rename = None;
                form.renaming = Some(wanted);
            } else {
                form.confirm_rename = Some(wanted);
            }
        }
        y += 34.0 + 12.0;
        if supporter {
            let mut show = online.account.as_ref().is_some_and(|a| a.in_credits);
            if ui.checkbox(x, y, t("Show my name in the supporters' credits"), &mut show) {
                online.send(ClientMsg::SetCredits { show });
                if let Some(a) = online.account.as_mut() {
                    a.in_credits = show;
                }
            }
            y += credits_h;
        }
        draw_line(x, y, x + fw, y, 1.0, BORDER);
        y += 13.0;
        // Recovery e-mail.
        ui.text(t("Recovery e-mail"), x, y, 15.0, GOLD, true);
        y += 24.0;
        for line in &email_note {
            ui.text(line, x, y, 13.0, MUTED, false);
            y += 17.0;
        }
        y += 6.0;
        let saved = online.account.as_ref().and_then(|a| a.recovery_email.clone());
        if ui.focus == Some(71) {
            focused = Some((71, y));
        }
        ui.text_input(71, Rect::new(x, y, fw, 34.0), &mut form.email, 120);
        y += 38.0;
        // The password, in every case (even for the e-mail of a payment:
        // whoever pays types that e-mail, and it may be someone else's).
        if ui.focus == Some(76) {
            focused = Some((76, y));
        }
        ui.password_input(76, Rect::new(x, y, fw - 120.0, 34.0), &mut form.email_password, 72);
        if form.email_password.is_empty() && ui.focus != Some(76) {
            ui.text(t("Password"), x + 10.0, text_y(y + 17.0, 14.0), 14.0, MUTED, false);
        }
        let ok = saved.as_deref() != Some(form.email.trim()) && !form.email_password.is_empty();
        if ui.button(Rect::new(x + fw - 110.0, y, 110.0, 34.0), t("Save"), ok) {
            online.send(ClientMsg::SetRecoveryEmail {
                email: form.email.trim().to_string(),
                password: std::mem::take(&mut form.email_password),
            });
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
        if ui.focus == Some(72) {
            focused = Some((72, y));
        }
        ui.password_input(72, field, &mut form.old_password, 72);
        y += 38.0;
        ui.text(t("New"), x, text_y(y + 17.0, 13.0), 13.0, MUTED, true);
        let field = Rect::new(x + label_w, y, fw - label_w, 34.0);
        if ui.focus == Some(73) {
            focused = Some((73, y));
        }
        ui.password_input(73, field, &mut form.new_password, 72);
        y += 38.0 + 6.0;
        let ok =
            snook_legend_proto::valid_password(&form.new_password) && !form.old_password.is_empty();
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
        if ui.focus == Some(74) {
            focused = Some((74, y));
        }
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
        if focused.map(|(id, _)| id) != form.shown_field {
            form.shown_field = focused.map(|(id, _)| id);
            if let Some((_, fy)) = focused {
                if fy + 34.0 > sh - 10.0 {
                    form.scroll += fy + 34.0 - (sh - 10.0);
                } else if fy < 10.0 {
                    form.scroll -= 10.0 - fy;
                }
            }
        }
        // A press on the dimmed screen around it closes it too, while
        // nothing has been typed (it would be lost).
        let typed = [
            &form.new_name,
            &form.email_password,
            &form.old_password,
            &form.new_password,
            &form.delete_password,
        ]
        .iter()
        .any(|f| !f.is_empty());
        // Signing out (to sign in to another account) is seldom wanted: a
        // quiet link across from Close, not a button.
        let row = Rect::new(x, y + 34.0 + 24.0, fw, 40.0);
        let label = t("Sign out");
        let size = 14.0;
        let lw = 10.0 + 18.0 + 7.0 + ui.text_size(label, size, true).x + 12.0;
        let link = Rect::new(row.x + row.w - lw, row.y, lw, row.h);
        let hover = ui.hovering(link);
        if hover {
            rrect(link, 8.0, Color::new(1.0, 1.0, 1.0, 0.05));
        }
        let color = if hover { CREAM } else { MUTED };
        crate::icons::draw(Icon::SignOut, vec2(link.x + 10.0 + 9.0, link.y + 20.0), 17.0, color);
        ui.text(label, link.x + 10.0 + 18.0 + 7.0, text_y(link.y + 20.0, size), size, color, true);
        if hover && ui.clicked(link) {
            self.sign_out = true;
            self.account = None;
            ui.focus = None;
            return;
        }
        if ui.button(Rect::new(x, row.y, 140.0, 40.0), t("Close"), true)
            || ui.escape()
            || (!typed && ui.dismissed(r))
        {
            self.account = None;
            ui.focus = None;
        }
    }

    fn report_dialog(&mut self, ui: &mut Ui, online: &mut Online) {
        let Some(dialog) = self.report.as_mut() else { return };
        match dialog.draw(ui) {
            crate::report::ReportOutcome::Open => {}
            crate::report::ReportOutcome::Cancelled => self.report = None,
            crate::report::ReportOutcome::Send(msg) => {
                online.send(msg);
                self.report = None;
            }
        }
    }
}

/// A rounded tag in the tier's colour; `mid` is its vertical middle.
/// Returns its width.
/// The account button (own profile), square, on the name's line.
const ACCOUNT_W: f32 = 36.0;

/// The width of the ghost's button (one's own, or another's).
fn ghost_width(ui: &Ui, own: bool) -> f32 {
    let label = if own { t("Play your ghost") } else { t("Play their ghost") };
    ui.text_size(label, 14.0, true).x + 32.0 + 22.0
}

/// One's own flag by the name (`r`, `drawn`: a flag was drawn there):
/// a click opens the list of countries. Under the pointer it lifts, with a
/// pencil over it; with no country yet, a globe in a frame invites one.
fn country_flag(ui: &mut Ui, r: Rect, drawn: bool, country: &str) {
    let hit = Rect::new(r.x - 4.0, r.y - 4.0, r.w + 8.0, r.h + 8.0);
    let hover = ui.hovering(hit);
    ui.block(hit);
    if !drawn {
        rrect(r, 4.0, Color::new(1.0, 1.0, 1.0, if hover { 0.08 } else { 0.04 }));
        let edge = if hover { GOLD } else { BORDER };
        rrect_lines(r, 4.0, 1.0, edge);
        let icon = if hover { Icon::Pencil } else { Icon::Globe };
        let color = if hover { GOLD } else { MUTED };
        crate::icons::draw(icon, vec2(r.x + r.w / 2.0, r.y + r.h / 2.0), 14.0, color);
    } else if hover {
        draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.0, 0.0, 0.0, 0.55));
        let c = vec2(r.x + r.w / 2.0, r.y + r.h / 2.0);
        crate::icons::draw(Icon::Pencil, c, 14.0, GOLD);
        rrect_lines(Rect::new(r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0), 4.0, 1.5, GOLD);
    }
    if hover {
        ui.tooltip(t("Change country"), hit);
        if ui.clicked(hit) {
            crate::countries::open_at(country);
        }
    }
}

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

/// The rating after each rated game under `rules`, as a line over a soft
/// fill, with the leagues it crosses, a scale, and the rating now by its
/// last point. The rules it is for in its top right corner.
fn rating_chart(ui: &mut Ui, p: &Profile, rules: Rules, area: Rect) {
    let pts = p.rating_history.get(rules);
    if pts.len() < 2 {
        let text = if p.relation == Relation::Me {
            t("Play rated games to see your rating here.")
        } else {
            t("No rated games yet.")
        };
        empty_state(ui, area, Icon::Trophy, text);
        return;
    }
    let frame = Rect::new(area.x, area.y, area.w, area.h - 22.0);
    rrect(frame, 8.0, Color::new(1.0, 1.0, 1.0, 0.03));
    // Which rules: inside the chart, in its corner (the cards above switch them).
    let tag_w = crate::rulesui::tag_w(ui, rules, 12.0);
    crate::rulesui::tag(ui, rules, frame.x + frame.w - tag_w - 10.0, frame.y + 14.0, 12.0);
    let chart = Rect::new(frame.x + 44.0, frame.y + 30.0, frame.w - 58.0, frame.h - 44.0);
    let lo = pts.iter().map(|p| p.1).min().unwrap_or(0) - 15;
    let hi = pts.iter().map(|p| p.1).max().unwrap_or(0) + 15;
    let span = (hi - lo).max(1) as f32;
    let y_of = |r: i32| chart.y + chart.h * (1.0 - (r - lo) as f32 / span);
    let at =
        |k: usize, r: i32| vec2(chart.x + chart.w * k as f32 / (pts.len() - 1) as f32, y_of(r));
    // The scale: three faint lines, their ratings by them.
    for k in 0..3 {
        let r = lo + ((hi - lo) as f32 * k as f32 / 2.0).round() as i32;
        let y = y_of(r);
        draw_line(chart.x, y, chart.x + chart.w, y, 1.0, Color::new(1.0, 1.0, 1.0, 0.05));
        let label = r.to_string();
        let lw = ui.text_size(&label, 11.0, false).x;
        ui.text(&label, chart.x - 8.0 - lw, text_y(y, 11.0), 11.0, MUTED, false);
    }
    // The leagues it crosses, dashed, named at the right.
    for tier in snook_legend_proto::Tier::ALL {
        let m = tier.min_rating();
        if m > lo && m < hi {
            let y = y_of(m);
            let c = tier_color(tier);
            let mut x = chart.x;
            while x < chart.x + chart.w {
                let e = (x + 6.0).min(chart.x + chart.w);
                draw_line(x, y, e, y, 1.0, Color::new(c.r, c.g, c.b, 0.35));
                x += 10.0;
            }
            let name = t(tier.name());
            let nw = ui.text_size(name, 11.0, true).x;
            ui.text(name, chart.x + chart.w - nw, y - 15.0, 11.0, c, true);
        }
    }
    // A soft fill under the line, then the line, green up and red down.
    let base = chart.y + chart.h;
    for k in 1..pts.len() {
        let (a, b) = (at(k - 1, pts[k - 1].1), at(k, pts[k].1));
        crate::paint::draw_triangle(
            a,
            b,
            vec2(b.x, base),
            Color::new(GOLD.r, GOLD.g, GOLD.b, 0.06),
        );
        crate::paint::draw_triangle(
            a,
            vec2(b.x, base),
            vec2(a.x, base),
            Color::new(GOLD.r, GOLD.g, GOLD.b, 0.06),
        );
    }
    for k in 1..pts.len() {
        let (a, b) = (at(k - 1, pts[k - 1].1), at(k, pts[k].1));
        let color = if pts[k].1 >= pts[k - 1].1 { WIN } else { ALERT };
        draw_line(a.x, a.y, b.x, b.y, 2.0, color);
    }
    // A dot on each game while there are few.
    if pts.len() <= 40 {
        for (k, (_, r)) in pts.iter().enumerate() {
            let c = at(k, *r);
            draw_circle(c.x, c.y, 2.5, Color::new(0.85, 0.8, 0.7, 0.9));
        }
    }
    // The rating now, by its point.
    let last = at(pts.len() - 1, pts[pts.len() - 1].1);
    draw_circle(last.x, last.y, 5.0, GOLD);
    draw_circle_lines(last.x, last.y, 8.0, 1.5, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.4));
    let now_r = pts[pts.len() - 1].1.to_string();
    let nw = ui.text_size(&now_r, 13.0, true).x + 12.0;
    let chip =
        Rect::new((last.x - nw - 10.0).max(chart.x), (last.y - 24.0).max(frame.y + 4.0), nw, 18.0);
    rrect(chip, 9.0, Color::new(0.06, 0.045, 0.035, 0.95));
    rrect_lines(chip, 9.0, 1.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.6));
    ui.text_centered(&now_r, chip.x + chip.w / 2.0, chip.y + 2.5, 13.0, GOLD, true);
    // When: the first game and the last (once, if the same day).
    let first = date(pts[0].0);
    let now = date(pts[pts.len() - 1].0);
    let y = frame.y + frame.h + 6.0;
    if first == now {
        ui.text_centered(&now, chart.x + chart.w / 2.0, y, 11.0, MUTED, false);
    } else {
        ui.text(&first, chart.x, y, 11.0, MUTED, false);
        let w = ui.text_size(&now, 11.0, false).x;
        ui.text(&now, chart.x + chart.w - w, y, 11.0, MUTED, false);
    }
}

/// Lifetime numbers from the games with statistics, and games left unfinished.
fn numbers(ui: &mut Ui, p: &Profile, area: Rect) {
    let c = &p.career;
    if c.games == 0 && p.abandon_rate == 0 {
        if p.cue_wins.is_empty() && p.cue_stats.is_empty() {
            empty_state(ui, area, Icon::Star, t("No games yet."));
        } else {
            cue_card(ui, p, Rect::new(area.x, area.y, area.w, CUE_CARD_H));
        }
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
    // Room under the numbers for the cue and its level.
    let cue_h = if p.cue_wins.is_empty() && p.cue_stats.is_empty() { 0.0 } else { CUE_CARD_H };
    let ch = ((area.h - 2.0 * gap - cue_h - if cue_h > 0.0 { gap } else { 0.0 }) / 3.0).min(72.0);
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
    if cue_h > 0.0 {
        let r = Rect::new(area.x, area.y + 3.0 * (ch + gap), area.w, cue_h);
        cue_card(ui, p, r);
    }
}

/// Height of [`cue_card`].
const CUE_CARD_H: f32 = 110.0;

/// The cue in use, its level (online wins with it) and what it has done;
/// the cue with the most wins too, when it is another.
fn cue_card(ui: &mut Ui, p: &Profile, r: Rect) {
    use snook_legend_proto::shop::{self, Category};
    rrect(r, 8.0, Color::new(1.0, 1.0, 1.0, 0.04));
    let code = shop::item(&p.player.cue)
        .filter(|i| i.category == Category::Cue)
        .map_or(Category::Cue.default_item(), |i| i.code);
    let wins = |c: &str| p.cue_wins.iter().find(|(x, _)| x == c).map_or(0, |(_, n)| *n);
    let pic_w = (r.w * 0.36).min(170.0);
    let pic = Rect::new(r.x + 8.0, r.y + 8.0, pic_w, r.h - 16.0);
    crate::shop::cue_picture(ui, code, pic, shop::cue_level(wins(code)));
    let x = pic.x + pic.w + 14.0;
    let room = r.x + r.w - 12.0 - x;
    let name = shop::item(code).map_or(code, |i| t(i.name));
    let (name, size) = ui.fitted(name, 16.0, room, true);
    ui.text(&name, x, r.y + 12.0, size, CREAM, true);
    let (line, progress, _) = crate::shop::cue_level_text(wins(code));
    let (line, size) = ui.fitted(&line, 13.0, room, false);
    ui.text(&line, x, r.y + 36.0, size, GOLD, false);
    let bar = Rect::new(x, r.y + 56.0, room, 4.0);
    rrect(bar, 2.0, Color::new(1.0, 1.0, 1.0, 0.1));
    rrect(Rect::new(bar.x, bar.y, bar.w * progress, bar.h), 2.0, GOLD);
    // Its record: games, wins and balls made with it.
    if let Some(s) = p.cue_stats.iter().find(|s| s.code == code).filter(|s| s.games > 0) {
        let rate = s.wins * 100 / s.games;
        let text = tw(
            "{games} games · {rate}% won · {balls} balls made",
            &[("games", &s.games), ("rate", &rate), ("balls", &s.potted)],
        );
        let (text, size) = ui.fitted(&text, 12.0, room, false);
        ui.text(&text, x, r.y + 68.0, size, CREAM, false);
    }
    if let Some((best, n)) = p.cue_wins.first().filter(|(c, n)| c != code && *n > wins(code)) {
        let best = shop::item(best).map_or(best.as_str(), |i| t(i.name));
        let text =
            tw("Most wins: {cue} (level {n})", &[("cue", &best), ("n", &shop::cue_level(*n))]);
        let (text, size) = ui.fitted(&text, 12.0, room, false);
        ui.text(&text, x, r.y + 88.0, size, MUTED, false);
    }
}

/// "Level 10 brings: Regular's teal": the next level with an item of its
/// own (none past the last).
pub fn next_reward(level: u32) -> Option<String> {
    let (at, achievement) = snook_legend_proto::xp::next_milestone(level)?;
    let item = snook_legend_proto::shop::ITEMS.iter().find(
        |i| matches!(i.price, snook_legend_proto::shop::Price::Achievement(a) if a == achievement),
    )?;
    Some(tw("Level {n} brings: {item}", &[("n", &at), ("item", &item_label(item))]))
}

/// "50 coins" or "100 coins and Skilled league chalk": what reaching a
/// league for the first time brings.
fn league_prize(tier: snook_legend_proto::Tier) -> String {
    let coins = tw("{n} coins", &[("n", &tier.prize())]);
    let item = tier.achievement().and_then(|a| {
        snook_legend_proto::shop::ITEMS.iter().find(
            |i| matches!(i.price, snook_legend_proto::shop::Price::Achievement(code) if code == a),
        )
    });
    match item {
        Some(item) => tw("{coins} and {item}", &[("coins", &coins), ("item", &item_label(item))]),
        None => coins,
    }
}

/// An item with what it is: "Legend (cue)".
fn item_label(item: &snook_legend_proto::shop::Item) -> String {
    use snook_legend_proto::shop::Category;
    let kind = match item.category {
        Category::Cue => t("cue"),
        Category::Cloth => t("cloth"),
        Category::Chalk => t("chalk"),
        _ => return t(item.name).to_string(),
    };
    format!("{} ({kind})", t(item.name))
}

/// The leagues and the levels, each with what it brings: where the player
/// stands on both roads and everything still ahead.
fn journey(ui: &mut Ui, online: &Online, p: &Profile, rules: Rules, area: Rect) {
    use snook_legend_proto::{Tier, xp};
    let player = &p.player;
    let col_w = (area.w - 24.0) / 2.0;
    let own = p.relation == Relation::Me;
    let state = |r: Rect, done: bool, here: bool| {
        let mid = r.y + r.h / 2.0;
        let c = vec2(r.x + r.w - 16.0, mid);
        if done {
            crate::icons::draw(Icon::Check, c, 14.0, WIN);
        } else if !here {
            crate::icons::draw(Icon::Lock, c, 12.0, Color::new(MUTED.r, MUTED.g, MUTED.b, 0.8));
        }
    };

    // The leagues, the highest at the top.
    let x = area.x;
    heading(ui, t("Leagues"), x, area.y + 10.0);
    let note = t("By rating. Each league pays once.");
    let (note, size) = ui.fitted(note, 12.0, col_w - 90.0, false);
    let nw = ui.text_size(&note, size, false).x;
    ui.text(&note, x + col_w - nw, area.y + 4.0, size, MUTED, false);
    let tiers: Vec<Tier> = Tier::ALL.into_iter().rev().collect();
    let top = area.y + 30.0;
    let row_h = ((area.h - 30.0) / tiers.len() as f32).min(50.0);
    // The leagues under the rules shown (each league pays once, under
    // either of the rules).
    let current = player.tier_in(rules);
    let peak = (*p.peaks.get(rules)).max(player.rating_in(rules));
    for (k, tier) in tiers.iter().enumerate() {
        let r = Rect::new(x, top + k as f32 * row_h, col_w, row_h - 5.0);
        let color = tier_color(*tier);
        let here = *tier == current && !player.guest;
        let reached = !player.guest && peak >= tier.min_rating();
        let fade = if reached || here { 1.0 } else { 0.55 };
        rrect(r, 8.0, Color::new(color.r, color.g, color.b, if here { 0.16 } else { 0.05 }));
        if here {
            rrect_lines(r, 8.0, 1.2, Color::new(color.r, color.g, color.b, 0.8));
        }
        let mid = r.y + r.h / 2.0;
        // The league's name with where it starts under it, its prize at the
        // right (on one line it was cut short).
        let two = r.h >= 30.0;
        let name_y = if two { r.y + (r.h - 30.0) / 2.0 + 1.0 } else { text_y(mid, 14.0) };
        ui.text(
            t(tier.name()),
            r.x + 12.0,
            name_y,
            14.0,
            Color::new(color.r, color.g, color.b, fade),
            true,
        );
        let from = if *tier == Tier::Rookie {
            t("where everyone starts").to_string()
        } else {
            tw("from {n}", &[("n", &tier.min_rating())])
        };
        let prize = if tier.prize() > 0 { league_prize(*tier) } else { String::new() };
        let muted = Color::new(MUTED.r, MUTED.g, MUTED.b, fade);
        let cream = Color::new(CREAM.r, CREAM.g, CREAM.b, fade);
        if two {
            ui.text(&from, r.x + 12.0, name_y + 18.0, 11.0, muted, false);
            let (prize, size) = ui.fitted(&prize, 12.5, r.w - 150.0, false);
            let pw = ui.text_size(&prize, size, false).x;
            ui.text(&prize, r.x + r.w - 34.0 - pw, text_y(mid, size), size, cream, false);
        } else {
            let line = if prize.is_empty() { from } else { format!("{from}  ·  {prize}") };
            let (line, size) = ui.fitted(&line, 12.0, r.w - 170.0, false);
            let lw = ui.text_size(&line, size, false).x;
            ui.text(&line, r.x + r.w - 34.0 - lw, text_y(mid, size), size, cream, false);
        }
        state(r, reached && tier.prize() > 0, here);
    }

    // The levels: the bar, what every level pays, and the levels that
    // bring an item.
    let x = area.x + col_w + 24.0;
    let level = player.level.max(1);
    heading(ui, t("Levels"), x, area.y + 10.0);
    let note = tw("By playing. Every level pays {n} coins.", &[("n", &xp::LEVEL_COINS)]);
    let (note, size) = ui.fitted(&note, 12.0, col_w - 90.0, false);
    let nw = ui.text_size(&note, size, false).x;
    ui.text(&note, x + col_w - nw, area.y + 4.0, size, MUTED, false);
    // One ladder, the highest level at the top like the leagues': the
    // levels that bring an item, and the player's own level in its place
    // among them (lit, with the way to the next level), not on a card of
    // its own.
    #[derive(Clone, Copy)]
    enum Step {
        /// A level that brings an item (and its achievement).
        Milestone(u32, &'static str),
        /// Where the player is (on a milestone: that milestone).
        Here(Option<&'static str>),
    }
    let mut steps: Vec<Step> = Vec::new();
    let mut placed = false;
    for &(at, achievement) in xp::MILESTONES.iter() {
        if !placed && level <= at {
            placed = true;
            if level == at {
                steps.push(Step::Here(Some(achievement)));
                continue;
            }
            steps.push(Step::Here(None));
        }
        steps.push(Step::Milestone(at, achievement));
    }
    if !placed {
        steps.push(Step::Here(None));
    }
    steps.reverse();
    let xp_known = online.xp.filter(|_| own);
    let item_of = |achievement: &str| {
        snook_legend_proto::shop::ITEMS
            .iter()
            .find(|i| {
                matches!(i.price, snook_legend_proto::shop::Price::Achievement(a) if a == achievement)
            })
            .map(item_label)
            .unwrap_or_default()
    };
    let row_h = ((area.y + area.h - top) / steps.len() as f32).min(50.0);
    for (k, step) in steps.iter().enumerate() {
        let r = Rect::new(x, top + k as f32 * row_h, col_w, row_h - 5.0);
        let mid = r.y + r.h / 2.0;
        match *step {
            Step::Here(milestone) => {
                rrect(r, 8.0, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.16));
                rrect_lines(r, 8.0, 1.4, Color::new(GOLD.r, GOLD.g, GOLD.b, 0.85));
                let with_bar = xp_known.is_some() && r.h >= 34.0;
                let line_mid = if with_bar { r.y + (r.h - 9.0) / 2.0 } else { mid };
                let title = tw("Level {n}", &[("n", &level)]);
                ui.text(&title, r.x + 14.0, text_y(line_mid, 16.0), 16.0, GOLD, true);
                let title_w = ui.text_size(&title, 16.0, true).x;
                // What the right side says: the milestone's item (won), or
                // the way to the next level, or (another's) where they are.
                let mut tip = None;
                let what = match (milestone, xp_known) {
                    (Some(a), Some(xp_now)) => {
                        tip = Some(xp_line(xp_now, level));
                        item_of(a)
                    }
                    (Some(a), None) => item_of(a),
                    (None, Some(xp_now)) => xp_line(xp_now, level),
                    (None, None) => t("Current level").to_string(),
                };
                let right = r.x + r.w - if milestone.is_some() { 34.0 } else { 14.0 };
                let room = right - (r.x + 14.0 + title_w + 16.0);
                let (what, size) = ui.fitted(&what, 13.0, room, false);
                let ww = ui.text_size(&what, size, false).x;
                ui.text(&what, right - ww, text_y(line_mid, size), size, CREAM, false);
                if milestone.is_some() {
                    state(Rect::new(r.x, r.y, r.w, line_mid * 2.0 - 2.0 * r.y), true, true);
                }
                if let (true, Some(xp_now)) = (with_bar, xp_known) {
                    let bar = Rect::new(r.x + 14.0, r.y + r.h - 11.0, r.w - 28.0, 4.0);
                    rrect(bar, 2.0, Color::new(1.0, 1.0, 1.0, 0.1));
                    let (_, into, span) = xp::progress(xp_now);
                    let k = if span == 0 { 1.0 } else { into as f32 / span as f32 };
                    if k > 0.0 {
                        rrect(Rect::new(bar.x, bar.y, (bar.w * k).max(4.0), bar.h), 2.0, GOLD);
                    }
                }
                if let Some(tip) = tip
                    && ui.hovering(r)
                {
                    ui.tooltip(&tip, r);
                }
            }
            Step::Milestone(at, achievement) => {
                let done = level >= at;
                let next = !done && xp::next_milestone(level).is_some_and(|(l, _)| l == at);
                let fade = if done || next { 1.0 } else { 0.55 };
                rrect(r, 8.0, Color::new(GOLD.r, GOLD.g, GOLD.b, if next { 0.08 } else { 0.04 }));
                let label = tw("Level {n}", &[("n", &at)]);
                ui.text(
                    &label,
                    r.x + 12.0,
                    text_y(mid, 14.0),
                    14.0,
                    Color::new(GOLD.r, GOLD.g, GOLD.b, fade),
                    true,
                );
                let mut what = item_of(achievement);
                if next {
                    what = format!(
                        "{what}  ·  {}",
                        if at - level == 1 {
                            t("1 level to go").to_string()
                        } else {
                            tw("{n} levels to go", &[("n", &(at - level))])
                        }
                    );
                }
                let (what, size) = ui.fitted(&what, 13.0, r.w - 130.0, false);
                let ww = ui.text_size(&what, size, false).x;
                ui.text(
                    &what,
                    r.x + r.w - 34.0 - ww,
                    text_y(mid, size),
                    size,
                    Color::new(CREAM.r, CREAM.g, CREAM.b, fade),
                    false,
                );
                state(r, done, false);
            }
        }
    }
}

/// The way to the next level ("120 XP to level 8"), or the top reached.
fn xp_line(xp_now: i64, level: u32) -> String {
    let (_, into, span) = snook_legend_proto::xp::progress(xp_now);
    if span == 0 {
        t("The top level.").to_string()
    } else {
        tw("{n} XP to level {next}", &[("n", &(span - into)), ("next", &(level + 1))])
    }
}
