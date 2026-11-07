//! Key injection and scrolling behind the [`KeyInjector`] trait, so the
//! mapping engine can be tested without sending real input. The enigo-backed
//! injector lives in the app crate; this module has no OS dependency.

use std::fmt;

pub use crate::config::{KeyCombo, Modifier};

/// The media key names a [`KeyCombo`] may use (src/keys.js NAMED): each
/// injector sends them as the OS media keys (Windows VK_MEDIA_* /
/// VK_VOLUME_*, macOS NX system keys).
pub const PLAY_PAUSE: &str = "PlayPause";
pub const MUTE: &str = "Mute";
pub const VOLUME_UP: &str = "VolumeUp";
pub const VOLUME_DOWN: &str = "VolumeDown";
pub const NEXT_TRACK: &str = "NextTrack";
pub const PREV_TRACK: &str = "PrevTrack";
pub const MEDIA_KEYS: [&str; 6] = [
    PLAY_PAUSE,
    MUTE,
    VOLUME_UP,
    VOLUME_DOWN,
    NEXT_TRACK,
    PREV_TRACK,
];

/// One key that can be pressed or released: a modifier or a named key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Keystroke {
    Modifier(Modifier),
    Key(String),
}

/// An error from a key injector backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectError(pub String);

impl fmt::Display for InjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "input injection failed: {}", self.0)
    }
}

impl std::error::Error for InjectError {}

/// Sends synthetic key and scroll input to the OS.
pub trait KeyInjector {
    /// Presses `key` and leaves it down.
    fn key_down(&mut self, key: &Keystroke) -> Result<(), InjectError>;

    /// Releases `key`.
    fn key_up(&mut self, key: &Keystroke) -> Result<(), InjectError>;

    /// Scrolls vertically by `amount` steps (negative is up).
    fn scroll(&mut self, amount: i32) -> Result<(), InjectError>;

    /// Presses the modifiers in order, then the key, then releases them in
    /// reverse order.
    fn tap_combo(&mut self, combo: &KeyCombo) -> Result<(), InjectError> {
        let mut keys: Vec<Keystroke> = combo
            .modifiers
            .iter()
            .map(|m| Keystroke::Modifier(*m))
            .collect();
        keys.push(Keystroke::Key(combo.key.clone()));
        for key in &keys {
            self.key_down(key)?;
        }
        for key in keys.iter().rev() {
            self.key_up(key)?;
        }
        Ok(())
    }
}

/// A call made on a [`MockInjector`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectCall {
    Down(Keystroke),
    Up(Keystroke),
    Scroll(i32),
}

/// A recording injector for tests: it sends nothing to the OS.
#[derive(Debug, Default)]
pub struct MockInjector {
    calls: Vec<InjectCall>,
    down: Vec<Keystroke>,
}

impl MockInjector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every call so far, in order.
    pub fn calls(&self) -> &[InjectCall] {
        &self.calls
    }

    /// The keys currently down, in the order they were pressed.
    pub fn keys_down(&self) -> &[Keystroke] {
        &self.down
    }
}

impl KeyInjector for MockInjector {
    fn key_down(&mut self, key: &Keystroke) -> Result<(), InjectError> {
        self.calls.push(InjectCall::Down(key.clone()));
        if !self.down.contains(key) {
            self.down.push(key.clone());
        }
        Ok(())
    }

    fn key_up(&mut self, key: &Keystroke) -> Result<(), InjectError> {
        self.calls.push(InjectCall::Up(key.clone()));
        self.down.retain(|k| k != key);
        Ok(())
    }

    fn scroll(&mut self, amount: i32) -> Result<(), InjectError> {
        self.calls.push(InjectCall::Scroll(amount));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use InjectCall::{Down, Up};

    fn key(name: &str) -> Keystroke {
        Keystroke::Key(name.to_string())
    }

    #[test]
    fn tap_combo_presses_modifiers_then_key_and_releases_in_reverse() {
        let mut mock = MockInjector::new();
        let combo = KeyCombo {
            modifiers: vec![Modifier::Ctrl, Modifier::Shift],
            key: "S".to_string(),
        };
        mock.tap_combo(&combo).unwrap();
        let ctrl = Keystroke::Modifier(Modifier::Ctrl);
        let shift = Keystroke::Modifier(Modifier::Shift);
        assert_eq!(
            mock.calls(),
            &[
                Down(ctrl.clone()),
                Down(shift.clone()),
                Down(key("S")),
                Up(key("S")),
                Up(shift),
                Up(ctrl),
            ]
        );
        assert!(mock.keys_down().is_empty());
    }

    #[test]
    fn mock_reports_keys_currently_down() {
        let mut mock = MockInjector::new();
        let alt = Keystroke::Modifier(Modifier::Alt);
        mock.key_down(&alt).unwrap();
        mock.key_down(&key("A")).unwrap();
        assert_eq!(mock.keys_down(), &[alt.clone(), key("A")]);
        mock.key_up(&alt).unwrap();
        assert_eq!(mock.keys_down(), &[key("A")]);
        assert_eq!(mock.calls(), &[Down(alt.clone()), Down(key("A")), Up(alt)]);
    }

    #[test]
    fn mock_records_each_scroll_call() {
        let mut mock = MockInjector::new();
        let injector: &mut dyn KeyInjector = &mut mock;
        injector.scroll(-1).unwrap();
        injector.scroll(3).unwrap();
        assert_eq!(
            mock.calls(),
            &[InjectCall::Scroll(-1), InjectCall::Scroll(3)]
        );
        assert!(mock.keys_down().is_empty());
    }
}
