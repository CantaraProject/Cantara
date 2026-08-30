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
//! Where it runs is not here. Cantara serves nothing itself: the server is
//! started by [`crate::logic::network_server`], a helper process that offers
//! the stream and the presenter console from one socket, and Cantara talks to
//! it through [`crate::logic::network_host`]. The reason for the helper is in
//! that module, and it is not a small one.
//!
//! It is plain HTTP on a local network. The password keeps the curious out; it
//! is not protection from anyone who is actually trying, and it travels in the
//! clear. Streaming somewhere that is not a local network is what the remote
//! server is for, and TLS belongs there.

pub mod protocol;
pub mod server;

pub use server::local_address;
pub use server::StreamServer;
