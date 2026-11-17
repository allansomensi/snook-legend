//! Correspondence ("daily") games in the game: the list in the lounge
//! (games, challenges, the latest finished), the table of a game (first
//! what the opponent played since the player last looked, then their own
//! visit with the normal controls) and the messages from the server.
//!
//! The table is an ordinary online [`Match`]: the opponent's strokes are
//! fed to it as if the server were sending them live (`shot_taken`,
//! `shot_result`…), and the player's shots go out as `corr_shoot` (see
//! [`outgoing`]). Nothing of a live match's flow is duplicated here.

use std::collections::VecDeque;

use macroquad::prelude::*;
use snook_legend_core::stats::ShotStats;
use snook_legend_core::{BallId, EightBallGame, GameOver, GameOverReason, Group, ShotReport};
use snook_legend_proto::correspondence::{CorrChallenge, CorrEvent, CorrGame, CorrResult};
use snook_legend_proto::replay::{Replay, Stroke, apply};
use snook_legend_proto::{AimState, ClientMsg, MatchResult, ServerMsg, valid_username};

#[allow(unused_imports)]
use crate::paint::{draw_circle, draw_circle_lines, draw_line, draw_rectangle};

use crate::config::AimGuide;
use crate::game::{Controller, Match, Phase, ShotMark};
use crate::i18n::{t, tw};
use crate::icons::Icon;
use crate::online::{
    Glyph, Online, OnlineAction, Tab, button_w, empty_state, fit_text, loading, placeholder,
    row_card, time_left,
};
use crate::ui::{ALERT, CREAM, GOLD, MUTED, Ui};

/// The challenge field in the lounge.
const CHALLENGE_FIELD: u64 = 4113;
/// Pause between the strokes of a replay, and the opponent's aim shown
/// before each shot.
const STROKE_GAP: f32 = 0.45;
const AIM_TIME: f32 = 0.9;
/// An unanswered "open this game" is forgotten after this long.
const OPEN_TIMEOUT: f64 = 12.0;

const WON: Color = Color::new(0.45, 1.0, 0.55, 1.0);

/// The lounge's view of this player's correspondence games.
#[derive(Default)]
pub struct Corr {
    pub games: Vec<CorrGame>,
    pub challenges: Vec<CorrChallenge>,
    /// The server sent the list.
    pub loaded: bool,
    /// A game the player asked to open, and when.
    opening: Option<(i64, f64)>,
    first: usize,
    input: String,
}

impl Corr {
    /// Games waiting for this player's move and challenges waiting for
    /// their answer (the badges).
    pub fn waiting(&self, me: &str) -> usize {
        let games = self.games.iter().filter(|g| g.my_turn()).count();
        let asked = self.challenges.iter().filter(|c| c.to.username == me).count();
        games + asked
    }
}

/// One stroke still to be shown on the table.
#[derive(Clone)]
struct Step {
    stroke: Stroke,
    /// `shots_taken()` before it.
    turn: u32,
    seat: usize,
    end: EightBallGame,
    report: Option<ShotReport>,
}

/// A correspondence game on the table (kept in the match's online state).
pub struct CorrTable {
    pub id: i64,
    /// What the server last said about the game.
    pub info: CorrGame,
    /// Strokes the table has (shown, being shown, or played here).
    known: usize,
    /// Strokes still to show.
    queue: VecDeque<Step>,
    /// How many strokes this replay has in all (for "3 of 5").
    replay_total: usize,
    wait: f32,
    /// The next shot's cue is being shown (0: not yet, 1: lined up).
    stage: u8,
    /// A stroke of the replay is still on screen.
    showing: bool,
    /// Who plays this player's seat when it is their turn.
    controller: Controller,
    /// The player took a shot since opening the game.
    played: bool,
    /// The turn passed after the player's visit (said once).
    sent: bool,
    /// The result was applied to the table.
    concluded: bool,
}

impl CorrTable {
    pub fn replaying(&self) -> bool {
        !self.queue.is_empty() || self.showing
    }
}

pub fn is_corr(m: &Match) -> bool {
    m.online.as_ref().is_some_and(|o| o.corr.is_some())
}

fn table(m: &Match) -> Option<&CorrTable> {
    m.online.as_ref()?.corr.as_ref()
}

fn table_mut(m: &mut Match) -> Option<&mut CorrTable> {
    m.online.as_mut()?.corr.as_mut()
}

/// Sends a challenge (rated or not as the lounge's switch says).
pub fn challenge(o: &mut Online, to: &str) {
    let rated = o.rated_games && !o.lan;
    o.send(ClientMsg::CorrChallenge { to: to.to_string(), rated });
}

/// A message for the server from the table, as the correspondence game
/// sends it (the opponent isn't there: aim and chat are not sent).
pub fn outgoing(c: &mut CorrTable, msg: ClientMsg) -> Option<ClientMsg> {
    match msg {
        ClientMsg::Shoot { turn, shot, called_pocket, cue_ball } => {
            c.known += 1;
            c.played = true;
            Some(ClientMsg::CorrShoot { id: c.id, turn, shot, called_pocket, cue_ball })
        }
        ClientMsg::Resign => Some(ClientMsg::CorrResign { id: c.id }),
        _ => None,
    }
}

