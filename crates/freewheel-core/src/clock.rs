//! The real [`Clock`], so time-based logic can be tested with a fake clock.

use std::time::Instant;

pub use crate::auth::Clock;

/// The real clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
