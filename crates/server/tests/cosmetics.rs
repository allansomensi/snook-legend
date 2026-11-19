//! Cosmetics: cues that level up with online wins, themed tables and halls
//! (bought, equipped and kept like any other item).
//!
//! The store tests run on the in-memory store always, and on PostgreSQL when
//! `SNOOK_LEGEND_TEST_DATABASE_URL` points at a database that may be wiped.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use snook_legend_core::{Difficulty, EightBallGame, ai};
use snook_legend_proto::shop::{self, Category};
use snook_legend_proto::{ClientMsg, PROTOCOL_VERSION, Role, ServerMsg, parse_server};
use snook_legend_server::lounge::Rules;
use snook_legend_server::store::{NewUser, Store};
use snook_legend_server::{Config, Server, now, serve};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

const PASSWORD: &str = "secret123";

async fn start() -> (String, std::sync::Arc<Server>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let rules = Rules { max_connections_per_ip: 100, ..Rules::default() };
    let config = Config { bind: addr, rules, ..Config::default() };
    let server = Server::with_store(config, Store::memory()).await;
    tokio::spawn(serve(listener, server.clone()));
    (format!("ws://{addr}/ws"), server)
}

struct Client {
    ws: Ws,
}

impl Client {
    async fn register(url: &str, name: &str) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello { version: PROTOCOL_VERSION }).await;
        c.send(ClientMsg::Register {
            username: name.into(),
            password: PASSWORD.into(),
            country: "BR".into(),
            minor: None,
        })
        .await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    async fn send(&mut self, msg: ClientMsg) {
        self.ws.send(Message::Text(msg.to_json().into())).await.unwrap();
    }

    /// Reads messages until one matches `pred`.
    async fn expect<F: Fn(&ServerMsg) -> bool>(&mut self, what: &str, pred: F) -> ServerMsg {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            let msg = tokio::time::timeout(left, self.ws.next())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
                .expect("connection closed")
                .unwrap();
            if let Message::Text(t) = msg {
                let m = parse_server(t.as_str()).unwrap();
                if pred(&m) {
                    return m;
                }
            }
        }
    }

    async fn login(url: &str, name: &str) -> Client {
        let (ws, _) = connect_async(url).await.unwrap();
        let mut c = Client { ws };
        c.send(ClientMsg::Hello { version: PROTOCOL_VERSION }).await;
        c.send(ClientMsg::Login { username: name.into(), password: PASSWORD.into() }).await;
        c.expect("auth", |m| matches!(m, ServerMsg::AuthOk { .. })).await;
        c
    }

    /// The first wallet the server sends that satisfies `pred(owned, equipped)`.
    async fn wallet_where(
        &mut self,
        pred: impl Fn(&[String], &[(String, String)]) -> bool,
    ) -> (Vec<String>, Vec<(String, String)>, Vec<(String, u32)>) {
        let m = self
            .expect(
                "wallet",
                |m| matches!(m, ServerMsg::Wallet { owned, equipped, .. } if pred(owned, equipped)),
            )
            .await;
        match m {
            ServerMsg::Wallet { owned, equipped, cue_wins, .. } => (owned, equipped, cue_wins),
            _ => unreachable!(),
        }
    }

    async fn error(&mut self) -> String {
        match self.expect("error", |m| matches!(m, ServerMsg::Error { .. })).await {
            ServerMsg::Error { message } => message,
            _ => unreachable!(),
        }
    }
}

/// Invites `b` from `a` and returns (game, seat of a, players as `a` sees them).
async fn start_match(
    a: &mut Client,
    b: &mut Client,
    b_name: &str,
    a_name: &str,
    rated: bool,
) -> (EightBallGame, usize, [snook_legend_proto::PlayerInfo; 2]) {
    a.send(ClientMsg::Invite { to: b_name.into(), rated }).await;
    b.expect("invite", |m| matches!(m, ServerMsg::Invited { .. })).await;
    b.send(ClientMsg::AnswerInvite { from: a_name.into(), accept: true }).await;
    let ServerMsg::MatchStarted { game, seat, players, .. } =
        a.expect("match a", |m| matches!(m, ServerMsg::MatchStarted { .. })).await
    else {
        unreachable!()
    };
    b.expect("match b", |m| matches!(m, ServerMsg::MatchStarted { .. })).await;
    (game, seat, players)
}