/// What the player sees of an event (a toast, and a notification from the
/// system when they are in another window); whether it is good news.
pub fn event_text(event: &CorrEvent, name: &str) -> (String, bool) {
    let n = [("name", &name as &dyn std::fmt::Display)];
    match event {
        CorrEvent::Challenged { rated: true } => {
            (tw("{name} challenges you to a rated daily game.", &n), true)
        }
        CorrEvent::Challenged { rated: false } => {
            (tw("{name} challenges you to a casual daily game.", &n), true)
        }
        CorrEvent::Accepted => (tw("{name} accepted your daily game.", &n), true),
        CorrEvent::Declined => (tw("{name} declined your daily game.", &n), false),
        CorrEvent::Withdrawn => (tw("{name} withdrew their daily game challenge.", &n), false),
        CorrEvent::YourTurn => (tw("Your turn in the daily game against {name}.", &n), true),
        CorrEvent::Ended { won: true, timeout: true, .. } => {
            (tw("{name} ran out of time: you won the daily game.", &n), true)
        }
        CorrEvent::Ended { won: true, .. } => {
            (tw("You won the daily game against {name}!", &n), true)
        }
        CorrEvent::Ended { won: false, timeout: true, .. } => {
            (tw("You ran out of time in the daily game against {name}.", &n), false)
        }
        CorrEvent::Ended { won: false, .. } => {
            (tw("You lost the daily game against {name}.", &n), false)
        }
    }
}

/// Handles a correspondence message from the server.
pub fn handle(o: &mut Online, msg: ServerMsg, current: &mut Option<Match>, guide: AimGuide) {
    match msg {
        ServerMsg::CorrList { games, challenges } => {
            o.corr.loaded = true;
            if let Some(m) = current.as_mut()
                && let Some(c) = table_mut(m)
                && let Some(g) = games.iter().find(|g| g.id == c.id)
            {
                c.info = g.clone();
            }
            o.corr.games = games;
            o.corr.challenges = challenges;
        }
        ServerMsg::CorrState { game, replay, seen } => {
            let viewing = current.as_ref().and_then(table).map(|c| c.id);
            if viewing == Some(game.id) {
                if let Some(m) = current.as_mut() {
                    refresh(m, game, &replay);
                }
                return;
            }
            let asked = o.corr.opening.take().is_some_and(|(id, _)| id == game.id);
            // Never over a live game.
            let free = current.as_ref().is_none_or(is_corr);
            if asked && free {
                if let Some(g) = o.corr.games.iter_mut().find(|g| g.id == game.id) {
                    g.unseen = 0;
                }
                match open(game, &replay, seen, guide) {
                    Some(m) => *current = Some(m),
                    None => o.toast(t("This game can't be shown."), false),
                }
            }
        }
        ServerMsg::CorrShot { id, turn, end, report } => {
            if let Some(m) = current.as_mut().filter(|m| table(m).is_some_and(|c| c.id == id)) {
                m.handle_server(ServerMsg::ShotResult { turn, end, report, clock_left: 0.0 });
            }
        }
        ServerMsg::CorrShotRejected { id, turn, reason, game } => {
            if let Some(m) = current.as_mut().filter(|m| table(m).is_some_and(|c| c.id == id)) {
                if let Some(c) = table_mut(m) {
                    c.known = game.shots_taken() as usize;
                }
                m.handle_server(ServerMsg::ShotRejected { turn, reason, game });
            }
        }
        ServerMsg::CorrNotice { id, opponent, event } => {
            let (text, good) = event_text(&event, &opponent);
            let viewing = current.as_ref().and_then(table).is_some_and(|c| c.id == id);
            // Watching that game: show what happened at once.
            if viewing && matches!(event, CorrEvent::YourTurn | CorrEvent::Ended { .. }) {
                o.send(ClientMsg::CorrOpen { id });
            }
            // Development aid: SNOOK_LEGEND_AUTODAILY=1 accepts challenges.
            if crate::dev::flag("SNOOK_LEGEND_AUTODAILY")
                && matches!(event, CorrEvent::Challenged { .. })
            {
                o.send(ClientMsg::CorrAnswer { id, accept: true });
            }
            match current.as_mut().filter(|_| viewing) {
                Some(m) => m.notify(text.clone(), false),
                None => o.toast(text.clone(), good),
            }
            o.attention = Some(text);
        }
        _ => {}
    }
}

/// Asks the server for a game, to open it at the table.
pub fn request(o: &mut Online, id: i64) {
    o.corr.opening = Some((id, get_time()));
    o.send(ClientMsg::CorrOpen { id });
}

/// Applies one stroke to `game`, counting it in the numbers; the report of
/// a shot.
fn account(
    game: &mut EightBallGame,
    stroke: &Stroke,
    stats: &mut [ShotStats; 2],
    timeline: &mut Vec<ShotMark>,
) -> Option<Option<ShotReport>> {
    let shooter = game.current_player();
    let in_hand = game.ball_in_hand().is_some();
    let cue = match stroke {
        Stroke::Shot { cue_ball: Some(p), .. } if in_hand => *p,
        _ => game.ball(BallId::CUE).xy(),
    };
    let report = apply(game, stroke).ok()?;
    if let (Some(r), Stroke::Shot { shot, .. }) = (&report, stroke) {
        stats[shooter % 2].add(r, in_hand);
        stats[shooter % 2].add_spin(shot, r);
        let made = if r.is_foul() {
            0
        } else {
            r.pocketed.iter().filter(|(b, _)| b.is_object()).count() as u32
        };
        timeline.push(ShotMark {
            shooter: shooter % 2,
            cue: (cue.x, cue.y),
            made,
            foul: r.is_foul(),
        });
    }
    Some(report)
}

