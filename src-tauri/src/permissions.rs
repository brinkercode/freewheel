//! The permission checks behind the tray's 'Needs permission' state and
//! the settings window's Permissions section, and `open_permission_pane`:
//! macOS's privacy panes, and on Linux freewheel's device rule
//! (linux_udev.rs), which `open_permission_pane` installs.
//! Input Monitoring is checked too: see ADR-002 in `freewheel_core::permissions`.
//!
//! P5, macOS 15: with Accessibility and Input Monitoring revoked the tray
//! shows 'Needs permission (click to fix)' (CI's macOS job, through
//! `--probe-permissions` inside freewheel.app), and open_permission_pane
//! passes the Accessibility and Input Monitoring pane URLs to its opener
//! (the opener plugin). In `--simulate` it opens nothing and names the pane.

use std::collections::BTreeMap;

use freewheel_core::permissions::{
    Grant, Permission, Permissions, check_all, check_each, status_line,
};

/// The permissions this OS needs: macOS's two, Linux's device rule, none
/// on Windows.
pub fn current() -> Permissions {
    if cfg!(target_os = "macos") {
        check_all(granted)
    } else if cfg!(target_os = "linux") {
        check_each(Permission::LINUX, granted)
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
        Permission::DeviceAccess => true,
    }
}

#[cfg(target_os = "linux")]
fn granted(permission: Permission) -> bool {
    permission != Permission::DeviceAccess || crate::linux_udev::granted()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
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

/// Opens the URL through the opener plugin (no shell); macOS only.
pub struct AppOpener<'a, R: tauri::Runtime>(pub &'a tauri::AppHandle<R>);

impl<R: tauri::Runtime> PaneOpener for AppOpener<'_, R> {
    fn open(&self, url: &str) -> Result<(), String> {
        if !cfg!(target_os = "macos") {
            return Err("permissions are only needed on macOS".into());
        }
        use tauri_plugin_opener::OpenerExt;
        self.0
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| format!("{url}: {e}"))
    }
}

/// Opens the System Settings pane of `permission` through `opener`.
pub fn open_pane_with(opener: &dyn PaneOpener, permission: &str) -> Result<(), String> {
    opener.open(pane_for(permission)?)
}

/// The pane's name as System Settings shows it.
fn pane_label(permission: Permission) -> &'static str {
    match permission {
        Permission::Accessibility => "Accessibility",
        Permission::InputMonitoring => "Input Monitoring",
        Permission::DeviceAccess => "Device rule",
    }
}

/// What `--simulate` says instead of opening the pane: it fakes the macOS
/// permissions on any OS, and the Simulator's "macOS permission" grants them.
pub fn simulated_pane(permission: &str) -> Result<String, String> {
    let p = Permission::from_name(permission)
        .ok_or_else(|| format!("unknown permission: {permission}"))?;
    Ok(format!(
        "Simulated: this would open System Settings › Privacy & Security › {}. \
         Allow it in the Simulator (macOS permission: Allowed).",
        pane_label(p)
    ))
}

/// What `open_permission_pane` does for `app`: in `--simulate` (the
/// simulator session is managed) nothing opens and the UI gets the
/// [`simulated_pane`] line to show; else macOS opens the pane (`None`).
pub fn open_pane_for<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    permission: &str,
) -> Result<Option<String>, String> {
    use tauri::Manager;
    if app.try_state::<crate::simulate::SimHandle>().is_some() {
        return simulated_pane(permission).map(Some);
    }
    #[cfg(target_os = "linux")]
    if Permission::from_name(permission) == Some(Permission::DeviceAccess) {
        return crate::linux_udev::install().map(Some);
    }
    open_pane_with(&AppOpener(app), permission).map(|()| None)
}

/// Opens the System Settings pane of `permission` (no shell); in
/// `--simulate` returns the line the UI shows instead.
#[tauri::command]
pub fn open_permission_pane(
    app: tauri::AppHandle,
    permission: String,
) -> Result<Option<String>, String> {
    let result = open_pane_for(&app, &permission);
    if let Err(error) = &result {
        tracing::warn!(%error, permission, "opening the permission pane failed");
    }
    result
}

/// The Linux desktops' Bluetooth settings (GNOME, KDE Plasma, Blueman),
/// each a program and its argument.
pub const LINUX_BLUETOOTH: [(&str, &str); 3] = [
    ("gnome-control-center", "bluetooth"),
    ("systemsettings", "kcm_bluetooth"),
    ("blueman-manager", ""),
];

