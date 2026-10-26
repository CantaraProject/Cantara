//! Presentations running on the server: their state, and everybody connected
//! to them.
//!
//! The state is the program's `RunningPresentation` as JSON, and the server
//! never looks inside it. What it does is keep the latest one, number every
//! change, hand it to whoever connects, and pass each change on to everybody
//! else — as the top-level fields that changed, so that "next slide" is a few
//! bytes and not the whole service. Every change is also written to the
//! database, so a restart of the server does not lose a service in progress.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Map, Value};
use sqlx::PgPool;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// What every connection to a session is told.
#[derive(Clone, Debug)]
pub enum Event {
    Replaced { revision: i64, state: Value },
    Patched { revision: i64, changes: Map<String, Value> },
    Viewers(usize),
    Ended,
}

pub struct Live {
    pub uuid: Uuid,
    current: Mutex<(i64, Option<Value>)>,
    events: broadcast::Sender<Event>,
    viewers: AtomicUsize,
}

#[derive(Default)]
pub struct LiveSessions {
    sessions: std::sync::Mutex<HashMap<Uuid, Arc<Live>>>,
}

impl LiveSessions {
    /// The live side of a session, loaded from the database the first time it
    /// is asked for.
    pub async fn get(&self, db: &PgPool, uuid: Uuid) -> AppResult<Arc<Live>> {
        if let Some(live) = self.sessions.lock().map_err(AppError::internal)?.get(&uuid) {
            return Ok(live.clone());
        }
        let (revision, state): (i64, Option<Value>) =
            sqlx::query_as("SELECT revision, state FROM sessions WHERE uuid = $1 AND ended_at IS NULL")
                .bind(uuid)
                .fetch_one(db)
                .await?;
        let (events, _) = broadcast::channel(256);
        let live = Arc::new(Live {
            uuid,
            current: Mutex::new((revision, state)),
            events,
            viewers: AtomicUsize::new(0),
        });
        let mut sessions = self.sessions.lock().map_err(AppError::internal)?;
        Ok(sessions.entry(uuid).or_insert(live).clone())
    }

    pub fn viewers(&self, uuid: Uuid) -> usize {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(&uuid).map(|live| live.viewers.load(Ordering::Relaxed)))
            .unwrap_or(0)
    }

    /// Ends a session for everybody connected to it.
    pub async fn end(&self, db: &PgPool, uuid: Uuid) -> AppResult<()> {
        sqlx::query("UPDATE sessions SET ended_at = now() WHERE uuid = $1 AND ended_at IS NULL")
            .bind(uuid)
            .execute(db)
            .await?;
        // What the presentation showed goes with it. The files themselves are
        // collected later, unless something else uses the same content —
        // see `crate::gc`.
        sqlx::query("DELETE FROM session_media WHERE session_uuid = $1")
            .bind(uuid)
            .execute(db)
            .await?;
        let live = self.sessions.lock().map_err(AppError::internal)?.remove(&uuid);
        if let Some(live) = live {
            let _ = live.events.send(Event::Ended);
        }
        Ok(())
    }
}

impl Live {
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub async fn snapshot(&self) -> (i64, Option<Value>) {
        self.current.lock().await.clone()
    }

    pub fn join(&self) {
        let count = self.viewers.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.events.send(Event::Viewers(count));
    }

    pub fn leave(&self) {
        let count = self.viewers.fetch_sub(1, Ordering::Relaxed).saturating_sub(1);
        let _ = self.events.send(Event::Viewers(count));
    }

    pub async fn replace(&self, db: &PgPool, state: Value) -> AppResult<()> {
        if !state.is_object() {
            return Err(AppError::BadRequest("the state has to be an object".to_string()));
        }
        let mut current = self.current.lock().await;
        let revision = current.0 + 1;
        persist(db, self.uuid, revision, &state).await?;
        *current = (revision, Some(state.clone()));
        let _ = self.events.send(Event::Replaced { revision, state });
        Ok(())
    }

    pub async fn patch(&self, db: &PgPool, changes: Map<String, Value>) -> AppResult<()> {
        let mut current = self.current.lock().await;
        let Some(mut state) = current.1.clone() else {
            return Err(AppError::BadRequest("nothing is being presented yet".to_string()));
        };
        if changes.is_empty() {
            return Ok(());
        }
        cantara_api::apply_patch(&mut state, &changes);
        let revision = current.0 + 1;
        persist(db, self.uuid, revision, &state).await?;
        *current = (revision, Some(state));
        let _ = self.events.send(Event::Patched { revision, changes });
        Ok(())
    }
}

async fn persist(db: &PgPool, uuid: Uuid, revision: i64, state: &Value) -> AppResult<()> {
    sqlx::query("UPDATE sessions SET state = $2, revision = $3, last_active = now() WHERE uuid = $1")
        .bind(uuid)
        .bind(state)
        .bind(revision)
        .execute(db)
        .await?;
    Ok(())
}