/// The strokes after the first `from` of `replay`, ready to be shown.
fn steps(replay: &Replay, from: usize) -> Option<VecDeque<Step>> {
    let mut game = replay.start();
    for s in &replay.strokes[..from] {
        apply(&mut game, s).ok()?;
    }
    let mut out = VecDeque::new();
    for s in &replay.strokes[from..] {
        let turn = game.shots_taken();
        let seat = game.current_player();
        let report = apply(&mut game, s).ok()?;
        out.push_back(Step { stroke: *s, turn, seat, end: game.clone(), report });
    }
    Some(out)
}

/// The table of a game: as it was when the player last looked, with what
/// happened since still to be shown.
pub fn open(info: CorrGame, replay: &Replay, seen: u32, guide: AimGuide) -> Option<Match> {
    let n = replay.strokes.len();
    let seen = (seen as usize).min(n);
    let mut game = replay.start();
    let mut stats = [ShotStats::default(); 2];
    let mut timeline = Vec::new();
    for s in &replay.strokes[..seen] {
        account(&mut game, s, &mut stats, &mut timeline)?;
    }
    let queue = steps(replay, seen)?;
    let seat = info.seat.min(1);
    let mut m = Match::online(
        info.id as u64,
        seat,
        &info.players,
        info.rated,
        game.clone(),
        0,
        0.0,
        None,
        guide,
        false,
    );
    m.stats = stats;
    m.timeline = timeline;
    if game.game_over().is_some() {
        m.jump_to(game);
    }
    let controller = m.controllers[seat];
    let replay_total = queue.len();
    if let Some(o) = m.online.as_mut() {
        o.corr = Some(CorrTable {
            id: info.id,
            info,
            known: n,
            queue,
            replay_total,
            wait: 0.8,
            stage: 0,
            showing: false,
            controller,
            played: false,
            sent: false,
            concluded: false,
        });
    }
    settle_controllers(&mut m);
    Some(m)
}

/// The game was sent again while on the table: show what is new.
fn refresh(m: &mut Match, info: CorrGame, replay: &Replay) {
    let Some(c) = table_mut(m) else { return };
    c.info = info;
    let n = replay.strokes.len();
    if n > c.known
        && let Some(new) = steps(replay, c.known)
    {
        if !c.replaying() {
            c.replay_total = 0;
            c.wait = 0.6;
        }
        c.replay_total += new.len();
        c.queue.extend(new);
        c.known = n;
    }
    settle_controllers(m);
}

/// Only the player's own turn (after any replay) is theirs to play.
fn settle_controllers(m: &mut Match) {
    let Some(o) = m.online.as_ref() else { return };
    let seat = o.seat;
    let Some(c) = o.corr.as_ref() else { return };
    let mine = !c.replaying() && m.game.game_over().is_none() && m.game.current_player() == seat;
    let want = if mine { c.controller } else { Controller::Remote };
    let was = m.controllers[seat];
    m.controllers[seat] = want;
    // The turn just came back (after the replay): start it afresh.
    if was != want && mine && matches!(m.phase, Phase::Turn { .. }) {
        let game = m.game.clone();
        m.jump_to(game);
    }
}

/// What to do next on the table.
enum Next {
    Aim(Option<::glam::DVec2>, AimState),
    Shoot(Step),
    Other(Step),
}

/// Runs the table of a correspondence game: shows the strokes still to be
/// shown, one by one, then gives the player their turn (call every frame).
pub fn drive(m: &mut Match, dt: f32) {
    if !is_corr(m) {
        return;
    }
    let at_rest = m.at_rest() || matches!(m.phase, Phase::GameOver);
    let yaw = m.aim.yaw;
    let next = {
        let c = table_mut(m).expect("checked");
        if c.showing && at_rest {
            c.showing = false;
        }
        match c.queue.front() {
            Some(_) if !at_rest => None,
            Some(step) => {
                c.wait -= dt;
                if c.wait > 0.0 {
                    None
                } else {
                    match step.stroke {
                        Stroke::Shot { shot, called, cue_ball, .. } if c.stage == 0 => {
                            c.stage = 1;
                            c.wait = AIM_TIME;
                            let aim = AimState {
                                yaw: shot.yaw,
                                elevation: shot.elevation,
                                tip: shot.tip,
                                power: 0.0,
                                called_pocket: called,
                            };
                            Some(Next::Aim(cue_ball, aim))
                        }
                        Stroke::Shot { .. } => {
                            c.stage = 0;
                            c.wait = STROKE_GAP;
                            c.showing = true;
                            c.queue.pop_front().map(Next::Shoot)
                        }
                        _ => {
                            c.wait = STROKE_GAP;
                            c.queue.pop_front().map(Next::Other)
                        }
                    }
                }
            }
            None => None,
        }
    };
    match next {
        Some(Next::Aim(cue_ball, aim)) => {
            if let Some(position) = cue_ball {
                m.handle_server(ServerMsg::OpponentCueBall { position });
            }
            // From where the cue is now, so it glides onto the shot.
            let start = AimState { yaw, ..aim };
            m.handle_server(ServerMsg::OpponentAim { aim: start });
            m.handle_server(ServerMsg::OpponentAim { aim });
        }
        Some(Next::Shoot(step)) => {
            let Stroke::Shot { shot, called, cue_ball, .. } = step.stroke else { return };
            m.handle_server(ServerMsg::ShotTaken {
                seat: step.seat,
                turn: step.turn,
                shot,
                called_pocket: called,
                cue_ball,
                timeout: false,
            });
            if let Some(report) = step.report {
                m.handle_server(ServerMsg::ShotResult {
                    turn: step.turn,
                    end: step.end,
                    report,
                    clock_left: 0.0,
                });
            }
        }
        Some(Next::Other(step)) => {
            if let Stroke::TimeFoul = step.stroke {
                m.handle_server(ServerMsg::TimeFoul {
                    seat: step.seat,
                    game: step.end,
                    clock_left: 0.0,
                });
            }
            // A resignation or a turn that ran out: the result says it.
        }
        None => {}
    }
    settle_controllers(m);
    finish_if_over(m);
    // The player's visit is over: the turn goes to the opponent.
    let seat = m.online.as_ref().map_or(0, |o| o.seat);
    let passed = m.at_rest() && m.game.game_over().is_none() && m.game.current_player() != seat;
    let c = table_mut(m).expect("checked");
    if c.played && !c.sent && passed && !c.replaying() {
        c.sent = true;
        m.notify(t("Turn sent."), false);
    }
}

