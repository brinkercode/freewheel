//! Linux device access: freewheel's udev rule (`linux/60-freewheel.rules`)
//! gives the logged-in user the Speed Editor's hidraw node (USB and BlueZ
//! Bluetooth) and `/dev/uinput` (the virtual keyboard used on Wayland). It is
//! the Linux row of the Permissions screen: the first run explains it and
//! installs it once through `pkexec` (the only privileged step freewheel ever
//! takes); the Device test points at it when opening the device is denied.
//!
//! [manual] A desktop with a Speed Editor: before the rule, the Device test
//! shows "permission denied" with the hint; Install the device rule asks for
//! the password once; after unplugging and plugging in, the device connects.

use std::path::{Path, PathBuf};

/// The rule, as shipped.
pub const RULE: &str = include_str!("../linux/60-freewheel.rules");
/// Its file name (60: before 73-seat-late, which applies `uaccess`).
pub const RULE_NAME: &str = "60-freewheel.rules";
/// Where freewheel installs it.
pub const INSTALL_DIR: &str = "/etc/udev/rules.d";
/// Where udev reads rules from (an admin or a distro package may have put
/// it in any of them).
pub const RULE_DIRS: [&str; 4] = [
    "/etc/udev/rules.d",
    "/run/udev/rules.d",
    "/usr/lib/udev/rules.d",
    "/lib/udev/rules.d",
];

/// The Speed Editor's ids as the kernel's HID_ID names them.
const HID_ID: &str = "00001EDB:0000DA0E";

/// What `pkexec` runs (as root, no other input than the rule file's path in
/// `$1`): install the rule, load uinput, apply the rule to the devices
/// already there.
pub const INSTALL_SCRIPT: &str = "set -e
install -m 0644 \"$1\" /etc/udev/rules.d/60-freewheel.rules
modprobe uinput 2>/dev/null || true
udevadm control --reload-rules
udevadm trigger --subsystem-match=hidraw --action=change
udevadm trigger --subsystem-match=misc --sysname-match=uinput --action=change";

/// What to run by hand when `pkexec` is not there or fails.
pub fn manual_steps(rule: &Path) -> String {
    format!(
        "sudo install -m 0644 '{}' {INSTALL_DIR}/{RULE_NAME} && sudo udevadm control --reload-rules && sudo udevadm trigger",
        rule.display()
    )
}

/// True when the rule is in one of `dirs`.
pub fn rule_installed(dirs: &[&str]) -> bool {
    dirs.iter().any(|d| Path::new(d).join(RULE_NAME).is_file())
}

/// True when this user may open `path` for reading and writing.
pub fn read_write(path: &Path) -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .is_ok()
}

/// The hidraw nodes of every Speed Editor attached now (USB or Bluetooth),
/// from `sysfs` (`<sysfs>/class/hidraw/hidrawN/device/uevent` has HID_ID).
pub fn speed_editor_nodes(sysfs: &Path, dev: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(sysfs.join("class/hidraw")) else {
        return Vec::new();
    };
    let mut nodes: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| {
            std::fs::read_to_string(e.path().join("device/uevent")).is_ok_and(|u| {
                u.lines().any(|l| {
                    l.strip_prefix("HID_ID=")
                        .is_some_and(|id| id.ends_with(HID_ID))
                })
            })
        })
        .map(|e| dev.join(e.file_name()))
        .collect();
    nodes.sort();
    nodes
}

/// Whether freewheel can do its job: the rule is installed, or (another
/// rule or group already did it) `/dev/uinput` and every attached Speed
/// Editor's node are open to this user.
pub fn granted() -> bool {
    if rule_installed(&RULE_DIRS) {
        return true;
    }
    let nodes = speed_editor_nodes(Path::new("/sys"), Path::new("/dev"));
    read_write(Path::new("/dev/uinput")) && nodes.iter().all(|n| read_write(n))
}

/// Why `pkexec` failed, from its exit code (126: the password dialog was
/// dismissed; 127: not authorised or no polkit agent).
pub fn pkexec_failure(code: Option<i32>) -> String {
    match code {
        Some(126) => "cancelled: the device rule was not installed".into(),
        Some(127) => "not allowed: the system did not authorise installing the device rule".into(),
        Some(c) => format!("installing the device rule failed (exit {c})"),
        None => "installing the device rule was interrupted".into(),
    }
}

