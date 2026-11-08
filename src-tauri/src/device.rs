//! The device loop: one thread owns the connection manager, the mapping
//! engine and the key injector. The tray drives it with [`Command`]s and
//! hears back an [`Update`] when the connection status or the battery changes.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use freewheel_core::battery::Battery;
use freewheel_core::clock::Clock;
use freewheel_core::config::{Config, OverrideMode};
use freewheel_core::connection::{ConnectionEvent, ConnectionManager, ConnectionStatus};
use freewheel_core::diagnostics::DeviceDiagnostics;
use freewheel_core::engine::{Engine, LedWriter};
use freewheel_core::hid::{HidError, HidTransport};
use freewheel_core::inject::KeyInjector;
use freewheel_core::launch::Launcher;
use freewheel_core::overrides::OverrideWorker;
use freewheel_core::profiles::FrontApp;
use freewheel_core::report::DeviceEvent;
use freewheel_core::status::KeyPressed;
use tracing::warn;

/// How long one loop turn waits for an input report or a command.
const POLL: Duration = Duration::from_millis(20);
/// Without a connected device there is nothing to read: the loop waits this
/// long for a command (which still ends the wait at once) between turns, so
/// a PC without a Speed Editor wakes freewheel 4 times a second, not 50.
/// The open retries come every 3 s (`RETRY_INTERVAL`) either way.
const IDLE_POLL: Duration = Duration::from_millis(250);
/// The Device test view hears at most this often while reports stream in…
const DIAGNOSTICS_MIN: Duration = Duration::from_millis(100);
/// …and at least this often (its re-auth countdown), whatever happens.
const DIAGNOSTICS_MAX: Duration = Duration::from_secs(1);

