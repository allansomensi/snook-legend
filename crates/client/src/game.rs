//! Match flow: turns, shot playback, the computer opponent, online play
//! (with the shooter's shot predicted locally) and the replay of the last shot.

use std::sync::{Arc, Mutex};

use ::glam::DVec2;
use snook_legend_core::ai::{self, AiDecision, AiPlayer};
use snook_legend_core::{
    BallId, BallInHand, BreakChoice, BreakOption, Difficulty, EightBallGame, Event, Foul,
    GameOverReason, PendingShot, PocketId, Rules, Shot, ShotError, ShotReport, Simulation,
};

use snook_legend_proto::{
    AimState, ChatKind, ChatLine, ClientMsg, MatchResult, PlayerInfo, SPECTATOR_SEAT, ServerMsg,
};

use crate::config::AimGuide;
use crate::i18n::{t, tw};
use crate::ui::{FeedKind, FeedLine};
use crate::{MatchMode, MatchSettings};

/// Who controls a seat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Controller {
    Human,
    Computer(AiPlayer),
    /// The opponent in an online match.
    Remote,
}

/// The player's cue settings.
#[derive(Debug, Clone, Copy)]
pub struct Aim {
    pub yaw: f64,
    pub elevation: f64,
    pub tip: DVec2,
    /// 0..=1, mapped to cue speed.
    pub power: f32,
    pub charging: bool,
}

impl Default for Aim {
    fn default() -> Self {
        Aim { yaw: 0.0, elevation: 0.0, tip: DVec2::ZERO, power: 0.0, charging: false }
    }
}

/// A computer move being computed on a worker thread, then acted out.
pub struct ComputerTurn {
    result: Arc<Mutex<Option<AiDecision>>>,
    decision: Option<AiDecision>,
    elapsed: f32,
    start_yaw: f64,
    /// How long the computer "studies the table" before moving the cue.
    think: f32,
    waited: f32,
    /// A small first aiming error, corrected before the stroke (like a person).
    first_miss: f64,
}

pub enum Phase {
    /// The current player is lining up a shot.
    Turn {
        placing_cue_ball: bool,
        computer: Option<ComputerTurn>,
    },
    /// Balls are rolling.
    Simulating(Box<PendingShot>),
    /// Online: the animation is over and the server's verdict is on its way
    /// (the balls stay where the animation left them).
    Settling(Box<PendingShot>),
    /// Short pause after a shot so the result can be read.
    Pause(f32),
    GameOver,
}

/// The last shot, kept so it can be watched again.
#[derive(Clone)]
pub struct ReplayShot {
    pub start: EightBallGame,
    pub shot: Shot,
    pub called: Option<PocketId>,
}

/// A replay being shown (the real game is untouched).
pub struct Replay {
    pub simulation: Simulation,
    /// Time since the replay ended (it closes itself shortly after).
    pub done: f32,
    events_seen: usize,
}

pub struct Notice {
    pub text: String,
    /// What it means under the rules of the game (a foul's consequence…),
    /// shown under the text.
    pub detail: Option<String>,
    pub important: bool,
    pub ttl: f32,
    /// Good news (balls in a row): floats on the table in gold, not in the
    /// colour of fouls.
    pub cheer: bool,
}

pub use snook_legend_core::stats::ShotStats;

/// A stored game, worked out shot by shot once, so it can be watched like
/// a video: the table before every stroke is known, and seeking is instant.
pub struct Video {
    pub strokes: Vec<snook_legend_proto::replay::Stroke>,
    /// The table before stroke `i` (and after the last one).
    pub states: Vec<EightBallGame>,
    /// Each player's numbers before stroke `i`.
    pub stats: Vec<[ShotStats; 2]>,
    /// The shot of each stroke (none for a time foul or a resignation).
    pub marks: Vec<Option<ShotMark>>,
    pub paused: bool,
    /// 0.5, 1, 2 or 4.
    pub speed: f32,
}

impl Video {
    pub fn of(replay: &snook_legend_proto::replay::Replay) -> Option<Video> {
        use snook_legend_proto::replay::{Stroke, apply};
        let mut game = replay.start();
        let mut stats = [ShotStats::default(); 2];
        let mut v = Video {
            strokes: replay.strokes.clone(),
            states: vec![game.clone()],
            stats: vec![stats],
            marks: Vec::new(),
            paused: false,
            speed: 1.0,
        };
        for stroke in &replay.strokes {
            let shooter = game.current_player();
            let in_hand = game.ball_in_hand().is_some();
            let cue = match stroke {
                Stroke::Shot { cue_ball: Some(p), .. } if in_hand => *p,
                _ => game.ball(BallId::CUE).xy(),
            };
            let report = apply(&mut game, stroke).ok()?;
            let mark = match (report, stroke) {
                (Some(r), Stroke::Shot { shot, .. }) => {
                    stats[shooter % 2].add(&r, in_hand);
                    stats[shooter % 2].add_spin(shot, &r);
                    // The balls made for the player (not the opponent's, nor an 8 that
                    // loses), as the numbers count them.
                    let made = r.balls_made().len() as u32;
                    Some(ShotMark {
                        shooter: shooter % 2,
                        cue: (cue.x, cue.y),
                        made,
                        foul: r.is_foul(),
                    })
                }
                _ => None,
            };
            v.marks.push(mark);
            v.states.push(game.clone());
            v.stats.push(stats);
        }
        Some(v)
    }

    /// Development aid and tests: a game the computer plays against itself,
    /// recorded (`shots` strokes at most).
    pub fn sample(seed: u64, shots: usize) -> snook_legend_proto::ReplayInfo {
        use snook_legend_proto::replay::{Replay, Stroke};
        let mut replay = Replay::new(seed, 0, snook_legend_core::Rules::Brazilian);
        let mut game = replay.start();
        while replay.strokes.len() < shots && game.game_over().is_none() {
            let d = ai::decide(&game, Difficulty::Easy, seed ^ replay.strokes.len() as u64);
            let stroke = Stroke::Shot {
                shot: d.shot,
                called: d.called_pocket,
                cue_ball: d.cue_ball,
                timeout: false,
            };
            if snook_legend_proto::replay::apply(&mut game, &stroke).is_err() {
                break;
            }
            replay.strokes.push(stroke);
        }
        snook_legend_proto::ReplayInfo {
            match_id: 0,
            players: ["Ana".into(), "Bia".into()],
            countries: ["BR".into(), "AR".into()],
            ratings: [1200, 1200],
            winner: game.game_over().map_or(0, |o| o.winner),
            rated: false,
            at: 0,
            replay,
        }
    }

    /// The next speed (0.5 → 1 → 2 → 4 → 0.5).
    pub fn next_speed(&mut self) {
        self.speed = match self.speed {
            s if s < 0.75 => 1.0,
            s if s < 1.5 => 2.0,
            s if s < 3.0 => 4.0,
            _ => 0.5,
        };
    }
}

/// One shot of the game, for the end-of-game charts.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShotMark {
    pub shooter: usize,
    /// Where the cue ball was when it was struck (table coordinates).
    pub cue: (f64, f64),
    /// Object balls legally made.
    pub made: u32,
    pub foul: bool,
}

/// What a shot clock running out did, as the rules of the game say: the
/// turn passes (with the cue ball in hand only where the rules give it).
fn time_foul_text(game: &EightBallGame, who: &str) -> String {
    if game.ball_in_hand().is_some() {
        tw("{who} ran out of time: ball in hand for the opponent.", &[("who", &who)])
    } else {
        tw("{who} ran out of time: the turn passes.", &[("who", &who)])
    }
}

pub struct Match {
    /// Each player's cue and chalk (shop codes).
    pub cue_looks: [(String, String); 2],
    /// Each player's cue level (marks on the cue).
    pub cue_levels: [u8; 2],
    /// Each player's winning effect (shop codes).
    pub effects: [String; 2],
    /// A game just won on the 8: (winner, the pocket), for the effect.
    pub win_effect: Option<(usize, snook_legend_core::PocketId)>,
    /// Each player's numbers this game.
    pub stats: [ShotStats; 2],
    /// Every shot of this game, in order (the momentum and the shot map).
    pub timeline: Vec<ShotMark>,
    /// The shot being shown was taken with the cue ball in hand.
    shot_in_hand: bool,
    pub game: EightBallGame,
    pub names: [String; 2],
    pub controllers: [Controller; 2],
    pub phase: Phase,
    pub aim: Aim,
    /// The pocket called for this shot: the 8's (the Brazilian rules), or
    /// the shot's (the official rules: none is a safety).
    pub called_pocket: Option<PocketId>,
    /// The official rules: the player chose the call (a pocket, or a
    /// safety); otherwise it follows the aim (see [`Match::follow_call`]).
    pub call_locked: bool,
    /// How long the computer has been weighing the choice a break left it.
    choice_wait: f32,
    pub notices: Vec<Notice>,
    /// Everything that happened this game, newest last (shown in the HUD).
    pub log: Vec<(String, bool)>,
    pub last_report: Option<ShotReport>,
    pub guide: AimGuide,
    pub games_played: u32,
    pub wins: [u32; 2],
    /// Collisions to play as sounds (drained by the audio module).
    pub sounds: Vec<ImpactSound>,
    /// Set for matches played through a server.
    pub online: Option<OnlineMatch>,
    /// Offline shot clock: (seconds per shot, seconds left).
    pub clock: Option<(f32, f32)>,
    pub last_shot: Option<ReplayShot>,
    pub replay: Option<Replay>,
    /// Esc was already used this frame (to cancel a stroke being charged).
    pub escape_used: bool,
    seed: u64,
    /// A stored game being watched again: the strokes still to play.
    pub script: Option<std::collections::VecDeque<snook_legend_proto::replay::Stroke>>,
    /// A stored game watched like a video: pause, speed and seeking.
    pub video: Option<Video>,
    /// A game against the computer to pass the time while "Play now"
    /// looks for an opponent (not saved; the online game takes its place).
    pub practice: bool,
    /// The exact shot the computer (or a replay) plays, not rebuilt from
    /// the power bar (which would round it).
    forced_shot: Option<Shot>,
    events_seen: usize,
    demo_timer: f32,
    /// Who the computer is, in a game against it.
    pub opponent: Option<crate::opponents::Opponent>,
    /// A game of the ladder: its rung and every stroke (the final game is
    /// sent to the server to claim the reward).
    pub ladder: Option<crate::ladder::LadderRun>,
    /// An attempt at the daily challenge.
    pub daily: Option<DailyRun>,
    /// A table to oneself (practice): racked again when a rack is over.
    pub free_play: bool,
    /// A drill session.
    pub drill: Option<DrillRun>,
    /// What the computer opponent just said, and for how long it has shown.
    pub banter: Option<(String, f32)>,
    /// Seconds since the opponent last spoke.
    banter_quiet: f32,
    /// The computer opponent speaks up now and then (a setting).
    pub talk: bool,
    /// The opponent's opening line has had its chance (once the setting
    /// for their remarks is known).
    greeted: bool,
    /// The player's game of a tournament against the computer: its result
    /// stands (no rematch).
    pub cup: bool,
    /// Each player's picture (an avatar code; empty: none chosen).
    pub avatars: [String; 2],
    /// The head string drawn at all times (a setting; off: only while the
    /// cue ball is placed behind it).
    pub head_string: bool,
    /// The balls made in a row by the player at the table this visit (the
    /// count by their picture, and a cheer at three, five, seven).
    pub visit_run: (usize, u32),
    /// Point out where to aim, set the power and shoot (a first game).
    pub coach: bool,
    /// The second of the shot clock last ticked (its last five tick).
    last_tick: Option<i32>,
    /// It was this player's turn at the last frame (online, a chime says
    /// when it comes back).
    my_turn_seen: bool,
}

/// A drill session: its tries, what each gave, and every stroke (for the
/// server, which counts them again).
#[derive(Debug, Clone, PartialEq)]
pub struct DrillRun {
    pub code: &'static str,
    /// The session the server started (0: practice, signed out): where
    /// each try's cue ball lies.
    pub session: u64,
    pub results: Vec<bool>,
    pub strokes: Vec<Shot>,
    /// The table is set up for the next try once the pause is over.
    reset: bool,
    pub over: bool,
    pub sent: bool,
    /// The server's count.
    pub result: Option<Result<u32, snook_legend_proto::drills::Refused>>,
}

impl DrillRun {
    pub fn drill(&self) -> &'static snook_legend_proto::drills::Drill {
        snook_legend_proto::drills::drill(self.code).expect("a known drill")
    }

    /// The score as the game sees it (a streak: the pots in a row).
    pub fn score(&self) -> u32 {
        self.results.iter().filter(|&&ok| ok).count() as u32
    }
}

/// How the power bar maps to the cue's speed, up to the limit of the stroke
/// (see `EightBallGame::max_cue_speed`: a power shot, or the break's own):
/// a curve that keeps most of the bar for the paces most strokes are played
/// at (half the bar is a medium stroke, about 1.7 m/s for the ball). A
/// little less bent than a square: on the slower cloth of ruleset 4 the low
/// end of the bar barely moved the ball.
pub const POWER_CURVE: f64 = 1.7;

/// The cue speed for a point of the power bar (0 to 1).
pub fn power_to_speed(power: f32, max: f64) -> f64 {
    (power.clamp(0.0, 1.0) as f64).powf(POWER_CURVE) * max
}

/// The point of the power bar of a cue speed.
pub fn speed_to_power(speed: f64, max: f64) -> f32 {
    (speed / max).clamp(0.0, 1.0).powf(1.0 / POWER_CURVE) as f32
}

/// How long the opponent's speech bubble shows (seconds).
pub const BANTER_TIME: f32 = 4.5;

/// An attempt at the daily challenge: one visit to the table, every stroke
/// kept for the server (which plays them again to count the balls).
#[derive(Debug, Clone, PartialEq)]
pub struct DailyRun {
    pub day: i64,
    /// Object balls on the table at the start.
    pub balls: u32,
    pub strokes: Vec<Shot>,
    /// Balls made so far (as the game sees them; the server's count rules).
    pub made: u32,
    /// The visit is over: the strokes are to be sent.
    pub over: bool,
    /// Sent to the server.
    pub sent: bool,
    /// The server's answer (and the coins it gave).
    pub result: Option<(
        Result<snook_legend_proto::daily::Outcome, snook_legend_proto::daily::Refused>,
        i64,
    )>,
}

