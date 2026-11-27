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
//!
//! The one who wrote is told how their messages fare: when they reach the
//! other player's game (at once if they are online, else as they sign in)
//! and when they are read ([`ServerMsg::DmStatus`]); and either sees the
//! other typing to them ([`ServerMsg::DmTyping`]), between friends or
//! players who already have a conversation. Games before
//! [`snook_legend_proto::DM_RECEIPTS_PROTOCOL`] are told none of it (they
//! would drop it as malformed), and a block keeps all of it back, both
//! ways.

use std::collections::{HashMap, HashSet};

use snook_legend_proto::dm::{
    DirectMessage, DmThread, HISTORY_PAGE, MESSAGES_FROM_ANYONE, NEW_A_DAY, UNANSWERED, clean_dm,
};
use snook_legend_proto::{ClientMsg, PlayerStatus, Rules, ServerMsg, is_quick_phrase};

use crate::limits::Bucket;
use crate::lounge::{PHRASES_ONLY, chat_off_reason, phrases_only, player_info};
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
/// The players a connection remembers having a conversation with (see
/// [`Limits`]): past this many, it starts over.
const TALKED_KEPT: usize = 256;

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(
        msg,
        ClientMsg::DmSend { .. }
            | ClientMsg::DmHistory { .. }
            | ClientMsg::DmRead { .. }
            | ClientMsg::DmThreads
            | ClientMsg::DmTyping { .. }
    )
}

/// A fresh bucket for a connection's messages: a few at once, then one
/// every couple of seconds (as the lounge's chat).
pub fn bucket() -> Bucket {
    Bucket::new(5.0, 0.5)
}

/// A fresh bucket for a connection's "typing…" notices: the game sends one
/// every [`snook_legend_proto::dm::TYPING_EVERY`] seconds at most while its
/// player types, and a second one at once when they turn to another
/// conversation.
pub fn typing_bucket() -> Bucket {
    Bucket::new(2.0, 0.5)
}

/// A connection's limits for private messages (kept with the account's
/// other limits a while after it closes), and the players it was found to
/// have a conversation with: saying "typing…" to them every few seconds
/// doesn't ask the database each time (a conversation, once there, stays).
#[derive(Debug, Clone)]
pub struct Limits {
    /// Messages sent (see [`bucket`]).
    messages: Bucket,
    /// "Typing…" notices (see [`typing_bucket`]): past it, they are dropped
    /// without a word.
    typing: Bucket,
    /// Players (ids) this account has a conversation with.
    talked: HashSet<i64>,
}

impl Limits {
    pub fn fresh() -> Limits {
        Limits { messages: bucket(), typing: typing_bucket(), talked: HashSet::new() }
    }
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
        // (A message read has been delivered, whatever the row says.)
        delivered: row.delivered_at.is_some() || row.read_at.is_some(),
    }
}

pub async fn handle(
    server: &Server,
    me: &User,
    msg: ClientMsg,
    send: &impl Fn(ServerMsg),
    limits: &mut Limits,
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
            if phrases_only(&me) && !is_quick_phrase(&text) {
                return error(PHRASES_ONLY);
            }
            // Counted before the database is asked anything (who are the
            // friends, who blocked whom): messages to strangers too.
            if !limits.messages.take() {
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
                Ok(mut row) => {
                    // Online, with a game that shows it: it reaches them
                    // now, and the one who wrote sees it did.
                    if has_messages(server, &other.username) {
                        match server.store.dm_delivered(row.id, at).await {
                            Ok(_) => row.delivered_at = Some(at),
                            Err(e) => tracing::warn!("direct message from {}: {e}", me.username),
                        }
                    }
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
            let (rows, friends) = tokio::join!(
                server.store.dm_history(me.id, other.id, before, page + 1),
                are_friends(server, &me, &other),
            );
            match rows {
                Ok(mut rows) => {
                    let more = rows.len() > HISTORY_PAGE;
                    rows.truncate(HISTORY_PAGE);
                    rows.reverse();
                    let names = HashMap::from([
                        (me.id, me.username.clone()),
                        (other.id, other.username.clone()),
                    ]);
                    let messages = rows.into_iter().map(|r| message(r, &names)).collect();
                    let online = server.with(|l| l.is_online(&other.username));
                    send(ServerMsg::DmHistory {
                        with: other.username,
                        messages,
                        more,
                        online,
                        // When they were last here: friends only (as their
                        // profile says it).
                        last_seen: if friends { other.last_seen } else { 0 },
                    });
                }
                Err(e) => tracing::warn!("conversation of {}: {e}", me.username),
            }
        }
        ClientMsg::DmRead { with } => {
            let Ok(Some(other)) = server.store.user_by_name(with.trim()).await else { return };
            match server.store.dm_read(me.id, other.id, now()).await {
                // The one who wrote sees they were read.
                Ok(Some(up_to)) => {
                    let msg = ServerMsg::DmStatus { with: me.username.clone(), up_to, read: true };
                    tell(server, &other.username, &me.username, msg);
                }
                Ok(None) => {}
                Err(e) => tracing::warn!("conversation of {}: {e}", me.username),
            }
        }
        ClientMsg::DmThreads => send(threads(server, &me).await),
        ClientMsg::DmTyping { to } => typing(server, &me, to.trim(), limits).await,
        _ => {}
    }
}

