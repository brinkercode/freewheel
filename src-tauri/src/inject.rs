//! The production [`KeyInjector`], backed by enigo. Tests never construct
//! it; they use `MockInjector`.

use enigo::{Axis, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use freewheel_core::inject::{InjectError, KeyInjector, Keystroke, Modifier};

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

fn to_enigo(key: &Keystroke) -> Result<Key, InjectError> {
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
        _ => return Err(InjectError(format!("unknown key: {name}"))),
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
