//! The server checks games won against the computer (the ladder's reward)
//! by asking the AI for every one of its strokes again: a decision must be
//! bit-identical on the player's computer and on the server, whatever the
//! platform. So the AI, like the physics and the rules it plays with, may
//! only use the portable maths of `snook_legend_core::math`, never the
//! platform's own `sin`, `cos`, `atan2`, `ln`…, and CI pins its decisions
//! to one fingerprint per ruleset on Linux, Windows and macOS.

use glam::DVec2;
use snook_legend_core::ai::{AiPlayer, decide_with};
use snook_legend_core::personalities::personality;
use snook_legend_core::{BallId, EightBallGame};

/// Calls that come from the platform's maths library (or that use it) and
/// may round differently from one system to another.
const NOT_PORTABLE: &[&str] = &[
    ".sin(",
    ".cos(",
    ".tan(",
    ".sin_cos(",
    ".asin(",
    ".acos(",
    ".atan(",
    ".atan2(",
    ".sinh(",
    ".cosh(",
    ".tanh(",
    ".exp(",
    ".exp2(",
    ".exp_m1(",
    ".ln(",
    ".log(",
    ".log2(",
    ".log10(",
    ".ln_1p(",
    ".powf(",
    ".powi(",
    ".cbrt(",
    ".hypot(",
    ".mul_add(",
    "from_angle(",
    "angle_between(",
    "angle_to(",
];

/// The code of a source file without comments and without its tests (the
/// `mod tests` at its end: a single item kept for the tests, like the one in
/// the physics, is scanned with the rest).
fn code_of(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let tests = lines
        .windows(2)
        .position(|w| w[0].trim() == "#[cfg(test)]" && w[1].trim_start().starts_with("mod tests"))
        .unwrap_or(lines.len());
    lines[..tests]
        .iter()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The AI plays every candidate stroke with the physics and the rules
/// before it decides, so they are held to the same standard.
#[test]
fn the_ai_uses_only_portable_maths() {
    let sources = [
        ("ai.rs", include_str!("../src/ai.rs")),
        ("personalities.rs", include_str!("../src/personalities.rs")),
        ("rng.rs", include_str!("../src/rng.rs")),
        ("physics.rs", include_str!("../src/physics.rs")),
        ("rules.rs", include_str!("../src/rules.rs")),
        ("table.rs", include_str!("../src/table.rs")),
    ];
    for (name, source) in sources {
        let code = code_of(source);
        // `math::from_angle` is the portable one.
        let code = code.replace("math::from_angle(", "math::portable_angle(");
        for call in NOT_PORTABLE {
            assert!(
                !code.contains(call),
                "{name} calls `{call}`: use snook_legend_core::math so every platform agrees"
            );
        }
    }
}

struct Fnv(u64);

impl Fnv {
    fn add(&mut self, v: u64) {
        for byte in v.to_le_bytes() {
            self.0 ^= byte as u64;
            self.0 = self.0.wrapping_mul(0x100_0000_01b3);
        }
    }
    fn f(&mut self, v: f64) {
        self.add(v.to_bits());
    }
}

/// Decisions of several computer players on a few tables, hashed bit by bit.
fn fingerprint(ruleset: u8) -> u64 {
    let mut h = Fnv(0xcbf2_9ce4_8422_2325);
    let players: Vec<AiPlayer> = ["leo", "lola", "bruno", "marta", "otto", "viktor"]
        .iter()
        .map(|c| personality(c).expect("exists").player)
        .collect();
    for seed in 0..3u64 {
        let mut g = EightBallGame::with_ruleset(seed, 0, ruleset);
        // The break (with the cue ball in hand behind the head string).
        let d = decide_with(&g, &players[(seed % 2) as usize], seed);
        if let Some(p) = d.cue_ball {
            g.place_cue_ball(p);
        }
        g.play_shot(&d.shot, d.called_pocket).expect("legal");
        if g.game_over().is_some() {
            continue;
        }
        // Then each player decides on the same table.
        for (k, p) in players.iter().enumerate() {
            let d = decide_with(&g, p, seed * 100 + k as u64);
            h.f(d.shot.yaw);
            h.f(d.shot.speed);
            h.f(d.shot.elevation);
            h.f(d.shot.tip.x);
            h.f(d.shot.tip.y);
            let c = d.cue_ball.unwrap_or(DVec2::splat(-9.0));
            h.f(c.x);
            h.f(c.y);
            h.add(d.called_pocket.map_or(99, |p| p.index() as u64));
        }
        h.f(g.ball(BallId::CUE).position.x);
    }
    h.0
}

#[test]
fn decisions_are_bit_identical_everywhere() {
    for (ruleset, expected) in EXPECTED {
        let got = fingerprint(ruleset);
        println!("ruleset {ruleset}: ai fingerprint {got:#x}");
        assert_eq!(
            got, expected,
            "the computer decided differently on this platform (ruleset {ruleset})"
        );
    }
}

/// Update only on purpose (a change to the physics, the rules or the AI of
/// the current ruleset), from the values printed above. An older ruleset's
/// value never changes: games recorded under it must replay the same (the
/// server checks the ladder's final game under the ruleset it was played
/// with).
const EXPECTED: [(u8, u64); 5] = [
    (1, 0x5f9e_be78_d9d8_361f),
    (2, 0x7f8d_52de_242c_eb40),
    (3, 0x9234_f79e_23ac_772c),
    (4, 0x0dc7_88cb_89fb_6a39),
    // Not the default yet: may still change, on purpose, until it is.
    (5, 0xe866_92d6_0300_b481),
];
