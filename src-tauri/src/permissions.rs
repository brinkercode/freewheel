//! The macOS permission checks behind the tray's 'Needs permission' state and
//! the settings window's Permissions section, and `open_permission_pane`.
//! Input Monitoring is checked too: see ADR-002 in `freewheel_core::permissions`.
//!
//! [manual] P5, macOS 15: with Accessibility and Input Monitoring revoked the
//! tray shows 'Needs permission (click to fix)', and open_permission_pane lands
//! on Privacy & Security > Accessibility and > Input Monitoring.

use std::collections::BTreeMap;

use freewheel_core::permissions::{Grant, Permission, Permissions, check_all};

/// The permissions this OS needs; none outside macOS.
pub fn current() -> Permissions {
    if cfg!(target_os = "macos") {
        check_all(granted)
    } else {
        Permissions::new()
    }
}

#[cfg(target_os = "macos")]
fn granted(permission: Permission) -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> u8;
    }
    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOHIDCheckAccess(request: u32) -> u32;
    }
    const LISTEN_EVENT: u32 = 1; // kIOHIDRequestTypeListenEvent
    const ACCESS_GRANTED: u32 = 0; // kIOHIDAccessTypeGranted
    // SAFETY: both take plain values and only read the process's TCC state.
    match permission {
        Permission::Accessibility => unsafe { AXIsProcessTrusted() != 0 },
        Permission::InputMonitoring => unsafe { IOHIDCheckAccess(LISTEN_EVENT) == ACCESS_GRANTED },
    }
}

#[cfg(not(target_os = "macos"))]
fn granted(_permission: Permission) -> bool {
    true
}

/// What the UI gets: permission name to "Granted" or "Missing".
pub fn to_ui(permissions: &Permissions) -> BTreeMap<&'static str, &'static str> {
    permissions
        .iter()
        .map(|(p, g)| {
            let grant = match g {
                Grant::Granted => "Granted",
                Grant::Missing => "Missing",
            };
            (p.name(), grant)
        })
        .collect()
}

#[tauri::command]
pub fn get_permissions() -> BTreeMap<&'static str, &'static str> {
    to_ui(&current())
}

/// The pane to open for a permission name from the UI.
pub fn pane_for(permission: &str) -> Result<&'static str, String> {
    Permission::from_name(permission)
        .map(Permission::pane_url)
        .ok_or_else(|| format!("unknown permission: {permission}"))
}

/// Opens the System Settings pane of `permission` (no shell).
#[tauri::command]
pub fn open_permission_pane(permission: String) -> Result<(), String> {
    let url = pane_for(&permission)?;
    if !cfg!(target_os = "macos") {
        return Err("permissions are only needed on macOS".into());
    }
    std::process::Command::new("/usr/bin/open")
        .arg(url)
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ui_gets_each_permission_by_name() {
        let p = check_all(|p| p == Permission::Accessibility);
        let ui = to_ui(&p);
        assert_eq!(ui["Accessibility"], "Granted");
        assert_eq!(ui["InputMonitoring"], "Missing");
    }

    #[test]
    fn open_permission_pane_targets_the_matching_pane() {
        assert_eq!(
            pane_for("Accessibility").unwrap(),
            Permission::Accessibility.pane_url()
        );
        assert_eq!(
            pane_for("InputMonitoring").unwrap(),
            Permission::InputMonitoring.pane_url()
        );
        assert!(pane_for("Camera").is_err());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn only_macos_needs_permissions() {
        assert!(current().is_empty());
        assert!(open_permission_pane("Accessibility".into()).is_err());
    }
}
