//! `freewheel --simulate`: one thread owns the simulator session (ADR-04).
//! The Simulator window drives it through the `simulate` and `sim_jump`
//! commands, and in simulator mode it stands in for the device loop: the
//! tray, the settings window's `status`/`key-pressed` events, its On/Off
//! switch and its saved configs all go to and come from the simulated Speed
//! Editor. Nothing here is registered or started without `--simulate`.

use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use freewheel_core::battery::Battery;
use freewheel_core::config::{Config, ConfigStore};
use freewheel_core::connection::ConnectionStatus;
use freewheel_core::diagnostics::DeviceDiagnostics;
use freewheel_core::inject::KeyInjector;
use freewheel_core::permissions::{Grant, Permission, Permissions};
use freewheel_core::report::Key;
use freewheel_core::simulator::{self, Control, SimStatus, Simulator};
use freewheel_core::status::KeyPressed;

use crate::device::{Command, Update};
use crate::launch::ProcessLauncher;

/// One message to the session thread.
pub enum Msg {
    /// A control from the Simulator window, answered with the new status.
    Ui(Control, Sender<SimStatus>),
    /// What the app sends the device loop: the switch, a saved config, quit.
    Device(Command),
}

/// Tauri state: the channel to the simulator thread. The receiving end
/// waits here until [`spawn`] runs the session.
pub struct SimHandle {
    tx: Sender<Msg>,
    rx: Mutex<Option<Receiver<Msg>>>,
    /// The simulated OS login item (Start at login never touches the OS here).
    login: std::sync::atomic::AtomicBool,
}

