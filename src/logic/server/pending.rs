//! Changes made here to files from a server, on their way there.
//!
//! Saving a song that came from a server library writes it where the program
//! reads it from — the cache on the desktop, the in-memory file system in the
//! browser — and puts it here. From here it goes to the server as a new
//! version, naming the version the edit started from (S6 of
//! `docs/specs/0008-server-version.md`). Offline it waits, kept on disk, and
//! goes when the server can be reached again.
//!
//! If the item has moved on on the server in the meantime, the server says so
//! (`version_conflict`), and the change stays here as a conflict until the user
//! decides: keep their version (it goes up as the newest), or take the
//! server's (theirs is dropped). Nothing is merged.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{client_for, item_of, ClientError};

/// One change waiting for the server.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct PendingChange {
    /// Where the edited file is read from here.
    pub path: String,
    pub instance: Uuid,
    pub tenant: Uuid,
    pub library: Uuid,
    pub lineage: Uuid,
    /// The version the edit started from.
    pub base_version: Uuid,
    pub file_name: String,
    pub content: String,
    /// The version the server has instead, when it refused.
    #[serde(default)]
    pub conflict: Option<Uuid>,
    /// Why the server refused, when it did for another reason — a library
    /// the user may only read, say.
    #[serde(default)]
    pub refused: Option<String>,
}

/// Where a file stands with the server.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SyncState {
    /// Saved here, not on the server yet.
    Waiting,
    /// The server has a newer version from somebody else.
    Conflict,
    /// The server will not take it.
    Refused,
}

static QUEUE: Mutex<Option<Vec<PendingChange>>> = Mutex::new(None);

#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "cantara-server-pending";

fn load() -> Vec<PendingChange> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        super::cache::cache_root()
            .and_then(|root| std::fs::read_to_string(root.join("pending.json")).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::logic::web_storage::read(STORAGE_KEY).unwrap_or_default()
    }
}

fn persist(queue: &[PendingChange]) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(root) = super::cache::cache_root() else { return };
        let written = std::fs::create_dir_all(&root).and_then(|()| {
            let text = serde_json::to_string(queue).map_err(std::io::Error::other)?;
            let temporary = root.join("pending.json.part");
            std::fs::write(&temporary, text)?;
            std::fs::rename(&temporary, root.join("pending.json"))
        });
        if let Err(error) = written {
            log::error!("changes for the server cannot be kept: {error}");
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::logic::web_storage::write(STORAGE_KEY, &queue.to_vec());
    }
}

fn with_queue<T>(work: impl FnOnce(&mut Vec<PendingChange>) -> T) -> Option<T> {
    let mut guard = QUEUE.lock().ok()?;
    let queue = guard.get_or_insert_with(load);
    let result = work(queue);
    persist(queue);
    Some(result)
}

fn read_queue<T>(work: impl FnOnce(&[PendingChange]) -> T) -> Option<T> {
    let mut guard = QUEUE.lock().ok()?;
    Some(work(guard.get_or_insert_with(load)))
}

/// Puts the text just saved at `path` on its way to the server, if the file
/// came from one. `true` if it did.
pub fn record(path: &str, content: &str) -> bool {
    let Some(item) = item_of(path) else { return false };
    let file_name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
    with_queue(|queue| match queue.iter_mut().find(|change| change.path == path) {
        // A second save before the first went up: the newer text, still
        // measured against the version the editing started from.
        Some(change) => {
            change.content = content.to_string();
            change.refused = None;
        }
        None => queue.push(PendingChange {
            path: path.to_string(),
            instance: item.instance,
            tenant: item.tenant,
            library: item.library,
            lineage: item.lineage,
            base_version: item.version,
            file_name,
            content: content.to_string(),
            conflict: None,
            refused: None,
        }),
    })
    .is_some()
}

/// Where the file at `path` stands, if a change to it is waiting.
pub fn state_of(path: &str) -> Option<SyncState> {
    read_queue(|queue| {
        queue.iter().find(|change| change.path == path).map(|change| {
            if change.conflict.is_some() {
                SyncState::Conflict
            } else if change.refused.is_some() {
                SyncState::Refused
            } else {
                SyncState::Waiting
            }
        })
    })
    .flatten()
}

/// Every change waiting.
pub fn all() -> Vec<PendingChange> {
    read_queue(|queue| queue.to_vec()).unwrap_or_default()
}

/// Resolves a conflict in favour of the change made here: it goes up as the
/// newest version, on top of the one the server has.
pub fn keep_mine(path: &str) {
    with_queue(|queue| {
        if let Some(change) = queue.iter_mut().find(|change| change.path == path)
            && let Some(current) = change.conflict.take()
        {
            change.base_version = current;
        }
    });
}

/// Resolves a conflict in favour of the server: the change made here is
/// dropped. The next look at the library brings the server's version.
pub fn take_theirs(path: &str) {
    with_queue(|queue| queue.retain(|change| change.path != path));
    // The edited copy stood where the old version is kept; it is no longer
    // what that version is.
    #[cfg(not(target_arch = "wasm32"))]
    let _ = std::fs::remove_file(path);
}

/// Sends what is waiting. `true` if anything reached the server, so that the
/// library should be looked at again.
pub async fn flush() -> bool {
    let waiting: Vec<PendingChange> = all()
        .into_iter()
        .filter(|change| change.conflict.is_none() && change.refused.is_none())
        .collect();
    let mut delivered = false;
    for change in waiting {
        let Some(client) = client_for(change.instance) else { continue };
        let result = client
            .replace_item(
                change.tenant,
                change.library,
                change.lineage,
                &change.file_name,
                change.content.clone().into_bytes(),
                Some(change.base_version),
            )
            .await;
        match result {
            Ok(_) => {
                delivered = true;
                with_queue(|queue| {
                    // Only if nothing newer was saved while this was on its way.
                    queue.retain(|waiting| !(waiting.path == change.path && waiting.content == change.content));
                });
            }
            Err(ClientError::Refused { status: 409, error }) => {
                let current = error.and_then(|error| error.current_version);
                with_queue(|queue| {
                    if let Some(waiting) = queue.iter_mut().find(|waiting| waiting.path == change.path) {
                        waiting.conflict = Some(current.unwrap_or(change.base_version));
                    }
                });
            }
            Err(ClientError::Refused { error, status }) => {
                let reason = error.map(|error| error.message).unwrap_or_else(|| format!("{status}"));
                with_queue(|queue| {
                    if let Some(waiting) = queue.iter_mut().find(|waiting| waiting.path == change.path) {
                        waiting.refused = Some(reason.clone());
                    }
                });
            }
            // Offline, or logged out: everything waits for the next round.
            Err(ClientError::Unreachable(_) | ClientError::Unauthorized | ClientError::Unreadable(_)) => break,
        }
    }
    delivered
}
