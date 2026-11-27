//! The player's own account: username, password, recovery e-mail, deletion,
//! the daily bonus and coins earned by playing.

use std::sync::Arc;

use snook_legend_proto::{
    ClientMsg, MAX_NAME_CHANGES, MAX_PASSWORD_BYTES, ServerMsg, valid_password, valid_username,
};

use crate::store::{Rename, User, day_of};
use crate::{Server, now, token_hash};

/// Coins for a finished rated game, and extra for winning it. Coins are
/// scarce on purpose: what the shop sells is meant to be earned over weeks
/// and months of play, not days (a player who plays a lot every day makes
/// some fifty coins a day, missions included; the cheapest item is 400).
pub const COINS_PER_GAME: i64 = 2;
pub const COINS_PER_WIN: i64 = 4;
/// Most coins a day from games (the daily bonus is on top).
pub const DAILY_EARN_CAP: i64 = 24;
pub const DAILY_BONUS: i64 = 5;
/// A game this short (in shots) earns nothing: it was arranged or abandoned.
pub const MIN_SHOTS_FOR_COINS: u32 = 8;

/// What the player is told when the link that confirms their recovery
/// e-mail is on its way.
pub const CONFIRM_SENT: &str = "Recovery e-mail saved. Open the link we sent to it to confirm it (look in the spam folder too).";

/// The link was sent again.
pub const CONFIRM_RESENT: &str = "We sent the link again. Open it to confirm your recovery e-mail (look in the spam folder too).";

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

pub(crate) async fn account_msg(
    server: &Server,
    user: &User,
    payment_email: Option<String>,
) -> ServerMsg {
    let (wallet, in_credits) =
        tokio::join!(server.store.wallet(user.id), server.store.in_credits(user.id));
    let price = server.ops.settings.int(crate::settings::NAME_CHANGE_PRICE).max(0);
    let unconfirmed = crate::recovery::needs_confirmation(server, user).await;
    ServerMsg::Account {
        recovery_email: user.recovery_email.clone(),
        payment_email,
        name_changes_left: MAX_NAME_CHANGES.saturating_sub(user.name_changes),
        // Staff keep their names (see `change_username`).
        name_change_price: if user.role.is_staff() { 0 } else { price as u32 },
        supporter: wallet.is_ok_and(|w| w.vip_until > 0),
        in_credits: in_credits.unwrap_or(false),
        recovery_email_unconfirmed: unconfirmed,
    }
}

/// Checks the current password before a change that asks for it. Wrong
/// ones count towards the same limits as signing in (guessing here is
/// guessing there); errors are sent.
async fn password_ok(
    server: &Server,
    user: &User,
    password: &str,
    ip: &str,
    error: &impl Fn(&str),
) -> bool {
    if password.len() > MAX_PASSWORD_BYTES {
        error("The current password is wrong.");
        return false;
    }
    if !server.auth().try_password(ip, &user.username) {
        error("Too many failed attempts. Wait a few minutes and try again.");
        return false;
    }
    match server.verify_password(password, &user.password_hash).await {
        Some(true) => {
            server.auth().succeeded(ip, &user.username);
            true
        }
        Some(false) => {
            error("The current password is wrong.");
            false
        }
        None => {
            server.auth().not_tried(ip, &user.username);
            error("The server is busy. Try again in a moment.");
            false
        }
    }
}