impl SimHandle {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        SimHandle {
            tx,
            rx: Mutex::new(Some(rx)),
            login: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// The simulated login item.
    pub fn start_at_login(&self) -> bool {
        self.login.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn set_start_at_login(&self, on: bool) {
        self.login.store(on, std::sync::atomic::Ordering::SeqCst);
    }

    #[cfg(test)]
    pub fn apply(&self, control: Control) -> Result<SimStatus, String> {
        apply_on(&self.tx, control, REPLY_TIMEOUT)
    }

    /// The sending end, for a control applied off the calling thread.
    pub fn sender(&self) -> Sender<Msg> {
        self.tx.clone()
    }

    fn take_receiver(&self) -> Option<Receiver<Msg>> {
        self.rx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }
}

/// How long a control waits for the session's answer before it gives up
/// with an error: a stuck session must never hang its caller.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// Sends `control` to the session on `tx` and waits up to `timeout` for the
/// new status.
pub fn apply_on(
    tx: &Sender<Msg>,
    control: Control,
    timeout: Duration,
) -> Result<SimStatus, String> {
    let (reply, answer) = mpsc::channel();
    tx.send(Msg::Ui(control, reply))
        .map_err(|e| e.to_string())?;
    answer.recv_timeout(timeout).map_err(|e| match e {
        mpsc::RecvTimeoutError::Timeout => {
            format!("the simulator did not answer within {timeout:?}")
        }
        mpsc::RecvTimeoutError::Disconnected => e.to_string(),
    })
}

/// What the session tells the rest of the app after each message.
#[derive(Debug, Clone, PartialEq)]
pub enum Mirror {
    /// Device loop updates, for the tray and the `status` event.
    Updates(Vec<Update>),
    /// How the device is attached while connected.
    Bus(Option<simulator::Bus>),
    /// Stepped aside for DaVinci Resolve.
    Paused(bool),
    /// freewheel is On (stepped aside counts as On): the Simulator window
    /// switched the simulated device itself.
    Enabled(bool),
    Permissions(Permissions),
    /// A config the Simulator window changed (sample / empty setup).
    Config(Box<Config>),
    /// What a device command (from the settings window or the tray) did,
    /// for the Simulator window's log right away ([`SIM_LOG_EVENT`]).
    Log(Vec<String>),
    /// The Device test view's data for the simulated Speed Editor changed.
    Diagnostics(Box<DeviceDiagnostics>),
}

/// The device loop updates between two statuses, in the order the real
/// loop would send them.
pub fn updates(before: Option<&SimStatus>, now: &SimStatus) -> Vec<Update> {
    let mut out = Vec::new();
    let connection = |s: &SimStatus| s.connection.unwrap_or(ConnectionStatus::Absent);
    if before.map(connection) != Some(connection(now)) {
        out.push(Update::State(connection(now)));
    }
    let battery = |s: &SimStatus| {
        s.battery.map(|level| Battery {
            level,
            charging: s.charging,
        })
    };
    if let Some(b) = battery(now)
        && before.and_then(battery) != Some(b)
    {
        out.push(Update::Battery(b));
    }
    if before.map(|b| b.page.as_str()) != Some(now.page.as_str()) {
        out.push(Update::Layer(now.page.clone()));
    }
    if before.map(|b| b.wheel_override) != Some(now.wheel_override)
        && (before.is_some() || now.wheel_override.is_some())
    {
        out.push(Update::Override(now.wheel_override));
    }
    if before.map(|b| &b.lit_leds) != Some(&now.lit_leds) {
        out.push(Update::Leds(now.lit_leds.clone()));
    }
    for held in &now.held_keys {
        if before.is_some_and(|b| b.held_keys.contains(held)) {
            continue;
        }
        if let Some(&key) = Key::ALL.iter().find(|k| format!("{k:?}") == *held) {
            out.push(Update::KeyPressed(KeyPressed::from(key)));
        }
    }
    out
}

/// The permissions the settings window sees for the simulator's switch:
/// 'Missing' takes Accessibility (key sending) away, as on a fresh Mac.
pub fn permissions(state: simulator::Permissions) -> Permissions {
    let accessibility = match state {
        simulator::Permissions::Granted => Grant::Granted,
        simulator::Permissions::Missing => Grant::Missing,
    };
    Permissions::from([
        (Permission::Accessibility, accessibility),
        (Permission::InputMonitoring, Grant::Granted),
    ])
}

/// Everything that changed between two statuses, for the [`Mirror`] sink.
pub fn mirror(before: Option<&SimStatus>, now: &SimStatus) -> Vec<Mirror> {
    let mut out = Vec::new();
    let paused = |s: &SimStatus| s.resolve_open && !s.enabled;
    // First, so the On/Off is never shown with the old connection's news.
    let on = |s: &SimStatus| s.enabled || paused(s);
    if before.map(on) != Some(on(now)) {
        out.push(Mirror::Enabled(on(now)));
    }
    let ups = updates(before, now);
    if !ups.is_empty() {
        out.push(Mirror::Updates(ups));
    }
    if before.map(|b| b.bus) != Some(now.bus) {
        out.push(Mirror::Bus(now.bus));
    }
    if before.map(paused) != Some(paused(now)) {
        out.push(Mirror::Paused(paused(now)));
    }
    if before.map(|b| b.permissions) != Some(now.permissions) {
        out.push(Mirror::Permissions(permissions(now.permissions)));
    }
    out
}

/// Runs the session until every sender is gone or [`Command::Quit`].
/// `sink` hears every change; what device commands did goes to it as
/// [`Mirror::Log`] at once.
pub fn session(mut sim: Simulator, rx: Receiver<Msg>, mut sink: impl FnMut(Mirror)) {
    let mut last: Option<SimStatus> = None;
    let mut last_diagnostics: Option<DeviceDiagnostics> = None;
    loop {
        let msg = match rx.recv_timeout(IDLE_TICK) {
            Ok(msg) => msg,
            // No control: a wheel turn that rested gets its line.
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let lines = sim.idle(std::time::Instant::now());
                if !lines.is_empty() {
                    sink(Mirror::Log(lines));
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        let config_before = sim.config().clone();
        let (mut status, reply) = match msg {
            Msg::Ui(control, reply) => (sim.apply(control), Some(reply)),
            Msg::Device(Command::SetEnabled(on)) => (sim.apply(Control::SetEnabled { on }), None),
            Msg::Device(Command::ExitOverride) => (sim.apply(Control::ExitOverride), None),
            Msg::Device(Command::Reload(config)) => {
                sim.sync_config(*config);
                (sim.apply(Control::Status), None)
            }
            Msg::Device(Command::Quit(done)) => {
                // Nothing stays held ('Do actions for real' stays as it is).
                sim.release_all();
                let _ = done.send(());
                return;
            }
        };
        for change in mirror(last.as_ref(), &status) {
            sink(change);
        }
        let diagnostics = sim.diagnostics();
        if last_diagnostics.as_ref() != Some(&diagnostics) {
            last_diagnostics = Some(diagnostics.clone());
            sink(Mirror::Diagnostics(Box::new(diagnostics)));
        }
        let toasts = std::mem::take(&mut status.toasts);
        match reply {
            Some(reply) => {
                if *sim.config() != config_before {
                    sink(Mirror::Config(Box::new(sim.config().clone())));
                }
                let mut answer = status.clone();
                answer.toasts = toasts;
                let _ = reply.send(answer);
            }
            None if !toasts.is_empty() => sink(Mirror::Log(toasts)),
            None => {}
        }
        last = Some(status);
    }
}

/// How often the idle session checks for a rested wheel turn.
const IDLE_TICK: Duration = Duration::from_millis(150);

/// The dial controls 'Do actions for real' turns: the OS volume and
/// brightness through the same OverrideWorker as the device loop.
#[cfg(not(test))]
fn real_dial() -> Box<dyn freewheel_core::overrides::DialOutput> {
    use freewheel_core::clock::SystemClock;
    use freewheel_core::config::OverrideSettings;
    use freewheel_core::overrides::{OverrideRunner, OverrideWorker};
    Box::new(OverrideWorker::spawn(OverrideRunner::new(
        crate::controls::os_controls(),
        OverrideSettings::default(),
        SystemClock,
    )))
}

#[cfg(test)]
fn real_dial() -> Box<dyn freewheel_core::overrides::DialOutput> {
    Box::new(freewheel_core::overrides::NoDial)
}

/// Starts the session thread on `store` (the simulator's own config, shared
/// with the settings window in this mode). `injector` is built there and,
/// with the launcher and the dial controls, is used while 'Do actions for
/// real' is on: the same outputs as the device loop. `sink` runs on that
/// thread.
pub fn spawn(
    handle: &SimHandle,
    store: ConfigStore,
    injector: impl FnOnce() -> Box<dyn KeyInjector> + Send + 'static,
    sink: impl FnMut(Mirror) + Send + 'static,
) {
    let Some(rx) = handle.take_receiver() else {
        tracing::warn!("simulator session already started");
        return;
    };
    std::thread::spawn(move || {
        let sim =
            Simulator::new(store, injector(), Box::new(ProcessLauncher)).with_dial(real_dial());
        session(sim, rx, sink);
    });
}

/// Forwards what the app sends the device loop (tray toggle, settings
/// switch, saved configs, quit) to the session.
pub fn forward_commands(handle: &SimHandle, commands: Receiver<Command>) {
    let tx = handle.tx.clone();
    std::thread::spawn(move || {
        for command in commands {
            if tx.send(Msg::Device(command)).is_err() {
                break;
            }
        }
    });
}

/// Runs the session as the app's device loop (in place of
/// `spawn_device` and the permission watch): the tray, the settings
/// window's events and its saved configs follow the simulated Speed Editor.
#[allow(clippy::too_many_arguments)]
pub fn attach(
    app: tauri::AppHandle,
    store: ConfigStore,
    threshold: u8,
    injector: impl FnOnce() -> Box<dyn KeyInjector> + Send + 'static,
    tray: tauri::tray::TrayIcon,
    commands: Receiver<Command>,
) {
    use freewheel_core::diagnostics::DEVICE_DIAGNOSTICS_EVENT;
    use freewheel_core::status::KEY_PRESSED_EVENT;
    use tauri::{Emitter, Manager};

    let handle = app.state::<SimHandle>();
    forward_commands(&handle, commands);
    let mut view = crate::tray::TrayView::new(threshold);
    let sink = {
        let app = app.clone();
        move |change: Mirror| {
            let settings = app.state::<crate::settings::Settings>();
            let status = match change {
                Mirror::Updates(ups) => {
                    let mut changed = None;
                    for update in ups {
                        if let Update::KeyPressed(key) = &update {
                            let _ = app.emit(KEY_PRESSED_EVENT, key);
                        }
                        changed = settings.apply(&update).or(changed);
                        view.apply(&tray, update);
                    }
                    changed
                }
                Mirror::Bus(bus) => {
                    settings.set_bus(bus);
                    Some(settings.status())
                }
                Mirror::Paused(paused) => settings.set_paused(paused),
                Mirror::Enabled(on) => settings.mirror_enabled(on),
                Mirror::Permissions(p) => settings.set_permissions(&p),
                Mirror::Config(config) => {
                    // Saved through the settings window's own path (the
                    // simulator's store in this mode), then shown.
                    let saved = serde_json::to_value(&*config)
                        .map_err(|e| e.to_string())
                        .and_then(|v| settings.save(&v).map_err(|e| format!("{e:?}")));
                    match saved {
                        Ok(()) => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.eval("location.reload();");
                            }
                        }
                        Err(e) => tracing::warn!(error = %e, "simulator setup not handed over"),
                    }
                    None
                }
                Mirror::Log(lines) => {
                    let _ = app.emit_to(SIMULATOR_WINDOW, SIM_LOG_EVENT, lines);
                    None
                }
                Mirror::Diagnostics(d) => {
                    let _ = app.emit(DEVICE_DIAGNOSTICS_EVENT, &d);
                    settings.apply(&Update::Diagnostics(d))
                }
            };
            if let Some(status) = status {
                crate::tray::emit_status(&app, &status);
            }
        }
    };
    spawn(&handle, store, injector, sink);
}

/// `--simulate` only: the WebView2 DevTools port the Windows e2e attaches
/// msedgedriver to (scripts/e2e-windows.mjs). On hosted windows-2025 runners
/// WebView2 ignores the port msedgedriver passes through the environment
/// (actions/runner-images#14738), so the app opens it itself.
pub const E2E_DEVTOOLS_PORT_ENV: &str = "FREEWHEEL_E2E_DEVTOOLS_PORT";
/// wry's default WebView2 arguments, kept when the port is added.
pub const WRY_DEFAULT_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

/// Every window's WebView2 arguments in this run: `None` (wry's defaults)
/// unless `--simulate` runs with [`E2E_DEVTOOLS_PORT_ENV`] set to a port.
pub fn browser_args(simulate: bool) -> Option<String> {
    let port = std::env::var(E2E_DEVTOOLS_PORT_ENV)
        .ok()
        .or_else(e2e_port_file);
    browser_args_for(simulate, port.as_deref())
}

/// Update e2e builds only (the `e2e` feature): the port from
/// `%LOCALAPPDATA%\freewheel-e2e\devtools-port` when the environment has
/// none. After an update the NSIS installer starts the new version through
/// the user's shell, which does not pass this process's environment on, so
/// the e2e (scripts/e2e-update-windows.mjs) leaves the port in that file too.
#[cfg(feature = "e2e")]
fn e2e_port_file() -> Option<String> {
    let dir = std::env::var_os("LOCALAPPDATA")?;
    std::fs::read_to_string(
        std::path::Path::new(&dir)
            .join("freewheel-e2e")
            .join("devtools-port"),
    )
    .ok()
}

#[cfg(not(feature = "e2e"))]
fn e2e_port_file() -> Option<String> {
    None
}

pub fn browser_args_for(simulate: bool, port: Option<&str>) -> Option<String> {
    let port: u16 = port?.trim().parse().ok().filter(|p| *p != 0)?;
    simulate.then(|| format!("{WRY_DEFAULT_ARGS} --remote-debugging-port={port}"))
}

/// Opens the Simulator window to the right of the settings window.
pub fn open_window<R: tauri::Runtime, M: tauri::Manager<R>>(app: &M) -> tauri::Result<()> {
    const WIDTH: f64 = 320.0;
    let main = app.get_webview_window("main");
    if let Some(settings) = &main {
        settings.set_title(&simulator::title("freewheel settings", true))?;
    }
    let url = tauri::WebviewUrl::App("index.html#simulator".into());
    let mut builder = tauri::WebviewWindowBuilder::new(app, SIMULATOR_WINDOW, url)
        .title(simulator::title(freewheel_core::APP_NAME, true))
        .inner_size(WIDTH, 860.0)
        .min_inner_size(280.0, 480.0);
    // Like the settings window (tauri.conf.json, tauri.macos.conf.json): the
    // app draws its own header; macOS keeps its traffic lights over it.
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .traffic_light_position(tauri::LogicalPosition::new(16.0, 18.0));
    }
    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.decorations(false).shadow(true);
    }
    // Both windows share one WebView2 environment, so the same arguments.
    if let Some(args) = browser_args(true) {
        builder = builder.additional_browser_args(&args);
    }
    if let Some(main) = &main
        && let (Ok(pos), Ok(size), Ok(scale)) = (
            main.outer_position(),
            main.outer_size(),
            main.scale_factor(),
        )
    {
        let (x, y) = window_spot(
            (pos.x as f64 / scale, pos.y as f64 / scale),
            size.width as f64 / scale,
        );
        builder = builder.position(x, y);
    }
    builder.build()?;
    Ok(())
}

