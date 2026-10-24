//! Sharing the running presentation through a server.
//!
//! The LAN stream and the remote console stay as they are; this is the other
//! road, for watching and controlling from anywhere the server can be reached.
//! The program presenting opens a session on a server and keeps it told what
//! the presentation is (`cantara_api::SessionClientMessage`): the whole of it
//! once, then only the top-level fields of `RunningPresentation` that changed —
//! a slide change is a `position`, not the service again. Whoever controls the
//! session through a control link changes it the same way, and what they did
//! comes back here and is put into the running presentation.
//!
//! Like the remote console, it is driven from the loop in `main` that polls
//! every fifty milliseconds: [`publish`] after a change here, [`drain`] for a
//! change made elsewhere. The connection itself runs beside it — a thread with
//! a runtime of its own on the desktop, the browser's `WebSocket` in the web
//! build.

// The browser build shares only where a server serves it.
#![cfg_attr(all(target_arch = "wasm32", not(feature = "server-client")), allow(dead_code))]

use std::sync::Mutex;

use cantara_api::{diff_top_level, apply_patch, SessionClientMessage, SessionInfo, SessionServerMessage};
use serde_json::Value;
use uuid::Uuid;

use super::{client_for, Client};
use crate::logic::states::RunningPresentation;

/// What the settings and the presentation options show about a share.
#[derive(Clone, Debug, PartialEq)]
pub struct Sharing {
    pub instance: Uuid,
    pub tenant: Uuid,
    pub session: SessionInfo,
    pub connected: bool,
    pub viewers: usize,
    pub error: Option<String>,
}

struct Shared {
    status: Sharing,
    /// What this program last told the server the presentation is.
    published: Option<Value>,
    /// What the server says it is, as of the last message from it.
    remote: Option<Value>,
    /// Messages sent and not yet reflected back. While any are on their way,
    /// what comes back is this program's own change and not adopted.
    in_flight: usize,
    /// Whether `remote` has news this program has not taken up.
    remote_changed: bool,
    /// The pictures and documents handed to the server already, and those on
    /// their way.
    uploaded: std::collections::HashSet<String>,
    uploading: std::collections::HashSet<String>,
    outgoing: Outgoing,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

/// The current share, if there is one.
pub fn status() -> Option<Sharing> {
    SHARED.lock().ok()?.as_ref().map(|shared| shared.status.clone())
}

fn handle(message: SessionServerMessage) {
    let Ok(mut guard) = SHARED.lock() else { return };
    let Some(shared) = guard.as_mut() else { return };
    match message {
        SessionServerMessage::Snapshot { state, .. } => {
            shared.status.connected = true;
            shared.remote = state;
            settle(shared);
        }
        SessionServerMessage::Patch { changes, .. } => {
            let mut state = shared.remote.take().unwrap_or(Value::Null);
            apply_patch(&mut state, &changes);
            shared.remote = Some(state);
            settle(shared);
        }
        SessionServerMessage::Viewers { count } => shared.status.viewers = count,
        SessionServerMessage::Ended => {
            shared.status.connected = false;
            shared.status.error = Some("ended".to_string());
        }
        SessionServerMessage::Error { message } => {
            shared.in_flight = shared.in_flight.saturating_sub(1);
            shared.status.error = Some(message);
        }
    }
}

/// After a message from the server: was it ours coming back, or news?
fn settle(shared: &mut Shared) {
    if shared.in_flight > 0 {
        shared.in_flight -= 1;
        return;
    }
    if shared.remote.is_some() && shared.remote != shared.published {
        shared.remote_changed = true;
    }
}

fn disconnected(error: Option<String>) {
    if let Ok(mut guard) = SHARED.lock()
        && let Some(shared) = guard.as_mut()
    {
        shared.status.connected = false;
        shared.status.error = error;
        shared.in_flight = 0;
    }
}

/// Starts sharing into `session`, which the caller has created on the server.
pub fn start(instance: Uuid, tenant: Uuid, session: SessionInfo) -> Result<(), String> {
    let client = client_for(instance).ok_or("the server is not known")?;
    stop(false);
    let outgoing = connect(&client, session.uuid)?;
    if let Ok(mut guard) = SHARED.lock() {
        *guard = Some(Shared {
            status: Sharing {
                instance,
                tenant,
                session,
                connected: false,
                viewers: 0,
                error: None,
            },
            published: None,
            remote: None,
            in_flight: 0,
            remote_changed: false,
            uploaded: Default::default(),
            uploading: Default::default(),
            outgoing,
        });
    }
    Ok(())
}

/// Stops sharing. With `end`, the session is ended on the server too, for
/// everybody watching.
pub fn stop(end: bool) {
    let shared = SHARED.lock().ok().and_then(|mut guard| guard.take());
    if let Some(shared) = shared {
        if end {
            shared.outgoing.send(&SessionClientMessage::End);
        }
        shared.outgoing.close();
    }
}

/// Tells the server about the presentation as it is now, if it changed.
// Called from the desktop's poll loop only, for now — see "What is left" in
// the spec.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn publish(now: Option<&RunningPresentation>) {
    let Ok(mut guard) = SHARED.lock() else { return };
    let Some(shared) = guard.as_mut() else { return };
    let Some(now) = now else { return };
    let Ok(value) = serde_json::to_value(now) else { return };
    if shared.published.as_ref() == Some(&value) {
        return;
    }
    let message = match &shared.published {
        Some(before) => SessionClientMessage::Patch {
            changes: diff_top_level(before, &value),
        },
        None => SessionClientMessage::Replace { state: value.clone() },
    };
    if shared.outgoing.send(&message) {
        shared.in_flight += 1;
        shared.published = Some(value);
    }
}

