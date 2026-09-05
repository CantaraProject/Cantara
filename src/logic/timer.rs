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
/// A whole number of milliseconds, and not an `f64`, for a reason that cost an
/// afternoon: **`serde_json` does not read floats back exactly.** Its default
/// parser is a fast approximation — exact round-tripping is behind the
/// `float_roundtrip` feature — so a timestamp of `1788605525443.4739` written
/// out came back as a *different* number, and a presentation compared against
/// the one that had just been sent to the helper was unequal to itself. It
/// failed in about one run of the test suite in three, which is the worst way
/// for a thing like this to fail.
///
/// An integer has none of that: JSON carries it exactly, on every path this
/// value takes. Nothing here wants sub-millisecond precision anyway — the
/// widget that reads it counts in seconds.
///
/// Signed, so that a clock set to before 1970 is merely a negative number
/// rather than an enormous positive one.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Timestamp(i64);

impl Timestamp {
    /// The clock, now.
    pub fn now() -> Self {
        Timestamp(now_milliseconds())
    }

    /// Builds one from a count of milliseconds since the Unix epoch.
    ///
    /// For tests, and for reading a time that came from somewhere else.
    pub fn from_milliseconds(milliseconds: i64) -> Self {
        Timestamp(milliseconds)
    }

    /// The count of milliseconds since the Unix epoch.
    pub fn milliseconds(self) -> i64 {
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
        Duration::from_millis(now.0.saturating_sub(self.0).max(0) as u64)
    }
}

/// Milliseconds since the Unix epoch, from the platform's clock.
///
/// A system clock set before 1970 reads as a negative number, which
/// [`Timestamp::elapsed_at`] copes with; there is nothing here worth carrying
/// an error for.
#[cfg(not(target_arch = "wasm32"))]
fn now_milliseconds() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(since) => since.as_millis() as i64,
        Err(before) => -(before.duration().as_millis() as i64),
    }
}

/// A browser has no `SystemTime` — `std::time::SystemTime::now` panics on
/// `wasm32-unknown-unknown`. `Date.now()` is the same reading, and is what
/// every other timestamp in a page comes from.
///
/// It hands back an `f64` holding a whole number of milliseconds; the cast is
/// where that becomes the integer everything else here uses. A float-to-integer
/// cast in Rust saturates rather than wrapping, so even an absurd clock gives a
/// number rather than nonsense.
#[cfg(target_arch = "wasm32")]
fn now_milliseconds() -> i64 {
    js_sys::Date::now() as i64
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
        let then = Timestamp::from_milliseconds(1_000_000);
        let now = Timestamp::from_milliseconds(1_137_500);

        assert_eq!(then.elapsed_at(now), Duration::from_millis(137_500));
    }

    /// The clock going backwards mid-service — NTP, or someone fixing the
    /// machine's time zone — must not produce a negative duration.
    /// `Duration::from_secs_f64` panics on one, and the panic would be in
    /// whatever is drawing a monitor view in front of a congregation.
    #[test]
    fn a_clock_set_backwards_reads_as_no_time_at_all() {
        let then = Timestamp::from_milliseconds(1_137_500);
        let now = Timestamp::from_milliseconds(1_000_000);

        assert_eq!(then.elapsed_at(now), Duration::ZERO);
    }

    /// It has to survive the journey to the helper and to a browser tab —
    /// that is the reason it is not an `Instant`.
    ///
    /// Sampled rather than checked once, because this is the test that failed
    /// when the value was an `f64`: `serde_json` reads floats back
    /// approximately by default, so *most* timestamps round-tripped and the
    /// occasional one did not. A single sample passed nine times out of ten
    /// and the suite failed one run in three, somewhere else entirely — in a
    /// presentation that had become unequal to itself. Whatever this type is
    /// made of has to carry every value exactly, and one example cannot say
    /// that.
    #[test]
    fn every_timestamp_is_the_same_time_after_a_round_trip_through_json() {
        for _ in 0..10_000 {
            let taken = Timestamp::now();

            let written = serde_json::to_string(&taken).expect("a timestamp is serialisable");
            let read: Timestamp = serde_json::from_str(&written).expect("and readable back");

            assert_eq!(read, taken, "written as {written}");
        }
    }

    /// A clock set to before 1970 is a number, not a panic and not an enormous
    /// positive one. Rare, and a machine that has lost its battery does it.
    #[test]
    fn a_time_before_the_epoch_is_negative() {
        let before = Timestamp::from_milliseconds(-5_000);
        let after = Timestamp::from_milliseconds(1_000);

        assert_eq!(before.elapsed_at(after), Duration::from_millis(6_000));
    }

    /// A sanity check on the clock itself: the epoch was a long time ago, and
    /// a target whose `now` returned nothing would make every timer read as
    /// decades.
    #[test]
    fn the_clock_reads_as_a_time_after_the_epoch() {
        assert!(
            Timestamp::now().milliseconds() > 1_600_000_000_000,
            "the clock reads as before September 2020"
        );
    }
}
