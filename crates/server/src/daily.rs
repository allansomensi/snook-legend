//! The daily challenge (see `snook_legend_proto::daily`): the day's seed, the
//! attempts, checking an attempt's strokes and the day's table.

use rand::RngCore;
use snook_legend_proto::daily::{self, Info, Outcome, Refused};
use snook_legend_proto::{ClientMsg, ServerMsg};

use crate::store::{DailyFinish, User};
use crate::{Server, now};

/// The longest attempt the server plays again (a visit clears at most
/// [`daily::MAX_BALLS`] balls, plus the stroke that ends it).
const MAX_STROKES: usize = daily::MAX_BALLS + 1;

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(msg, ClientMsg::Daily | ClientMsg::DailyStart | ClientMsg::DailySubmit { .. })
}

/// Today's seed (drawn the first time).
async fn seed(server: &Server, day: i64) -> Option<u64> {
    let fresh = rand::thread_rng().next_u64();
    match server.store.daily_seed(day, fresh).await {
        Ok(seed) => Some(seed),
        Err(e) => {
            tracing::error!("daily challenge seed: {e}");
            None
        }
    }
}

/// Today's challenge as `me` sees it.
pub async fn info(server: &Server, me: &User) -> Option<Info> {
    let t = now();
    let day = daily::day_of(t);
    let seed = seed(server, day).await?;
    let (row, board, rank) = tokio::join!(
        server.store.daily_row(me.id, day),
        server.store.daily_board(day, daily::BOARD_SIZE as u32),
        server.store.daily_rank(me.id, day),
    );
    let row = row.ok()?;
    let (board, players) = board.ok()?;
    Some(Info {
        day,
        balls: daily::balls(seed),
        attempts_left: daily::ATTEMPTS.saturating_sub(row.attempts),
        best: row.best,
        rank: rank.ok().flatten(),
        open: row.started_at > 0 && t - row.started_at <= daily::TIME_LIMIT_SECS,
        board: board
            .into_iter()
            .map(|r| daily::Entry {
                username: r.username,
                country: r.country,
                balls: r.balls,
                at: r.at,
            })
            .collect(),
        players,
    })
}

pub async fn handle(server: &Server, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    let info_msg = |info: Option<Info>| match info {
        Some(info) => ServerMsg::Daily { info },
        None => ServerMsg::Error { message: "Something went wrong. Try again.".into() },
    };
    match msg {
        ClientMsg::Daily => send(info_msg(info(server, me).await)),
        ClientMsg::DailyStart => {
            let t = now();
            let day = daily::day_of(t);
            let Some(seed) = seed(server, day).await else {
                return send(info_msg(None));
            };
            match server.store.daily_start(me.id, day, t, daily::ATTEMPTS).await {
                Ok(true) => send(ServerMsg::DailyStarted { day, seed }),
                Ok(false) => send(ServerMsg::DailyResult {
                    day,
                    result: Err(Refused::NoAttemptsLeft),
                    coins: 0,
                }),
                Err(e) => {
                    tracing::error!("daily start of {}: {e}", me.username);
                    send(info_msg(None));
                }
            }
        }
        ClientMsg::DailySubmit { day, strokes } => {
            let result = submit(server, me, day, strokes).await;
            let coins = match &result {
                Ok((_, coins)) => *coins,
                Err(_) => 0,
            };
            send(ServerMsg::DailyResult { day, result: result.map(|(o, _)| o), coins });
            send(info_msg(info(server, me).await));
        }
        _ => {}
    }
}

/// Checks and counts an attempt: its outcome and the coins it earned.
async fn submit(
    server: &Server,
    me: &User,
    day: i64,
    strokes: Vec<snook_legend_core::Shot>,
) -> Result<(Outcome, i64), Refused> {
    let t = now();
    if day != daily::day_of(t) {
        // Past midnight: the day's table is closed (the attempt is lost).
        return Err(Refused::DayOver);
    }
    if strokes.len() > MAX_STROKES {
        return Err(Refused::BadStroke);
    }
    let seed = seed(server, day).await.ok_or(Refused::Other)?;
    // A few strokes of simulation: off the async threads.
    let outcome = tokio::task::spawn_blocking(move || daily::score(seed, &strokes))
        .await
        .unwrap_or(Err(Refused::Other));
    // A refused attempt still ends the one under way.
    let (balls, cleared) = outcome.map_or((0, false), |o| (o.balls, o.cleared));
    let finish = server
        .store
        .daily_finish(me.id, day, t, balls, cleared, daily::TIME_LIMIT_SECS)
        .await
        .map_err(|e| {
            tracing::error!("daily result of {}: {e}", me.username);
            Refused::Other
        })?;
    let DailyFinish::Counted { first_clear, .. } = finish else {
        return Err(Refused::NotStarted);
    };
    let outcome = outcome?;
    tracing::info!(user = %me.username, day, balls = outcome.balls, "daily challenge");
    let xp = snook_legend_proto::xp::DAILY_ATTEMPT
        + snook_legend_proto::xp::DAILY_PER_BALL * outcome.balls;
    crate::account::award_xp(server, me, xp).await;
    let mut coins = 0;
    if first_clear {
        let reference = format!("daily:{day}:{}", me.id);
        if let Ok(true) = server
            .store
            .credit(me.id, daily::CLEAR_COINS, "daily challenge", Some(&reference), t)
            .await
        {
            coins = daily::CLEAR_COINS;
            crate::shop::push_wallet(server, me).await;
        }
    }
    Ok((outcome, coins))
}
