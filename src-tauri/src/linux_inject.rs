//! Linux key injection. An X11 session types through XTest (enigo, the
//! default [`crate::inject::EnigoInjector`]); a Wayland session has no
//! XTest for its native apps, so freewheel creates a virtual keyboard with
//! `/dev/uinput` ([`UinputInjector`]), which the compositor reads like a real
//! one. `/dev/uinput` is opened to the logged-in user by freewheel's udev
//! rule (linux_udev.rs). Media keys go through the same paths (XF86 keysyms
//! on X11, KEY_PLAYPAUSE... on uinput). Dropping the virtual keyboard
//! destroys it, which makes the kernel release any key still held.
//!
//! [manual] GNOME on Wayland with the rule installed: a Speed Editor key
//! mapped to Ctrl+C copies in a native GTK app.

use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::Path;

use freewheel_core::inject::{self as media, InjectError, KeyInjector, Keystroke, Modifier};

/// The kind of graphical session freewheel runs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    X11,
    Wayland,
    /// Neither (a console, or the environment says nothing).
    Unknown,
}

/// The session from its environment: `XDG_SESSION_TYPE` first, then
/// `WAYLAND_DISPLAY` / `DISPLAY`.
pub fn session(var: impl Fn(&str) -> Option<String>) -> Session {
    match var("XDG_SESSION_TYPE").as_deref() {
        Some("wayland") => return Session::Wayland,
        Some("x11") => return Session::X11,
        _ => {}
    }
    let set = |name| var(name).is_some_and(|v| !v.is_empty());
    if set("WAYLAND_DISPLAY") {
        Session::Wayland
    } else if set("DISPLAY") {
        Session::X11
    } else {
        Session::Unknown
    }
}

/// This process's session.
pub fn current_session() -> Session {
    session(|name| std::env::var(name).ok())
}

// linux/input-event-codes.h
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const SYN_REPORT: u16 = 0;
const REL_WHEEL: u16 = 0x08;
const BUS_VIRTUAL: u16 = 0x06;

/// The Linux key code of a freewheel key name (a US layout for single
/// characters: what the key's label says on a US keyboard).
pub fn key_code(key: &Keystroke) -> Result<u16, InjectError> {
    let name = match key {
        Keystroke::Modifier(Modifier::Ctrl) => return Ok(29),
        Keystroke::Modifier(Modifier::Shift) => return Ok(42),
        Keystroke::Modifier(Modifier::Alt) => return Ok(56),
        Keystroke::Modifier(Modifier::Meta) => return Ok(125),
        Keystroke::Key(name) => name.as_str(),
    };
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return char_code(c.to_ascii_lowercase())
            .ok_or_else(|| InjectError(format!("no key for '{c}' on the virtual keyboard")));
    }
    Ok(match name {
        "Enter" | "Return" => 28,
        "Tab" => 15,
        "Space" => 57,
        "Escape" => 1,
        "Backspace" => 14,
        "Delete" => 111,
        "Insert" => 110,
        "Home" => 102,
        "End" => 107,
        "PageUp" => 104,
        "PageDown" => 109,
        "Up" => 103,
        "Down" => 108,
        "Left" => 105,
        "Right" => 106,
        "F1" => 59,
        "F2" => 60,
        "F3" => 61,
        "F4" => 62,
        "F5" => 63,
        "F6" => 64,
        "F7" => 65,
        "F8" => 66,
        "F9" => 67,
        "F10" => 68,
        "F11" => 87,
        "F12" => 88,
        media::PLAY_PAUSE => 164,
        media::MUTE => 113,
        media::VOLUME_UP => 115,
        media::VOLUME_DOWN => 114,
        media::NEXT_TRACK => 163,
        media::PREV_TRACK => 165,
        _ => return Err(InjectError(format!("unknown key: {name}"))),
    })
}

/// The key code of an unshifted US-layout character.
fn char_code(c: char) -> Option<u16> {
    const ROWS: [(&str, u16); 4] = [
        ("1234567890-=", 2),
        ("qwertyuiop[]", 16),
        ("asdfghjkl;'`", 30),
        ("zxcvbnm,./", 44),
    ];
    if c == '\\' {
        return Some(43);
    }
    ROWS.iter()
        .find_map(|(row, first)| row.chars().position(|k| k == c).map(|i| first + i as u16))
}