/// The bytes of a picture or document a slide names, wherever this build
/// keeps them.
fn read_media(path: &str) -> Option<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read(path).ok()
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::logic::settings::RepositoryType::web_read_file(path)
    }
}

/// Hands the server the pictures and documents the presentation shows that it
/// does not have yet, so that a viewer on another device sees what the room
/// sees. Videos are left out: they play in the room (Q11).
pub async fn upload_media() {
    let (client, session, wanted) = {
        let Ok(mut guard) = SHARED.lock() else { return };
        let Some(shared) = guard.as_mut() else { return };
        let Some(published) = &shared.published else { return };
        let wanted: Vec<String> = cantara_api::media_paths(published)
            .into_iter()
            .filter(|path| !shared.uploaded.contains(path) && !shared.uploading.contains(path))
            .collect();
        if wanted.is_empty() {
            return;
        }
        let Some(client) = client_for(shared.status.instance) else { return };
        shared.uploading.extend(wanted.iter().cloned());
        (client, shared.status.session.uuid, wanted)
    };
    for path in wanted {
        let sent = match read_media(&path) {
            Some(bytes) => match client.put_session_media(session, &path, bytes).await {
                Ok(()) => true,
                Err(error) => {
                    log::warn!("{path} cannot be shared: {error}");
                    false
                }
            },
            // Nothing to send; asking again would not change that.
            None => true,
        };
        if let Ok(mut guard) = SHARED.lock()
            && let Some(shared) = guard.as_mut()
        {
            shared.uploading.remove(&path);
            if sent {
                shared.uploaded.insert(path);
            }
        }
    }
}

/// Takes up a change made by somebody controlling the session. `true` if the
/// presentation was changed.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn drain(presentations: &mut [RunningPresentation]) -> bool {
    let Ok(mut guard) = SHARED.lock() else { return false };
    let Some(shared) = guard.as_mut() else { return false };
    if !shared.remote_changed {
        return false;
    }
    shared.remote_changed = false;
    let Some(remote) = shared.remote.clone() else { return false };
    let Ok(presentation) = serde_json::from_value::<RunningPresentation>(remote.clone()) else {
        return false;
    };
    shared.published = Some(remote);
    match presentations.first_mut() {
        Some(first) => {
            *first = presentation;
            true
        }
        None => false,
    }
}

// ── The connection ───────────────────────────────────────────────────────────

/// Where messages to the server go.
#[cfg(not(target_arch = "wasm32"))]
struct Outgoing(tokio::sync::mpsc::UnboundedSender<Option<String>>);

#[cfg(not(target_arch = "wasm32"))]
impl Outgoing {
    fn send(&self, message: &SessionClientMessage) -> bool {
        serde_json::to_string(message)
            .ok()
            .is_some_and(|text| self.0.send(Some(text)).is_ok())
    }

