//! Private messages (see `snook_legend_proto::dm`).
//!
//! The chat's rules apply as they do everywhere else: a muted player sends
//! nothing, an account whose chat is off (see [`User::chat_restricted`])
//! neither sends nor receives, one kept to the quick phrases sends only
//! those, and nobody writes to someone who blocked them or whom they
//! blocked. Friends write to each other freely; to anyone else (see
//! [`stranger_may_write`]) only adults, to players who take messages from
//! anyone, a few messages until they answer and a few new conversations a
//! day. A message to someone offline waits for them (the list of
//! conversations after signing in says how many are unread).

use std::collections::HashMap;

use snook_legend_proto::dm::{
    DirectMessage, DmThread, HISTORY_PAGE, MESSAGES_FROM_ANYONE, NEW_A_DAY, UNANSWERED, clean_dm,
};
use snook_legend_proto::{ClientMsg, ServerMsg, is_quick_phrase};

use crate::limits::Bucket;
use crate::lounge::{PHRASES_ONLY, chat_off_reason};
use crate::store::{DmRow, FriendState, User};
use crate::{Server, now};

/// What a player who may not write to someone is told (whatever the
/// reason: a block or an age group is not given away).
pub const CANT_WRITE: &str = "You can't send messages to this player.";
/// Under 18 (or no age group given): only friends write to each other.
pub const FRIENDS_ONLY: &str = "You can only send messages to your friends.";
/// The player written to takes messages from their friends only.
pub const THEIR_FRIENDS_ONLY: &str = "This player only takes messages from friends.";
/// [`UNANSWERED`] messages to a player who isn't a friend, unanswered.
pub const WAIT_FOR_ANSWER: &str = "Wait for an answer before writing again.";
/// [`NEW_A_DAY`] new conversations with players who aren't friends today.
pub const TOO_MANY_NEW: &str = "You've started enough new conversations today. Try again tomorrow.";
/// The name is no one's (or is the player's own).
pub const NO_SUCH_PLAYER: &str = "No such player.";
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
            let other = match server.store.user_by_name(to.trim()).await {
                Ok(Some(u)) if u.deleted_at.is_none() && u.id != me.id => u,
                Ok(_) => return error(NO_SUCH_PLAYER),
                Err(e) => {
                    tracing::warn!("direct message from {}: {e}", me.username);
                    return error("Something went wrong. Please try again.");
                }
            };
            if !may_write(server, &me, &other).await {
                return error(CANT_WRITE);
            }
            if !are_friends(server, &me, &other).await
                && let Err(why) = stranger_may_write(server, &me, &other, at).await
            {
                return error(why);
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
                return error(NO_SUCH_PLAYER);
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

/// Whether `me` and `other` are friends.
async fn are_friends(server: &Server, me: &User, other: &User) -> bool {
    match server.store.friends(me.id).await {
        Ok(friends) => {
            friends.iter().any(|(u, st)| *st == FriendState::Friends && u.id == other.id)
        }
        Err(e) => {
            tracing::warn!("friends of {}: {e}", me.username);
            false
        }
    }
}

/// Whether `me` may write to `other`, who isn't a friend (at `at`): both
/// said they are 18 or older (minors write to their friends only, and only
/// friends write to them), `other` takes messages from anyone, `me` hasn't
/// sent [`UNANSWERED`] messages that `other` didn't answer, and a new
/// conversation isn't one too many today.
async fn stranger_may_write(
    server: &Server,
    me: &User,
    other: &User,
    at: i64,
) -> Result<(), &'static str> {
    if me.minor != Some(false) {
        return Err(FRIENDS_ONLY);
    }
    // (Their age isn't given away: they just can't be written to.)
    if other.minor != Some(false) {
        return Err(CANT_WRITE);
    }
    if !takes_from_anyone(server, other).await {
        return Err(THEIR_FRIENDS_ONLY);
    }
    let oops = |e: crate::store::StoreError| {
        tracing::warn!("direct message from {}: {e}", me.username);
        "Something went wrong. Please try again."
    };
    let waiting = server.store.dm_waiting(me.id, other.id).await.map_err(oops)?;
    if waiting >= UNANSWERED {
        return Err(WAIT_FOR_ANSWER);
    }
    let new = server.store.dm_history(me.id, other.id, None, 1).await.map_err(oops)?.is_empty();
    if new && server.store.dm_started(me.id, at - 86_400).await.map_err(oops)? >= NEW_A_DAY {
        return Err(TOO_MANY_NEW);
    }
    Ok(())
}

/// Whether `user` takes messages from players who aren't their friends:
/// the setting kept with their account's ([`MESSAGES_FROM_ANYONE`]), on
/// unless they turned it off.
async fn takes_from_anyone(server: &Server, user: &User) -> bool {
    let Ok(Some((data, _))) = server.store.account_settings(user.id).await else { return true };
    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&data)
        .ok()
        .and_then(|m| m.get(MESSAGES_FROM_ANYONE).and_then(|v| v.as_str()).map(|v| v != "false"))
        .unwrap_or(true)
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
