//! Simulator mode (`freewheel --simulate`): the real connection manager and
//! mapping engine on a [`FakeTransport`] that the Simulator window drives
//! instead of a Speed Editor. Injection is off until "send for real" is on;
//! until then every action is a toast and a log line ("would press Ctrl+C").

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::clock::Clock;
use crate::config::{Action, Config, ConfigStore, JogMode};
use crate::connection::{ConnectionEvent, ConnectionManager, ConnectionStatus, RETRY_INTERVAL};
use crate::engine::Engine;
use crate::first_run::{FakeFirstRun, FirstRun};
use crate::hid::{FakeTransport, HidError, HidTransport};
use crate::inject::{InjectError, KeyCombo, KeyInjector, Keystroke};
use crate::launch::{LaunchError, Launcher};
use crate::report::{DeviceEvent, Key};
use crate::resolve::{FakeProcessLister, HandOff, ProcessLister, ResolveWatcher};

/// The command-line flag that starts simulator mode.
pub const FLAG: &str = "--simulate";
/// The simulator's config lives in this subdirectory of the real config dir.
pub const SIMULATOR_DIR: &str = "simulator";

/// True when the command line asks for simulator mode.
pub fn requested(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|a| a == FLAG)
}

/// A window title or tooltip, marked when it belongs to simulator mode.
pub fn title(base: &str, simulate: bool) -> String {
    if simulate {
        format!("{base} (Simulator)")
    } else {
        base.to_string()
    }
}
/// The session timeout (s) the simulated device grants in its auth status.
const SESSION_S: u64 = 600;

/// The simulator's own config store, next to (never on) the real one.
pub fn simulator_store(real: &ConfigStore) -> ConfigStore {
    ConfigStore::new(real.dir().join(SIMULATOR_DIR))
}

/// How the Speed Editor (real or simulated) is attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum Bus {
    Usb,
    Bluetooth,
}

/// One control of the Simulator window.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type")]
pub enum Control {
    Status,
    Connect {
        bus: Bus,
    },
    Disconnect,
    Battery {
        level: u8,
        charging: bool,
    },
    Key {
        key: String,
        down: bool,
    },
    Wheel {
        value: i32,
    },
    WheelMode {
        mode: JogMode,
    },
    SetEnabled {
        on: bool,
    },
    SendForReal {
        on: bool,
    },
    SetBinding {
        key: String,
        action: Option<Action>,
    },
    /// Binds [`sample_mappings`] through the same path as `SetBinding`.
    LoadSampleMappings,
    /// The empty setup: every binding on every page cleared, extra pages removed.
    ClearMappings,
    /// Starts or quits the Resolve process the Resolve watcher polls for.
    Resolve {
        open: bool,
    },
    /// Sets the macOS permissions the app sees; nothing asks the OS.
    Permissions {
        state: Permissions,
    },
    /// Moves the fake update source to its next [`update_step`].
    StepUpdate,
    /// Puts the app back in its first-run state.
    ResetFirstRun,
}

/// The macOS permissions (Accessibility, Input Monitoring) as the app sees them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permissions {
    Granted,
    Missing,
}

/// The updater state in the shape the settings window's update banner reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum UpdateState {
    Available {
        version: String,
    },
    Downloading {
        version: String,
        progress: Option<u8>,
    },
    Ready {
        version: String,
    },
}

/// The version the simulator's fake update source offers; it never goes online.
pub const FAKE_UPDATE_VERSION: &str = "99.0.0";

/// The fake update source: no update, available, downloading, ready, and
/// back to no update.
pub fn update_step(current: Option<&UpdateState>) -> Option<UpdateState> {
    let version = FAKE_UPDATE_VERSION.to_string();
    match current {
        None => Some(UpdateState::Available { version }),
        Some(UpdateState::Available { .. }) => Some(UpdateState::Downloading {
            version,
            progress: Some(50),
        }),
        Some(UpdateState::Downloading { .. }) => Some(UpdateState::Ready { version }),
        Some(UpdateState::Ready { .. }) => None,
    }
}

/// The layer 'Load sample mappings' adds, pushed by [`SAMPLE_PUSH_KEY`].
pub const SAMPLE_LAYER: &str = "edit";
/// The key that pushes [`SAMPLE_LAYER`]; it has an LED, lit on that page.
pub const SAMPLE_PUSH_KEY: &str = "Snap";

/// The bindings 'Load sample mappings' sets: (layer, key, action).
pub fn sample_mappings() -> Vec<(String, String, Action)> {
    let combo = |modifiers: &[crate::config::Modifier], key: &str| Action::Hotkey {
        combo: KeyCombo {
            modifiers: modifiers.to_vec(),
            key: key.into(),
        },
    };
    use crate::config::Modifier::Ctrl;
    [
        ("base", "Cut", combo(&[Ctrl], "C")),
        ("base", "In", combo(&[], "I")),
        ("base", "Out", combo(&[], "O")),
        ("base", "Esc", combo(&[Ctrl], "Z")),
        (
            "base",
            SAMPLE_PUSH_KEY,
            Action::LayerPush {
                target: SAMPLE_LAYER.into(),
            },
        ),
        (SAMPLE_LAYER, "Cut", combo(&[], "X")),
        (SAMPLE_LAYER, "In", combo(&[Ctrl], "V")),
    ]
    .into_iter()
    .map(|(l, k, a)| (l.to_string(), k.to_string(), a))
    .collect()
}