/// The game is over on the server and everything was shown: the result.
fn finish_if_over(m: &mut Match) {
    let in_progress = m.shot_in_progress();
    let Some(c) = table(m) else { return };
    if c.concluded || c.replaying() {
        return;
    }
    let Some(r) = c.info.result.clone() else { return };
    let rated = c.info.rated;
    if in_progress {
        return;
    }
    if let Some(c) = table_mut(m) {
        c.concluded = true;
    }
    if m.game.game_over().is_none() {
        // A resignation or a turn that ran out: said in the words of this
        // kind of game.
        let loser = m.names[1 - r.winner.min(1)].clone();
        let text = if r.timeout {
            tw("{name} ran out of time.", &[("name", &loser)])
        } else {
            tw("{name} resigned.", &[("name", &loser)])
        };
        m.game.conclude(r.winner, r.reason);
        m.notify(text, true);
        m.wins[r.winner.min(1)] += 1;
        m.games_played += 1;
        m.stop_replay();
        m.phase = Phase::GameOver;
    }
    let result = MatchResult {
        winner: r.winner,
        reason: r.reason,
        rated,
        rating_delta: r.delta,
        new_ratings: r.ratings,
    };
    m.handle_server(ServerMsg::MatchEnded { result });
}

/// How a correspondence game ended, when its own words are needed (a turn
/// that ran out is not "leaving the game").
pub fn result_line(m: &Match, over: GameOver) -> Option<String> {
    let c = table(m)?;
    let timeout = c.info.result.as_ref().is_some_and(|r| r.timeout);
    (timeout && over.reason == GameOverReason::Abandoned)
        .then(|| tw("{name} ran out of time", &[("name", &m.names[1 - over.winner.min(1)])]))
}

/// "2 d 5 h", "5 h", "12 min": the time left for a turn, short.
pub fn short_left(secs: i64) -> String {
    let secs = secs.max(0);
    if secs >= 86_400 {
        tw("{d} d {h} h", &[("d", &(secs / 86_400)), ("h", &(secs % 86_400 / 3600))])
    } else if secs >= 3600 {
        tw("{n} h", &[("n", &(secs / 3600))])
    } else {
        tw("{n} min", &[("n", &(secs / 60).max(1))])
    }
}

/// The time left on the active player's card: (text, colour).
pub fn card_clock(m: &Match, seat: usize) -> Option<(String, Color)> {
    let c = table(m)?;
    if m.game.game_over().is_some() || m.game.current_player() != seat || c.replaying() {
        return None;
    }
    let left = c.info.deadline - crate::game::unix_now();
    let color = if left < 6 * 3600 { ALERT } else { GOLD };
    Some((short_left(left), color))
}

/// The line under the top bar while waiting for the opponent.
pub fn status(m: &Match) -> Option<String> {
    let c = table(m)?;
    let o = m.online.as_ref()?;
    if c.replaying() || m.game.game_over().is_some() || m.game.current_player() == o.seat {
        return None;
    }
    let name = &m.names[1 - o.seat];
    Some(tw("Waiting for {name} to play", &[("name", name)]))
}

/// What the correspondence bar asked for.
pub enum BarAction {
    None,
    /// Back to the list of daily games.
    Back,
}

