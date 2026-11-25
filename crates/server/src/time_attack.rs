//! Against the clock online (see `snook_legend_proto::time_attack`): the
//! attempts, checking and timing an attempt's strokes, the week's table and
//! its prizes.
//!
//! Each attempt is dealt a rack of its own when it starts (the seed is
//! kept with the attempt, and only told to the player then): an attempt
//! can't be rehearsed. The time is the server's, in milliseconds from the
//! start it answered to the strokes reaching it, never less than the balls
//! rolled, plus the fouls' penalties.

use rand::RngCore;
use snook_legend_proto::inbox::{Action, Kind};
use snook_legend_proto::time_attack::{
    self, ATTEMPTS_PER_DAY, BOARD_SIZE, Entry, Info, Outcome, PRIZES, Refused, Stroke,
    TIME_LIMIT_SECS, day_of, week_of,
};
use snook_legend_proto::{ClientMsg, ServerMsg};

use crate::store::{AttemptFinish, BestRow, User};
use crate::{Server, now};

pub fn is_msg(msg: &ClientMsg) -> bool {
    matches!(
        msg,
        ClientMsg::TimeAttack | ClientMsg::TimeAttackStart | ClientMsg::TimeAttackSubmit { .. }
    )
}

/// Milliseconds since 1970 (the attempts' clock).
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn entry(r: BestRow) -> Entry {
    Entry { username: r.username, country: r.country, ms: r.ms, at: r.at }
}

/// The week against the clock as `me` sees it.
pub async fn info(server: &Server, me: &User) -> Option<Info> {
    let t = now();
    let week = week_of(t);
    let (board, best, used, open, last) = tokio::join!(
        server.store.time_attack_board(week, BOARD_SIZE as u32),
        server.store.time_attack_best(me.id, week),
        server.store.time_attack_attempts(me.id, day_of(t)),
        server.store.time_attack_open(me.id),
        server.store.time_attack_board(week - 1, PRIZES.len() as u32),
    );
    let (board, players) = board.ok()?;
    let best = best.ok()?;
    let open = open.ok()?.is_some_and(|a| now_ms() - a.started_ms <= TIME_LIMIT_SECS * 1000);
    Some(Info {
        week,
        ends_at: time_attack::week_start(week + 1),
        attempts_left: ATTEMPTS_PER_DAY.saturating_sub(used.ok()?),
        best_ms: best.map(|b| b.0),
        rank: best.and_then(|b| b.1),
        players,
        board: board.into_iter().map(entry).collect(),
        last_week: last.map(|(b, _)| b.into_iter().map(entry).collect()).unwrap_or_default(),
        open,
    })
}

pub async fn handle(server: &Server, me: &User, msg: ClientMsg, send: &impl Fn(ServerMsg)) {
    let info_msg = |info: Option<Info>| match info {
        Some(info) => ServerMsg::TimeAttack { info },
        None => ServerMsg::Error { message: "Something went wrong. Try again.".into() },
    };
    match msg {
        ClientMsg::TimeAttack => send(info_msg(info(server, me).await)),
        ClientMsg::TimeAttackStart => {
            let t = now();
            let seed = rand::thread_rng().next_u64();
            match server
                .store
                .time_attack_start(me.id, day_of(t), now_ms(), seed, ATTEMPTS_PER_DAY)
                .await
            {
                Ok(Some(attempt)) => {
                    tracing::info!(user = %me.username, attempt, "against the clock: started");
                    send(ServerMsg::TimeAttackStarted { attempt, seed })
                }
                Ok(None) => send(ServerMsg::TimeAttackResult {
                    attempt: 0,
                    result: Err(Refused::NoAttemptsLeft),
                }),
                Err(e) => {
                    tracing::error!("against the clock, start of {}: {e}", me.username);
                    send(info_msg(None));
                }
            }
        }
        ClientMsg::TimeAttackSubmit { attempt, strokes } => {
            let result = submit(server, me, attempt, strokes).await;
            send(ServerMsg::TimeAttackResult { attempt, result });
            send(info_msg(info(server, me).await));
        }
        _ => {}
    }
}

