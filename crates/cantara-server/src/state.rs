//! What every request handler has access to.

use std::ops::Deref;
use std::sync::Arc;

use sqlx::PgPool;

use crate::blobs::BlobStore;
use crate::config::Config;
use crate::limiter::Limiter;
use crate::live::LiveSessions;
use crate::oidc::OidcClients;

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

pub struct Inner {
    pub config: Config,
    pub db: PgPool,
    pub blobs: BlobStore,
    /// The presentations running on this server, with whoever is connected.
    pub live: LiveSessions,
    /// Counts failed logins, so that a password cannot be guessed at speed.
    pub limiter: Limiter,
    pub oidc: OidcClients,
    pub mailer: crate::mail::Mailer,
}

impl AppState {
    pub fn new(config: Config, db: PgPool, blobs: BlobStore) -> AppState {
        let oidc = OidcClients::new(config.oidc_providers.clone());
        // Checked in `prepare` already; a configuration that got this far
        // builds.
        let mailer = crate::mail::Mailer::new(&config.mail).unwrap_or(crate::mail::Mailer::Off);
        AppState(Arc::new(Inner {
            mailer,
            live: LiveSessions::default(),
            limiter: Limiter::default(),
            oidc,
            config,
            db,
            blobs,
        }))
    }
}

impl Deref for AppState {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}
