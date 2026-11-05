//! Time behind the [`Clock`] trait, so time-based logic can be tested with a
//! fake clock.

use std::time::Instant;

/// A source of monotonic time.
pub trait Clock: Send + Sync {
    /// The current instant.
    fn now(&self) -> Instant;
}

/// The real clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
