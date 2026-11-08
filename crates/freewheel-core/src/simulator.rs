//! Simulator mode (`freewheel --simulate`): the real connection manager and
//! mapping engine on a [`FakeTransport`] that the Simulator window drives
//! instead of a Speed Editor. With 'Do actions for real' on (the default,
//! kept in the simulator's own state file) a press does what its mapping
//! says on this PC, through the same injector, launcher and dial controls as
//! the device loop; off, each result line says "not done: Do actions for real
//! is off (would press Ctrl+C)".

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::clock::Clock;
use crate::config::{Action, Config, ConfigStore, JogMode, OverrideMode};
use crate::connection::{ConnectionEvent, ConnectionManager, ConnectionStatus, RETRY_INTERVAL};
use crate::diagnostics::{DeviceDiagnostics, DeviceInfo};
use crate::engine::{Engine, LedWriter};
use crate::first_run::{FakeFirstRun, FirstRun};
use crate::hid::{FakeTransport, HidError, HidTransport};
use crate::inject::{InjectError, KeyCombo, KeyInjector, Keystroke};
use crate::launch::{LaunchError, Launcher};
use crate::overrides::{DialOutput, NoDial};
use crate::profiles::FrontApp;
use crate::report::{DeviceEvent, Key};
use crate::resolve::{FakeProcessLister, HandOff, ProcessLister, ResolveWatcher};

/// The command-line flag that starts simulator mode.
pub const FLAG: &str = "--simulate";
/// The simulator's config lives in this subdirectory of the real config dir.
pub const SIMULATOR_DIR: &str = "simulator";
/// The Simulator's own switches, next to its config.
pub const STATE_FILE: &str = "simulator-state.json";

/// What the Simulator keeps between runs (not part of any config).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimState {
    /// 'Do actions for real': presses send keys, open apps and change the
    /// volume on this PC. On unless the user turned it off.
    #[serde(default = "on")]
    pub do_actions_for_real: bool,
}

fn on() -> bool {
    true
}

impl Default for SimState {
    fn default() -> Self {
        SimState {
            do_actions_for_real: true,
        }
    }
}

