//! Private messages: a conversation with each player, kept on the server (a
//! message to someone offline waits for them), with the count of the ones
//! not read yet. The game shows them in the inbox (and over the lounge,
//! from the friends list).
//!
//! The same rules as the chat apply: nobody writes to someone who blocked
//! them (or whom they blocked), and an account whose chat is off (under 18
//! without a parent's or guardian's permission, or no age group given)
//! neither sends nor receives any. Friends write to each other freely.
//! Players who aren't friends write to each other only when both said they
//! are 18 or older, when the one written to takes messages from anyone
//! (Settings → Game, [`MESSAGES_FROM_ANYONE`]), a few messages until they
//! answer ([`UNANSWERED`]) and a few new conversations a day
//! ([`NEW_A_DAY`]).

use serde::{Deserialize, Serialize};

/// The setting (kept with the account's, see `ClientMsg::SaveSettings`)
/// that lets players who aren't friends write: "true" or "false" (on when
/// missing).
pub const MESSAGES_FROM_ANYONE: &str = "messages_from_anyone";
/// Messages to a player who isn't a friend before they answer.
pub const UNANSWERED: u32 = 3;
/// New conversations with players who aren't friends, in a day.
pub const NEW_A_DAY: u32 = 10;

/// The longest message, in characters (as a chat line).
pub const MAX_DM_LEN: usize = crate::MAX_CHAT_LEN;
/// Messages sent in one answer to [`crate::ClientMsg::DmHistory`] (older
/// ones are asked for page by page).
pub const HISTORY_PAGE: usize = 50;

/// One message between two players.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectMessage {
    /// The server's number for it (later messages have larger numbers).
    pub id: i64,
    pub from: String,
    pub to: String,
    pub text: String,
    /// Unix seconds.
    pub at: i64,
    /// The receiver has read it.
    #[serde(default)]
    pub read: bool,
}

/// A conversation, as the list of conversations shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmThread {
    /// The player on the other side.
    pub with: String,
    /// The newest message, either way.
    pub last: DirectMessage,
    /// Messages from them not read yet.
    pub unread: u32,
}

/// Cleans a direct message (see [`crate::clean_text`]).
pub fn clean_dm(text: &str) -> String {
    crate::clean_text(text, MAX_DM_LEN)
}
