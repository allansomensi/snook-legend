//! The medals of the tournaments against the computer (the knockout and the
//! arena the game runs offline): a gold, a silver or a bronze for the
//! player who plays one to its end in the first three places. The game
//! keeps them until the account is online, and the server keeps them for
//! the account. They are the player's own: shown with the tournaments, not
//! on the profile.

use serde::{Deserialize, Serialize};

/// The kind of tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Knockout,
    Arena,
    /// A kind this version doesn't know.
    #[serde(other)]
    Unknown,
}

impl Format {
    pub fn code(self) -> i16 {
        match self {
            Format::Knockout => 0,
            Format::Arena => 1,
            Format::Unknown => -1,
        }
    }

    pub fn from_code(code: i16) -> Format {
        match code {
            0 => Format::Knockout,
            1 => Format::Arena,
            _ => Format::Unknown,
        }
    }
}

/// A medal won: the tournament (its draw's number, the same for no two
/// tournaments of a player), its kind, and the place (1 gold, 2 silver,
/// 3 bronze).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Medal {
    pub cup: u64,
    pub format: Format,
    pub place: u8,
}

impl Medal {
    /// A medal that can be: a known kind, the first three places.
    pub fn is_sound(&self) -> bool {
        self.format != Format::Unknown && (1..=3).contains(&self.place)
    }
}

/// The medals of a player: gold, silver and bronze, in each kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Medals {
    pub knockout: [u32; 3],
    pub arena: [u32; 3],
}

impl Medals {
    /// Counts one more.
    pub fn add(&mut self, medal: &Medal) {
        if !medal.is_sound() {
            return;
        }
        let k = medal.place as usize - 1;
        match medal.format {
            Format::Knockout => self.knockout[k] += 1,
            Format::Arena => self.arena[k] += 1,
            Format::Unknown => {}
        }
    }

    pub fn total(&self) -> u32 {
        self.knockout.iter().chain(&self.arena).sum()
    }
}

/// The medal for finishing in `place` (none past the third).
pub fn medal_for(place: u32) -> Option<u8> {
    (1..=3).contains(&place).then_some(place as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medals_count_by_kind_and_place() {
        let mut m = Medals::default();
        m.add(&Medal { cup: 1, format: Format::Knockout, place: 1 });
        m.add(&Medal { cup: 2, format: Format::Arena, place: 3 });
        m.add(&Medal { cup: 3, format: Format::Arena, place: 3 });
        // Not medals: a fourth place, an unknown kind.
        m.add(&Medal { cup: 4, format: Format::Arena, place: 4 });
        m.add(&Medal { cup: 5, format: Format::Unknown, place: 1 });
        assert_eq!(m, Medals { knockout: [1, 0, 0], arena: [0, 0, 2] });
        assert_eq!(m.total(), 3);
        assert_eq!((medal_for(1), medal_for(3), medal_for(5)), (Some(1), Some(3), None));
        let f: Format = serde_json::from_str("\"swiss\"").unwrap();
        assert_eq!(f, Format::Unknown);
        assert_eq!(Format::from_code(Format::Arena.code()), Format::Arena);
    }
}
