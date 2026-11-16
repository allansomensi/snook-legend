//! The player's own account: username, password, recovery e-mail, deletion,
//! the daily bonus and coins earned by playing.

use std::sync::Arc;

use snookered_proto::{ClientMsg, MAX_NAME_CHANGES, ServerMsg, valid_password, valid_username};

use crate::store::{Rename, User, day_of};
use crate::{Server, now, token_hash};

/// Coins for a finished rated game, and extra for winning it.
pub const COINS_PER_GAME: i64 = 4;
pub const COINS_PER_WIN: i64 = 6;
/// Most coins a day from games (the daily bonus is on top).
pub const DAILY_EARN_CAP: i64 = 50;
pub const DAILY_BONUS: i64 = 10;
/// A game this short (in shots) earns nothing: it was arranged or abandoned.
pub const MIN_SHOTS_FOR_COINS: u32 = 8;

/// A plausible e-mail address (one `@`, a dot in the domain, no spaces).
pub fn valid_email(e: &str) -> bool {
    let Some((user, domain)) = e.split_once('@') else { return false };
    e.len() <= 254
        && !user.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains('@')
        && e.chars().all(|c| c.is_ascii_graphic())
}

pub(crate) fn account_msg(user: &User, payment_email: Option<String>) -> ServerMsg {
    ServerMsg::Account {
        recovery_email: user.recovery_email.clone(),
        payment_email,
        name_changes_left: MAX_NAME_CHANGES.saturating_sub(user.name_changes),
    }
}

/// An account message from a signed-in player (never a guest).
/// Returns true when the connection must close (the account was deleted).
pub async fn handle(
    server: &Arc<Server>,
    me: &User,
    token: Option<&str>,
    msg: ClientMsg,
    send: &impl Fn(ServerMsg),
) -> bool {
    let error = |text: &str| send(ServerMsg::Error { message: text.to_string() });
    // The session's copy may be old: read the account again.
    let Ok(Some(fresh)) = server.store.user_by_id(me.id).await else {
        error("Something went wrong. Please try again.");
        return false;
    };
    match msg {
        ClientMsg::Account => {
            let email = server.store.payment_email(me.id).await.ok().flatten();
            send(account_msg(&fresh, email));
        }
        ClientMsg::ChangePassword { old, new } => {
            if !valid_password(&new) {
                error("Passwords need 8 or more characters, not only digits.");
                return false;
            }
            if !server.verify_password(&old, &fresh.password_hash).await {
                error("The current password is wrong.");
                return false;
            }
            let Some(hash) = server.hash_password(&new).await else {
                error("Something went wrong. Please try again.");
                return false;
            };
            if server.store.set_password(me.id, &hash).await.is_err() {
                error("Something went wrong. Please try again.");
                return false;
            }
            // Sign out everywhere else: keep only this connection's session.
            let _ = server.store.delete_user_sessions(me.id).await;
            if let Some(t) = token {
                let at = now();
                let _ = server
                    .store
                    .create_session(
                        &token_hash(t),
                        me.id,
                        crate::store::SessionKind::Game,
                        at,
                        at + crate::SESSION_LIFETIME,
                    )
                    .await;
            }
            tracing::info!(user = me.id, "password changed");
            send(ServerMsg::Notice { text: "Password changed.".into() });
        }
        ClientMsg::SetRecoveryEmail { email } => {
            let email = email.trim().to_lowercase();
            let email = (!email.is_empty()).then_some(email);
            if email.as_deref().is_some_and(|e| !valid_email(e)) {
                error("That doesn't look like an e-mail address.");
                return false;
            }
            if server.store.set_recovery_email(me.id, email.as_deref()).await.is_err() {
                error("Something went wrong. Please try again.");
                return false;
            }
            let mut fresh = fresh;
            fresh.recovery_email = email.clone();
            let payment = server.store.payment_email(me.id).await.ok().flatten();
            send(account_msg(&fresh, payment));
            send(ServerMsg::Notice {
                text: if email.is_some() {
                    "Recovery e-mail saved.".into()
                } else {
                    "Recovery e-mail removed.".into()
                },
            });
        }
        ClientMsg::DeleteAccount { password } => {
            if !server.verify_password(&password, &fresh.password_hash).await {
                error("The current password is wrong.");
                return false;
            }
            if server.store.delete_account(me.id, now()).await.is_err() {
                error("Something went wrong. Please try again.");
                return false;
            }
            tracing::info!(user = me.id, "account deleted");
            server.invalidate_leaderboard();
            send(ServerMsg::AccountDeleted);
            return true;
        }
        _ => {}
    }
    false
}