impl SimState {
    /// The state in `dir`; the default when there is none or it is unreadable.
    pub fn load(dir: &std::path::Path) -> Self {
        std::fs::read(dir.join(STATE_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(&self, dir: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(dir.join(STATE_FILE), json)
    }
}

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
    /// The empty setup: every binding on every page cleared, extra pages and profiles removed.
    ClearMappings,
    /// Starts or quits the Resolve process the Resolve watcher polls for.
    Resolve {
        open: bool,
    },
    /// Sets the macOS permissions the app sees; nothing asks the OS.
    Permissions {
        state: Permissions,
    },
    /// Publishes a newer version on the fake update source ([`update_step`]).
    StepUpdate,
    /// Puts the app back in its first-run state.
    ResetFirstRun,
    /// The Wheel tab's 'Back to normal': leaves the dial override.
    ExitOverride,
    /// Fakes the app in front, by its program name (`notepad.exe`); `None`
    /// is freewheel's own window.
    FrontApp {
        name: Option<String>,
    },
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
    /// A newer version is published (the settings window takes it from there).
    Available { version: String },
}

/// The version the simulator's fake update source offers; it never goes online.
pub const FAKE_UPDATE_VERSION: &str = "99.0.0";

/// The fake update source after a 'New version': a newer version is
/// published. The settings window (src/simUpdate.js) picks the version
/// number, finds it and runs the download, ready and restart states itself,
/// exactly as with the real updater.
pub fn update_step() -> Option<UpdateState> {
    Some(UpdateState::Available {
        version: FAKE_UPDATE_VERSION.to_string(),
    })
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
        // The prototype's media keys, sent as the OS media keys.
        ("base", "StopPlay", combo(&[], crate::inject::PLAY_PAUSE)),
        ("base", "AudioOnly", combo(&[], crate::inject::MUTE)),
        (
            "base",
            "FullView",
            Action::OverrideToggle {
                mode: OverrideMode::SystemVolume,
            },
        ),
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
    /// The dial override a key turned the wheel into, if any.
    pub wheel_override: Option<OverrideMode>,
    /// The last app in front other than freewheel, real or faked.
    pub front_app: Option<String>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A [`FakeTransport`] shared by the simulator and the connection manager.
#[derive(Clone, Default)]
struct SimDevice(Arc<Mutex<FakeTransport>>);

impl SimDevice {
    /// A device on `bus` scripted to pass one auth handshake.
    fn plugged(bus: Bus) -> Self {
        let device = Self::default();
        lock(&device.0).set_info(DeviceInfo {
            bus: Some(bus),
            vendor_id: 0x1edb,
            product_id: 0xda0e,
            product: Some("DaVinci Resolve Speed Editor (simulated)".into()),
            candidates: 1,
            ..DeviceInfo::default()
        });
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
    fn info(&self) -> Option<DeviceInfo> {
        lock(&self.0).info()
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

/// The injector and launcher while 'Do actions for real' is off: they do
/// nothing; the engine's result lines say what they would have done.
#[derive(Default)]
struct NoOutput;

impl KeyInjector for NoOutput {
    fn key_down(&mut self, _: &Keystroke) -> Result<(), InjectError> {
        Ok(())
    }
    fn key_up(&mut self, _: &Keystroke) -> Result<(), InjectError> {
        Ok(())
    }
    fn scroll(&mut self, _: i32) -> Result<(), InjectError> {
        Ok(())
    }
}

impl Launcher for NoOutput {
    fn launch(&mut self, _: &str, _: &[String]) -> Result<(), LaunchError> {
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
    /// The LEDs written to the simulated device, as the device loop does.
    leds: LedWriter,
    led_session: Option<u64>,
    injector: Box<dyn KeyInjector>,
    launcher: Box<dyn Launcher>,
    dial: Box<dyn DialOutput>,
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
    /// The last app in front other than freewheel itself.
    front: Option<FrontApp>,
}

impl Simulator {
    /// Starts with no device attached. `injector` and `launcher` are only
    /// used while 'Do actions for real' is on ([`SimState`], on by default);
    /// the dial controls are [`NoDial`] until [`with_dial`](Self::with_dial).
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
                p.device = SimDevice::plugged(p.bus);
                Ok(p.device.clone())
            }
            None => Err(HidError::Io("no simulated device attached".into())),
        });
        let clock = SimClock::new();
        let real = SimState::load(store.dir()).do_actions_for_real;
        info!(
            "simulator: Do actions for real is {}",
            if real { "on" } else { "off" }
        );
        let mut sim = Simulator {
            manager: ConnectionManager::new(open, clock.clone(), (|c: u64| !c) as fn(u64) -> u64),
            engine: sim_engine(&config, JogModeKeep::Default, real),
            leds: LedWriter::default(),
            led_session: None,
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
            dial: Box::new(NoDial),
            send_for_real: real,
            connection: None,
            battery: None,
            held: Vec::new(),
            toasts: Vec::new(),
            front: None,
        };
        sim.tick();
        sim
    }

    /// Turns the dial overrides (volume, brightness) through `dial` while
    /// 'Do actions for real' is on, as the device loop's OverrideWorker.
    pub fn with_dial(mut self, mut dial: Box<dyn DialOutput>) -> Self {
        dial.set_steps(self.config.overrides.clone());
        self.dial = dial;
        self
    }

    /// Ends a wheel turn that has rested at `now` and returns its line
    /// (the session thread calls this while no control comes in).
    pub fn idle(&mut self, now: Instant) -> Vec<String> {
        self.engine.end_idle_wheel_gesture(now);
        self.engine.take_outcomes()
    }

    /// Quit: releases every held key on the side in use, keeping the switch.
    pub fn release_all(&mut self) {
        self.run(|engine, injector, _| engine.release_all(injector));
        self.dial.release();
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
            Control::WheelMode { mode } => self.run(|engine, _, _| {
                engine.set_jog_mode(mode);
                Ok(())
            }),
            Control::SetEnabled { on } => {
                self.resolve.manual_switch();
                self.set_enabled(on);
            }
            Control::SendForReal { on } => {
                // Nothing stays held on the side we switch away from.
                self.release_all();
                self.send_for_real = on;
                self.engine.set_dry_run(!on);
                let state = SimState {
                    do_actions_for_real: on,
                };
                if let Err(e) = state.save(self.store.dir()) {
                    warn!(error = %e, "simulator state not saved");
                }
                let line = if on {
                    "Do actions for real is on: presses send keys, open apps and change the volume on this PC"
                } else {
                    "Do actions for real is off: presses only show what they would do"
                };
                info!("simulator: {line}");
                self.toasts.push(line.into());
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
                self.config.profiles.clear();
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
            Control::StepUpdate => self.update = update_step(),
            Control::ResetFirstRun => self.first_run.reset(),
            Control::ExitOverride => {
                self.run(|engine, _, _| {
                    engine.exit_override();
                    Ok(())
                });
            }
            Control::FrontApp { name } => {
                let app = match name.as_deref().map(str::trim) {
                    Some(n) if !n.is_empty() => FrontApp::new(&[n]),
                    _ => FrontApp::own(),
                };
                info!("simulator: {} is in front", app.name);
                self.front_app(app);
            }
        }
        self.status()
    }

    /// Another app came to the front (the OS watcher in --simulate, or the
    /// Simulator's picker): the engine switches profile as the device loop does.
    pub fn front_app(&mut self, app: FrontApp) {
        if !app.own {
            self.front = Some(app.clone());
        }
        if self.engine.front_app(&app) {
            self.toasts.extend(self.engine.take_outcomes());
        }
    }

    /// A rebuilt engine hears the app in front again.
    fn refront(&mut self) {
        if let Some(app) = self.front.clone() {
            self.engine.front_app(&app);
            self.engine.take_outcomes();
        }
    }

    fn set_enabled(&mut self, on: bool) {
        if !on && self.connection == Some(ConnectionStatus::Connected) {
            // As the device loop: dark before the handle closes.
            self.led_session = None;
            let _ = self.leds.clear(&mut |r| self.manager.write(r));
        }
        let events = self.manager.set_enabled(on);
        self.feed(events);
        if on && self.connection != Some(ConnectionStatus::Connected) {
            self.clock.advance(RETRY_INTERVAL);
            self.tick();
        }
    }

    fn status(&mut self) -> SimStatus {
        let connected = self.connection == Some(ConnectionStatus::Connected);
        // The LEDs go to the simulated device too, so the Device test view
        // lists them as the real one would (all again on each connect/re-auth).
        if connected {
            if self.led_session != Some(self.manager.session()) {
                self.led_session = Some(self.manager.session());
                self.leds.reset();
            }
            let leds = self.engine.leds();
            let _ = self.leds.sync(&leds, &mut |r| self.manager.write(r));
        } else {
            self.led_session = None;
        }
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
            lit_leds: if connected { leds.names() } else { vec![] },
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
            wheel_override: self.engine.active_override(),
            front_app: self.front.as_ref().map(|f| f.name.clone()),
        }
    }

    /// The Device test view's data for the simulated Speed Editor: the same
    /// recorder the real device loop uses, fed the simulated reports.
    pub fn diagnostics(&self) -> DeviceDiagnostics {
        let mut d = self.manager.diagnostics();
        d.simulated = true;
        d
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
        let changes = crate::describe::config_changes(&self.config, &config).len();
        self.config = config;
        self.dial.set_steps(self.config.overrides.clone());
        self.engine = sim_engine(
            &self.config,
            JogModeKeep::Keep(self.engine.jog_mode()),
            self.send_for_real,
        );
        self.engine.take_reports();
        self.refront();
        info!("simulator: running the saved config ({changes} changes)");
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
        self.engine = sim_engine(
            &self.config,
            JogModeKeep::Keep(self.engine.jog_mode()),
            self.send_for_real,
        );
        self.engine.take_reports();
        self.refront();
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

    /// Runs `f` on the engine with the real outputs (the same injector,
    /// launcher and dial controls as the device loop) or, with 'Do actions
    /// for real' off, outputs that do nothing.
    fn run(
        &mut self,
        f: impl FnOnce(&mut Engine, &mut dyn KeyInjector, &mut dyn Launcher) -> Result<(), InjectError>,
    ) {
        let dial = self.engine.active_override();
        let result = if self.send_for_real {
            f(&mut self.engine, &mut *self.injector, &mut *self.launcher)
        } else {
            f(&mut self.engine, &mut NoOutput, &mut NoOutput)
        };
        if let Err(e) = result {
            warn!(error = %e, "simulated device: action failed");
        }
        // The engine said what each action did (or would have done).
        self.toasts.extend(self.engine.take_outcomes());
        self.engine.take_launch_errors();
        let ticks = self.engine.take_override_ticks();
        let exited = self.engine.take_override_exit();
        if self.send_for_real {
            // As the device loop: leaving app volume drops its tap,
            // entering it asks for the permission.
            if exited && dial == Some(OverrideMode::AppVolume) {
                self.dial.release();
            }
            let now = self.engine.active_override();
            if now != dial && now == Some(OverrideMode::AppVolume) {
                self.dial.activate(OverrideMode::AppVolume);
            }
            for (mode, ticks) in ticks {
                self.dial.push(mode, ticks);
            }
        }
        // Output reports (the wheel mode) go to the simulated device, so the
        // Device test view lists them as the real one would.
        for report in self.engine.take_reports() {
            let _ = self.manager.write(&report);
        }
    }
}

/// Whether a rebuilt engine keeps the wheel mode it was in.
enum JogModeKeep {
    Default,
    Keep(JogMode),
}

/// The engine for the simulated device: its lines say so, and with 'Do
/// actions for real' off its results say what they would have done.
fn sim_engine(config: &Config, mode: JogModeKeep, real: bool) -> Engine {
    let mut engine = Engine::new(config);
    if let JogModeKeep::Keep(mode) = mode {
        engine.set_jog_mode(mode);
    }
    engine.set_simulated(true);
    engine.set_dry_run(!real);
    engine
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

    /// A simulator with 'Do actions for real' off (most tests check what
    /// it would do); [`real_sim_in`] keeps the default.
    fn sim_in(dir: &std::path::Path) -> (Simulator, SharedMock) {
        let (mut sim, mock) = real_sim_in(dir);
        sim.apply(Control::SendForReal { on: false });
        (sim, mock)
    }

    fn real_sim_in(dir: &std::path::Path) -> (Simulator, SharedMock) {
        let mock = SharedMock::default();
        let sim = Simulator::new(
            ConfigStore::new(dir),
            Box::new(mock.clone()),
            Box::new(MockLauncher::new()),
        );
        (sim, mock)
    }

    /// The result line with 'Do actions for real' off.
    fn off(what: &str) -> String {
        format!("{} (would {what})", crate::engine::NOT_DONE)
    }

    /// Turns the wheel by `value` and lets it rest: the turn's line.
    fn turn(sim: &mut Simulator, value: i32) -> Vec<String> {
        let mut lines = sim.apply(Control::Wheel { value }).toasts;
        lines.extend(sim.idle(Instant::now() + Duration::from_secs(1)));
        lines
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
    fn the_sample_media_keys_press_play_pause_and_mute_and_full_view_is_a_knob() {
        let (_dir, mut sim, mock) = connected(Bus::Usb);
        sim.apply(Control::LoadSampleMappings);
        sim.apply(Control::SendForReal { on: true });
        key(&mut sim, "StopPlay", true);
        key(&mut sim, "StopPlay", false);
        key(&mut sim, "AudioOnly", true);
        key(&mut sim, "AudioOnly", false);
        let pressed: Vec<_> = mock
            .calls()
            .into_iter()
            .filter_map(|c| match c {
                InjectCall::Down(Keystroke::Key(k)) => Some(k),
                _ => None,
            })
            .collect();
        assert_eq!(pressed, [crate::inject::PLAY_PAUSE, crate::inject::MUTE]);
        key(&mut sim, "FullView", true);
        let s = key(&mut sim, "FullView", false);
        assert_eq!(s.wheel_override, Some(OverrideMode::SystemVolume));
        let s = sim.apply(Control::ExitOverride);
        assert_eq!(s.wheel_override, None);
        assert_eq!(s.toasts, ["the wheel is back to normal"]);
        // Nothing to leave: nothing said.
        assert!(sim.apply(Control::ExitOverride).toasts.is_empty());
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
            assert_eq!(s.toasts, [off("press Ctrl+C")], "{name}");
            let s = key(&mut sim, &name, false);
            assert!(s.held_keys.is_empty(), "{name}");
            assert!(s.toasts.is_empty(), "{name}");
        }
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn the_faked_front_app_switches_profiles_and_default_comes_back_on_its_page() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        for (id, name) in [("music", "Music"), ("notes", "Notes")] {
            config.layers.push(Layer {
                id: id.into(),
                name: name.into(),
                bindings: BTreeMap::new(),
            });
        }
        config.profiles.push(crate::config::Profile {
            id: "writing".into(),
            name: "Writing".into(),
            apps: vec![crate::config::ProfileApp {
                name: "Notepad".into(),
                windows: Some("notepad.exe".into()),
                macos: Some("notepad.exe".into()),
                linux: Some("notepad.exe".into()),
            }],
            pages: vec!["notes".into()],
        });
        config.layers[0].bindings.insert(
            "Cam1".into(),
            Action::LayerPush {
                target: "music".into(),
            },
        );
        ConfigStore::new(dir.path()).save(&config).unwrap();
        let (mut sim, _) = sim_in(dir.path());
        sim.apply(Control::Connect { bus: Bus::Usb });
        // On Default, CAM 1 goes to Music.
        sim.apply(Control::Key {
            key: "Cam1".into(),
            down: true,
        });
        sim.apply(Control::Key {
            key: "Cam1".into(),
            down: false,
        });
        assert_eq!(sim.apply(Control::Status).page, "music");
        let front = |sim: &mut Simulator, name: Option<&str>| {
            sim.apply(Control::FrontApp {
                name: name.map(str::to_string),
            })
        };
        let s = front(&mut sim, Some("notepad.exe"));
        assert_eq!(
            (s.page.as_str(), s.front_app.as_deref()),
            ("notes", Some("notepad.exe"))
        );
        assert!(
            !s.lit_leds.contains(&"Cam1".to_string()),
            "{:?}",
            s.lit_leds
        );
        assert_eq!(
            s.toasts,
            ["profile is now \"Writing\", page \"Notes\" (notepad.exe is in front)"]
        );
        // The Simulator window itself is freewheel: nothing changes.
        let s = front(&mut sim, None);
        assert_eq!(s.page, "notes");
        // A saved config keeps the profile of the app in front.
        let mut saved = sim.config().clone();
        saved.layers[2].name = "Notepad keys".into();
        sim.sync_config(saved);
        assert_eq!(sim.apply(Control::Status).page, "notes");
        // Another app: Default (a save starts it on its first page, as in 1.0.0).
        let s = front(&mut sim, Some("explorer.exe"));
        assert_eq!(s.page, "base");
        // Without a save in between, Default comes back on the page it was on.
        sim.apply(Control::Key {
            key: "Cam1".into(),
            down: true,
        });
        sim.apply(Control::Key {
            key: "Cam1".into(),
            down: false,
        });
        front(&mut sim, Some("notepad.exe"));
        let s = front(&mut sim, Some("explorer.exe"));
        assert_eq!(s.page, "music");
        assert!(s.lit_leds.contains(&"Cam1".to_string()));
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

        assert_eq!(key(&mut sim, "Cut", true).toasts, [off("press Ctrl+B")]);
        key(&mut sim, "Cut", false);
        assert_eq!(key(&mut sim, "Cam1", true).page, "edit");
        key(&mut sim, "Cam1", false);
        assert_eq!(key(&mut sim, "Cut", true).toasts, [off("press X")]);
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
        assert_eq!(key(&mut sim, "Cut", true).toasts, [off("press Ctrl+S")]);
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
        assert_eq!(key(&mut sim, "Cut", true).toasts, [off("press Ctrl+C")]);
        key(&mut sim, "Cut", false);
        let s = key(&mut sim, SAMPLE_PUSH_KEY, true);
        assert_eq!(s.page, SAMPLE_LAYER);
        assert_eq!(s.lit_leds, ["Snap", "Scrl"]);
        key(&mut sim, SAMPLE_PUSH_KEY, false);
        assert_eq!(key(&mut sim, "Cut", true).toasts, [off("press X")]);
        key(&mut sim, "Cut", false);
        let s = key(&mut sim, SAMPLE_PUSH_KEY, true);
        assert_eq!(
            (s.page.as_str(), s.lit_leds),
            ("base", vec!["Scrl".to_string()])
        );
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn a_latched_key_and_the_pressed_wheel_button_light_in_the_simulator() {
        let (_dir, mut sim, _) = connected(Bus::Usb);
        let mut config = sim.config().clone();
        let base = config.base_layer.clone();
        let layer = config.layers.iter_mut().find(|l| l.id == base).unwrap();
        let ctrl = KeyCombo {
            modifiers: vec![Modifier::Ctrl],
            key: String::new(),
        };
        layer
            .bindings
            .insert("Trans".into(), Action::Latched { combo: ctrl });
        assert!(sim.sync_config(config));
        key(&mut sim, "Trans", true);
        let s = key(&mut sim, "Trans", false);
        assert_eq!(s.lit_leds, ["Trans", "Scrl"]);
        // An unbound wheel button switches to its printed mode.
        key(&mut sim, "Shtl", true);
        let s = key(&mut sim, "Shtl", false);
        assert_eq!(
            (s.jog_mode, s.lit_leds),
            (
                JogMode::Shuttle,
                vec!["Trans".to_string(), "Shtl".to_string()]
            )
        );
        key(&mut sim, "Trans", true);
        assert_eq!(key(&mut sim, "Trans", false).lit_leds, ["Shtl"]);
        // Off: nothing lit.
        assert!(
            sim.apply(Control::SetEnabled { on: false })
                .lit_leds
                .is_empty()
        );
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
            (JogMode::Scroll, "Scrl", "scroll -1 ×1", "scroll 1 ×1"),
            (JogMode::Jog, "Jog", "press Left ×1", "press Right ×1"),
            (JogMode::Shuttle, "Shtl", "press Left ×1", "press Right ×1"),
        ] {
            let s = sim.apply(Control::WheelMode { mode });
            assert_eq!((s.jog_mode, s.lit_leds), (mode, vec![led.to_string()]));
            let m = crate::describe::jog_mode(mode);
            let line = |v: &str, did: &str| {
                format!(
                    "simulated device: wheel turned {v} in {m} mode (1 report) -> {}",
                    off(did)
                )
            };
            assert_eq!(turn(&mut sim, -1), [line("-1", left)]);
            // Shuttle reports the dial position: -1 to 1 is a turn of 2.
            let by = if mode == JogMode::Shuttle { "+2" } else { "+1" };
            assert_eq!(turn(&mut sim, 1), [line(by, right)]);
        }
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn the_volume_and_brightness_wheel_modes_say_what_they_would_change() {
        let dir = tempfile::tempdir().unwrap();
        let (mut sim, mock) = sim_in(dir.path());
        sim.apply(Control::Connect { bus: Bus::Usb });
        for (mode, what) in [
            (JogMode::Volume, "volume"),
            (JogMode::Brightness, "brightness"),
        ] {
            let s = sim.apply(Control::WheelMode { mode });
            assert_eq!(s.jog_mode, mode);
            assert_eq!(
                turn(&mut sim, 2),
                [format!(
                    "simulated device: wheel turned +2 in {what} knob mode (1 report) -> {}",
                    off(&format!("turn {what} up ×2"))
                )]
            );
            sim.apply(Control::Wheel { value: -1 });
            let lines = turn(&mut sim, -2);
            assert_eq!(lines.len(), 1, "{lines:?}");
            assert!(
                lines[0].ends_with(&format!(
                    "(2 reports) -> {}",
                    off(&format!("turn {what} down ×3"))
                )),
                "{lines:?}"
            );
        }
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn send_for_real_off_only_toasts_and_logs_on_reaches_the_injector() {
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

        let mut s = None;
        let log = crate::logging::capture(tracing::Level::INFO, || {
            s = Some(key(&mut sim, "Cut", true));
        });
        assert_eq!(s.unwrap().toasts, [off("press Ctrl+C")]);
        assert!(log.contains(&off("press Ctrl+C")), "{log}");
        assert!(
            log.contains(r#"simulated device: key CUT pressed on page "Base" -> Press Ctrl+C"#),
            "{log}"
        );
        key(&mut sim, "Cut", false);
        assert_eq!(key(&mut sim, "In", true).toasts, [off("hold A")]);
        assert_eq!(key(&mut sim, "In", false).toasts, [off("release A")]);
        assert!(turn(&mut sim, 1)[0].ends_with(&off("scroll 1 ×1")));
        assert!(mock.calls().is_empty());

        assert!(sim.apply(Control::SendForReal { on: true }).send_for_real);
        let s = key(&mut sim, "Cut", true);
        assert_eq!(s.toasts, ["sent Ctrl+C"]);
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
        assert_eq!(key(&mut sim, "In", true).toasts, [off("hold Shift")]);
        assert_eq!(key(&mut sim, "In", false).toasts, [off("release Shift")]);
        assert_eq!(key(&mut sim, "Snap", true).toasts, [off("hold Shift")]);
        assert!(key(&mut sim, "Snap", false).toasts.is_empty());
        assert_eq!(key(&mut sim, "Snap", true).toasts, [off("release Shift")]);
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
    fn new_version_publishes_an_update_offline_every_time() {
        let dir = tempfile::tempdir().unwrap();
        let (mut sim, _) = sim_in(dir.path());
        assert_eq!(sim.apply(Control::Status).update, None);
        // The settings window's simulated update server (src/simUpdate.js)
        // takes it from there; each press publishes again.
        let expected = serde_json::json!({"status": "available", "version": FAKE_UPDATE_VERSION});
        for _ in 0..3 {
            let s = sim.apply(Control::StepUpdate);
            assert_eq!(serde_json::to_value(&s).unwrap()["update"], expected);
        }
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
        let states: [(&str, &str, Check); 6] = [
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

    #[test]
    fn the_device_test_view_sees_the_simulated_reports_both_ways() {
        let (_dir, mut sim, _mock) = connected(Bus::Bluetooth);
        let d = sim.diagnostics();
        assert!(d.simulated);
        assert_eq!(d.status, Some(ConnectionStatus::Connected));
        assert_eq!(d.device.as_ref().and_then(|i| i.bus), Some(Bus::Bluetooth));
        assert_eq!(d.auth.state, crate::diagnostics::AuthState::Passed);
        assert_eq!(d.auth.session_s, Some(600));
        // The connect set the device wheel mode, then its LEDs (newest
        // first): every key dark, SCRL lit.
        let out: Vec<_> = d.reports[..3]
            .iter()
            .map(|r| (r.direction, r.decoded.as_str()))
            .collect();
        let o = crate::diagnostics::Direction::Out;
        assert_eq!(
            out,
            [
                (o, "wheel LEDs 0x04"),
                (o, "key LEDs mask 0x00000"),
                (o, "set wheel mode 0 (relative)")
            ]
        );
        assert_eq!(d.reports[2].hex, "03 00 00 00 00 00 ff");

        key(&mut sim, "Cut", true);
        sim.apply(Control::Wheel { value: -3 });
        sim.apply(Control::Battery {
            level: 42,
            charging: false,
        });
        let d = sim.diagnostics();
        assert_eq!(d.held, ["CUT"]);
        assert_eq!(d.presses, ["CUT"]);
        assert_eq!(d.wheel.total, -3);
        assert_eq!(d.battery.map(|b| b.level), Some(42));
        assert_eq!(d.reports[0].hex, "07 00 2a");
        assert_eq!(d.reports[1].decoded, "wheel mode 0 (relative) value -3");
        assert_eq!(d.reports[2].decoded, "keys held: CUT");

        sim.apply(Control::WheelMode {
            mode: JogMode::Shuttle,
        });
        let d = sim.diagnostics();
        assert_eq!(d.reports[0].decoded, "wheel LEDs 0x02");
        assert_eq!(
            d.reports[1].decoded,
            "set wheel mode 3 (absolute, dead zero)"
        );

        sim.apply(Control::Disconnect);
        let d = sim.diagnostics();
        assert_eq!(d.status, Some(ConnectionStatus::Absent));
        assert_eq!(d.disconnects, 1);
        assert!(d.held.is_empty());

        // Off: everything dark before the handle closes.
        sim.apply(Control::Connect { bus: Bus::Usb });
        sim.apply(Control::SetEnabled { on: false });
        let d = sim.diagnostics();
        assert_eq!(d.reports[0].decoded, "wheel LEDs 0x00");
        assert_eq!(d.reports[1].decoded, "key LEDs mask 0x00000");
    }

    /// A launcher and dial the test keeps a handle to after the simulator takes them.
    #[derive(Clone, Default)]
    struct Recorder(Rc<RefCell<Vec<String>>>);

    impl Launcher for Recorder {
        fn launch(&mut self, path: &str, args: &[String]) -> Result<(), LaunchError> {
            self.0.borrow_mut().push(format!("launch {path} {args:?}"));
            Ok(())
        }
    }

    impl DialOutput for Recorder {
        fn push(&mut self, mode: OverrideMode, ticks: i32) {
            self.0.borrow_mut().push(format!("dial {mode:?} {ticks}"));
        }
        fn activate(&mut self, mode: OverrideMode) {
            self.0.borrow_mut().push(format!("activate {mode:?}"));
        }
        fn release(&mut self) {
            self.0.borrow_mut().push("release".into());
        }
        fn set_steps(&mut self, _: crate::config::OverrideSettings) {}
    }

    fn recorded_sim(dir: &std::path::Path) -> (Simulator, Recorder, SharedMock) {
        let rec = Recorder::default();
        let mock = SharedMock::default();
        let sim = Simulator::new(
            ConfigStore::new(dir),
            Box::new(mock.clone()),
            Box::new(rec.clone()),
        )
        .with_dial(Box::new(rec.clone()));
        (sim, rec, mock)
    }

    #[test]
    fn do_actions_for_real_is_on_by_default_runs_the_real_outputs_and_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let (mut sim, rec, mock) = recorded_sim(dir.path());
        let s = sim.apply(Control::Connect { bus: Bus::Usb });
        assert!(s.send_for_real, "on by default");
        let site = Action::LaunchApp {
            path: "explorer.exe".into(),
            args: vec!["https://example.com".into()],
            enabled: true,
            cwd: None,
            per_os: None,
        };
        sim.apply(Control::SetBinding {
            key: "Cut".into(),
            action: Some(site),
        });
        sim.apply(Control::SetBinding {
            key: "In".into(),
            action: Some(hotkey(&[Modifier::Ctrl], "C")),
        });
        let s = key(&mut sim, "Cut", true);
        assert_eq!(
            s.toasts,
            [r#"launched example.com ok: explorer.exe "https://example.com""#]
        );
        assert_eq!(
            *rec.0.borrow(),
            [r#"launch explorer.exe ["https://example.com"]"#]
        );
        key(&mut sim, "Cut", false);
        assert_eq!(key(&mut sim, "In", true).toasts, ["sent Ctrl+C"]);
        assert_eq!(mock.calls().len(), 4);
        key(&mut sim, "In", false);
        // The wheel as a volume knob turns the real dial controls.
        sim.apply(Control::WheelMode {
            mode: JogMode::Volume,
        });
        let lines = turn(&mut sim, 3);
        assert_eq!(
            lines,
            [
                "simulated device: wheel turned +3 in volume knob mode (1 report) -> turn volume up ×3"
            ]
        );
        assert!(rec.0.borrow().contains(&"dial SystemVolume 3".to_string()));

        // Off: nothing runs, the line says so, and the next run remembers.
        let s = sim.apply(Control::SendForReal { on: false });
        assert!(!s.send_for_real);
        assert!(
            s.toasts[0].starts_with("Do actions for real is off"),
            "{:?}",
            s.toasts
        );
        sim.release_all();
        drop(sim);
        let (mut sim, rec, _) = recorded_sim(dir.path());
        sim.apply(Control::Connect { bus: Bus::Usb });
        assert!(!sim.apply(Control::Status).send_for_real);
        assert_eq!(
            key(&mut sim, "Cut", true).toasts,
            [off(r#"run explorer.exe "https://example.com""#)]
        );
        assert!(rec.0.borrow().is_empty());
        sim.apply(Control::SendForReal { on: true });
        drop(sim);
        assert!(SimState::load(dir.path()).do_actions_for_real);
    }
}
