//! The ladder (the computer tour): fifteen computer opponents of
//! increasing strength, beaten one after the other. Beating the last one,
//! once all the others are beaten, earns an item sold nowhere else.
//!
//! The ladder is played offline, so the game keeps the progress; the reward
//! is the server's to give. The game sends the recorded final game and the
//! server plays it again ([`verify`]): it must be a legal, complete game the
//! player won, and every stroke of the computer must be exactly what
//! [`snook_legend_core::ai::decide_with`] plays on that table with the seed of
//! [`ai_seed`]. The computer's choices are deterministic on every platform,
//! so a genuine game always passes and an edited one never does.

use serde::{Deserialize, Serialize};
use snook_legend_core::ai::{self, AiPlayer};
use snook_legend_core::personalities::{self, Personality};
use snook_legend_core::{BallId, EightBallGame};

use crate::replay::{Replay, Stroke, apply};

/// The opponents, from the first rung to the last (personality codes).
pub const RUNGS: [&str; 15] = [
    "leo", "nina", "tito", "lola", "bruno", "ines", "marta", "otto", "diego", "rosa", "kenji",
    "amara", "sofia", "yuri", "viktor",
];

/// The ladder before it grew to fifteen (a saved progress from then counts
/// rungs of this list).
pub const RUNGS_OF_TEN: [&str; 10] =
    ["leo", "nina", "lola", "bruno", "marta", "otto", "rosa", "kenji", "sofia", "viktor"];

/// The shop item for beating the whole ladder.
pub const REWARD_ITEM: &str = "cue_ladder";

/// The achievement for beating the whole ladder (it comes with the item).
pub const ACHIEVEMENT: &str = "ladder_champion";

/// The player's seat in a game against the computer (the computer has the
/// other one).
pub const PLAYER_SEAT: usize = 0;

/// More strokes than any real game has: longer claims are refused unplayed.
pub const MAX_STROKES: usize = 400;

/// The opponent on rung `k` (0 is the first).
pub fn rung(k: usize) -> Option<&'static Personality> {
    RUNGS.get(k).and_then(|code| personalities::personality(code))
}

/// The last opponent of the ladder.
pub fn final_opponent() -> &'static Personality {
    rung(RUNGS.len() - 1).expect("the ladder's opponents exist")
}

/// How the opponent of rung `k` plays on the tour: themselves at their
/// best. Winning the tour is meant to be hard, so each of them aims and
/// paces their strokes much more tightly than in a free game, looks at
/// more shots, thinks further about where the cue ball goes, and all but
/// the first two play safe when nothing is on. The style stays their own.
/// The server checks the final game against this very player.
pub fn tour_player(k: usize) -> Option<AiPlayer> {
    Some(tour_form(&rung(k)?.player, k))
}

/// `player` as they play on rung `k` of the tour (see [`tour_player`]).
pub fn tour_form(player: &AiPlayer, k: usize) -> AiPlayer {
    use snook_legend_core::ai::Search;
    let mut skill = player.skill;
    skill.aim_noise_deg *= 0.6;
    skill.speed_noise *= 0.6;
    skill.search = match skill.search {
        Search::Narrow => Search::Normal,
        _ => Search::Wide,
    };
    skill.position_weight = skill.position_weight * 1.3 + 80.0;
    skill.plays_safe |= k >= 2;
    skill.banks |= k >= 6;
    AiPlayer { skill: skill.clamped(), ..*player }
}

/// The tour's last opponent, as they play on it.
pub fn final_player() -> AiPlayer {
    tour_player(RUNGS.len() - 1).expect("the ladder's opponents exist")
}

/// The seed of the computer's decision on a table: the game's rack seed and
/// the number of shots taken so far. The game uses it for every game
/// against the computer, so a game can be replayed (and checked) exactly.
pub fn ai_seed(rack_seed: u64, shots_taken: u32) -> u64 {
    rack_seed ^ ((shots_taken as u64) << 32)
}

/// The stroke the computer plays on this table, as the game records it: the
/// shot, the called pocket and, with the cue ball in hand, where it stands.
pub fn computer_stroke(game: &EightBallGame, player: &AiPlayer, rack_seed: u64) -> Stroke {
    let d = ai::decide_with(game, player, ai_seed(rack_seed, game.shots_taken()));
    let cue_ball =
        game.ball_in_hand().map(|_| d.cue_ball.unwrap_or_else(|| game.ball(BallId::CUE).xy()));
    Stroke::Shot { shot: d.shot, called: d.called_pocket, cue_ball, timeout: false }
}