/// What the UI sees after each control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimStatus {
    pub connection: Option<ConnectionStatus>,
    pub bus: Option<Bus>,
    pub battery: Option<u8>,
    pub charging: bool,
    pub low_battery: bool,
    pub page: String,
    pub jog_mode: JogMode,
    pub lit_leds: Vec<String>,
    pub held_keys: Vec<String>,
    pub enabled: bool,
    pub send_for_real: bool,
    /// The actions since the last status, e.g. "would press Ctrl+C".
    pub toasts: Vec<String>,
    /// Every device key, for the window's key buttons.
    pub keys: Vec<String>,
    pub resolve_open: bool,
    pub permissions: Permissions,
    pub update: Option<UpdateState>,
    pub first_run: bool,
    /// How many keys are set, over every page (0 = the empty setup).
    pub mapped_keys: usize,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A [`FakeTransport`] shared by the simulator and the connection manager.
#[derive(Clone, Default)]
struct SimDevice(Arc<Mutex<FakeTransport>>);

impl SimDevice {
    /// A device scripted to pass one auth handshake.
    fn plugged() -> Self {
        let device = Self::default();
        for (sub, payload) in [(0u8, 0x5eed_u64), (2, 0), (4, SESSION_S)] {
            let mut report = vec![0x06, sub];
            report.extend_from_slice(&payload.to_le_bytes());
            lock(&device.0).push_feature(report);
        }
        device
    }
}

impl HidTransport for SimDevice {
    fn read_timeout(&mut self, buf: &mut [u8], t: Duration) -> Result<usize, HidError> {
        lock(&self.0).read_timeout(buf, t)
    }
    fn write(&mut self, data: &[u8]) -> Result<usize, HidError> {
        lock(&self.0).write(data)
    }
    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize, HidError> {
        lock(&self.0).get_feature_report(buf)
    }
    fn send_feature_report(&mut self, data: &[u8]) -> Result<(), HidError> {
        lock(&self.0).send_feature_report(data)
    }
    fn close(&mut self) {
        lock(&self.0).close()
    }
}

/// Simulated time: it only moves when a control moves it.
#[derive(Clone)]
struct SimClock(Arc<Mutex<Instant>>);

impl SimClock {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Instant::now())))
    }
    fn advance(&self, d: Duration) {
        *lock(&self.0) += d;
    }
}

impl Clock for SimClock {
    fn now(&self) -> Instant {
        *lock(&self.0)
    }
}

/// The injector and launcher while "send for real" is off: each action
/// becomes a toast and a log line instead of input.
#[derive(Default)]
struct DryRun(Vec<String>);

impl DryRun {
    fn would(&mut self, what: String) {
        let line = format!("would {what}");
        info!("{line}");
        self.0.push(line);
    }
}

fn keystroke_name(key: &Keystroke) -> String {
    match key {
        Keystroke::Modifier(m) => format!("{m:?}"),
        Keystroke::Key(k) => k.clone(),
    }
}

impl KeyInjector for DryRun {
    fn key_down(&mut self, key: &Keystroke) -> Result<(), InjectError> {
        self.would(format!("hold {}", keystroke_name(key)));
        Ok(())
    }
    fn key_up(&mut self, key: &Keystroke) -> Result<(), InjectError> {
        self.would(format!("release {}", keystroke_name(key)));
        Ok(())
    }
    fn scroll(&mut self, amount: i32) -> Result<(), InjectError> {
        self.would(format!("scroll {amount}"));
        Ok(())
    }
    fn tap_combo(&mut self, combo: &KeyCombo) -> Result<(), InjectError> {
        let mut parts: Vec<String> = combo.modifiers.iter().map(|m| format!("{m:?}")).collect();
        parts.push(combo.key.clone());
        self.would(format!("press {}", parts.join("+")));
        Ok(())
    }
}

impl Launcher for DryRun {
    fn launch(&mut self, path: &str, _args: &[String]) -> Result<(), LaunchError> {
        self.would(format!("launch {path}"));
        Ok(())
    }
}

struct Plugged {
    bus: Bus,
    device: SimDevice,
}

type Opener = Box<dyn FnMut() -> Result<SimDevice, HidError>>;

/// One simulator session: its own config, a fake device and the engine.
pub struct Simulator {
    store: ConfigStore,
    config: Config,
    plug: Arc<Mutex<Option<Plugged>>>,
    clock: SimClock,
    /// Drives the Shuttle repeat: one second per wheel control.
    shuttle_clock: SimClock,
    manager: ConnectionManager<SimDevice, Opener, SimClock, fn(u64) -> u64>,
    engine: Engine,
    injector: Box<dyn KeyInjector>,
    launcher: Box<dyn Launcher>,
    send_for_real: bool,
    connection: Option<ConnectionStatus>,
    battery: Option<(u8, bool)>,
    held: Vec<Key>,
    toasts: Vec<String>,
    resolve: ResolveWatcher,
    processes: FakeProcessLister,
    permissions: Permissions,
    update: Option<UpdateState>,
    first_run: Box<dyn FirstRun>,
}

