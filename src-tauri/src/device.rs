//! The device loop: one thread owns the connection manager, the mapping
//! engine and the key injector. The tray drives it with [`Command`]s and
//! hears back an [`Update`] when the connection status or the battery changes.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use freewheel_core::battery::Battery;
use freewheel_core::clock::Clock;
use freewheel_core::config::{Config, OverrideMode};
use freewheel_core::connection::{ConnectionEvent, ConnectionManager, ConnectionStatus};
use freewheel_core::engine::Engine;
use freewheel_core::hid::{HidError, HidTransport};
use freewheel_core::inject::KeyInjector;
use freewheel_core::launch::Launcher;
use freewheel_core::overrides::OverrideWorker;
use freewheel_core::report::DeviceEvent;
use freewheel_core::status::KeyPressed;
use tracing::{info, warn};

/// How long one loop turn waits for an input report or a command.
const POLL: Duration = Duration::from_millis(20);

pub enum Command {
    /// The tray's On/Off toggle.
    SetEnabled(bool),
    /// `save_config`: release every held key and run the new config.
    Reload(Box<Config>),
    /// The quit hook: release every held key, then acknowledge and stop.
    Quit(Sender<()>),
}

/// What the tray icon shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    On,
    Off,
    NotConnected,
}

/// What the device loop tells the tray and the settings window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// The connection status changed.
    State(ConnectionStatus),
    Battery(Battery),
    /// The active layer changed.
    Layer(String),
    /// A key went down, named for the Keys tab.
    KeyPressed(KeyPressed),
}

impl From<ConnectionStatus> for TrayState {
    fn from(status: ConnectionStatus) -> Self {
        match status {
            ConnectionStatus::Connected => TrayState::On,
            ConnectionStatus::Off => TrayState::Off,
            ConnectionStatus::Absent | ConnectionStatus::AuthFailed => TrayState::NotConnected,
        }
    }
}

/// Asks the device loop to release every held key and waits, at most
/// `timeout`, until it has. Returns whether it did.
pub fn shutdown(commands: &Sender<Command>, timeout: Duration) -> bool {
    let (done, released) = mpsc::channel();
    commands.send(Command::Quit(done)).is_ok() && released.recv_timeout(timeout).is_ok()
}

/// What the engine drives: injected keys, launched apps and the dial
/// overrides (system volume, app volume, brightness).
pub struct Outputs<'a> {
    pub injector: &'a mut dyn KeyInjector,
    pub launcher: &'a mut dyn Launcher,
    pub overrides: &'a OverrideWorker,
}

/// Runs the device loop until [`Command::Quit`] or until every sender is gone.
pub fn run<T, O, C, F>(
    mut manager: ConnectionManager<T, O, C, F>,
    mut engine: Engine,
    clock: &dyn Clock,
    outputs: Outputs<'_>,
    commands: Receiver<Command>,
    mut on_update: impl FnMut(Update),
) where
    T: HidTransport,
    O: FnMut() -> Result<T, HidError>,
    C: Clock + Clone,
    F: Fn(u64) -> u64,
{
    let Outputs {
        injector,
        launcher,
        overrides,
    } = outputs;
    loop {
        // While connected the read itself waits; otherwise the command wait does.
        let wait = if manager.status() == Some(ConnectionStatus::Connected) {
            Duration::ZERO
        } else {
            POLL
        };
        let mut events = match commands.recv_timeout(wait) {
            Ok(Command::SetEnabled(on)) => manager.set_enabled(on),
            Ok(Command::Reload(config)) => {
                release(&mut engine, injector, overrides);
                overrides.set_steps(config.overrides.clone());
                engine = Engine::new(&config);
                on_update(Update::Layer(engine.current_layer().to_string()));
                Vec::new()
            }
            Ok(Command::Quit(done)) => {
                release(&mut engine, injector, overrides);
                let _ = done.send(());
                return;
            }
            Err(RecvTimeoutError::Timeout) => Vec::new(),
            Err(RecvTimeoutError::Disconnected) => {
                return release(&mut engine, injector, overrides);
            }
        };
        events.extend(manager.tick(POLL));
        for event in events {
            match event {
                ConnectionEvent::Status(status) => on_update(Update::State(status)),
                ConnectionEvent::Device(event) => {
                    match event {
                        DeviceEvent::Battery { level, charging } => {
                            on_update(Update::Battery(Battery { level, charging }));
                        }
                        DeviceEvent::Press(key) => on_update(Update::KeyPressed(key.into())),
                        _ => {}
                    }
                    let layer = engine.current_layer().to_string();
                    let dial = engine.active_override();
                    if let Err(e) = engine.handle(&event, injector, launcher) {
                        warn!(error = %e, "device event failed");
                    }
                    if engine.current_layer() != layer {
                        on_update(Update::Layer(engine.current_layer().to_string()));
                    }
                    if engine.active_override() != dial {
                        dial_changed(dial, engine.active_override(), overrides);
                    }
                }
            }
        }
        if let Err(e) = engine.tick(clock, injector) {
            warn!(error = %e, "shuttle repeat failed");
        }
        // Only queues: a slow control never delays the next key.
        for (mode, ticks) in engine.take_override_ticks() {
            overrides.push(mode, ticks);
        }
        // The worker logs each failed control call; drop them so they
        // cannot grow.
        overrides.take_errors();
        // LED output is not wired to the device yet; drop what the engine
        // queued so it cannot grow.
        engine.take_reports();
        engine.take_launch_errors();
    }
}

