//! Waiting, and what time it is — without a web view.
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
//!
//! [`Timestamp`] is here for the same reason: reading the clock is the other
//! thing whose answer depends on the target and not on the caller, and one
//! place that knows about `wasm32` is better than one in every module that
//! needs to know how long something has been going on.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A point on the wall clock, as milliseconds since the Unix epoch.
///
/// Deliberately not [`std::time::Instant`], which is the type this would
/// otherwise be. An `Instant` is a reading of a monotonic clock that means
/// nothing outside the process that took it, and every point that needs a time
/// here has to leave the process: to the helper that serves the network views,
/// to the browser tab the web build synchronises through, and to whatever
/// reloads and expects to be told the same number it was told before. None of
/// those can carry an `Instant`, and all of them can carry a count of
/// milliseconds.
///
/// The cost is the cost of a wall clock: it can be set backwards, by the user
/// or by NTP, and an elapsed time measured across that is wrong. So
/// [`elapsed`](Self::elapsed) saturates at zero rather than going negative —
/// a timer on a platform monitor that reads `0:00` for a moment is a great
/// deal better than one that reads something impossible, and a service is not
/// where a clock correction should produce a panic.
///
/// `f64` rather than an integer because the browser's clock is an `f64` and
/// the conversion has to happen somewhere; at this magnitude — a count of
/// milliseconds, about 1.8 × 10¹² today — an `f64` is exact to well under a
/// millisecond, and it serialises to JSON as a number either way.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, PartialOrd, Debug)]
pub struct Timestamp(f64);

impl Timestamp {
    /// The clock, now.
    pub fn now() -> Self {
        Timestamp(now_milliseconds())
    }

    /// Builds one from a count of milliseconds since the Unix epoch.
    ///
    /// For tests, and for reading a time that came from somewhere else.
    pub fn from_milliseconds(milliseconds: f64) -> Self {
        Timestamp(milliseconds)
    }

    /// The count of milliseconds since the Unix epoch.
    pub fn milliseconds(self) -> f64 {
        self.0
    }

    /// How long ago this was.
    ///
    /// Zero for a time in the future, which is what a clock set backwards
    /// since this was taken looks like from here. See the note on the type.
    pub fn elapsed(self) -> Duration {
        self.elapsed_at(Timestamp::now())
    }

    /// The same, against a stated `now`.
    ///
    /// Split out so that the rule about going backwards can be tested without
    /// waiting for a real clock, or changing one.
    pub fn elapsed_at(self, now: Timestamp) -> Duration {
        Duration::from_secs_f64(((now.0 - self.0) / 1000.0).max(0.0))
    }
}

/// Milliseconds since the Unix epoch, from the platform's clock.
///
/// A system clock set before 1970 reads as the epoch rather than as a negative
/// time. It is not a situation worth carrying an error for, and zero is a time
/// every caller here can already cope with.
#[cfg(not(target_arch = "wasm32"))]
fn now_milliseconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

/// A browser has no `SystemTime` — `std::time::SystemTime::now` panics on
/// `wasm32-unknown-unknown`. `Date.now()` is the same reading, and is what
/// every other timestamp in a page comes from.
#[cfg(target_arch = "wasm32")]
fn now_milliseconds() -> f64 {
    js_sys::Date::now()
}

/// Yields for `duration`, then continues.
///
/// Safe to call from anything Dioxus spawns, on every target.
#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

/// A browser has no tokio; its timers are the ones the event loop provides.
///
/// `setTimeout` takes a 32-bit count of milliseconds, so anything longer than
/// about seven weeks is capped rather than wrapped: a truncating cast would
/// turn a very long wait into a very short one, which is the opposite of what
/// was asked for and the harder failure to find.
#[cfg(target_arch = "wasm32")]
pub async fn sleep(duration: Duration) {
    let milliseconds = duration.as_millis().min(u32::MAX as u128) as u32;
    gloo_timers::future::TimeoutFuture::new(milliseconds).await;
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

    /// The ordinary reading: a time in the past is that long ago.
    #[test]
    fn elapsed_counts_from_then_to_now() {
        let then = Timestamp::from_milliseconds(1_000_000.0);
        let now = Timestamp::from_milliseconds(1_137_500.0);

        assert_eq!(then.elapsed_at(now), Duration::from_millis(137_500));
    }

    /// The clock going backwards mid-service — NTP, or someone fixing the
    /// machine's time zone — must not produce a negative duration.
    /// `Duration::from_secs_f64` panics on one, and the panic would be in
    /// whatever is drawing a monitor view in front of a congregation.
    #[test]
    fn a_clock_set_backwards_reads_as_no_time_at_all() {
        let then = Timestamp::from_milliseconds(1_137_500.0);
        let now = Timestamp::from_milliseconds(1_000_000.0);

        assert_eq!(then.elapsed_at(now), Duration::ZERO);
    }

    /// It has to survive the journey to the helper and to a browser tab —
    /// that is the reason it is not an `Instant`.
    #[test]
    fn a_timestamp_is_the_same_time_after_a_round_trip_through_json() {
        let taken = Timestamp::now();

        let written = serde_json::to_string(&taken).expect("a timestamp is serialisable");
        let read: Timestamp = serde_json::from_str(&written).expect("and readable back");

        assert_eq!(read, taken);
    }

    /// A sanity check on the clock itself: the epoch was a long time ago, and
    /// a target whose `now` returned nothing would make every timer read as
    /// decades.
    #[test]
    fn the_clock_reads_as_a_time_after_the_epoch() {
        assert!(
            Timestamp::now().milliseconds() > 1_600_000_000_000.0,
            "the clock reads as before September 2020"
        );
    }
}
