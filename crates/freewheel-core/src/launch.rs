//! Application launching behind the [`Launcher`] trait, so the mapping engine
//! can be tested without starting processes. The production launcher lives
//! in the app crate and spawns the program directly, never through a shell.

use std::fmt;

/// An error from a launcher backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchError(pub String);

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "app launch failed: {}", self.0)
    }
}

impl std::error::Error for LaunchError {}

/// Starts an application with an argv array; no shell sees the arguments.
pub trait Launcher {
    fn launch(&mut self, path: &str, args: &[String]) -> Result<(), LaunchError>;

    /// Starts it in the folder `cwd` (a mappings file's `cwd`); launchers
    /// that start nothing real ignore the folder.
    fn launch_in(
        &mut self,
        path: &str,
        args: &[String],
        cwd: Option<&str>,
    ) -> Result<(), LaunchError> {
        let _ = cwd;
        self.launch(path, args)
    }
}

/// A recording launcher for tests: it starts nothing.
#[derive(Debug, Default)]
pub struct MockLauncher {
    calls: Vec<(String, Vec<String>)>,
    fail: bool,
}

impl MockLauncher {
    pub fn new() -> Self {
        Self::default()
    }

    /// A launcher whose every launch fails (after recording the call).
    pub fn failing() -> Self {
        Self {
            fail: true,
            ..Self::default()
        }
    }

    /// Every launch so far, in order, as (path, args).
    pub fn calls(&self) -> &[(String, Vec<String>)] {
        &self.calls
    }
}

impl Launcher for MockLauncher {
    fn launch(&mut self, path: &str, args: &[String]) -> Result<(), LaunchError> {
        self.calls.push((path.to_string(), args.to_vec()));
        if self.fail {
            return Err(LaunchError(format!("cannot start {path}")));
        }
        Ok(())
    }
}
