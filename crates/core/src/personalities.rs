//! The named computer opponents: each one a [`Skill`] and a [`Style`] of
//! its own, so they are told apart by how they play, not only by how well.
//!
//! Names are proper names (the same in every language); the epithet and
//! the description are English here and translated by the game. The codes
//! are stable: saved games, the ladder and the server refer to them.

use crate::ai::{AiPlayer, Search, Skill, Style};

/// A computer opponent with a name and a way of playing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Personality {
    /// Stable identifier.
    pub code: &'static str,
    /// A proper name, never translated.
    pub name: &'static str,
    /// What they are known as (English, translated by the game).
    pub epithet: &'static str,
    /// How they play, in a sentence or two (English, translated by the game).
    pub description: &'static str,
    /// How hard they are to beat, 1 to 5.
    pub strength: u8,
    pub player: AiPlayer,
}

const fn skill(
    aim_noise_deg: f64,
    speed_noise: f64,
    break_speed: f64,
    search: Search,
    position_weight: f64,
    plays_safe: bool,
    banks: bool,
) -> Skill {
    Skill { aim_noise_deg, speed_noise, break_speed, search, position_weight, plays_safe, banks }
}

const N: Style = Style::NEUTRAL;

/// Every named opponent, weakest first.
pub const PERSONALITIES: &[Personality] = &[
    Personality {
        code: "leo",
        name: "Leo",
        epithet: "The Rookie",
        description: "Still learning: aims at the nearest pocket, centre ball, and hopes. Never plays safe.",
        strength: 1,
        player: AiPlayer {
            skill: skill(1.7, 0.2, 5.5, Search::Narrow, 0.0, false, false),
            style: Style { spin: 0.0, power: 0.2, ..N },
        },
    },
    Personality {
        code: "nina",
        name: "Nina",
        epithet: "The Weekend Player",
        description: "Knows the basics and drops the easy ones, but rarely thinks about where the cue ball goes.",
        strength: 1,
        player: AiPlayer {
            skill: skill(1.15, 0.15, 5.9, Search::Narrow, 0.0, false, false),
            style: N,
        },
    },
    Personality {
        code: "tito",
        name: "Tito",
        epithet: "The Gambler",
        description: "Bets on every shot: as hard as the cue allows, with all the spin he can find. Safe play bores him.",
        strength: 1,
        player: AiPlayer {
            skill: skill(1.35, 0.22, 7.0, Search::Normal, 0.0, false, true),
            style: Style { safety_threshold: 150.0, power: 0.9, spin: 1.5, banks: 0.35, ..N },
        },
    },
    Personality {
        code: "lola",
        name: "Lola",
        epithet: "The Bank Artist",
        description: "Sees angles nobody else does: whenever a ball can go off a cushion and in, she banks it.",
        strength: 2,
        player: AiPlayer {
            skill: skill(0.95, 0.12, 6.0, Search::Normal, 100.0, true, true),
            style: Style { banks: 1.0, spin: 0.8, ..N },
        },
    },
    Personality {
        code: "bruno",
        name: "Bruno",
        epithet: "The Cannon",
        description: "Goes for everything, as hard as he can: long pots, thunderous breaks, hardly ever a safety.",
        strength: 2,
        player: AiPlayer {
            skill: skill(0.85, 0.14, 7.2, Search::Normal, 120.0, true, false),
            style: Style { safety_threshold: 250.0, safety_value: 0.4, power: 1.0, spin: 0.7, ..N },
        },
    },
    Personality {
        code: "ines",
        name: "Inês",
        epithet: "The Soft Touch",
        description: "Never hits harder than she must: the balls die at the pocket's edge and the cue ball stops where she wants it.",
        strength: 2,
        player: AiPlayer {
            skill: skill(0.9, 0.07, 5.8, Search::Normal, 180.0, true, false),
            style: Style { power: -1.0, position: 1.5, spin: 0.6, safety_threshold: 760.0, ..N },
        },
    },
    Personality {
        code: "marta",
        name: "Marta",
        epithet: "The Wall",
        description: "Only shoots when the pot is on. Otherwise she tucks the cue ball away and waits for your mistake.",
        strength: 3,
        player: AiPlayer {
            skill: skill(0.7, 0.08, 6.0, Search::Normal, 200.0, true, false),
            style: Style {
                safety_threshold: 1100.0,
                safety_value: 2.5,
                power: -0.6,
                spin: 0.8,
                ..N
            },
        },
    },
    Personality {
        code: "otto",
        name: "Otto",
        epithet: "The Old Pro",
        description: "Forty years at the table: a smooth stroke, a raised cue, side spin, and never in a hurry.",
        strength: 3,
        player: AiPlayer {
            skill: skill(0.6, 0.08, 6.4, Search::Normal, 260.0, true, true),
            style: Style {
                safety_threshold: 800.0,
                safety_value: 1.2,
                power: -0.2,
                spin: 1.25,
                elevation: 0.06,
                ..N
            },
        },
    },
    Personality {
        code: "diego",
        name: "Diego",
        epithet: "The Showman",
        description: "Plays to the crowd: a raised cue, swerving spin and a bank whenever there's one. It works more often than it should.",
        strength: 3,
        player: AiPlayer {
            skill: skill(0.65, 0.09, 6.8, Search::Normal, 220.0, true, true),
            style: Style { spin: 1.5, elevation: 0.13, banks: 0.6, power: 0.3, ..N },
        },
    },
    Personality {
        code: "rosa",
        name: "Rosa",
        epithet: "The Hustler",
        description: "Plays just well enough to win, then a little better. Balanced, sharp and hard to read.",
        strength: 4,
        player: AiPlayer {
            skill: skill(0.48, 0.06, 6.9, Search::Wide, 330.0, true, true),
            style: N,
        },
    },
    Personality {
        code: "kenji",
        name: "Kenji",
        epithet: "The Professor",
        description: "Thinks three balls ahead: every shot leaves the cue ball where the next one is easy.",
        strength: 4,
        player: AiPlayer {
            skill: skill(0.42, 0.05, 6.6, Search::Wide, 450.0, true, true),
            style: Style { position: 2.0, spin: 1.3, power: -0.3, ..N },
        },
    },
    Personality {
        code: "amara",
        name: "Amara",
        epithet: "The Tactician",
        description: "Plans the whole rack: snookers you when the pot isn't sure, and runs out when it is.",
        strength: 4,
        player: AiPlayer {
            skill: skill(0.45, 0.05, 6.6, Search::Wide, 420.0, true, true),
            style: Style {
                safety_threshold: 950.0,
                safety_value: 2.0,
                position: 1.6,
                power: -0.4,
                ..N
            },
        },
    },
    Personality {
        code: "sofia",
        name: "Sofia",
        epithet: "The Sharpshooter",
        description: "Pots from anywhere, at any angle. Her safeties are an afterthought: she doesn't need them.",
        strength: 5,
        player: AiPlayer {
            skill: skill(0.3, 0.05, 7.0, Search::Wide, 300.0, true, true),
            style: Style { safety_threshold: 450.0, safety_value: 0.7, power: 0.3, ..N },
        },
    },
    Personality {
        code: "yuri",
        name: "Yuri",
        epithet: "The Machine",
        description: "No nerves, no flair: centre ball, measured pace, the same perfect stroke every time.",
        strength: 5,
        player: AiPlayer {
            skill: skill(0.24, 0.02, 6.8, Search::Wide, 380.0, true, false),
            style: Style { spin: 0.35, power: -0.1, ..N },
        },
    },
    Personality {
        code: "viktor",
        name: "Viktor",
        epithet: "The Champion",
        description: "Unbeaten for years. Precise, patient and merciless: the last stage of the tour.",
        strength: 5,
        player: AiPlayer {
            skill: skill(0.2, 0.035, 7.2, Search::Wide, 520.0, true, true),
            style: Style { position: 1.2, safety_threshold: 750.0, safety_value: 1.2, ..N },
        },
    },
];

