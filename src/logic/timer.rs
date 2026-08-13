//! Waiting, without a web view.
//!
//! Several views watch something that a background thread fills in — the
//! installed font families, the thumbnails of a library, a PDF page being
//! rendered — and a thread cannot write to a signal. So they look, wait a
//! moment and look again.
//!
//! That wait used to be `document::eval("await new Promise(r => setTimeout(r,
//! 150))")`: a script compiled and run in the page for every tick of every
//! poll. It works, but it makes the page the program's clock, it costs a
//! round trip through the web view each time, and it is a piece of JavaScript
//! in a place that needs none.
//!
//! Here it is a timer on the platform's own runtime instead. Every native
//! target already runs on tokio — Dioxus builds on it — and the browser has
//! `gloo-timers`, which is what wasm-bindgen's futures use anyway.

use std::time::Duration;

/// Yields for `duration`, then continues.
///
/// Safe to call from anything Dioxus spawns, on every target.
#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

/// A browser has no tokio; its timers are the ones the event loop provides.
#[cfg(target_arch = "wasm32")]
pub async fn sleep(duration: Duration) {
    gloo_timers::future::TimeoutFuture::new(duration.as_millis() as u32).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The point of it: it comes back, and not before it is due. A poll that
    /// returned at once would spin a core; one that never returned would hang
    /// the view that is waiting for its fonts.
    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn test_sleep_waits_and_returns() {
        let started = std::time::Instant::now();
        sleep(Duration::from_millis(50)).await;
        assert!(
            started.elapsed() >= Duration::from_millis(40),
            "the timer came back early"
        );
    }
}