/// The bottom bar when the player has nothing to play: the replay of what
/// happened (with "Skip"), or the turn sent / the opponent's turn (with the
/// way back to the list). Drawn over the empty bar.
pub fn draw_bar(ui: &mut Ui, m: &mut Match, bar: Rect) -> BarAction {
    let Some(c) = table(m) else { return BarAction::None };
    let seat = m.online.as_ref().map_or(0, |o| o.seat);
    if m.game.game_over().is_some() && !c.replaying() {
        return BarAction::None;
    }
    let cy = bar.y + bar.h / 2.0;
    let sw = bar.x + bar.w;
    if c.replaying() {
        let name = m.names[1 - seat].clone();
        let done = c.replay_total - c.queue.len() - c.showing as usize;
        let title = tw("Replaying {name}'s turn", &[("name", &name)]);
        let sub = tw(
            "Stroke {n} of {total}",
            &[("n", &(done + 1).min(c.replay_total)), ("total", &c.replay_total)],
        );
        let skip = t("Skip");
        let w = button_w(ui, skip, 36.0).max(110.0);
        crate::icons::draw(Icon::Replay, vec2(bar.x + 30.0, cy), 22.0, GOLD);
        let room = sw - w - 30.0 - (bar.x + 52.0);
        let (title, size) = fit_text(ui, &title, 16.0, room, true);
        ui.text(&title, bar.x + 52.0, cy - 17.0, size, CREAM, true);
        ui.text(&ui.ellipsize(&sub, room, 12.0), bar.x + 52.0, cy + 4.0, 12.0, MUTED, false);
        let enter = is_key_pressed(KeyCode::Enter) && !ui.wants_keyboard;
        if ui.button(Rect::new(sw - w - 14.0, cy - 18.0, w, 36.0), skip, true) || enter {
            skip_replay(m);
        }
        return BarAction::None;
    }
    if m.game.current_player() == seat || m.shot_in_progress() {
        return BarAction::None;
    }
    let name = m.names[1 - seat].clone();
    let left = short_left(c.info.deadline - crate::game::unix_now());
    let (icon, title, sub) = if c.sent {
        (
            Icon::Check,
            t("Turn sent").to_string(),
            tw(
                "{name} has {time} to play. You'll be told when it's your turn.",
                &[("name", &name), ("time", &left)],
            ),
        )
    } else {
        (
            Icon::Hourglass,
            tw("{name}'s turn", &[("name", &name)]),
            tw("{time} left to play. You'll be told when it's your turn.", &[("time", &left)]),
        )
    };
    let back = t("Back to daily games");
    let w = button_w(ui, back, 36.0).max(150.0);
    crate::icons::draw(icon, vec2(bar.x + 30.0, cy), 22.0, if c.sent { WON } else { GOLD });
    let room = sw - w - 30.0 - (bar.x + 52.0);
    let (title, size) = fit_text(ui, &title, 16.0, room, true);
    ui.text(&title, bar.x + 52.0, cy - 17.0, size, CREAM, true);
    let (sub, sub_size) = fit_text(ui, &sub, 12.0, room, false);
    ui.text(&sub, bar.x + 52.0, cy + 4.0, sub_size, MUTED, false);
    let enter = is_key_pressed(KeyCode::Enter) && !ui.wants_keyboard;
    if ui.primary_button(Rect::new(sw - w - 14.0, cy - 18.0, w, 36.0), back) || enter {
        return BarAction::Back;
    }
    BarAction::None
}

/// Shows everything still to be shown at once.
fn skip_replay(m: &mut Match) {
    let Some(c) = table_mut(m) else { return };
    let steps: Vec<Step> = c.queue.drain(..).collect();
    c.stage = 0;
    c.showing = false;
    let Some(last) = steps.last().map(|s| s.end.clone()) else { return };
    // The numbers of the skipped shots still count.
    let mut game = m.game.clone();
    let mut stats = m.stats;
    let mut timeline = std::mem::take(&mut m.timeline);
    for s in &steps {
        if account(&mut game, &s.stroke, &mut stats, &mut timeline).is_none() {
            break;
        }
    }
    m.stats = stats;
    m.timeline = timeline;
    m.notices.clear();
    m.jump_to(last);
    settle_controllers(m);
}

// ---------------------------------------------------------------------------
// The lounge.

/// A row of the list.
enum Row {
    Asked(CorrChallenge),
    Game(CorrGame),
    Sent(CorrChallenge),
}

fn rows(c: &Corr, me: &str) -> Vec<Row> {
    let mut active: Vec<&CorrGame> = c.games.iter().filter(|g| g.result.is_none()).collect();
    // Waiting for this player first, the closest deadline first.
    active.sort_by_key(|g| (!g.my_turn(), g.deadline));
    let mut done: Vec<&CorrGame> = c.games.iter().filter(|g| g.result.is_some()).collect();
    done.sort_by_key(|g| std::cmp::Reverse(g.result.as_ref().map_or(0, |r| r.at)));
    let asked = c.challenges.iter().filter(|x| x.to.username == me).cloned().map(Row::Asked);
    let sent = c.challenges.iter().filter(|x| x.from.username == me).cloned().map(Row::Sent);
    let (mine, theirs): (Vec<&CorrGame>, Vec<&CorrGame>) =
        active.into_iter().partition(|g| g.my_turn());
    asked
        .chain(mine.into_iter().cloned().map(Row::Game))
        .chain(theirs.into_iter().cloned().map(Row::Game))
        .chain(sent)
        .chain(done.into_iter().cloned().map(Row::Game))
        .collect()
}

/// A small ball of a group: solid or striped (the open table: none).
fn group_ball(x: f32, y: f32, r: f32, group: Option<Group>) {
    match group {
        Some(Group::Solids) => draw_circle(x, y, r, GOLD),
        Some(Group::Stripes) => {
            draw_circle(x, y, r, Color::new(0.96, 0.93, 0.85, 1.0));
            draw_rectangle(x - r * 0.85, y - r * 0.3, r * 1.7, r * 0.6, GOLD);
            draw_circle_lines(x, y, r, 1.0, GOLD);
        }
        None => draw_circle_lines(x, y, r, 1.2, MUTED),
    }
}