/// Where the Simulator window goes: just right of the settings window.
pub fn window_spot(main_pos: (f64, f64), main_width: f64) -> (f64, f64) {
    (main_pos.0 + main_width + 8.0, main_pos.1.max(0.0))
}

/// Async, so it never runs on the main thread: the session's sink updates
/// the tray, which waits for the main thread, so a sync command waiting
/// here for the session's answer deadlocked the whole app (both windows
/// froze right after the Simulator window's first `Status`). The wait
/// itself runs on a blocking worker.
#[tauri::command]
pub async fn simulate(
    control: Control,
    sim: tauri::State<'_, SimHandle>,
    app: tauri::AppHandle,
) -> Result<SimStatus, String> {
    let steps_update = control == Control::StepUpdate;
    let tx = sim.sender();
    let status =
        tauri::async_runtime::spawn_blocking(move || apply_on(&tx, control, REPLY_TIMEOUT))
            .await
            .map_err(|e| e.to_string())??;
    if steps_update {
        // "New version": the simulated update source publishes one, then
        // the settings window finds it (src/selfupdate.rs, src/App.jsx).
        use tauri::Manager;
        if let Some(published) = app
            .try_state::<crate::selfupdate::Updater>()
            .and_then(|u| u.sim_publish())
        {
            tracing::info!(%published, "simulate: new version published");
        }
        // A closed settings window finds it by itself when it opens.
        if app
            .get_webview_window(crate::app_windows::SETTINGS_WINDOW)
            .is_some()
        {
            crate::app_windows::emit_to_settings(&app, SIM_UPDATE_EVENT, &status.update);
        }
    }
    Ok(status)
}