/// Changes a signed-in player's username (at most [`MAX_NAME_CHANGES`]
/// times, never during a game, at a table, in the queue or in a
/// tournament). Returns the account under its new name; the caller moves
/// the player's session in the lounge to it.
pub async fn change_username(
    server: &Server,
    me: &User,
    username: &str,
    send: &impl Fn(ServerMsg),
) -> Option<User> {
    let error = |text: &str| send(ServerMsg::Error { message: text.to_string() });
    let name = username.trim();
    let Ok(Some(mut fresh)) = server.store.user_by_id(me.id).await else {
        error("Something went wrong. Please try again.");
        return None;
    };
    if fresh.name_changes >= MAX_NAME_CHANGES {
        error("You can't change your username again.");
        return None;
    }
    if name == fresh.username {
        error("That is already your username.");
        return None;
    }
    // The same rules as for a new account (a deleted account's name has a
    // dash, so it is never valid).
    if !valid_username(name) {
        error("Usernames have 3–16 letters, digits or underscores.");
        return None;
    }
    if server.is_reserved_name(name) && !name.eq_ignore_ascii_case(&fresh.username) {
        error("That username is taken.");
        return None;
    }
    if let Some(why) = server.with(|l| l.rename_blocker(&me.username)) {
        error(why);
        return None;
    }
    match server.store.change_username(me.id, name, MAX_NAME_CHANGES).await {
        Ok(Rename::Done) => {}
        Ok(Rename::Taken) => {
            error("That username is taken.");
            return None;
        }
        Ok(Rename::NoChangesLeft) => {
            error("You can't change your username again.");
            return None;
        }
        Err(e) => {
            tracing::error!("username change: {e}");
            error("Something went wrong. Please try again.");
            return None;
        }
    }
    tracing::info!(user = me.id, from = %fresh.username, to = name, "username changed");
    fresh.username = name.to_string();
    fresh.name_changes += 1;
    server.invalidate_leaderboard();
    let payment = server.store.payment_email(me.id).await.ok().flatten();
    send(account_msg(&fresh, payment));
    send(ServerMsg::Notice { text: "Username changed.".into() });
    Some(fresh)
}

/// Signing in: marks the day as active and pays the daily bonus once a day.
pub async fn signed_in(server: &Server, user: &User) {
    let t = now();
    let day = day_of(t);
    let _ = server.store.mark_active(user.id, day).await;
    if server.config.rules.lan {
        return;
    }
    if let Ok(true) = server.store.claim_daily(user.id, day, DAILY_BONUS).await {
        server.with(|l| {
            l.notify(
                &user.username,
                ServerMsg::CoinsEarned { amount: DAILY_BONUS as u32, reason: "daily".into() },
            )
        });
    }
    crate::missions::push(server, user).await;
    // Where the account level stands (for its bar in the game).
    server.with(|l| l.notify(&user.username, ServerMsg::Xp { xp: user.xp, gained: 0 }));
}

/// Coins for a finished rated game (capped per day). `won`: the player won.
pub async fn game_coins(server: &Server, user: &User, won: bool) {
    let amount = COINS_PER_GAME + if won { COINS_PER_WIN } else { 0 };
    let t = now();
    match server.store.earn(user.id, day_of(t), amount, DAILY_EARN_CAP, "game", t).await {
        Ok(n) if n > 0 => server.with(|l| {
            l.notify(
                &user.username,
                ServerMsg::CoinsEarned {
                    amount: n as u32,
                    reason: if won { "win" } else { "game" }.into(),
                },
            )
        }),
        Ok(_) => {}
        Err(e) => tracing::error!("coins for a game: {e}"),
    }
}

/// Experience for `user` (the account level): sent to them, and their level
/// shown to others when it goes up. Never on a local network.
pub async fn award_xp(server: &Server, user: &User, amount: u32) {
    if amount == 0 || server.with(|l| l.rules.lan) {
        return;
    }
    match server.store.add_xp(user.id, amount).await {
        Ok(xp) => server.with(|l| {
            l.notify(&user.username, snookered_proto::ServerMsg::Xp { xp, gained: amount });
            l.set_xp(&user.username, xp);
        }),
        Err(e) => tracing::warn!("xp for {}: {e}", user.username),
    }
}

#[cfg(test)]
mod tests {
    use super::valid_email;

    #[test]
    fn emails() {
        assert!(valid_email("ana@example.com"));
        assert!(valid_email("a.b+c@mail.co.uk"));
        assert!(!valid_email("ana@example"));
        assert!(!valid_email("ana example.com"));
        assert!(!valid_email("@example.com"));
        assert!(!valid_email("ana@.com"));
        assert!(!valid_email("ana@@x.com"));
        assert!(!valid_email("ana @x.com"));
    }
}
