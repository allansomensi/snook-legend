//! The account level: experience earned by playing, whatever the result,
//! so a player feels progress even on a losing day. Separate from the
//! rating (which says how well someone plays, and is what others see: the
//! league). The level is the player's own road: every level pays a few
//! coins and some levels bring an item that can be had no other way (see
//! [`MILESTONES`]). The road is long on purpose: its rewards are rare.

/// XP for an online game played to the end, won or lost.
pub const WIN: u32 = 60;
pub const LOSS: u32 = 30;
/// And for each object ball made in it.
pub const PER_BALL: u32 = 2;
/// An attempt at the daily challenge, and each ball made in it.
pub const DAILY_ATTEMPT: u32 = 10;
pub const DAILY_PER_BALL: u32 = 5;
/// Each new star in a drill.
pub const DRILL_STAR: u32 = 40;
/// An achievement unlocked, by its medal (bronze to ruby): the easy ones
/// are worth little, the hard ones a good deal.
pub const ACHIEVEMENT: [u32; 5] = [25, 50, 100, 150, 250];

/// The XP an achievement brings (none for a code it doesn't know).
pub fn for_achievement(code: &str) -> u32 {
    crate::ACHIEVEMENTS
        .iter()
        .find(|a| a.code == code)
        .map_or(0, |a| ACHIEVEMENT[(a.level as usize).min(ACHIEVEMENT.len() - 1)])
}
/// The highest level.
pub const MAX_LEVEL: u32 = 100;

/// Coins for every level reached.
pub const LEVEL_COINS: u32 = 20;

/// The levels that bring an item of their own, and the achievement that
/// carries it (the shop's items priced by these achievements).
pub const MILESTONES: [(u32, &str); 7] = [
    (5, "level_5"),
    (10, "level_10"),
    (15, "level_15"),
    (20, "level_20"),
    (30, "level_30"),
    (40, "level_40"),
    (50, "level_50"),
];

/// The achievements of the milestones a player of `level` has passed.
pub fn milestones_reached(level: u32) -> impl Iterator<Item = &'static str> {
    MILESTONES.iter().filter(move |(l, _)| *l <= level).map(|(_, code)| *code)
}

/// The next level that brings an item, after `level`.
pub fn next_milestone(level: u32) -> Option<(u32, &'static str)> {
    MILESTONES.iter().copied().find(|(l, _)| *l > level)
}

/// XP from a level to the next: a good deal more each time. Level 5 is
/// some thirty games away, level 10 over a hundred, level 20 four hundred,
/// level 50 a couple of thousand.
pub fn step(level: u32) -> u64 {
    250 + 120 * (level.max(1) as u64 - 1)
}

/// The XP needed to reach `level` (level 1 needs none).
pub fn to_reach(level: u32) -> u64 {
    let n = level.clamp(1, MAX_LEVEL) as u64 - 1;
    250 * n + 120 * n * n.saturating_sub(1) / 2
}

/// The level of `xp`.
pub fn level_of(xp: i64) -> u32 {
    let xp = xp.max(0) as u64;
    let mut level = 1;
    while level < MAX_LEVEL && to_reach(level + 1) <= xp {
        level += 1;
    }
    level
}

/// The level of `xp`, the XP made into it and what it takes to the next
/// (0 at the top level).
pub fn progress(xp: i64) -> (u32, u64, u64) {
    let level = level_of(xp);
    let into = xp.max(0) as u64 - to_reach(level);
    let span = if level >= MAX_LEVEL { 0 } else { step(level) };
    (level, into, span)
}

/// XP for an online game: the result and the balls made.
pub fn for_game(won: bool, balls: u32) -> u32 {
    (if won { WIN } else { LOSS }) + PER_BALL * balls.min(8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_climb_slower_and_slower() {
        assert_eq!(level_of(0), 1);
        assert_eq!(level_of(-5), 1);
        assert_eq!(to_reach(2), 250);
        assert_eq!(to_reach(3), 620);
        assert_eq!(level_of(249), 1);
        assert_eq!(level_of(250), 2);
        assert_eq!(level_of(619), 2);
        assert_eq!(level_of(620), 3);
        assert_eq!(progress(630), (3, 10, 490));
        for l in 1..MAX_LEVEL {
            assert_eq!(to_reach(l + 1) - to_reach(l), step(l));
            assert_eq!(level_of(to_reach(l) as i64), l);
        }
        assert_eq!(level_of(i64::MAX), MAX_LEVEL);
        assert_eq!(progress(i64::MAX).2, 0);
        // A game a day for a year is somewhere in the teens: the road is
        // long.
        assert!((12..20).contains(&level_of(365 * 50)));
        // The milestones, in order.
        assert_eq!(milestones_reached(4).count(), 0);
        assert_eq!(milestones_reached(15).collect::<Vec<_>>(), ["level_5", "level_10", "level_15"]);
        assert_eq!(next_milestone(1), Some((5, "level_5")));
        assert_eq!(next_milestone(20), Some((30, "level_30")));
        assert_eq!(next_milestone(50), None);
        assert_eq!(for_game(true, 3), 66);
        assert_eq!(for_game(false, 20), 46);
    }
}