/// The groups and the balls left, the player's first: "● 3 : 5 ◍" (or
/// "open table"), with the details under the pointer.
fn draw_score(ui: &Ui, g: &CorrGame, right: f32, y: f32) -> f32 {
    let me = g.seat.min(1);
    if g.groups == [None, None] {
        let text = t("open table");
        let w = ui.text_size(text, 12.0, false).x;
        ui.text(text, right - w, y, 12.0, MUTED, false);
        return w;
    }
    let text = format!("{} : {}", g.left[me], g.left[1 - me]);
    let tw_ = ui.text_size(&text, 13.0, true).x;
    let r = 6.0;
    let x1 = right - r;
    group_ball(x1, y + 7.0, r, g.groups[1 - me]);
    let tx = x1 - r - 6.0 - tw_;
    ui.text(&text, tx, y, 13.0, CREAM, true);
    group_ball(tx - 6.0 - r, y + 7.0, r, g.groups[me]);
    let w = tw_ + 4.0 * r + 12.0;
    let area = Rect::new(right - w, y - 2.0, w, 18.0);
    if ui.hovering(area) {
        let group = |k: usize| g.groups[k].map(crate::i18n::group).unwrap_or(t("open table"));
        let tip = tw(
            "You: {mine}, {a} left · {name}: {theirs}, {b} left",
            &[
                ("mine", &group(me)),
                ("a", &g.left[me]),
                ("name", &g.players[1 - me].username),
                ("theirs", &group(1 - me)),
                ("b", &g.left[1 - me]),
            ],
        );
        ui.tooltip(&tip, area);
    }
    w
}

/// The lounge's "Daily" tab: challenge someone, then the games and the
/// challenges, the player's turns first.
pub fn draw_tab(o: &mut Online, ui: &mut Ui, area: Rect) -> Option<OnlineAction> {
    if o.corr.opening.is_some_and(|(_, at)| get_time() - at > OPEN_TIMEOUT) {
        o.corr.opening = None;
    }
    let mut y = area.y;
    let label = t("Challenge");
    let bw = button_w(ui, label, 34.0).max(110.0);
    let input = Rect::new(area.x, y, area.w - bw - 8.0, 34.0);
    let enter = ui.text_input(CHALLENGE_FIELD, input, &mut o.corr.input, 16);
    placeholder(ui, CHALLENGE_FIELD, input, &o.corr.input, t("Challenge a player by username"));
    let ok = valid_username(o.corr.input.trim());
    if (ui.button(Rect::new(input.x + input.w + 8.0, y, bw, 34.0), label, ok) || enter) && ok {
        let to = o.corr.input.trim().to_string();
        challenge(o, &to);
        o.corr.input.clear();
    }
    y += 34.0 + 10.0;
    let toggle_w = area.w.min(280.0);
    o.rated_toggle(ui, Rect::new(area.x, y, toggle_w, 32.0));
    let hint = t("3 days for each turn");
    let hint_room = area.w - toggle_w - 14.0;
    if ui.text_size(hint, 12.0, false).x <= hint_room {
        ui.text(hint, area.x + toggle_w + 14.0, y + 9.0, 12.0, MUTED, false);
    }
    y += 32.0 + 12.0;
    let list = Rect::new(area.x, y, area.w, area.y + area.h - y);
    if !o.corr.loaded {
        loading(ui, list);
        return None;
    }
    let me = o.me.as_ref().map(|m| m.username.clone()).unwrap_or_default();
    let rows = rows(&o.corr, &me);
    if rows.is_empty() {
        let text =
            t("No daily games yet. Challenge a friend: each of you plays a turn whenever you can.");
        if empty_state(ui, list, Glyph::Icon(Icon::Hourglass), text, Some(t("Friends"))) {
            o.tab = Tab::Friends;
            o.send(ClientMsg::Friends);
        }
        return None;
    }
    let row_h = 54.0;
    let visible = (list.h / row_h).floor().max(1.0) as usize;
    o.corr.first = ui.scroll_list(list, o.corr.first, rows.len(), visible);
    let row_w = list.w - if rows.len() > visible { 12.0 } else { 0.0 };
    let now = crate::game::unix_now();
    let mut send = None;
    let mut open = None;
    let opening = o.corr.opening.map(|(id, _)| id);
    for (k, row) in rows.iter().skip(o.corr.first).take(visible).enumerate() {
        let r = Rect::new(list.x, list.y + k as f32 * row_h, row_w, row_h - 6.0);
        let by = r.y + (r.h - 30.0) / 2.0;
        match row {
            Row::Asked(c) => {
                row_card(ui, r, false);
                let accept = t("Accept");
                let decline = t("Decline");
                let w = button_w(ui, accept, 30.0).max(button_w(ui, decline, 30.0)).max(86.0);
                let decline_r = Rect::new(r.x + r.w - 7.0 - w, by, w, 30.0);
                let accept_r = Rect::new(decline_r.x - 8.0 - w, by, w, 30.0);
                draw_circle(r.x + 14.0, r.y + r.h / 2.0, 4.5, GOLD);
                let room = accept_r.x - 12.0 - (r.x + 28.0);
                ui.player_name(&c.from, r.x + 28.0, r.y + 7.0, 15.0, room);
                let kind = if c.rated { t("rated") } else { t("casual") };
                let sub = tw("challenges you · {kind}", &[("kind", &kind)]);
                let (sub, size) = fit_text(ui, &sub, 12.0, room, false);
                ui.text(&sub, r.x + 28.0, r.y + 27.0, size, GOLD, false);
                if ui.primary_button(accept_r, accept) {
                    send = Some(ClientMsg::CorrAnswer { id: c.id, accept: true });
                }
                if ui.button(decline_r, decline, true) {
                    send = Some(ClientMsg::CorrAnswer { id: c.id, accept: false });
                }
            }
            Row::Sent(c) => {
                row_card(ui, r, false);
                let label = t("Cancel");
                let w = button_w(ui, label, 30.0).max(96.0);
                let b = Rect::new(r.x + r.w - 7.0 - w, by, w, 30.0);
                draw_circle_lines(r.x + 14.0, r.y + r.h / 2.0, 4.5, 1.2, MUTED);
                let room = b.x - 12.0 - (r.x + 28.0);
                ui.player_name(&c.to, r.x + 28.0, r.y + 7.0, 15.0, room);
                let kind = if c.rated { t("rated") } else { t("casual") };
                let sub = tw(
                    "waiting for an answer · {kind} · {time} left",
                    &[("kind", &kind), ("time", &time_left(c.expires_at - now))],
                );
                let (sub, size) = fit_text(ui, &sub, 12.0, room, false);
                ui.text(&sub, r.x + 28.0, r.y + 27.0, size, MUTED, false);
                if ui.button(b, label, true) {
                    send = Some(ClientMsg::CorrCancel { id: c.id });
                }
            }
            Row::Game(g) => {
                let hover = row_card(ui, r, true);
                let mine = g.my_turn();
                let label = match (&g.result, mine) {
                    (None, true) => t("Play"),
                    _ => t("Look"),
                };
                let w = button_w(ui, label, 30.0).max(button_w(ui, t("Look"), 30.0)).max(84.0);
                let b = Rect::new(r.x + r.w - 7.0 - w, by, w, 30.0);
                let (dot, sub, sub_color) = match &g.result {
                    None if mine => (
                        GOLD,
                        tw("your turn · {time} left", &[("time", &time_left(g.deadline - now))]),
                        GOLD,
                    ),
                    None => (
                        Color::new(MUTED.r, MUTED.g, MUTED.b, 0.6),
                        tw("their turn · {time} left", &[("time", &time_left(g.deadline - now))]),
                        MUTED,
                    ),
                    Some(res) => finished_line(g, res),
                };
                draw_circle(r.x + 14.0, r.y + r.h / 2.0, 4.5, dot);
                // The score on the right of the name line, before the button.
                let score_w = draw_score(ui, g, b.x - 12.0, r.y + 8.0);
                let room = b.x - 12.0 - score_w - 12.0 - (r.x + 28.0);
                let opp = g.opponent();
                let used = ui.player_name(opp, r.x + 28.0, r.y + 7.0, 15.0, room);
                let rating = opp.rating.to_string();
                if used + 8.0 + ui.text_size(&rating, 12.0, false).x <= room {
                    ui.text(&rating, r.x + 28.0 + used + 8.0, r.y + 9.5, 12.0, MUTED, false);
                }
                let room = b.x - 12.0 - (r.x + 28.0);
                let (sub, size) = fit_text(ui, &sub, 12.0, room, false);
                ui.text(&sub, r.x + 28.0, r.y + 27.0, size, sub_color, false);
                if opening == Some(g.id) {
                    ui.button(b, "", false);
                    crate::ui::spinner(vec2(b.x + b.w / 2.0, b.y + b.h / 2.0), 7.0, GOLD);
                } else if mine && g.result.is_none() {
                    if ui.primary_button(b, label) {
                        open = Some(g.id);
                    }
                } else if ui.button(b, label, true) {
                    open = Some(g.id);
                }
                if hover && ui.clicked(r) && !b.contains(ui.mouse()) {
                    open = Some(g.id);
                }
            }
        }
    }
    if let Some(msg) = send {
        o.send(msg);
    }
    if let Some(id) = open {
        request(o, id);
    }
    None
}