/// State of a match played on a server.
pub struct OnlineMatch {
    pub seat: usize,
    /// Whether the game is rated (kept with the match state).
    #[allow(dead_code)]
    pub rated: bool,
    pub ratings: [i32; 2],
    /// The players who are administrators (their names are red).
    pub admins: [bool; 2],
    pub countries: [String; 2],
    pub tournament: Option<u64>,
    pub shot_clock: f32,
    /// Seconds left for the current shot (counts down during a turn).
    pub clock_left: f32,
    /// Messages for the server (drained by the network code).
    pub outbox: Vec<ClientMsg>,
    pub result: Option<MatchResult>,
    pub opponent_connected: bool,
    pub opponent_left: bool,
    pub rematch_offered: bool,
    pub rematch_requested: bool,
    /// Chat with the opponent and what happened in the game, in order.
    pub feed: Vec<FeedLine>,
    /// Chat lines the player has not looked at.
    pub unread: usize,
    /// The newest unread line and how long ago it came (a preview by the
    /// closed chat for a few seconds).
    pub preview: Option<(FeedLine, f32)>,
    /// Achievements of this player (they unlock reactions).
    pub unlocked: Vec<String>,
    /// This player is VIP (the VIP reactions).
    pub vip: bool,
    /// Reactions on screen: who sent which (a seat, or [`SPECTATOR_SEAT`]),
    /// and for how long it has shown.
    pub reactions: Vec<(usize, &'static str, f32)>,
    /// Watching a friend's game (`seat` is the friend's).
    pub spectator: bool,
    /// How many people are watching.
    pub spectators: u32,
    /// How many spectators cheer for each seat.
    pub support: [u32; 2],
    /// The seat this spectator cheers for.
    pub cheering: Option<usize>,
    /// The opponent has sent their aim this turn (until then their cue is hidden).
    pub remote_aimed: bool,
    /// The opponent's latest aim; the cue glides towards it.
    remote_target: Option<AimState>,
    /// Watching: how hard the shot being lined up is.
    pub assessor: crate::assess::Assessor,
    /// The turn being animated (or awaited).
    sim_turn: Option<u32>,
    /// This player's own shot, played before the server confirmed it.
    predicted: Option<(u32, Shot)>,
    /// The table before the predicted shot (to roll back if refused).
    turn_start: Option<EightBallGame>,
    /// The server's result for the shot being animated.
    pending_end: Option<(EightBallGame, ShotReport)>,
    /// The answer to the break's choice, sent and not confirmed yet.
    pub choice_sent: Option<BreakOption>,
    /// The match ended while a shot was still being shown.
    ended_pending: bool,
    aim_timer: f32,
    last_aim: Option<AimState>,
    last_cue: Option<DVec2>,
    /// Shot clock for the next turn, from the server.
    next_clock: f32,
    /// A correspondence game (a turn at a time; see [`crate::correspondence`]).
    pub corr: Option<crate::correspondence::CorrTable>,
}

/// A sound-worthy collision during playback.
#[derive(Debug, Clone, Copy)]
pub struct ImpactSound {
    pub kind: ImpactKind,
    /// Impact speed in m/s.
    pub speed: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImpactKind {
    Cue,
    Ball,
    Cushion,
    Pocket,
    /// The shot clock's last seconds.
    Tick,
    /// The turn comes to the player (online, or the computer is done).
    Chime,
}

impl Match {
    pub fn new(settings: &MatchSettings, seed: u64) -> Self {
        let level_name = |d| tw("Computer ({level})", &[("level", &crate::i18n::difficulty(d))]);
        let (names, controllers) = match &settings.mode {
            MatchMode::VsComputer(o) => {
                // On the tour an opponent plays at their best (and the
                // server checks the final game against that player).
                let player = match settings.ladder {
                    Some(rung) => snook_legend_proto::ladder::tour_form(&o.player(), rung as usize),
                    None => o.player(),
                };
                (
                    [settings.names[0].clone(), o.display_name()],
                    [Controller::Human, Controller::Computer(player)],
                )
            }
            MatchMode::HotSeat => (settings.names.clone(), [Controller::Human, Controller::Human]),
            MatchMode::Demo => (
                [level_name(Difficulty::Hard), level_name(Difficulty::Medium)],
                [
                    Controller::Computer(Difficulty::Hard.player()),
                    Controller::Computer(Difficulty::Medium.player()),
                ],
            ),
        };
        let opponent = match &settings.mode {
            MatchMode::VsComputer(o) => Some(o.clone()),
            _ => None,
        };
        // A ladder game records every stroke from the break.
        let ladder =
            settings.ladder.filter(|_| opponent.is_some()).map(|rung| crate::ladder::LadderRun {
                rung,
                record: snook_legend_proto::replay::Replay::new(seed, 0, settings.rules),
                reported: false,
                dealt: false,
                account: String::new(),
            });
        let mut m = Match {
            game: EightBallGame::new(seed, 0, settings.rules),
            names,
            controllers,
            phase: Phase::GameOver,
            aim: Aim::default(),
            called_pocket: None,
            call_locked: false,
            choice_wait: 0.0,
            notices: Vec::new(),
            log: Vec::new(),
            last_report: None,
            stats: [ShotStats::default(); 2],
            timeline: Vec::new(),
            cue_looks: Default::default(),
            cue_levels: [0; 2],
            effects: Default::default(),
            win_effect: None,
            shot_in_hand: false,
            guide: settings.guide,
            games_played: 0,
            wins: [0, 0],
            sounds: Vec::new(),
            online: None,
            clock: settings.shot_clock.map(|s| (s as f32, s as f32)),
            last_shot: None,
            replay: None,
            escape_used: false,
            seed,
            script: None,
            video: None,
            practice: false,
            forced_shot: None,
            events_seen: 0,
            demo_timer: 0.0,
            opponent,
            ladder,
            daily: None,
            free_play: false,
            drill: None,
            banter: None,
            banter_quiet: 0.0,
            // Off until the game's loop sets it from the settings: the
            // opening line used to be said before the setting was read,
            // also with the remarks turned off.
            talk: false,
            greeted: false,
            cup: false,
            avatars: Default::default(),
            head_string: false,
            visit_run: (0, 0),
            last_tick: None,
            my_turn_seen: false,
            coach: false,
        };
        m.begin_turn();
        m
    }

    /// Free practice only: the last shot is taken back (the balls go back
    /// where they were before it). Whether there was one to take back.
    pub fn take_back(&mut self) -> bool {
        if !self.free_play || !matches!(self.phase, Phase::Turn { .. } | Phase::Pause(_)) {
            return false;
        }
        let Some(last) = self.last_shot.take() else { return false };
        self.game = last.start;
        self.replay = None;
        self.begin_turn();
        true
    }

    /// A session of the drill `code` (`session`: the server's, or 0 for
    /// practice).
    pub fn drill(code: &str, name: &str, guide: AimGuide, session: u64) -> Option<Match> {
        let d = snook_legend_proto::drills::drill(code)?;
        let settings = MatchSettings {
            mode: MatchMode::HotSeat,
            names: [name.to_string(), t(d.name).to_string()],
            guide,
            shot_clock: None,
            ladder: None,
            rules: snook_legend_core::Rules::Brazilian,
        };
        let mut m = Match::new(&settings, 1);
        m.game = snook_legend_proto::drills::game_for_session_try(d, session, 0);
        m.drill = Some(DrillRun {
            code: d.code,
            session,
            results: Vec::new(),
            strokes: Vec::new(),
            reset: false,
            over: false,
            sent: false,
            result: None,
        });
        m.begin_turn();
        m.notify(t(d.description), true);
        Some(m)
    }

    /// The computer opponent may say something about `moment` (a
    /// personality's line, in a bubble by its card).
    pub fn say(&mut self, moment: crate::banter::Moment) {
        // Only a game against the computer, not the waiting room's practice,
        // and only when the player wants the remarks.
        if !self.talk || !(self.is_against_computer() || self.practice) || self.free_play {
            return;
        }
        let Some(o) = &self.opponent else { return };
        let code = match o {
            crate::opponents::Opponent::Personality { code } => code.as_str(),
            crate::opponents::Opponent::Ghost { .. } => return,
            _ => "",
        };
        let turn = self.game.shots_taken() as u64 + 1000 * self.games_played as u64;
        if !moment.always()
            && (self.banter_quiet < 6.0
                || crate::banter::roll(self.seed ^ turn, moment as u64) > moment.chance())
        {
            return;
        }
        let lines = crate::banter::lines(code, moment);
        if lines.is_empty() {
            return;
        }
        let k = (crate::banter::roll(turn, self.seed) * lines.len() as f32) as usize;
        self.banter = Some((t(lines[k.min(lines.len() - 1)]).to_string(), 0.0));
        self.banter_quiet = 0.0;
    }

    /// An attempt at the daily challenge from the position of `seed`.
    pub fn daily(day: i64, seed: u64, name: &str, guide: AimGuide) -> Match {
        let settings = MatchSettings {
            mode: MatchMode::HotSeat,
            names: [name.to_string(), t("Daily challenge").to_string()],
            guide,
            shot_clock: None,
            ladder: None,
            rules: snook_legend_core::Rules::Brazilian,
        };
        let mut m = Match::new(&settings, seed);
        m.game = snook_legend_proto::daily::game(seed);
        m.daily = Some(DailyRun {
            day,
            balls: snook_legend_proto::daily::balls(seed),
            strokes: Vec::new(),
            made: 0,
            over: false,
            sent: false,
            result: None,
        });
        m.begin_turn();
        m.notify(
            t("Make as many balls as you can in one visit: a miss or a foul ends the attempt."),
            true,
        );
        m
    }

    /// Watches a stored game again, stroke by stroke.
    pub fn replay_of(info: &snook_legend_proto::ReplayInfo, guide: AimGuide) -> Match {
        let settings = MatchSettings {
            mode: MatchMode::Demo,
            names: info.players.clone(),
            guide,
            shot_clock: None,
            ladder: None,
            rules: info.replay.rules,
        };
        let mut m = Match::new(&settings, info.replay.seed);
        m.game = info.replay.start();
        m.names = info.players.clone();
        m.script = Some(info.replay.strokes.iter().copied().collect());
        m.video = Video::of(&info.replay);
        m.begin_turn();
        m
    }

    /// Strokes of the stored game already played (or being played).
    pub fn video_position(&self) -> usize {
        let total = self.video.as_ref().map_or(0, |v| v.strokes.len());
        total - self.script.as_ref().map_or(0, |s| s.len())
    }

    /// Jumps to the table as it was before stroke `k` of the stored game.
    pub fn seek(&mut self, k: usize) {
        let Some(v) = &self.video else { return };
        let k = k.min(v.strokes.len());
        self.game = v.states[k].clone();
        self.stats = v.stats[k];
        self.timeline = v.marks[..k].iter().flatten().copied().collect();
        self.script = Some(v.strokes[k..].iter().copied().collect());
        self.phase = Phase::GameOver;
        self.replay = None;
        self.last_shot = None;
        self.last_report = None;
        self.win_effect = None;
        self.notices.clear();
        self.begin_turn();
    }

    /// This is the player's game of a tournament against the computer:
    /// its result stands, and the opponent plays at their best (winning a
    /// tournament is meant to be hard; the tour's final is harder still).
    pub fn make_cup_game(&mut self) {
        self.cup = true;
        if let Some(o) = &self.opponent {
            self.controllers[1] =
                Controller::Computer(snook_legend_proto::ladder::cup_form(&o.player()));
        }
    }

    /// A game against the computer saved by [`crate::saved`], picked up again.
    pub fn resume(saved: &crate::saved::SavedGame, guide: AimGuide) -> Match {
        let settings = MatchSettings {
            mode: MatchMode::VsComputer(saved.opponent()),
            names: saved.names.clone(),
            guide,
            shot_clock: saved.shot_clock,
            ladder: None,
            rules: saved.game.rules(),
        };
        let mut m = Match::new(&settings, saved.seed);
        m.ladder = saved.ladder.clone();
        if let Some(run) = &saved.ladder {
            m.controllers[1] = Controller::Computer(snook_legend_proto::ladder::tour_form(
                &saved.opponent().player(),
                run.rung as usize,
            ));
        }
        m.game = saved.game.clone();
        m.names = saved.names.clone();
        // In the language of today (a ghost is "Ghost of …").
        if let Some(o) = &saved.opponent {
            m.names[1] = o.display_name();
        }
        m.stats = saved.stats;
        m.timeline = saved.timeline.clone();
        m.wins = saved.wins;
        m.games_played = saved.games_played;
        m.begin_turn();
        m
    }

    /// A game of a person against the computer (not online, not a replay).
    pub fn is_against_computer(&self) -> bool {
        matches!(self.controllers, [Controller::Human, Controller::Computer(_)])
            && !self.is_online()
            && !self.is_replay()
            && !self.practice
    }

    /// The seed of this match (the computer's choices follow from it).
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Between shots: no ball moving, no shot or replay on screen.
    pub fn at_rest(&self) -> bool {
        matches!(self.phase, Phase::Turn { .. }) && self.replay.is_none()
    }

    /// Watching a stored game.
    pub fn is_replay(&self) -> bool {
        self.script.is_some()
    }

    /// A game being played at this computer: not an online match, not a
    /// stored game being watched (the computer, two players, a drill…).
    pub fn is_played_offline(&self) -> bool {
        !self.is_online() && !self.is_replay()
    }

    /// Focus mode applies to this game: one being played, here or online
    /// (not a stored game, not someone else's game being watched).
    pub fn is_focus_game(&self) -> bool {
        !self.is_replay() && !self.is_spectating()
    }

