//! The permissions freewheel needs (macOS: Accessibility and Input
//! Monitoring; Linux: the device rule), and what the tray shows for them.
//!
//! Input Monitoring: ADR-002 (HID access through the hidapi crate) leaves open
//! whether macOS needs Input Monitoring before hidapi can read the Speed
//! Editor; its default, followed here, is to check and show both
//! Accessibility and Input Monitoring.

use std::collections::BTreeMap;

/// A permission freewheel asks macOS for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Permission {
    /// Key injection (AXIsProcessTrusted).
    Accessibility,
    /// Reading the Speed Editor through hidapi (ADR-002).
    InputMonitoring,
    /// Linux: freewheel's udev rule (`60-freewheel.rules`), which gives the
    /// logged-in user the Speed Editor's hidraw node and `/dev/uinput`.
    DeviceAccess,
}

impl Permission {
    /// The macOS permissions.
    pub const ALL: [Permission; 2] = [Permission::Accessibility, Permission::InputMonitoring];
    /// The Linux permission: the device rule.
    pub const LINUX: [Permission; 1] = [Permission::DeviceAccess];

    /// The name the UI and the `open_permission_pane` command use.
    pub fn name(self) -> &'static str {
        match self {
            Permission::Accessibility => "Accessibility",
            Permission::InputMonitoring => "InputMonitoring",
            Permission::DeviceAccess => "DeviceAccess",
        }
    }

    pub fn from_name(name: &str) -> Option<Permission> {
        Permission::ALL
            .into_iter()
            .chain(Permission::LINUX)
            .find(|p| p.name() == name)
    }

    /// The System Settings pane where the user grants it (none for the
    /// Linux device rule: freewheel installs it).
    pub fn pane_url(self) -> &'static str {
        match self {
            Permission::Accessibility => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            Permission::InputMonitoring => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
            }
            Permission::DeviceAccess => "",
        }
    }
}

/// Whether macOS has granted a permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum Grant {
    Granted,
    Missing,
}

/// Every permission and its grant.
pub type Permissions = BTreeMap<Permission, Grant>;

/// Asks `check` about every macOS permission.
pub fn check_all(check: impl FnMut(Permission) -> bool) -> Permissions {
    check_each(Permission::ALL, check)
}

/// Asks `check` about each of `list`.
pub fn check_each(
    list: impl IntoIterator<Item = Permission>,
    mut check: impl FnMut(Permission) -> bool,
) -> Permissions {
    list.into_iter()
        .map(|p| {
            (
                p,
                if check(p) {
                    Grant::Granted
                } else {
                    Grant::Missing
                },
            )
        })
        .collect()
}

/// Whether any permission is missing.
pub fn any_missing(permissions: &Permissions) -> bool {
    permissions.values().any(|g| *g == Grant::Missing)
}

/// The tray status line while a permission is missing.
pub const NEEDS_PERMISSION: &str = "Needs permission (click to fix)";

/// The tray status line: `Some` while a permission is missing.
pub fn status_line(permissions: &Permissions) -> Option<&'static str> {
    any_missing(permissions).then_some(NEEDS_PERMISSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_permission_is_checked() {
        let p = check_all(|p| p == Permission::Accessibility);
        assert_eq!(p[&Permission::Accessibility], Grant::Granted);
        assert_eq!(p[&Permission::InputMonitoring], Grant::Missing);
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn the_status_line_reads_needs_permission_only_while_one_is_missing() {
        assert_eq!(status_line(&check_all(|_| true)), None);
        assert_eq!(
            status_line(&check_all(|p| p != Permission::Accessibility)),
            Some("Needs permission (click to fix)")
        );
        assert_eq!(
            status_line(&check_all(|p| p != Permission::InputMonitoring)),
            Some(NEEDS_PERMISSION)
        );
    }

    #[test]
    fn each_permission_opens_its_own_privacy_pane() {
        assert!(
            Permission::Accessibility
                .pane_url()
                .ends_with("?Privacy_Accessibility")
        );
        assert!(
            Permission::InputMonitoring
                .pane_url()
                .ends_with("?Privacy_ListenEvent")
        );
        for p in Permission::ALL {
            assert!(
                p.pane_url()
                    .starts_with("x-apple.systempreferences:com.apple.preference.security")
            );
        }
    }

    #[test]
    fn linux_asks_only_for_the_device_rule() {
        let p = check_each(Permission::LINUX, |_| false);
        assert_eq!(p.len(), 1);
        assert_eq!(p[&Permission::DeviceAccess], Grant::Missing);
        assert_eq!(status_line(&p), Some(NEEDS_PERMISSION));
        assert_eq!(status_line(&check_each(Permission::LINUX, |_| true)), None);
        assert_eq!(Permission::DeviceAccess.pane_url(), "");
    }

    #[test]
    fn names_round_trip() {
        for p in Permission::ALL.into_iter().chain(Permission::LINUX) {
            assert_eq!(Permission::from_name(p.name()), Some(p));
        }
        assert_eq!(Permission::from_name("Camera"), None);
    }
}
