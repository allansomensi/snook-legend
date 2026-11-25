//! The feats behind the secret achievements: what a player did in one game,
//! read from its replay once it ends (the server replays every rated game
//! anyway, for the shot analysis). Shared so that a test can play a game
//! and see what it earns.
//!
//! A feat of skill counts whatever the result (three balls with one shot is
//! three balls with one shot); those about turning a game round need the
//! win, on the table (not a resignation nor a game left).

use snook_legend_core::{BallId, GameOverReason, Group, PocketId};

use crate::replay::{Replay, apply};

/// Six balls behind: the opponent had made six more of their group.
pub const COMEBACK_BEHIND: usize = 6;
/// Balls of one's own group still on the table when the opponent was on
/// the 8, for a last stand.
pub const LAST_STAND_LEFT: usize = 5;
/// Shots (both players') in a marathon.
pub const MARATHON_SHOTS: usize = 60;
/// Fouls of the winner in an ugly win.
pub const UGLY_FOULS: usize = 4;
/// Balls made on the break for a thunder break.
pub const THUNDER_BREAK: usize = 5;

/// The secret achievements `seat` earned in the game `replay` (codes of
/// [`crate::ACHIEVEMENTS`]). Errors when the replay can't be played back.
pub fn detect(replay: &Replay, seat: usize) -> Result<Vec<&'static str>, String> {
    let seat = seat % 2;
    let other = 1 - seat;
    let mut game = replay.start();
    let mut feats: Vec<&'static str> = Vec::new();
    let add = |code: &'static str, feats: &mut Vec<&'static str>| {
        if !feats.contains(&code) {
            feats.push(code);
        }
    };
    let (mut shots, mut fouls) = (0usize, 0usize);
    let (mut behind, mut stand) = (false, false);
    // The opponent broke, and has not shot since.
    let mut stolen = replay.breaker % 2 == other;
    let mut first = true;
    for stroke in &replay.strokes {
        let Some(report) = apply(&mut game, stroke)? else { continue };
        shots += 1;
        if report.shooter == other && !first {
            stolen = false;
        }
        first = false;
        if report.shooter == seat {
            if report.is_foul() {
                fouls += 1;
            }
            let objects: Vec<(BallId, PocketId)> =
                report.pocketed.iter().copied().filter(|(b, _)| b.is_object()).collect();
            if report.was_break {
                if objects.len() >= THUNDER_BREAK {
                    add("secret_thunder_break", &mut feats);
                }
                if replay.rules == snook_legend_core::Rules::Official
                    && objects.iter().any(|(b, _)| *b == BallId::EIGHT)
                {
                    add("secret_spotted_eight", &mut feats);
                }
            } else if !report.is_foul() {
                if report.balls_made().iter().filter(|b| **b != BallId::EIGHT).count() >= 3 {
                    add("secret_three_in_one", &mut feats);
                }
                let mut pockets: Vec<PocketId> = objects.iter().map(|(_, p)| *p).collect();
                pockets.sort();
                pockets.dedup();
                if pockets.len() >= 4 {
                    add("secret_four_corners", &mut feats);
                }
            }
        }
        if let (Some(mine), Some(theirs)) = (game.group_of(seat), game.group_of(other))
            && game.game_over().is_none()
        {
            let (left, their_left) = (left(&game, mine), left(&game, theirs));
            behind |= left >= their_left + COMEBACK_BEHIND;
            stand |= their_left == 0 && left >= LAST_STAND_LEFT;
        }
    }
    let won = game.game_over().is_some_and(|o| {
        o.winner == seat
            && !matches!(o.reason, GameOverReason::Resignation | GameOverReason::Abandoned)
    });
    if won {
        if behind {
            add("secret_comeback", &mut feats);
        }
        if stand {
            add("secret_last_stand", &mut feats);
        }
        if stolen {
            add("secret_steal", &mut feats);
        }
        if shots >= MARATHON_SHOTS {
            add("secret_marathon", &mut feats);
        }
        if fouls >= UGLY_FOULS {
            add("secret_ugly", &mut feats);
        }
    }
    Ok(feats)
}

fn left(game: &snook_legend_core::EightBallGame, group: Group) -> usize {
    game.remaining(group).len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::Stroke;
    use snook_legend_core::{Rules, Shot};

    /// A whole game played by a simple, deterministic player: it aims at the
    /// first legal ball, a little off at times.
    fn play(seed: u64, rules: Rules) -> Replay {
        let mut replay = Replay::new(seed, 0, rules);
        let mut game = replay.start();
        for k in 0..300 {
            if game.game_over().is_some() {
                break;
            }
            if let Some(choice) = game.pending_choice() {
                let option = choice.options()[0];
                let stroke = Stroke::Choice { option };
                apply(&mut game, &stroke).unwrap();
                replay.strokes.push(stroke);
                continue;
            }
            let target = game.legal_targets().first().copied().unwrap_or(BallId::EIGHT);
            let cue = game.ball(BallId::CUE).xy();
            let to = game.ball(target).xy() - cue;
            let yaw = to.y.atan2(to.x) + ((k * 7919 % 13) as f64 - 6.0) * 0.004;
            let called = game.needs_called_pocket().then(|| game.nearest_pocket(target));
            let stroke =
                Stroke::Shot { shot: Shot::new(yaw, 2.5), called, cue_ball: None, timeout: false };
            if apply(&mut game, &stroke).is_err() {
                break;
            }
            replay.strokes.push(stroke);
        }
        replay
    }

    #[test]
    fn games_are_read_without_error_and_feats_are_rare() {
        let mut found = 0;
        for seed in 0..6 {
            for rules in [Rules::Brazilian, Rules::Official] {
                let replay = play(seed, rules);
                for seat in 0..2 {
                    let feats = detect(&replay, seat).unwrap();
                    found += feats.len();
                    for f in feats {
                        let a = crate::ACHIEVEMENTS.iter().find(|a| a.code == f).unwrap();
                        assert!(a.secret, "{f}");
                    }
                }
            }
        }
        // A clumsy player earns next to nothing.
        assert!(found <= 12, "{found} feats");
    }

    #[test]
    fn a_resigned_game_wins_no_feat_of_winning() {
        let mut replay = Replay::new(3, 1, Rules::Brazilian);
        replay.strokes.push(Stroke::Resign { seat: 1 });
        let feats = detect(&replay, 0).unwrap();
        assert!(!feats.contains(&"secret_steal"), "{feats:?}");
        assert!(feats.is_empty());
    }
}