    /// A match on the server, from its `MatchStarted` message.
    #[allow(clippy::too_many_arguments)]
    pub fn online(
        match_id: u64,
        seat: usize,
        players: &[PlayerInfo; 2],
        rated: bool,
        game: EightBallGame,
        shot_clock_secs: u32,
        clock_left: f32,
        tournament: Option<u64>,
        guide: AimGuide,
        spectating: bool,
    ) -> Self {
        let rules = game.rules();
        let mut controllers = [Controller::Remote; 2];
        // Development aid: SNOOK_LEGEND_AUTOPLAY=1 lets the computer play this seat
        // (two such games can play a whole online match by themselves).
        // A spectator only watches: both seats are played from the server.
        if !spectating {
            controllers[seat] = if crate::dev::flag("SNOOK_LEGEND_AUTOPLAY") {
                Controller::Computer(Difficulty::Hard.player())
            } else {
                Controller::Human
            };
        }
        let mut m = Match {
            cue_looks: [
                (players[0].cue.clone(), players[0].chalk.clone()),
                (players[1].cue.clone(), players[1].chalk.clone()),
            ],
            cue_levels: [players[0].cue_level, players[1].cue_level],
            effects: [players[0].effect.clone(), players[1].effect.clone()],
            win_effect: None,
            game,
            names: [players[0].username.clone(), players[1].username.clone()],
            controllers,
            phase: Phase::GameOver,
            aim: Aim::default(),
            called_pocket: None,
            call_locked: false,
            choice_wait: 0.0,
            notices: Vec::new(),
            log: Vec::new(),
            last_report: None,
            stats: [ShotStats::default(); 2],
            timeline: Vec::new(),
            shot_in_hand: false,
            // No full guide against people.
            guide: guide.online(),
            games_played: 0,
            wins: [0, 0],
            sounds: Vec::new(),
            online: Some(OnlineMatch {
                seat,
                rated,
                ratings: players.each_ref().map(|p| p.rating_in(rules)),
                admins: players.each_ref().map(|p| p.role == snook_legend_proto::Role::Admin),
                countries: [players[0].country.clone(), players[1].country.clone()],
                tournament,
                shot_clock: shot_clock_secs as f32,
                clock_left,
                outbox: Vec::new(),
                result: None,
                opponent_connected: true,
                opponent_left: false,
                rematch_offered: false,
                rematch_requested: false,
                feed: Vec::new(),
                unread: 0,
                preview: None,
                unlocked: Vec::new(),
                reactions: Vec::new(),
                spectator: spectating,
                spectators: 0,
                vip: false,
                support: [0, 0],
                cheering: None,
                remote_aimed: false,
                remote_target: None,
                assessor: Default::default(),
                sim_turn: None,
                predicted: None,
                turn_start: None,
                pending_end: None,
                choice_sent: None,
                ended_pending: false,
                aim_timer: 0.0,
                last_aim: None,
                last_cue: None,
                next_clock: clock_left,
                corr: None,
            }),
            clock: None,
            last_shot: None,
            replay: None,
            escape_used: false,
            seed: match_id,
            script: None,
            video: None,
            practice: false,
            forced_shot: None,
            events_seen: 0,
            demo_timer: 0.0,
            opponent: None,
            ladder: None,
            daily: None,
            free_play: false,
            drill: None,
            banter: None,
            banter_quiet: 0.0,
            talk: true,
            greeted: true,
            cup: false,
            avatars: [players[0].avatar.clone(), players[1].avatar.clone()],
            head_string: false,
            visit_run: (0, 0),
            last_tick: None,
            my_turn_seen: false,
            coach: false,
        };
        m.begin_turn();
        if let Some(o) = m.online.as_mut() {
            o.clock_left = clock_left;
        }
        m
    }

    /// Starts the next online game with the same opponent (a rematch, or the
    /// same game after reconnecting), keeping the score and the chat.
    pub fn continue_online(&mut self, next: Match, same_game: bool) {
        let wins = self.wins;
        let games = self.games_played;
        let feed = self.online.as_ref().map(|o| o.feed.clone()).unwrap_or_default();
        let log = std::mem::take(&mut self.log);
        let guide = self.guide;
        let swapped = next.names[0] != self.names[0];
        *self = next;
        self.wins = if swapped { [wins[1], wins[0]] } else { wins };
        self.games_played = games;
        if same_game {
            self.log = log;
        }
        self.guide = guide;
        if let Some(o) = self.online.as_mut() {
            o.feed = feed;
        }
    }

    /// The aiming guide drawn: never the full guide in a game against a
    /// person over the network (rated or not, watching included), whatever
    /// `guide` says; the full one is for free games against the computer
    /// and practice.
    pub fn guide_in_use(&self) -> AimGuide {
        // The daily challenge, the drills and the tour are ranked or
        // rewarded: the same rule.
        if self.online.is_some()
            || self.daily.is_some()
            || self.drill.is_some()
            || self.ladder.is_some()
        {
            self.guide.online()
        } else {
            self.guide
        }
    }

    pub fn is_online(&self) -> bool {
        self.online.is_some()
    }

    /// The server's id of an online match.
    pub fn match_id(&self) -> u64 {
        self.seed
    }

    /// Starts a new game; the players alternate the break. Online, this asks
    /// the opponent for a rematch.
    pub fn rematch(&mut self) {
        if self.cup {
            // A tournament's game is played once.
            return;
        }
        if let Some(o) = self.online.as_mut() {
            if !o.rematch_requested && !o.opponent_left && o.tournament.is_none() {
                o.rematch_requested = true;
                o.outbox.push(ClientMsg::Rematch);
            }
            return;
        }
        let seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let breaker = (self.games_played % 2) as usize;
        self.deal(seed, breaker);
    }

    /// Starts a new game from the rack `seed`, `breaker` to break. The
    /// tour's final, played for the reward, is dealt by the server.
    pub fn deal(&mut self, seed: u64, breaker: usize) {
        self.seed = seed;
        let rules = self.game.rules();
        self.game = EightBallGame::new(seed, breaker, rules);
        if let Some(run) = self.ladder.as_mut() {
            run.record = snook_legend_proto::replay::Replay::new(seed, breaker, rules);
            run.reported = false;
            run.dealt = false;
            run.account.clear();
        }
        self.stats = [ShotStats::default(); 2];
        self.visit_run = (0, 0);
        self.timeline.clear();
        self.last_report = None;
        self.last_shot = None;
        self.replay = None;
        self.notices.clear();
        self.log.clear();
        self.begin_turn();
    }

    pub fn current_controller(&self) -> Controller {
        self.controllers[self.game.current_player()]
    }

    pub fn is_human_turn(&self) -> bool {
        matches!(self.phase, Phase::Turn { .. })
            && self.current_controller() == Controller::Human
            && self.replay.is_none()
    }

    pub fn is_placing_cue_ball(&self) -> bool {
        matches!(self.phase, Phase::Turn { placing_cue_ball: true, .. })
    }

    pub fn set_placing(&mut self, placing: bool) {
        if let Phase::Turn { placing_cue_ball, .. } = &mut self.phase {
            *placing_cue_ball = placing && self.game.ball_in_hand().is_some();
        }
    }

    /// Balls as currently displayed (a replay, mid-shot or at rest).
    /// Tells this game from another one (its rack's seed).
    pub fn identity(&self) -> u64 {
        self.seed
    }

    pub fn balls(&self) -> &[snook_legend_core::Ball] {
        if let Some(r) = &self.replay {
            return &r.simulation.balls;
        }
        match &self.phase {
            Phase::Simulating(p) | Phase::Settling(p) => &p.simulation.balls,
            _ => self.game.balls(),
        }
    }

    /// True while balls are moving or the result is not in yet.
    pub fn shot_in_progress(&self) -> bool {
        matches!(self.phase, Phase::Simulating(_) | Phase::Settling(_))
    }

    pub fn notify(&mut self, text: impl Into<String>, important: bool) {
        self.announce(text.into(), important, 4.0);
    }

    /// An important notice with what it means under the rules (shown under
    /// it on the table, and with it in the log).
    pub fn notify_detail(&mut self, text: impl Into<String>, detail: Option<String>) {
        let (text, ttl) = (text.into(), if detail.is_some() { 6.0 } else { 4.0 });
        announce_into(
            &mut self.notices,
            &mut self.log,
            self.online.as_mut(),
            text,
            detail,
            true,
            ttl,
        );
    }

    fn announce(&mut self, text: String, important: bool, ttl: f32) {
        let online = self.online.as_mut();
        announce_into(&mut self.notices, &mut self.log, online, text, None, important, ttl);
    }

    /// Good news, shown on the table in gold (and in the log as an event).
    fn good_news(&mut self, text: String, ttl: f32) {
        self.announce(text, false, ttl);
        if let Some(n) = self.notices.last_mut() {
            n.important = true;
            n.cheer = true;
        }
    }

    /// The shot clock for the current turn: (seconds left, seconds per shot).
    pub fn shot_clock(&self) -> Option<(f32, f32)> {
        if let Some(o) = &self.online {
            // A correspondence turn has days, not seconds (shown elsewhere).
            if o.corr.is_some() {
                return None;
            }
            return Some((o.clock_left, o.shot_clock));
        }
        self.clock.map(|(total, left)| (left, total))
    }

    /// Watches the last shot again (not while a shot is being played).
    pub fn start_replay(&mut self) {
        if self.shot_in_progress() {
            return;
        }
        let Some(last) = &self.last_shot else { return };
        if let Ok(pending) = last.start.prepare_shot(&last.shot, last.called) {
            self.sounds.push(ImpactSound { kind: ImpactKind::Cue, speed: last.shot.speed as f32 });
            self.replay =
                Some(Replay { simulation: pending.simulation, done: 0.0, events_seen: 0 });
        }
    }

    pub fn stop_replay(&mut self) {
        self.replay = None;
    }

    /// Shows `game` at once, ending whatever is on screen (skipping the
    /// replay of a correspondence game's strokes).
    pub fn jump_to(&mut self, game: EightBallGame) {
        self.replay = None;
        self.last_shot = None;
        self.game = game;
        if self.game.game_over().is_some() {
            self.phase = Phase::GameOver;
        } else {
            self.begin_turn();
        }
    }

    fn begin_turn(&mut self) {
        // Development aid (screenshots): SNOOK_LEGEND_AUTOPLAY_SHOTS=<n> lets
        // the computer play the first player's turns of an offline game for
        // its first n shots, then hands the cue back.
        if self.online.is_none()
            && let Some(n) =
                crate::dev::var("SNOOK_LEGEND_AUTOPLAY_SHOTS").and_then(|v| v.parse::<u32>().ok())
        {
            self.controllers[0] = if self.game.shots_taken() < n {
                Controller::Computer(Difficulty::Hard.player())
            } else {
                Controller::Human
            };
            // The player's cue level and centred, whatever the computer did.
            if self.game.current_player() == 0 && self.controllers[0] == Controller::Human {
                self.aim.elevation = 0.0;
                self.aim.tip = DVec2::ZERO;
            }
        }
        // A drill's next try starts from its position again.
        if let Some(run) = self.drill.as_mut().filter(|r| r.reset) {
            run.reset = false;
            let k = run.results.len() as u32;
            self.game =
                snook_legend_proto::drills::game_for_session_try(run.drill(), run.session, k);
        }
        // A replay: the next stroke decides the turn.
        let mut scripted = None;
        while let Some(script) = self.script.as_mut() {
            use snook_legend_proto::replay::Stroke;
            match script.pop_front() {
                Some(Stroke::Shot { shot, called, cue_ball, .. }) => {
                    scripted = Some(AiDecision { shot, called_pocket: called, cue_ball });
                    break;
                }
                Some(Stroke::TimeFoul) => {
                    let name = self.names[self.game.current_player()].clone();
                    self.notify(tw("{name} ran out of time.", &[("name", &name)]), false);
                    self.game.time_foul();
                }
                Some(Stroke::Choice { option }) => {
                    let chooser = self.game.current_player();
                    if let Some(choice) = self.game.pending_choice()
                        && self.game.choose(option).is_ok()
                    {
                        self.tell_choice(chooser, choice, option);
                    }
                }
                Some(stroke @ (Stroke::Resign { seat } | Stroke::Abandon { seat })) => {
                    if matches!(stroke, Stroke::Abandon { .. }) {
                        self.game.abandon(seat % 2);
                    } else {
                        self.game.resign(seat % 2);
                    }
                    let name = self.names[seat % 2].clone();
                    self.notify(tw("{name} resigned.", &[("name", &name)]), true);
                    self.phase = Phase::GameOver;
                    return;
                }
                None => {
                    self.phase = Phase::GameOver;
                    return;
                }
            }
        }
        // A choice the break left: no shot to work out (the computer
        // answers it after a moment, see `run_computer_choice`).
        let choosing = self.game.pending_choice().is_some() && scripted.is_none();
        self.choice_wait = 0.0;
        let computer = match self.current_controller() {
            _ if choosing => None,
            Controller::Computer(_) if scripted.is_some() => Some(ComputerTurn {
                result: Arc::new(Mutex::new(scripted)),
                decision: None,
                elapsed: 0.0,
                start_yaw: 0.0,
                think: 0.5,
                waited: 0.0,
                first_miss: 0.0,
            }),
            Controller::Computer(player) => {
                let result = Arc::new(Mutex::new(None));
                let slot = result.clone();
                let game = self.game.clone();
                // The seed the server uses to check a ladder game.
                let seed = snook_legend_proto::ladder::ai_seed(self.seed, self.game.shots_taken());
                std::thread::spawn(move || {
                    // Never leave the computer "thinking" forever: if the search
                    // fails, play a plain shot at a legal ball.
                    let decision =
                        std::panic::catch_unwind(|| ai::decide_with(&game, &player, seed))
                            .unwrap_or_else(|_| fallback_decision(&game));
                    *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(decision);
                });
                // Pseudo-random pacing from the seed: 1.0–2.8 s of thinking.
                let r = |k: u64| {
                    ((seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> k) & 0xffff) as f32 / 65535.0
                };
                Some(ComputerTurn {
                    result,
                    decision: None,
                    elapsed: 0.0,
                    start_yaw: 0.0,
                    think: 1.0 + 1.8 * r(3),
                    waited: 0.0,
                    first_miss: ((r(19) - 0.5) * 6.0).to_radians() as f64,
                })
            }
            Controller::Human | Controller::Remote => None,
        };
        if let Some(o) = self.online.as_mut() {
            o.clock_left = o.next_clock.min(o.shot_clock);
            o.next_clock = o.shot_clock;
            o.last_aim = None;
            o.last_cue = None;
            o.remote_aimed = false;
            o.remote_target = None;
            o.sim_turn = None;
            o.predicted = None;
            o.turn_start = None;
            o.choice_sent = None;
        }
        if let Some((total, left)) = &mut self.clock {
            *left = *total;
        }
        self.aim = Aim { yaw: self.game.default_aim(), ..Aim::default() };
        // The Brazilian rules: a pocket called for the 8 in an earlier visit
        // still stands. The official rules: every shot is called afresh
        // (from the aim, until the player picks a pocket or a safety).
        self.called_pocket = self.game.called_pocket_of(self.game.current_player());
        self.call_locked = false;
        // Ball in hand never locks the turn: the cue ball can be dragged at
        // any time, and the power bar and aiming work straight away.
        self.phase = Phase::Turn { placing_cue_ball: false, computer };
    }

