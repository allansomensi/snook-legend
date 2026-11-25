//! Against the clock in the game: a full rack to clear alone, as fast as
//! possible (the rules in `snook_legend_core::rules::time_attack`, the
//! week's table in `snook_legend_proto::time_attack`).
//!
//! Signed in, an attempt is dealt by the server and its strokes go back to
//! it once the table is clear: the server times it and puts it on the
//! week's table. Signed out (or for practice) a rack is dealt here and the
//! best time is kept on this computer.

use macroquad::prelude::*;
use snook_legend_core::rules::time_attack::penalised;
use snook_legend_core::{BallId, Shot, ShotReport};
use snook_legend_proto::time_attack::{
    self, ATTEMPTS_PER_DAY, FOUL_PENALTY_MS, Info, Outcome, PRIZES, Refused, Stroke, clock,
};
use snook_legend_proto::{ClientMsg, ServerMsg};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_line, draw_rectangle};

use crate::config::Config;
use crate::game::{Match, Phase};
use crate::hud::HudAction;
use crate::i18n::{t, tw};
use crate::online::Online;
use crate::ui::{ALERT, BORDER, CREAM, GOLD, MUTED, Ui, rrect};

/// What the server said about the week against the clock.
#[derive(Default)]
pub struct TimeAttackNet {
    pub info: Option<Info>,
    /// An attempt the server started: (attempt, rack), for the game to set up.
    pub started: Option<(i64, u64)>,
    /// The server's answer to the attempt sent.
    pub result: Option<(i64, Result<Outcome, Refused>)>,
    /// Why the last start was refused.
    pub refused: Option<Refused>,
}

impl TimeAttackNet {
    /// Messages against the clock: None when handled here.
    pub fn message(&mut self, msg: ServerMsg) -> Option<ServerMsg> {
        match msg {
            ServerMsg::TimeAttack { info } => self.info = Some(info),
            ServerMsg::TimeAttackStarted { attempt, seed } => {
                self.refused = None;
                self.started = Some((attempt, seed));
            }
            ServerMsg::TimeAttackResult { attempt: 0, result: Err(r) } => self.refused = Some(r),
            ServerMsg::TimeAttackResult { attempt, result } => {
                self.result = Some((attempt, result))
            }
            other => return Some(other),
        }
        None
    }
}

/// An attempt on the table.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeAttackRun {
    /// The server's attempt (0: one of this computer's own).
    pub attempt: i64,
    pub seed: u64,
    /// Every stroke, with the cue ball where it was put when in hand.
    pub strokes: Vec<Stroke>,
    /// When the clock started (`get_time`).
    pub started: f64,
    /// Shots that cost a penalty so far.
    pub fouls: u32,
    /// The time when the table was cleared (seconds, penalties included).
    pub finished: Option<f32>,
    pub sent: bool,
    /// The server's answer.
    pub result: Option<Result<Outcome, Refused>>,
    /// The best on this computer before this attempt (seconds).
    pub best_before: Option<f32>,
}

impl TimeAttackRun {
    /// The clock as the player sees it: seconds since the start, with the
    /// penalties (stopped once the table is clear).
    pub fn elapsed(&self) -> f32 {
        self.finished.unwrap_or_else(|| {
            (get_time() - self.started) as f32 + self.fouls as f32 * FOUL_PENALTY_MS as f32 / 1000.0
        })
    }
}

/// The best time on this computer (seconds), from its file.
fn best_path() -> Option<std::path::PathBuf> {
    crate::config::config_dir().map(|d| d.join("time_attack_best.txt"))
}

pub fn offline_best() -> Option<f32> {
    let text = std::fs::read_to_string(best_path()?).ok()?;
    text.trim().parse::<f32>().ok().filter(|s| s.is_finite() && *s > 0.0)
}

fn keep_best(secs: f32) {
    let Some(p) = best_path() else { return };
    if offline_best().is_some_and(|b| b <= secs) {
        return;
    }
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = crate::util::write_atomically(&p, format!("{secs:.1}").as_bytes());
}

/// A time in seconds as the clock shows it.
pub fn secs_clock(secs: f32) -> String {
    clock((secs.max(0.0) * 1000.0) as u32)
}

impl TimeAttackRun {
    /// A new attempt on the rack of `seed` (`attempt`: the server's, 0 for
    /// one of this computer's own); its clock starts now.
    pub fn new(attempt: i64, seed: u64) -> TimeAttackRun {
        TimeAttackRun {
            attempt,
            seed,
            strokes: Vec::new(),
            started: get_time(),
            fouls: 0,
            finished: None,
            sent: false,
            result: None,
            best_before: offline_best(),
        }
    }
}

