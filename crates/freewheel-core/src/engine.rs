//! The mapping engine: turns device key events on the current layer into
//! key injections. Bindings are keyed by the key's name (e.g. `"Cut"`).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::clock::Clock;
use crate::config::{Action, Config, JogMode, JogSettings, KeyCombo, Layer};
use crate::inject::{InjectError, KeyInjector, Keystroke};
use crate::report::{DeviceEvent, DeviceJogMode, Key, set_jog_mode};

/// Shuttle repeat rate cap when the binding sets no `shuttle_max_rate_hz`.
const DEFAULT_SHUTTLE_MAX_RATE_HZ: u32 = 20;

/// Runtime engine state for one config.
pub struct Engine {
    layers: Vec<Layer>,
    layer_stack: Vec<String>,
    held: BTreeMap<Key, KeyCombo>,
    latched: Vec<KeyCombo>,
    jog: JogSettings,
    jog_mode: JogMode,
    /// Jog/Scroll ticks not yet worth a threshold; Shuttle: the dial position.
    jog_value: i32,
    last_shuttle_fire: Option<Instant>,
    reports: Vec<Vec<u8>>,
}

impl Engine {
    pub fn new(config: &Config) -> Self {
        Engine {
            layers: config.layers.clone(),
            layer_stack: vec![config.base_layer.clone()],
            held: BTreeMap::new(),
            latched: Vec::new(),
            jog: config.jog.clone(),
            jog_mode: config.jog.default_mode,
            jog_value: 0,
            last_shuttle_fire: None,
            reports: Vec::new(),
        }
    }

    pub fn jog_mode(&self) -> JogMode {
        self.jog_mode
    }

    /// Drains the output reports the engine wants written to the device.
    pub fn take_reports(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.reports)
    }

    /// Repeats the Shuttle action while the dial is off-centre, at a rate of
    /// one per threshold of distance per second, capped at the max rate.
    pub fn tick(
        &mut self,
        clock: &dyn Clock,
        injector: &mut dyn KeyInjector,
    ) -> Result<(), InjectError> {
        if self.jog_mode != JogMode::Shuttle {
            return Ok(());
        }
        let Some(binding) = self.jog.per_mode.get(&JogMode::Shuttle) else {
            return Ok(());
        };
        let max = binding
            .shuttle_max_rate_hz
            .unwrap_or(DEFAULT_SHUTTLE_MAX_RATE_HZ);
        let rate = (self.jog_value.unsigned_abs() / binding.threshold.max(1)).min(max);
        if rate == 0 {
            self.last_shuttle_fire = None;
            return Ok(());
        }
        let now = clock.now();
        let due = self
            .last_shuttle_fire
            .is_none_or(|t| now - t >= Duration::from_secs(1) / rate);
        if !due {
            return Ok(());
        }
        self.last_shuttle_fire = Some(now);
        let action = if self.jog_value < 0 {
            &binding.left
        } else {
            &binding.right
        };
        fire(action, injector)
    }

    fn set_jog_mode(&mut self, mode: JogMode) {
        self.jog_mode = mode;
        self.jog_value = 0;
        self.last_shuttle_fire = None;
        let device = match mode {
            JogMode::Shuttle => DeviceJogMode::AbsoluteDeadzero,
            JogMode::Jog | JogMode::Scroll => DeviceJogMode::Relative,
        };
        self.reports.push(set_jog_mode(device).to_vec());
    }

    fn jog(&mut self, value: i32, injector: &mut dyn KeyInjector) -> Result<(), InjectError> {
        if self.jog_mode == JogMode::Shuttle {
            self.jog_value = value;
            return Ok(());
        }
        let Some(binding) = self.jog.per_mode.get(&self.jog_mode) else {
            return Ok(());
        };
        let threshold = binding.threshold.max(1) as i32;
        self.jog_value += value;
        while self.jog_value.abs() >= threshold {
            if self.jog_value < 0 {
                self.jog_value += threshold;
                fire(&binding.left, injector)?;
            } else {
                self.jog_value -= threshold;
                fire(&binding.right, injector)?;
            }
        }
        Ok(())
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
                Some(Action::SetJogMode { mode }) => {
                    self.set_jog_mode(mode);
                    Ok(())
                }
                _ => Ok(()),
            },
            // Held keys release on the physical release, whatever the layer.
            DeviceEvent::Release(key) => match self.held.remove(key) {
                Some(combo) => combo_up(injector, &combo),
                None => Ok(()),
            },
            DeviceEvent::Jog { value, .. } => self.jog(*value, injector),
            _ => Ok(()),
        }
    }

    fn binding(&self, key: Key) -> Option<&Action> {
        let current = self.layer_stack.last()?;
        let layer = self.layers.iter().find(|l| &l.id == current)?;
        layer.bindings.get(&format!("{key:?}"))
    }
}