    pub fn shot_from_aim(&self) -> Shot {
        let max = self.game.max_cue_speed();
        Shot {
            yaw: self.aim.yaw,
            elevation: self.aim.elevation,
            tip: self.aim.tip,
            speed: power_to_speed(self.aim.power, max),
        }
    }

    /// Strikes the cue ball with the current aim. Returns false (with a notice) if
    /// the shot is not allowed yet. Online, the shot plays at once and the
    /// server's verdict is applied when the balls stop.
    pub fn shoot(&mut self) -> bool {
        if !matches!(self.phase, Phase::Turn { .. }) {
            return false;
        }
        self.replay = None;
        self.follow_call();
        let shot = self.forced_shot.take().unwrap_or_else(|| self.shot_from_aim());
        match self.game.prepare_shot(&shot, self.called_pocket) {
            Ok(pending) => {
                if let Some(run) = self.daily.as_mut() {
                    run.strokes.push(shot);
                }
                if let Some(run) = self.drill.as_mut() {
                    run.strokes.push(shot);
                }
                if let Some(run) = self.ladder.as_mut() {
                    let cue_ball =
                        self.game.ball_in_hand().map(|_| self.game.ball(BallId::CUE).xy());
                    run.record.strokes.push(snook_legend_proto::replay::Stroke::Shot {
                        shot,
                        called: self.called_pocket,
                        cue_ball,
                        timeout: false,
                    });
                }
                if let Some(o) = self.online.as_mut() {
                    let turn = self.game.shots_taken();
                    let cue_ball =
                        self.game.ball_in_hand().map(|_| self.game.ball(BallId::CUE).xy());
                    o.outbox.push(ClientMsg::Shoot {
                        turn,
                        shot,
                        called_pocket: self.called_pocket,
                        cue_ball,
                    });
                    o.predicted = Some((turn, shot));
                    o.sim_turn = Some(turn);
                    o.turn_start = Some(self.game.clone());
                    o.pending_end = None;
                }
                self.start_animation(pending, shot);
                true
            }
            Err(e) => {
                self.shot_error(e);
                false
            }
        }
    }

    fn start_animation(&mut self, pending: PendingShot, shot: Shot) {
        self.shot_in_hand = self.game.ball_in_hand().is_some();
        self.last_shot =
            Some(ReplayShot { start: self.game.clone(), shot, called: pending.called_pocket });
        self.replay = None;
        self.sounds.push(ImpactSound { kind: ImpactKind::Cue, speed: shot.speed as f32 });
        self.events_seen = 0;
        self.aim.charging = false;
        self.phase = Phase::Simulating(Box::new(pending));
    }

    fn shot_error(&mut self, e: ShotError) {
        match e {
            ShotError::MustCallPocket => {
                self.notify(t("Call a pocket for the 8-ball first: click a pocket."), true)
            }
            e => self.notify(tw("Can't shoot: {e}.", &[("e", &crate::i18n::shot_error(e))]), true),
        }
    }

    fn report_shot(&mut self, report: &ShotReport) {
        self.count_visit(report);
        self.stats[report.shooter % 2].add(report, self.shot_in_hand);
        if let Some(last) = &self.last_shot {
            self.stats[report.shooter % 2].add_spin(&last.shot, report);
            let cue = last.start.ball(BallId::CUE).position;
            // The balls made for the player (not the opponent's, nor an 8 that
            // loses), as the numbers count them.
            let made = report.balls_made().len() as u32;
            self.timeline.push(ShotMark {
                shooter: report.shooter % 2,
                cue: (cue.x, cue.y),
                made,
                foul: report.is_foul(),
            });
        }
        self.shot_in_hand = false;
        let shooter = self.names[report.shooter].clone();
        let next = self.names[self.game.current_player()].clone();
        let rules = self.game.rules();
        // A foul: what it was and, under it, what it gives the opponent.
        let foul_said = report.is_foul() && report.game_over.is_none() && !report.break_again;
        if report.is_foul() {
            let fouls: Vec<String> = report.fouls.iter().map(crate::i18n::foul).collect();
            let text = tw(
                "Foul by {shooter}: {foul}.",
                &[("shooter", &shooter), ("foul", &fouls.join("; "))],
            );
            let detail = foul_said.then(|| self.foul_consequence(report, &next)).flatten();
            self.notify_detail(text, detail);
        }
        if let Some(g) = report.group_assigned {
            self.notify(
                tw(
                    "{shooter} takes {group}.",
                    &[("shooter", &shooter), ("group", &crate::i18n::group(g))],
                ),
                false,
            );
        }
        let potted: Vec<String> = report
            .pocketed
            .iter()
            .filter(|(b, _)| b.is_object())
            .map(|(b, _)| crate::i18n::ball(*b))
            .collect();
        if !potted.is_empty() {
            self.notify(
                tw(
                    "{shooter} pocketed {balls}.",
                    &[("shooter", &shooter), ("balls", &potted.join(", "))],
                ),
                false,
            );
        }
        for b in &report.spotted {
            self.notify(tw("The {ball} was spotted.", &[("ball", &crate::i18n::ball(*b))]), false);
        }
        if let Some(over) = report.game_over {
            let winner = self.names[over.winner].clone();
            let how = crate::i18n::result_line(over.reason, &winner, &self.names[1 - over.winner]);
            self.notify(tw("{winner} wins! ({how})", &[("winner", &winner), ("how", &how)]), true);
            return;
        }
        if let Some(choice) = report.choice {
            // The official rules: the break leaves a decision to make.
            let breaker = self.names[report.shooter % 2].clone();
            let (text, detail) = crate::i18n::break_choice(choice, &next, &breaker);
            self.notify_detail(text, Some(detail));
            return;
        }
        if report.continues {
            return;
        }
        if report.break_again {
            // The Brazilian rules: the break is handed over.
            let text = if report.first_contact.is_none() {
                "No ball was hit on the break: {next} breaks, with the cue ball in hand behind the head string."
            } else if report.pocketed.iter().any(|(b, _)| b.is_object()) {
                "The cue ball went down on the break with balls of both groups: no penalty, the table stays open and {next} breaks, with the cue ball in hand behind the head string."
            } else {
                "The cue ball went down on the break with no ball made: {next} breaks, with the cue ball in hand behind the head string."
            };
            self.notify(tw(text, &[("next", &next)]), true);
        } else if foul_said {
            // Said under the foul.
        } else if let Some(bih) = self.game.ball_in_hand() {
            let where_ = match bih {
                BallInHand::Kitchen => t(" behind the head string"),
                BallInHand::Anywhere => "",
            };
            self.notify(
                tw("{next} has ball in hand{where}.", &[("next", &next), ("where", &where_)]),
                false,
            );
        } else if report.opponent_ball {
            // The Brazilian rules: an opponent's ball made passes the turn,
            // even with one's own.
            self.notify(
                tw(
                    "{shooter} made an opponent's ball: {next} to shoot.",
                    &[("shooter", &shooter), ("next", &next)],
                ),
                false,
            );
        } else if rules == Rules::Official && !report.was_break && !potted.is_empty() {
            // The official rules: balls made without the call stay down,
            // and the turn passes.
            let text = match report.called {
                None => tw("No pocket was called (a safety): {next} to shoot.", &[("next", &next)]),
                Some(pocket) => tw(
                    "No ball went in the called pocket ({pocket}): {next} to shoot.",
                    &[("pocket", &crate::i18n::pocket(pocket)), ("next", &next)],
                ),
            };
            self.notify(text, false);
        }
    }

    /// What a foul gives the opponent, as the rules of the game say: the
    /// cue ball in hand (the official rules), or balls and where the cue
    /// ball is played from (the Brazilian rules).
    fn foul_consequence(&self, report: &ShotReport, next: &str) -> Option<String> {
        match self.game.rules() {
            Rules::Official => self.game.ball_in_hand().map(|bih| match bih {
                BallInHand::Anywhere => tw(
                    "{next} has the cue ball in hand: anywhere on the table.",
                    &[("next", &next)],
                ),
                BallInHand::Kitchen => tw(
                    "{next} has the cue ball in hand behind the head string.",
                    &[("next", &next)],
                ),
            }),
            Rules::Brazilian => {
                let mut parts = Vec::new();
                if !report.penalty.is_empty() {
                    let given: Vec<String> =
                        report.penalty.iter().map(|b| crate::i18n::ball(*b)).collect();
                    let other = &self.names[1 - report.shooter % 2];
                    parts.push(tw(
                        "Penalty: {balls} off the table for {name}.",
                        &[("balls", &given.join(", ")), ("name", other)],
                    ));
                }
                let scratched = report
                    .fouls
                    .iter()
                    .any(|f| matches!(f, Foul::CueBallPocketed | Foul::CueBallOffTable));
                if let Some(bih) = self.game.ball_in_hand() {
                    let where_ = match bih {
                        BallInHand::Kitchen => t(" behind the head string"),
                        BallInHand::Anywhere => "",
                    };
                    parts.push(tw(
                        "{next} has ball in hand{where}.",
                        &[("next", &next), ("where", &where_)],
                    ));
                } else if scratched {
                    // No ball in hand: the cue ball is back on the head
                    // spot, where the next shot is played from.
                    parts.push(tw(
                        "The cue ball is back on the head spot: {next} plays from there.",
                        &[("next", &next)],
                    ));
                }
                (!parts.is_empty()).then(|| parts.join(" "))
            }
        }
    }

    /// The player to move answers the choice the break left them (the
    /// official rules): at once here; online, through the server.
    pub fn choose(&mut self, option: BreakOption) {
        let Some(choice) = self.game.pending_choice() else { return };
        if !choice.options().contains(&option) || !matches!(self.phase, Phase::Turn { .. }) {
            return;
        }
        if let Some(o) = self.online.as_mut() {
            if o.choice_sent.is_none() {
                let turn = self.game.shots_taken();
                o.outbox.push(ClientMsg::Choose { turn, option });
                o.choice_sent = Some(option);
            }
            return;
        }
        let chooser = self.game.current_player();
        if self.game.choose(option).is_err() {
            return;
        }
        if let Some(run) = self.ladder.as_mut() {
            run.record.strokes.push(snook_legend_proto::replay::Stroke::Choice { option });
        }
        self.tell_choice(chooser, choice, option);
        self.begin_turn();
    }

    /// Says what `chooser` chose.
    fn tell_choice(&mut self, chooser: usize, choice: BreakChoice, option: BreakOption) {
        let name = self.names[chooser % 2].clone();
        let other = self.names[1 - chooser % 2].clone();
        let text = crate::i18n::chosen(choice, option, &name, &other);
        self.notify(text, true);
    }
}

/// The computer's pacing, so its turns read like a person's: turn the cue
/// towards the shot, refine the aim, pause, then draw the cue back and shoot.
const AI_AIM_TIME: f32 = 1.3;
const AI_REFINE_TIME: f32 = 0.9;
const AI_SETTLE_TIME: f32 = 0.5;
const AI_STROKE_TIME: f32 = 1.1;

impl Match {
    /// Advances the match by `dt` seconds of real time.
    pub fn update(&mut self, dt: f32) {
        if !self.greeted && self.talk {
            self.greeted = true;
            self.say(crate::banter::Moment::Start);
        }
        // A stored game: paused, slowed down or sped up.
        let dt = match &self.video {
            Some(v) if v.paused => return,
            Some(v) => dt * v.speed,
            None => dt,
        };
        self.run_online(dt);
        self.run_offline_clock(dt);
        self.follow_call();
        self.run_computer_choice(dt);
        self.run_computer_turn(dt);
        self.run_simulation(dt);
        self.run_replay(dt);
        self.turn_sounds();
        if let Phase::Pause(t) = &mut self.phase {
            *t -= dt;
            if *t <= 0.0 {
                self.begin_turn();
            }
        }
        // The opponent's speech bubble fades after a few seconds.
        self.banter_quiet += dt;
        if let Some((_, age)) = &mut self.banter {
            *age += dt;
            if *age > BANTER_TIME {
                self.banter = None;
            }
        }
        for n in &mut self.notices {
            n.ttl -= dt;
        }
        self.notices.retain(|n| n.ttl > 0.0);
        // Computer-vs-computer matches restart by themselves.
        let all_computers = self.controllers.iter().all(|c| matches!(c, Controller::Computer(_)));
        if all_computers && self.script.is_none() && matches!(self.phase, Phase::GameOver) {
            self.demo_timer += dt;
            if self.demo_timer > 4.0 {
                self.demo_timer = 0.0;
                self.rematch();
            }
        }
    }

    /// The current player concedes the game (online: this player).
    pub fn resign(&mut self) {
        if self.game.game_over().is_some() {
            return;
        }
        if let Some(o) = self.online.as_mut() {
            // The server ends the game and tells both players.
            o.outbox.push(ClientMsg::Resign);
            return;
        }
        let player = self.game.current_player();
        self.game.resign(player);
        self.replay = None;
        if let Some(over) = self.game.game_over() {
            self.wins[over.winner] += 1;
            self.games_played += 1;
            let name = self.names[player].clone();
            self.notify(tw("{name} resigned.", &[("name", &name)]), true);
        }
        self.phase = Phase::GameOver;
    }