/// The first of `programs` found in `path` (a `PATH` value).
pub fn first_in_path<'a>(
    programs: &[(&'a str, &'a str)],
    path: &std::ffi::OsStr,
) -> Option<(&'a str, &'a str)> {
    programs
        .iter()
        .copied()
        .find(|(program, _)| std::env::split_paths(path).any(|dir| dir.join(program).is_file()))
}

/// The program and URL that open the OS Bluetooth settings, with no shell:
/// System Settings > Bluetooth on macOS 13+, Settings > Bluetooth & devices
/// on Windows, the desktop's Bluetooth settings on Linux.
pub fn bluetooth_settings() -> Result<(&'static str, &'static str), String> {
    if cfg!(target_os = "linux") {
        let path = std::env::var_os("PATH").unwrap_or_default();
        return first_in_path(&LINUX_BLUETOOTH, &path)
            .ok_or_else(|| "no Bluetooth settings app found (GNOME, KDE or Blueman)".into());
    }
    if cfg!(target_os = "macos") {
        Ok((
            "/usr/bin/open",
            "x-apple.systempreferences:com.apple.Bluetooth-Settings.extension",
        ))
    } else if cfg!(windows) {
        Ok(("explorer.exe", "ms-settings:bluetooth"))
    } else {
        Err("Bluetooth settings open only on macOS, Windows and Linux".into())
    }
}

/// Opens the OS Bluetooth settings for the first run's 'Open Bluetooth settings'.
#[tauri::command]
pub fn open_bluetooth_settings() -> Result<(), String> {
    let result = bluetooth_settings().and_then(|(program, url)| {
        std::process::Command::new(program)
            .args((!url.is_empty()).then_some(url))
            .spawn()
            .map(drop)
            .map_err(|e| format!("{program} {url}: {e}"))
    });
    if let Err(error) = &result {
        tracing::warn!(%error, "opening the Bluetooth settings failed");
    }
    result
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
    fn bluetooth_settings_open_the_os_pane() {
        let target = bluetooth_settings();
        if cfg!(target_os = "macos") {
            assert_eq!(
                target.unwrap(),
                (
                    "/usr/bin/open",
                    "x-apple.systempreferences:com.apple.Bluetooth-Settings.extension"
                )
            );
        } else if cfg!(windows) {
            assert_eq!(target.unwrap(), ("explorer.exe", "ms-settings:bluetooth"));
        } else if let Ok(t) = target {
            assert!(LINUX_BLUETOOTH.contains(&t));
        }
    }

    #[test]
    fn linux_opens_the_first_bluetooth_settings_app_it_finds() {
        let dir = tempfile::tempdir().unwrap();
        let path =
            std::env::join_paths([dir.path().join("none"), dir.path().to_path_buf()]).unwrap();
        assert_eq!(first_in_path(&LINUX_BLUETOOTH, &path), None);
        std::fs::write(dir.path().join("systemsettings"), "").unwrap();
        assert_eq!(
            first_in_path(&LINUX_BLUETOOTH, &path),
            Some(("systemsettings", "kcm_bluetooth"))
        );
        std::fs::write(dir.path().join("gnome-control-center"), "").unwrap();
        assert_eq!(
            first_in_path(&LINUX_BLUETOOTH, &path),
            Some(("gnome-control-center", "bluetooth"))
        );
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
        let dir = std::env::temp_dir().join(format!("freewheel-permcheck-{}", std::process::id()));
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

    #[cfg(windows)]
    #[test]
    fn windows_needs_no_permissions() {
        assert!(current().is_empty());
        let app = tauri::test::mock_app();
        assert!(open_pane_for(app.handle(), "Accessibility").is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_needs_only_the_device_rule() {
        let p = current();
        assert_eq!(
            p.keys().copied().collect::<Vec<_>>(),
            [Permission::DeviceAccess]
        );
        assert_eq!(
            to_ui(&p).keys().copied().collect::<Vec<_>>(),
            ["DeviceAccess"]
        );
        let app = tauri::test::mock_app();
        assert!(open_pane_for(app.handle(), "Accessibility").is_err());
    }

    #[test]
    fn simulate_names_the_pane_it_would_open_and_how_to_grant_it() {
        use tauri::Manager;
        let app = tauri::test::mock_app();
        app.manage(crate::simulate::SimHandle::new());
        let said = open_pane_for(app.handle(), "InputMonitoring")
            .unwrap()
            .unwrap();
        assert_eq!(
            said,
            "Simulated: this would open System Settings › Privacy & Security › Input Monitoring. \
             Allow it in the Simulator (macOS permission: Allowed)."
        );
        assert!(said.contains("Allowed"));
        assert!(
            simulated_pane("Accessibility")
                .unwrap()
                .contains("Privacy & Security › Accessibility.")
        );
        assert!(open_pane_for(app.handle(), "Camera").is_err());
    }
}