/// An account message from a signed-in player (never a guest), from the
/// address `ip`. Returns true when the connection must close (the account
/// was deleted).
pub async fn handle(
    server: &Arc<Server>,
    me: &User,
    token: Option<&str>,
    ip: &str,
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
            send(account_msg(server, &fresh, email).await);
        }
        ClientMsg::ChangePassword { old, new } => {
            if !valid_password(&new) {
                error("Passwords need 8 or more characters, not only digits.");
                return false;
            }
            if !password_ok(server, &fresh, &old, ip, &error).await {
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
            // The recovery e-mail is told (whoever changed it).
            crate::recovery::notify_password_changed(
                server,
                &fresh,
                crate::mail::Lang::of_country(&fresh.country),
            );
            send(ServerMsg::Notice { text: "Password changed.".into() });
        }
        ClientMsg::SetRecoveryEmail { email, password, lang } => {
            let email = email.trim().to_lowercase();
            // The password, not the session alone: a stolen session must
            // not set (or remove) where the account is recovered from. Not
            // even to the e-mail of a payment: whoever pays types that one.
            if !password_ok(server, &fresh, &password, ip, &error).await {
                return false;
            }
            let payment = server.store.payment_email(me.id).await.ok().flatten();
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
            // A new address is confirmed by the link sent to it (an
            // address confirmed before needs nothing).
            let to_confirm = match &email {
                Some(e) if server.mail.enabled() => {
                    let confirmed = server.store.confirmed_email(me.id).await.ok().flatten();
                    !confirmed.is_some_and(|c| c.eq_ignore_ascii_case(e))
                }
                _ => false,
            };
            let sent = match (&email, to_confirm) {
                (Some(e), true) => Some(
                    crate::recovery::send_confirmation(
                        server,
                        &fresh,
                        e,
                        crate::mail::Lang::parse(&lang),
                    )
                    .await,
                ),
                _ => None,
            };
            send(account_msg(server, &fresh, payment).await);
            send(ServerMsg::Notice {
                text: match (&email, &sent) {
                    (None, _) => "Recovery e-mail removed.".into(),
                    (Some(_), Some(Ok(()))) => CONFIRM_SENT.into(),
                    _ => "Recovery e-mail saved.".into(),
                },
            });
            if let Some(Err(why)) = sent {
                error(why);
            }
        }
        ClientMsg::ConfirmEmailAgain { lang } => {
            let Some(email) = fresh.recovery_email.clone() else {
                error("Set a recovery e-mail first.");
                return false;
            };
            if !crate::recovery::needs_confirmation(server, &fresh).await {
                send(ServerMsg::Notice { text: "Your recovery e-mail is confirmed.".into() });
                return false;
            }
            match crate::recovery::send_confirmation(
                server,
                &fresh,
                &email,
                crate::mail::Lang::parse(&lang),
            )
            .await
            {
                Ok(()) => send(ServerMsg::Notice { text: CONFIRM_RESENT.into() }),
                Err(why) => error(why),
            }
        }
        ClientMsg::DeleteAccount { password } => {
            if !password_ok(server, &fresh, &password, ip, &error).await {
                return false;
            }
            // Not in the middle of a game or a tournament: the name is freed
            // at once, and whoever took it could take the seat.
            let busy = server.with(|l| {
                let busy = l.is_busy(&fresh.username);
                if !busy {
                    l.drop_invites_named(&fresh.username);
                }
                busy
            });
            if busy {
                error(
                    "Finish your game, and leave your table, the queue or your tournament first.",
                );
                return false;
            }
            // A VIP subscription would go on billing an account that is
            // gone (and that can no longer reach the page to cancel it): it
            // ends first, or the account stays.
            if let Some(stripe) = &server.stripe
                && let Ok(w) = server.store.wallet(me.id).await
                && let Some(customer) = w.stripe_customer.as_deref()
                && let Err(e) = stripe.cancel_subscriptions(customer).await
            {
                tracing::warn!(user = me.id, "subscription not cancelled: {e}");
                error("Your VIP subscription could not be cancelled. Try again in a moment.");
                return false;
            }
            // Their games by correspondence end now, lost (not days later).
            if !server.config.settings.lan {
                crate::correspondence::forfeit_all(server, me.id).await;
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
    // Once the free changes are used, a change costs coins (a sink: see
    // `settings::NAME_CHANGE_PRICE`; 0: no more changes).
    let price = if fresh.name_changes >= MAX_NAME_CHANGES {
        let price = server.ops.settings.int(crate::settings::NAME_CHANGE_PRICE);
        if price <= 0 {
            error("You can't change your username again.");
            return None;
        }
        price
    } else {
        0
    };
    // Staff keep their names: an administrator is made by name (see
    // `SNOOK_LEGEND_ADMINS`), and a listed name left behind by a rename
    // would make whoever took it next an administrator.
    if fresh.role.is_staff() || server.is_admin_name(&fresh.username) {
        error("Staff accounts can't change their username.");
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
    if let Some(why) = crate::moderation::name_refusal(server, name) {
        error(&why);
        return None;
    }
    if server.is_reserved_name(name) && !name.eq_ignore_ascii_case(&fresh.username) {
        error("That username is taken.");
        return None;
    }
    let blocker = server.with(|l| {
        let why = l.rename_blocker(&me.username);
        if why.is_none() {
            l.drop_invites_named(&me.username);
        }
        why
    });
    if let Some(why) = blocker {
        error(why);
        return None;
    }
    // A paid change: the coins first (given back if the name can't be had).
    let paid = if price > 0 {
        let reference = format!("name:{}:{}:{}", me.id, fresh.name_changes, crate::now_millis());
        match server.store.spend(me.id, price, "name change", Some(&reference), now()).await {
            Ok(true) => Some(reference),
            Ok(false) => {
                error("Not enough coins.");
                return None;
            }
            Err(e) => {
                tracing::error!("paying a username change: {e}");
                error("Something went wrong. Please try again.");
                return None;
            }
        }
    } else {
        None
    };
    let limit = if paid.is_some() { u32::MAX } else { MAX_NAME_CHANGES };
    let failed = match server.store.change_username(me.id, name, limit).await {
        Ok(Rename::Done) => None,
        Ok(Rename::Taken) => Some("That username is taken."),
        Ok(Rename::NoChangesLeft) => Some("You can't change your username again."),
        Err(e) => {
            tracing::error!("username change: {e}");
            Some("Something went wrong. Please try again.")
        }
    };
    if let Some(why) = failed {
        if let Some(reference) = &paid {
            let back = format!("refund:{reference}");
            crate::pay(server, me.id, &me.username, price, "refund", &back).await;
        }
        error(why);
        return None;
    }
    if paid.is_some() {
        crate::shop::push_wallet(server, me).await;
    }
    tracing::info!(user = me.id, from = %fresh.username, to = name, "username changed");
    use snook_legend_proto::moderation::ChangeKind;
    crate::moderation::record_change(server, me.id, ChangeKind::Name, &fresh.username, name, None)
        .await;
    fresh.username = name.to_string();
    fresh.name_changes += 1;
    server.invalidate_leaderboard();
    let payment = server.store.payment_email(me.id).await.ok().flatten();
    send(account_msg(server, &fresh, payment).await);
    send(ServerMsg::Notice { text: "Username changed.".into() });
    Some(fresh)
}

/// Signing in: marks the day as active and pays the daily bonus once a day.
pub async fn signed_in(server: &Server, user: &User) {
    let t = now();
    let day = day_of(t);
    let _ = server.store.mark_active(user.id, day).await;
    // Warnings from the staff written while the player was away.
    deliver_warnings(server, user).await;
    // The inbox (warnings and all), and why they are muted if they are.
    crate::inbox::signed_in(server, user).await;
    if server.config.settings.lan {
        return;
    }
    // The events running and coming (double experience, a themed cup…).
    crate::events::signed_in(server, user);
    // The daily bonus, with its streak (see `crate::ops::daily_bonus`).
    crate::ops::daily_bonus(server, user).await;
    crate::missions::push(server, user).await;
    // Where the account level stands (for its bar in the game).
    server.with(|l| l.notify(&user.username, ServerMsg::Xp { xp: user.xp, gained: 0 }));
    // The milestones of a level reached before they brought anything.
    if snook_legend_proto::xp::level_of(user.xp) >= snook_legend_proto::xp::MILESTONES[0].0 {
        level_rewards(server, user, user.xp, user.xp).await;
    }
}

/// Coins for a finished rated game (capped per day). `won`: the player won.
/// A coin event multiplies them, and the day's cap with them.
pub async fn game_coins(server: &Server, user: &User, won: bool) {
    let boost = server.boost(snook_legend_proto::events::EventKind::Coins) as i64;
    let amount = (COINS_PER_GAME + if won { COINS_PER_WIN } else { 0 }) * boost;
    let t = now();
    match server.store.earn(user.id, day_of(t), amount, DAILY_EARN_CAP * boost, "game", t).await {
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
    if amount == 0 || server.with(|l| l.settings.lan) {
        return;
    }
    // An experience event multiplies it.
    let amount = amount * server.boost(snook_legend_proto::events::EventKind::Xp);
    match server.store.add_xp(user.id, amount).await {
        Ok(xp) => {
            server.with(|l| {
                l.notify(&user.username, snook_legend_proto::ServerMsg::Xp { xp, gained: amount });
                l.set_xp(&user.username, xp);
            });
            level_rewards(server, user, xp - amount as i64, xp).await;
        }
        Err(e) => tracing::warn!("xp for {}: {e}", user.username),
    }
}

/// What a rating that went from `before` to `after` pays: the coins of
/// every league reached, once per league for good (the ledger keeps it
/// single: a player who drops out of a league and comes back is not paid
/// again). The league's item comes with its achievement.
pub async fn league_rewards(server: &Server, user: &User, before: i32, after: i32) {
    if server.with(|l| l.settings.lan) {
        return;
    }
    let t = now();
    let mut coins = 0u32;
    for tier in snook_legend_proto::Tier::ALL {
        let min = tier.min_rating();
        if tier.prize() == 0 || before >= min || after < min {
            continue;
        }
        let reference = format!("league:{}:{}", user.id, tier.name());
        match server.store.credit(user.id, tier.prize() as i64, "league", Some(&reference), t).await
        {
            Ok(true) => coins += tier.prize(),
            Ok(false) => {}
            Err(e) => tracing::error!("league coins for {}: {e}", user.username),
        }
    }
    if coins > 0 {
        server.with(|l| {
            l.notify(
                &user.username,
                ServerMsg::CoinsEarned { amount: coins, reason: "league".into() },
            )
        });
        crate::shop::push_wallet(server, user).await;
    }
}

/// What the account level pays as it goes from `before` to `after` XP: a
/// few coins for every level reached (each level once, whatever happens:
/// the ledger keeps it single) and, at the milestones, the achievement
/// that carries an item. With `before == after` (on signing in) only the
/// milestones are looked at, for an account that reached them before they
/// paid anything.
pub async fn level_rewards(server: &Server, user: &User, before: i64, after: i64) {
    use snook_legend_proto::xp;
    let (old, new) = (xp::level_of(before), xp::level_of(after));
    let t = now();
    let mut coins = 0u32;
    for level in (old + 1)..=new {
        let reference = format!("level:{}:{level}", user.id);
        match server
            .store
            .credit(user.id, xp::LEVEL_COINS as i64, "level", Some(&reference), t)
            .await
        {
            Ok(true) => coins += xp::LEVEL_COINS,
            Ok(false) => {}
            Err(e) => tracing::error!("level coins for {}: {e}", user.username),
        }
    }
    if coins > 0 {
        server.with(|l| {
            l.notify(
                &user.username,
                ServerMsg::CoinsEarned { amount: coins, reason: "level".into() },
            )
        });
    }
    let codes: Vec<&str> = xp::milestones_reached(new).collect();
    let mut unlocked = false;
    if !codes.is_empty() {
        match server.store.add_achievements(user.id, &codes, t).await {
            Ok(fresh) if !fresh.is_empty() => {
                unlocked = true;
                crate::inbox::achievements(server, user.id, &fresh).await;
                let all = server.store.achievements(user.id).await.unwrap_or_default();
                let all: Vec<String> = all.into_iter().map(|(code, _)| code).collect();
                server.with(|l| {
                    for code in fresh {
                        l.notify(&user.username, ServerMsg::AchievementUnlocked { code });
                    }
                    l.set_achievements(&user.username, all);
                });
            }
            Ok(_) => {}
            Err(e) => tracing::error!("level achievements for {}: {e}", user.username),
        }
    }
    if coins > 0 || unlocked {
        crate::shop::push_wallet(server, user).await;
    }
}

/// Shows the player the staff's warnings they haven't seen (each once).
pub async fn deliver_warnings(server: &Server, user: &User) {
    let Ok(warnings) = server.store.take_warnings(user.id, now()).await else { return };
    // A game with an inbox has them there (pinned, to confirm).
    if server.with(|l| l.has_inbox(&user.username)) {
        return;
    }
    for text in warnings {
        server.with(|l| l.notify(&user.username, ServerMsg::Notice { text: warning_text(&text) }));
    }
}

/// How a warning from the staff reads to the player.
pub fn warning_text(text: &str) -> String {
    format!("Warning from the staff: {text}")
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