impl Simulator {
    /// Starts with no device attached. `injector` and `launcher` are only
    /// used while "send for real" is on.
    pub fn new(
        mut store: ConfigStore,
        injector: Box<dyn KeyInjector>,
        launcher: Box<dyn Launcher>,
    ) -> Self {
        let (config, error) = store.load_or_default();
        if let Some(e) = error {
            warn!(error = %e, "simulator config unreadable, using defaults");
        }
        let plug: Arc<Mutex<Option<Plugged>>> = Arc::default();
        let slot = Arc::clone(&plug);
        let open: Opener = Box::new(move || match lock(&slot).as_mut() {
            Some(p) => {
                p.device = SimDevice::plugged();
                Ok(p.device.clone())
            }
            None => Err(HidError::Io("no simulated device attached".into())),
        });
        let clock = SimClock::new();
        let mut sim = Simulator {
            manager: ConnectionManager::new(open, clock.clone(), (|c: u64| !c) as fn(u64) -> u64),
            engine: Engine::new(&config),
            resolve: ResolveWatcher::from_settings(&config.settings),
            processes: FakeProcessLister::new(),
            permissions: Permissions::Granted,
            update: None,
            first_run: Box::new(FakeFirstRun::default()),
            store,
            config,
            plug,
            clock,
            shuttle_clock: SimClock::new(),
            injector,
            launcher,
            send_for_real: false,
            connection: None,
            battery: None,
            held: Vec::new(),
            toasts: Vec::new(),
        };
        sim.tick();
        sim
    }

    /// Applies one control and returns the status the UI receives.
    pub fn apply(&mut self, control: Control) -> SimStatus {
        match control {
            Control::Status => {}
            Control::Connect { bus } => {
                self.unplug();
                *lock(&self.plug) = Some(Plugged {
                    bus,
                    device: SimDevice::default(),
                });
                self.clock.advance(RETRY_INTERVAL);
                self.tick();
            }
            Control::Disconnect => self.unplug(),
            Control::Battery { level, charging } => {
                self.input(vec![0x07, charging.into(), level]);
            }
            Control::Key { key, down } => self.key(&key, down),
            Control::Wheel { value } => {
                let mode = if self.engine.jog_mode() == JogMode::Shuttle {
                    3
                } else {
                    0
                };
                let mut report = vec![0x03, mode];
                report.extend_from_slice(&value.to_le_bytes());
                if self.input(report) {
                    self.shuttle_clock.advance(Duration::from_secs(1));
                    let clock = self.shuttle_clock.clone();
                    self.run(|engine, injector, _| engine.tick(&clock, injector));
                }
            }
            Control::WheelMode { mode } => self.engine.set_jog_mode(mode),
            Control::SetEnabled { on } => {
                self.resolve.manual_switch();
                self.set_enabled(on);
            }
            Control::SendForReal { on } => {
                // Nothing stays held on the side we switch away from.
                self.run(|engine, injector, _| engine.release_all(injector));
                self.send_for_real = on;
            }
            Control::SetBinding { key, action } => self.set_binding(key, action),
            Control::LoadSampleMappings => {
                for (layer, key, action) in sample_mappings() {
                    self.bind(&layer, key, Some(action));
                }
                self.reload();
                info!("simulator: sample mappings loaded");
                self.toasts.push("sample mappings loaded".into());
            }
            Control::ClearMappings => {
                let base = self.config.base_layer.clone();
                self.config.layers.retain(|l| l.id == base);
                for layer in &mut self.config.layers {
                    layer.bindings.clear();
                }
                self.reload();
                info!("simulator: mappings cleared");
                self.toasts.push("empty setup loaded".into());
            }
            Control::Resolve { open } => {
                let name = self.resolve.process_name().to_owned();
                if open {
                    self.processes.start(&name);
                } else {
                    self.processes.stop(&name);
                }
                let enabled = self.connection != Some(ConnectionStatus::Off);
                match self.resolve.poll(&mut self.processes, enabled) {
                    Some(HandOff::SwitchOff { notification }) => {
                        self.toasts.push(notification.into());
                        self.set_enabled(false);
                    }
                    Some(HandOff::SwitchOn) => self.set_enabled(true),
                    None => {}
                }
            }
            Control::Permissions { state } => self.permissions = state,
            Control::StepUpdate => self.update = update_step(self.update.as_ref()),
            Control::ResetFirstRun => self.first_run.reset(),
        }
        self.status()
    }

    fn set_enabled(&mut self, on: bool) {
        let events = self.manager.set_enabled(on);
        self.feed(events);
        if on && self.connection != Some(ConnectionStatus::Connected) {
            self.clock.advance(RETRY_INTERVAL);
            self.tick();
        }
    }

    fn status(&mut self) -> SimStatus {
        let connected = self.connection == Some(ConnectionStatus::Connected);
        let jog_mode = self.engine.jog_mode();
        let leds = self.engine.leds();
        let threshold = self.config.settings.low_battery_threshold;
        SimStatus {
            connection: self.connection,
            bus: lock(&self.plug)
                .as_ref()
                .filter(|_| connected)
                .map(|p| p.bus),
            battery: self.battery.map(|(level, _)| level),
            charging: self.battery.is_some_and(|(_, c)| c),
            low_battery: self.battery.is_some_and(|(level, _)| level < threshold),
            page: self.engine.current_layer().to_string(),
            jog_mode,
            lit_leds: if connected {
                let buttons = leds.buttons.iter().map(|l| format!("{l:?}"));
                buttons.chain([format!("{:?}", leds.jog)]).collect()
            } else {
                vec![]
            },
            held_keys: self.held.iter().map(|k| format!("{k:?}")).collect(),
            enabled: self.connection != Some(ConnectionStatus::Off),
            send_for_real: self.send_for_real,
            toasts: std::mem::take(&mut self.toasts),
            keys: Key::ALL.iter().map(|k| format!("{k:?}")).collect(),
            resolve_open: self.processes.is_running(self.resolve.process_name()),
            permissions: self.permissions,
            update: self.update.clone(),
            first_run: self.first_run.is_first_run(),
            mapped_keys: self.config.layers.iter().map(|l| l.bindings.len()).sum(),
        }
    }