    fn close(&self) {
        let _ = self.0.send(None);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn connect(client: &Client, session: Uuid) -> Result<Outgoing, String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::Message;

    let mut request = client
        .socket_url(session, None)
        .into_client_request()
        .map_err(|error| error.to_string())?;
    if let Some(token) = client.token() {
        let value = format!("Bearer {token}").parse().map_err(|_| "the token cannot be sent".to_string())?;
        request.headers_mut().insert("Authorization", value);
    }
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<Option<String>>();

    std::thread::Builder::new()
        .name("cantara-session".to_string())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
                disconnected(Some("no runtime".to_string()));
                return;
            };
            runtime.block_on(async move {
                let socket = match tokio_tungstenite::connect_async(request).await {
                    Ok((socket, _)) => socket,
                    Err(error) => {
                        disconnected(Some(error.to_string()));
                        return;
                    }
                };
                let (mut write, mut read) = socket.split();
                loop {
                    tokio::select! {
                        outgoing = receiver.recv() => match outgoing {
                            Some(Some(text)) => {
                                if write.send(Message::Text(text.into())).await.is_err() {
                                    disconnected(Some("the connection broke".to_string()));
                                    return;
                                }
                            }
                            _ => {
                                let _ = write.send(Message::Close(None)).await;
                                return;
                            }
                        },
                        incoming = read.next() => match incoming {
                            Some(Ok(Message::Text(text))) => {
                                if let Ok(message) = serde_json::from_str::<SessionServerMessage>(text.as_str()) {
                                    handle(message);
                                }
                            }
                            Some(Ok(Message::Close(_))) | None => {
                                disconnected(None);
                                return;
                            }
                            Some(Err(error)) => {
                                disconnected(Some(error.to_string()));
                                return;
                            }
                            Some(Ok(_)) => {}
                        },
                    }
                }
            });
        })
        .map_err(|error| error.to_string())?;
    Ok(Outgoing(sender))
}

#[cfg(target_arch = "wasm32")]
struct Outgoing(web_sys::WebSocket);

// The browser build is single-threaded; the socket only lives in the static
// so that the poll loop can reach it.
#[cfg(target_arch = "wasm32")]
unsafe impl Send for Outgoing {}

#[cfg(target_arch = "wasm32")]
impl Outgoing {
    fn send(&self, message: &SessionClientMessage) -> bool {
        if self.0.ready_state() != web_sys::WebSocket::OPEN {
            return false;
        }
        serde_json::to_string(message)
            .ok()
            .is_some_and(|text| self.0.send_with_str(&text).is_ok())
    }

    fn close(&self) {
        let _ = self.0.close();
    }
}

#[cfg(target_arch = "wasm32")]
fn connect(client: &Client, session: Uuid) -> Result<Outgoing, String> {
    let socket = open_socket(&client.socket_url(session, None), handle, disconnected)?;
    Ok(Outgoing(socket))
}

/// Opens a socket in the browser and hands every message to `on_message`.
#[cfg(target_arch = "wasm32")]
pub fn open_socket(
    url: &str,
    on_message: fn(SessionServerMessage),
    on_close: fn(Option<String>),
) -> Result<web_sys::WebSocket, String> {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    let socket = web_sys::WebSocket::new(url).map_err(|_| "the socket cannot be opened".to_string())?;
    let message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
        if let Some(text) = event.data().as_string()
            && let Ok(message) = serde_json::from_str::<SessionServerMessage>(&text)
        {
            on_message(message);
        }
    });
    socket.set_onmessage(Some(message.as_ref().unchecked_ref()));
    message.forget();
    let close = Closure::<dyn FnMut(web_sys::CloseEvent)>::new(move |_event: web_sys::CloseEvent| {
        on_close(None);
    });
    socket.set_onclose(Some(close.as_ref().unchecked_ref()));
    close.forget();
    Ok(socket)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shared() -> Shared {
        let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
        Shared {
            status: Sharing {
                instance: Uuid::nil(),
                tenant: Uuid::nil(),
                session: SessionInfo {
                    uuid: Uuid::nil(),
                    name: String::new(),
                    owner: cantara_api::UserRef { uuid: Uuid::nil(), display_name: String::new() },
                    created_at: chrono::DateTime::UNIX_EPOCH,
                    revision: 0,
                    running: false,
                    anonymous_control: false,
                    viewers: 0,
                    role: cantara_api::Role::Manage,
                    watch_link: None,
                    control_link: None,
                },
                connected: true,
                viewers: 0,
                error: None,
            },
            published: None,
            remote: None,
            in_flight: 0,
            remote_changed: false,
            uploaded: Default::default(),
            uploading: Default::default(),
            outgoing: Outgoing(sender),
        }
    }

    #[test]
    fn our_own_change_coming_back_is_not_news() {
        let mut shared = shared();
        shared.published = Some(json!({ "position": 2 }));
        shared.in_flight = 1;
        shared.remote = Some(json!({ "position": 2 }));
        settle(&mut shared);
        assert!(!shared.remote_changed);
        assert_eq!(shared.in_flight, 0);
    }

    #[test]
    fn a_change_from_a_controller_is_taken_up() {
        let mut shared = shared();
        shared.published = Some(json!({ "position": 2 }));
        shared.remote = Some(json!({ "position": 3 }));
        settle(&mut shared);
        assert!(shared.remote_changed);
    }
}
