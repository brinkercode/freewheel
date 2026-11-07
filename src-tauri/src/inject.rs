//! The production [`KeyInjector`], backed by enigo. Tests never construct
//! it; they use `MockInjector`.

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
        // sends VK_MEDIA_PLAY_PAUSE / VK_VOLUME_MUTE... on Windows and the
        // NX system-defined key events on macOS.
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

    #[test]
    fn the_media_key_names_match_the_frontend() {
        let js = include_str!("../../src/keys.js");
        for name in media::MEDIA_KEYS {
            assert!(js.contains(&format!("\"{name}\"")), "{name}");
        }
    }
}
