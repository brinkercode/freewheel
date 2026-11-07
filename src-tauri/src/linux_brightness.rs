//! Linux brightness: the laptop panel through the kernel's backlight class
//! (`/sys/class/backlight/*`, read directly; written through
//! systemd-logind's `Session.SetBrightness` over D-Bus, which lets the
//! active session's user do it without root), and external monitors over
//! DDC/CI on their DRM connector's I2C bus (`/dev/i2c-N`, open to the user
//! when they are in the `i2c` group or a udev rule says so). Each source
//! that is not there says why; the dial fails only when all of them do.
//!
//! [manual] A laptop on GNOME: the dial on Change screen brightness moves
//! the panel and the GNOME brightness slider; with an external monitor and
//! `i2c-dev` loaded, the monitor follows.

use std::path::{Path, PathBuf};

use freewheel_core::ddc::{DdcBrightness, DdcDisplay, REPLY_LEN};
use freewheel_core::overrides::{BrightnessControl, ControlError, Level, Screen, step_displays};

/// What a missing source says.
pub const NO_BACKLIGHT: &str = "no built-in display backlight";
pub const NO_DDC: &str =
    "no external display answers DDC/CI (is i2c-dev loaded and /dev/i2c-* open to you?)";

/// Reads a sysfs number.
fn read_number(path: &Path) -> Result<u32, ControlError> {
    std::fs::read_to_string(path)
        .map_err(|e| ControlError(format!("{}: {e}", path.display())))?
        .trim()
        .parse()
        .map_err(|e| ControlError(format!("{}: {e}", path.display())))
}

/// Sets a backlight's brightness (the device's name and its raw value).
pub type SetBacklight = fn(&str, u32) -> Result<(), ControlError>;

/// One `/sys/class/backlight/<name>` device.
pub struct Backlight {
    pub dir: PathBuf,
    pub set: SetBacklight,
}