/// Why a claim for the ladder's reward was not granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimFailure {
    /// More strokes than a game can have.
    TooLong,
    /// A stroke the rules refuse (or one the player can't make).
    Illegal,
    /// The game doesn't end.
    NotFinished,
    /// The player didn't win it.
    NotWon,
    /// A stroke of the computer is not what it plays on that table.
    ComputerMismatch,
    /// The server is checking other games: try again in a moment.
    Busy,
    /// Claimed too often: try again in a minute.
    TooSoon,
    /// Something went wrong on the server.
    ServerError,
    /// A reason this version of the game doesn't know.
    #[serde(other)]
    Unknown,
}

/// The server's answer to a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ClaimResult {
    /// The item is this account's (`already`: it was before this claim).
    Granted {
        already: bool,
    },
    Failed {
        reason: ClaimFailure,
    },
}

/// Bitwise equality of two strokes (a computer stroke must be exactly the
/// one it plays, down to the last bit of every number).
fn same(a: &Stroke, b: &Stroke) -> bool {
    match (a, b) {
        (
            Stroke::Shot { shot: s, called: c, cue_ball: p, timeout: t },
            Stroke::Shot { shot: s2, called: c2, cue_ball: p2, timeout: t2 },
        ) => {
            let bits = |v: f64| v.to_bits();
            bits(s.yaw) == bits(s2.yaw)
                && bits(s.elevation) == bits(s2.elevation)
                && bits(s.speed) == bits(s2.speed)
                && bits(s.tip.x) == bits(s2.tip.x)
                && bits(s.tip.y) == bits(s2.tip.y)
                && c == c2
                && t == t2
                && match (p, p2) {
                    (Some(p), Some(p2)) => bits(p.x) == bits(p2.x) && bits(p.y) == bits(p2.y),
                    (None, None) => true,
                    _ => false,
                }
        }
        _ => false,
    }
}

/// Plays a recorded game of the ladder's final rung again and checks it:
/// legal, complete, won by the player, and every computer stroke exactly
/// what the final opponent plays. Returns the final table.
///
/// This simulates the whole game and asks the computer for every one of
/// its strokes: seconds of CPU time. Run it off the async runtime.
pub fn verify(replay: &Replay) -> Result<EightBallGame, ClaimFailure> {
    verify_against(replay, &final_player())
}

