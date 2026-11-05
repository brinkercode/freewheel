//! The mapping engine: turns device key events on the current layer into
//! key injections. Bindings are keyed by the key's name (e.g. `"Cut"`).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use tracing::warn;

use crate::clock::Clock;
use crate::config::{Action, Config, JogMode, JogSettings, KeyCombo, Layer, OverrideMode};
use crate::hid::{HidError, HidTransport};
use crate::inject::{InjectError, KeyInjector, Keystroke};
use crate::launch::{LaunchError, Launcher};
use crate::report::{
    DeviceEvent, DeviceJogMode, JogLed, Key, Led, jog_led_report, led_report, set_jog_mode,
};

/// Shuttle repeat rate cap when the binding sets no `shuttle_max_rate_hz`.
const DEFAULT_SHUTTLE_MAX_RATE_HZ: u32 = 20;

/// Most layers on the stack, the base layer included (ADR-07 default).
pub const MAX_LAYER_DEPTH: usize = 8;

/// Runtime engine state for one config.
pub struct Engine {
    layers: Vec<Layer>,
    layer_stack: Vec<String>,
    /// `push_keys[i]` pushed `layer_stack[i + 1]` and pops it again.
    push_keys: Vec<Key>,
    held: BTreeMap<Key, KeyCombo>,
    latched: Vec<KeyCombo>,
    jog: JogSettings,
    jog_mode: JogMode,
    /// Jog/Scroll ticks not yet worth a threshold; Shuttle: the dial position.
    jog_value: i32,
    last_shuttle_fire: Option<Instant>,
    reports: Vec<Vec<u8>>,
    launch_errors: Vec<LaunchError>,
    active_override: Option<OverrideMode>,
    override_ticks: Vec<(OverrideMode, i32)>,
}

impl Engine {
    pub fn new(config: &Config) -> Self {
        Engine {
            layers: config.layers.clone(),
            layer_stack: vec![config.base_layer.clone()],
            push_keys: Vec::new(),
            held: BTreeMap::new(),
            latched: Vec::new(),
            jog: config.jog.clone(),
            jog_mode: config.jog.default_mode,
            jog_value: 0,
            last_shuttle_fire: None,
            reports: Vec::new(),
            launch_errors: Vec::new(),
            active_override: None,
            override_ticks: Vec::new(),
        }
    }

    /// The dial override in use, if any.
    pub fn active_override(&self) -> Option<OverrideMode> {
        self.active_override
    }

    /// Drains the dial ticks taken by the active override, for an
    /// [`OverrideWorker`](crate::overrides::OverrideWorker).
    pub fn take_override_ticks(&mut self) -> Vec<(OverrideMode, i32)> {
        std::mem::take(&mut self.override_ticks)
    }

    /// Switches the dial to `mode`, or back to jog actions if it is active.
    fn toggle_override(&mut self, mode: OverrideMode) {
        self.active_override = (self.active_override != Some(mode)).then_some(mode);
        self.jog_value = 0;
        self.last_shuttle_fire = None;
    }

    /// Releases every held and latched key and stops the active dial
    /// override and the Shuttle repeat, so nothing stays stuck. Calling it
    /// again injects nothing.
    pub fn release_all(&mut self, injector: &mut dyn KeyInjector) -> Result<(), InjectError> {
        self.active_override = None;
        self.override_ticks.clear();
        self.jog_value = 0;
        self.last_shuttle_fire = None;
        let held = std::mem::take(&mut self.held).into_values();
        let latched = std::mem::take(&mut self.latched);
        // Release all of them even if one fails; report the first error.
        held.chain(latched)
            .map(|combo| combo_up(injector, &combo))
            .fold(Ok(()), Result::and)
    }

    pub fn jog_mode(&self) -> JogMode {
        self.jog_mode
    }

