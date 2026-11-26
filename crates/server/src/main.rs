//! Snook Legend server executable.
//!
//! Configuration (environment variables):
//!
//! | Variable | Default | Meaning |
//! |---|---|---|
//! | `PORT` / `SNOOK_LEGEND_BIND` | `0.0.0.0:7878` | port (`PORT`, as set by hosts like Render) or full address |
//! | `DATABASE_URL` | — | PostgreSQL connection string (required unless `SNOOK_LEGEND_IN_MEMORY=1`) |
//! | `SNOOK_LEGEND_DB_POOL` | `5` | database connections |
//! | `SNOOK_LEGEND_IN_MEMORY` | — | `1`: keep everything in memory (development only) |
//! | `SNOOK_LEGEND_ADMINS` | — | comma-separated usernames that become administrators |
//! | `SNOOK_LEGEND_CLIENT_IP_HEADER` | — | headers with the real client address behind a proxy, tried in order (e.g. `cf-connecting-ip,true-client-ip,x-forwarded-for`) |
//! | `SNOOK_LEGEND_PROXY_HOPS` | `1` | proxies that append to `X-Forwarded-For` |
//! | `SNOOK_LEGEND_TRUSTED_PROXIES` | private networks | where the proxy connects from (`10.0.0.0/8,…`; `*`: anywhere): the address headers of anyone else are ignored |
//! | `SNOOK_LEGEND_MOTD` | a welcome line | message shown in the lounge (first value: the portal's settings change it live) |
//! | `SNOOK_LEGEND_SHOT_CLOCK` | `30` | seconds per shot (first value: see the portal's settings) |
//! | `SNOOK_LEGEND_STRIPE_SECRET` | — | Stripe secret key (payments; see `payments.rs`) |
//! | `SNOOK_LEGEND_STRIPE_WEBHOOK_SECRET` | — | Stripe webhook signing secret |
//! | `SNOOK_LEGEND_MSSTORE_TENANT_ID`, `_CLIENT_ID`, `_CLIENT_SECRET` | — | Microsoft Entra app for Store purchases (see `msstore.rs`) |
//! | `SNOOK_LEGEND_MSSTORE_PRODUCTS` | — | `coins_500=9XXXXXXXXXXX;…;vip_month=9XXXXXXXXXXX`: the Store add-on of each coin pack and of the month of VIP |
//! | `SNOOK_LEGEND_STRIPE_PRICES` | — | `code=price_id\|label;…` for coin packs and `vip_monthly` |
//! | `SNOOK_LEGEND_PUBLIC_URL` | — | the server's public address (payment return pages) |
//! | `SNOOK_LEGEND_SITE_URL` | `https://snooklegend.app` | the game's website, with the terms of use and the privacy policy: the server's old addresses for them (`/termos`, `/privacy`…) and `/` lead there |
//! | `SNOOK_LEGEND_ADMIN_ORIGIN` | — | the website's origin, where the staff portal is (`https://snooklegend.app`, pages `/admin`): the only one allowed to call `/admin/api` from a browser; `/admin` here leads there |
//! | `SNOOK_LEGEND_DRAIN_SECS` | `240` | on SIGTERM: how long games in progress may go on before they are handed to the next server |
//! | `SNOOK_LEGEND_CUP` | — | the weekly cup's first settings, e.g. `sat 20:00` (options: `60m`, `fee=100`, `utc-3`, `official` for the official rules); the portal's settings change it live |
//! | `RUST_LOG` | `info` | log filter |

use std::time::Duration;