/// Fires a jog left/right action once.
fn fire(action: &Option<Action>, injector: &mut dyn KeyInjector) -> Result<(), InjectError> {
    match action {
        Some(Action::Hotkey { combo }) => injector.tap_combo(combo),
        Some(Action::Scroll { amount }) => injector.scroll(*amount),
        _ => Ok(()),
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

    use crate::config::{JogBinding, JogMode};
    use crate::inject::InjectCall::Scroll;
    use std::cell::Cell;
    use std::time::{Duration, Instant};

    struct FakeClock(Cell<Instant>);

    impl Clock for FakeClock {
        fn now(&self) -> Instant {
            self.0.get()
        }
    }

    impl FakeClock {
        fn advance(&self, ms: u64) {
            self.0.set(self.0.get() + Duration::from_millis(ms));
        }
    }

    fn jog(value: i32) -> DeviceEvent {
        DeviceEvent::Jog { mode: 0, value }
    }

    fn jog_engine(mode: JogMode, threshold: u32, max_hz: Option<u32>) -> Engine {
        let mut config = Config::default();
        config.jog.default_mode = mode;
        config.jog.per_mode.insert(
            mode,
            JogBinding {
                left: Some(Action::Scroll { amount: -5 }),
                right: Some(Action::Scroll { amount: 5 }),
                threshold,
                shuttle_max_rate_hz: max_hz,
            },
        );
        Engine::new(&config)
    }

    #[test]
    fn set_jog_mode_switches_mode_and_emits_report() {
        let mut config = Config::default();
        let bindings = &mut config.layers[0].bindings;
        bindings.insert("Jog".into(), Action::SetJogMode { mode: JogMode::Jog });
        bindings.insert(
            "Shtl".into(),
            Action::SetJogMode {
                mode: JogMode::Shuttle,
            },
        );
        bindings.insert(
            "Scrl".into(),
            Action::SetJogMode {
                mode: JogMode::Scroll,
            },
        );
        let mut engine = Engine::new(&config);
        let mut mock = MockInjector::new();
        assert_eq!(engine.jog_mode(), JogMode::Scroll);
        for (key, mode, byte) in [
            (Key::Jog, JogMode::Jog, 0),
            (Key::Shtl, JogMode::Shuttle, 3),
            (Key::Scrl, JogMode::Scroll, 0),
        ] {
            engine.handle(&Press(key), &mut mock).unwrap();
            assert_eq!(engine.jog_mode(), mode);
            assert_eq!(
                engine.take_reports(),
                vec![vec![0x03, byte, 0, 0, 0, 0, 0xff]]
            );
        }
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn jog_and_scroll_fire_once_per_threshold() {
        for mode in [JogMode::Jog, JogMode::Scroll] {
            let mut engine = jog_engine(mode, 10, None);
            let mut mock = MockInjector::new();
            engine.handle(&jog(4), &mut mock).unwrap();
            engine.handle(&jog(5), &mut mock).unwrap();
            assert!(
                mock.calls().is_empty(),
                "{mode:?}: 9 ticks is under threshold"
            );
            engine.handle(&jog(1), &mut mock).unwrap();
            assert_eq!(mock.calls(), &[Scroll(5)]);
            engine.handle(&jog(25), &mut mock).unwrap();
            assert_eq!(mock.calls(), &[Scroll(5), Scroll(5), Scroll(5)]);
            // 5 ticks remain; turning back 15 nets -10: one left.
            engine.handle(&jog(-15), &mut mock).unwrap();
            assert_eq!(mock.calls()[3..], [Scroll(-5)]);
        }
    }

    #[test]
    fn shuttle_repeats_scaled_by_distance_and_stops_at_centre() {
        let mut engine = jog_engine(JogMode::Shuttle, 10, Some(8));
        let clock = FakeClock(Cell::new(Instant::now()));
        let mut mock = MockInjector::new();
        // 20 off-centre with threshold 10: 2 Hz, one every 500 ms.
        engine.handle(&jog(20), &mut mock).unwrap();
        assert!(mock.calls().is_empty());
        engine.tick(&clock, &mut mock).unwrap();
        assert_eq!(mock.calls(), &[Scroll(5)]);
        clock.advance(499);
        engine.tick(&clock, &mut mock).unwrap();
        assert_eq!(mock.calls().len(), 1);
        clock.advance(1);
        engine.tick(&clock, &mut mock).unwrap();
        assert_eq!(mock.calls().len(), 2);
        // 40 to the left: 4 Hz, one every 250 ms.
        engine.handle(&jog(-40), &mut mock).unwrap();
        clock.advance(250);
        engine.tick(&clock, &mut mock).unwrap();
        clock.advance(250);
        engine.tick(&clock, &mut mock).unwrap();
        assert_eq!(mock.calls()[2..], [Scroll(-5), Scroll(-5)]);
        // Far off-centre is capped at 8 Hz: 125 ms.
        engine.handle(&jog(1000), &mut mock).unwrap();
        clock.advance(124);
        engine.tick(&clock, &mut mock).unwrap();
        assert_eq!(mock.calls().len(), 4);
        clock.advance(1);
        engine.tick(&clock, &mut mock).unwrap();
        assert_eq!(mock.calls().len(), 5);
        // Centre stops it.
        engine.handle(&jog(0), &mut mock).unwrap();
        for _ in 0..10 {
            clock.advance(1000);
            engine.tick(&clock, &mut mock).unwrap();
        }
        assert_eq!(mock.calls().len(), 5);
    }

    #[test]
    fn default_config_scroll_mode_scrolls_minus_one_left_and_one_right() {
        let mut engine = Engine::new(&Config::default());
        let mut mock = MockInjector::new();
        engine.handle(&jog(-1), &mut mock).unwrap();
        engine.handle(&jog(1), &mut mock).unwrap();
        assert_eq!(mock.calls(), &[Scroll(-1), Scroll(1)]);
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