pub enum Command {
    /// The tray's On/Off toggle.
    SetEnabled(bool),
    /// `save_config`: release every held key and run the new config.
    Reload(Box<Config>),
    /// The quit hook: release every held key, then acknowledge and stop.
    Quit(Sender<()>),
    /// The Wheel tab's 'Back to normal': leave the dial override.
    ExitOverride,
    /// Another app came to the front (front_app.rs): its profile, or Default.
    FrontApp(FrontApp),
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
    /// The dial override a key turned the wheel into (None: back to normal).
    Override(Option<OverrideMode>),
    /// The Help & backup Device test view's data changed.
    Diagnostics(Box<DeviceDiagnostics>),
    /// The keys lit on the device, by binding name (empty: none, or not
    /// connected), for the Keys tab drawing.
    Leds(Vec<String>),
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
    let mut diagnostics_at: Option<std::time::Instant> = None;
    let mut device = DeviceOutput::default();
    // The last app in front other than freewheel: a new config hears it again.
    let mut front: Option<FrontApp> = None;
    loop {
        // While connected the read itself waits; otherwise the command wait does.
        let wait = if manager.status() == Some(ConnectionStatus::Connected) {
            Duration::ZERO
        } else {
            idle_wait(manager.next_attempt_in())
        };
        let mut events = match commands.recv_timeout(wait) {
            Ok(Command::SetEnabled(on)) => {
                if !on {
                    // Off and the step aside for Resolve: dark before the
                    // handle closes.
                    device.clear(&mut manager);
                }
                manager.set_enabled(on)
            }
            Ok(Command::FrontApp(app)) => {
                if !app.own {
                    front = Some(app.clone());
                }
                if engine.front_app(&app) {
                    on_update(Update::Layer(engine.current_layer().to_string()));
                }
                engine.take_outcomes();
                Vec::new()
            }
            Ok(Command::ExitOverride) => {
                let dial = engine.active_override();
                if engine.exit_override() {
                    dial_changed(dial, None, overrides);
                    on_update(Update::Override(None));
                }
                Vec::new()
            }
            Ok(Command::Reload(config)) => {
                if engine.active_override().is_some() {
                    on_update(Update::Override(None));
                }
                release(&mut engine, injector, overrides);
                overrides.set_steps(config.overrides.clone());
                // The wheel stays in the mode it is in (as the Simulator's
                // sync_config); the device hears it and the LEDs again.
                let mode = engine.jog_mode();
                engine = Engine::new(&config);
                engine.set_jog_mode(mode);
                if let Some(app) = &front {
                    engine.front_app(app);
                }
                device.resend();
                on_update(Update::Layer(engine.current_layer().to_string()));
                Vec::new()
            }
            Ok(Command::Quit(done)) => {
                release(&mut engine, injector, overrides);
                device.clear(&mut manager);
                let _ = done.send(());
                return;
            }
            Err(RecvTimeoutError::Timeout) => Vec::new(),
            Err(RecvTimeoutError::Disconnected) => {
                device.clear(&mut manager);
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
                        on_update(Update::Override(engine.active_override()));
                    }
                }
            }
        }
        if let Err(e) = engine.tick(clock, injector) {
            warn!(error = %e, "shuttle repeat failed");
        }
        // A wheel turn that rested is written as its one line; the engine
        // logged every result already.
        engine.end_idle_wheel_gesture(std::time::Instant::now());
        engine.take_outcomes();
        // Only queues: a slow control never delays the next key.
        for (mode, ticks) in engine.take_override_ticks() {
            overrides.push(mode, ticks);
        }
        // The worker logs each failed control call; drop them so they
        // cannot grow.
        overrides.take_errors();
        // Output reports (the wheel mode the engine needs) go to the device.
        // A failed write is logged and shown in the Device test view; the
        // engine reads every wheel report by its own mode byte, so a lost
        // mode write cannot make the wheel run away.
        for report in engine.take_reports() {
            if manager.status() == Some(ConnectionStatus::Connected)
                && let Err(e) = manager.write(&report)
            {
                warn!(error = %e, "output report not written");
            }
        }
        if let Some(lit) = device.sync(&mut manager, &engine) {
            on_update(Update::Leds(lit));
        }
        engine.take_launch_errors();
        let now = std::time::Instant::now();
        let since = diagnostics_at.map(|t| now.saturating_duration_since(t));
        if since.is_none_or(|s| s >= DIAGNOSTICS_MAX)
            || (since.is_some_and(|s| s >= DIAGNOSTICS_MIN) && manager.take_diagnostics_changed())
        {
            diagnostics_at = Some(now);
            on_update(Update::Diagnostics(Box::new(manager.diagnostics())));
        }
    }
}

/// The command wait of a loop turn without a connected device: until the
/// next open attempt, at most [`IDLE_POLL`], at least [`POLL`].
fn idle_wait(next_attempt: Option<Duration>) -> Duration {
    next_attempt.unwrap_or(IDLE_POLL).clamp(POLL, IDLE_POLL)
}

/// The key and wheel LEDs (bmd.py output reports 0x02 and 0x04, see
/// [`LedWriter`]): written only while the device is connected and
/// authenticated, all again after every connect and re-auth (a device that
/// slept or was reset comes back dark), cleared on Off / step aside / quit.
#[derive(Default)]
struct DeviceOutput {
    leds: LedWriter,
    /// The manager session the LEDs were last sent in.
    session: Option<u64>,
    /// The lit keys the UI was last told about.
    shown: Option<Vec<String>>,
}

impl DeviceOutput {
    /// Writes the LEDs that changed; returns the lit keys when they changed
    /// for the UI.
    fn sync<T, O, C, F>(
        &mut self,
        manager: &mut ConnectionManager<T, O, C, F>,
        engine: &Engine,
    ) -> Option<Vec<String>>
    where
        T: HidTransport,
        O: FnMut() -> Result<T, HidError>,
        C: Clock + Clone,
        F: Fn(u64) -> u64,
    {
        let lit = if manager.status() == Some(ConnectionStatus::Connected) {
            if self.session != Some(manager.session()) {
                self.session = Some(manager.session());
                self.leds.reset();
            }
            let leds = engine.leds();
            if let Err(e) = self.leds.sync(&leds, &mut |r| manager.write(r)) {
                warn!(error = %e, "setting the key lights failed");
            }
            leds.names()
        } else {
            self.session = None;
            Vec::new()
        };
        (self.shown.as_ref() != Some(&lit)).then(|| {
            self.shown = Some(lit.clone());
            lit
        })
    }

