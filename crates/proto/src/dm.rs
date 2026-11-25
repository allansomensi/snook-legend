//! Direct messages between friends: a conversation with each friend, kept
//! on the server (a message to someone offline waits for them), with the
//! count of the ones not read yet.
//!
//! Only friends write to each other, and the same rules as the chat apply:
//! nobody writes to someone who blocked them (or whom they blocked), and an
//! account whose chat is off (under 18 without a parent's or guardian's
//! permission, or no age group given) neither sends nor receives any.

use serde::{Deserialize, Serialize};

/// The longest message, in characters (as a chat line).
pub const MAX_DM_LEN: usize = crate::MAX_CHAT_LEN;
/// Messages sent in one answer to [`crate::ClientMsg::DmHistory`] (older
/// ones are asked for page by page).
pub const HISTORY_PAGE: usize = 50;

/// One message between two friends.
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
    /// The friend on the other side.
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
