//! A brake on guessing passwords.
//!
//! Counts failed attempts per key (an e-mail address, an IP address) in a
//! sliding minute, in memory. A server restart forgets the count, which is
//! fine: the point is to make guessing slow, not to keep a record.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How many failures a key may have within [`WINDOW`].
const ALLOWED: usize = 10;
const WINDOW: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct Limiter {
    failures: Mutex<HashMap<String, Vec<Instant>>>,
}

impl Limiter {
    /// Whether `key` may try again now.
    pub fn allows(&self, key: &str) -> bool {
        let Ok(mut failures) = self.failures.lock() else {
            return true;
        };
        let now = Instant::now();
        let entry = failures.entry(key.to_string()).or_default();
        entry.retain(|at| now.duration_since(*at) < WINDOW);
        entry.len() < ALLOWED
    }

    pub fn failed(&self, key: &str) {
        if let Ok(mut failures) = self.failures.lock() {
            failures.entry(key.to_string()).or_default().push(Instant::now());
            // Keep the map from growing without bound under a spray of keys.
            if failures.len() > 10_000 {
                let now = Instant::now();
                failures.retain(|_, at| at.iter().any(|at| now.duration_since(*at) < WINDOW));
            }
        }
    }

    pub fn succeeded(&self, key: &str) {
        if let Ok(mut failures) = self.failures.lock() {
            failures.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_stopped_after_too_many_failures_and_freed_by_success() {
        let limiter = Limiter::default();
        for _ in 0..ALLOWED {
            assert!(limiter.allows("someone"));
            limiter.failed("someone");
        }
        assert!(!limiter.allows("someone"));
        assert!(limiter.allows("someone else"));
        limiter.succeeded("someone");
        assert!(limiter.allows("someone"));
    }
}