/// A finished game's line: (dot colour, text, text colour).
fn finished_line(g: &CorrGame, r: &CorrResult) -> (Color, String, Color) {
    let me = g.seat.min(1);
    let won = r.winner == me;
    let how = if r.timeout {
        if won { t("the opponent ran out of time") } else { t("ran out of time") }
    } else {
        crate::i18n::history_reason(r.reason, won)
    };
    let mut text = if won {
        tw("won · {how}", &[("how", &how)])
    } else {
        tw("lost · {how}", &[("how", &how)])
    };
    if g.rated {
        let d = r.delta[me];
        text = format!("{text} · {}{d}", if d >= 0 { "+" } else { "" });
    }
    let color = if won { WON } else { ALERT };
    (color, text, if won { CREAM } else { MUTED })
}

#[cfg(test)]
mod tests {
    use super::*;
    use snook_legend_core::{Difficulty, ai};
    use snook_legend_proto::{PlayerInfo, PlayerStatus};

    fn player(name: &str) -> PlayerInfo {
        PlayerInfo {
            username: name.into(),
            rating: 1200,
            games: 0,
            wins: 0,
            status: PlayerStatus::Available,
            country: "BR".into(),
            role: Default::default(),
            guest: false,
            vip: false,
            cue: String::new(),
            chalk: String::new(),
            frame: String::new(),
            name_style: String::new(),
            effect: String::new(),
            status_message: String::new(),
            avatar: String::new(),
            vs_computer: false,
            away: false,
            level: 0,
            cue_level: 0,
        }
    }

    fn info(seat: usize, turn: usize, strokes: u32) -> CorrGame {
        CorrGame {
            id: 5,
            players: [player("ana"), player("bia")],
            seat,
            rated: true,
            turn,
            deadline: crate::game::unix_now() + 3600,
            strokes,
            unseen: 0,
            groups: [None, None],
            left: [7, 7],
            started_at: 0,
            updated_at: 0,
            result: None,
        }
    }

