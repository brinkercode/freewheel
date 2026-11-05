//! The production [`Launcher`]: spawns the program directly with an argv
//! array (`Command::new`), never through a shell, so arguments are passed to
//! the OS unchanged and never interpreted (threat T2).

// Wired into the mapping engine by a later card.
#![allow(dead_code)]

use std::process::Command;

use freewheel_core::launch::{LaunchError, Launcher};

pub struct ProcessLauncher;

impl Launcher for ProcessLauncher {
    fn launch(&mut self, path: &str, args: &[String]) -> Result<(), LaunchError> {
        Command::new(path)
            .args(args)
            .spawn()
            .map(drop)
            .map_err(|e| LaunchError(format!("{path}: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_program_is_an_error_not_a_panic() {
        let err = ProcessLauncher
            .launch("/nonexistent/freewheel-test-app", &["$(id)".into()])
            .unwrap_err();
        assert!(err.0.starts_with("/nonexistent/freewheel-test-app: "));
    }
}
