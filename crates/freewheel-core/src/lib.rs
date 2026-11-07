//! freewheel-core: platform-free logic of the freewheel macro pad.

pub mod auth;
pub mod battery;
pub mod clock;
pub mod config;
pub mod connection;
pub mod ddc;
pub mod diagnostics;
pub mod engine;
pub mod first_run;
pub mod hid;
pub mod inject;
pub mod launch;
pub mod logging;
pub mod overrides;
pub mod permissions;
pub mod resolve;
pub mod simulator;
pub mod status;
pub mod support;
#[cfg(test)]
mod ts_bindings;

/// The application name shown by the tray and the settings window.
pub const APP_NAME: &str = "freewheel";

pub mod report;

#[cfg(test)]
mod tests {
    #[test]
    fn app_name_is_freewheel() {
        assert_eq!(super::APP_NAME, "freewheel");
    }
}