    /// Offline shot clock (two players, or the human against the computer).
    fn run_offline_clock(&mut self, dt: f32) {
        // The replay of the last shot doesn't stop the clock (pressing R
        // again and again would have frozen it); it never runs out during
        // one, though: the turn is lost with the table in sight.
        let replay = self.replay.is_some();
        let human = self.current_controller() == Controller::Human;
        let turn = matches!(self.phase, Phase::Turn { .. });
        let Some((_, left)) = &mut self.clock else { return };
        if !(human && turn) {
            return;
        }
        if replay {
            *left = (*left - dt).max(0.5f32.min(*left));
            return;
        }
        *left -= dt;
        if *left > 0.0 {
            return;
        }
        let who = self.names[self.game.current_player()].clone();
        // A choice not made in time takes its first option.
        if let Some(choice) = self.game.pending_choice() {
            self.notify(tw("{name} ran out of time.", &[("name", &who)]), false);
            self.choose(choice.options()[0]);
            return;
        }
        self.game.time_foul();
        if let Some(run) = self.ladder.as_mut() {
            run.record.strokes.push(snook_legend_proto::replay::Stroke::TimeFoul);
        }
        let text = time_foul_text(&self.game, &who);
        self.notify(text, true);
        self.begin_turn();
    }

    /// The official rules: while the player hasn't chosen the call, it is
    /// the pocket their aim sends the object ball to (none: a safety).
    fn follow_call(&mut self) {
        if self.call_locked || !self.game.calls_shots() || !self.is_human_turn() {
            return;
        }
        self.called_pocket = crate::play::predicted_pocket(&self.game, self.aim.yaw);
    }

    /// The official rules: the player calls `pocket` for this shot (none: a
    /// safety) instead of the pocket their aim points at.
    pub fn set_call(&mut self, pocket: Option<PocketId>) {
        self.called_pocket = pocket;
        self.call_locked = true;
    }

    /// The official rules: the call follows the aim again.
    pub fn call_from_aim(&mut self) {
        self.call_locked = false;
        self.follow_call();
    }

    /// The computer answers the choice a break left it, after a moment
    /// (as a person would read the table first).
    fn run_computer_choice(&mut self, dt: f32) {
        if self.replay.is_some()
            || self.script.is_some()
            || !matches!(self.phase, Phase::Turn { .. })
            || !matches!(self.current_controller(), Controller::Computer(_))
            || self.game.pending_choice().is_none()
        {
            return;
        }
        self.choice_wait += dt;
        if self.choice_wait >= 1.6
            && let Some(option) = ai::choose(&self.game)
        {
            self.choose(option);
        }
    }

    fn run_computer_turn(&mut self, dt: f32) {
        let max_speed = self.game.max_cue_speed();
        let current_yaw = self.aim.yaw;
        if self.replay.is_some() {
            return;
        }
        let Phase::Turn { computer: Some(turn), .. } = &mut self.phase else {
            return;
        };
        turn.waited += dt;
        if turn.decision.is_none() {
            if turn.waited < turn.think {
                return;
            }
            let ready = turn.result.lock().unwrap_or_else(|e| e.into_inner()).take();
            match ready {
                Some(decision) => {
                    if let (Some(pocket), true) =
                        (decision.called_pocket, self.game.may_call_pocket())
                    {
                        let name = self.names[self.game.current_player()].clone();
                        let text = tw(
                            "{name} calls the 8-ball in the {pocket} pocket.",
                            &[("name", &name), ("pocket", &crate::i18n::pocket(pocket))],
                        );
                        announce_into(
                            &mut self.notices,
                            &mut self.log,
                            None,
                            text,
                            None,
                            true,
                            5.0,
                        );
                    }
                    turn.decision = Some(decision);
                    turn.start_yaw = current_yaw;
                    turn.elapsed = 0.0;
                }
                None => return,
            }
        }
        let decision = turn.decision.expect("set above");
        turn.elapsed += dt;
        let t = turn.elapsed;
        let start_yaw = turn.start_yaw;

        if let Some(p) = decision.cue_ball
            && self.game.ball(BallId::CUE).xy() != p
        {
            self.game.place_cue_ball(p);
        }
        let target_power = speed_to_power(decision.shot.speed, max_speed);
        let first_miss = turn.first_miss;
        let ease = |k: f32| {
            let k = k.clamp(0.0, 1.0);
            (k * k * (3.0 - 2.0 * k)) as f64
        };
        let delta = angle_delta(start_yaw, decision.shot.yaw);
        // Swing the cue near the line, then refine it slowly onto the shot.
        let rough = start_yaw + (delta + first_miss) * ease(t / AI_AIM_TIME);
        let fine = ease((t - AI_AIM_TIME) / AI_REFINE_TIME);
        self.aim.yaw = rough - first_miss * fine;
        self.aim.tip = decision.shot.tip;
        self.aim.elevation = decision.shot.elevation;
        self.called_pocket = decision.called_pocket;
        let stroke_start = AI_AIM_TIME + AI_REFINE_TIME + AI_SETTLE_TIME;
        if t > stroke_start {
            self.aim.charging = true;
            let k = ((t - stroke_start) / AI_STROKE_TIME).clamp(0.0, 1.0);
            self.aim.power = target_power * (k * k * (3.0 - 2.0 * k));
        }
        if t > stroke_start + AI_STROKE_TIME + 0.35 {
            self.aim.yaw = decision.shot.yaw;
            self.aim.power = target_power;
            self.forced_shot = Some(decision.shot);
            if !self.shoot() {
                self.called_pocket = Some(PocketId::FootLeft);
                self.forced_shot = Some(decision.shot);
                if !self.shoot() && self.online.is_none() {
                    // Nothing it decided can be played (a stroke of a
                    // stored game that no longer fits): the turn passes,
                    // as when the clock runs out, instead of trying again
                    // every frame.
                    let who = self.names[self.game.current_player()].clone();
                    self.game.time_foul();
                    if let Some(run) = self.ladder.as_mut() {
                        run.record.strokes.push(snook_legend_proto::replay::Stroke::TimeFoul);
                    }
                    let text = time_foul_text(&self.game, &who);
                    self.notify(text, false);
                    self.begin_turn();
                }
            }
        }
    }

    /// The balls made in a row this visit: counted on every legal shot that
    /// keeps the table (or wins the game), back to nothing when the turn
    /// passes. Three, five and seven in a row are cheered, and a game won
    /// in one visit from the first ball to the 8.
    fn count_visit(&mut self, report: &ShotReport) {
        let shooter = report.shooter % 2;
        let won = report.game_over.is_some_and(|g| g.winner % 2 == shooter);
        if self.visit_run.0 != shooter {
            self.visit_run = (shooter, 0);
        }
        let made = report.balls_made().len() as u32;
        if report.is_foul() || !(report.continues || won) || made == 0 {
            self.visit_run.1 = 0;
            return;
        }
        let before = self.visit_run.1;
        self.visit_run.1 += made;
        let after = self.visit_run.1;
        let name = self.names[shooter].clone();
        if won && after >= 8 {
            self.good_news(tw("{name} cleared the table in one visit!", &[("name", &name)]), 5.0);
        } else if let Some(n) = [7, 5, 3].into_iter().find(|n| before < *n && after >= *n) {
            self.good_news(tw("{name}: {n} in a row!", &[("name", &name), ("n", &n)]), 3.0);
        }
    }

    /// The clock's last five seconds tick, and online a soft chime says the
    /// turn has come back to this player.
    fn turn_sounds(&mut self) {
        let mine = matches!(self.phase, Phase::Turn { .. })
            && self.current_controller() == Controller::Human;
        let watching = self.is_spectating();
        if mine && !self.my_turn_seen && self.online.is_some() && !watching && !self.is_replay() {
            self.sounds.push(ImpactSound { kind: ImpactKind::Chime, speed: 1.0 });
        }
        self.my_turn_seen = mine;
        let left = self.shot_clock().map(|(left, _)| left).filter(|l| *l > 0.0 && *l <= 5.0);
        match left {
            Some(left) if mine && !watching => {
                let second = left.ceil() as i32;
                if self.last_tick != Some(second) {
                    self.last_tick = Some(second);
                    self.sounds.push(ImpactSound { kind: ImpactKind::Tick, speed: 1.0 });
                }
            }
            _ => self.last_tick = None,
        }
    }

    fn collect_sounds(events: &[Event], from: usize, out: &mut Vec<ImpactSound>) {
        for e in &events[from.min(events.len())..] {
            let sound = match *e {
                Event::BallBall { speed, .. } => Some((ImpactKind::Ball, speed)),
                Event::Cushion { speed, .. } => Some((ImpactKind::Cushion, speed)),
                Event::Slate { speed, .. } => Some((ImpactKind::Cushion, speed * 0.5)),
                Event::Pocketed { .. } => Some((ImpactKind::Pocket, 1.0)),
                Event::OffTable { .. } => None,
            };
            if let Some((kind, speed)) = sound {
                out.push(ImpactSound { kind, speed: speed as f32 });
            }
        }
    }

    fn run_simulation(&mut self, dt: f32) {
        // Real time, so shots never play in slow motion on slow computers.
        let dt = (dt as f64).min(0.1);
        let events_seen = self.events_seen;
        let Phase::Simulating(pending) = &mut self.phase else {
            return;
        };
        let moving = pending.simulation.advance(dt);
        Self::collect_sounds(pending.simulation.events(), events_seen, &mut self.sounds);
        self.events_seen = pending.simulation.events().len();
        if !moving {
            self.end_animation();
        }
    }

    /// The balls stopped: apply the result (online, the server's).
    fn end_animation(&mut self) {
        let phase = std::mem::replace(&mut self.phase, Phase::Pause(0.7));
        let (Phase::Simulating(pending) | Phase::Settling(pending)) = phase else {
            self.phase = phase;
            return;
        };
        let report = match self.online.as_mut() {
            Some(o) => match o.pending_end.take() {
                Some((end, report)) => {
                    if crate::dev::flag("SNOOK_LEGEND_NET_DEBUG") {
                        let mut local = self.game.clone();
                        local.finish_shot((*pending).clone());
                        let same = local == end;
                        eprintln!(
                            "shot {}: prediction {}",
                            end.shots_taken(),
                            if same { "matched the server" } else { "DIFFERED from the server" }
                        );
                    }
                    // The server's result is authoritative.
                    self.game = end;
                    report
                }
                None if o.ended_pending => {
                    // The game ended while the shot was on its way (the
                    // server keeps no result for a shot of a finished
                    // game): its end, with the balls where they stopped,
                    // not a wait for nothing.
                    o.ended_pending = false;
                    let result = o.result.clone();
                    let mut local = self.game.clone();
                    local.finish_shot(*pending);
                    if local.game_over().is_none() {
                        self.game = local;
                    }
                    match result {
                        Some(result) => self.settle_result(&result),
                        None => self.phase = Phase::GameOver,
                    }
                    return;
                }
                None => {
                    // Wait for it, showing the balls where they stopped.
                    self.phase = Phase::Settling(pending);
                    return;
                }
            },
            None => self.game.finish_shot(*pending),
        };
        self.report_shot(&report);
        self.after_shot(&report);
    }

    /// Plays any shot still on screen to its end at once (a newer message
    /// from the server needs the table at rest).
    fn fast_forward(&mut self) {
        if let Phase::Simulating(p) = &mut self.phase {
            p.simulation.run_to_rest();
            self.end_animation();
        }
    }

    fn after_shot(&mut self, report: &ShotReport) {
        // A drill: did the try work? Then the next try, or the end.
        if let Some(run) = self.drill.as_mut() {
            let d = run.drill();
            let ok = self
                .last_shot
                .as_ref()
                .and_then(|s| snook_legend_proto::drills::try_shot(d, &s.start, &s.shot))
                .unwrap_or(false);
            run.results.push(ok);
            let done = run.results.len() as u32 >= d.tries
                || (d.goal == snook_legend_proto::drills::Goal::Streak && !ok);
            let made = run.score();
            let text = if ok {
                tw("Made it! {n} of {total}", &[("n", &made), ("total", &d.tries)])
            } else {
                t("Missed.").to_string()
            };
            if done {
                run.over = true;
            } else {
                run.reset = true;
            }
            self.notify(text, ok);
            self.last_report = Some(report.clone());
            self.phase = if done { Phase::GameOver } else { Phase::Pause(1.2) };
            return;
        }
        // The computer opponent's remark about this shot (the player has
        // seat 0 against the computer).
        {
            use crate::banter::Moment;
            let mine = report.shooter == 0;
            let moment = match report.game_over {
                Some(over) if over.winner == 0 => Moment::YouWin,
                Some(_) => Moment::IWin,
                None if report.is_foul() => {
                    if mine {
                        Moment::YouFoul
                    } else {
                        Moment::IFoul
                    }
                }
                None if report.continues => {
                    if mine {
                        Moment::YouPot
                    } else {
                        Moment::IPot
                    }
                }
                None => {
                    if mine {
                        Moment::YouMiss
                    } else {
                        Moment::IMiss
                    }
                }
            };
            self.say(moment);
        }
        // Practice on one's own: a finished rack (the 8 down, however)
        // just starts another.
        if self.free_play && report.game_over.is_some() {
            self.notify(t("A new rack."), false);
            self.rematch();
            return;
        }
        // The daily challenge: the visit ends at the first stroke that
        // doesn't keep the table, or with the table clear.
        if let Some(run) = self.daily.as_mut() {
            let keeps = report.continues && report.game_over.is_none();
            if keeps {
                run.made +=
                    report.pocketed.iter().filter(|(b, _)| *b != BallId::CUE).count() as u32;
            }
            if !keeps || run.made >= run.balls {
                run.over = true;
                self.last_report = Some(report.clone());
                self.phase = Phase::GameOver;
                return;
            }
        }
        if let Some(over) = report.game_over
            && over.reason == snook_legend_core::GameOverReason::PocketedEight
            && let Some((_, pocket)) =
                report.pocketed.iter().find(|(b, _)| *b == snook_legend_core::BallId::EIGHT)
        {
            self.win_effect = Some((over.winner, *pocket));
        }
        if let Some(over) = report.game_over {
            self.wins[over.winner] += 1;
            self.games_played += 1;
            self.phase = Phase::GameOver;
        } else if !matches!(self.phase, Phase::Pause(_)) {
            self.phase = Phase::Pause(0.7);
        }
        self.last_report = Some(report.clone());
        let ended = self.online.as_mut().is_some_and(|o| std::mem::take(&mut o.ended_pending));
        if ended && let Some(result) = self.online.as_ref().and_then(|o| o.result.clone()) {
            self.apply_result(&result);
        }
    }

