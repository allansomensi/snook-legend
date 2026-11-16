//! The account level: experience earned by playing, whatever the result,
//! so a player feels progress even on a losing day. Separate from the
//! rating (which says how well someone plays).

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
/// An achievement unlocked.
pub const ACHIEVEMENT: u32 = 100;
/// The highest level.
pub const MAX_LEVEL: u32 = 100;

/// XP from a level to the next: a little more each time.
pub fn step(level: u32) -> u64 {
    100 + 40 * (level.max(1) as u64 - 1)
}

/// The XP needed to reach `level` (level 1 needs none).
pub fn to_reach(level: u32) -> u64 {
    let n = level.clamp(1, MAX_LEVEL) as u64 - 1;
    100 * n + 40 * n * n.saturating_sub(1) / 2
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
        assert_eq!(to_reach(2), 100);
        assert_eq!(to_reach(3), 240);
        assert_eq!(level_of(99), 1);
        assert_eq!(level_of(100), 2);
        assert_eq!(level_of(239), 2);
        assert_eq!(level_of(240), 3);
        assert_eq!(progress(250), (3, 10, 180));
        for l in 1..MAX_LEVEL {
            assert_eq!(to_reach(l + 1) - to_reach(l), step(l));
            assert_eq!(level_of(to_reach(l) as i64), l);
        }
        assert_eq!(level_of(i64::MAX), MAX_LEVEL);
        assert_eq!(progress(i64::MAX).2, 0);
        // A game a day for a year is somewhere in the thirties.
        assert!((25..45).contains(&level_of(365 * 50)));
        assert_eq!(for_game(true, 3), 66);
        assert_eq!(for_game(false, 20), 46);
    }
}