    /// The config the session runs.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Runs `config` (already saved by the settings window) when it differs
    /// from the running one; returns whether it did.
    pub fn sync_config(&mut self, config: Config) -> bool {
        if config == self.config {
            return false;
        }
        self.run(|engine, injector, _| engine.release_all(injector));
        self.resolve = ResolveWatcher::from_settings(&config.settings);
        self.config = config;
        let mode = self.engine.jog_mode();
        self.engine = Engine::new(&self.config);
        self.engine.set_jog_mode(mode);
        self.engine.take_reports();
        info!("simulator: config from the settings window applied");
        true
    }

    fn key(&mut self, name: &str, down: bool) {
        let Some(&key) = Key::ALL.iter().find(|k| format!("{k:?}") == name) else {
            self.toasts.push(format!("unknown key {name}"));
            return;
        };
        let mut held = self.held.clone();
        held.retain(|&k| k != key);
        if down {
            held.push(key);
        }
        let mut report = vec![0x04];
        for i in 0..6 {
            let code = held.get(i).map_or(0, |&k| k as u16);
            report.extend_from_slice(&code.to_le_bytes());
        }
        if self.input(report) {
            self.held = held;
        }
    }

    fn set_binding(&mut self, key: String, action: Option<Action>) {
        let page = self.engine.current_layer().to_string();
        self.bind(&page, key, action);
        self.reload();
    }

    /// Sets or clears one binding on `page`, adding the layer if it is new.
    fn bind(&mut self, page: &str, key: String, action: Option<Action>) {
        if !self.config.layers.iter().any(|l| l.id == page) {
            self.config.layers.push(crate::config::Layer {
                id: page.into(),
                name: page.into(),
                bindings: Default::default(),
            });
        }
        if let Some(layer) = self.config.layers.iter_mut().find(|l| l.id == page) {
            match action {
                Some(a) => layer.bindings.insert(key, a),
                None => layer.bindings.remove(&key),
            };
        }
    }

    /// Saves the simulator config and rebuilds the engine from it.
    fn reload(&mut self) {
        if let Err(e) = self.store.save(&self.config) {
            warn!(error = %e, "simulator config not saved");
            self.toasts.push(e.to_string());
        }
        self.run(|engine, injector, _| engine.release_all(injector));
        let mode = self.engine.jog_mode();
        self.engine = Engine::new(&self.config);
        self.engine.set_jog_mode(mode);
        self.engine.take_reports();
    }

    /// Sends one input report from the device; false when none is connected.
    fn input(&mut self, report: Vec<u8>) -> bool {
        if self.connection != Some(ConnectionStatus::Connected) {
            self.toasts.push("no device connected".into());
            return false;
        }
        if let Some(p) = lock(&self.plug).as_ref() {
            lock(&p.device.0).push_input(report);
        }
        self.tick();
        true
    }

    /// Removes the device; the manager sees its next read fail.
    fn unplug(&mut self) {
        if lock(&self.plug).take().is_some() {
            self.tick();
        }
    }

    fn tick(&mut self) {
        let events = self.manager.tick(Duration::ZERO);
        self.feed(events);
    }

    fn feed(&mut self, events: Vec<ConnectionEvent>) {
        for event in events {
            match event {
                ConnectionEvent::Status(s) => self.connection = Some(s),
                ConnectionEvent::Device(e) => {
                    match e {
                        DeviceEvent::Battery { charging, level } => {
                            self.battery = Some((level, charging));
                        }
                        DeviceEvent::Disconnected => {
                            self.battery = None;
                            self.held.clear();
                        }
                        _ => {}
                    }
                    self.run(|engine, injector, launcher| engine.handle(&e, injector, launcher));
                }
            }
        }
    }

