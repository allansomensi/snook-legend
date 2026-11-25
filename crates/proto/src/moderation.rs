//! What the moderation tells a player, as facts the game words itself: why
//! a line of chat (a status, a note, a name…) was turned away, the history
//! of an account's names and countries, and who a chat line mentions.

use serde::{Deserialize, Serialize};

/// Why something a player wrote was turned away.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "why", rename_all = "snake_case")]
pub enum TextRefusal {
    /// The player is muted until `until` (Unix seconds); `reason`: what the
    /// staff wrote (may be empty).
    Muted {
        until: i64,
        #[serde(default)]
        reason: String,
    },
    /// The text has a word the game doesn't allow (`word`: as listed, so
    /// the player knows what to take out).
    BannedWord { word: String },
    /// A reason this version doesn't know.
    #[serde(other)]
    Other,
}

/// What a text that was turned away was going to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextPlace {
    /// The lounge's chat.
    #[default]
    Chat,
    /// The chat at the table.
    MatchChat,
    /// The status message on the player's card.
    Status,
    /// A note on the "looking for a game" board.
    Seek,
    /// The message that goes with a present.
    Gift,
    /// A new username.
    Username,
    /// A direct message to a friend.
    Direct,
    /// The chat of a game's spectators.
    SpectatorChat,
    #[serde(other)]
    Other,
}

/// A change of name or country on an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Name,
    Country,
    #[serde(other)]
    Other,
}

/// One change of an account's name or country, for its history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileChange {
    pub kind: ChangeKind,
    pub old: String,
    pub new: String,
    /// Unix seconds.
    pub at: i64,
}

/// The `@names` written in `text` that could be usernames, in order, each
/// once (lower case). The server keeps those of real players.
pub fn mention_candidates(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // An "@" that starts a word: not in the middle of an e-mail.
        let starts = chars[i] == '@' && (i == 0 || !is_name_char(chars[i - 1]));
        if starts {
            let name: String =
                chars[i + 1..].iter().take_while(|c| is_name_char(**c)).take(17).collect();
            if crate::valid_username(&name) {
                let name = name.to_lowercase();
                if !out.contains(&name) {
                    out.push(name);
                }
            }
            i += 1 + name.chars().count();
        } else {
            i += 1;
        }
    }
    out
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Most players one chat line may mention.
pub const MAX_MENTIONS: usize = 5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_are_whole_names_after_an_at() {
        assert_eq!(mention_candidates("hi @Ana and @bob_2!"), ["ana", "bob_2"]);
        assert_eq!(mention_candidates("mail ana@example.com"), Vec::<String>::new());
        assert_eq!(mention_candidates("@ab @abc @abc"), ["abc"]);
        assert_eq!(mention_candidates("@"), Vec::<String>::new());
        // Longer than a name can be: not a name.
        assert_eq!(mention_candidates("@abcdefghijklmnopq"), Vec::<String>::new());
    }

    #[test]
    fn refusals_round_trip_and_tolerate_the_unknown() {
        let r = TextRefusal::Muted { until: 5, reason: "spam".into() };
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<TextRefusal>(&json).unwrap(), r);
        let later: TextRefusal = serde_json::from_str(r#"{"why":"too_loud"}"#).unwrap();
        assert_eq!(later, TextRefusal::Other);
    }
}