    /// Drains the output reports the engine wants written to the device.
    pub fn take_reports(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.reports)
    }

    /// Drains the error events of failed app launches.
    pub fn take_launch_errors(&mut self) -> Vec<LaunchError> {
        std::mem::take(&mut self.launch_errors)
    }

    /// Repeats the Shuttle action while the dial is off-centre, at a rate of
    /// one per threshold of distance per second, capped at the max rate.
    pub fn tick(
        &mut self,
        clock: &dyn Clock,
        injector: &mut dyn KeyInjector,
    ) -> Result<(), InjectError> {
        if self.jog_mode != JogMode::Shuttle || self.active_override.is_some() {
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

    pub fn set_jog_mode(&mut self, mode: JogMode) {
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
        if let Some(mode) = self.active_override {
            if value != 0 {
                self.override_ticks.push((mode, value));
            }
            return Ok(());
        }
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

    /// Applies one device event, injecting through `injector` and starting
    /// apps through `launcher`. A failed launch becomes a launch error event.
    pub fn handle(
        &mut self,
        event: &DeviceEvent,
        injector: &mut dyn KeyInjector,
        launcher: &mut dyn Launcher,
    ) -> Result<(), InjectError> {
        match event {
            // The button that pushed the current layer is its 'back' button,
            // whatever the layer binds it to.
            DeviceEvent::Press(key) if self.push_keys.last() == Some(key) => {
                self.push_keys.pop();
                self.layer_stack.pop();
                Ok(())
            }
            DeviceEvent::Press(key) => match self.binding(*key).cloned() {
                Some(Action::LayerPush { target }) => {
                    self.push_layer(*key, target);
                    Ok(())
                }
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
                Some(Action::OverrideToggle { mode }) => {
                    self.toggle_override(mode);
                    Ok(())
                }
                Some(Action::LaunchApp {
                    path,
                    args,
                    enabled: true,
                }) => {
                    if let Err(e) = launcher.launch(&path, &args) {
                        warn!(%path, error = %e, "app launch failed");
                        self.launch_errors.push(e);
                    }
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
            // The connection manager sends this on Off and when the device goes.
            DeviceEvent::Disconnected => self.release_all(injector),
            _ => Ok(()),
        }
    }

    /// The layer on top of the stack.
    pub fn current_layer(&self) -> &str {
        self.layer_stack
            .last()
            .map(String::as_str)
            .unwrap_or_default()
    }

    /// Pushes `target`, with `key` as its back button. A missing target or a
    /// push beyond [`MAX_LAYER_DEPTH`] is ignored and logged.
    fn push_layer(&mut self, key: Key, target: String) {
        if !self.layers.iter().any(|l| l.id == target) {
            warn!(layer = %target, "layer push ignored: no such layer");
        } else if self.layer_stack.len() >= MAX_LAYER_DEPTH {
            warn!(layer = %target, depth = MAX_LAYER_DEPTH, "layer push ignored: stack full");
        } else {
            self.layer_stack.push(target);
            self.push_keys.push(key);
        }
    }

    fn binding(&self, key: Key) -> Option<&Action> {
        let current = self.layer_stack.last()?;
        let layer = self.layers.iter().find(|l| &l.id == current)?;
        layer.bindings.get(&format!("{key:?}"))
    }

    /// The LEDs for the current state: the jog mode, the button that pushed
    /// the current layer (its 'back' button) and the active override's
    /// toggle button on the current layer.
    pub fn leds(&self) -> LedState {
        let push_key = self.push_keys.last().copied();
        let override_key = self.active_override.and_then(|mode| {
            Key::ALL.iter().copied().find(|&k| {
                Some(k) != push_key && self.binding(k) == Some(&Action::OverrideToggle { mode })
            })
        });
        LedState {
            buttons: push_key
                .into_iter()
                .chain(override_key)
                .filter_map(Led::for_key)
                .collect(),
            jog: match self.jog_mode {
                JogMode::Jog => JogLed::Jog,
                JogMode::Shuttle => JogLed::Shtl,
                JogMode::Scroll => JogLed::Scrl,
            },
        }
    }
}

/// The lit LEDs, derived by [`Engine::leds`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedState {
    pub buttons: Vec<Led>,
    pub jog: JogLed,
}

/// Writes LED reports to the device, each only when its LEDs change.
#[derive(Debug, Default)]
pub struct LedWriter {
    last: Option<LedState>,
}

impl LedWriter {
    pub fn sync(
        &mut self,
        leds: &LedState,
        transport: &mut dyn HidTransport,
    ) -> Result<(), HidError> {
        let last = self.last.as_ref();
        if last.map(|l| &l.buttons) != Some(&leds.buttons) {
            transport.write(&led_report(&leds.buttons))?;
        }
        if last.map(|l| l.jog) != Some(leds.jog) {
            transport.write(&jog_led_report(&[leds.jog]))?;
        }
        self.last = Some(leds.clone());
        Ok(())
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
    use crate::launch::MockLauncher;
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
        engine
            .handle(&Press(Key::Cut), &mut mock, &mut MockLauncher::new())
            .unwrap();
        let tap = [Down(ctrl()), Down(s()), Up(s()), Up(ctrl())];
        assert_eq!(mock.calls(), &tap);
        engine
            .handle(&Release(Key::Cut), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert_eq!(mock.calls(), &tap);
    }

    #[test]
    fn held_goes_down_on_press_and_up_on_release() {
        let mut engine = engine_with(Key::In, Action::Held { combo: ctrl_s() });
        let mut mock = MockInjector::new();
        engine
            .handle(&Press(Key::In), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert_eq!(mock.calls(), &[Down(ctrl()), Down(s())]);
        assert_eq!(mock.keys_down(), &[ctrl(), s()]);
        engine
            .handle(&Release(Key::In), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert_eq!(mock.calls()[2..], [Up(s()), Up(ctrl())]);
        assert!(mock.keys_down().is_empty());
    }

    #[test]
    fn latched_goes_down_on_first_press_and_up_on_second() {
        let mut engine = engine_with(Key::Snap, Action::Latched { combo: ctrl_s() });
        let mut mock = MockInjector::new();
        engine
            .handle(&Press(Key::Snap), &mut mock, &mut MockLauncher::new())
            .unwrap();
        engine
            .handle(&Release(Key::Snap), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert_eq!(mock.calls(), &[Down(ctrl()), Down(s())]);
        assert_eq!(mock.keys_down(), &[ctrl(), s()]);
        engine
            .handle(&Press(Key::Snap), &mut mock, &mut MockLauncher::new())
            .unwrap();
        engine
            .handle(&Release(Key::Snap), &mut mock, &mut MockLauncher::new())
            .unwrap();
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
            engine
                .handle(&Press(key), &mut mock, &mut MockLauncher::new())
                .unwrap();
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
            engine
                .handle(&jog(4), &mut mock, &mut MockLauncher::new())
                .unwrap();
            engine
                .handle(&jog(5), &mut mock, &mut MockLauncher::new())
                .unwrap();
            assert!(
                mock.calls().is_empty(),
                "{mode:?}: 9 ticks is under threshold"
            );
            engine
                .handle(&jog(1), &mut mock, &mut MockLauncher::new())
                .unwrap();
            assert_eq!(mock.calls(), &[Scroll(5)]);
            engine
                .handle(&jog(25), &mut mock, &mut MockLauncher::new())
                .unwrap();
            assert_eq!(mock.calls(), &[Scroll(5), Scroll(5), Scroll(5)]);
            // 5 ticks remain; turning back 15 nets -10: one left.
            engine
                .handle(&jog(-15), &mut mock, &mut MockLauncher::new())
                .unwrap();
            assert_eq!(mock.calls()[3..], [Scroll(-5)]);
        }
    }

    #[test]
    fn shuttle_repeats_scaled_by_distance_and_stops_at_centre() {
        let mut engine = jog_engine(JogMode::Shuttle, 10, Some(8));
        let clock = FakeClock(Cell::new(Instant::now()));
        let mut mock = MockInjector::new();
        // 20 off-centre with threshold 10: 2 Hz, one every 500 ms.
        engine
            .handle(&jog(20), &mut mock, &mut MockLauncher::new())
            .unwrap();
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
        engine
            .handle(&jog(-40), &mut mock, &mut MockLauncher::new())
            .unwrap();
        clock.advance(250);
        engine.tick(&clock, &mut mock).unwrap();
        clock.advance(250);
        engine.tick(&clock, &mut mock).unwrap();
        assert_eq!(mock.calls()[2..], [Scroll(-5), Scroll(-5)]);
        // Far off-centre is capped at 8 Hz: 125 ms.
        engine
            .handle(&jog(1000), &mut mock, &mut MockLauncher::new())
            .unwrap();
        clock.advance(124);
        engine.tick(&clock, &mut mock).unwrap();
        assert_eq!(mock.calls().len(), 4);
        clock.advance(1);
        engine.tick(&clock, &mut mock).unwrap();
        assert_eq!(mock.calls().len(), 5);
        // Centre stops it.
        engine
            .handle(&jog(0), &mut mock, &mut MockLauncher::new())
            .unwrap();
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
        engine
            .handle(&jog(-1), &mut mock, &mut MockLauncher::new())
            .unwrap();
        engine
            .handle(&jog(1), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert_eq!(mock.calls(), &[Scroll(-1), Scroll(1)]);
    }

    fn push(target: &str) -> Action {
        Action::LayerPush {
            target: target.into(),
        }
    }

    /// Base layer plus `n` more ("l1".."ln"), each with only the bindings given.
    fn layered(base: &[(Key, Action)], n: usize, top: &[(Key, Action)]) -> Engine {
        let mut config = Config::default();
        for (key, action) in base {
            config.layers[0]
                .bindings
                .insert(format!("{key:?}"), action.clone());
        }
        for i in 1..=n {
            let mut bindings = BTreeMap::new();
            if i == n {
                for (key, action) in top {
                    bindings.insert(format!("{key:?}"), action.clone());
                }
            }
            config.layers.push(Layer {
                id: format!("l{i}"),
                name: format!("Layer {i}"),
                bindings,
            });
        }
        Engine::new(&config)
    }

    #[test]
    fn layer_push_switches_and_same_button_pops_back_even_if_bound() {
        let mut engine = layered(
            &[(Key::Cam1, push("l1"))],
            1,
            &[
                (Key::Cam1, Action::Hotkey { combo: ctrl_s() }),
                (Key::Cut, Action::Hotkey { combo: ctrl_s() }),
            ],
        );
        let mut mock = MockInjector::new();
        assert_eq!(engine.current_layer(), "base");
        engine
            .handle(&Press(Key::Cam1), &mut mock, &mut MockLauncher::new())
            .unwrap();
        engine
            .handle(&Release(Key::Cam1), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert_eq!(engine.current_layer(), "l1");
        // The target's bindings apply.
        engine
            .handle(&Press(Key::Cut), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert_eq!(mock.calls().len(), 4);
        // The push button is 'back', not the target's Hotkey.
        engine
            .handle(&Press(Key::Cam1), &mut mock, &mut MockLauncher::new())
            .unwrap();
        engine
            .handle(&Release(Key::Cam1), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert_eq!(engine.current_layer(), "base");
        assert_eq!(mock.calls().len(), 4);
    }

    #[test]
    fn key_held_before_layer_change_stays_down_until_its_release() {
        let mut engine = layered(
            &[
                (Key::In, Action::Held { combo: ctrl_s() }),
                (Key::Cam1, push("l1")),
            ],
            1,
            &[(Key::In, Action::Hotkey { combo: ctrl_s() })],
        );
        let mut mock = MockInjector::new();
        engine
            .handle(&Press(Key::In), &mut mock, &mut MockLauncher::new())
            .unwrap();
        engine
            .handle(&Press(Key::Cam1), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert_eq!(engine.current_layer(), "l1");
        assert_eq!(mock.keys_down(), &[ctrl(), s()]);
        engine
            .handle(&Press(Key::Cam1), &mut mock, &mut MockLauncher::new())
            .unwrap();
        engine
            .handle(&Press(Key::Cam1), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert_eq!(mock.keys_down(), &[ctrl(), s()]);
        engine
            .handle(&Release(Key::In), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert!(mock.keys_down().is_empty());
        assert_eq!(mock.calls()[2..], [Up(s()), Up(ctrl())]);
    }

    #[test]
    fn latched_keys_stay_latched_across_layer_changes() {
        let mut engine = layered(
            &[
                (Key::Snap, Action::Latched { combo: ctrl_s() }),
                (Key::Cam1, push("l1")),
            ],
            1,
            &[],
        );
        let mut mock = MockInjector::new();
        engine
            .handle(&Press(Key::Snap), &mut mock, &mut MockLauncher::new())
            .unwrap();
        engine
            .handle(&Release(Key::Snap), &mut mock, &mut MockLauncher::new())
            .unwrap();
        engine
            .handle(&Press(Key::Cam1), &mut mock, &mut MockLauncher::new())
            .unwrap();
        // Unbound on l1: pressing it there leaves the latch alone.
        engine
            .handle(&Press(Key::Snap), &mut mock, &mut MockLauncher::new())
            .unwrap();
        engine
            .handle(&Release(Key::Snap), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert_eq!(mock.keys_down(), &[ctrl(), s()]);
        engine
            .handle(&Press(Key::Cam1), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert_eq!(engine.current_layer(), "base");
        assert_eq!(mock.keys_down(), &[ctrl(), s()]);
        engine
            .handle(&Press(Key::Snap), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert!(mock.keys_down().is_empty());
    }

    #[test]
    fn push_to_missing_layer_is_ignored() {
        let mut engine = layered(&[(Key::Cam1, push("nope"))], 0, &[]);
        let mut mock = MockInjector::new();
        engine
            .handle(&Press(Key::Cam1), &mut mock, &mut MockLauncher::new())
            .unwrap();
        engine
            .handle(&Release(Key::Cam1), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert_eq!(engine.current_layer(), "base");
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn push_beyond_max_depth_is_ignored_and_pops_still_work() {
        // Each layer pushes the next with its own key: 9 pushes possible.
        let keys = [
            Key::Cam1,
            Key::Cam2,
            Key::Cam3,
            Key::Cam4,
            Key::Cam5,
            Key::Cam6,
            Key::Cam7,
            Key::Cam8,
            Key::Cam9,
        ];
        let mut config = Config::default();
        config.layers[0]
            .bindings
            .insert(format!("{:?}", keys[0]), push("l1"));
        for i in 1..=keys.len() {
            let mut bindings = BTreeMap::new();
            if i < keys.len() {
                bindings.insert(format!("{:?}", keys[i]), push(&format!("l{}", i + 1)));
            }
            config.layers.push(Layer {
                id: format!("l{i}"),
                name: format!("Layer {i}"),
                bindings,
            });
        }
        let mut engine = Engine::new(&config);
        let mut mock = MockInjector::new();
        for key in keys {
            engine
                .handle(&Press(key), &mut mock, &mut MockLauncher::new())
                .unwrap();
        }
        assert_eq!(engine.current_layer(), format!("l{}", MAX_LAYER_DEPTH - 1));
        for key in keys[..MAX_LAYER_DEPTH - 1].iter().rev() {
            engine
                .handle(&Press(*key), &mut mock, &mut MockLauncher::new())
                .unwrap();
        }
        assert_eq!(engine.current_layer(), "base");
        assert!(mock.calls().is_empty());
    }

    fn launch_app(args: &[&str], enabled: bool) -> Action {
        Action::LaunchApp {
            path: "/usr/bin/app".into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            enabled,
        }
    }

    #[test]
    fn enabled_launch_app_calls_launcher_once_and_disabled_does_nothing() {
        let mut engine = engine_with(Key::Cam1, launch_app(&["-a", "b c"], true));
        let mut mock = MockInjector::new();
        let mut launcher = MockLauncher::new();
        engine
            .handle(&Press(Key::Cam1), &mut mock, &mut launcher)
            .unwrap();
        engine
            .handle(&Release(Key::Cam1), &mut mock, &mut launcher)
            .unwrap();
        let args = vec!["-a".to_string(), "b c".to_string()];
        assert_eq!(launcher.calls(), &[("/usr/bin/app".to_string(), args)]);
        assert!(mock.calls().is_empty());

        let mut engine = engine_with(Key::Cam1, launch_app(&["-a"], false));
        let mut launcher = MockLauncher::new();
        engine
            .handle(&Press(Key::Cam1), &mut mock, &mut launcher)
            .unwrap();
        engine
            .handle(&Release(Key::Cam1), &mut mock, &mut launcher)
            .unwrap();
        assert!(launcher.calls().is_empty());
        assert!(engine.take_launch_errors().is_empty());
    }

    #[test]
    fn launch_args_with_shell_metacharacters_reach_launcher_unchanged() {
        let args = ["a; rm -rf ~", "x && y", "$(whoami)", "`id`|z"];
        let mut engine = engine_with(Key::Cam1, launch_app(&args, true));
        let mut launcher = MockLauncher::new();
        engine
            .handle(&Press(Key::Cam1), &mut MockInjector::new(), &mut launcher)
            .unwrap();
        assert_eq!(launcher.calls().len(), 1);
        assert_eq!(launcher.calls()[0].1, args);
    }

    #[test]
    fn launch_failure_becomes_error_event_and_engine_keeps_running() {
        let mut config = Config::default();
        let bindings = &mut config.layers[0].bindings;
        bindings.insert("Cam1".into(), launch_app(&[], true));
        bindings.insert("Cut".into(), Action::Hotkey { combo: ctrl_s() });
        let mut engine = Engine::new(&config);
        let mut mock = MockInjector::new();
        let mut launcher = MockLauncher::failing();
        assert_eq!(
            engine.handle(&Press(Key::Cam1), &mut mock, &mut launcher),
            Ok(())
        );
        assert_eq!(
            engine.take_launch_errors(),
            [LaunchError("cannot start /usr/bin/app".into())]
        );
        assert!(engine.take_launch_errors().is_empty());
        engine
            .handle(&Press(Key::Cut), &mut mock, &mut launcher)
            .unwrap();
        assert_eq!(mock.calls().len(), 4);
        engine
            .handle(&Press(Key::Cam1), &mut mock, &mut launcher)
            .unwrap();
        assert_eq!(launcher.calls().len(), 2);
    }

    fn override_engine() -> Engine {
        let mut config = Config::default();
        let bindings = &mut config.layers[0].bindings;
        for (key, mode) in [
            ("Cam1", OverrideMode::SystemVolume),
            ("Cam2", OverrideMode::AppVolume),
            ("Cam3", OverrideMode::Brightness),
        ] {
            bindings.insert(key.into(), Action::OverrideToggle { mode });
        }
        bindings.insert("Cut".into(), Action::Hotkey { combo: ctrl_s() });
        Engine::new(&config)
    }

    #[test]
    fn override_button_toggles_its_mode_and_again_returns_to_jog_actions() {
        let mut engine = override_engine();
        let mut mock = MockInjector::new();
        let mut l = MockLauncher::new();
        engine.handle(&Press(Key::Cam1), &mut mock, &mut l).unwrap();
        assert_eq!(engine.active_override(), Some(OverrideMode::SystemVolume));
        engine.handle(&jog(2), &mut mock, &mut l).unwrap();
        engine.handle(&jog(-1), &mut mock, &mut l).unwrap();
        assert!(mock.calls().is_empty());
        assert_eq!(
            engine.take_override_ticks(),
            [
                (OverrideMode::SystemVolume, 2),
                (OverrideMode::SystemVolume, -1)
            ]
        );
        // Another override button switches straight to its mode.
        engine.handle(&Press(Key::Cam3), &mut mock, &mut l).unwrap();
        assert_eq!(engine.active_override(), Some(OverrideMode::Brightness));
        engine.handle(&Press(Key::Cam3), &mut mock, &mut l).unwrap();
        assert_eq!(engine.active_override(), None);
        engine.handle(&jog(1), &mut mock, &mut l).unwrap();
        assert_eq!(mock.calls(), &[Scroll(1)]);
        assert!(engine.take_override_ticks().is_empty());
    }

    #[test]
    fn dial_ticks_step_active_control_and_slow_brightness_never_delays_keys() {
        use crate::overrides::ControlCall::{Down, Up};
        use crate::overrides::{Controls, FakeControl, OverrideRunner, OverrideWorker};

        let (volume, brightness) = (FakeControl::new(), FakeControl::new());
        let controls = Controls {
            volume: Box::new(volume.clone()),
            app_volume: Box::new(FakeControl::new()),
            brightness: Box::new(brightness.clone()),
        };
        let mut config = Config::default();
        config.overrides.system_volume_step = 0.5;
        let runner = OverrideRunner::new(
            controls,
            config.overrides.clone(),
            crate::clock::SystemClock,
        );
        let worker = OverrideWorker::spawn(runner);
        let mut engine = override_engine();
        let mut mock = MockInjector::new();
        let mut l = MockLauncher::new();
        let mut feed = |engine: &mut Engine, mock: &mut MockInjector, e: DeviceEvent| {
            engine.handle(&e, mock, &mut l).unwrap();
            for (mode, ticks) in engine.take_override_ticks() {
                worker.push(mode, ticks);
            }
        };

        feed(&mut engine, &mut mock, Press(Key::Cam1));
        feed(&mut engine, &mut mock, jog(1));
        wait_until(|| volume.calls() == [Up(0.5)]);
        feed(&mut engine, &mut mock, jog(-1));
        wait_until(|| volume.calls() == [Up(0.5), Down(0.5)]);

        // Brightness blocks in the fake; keys still go through at once.
        let slow = brightness.hold();
        feed(&mut engine, &mut mock, Press(Key::Cam3));
        feed(&mut engine, &mut mock, jog(-1));
        feed(&mut engine, &mut mock, Press(Key::Cut));
        assert_eq!(mock.calls().len(), 4);
        assert!(brightness.calls().is_empty());
        drop(slow);
        wait_until(|| brightness.calls() == [Down(5.0)]);
        drop(worker);
    }

    fn wait_until(f: impl Fn() -> bool) {
        let end = Instant::now() + Duration::from_secs(5);
        while !f() {
            assert!(Instant::now() < end, "timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn leds_map_jog_mode_to_its_jog_led_and_light_no_buttons_at_rest() {
        for (mode, led) in [
            (JogMode::Jog, JogLed::Jog),
            (JogMode::Shuttle, JogLed::Shtl),
            (JogMode::Scroll, JogLed::Scrl),
        ] {
            let engine = jog_engine(mode, 1, None);
            assert_eq!(
                engine.leds(),
                LedState {
                    buttons: vec![],
                    jog: led
                }
            );
        }
    }

    #[test]
    fn pushing_a_layer_lights_its_push_button_and_popping_clears_it() {
        let mut engine = layered(&[(Key::Cam1, push("l1"))], 1, &[]);
        let mut l = MockLauncher::new();
        let mut mock = MockInjector::new();
        engine.handle(&Press(Key::Cam1), &mut mock, &mut l).unwrap();
        assert_eq!(engine.leds().buttons, [Led::Cam1]);
        engine.handle(&Press(Key::Cam1), &mut mock, &mut l).unwrap();
        assert!(engine.leds().buttons.is_empty());
    }

    #[test]
    fn active_override_lights_its_toggle_button() {
        let mut engine = override_engine();
        let mut l = MockLauncher::new();
        let mut mock = MockInjector::new();
        engine.handle(&Press(Key::Cam2), &mut mock, &mut l).unwrap();
        assert_eq!(engine.leds().buttons, [Led::Cam2]);
        engine.handle(&Press(Key::Cam3), &mut mock, &mut l).unwrap();
        assert_eq!(engine.leds().buttons, [Led::Cam3]);
        engine.handle(&Press(Key::Cam3), &mut mock, &mut l).unwrap();
        assert!(engine.leds().buttons.is_empty());
    }

    #[test]
    fn led_reports_are_written_only_when_the_leds_change() {
        use crate::hid::{FakeTransport, Written::Output};
        let mut config = Config::default();
        let bindings = &mut config.layers[0].bindings;
        bindings.insert("Cam1".into(), push("l1"));
        bindings.insert("Cut".into(), Action::Hotkey { combo: ctrl_s() });
        bindings.insert("Jog".into(), Action::SetJogMode { mode: JogMode::Jog });
        config.layers.push(Layer {
            id: "l1".into(),
            name: "Layer 1".into(),
            bindings: BTreeMap::new(),
        });
        let mut engine = Engine::new(&config);
        let mut fake = FakeTransport::new();
        let mut writer = LedWriter::default();
        let mut mock = MockInjector::new();
        let mut l = MockLauncher::new();
        writer.sync(&engine.leds(), &mut fake).unwrap();
        for e in [
            Press(Key::Cut),
            Release(Key::Cut),
            jog(3),
            Press(Key::Cam1),
            Release(Key::Cam1),
            Press(Key::Cam1),
            Press(Key::Jog),
            Press(Key::Jog),
        ] {
            engine.handle(&e, &mut mock, &mut l).unwrap();
            writer.sync(&engine.leds(), &mut fake).unwrap();
        }
        assert_eq!(
            fake.written(),
            &[
                Output(vec![0x02, 0, 0, 0, 0]),
                Output(vec![0x04, 0x04]),
                // Cam1 pushes l1: its LED (bit 14) lights.
                Output(vec![0x02, 0, 0x40, 0, 0]),
                // Cam1 pops back: cleared.
                Output(vec![0x02, 0, 0, 0, 0]),
                Output(vec![0x04, 0x01]),
            ]
        );
    }

    #[test]
    fn release_all_releases_held_and_latched_and_twice_injects_nothing() {
        let mut config = Config::default();
        let bindings = &mut config.layers[0].bindings;
        bindings.insert("In".into(), Action::Held { combo: ctrl_s() });
        let shift_a = KeyCombo {
            modifiers: vec![Modifier::Shift],
            key: "A".into(),
        };
        bindings.insert("Snap".into(), Action::Latched { combo: shift_a });
        let mut engine = Engine::new(&config);
        let mut mock = MockInjector::new();
        let mut l = MockLauncher::new();
        engine.handle(&Press(Key::In), &mut mock, &mut l).unwrap();
        engine.handle(&Press(Key::Snap), &mut mock, &mut l).unwrap();
        engine
            .handle(&Release(Key::Snap), &mut mock, &mut l)
            .unwrap();
        assert_eq!(mock.keys_down().len(), 4);

        engine.release_all(&mut mock).unwrap();
        assert!(mock.keys_down().is_empty());
        let calls = mock.calls().len();
        engine.release_all(&mut mock).unwrap();
        assert_eq!(mock.calls().len(), calls);
        // The held key's physical release later injects nothing either.
        engine.handle(&Release(Key::In), &mut mock, &mut l).unwrap();
        assert_eq!(mock.calls().len(), calls);
    }

    #[test]
    fn disconnected_event_runs_release_all() {
        let mut engine = engine_with(Key::In, Action::Held { combo: ctrl_s() });
        let mut mock = MockInjector::new();
        let mut l = MockLauncher::new();
        engine.handle(&Press(Key::In), &mut mock, &mut l).unwrap();
        engine
            .handle(&DeviceEvent::Disconnected, &mut mock, &mut l)
            .unwrap();
        assert_eq!(mock.calls()[2..], [Up(s()), Up(ctrl())]);
        assert!(mock.keys_down().is_empty());
    }

    #[test]
    fn release_all_stops_dial_override_and_shuttle_repeat() {
        let mut engine = override_engine();
        let mut mock = MockInjector::new();
        let mut l = MockLauncher::new();
        engine.handle(&Press(Key::Cam1), &mut mock, &mut l).unwrap();
        engine.handle(&jog(3), &mut mock, &mut l).unwrap();
        engine.release_all(&mut mock).unwrap();
        assert_eq!(engine.active_override(), None);
        assert!(engine.take_override_ticks().is_empty());

        let mut engine = jog_engine(JogMode::Shuttle, 10, None);
        let clock = FakeClock(Cell::new(Instant::now()));
        engine.handle(&jog(50), &mut mock, &mut l).unwrap();
        engine.tick(&clock, &mut mock).unwrap();
        assert_eq!(mock.calls(), &[Scroll(5)]);
        engine.release_all(&mut mock).unwrap();
        for _ in 0..10 {
            clock.advance(1000);
            engine.tick(&clock, &mut mock).unwrap();
        }
        assert_eq!(mock.calls(), &[Scroll(5)]);
    }

    #[test]
    fn unbound_button_does_nothing() {
        let mut engine = engine_with(Key::Cut, Action::Hotkey { combo: ctrl_s() });
        let mut mock = MockInjector::new();
        engine
            .handle(&Press(Key::Out), &mut mock, &mut MockLauncher::new())
            .unwrap();
        engine
            .handle(&Release(Key::Out), &mut mock, &mut MockLauncher::new())
            .unwrap();
        assert!(mock.calls().is_empty());
    }
}