    /// A game the computer played for `n` strokes.
    fn recorded(n: usize) -> Replay {
        let mut replay = Replay::new(77, 0);
        let mut game = replay.start();
        for k in 0..n {
            let d = ai::decide(&game, Difficulty::Easy, k as u64);
            let s = Stroke::Shot {
                shot: d.shot,
                called: d.called_pocket,
                cue_ball: d.cue_ball,
                timeout: false,
            };
            apply(&mut game, &s).unwrap();
            replay.strokes.push(s);
        }
        replay
    }

    fn run(m: &mut Match, seconds: f32) {
        for _ in 0..(seconds * 60.0) as usize {
            m.update(1.0 / 60.0);
            drive(m, 1.0 / 60.0);
        }
    }

    #[test]
    fn the_unseen_strokes_are_replayed_then_the_turn_is_the_players() {
        let replay = recorded(3);
        let end = replay.play(None).unwrap().0;
        let seat = end.current_player();
        let mut m = open(info(seat, seat, 3), &replay, 1, AimGuide::Standard).unwrap();
        // Before the replay: the table as last seen, nobody to play here.
        assert_eq!(m.game.shots_taken(), 1);
        assert!(table(&m).unwrap().replaying());
        assert_eq!(m.controllers[seat], Controller::Remote);
        run(&mut m, 60.0);
        assert!(!table(&m).unwrap().replaying());
        assert_eq!(m.game, end, "the replay ends on the server's table");
        assert_eq!(m.controllers[seat], Controller::Human);
        assert!(m.is_human_turn());
        assert_eq!(m.stats[0].shots + m.stats[1].shots, 3, "every shot counts");
    }

    #[test]
    fn skipping_shows_the_end_at_once() {
        let replay = recorded(4);
        let end = replay.play(None).unwrap().0;
        let seat = end.current_player();
        let mut m = open(info(seat, seat, 4), &replay, 0, AimGuide::Standard).unwrap();
        run(&mut m, 1.0);
        skip_replay(&mut m);
        assert_eq!(m.game, end);
        assert!(!table(&m).unwrap().replaying());
        assert_eq!(m.stats[0].shots + m.stats[1].shots, 4);
        assert_eq!(m.timeline.len(), 4);
    }

    #[test]
    fn shots_go_out_for_this_game_and_the_rest_stays_here() {
        let replay = recorded(0);
        let mut m = open(info(0, 0, 0), &replay, 0, AimGuide::Standard).unwrap();
        let c = m.online.as_mut().unwrap().corr.as_mut().unwrap();
        let shot = snook_legend_core::Shot::new(0.1, 2.0);
        let out =
            outgoing(c, ClientMsg::Shoot { turn: 0, shot, called_pocket: None, cue_ball: None });
        assert!(matches!(out, Some(ClientMsg::CorrShoot { id: 5, turn: 0, .. })));
        assert_eq!((c.known, c.played), (1, true));
        assert!(matches!(outgoing(c, ClientMsg::Resign), Some(ClientMsg::CorrResign { id: 5 })));
        assert!(outgoing(c, ClientMsg::MatchChat { text: "hi".into() }).is_none());
        assert!(outgoing(c, ClientMsg::LeaveMatch).is_none());
    }

    #[test]
    fn a_turn_that_ran_out_ends_the_game_in_its_own_words() {
        let mut replay = recorded(2);
        let seat = replay.play(None).unwrap().0.current_player();
        replay.strokes.push(Stroke::Abandon { seat });
        let mut g = info(0, seat, 3);
        g.result = Some(CorrResult {
            winner: 1 - seat,
            reason: GameOverReason::Abandoned,
            timeout: true,
            delta: [-10, 10],
            ratings: [1190, 1210],
            at: 0,
            match_id: 3,
        });
        let mut m = open(g, &replay, 2, AimGuide::Standard).unwrap();
        run(&mut m, 5.0);
        let over = m.game.game_over().expect("over");
        assert_eq!(over.winner, 1 - seat);
        assert!(matches!(m.phase, Phase::GameOver));
        assert!(result_line(&m, over).is_some());
        assert!(m.online.as_ref().unwrap().result.is_some());
    }

    #[test]
    fn the_list_puts_the_players_turns_first() {
        let mut c = Corr::default();
        let mut theirs = info(0, 1, 4);
        theirs.id = 1;
        let mut mine = info(0, 0, 4);
        mine.id = 2;
        let mut done = info(0, 0, 9);
        done.id = 3;
        done.result = Some(CorrResult {
            winner: 0,
            reason: GameOverReason::PocketedEight,
            timeout: false,
            delta: [8, -8],
            ratings: [1208, 1192],
            at: 10,
            match_id: 1,
        });
        c.games = vec![theirs, done, mine];
        c.challenges = vec![CorrChallenge {
            id: 9,
            from: player("caio"),
            to: player("ana"),
            rated: false,
            at: 0,
            expires_at: 100,
        }];
        let order: Vec<String> = rows(&c, "ana")
            .iter()
            .map(|r| match r {
                Row::Asked(x) | Row::Sent(x) => format!("c{}", x.id),
                Row::Game(g) => format!("g{}", g.id),
            })
            .collect();
        assert_eq!(order, ["c9", "g2", "g1", "g3"]);
        assert_eq!(c.waiting("ana"), 2, "one turn and one challenge");
    }

    #[test]
    fn time_left_is_short() {
        crate::i18n::set_lang(crate::i18n::Lang::English);
        assert_eq!(short_left(2 * 86_400 + 5 * 3600 + 7), "2 d 5 h");
        assert_eq!(short_left(5 * 3600 + 100), "5 h");
        assert_eq!(short_left(30), "1 min");
        assert_eq!(short_left(-5), "1 min");
    }
}