use snook_legend_server::lounge::Settings;
use snook_legend_server::{Config, Server, serve};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let mut settings = Settings::default();
    if let Some(motd) = env("SNOOK_LEGEND_MOTD") {
        settings.motd = motd;
    }
    if let Some(secs) = env("SNOOK_LEGEND_SHOT_CLOCK").and_then(|s| s.parse::<u64>().ok()) {
        settings.shot_clock = Duration::from_secs(secs.clamp(10, 120));
    }
    if let Some(cup) = env("SNOOK_LEGEND_CUP") {
        settings.cup = snook_legend_server::lounge::Cup::parse(&cup);
        match &settings.cup {
            Some(c) => tracing::info!("weekly cup: {c:?}"),
            None => tracing::warn!("SNOOK_LEGEND_CUP not understood: {cup}"),
        }
    }
    let drain_secs =
        env("SNOOK_LEGEND_DRAIN_SECS").and_then(|s| s.parse::<u64>().ok()).unwrap_or(240);
    let bind = match (env("SNOOK_LEGEND_BIND"), env("PORT")) {
        (Some(b), _) => b,
        (None, Some(port)) => format!("0.0.0.0:{port}"),
        (None, None) => "0.0.0.0:7878".into(),
    };
    let in_memory = env("SNOOK_LEGEND_IN_MEMORY").is_some_and(|v| v == "1" || v == "true");
    let database_url = env("DATABASE_URL");
    if database_url.is_none() && !in_memory {
        eprintln!(
            "DATABASE_URL is not set (use SNOOK_LEGEND_IN_MEMORY=1 to run without a database)"
        );
        std::process::exit(1);
    }
    let config = Config {
        bind: bind.parse().expect("SNOOK_LEGEND_BIND must be host:port"),
        database_url: if in_memory { None } else { database_url },
        database_pool: env("SNOOK_LEGEND_DB_POOL").and_then(|v| v.parse().ok()).unwrap_or(5),
        settings,
        admins: env("SNOOK_LEGEND_ADMINS")
            .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default(),
        client_ip_header: env("SNOOK_LEGEND_CLIENT_IP_HEADER").map(|h| h.to_lowercase()),
        proxy_hops: env("SNOOK_LEGEND_PROXY_HOPS").and_then(|v| v.parse().ok()).unwrap_or(1),
        trusted_proxies: env("SNOOK_LEGEND_TRUSTED_PROXIES").map(|v| {
            let networks = snook_legend_server::parse_networks(&v);
            // An entry that can't be read is a proxy not trusted: with none
            // left, every player would share the proxy's address (and its
            // limits). Said out loud, not left to be found out.
            let asked = v.split(',').filter(|s| !s.trim().is_empty()).count();
            if networks.len() < asked {
                eprintln!(
                    "warning: SNOOK_LEGEND_TRUSTED_PROXIES: {} of {asked} entries could not be read",
                    asked - networks.len()
                );
            }
            networks
        }),
        stripe: snook_legend_server::payments::StripeConfig::from_env(),
        msstore: snook_legend_server::msstore::MsStoreConfig::from_env(),
        site_url: match env("SNOOK_LEGEND_SITE_URL") {
            Some(v) if v.starts_with("https://") || v.starts_with("http://") => {
                v.trim().trim_end_matches('/').to_string()
            }
            Some(_) => {
                eprintln!("warning: SNOOK_LEGEND_SITE_URL must start with https://: ignored");
                snook_legend_server::pages::SITE_URL.into()
            }
            None => snook_legend_server::pages::SITE_URL.into(),
        },
        admin_origin: match env("SNOOK_LEGEND_ADMIN_ORIGIN") {
            Some(v) if v.starts_with("https://") || v.starts_with("http://") => {
                Some(v.trim().trim_end_matches('/').to_ascii_lowercase())
            }
            Some(_) => {
                eprintln!("warning: SNOOK_LEGEND_ADMIN_ORIGIN must start with https://: ignored");
                None
            }
            None => None,
        },
        ..Config::default()
    };
    match &config.admin_origin {
        Some(origin) => tracing::info!("staff API open to the portal at {origin}"),
        None => tracing::info!("staff API: no portal origin (SNOOK_LEGEND_ADMIN_ORIGIN not set)"),
    }
    if env("SNOOK_LEGEND_ADMIN_HOST").is_some() {
        tracing::warn!(
            "SNOOK_LEGEND_ADMIN_HOST is no longer used: the portal is its own site; set SNOOK_LEGEND_ADMIN_ORIGIN"
        );
    }
    match &config.stripe {
        Some(s) => tracing::info!("payments on ({} prices)", s.prices.len()),
        None => tracing::info!("payments off (SNOOK_LEGEND_STRIPE_* not set)"),
    }
    match &config.msstore {
        Some(m) => {
            let packs = m.offers().len();
            let vip = if m.vip_offer().is_empty() { "" } else { ", VIP month" };
            tracing::info!("Microsoft Store purchases on ({packs} packs{vip})");
        }
        None => tracing::info!("Microsoft Store purchases off (SNOOK_LEGEND_MSSTORE_* not set)"),
    }
    if in_memory {
        tracing::warn!("running in memory: accounts and games are lost when the server stops");
    }
    let bind = config.bind;
    let server = match Server::new(config).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not start: {e}");
            std::process::exit(1);
        }
    };
    let listener = TcpListener::bind(bind).await.expect("could not bind the server address");
    tracing::info!(
        "{} server {} listening on {bind}",
        snook_legend_proto::GAME_NAME,
        env!("CARGO_PKG_VERSION")
    );
    let shutdown = async {
        let ctrl_c = tokio::signal::ctrl_c();
        #[cfg(unix)]
        {
            let mut term =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("signal handler");
            tokio::select! {
                _ = ctrl_c => {}
                _ = term.recv() => {}
            }
        }
        #[cfg(not(unix))]
        let _ = ctrl_c.await;
    };
    let serving = tokio::spawn(serve(listener, server.clone()));
    let maintenance = server.clone();
    tokio::select! {
        result = serving => {
            if let Ok(Err(e)) = result {
                tracing::error!("server stopped: {e}");
            }
        }
        _ = shutdown => {
            // Keep serving while the games in progress finish.
            tracing::info!("shutting down: draining for up to {drain_secs} s");
            snook_legend_server::drain(server, drain_secs).await;
        }
        // A maintenance scheduled in the portal: the same drain, then the
        // process ends (cleanly) and the host starts it again.
        _ = maintenance.ops.maintenance.due() => {
            tracing::info!("maintenance: draining for up to {drain_secs} s, then stopping");
            snook_legend_server::drain(server, drain_secs).await;
        }
    }
}