/// Keeps a stroke of an attempt (the cue ball's place when in hand).
pub fn record_stroke(m: &mut Match, shot: Shot) {
    let cue_ball = m.game.ball_in_hand().map(|_| m.game.ball(BallId::CUE).xy());
    if let Some(run) = m.time_attack.as_mut() {
        run.strokes.push(Stroke { shot, cue_ball });
    }
}

/// After a shot of an attempt: its penalty, and the end once the table is
/// clear. True when the shot ended the attempt (nothing more to do).
pub fn after_shot(m: &mut Match, report: &ShotReport) -> bool {
    let Some(run) = m.time_attack.as_mut() else { return false };
    if penalised(report) {
        run.fouls += 1;
        m.notify(tw("Foul: +{n} s", &[("n", &(FOUL_PENALTY_MS / 1000))]), true);
    }
    let Some(run) = m.time_attack.as_mut() else { return false };
    if report.game_over.is_none() {
        return false;
    }
    let secs = run.elapsed();
    run.finished = Some(secs);
    let offline = run.attempt == 0;
    if offline {
        keep_best(secs);
    }
    m.last_report = Some(report.clone());
    m.phase = Phase::GameOver;
    m.notify(tw("Table cleared in {time}!", &[("time", &secs_clock(secs))]), true);
    true
}

/// Each frame: an attempt the server started begins (true: a new table is
/// on, the caller resets the view), a cleared one goes to the server, and
/// its answer comes to the table.
pub fn tick(current: &mut Option<Match>, online: &mut Online, config: &Config) -> bool {
    let mut started = false;
    if !current.as_ref().is_some_and(|m| m.is_online())
        && let Some((attempt, seed)) = online.time_attack.started.take()
    {
        let name = online.me.as_ref().map_or(config.player_name.clone(), |me| me.username.clone());
        online.time_attack.result = None;
        *current = Some(Match::time_attack(attempt, seed, &name, config.guide));
        started = true;
    }
    let Some(run) = current.as_mut().and_then(|m| m.time_attack.as_mut()) else {
        return started;
    };
    if run.finished.is_some() && run.attempt != 0 && !run.sent && online.live() {
        run.sent = true;
        online.send(ClientMsg::TimeAttackSubmit {
            attempt: run.attempt,
            strokes: run.strokes.clone(),
        });
    }
    if let Some((attempt, result)) = online.time_attack.result.take()
        && attempt == run.attempt
    {
        run.result = Some(result);
    }
    started
}

/// The running clock, big, at the top of the table.
pub fn draw_clock(ui: &mut Ui, m: &Match) {
    let Some(run) = &m.time_attack else { return };
    let sw = crate::ui::vw();
    let text = secs_clock(run.elapsed());
    let size = 40.0;
    let w = ui.text_size(&text, size, true).x + 36.0;
    let r = Rect::new((sw - w) / 2.0, 54.0, w, 56.0);
    rrect(r, 10.0, Color::new(0.0, 0.0, 0.0, 0.55));
    let color = if run.finished.is_some() { GOLD } else { CREAM };
    ui.text_centered(&text, sw / 2.0, r.y + 7.0, size, color, true);
    let left = m.game.balls_left();
    let sub = if run.fouls > 0 {
        tw(
            "{n} balls left · {f} fouls (+{s} s)",
            &[("n", &left), ("f", &run.fouls), ("s", &(run.fouls * FOUL_PENALTY_MS / 1000))],
        )
    } else {
        tw("{n} balls left", &[("n", &left)])
    };
    ui.text_centered(&sub, sw / 2.0, r.y + r.h + 4.0, 13.0, MUTED, false);
}

