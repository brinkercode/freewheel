//! The production [`KeyInjector`], backed by enigo (Windows SendInput,
//! macOS CGEvent, Linux X11 XTest), and [`os_injector`], the one this
//! session uses (on Linux Wayland: the uinput keyboard, linux_inject.rs).
//! Tests never construct them; they use `MockInjector`.

use enigo::{Axis, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use freewheel_core::inject::{self as media, InjectError, KeyInjector, Keystroke, Modifier};

pub struct EnigoInjector {
    enigo: Enigo,
}

impl EnigoInjector {
    pub fn new() -> Result<Self, InjectError> {
        let enigo = Enigo::new(&Settings::default()).map_err(err)?;
        Ok(Self { enigo })
    }

    fn send(&mut self, key: &Keystroke, direction: Direction) -> Result<(), InjectError> {
        let key = to_enigo(key)?;
        self.enigo.key(key, direction).map_err(err)
    }
}

fn err(e: impl std::fmt::Display) -> InjectError {
    InjectError(e.to_string())
}

pub(crate) fn to_enigo(key: &Keystroke) -> Result<Key, InjectError> {
    let name = match key {
        Keystroke::Modifier(Modifier::Ctrl) => return Ok(Key::Control),
        Keystroke::Modifier(Modifier::Shift) => return Ok(Key::Shift),
        Keystroke::Modifier(Modifier::Alt) => return Ok(Key::Alt),
        Keystroke::Modifier(Modifier::Meta) => return Ok(Key::Meta),
        Keystroke::Key(name) => name.as_str(),
    };
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Ok(Key::Unicode(c.to_ascii_lowercase()));
    }
    Ok(match name {
        "Enter" | "Return" => Key::Return,
        "Tab" => Key::Tab,
        "Space" => Key::Space,
        "Escape" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Up" => Key::UpArrow,
        "Down" => Key::DownArrow,
        "Left" => Key::LeftArrow,
        "Right" => Key::RightArrow,
        "F1" => Key::F1,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F6" => Key::F6,
        "F7" => Key::F7,
        "F8" => Key::F8,
        "F9" => Key::F9,
        "F10" => Key::F10,
        "F11" => Key::F11,
        "F12" => Key::F12,
        // The OS media keys (freewheel_core::inject::MEDIA_KEYS): enigo
        // sends VK_MEDIA_PLAY_PAUSE / VK_VOLUME_MUTE... on Windows, the
        // NX system-defined key events on macOS and the XF86Audio keysyms
        // on X11.
        media::PLAY_PAUSE => Key::MediaPlayPause,
        media::MUTE => Key::VolumeMute,
        media::VOLUME_UP => Key::VolumeUp,
        media::VOLUME_DOWN => Key::VolumeDown,
        media::NEXT_TRACK => Key::MediaNextTrack,
        media::PREV_TRACK => Key::MediaPrevTrack,
        // A mappings file's other named keys (F13-F24, the number pad...).
        other => {
            return crate::key_names::extra_key(other)
                .ok_or_else(|| InjectError(format!("unknown key: {name}")));
        }
    })
}

/// The key injector for this OS and session.
pub fn os_injector() -> Result<Box<dyn KeyInjector + Send>, InjectError> {
    #[cfg(target_os = "linux")]
    {
        crate::linux_inject::injector()
    }
    #[cfg(not(target_os = "linux"))]
    {
        EnigoInjector::new().map(|i| Box::new(i) as Box<dyn KeyInjector + Send>)
    }
}

impl KeyInjector for EnigoInjector {
    fn key_down(&mut self, key: &Keystroke) -> Result<(), InjectError> {
        self.send(key, Direction::Press)
    }

    fn key_up(&mut self, key: &Keystroke) -> Result<(), InjectError> {
        self.send(key, Direction::Release)
    }

    fn scroll(&mut self, amount: i32) -> Result<(), InjectError> {
        self.enigo.scroll(amount, Axis::Vertical).map_err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_media_key_name_is_sendable() {
        for name in media::MEDIA_KEYS {
            assert!(to_enigo(&Keystroke::Key(name.into())).is_ok(), "{name}");
        }
        assert_eq!(
            to_enigo(&Keystroke::Key("PlayPause".into())).unwrap(),
            Key::MediaPlayPause
        );
        assert_eq!(
            to_enigo(&Keystroke::Key("Mute".into())).unwrap(),
            Key::VolumeMute
        );
        assert!(to_enigo(&Keystroke::Key("Hyper".into())).is_err());
    }

    /// Real injection on X11: an XTest key press reaches a window that has
    /// the focus. Needs an X server (CI's Linux job runs it under Xvfb with
    /// --ignored).
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs an X server; CI's Linux job runs it under Xvfb"]
    fn xtest_key_presses_reach_an_x11_window() {
        use x11rb::connection::Connection;
        use x11rb::protocol::Event;
        use x11rb::protocol::xproto::{
            ConnectionExt, CreateWindowAux, EventMask, InputFocus, WindowClass,
        };
        let (conn, n) = x11rb::connect(None).expect("an X server ($DISPLAY)");
        let screen = conn.setup().roots[n].clone();
        let win = conn.generate_id().unwrap();
        conn.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            win,
            screen.root,
            0,
            0,
            200,
            200,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .event_mask(EventMask::KEY_PRESS | EventMask::KEY_RELEASE | EventMask::EXPOSURE),
        )
        .unwrap();
        conn.map_window(win).unwrap();
        conn.flush().unwrap();
        loop {
            if let Event::Expose(_) = conn.wait_for_event().unwrap() {
                break;
            }
        }
        conn.set_input_focus(InputFocus::PARENT, win, x11rb::CURRENT_TIME)
            .unwrap()
            .check()
            .unwrap();
        let mut injector = EnigoInjector::new().expect("XTest");
        let k = Keystroke::Key("k".into());
        injector.key_down(&k).unwrap();
        injector.key_up(&k).unwrap();
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut seen = Vec::new();
        while seen.len() < 2 && std::time::Instant::now() < end {
            match conn.poll_for_event().unwrap() {
                Some(Event::KeyPress(e)) => seen.push(("press", e.detail)),
                Some(Event::KeyRelease(e)) => seen.push(("release", e.detail)),
                Some(_) => {}
                None => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
        assert_eq!(seen.len(), 2, "the window saw {seen:?}");
        let map = conn
            .get_keyboard_mapping(min, max - min + 1)
            .unwrap()
            .reply()
            .unwrap();
        let per = usize::from(map.keysyms_per_keycode);
        let keysym = map.keysyms[usize::from(seen[0].1 - min) * per];
        assert_eq!(
            keysym, 0x6b,
            "keycode {} is keysym {keysym:#x}, not k",
            seen[0].1
        );
        assert_eq!((seen[0].0, seen[1].0), ("press", "release"));
        println!("XTest: the window got k (keycode {})", seen[0].1);
    }

    #[test]
    fn the_media_key_names_match_the_frontend() {
        let js = include_str!("../../src/keys.js");
        for name in media::MEDIA_KEYS {
            assert!(js.contains(&format!("\"{name}\"")), "{name}");
        }
    }
}