/// Every key code the virtual keyboard declares (all of [`key_code`]'s).
fn declared_keys() -> Vec<u16> {
    let mut keys: Vec<u16> = (1..=88).collect();
    keys.extend([
        102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 113, 114, 115, 125, 163, 164, 165,
    ]);
    keys
}

/// One `struct input_event` (64-bit: a 16-byte timeval, then type, code,
/// value); the kernel stamps the time.
pub fn event_bytes(kind: u16, code: u16, value: i32) -> [u8; 24] {
    let mut b = [0u8; 24];
    b[16..18].copy_from_slice(&kind.to_ne_bytes());
    b[18..20].copy_from_slice(&code.to_ne_bytes());
    b[20..24].copy_from_slice(&value.to_ne_bytes());
    b
}

/// `struct uinput_setup`: the device's ids and name.
#[repr(C)]
struct UinputSetup {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
    name: [u8; 80],
    ff_effects_max: u32,
}

// The uinput ioctls (_IO / _IOW on 'U').
const UI_DEV_CREATE: libc::c_ulong = 0x5501;
const UI_DEV_DESTROY: libc::c_ulong = 0x5502;
const UI_DEV_SETUP: libc::c_ulong = 0x405c_5503;
const UI_SET_EVBIT: libc::c_ulong = 0x4004_5564;
const UI_SET_KEYBIT: libc::c_ulong = 0x4004_5565;
const UI_SET_RELBIT: libc::c_ulong = 0x4004_5566;
/// _IOC(_IOC_READ, 'U', 44, 64): the created device's sysfs name.
#[cfg(test)]
const UI_GET_SYSNAME_64: libc::c_ulong = 0x8040_552c;

/// The virtual keyboard's name, as `/proc/bus/input/devices` lists it.
pub const DEVICE_NAME: &str = "freewheel virtual keyboard";

/// A virtual keyboard (with a scroll wheel) on `/dev/uinput`.
pub struct UinputInjector {
    file: File,
}

fn os_err(what: &str) -> InjectError {
    InjectError(format!("{what}: {}", std::io::Error::last_os_error()))
}

impl UinputInjector {
    /// Creates the virtual keyboard on `/dev/uinput`.
    pub fn new() -> Result<Self, InjectError> {
        Self::on(Path::new("/dev/uinput"))
    }

    /// Creates the virtual keyboard on the uinput node `path`.
    pub fn on(path: &Path) -> Result<Self, InjectError> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|e| {
                InjectError(format!(
                    "{}: {e} (freewheel's device rule gives it to you)",
                    path.display()
                ))
            })?;
        let fd = file.as_raw_fd();
        let ioctl = |request, value: libc::c_int, what: &str| {
            // SAFETY: plain uinput ioctls taking an int, on our own fd.
            if unsafe { libc::ioctl(fd, request, value) } < 0 {
                Err(os_err(what))
            } else {
                Ok(())
            }
        };
        ioctl(UI_SET_EVBIT, EV_KEY.into(), "UI_SET_EVBIT EV_KEY")?;
        ioctl(UI_SET_EVBIT, EV_REL.into(), "UI_SET_EVBIT EV_REL")?;
        ioctl(UI_SET_RELBIT, REL_WHEEL.into(), "UI_SET_RELBIT REL_WHEEL")?;
        for key in declared_keys() {
            ioctl(UI_SET_KEYBIT, key.into(), "UI_SET_KEYBIT")?;
        }
        let mut setup = UinputSetup {
            bustype: BUS_VIRTUAL,
            vendor: 0x1edb,
            product: 0xf00d,
            version: 1,
            name: [0; 80],
            ff_effects_max: 0,
        };
        setup.name[..DEVICE_NAME.len()].copy_from_slice(DEVICE_NAME.as_bytes());
        // SAFETY: UI_DEV_SETUP reads a `struct uinput_setup`, which
        // UinputSetup lays out (repr(C), 92 bytes).
        if unsafe { libc::ioctl(fd, UI_DEV_SETUP, &raw const setup) } < 0 {
            return Err(os_err("UI_DEV_SETUP"));
        }
        // SAFETY: UI_DEV_CREATE takes no argument.
        if unsafe { libc::ioctl(fd, UI_DEV_CREATE) } < 0 {
            return Err(os_err("UI_DEV_CREATE"));
        }
        tracing::info!("virtual keyboard created (uinput)");
        Ok(Self { file })
    }

    /// The created device's sysfs name (`inputN`).
    #[cfg(test)]
    pub fn sysname(&self) -> Option<String> {
        let mut buf = [0u8; 64];
        // SAFETY: UI_GET_SYSNAME writes at most 64 bytes into `buf`.
        let n = unsafe { libc::ioctl(self.file.as_raw_fd(), UI_GET_SYSNAME_64, buf.as_mut_ptr()) };
        if n < 0 {
            return None;
        }
        let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
        Some(String::from_utf8_lossy(&buf[..end]).into_owned())
    }

    fn emit(&mut self, events: &[(u16, u16, i32)]) -> Result<(), InjectError> {
        let mut bytes = Vec::with_capacity((events.len() + 1) * 24);
        for (kind, code, value) in events {
            bytes.extend_from_slice(&event_bytes(*kind, *code, *value));
        }
        bytes.extend_from_slice(&event_bytes(EV_SYN, SYN_REPORT, 0));
        self.file
            .write_all(&bytes)
            .map_err(|e| InjectError(format!("uinput write: {e}")))
    }
}