/// Switches the dial override controls when the engine's mode changes:
/// leaving app volume drops its tap, entering it asks for the permission.
fn dial_changed(from: Option<OverrideMode>, to: Option<OverrideMode>, overrides: &OverrideWorker) {
    info!(?from, ?to, "dial override changed");
    if from == Some(OverrideMode::AppVolume) {
        overrides.release();
    }
    if to == Some(OverrideMode::AppVolume) {
        overrides.activate(OverrideMode::AppVolume);
    }
}

fn release(engine: &mut Engine, injector: &mut dyn KeyInjector, overrides: &OverrideWorker) {
    if let Err(e) = engine.release_all(injector) {
        warn!(error = %e, "releasing held keys failed");
    }
    overrides.release();
}

#[cfg(test)]
mod tests {
    use super::*;
    use freewheel_core::clock::SystemClock;
    use freewheel_core::config::{Action, Config, KeyCombo};
    use freewheel_core::hid::FakeTransport;
    use freewheel_core::inject::MockInjector;
    use freewheel_core::launch::MockLauncher;
    use freewheel_core::overrides::ControlCall::{Down, Up};
    use freewheel_core::overrides::{Controls, FakeControl, OverrideRunner};
    use freewheel_core::report::{DeviceEvent, Key};
    use std::thread;

    const WAIT: Duration = Duration::from_secs(5);

    type Opener = fn() -> Result<FakeTransport, HidError>;

    /// A manager whose device is never there.
    fn absent_manager() -> ConnectionManager<FakeTransport, Opener, SystemClock, fn(u64) -> u64> {
        ConnectionManager::new(|| Err(HidError::Disconnected), SystemClock, |c| c)
    }

    fn engine_holding_in(injector: &mut MockInjector) -> Engine {
        let mut config = Config::default();
        let combo = KeyCombo {
            modifiers: vec![],
            key: "S".into(),
        };
        config.layers[0]
            .bindings
            .insert(format!("{:?}", Key::In), Action::Held { combo });
        let mut engine = Engine::new(&config);
        engine
            .handle(
                &DeviceEvent::Press(Key::In),
                injector,
                &mut MockLauncher::new(),
            )
            .unwrap();
        engine
    }

    #[test]
    fn tray_state_follows_the_connection_status() {
        assert_eq!(TrayState::from(ConnectionStatus::Connected), TrayState::On);
        assert_eq!(TrayState::from(ConnectionStatus::Off), TrayState::Off);
        assert_eq!(
            TrayState::from(ConnectionStatus::Absent),
            TrayState::NotConnected
        );
        assert_eq!(
            TrayState::from(ConnectionStatus::AuthFailed),
            TrayState::NotConnected
        );
    }

