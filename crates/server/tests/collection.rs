//! The collection: how and when each item was got (migration 0029), the
//! items that come with achievements, and the items that stay with whoever
//! won them. The in-memory store is always tested; PostgreSQL when
//! `SNOOK_LEGEND_TEST_DATABASE_URL` points at an empty database.

use std::sync::{Arc, Mutex};

use snook_legend_proto::collection::Origin;
use snook_legend_proto::{ClientMsg, Role, ServerMsg};
use snook_legend_server::store::{BuyOutcome, NewGift, NewUser, Store};
use snook_legend_server::{Config, Server, collection};

fn new_user(name: &str) -> NewUser<'_> {
    NewUser {
        username: name,
        password_hash: "h",
        country: "BR",
        rating: 1200,
        role: Role::Player,
        ip: "1.2.3.4",
        minor: None,
    }
}

async fn exercise(store: &Store) {
    let t = 1_800_000_000;
    let a = store.create_user(&new_user("col_a")).await.unwrap().unwrap();
    let b = store.create_user(&new_user("col_b")).await.unwrap().unwrap();
    store.credit(a.id, 5000, "test", None, t).await.unwrap();
    assert_eq!(store.buy_item(a.id, "cloth_blue", 1000, t).await.unwrap(), BuyOutcome::Bought);
    let gift = NewGift { from: a.id, to: b.id, code: "cue_ocean", message: "", at: t + 5 };
    assert_eq!(store.give_item(&gift, 3000).await.unwrap().0, BuyOutcome::Bought);
    assert!(store.grant_item(b.id, "cue_ebony", "staff", t + 9).await.unwrap());

    // What was owned before origins were kept: worked out from how it came.
    let rows = store.owned_items(a.id).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].code.as_str(), rows[0].acquired_at), ("cloth_blue", t));
    assert_eq!(collection::origin_of(&rows[0]), Origin::Bought { price: 1000 });
    let rows = store.owned_items(b.id).await.unwrap();
    let ocean = rows.iter().find(|r| r.code == "cue_ocean").unwrap();
    assert_eq!(ocean.giver.as_deref(), Some("col_a"));
    assert_eq!(collection::origin_of(ocean), Origin::Gift { from: "col_a".into() });
    assert_eq!(ocean.acquired_at, t + 5);
    let ebony = rows.iter().find(|r| r.code == "cue_ebony").unwrap();
    assert_eq!(collection::origin_of(ebony), Origin::Staff { reason: String::new() });

    // Recorded: kept with the item, the first origin stays.
    let won = r#"{"how":"tournament","name":"Winter Cup 2026","place":1}"#;
    assert!(!store.record_origin(b.id, "cue_ebony", won, t + 20).await.unwrap());
    assert!(!store.record_origin(b.id, "cue_ebony", r#"{"how":"welcome_pack"}"#, t).await.unwrap());
    let rows = store.owned_items(b.id).await.unwrap();
    let ebony = rows.iter().find(|r| r.code == "cue_ebony").unwrap();
    assert_eq!(
        collection::origin_of(ebony),
        Origin::Tournament { name: "Winter Cup 2026".into(), place: 1 }
    );
    assert_eq!(ebony.acquired_at, t + 9, "when it was got stays too");
    // Recording an item not owned yet gives it.
    let month = r#"{"how":"vip_month","month":"2026-12"}"#;
    assert!(store.record_origin(a.id, "vip_month_2026_12", month, t + 30).await.unwrap());
    assert!(store.wallet(a.id).await.unwrap().owned.iter().any(|o| o == "vip_month_2026_12"));
    let rows = store.owned_items(a.id).await.unwrap();
    assert_eq!(rows.last().map(|r| r.code.as_str()), Some("vip_month_2026_12"));
    // Taken back (a refund): its origin goes with it.
    assert_eq!(store.revoke_items(a.id, &["cloth_blue"]).await.unwrap(), 1);
    assert!(store.owned_items(a.id).await.unwrap().iter().all(|r| r.code != "cloth_blue"));
}

#[tokio::test]
async fn memory_store() {
    exercise(&Store::memory()).await;
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
    exercise(&Store::postgres(&url, 3).await.unwrap()).await;
}

async fn server_with_player(name: &str) -> (Arc<Server>, snook_legend_server::store::User) {
    let server = Server::new(Config::default()).await.unwrap();
    let user = server.store.create_user(&new_user(name)).await.unwrap().unwrap();
    (server, user)
}

/// The whole collection: what is owned, what came with achievements (and
/// their reactions), each with how and when.
#[tokio::test]
async fn the_collection_lists_items_and_achievements() {
    let (server, me) = server_with_player("collector").await;
    let t = 1_800_000_000;
    let origin = Origin::Event { name: "Christmas 2026".into() };
    assert!(collection::give(&server, me.id, "cue_christmas", origin.clone()).await);
    assert!(!collection::give(&server, me.id, "cue_christmas", Origin::Unknown).await);
    server.store.add_achievements(me.id, &["streak_30"], t).await.unwrap();
    let items = collection::items_of(&server, me.id).await;
    let find = |code: &str| items.iter().find(|i| i.code == code);
    assert_eq!(find("cue_christmas").map(|i| &i.origin), Some(&origin));
    // The ruby medal's cue and frame, from the day it was won.
    for code in ["cue_untouchable", "frame_ruby"] {
        let item = find(code).unwrap_or_else(|| panic!("{code}"));
        assert_eq!(item.origin, Origin::Achievement { code: "streak_30".into() });
        assert_eq!(item.acquired_at, t);
    }
    // Asked for by the game.
    let sent = Arc::new(Mutex::new(Vec::new()));
    let out = sent.clone();
    collection::handle(&server, &me, ClientMsg::Collection, &move |m| out.lock().unwrap().push(m))
        .await;
    let sent = sent.lock().unwrap();
    assert!(matches!(&sent[..], [ServerMsg::Collection { items }] if items.len() == 3));
}

/// What stays with whoever won it can't be given to a friend.
#[tokio::test]
async fn won_items_cant_be_given() {
    let (server, me) = server_with_player("giver").await;
    let errors = Arc::new(Mutex::new(Vec::new()));
    for code in ["cue_season_champion", "cue_untouchable", "effect_crown"] {
        let out = errors.clone();
        let msg =
            ClientMsg::GiftItem { to: "nobody".into(), code: code.into(), message: String::new() };
        snook_legend_server::shop::handle(&server, &me, msg, &move |m| {
            if let ServerMsg::Error { message } = m {
                out.lock().unwrap().push(message);
            }
        })
        .await;
    }
    let errors = errors.lock().unwrap();
    assert_eq!(errors.len(), 3);
    assert!(errors.iter().all(|e| e.contains("stays with whoever won it")), "{errors:?}");
}
