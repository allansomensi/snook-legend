//! The players' inbox (see `snook_legend_proto::inbox`): what the staff and
//! the game tell one player, kept until read and after.
//!
//! Everything that notifies a player goes through [`send`]: it stores the
//! notification and, if the player is online, pushes it at once
//! ([`ServerMsg::InboxNew`]).

use snook_legend_proto::inbox::{Action, Kind};
use snook_legend_proto::{ClientMsg, ServerMsg};

use crate::Server;
use crate::store::User;

/// A notification about to be sent.
#[derive(Debug, Clone, PartialEq)]
pub struct New {
    pub kind: Kind,
    /// What the staff wrote (empty for the game's own).
    pub text: String,
    /// The staff member who sends it (empty: the game).
    pub from: String,
    /// The staff want to know it was read.
    pub important: bool,
    /// Pinned at the top until then (0: not pinned; `i64::MAX`: until taken
    /// down).
    pub pinned_until: i64,
    pub action: Action,
}

impl New {
    /// A notification of the game's own, with no text, button or pin.
    pub fn game(kind: Kind) -> New {
        New {
            kind,
            text: String::new(),
            from: String::new(),
            important: false,
            pinned_until: 0,
            action: Action::None,
        }
    }

    pub fn text(mut self, text: impl Into<String>) -> New {
        self.text = text.into();
        self
    }

    pub fn from(mut self, staff: impl Into<String>) -> New {
        self.from = staff.into();
        self
    }

    pub fn important(mut self) -> New {
        self.important = true;
        self
    }

    pub fn pinned_until(mut self, until: i64) -> New {
        self.pinned_until = until;
        self
    }

    pub fn action(mut self, action: Action) -> New {
        self.action = action;
        self
    }
}

/// Stores a notification for `user_id` and pushes it if they are online.
/// Returns its id (None when it could not be stored: the error is logged).
pub async fn send(server: &Server, user_id: i64, n: New) -> Option<i64> {
    // TODO(0.12 inbox): stored and pushed by the inbox's implementation.
    let _ = (server, user_id, n);
    None
}

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(msg, ClientMsg::Inbox | ClientMsg::InboxRead { .. } | ClientMsg::InboxDelete { .. })
}

pub async fn handle(server: &Server, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    // TODO(0.12 inbox): list, read, delete.
    let _ = (server, me, msg);
    send(ServerMsg::Inbox { items: Vec::new(), unread: 0 });
}
