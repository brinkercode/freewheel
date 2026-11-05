//! `freewheel --simulate --smoke`: CI's proof that the built and the installed
//! exe start on a stock Windows. It opens the main window, waits for its page
//! to load, then exits instead of staying in the tray.

use std::time::Duration;

use freewheel_core::simulator;

/// The command-line flag that asks for a smoke run (with `--simulate` only).
pub const FLAG: &str = "--smoke";
/// How long the main window's page gets to load before the run fails.
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// True when the command line asks for a smoke run in simulator mode; a
/// smoke run never touches the real device.
pub fn requested(args: impl IntoIterator<Item = String>) -> bool {
    let args: Vec<String> = args.into_iter().collect();
    simulator::requested(args.iter().cloned()) && args.iter().any(|a| a == FLAG)
}

/// The process exit code for a smoke run: 0 once the page loaded, else 1.
pub fn exit_code(loaded: bool) -> i32 {
    if loaded { 0 } else { 1 }
}

/// Fails the run when the page has not loaded within [`TIMEOUT`].
pub fn arm_timeout(app: tauri::AppHandle<tauri::Wry>) {
    std::thread::spawn(move || {
        std::thread::sleep(TIMEOUT);
        tracing::error!("smoke: the main window did not load within {TIMEOUT:?}");
        app.exit(exit_code(false));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn smoke_needs_both_flags() {
        assert!(requested(args(&["freewheel.exe", "--simulate", "--smoke"])));
        assert!(requested(args(&["freewheel.exe", "--smoke", "--simulate"])));
        assert!(!requested(args(&["freewheel.exe", "--smoke"])));
        assert!(!requested(args(&["freewheel.exe", "--simulate"])));
        assert!(!requested(args(&["freewheel.exe"])));
    }

    #[test]
    fn loaded_exits_zero_and_a_timeout_exits_non_zero() {
        assert_eq!(exit_code(true), 0);
        assert_ne!(exit_code(false), 0);
    }
}
