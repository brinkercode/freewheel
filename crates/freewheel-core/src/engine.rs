//! The mapping engine: turns device key events on the current layer into
//! key injections. Bindings are keyed by the key's name (e.g. `"Cut"`).

use std::collections::BTreeMap;

use crate::config::{Action, Config, KeyCombo, Layer};
use crate::inject::{InjectError, KeyInjector, Keystroke};
use crate::report::{DeviceEvent, Key};

/// Runtime engine state for one config.
pub struct Engine {
    layers: Vec<Layer>,
    layer_stack: Vec<String>,
    held: BTreeMap<Key, KeyCombo>,
    latched: Vec<KeyCombo>,
}

impl Engine {
    pub fn new(config: &Config) -> Self {
        Engine {
            layers: config.layers.clone(),
            layer_stack: vec![config.base_layer.clone()],
            held: BTreeMap::new(),
            latched: Vec::new(),
        }
    }

    /// Applies one device event, injecting through `injector`.
    pub fn handle(
        &mut self,
        event: &DeviceEvent,
        injector: &mut dyn KeyInjector,
    ) -> Result<(), InjectError> {
        match event {
            DeviceEvent::Press(key) => match self.binding(*key).cloned() {
                Some(Action::Hotkey { combo }) => injector.tap_combo(&combo),
                Some(Action::Held { combo }) => {
                    combo_down(injector, &combo)?;
                    self.held.insert(*key, combo);
                    Ok(())
                }
                Some(Action::Latched { combo }) => {
                    if let Some(i) = self.latched.iter().position(|c| *c == combo) {
                        self.latched.remove(i);
                        combo_up(injector, &combo)
                    } else {
                        combo_down(injector, &combo)?;
                        self.latched.push(combo);
                        Ok(())
                    }
                }
                _ => Ok(()),
            },
            // Held keys release on the physical release, whatever the layer.
            DeviceEvent::Release(key) => match self.held.remove(key) {
                Some(combo) => combo_up(injector, &combo),
                None => Ok(()),
            },
            _ => Ok(()),
        }
    }

    fn binding(&self, key: Key) -> Option<&Action> {
        let current = self.layer_stack.last()?;
        let layer = self.layers.iter().find(|l| &l.id == current)?;
        layer.bindings.get(&format!("{key:?}"))
    }
}

fn keystrokes(combo: &KeyCombo) -> Vec<Keystroke> {
    let mut keys: Vec<Keystroke> = combo
        .modifiers
        .iter()
        .map(|m| Keystroke::Modifier(*m))
        .collect();
    keys.push(Keystroke::Key(combo.key.clone()));
    keys
}

fn combo_down(injector: &mut dyn KeyInjector, combo: &KeyCombo) -> Result<(), InjectError> {
    keystrokes(combo)
        .iter()
        .try_for_each(|k| injector.key_down(k))
}

fn combo_up(injector: &mut dyn KeyInjector, combo: &KeyCombo) -> Result<(), InjectError> {
    keystrokes(combo)
        .iter()
        .rev()
        .try_for_each(|k| injector.key_up(k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Modifier;
    use crate::inject::InjectCall::{Down, Up};
    use crate::inject::MockInjector;
    use DeviceEvent::{Press, Release};

    fn ctrl_s() -> KeyCombo {
        KeyCombo {
            modifiers: vec![Modifier::Ctrl],
            key: "S".into(),
        }
    }

    fn engine_with(key: Key, action: Action) -> Engine {
        let mut config = Config::default();
        config.layers[0].bindings.insert(format!("{key:?}"), action);
        Engine::new(&config)
    }

    fn ctrl() -> Keystroke {
        Keystroke::Modifier(Modifier::Ctrl)
    }

    fn s() -> Keystroke {
        Keystroke::Key("S".into())
    }

    #[test]
    fn hotkey_taps_on_press_and_ignores_release() {
        let mut engine = engine_with(Key::Cut, Action::Hotkey { combo: ctrl_s() });
        let mut mock = MockInjector::new();
        engine.handle(&Press(Key::Cut), &mut mock).unwrap();
        let tap = [Down(ctrl()), Down(s()), Up(s()), Up(ctrl())];
        assert_eq!(mock.calls(), &tap);
        engine.handle(&Release(Key::Cut), &mut mock).unwrap();
        assert_eq!(mock.calls(), &tap);
    }

    #[test]
    fn held_goes_down_on_press_and_up_on_release() {
        let mut engine = engine_with(Key::In, Action::Held { combo: ctrl_s() });
        let mut mock = MockInjector::new();
        engine.handle(&Press(Key::In), &mut mock).unwrap();
        assert_eq!(mock.calls(), &[Down(ctrl()), Down(s())]);
        assert_eq!(mock.keys_down(), &[ctrl(), s()]);
        engine.handle(&Release(Key::In), &mut mock).unwrap();
        assert_eq!(mock.calls()[2..], [Up(s()), Up(ctrl())]);
        assert!(mock.keys_down().is_empty());
    }

    #[test]
    fn latched_goes_down_on_first_press_and_up_on_second() {
        let mut engine = engine_with(Key::Snap, Action::Latched { combo: ctrl_s() });
        let mut mock = MockInjector::new();
        engine.handle(&Press(Key::Snap), &mut mock).unwrap();
        engine.handle(&Release(Key::Snap), &mut mock).unwrap();
        assert_eq!(mock.calls(), &[Down(ctrl()), Down(s())]);
        assert_eq!(mock.keys_down(), &[ctrl(), s()]);
        engine.handle(&Press(Key::Snap), &mut mock).unwrap();
        engine.handle(&Release(Key::Snap), &mut mock).unwrap();
        assert_eq!(mock.calls()[2..], [Up(s()), Up(ctrl())]);
        assert!(mock.keys_down().is_empty());
    }

    #[test]
    fn unbound_button_does_nothing() {
        let mut engine = engine_with(Key::Cut, Action::Hotkey { combo: ctrl_s() });
        let mut mock = MockInjector::new();
        engine.handle(&Press(Key::Out), &mut mock).unwrap();
        engine.handle(&Release(Key::Out), &mut mock).unwrap();
        assert!(mock.calls().is_empty());
    }
}