/// The panel once the table is clear.
pub fn draw_end(ui: &mut Ui, m: &Match) -> Option<HudAction> {
    let (Phase::GameOver, Some(run)) = (&m.phase, &m.time_attack) else { return None };
    let secs = run.finished?;
    let (sw, sh) = (crate::ui::vw(), crate::ui::vh());
    let w = 460.0f32.min(sw - 20.0);
    let r = Rect::new((sw - w) / 2.0, sh / 2.0 - 120.0, w, 240.0);
    ui.panel(r);
    crate::ui::rrect_lines(r, 8.0, 2.0, GOLD);
    ui.text_centered(t("Table cleared!"), sw / 2.0, r.y + 16.0, 28.0, GOLD, true);
    ui.text_centered(&secs_clock(secs), sw / 2.0, r.y + 56.0, 34.0, CREAM, true);
    let detail = tw(
        "{shots} shots · {fouls} fouls",
        &[("shots", &run.strokes.len()), ("fouls", &run.fouls)],
    );
    ui.text_centered(&detail, sw / 2.0, r.y + 100.0, 14.0, MUTED, false);
    let (line, color) = if run.attempt == 0 {
        match run.best_before {
            Some(b) if b <= secs => {
                (tw("Your best on this computer: {time}", &[("time", &secs_clock(b))]), CREAM)
            }
            _ => (t("A new best on this computer!").to_string(), GOLD),
        }
    } else {
        match &run.result {
            None if run.sent => (t("Sending your attempt to the server…").to_string(), MUTED),
            None => (t("Will be sent when you're back online.").to_string(), MUTED),
            Some(Ok(o)) => {
                let time = clock(o.ms);
                let text = match (o.best, o.rank) {
                    (true, Some(rank)) => tw(
                        "Counted: {time} · your best this week, #{rank}",
                        &[("time", &time), ("rank", &rank)],
                    ),
                    (true, None) => tw("Counted: {time} · your best this week", &[("time", &time)]),
                    _ => tw("Counted: {time}", &[("time", &time)]),
                };
                (text, if o.best { GOLD } else { CREAM })
            }
            Some(Err(Refused::NotStarted)) => {
                (t("Too late: an attempt must be sent within 20 minutes.").to_string(), ALERT)
            }
            Some(Err(_)) => (t("The attempt could not be counted.").to_string(), ALERT),
        }
    };
    let (line, size) = ui.fitted(&line, 14.0, w - 40.0, false);
    ui.text_centered(&line, sw / 2.0, r.y + 130.0, size, color, false);
    let bw = ((w - 60.0) / 2.0).min(200.0);
    let back = Rect::new(sw / 2.0 - bw - 6.0, r.y + 178.0, bw, 40.0);
    let again = Rect::new(sw / 2.0 + 6.0, r.y + 178.0, bw, 40.0);
    let mut action = None;
    if ui.button(back, t("See the week's table"), true) {
        action = Some(HudAction::MainMenu);
    }
    if ui.primary_button(again, t("Try again")) {
        action = Some(HudAction::TimeAttackAgain);
    }
    action
}

/// The tab of the screen against the computer: the rules, the player's
/// best and attempts, the week's table and last week's winners. Returns
/// true to start an attempt of this computer's own (not counted).
pub fn draw_tab(ui: &mut Ui, online: &mut Online, asked: &mut Option<i64>, body: Rect) -> bool {
    let mut offline_start = false;
    let week = time_attack::week_of(crate::util::unix_now());
    if online.signed_in() && *asked != Some(week) {
        *asked = Some(week);
        online.send(ClientMsg::TimeAttack);
    }
    let left_w = (body.w * 0.46).floor();
    let x = body.x + 4.0;
    let w = left_w - 8.0;
    let mut y = body.y + 6.0;
    let say = |ui: &Ui, text: &str, size: f32, color: Color, y: &mut f32| {
        for l in ui.wrap(text, w, size) {
            ui.text(&l, x, *y, size, color, false);
            *y += size + 5.0;
        }
    };
    ui.text(t("Against the clock"), x, y, 20.0, GOLD, true);
    y += 32.0;
    say(
        ui,
        t("Clear the table as fast as you can: any order, the 8 last. Each foul adds 10 seconds."),
        14.0,
        CREAM,
        &mut y,
    );
    y += 4.0;
    say(
        ui,
        &tw(
            "{n} attempts a day count on the week's table. The week's three quickest win {a}, {b} and {c} coins.",
            &[("n", &ATTEMPTS_PER_DAY), ("a", &PRIZES[0]), ("b", &PRIZES[1]), ("c", &PRIZES[2])],
        ),
        13.0,
        MUTED,
        &mut y,
    );
    if let Some(best) = offline_best() {
        say(
            ui,
            &tw("Your best on this computer: {time}", &[("time", &secs_clock(best))]),
            13.0,
            MUTED,
            &mut y,
        );
    }
    y += 8.0;
    let info = online.time_attack.info.clone().filter(|i| i.week == week);
    if online.signed_in() {
        if let Some(info) = &info {
            let stat = |ui: &Ui, label: &str, value: &str, y: &mut f32| {
                ui.text(label, x, *y, 13.0, MUTED, false);
                let vw = ui.text_size(value, 15.0, true).x;
                ui.text(value, x + w - vw, *y - 1.0, 15.0, CREAM, true);
                *y += 24.0;
            };
            let left = format!("{} / {}", info.attempts_left, ATTEMPTS_PER_DAY);
            stat(ui, t("Attempts left today"), &left, &mut y);
            let best = match (info.best_ms, info.rank) {
                (Some(ms), Some(r)) => format!("{} · #{r}", clock(ms)),
                (Some(ms), None) => clock(ms),
                _ => t("not yet").to_string(),
            };
            stat(ui, t("Your best this week"), &best, &mut y);
            let left_secs = (info.ends_at - crate::util::unix_now()).max(0);
            let ends = tw(
                "The week ends in {d} d {h} h",
                &[("d", &(left_secs / 86_400)), ("h", &(left_secs % 86_400 / 3600))],
            );
            say(ui, &ends, 13.0, MUTED, &mut y);
            if online.time_attack.refused == Some(Refused::NoAttemptsLeft) {
                say(ui, t("No attempts left today: come back tomorrow."), 13.0, ALERT, &mut y);
            }
            let b = Rect::new(x, y + 4.0, w.min(240.0), 40.0);
            if info.attempts_left > 0 && ui.primary_button(b, t("Start an attempt")) {
                online.time_attack.refused = None;
                online.send(ClientMsg::TimeAttackStart);
            }
            y = b.y + b.h + 10.0;
        } else {
            crate::ui::spinner(vec2(x + 10.0, y + 8.0), 8.0, GOLD);
            ui.text(t("Loading the week's table…"), x + 26.0, y, 14.0, MUTED, false);
            y += 30.0;
        }
    } else {
        say(ui, t("Sign in to put your times on the week's table."), 14.0, CREAM, &mut y);
    }
    let practice = Rect::new(x, y + 2.0, w.min(240.0), 36.0);
    if ui.button(practice, t("Practice (not counted)"), true) {
        offline_start = true;
    }
    let right = Rect::new(body.x + left_w + 24.0, body.y, body.w - left_w - 24.0, body.h);
    draw_board(ui, info.as_ref(), online, right);
    offline_start
}

