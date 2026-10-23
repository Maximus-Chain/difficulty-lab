//! Minimal work-stealing parallel map over job indices.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Runs `f(0..jobs)` on all cores and returns the results in job order.
pub fn parallel<R: Send>(jobs: usize, f: impl Fn(usize) -> R + Sync) -> Vec<R> {
    parallel_with(jobs, None, f)
}

/// Like [`parallel`], with an explicit thread count (e.g. to avoid flooding a node's RPC queue).
pub fn parallel_with<R: Send>(jobs: usize, threads: Option<usize>, f: impl Fn(usize) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let results = Mutex::new((0..jobs).map(|_| None).collect::<Vec<Option<R>>>());
    let threads = threads.unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()));
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= jobs {
                        break;
                    }
                    let r = f(i);
                    results.lock().unwrap()[i] = Some(r);
                }
            });
        }
    });
    results.into_inner().unwrap().into_iter().map(Option::unwrap).collect()
}
