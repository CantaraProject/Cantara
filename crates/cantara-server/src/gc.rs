//! Removing file content nothing refers to any more.
//!
//! Content is kept once per hash and may be shared by many things: versions
//! of library items, the pictures of shared presentations, design packages.
//! When the last of them goes — a presentation ends, an item's last version
//! is deleted with its library — the content stays behind. This finds it and
//! deletes it, once an hour.
//!
//! The race to be careful of is content stored again just as it is
//! collected: an upload finds the file already there, skips writing it, and
//! is about to refer to it while it is deleted. Two things close that. Every
//! store refreshes the content's `created_at` in the same transaction that
//! refers to it, and only content untouched for [`MINIMUM_AGE`] is a
//! candidate. And the file is only removed after the row is gone and has
//! *stayed* gone for a moment, so that a store that put it back in the
//! meantime is seen.

use std::time::Duration;

use crate::error::AppResult;
use crate::state::AppState;

/// Content younger than this is never collected.
pub const MINIMUM_AGE: Duration = Duration::from_secs(60 * 60);

/// How long a removed row has to stay removed before its file goes.
const SETTLE: Duration = Duration::from_secs(10);

/// Collects content unused for at least `minimum_age`. Returns how many files
/// were removed.
pub async fn collect(state: &AppState, minimum_age: Duration, settle: Duration) -> AppResult<usize> {
    let seconds = minimum_age.as_secs_f64();
    let gone: Vec<String> = sqlx::query_scalar(
        "DELETE FROM blobs b
          WHERE b.created_at < now() - make_interval(secs => $1)
            AND NOT EXISTS (SELECT 1 FROM item_versions v WHERE v.sha256 = b.sha256)
            AND NOT EXISTS (SELECT 1 FROM session_media m WHERE m.sha256 = b.sha256)
            AND NOT EXISTS (SELECT 1 FROM settings_versions s WHERE s.package_sha256 = b.sha256)
         RETURNING b.sha256",
    )
    .bind(seconds)
    .fetch_all(&state.db)
    .await?;
    if gone.is_empty() {
        return Ok(0);
    }

    tokio::time::sleep(settle).await;
    let mut removed = 0;
    for sha256 in gone {
        let back: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM blobs WHERE sha256 = $1)")
            .bind(&sha256)
            .fetch_one(&state.db)
            .await?;
        if back {
            continue;
        }
        match state.blobs.remove(&sha256).await {
            Ok(true) => removed += 1,
            Ok(false) => {}
            Err(error) => tracing::warn!("{sha256} could not be removed: {error}"),
        }
    }
    Ok(removed)
}

/// What [`remove_sessions`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SessionsRemoved {
    /// Presentations nobody ended, ended for being idle.
    pub ended: usize,
    /// Ended presentations deleted from the database.
    pub deleted: usize,
}

/// Ends presentations that have not changed for
/// [`Config::idle_session_days`](crate::config::Config) — whoever presented
/// them closed the laptop rather than the presentation — and deletes the
/// ones ended longer ago than
/// [`Config::keep_ended_sessions_days`](crate::config::Config), with what was
/// shared of them. Their pictures and videos are then content nothing refers
/// to, which [`collect`] takes.
pub async fn remove_sessions(state: &AppState) -> AppResult<SessionsRemoved> {
    let idle: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT uuid FROM sessions
          WHERE ended_at IS NULL AND last_active < now() - make_interval(days => $1)",
    )
    .bind(state.config.idle_session_days as i32)
    .fetch_all(&state.db)
    .await?;
    for session in &idle {
        // Ended the way its owner would end it: whoever is still connected
        // is told.
        state.live.end(&state.db, *session).await?;
    }

    let mut tx = state.db.begin().await?;
    let deleted: Vec<uuid::Uuid> = sqlx::query_scalar(
        "DELETE FROM sessions
          WHERE ended_at IS NOT NULL AND ended_at <= now() - make_interval(days => $1)
         RETURNING uuid",
    )
    .bind(state.config.keep_ended_sessions_days as i32)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM grants WHERE resource_kind = 'session' AND resource_uuid = ANY($1)")
        .bind(&deleted)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(SessionsRemoved {
        ended: idle.len(),
        deleted: deleted.len(),
    })
}

/// Tidies up once an hour, for as long as the server runs: old
/// presentations first, then the content nothing refers to any more.
pub fn run_periodically(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60 * 60));
        loop {
            interval.tick().await;
            match remove_sessions(&state).await {
                Ok(SessionsRemoved { ended: 0, deleted: 0 }) => {}
                Ok(removed) => tracing::info!(
                    "ended {} idle presentations, deleted {} ended ones",
                    removed.ended,
                    removed.deleted
                ),
                Err(error) => tracing::warn!("removing old presentations failed: {error}"),
            }
            match collect(&state, MINIMUM_AGE, SETTLE).await {
                Ok(0) => {}
                Ok(removed) => tracing::info!("removed {removed} files nothing refers to"),
                Err(error) => tracing::warn!("collecting unused files failed: {error}"),
            }
        }
    });
}
