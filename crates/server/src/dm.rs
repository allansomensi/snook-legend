//! Direct messages between friends (see `snook_legend_proto::dm`).
//!
//! Only friends write to each other, and the chat's rules apply as they do
//! everywhere else: a muted player sends nothing, an account whose chat is
//! off (see [`User::chat_restricted`]) neither sends nor receives, one kept
//! to the quick phrases sends only those, and nobody writes to someone who
//! blocked them or whom they blocked. A message to a friend who is offline
//! waits for them (the list of conversations after signing in says how
//! many are unread).

use std::collections::HashMap;

use snook_legend_proto::dm::{DirectMessage, DmThread, HISTORY_PAGE, clean_dm};
use snook_legend_proto::{ClientMsg, ServerMsg, is_quick_phrase};

use crate::limits::Bucket;
use crate::lounge::{PHRASES_ONLY, chat_off_reason};
use crate::store::{DmRow, FriendState, User};
use crate::{Server, now};

/// What a player who may not write to someone is told (whatever the
/// reason: a block or an age group is not given away).
pub const CANT_WRITE: &str = "You can't send messages to this player.";
/// Only friends write to each other.
pub const FRIENDS_ONLY: &str = "You can only send messages to your friends.";
/// Past the connection's limit (see [`bucket`]).
pub const TOO_QUICKLY: &str = "You're sending messages too quickly.";

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(
        msg,
        ClientMsg::DmSend { .. }
            | ClientMsg::DmHistory { .. }
            | ClientMsg::DmRead { .. }
            | ClientMsg::DmThreads
    )
}

/// A fresh bucket for a connection's messages: a few at once, then one
/// every couple of seconds (as the lounge's chat).
pub fn bucket() -> Bucket {
    Bucket::new(5.0, 0.5)
}

fn message(row: DmRow, names: &HashMap<i64, String>) -> DirectMessage {
    let name = |id: i64| names.get(&id).cloned().unwrap_or_default();
    DirectMessage {
        id: row.id,
        from: name(row.from),
        to: name(row.to),
        text: row.text,
        at: row.at,
        read: row.read_at.is_some(),
    }
}

pub async fn handle(
    server: &Server,
    me: &User,
    msg: ClientMsg,
    send: &impl Fn(ServerMsg),
    bucket: &mut Bucket,
) {
    // The account as the lounge has it now (a mute or the chat a parent
    // turned off since this connection signed in).
    let me = server.with(|l| l.session_user(&me.username).cloned()).unwrap_or_else(|| me.clone());
    let error = |message: &str| send(ServerMsg::Error { message: message.into() });
    match msg {
        ClientMsg::DmSend { to, text } => {
            let text = clean_dm(&text);
            if text.is_empty() {
                return;
            }
            let at = now();
            // Muted (why, until when), or a word the chat doesn't allow.
            let place = snook_legend_proto::moderation::TextPlace::Direct;
            if let Some(msg) = crate::moderation::refusal(server, &me, &text, place) {
                return send(msg);
            }
            if me.chat_restricted() {
                return error(chat_off_reason(&me));
            }
            if crate::lounge::phrases_only(&me) && !is_quick_phrase(&text) {
                return error(PHRASES_ONLY);
            }
            // Counted before the database is asked anything (who are the
            // friends, who blocked whom): messages to strangers too.
            if !bucket.take() {
                return error(TOO_QUICKLY);
            }
            let other = match friend(server, &me, &to).await {
                Ok(other) => other,
                Err(why) => return error(why),
            };
            if !may_write(server, &me, &other).await {
                return error(CANT_WRITE);
            }
            match server.store.dm_send(me.id, other.id, &text, at).await {
                Ok(row) => {
                    let names = HashMap::from([
                        (me.id, me.username.clone()),
                        (other.id, other.username.clone()),
                    ]);
                    let message = message(row, &names);
                    send(ServerMsg::Dm { message: message.clone() });
                    notify(server, &other.username, ServerMsg::Dm { message });
                }
                Err(e) => {
                    tracing::error!("direct message from {}: {e}", me.username);
                    error("Something went wrong. Please try again.");
                }
            }
        }
        ClientMsg::DmHistory { with, before } => {
            // (A conversation stays the player's after a friendship ends:
            // they can read it, not add to it.)
            let Ok(Some(other)) = server.store.user_by_name(with.trim()).await else {
                return error("No such player.");
            };
            let page = HISTORY_PAGE as u32;
            match server.store.dm_history(me.id, other.id, before, page + 1).await {
                Ok(mut rows) => {
                    let more = rows.len() > HISTORY_PAGE;
                    rows.truncate(HISTORY_PAGE);
                    rows.reverse();
                    let names = HashMap::from([
                        (me.id, me.username.clone()),
                        (other.id, other.username.clone()),
                    ]);
                    let messages = rows.into_iter().map(|r| message(r, &names)).collect();
                    send(ServerMsg::DmHistory { with: other.username, messages, more });
                }
                Err(e) => tracing::warn!("conversation of {}: {e}", me.username),
            }
        }
        ClientMsg::DmRead { with } => {
            if let Ok(Some(other)) = server.store.user_by_name(with.trim()).await
                && let Err(e) = server.store.dm_read(me.id, other.id, now()).await
            {
                tracing::warn!("conversation of {}: {e}", me.username);
            }
        }
        ClientMsg::DmThreads => send(threads(server, &me).await),
        _ => {}
    }
}