/// `me` is typing to `to`, who is told so if they may be (see
/// [`crate::lounge::Lounge::typing_to`]) when `me` may write (not muted,
/// the chat on, not kept to the quick phrases, which aren't typed) and the
/// two are friends or have a conversation already. Never answered: past
/// the connection's limit, or for any other reason, the notice just goes.
async fn typing(server: &Server, me: &User, to: &str, limits: &mut Limits) {
    if !limits.typing.take() || me.is_muted(now()) || me.chat_restricted() || phrases_only(me) {
        return;
    }
    let Some((other, friends)) = server.with(|l| l.typing_to(&me.username, to)) else { return };
    if !friends && !limits.talked.contains(&other) {
        match server.store.dm_history(me.id, other, None, 1).await {
            Ok(rows) if !rows.is_empty() => {
                if limits.talked.len() >= TALKED_KEPT {
                    limits.talked.clear();
                }
                limits.talked.insert(other);
            }
            Ok(_) => return,
            Err(e) => return tracing::warn!("typing of {}: {e}", me.username),
        }
    }
    tell(server, to, &me.username, ServerMsg::DmTyping { from: me.username.clone() });
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

/// `me`'s conversations, newest first, each with the other player as they
/// are now: as the lounge shows them when they are online, else as the
/// friends list does.
pub async fn threads(server: &Server, me: &User) -> ServerMsg {
    let rows = match server.store.dm_threads(me.id).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("conversations of {}: {e}", me.username);
            Vec::new()
        }
    };
    // The others in one request.
    let others: Vec<i64> = rows.iter().map(|r| r.other).collect();
    let users: HashMap<i64, User> = server
        .store
        .users_by_ids(&others)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|u| u.deleted_at.is_none())
        .map(|u| (u.id, u))
        .collect();
    let mut names: HashMap<i64, String> =
        users.values().map(|u| (u.id, u.username.clone())).collect();
    names.insert(me.id, me.username.clone());
    let threads = server.with(|l| {
        rows.into_iter()
            .filter_map(|row| {
                let user = users.get(&row.other)?;
                let online = l.player(&user.username);
                Some(DmThread {
                    with: user.username.clone(),
                    last: message(row.last, &names),
                    unread: row.unread,
                    online: online.is_some(),
                    player: Some(online.unwrap_or_else(|| {
                        player_info(user, false, PlayerStatus::Available, Rules::default())
                    })),
                })
            })
            .collect()
    });
    ServerMsg::DmThreads { threads }
}

/// After signing in: what waited for the player reaches their game (unless
/// it is a 0.11 game, which can't show it: it waits on), and those who
/// wrote it are told so; then the conversations, when there are some (the
/// unread count shows at once).
pub async fn signed_in(server: &Server, user: &User) {
    if has_messages(server, &user.username) {
        delivered(server, user).await;
    }
    let msg = threads(server, user).await;
    if matches!(&msg, ServerMsg::DmThreads { threads } if !threads.is_empty()) {
        notify(server, &user.username, msg);
    }
}

/// Everything that waited for `user` reached their game: each player who
/// wrote some of it is told so, up to their newest message.
async fn delivered(server: &Server, user: &User) {
    let newest = match server.store.dm_delivered_to(user.id, now()).await {
        Ok(newest) if !newest.is_empty() => newest,
        Ok(_) => return,
        Err(e) => return tracing::warn!("messages waiting for {}: {e}", user.username),
    };
    let senders: Vec<i64> = newest.iter().map(|&(from, _)| from).collect();
    let names: HashMap<i64, String> = server
        .store
        .users_by_ids(&senders)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|u| (u.id, u.username))
        .collect();
    for (from, up_to) in newest {
        let Some(name) = names.get(&from) else { continue };
        let msg = ServerMsg::DmStatus { with: user.username.clone(), up_to, read: false };
        tell(server, name, &user.username, msg);
    }
}

/// The first protocol whose games have direct messages (0.12.0). A 0.11
/// game (24) can't read [`ServerMsg::Dm`] nor [`ServerMsg::DmThreads`]: it
/// would only drop them as malformed (the messages wait in the store for
/// when the player updates).
pub const DM_PROTOCOL: u32 = 25;

/// Whether `name` is online with a game that has direct messages.
fn has_messages(server: &Server, name: &str) -> bool {
    server.with(|l| l.protocol_of(name)).is_some_and(|p| p >= DM_PROTOCOL)
}

/// Sends `msg` to `name` if their game has direct messages.
fn notify(server: &Server, name: &str, msg: ServerMsg) {
    server.with(|l| {
        if l.protocol_of(name).is_some_and(|p| p >= DM_PROTOCOL) {
            l.notify(name, msg)
        }
    });
}

/// Tells `to` how the conversation with `from` fares (a receipt, "typing…")
/// if they may be told (see [`crate::lounge::Lounge::takes_receipts`]).
fn tell(server: &Server, to: &str, from: &str, msg: ServerMsg) {
    server.with(|l| {
        if l.takes_receipts(to, from) {
            l.notify(to, msg)
        }
    });
}
