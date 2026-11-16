//! Status messages, recent opponents and the staff's side of the "looking
//! for a game" board (the board itself lives in the lounge).

use std::collections::HashSet;

use snookered_proto::{ClientMsg, PlayerStatus, RecentOpponent, Role, ServerMsg, clean_status};

use crate::lounge::{key, player_info};
use crate::store::User;
use crate::{Server, now};

/// How many different recent opponents a player is shown.
pub const RECENT_OPPONENTS: u32 = 20;

/// Whether `staff` may moderate what `target` wrote: staff only, and only
/// administrators act on other staff.
pub fn may_moderate(staff: Role, target: Role) -> bool {
    staff.is_staff() && (!target.is_staff() || staff == Role::Admin)
}

/// A social message from a signed-in player (never a guest). Returns the
/// player's account when it changed (their own status message).
pub async fn handle(
    server: &Server,
    me: &User,
    msg: ClientMsg,
    send: &impl Fn(ServerMsg),
) -> Option<User> {
    let error = |text: &str| send(ServerMsg::Error { message: text.to_string() });
    match msg {
        ClientMsg::SetStatusMessage { text } => {
            // The account as stored: a mute may have come since signing in.
            let Ok(Some(fresh)) = server.store.user_by_id(me.id).await else {
                error("Something went wrong. Please try again.");
                return None;
            };
            let t = now();
            if fresh.is_muted(t) {
                let mins = (fresh.muted_until.unwrap_or(t) - t + 59) / 60;
                error(&format!("You are muted for {mins} more minutes."));
                return None;
            }
            let text = clean_status(&text);
            if text == fresh.status_message {
                return None;
            }
            if let Err(e) = server.store.set_status_message(me.id, &text).await {
                tracing::error!("status message: {e}");
                error("Something went wrong. Please try again.");
                return None;
            }
            set_status_in_lounge(server, &me.username, &text);
            let mut updated = me.clone();
            updated.status_message = text;
            Some(updated)
        }
        ClientMsg::ClearStatusMessage { username } => {
            let target = match server.store.user_by_name(username.trim()).await {
                Ok(Some(u)) => u,
                _ => {
                    error("No such player.");
                    return None;
                }
            };
            if !may_moderate(me.role, target.role) || target.id == me.id {
                error("You can't do that.");
                return None;
            }
            if target.status_message.is_empty() {
                return None;
            }
            if let Err(e) = server.store.set_status_message(target.id, "").await {
                tracing::error!("clearing a status message: {e}");
                return None;
            }
            set_status_in_lounge(server, &target.username, "");
            let notice = "A moderator cleared your status message.";
            server.with(|l| l.notify(&target.username, ServerMsg::Notice { text: notice.into() }));
            audit(server, me, "clear-status", &target.username, &target.status_message).await;
            send(ServerMsg::Notice { text: "Status message cleared.".into() });
            None
        }
        ClientMsg::RemoveSeek { username } => {
            let target = server.store.user_by_name(username.trim()).await.ok().flatten();
            let role = target.as_ref().map(|u| u.role).unwrap_or_default();
            if !may_moderate(me.role, role) {
                error("You can't do that.");
                return None;
            }
            if server.with(|l| l.remove_seek_of(&username)) {
                audit(server, me, "remove-seek", &username, "").await;
            }
            None
        }
        ClientMsg::RecentOpponents => {
            send(recent_msg(server, me).await);
            None
        }
        _ => None,
    }
}

/// Puts a new status message on an online player's card (everyone in the
/// lounge sees it change).
fn set_status_in_lounge(server: &Server, name: &str, text: &str) {
    server.with(|l| {
        if let Some(u) = l.session_user(name) {
            let mut u = u.clone();
            u.status_message = text.to_string();
            l.refresh_user(u);
        }
    });
}

async fn audit(server: &Server, staff: &User, action: &str, target: &str, details: &str) {
    if let Err(e) = server.store.audit(staff.id, action, target, details, now()).await {
        tracing::error!("audit log: {e}");
    }
    tracing::info!(staff = %staff.username, action, target, "staff action in the game");
}

/// The people `me` played recently (not those they blocked), as they are
/// now: online ones with their status in the lounge.
pub async fn recent_msg(server: &Server, me: &User) -> ServerMsg {
    let (rows, blocked) = tokio::join!(
        server.store.recent_opponents(me.id, RECENT_OPPONENTS),
        server.store.blocked(me.id)
    );
    let rows = match rows {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("recent opponents: {e}");
            Vec::new()
        }
    };
    let blocked: HashSet<i64> = blocked.unwrap_or_default().iter().map(|u| u.id).collect();
    let players = server.with(|l| {
        rows.into_iter()
            .filter(|r| !blocked.contains(&r.user.id) && key(&r.user.username) != key(&me.username))
            .map(|r| {
                let live = l.player(&r.user.username);
                RecentOpponent {
                    online: live.is_some(),
                    player: live
                        .unwrap_or_else(|| player_info(&r.user, false, PlayerStatus::Available)),
                    at: r.at,
                    won: r.won,
                    rated: r.rated,
                    games: r.games,
                }
            })
            .collect()
    });
    ServerMsg::RecentOpponents { players }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_staff_moderate_and_only_admins_moderate_staff() {
        assert!(!may_moderate(Role::Player, Role::Player));
        assert!(may_moderate(Role::Moderator, Role::Player));
        assert!(!may_moderate(Role::Moderator, Role::Moderator));
        assert!(!may_moderate(Role::Moderator, Role::Admin));
        assert!(may_moderate(Role::Admin, Role::Moderator));
        assert!(may_moderate(Role::Admin, Role::Player));
    }
}
