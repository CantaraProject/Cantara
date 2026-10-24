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
        match tokio::fs::remove_file(state.blobs.path_of(&sha256)).await {
            Ok(()) => removed += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!("{sha256} could not be removed: {error}"),
        }
    }
    Ok(removed)
}

/// Collects once an hour, for as long as the server runs.
pub fn run_periodically(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60 * 60));
        loop {
            interval.tick().await;
            match collect(&state, MINIMUM_AGE, SETTLE).await {
                Ok(0) => {}
                Ok(removed) => tracing::info!("removed {removed} files nothing refers to"),
                Err(error) => tracing::warn!("collecting unused files failed: {error}"),
            }
        }
    });
}