/// The week's table, and last week's winners under it.
fn draw_board(ui: &mut Ui, info: Option<&Info>, online: &Online, r: Rect) {
    rrect(r, 8.0, Color::new(1.0, 1.0, 1.0, 0.03));
    let x = r.x + 14.0;
    let w = r.w - 28.0;
    ui.text(t("This week"), x, r.y + 12.0, 16.0, GOLD, true);
    let Some(info) = info else { return };
    let players = tw("{n} players", &[("n", &info.players)]);
    let pw = ui.text_size(&players, 12.0, false).x;
    ui.text(&players, x + w - pw, r.y + 15.0, 12.0, MUTED, false);
    let winners_h =
        if info.last_week.is_empty() { 0.0 } else { 40.0 + 24.0 * info.last_week.len() as f32 };
    let me = online.me.as_ref().map(|m| m.username.as_str()).unwrap_or("");
    let row_h = 28.0;
    let mut y = r.y + 44.0;
    if info.board.is_empty() {
        let text = t("Nobody has cleared the table this week yet. Be the first!");
        for l in ui.wrap(text, w, 14.0) {
            ui.text(&l, x, y, 14.0, MUTED, false);
            y += 19.0;
        }
    } else {
        let rows = (((r.h - 52.0 - winners_h) / row_h).floor().max(1.0)) as usize;
        for (k, e) in info.board.iter().take(rows).enumerate() {
            let mine = e.username.eq_ignore_ascii_case(me);
            if mine {
                rrect(
                    Rect::new(r.x + 6.0, y - 2.0, r.w - 12.0, row_h - 2.0),
                    6.0,
                    Color::new(GOLD.r, GOLD.g, GOLD.b, 0.12),
                );
            }
            let color = if k < PRIZES.len() { GOLD } else { CREAM };
            ui.text(&format!("{}.", k + 1), x, y + 4.0, 14.0, color, true);
            let time = clock(e.ms);
            let tw_ = ui.text_size(&time, 14.0, true).x;
            ui.text(&time, x + w - tw_, y + 4.0, 14.0, color, true);
            ui.name_with_flag(
                &e.username,
                &e.country,
                CREAM,
                x + 34.0,
                y + 4.0,
                14.0,
                w - tw_ - 50.0,
            );
            y += row_h;
        }
    }
    if !info.last_week.is_empty() {
        let mut y = r.y + r.h - winners_h + 6.0;
        draw_line(x, y - 6.0, x + w, y - 6.0, 1.0, BORDER);
        ui.text(t("Last week's winners"), x, y + 4.0, 13.0, MUTED, true);
        y += 30.0;
        for (k, e) in info.last_week.iter().enumerate() {
            let prize = format!("{} · +{}", clock(e.ms), PRIZES.get(k).copied().unwrap_or(0));
            let pw = ui.text_size(&prize, 13.0, false).x;
            ui.text(&format!("{}.", k + 1), x, y, 13.0, GOLD, true);
            ui.text(&prize, x + w - pw, y, 13.0, MUTED, false);
            ui.name_with_flag(&e.username, &e.country, CREAM, x + 34.0, y, 13.0, w - pw - 50.0);
            y += 24.0;
        }
    }
}