    /// Sends every LED again on the next sync.
    fn resend(&mut self) {
        self.session = None;
    }

    /// Turns every LED off while the device is still open.
    fn clear<T, O, C, F>(&mut self, manager: &mut ConnectionManager<T, O, C, F>)
    where
        T: HidTransport,
        O: FnMut() -> Result<T, HidError>,
        C: Clock + Clone,
        F: Fn(u64) -> u64,
    {
        self.session = None;
        if manager.status() == Some(ConnectionStatus::Connected)
            && let Err(e) = self.leds.clear(&mut |r| manager.write(r))
        {
            warn!(error = %e, "turning the key lights off failed");
        }
    }
}

/// Switches the dial override controls when the engine's mode changes:
/// leaving app volume drops its tap, entering it asks for the permission.
fn dial_changed(from: Option<OverrideMode>, to: Option<OverrideMode>, overrides: &OverrideWorker) {
    tracing::debug!(?from, ?to, "dial override changed");
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

    #[test]
    fn without_a_device_the_loop_wakes_4_times_a_second() {
        assert_eq!(idle_wait(None), Duration::from_millis(250));
        assert_eq!(
            idle_wait(Some(Duration::from_secs(3))),
            Duration::from_millis(250)
        );
        assert_eq!(
            idle_wait(Some(Duration::from_millis(100))),
            Duration::from_millis(100)
        );
        assert_eq!(idle_wait(Some(Duration::ZERO)), POLL);
    }
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

    /// Passes every update but the Device test view's and the LEDs' on to `f`.
    fn skip_diagnostics(mut f: impl FnMut(Update)) -> impl FnMut(Update) {
        move |u| {
            if !matches!(u, Update::Diagnostics(_) | Update::Leds(_)) {
                f(u)
            }
        }
    }

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
                let on_state = skip_diagnostics(move |st| state_tx.send(st).unwrap());
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
                    skip_diagnostics(move |u| update_tx.send(u).unwrap()),
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
    fn a_press_logs_key_page_and_mapping_runs_it_and_logs_the_result() {
        let site = Action::LaunchApp {
            path: "explorer.exe".into(),
            args: vec!["https://example.com".into()],
            enabled: true,
            cwd: None,
            per_os: None,
        };
        let config = with_binding(Key::Cut, site);
        let manager = connected_manager(vec![key_report(Key::Cut), jog_report(3)]);
        let (tx, rx) = mpsc::channel();
        let mut injector = MockInjector::new();
        let mut launcher = MockLauncher::new();
        let log = thread::scope(|s| {
            let (injector, launcher) = (&mut injector, &mut launcher);
            let config = &config;
            let looped = s.spawn(move || {
                freewheel_core::logging::capture(tracing::Level::INFO, || {
                    run(
                        manager,
                        Engine::new(config),
                        &SystemClock,
                        Outputs {
                            injector,
                            launcher,
                            overrides: &idle_overrides(),
                        },
                        rx,
                        |_| {},
                    );
                })
            });
            // The inputs run out, so the device goes and the wheel turn ends.
            thread::sleep(Duration::from_millis(300));
            assert!(shutdown(&tx, WAIT));
            looped.join().unwrap()
        });
        for line in [
            r#"key CUT pressed on page "Base" -> Open website example.com"#,
            r#"launched example.com ok: explorer.exe "https://example.com""#,
            "wheel turned +3 in Scroll mode (1 report) -> scroll 1 ×3",
        ] {
            assert!(log.contains(line), "{line} missing:\n{log}");
        }
        // The real device's lines name no simulator.
        assert!(!log.contains("simulated device"), "{log}");
        assert_eq!(
            launcher.calls(),
            [(
                "explorer.exe".to_string(),
                vec!["https://example.com".to_string()]
            )]
        );
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
                    skip_diagnostics(move |u| update_tx.send(u).unwrap()),
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
                    skip_diagnostics(move |u| update_tx.send(u).unwrap()),
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

    /// A device the test keeps a handle to: reads replay `inputs`, then
    /// time out (staying connected) until `unplug`; every write is recorded.
    #[derive(Clone, Default)]
    struct Dev(std::sync::Arc<std::sync::Mutex<DevState>>);

    #[derive(Default)]
    struct DevState {
        inputs: std::collections::VecDeque<Vec<u8>>,
        features: std::collections::VecDeque<Vec<u8>>,
        written: Vec<freewheel_core::hid::Written>,
        unplugged: bool,
        closed: bool,
    }

    impl Dev {
        fn authenticating() -> Dev {
            let dev = Dev::default();
            dev.handshake();
            dev
        }
        fn handshake(&self) {
            let mut s = self.0.lock().unwrap();
            for (sub, payload) in [(0u8, 0x1234u64), (2, 0), (4, 300)] {
                let mut r = vec![0x06, sub];
                r.extend_from_slice(&payload.to_le_bytes());
                s.features.push_back(r);
            }
        }
        fn input(&self, report: Vec<u8>) {
            self.0.lock().unwrap().inputs.push_back(report);
        }
        fn unplug(&self) {
            self.0.lock().unwrap().unplugged = true;
        }
        /// The output reports written, in order (features left out).
        fn outputs(&self) -> Vec<Vec<u8>> {
            use freewheel_core::hid::Written::Output;
            let s = self.0.lock().unwrap();
            s.written
                .iter()
                .filter_map(|w| match w {
                    Output(r) => Some(r.clone()),
                    _ => None,
                })
                .collect()
        }
        fn closed(&self) -> bool {
            self.0.lock().unwrap().closed
        }
    }

    impl HidTransport for Dev {
        fn read_timeout(&mut self, buf: &mut [u8], _: Duration) -> Result<usize, HidError> {
            let mut s = self.0.lock().unwrap();
            if s.closed || s.unplugged {
                return Err(HidError::Disconnected);
            }
            match s.inputs.pop_front() {
                Some(r) => {
                    buf[..r.len()].copy_from_slice(&r);
                    Ok(r.len())
                }
                None => {
                    drop(s);
                    thread::sleep(Duration::from_millis(1));
                    Ok(0)
                }
            }
        }
        fn write(&mut self, data: &[u8]) -> Result<usize, HidError> {
            let mut s = self.0.lock().unwrap();
            if s.closed || s.unplugged {
                return Err(HidError::Disconnected);
            }
            s.written
                .push(freewheel_core::hid::Written::Output(data.to_vec()));
            Ok(data.len())
        }
        fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize, HidError> {
            let mut s = self.0.lock().unwrap();
            let r = s.features.pop_front().ok_or(HidError::Disconnected)?;
            buf[..r.len()].copy_from_slice(&r);
            Ok(r.len())
        }
        fn send_feature_report(&mut self, data: &[u8]) -> Result<(), HidError> {
            let mut s = self.0.lock().unwrap();
            s.written
                .push(freewheel_core::hid::Written::Feature(data.to_vec()));
            Ok(())
        }
        fn close(&mut self) {
            self.0.lock().unwrap().closed = true;
        }
    }

    /// A clock the test moves forward, for the re-auth.
    #[derive(Clone)]
    struct TestClock(std::sync::Arc<std::sync::Mutex<std::time::Instant>>);

    impl Clock for TestClock {
        fn now(&self) -> std::time::Instant {
            *self.0.lock().unwrap()
        }
    }

    type DevOpener = Box<dyn FnMut() -> Result<Dev, HidError> + Send>;

    /// A manager that opens `devs` in order (then nothing is plugged in).
    fn manager_of(
        devs: &[&Dev],
        clock: TestClock,
    ) -> ConnectionManager<Dev, DevOpener, TestClock, fn(u64) -> u64> {
        let mut queue: std::collections::VecDeque<Dev> =
            devs.iter().map(|d| (*d).clone()).collect();
        let open: DevOpener = Box::new(move || queue.pop_front().ok_or(HidError::Disconnected));
        ConnectionManager::new(open, clock, (|c: u64| !c) as fn(u64) -> u64)
    }

    fn test_clock() -> TestClock {
        TestClock(std::sync::Arc::new(std::sync::Mutex::new(
            std::time::Instant::now(),
        )))
    }

    /// A key report with `key` down, then all keys up.
    fn tap(dev: &Dev, key: Key) {
        dev.input(key_report(key));
        let mut up = vec![0u8; 13];
        up[0] = 0x04;
        dev.input(up);
    }

    const SCROLL_MODE: [u8; 7] = [0x03, 0, 0, 0, 0, 0, 0xff];
    const SHUTTLE_MODE: [u8; 7] = [0x03, 3, 0, 0, 0, 0, 0xff];
    const KEYS_OFF: [u8; 5] = [0x02, 0, 0, 0, 0];
    const JOG_OFF: [u8; 2] = [0x04, 0];
    const SCRL_LIT: [u8; 2] = [0x04, 0x04];

    /// The config the LED tests run: SNAP latches Ctrl, CAM 1 goes to page
    /// "edit"; the profile Notes (page "notes") is for notepad.exe (SHTL,
    /// JOG, SCRL unbound: their printed modes).
    fn led_config() -> Config {
        use freewheel_core::config::{Layer, Modifier};
        let mut config = with_binding(
            Key::Snap,
            Action::Latched {
                combo: KeyCombo {
                    modifiers: vec![Modifier::Ctrl],
                    key: String::new(),
                },
            },
        );
        config.layers[0].bindings.insert(
            "Cam1".into(),
            Action::LayerPush {
                target: "edit".into(),
            },
        );
        config.layers.push(Layer {
            id: "edit".into(),
            name: "Edit".into(),
            bindings: Default::default(),
        });
        config.layers.push(Layer {
            id: "notes".into(),
            name: "Notes".into(),
            bindings: Default::default(),
        });
        config.profiles.push(freewheel_core::config::Profile {
            id: "notes".into(),
            name: "Notes".into(),
            apps: vec![freewheel_core::config::ProfileApp {
                name: "Notepad".into(),
                windows: Some("notepad.exe".into()),
                macos: Some("notepad.exe".into()),
                linux: Some("notepad.exe".into()),
            }],
            pages: vec!["notes".into()],
            color: None,
            icon: None,
        });
        config
    }

    /// Runs the loop on `manager` and `led_config()` while `body` drives it;
    /// returns every LED update the UI heard.
    fn with_loop<M>(
        manager: M,
        body: impl FnOnce(&Sender<Command>, &dyn Fn(&[&str])),
    ) -> Vec<Vec<String>>
    where
        M: Send + 'static,
        M: RunManager,
    {
        let (tx, rx) = mpsc::channel();
        let (update_tx, updates) = mpsc::channel();
        let mut injector = MockInjector::new();
        let heard = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        thread::scope(|s| {
            let injector = &mut injector;
            s.spawn(move || {
                manager.run_with(
                    Engine::new(&led_config()),
                    Outputs {
                        injector,
                        launcher: &mut MockLauncher::new(),
                        overrides: &idle_overrides(),
                    },
                    rx,
                    move |u| {
                        let _ = update_tx.send(u);
                    },
                );
            });
            let heard2 = std::sync::Arc::clone(&heard);
            let wait_lit = move |want: &[&str]| {
                let end = std::time::Instant::now() + WAIT;
                loop {
                    let h = heard2.lock().unwrap();
                    if h.last().is_some_and(|l: &Vec<String>| l == want) {
                        return;
                    }
                    drop(h);
                    assert!(std::time::Instant::now() < end, "never lit {want:?}");
                    if let Ok(Update::Leds(l)) = updates.recv_timeout(Duration::from_millis(5)) {
                        heard2.lock().unwrap().push(l);
                    }
                }
            };
            body(&tx, &wait_lit);
            assert!(shutdown(&tx, WAIT));
        });
        std::sync::Arc::try_unwrap(heard)
            .unwrap()
            .into_inner()
            .unwrap()
    }

    /// Lets [`with_loop`] take any manager type.
    trait RunManager {
        fn run_with(
            self,
            engine: Engine,
            outputs: Outputs<'_>,
            commands: Receiver<Command>,
            on_update: impl FnMut(Update),
        );
    }

    impl<T, O, C, F> RunManager for ConnectionManager<T, O, C, F>
    where
        T: HidTransport,
        O: FnMut() -> Result<T, HidError>,
        C: Clock + Clone,
        F: Fn(u64) -> u64,
    {
        fn run_with(
            self,
            engine: Engine,
            outputs: Outputs<'_>,
            commands: Receiver<Command>,
            on_update: impl FnMut(Update),
        ) {
            run(self, engine, &SystemClock, outputs, commands, on_update);
        }
    }

    #[test]
    fn connecting_sends_the_wheel_mode_and_lights_scrl_then_off_and_quit_go_dark() {
        let dev = Dev::authenticating();
        let heard = with_loop(manager_of(&[&dev], test_clock()), |tx, lit| {
            lit(&["Scrl"]);
            // After the handshake: the wheel mode, every key dark, SCRL lit.
            assert_eq!(dev.outputs(), [&SCROLL_MODE[..], &KEYS_OFF, &SCRL_LIT]);
            tx.send(Command::SetEnabled(false)).unwrap();
            lit(&[]);
            // Off (and the step aside for Resolve, which sends the same
            // command) turns everything off before the handle closes.
            assert_eq!(dev.outputs()[3..], [&KEYS_OFF[..], &JOG_OFF]);
            assert!(dev.closed());
        });
        assert_eq!(heard, [vec!["Scrl".to_string()], vec![]]);
    }

    #[test]
    fn the_app_in_front_is_heard_and_stepping_aside_keeps_priority() {
        let clock = test_clock();
        let (first, second) = (Dev::authenticating(), Dev::authenticating());
        let manager = manager_of(&[&first, &second], clock.clone());
        let front = |name: &str| Command::FrontApp(FrontApp::new(&[name]));
        with_loop(manager, |tx, lit| {
            lit(&["Scrl"]);
            // Notepad in front: its profile's page (nothing of Default lit).
            tx.send(front("notepad.exe")).unwrap();
            tx.send(Command::FrontApp(FrontApp::own())).unwrap();
            tx.send(front("code.exe")).unwrap();
            lit(&["Scrl"]);
            // Stepped aside for Resolve (Off): Notepad coming to the front
            // turns nothing back on and lights nothing.
            tx.send(Command::SetEnabled(false)).unwrap();
            lit(&[]);
            tx.send(front("notepad.exe")).unwrap();
            thread::sleep(Duration::from_millis(100));
            assert!(first.closed());
            assert!(second.outputs().is_empty());
            tx.send(Command::SetEnabled(true)).unwrap();
            *clock.0.lock().unwrap() += freewheel_core::connection::RETRY_INTERVAL;
            lit(&["Scrl"]);
            tx.send(Command::Reload(Box::new(led_config()))).unwrap();
            thread::sleep(Duration::from_millis(100));
            lit(&["Scrl"]);
        });
    }

    #[test]
    fn quit_turns_the_lights_off_before_acknowledging() {
        let dev = Dev::authenticating();
        with_loop(manager_of(&[&dev], test_clock()), |_, lit| lit(&["Scrl"]));
        // with_loop ends with the quit hook.
        assert_eq!(dev.outputs()[3..], [&KEYS_OFF[..], &JOG_OFF]);
    }

    #[test]
    fn wheel_buttons_latched_keys_and_the_page_key_light_on_the_device() {
        let dev = Dev::authenticating();
        let heard = with_loop(manager_of(&[&dev], test_clock()), |_, lit| {
            lit(&["Scrl"]);
            tap(&dev, Key::Shtl);
            lit(&["Shtl"]);
            tap(&dev, Key::Snap);
            lit(&["Snap", "Shtl"]);
            tap(&dev, Key::Cam1);
            lit(&["Cam1", "Snap", "Shtl"]);
            tap(&dev, Key::Cam1);
            lit(&["Snap", "Shtl"]);
            tap(&dev, Key::Snap);
            lit(&["Shtl"]);
            tap(&dev, Key::Jog);
            lit(&["Jog"]);
        });
        assert_eq!(heard.len(), 7);
        assert_eq!(
            dev.outputs()[3..],
            [
                // SHTL: the wheel reads the dial position, its LED lights.
                &SHUTTLE_MODE[..],
                &[0x04, 0x02],
                // SNAP latched: bit 5.
                &[0x02, 0x20, 0, 0, 0],
                // CAM 1 goes to its page: + bit 14.
                &[0x02, 0x20, 0x40, 0, 0],
                // and back.
                &[0x02, 0x20, 0, 0, 0],
                // SNAP unlatched.
                &KEYS_OFF,
                // JOG (counts ticks, as Scroll does).
                &SCROLL_MODE,
                &[0x04, 0x01],
                // The quit hook.
                &KEYS_OFF,
                &JOG_OFF,
            ]
        );
    }

    #[test]
    fn a_reconnect_and_a_reauth_send_the_wheel_mode_and_leds_again() {
        let clock = test_clock();
        let (first, second) = (Dev::authenticating(), Dev::authenticating());
        let manager = manager_of(&[&first, &second], clock.clone());
        with_loop(manager, |_, lit| {
            lit(&["Scrl"]);
            tap(&first, Key::Shtl);
            tap(&first, Key::Snap);
            lit(&["Snap", "Shtl"]);
            // Unplugged: the latch is released (release all), nothing lit.
            first.unplug();
            lit(&[]);
            // Plugged back in (the retry is 3 s): the wheel mode the engine
            // still has, and its LEDs, on the new handle.
            *clock.0.lock().unwrap() += freewheel_core::connection::RETRY_INTERVAL;
            lit(&["Shtl"]);
            wait_until(|| second.outputs().len() == 3);
            assert_eq!(
                second.outputs(),
                [&SHUTTLE_MODE[..], &KEYS_OFF, &[0x04, 0x02]]
            );
            // The re-auth (300 s timeout, 10 s early) sends the LEDs again.
            second.handshake();
            *clock.0.lock().unwrap() += Duration::from_secs(290);
            wait_until(|| second.outputs().len() == 5);
            assert_eq!(second.outputs()[3..], second.outputs()[1..3]);
        });
    }

    #[test]
    fn a_saved_config_keeps_the_wheel_mode_and_sends_it_and_the_leds_again() {
        let dev = Dev::authenticating();
        with_loop(manager_of(&[&dev], test_clock()), |tx, lit| {
            lit(&["Scrl"]);
            tap(&dev, Key::Shtl);
            lit(&["Shtl"]);
            tx.send(Command::Reload(Box::new(led_config()))).unwrap();
            wait_until(|| dev.outputs().len() == 8);
            assert_eq!(
                dev.outputs()[5..],
                [&SHUTTLE_MODE[..], &KEYS_OFF, &[0x04, 0x02]]
            );
        });
    }

    #[test]
    fn the_ui_hears_no_lit_keys_while_the_device_is_absent_or_off() {
        let heard = with_loop(absent_manager(), |tx, lit| {
            tx.send(Command::SetEnabled(false)).unwrap();
            tx.send(Command::SetEnabled(true)).unwrap();
            lit(&[]);
        });
        assert_eq!(heard, [Vec::<String>::new()]);
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

    #[test]
    fn a_connect_writes_the_wheel_mode_and_the_device_test_view_hears_the_reports() {
        use freewheel_core::hid::Written;
        use freewheel_core::report::{DeviceJogMode, set_jog_mode};
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct Shared(Arc<Mutex<FakeTransport>>);
        impl HidTransport for Shared {
            /// After its script the device stays quiet (no disconnect).
            fn read_timeout(&mut self, b: &mut [u8], t: Duration) -> Result<usize, HidError> {
                let read = self.0.lock().unwrap().read_timeout(b, t);
                read.or_else(|_| {
                    thread::sleep(Duration::from_millis(1));
                    Ok(0)
                })
            }
            fn write(&mut self, d: &[u8]) -> Result<usize, HidError> {
                self.0.lock().unwrap().write(d)
            }
            fn get_feature_report(&mut self, b: &mut [u8]) -> Result<usize, HidError> {
                self.0.lock().unwrap().get_feature_report(b)
            }
            fn send_feature_report(&mut self, d: &[u8]) -> Result<(), HidError> {
                self.0.lock().unwrap().send_feature_report(d)
            }
            fn close(&mut self) {}
        }

        let dev = Shared::default();
        {
            let mut f = dev.0.lock().unwrap();
            for (sub, payload) in [(0u8, 0x1234u64), (2, 0), (4, 300)] {
                let mut r = vec![0x06, sub];
                r.extend_from_slice(&payload.to_le_bytes());
                f.push_feature(r);
            }
            f.push_input(key_report(Key::Cut));
        }
        let mut once = Some(dev.clone());
        let open = move || once.take().ok_or(HidError::Disconnected);
        let manager = ConnectionManager::new(open, SystemClock, (|c: u64| !c) as fn(u64) -> u64);
        let mut config = Config::default();
        config.jog.default_mode = freewheel_core::config::JogMode::Shuttle;
        let (tx, rx) = mpsc::channel();
        let (update_tx, updates) = mpsc::channel();
        let mut injector = MockInjector::new();
        thread::scope(|s| {
            let injector = &mut injector;
            let config = &config;
            s.spawn(move || {
                run(
                    manager,
                    Engine::new(config),
                    &SystemClock,
                    Outputs {
                        injector,
                        launcher: &mut MockLauncher::new(),
                        overrides: &idle_overrides(),
                    },
                    rx,
                    move |u| {
                        if let Update::Diagnostics(d) = u {
                            let _ = update_tx.send(d);
                        }
                    },
                );
            });
            let end = std::time::Instant::now() + WAIT;
            let seen = loop {
                let d = updates.recv_timeout(WAIT).expect("diagnostics");
                if d.held == ["CUT"] || std::time::Instant::now() > end {
                    break d;
                }
            };
            assert_eq!(seen.status, Some(ConnectionStatus::Connected));
            assert_eq!(seen.presses, ["CUT"]);
            assert!(
                seen.reports
                    .iter()
                    .any(|r| r.hex == "03 03 00 00 00 00 ff" && !r.failed),
                "{:?}",
                seen.reports
            );
            assert!(shutdown(&tx, WAIT));
        });
        let written = dev.0.lock().unwrap().written().to_vec();
        assert_eq!(
            written[3],
            Written::Output(set_jog_mode(DeviceJogMode::AbsoluteDeadzero).to_vec())
        );
    }
}
