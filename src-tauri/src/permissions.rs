//! The macOS permission checks behind the tray's 'Needs permission' state and
//! the settings window's Permissions section, and `open_permission_pane`.
//! Input Monitoring is checked too: see ADR-002 in `freewheel_core::permissions`.
//!
//! P5, macOS 15: with Accessibility and Input Monitoring revoked the tray
//! shows 'Needs permission (click to fix)' (CI's macOS job, through
//! `--probe-permissions` inside freewheel.app), and open_permission_pane
//! passes the Accessibility and Input Monitoring pane URLs to its opener.

use std::collections::BTreeMap;

use freewheel_core::permissions::{Grant, Permission, Permissions, check_all, status_line};

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

/// The OS effect behind `open_permission_pane`: opens a System Settings URL.
pub trait PaneOpener {
    fn open(&self, url: &str) -> Result<(), String>;
}

/// Opens the URL with /usr/bin/open (no shell); macOS only.
pub struct SystemOpener;

impl PaneOpener for SystemOpener {
    fn open(&self, url: &str) -> Result<(), String> {
        if !cfg!(target_os = "macos") {
            return Err("permissions are only needed on macOS".into());
        }
        std::process::Command::new("/usr/bin/open")
            .arg(url)
            .spawn()
            .map(drop)
            .map_err(|e| e.to_string())
    }
}

/// Opens the System Settings pane of `permission` through `opener`.
pub fn open_pane_with(opener: &dyn PaneOpener, permission: &str) -> Result<(), String> {
    opener.open(pane_for(permission)?)
}

/// Opens the System Settings pane of `permission` (no shell).
#[tauri::command]
pub fn open_permission_pane(permission: String) -> Result<(), String> {
    open_pane_with(&SystemOpener, &permission)
}

/// `freewheel --probe-permissions`: prints [`probe_report`] and exits. CI's
/// P5 check launches it from inside freewheel.app, so the probe reads the
/// bundle's own grants.
pub const PROBE_FLAG: &str = "--probe-permissions";

/// True when the command line asks for a permission probe.
pub fn probe_requested(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|a| a == PROBE_FLAG)
}

/// One `name=Granted|Missing` line per permission, then `tray=` and the
/// tray's status line (empty when nothing is missing).
pub fn probe_report(permissions: &Permissions) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for (name, grant) in to_ui(permissions) {
        let _ = writeln!(out, "{name}={grant}");
    }
    let _ = writeln!(out, "tray={}", status_line(permissions).unwrap_or(""));
    out
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

    /// Records every URL instead of opening it.
    #[derive(Default)]
    struct StubOpener(std::cell::RefCell<Vec<String>>);

    impl PaneOpener for StubOpener {
        fn open(&self, url: &str) -> Result<(), String> {
            self.0.borrow_mut().push(url.to_owned());
            Ok(())
        }
    }

    #[test]
    fn open_permission_pane_passes_each_matching_privacy_url_to_the_opener() {
        let opener = StubOpener::default();
        open_pane_with(&opener, "Accessibility").unwrap();
        open_pane_with(&opener, "InputMonitoring").unwrap();
        assert_eq!(
            *opener.0.borrow(),
            [
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
            ]
        );
        assert!(open_pane_with(&opener, "Camera").is_err());
        assert_eq!(opener.0.borrow().len(), 2);
    }

    #[test]
    fn the_probe_flag_is_recognised() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(probe_requested(args(&["freewheel", "--probe-permissions"])));
        assert!(!probe_requested(args(&["freewheel"])));
        assert!(!probe_requested(args(&["freewheel", "--simulate"])));
    }

    #[test]
    fn the_probe_report_lists_each_grant_and_the_tray_line() {
        assert_eq!(
            probe_report(&check_all(|_| false)),
            "Accessibility=Missing\nInputMonitoring=Missing\n\
             tray=Needs permission (click to fix)\n"
        );
        assert_eq!(
            probe_report(&check_all(|_| true)),
            "Accessibility=Granted\nInputMonitoring=Granted\ntray=\n"
        );
    }

    /// P5 on macOS 15: with freewheel's Accessibility and Input Monitoring
    /// grants reset, the probe run inside freewheel.app reports both missing
    /// and the tray reads 'Needs permission (click to fix)'. It resets real
    /// TCC grants, so it is ignored by default: only CI's macOS job runs it
    /// (`-- --ignored p5_`), with FREEWHEEL_APP set to the built and
    /// registered bundle.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "resets freewheel's TCC grants; CI's macOS job runs it with --ignored"]
    fn p5_revoked_grants_show_needs_permission_inside_freewheel_app() {
        let app = std::env::var("FREEWHEEL_APP").expect("FREEWHEEL_APP: path to freewheel.app");
        for service in ["Accessibility", "ListenEvent"] {
            let out = std::process::Command::new("/usr/bin/tccutil")
                .args(["reset", service, "com.brinkercode.freewheel"])
                .output()
                .expect("run tccutil");
            assert!(
                out.status.success(),
                "tccutil reset {service} com.brinkercode.freewheel failed \
                 (is the freewheel .app registered?): {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        // Launched through LaunchServices so the app, not this test, is the
        // process TCC checks.
        let dir = std::env::temp_dir().join(format!("freewheel-p5-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (stdout, stderr) = (dir.join("probe.out"), dir.join("probe.err"));
        std::fs::write(&stdout, "").unwrap();
        std::fs::write(&stderr, "").unwrap();
        let status = std::process::Command::new("/usr/bin/open")
            .args(["-n", "-W", "--stdout"])
            .arg(&stdout)
            .arg("--stderr")
            .arg(&stderr)
            .arg(&app)
            .args(["--args", PROBE_FLAG])
            .status()
            .expect("run open");
        let report = std::fs::read_to_string(&stdout).unwrap();
        assert!(
            status.success(),
            "open {app} {PROBE_FLAG} failed: {}",
            std::fs::read_to_string(&stderr).unwrap_or_default()
        );
        let lines: Vec<&str> = report.lines().collect();
        assert!(lines.contains(&"Accessibility=Missing"), "probe: {report}");
        assert!(
            lines.contains(&"InputMonitoring=Missing"),
            "probe: {report}"
        );
        assert!(
            lines.contains(&"tray=Needs permission (click to fix)"),
            "probe: {report}"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn only_macos_needs_permissions() {
        assert!(current().is_empty());
        assert!(open_permission_pane("Accessibility".into()).is_err());
    }
}