    #[test]
    fn toggle_switches_the_connection_manager_off_and_on() {
        let (tx, rx) = mpsc::channel();
        let (state_tx, states) = mpsc::channel();
        let mut injector = MockInjector::new();
        thread::scope(|s| {
            let injector = &mut injector;
            s.spawn(move || {
                let on_state = move |st| state_tx.send(st).unwrap();
                let mut launcher = MockLauncher::new();
                run(
                    absent_manager(),
                    Engine::new(&Config::default()),
                    &SystemClock,
                    Outputs {
                        injector,
                        launcher: &mut launcher,
                        overrides: &idle_overrides(),
                    },
                    rx,
                    on_state,
                );
            });
            assert_eq!(
                states.recv_timeout(WAIT),
                Ok(Update::State(ConnectionStatus::Absent))
            );
            tx.send(Command::SetEnabled(false)).unwrap();
            assert_eq!(
                states.recv_timeout(WAIT),
                Ok(Update::State(ConnectionStatus::Off))
            );
            tx.send(Command::SetEnabled(true)).unwrap();
            assert_eq!(
                states.recv_timeout(WAIT),
                Ok(Update::State(ConnectionStatus::Absent))
            );
            assert!(shutdown(&tx, WAIT));
        });
    }

    #[test]
    fn battery_reports_reach_the_tray() {
        let mut dev = FakeTransport::new();
        for (sub, payload) in [(0u8, 0x1234u64), (2, 0), (4, 300)] {
            let mut r = vec![0x06, sub];
            r.extend_from_slice(&payload.to_le_bytes());
            dev.push_feature(r);
        }
        dev.push_input([0x07, 1, 64]);
        let mut dev = Some(dev);
        let open = move || dev.take().ok_or(HidError::Disconnected);
        let manager = ConnectionManager::new(open, SystemClock, (|c: u64| !c) as fn(u64) -> u64);
        let (tx, rx) = mpsc::channel();
        let (update_tx, updates) = mpsc::channel();
        let mut injector = MockInjector::new();
        thread::scope(|s| {
            let injector = &mut injector;
            s.spawn(move || {
                let mut launcher = MockLauncher::new();
                run(
                    manager,
                    Engine::new(&Config::default()),
                    &SystemClock,
                    Outputs {
                        injector,
                        launcher: &mut launcher,
                        overrides: &idle_overrides(),
                    },
                    rx,
                    move |u| update_tx.send(u).unwrap(),
                );
            });
            assert_eq!(
                updates.recv_timeout(WAIT),
                Ok(Update::State(ConnectionStatus::Connected))
            );
            assert_eq!(
                updates.recv_timeout(WAIT),
                Ok(Update::Battery(Battery {
                    level: 64,
                    charging: true
                }))
            );
            assert!(shutdown(&tx, WAIT));
        });
    }

    type OnceOpener = Box<dyn FnMut() -> Result<FakeTransport, HidError> + Send>;

    /// A manager whose device authenticates, then sends `inputs`.
    fn connected_manager(
        inputs: Vec<Vec<u8>>,
    ) -> ConnectionManager<FakeTransport, OnceOpener, SystemClock, fn(u64) -> u64> {
        let mut dev = FakeTransport::new();
        for (sub, payload) in [(0u8, 0x1234u64), (2, 0), (4, 300)] {
            let mut r = vec![0x06, sub];
            r.extend_from_slice(&payload.to_le_bytes());
            dev.push_feature(r);
        }
        for input in inputs {
            dev.push_input(input);
        }
        let mut dev = Some(dev);
        let open: OnceOpener = Box::new(move || dev.take().ok_or(HidError::Disconnected));
        ConnectionManager::new(open, SystemClock, (|c: u64| !c) as fn(u64) -> u64)
    }

    /// A 0x04 key report with `key` down.
    fn key_report(key: Key) -> Vec<u8> {
        let mut r = vec![0x04; 13];
        r[1..].fill(0);
        r[1..3].copy_from_slice(&(key as u16).to_le_bytes());
        r
    }

