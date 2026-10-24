//! The Cantara server: libraries, designs, running orders and presentations
//! shared between the Cantara programs of one organisation.
//!
//! Licensed under the AGPL-3.0-or-later. See
//! `docs/specs/0008-server-version.md` for what it is and why it is built so,
//! and `deploy/` for how to run it.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
// Rows are read as tuples, in the order the query names its columns, right
// beside the query. Naming each tuple type would put the column list in two
// places.
#![allow(clippy::type_complexity)]

pub mod api;
pub mod auth;
pub mod blobs;
pub mod bootstrap;
pub mod config;
pub mod device;
pub mod error;
pub mod gc;
pub mod invitations;
pub mod limiter;
pub mod mail;
pub mod live;
pub mod oidc;
pub mod pages;
pub mod secrets;
pub mod songs;
pub mod state;
pub mod store;

use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::blobs::BlobStore;
use crate::config::Config;
use crate::state::AppState;

/// Every route the server answers, with `state` in it.
pub fn app(state: AppState) -> Router {
    let body_limit = usize::try_from(state.config.max_upload_bytes)
        .unwrap_or(usize::MAX)
        .saturating_add(1024 * 1024);
    let web_dir = state.config.web_dir.clone();

    let router = Router::new()
        .nest(cantara_api::API_PREFIX, api::routes())
        .route("/login", get(pages::login_page).post(pages::login_submit))
        .route("/register", get(pages::register_page).post(pages::register_submit))
        .route("/invite/{token}", get(pages::invite_page).post(pages::invite_accept))
        .route("/authorize", get(pages::authorize_page).post(pages::authorize_submit))
        .route("/device", get(device::device_page).post(device::device_submit))
        .route("/auth/oidc/callback", get(oidc::callback))
        .route("/auth/oidc/{provider}", get(oidc::start))
        .route("/auth/oidc/t/{tenant}/{provider}", get(oidc::start_tenant))
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready));

    // The web build, where there is one. Every path it does not have a file
    // for is its own route — `/settings`, `/s/{token}` — so it gets the page
    // and decides for itself.
    let router = match web_dir {
        Some(dir) => {
            let index = dir.join("index.html");
            router.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)))
        }
        None => router,
    };

    router
        .layer(DefaultBodyLimit::max(body_limit))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Ready when the database answers.
async fn ready(State(state): State<AppState>) -> StatusCode {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Connects, migrates, prepares a fresh server, and returns the state to serve
/// from.
pub async fn prepare(config: Config) -> Result<AppState, String> {
    let db = PgPoolOptions::new()
        .max_connections(20)
        .connect(&config.database_url)
        .await
        .map_err(|error| format!("cannot reach the database: {error}"))?;
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .map_err(|error| format!("cannot migrate the database: {error}"))?;
    mail::Mailer::new(&config.mail)?;
    let blobs = BlobStore::open(&config.blob_dir)
        .map_err(|error| format!("cannot use {}: {error}", config.blob_dir.display()))?;
    bootstrap::run(&db, &config)
        .await
        .map_err(|error| format!("cannot prepare the server: {error}"))?;
    Ok(AppState::new(config, db, blobs))
}