    fn run_replay(&mut self, dt: f32) {
        let Some(r) = self.replay.as_mut() else { return };
        let seen = r.events_seen;
        let moving = r.simulation.advance((dt as f64).min(0.1));
        Self::collect_sounds(r.simulation.events(), seen, &mut self.sounds);
        r.events_seen = r.simulation.events().len();
        if !moving {
            r.done += dt;
            if r.done > 1.2 {
                self.replay = None;
            }
        }
    }

    /// Simulation time of the shot in progress (for the cue follow-through).
    pub fn shot_time(&self) -> Option<f64> {
        if let Some(r) = &self.replay {
            return Some(r.simulation.time());
        }
        match &self.phase {
            Phase::Simulating(p) => Some(p.simulation.time()),
            _ => None,
        }
    }
}

impl Match {
    /// Development aid (`--on-eight`): clears the solids so player 0 is on the
    /// 8-ball, to try the pocket call.
    /// Development aid: a made-up game's worth of shots for the end-of-game
    /// charts (with SNOOK_LEGEND_RESIGN).
    pub fn debug_shots(&mut self) {
        let mut x = self.seed | 1;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % 1000) as f64 / 1000.0
        };
        let mut shooter = 0;
        for _ in 0..34 {
            let made = if next() < 0.55 { 1 + (next() < 0.2) as u32 } else { 0 };
            let foul = made == 0 && next() < 0.15;
            let cue = ((next() - 0.5) * 2.1, (next() - 0.5) * 1.0);
            self.timeline.push(ShotMark { shooter, cue, made, foul });
            if made == 0 {
                shooter = 1 - shooter;
            }
        }
    }

    pub fn debug_on_eight(&mut self) {
        let Ok(mut v) = serde_json::to_value(&self.game) else { return };
        if let Some(balls) = v["balls"].as_array_mut() {
            for b in balls.iter_mut() {
                let id = b["id"].as_u64().unwrap_or(0);
                if (1..=7).contains(&id) || (9..=12).contains(&id) {
                    b["status"] = serde_json::json!({ "Pocketed": "FootLeft" });
                }
            }
        }
        v["groups"] = serde_json::json!(["Solids", "Stripes"]);
        v["is_break"] = serde_json::json!(false);
        v["ball_in_hand"] = serde_json::json!("Anywhere");
        v["current"] = serde_json::json!(0);
        if let Ok(g) = serde_json::from_value(v) {
            self.game = g;
            self.begin_turn();
        }
    }

    /// Development aid (screenshots): a soft break, as the game starts, so
    /// the official rules' choice comes up.
    pub fn debug_soft_break(&mut self) {
        if !self.game.is_break() {
            return;
        }
        let Ok(mut pending) =
            self.game.prepare_shot(&Shot::new(self.game.default_aim(), 0.6), None)
        else {
            return;
        };
        pending.simulation.run_to_rest();
        let report = self.game.finish_shot(pending);
        self.report_shot(&report);
        self.begin_turn();
    }
}

/// The signed difference `to - from`, in (-π, π].
pub fn angle_delta(from: f64, to: f64) -> f64 {
    let mut d = (to - from).rem_euclid(std::f64::consts::TAU);
    if d > std::f64::consts::PI {
        d -= std::f64::consts::TAU;
    }
    d
}

/// A simple legal-looking shot for when the AI search fails.
fn fallback_decision(game: &EightBallGame) -> AiDecision {
    let cue = game.ball(BallId::CUE).xy();
    let target = game.legal_targets().into_iter().next().map(|id| game.ball(id).xy());
    let d = target.map(|t| t - cue).unwrap_or(DVec2::X);
    let called = game.needs_called_pocket().then_some(PocketId::FootLeft);
    AiDecision { cue_ball: None, shot: Shot::new(d.y.atan2(d.x), 2.0), called_pocket: called }
}

/// Adds a message to the log (and online, to the chat), and shows it for
/// `ttl` seconds.
fn announce_into(
    notices: &mut Vec<Notice>,
    log: &mut Vec<(String, bool)>,
    online: Option<&mut OnlineMatch>,
    text: String,
    detail: Option<String>,
    important: bool,
    ttl: f32,
) {
    // The log and the chat say it in one line.
    let line = match &detail {
        Some(d) => format!("{text} {d}"),
        None => text.clone(),
    };
    log.push((line.clone(), important));
    if log.len() > 60 {
        log.remove(0);
    }
    if let Some(o) = online {
        o.push_feed(FeedLine::event(&line, important));
    }
    notices.push(Notice { text, detail, important, ttl, cheer: false });
    if notices.len() > 5 {
        notices.remove(0);
    }
}