impl Backlight {
    fn name_str(&self) -> String {
        self.dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

impl Screen for Backlight {
    fn name(&self) -> String {
        format!("backlight {}", self.name_str())
    }
    fn level(&mut self) -> Result<Level, ControlError> {
        Ok(Level {
            // 0 can switch some panels off: keep 1 % lit.
            min: read_number(&self.dir.join("max_brightness"))? / 100,
            current: read_number(&self.dir.join("brightness"))?,
            max: read_number(&self.dir.join("max_brightness"))?,
        })
    }
    fn set(&mut self, value: u32) -> Result<(), ControlError> {
        (self.set)(&self.name_str(), value)
    }
}

/// The backlights under `class` (`/sys/class/backlight`); all are stepped.
pub fn backlights(class: &Path, set: SetBacklight) -> Vec<Backlight> {
    let Ok(entries) = std::fs::read_dir(class) else {
        return Vec::new();
    };
    let mut out: Vec<Backlight> = entries
        .flatten()
        .map(|e| Backlight { dir: e.path(), set })
        .filter(|b| b.dir.join("max_brightness").exists())
        .collect();
    out.sort_by(|a, b| a.dir.cmp(&b.dir));
    out
}

/// logind's `SetBrightness("backlight", name, value)` on this session;
/// writing the sysfs file directly when that is open to us (an older
/// systemd or a udev rule).
pub fn logind_set(name: &str, value: u32) -> Result<(), ControlError> {
    let via_logind = (|| -> zbus::Result<()> {
        let conn = zbus::blocking::Connection::system()?;
        conn.call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1/session/auto",
            Some("org.freedesktop.login1.Session"),
            "SetBrightness",
            &("backlight", name, value),
        )?;
        Ok(())
    })();
    match via_logind {
        Ok(()) => Ok(()),
        Err(e) => {
            let file = Path::new("/sys/class/backlight")
                .join(name)
                .join("brightness");
            std::fs::write(&file, value.to_string()).map_err(|w| {
                ControlError(format!(
                    "backlight {name}: logind: {e}; {}: {w}",
                    file.display()
                ))
            })
        }
    }
}

/// The built-in panel(s).
pub struct BacklightBrightness {
    class: PathBuf,
    set: SetBacklight,
}

impl BacklightBrightness {
    pub fn new(class: PathBuf, set: SetBacklight) -> Self {
        Self { class, set }
    }
    fn step(&mut self, percent: i32) -> Result<(), ControlError> {
        let screens: Vec<Result<Box<dyn Screen>, ControlError>> = backlights(&self.class, self.set)
            .into_iter()
            .map(|b| Ok(Box::new(b) as Box<dyn Screen>))
            .collect();
        if screens.is_empty() {
            return Err(ControlError(NO_BACKLIGHT.into()));
        }
        step_displays(screens, percent)
    }
}

impl BrightnessControl for BacklightBrightness {
    fn step_up(&mut self, step: u8) -> Result<(), ControlError> {
        self.step(step.into())
    }
    fn step_down(&mut self, step: u8) -> Result<(), ControlError> {
        self.step(-i32::from(step))
    }
}

/// The I2C buses of the DRM connectors (`<drm>/card*-*/i2c-N` or `ddc`):
/// only the buses that go to a monitor are ever written to.
pub fn ddc_buses(drm: &Path) -> Vec<(String, u32)> {
    let Ok(entries) = std::fs::read_dir(drm) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let connector = e.file_name().to_string_lossy().into_owned();
        // card0-DP-1, card1-HDMI-A-1; not card0 itself or renderD128.
        if !connector.starts_with("card") || !connector.contains('-') {
            continue;
        }
        let mut bus = std::fs::read_dir(e.path()).ok().and_then(|inner| {
            inner.flatten().find_map(|i| {
                i.file_name()
                    .to_str()?
                    .strip_prefix("i2c-")?
                    .parse::<u32>()
                    .ok()
            })
        });
        if bus.is_none() {
            bus = std::fs::read_link(e.path().join("ddc"))
                .ok()
                .and_then(|l| l.file_name()?.to_str()?.strip_prefix("i2c-")?.parse().ok());
        }
        if let Some(n) = bus {
            out.push((connector, n));
        }
    }
    out.sort();
    out.dedup_by_key(|(_, n)| *n);
    out
}

/// I2C_SLAVE: the address the next reads and writes go to.
const I2C_SLAVE: libc::c_ulong = 0x0703;
/// A monitor needs a moment between a request and its reply.
const REPLY_DELAY: std::time::Duration = std::time::Duration::from_millis(50);

/// A monitor on `/dev/i2c-N`.
struct I2cMonitor {
    name: String,
    file: std::fs::File,
}

impl I2cMonitor {
    fn open(connector: &str, bus: u32) -> Result<Self, ControlError> {
        use std::os::fd::AsRawFd;
        let path = format!("/dev/i2c-{bus}");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| ControlError(format!("{connector} ({path}): {e}")))?;
        // SAFETY: I2C_SLAVE takes the 7-bit address as an int.
        if unsafe {
            libc::ioctl(
                file.as_raw_fd(),
                I2C_SLAVE,
                libc::c_ulong::from(freewheel_core::ddc::DDC_ADDRESS),
            )
        } < 0
        {
            return Err(ControlError(format!(
                "{connector} ({path}): I2C_SLAVE: {}",
                std::io::Error::last_os_error()
            )));
        }
        Ok(Self {
            name: connector.to_string(),
            file,
        })
    }
}

impl DdcDisplay for I2cMonitor {
    fn name(&self) -> String {
        self.name.clone()
    }
    fn write(&mut self, packet: &[u8]) -> Result<(), ControlError> {
        use std::io::Write;
        self.file
            .write_all(packet)
            .map_err(|e| ControlError(format!("DDC write: {e}")))
    }
    fn read(&mut self, request: &[u8], reply: &mut [u8]) -> Result<(), ControlError> {
        use std::io::Read;
        self.write(request)?;
        std::thread::sleep(REPLY_DELAY);
        let n = reply.len().min(REPLY_LEN);
        self.file
            .read_exact(&mut reply[..n])
            .map_err(|e| ControlError(format!("DDC read: {e}")))
    }
}

