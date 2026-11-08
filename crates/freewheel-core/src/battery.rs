//! The battery tray tooltip and the once-only low-battery notification.

/// The Speed Editor's last battery report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Battery {
    pub level: u8,
    pub charging: bool,
}

/// The tray tooltip: `Speed Editor 64% (charging)` while battery data is
/// known, otherwise only the device `status`.
pub fn tooltip(status: &str, battery: Option<Battery>) -> String {
    match battery {
        Some(Battery { level, charging }) => {
            let charging = if charging { " (charging)" } else { "" };
            format!("Speed Editor {level}%{charging}")
        }
        None => status.to_string(),
    }
}

/// Whether `level` is low: at or below `threshold`
/// (`settings.low_battery_threshold`, SCHEMA.md `low_battery_warning`).
pub fn is_low(level: u8, threshold: u8) -> bool {
    level <= threshold
}

/// Fires once when the level drops to the threshold or below; re-arms after
/// the device charges or rises back above the threshold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowBatteryAlert {
    armed: bool,
}

impl Default for LowBatteryAlert {
    fn default() -> Self {
        Self { armed: true }
    }
}

impl LowBatteryAlert {
    /// Takes a battery report; returns whether to notify now.
    pub fn update(&mut self, battery: Battery, threshold: u8) -> bool {
        if battery.charging || !is_low(battery.level, threshold) {
            self.armed = true;
            return false;
        }
        std::mem::replace(&mut self.armed, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(level: u8, charging: bool) -> Battery {
        Battery { level, charging }
    }

    #[test]
    fn tooltip_shows_level_and_charging() {
        assert_eq!(
            tooltip("Connected", Some(at(64, true))),
            "Speed Editor 64% (charging)"
        );
        assert_eq!(
            tooltip("Connected", Some(at(64, false))),
            "Speed Editor 64%"
        );
    }

    #[test]
    fn tooltip_without_battery_data_shows_only_the_status() {
        assert_eq!(tooltip("Looking for device", None), "Looking for device");
    }

    #[test]
    fn notifies_once_at_or_below_the_threshold() {
        let mut alert = LowBatteryAlert::default();
        assert!(!alert.update(at(16, false), 15));
        // SCHEMA.md: "at or below this percent".
        assert!(alert.update(at(15, false), 15));
        assert!(!alert.update(at(14, false), 15));
        assert!(!alert.update(at(5, false), 15));
    }

    #[test]
    fn re_arms_after_rising_above_the_threshold() {
        let mut alert = LowBatteryAlert::default();
        assert!(alert.update(at(14, false), 15));
        assert!(!alert.update(at(15, false), 15));
        assert!(!alert.update(at(16, false), 15));
        assert!(alert.update(at(15, false), 15));
    }

    #[test]
    fn low_is_at_or_below_the_threshold() {
        assert!(is_low(15, 15));
        assert!(is_low(0, 0));
        assert!(!is_low(16, 15));
        assert!(!is_low(1, 0));
    }

    #[test]
    fn re_arms_after_charging_and_never_fires_while_charging() {
        let mut alert = LowBatteryAlert::default();
        assert!(alert.update(at(10, false), 15));
        assert!(!alert.update(at(10, true), 15));
        assert!(alert.update(at(10, false), 15));
        assert!(!alert.update(at(3, true), 15));
    }
}
