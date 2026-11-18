//! Tournaments against the computer: the player and the fifteen named
//! opponents, sixteen in all, in a knockout (four rounds, lose and you are
//! out) or an arena (five rounds for everyone, on points). The player plays
//! their own games at the table; the games between the others are decided
//! by their strength and a draw. Kept in a file next to the settings, so a
//! tournament goes on after the game is closed.

use serde::{Deserialize, Serialize};
use snook_legend_core::personalities::{self, PERSONALITIES};
use snook_legend_core::rng::Rng;

/// Bumped when the file changes incompatibly (an older file is ignored).
const VERSION: u32 = 1;

/// Who plays: the player and the fifteen opponents.
pub const ENTRANTS: usize = 16;
/// Rounds of an arena.
pub const ARENA_ROUNDS: usize = 5;
/// Points for a win in an arena, and for one on a run (the third win in a
/// row and every one after it).
pub const WIN_POINTS: u32 = 2;
pub const STREAK_POINTS: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Knockout,
    Arena,
}

/// A game played: the two entrants and who won (their places in
/// [`Cup::entrants`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub a: u8,
    pub b: u8,
    pub winner: u8,
}

/// A line of an arena's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    pub entrant: u8,
    pub points: u32,
    pub wins: u32,
    pub losses: u32,
    /// The points of everyone they played (splits ties).
    pub opposition: u32,
}

/// A tournament.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cup {
    version: u32,
    pub format: Format,
    seed: u64,
    /// The entrants in the order of the draw: a personality's code, or
    /// nothing for the player.
    pub entrants: Vec<String>,
    /// The rounds played, each with its games.
    pub rounds: Vec<Vec<Game>>,
    /// The seed of the player's game being played (it counts when it ends).
    #[serde(default)]
    pub playing: Option<u64>,
    /// The seed of the player's last game that counted (to go back to this
    /// screen after it).
    #[serde(default)]
    pub counted: Option<u64>,
}

fn path() -> Option<std::path::PathBuf> {
    crate::config::config_dir().map(|d| d.join("cup.json"))
}

impl Cup {
    /// A new tournament, drawn from `seed`.
    pub fn new(format: Format, seed: u64) -> Cup {
        let mut entrants: Vec<String> =
            PERSONALITIES.iter().take(ENTRANTS - 1).map(|p| p.code.to_string()).collect();
        entrants.push(String::new());
        Rng::new(seed).shuffle(&mut entrants);
        Cup {
            version: VERSION,
            format,
            seed,
            entrants,
            rounds: Vec::new(),
            playing: None,
            counted: None,
        }
    }

    /// The tournament on disk, if there is a sound one.
    pub fn load() -> Option<Cup> {
        let text = std::fs::read_to_string(path()?).ok()?;
        Cup::parse(&text)
    }

