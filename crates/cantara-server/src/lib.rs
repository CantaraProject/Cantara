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

/// Where file content is kept, as configured.
fn open_storage(config: &Config) -> Result<BlobStore, String> {
    use std::sync::Arc;

    let receiving = |error: std::io::Error| format!("cannot use {}: {error}", config.blob_dir.display());
    match &config.storage {
        config::StorageConfig::Directory => BlobStore::open(&config.blob_dir).map_err(receiving),
        config::StorageConfig::Memory => {
            BlobStore::object(Arc::new(object_store::memory::InMemory::new()), "", &config.blob_dir.join("tmp")).map_err(receiving)
        }
        config::StorageConfig::S3 {
            bucket,
            region,
            endpoint,
            access_key_id,
            secret_access_key,
            prefix,
            path_style,
        } => {
            // What the environment says the AWS way (`AWS_*`) is the base;
            // Cantara's own variables win.
            let mut builder = object_store::aws::AmazonS3Builder::from_env().with_bucket_name(bucket);
            if let Some(region) = region {
                builder = builder.with_region(region);
            }
            if let Some(endpoint) = endpoint {
                builder = builder
                    .with_endpoint(endpoint)
                    .with_allow_http(endpoint.starts_with("http://"));
            }
            if let Some(key) = access_key_id {
                builder = builder.with_access_key_id(key);
            }
            if let Some(secret) = secret_access_key {
                builder = builder.with_secret_access_key(secret);
            }
            let store = builder
                .with_virtual_hosted_style_request(!path_style)
                .build()
                .map_err(|error| format!("the S3 storage cannot be used: {error}"))?;
            BlobStore::object(Arc::new(store), prefix, &config.blob_dir.join("tmp")).map_err(receiving)
        }
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
    let blobs = open_storage(&config)?;
    tracing::info!("keeping file content in {}", blobs.describe());
    bootstrap::run(&db, &config)
        .await
        .map_err(|error| format!("cannot prepare the server: {error}"))?;
    Ok(AppState::new(config, db, blobs))
}
