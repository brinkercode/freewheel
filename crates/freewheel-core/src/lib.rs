//! freewheel-core: platform-free logic of the freewheel macro pad.

pub mod hid;

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