/// `name`, if they are `me`'s friend.
async fn friend(server: &Server, me: &User, name: &str) -> Result<User, &'static str> {
    let friends = server.store.friends(me.id).await.map_err(|e| {
        tracing::warn!("friends of {}: {e}", me.username);
        "Something went wrong. Please try again."
    })?;
    friends
        .into_iter()
        .find(|(u, st)| *st == FriendState::Friends && u.username.eq_ignore_ascii_case(name.trim()))
        .map(|(u, _)| u)
        .ok_or(FRIENDS_ONLY)
}

/// Whether `me` may write to `other`: neither blocked the other, and the
/// chat of `other` is on.
async fn may_write(server: &Server, me: &User, other: &User) -> bool {
    let (mine, theirs) = tokio::join!(server.store.blocked(me.id), server.store.blocked(other.id));
    let blocks = |list: &[User], id: i64| list.iter().any(|u| u.id == id);
    match (mine, theirs) {
        (Ok(mine), Ok(theirs)) => {
            !blocks(&mine, other.id) && !blocks(&theirs, me.id) && !other.chat_restricted()
        }
        _ => false,
    }
}

/// `me`'s conversations, newest first.
pub async fn threads(server: &Server, me: &User) -> ServerMsg {
    let rows = match server.store.dm_threads(me.id).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("conversations of {}: {e}", me.username);
            Vec::new()
        }
    };
    let mut names: HashMap<i64, String> = HashMap::from([(me.id, me.username.clone())]);
    for (u, _) in server.store.friends(me.id).await.unwrap_or_default() {
        names.insert(u.id, u.username);
    }
    // The others (not friends) in one request.
    let others: Vec<i64> =
        rows.iter().map(|r| r.other).filter(|id| !names.contains_key(id)).collect();
    for u in server.store.users_by_ids(&others).await.unwrap_or_default() {
        if u.deleted_at.is_none() {
            names.insert(u.id, u.username);
        }
    }
    let mut threads = Vec::new();
    for row in rows {
        let Some(with) = names.get(&row.other).cloned() else { continue };
        threads.push(DmThread { with, last: message(row.last, &names), unread: row.unread });
    }
    ServerMsg::DmThreads { threads }
}

/// After signing in: the conversations, when there are some (the unread
/// count shows at once).
pub async fn signed_in(server: &Server, user: &User) {
    let msg = threads(server, user).await;
    if matches!(&msg, ServerMsg::DmThreads { threads } if !threads.is_empty()) {
        notify(server, &user.username, msg);
    }
}

/// The first protocol whose games have direct messages (0.12.0). A 0.11
/// game (24) can't read [`ServerMsg::Dm`] nor [`ServerMsg::DmThreads`]: it
/// would only drop them as malformed (the messages wait in the store for
/// when the player updates).
pub const DM_PROTOCOL: u32 = 25;

/// Sends `msg` to `name` if their game has direct messages.
fn notify(server: &Server, name: &str, msg: ServerMsg) {
    server.with(|l| {
        if l.protocol_of(name).is_some_and(|p| p >= DM_PROTOCOL) {
            l.notify(name, msg)
        }
    });
}