/// The Simulator's "New version's notes": `choice` is `Next`, `Sample`,
/// `Paste` (with `text`) or `State` (only says what they are now). The next
/// "New version" carries them, verbatim, through check, download, restart
/// and What's new.
#[tauri::command]
pub fn sim_notes(
    choice: String,
    text: Option<String>,
    updater: tauri::State<'_, crate::selfupdate::Updater>,
) -> Result<crate::selfupdate::SimNotes, String> {
    use crate::selfupdate::NotesChoice;
    let choice = match choice.as_str() {
        "State" => None,
        "Next" => Some(NotesChoice::Next),
        "Sample" => Some(NotesChoice::Sample),
        "Paste" => Some(NotesChoice::Paste),
        other => return Err(format!("no notes choice {other}")),
    };
    let notes = updater.sim_notes(choice, text)?;
    if choice.is_some() {
        tracing::info!(choice = ?notes.choice, bytes = notes.notes.len(), "simulator: update notes");
    }
    Ok(notes)
}

/// A screen of the app the Simulator window's "Jump to" opens, named as
/// the window sends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    /// The settings window, shown and focused.
    Settings,
    /// The first-run flow: its done flag cleared, the window reloaded.
    FirstRun,
    /// Keys tab, CUT selected, its key panel recording a shortcut (the
    /// settings window hears [`SIM_RECORD_KEY_EVENT`]).
    RecordKey,
    /// The OS colour scheme the settings window follows on "System".
    Light,
    Dark,
    /// Reloads the settings window, e.g. after the Keys setup changed.
    Reload,
    /// Quits the app, as the tray's Quit does.
    Quit,
}