    /// Reads a saved tournament; anything that doesn't hold together is
    /// refused (the player starts a new one).
    pub fn parse(text: &str) -> Option<Cup> {
        let cup: Cup = serde_json::from_str(text).ok()?;
        let sound = cup.version == VERSION
            && cup.entrants.len() == ENTRANTS
            && cup.entrants.iter().filter(|e| e.is_empty()).count() == 1
            && cup.entrants.iter().all(|e| e.is_empty() || personalities::personality(e).is_some())
            && cup.rounds.len() <= cup.total_rounds()
            && cup.rounds.iter().flatten().all(|g| {
                (g.a as usize) < ENTRANTS
                    && (g.b as usize) < ENTRANTS
                    && g.a != g.b
                    && (g.winner == g.a || g.winner == g.b)
            });
        // The rounds must be the ones this draw gives.
        let mut replay = Cup { rounds: Vec::new(), ..cup.clone() };
        for round in &cup.rounds {
            let mut expected = replay.pairings();
            let mut got: Vec<(u8, u8)> = round.iter().map(|g| (g.a, g.b)).collect();
            expected.sort();
            got.sort();
            if expected != got {
                return None;
            }
            replay.rounds.push(round.clone());
        }
        sound.then_some(cup)
    }

    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(self) {
            // Written beside and renamed: never half a file.
            let _ = crate::util::write_atomically(&p, text.as_bytes());
        }
    }

    /// Forgets the tournament on disk.
    pub fn discard() {
        if let Some(p) = path() {
            let _ = std::fs::remove_file(p);
        }
    }

    /// The player's place among the entrants.
    pub fn player(&self) -> u8 {
        self.entrants.iter().position(|e| e.is_empty()).unwrap_or(0) as u8
    }

    pub fn is_player(&self, entrant: u8) -> bool {
        entrant == self.player()
    }

    /// The opponent in place `entrant` (none for the player).
    pub fn personality(&self, entrant: u8) -> Option<&'static personalities::Personality> {
        personalities::personality(self.entrants.get(entrant as usize)?)
    }

    fn strength(&self, entrant: u8) -> f64 {
        // The player is taken for a middling entrant when a game of theirs
        // has to be decided without them (never, as things stand: they play
        // their own; a knockout played out after they lose has only the
        // others left).
        self.personality(entrant).map_or(3.0, |p| p.strength as f64)
    }

    pub fn total_rounds(&self) -> usize {
        match self.format {
            Format::Knockout => 4,
            Format::Arena => ARENA_ROUNDS,
        }
    }

    /// Every round has been played.
    pub fn finished(&self) -> bool {
        self.rounds.len() >= self.total_rounds()
    }

    /// Who is still in a knockout before round `round` (in bracket order).
    pub fn alive(&self, round: usize) -> Vec<u8> {
        let mut alive: Vec<u8> = (0..ENTRANTS as u8).collect();
        for games in self.rounds.iter().take(round) {
            alive = alive
                .chunks(2)
                .filter_map(|pair| {
                    games
                        .iter()
                        .find(|g| pair.contains(&g.a) && pair.contains(&g.b))
                        .map(|g| g.winner)
                })
                .collect();
        }
        alive
    }

    /// The player is still in the tournament (always, in an arena).
    pub fn player_in(&self) -> bool {
        match self.format {
            Format::Arena => true,
            Format::Knockout => self.alive(self.rounds.len()).contains(&self.player()),
        }
    }

    /// The games of the round to play next (empty when it is over).
    pub fn pairings(&self) -> Vec<(u8, u8)> {
        if self.finished() {
            return Vec::new();
        }
        match self.format {
            Format::Knockout => {
                self.alive(self.rounds.len()).chunks(2).map(|p| (p[0], p[1])).collect()
            }
            Format::Arena => self.arena_pairings(),
        }
    }

    /// An arena's next round: down the table, each against the nearest
    /// below they have not played yet (the nearest at all when there is
    /// none).
    fn arena_pairings(&self) -> Vec<(u8, u8)> {
        let order: Vec<u8> = self.standings().iter().map(|s| s.entrant).collect();
        let met = |a: u8, b: u8| {
            self.rounds.iter().flatten().any(|g| (g.a, g.b) == (a, b) || (g.a, g.b) == (b, a))
        };
        let mut free = order;
        let mut pairs = Vec::new();
        while free.len() >= 2 {
            let a = free.remove(0);
            let k = free.iter().position(|b| !met(a, *b)).unwrap_or(0);
            pairs.push((a, free.remove(k)));
        }
        pairs
    }

    /// Who the player meets next (none when they are out or it is over).
    pub fn next_opponent(&self) -> Option<u8> {
        let me = self.player();
        self.pairings().into_iter().find_map(|(a, b)| {
            if a == me {
                Some(b)
            } else if b == me {
                Some(a)
            } else {
                None
            }
        })
    }

    /// The player's game of this round is about to be played with `seed`.
    pub fn begin(&mut self, seed: u64) {
        self.playing = Some(seed);
    }

    /// The player's game of this round ended: the round's other games are
    /// decided and the round is closed. A knockout the player is out of is
    /// played to its end.
    pub fn report(&mut self, won: bool) {
        let Some(opponent) = self.next_opponent() else { return };
        let me = self.player();
        self.counted = self.playing.take();
        self.close_round(Some(Game {
            a: me,
            b: opponent,
            winner: if won { me } else { opponent },
        }));
        while self.format == Format::Knockout && !self.finished() && !self.player_in() {
            self.close_round(None);
        }
    }

    /// Plays the round: `mine` as it was played, the rest by a draw.
    fn close_round(&mut self, mine: Option<Game>) {
        let round = self.rounds.len() as u64;
        let mut rng = Rng::new(self.seed ^ (round + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let games = self
            .pairings()
            .into_iter()
            .map(|(a, b)| match mine {
                Some(g) if (g.a == a && g.b == b) || (g.a == b && g.b == a) => {
                    Game { a, b, winner: g.winner }
                }
                _ => {
                    // A point of strength is worth about two wins in three.
                    let edge = self.strength(a) - self.strength(b);
                    let p = 1.0 / (1.0 + 10f64.powf(-edge * 0.3));
                    Game { a, b, winner: if rng.next_f64() < p { a } else { b } }
                }
            })
            .collect();
        self.rounds.push(games);
    }

    /// An arena's table: points, then the points of the opponents met,
    /// then the order of the draw.
    pub fn standings(&self) -> Vec<Standing> {
        let mut table: Vec<Standing> = (0..ENTRANTS as u8)
            .map(|entrant| Standing { entrant, points: 0, wins: 0, losses: 0, opposition: 0 })
            .collect();
        let mut run = [0u32; ENTRANTS];
        for g in self.rounds.iter().flatten() {
            let (w, l) = (g.winner as usize, if g.winner == g.a { g.b } else { g.a } as usize);
            run[w] += 1;
            run[l] = 0;
            table[w].wins += 1;
            table[w].points += if run[w] >= 3 { STREAK_POINTS } else { WIN_POINTS };
            table[l].losses += 1;
        }
        let points: Vec<u32> = table.iter().map(|s| s.points).collect();
        for g in self.rounds.iter().flatten() {
            table[g.a as usize].opposition += points[g.b as usize];
            table[g.b as usize].opposition += points[g.a as usize];
        }
        table.sort_by(|x, y| {
            y.points
                .cmp(&x.points)
                .then(y.opposition.cmp(&x.opposition))
                .then(x.entrant.cmp(&y.entrant))
        });
        table
    }

    /// The winner, once it is over.
    pub fn champion(&self) -> Option<u8> {
        if !self.finished() {
            return None;
        }
        match self.format {
            Format::Knockout => self.rounds.last()?.first().map(|g| g.winner),
            Format::Arena => self.standings().first().map(|s| s.entrant),
        }
    }

    /// Where the player finished, once it is over (1 is the champion). In
    /// a knockout: 1, 2, then the places shared by those out in the same
    /// round (3 for the semi-finals, 5 for the quarter-finals, 9 before).
    pub fn player_place(&self) -> Option<u32> {
        if !self.finished() {
            return None;
        }
        let me = self.player();
        match self.format {
            Format::Arena => {
                self.standings().iter().position(|s| s.entrant == me).map(|k| k as u32 + 1)
            }
            Format::Knockout => {
                if self.champion() == Some(me) {
                    return Some(1);
                }
                let out =
                    (0..self.rounds.len()).find(|r| !self.alive(r + 1).contains(&me)).unwrap_or(0);
                Some(match out {
                    3 => 2,
                    2 => 3,
                    1 => 5,
                    _ => 9,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_knockout_runs_to_a_champion() {
        for seed in 0..40 {
            let mut cup = Cup::new(Format::Knockout, seed);
            assert_eq!(cup.entrants.len(), ENTRANTS);
            assert_eq!(cup.entrants.iter().filter(|e| e.is_empty()).count(), 1);
            let mut games = 0;
            while let Some(opponent) = cup.next_opponent() {
                assert!(!cup.is_player(opponent));
                assert!(cup.personality(opponent).is_some());
                // Win the first two, lose the third.
                cup.begin(seed + games);
                cup.report(games < 2);
                games += 1;
                assert_eq!(Cup::parse(&serde_json::to_string(&cup).unwrap()), Some(cup.clone()));
            }
            assert_eq!(games, 3);
            assert!(cup.finished() && !cup.player_in());
            assert_eq!(cup.rounds.iter().map(|r| r.len()).collect::<Vec<_>>(), vec![8, 4, 2, 1]);
            let champion = cup.champion().unwrap();
            assert!(!cup.is_player(champion));
            assert_eq!(cup.player_place(), Some(3), "out in the semi-finals");
            assert_eq!(cup.counted, Some(seed + 2));
        }
        // Winning every game wins it.
        let mut cup = Cup::new(Format::Knockout, 7);
        while cup.next_opponent().is_some() {
            cup.report(true);
        }
        assert_eq!(cup.champion(), Some(cup.player()));
        assert_eq!(cup.player_place(), Some(1));
        assert_eq!(cup.rounds.len(), 4);
    }

    #[test]
    fn an_arena_is_five_rounds_for_everyone_on_points() {
        for seed in 0..40 {
            let mut cup = Cup::new(Format::Arena, seed);
            let mut met = Vec::new();
            for round in 0..ARENA_ROUNDS {
                let pairs = cup.pairings();
                assert_eq!(pairs.len(), ENTRANTS / 2);
                let mut seen: Vec<u8> = pairs.iter().flat_map(|(a, b)| [*a, *b]).collect();
                seen.sort();
                assert_eq!(seen, (0..ENTRANTS as u8).collect::<Vec<_>>(), "everyone plays");
                let opponent = cup.next_opponent().unwrap();
                assert!(!met.contains(&opponent), "seed {seed}: the same opponent twice");
                met.push(opponent);
                cup.report(round != 1);
                assert_eq!(Cup::parse(&serde_json::to_string(&cup).unwrap()), Some(cup.clone()));
            }
            assert!(cup.finished() && cup.next_opponent().is_none());
            let table = cup.standings();
            assert!(table.windows(2).all(|w| w[0].points >= w[1].points));
            assert!(table.iter().all(|s| s.wins + s.losses == ARENA_ROUNDS as u32));
            let me = table.iter().find(|s| cup.is_player(s.entrant)).unwrap();
            // Won, lost, then three in a row: the third of them on a run.
            assert_eq!((me.wins, me.losses), (4, 1));
            assert_eq!(me.points, 3 * WIN_POINTS + STREAK_POINTS);
            assert!(cup.player_place().is_some_and(|p| (1..=16).contains(&p)));
            assert_eq!(cup.champion(), Some(table[0].entrant));
        }
    }

    #[test]
    fn stronger_opponents_go_further() {
        // Over many knockouts the player is out of at once, the strongest
        // win far more often than the weakest.
        let mut titles = std::collections::HashMap::new();
        for seed in 0..400 {
            let mut cup = Cup::new(Format::Knockout, seed);
            cup.report(false);
            let champion = cup.personality(cup.champion().unwrap()).unwrap();
            *titles.entry(champion.strength).or_insert(0u32) += 1;
        }
        let strong: u32 = titles.iter().filter(|(s, _)| **s >= 4).map(|(_, n)| n).sum();
        let weak: u32 = titles.iter().filter(|(s, _)| **s <= 2).map(|(_, n)| n).sum();
        assert!(strong > 3 * weak.max(1), "{titles:?}");
    }

    #[test]
    fn a_damaged_file_is_refused() {
        let mut cup = Cup::new(Format::Knockout, 3);
        cup.report(true);
        let good = serde_json::to_string(&cup).unwrap();
        assert!(Cup::parse(&good).is_some());
        assert!(Cup::parse("{").is_none());
        // A game between entrants the draw never paired.
        let mut bad = cup.clone();
        bad.rounds[0][0].a = bad.rounds[0][1].a;
        assert!(Cup::parse(&serde_json::to_string(&bad).unwrap()).is_none());
        // A winner who did not play.
        let mut bad = cup.clone();
        bad.rounds[0][0].winner = bad.rounds[0][1].a;
        assert!(Cup::parse(&serde_json::to_string(&bad).unwrap()).is_none());
        let mut bad = cup;
        bad.entrants[0] = "nobody".into();
        assert!(Cup::parse(&serde_json::to_string(&bad).unwrap()).is_none());
    }
}
