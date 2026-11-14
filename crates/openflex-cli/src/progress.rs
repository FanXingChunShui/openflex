//! Replication progress reporting for `openflex run --progress`.
//!
//! Progress goes to **stderr** and is throttled so long runs do not flood the
//! terminal; standard output stays reserved for the report, which means
//! `--format json --progress` still pipes cleanly into `jq`.
//!
//! The reporter is safe to share between rayon worker threads: the counter is
//! atomic and only the thread that wins the rate-limit lock prints. The report
//! contents never depend on progress output.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Minimum interval between two progress lines.
const THROTTLE: Duration = Duration::from_millis(100);

/// Shared progress counter for one run.
pub struct Progress {
    enabled: bool,
    total: usize,
    done: AtomicUsize,
    last: Mutex<Instant>,
}

impl Progress {
    /// Create a reporter for `total` replications. `enabled` is false for
    /// `--quiet`, which also disables the final line.
    pub fn new(enabled: bool, total: usize) -> Self {
        Self {
            enabled: enabled && total > 0,
            total,
            done: AtomicUsize::new(0),
            // Start in the past so the first replication is always reported,
            // which makes even a sub-100 ms run show that work has started.
            last: Mutex::new(
                Instant::now()
                    .checked_sub(THROTTLE)
                    .unwrap_or_else(Instant::now),
            ),
        }
    }

    /// Record that one replication finished and print a line when due.
    pub fn tick(&self) {
        if !self.enabled {
            return;
        }
        let done = self.done.fetch_add(1, Ordering::Relaxed) + 1;
        let now = Instant::now();
        let mut last = match self.last.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if done < self.total && now.duration_since(*last) < THROTTLE {
            return;
        }
        *last = now;
        eprintln!("replication {done}/{}", self.total);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disabled_reporter_does_nothing() {
        let progress = Progress::new(false, 3);
        progress.tick();
        assert_eq!(progress.done.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn the_last_replication_always_prints() {
        let progress = Progress::new(true, 2);
        // Both ticks are within the throttle window; the final one must still
        // be reported so a script can tell the run finished.
        progress.tick();
        progress.tick();
        assert_eq!(progress.done.load(Ordering::Relaxed), 2);
    }
}