impl std::str::FromStr for Jump {
    type Err = String;
    fn from_str(name: &str) -> Result<Self, String> {
        Ok(match name {
            "Settings" => Jump::Settings,
            "FirstRun" => Jump::FirstRun,
            "RecordKey" => Jump::RecordKey,
            "Light" => Jump::Light,
            "Dark" => Jump::Dark,
            "Reload" => Jump::Reload,
            "Quit" => Jump::Quit,
            other => return Err(format!("no screen {other}")),
        })
    }
}

/// Simulate-only event to the settings window: open the key panel of the
/// named key, recording a shortcut (src/simulateEvents.js).
pub const SIM_RECORD_KEY_EVENT: &str = "sim-record-key";
/// Simulate-only event to the settings window: the update banner's state
/// (`null` hides it), as the Simulator's "New version" steps it.
pub const SIM_UPDATE_EVENT: &str = "sim-update";
/// Simulate-only event to the Simulator window: lines for its log, from
/// what the settings window, the tray or a device command did
/// (src/Simulator.jsx).
pub const SIM_LOG_EVENT: &str = "sim-log";
/// The Simulator window's label.
pub const SIMULATOR_WINDOW: &str = "simulator";

/// In `--simulate` only: puts `line` in the Simulator window's log, so
/// what the app did (or would do to the real OS) is visible. Outside
/// --simulate it does nothing.
pub fn note<R: tauri::Runtime>(app: &tauri::AppHandle<R>, line: impl Into<String>) {
    use tauri::Emitter;
    if simulating(app) {
        let line = line.into();
        tracing::info!(%line, "simulator log");
        let _ = app.emit_to(SIMULATOR_WINDOW, SIM_LOG_EVENT, vec![line]);
    }
}

/// How long the main thread may not answer before the log says it is
/// blocked ([`watch_main_thread`]).
pub const MAIN_THREAD_STALL: Duration = Duration::from_secs(2);

/// `--simulate`: a freeze shows in the log. A thread keeps asking the main
/// thread for a round trip, one after another; when an answer takes over
/// [`MAIN_THREAD_STALL`], it logs that the main thread is blocked (and,
/// once it answers, for how long), so a stuck UI step in the e2e can be told
/// from a deadlock by the log's timeline.
pub fn watch_main_thread<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    std::thread::spawn(move || {
        loop {
            let (tx, rx) = mpsc::channel();
            let asked = std::time::Instant::now();
            if app
                .run_on_main_thread(move || {
                    let _ = tx.send(());
                })
                .is_err()
            {
                return;
            }
            match rx.recv_timeout(MAIN_THREAD_STALL) {
                Ok(()) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    tracing::warn!(
                        ms = MAIN_THREAD_STALL.as_millis() as u64,
                        "main thread blocked: no answer"
                    );
                    if rx.recv().is_err() {
                        return;
                    }
                    tracing::warn!(
                        ms = asked.elapsed().as_millis() as u64,
                        "main thread answers again"
                    );
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
            // The watchdog's own pace (one round trip a second), not a wait
            // in any flow.
            std::thread::sleep(Duration::from_secs(1).saturating_sub(asked.elapsed()));
        }
    });
}