/// Plays a whole game with the computer's shots on both sides; returns the
/// winner's seat and the number of shots.
async fn play_out(
    a: &mut Client,
    b: &mut Client,
    mut game: EightBallGame,
    sa: usize,
) -> (usize, u32) {
    for k in 0..400u64 {
        let d = ai::decide(&game, Difficulty::Hard, k);
        let msg = ClientMsg::Shoot {
            turn: game.shots_taken(),
            shot: d.shot,
            called_pocket: d.called_pocket,
            cue_ball: d.cue_ball,
        };
        if game.current_player() == sa {
            a.send(msg).await
        } else {
            b.send(msg).await
        }
        let ServerMsg::ShotResult { end, .. } =
            a.expect("result", |m| matches!(m, ServerMsg::ShotResult { .. })).await
        else {
            unreachable!()
        };
        b.expect("result b", |m| matches!(m, ServerMsg::ShotResult { .. })).await;
        game = end;
        if let Some(over) = game.game_over() {
            a.expect("end", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
            b.expect("end b", |m| matches!(m, ServerMsg::MatchEnded { .. })).await;
            return (over.winner, game.shots_taken());
        }
    }
    panic!("the game did not finish");
}

/// A casual game won online counts for the cue the winner had in use; the
/// tenth win is level 1, which the profile and the next table show.
#[tokio::test]
async fn online_wins_level_up_the_cue_in_use() {
    let (url, server) = start().await;
    let mut a = Client::register(&url, "alice").await;
    let mut b = Client::register(&url, "bob").await;
    let ids = [
        server.store.user_by_name("alice").await.unwrap().unwrap().id,
        server.store.user_by_name("bob").await.unwrap().unwrap().id,
    ];
    // Nine wins each already: the next one is level 1.
    for id in ids {
        for _ in 0..9 {
            server.store.add_cue_win(id, "cue_classic").await.unwrap();
        }
    }
    let (game, sa, players) = start_match(&mut a, &mut b, "bob", "alice", false).await;
    assert_eq!(players.map(|p| p.cue_level), [0, 0], "nine wins: no level yet");
    let (winner, shots) = play_out(&mut a, &mut b, game, sa).await;
    let (winner_id, loser_id) = if winner == sa { (ids[0], ids[1]) } else { (ids[1], ids[0]) };
    let counted = shots >= 8;
    let want = if counted { 10 } else { 9 };
    // Counted once the game is recorded (in the background).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let w = server.store.wallet(winner_id).await.unwrap();
        if w.cue_wins.get("cue_classic") == Some(&want) {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "the win was not counted: {w:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let loser = server.store.wallet(loser_id).await.unwrap();
    assert_eq!(loser.cue_wins.get("cue_classic"), Some(&9), "a loss doesn't count");
    if !counted {
        return;
    }
    // The loser's cue counts the game played (nine wins, ten games).
    loop {
        let w = server.store.wallet(loser_id).await.unwrap();
        if w.cue_games.get("cue_classic").is_some_and(|g| g.0 == 10) {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "the game was not counted: {w:?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // Everyone sees the level: on the profile and on the player card.
    let winner_name = if winner == sa { "alice" } else { "bob" };
    a.send(ClientMsg::Profile { username: winner_name.into() }).await;
    let ServerMsg::Profile { profile } =
        a.expect("profile", |m| matches!(m, ServerMsg::Profile { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(profile.player.cue_level, 1);
    assert_eq!(profile.cue_wins, vec![("cue_classic".to_string(), 10)]);
    let stats = &profile.cue_stats;
    assert!(stats.iter().any(|c| c.code == "cue_classic" && c.games == 10 && c.wins == 10));
    // The winner's wallet says so too.
    let w = server.store.wallet(winner_id).await.unwrap();
    assert_eq!(snook_legend_server::shop::cue_level_of(&w, "cue_classic"), 1);
}

/// Wins with one cue stay with that cue: changing cues starts another count.
#[tokio::test]
async fn wins_belong_to_each_cue() {
    let (_url, server) = start().await;
    let user = server
        .store
        .create_user(&NewUser {
            username: "carol",
            password_hash: "h",
            country: "BR",
            rating: 1200,
            role: Role::Player,
            ip: "1.2.3.4",
            minor: None,
        })
        .await
        .unwrap()
        .unwrap();
    for _ in 0..3 {
        snook_legend_server::shop::count_cue_win(&server, &user).await;
    }
    server.store.credit(user.id, 1000, "test", None, now()).await.unwrap();
    server.store.buy_item(user.id, "cue_maple", 750, now()).await.unwrap();
    server.store.equip(user.id, "cue", "cue_maple").await.unwrap();
    snook_legend_server::shop::count_cue_win(&server, &user).await;
    let w = server.store.wallet(user.id).await.unwrap();
    assert_eq!(w.cue_wins.get("cue_classic"), Some(&3));
    assert_eq!(w.cue_wins.get("cue_maple"), Some(&1));
    // A cue no longer allowed (a VIP cue after VIP ended) never counts:
    // the win goes to the cue actually in use, the default.
    server.store.equip(user.id, "cue", "cue_royal").await.unwrap();
    snook_legend_server::shop::count_cue_win(&server, &user).await;
    let w = server.store.wallet(user.id).await.unwrap();
    assert_eq!(w.cue_wins.get("cue_classic"), Some(&4));
    assert_eq!(w.cue_wins.get("cue_royal"), None);
}

/// Tables and halls are sold, equipped and checked like every other item.
#[tokio::test]
async fn tables_and_halls_are_bought_and_used() {
    let (url, server) = start().await;
    let mut a = Client::register(&url, "dave").await;
    let id = server.store.user_by_name("dave").await.unwrap().unwrap().id;
    let has = |equipped: &[(String, String)], c: &str, i: &str| {
        equipped.iter().any(|(x, y)| x == c && y == i)
    };

    // Not owned yet, too few coins.
    a.send(ClientMsg::Equip { code: "hall_club".into() }).await;
    assert_eq!(a.error().await, "You don't have this item.");
    a.send(ClientMsg::BuyItem { code: "table_club".into() }).await;
    assert_eq!(a.error().await, "Not enough coins.");

    server.store.credit(id, 20_000, "test", None, now()).await.unwrap();
    a.send(ClientMsg::BuyItem { code: "table_club".into() }).await;
    a.expect("bought", |m| matches!(m, ServerMsg::ItemBought { code, .. } if code == "table_club"))
        .await;
    let (owned, equipped, _) = a.wallet_where(|_, e| has(e, "table", "table_club")).await;
    assert!(owned.contains(&"table_club".to_string()), "bought and put on");
    assert!(!equipped.iter().any(|(c, _)| c == "hall"));
    a.send(ClientMsg::BuyItem { code: "hall_beach".into() }).await;
    a.wallet_where(|_, e| has(e, "hall", "hall_beach")).await;
    let w = server.store.wallet(id).await.unwrap();
    // (Less the prices; the daily bonus may have come on signing in.)
    let spent = 20_000 - w.coins;
    assert!(
        [6250 + 4500, 6250 + 4500 - snook_legend_server::account::DAILY_BONUS].contains(&spent),
        "{spent}"
    );
    // The free defaults can always be used.
    a.send(ClientMsg::Equip { code: "hall_pool".into() }).await;
    a.wallet_where(|_, e| has(e, "hall", "hall_pool")).await;

    // The VIP table only for VIP members (another connection: the shop
    // takes a few messages at a time).
    let mut b = Client::login(&url, "dave").await;
    b.send(ClientMsg::Equip { code: "table_royal".into() }).await;
    assert_eq!(b.error().await, "You don't have this item.");
    b.send(ClientMsg::BuyItem { code: "table_royal".into() }).await;
    assert_eq!(b.error().await, "This item can't be bought with coins.");
    server.store.set_vip(id, now() + 3600, true).await.unwrap();
    b.send(ClientMsg::Equip { code: "table_royal".into() }).await;
    b.wallet_where(|_, e| has(e, "table", "table_royal")).await;
    // VIP over: the classic table is the one in use again.
    server.store.set_vip(id, now() - 10, true).await.unwrap();
    let w = server.store.wallet(id).await.unwrap();
    assert_eq!(
        snook_legend_server::shop::effective(&w, &[], Category::Table, now()),
        "table_classic"
    );
    assert_eq!(snook_legend_server::shop::effective(&w, &[], Category::Hall, now()), "hall_pool");
}

/// The prices of tables and halls sit with the dearest items of the shop.
#[test]
fn table_and_hall_prices_fit_the_economy() {
    let dearest = shop::ITEMS
        .iter()
        .filter(|i| !matches!(i.category, Category::Table | Category::Hall))
        .filter_map(|i| i.coins())
        .max()
        .unwrap();
    for i in shop::ITEMS.iter().filter(|i| matches!(i.category, Category::Table | Category::Hall)) {
        if let Some(price) = i.coins() {
            assert!((3000..=dearest).contains(&price), "{}: {price}", i.code);
        }
    }
}

async fn exercise(store: Store) {
    let user = store
        .create_user(&NewUser {
            username: "erin",
            password_hash: "h",
            country: "PT",
            rating: 1200,
            role: Role::Player,
            ip: "1.2.3.4",
            minor: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert!(store.wallet(user.id).await.unwrap().cue_wins.is_empty());
    assert_eq!(store.add_cue_win(user.id, "cue_maple").await.unwrap(), 1);
    assert_eq!(store.add_cue_win(user.id, "cue_maple").await.unwrap(), 2);
    assert_eq!(store.add_cue_win(user.id, "cue_carbon").await.unwrap(), 1);
    let w = store.wallet(user.id).await.unwrap();
    assert_eq!(w.cue_wins.get("cue_maple"), Some(&2));
    assert_eq!(w.cue_wins.get("cue_carbon"), Some(&1));
    assert_eq!(w.cue_wins.len(), 2);
    // Games are counted with the balls made, won or lost.
    assert_eq!(store.add_cue_game(user.id, "cue_maple", false, 4).await.unwrap(), 2);
    assert_eq!(store.add_cue_game(user.id, "cue_maple", true, 7).await.unwrap(), 3);
    let w = store.wallet(user.id).await.unwrap();
    assert_eq!(w.cue_games.get("cue_maple"), Some(&(4, 11)));
    assert_eq!(w.cue_wins.get("cue_maple"), Some(&3));
    // Equipped tables and halls are kept by category.
    store.equip(user.id, "table", "table_beach").await.unwrap();
    store.equip(user.id, "hall", "hall_loft").await.unwrap();
    store.equip(user.id, "table", "table_modern").await.unwrap();
    let w = store.wallet(user.id).await.unwrap();
    assert_eq!(w.equipped.get("table").map(String::as_str), Some("table_modern"));
    assert_eq!(w.equipped.get("hall").map(String::as_str), Some("hall_loft"));
}

#[tokio::test]
async fn memory_store() {
    exercise(Store::memory()).await;
}

#[tokio::test]
async fn postgres_store() {
    let Ok(url) = std::env::var("SNOOK_LEGEND_TEST_DATABASE_URL") else {
        eprintln!("SNOOK_LEGEND_TEST_DATABASE_URL not set: skipping");
        return;
    };
    {
        use sqlx::Executor;
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        pool.execute("DROP SCHEMA public CASCADE; CREATE SCHEMA public;").await.unwrap();
    }
    exercise(Store::postgres(&url, 3).await.unwrap()).await;
}