/// A random seed from the clock.
/// Seconds since 1970 (0 if the clock is broken).
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn clock_seed() -> u64 {
    // Development aid (screenshots): SNOOK_LEGEND_SEED=<n> racks and breaks
    // the same way every time.
    if let Some(seed) = crate::dev::var("SNOOK_LEGEND_SEED").and_then(|v| v.parse().ok()) {
        return seed;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(7)
}

/// How often the aim is streamed to the opponent.
const AIM_INTERVAL: f32 = 0.05;
/// How fast the opponent's cue glides to their latest aim (1/s).
const REMOTE_AIM_RATE: f64 = 16.0;

impl OnlineMatch {
    fn push_feed(&mut self, line: FeedLine) {
        self.feed.push(line);
        if self.feed.len() > 150 {
            self.feed.remove(0);
        }
    }
}

impl Match {
    /// Online bookkeeping: the shot clock, streaming the aim and smoothing
    /// the opponent's cue.
    fn run_online(&mut self, dt: f32) {
        let my_turn = self.is_human_turn();
        let placing = self.is_placing_cue_ball();
        let remote_turn = self.current_controller() == Controller::Remote
            && matches!(self.phase, Phase::Turn { .. });
        let cue = self.game.ball(BallId::CUE).xy();
        let aim = AimState {
            yaw: self.aim.yaw,
            elevation: self.aim.elevation,
            tip: self.aim.tip,
            power: if self.aim.charging { self.aim.power } else { 0.0 },
            called_pocket: self.called_pocket,
        };
        let Some(o) = self.online.as_mut() else { return };
        if matches!(self.phase, Phase::Turn { .. }) {
            o.clock_left = (o.clock_left - dt).max(0.0);
        } else {
            // The server's clock for the next turn already runs during playback.
            o.next_clock = (o.next_clock - dt).max(0.0);
        }
        // Watching: the shot as the shooter aims it, assessed once it settles
        // (before they set the power, an ordinary stroke is assumed).
        match o.remote_target.filter(|_| remote_turn && o.spectator) {
            Some(target) => {
                let power = if target.power > 0.05 { target.power } else { 0.4 };
                let shot = Shot {
                    yaw: target.yaw,
                    elevation: target.elevation,
                    tip: target.tip,
                    speed: power_to_speed(power, self.game.max_cue_speed()),
                };
                let turn = self.game.shots_taken();
                o.assessor.update(&self.game, turn, shot, target.called_pocket, dt);
            }
            None => o.assessor.clear(),
        }
        if remote_turn && let Some(target) = o.remote_target {
            let k = 1.0 - (-(dt as f64) * REMOTE_AIM_RATE).exp();
            self.aim.yaw += angle_delta(self.aim.yaw, target.yaw) * k;
            self.aim.elevation += (target.elevation - self.aim.elevation) * k;
            self.aim.tip += (target.tip - self.aim.tip) * k;
            self.aim.power += (target.power - self.aim.power) * k as f32;
            self.aim.charging = target.power > 0.0;
        }
        if !my_turn {
            return;
        }
        o.aim_timer -= dt;
        if o.aim_timer > 0.0 {
            return;
        }
        o.aim_timer = AIM_INTERVAL;
        if placing {
            if o.last_cue != Some(cue) {
                o.last_cue = Some(cue);
                o.outbox.push(ClientMsg::PlaceCueBall { position: cue });
            }
        } else if o.last_aim != Some(aim) {
            o.last_aim = Some(aim);
            o.outbox.push(ClientMsg::Aim { aim });
        }
    }

    /// Ends the game with the server's result (resignation, abandonment…).
    fn apply_result(&mut self, result: &MatchResult) {
        if self.game.game_over().is_some() {
            return;
        }
        self.settle_result(result);
    }

    /// The end of the game as the server gives it: the notice, the series
    /// count and the end panel (also when the table shown is already over,
    /// as after a shot the server rolled back).
    fn settle_result(&mut self, result: &MatchResult) {
        let winner = result.winner % 2;
        let result = &MatchResult { winner, ..result.clone() };
        let loser = 1 - result.winner;
        let name = self.names[loser].clone();
        let text = match result.reason {
            GameOverReason::Abandoned => tw("{name} left the game.", &[("name", &name)]),
            GameOverReason::Resignation => tw("{name} resigned.", &[("name", &name)]),
            reason => {
                let winner = &self.names[result.winner];
                let how = crate::i18n::result_line(reason, winner, &name);
                tw("{winner} wins! ({how})", &[("winner", winner), ("how", &how)])
            }
        };
        self.game.conclude(result.winner, result.reason);
        self.notify(text, true);
        self.wins[result.winner] += 1;
        self.games_played += 1;
        self.replay = None;
        self.phase = Phase::GameOver;
    }

    /// Applies a match message from the server.
    pub fn handle_server(&mut self, msg: ServerMsg) {
        let remote_turn = self.current_controller() == Controller::Remote
            && matches!(self.phase, Phase::Turn { .. });
        let in_progress = self.shot_in_progress();
        let Some(o) = self.online.as_mut() else { return };
        let opponent = self.names[1 - o.seat].clone();
        match msg {
            ServerMsg::OpponentAim { aim } => {
                if remote_turn && aim.is_finite() {
                    let max_elevation = self.game.params().max_elevation;
                    let target = AimState {
                        yaw: aim.yaw,
                        elevation: aim.elevation.clamp(0.0, max_elevation),
                        tip: if aim.tip.length() > 1.0 { aim.tip.normalize() } else { aim.tip },
                        power: aim.power.clamp(0.0, 1.0),
                        called_pocket: aim.called_pocket,
                    };
                    if !o.remote_aimed {
                        // First aim this turn: jump there.
                        self.aim.yaw = target.yaw;
                        self.aim.elevation = target.elevation;
                        self.aim.tip = target.tip;
                    }
                    o.remote_aimed = true;
                    o.remote_target = Some(target);
                    self.called_pocket = target.called_pocket;
                }
            }
            ServerMsg::OpponentCueBall { position } => {
                if remote_turn && position.is_finite() {
                    self.game.place_cue_ball(position);
                }
            }
            ServerMsg::ShotTaken { seat, turn, shot, called_pocket, cue_ball, timeout } => {
                // Our own shot, already playing.
                if o.predicted.is_some_and(|(t, s)| t == turn && s == shot)
                    && seat == o.seat
                    && !timeout
                {
                    return;
                }
                let replace_prediction = o.predicted.is_some_and(|(t, _)| t == turn);
                let base = if replace_prediction { o.turn_start.take() } else { None };
                o.predicted = None;
                if timeout {
                    let text = tw(
                        "Time's up: {name}'s shot was played at full power.",
                        &[("name", &self.names[seat])],
                    );
                    self.announce(text, true, 5.0);
                }
                if let Some(pocket) = called_pocket
                    && self.online.as_ref().is_some_and(|o| o.seat != seat)
                    && self.game.called_pocket_of(seat).is_none()
                {
                    let text = tw(
                        "{name} calls the 8-ball in the {pocket} pocket.",
                        &[("name", &self.names[seat]), ("pocket", &crate::i18n::pocket(pocket))],
                    );
                    self.announce(text, false, 4.0);
                }
                match base {
                    Some(start) => self.game = start,
                    None => self.fast_forward(),
                }
                if let Some(p) = cue_ball {
                    self.game.place_cue_ball(p);
                }
                let o = self.online.as_mut().expect("online");
                o.sim_turn = Some(turn);
                o.pending_end = None;
                o.rematch_offered = false;
                self.aim = Aim {
                    yaw: shot.yaw,
                    elevation: shot.elevation,
                    tip: shot.tip,
                    power: 0.0,
                    charging: false,
                };
                self.called_pocket = called_pocket;
                match self.game.prepare_shot(&shot, called_pocket) {
                    Ok(pending) => self.start_animation(pending, shot),
                    // Should not happen: the server's result will be adopted
                    // as it arrives, without an animation.
                    Err(_) => {
                        if let Some(o) = self.online.as_mut() {
                            o.sim_turn = None;
                        }
                    }
                }
            }
            ServerMsg::ShotResult { turn, end, report, clock_left } => {
                o.next_clock = clock_left;
                if o.sim_turn == Some(turn) {
                    o.pending_end = Some((end, report));
                    if matches!(self.phase, Phase::Settling(_)) {
                        self.end_animation();
                    }
                } else {
                    // Nothing on screen for this shot (e.g. just reconnected).
                    self.fast_forward();
                    self.game = end;
                    self.report_shot(&report);
                    self.after_shot(&report);
                }
            }
            ServerMsg::ShotRejected { turn, reason, game } => {
                // Our own predicted shot, or the shot on screen when the
                // server could not play it: back to the table as it was. A
                // shot of ours that came too late (the clock's own shot
                // for that turn is already playing) is refused without
                // touching the one on screen.
                let ours = o.predicted.is_some_and(|(t, _)| t == turn);
                let failed = in_progress
                    && o.sim_turn == Some(turn)
                    && reason == "The server could not play that shot.";
                if ours || failed {
                    o.predicted = None;
                    o.sim_turn = None;
                    o.turn_start = None;
                    o.pending_end = None;
                    let ended = std::mem::take(&mut o.ended_pending);
                    let result = o.result.clone().filter(|_| ended);
                    self.game = game;
                    self.replay = None;
                    self.last_shot = None;
                    self.phase = Phase::Turn { placing_cue_ball: false, computer: None };
                    // The game ended meanwhile (a resignation while the shot
                    // was on its way): its end, never a turn that can't be
                    // played.
                    match result {
                        Some(result) => self.settle_result(&result),
                        None if self.game.game_over().is_some() => self.phase = Phase::GameOver,
                        None => {}
                    }
                } else if matches!(self.phase, Phase::Turn { .. }) {
                    self.game = game;
                }
                self.notify(crate::i18n::server_text(&reason), true);
            }
            ServerMsg::Chose { seat, option, game, clock_left, .. } => {
                o.next_clock = clock_left;
                o.choice_sent = None;
                self.fast_forward();
                let choice = self.game.pending_choice();
                self.game = game;
                if let Some(choice) = choice {
                    self.tell_choice(seat, choice, option);
                }
                self.begin_turn();
            }
            ServerMsg::TimeFoul { seat, game, clock_left } => {
                o.next_clock = clock_left;
                let who = self.names[seat].clone();
                self.fast_forward();
                self.game = game;
                let text = time_foul_text(&self.game, &who);
                self.notify(text, true);
                self.begin_turn();
            }
            ServerMsg::MatchEnded { result } => {
                for (r, d) in o.ratings.iter_mut().zip(result.new_ratings) {
                    if result.rated {
                        *r = d;
                    }
                }
                o.result = Some(result.clone());
                if matches!(self.phase, Phase::Settling(_)) && o.pending_end.is_none() {
                    // The balls have stopped and no result is coming for
                    // that shot: the end now, with the balls where they are.
                    if let Phase::Settling(pending) =
                        std::mem::replace(&mut self.phase, Phase::GameOver)
                    {
                        let mut local = self.game.clone();
                        local.finish_shot(*pending);
                        if local.game_over().is_none() {
                            self.game = local;
                        }
                    }
                    self.settle_result(&result);
                } else if in_progress {
                    // Finish showing the shot first (it may be what decided the game).
                    o.ended_pending = true;
                } else if self.game.game_over().is_none() {
                    self.apply_result(&result);
                }
            }
            ServerMsg::RematchOffered => {
                o.rematch_offered = true;
                self.notify(tw("{opponent} wants a rematch.", &[("opponent", &opponent)]), true);
            }
            ServerMsg::OpponentLeft => {
                o.opponent_left = true;
                self.notify(
                    tw("{opponent} went back to the lounge.", &[("opponent", &opponent)]),
                    false,
                );
            }
            ServerMsg::OpponentConnection { connected, grace_secs } => {
                o.opponent_connected = connected;
                if connected {
                    self.notify(tw("{opponent} is back.", &[("opponent", &opponent)]), false);
                } else {
                    self.notify(
                        tw(
                            "{opponent} lost the connection. Waiting up to {secs} s…",
                            &[("opponent", &opponent), ("secs", &grace_secs)],
                        ),
                        true,
                    );
                }
            }
            ServerMsg::MatchChat { line } => {
                let feed = FeedLine::chat(&line);
                if !line.from.eq_ignore_ascii_case(&self.names[o.seat]) {
                    o.unread += 1;
                    o.preview = Some((feed.clone(), 0.0));
                }
                o.push_feed(feed);
            }
            ServerMsg::Reaction { seat, code } => {
                if let Some(r) = snook_legend_proto::reaction(&code) {
                    if seat < 2 {
                        // One at a time per player: a new one replaces the last.
                        o.reactions.retain(|(s, ..)| *s != seat);
                    } else {
                        // The crowd: a few at most, the oldest go first.
                        let crowd = o.reactions.iter().filter(|(s, ..)| *s >= 2).count();
                        if crowd >= 4
                            && let Some(i) = o.reactions.iter().position(|(s, ..)| *s >= 2)
                        {
                            o.reactions.remove(i);
                        }
                    }
                    o.reactions.push((seat.min(SPECTATOR_SEAT), r.code, 0.0));
                }
            }
            ServerMsg::Spectators { count } => o.spectators = count,
            ServerMsg::Support { counts } => o.support = counts,
            ServerMsg::Error { message } => {
                self.notify(crate::i18n::server_text(&message), true);
            }
            _ => {}
        }
    }

    /// Sends a reaction to the table.
    pub fn send_reaction(&mut self, code: &str) {
        if let Some(o) = self.online.as_mut() {
            o.outbox.push(ClientMsg::React { code: code.to_string() });
        }
    }

    /// Watching someone else's game.
    /// A spectator cheers for the player in `seat`.
    pub fn cheer(&mut self, seat: usize) {
        if let Some(o) = self.online.as_mut().filter(|o| o.spectator && o.cheering != Some(seat)) {
            o.cheering = Some(seat);
            o.outbox.push(ClientMsg::Cheer { seat });
        }
    }

    pub fn is_spectating(&self) -> bool {
        self.online.as_ref().is_some_and(|o| o.spectator)
    }

    /// Sends a chat line to the opponent.
    pub fn send_chat(&mut self, text: &str) {
        let text = snook_legend_proto::clean_chat(text);
        if let Some(o) = self.online.as_mut()
            && !o.spectator
            && !text.is_empty()
        {
            o.outbox.push(ClientMsg::MatchChat { text });
        }
    }
}

impl FeedLine {
    pub fn chat(line: &ChatLine) -> FeedLine {
        let (from, text, kind) = match line.kind {
            ChatKind::Player => {
                // A quick phrase reads in this player's language.
                let text = if snook_legend_proto::QUICK_CHAT.contains(&line.text.as_str()) {
                    crate::i18n::t(&line.text).to_string()
                } else {
                    line.text.clone()
                };
                (Some((line.from.clone(), line.rating)), text, FeedKind::Player)
            }
            // The server's own announcements are translated; what the
            // staff write comes through as it is.
            ChatKind::Announcement => {
                (None, crate::i18n::server_text(&line.text), FeedKind::Announcement)
            }
            ChatKind::System => (None, crate::i18n::server_text(&line.text), FeedKind::Event),
        };
        FeedLine { from, text, kind, id: line.id, admin: line.admin, deleted: line.deleted }
    }

    pub fn event(text: &str, important: bool) -> FeedLine {
        FeedLine {
            from: None,
            text: text.to_string(),
            kind: if important { FeedKind::Alert } else { FeedKind::Event },
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_stats_count_runs_fouls_and_ball_in_hand() {
        use snook_legend_core::{Foul, PocketId};
        let report = |made: &[u8], foul: bool, continues: bool| ShotReport {
            shooter: 0,
            was_break: false,
            first_contact: Some(BallId(1)),
            pocketed: made.iter().map(|n| (BallId(*n), PocketId::FootLeft)).collect(),
            fouls: if foul { vec![Foul::NoContact] } else { vec![] },
            spotted: vec![],
            group_assigned: None,
            continues,
            game_over: None,
            opponent_ball: false,
            penalty: vec![],
            break_again: false,
            called: None,
            choice: None,
        };
        let mut s = ShotStats::default();
        s.add(&report(&[1, 2], false, true), false);
        s.add(&report(&[3], false, true), false);
        s.add(&report(&[], false, false), false);
        s.add(&report(&[4], false, true), true);
        s.add(&report(&[5], true, false), false);
        assert_eq!((s.shots, s.potting_shots, s.balls, s.fouls), (5, 3, 4, 1));
        assert_eq!(s.best_run, 3);
        assert_eq!(s.pot_rate(), 60);
        assert_eq!(s.in_hand_rate(), 20);
    }

    #[test]
    fn a_replay_plays_the_stored_game_exactly() {
        use snook_legend_proto::replay::{Replay, Stroke};
        // A game played by the computer, recorded stroke by stroke.
        let mut replay = Replay::new(77, 0, snook_legend_core::Rules::Brazilian);
        let mut game = replay.start();
        for k in 0..6 {
            if game.game_over().is_some() {
                break;
            }
            let d = ai::decide(&game, Difficulty::Hard, k);
            let stroke = Stroke::Shot {
                shot: d.shot,
                called: d.called_pocket,
                cue_ball: d.cue_ball,
                timeout: false,
            };
            snook_legend_proto::replay::apply(&mut game, &stroke).unwrap();
            replay.strokes.push(stroke);
        }
        let info = snook_legend_proto::ReplayInfo {
            match_id: 1,
            players: ["a".into(), "b".into()],
            countries: Default::default(),
            ratings: [1200, 1200],
            winner: 0,
            rated: false,
            at: 0,
            replay: replay.clone(),
        };
        let mut m = Match::replay_of(&info, AimGuide::Full);
        assert!(m.is_replay());
        for _ in 0..20_000 {
            if m.script.as_ref().is_some_and(|s| s.is_empty()) && matches!(m.phase, Phase::GameOver)
            {
                break;
            }
            m.update(0.02);
        }
        assert_eq!(m.game, game, "the same table at the end");
    }

    fn players() -> [PlayerInfo; 2] {
        let p = |n: &str| PlayerInfo {
            username: n.into(),
            status: snook_legend_proto::PlayerStatus::Playing,
            ..Default::default()
        };
        [p("me"), p("them")]
    }

    fn online_match(seat: usize) -> Match {
        let game = EightBallGame::new(4, 0, snook_legend_core::Rules::Brazilian);
        Match::online(1, seat, &players(), false, game, 30, 30.0, None, AimGuide::Full, false)
    }

    /// Free practice: a shot taken back leaves the table as it was; never
    /// in a real game.
    #[test]
    fn free_practice_takes_shots_back() {
        let settings =
            MatchSettings::from_config(MatchMode::HotSeat, &crate::config::Config::default());
        let mut m = Match::new(&settings, 9);
        m.free_play = true;
        let before = m.game.clone();
        m.aim.power = 0.8;
        assert!(m.shoot());
        finish_animation(&mut m);
        assert_ne!(m.game, before);
        assert!(m.take_back());
        assert_eq!(m.game, before);
        assert!(!m.take_back(), "only the last shot");
        // A real game never takes shots back.
        let mut real = Match::new(&settings, 9);
        real.aim.power = 0.8;
        assert!(real.shoot());
        finish_animation(&mut real);
        assert!(!real.take_back());
    }

    /// Asked for the full guide (a setting, a stale value): an online game
    /// still draws the standard one; offline keeps it.
    #[test]
    fn the_full_guide_never_shows_online() {
        let mut m = online_match(0);
        m.guide = AimGuide::Full;
        assert_eq!(m.guide_in_use(), AimGuide::Standard);
        let game = EightBallGame::new(4, 0, snook_legend_core::Rules::Brazilian);
        let mut rated =
            Match::online(1, 0, &players(), true, game, 30, 30.0, None, AimGuide::Full, false);
        rated.guide = AimGuide::Full;
        assert_eq!(rated.guide_in_use(), AimGuide::Standard);
        let settings =
            MatchSettings::from_config(MatchMode::HotSeat, &crate::config::Config::default());
        let mut offline = Match::new(&settings, 1);
        offline.guide = AimGuide::Full;
        assert_eq!(offline.guide_in_use(), AimGuide::Full);
    }

    fn finish_animation(m: &mut Match) {
        for _ in 0..2000 {
            m.update(0.05);
            if !matches!(m.phase, Phase::Simulating(_)) {
                break;
            }
        }
    }

    #[test]
    fn opponent_winning_on_the_eight_is_not_a_resignation() {
        // The bug: the match ended while the deciding shot was still being
        // shown, and the game reported a resignation instead.
        let mut m = online_match(1);
        let shot = Shot::new(0.0, 6.0);
        m.handle_server(ServerMsg::ShotTaken {
            seat: 0,
            turn: 0,
            shot,
            called_pocket: None,
            cue_ball: None,
            timeout: false,
        });
        assert!(matches!(m.phase, Phase::Simulating(_)));
        let mut end = EightBallGame::new(4, 0, snook_legend_core::Rules::Brazilian);
        let report = end.play_shot(&shot, None).unwrap();
        // Stand-in for a game the 8-ball decided.
        end.conclude(0, GameOverReason::PocketedEight);
        m.handle_server(ServerMsg::ShotResult { turn: 0, end, report, clock_left: 30.0 });
        m.handle_server(ServerMsg::MatchEnded {
            result: MatchResult {
                winner: 0,
                reason: GameOverReason::PocketedEight,
                rated: true,
                rating_delta: [10, -10],
                new_ratings: [1210, 1190],
            },
        });
        assert!(m.game.game_over().is_none(), "still animating");
        finish_animation(&mut m);
        let over = m.game.game_over().expect("game over");
        assert_eq!(over.reason, GameOverReason::PocketedEight);
        assert!(!m.log.iter().any(|(l, _)| l.contains("resigned")), "{:?}", m.log);
    }

    #[test]
    fn own_shot_plays_at_once_and_adopts_the_server_result() {
        let mut m = online_match(0);
        m.aim.power = 0.8;
        m.aim.yaw = 0.0;
        assert!(m.shoot());
        assert!(matches!(m.phase, Phase::Simulating(_)), "no waiting for the server");
        let o = m.online.as_ref().unwrap();
        let Some(ClientMsg::Shoot { turn, shot, .. }) = o.outbox.last().cloned() else {
            panic!("shot not sent")
        };
        // The server echoes it (ignored) and sends the result.
        m.handle_server(ServerMsg::ShotTaken {
            seat: 0,
            turn,
            shot,
            called_pocket: None,
            cue_ball: None,
            timeout: false,
        });
        let mut server = EightBallGame::new(4, 0, snook_legend_core::Rules::Brazilian);
        let report = server.play_shot(&shot, None).unwrap();
        finish_animation(&mut m);
        assert!(matches!(m.phase, Phase::Settling(_)), "waits for the verdict");
        m.handle_server(ServerMsg::ShotResult {
            turn,
            end: server.clone(),
            report,
            clock_left: 30.0,
        });
        assert_eq!(m.game, server);
    }

    #[test]
    fn refused_shot_rolls_back() {
        let mut m = online_match(0);
        m.aim.power = 0.5;
        let before = m.game.clone();
        assert!(m.shoot());
        m.handle_server(ServerMsg::ShotRejected {
            turn: 0,
            reason: "It's not your turn.".into(),
            game: before.clone(),
        });
        assert!(matches!(m.phase, Phase::Turn { .. }));
        assert_eq!(m.game, before);
    }

    /// The server could not play the opponent's shot: the animation stops
    /// and the table goes back, instead of waiting for a result for ever.
    #[test]
    fn an_opponents_refused_shot_rolls_back_too() {
        let mut m = online_match(1);
        let before = m.game.clone();
        m.handle_server(ServerMsg::ShotTaken {
            seat: 0,
            turn: 0,
            shot: Shot::new(0.0, 6.0),
            called_pocket: None,
            cue_ball: None,
            timeout: false,
        });
        assert!(matches!(m.phase, Phase::Simulating(_)));
        m.handle_server(ServerMsg::ShotRejected {
            turn: 0,
            reason: "The server could not play that shot.".into(),
            game: before.clone(),
        });
        assert!(matches!(m.phase, Phase::Turn { .. }));
        assert_eq!(m.game, before);
    }

    /// A shot of ours that reached the server after its clock had played
    /// the turn: refused, and the clock's shot on screen plays on.
    #[test]
    fn a_shot_too_late_leaves_the_clocks_shot_alone() {
        let mut m = online_match(0);
        m.aim.power = 0.5;
        assert!(m.shoot());
        let before = m.game.clone();
        let late = Shot::new(0.0, 6.0);
        m.handle_server(ServerMsg::ShotTaken {
            seat: 0,
            turn: 0,
            shot: late,
            called_pocket: None,
            cue_ball: None,
            timeout: true,
        });
        assert!(matches!(m.phase, Phase::Simulating(_)));
        m.handle_server(ServerMsg::ShotRejected {
            turn: 0,
            reason: "It's not your turn.".into(),
            game: before,
        });
        assert!(matches!(m.phase, Phase::Simulating(_)), "the clock's shot goes on");
        let mut server = EightBallGame::new(4, 0, snook_legend_core::Rules::Brazilian);
        let report = server.play_shot(&late, None).unwrap();
        m.handle_server(ServerMsg::ShotResult {
            turn: 0,
            end: server.clone(),
            report,
            clock_left: 30.0,
        });
        finish_animation(&mut m);
        assert_eq!(m.game, server, "its result is taken");
    }

    /// The game ended (a resignation) while a shot was on its way, and the
    /// server sends no result for it: the end shows, no endless wait.
    #[test]
    fn a_game_that_ends_during_a_shot_ends_on_screen() {
        let mut m = online_match(1);
        m.handle_server(ServerMsg::ShotTaken {
            seat: 0,
            turn: 0,
            shot: Shot::new(0.0, 6.0),
            called_pocket: None,
            cue_ball: None,
            timeout: false,
        });
        m.handle_server(ServerMsg::MatchEnded {
            result: MatchResult {
                winner: 1,
                reason: GameOverReason::Resignation,
                rated: true,
                rating_delta: [-10, 10],
                new_ratings: [1190, 1210],
            },
        });
        finish_animation(&mut m);
        assert!(matches!(m.phase, Phase::GameOver), "not settling for ever");
        assert_eq!(m.wins, [0, 1]);

        // The same, with the balls already at rest when the end arrives.
        let mut m = online_match(1);
        m.handle_server(ServerMsg::ShotTaken {
            seat: 0,
            turn: 0,
            shot: Shot::new(0.0, 6.0),
            called_pocket: None,
            cue_ball: None,
            timeout: false,
        });
        finish_animation(&mut m);
        assert!(matches!(m.phase, Phase::Settling(_)));
        m.handle_server(ServerMsg::MatchEnded {
            result: MatchResult {
                winner: 1,
                reason: GameOverReason::Resignation,
                rated: true,
                rating_delta: [-10, 10],
                new_ratings: [1190, 1210],
            },
        });
        assert!(matches!(m.phase, Phase::GameOver));
        assert_eq!(m.wins, [0, 1]);
    }

    /// Balls made in a row: counted while the table is kept, cheered at
    /// three, and back to nothing when the turn passes.
    #[test]
    fn a_visit_counts_its_balls_in_a_row() {
        let mut m = online_match(0);
        let report = |made: &[u8], continues: bool| ShotReport {
            shooter: 0,
            was_break: false,
            // A solid hit first: the shooter's group.
            first_contact: Some(BallId(1)),
            pocketed: made.iter().map(|b| (BallId(*b), PocketId::FootLeft)).collect(),
            fouls: vec![],
            spotted: vec![],
            group_assigned: None,
            continues,
            game_over: None,
            opponent_ball: false,
            penalty: vec![],
            break_again: false,
            called: None,
            choice: None,
        };
        m.count_visit(&report(&[1, 2], true));
        assert_eq!(m.visit_run, (0, 2));
        m.count_visit(&report(&[3], true));
        assert_eq!(m.visit_run, (0, 3));
        assert!(m.notices.iter().any(|n| n.text.contains("3 in a row")), "cheered");
        m.count_visit(&report(&[], false));
        assert_eq!(m.visit_run, (0, 0), "a miss ends the run");
    }

    /// An offline game under `rules` between `controllers`.
    fn offline(rules: Rules, controllers: [Controller; 2]) -> Match {
        let settings = MatchSettings {
            mode: MatchMode::HotSeat,
            names: ["Ana".into(), "Bia".into()],
            guide: AimGuide::Standard,
            shot_clock: None,
            ladder: None,
            rules,
        };
        let mut m = Match::new(&settings, 3);
        m.controllers = controllers;
        m.begin_turn();
        m
    }

    /// Plays `shot` and lets the balls stop and the pause run out.
    fn play(m: &mut Match, shot: Shot) {
        m.forced_shot = Some(shot);
        assert!(m.shoot());
        for _ in 0..4000 {
            m.update(0.05);
            if matches!(m.phase, Phase::Turn { .. } | Phase::GameOver) {
                break;
            }
        }
    }

    /// The official rules: a soft break leaves the choice to the other
    /// player; nobody shoots until it is made, and it says what was chosen.
    #[test]
    fn the_official_break_choice_is_made_before_any_shot() {
        let mut m = offline(Rules::Official, [Controller::Human; 2]);
        let soft = Shot::new(m.game.default_aim(), 0.6);
        play(&mut m, soft);
        let choice = m.game.pending_choice().expect("an illegal break");
        assert_eq!(choice, BreakChoice::IllegalBreak { scratch: false });
        assert_eq!(m.game.current_player(), 1, "the incoming player decides");
        assert!(m.notices.iter().any(|n| n.text.contains("Illegal break")), "said");
        assert!(!m.shoot(), "no shot before the choice");
        m.choose(BreakOption::OpponentBreaks);
        assert!(m.game.pending_choice().is_none());
        assert!(m.game.is_break() && m.game.current_player() == 0, "Ana breaks again");
        assert!(m.notices.iter().any(|n| n.text.contains("Ana breaks")), "the choice is told");
    }

    /// The computer answers a choice by itself, after a moment.
    #[test]
    fn the_computer_makes_its_choice() {
        let cpu = Controller::Computer(Difficulty::Medium.player());
        let mut m = offline(Rules::Official, [Controller::Human, cpu]);
        let soft = Shot::new(m.game.default_aim(), 0.6);
        play(&mut m, soft);
        assert!(m.game.pending_choice().is_some());
        m.update(0.5);
        assert!(m.game.pending_choice().is_some(), "it reads the table first");
        for _ in 0..40 {
            m.update(0.1);
        }
        assert!(m.game.pending_choice().is_none(), "chosen");
    }

    /// A foul says, under it, what it gives the opponent under the rules of
    /// the game: the cue ball in hand anywhere (official), or the head spot
    /// and a ball (Brazilian).
    #[test]
    fn a_foul_says_what_it_gives_under_each_rules() {
        for rules in Rules::ALL {
            let mut m = offline(rules, [Controller::Human; 2]);
            // After the break, the groups decided (through a saved game).
            let mut v = serde_json::to_value(&m.game).unwrap();
            v["is_break"] = false.into();
            v["ball_in_hand"] = serde_json::Value::Null;
            v["groups"] = serde_json::json!(["Solids", "Stripes"]);
            m.game = serde_json::from_value(v).unwrap();
            m.begin_turn();
            // Away from every ball: no contact.
            let cue = m.game.ball(BallId::CUE).xy();
            let yaw = if cue.x < 0.0 { std::f64::consts::PI } else { 0.0 };
            play(&mut m, Shot::new(yaw, 0.8));
            let foul =
                m.notices.iter().find(|n| n.text.starts_with("Foul by Ana")).expect("a foul");
            let detail = foul.detail.clone().unwrap_or_default();
            match rules {
                Rules::Official => assert!(detail.contains("anywhere on the table"), "{detail}"),
                Rules::Brazilian => assert!(detail.contains("Penalty"), "{detail}"),
            }
        }
    }

    #[test]
    fn replay_does_not_touch_the_game() {
        let settings = MatchSettings {
            mode: MatchMode::HotSeat,
            names: ["a".into(), "b".into()],
            guide: AimGuide::Full,
            shot_clock: None,
            ladder: None,
            rules: snook_legend_core::Rules::Brazilian,
        };
        let mut m = Match::new(&settings, 3);
        m.aim.power = 0.9;
        assert!(m.shoot());
        for _ in 0..2000 {
            m.update(0.05);
            if matches!(m.phase, Phase::Turn { .. } | Phase::GameOver) {
                break;
            }
        }
        let after = m.game.clone();
        m.start_replay();
        assert!(m.replay.is_some());
        assert!(!m.is_human_turn(), "no aiming during a replay");
        for _ in 0..2000 {
            m.update(0.05);
            if m.replay.is_none() {
                break;
            }
        }
        assert_eq!(m.game, after);
    }
}

#[cfg(test)]
mod video_tests {
    use super::*;

    #[test]
    fn a_stored_game_can_be_watched_from_any_shot() {
        let info = Video::sample(7, 8);
        let n = info.replay.strokes.len();
        assert!(n >= 4, "a few shots were played");
        let mut m = Match::replay_of(&info, crate::config::AimGuide::Standard);
        let v = m.video.as_ref().expect("the game works out");
        assert_eq!((v.states.len(), v.stats.len(), v.marks.len()), (n + 1, n + 1, n));
        assert_eq!(m.video_position(), 1, "the first shot is next");
        // Straight to the fourth shot: the table as it was then.
        m.seek(3);
        assert_eq!(m.game, m.video.as_ref().unwrap().states[3]);
        assert_eq!(m.video_position(), 4);
        assert_eq!(m.timeline.len(), m.video.as_ref().unwrap().marks[..3].iter().flatten().count());
        // Back to the start, then past the end.
        m.seek(0);
        assert_eq!(m.game, info.replay.start());
        m.seek(n + 5);
        assert_eq!(m.video_position(), n);
        assert!(matches!(m.phase, Phase::GameOver));
        // Paused, the table doesn't move.
        m.seek(1);
        m.video.as_mut().unwrap().paused = true;
        let before = m.game.clone();
        for _ in 0..50 {
            m.update(0.1);
        }
        assert_eq!(m.game, before);
    }
}

#[cfg(test)]
mod ladder_tests {
    use super::*;
    use crate::opponents::Opponent;
    use snook_legend_proto::ladder::{ClaimFailure, PLAYER_SEAT, verify_against};

    /// A whole ladder game played through the match (the computer thinking
    /// on its thread, the animations), recorded exactly as the server will
    /// check it.
    #[test]
    fn a_ladder_game_is_recorded_as_the_server_checks_it() {
        let opponent = Opponent::personality("leo");
        let settings = MatchSettings {
            mode: MatchMode::VsComputer(opponent.clone()),
            names: ["Ana".into(), String::new()],
            guide: AimGuide::Standard,
            shot_clock: None,
            ladder: Some(9),
            rules: snook_legend_core::Rules::Brazilian,
        };
        let mut m = Match::new(&settings, 11);
        assert_eq!(m.names[1], "Leo");
        // The person is played by a steady computer player here.
        let me = Difficulty::Medium.player();
        for _ in 0..200_000 {
            if m.game.game_over().is_some() {
                break;
            }
            if m.is_human_turn() {
                let d = ai::decide_with(&m.game, &me, 77 + m.game.shots_taken() as u64);
                if let Some(p) = d.cue_ball {
                    m.game.place_cue_ball(p);
                }
                if d.called_pocket.is_some() {
                    m.called_pocket = d.called_pocket;
                }
                m.forced_shot = Some(d.shot);
                assert!(m.shoot());
            } else if matches!(m.phase, Phase::Turn { .. }) {
                // Give the computer's thread a moment.
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            m.update(0.1);
        }
        let over = m.game.game_over().expect("the game ends");
        let run = m.ladder.as_ref().expect("a ladder game");
        assert!(!run.record.strokes.is_empty());
        let verdict = verify_against(
            &run.record,
            &snook_legend_proto::ladder::tour_form(&opponent.player(), 9),
        );
        if over.winner == PLAYER_SEAT {
            assert_eq!(verdict.map(|g| g.game_over()), Ok(Some(over)));
        } else {
            assert_eq!(verdict.unwrap_err(), ClaimFailure::NotWon);
        }
        // A rematch starts a new record.
        m.rematch();
        let run = m.ladder.as_ref().unwrap();
        assert!(run.record.strokes.is_empty() && !run.reported);
        assert_eq!(run.record.seed, m.seed());
    }
}