impl KeyInjector for UinputInjector {
    fn key_down(&mut self, key: &Keystroke) -> Result<(), InjectError> {
        let code = key_code(key)?;
        self.emit(&[(EV_KEY, code, 1)])
    }

    fn key_up(&mut self, key: &Keystroke) -> Result<(), InjectError> {
        let code = key_code(key)?;
        self.emit(&[(EV_KEY, code, 0)])
    }

    fn scroll(&mut self, amount: i32) -> Result<(), InjectError> {
        // REL_WHEEL is positive up; freewheel's amount is negative up.
        self.emit(&[(EV_REL, REL_WHEEL, -amount)])
    }
}

impl Drop for UinputInjector {
    fn drop(&mut self) {
        // SAFETY: UI_DEV_DESTROY takes no argument; the kernel releases
        // every key the device still holds.
        unsafe { libc::ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY) };
    }
}

/// The injector for this session: XTest on X11 (uinput if X is not
/// reachable), the uinput keyboard on Wayland (XTest, which reaches only
/// XWayland apps, if uinput is not open to us).
pub fn injector() -> Result<Box<dyn KeyInjector + Send>, InjectError> {
    let enigo =
        || crate::inject::EnigoInjector::new().map(|i| Box::new(i) as Box<dyn KeyInjector + Send>);
    let uinput = || UinputInjector::new().map(|i| Box::new(i) as Box<dyn KeyInjector + Send>);
    let session = current_session();
    tracing::info!(?session, "key injection");
    match session {
        Session::X11 => enigo().or_else(|e| {
            tracing::warn!(error = %e, "XTest not available, trying uinput");
            uinput()
        }),
        Session::Wayland | Session::Unknown => uinput().or_else(|e| {
            tracing::warn!(error = %e, "no uinput keyboard: keys reach only XWayland apps");
            enigo().map_err(|x| InjectError(format!("{e}; XTest: {x}")))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            vars.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn the_session_type_picks_the_injector() {
        assert_eq!(
            session(env(&[("XDG_SESSION_TYPE", "wayland"), ("DISPLAY", ":0")])),
            Session::Wayland
        );
        assert_eq!(
            session(env(&[
                ("XDG_SESSION_TYPE", "x11"),
                ("WAYLAND_DISPLAY", "w")
            ])),
            Session::X11
        );
        assert_eq!(
            session(env(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")])),
            Session::Wayland
        );
        assert_eq!(
            session(env(&[("XDG_SESSION_TYPE", "tty"), ("DISPLAY", ":99")])),
            Session::X11
        );
        assert_eq!(session(env(&[("DISPLAY", "")])), Session::Unknown);
        assert_eq!(session(env(&[])), Session::Unknown);
    }

    #[test]
    fn keys_map_to_linux_key_codes() {
        let k = |s: &str| key_code(&Keystroke::Key(s.into()));
        assert_eq!(k("a"), Ok(30));
        assert_eq!(k("C"), Ok(46));
        assert_eq!(k("z"), Ok(44));
        assert_eq!(k("1"), Ok(2));
        assert_eq!(k("0"), Ok(11));
        assert_eq!(k("/"), Ok(53));
        assert_eq!(k("\\"), Ok(43));
        assert_eq!(k("Enter"), Ok(28));
        assert_eq!(k("F12"), Ok(88));
        assert_eq!(k("Left"), Ok(105));
        assert!(k("!").is_err());
        assert!(k("Hyper").is_err());
        assert_eq!(key_code(&Keystroke::Modifier(Modifier::Meta)), Ok(125));
        assert_eq!(key_code(&Keystroke::Modifier(Modifier::Ctrl)), Ok(29));
    }

    #[test]
    fn every_media_key_and_every_named_key_is_declared() {
        let declared = declared_keys();
        for name in media::MEDIA_KEYS.iter().copied().chain([
            "Enter",
            "Tab",
            "Space",
            "Escape",
            "Backspace",
            "Delete",
            "Insert",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "Up",
            "Down",
            "Left",
            "Right",
            "F1",
            "F11",
            "F12",
            "a",
            "`",
        ]) {
            let code = key_code(&Keystroke::Key(name.into())).unwrap();
            assert!(declared.contains(&code), "{name} ({code})");
        }
        // The same names enigo knows.
        for name in media::MEDIA_KEYS {
            assert!(crate::inject::to_enigo(&Keystroke::Key(name.into())).is_ok());
        }
    }

    #[test]
    fn input_events_have_the_kernel_layout() {
        assert_eq!(std::mem::size_of::<UinputSetup>(), 92);
        let b = event_bytes(EV_KEY, 30, 1);
        assert_eq!(&b[..16], &[0u8; 16]);
        assert_eq!(u16::from_ne_bytes([b[16], b[17]]), EV_KEY);
        assert_eq!(u16::from_ne_bytes([b[18], b[19]]), 30);
        assert_eq!(i32::from_ne_bytes([b[20], b[21], b[22], b[23]]), 1);
    }

    #[test]
    fn a_missing_uinput_node_says_how_to_get_it() {
        let e = UinputInjector::on(Path::new("/nonexistent/uinput"))
            .err()
            .unwrap();
        assert!(e.0.contains("device rule"), "{e:?}");
    }

    /// Real injection through the kernel: the virtual keyboard's own evdev
    /// node sees the key press. Needs a writable /dev/uinput and a readable
    /// /dev/input/event* (CI's Linux job loads uinput and opens both).
    #[test]
    #[ignore = "needs /dev/uinput; CI's Linux job runs it with --ignored"]
    fn uinput_key_presses_reach_the_kernel() {
        use std::io::Read;
        let mut kb = UinputInjector::new().expect("open /dev/uinput");
        let sys = kb.sysname().expect("UI_GET_SYSNAME");
        let dir = Path::new("/sys/devices/virtual/input").join(&sys);
        let name = std::fs::read_to_string(dir.join("name")).unwrap();
        assert_eq!(name.trim(), DEVICE_NAME);
        let event = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .find_map(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                n.starts_with("event").then_some(n)
            })
            .expect("an event node");
        let node = Path::new("/dev/input").join(&event);
        // udev may still be setting the node's mode.
        let mut reader = None;
        for _ in 0..50 {
            if let Ok(f) = File::open(&node) {
                reader = Some(f);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let mut reader = reader.unwrap_or_else(|| panic!("open {}", node.display()));
        kb.key_down(&Keystroke::Key("k".into())).unwrap();
        kb.key_up(&Keystroke::Key("k".into())).unwrap();
        kb.scroll(-2).unwrap();
        let mut seen = Vec::new();
        let mut buf = [0u8; 24];
        while seen.len() < 3 {
            reader.read_exact(&mut buf).unwrap();
            let kind = u16::from_ne_bytes([buf[16], buf[17]]);
            let code = u16::from_ne_bytes([buf[18], buf[19]]);
            let value = i32::from_ne_bytes([buf[20], buf[21], buf[22], buf[23]]);
            if kind != EV_SYN {
                seen.push((kind, code, value));
            }
        }
        assert_eq!(
            seen,
            [(EV_KEY, 37, 1), (EV_KEY, 37, 0), (EV_REL, REL_WHEEL, 2)]
        );
        println!("uinput: {sys} ({event}) sent k down, k up, wheel up 2");
    }
}