    /// Runs `f` on the engine with the real or the dry-run outputs.
    fn run(
        &mut self,
        f: impl FnOnce(&mut Engine, &mut dyn KeyInjector, &mut dyn Launcher) -> Result<(), InjectError>,
    ) {
        let result = if self.send_for_real {
            f(&mut self.engine, &mut *self.injector, &mut *self.launcher)
        } else {
            let (mut keys, mut apps) = (DryRun::default(), DryRun::default());
            let result = f(&mut self.engine, &mut keys, &mut apps);
            self.toasts.extend(keys.0.into_iter().chain(apps.0));
            result
        };
        if let Err(e) = result {
            warn!(error = %e, "simulated action failed");
            self.toasts.push(e.to_string());
        }
        let errors = self.engine.take_launch_errors();
        self.toasts.extend(errors.iter().map(ToString::to_string));
        self.engine.take_reports();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{JogBinding, Layer, Modifier};
    use crate::inject::{InjectCall, MockInjector};
    use crate::launch::MockLauncher;
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    /// A mock the test keeps a handle to after the simulator takes it.
    #[derive(Clone, Default)]
    struct SharedMock(Rc<RefCell<MockInjector>>);

    impl KeyInjector for SharedMock {
        fn key_down(&mut self, key: &Keystroke) -> Result<(), InjectError> {
            self.0.borrow_mut().key_down(key)
        }
        fn key_up(&mut self, key: &Keystroke) -> Result<(), InjectError> {
            self.0.borrow_mut().key_up(key)
        }
        fn scroll(&mut self, amount: i32) -> Result<(), InjectError> {
            self.0.borrow_mut().scroll(amount)
        }
    }

    impl SharedMock {
        fn calls(&self) -> Vec<InjectCall> {
            self.0.borrow().calls().to_vec()
        }
    }

    fn hotkey(modifiers: &[Modifier], key: &str) -> Action {
        Action::Hotkey {
            combo: KeyCombo {
                modifiers: modifiers.to_vec(),
                key: key.into(),
            },
        }
    }

    fn sim_in(dir: &std::path::Path) -> (Simulator, SharedMock) {
        let mock = SharedMock::default();
        let sim = Simulator::new(
            ConfigStore::new(dir),
            Box::new(mock.clone()),
            Box::new(MockLauncher::new()),
        );
        (sim, mock)
    }

    fn connected(bus: Bus) -> (tempfile::TempDir, Simulator, SharedMock) {
        let dir = tempfile::tempdir().unwrap();
        let (mut sim, mock) = sim_in(dir.path());
        sim.apply(Control::Connect { bus });
        (dir, sim, mock)
    }

    fn key(sim: &mut Simulator, name: &str, down: bool) -> SimStatus {
        sim.apply(Control::Key {
            key: name.into(),
            down,
        })
    }

    #[test]
    fn starts_with_no_device_found() {
        let dir = tempfile::tempdir().unwrap();
        let (mut sim, _) = sim_in(dir.path());
        let s = sim.apply(Control::Status);
        assert_eq!(s.connection, Some(ConnectionStatus::Absent));
        assert_eq!((s.bus, s.battery), (None, None));
        assert!(s.lit_leds.is_empty());
        assert_eq!(s.page, "base");
        assert!(!s.send_for_real);
        assert_eq!(s.keys.len(), Key::ALL.len());
        assert_eq!(key(&mut sim, "Cut", true).toasts, ["no device connected"]);
    }

    #[test]
    fn connects_over_usb_or_bluetooth_and_disconnects() {
        for bus in [Bus::Usb, Bus::Bluetooth] {
            let (_dir, mut sim, _) = connected(bus);
            let s = sim.apply(Control::Status);
            assert_eq!(s.connection, Some(ConnectionStatus::Connected));
            assert_eq!(s.bus, Some(bus));
            assert_eq!(s.lit_leds, ["Scrl"]);

            let s = sim.apply(Control::Disconnect);
            assert_eq!(s.connection, Some(ConnectionStatus::Absent));
            assert_eq!(s.bus, None);
            assert!(s.lit_leds.is_empty());
        }
        // Switching bus reconnects on the new one.
        let (_dir, mut sim, _) = connected(Bus::Usb);
        let s = sim.apply(Control::Connect {
            bus: Bus::Bluetooth,
        });
        assert_eq!(s.connection, Some(ConnectionStatus::Connected));
        assert_eq!(s.bus, Some(Bus::Bluetooth));
    }

    #[test]
    fn battery_levels_including_low_battery() {
        let (_dir, mut sim, _) = connected(Bus::Bluetooth);
        let s = sim.apply(Control::Battery {
            level: 80,
            charging: true,
        });
        assert_eq!(
            (s.battery, s.charging, s.low_battery),
            (Some(80), true, false)
        );
        let s = sim.apply(Control::Battery {
            level: 15,
            charging: false,
        });
        assert_eq!((s.battery, s.low_battery), (Some(15), false));
        let s = sim.apply(Control::Battery {
            level: 14,
            charging: false,
        });
        assert_eq!(
            (s.battery, s.charging, s.low_battery),
            (Some(14), false, true)
        );
        assert_eq!(sim.apply(Control::Disconnect).battery, None);
    }

    #[test]
    fn off_and_on_reach_the_ui() {
        let (_dir, mut sim, _) = connected(Bus::Usb);
        let s = sim.apply(Control::SetEnabled { on: false });
        assert_eq!(s.connection, Some(ConnectionStatus::Off));
        assert!(!s.enabled);
        assert!(s.lit_leds.is_empty());
        let s = sim.apply(Control::SetEnabled { on: true });
        assert_eq!(s.connection, Some(ConnectionStatus::Connected));
        assert!(s.enabled);
    }

    #[test]
    fn every_key_presses_and_releases_through_the_engine() {
        let (_dir, mut sim, mock) = connected(Bus::Usb);
        for &k in Key::ALL {
            let name = format!("{k:?}");
            sim.apply(Control::SetBinding {
                key: name.clone(),
                action: Some(hotkey(&[Modifier::Ctrl], "C")),
            });
            let s = key(&mut sim, &name, true);
            assert_eq!(s.held_keys, std::slice::from_ref(&name));
            assert_eq!(s.toasts, ["would press Ctrl+C"], "{name}");
            let s = key(&mut sim, &name, false);
            assert!(s.held_keys.is_empty(), "{name}");
            assert!(s.toasts.is_empty(), "{name}");
        }
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn layer_push_changes_the_page_with_several_keys_set() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.layers.push(Layer {
            id: "edit".into(),
            name: "Edit".into(),
            bindings: BTreeMap::from([("Cut".into(), hotkey(&[], "X"))]),
        });
        let base = &mut config.layers[0].bindings;
        base.insert(
            "Cam1".into(),
            Action::LayerPush {
                target: "edit".into(),
            },
        );
        base.insert("Cut".into(), hotkey(&[Modifier::Ctrl], "B"));
        base.insert("In".into(), hotkey(&[], "I"));
        ConfigStore::new(dir.path()).save(&config).unwrap();
        let (mut sim, _) = sim_in(dir.path());
        sim.apply(Control::Connect { bus: Bus::Usb });

        assert_eq!(key(&mut sim, "Cut", true).toasts, ["would press Ctrl+B"]);
        key(&mut sim, "Cut", false);
        assert_eq!(key(&mut sim, "Cam1", true).page, "edit");
        key(&mut sim, "Cam1", false);
        assert_eq!(key(&mut sim, "Cut", true).toasts, ["would press X"]);
        key(&mut sim, "Cut", false);
        assert_eq!(key(&mut sim, "Cam1", true).page, "base");
    }

    #[test]
    fn clear_mappings_leaves_one_empty_page() {
        let (dir, mut sim, _) = connected(Bus::Usb);
        assert!(sim.apply(Control::LoadSampleMappings).mapped_keys >= 5);
        let s = sim.apply(Control::ClearMappings);
        assert_eq!(s.mapped_keys, 0);
        assert_eq!(s.toasts, ["empty setup loaded"]);
        let saved = ConfigStore::new(dir.path()).load_or_default().0;
        assert_eq!(saved.layers.len(), 1);
        assert_eq!(saved.layers[0].id, saved.base_layer);
        assert!(key(&mut sim, "Cut", true).toasts.is_empty());
        let c: Control = serde_json::from_str(r#"{"type":"ClearMappings"}"#).unwrap();
        assert_eq!(c, Control::ClearMappings);
    }

    #[test]
    fn sync_config_runs_a_changed_config_only() {
        let (_dir, mut sim, _) = connected(Bus::Usb);
        let same = sim.config().clone();
        assert!(!sim.sync_config(same));
        let mut changed = sim.config().clone();
        let base = changed.base_layer.clone();
        let layer = changed.layers.iter_mut().find(|l| l.id == base).unwrap();
        layer
            .bindings
            .insert("Cut".into(), hotkey(&[Modifier::Ctrl], "S"));
        assert!(sim.sync_config(changed));
        assert_eq!(sim.apply(Control::Status).mapped_keys, 1);
        assert_eq!(key(&mut sim, "Cut", true).toasts, ["would press Ctrl+S"]);
    }

    #[test]
    fn load_sample_mappings_sets_keys_and_a_layer_push_page() {
        let (dir, mut sim, mock) = connected(Bus::Usb);
        let s = sim.apply(Control::LoadSampleMappings);
        assert_eq!(s.toasts, ["sample mappings loaded"]);
        assert_eq!(
            (s.page.as_str(), s.lit_leds.clone()),
            ("base", vec!["Scrl".to_string()])
        );

        // The engine's config holds every sample binding, saved to the
        // simulator's own store.
        let saved = ConfigStore::new(dir.path()).load_or_default().0;
        for (page, k, action) in sample_mappings() {
            for config in [&sim.config, &saved] {
                let layer = config.layers.iter().find(|l| l.id == page).unwrap();
                assert_eq!(layer.bindings.get(&k), Some(&action), "{page}/{k}");
            }
        }
        assert!(sample_mappings().len() >= 5);

        // The engine acts on them.
        assert_eq!(key(&mut sim, "Cut", true).toasts, ["would press Ctrl+C"]);
        key(&mut sim, "Cut", false);
        let s = key(&mut sim, SAMPLE_PUSH_KEY, true);
        assert_eq!(s.page, SAMPLE_LAYER);
        assert_eq!(s.lit_leds, ["Snap", "Scrl"]);
        key(&mut sim, SAMPLE_PUSH_KEY, false);
        assert_eq!(key(&mut sim, "Cut", true).toasts, ["would press X"]);
        key(&mut sim, "Cut", false);
        let s = key(&mut sim, SAMPLE_PUSH_KEY, true);
        assert_eq!(
            (s.page.as_str(), s.lit_leds),
            ("base", vec!["Scrl".to_string()])
        );
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn wheel_left_and_right_in_each_wheel_mode() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        let arrows = JogBinding {
            left: Some(hotkey(&[], "Left")),
            right: Some(hotkey(&[], "Right")),
            threshold: 1,
            shuttle_max_rate_hz: None,
        };
        config.jog.per_mode.insert(JogMode::Jog, arrows.clone());
        config.jog.per_mode.insert(JogMode::Shuttle, arrows);
        ConfigStore::new(dir.path()).save(&config).unwrap();
        let (mut sim, mock) = sim_in(dir.path());
        sim.apply(Control::Connect { bus: Bus::Usb });

        for (mode, led, left, right) in [
            (JogMode::Scroll, "Scrl", "would scroll -1", "would scroll 1"),
            (JogMode::Jog, "Jog", "would press Left", "would press Right"),
            (
                JogMode::Shuttle,
                "Shtl",
                "would press Left",
                "would press Right",
            ),
        ] {
            let s = sim.apply(Control::WheelMode { mode });
            assert_eq!((s.jog_mode, s.lit_leds), (mode, vec![led.to_string()]));
            assert_eq!(sim.apply(Control::Wheel { value: -1 }).toasts, [left]);
            assert_eq!(sim.apply(Control::Wheel { value: 1 }).toasts, [right]);
        }
        assert!(mock.calls().is_empty());
    }

    /// Collects what the `tracing` fmt layer writes.
    #[derive(Clone, Default)]
    struct LogBuf(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for LogBuf {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            lock(&self.0).extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn send_for_real_off_only_toasts_and_logs_on_reaches_the_injector() {
        let logs = LogBuf::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let (_dir, mut sim, mock) = connected(Bus::Usb);
        sim.apply(Control::SetBinding {
            key: "Cut".into(),
            action: Some(hotkey(&[Modifier::Ctrl], "C")),
        });
        sim.apply(Control::SetBinding {
            key: "In".into(),
            action: Some(Action::Held {
                combo: KeyCombo {
                    modifiers: vec![],
                    key: "A".into(),
                },
            }),
        });

        let s = tracing::subscriber::with_default(subscriber, || key(&mut sim, "Cut", true));
        assert_eq!(s.toasts, ["would press Ctrl+C"]);
        let log = String::from_utf8(lock(&logs.0).clone()).unwrap();
        assert!(log.contains("would press Ctrl+C"), "{log}");
        key(&mut sim, "Cut", false);
        assert_eq!(key(&mut sim, "In", true).toasts, ["would hold A"]);
        assert_eq!(key(&mut sim, "In", false).toasts, ["would release A"]);
        assert_eq!(
            sim.apply(Control::Wheel { value: 1 }).toasts,
            ["would scroll 1"]
        );
        assert!(mock.calls().is_empty());

        assert!(sim.apply(Control::SendForReal { on: true }).send_for_real);
        let s = key(&mut sim, "Cut", true);
        assert!(s.toasts.is_empty());
        let ctrl = Keystroke::Modifier(Modifier::Ctrl);
        let c = Keystroke::Key("C".into());
        assert_eq!(
            mock.calls(),
            [
                InjectCall::Down(ctrl.clone()),
                InjectCall::Down(c.clone()),
                InjectCall::Up(c),
                InjectCall::Up(ctrl),
            ]
        );
        key(&mut sim, "Cut", false);

        // A key held for real is released when the switch goes off.
        key(&mut sim, "In", true);
        assert_eq!(mock.0.borrow().keys_down().len(), 1);
        sim.apply(Control::SendForReal { on: false });
        assert!(mock.0.borrow().keys_down().is_empty());
    }

    #[test]
    fn simulator_holds_and_toggles_a_lone_modifier() {
        let (_dir, mut sim, mock) = connected(Bus::Usb);
        let shift = KeyCombo {
            modifiers: vec![Modifier::Shift],
            key: String::new(),
        };
        for (name, action) in [
            (
                "In",
                Action::Held {
                    combo: shift.clone(),
                },
            ),
            ("Snap", Action::Latched { combo: shift }),
        ] {
            sim.apply(Control::SetBinding {
                key: name.into(),
                action: Some(action),
            });
        }
        assert_eq!(key(&mut sim, "In", true).toasts, ["would hold Shift"]);
        assert_eq!(key(&mut sim, "In", false).toasts, ["would release Shift"]);
        assert_eq!(key(&mut sim, "Snap", true).toasts, ["would hold Shift"]);
        assert!(key(&mut sim, "Snap", false).toasts.is_empty());
        assert_eq!(key(&mut sim, "Snap", true).toasts, ["would release Shift"]);
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn session_uses_its_own_config_and_never_touches_the_real_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut real = ConfigStore::new(dir.path());
        let mut real_config = Config::default();
        real_config.layers[0]
            .bindings
            .insert("Cut".into(), hotkey(&[], "Z"));
        real.save(&real_config).unwrap();
        let before = std::fs::read(real.path()).unwrap();

        let sim_store = simulator_store(&real);
        assert_ne!(sim_store.path(), real.path());
        let mock = SharedMock::default();
        let mut sim = Simulator::new(
            sim_store,
            Box::new(mock.clone()),
            Box::new(MockLauncher::new()),
        );
        sim.apply(Control::Connect {
            bus: Bus::Bluetooth,
        });
        // The simulator does not read the real mappings.
        assert!(key(&mut sim, "Cut", true).toasts.is_empty());
        key(&mut sim, "Cut", false);
        sim.apply(Control::SetBinding {
            key: "Cut".into(),
            action: Some(hotkey(&[Modifier::Ctrl], "C")),
        });
        sim.apply(Control::SetBinding {
            key: "In".into(),
            action: Some(hotkey(&[], "I")),
        });
        sim.apply(Control::SetBinding {
            key: "In".into(),
            action: None,
        });
        key(&mut sim, "Cut", true);
        sim.apply(Control::Disconnect);
        drop(sim);

        assert_eq!(std::fs::read(real.path()).unwrap(), before);
        let mut store = simulator_store(&real);
        let saved = store.load().unwrap();
        assert_eq!(
            saved.layers[0].bindings.get("Cut"),
            Some(&hotkey(&[Modifier::Ctrl], "C"))
        );
        assert!(!saved.layers[0].bindings.contains_key("In"));
    }

    #[test]
    fn resolve_closed_pauses_and_open_resumes_through_the_watcher() {
        let (_dir, mut sim, _) = connected(Bus::Usb);
        let s = sim.apply(Control::Resolve { open: true });
        assert!(s.resolve_open);
        assert_eq!(s.connection, Some(ConnectionStatus::Off));
        assert!(!s.enabled);
        assert_eq!(s.toasts, [crate::resolve::REPLUG_NOTIFICATION]);
        let s = sim.apply(Control::Resolve { open: false });
        assert!(!s.resolve_open);
        assert_eq!(s.connection, Some(ConnectionStatus::Connected));
        assert!(s.enabled);
        assert!(sim.processes.polls() >= 2);
    }

    #[test]
    fn a_manual_switch_while_resolve_is_open_wins() {
        let (_dir, mut sim, _) = connected(Bus::Usb);
        sim.apply(Control::Resolve { open: true });
        sim.apply(Control::SetEnabled { on: true });
        sim.apply(Control::SetEnabled { on: false });
        let s = sim.apply(Control::Resolve { open: false });
        assert_eq!(s.connection, Some(ConnectionStatus::Off));
    }

    #[test]
    fn permissions_follow_each_choice_without_touching_the_os() {
        let (_dir, mut sim, mock) = connected(Bus::Usb);
        assert_eq!(sim.apply(Control::Status).permissions, Permissions::Granted);
        for state in [Permissions::Missing, Permissions::Granted] {
            let s = sim.apply(Control::Permissions { state });
            assert_eq!(s.permissions, state);
            assert!(s.toasts.is_empty());
        }
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn update_steps_through_the_updater_states_offline() {
        let dir = tempfile::tempdir().unwrap();
        let (mut sim, _) = sim_in(dir.path());
        assert_eq!(sim.apply(Control::Status).update, None);
        // The JSON the update banner (src/UpdateBanner.jsx) reads.
        for expected in [
            serde_json::json!({"status": "available", "version": FAKE_UPDATE_VERSION}),
            serde_json::json!({"status": "downloading", "version": FAKE_UPDATE_VERSION, "progress": 50}),
            serde_json::json!({"status": "ready", "version": FAKE_UPDATE_VERSION}),
        ] {
            let s = sim.apply(Control::StepUpdate);
            let json = serde_json::to_value(&s).unwrap();
            assert_eq!(json["update"], expected);
        }
        assert_eq!(sim.apply(Control::StepUpdate).update, None);
    }

    #[test]
    fn reset_to_first_run_returns_the_app_to_first_run() {
        let dir = tempfile::tempdir().unwrap();
        let (mut sim, _) = sim_in(dir.path());
        assert!(!sim.apply(Control::Status).first_run);
        assert!(sim.apply(Control::ResetFirstRun).first_run);
    }

    #[test]
    fn every_ticket_state_is_reachable_from_a_control() {
        let (_dir, mut sim, _) = connected(Bus::Usb);
        let reach = |sim: &mut Simulator, json: &str| {
            let control: Control = serde_json::from_str(json).unwrap();
            sim.apply(control)
        };
        type Check = fn(&SimStatus) -> bool;
        let states: [(&str, &str, Check); 8] = [
            ("Resolve open", r#"{"type":"Resolve","open":true}"#, |s| {
                s.resolve_open && !s.enabled
            }),
            (
                "Resolve closed",
                r#"{"type":"Resolve","open":false}"#,
                |s| !s.resolve_open && s.enabled,
            ),
            (
                "permissions missing",
                r#"{"type":"Permissions","state":"Missing"}"#,
                |s| s.permissions == Permissions::Missing,
            ),
            (
                "permissions granted",
                r#"{"type":"Permissions","state":"Granted"}"#,
                |s| s.permissions == Permissions::Granted,
            ),
            ("update available", r#"{"type":"StepUpdate"}"#, |s| {
                matches!(s.update, Some(UpdateState::Available { .. }))
            }),
            ("downloading", r#"{"type":"StepUpdate"}"#, |s| {
                matches!(s.update, Some(UpdateState::Downloading { .. }))
            }),
            ("ready", r#"{"type":"StepUpdate"}"#, |s| {
                matches!(s.update, Some(UpdateState::Ready { .. }))
            }),
            ("first run", r#"{"type":"ResetFirstRun"}"#, |s| s.first_run),
        ];
        for (name, control, check) in states {
            assert!(check(&reach(&mut sim, control)), "{name}");
        }
    }

    #[test]
    fn flag_and_titles_mark_simulator_mode() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(requested(args(&["freewheel", "--simulate"])));
        assert!(!requested(args(&["freewheel"])));
        assert_eq!(
            title("freewheel settings", true),
            "freewheel settings (Simulator)"
        );
        assert_eq!(title("freewheel settings", false), "freewheel settings");
    }

    #[test]
    fn controls_deserialize_from_the_window_json() {
        let c: Control = serde_json::from_str(r#"{"type":"Connect","bus":"Bluetooth"}"#).unwrap();
        assert_eq!(
            c,
            Control::Connect {
                bus: Bus::Bluetooth
            }
        );
        let c: Control = serde_json::from_str(r#"{"type":"Key","key":"Cut","down":true}"#).unwrap();
        assert_eq!(
            c,
            Control::Key {
                key: "Cut".into(),
                down: true
            }
        );
        let c: Control = serde_json::from_str(r#"{"type":"WheelMode","mode":"Shuttle"}"#).unwrap();
        assert_eq!(
            c,
            Control::WheelMode {
                mode: JogMode::Shuttle
            }
        );
        let c: Control = serde_json::from_str(r#"{"type":"LoadSampleMappings"}"#).unwrap();
        assert_eq!(c, Control::LoadSampleMappings);
    }
}
