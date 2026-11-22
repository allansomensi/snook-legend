//! The terms of use and the privacy policy live on the game's website (see
//! the `snooklegend.app` repository); the server's old addresses for them,
//! which the stores, Stripe and older games link to, lead there:
//!
//! | Here | On the website |
//! |---|---|
//! | `/privacidade`, `/privacy`, `/privacidad` | `/pt/privacidade/`, `/privacy/`, `/es/privacidad/` |
//! | `/termos`, `/terms`, `/terminos` | `/pt/termos/`, `/terms/`, `/es/terminos/` |
//!
//! The website is [`crate::Config::site_url`].

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;

use crate::Server;

/// The game's website.
pub const SITE_URL: &str = "https://snooklegend.app";

/// Each old address and the page it leads to on the website.
pub const PAGES: [(&str, &str); 6] = [
    ("/privacidade", "/pt/privacidade/"),
    ("/privacy", "/privacy/"),
    ("/privacidad", "/es/privacidad/"),
    ("/termos", "/pt/termos/"),
    ("/terms", "/terms/"),
    ("/terminos", "/es/terminos/"),
];

/// A permanent redirect to `path` on the website.
fn to_site(server: &Server, path: &str) -> Response {
    let site = server.config.site_url.trim_end_matches('/');
    Redirect::permanent(&format!("{site}{path}")).into_response()
}

/// The routes: the old addresses, and the root, which leads to the website.
pub fn router() -> Router<Arc<Server>> {
    let mut router = Router::new()
        .route("/", get(|State(server): State<Arc<Server>>| async move { to_site(&server, "/") }));
    for (old, new) in PAGES {
        router = router.route(
            old,
            get(move |State(server): State<Arc<Server>>| async move { to_site(&server, new) }),
        );
    }
    router
}
