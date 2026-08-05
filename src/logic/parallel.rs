//! Spreading per-file work over the machine's cores.
//!
//! Reading a library is not one long computation but a few thousand
//! independent ones — hashing a file, parsing a PDF — and every one of them
//! spends most of its time waiting for the disk. Done one after the other that
//! is a visible pause at startup and whenever a repository changes; done a
//! handful at a time it is not.
//!
//! Only the native builds have threads at all: the web build runs on the
//! browser's single thread, and its libraries are already in memory rather
//! than on a disk to wait for.

use std::sync::atomic::{AtomicUsize, Ordering};

/// The most threads a scan is spread over.
///
/// The work is dominated by waiting for the disk, so a few more threads than
/// cores would still help on an SSD — but on a spinning disk or a network
/// share they turn one sequential read into many competing seeks, which is
/// slower than not parallelising at all. This is the point where the two
/// cases are both served well enough.
const MAX_WORKERS: usize = 8;

/// Applies `work` to every item, several items at a time, and returns the
/// results in the order the items came in.
///
/// Work is handed out one item at a time rather than in equal blocks, because
/// the items are not equally expensive: one 200-page PDF among two thousand
/// songs would otherwise decide how long the whole scan takes.
///
/// A panic in `work` is passed on to the caller rather than swallowed — a
/// half-filled result would be indistinguishable from files that legitimately
/// have nothing to contribute.
pub fn map_parallel<T, R, F>(items: &[T], work: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let workers = worker_count(items.len());
    if workers <= 1 {
        return items.iter().map(work).collect();
    }

    let next = AtomicUsize::new(0);
    let next = &next;
    let work = &work;

    let mut produced: Vec<(usize, R)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(move || {
                    let mut done: Vec<(usize, R)> = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            return done;
                        };
                        done.push((index, work(item)));
                    }
                })
            })
            .collect();

        let mut produced: Vec<(usize, R)> = Vec::with_capacity(items.len());
        for handle in handles {
            match handle.join() {
                Ok(done) => produced.extend(done),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        produced
    });

    produced.sort_by_key(|(index, _)| *index);
    produced.into_iter().map(|(_, value)| value).collect()
}

/// How many threads to spread `items` over.
fn worker_count(items: usize) -> usize {
    if items <= 1 {
        return 1;
    }
    let cores = std::thread::available_parallelism()
        .map(|cores| cores.get())
        .unwrap_or(1);
    cores.clamp(1, MAX_WORKERS).min(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The results must line up with the items they came from, whichever
    /// thread happened to produce them first.
    #[test]
    fn results_keep_the_order_of_the_items() {
        let items: Vec<usize> = (0..1000).collect();

        let doubled = map_parallel(&items, |item| item * 2);

        assert_eq!(doubled, items.iter().map(|item| item * 2).collect::<Vec<_>>());
    }

    /// Every item is handled exactly once — no item is claimed by two threads
    /// and none is left behind.
    #[test]
    fn every_item_is_handled_once() {
        let items: Vec<usize> = (0..1000).collect();
        let seen = AtomicUsize::new(0);

        map_parallel(&items, |_| {
            seen.fetch_add(1, Ordering::Relaxed);
        });

        assert_eq!(seen.load(Ordering::Relaxed), items.len());
    }

    /// The degenerate cases must not need a thread at all.
    #[test]
    fn a_short_list_is_handled_without_threads() {
        assert_eq!(worker_count(0), 1);
        assert_eq!(worker_count(1), 1);
        assert!(worker_count(1000) <= MAX_WORKERS);

        assert!(map_parallel::<usize, usize, _>(&[], |item| *item).is_empty());
        assert_eq!(map_parallel(&[7], |item| item * 2), vec![14]);
    }
}