/// The monitors that open; an error naming each that does not when none does.
fn probe() -> Result<Vec<Box<dyn DdcDisplay>>, ControlError> {
    let buses = ddc_buses(Path::new("/sys/class/drm"));
    if buses.is_empty() {
        return Err(ControlError(NO_DDC.into()));
    }
    let mut monitors: Vec<Box<dyn DdcDisplay>> = Vec::new();
    let mut errors = Vec::new();
    for (connector, bus) in buses {
        match I2cMonitor::open(&connector, bus) {
            Ok(m) => monitors.push(Box::new(m)),
            Err(e) => errors.push(e.0),
        }
    }
    if monitors.is_empty() {
        Err(ControlError(format!("{NO_DDC}: {}", errors.join("; "))))
    } else {
        Ok(monitors)
    }
}

/// External monitors over DDC/CI.
pub fn ddc_brightness() -> DdcBrightness {
    DdcBrightness::new(Box::new(probe))
}

/// Every display this computer has: the panel and the monitors.
pub fn brightness() -> crate::controls::EveryDisplay {
    crate::controls::EveryDisplay(vec![
        Box::new(BacklightBrightness::new(
            PathBuf::from("/sys/class/backlight"),
            logind_set,
        )),
        Box::new(ddc_brightness()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static SET: Mutex<Vec<(String, u32)>> = Mutex::new(Vec::new());
    fn record(name: &str, value: u32) -> Result<(), ControlError> {
        SET.lock().unwrap().push((name.to_string(), value));
        Ok(())
    }

    #[test]
    fn the_panel_steps_by_percent_of_its_range_through_logind() {
        let class = tempfile::tempdir().unwrap();
        let panel = class.path().join("intel_backlight");
        std::fs::create_dir_all(&panel).unwrap();
        std::fs::write(panel.join("max_brightness"), "1000\n").unwrap();
        std::fs::write(panel.join("brightness"), "500\n").unwrap();
        let mut b = BacklightBrightness::new(class.path().to_path_buf(), record);
        b.step_up(10).unwrap();
        std::fs::write(panel.join("brightness"), "15\n").unwrap();
        b.step_down(10).unwrap();
        let set = SET.lock().unwrap().clone();
        // 10 % of the 10..=1000 range is 99; down stops at the 1 % floor.
        assert_eq!(
            set,
            [
                ("intel_backlight".to_string(), 599),
                ("intel_backlight".to_string(), 10)
            ]
        );
        let none = tempfile::tempdir().unwrap();
        let e = BacklightBrightness::new(none.path().to_path_buf(), record)
            .step_up(5)
            .unwrap_err();
        assert_eq!(e.0, NO_BACKLIGHT);
    }

    #[test]
    fn only_the_display_connectors_i2c_buses_are_used() {
        let drm = tempfile::tempdir().unwrap();
        for d in [
            "card0",
            "renderD128",
            "card0-eDP-1",
            "card1-HDMI-A-1/i2c-7",
            "card1-DP-2/i2c-9",
            "card1-DP-3",
        ] {
            std::fs::create_dir_all(drm.path().join(d)).unwrap();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink("../../i2c-4", drm.path().join("card0-eDP-1/ddc")).unwrap();
        let buses = ddc_buses(drm.path());
        assert_eq!(
            buses,
            [
                ("card0-eDP-1".to_string(), 4),
                ("card1-DP-2".to_string(), 9),
                ("card1-HDMI-A-1".to_string(), 7)
            ]
        );
        assert!(ddc_buses(Path::new("/nonexistent")).is_empty());
    }
}
