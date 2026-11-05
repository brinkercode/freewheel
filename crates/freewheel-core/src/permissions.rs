//! macOS permissions freewheel needs, and what the tray shows for them.
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
}

impl Permission {
    pub const ALL: [Permission; 2] = [Permission::Accessibility, Permission::InputMonitoring];

    /// The name the UI and the `open_permission_pane` command use.
    pub fn name(self) -> &'static str {
        match self {
            Permission::Accessibility => "Accessibility",
            Permission::InputMonitoring => "InputMonitoring",
        }
    }

    pub fn from_name(name: &str) -> Option<Permission> {
        Permission::ALL.into_iter().find(|p| p.name() == name)
    }

    /// The System Settings pane where the user grants it.
    pub fn pane_url(self) -> &'static str {
        match self {
            Permission::Accessibility => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            Permission::InputMonitoring => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
            }
        }
    }
}

/// Whether macOS has granted a permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grant {
    Granted,
    Missing,
}

/// Every permission and its grant.
pub type Permissions = BTreeMap<Permission, Grant>;

/// Asks `check` about every permission.
pub fn check_all(mut check: impl FnMut(Permission) -> bool) -> Permissions {
    Permission::ALL
        .into_iter()
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
    fn names_round_trip() {
        for p in Permission::ALL {
            assert_eq!(Permission::from_name(p.name()), Some(p));
        }
        assert_eq!(Permission::from_name("Camera"), None);
    }
}