/// Checks, times and counts an attempt.
async fn submit(
    server: &Server,
    me: &User,
    attempt: i64,
    strokes: Vec<Stroke>,
) -> Result<Outcome, Refused> {
    let arrived = now_ms();
    if strokes.len() > time_attack::MAX_STROKES {
        return Err(Refused::BadStroke);
    }
    // Not the attempt under way (or one that ran out of time): refused
    // before the simulation costs anything.
    let open = server.store.time_attack_open(me.id).await.map_err(|_| Refused::Other)?;
    let Some(open) = open.filter(|a| a.id == attempt) else {
        return Err(Refused::NotStarted);
    };
    let wall = arrived - open.started_ms;
    if !(0..=TIME_LIMIT_SECS * 1000).contains(&wall) {
        let _ = server.store.time_attack_finish(me.id, attempt, arrived, 0, 0, None).await;
        return Err(Refused::NotStarted);
    }
    // Up to a hundred and fifty strokes of simulation: off the async threads.
    let seed = open.seed;
    let run = tokio::task::spawn_blocking(move || time_attack::replay(seed, &strokes))
        .await
        .unwrap_or(Err(Refused::Other));
    let t = now();
    let week = week_of(t);
    let counted = run.map(|r| (r.time_ms(wall as u32), r.shots, r.fouls));
    let finish = server
        .store
        .time_attack_finish(me.id, attempt, arrived, week, t, counted.ok())
        .await
        .map_err(|e| {
            tracing::error!("against the clock, result of {}: {e}", me.username);
            Refused::Other
        })?;
    let AttemptFinish::Counted { best } = finish else { return Err(Refused::NotStarted) };
    let (ms, shots, fouls) = counted?;
    let rank = server.store.time_attack_best(me.id, week).await.ok().flatten().and_then(|b| b.1);
    tracing::info!(user = %me.username, attempt, ms, fouls, "against the clock: cleared");
    crate::account::award_xp(server, me, snook_legend_proto::xp::DAILY_ATTEMPT).await;
    Ok(Outcome { ms, shots, fouls, best, rank })
}

/// Pays last week's prizes if they haven't been (each place once, however
/// often this runs, on any server): coins and a note in the inbox.
pub async fn pay_prizes(server: &Server) {
    let t = now();
    let week = week_of(t) - 1;
    let Ok((board, _)) = server.store.time_attack_board(week, PRIZES.len() as u32).await else {
        return;
    };
    for (k, row) in board.into_iter().enumerate() {
        let (place, coins) = (k as u32 + 1, PRIZES[k]);
        match server.store.time_attack_prize(week, place, row.user_id, coins, row.ms, t).await {
            Ok(true) => {}
            Ok(false) => continue,
            Err(e) => {
                tracing::error!("against the clock, prize of week {week}: {e}");
                return;
            }
        }
        let reference = format!("time_attack:{week}:{place}");
        let credit =
            || server.store.credit(row.user_id, coins, "against the clock", Some(&reference), t);
        if let Err(e) = crate::retrying("paying a prize against the clock", credit).await {
            tracing::error!("against the clock, prize of {}: {e}", row.username);
            continue;
        }
        tracing::info!(user = %row.username, week, place, coins, "against the clock: prize");
        let text = format!("{} · {}", week_label(week), time_attack::clock(row.ms));
        let note = crate::inbox::New::game(Kind::TournamentResult {
            id: 0,
            name: "Against the clock".into(),
            place,
            prize: coins,
        })
        .text(text)
        .action(Action::Profile { username: row.username.clone() });
        crate::inbox::send(server, row.user_id, note).await;
        if let Ok(Some(user)) = server.store.user_by_id(row.user_id).await {
            crate::shop::push_wallet(server, &user).await;
            server.with(|l| {
                l.notify(
                    &user.username,
                    ServerMsg::CoinsEarned { amount: coins as u32, reason: "time_attack".into() },
                )
            });
        }
    }
}

/// "2026-11-23": the week's Monday (read the same in every language).
fn week_label(week: i64) -> String {
    let days = time_attack::week_start(week).div_euclid(86_400);
    let (y, m, d) = civil(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The date (year, month, day) of a day counted from 1970-01-01.
fn civil(days: i64) -> (i64, u32, u32) {
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Pays the week's prizes as soon as a week is over (checked every few
/// minutes: a restart on Monday morning pays them all the same).
pub async fn run(server: std::sync::Arc<Server>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
    loop {
        interval.tick().await;
        pay_prizes(&server).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeks_are_named_by_their_monday() {
        assert_eq!(week_label(week_of(1_795_392_000)), "2026-11-23");
        assert_eq!(civil(0), (1970, 1, 1));
    }
}