/// The opponent with this code.
pub fn personality(code: &str) -> Option<&'static Personality> {
    PERSONALITIES.iter().find(|p| p.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_consistent() {
        let mut codes: Vec<_> = PERSONALITIES.iter().map(|p| p.code).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), PERSONALITIES.len());
        for p in PERSONALITIES {
            assert!((1..=5).contains(&p.strength), "{}", p.code);
            assert!(p.code.chars().all(|c| c.is_ascii_lowercase()), "{}", p.code);
            assert!(p.description.len() < 140, "{}: keep it to a line or two", p.code);
            assert_eq!(p.player.style.clamped(), p.player.style, "{}", p.code);
            assert_eq!(p.player.skill.clamped(), p.player.skill, "{}", p.code);
            assert_eq!(personality(p.code), Some(p));
        }
        // Weakest first.
        assert!(PERSONALITIES.windows(2).all(|w| w[0].strength <= w[1].strength));
        assert!(personality("nobody").is_none());
    }

    /// Every opponent plays real, legal strokes on a fresh table: the break
    /// and the two strokes after it.
    #[test]
    fn every_opponent_plays_legal_strokes() {
        use crate::EightBallGame;
        for (k, p) in PERSONALITIES.iter().enumerate() {
            let mut g = EightBallGame::new(k as u64 + 1, 0, crate::Rules::Brazilian);
            for turn in 0..3u64 {
                if g.game_over().is_some() {
                    break;
                }
                let d = crate::ai::decide_with(&g, &p.player, turn * 31 + k as u64);
                if let Some(at) = d.cue_ball {
                    assert!(g.place_cue_ball(at), "{}: a cue ball placement it can't make", p.code);
                }
                g.play_shot(&d.shot, d.called_pocket)
                    .unwrap_or_else(|e| panic!("{}: a stroke the rules refuse: {e}", p.code));
            }
            assert!(g.shots_taken() > 0, "{}", p.code);
        }
    }
}