    fn with_binding(key: Key, action: Action) -> Config {
        let mut config = Config::default();
        config.layers[0].bindings.insert(format!("{key:?}"), action);
        config
    }

    /// An override worker on fakes that no test looks at.
    fn idle_overrides() -> OverrideWorker {
        overrides_on(FakeControl::new(), FakeControl::new(), &Config::default())
    }

    fn overrides_on(
        volume: FakeControl,
        brightness: FakeControl,
        config: &Config,
    ) -> OverrideWorker {
        let controls = Controls {
            volume: Box::new(volume),
            app_volume: Box::new(FakeControl::new()),
            brightness: Box::new(brightness),
        };
        OverrideWorker::spawn(OverrideRunner::new(
            controls,
            config.overrides.clone(),
            SystemClock,
        ))
    }

    /// A 0x03 jog report turning the wheel by `value`.
    fn jog_report(value: i32) -> Vec<u8> {
        let mut r = vec![0x03, 0];
        r.extend_from_slice(&value.to_le_bytes());
        r
    }

    fn wait_until(f: impl Fn() -> bool) {
        let end = std::time::Instant::now() + WAIT;
        while !f() {
            assert!(std::time::Instant::now() < end, "timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn a_wheel_turn_with_volume_or_brightness_picked_reaches_that_control() {
        for (mode, picked) in [
            (OverrideMode::SystemVolume, "volume"),
            (OverrideMode::Brightness, "brightness"),
        ] {
            let config = with_binding(Key::Cam1, Action::OverrideToggle { mode });
            let manager = connected_manager(vec![key_report(Key::Cam1), jog_report(2)]);
            let (volume, brightness) = (FakeControl::new(), FakeControl::new());
            let (tx, rx) = mpsc::channel();
            let mut injector = MockInjector::new();
            thread::scope(|s| {
                let injector = &mut injector;
                let (v, b) = (volume.clone(), brightness.clone());
                let config = &config;
                s.spawn(move || {
                    run(
                        manager,
                        Engine::new(config),
                        &SystemClock,
                        Outputs {
                            injector,
                            launcher: &mut MockLauncher::new(),
                            overrides: &overrides_on(v, b, config),
                        },
                        rx,
                        |_| {},
                    );
                });
                let steps = &config.overrides;
                if mode == OverrideMode::SystemVolume {
                    wait_until(|| volume.calls() == [Up(steps.system_volume_step * 2.0)]);
                    assert!(brightness.calls().is_empty(), "{picked}");
                } else {
                    wait_until(|| {
                        brightness.calls() == [Up(f32::from(steps.brightness_step) * 2.0)]
                    });
                    assert!(volume.calls().is_empty(), "{picked}");
                }
                assert!(shutdown(&tx, WAIT));
            });
            assert!(
                injector.calls().is_empty(),
                "{picked}: the turn sent no keys"
            );
        }
    }

    #[test]
    fn a_saved_config_changes_the_dial_step() {
        let mut saved = with_binding(
            Key::Cam1,
            Action::OverrideToggle {
                mode: OverrideMode::SystemVolume,
            },
        );
        saved.overrides.system_volume_step = 0.25;
        let manager = connected_manager(vec![key_report(Key::Cam1), jog_report(-1)]);
        let volume = FakeControl::new();
        let (tx, rx) = mpsc::channel();
        // Queued before the loop runs, so it lands before the turn is read.
        tx.send(Command::Reload(Box::new(saved))).unwrap();
        let mut injector = MockInjector::new();
        thread::scope(|s| {
            let injector = &mut injector;
            let v = volume.clone();
            s.spawn(move || {
                let config = Config::default();
                run(
                    manager,
                    Engine::new(&config),
                    &SystemClock,
                    Outputs {
                        injector,
                        launcher: &mut MockLauncher::new(),
                        overrides: &overrides_on(v, FakeControl::new(), &config),
                    },
                    rx,
                    |_| {},
                );
            });
            wait_until(|| volume.calls() == [Down(0.25)]);
            assert!(shutdown(&tx, WAIT));
        });
    }

    #[test]
    fn reload_applies_a_saved_config_to_the_running_engine() {
        let combo = KeyCombo {
            modifiers: vec![],
            key: "X".into(),
        };
        let config = with_binding(Key::Cut, Action::Hotkey { combo });
        let manager = connected_manager(vec![key_report(Key::Cut)]);
        let (tx, rx) = mpsc::channel();
        // Queued before the loop runs, so it lands before the press is read.
        tx.send(Command::Reload(Box::new(config))).unwrap();
        let (update_tx, updates) = mpsc::channel();
        let mut injector = MockInjector::new();
        thread::scope(|s| {
            let injector = &mut injector;
            s.spawn(move || {
                run(
                    manager,
                    Engine::new(&Config::default()),
                    &SystemClock,
                    Outputs {
                        injector,
                        launcher: &mut MockLauncher::new(),
                        overrides: &idle_overrides(),
                    },
                    rx,
                    move |u| update_tx.send(u).unwrap(),
                );
            });
            assert_eq!(updates.recv_timeout(WAIT), Ok(Update::Layer("base".into())));
            assert_eq!(
                updates.recv_timeout(WAIT),
                Ok(Update::State(ConnectionStatus::Connected))
            );
            assert_eq!(
                updates.recv_timeout(WAIT),
                Ok(Update::KeyPressed(Key::Cut.into()))
            );
            assert!(shutdown(&tx, WAIT));
        });
        let x = freewheel_core::inject::Keystroke::Key("X".into());
        assert_eq!(
            injector.calls(),
            [
                freewheel_core::inject::InjectCall::Down(x.clone()),
                freewheel_core::inject::InjectCall::Up(x)
            ]
        );
    }

    #[test]
    fn a_press_names_its_printed_key_and_a_layer_change_is_reported() {
        let mut config = with_binding(
            Key::Cam1,
            Action::LayerPush {
                target: "edit".into(),
            },
        );
        config.layers.push(freewheel_core::config::Layer {
            id: "edit".into(),
            name: "Edit".into(),
            bindings: Default::default(),
        });
        let manager = connected_manager(vec![key_report(Key::Cam1)]);
        let (tx, rx) = mpsc::channel();
        let (update_tx, updates) = mpsc::channel();
        let mut injector = MockInjector::new();
        thread::scope(|s| {
            let injector = &mut injector;
            s.spawn(move || {
                run(
                    manager,
                    Engine::new(&config),
                    &SystemClock,
                    Outputs {
                        injector,
                        launcher: &mut MockLauncher::new(),
                        overrides: &idle_overrides(),
                    },
                    rx,
                    move |u| update_tx.send(u).unwrap(),
                );
            });
            assert_eq!(
                updates.recv_timeout(WAIT),
                Ok(Update::State(ConnectionStatus::Connected))
            );
            let pressed = updates.recv_timeout(WAIT).unwrap();
            assert_eq!(
                pressed,
                Update::KeyPressed(KeyPressed {
                    key: "Cam1".into(),
                    label: "CAM 1".into()
                })
            );
            assert_eq!(updates.recv_timeout(WAIT), Ok(Update::Layer("edit".into())));
            assert!(shutdown(&tx, WAIT));
        });
    }

    #[test]
    fn quit_hook_releases_held_keys_before_acknowledging() {
        let mut injector = MockInjector::new();
        let engine = engine_holding_in(&mut injector);
        assert!(!injector.keys_down().is_empty());
        let (tx, rx) = mpsc::channel();
        thread::scope(|s| {
            let injector = &mut injector;
            s.spawn(move || {
                let mut launcher = MockLauncher::new();
                run(
                    absent_manager(),
                    engine,
                    &SystemClock,
                    Outputs {
                        injector,
                        launcher: &mut launcher,
                        overrides: &idle_overrides(),
                    },
                    rx,
                    |_| {},
                );
            });
            assert!(shutdown(&tx, WAIT));
        });
        assert!(injector.keys_down().is_empty());
    }

    #[test]
    fn shutdown_reports_a_loop_that_is_gone() {
        let (tx, rx) = mpsc::channel();
        drop(rx);
        assert!(!shutdown(&tx, WAIT));
    }
}
