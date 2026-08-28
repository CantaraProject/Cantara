//! Offering the running presentation to browsers on the local network.
//!
//! Cantara puts up a small HTTP server; anyone on the same network opens the
//! address and follows along. A phone in a pew shows the words, a musician's
//! tablet will show what is coming — the address is one, the views over it are
//! several.
//!
//! Three parts, kept apart on purpose:
//!
//! - [`protocol`] is what a viewer is told: pure data, built from a
//!   [`RunningPresentation`](crate::logic::states::RunningPresentation) and
//!   tested without a socket in sight.
//! - `server` runs the thing, on a runtime of its own so that it cannot
//!   interfere with the window.
//! - The page itself is `assets/stream_viewer.html`, served as it is.
//!
//! It is plain HTTP on a local network. The password keeps the curious out; it
//! is not protection from anyone who is actually trying, and it travels in the
//! clear. Streaming somewhere that is not a local network is what the remote
//! server is for, and TLS belongs there.

pub mod protocol;
pub mod server;

pub use server::local_address;
pub use server::StreamServer;

use std::sync::{Mutex, OnceLock};

use protocol::StreamState;

/// The one server this program may have running.
///
/// A process-wide handle rather than something held in a signal, for two
/// reasons: the server owns a thread and a socket, which is not the sort of
/// thing to hand around by value, and it has to outlive the view that switched
/// it on. Streaming stops when it is switched off or the program ends, not
/// when the user navigates away from the panel with the switch on it.
static SERVER: OnceLock<Mutex<Option<StreamServer>>> = OnceLock::new();

fn server() -> &'static Mutex<Option<StreamServer>> {
    SERVER.get_or_init(|| Mutex::new(None))
}

/// Starts offering the presentation on `port`, and says where to find it.
///
/// Returns the address to show the user, or what went wrong — a port already
/// in use is the likeliest outcome and is not a reason to fall over.
pub fn enable(port: u16, password: String) -> Result<String, String> {
    let mut held = server()
        .lock()
        .map_err(|_| "the streaming server is in a bad state".to_string())?;

    // Already running: switching on twice is not an error, it is the same
    // answer twice.
    if let Some(running) = held.as_ref() {
        return Ok(running.address());
    }

    let started = StreamServer::start(port, password)?;
    let address = started.address();
    *held = Some(started);
    Ok(address)
}

/// Stops offering it. Doing this when nothing is running is not an error.
pub fn disable() {
    if let Ok(mut held) = server().lock() {
        // Dropping the handle stops the thread and gives the port back.
        held.take();
    }
}

/// Whether the server is up.
pub fn is_enabled() -> bool {
    server()
        .lock()
        .map(|held| held.is_some())
        .unwrap_or(false)
}

/// The address to type into a phone, if there is one.
pub fn address() -> Option<String> {
    server()
        .lock()
        .ok()
        .and_then(|held| held.as_ref().map(|running| running.address()))
}

/// Tells every viewer where the presentation now stands.
///
/// Does nothing when streaming is off, so a caller may publish on every change
/// without asking first.
pub fn publish(state: StreamState) {
    if let Ok(mut held) = server().lock()
        && let Some(running) = held.as_mut()
    {
        running.publish(state);
    }
}

/// Whether a picture has already been handed to the server.
pub fn has_media(id: &str) -> bool {
    server()
        .lock()
        .map(|held| held.as_ref().is_some_and(|running| running.has_media(id)))
        .unwrap_or(false)
}

/// Hands a picture over, under the name the state gives it.
pub fn publish_media(id: String, bytes: Vec<u8>, content_type: &'static str) {
    if let Ok(held) = server().lock()
        && let Some(running) = held.as_ref()
    {
        running.publish_media(id, bytes, content_type);
    }
}

/// Whether a video has already been registered with the server.
pub fn has_video(id: &str) -> bool {
    server()
        .lock()
        .map(|held| held.as_ref().is_some_and(|running| running.has_video(id)))
        .unwrap_or(false)
}

/// Says where a video is, so the server can serve it from there.
///
/// The path rather than the bytes: a service video is far too large to hold in
/// memory for the length of a presentation, and a browser asks for it in pieces
/// anyway.
pub fn publish_video(id: String, path: std::path::PathBuf) {
    if let Ok(held) = server().lock()
        && let Some(running) = held.as_ref()
    {
        running.publish_video(id, path);
    }
}