/// Installs the rule through `pkexec` (one password prompt); returns the
/// line the window shows.
pub fn install() -> Result<String, String> {
    let dir = std::env::temp_dir().join(format!("freewheel-udev-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let rule = dir.join(RULE_NAME);
    std::fs::write(&rule, RULE).map_err(|e| format!("{}: {e}", rule.display()))?;
    let status = std::process::Command::new("pkexec")
        .args(["/bin/sh", "-c", INSTALL_SCRIPT, "freewheel-udev"])
        .arg(&rule)
        .status();
    let result = match status {
        Ok(s) if s.success() => Ok(
            "Installed freewheel's device rule. Unplug your Speed Editor and plug it in again."
                .to_string(),
        ),
        Ok(s) => Err(format!(
            "{}. To install it yourself: {}",
            pkexec_failure(s.code()),
            manual_steps(&rule)
        )),
        Err(e) => Err(format!(
            "pkexec is not available ({e}). To install the rule yourself: {}",
            manual_steps(&rule)
        )),
    };
    // Keep the file for the manual steps only when they are needed.
    if result.is_ok() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_gives_the_speed_editor_and_uinput_to_the_seat() {
        let rules: Vec<&str> = RULE
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .collect();
        assert_eq!(rules.len(), 3, "{rules:?}");
        assert!(rules.iter().all(|r| r.contains("TAG+=\"uaccess\"")));
        assert!(rules[0].contains("ATTRS{idVendor}==\"1edb\", ATTRS{idProduct}==\"da0e\""));
        assert!(rules[1].contains("KERNELS==\"*:1EDB:DA0E.*\""));
        assert!(rules[2].starts_with("KERNEL==\"uinput\""));
        // uaccess must be tagged before 73-seat-late.rules applies it.
        assert!(RULE_NAME.starts_with("60-"));
        assert!(INSTALL_SCRIPT.contains("/etc/udev/rules.d/60-freewheel.rules"));
        assert!(
            INSTALL_SCRIPT.contains("\"$1\""),
            "the path is an argument, not interpolated"
        );
    }

    #[test]
    fn the_rule_is_found_in_any_udev_rules_dir() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().to_str().unwrap();
        assert!(!rule_installed(&[d]));
        std::fs::write(dir.path().join(RULE_NAME), RULE).unwrap();
        assert!(rule_installed(&["/nonexistent", d]));
    }

    #[test]
    fn speed_editor_nodes_are_found_by_hid_id_over_usb_and_bluetooth() {
        let sys = tempfile::tempdir().unwrap();
        for (node, id) in [
            ("hidraw0", "0003:0000046D:0000C52B"),
            ("hidraw3", "0003:00001EDB:0000DA0E"),
            ("hidraw7", "0005:00001EDB:0000DA0E"),
        ] {
            let d = sys.path().join("class/hidraw").join(node).join("device");
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(
                d.join("uevent"),
                format!("DRIVER=hid-generic\nHID_ID={id}\nHID_NAME=x\n"),
            )
            .unwrap();
        }
        assert_eq!(
            speed_editor_nodes(sys.path(), Path::new("/dev")),
            [PathBuf::from("/dev/hidraw3"), PathBuf::from("/dev/hidraw7")]
        );
        assert!(speed_editor_nodes(Path::new("/nonexistent"), Path::new("/dev")).is_empty());
    }

    #[test]
    fn pkexec_failures_say_what_happened() {
        assert!(pkexec_failure(Some(126)).starts_with("cancelled"));
        assert!(pkexec_failure(Some(127)).starts_with("not allowed"));
        assert!(pkexec_failure(Some(1)).contains("exit 1"));
        let steps = manual_steps(Path::new("/tmp/x/60-freewheel.rules"));
        assert!(steps.starts_with(
            "sudo install -m 0644 '/tmp/x/60-freewheel.rules' /etc/udev/rules.d/60-freewheel.rules"
        ));
    }

    #[test]
    fn a_file_this_user_cannot_open_is_not_read_write() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("node");
        std::fs::write(&f, "").unwrap();
        assert!(read_write(&f));
        assert!(!read_write(&dir.path().join("missing")));
    }
}
