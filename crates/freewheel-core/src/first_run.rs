//! Whether the app is on its first run. The first-run flow is not built yet,
//! so only [`FakeFirstRun`] implements [`FirstRun`]; the first-run card adds
//! the real one behind the same trait.

/// The app's first-run state.
pub trait FirstRun {
    fn is_first_run(&self) -> bool;
    /// Puts the app back in its first-run state.
    fn reset(&mut self);
}

/// An in-memory first-run flag; it starts with first run already done.
#[derive(Debug, Clone, Default)]
pub struct FakeFirstRun {
    first_run: bool,
}

impl FirstRun for FakeFirstRun {
    fn is_first_run(&self) -> bool {
        self.first_run
    }
    fn reset(&mut self) {
        self.first_run = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_starts_done_and_reset_returns_to_first_run() {
        let mut f = FakeFirstRun::default();
        assert!(!f.is_first_run());
        f.reset();
        assert!(f.is_first_run());
    }
}