/// Whether this run is `--simulate` (the [`SimHandle`] is managed only then).
pub fn simulating<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    use tauri::Manager;
    app.try_state::<SimHandle>().is_some()
}

/// The key "Record a key" opens.
pub const RECORD_KEY: &str = "Cut";
/// What the settings window runs on each page load in --simulate: the flag
/// its simulate-only listeners check (src/simulateEvents.js).
pub const SIMULATE_FLAG_SCRIPT: &str = "window.__FREEWHEEL_SIMULATE__ = true;";

/// The webview's localStorage key of the first-run flag (src/shell.js).
pub const FIRST_RUN_DONE: &str = "freewheel.first-run-done";

/// The script a jump runs in the settings window, if any.
pub fn jump_script(jump: Jump) -> Option<String> {
    match jump {
        Jump::Settings | Jump::Quit | Jump::RecordKey => None,
        Jump::Reload => Some("location.reload();".into()),
        Jump::FirstRun => Some(format!(
            "localStorage.removeItem({FIRST_RUN_DONE:?}); location.reload();"
        )),
        Jump::Light | Jump::Dark => Some(format!(
            "document.documentElement.dataset.theme = {:?};",
            if jump == Jump::Dark { "dark" } else { "light" }
        )),
    }
}

#[tauri::command]
pub fn sim_jump(to: String, app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let to: Jump = to.parse()?;
    tracing::info!(?to, "simulator: jump");
    if to == Jump::Quit {
        app.exit(0);
        return Ok(());
    }
    let script = jump_script(to);
    if matches!(to, Jump::Settings | Jump::FirstRun | Jump::RecordKey) {
        // Built again first when it was closed (app_windows.rs); what the
        // jump sends waits until the new page listens.
        crate::app_windows::show_settings(&app);
    } else if app.get_webview_window("main").is_none() {
        return Err("the settings window is closed".into());
    }
    if let Some(script) = script {
        crate::app_windows::eval_in_settings(&app, script);
    }
    if to == Jump::RecordKey {
        crate::app_windows::emit_to_settings(&app, SIM_RECORD_KEY_EVENT, RECORD_KEY);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use freewheel_core::config::Action;
    use freewheel_core::inject::{KeyCombo, MockInjector};
    use freewheel_core::simulator::Bus;
    use std::sync::Arc;
    use std::time::Duration;

    type Seen = Arc<Mutex<Vec<Mirror>>>;

    fn running() -> (tempfile::TempDir, SimHandle, Sender<Command>, Seen) {
        let dir = tempfile::tempdir().unwrap();
        let handle = SimHandle::new();
        let seen: Seen = Arc::default();
        let sink = {
            let seen = Arc::clone(&seen);
            move |m| seen.lock().unwrap().push(m)
        };
        spawn(
            &handle,
            ConfigStore::new(dir.path()),
            || Box::new(MockInjector::new()),
            sink,
        );
        let (commands, rx) = mpsc::channel();
        forward_commands(&handle, rx);
        (dir, handle, commands, seen)
    }

    fn take(seen: &Seen) -> Vec<Mirror> {
        std::mem::take(&mut *seen.lock().unwrap())
    }

    fn all_updates(m: &[Mirror]) -> Vec<Update> {
        m.iter()
            .filter_map(|m| match m {
                Mirror::Updates(u) => Some(u.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn key(sim: &SimHandle, name: &str, down: bool) {
        sim.apply(Control::Key {
            key: name.into(),
            down,
        })
        .unwrap();
    }

    #[test]
    fn session_thread_answers_each_control() {
        let (_dir, sim, _commands, _) = running();
        let s = sim.apply(Control::Status).unwrap();
        assert_eq!(s.connection, Some(ConnectionStatus::Absent));
        let s = sim.apply(Control::Connect { bus: Bus::Usb }).unwrap();
        assert_eq!(s.connection, Some(ConnectionStatus::Connected));
        assert_eq!(s.bus, Some(Bus::Usb));
    }

    #[test]
    fn the_tray_and_settings_window_follow_the_simulated_device() {
        let (_dir, sim, _commands, seen) = running();
        sim.apply(Control::Status).unwrap();
        let first = take(&seen);
        assert!(all_updates(&first).contains(&Update::State(ConnectionStatus::Absent)));
        assert!(first.contains(&Mirror::Paused(false)));

        sim.apply(Control::Connect {
            bus: Bus::Bluetooth,
        })
        .unwrap();
        sim.apply(Control::Battery {
            level: 12,
            charging: false,
        })
        .unwrap();
        let now = take(&seen);
        let ups = all_updates(&now);
        assert!(ups.contains(&Update::State(ConnectionStatus::Connected)));
        assert!(ups.contains(&Update::Battery(Battery {
            level: 12,
            charging: false
        })));
        assert!(now.contains(&Mirror::Bus(Some(Bus::Bluetooth))));
        // The lit wheel button reaches the settings window's drawing.
        assert!(ups.contains(&Update::Leds(vec!["Scrl".into()])));

        key(&sim, "Cut", true);
        assert_eq!(
            all_updates(&take(&seen)),
            [Update::KeyPressed(KeyPressed::from(Key::Cut))]
        );
        // Releasing is not a press.
        key(&sim, "Cut", false);
        assert!(all_updates(&take(&seen)).is_empty());
    }

    #[test]
    fn the_settings_switch_and_saves_drive_the_session() {
        let (_dir, sim, commands, seen) = running();
        sim.apply(Control::Connect { bus: Bus::Usb }).unwrap();
        take(&seen);
        commands.send(Command::SetEnabled(false)).unwrap();
        // The forwarder runs on its own thread; give it the message first.
        std::thread::sleep(Duration::from_millis(50));
        let s = sim.apply(Control::Status).unwrap();
        assert!(!s.enabled);
        assert!(all_updates(&take(&seen)).contains(&Update::State(ConnectionStatus::Off)));

        // A config saved by the settings window runs in the session and is
        // not echoed back as a window change.
        let mut config = Config::default();
        let base = config.base_layer.clone();
        let layer = config.layers.iter_mut().find(|l| l.id == base).unwrap();
        let save = Action::Hotkey {
            combo: KeyCombo {
                modifiers: vec![],
                key: "S".into(),
            },
        };
        layer.bindings.insert("Cut".into(), save);
        commands.send(Command::Reload(Box::new(config))).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let s = sim.apply(Control::Status).unwrap();
        assert_eq!(s.mapped_keys, 1);
        assert!(!take(&seen).iter().any(|m| matches!(m, Mirror::Config(_))));
    }

    #[test]
    fn window_setups_are_handed_to_the_settings_window() {
        let (_dir, sim, _commands, seen) = running();
        sim.apply(Control::LoadSampleMappings).unwrap();
        let config = take(&seen)
            .into_iter()
            .find_map(|m| match m {
                Mirror::Config(c) => Some(c),
                _ => None,
            })
            .expect("sample setup mirrored");
        assert!(config.layers.iter().any(|l| !l.bindings.is_empty()));
        sim.apply(Control::ClearMappings).unwrap();
        assert!(take(&seen).iter().any(
            |m| matches!(m, Mirror::Config(c) if c.layers.iter().all(|l| l.bindings.is_empty()))
        ));
    }

    #[test]
    fn resolve_and_permissions_reach_the_status() {
        let (_dir, sim, _commands, seen) = running();
        sim.apply(Control::Connect { bus: Bus::Usb }).unwrap();
        take(&seen);
        sim.apply(Control::Resolve { open: true }).unwrap();
        assert!(take(&seen).contains(&Mirror::Paused(true)));
        sim.apply(Control::Permissions {
            state: simulator::Permissions::Missing,
        })
        .unwrap();
        assert!(take(&seen).contains(&Mirror::Permissions(permissions(
            simulator::Permissions::Missing
        ))));
    }

    #[test]
    fn device_command_toasts_reach_the_window_log_at_once() {
        let (_dir, sim, commands, seen) = running();
        sim.apply(Control::Connect { bus: Bus::Usb }).unwrap();
        let mut config = Config::default();
        let base = config.base_layer.clone();
        let layer = config.layers.iter_mut().find(|l| l.id == base).unwrap();
        layer.bindings.insert(
            "Cam1".into(),
            Action::OverrideToggle {
                mode: freewheel_core::config::OverrideMode::SystemVolume,
            },
        );
        commands.send(Command::Reload(Box::new(config))).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        key(&sim, "Cam1", true);
        key(&sim, "Cam1", false);
        let s = sim.apply(Control::Status).unwrap();
        assert_eq!(
            s.wheel_override,
            Some(freewheel_core::config::OverrideMode::SystemVolume)
        );
        take(&seen);
        // The Wheel tab's 'Back to normal' is a device command: its line
        // goes to the log right away, and the override update follows.
        commands.send(Command::ExitOverride).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let now = take(&seen);
        assert!(
            now.contains(&Mirror::Log(vec!["the wheel is back to normal".into()])),
            "{now:?}"
        );
        assert!(all_updates(&now).contains(&Update::Override(None)));
        assert_eq!(sim.apply(Control::Status).unwrap().wheel_override, None);
    }

    #[test]
    fn quit_releases_and_stops_the_session() {
        let (_dir, sim, commands, _) = running();
        sim.apply(Control::Status).unwrap();
        let (done, released) = mpsc::channel();
        commands.send(Command::Quit(done)).unwrap();
        released.recv_timeout(Duration::from_secs(2)).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert!(sim.apply(Control::Status).is_err());
    }

    /// The freeze: the sink waits for the thread that sent the control
    /// (the tray waits for the main thread). A stuck session answers with
    /// an error in time instead of hanging the caller.
    #[test]
    fn a_stuck_session_never_hangs_the_caller() {
        let dir = tempfile::tempdir().unwrap();
        let handle = SimHandle::new();
        let (_never, blocked) = mpsc::channel::<()>();
        let blocked = Mutex::new(blocked);
        spawn(
            &handle,
            ConfigStore::new(dir.path()),
            || Box::new(MockInjector::new()),
            move |_| {
                let _ = blocked.lock().unwrap().recv();
            },
        );
        let started = std::time::Instant::now();
        let err = apply_on(
            &handle.sender(),
            Control::Status,
            Duration::from_millis(300),
        )
        .unwrap_err();
        assert!(err.contains("did not answer"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    /// Tauri runs sync commands on the main thread; every command that
    /// waits for the session must be async (see `simulate`).
    #[test]
    fn commands_that_wait_for_the_session_are_async() {
        let src = include_str!("simulate.rs");
        let src = &src[..src.find("#[cfg(test)]\nmod tests").unwrap()];
        for command in src.split("#[tauri::command]").skip(1) {
            let body = &command[..command.find("\n}\n").unwrap_or(command.len())];
            let waits =
                body.contains("apply(") || body.contains("apply_on(") || body.contains(".recv");
            assert!(
                !waits || body.trim_start().starts_with("pub async fn"),
                "a sync command waits for the session: {}",
                body.lines().find(|l| l.contains("fn ")).unwrap_or_default()
            );
        }
        assert!(src.contains("pub async fn simulate("));
    }

    #[test]
    fn jumps_parse_and_script_the_settings_window() {
        let j: Jump = "FirstRun".parse().unwrap();
        assert_eq!(j, Jump::FirstRun);
        assert!("Nowhere".parse::<Jump>().is_err());
        assert!(
            jump_script(j)
                .unwrap()
                .contains(r#"localStorage.removeItem("freewheel.first-run-done")"#)
        );
        assert_eq!(
            jump_script("Dark".parse().unwrap()).unwrap(),
            r#"document.documentElement.dataset.theme = "dark";"#
        );
        assert!(jump_script(Jump::Settings).is_none());
        assert!(jump_script(Jump::Quit).is_none());
        assert_eq!(jump_script(Jump::Reload).unwrap(), "location.reload();");
        assert!(jump_script(Jump::RecordKey).is_none());
    }

    #[test]
    fn the_e2e_devtools_port_opens_only_in_simulate_mode() {
        assert_eq!(
            browser_args_for(true, Some("9222")).as_deref(),
            Some(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port=9222"
            )
        );
        assert_eq!(browser_args_for(false, Some("9222")), None);
        assert_eq!(browser_args_for(true, None), None);
        assert_eq!(browser_args_for(true, Some("0")), None);
        assert_eq!(browser_args_for(true, Some("nope")), None);
    }

    #[test]
    fn the_e2e_port_matches_the_script() {
        let js = include_str!("../../scripts/e2e-windows.mjs");
        assert!(js.contains(E2E_DEVTOOLS_PORT_ENV));
    }

    #[test]
    fn the_simulator_window_sits_right_of_the_settings_window() {
        assert_eq!(window_spot((100.0, 50.0), 1000.0), (1108.0, 50.0));
        assert_eq!(window_spot((0.0, -20.0), 800.0), (808.0, 0.0));
    }

    #[test]
    fn the_simulate_events_match_the_frontend() {
        let js = include_str!("../../src/simulateEvents.js");
        for name in [
            SIM_RECORD_KEY_EVENT,
            SIM_UPDATE_EVENT,
            SIM_LOG_EVENT,
            "__FREEWHEEL_SIMULATE__",
        ] {
            assert!(js.contains(name), "{name}");
        }
        assert!(SIMULATE_FLAG_SCRIPT.contains("__FREEWHEEL_SIMULATE__"));
    }

    #[test]
    fn the_first_run_key_matches_the_frontend() {
        assert!(include_str!("../../src/shell.js").contains(&format!("{FIRST_RUN_DONE:?}")));
    }
}