/// [`verify`] against any computer player.
pub fn verify_against(replay: &Replay, opponent: &AiPlayer) -> Result<EightBallGame, ClaimFailure> {
    if replay.strokes.len() > MAX_STROKES {
        return Err(ClaimFailure::TooLong);
    }
    let mut game = replay.start();
    for stroke in &replay.strokes {
        if game.game_over().is_some() {
            // Strokes after the end.
            return Err(ClaimFailure::Illegal);
        }
        if game.current_player() == PLAYER_SEAT {
            // The player shoots or runs out of time; resigning loses.
            match stroke {
                Stroke::Shot { timeout: false, .. } | Stroke::TimeFoul => {}
                _ => return Err(ClaimFailure::Illegal),
            }
        } else {
            let expected = computer_stroke(&game, opponent, replay.seed);
            if !same(stroke, &expected) {
                return Err(ClaimFailure::ComputerMismatch);
            }
        }
        apply(&mut game, stroke).map_err(|_| ClaimFailure::Illegal)?;
    }
    match game.game_over() {
        None => Err(ClaimFailure::NotFinished),
        Some(over) if over.winner != PLAYER_SEAT => Err(ClaimFailure::NotWon),
        Some(_) => Ok(game),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snook_legend_core::Difficulty;

    /// A game the player (played by a strong computer here) wins against
    /// `opponent`, recorded as the game records it.
    fn won_game(opponent: &AiPlayer, mut seed: u64) -> Replay {
        let me = Difficulty::Hard.player();
        loop {
            let mut replay = Replay::new(seed, 0);
            let mut game = replay.start();
            while game.game_over().is_none() && replay.strokes.len() < 200 {
                let stroke = if game.current_player() == PLAYER_SEAT {
                    computer_stroke(&game, &me, seed ^ 0x5555)
                } else {
                    computer_stroke(&game, opponent, replay.seed)
                };
                apply(&mut game, &stroke).unwrap();
                replay.strokes.push(stroke);
            }
            if game.game_over().is_some_and(|o| o.winner == PLAYER_SEAT) {
                return replay;
            }
            seed += 1;
        }
    }

    #[test]
    fn the_ladder_is_fifteen_personalities_getting_stronger() {
        let rungs: Vec<_> = (0..RUNGS.len()).map(|k| rung(k).expect("exists")).collect();
        assert_eq!(rungs.len(), 15);
        assert_eq!(rungs.len(), personalities::PERSONALITIES.len(), "every opponent is on it");
        assert!(rungs.windows(2).all(|w| w[0].strength <= w[1].strength));
        assert_eq!(final_opponent().code, "viktor");
        assert!(rung(15).is_none());
        // The ladder of ten, in its order, inside the new one.
        let old: Vec<_> = RUNGS.iter().filter(|c| RUNGS_OF_TEN.contains(c)).collect();
        assert!(old.iter().zip(RUNGS_OF_TEN.iter()).all(|(a, b)| *a == b));
        assert_ne!(ai_seed(7, 1), ai_seed(7, 2));
    }

    #[test]
    fn genuine_games_pass_and_edited_ones_fail() {
        // A weak opponent keeps the test quick; the check is the same.
        let weak = Difficulty::Easy.player();
        let replay = won_game(&weak, 3);
        assert!(verify_against(&replay, &weak).is_ok());

        // The same game, unfinished.
        let mut cut = replay.clone();
        cut.strokes.pop();
        assert_eq!(verify_against(&cut, &weak).unwrap_err(), ClaimFailure::NotFinished);

        // A computer stroke nudged by the smallest amount.
        let mut edited = replay.clone();
        let k = (0..edited.strokes.len())
            .find(|&k| {
                let (g, _) = replay.play(Some(k)).unwrap();
                g.current_player() != PLAYER_SEAT
            })
            .expect("the computer played");
        if let Stroke::Shot { shot, .. } = &mut edited.strokes[k] {
            shot.yaw = f64::from_bits(shot.yaw.to_bits() + 1);
        }
        assert_eq!(verify_against(&edited, &weak).unwrap_err(), ClaimFailure::ComputerMismatch);

        // Against another opponent than the one it was played against.
        assert_eq!(
            verify_against(&replay, &Difficulty::Hard.player()).unwrap_err(),
            ClaimFailure::ComputerMismatch
        );

        // A resignation in the player's place, and strokes after the end.
        let mut resigned = cut.clone();
        resigned.strokes.push(Stroke::Resign { seat: PLAYER_SEAT });
        assert!(verify_against(&resigned, &weak).is_err());
        let mut extra = replay.clone();
        extra.strokes.push(Stroke::TimeFoul);
        assert_eq!(verify_against(&extra, &weak).unwrap_err(), ClaimFailure::Illegal);

        // Far too long.
        let mut long = Replay::new(1, 0);
        long.strokes = vec![Stroke::TimeFoul; MAX_STROKES + 1];
        assert_eq!(verify_against(&long, &weak).unwrap_err(), ClaimFailure::TooLong);
    }

    #[test]
    fn a_lost_game_is_not_a_win() {
        let weak = Difficulty::Easy.player();
        let lost = (20..40)
            .find_map(|seed| won_game_by_computer(&weak, seed))
            .expect("some game the computer wins");
        assert_eq!(verify_against(&lost, &weak).unwrap_err(), ClaimFailure::NotWon);
    }

    fn won_game_by_computer(opponent: &AiPlayer, seed: u64) -> Option<Replay> {
        // A clumsy player: soft shots straight up the table.
        let mut replay = Replay::new(seed, 1);
        let mut game = replay.start();
        while game.game_over().is_none() && replay.strokes.len() < 300 {
            let stroke = if game.current_player() == PLAYER_SEAT {
                if game.ball_in_hand().is_some() {
                    Stroke::TimeFoul
                } else {
                    let called =
                        game.needs_called_pocket().then_some(snook_legend_core::PocketId::FootLeft);
                    Stroke::Shot {
                        shot: snook_legend_core::Shot::new(1.0, 0.6),
                        called,
                        cue_ball: None,
                        timeout: false,
                    }
                }
            } else {
                computer_stroke(&game, opponent, replay.seed)
            };
            apply(&mut game, &stroke).ok()?;
            replay.strokes.push(stroke);
        }
        game.game_over().filter(|o| o.winner != PLAYER_SEAT).map(|_| replay)
    }
}
