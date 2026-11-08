#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_icons;
#[cfg(windows)]
mod app_volume;
mod app_windows;
mod apps;
mod autostart;
#[cfg(windows)]
mod brightness;
#[cfg(any(target_os = "macos", all(unix, test)))]
mod builtin_brightness;
mod controls;
#[cfg(target_os = "macos")]
mod ddc_macos;
mod device;
mod e2e;
mod front_app;
mod hidapi_transport;
mod inject;
mod key_names;
mod launch;
#[cfg(target_os = "linux")]
mod linux_audio;
#[cfg(target_os = "linux")]
mod linux_brightness;
#[cfg(target_os = "linux")]
mod linux_desktop;
#[cfg(target_os = "linux")]
mod linux_idle;
#[cfg(target_os = "linux")]
mod linux_inject;
#[cfg(target_os = "linux")]
mod linux_udev;
#[cfg(target_os = "macos")]
mod mac_app_volume;
#[cfg(target_os = "macos")]
#[allow(dead_code)] // wired into the override worker by a later card
mod macos_volume;
mod notify;
mod permissions;
mod release_notes;
mod resolve_watch;
mod selfupdate;
mod settings;
mod simulate;
mod smoke;
mod sysinfo_lister;
mod tray;
mod updates;
#[cfg(any(windows, test))]
#[allow(dead_code)] // wired up with the override worker
mod volume;

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use freewheel_core::clock::SystemClock;
use freewheel_core::config::{Config, ConfigStore};
use freewheel_core::diagnostics::DEVICE_DIAGNOSTICS_EVENT;
use freewheel_core::inject::{KeyInjector, MockInjector};
use freewheel_core::logging::Logger;
use freewheel_core::resolve::{POLL_INTERVAL, ResolveWatcher};
use freewheel_core::simulator;
use freewheel_core::status::KEY_PRESSED_EVENT;
use tauri::{Emitter, Manager};

/// How long the quit hook waits for the device loop to release keys.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// Opens the log in the OS per-user data dir (`<data_local_dir>/logs`).
/// Logging is best effort: the app still runs when the dir is unusable.
fn init_logging() -> Option<Logger> {
    let dirs = directories::ProjectDirs::from("", "", freewheel_core::APP_NAME)?;
    let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
    let logger = Logger::init(
        &dirs.data_local_dir().join("logs"),
        home.as_deref(),
        Arc::new(SystemClock),
    )
    .ok()?;
    let _ = logger.install_global();
    logger.install_panic_hook();
    logger.log_startup(env!("CARGO_PKG_VERSION"), &os_info::get().to_string());
    Some(logger)
}

/// The Speed Editor's auth response (freewheel-core's port of bmd.py's
/// `bmd_kbd_auth`, checked against recorded vectors).
fn auth_response(challenge: u64) -> u64 {
    freewheel_core::auth::compute_response(challenge)
}

/// The response function the device loop authenticates with.
const AUTH_RESPONSE: fn(u64) -> u64 = auth_response;

/// The device loop's connection manager: the real auth response and the
/// idle re-auth that brings a slept Bluetooth device back.
fn device_manager<T, O>(
    open: O,
) -> freewheel_core::connection::ConnectionManager<T, O, SystemClock, fn(u64) -> u64>
where
    T: freewheel_core::hid::HidTransport,
    O: FnMut() -> Result<T, freewheel_core::hid::HidError>,
{
    freewheel_core::connection::ConnectionManager::new(open, SystemClock, AUTH_RESPONSE)
        .with_idle_reauth(freewheel_core::connection::IDLE_REAUTH)
}

/// Starts the device loop on its own thread; the tray shows its state and
/// the settings window hears the `status` and `key-pressed` events.
fn spawn_device(
    app: tauri::AppHandle<tauri::Wry>,
    config: Config,
    tray: tauri::tray::TrayIcon<tauri::Wry>,
    commands: mpsc::Receiver<device::Command>,
) {
    use freewheel_core::connection::ConnectionStatus;
    use freewheel_core::engine::Engine;
    use freewheel_core::overrides::{OverrideRunner, OverrideWorker};

    std::thread::spawn(move || {
        let (mut injector, mut api) = match (inject::os_injector(), hidapi::HidApi::new()) {
            (Ok(injector), Ok(api)) => (injector, api),
            (Err(e), _) => return tracing::error!(error = %e, "no key injection"),
            (_, Err(e)) => return tracing::error!(error = %e, "no HID access"),
        };
        // The bus of the device opened last, for the `status` event.
        let bus = Arc::new(std::sync::Mutex::new(None));
        let open = {
            let bus = Arc::clone(&bus);
            move || {
                let device = hidapi_transport::HidapiTransport::open(&mut api)?;
                *bus.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = device.bus();
                Ok(device)
            }
        };
        let manager = device_manager(open);
        let overrides = OverrideWorker::spawn(OverrideRunner::new(
            controls::os_controls(),
            config.overrides.clone(),
            SystemClock,
        ));
        device::run(
            manager,
            Engine::new(&config),
            &SystemClock,
            device::Outputs {
                injector: &mut *injector,
                launcher: &mut launch::ProcessLauncher,
                overrides: &overrides,
            },
            commands,
            {
                let mut view = tray::TrayView::new();
                move |update| {
                    if let device::Update::KeyPressed(key) = &update {
                        let _ = app.emit(KEY_PRESSED_EVENT, key);
                    }
                    if let device::Update::Diagnostics(d) = &update {
                        let _ = app.emit(DEVICE_DIAGNOSTICS_EVENT, d);
                    }
                    let settings = app.state::<settings::Settings>();
                    if update == device::Update::State(ConnectionStatus::Connected) {
                        settings.set_bus(
                            *bus.lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner),
                        );
                    }
                    if let Some(status) = settings.apply(&update) {
                        tray::emit_status(&app, &status);
                    }
                    view.apply(&tray, update)
                }
            },
        );
    });
}

/// How often the tray re-checks the permissions.
const PERMISSION_POLL: Duration = Duration::from_secs(2);

/// Keeps the status event's permissions (and so the tray's 'Needs
/// permission' line) current, checking every [`PERMISSION_POLL`] (macOS's
/// privacy grants, Linux's device rule; Windows needs nothing).
fn spawn_permission_watch(app: tauri::AppHandle<tauri::Wry>) {
    if cfg!(windows) {
        return;
    }
    std::thread::spawn(move || {
        loop {
            let current = permissions::current();
            if let Some(event) = app.state::<settings::Settings>().set_permissions(&current) {
                tray::emit_status(&app, &event);
            }
            std::thread::sleep(PERMISSION_POLL);
        }
    });
}

/// Starts the Resolve hand-off on its own thread: it polls for the
/// configured process and switches the device loop Off or On; the tray
/// notifies when freewheel stepped aside (notify.rs).
fn spawn_resolve_watch(
    app: tauri::AppHandle<tauri::Wry>,
    config: &Config,
    manual: mpsc::Receiver<resolve_watch::Message>,
    commands: mpsc::Sender<device::Command>,
) {
    let watcher = ResolveWatcher::from_settings(&config.settings);
    std::thread::spawn(move || {
        let pause = |pause: resolve_watch::Pause<'_>| {
            let paused = pause != resolve_watch::Pause::Ended;
            if let Some(status) = app.state::<settings::Settings>().set_paused(paused) {
                tracing::info!(paused, "Resolve hand-off");
                tray::emit_status(&app, &status);
            }
        };
        resolve_watch::run(
            watcher,
            &mut sysinfo_lister::SysinfoLister::new(),
            true,
            POLL_INTERVAL,
            manual,
            commands,
            pause,
        );
    });
}

/// The injector 'Do actions for real' uses in simulator mode.
#[cfg(not(test))]
fn real_injector() -> Box<dyn KeyInjector> {
    match inject::os_injector() {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!(error = %e, "no key injector: Do actions for real sends no keys");
            Box::new(MockInjector::new())
        }
    }
}

#[cfg(test)]
fn real_injector() -> Box<dyn KeyInjector> {
    Box::new(MockInjector::new())
}

/// `--simulate`: the simulator session stands in for the device loop; it
/// starts in `setup` ([`simulate::attach`]).
fn with_simulator(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
        .manage(simulate::SimHandle::new())
        .invoke_handler(tauri::generate_handler![
            simulate::simulate,
            simulate::sim_jump,
            simulate::sim_notes,
            tray::sim_tray,
            tray::tray_update_state,
            app_windows::settings_ready,
            app_icons::app_icons,
            settings::show_logs_file,
            settings::exit_wheel_override,
            apps::list_apps,
            apps::choose_app,
            front_app::profile_app,
            front_app::front_app_support,
            settings::get_config,
            settings::save_config,
            settings::get_status,
            settings::get_device_diagnostics,
            settings::set_enabled,
            settings::get_start_at_login,
            settings::set_start_at_login,
            settings::app_version,
            settings::export_mappings,
            settings::import_mappings,
            permissions::get_permissions,
            permissions::open_permission_pane,
            permissions::open_bluetooth_settings,
            settings::export_logs,
            settings::set_detailed_logs,
            settings::desktop_hints,
            updates::open_release_page,
            release_notes::release_notes,
            selfupdate::update_check,
            selfupdate::update_download,
            selfupdate::update_install
        ])
}

fn main() {
    let logger = init_logging();
    if permissions::probe_requested(std::env::args()) {
        let report = permissions::probe_report(&permissions::current());
        tracing::info!(%report, "permission probe");
        print!("{report}");
        return;
    }
    let mut context = tauri::generate_context!();
    debug_assert_eq!(
        context.config().product_name.as_deref(),
        Some(freewheel_core::APP_NAME)
    );
    let version = context.package_info().version.to_string();
    if selfupdate::headless_requested(std::env::args()) {
        std::process::exit(selfupdate::run_headless(&version));
    }
    // Started by an update: the old version exits first (single instance).
    selfupdate::wait_after_update(std::env::args(), Duration::from_secs(15));
    let (commands, receiver) = mpsc::channel();
    let quit_commands = commands.clone();
    let simulate = simulator::requested(std::env::args());
    if let Some(args) = simulate::browser_args(simulate || cfg!(feature = "e2e")) {
        // The Windows e2e (scripts/e2e.mjs, and with the e2e-only
        // `e2e` feature scripts/e2e-update-windows.mjs) attaches to this port.
        for window in &mut context.config_mut().app.windows {
            window.additional_browser_args = Some(args.clone());
        }
    }
    let smoke = smoke::requested(std::env::args());
    // Registered first, so a second launch only focuses this one. `--simulate` always runs its own instance:
    // handing off would show an already-running (often older, installed) freewheel instead of this build.
    let mut builder = tauri::Builder::default();
    if !simulate {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tray::show_settings(app);
        }));
    }
    let mut builder = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init());
    // `--simulate` never updates for real: the same update state machine
    // runs with the simulated source (src/selfupdate.rs).
    let updater = !simulate && selfupdate::configured();
    builder = builder.manage(release_notes::NotesCache::default());
    builder = builder.manage(if simulate {
        selfupdate::Updater::simulated(&version)
    } else {
        selfupdate::Updater::release(&version)
    });
    builder = if simulate {
        with_simulator(builder)
    } else {
        builder.invoke_handler(tauri::generate_handler![
            settings::get_config,
            settings::save_config,
            settings::get_status,
            settings::get_device_diagnostics,
            settings::set_enabled,
            settings::get_start_at_login,
            settings::set_start_at_login,
            settings::app_version,
            settings::export_mappings,
            settings::import_mappings,
            permissions::get_permissions,
            permissions::open_permission_pane,
            permissions::open_bluetooth_settings,
            settings::export_logs,
            settings::set_detailed_logs,
            settings::desktop_hints,
            settings::show_logs_file,
            settings::exit_wheel_override,
            apps::list_apps,
            apps::choose_app,
            front_app::profile_app,
            front_app::front_app_support,
            app_icons::app_icons,
            tray::tray_update_state,
            app_windows::settings_ready,
            updates::open_release_page,
            release_notes::release_notes,
            selfupdate::update_check,
            selfupdate::update_download,
            selfupdate::update_install
        ])
    };
    let app = builder
        .setup(move |app| {
            let mut store = ConfigStore::from_project_dirs().unwrap_or_else(|| {
                ConfigStore::new(std::env::temp_dir().join(freewheel_core::APP_NAME))
            });
            if simulate {
                // The settings window edits the simulator's config, never the real one.
                store = simulator::simulator_store(&store);
            }
            let sim_store = ConfigStore::new(store.dir());
            let (config, error) = store.load_or_default();
            if let Some(e) = error {
                tracing::warn!(error = %e, "config not loaded, running with defaults");
            }
            let (manual, manual_rx) = mpsc::channel();
            app.manage(app_windows::Lifecycle::default());
            app.manage(
                settings::Settings::new(store, config.clone(), commands.clone(), manual)
                    .with_logger(logger.clone()),
            );
            // The app in front picks its profile (sent like any other
            // device command, so --simulate's session hears it too).
            front_app::watch(commands.clone());
            spawn_resolve_watch(app.handle().clone(), &config, manual_rx, commands);
            // What the last update left behind: the old version, the download.
            if !simulate {
                std::thread::spawn(selfupdate::cleanup_current);
                // A login item an older freewheel wrote unquoted (#197) is
                // written again with this freewheel's quoted path.
                let handle = app.handle().clone();
                std::thread::spawn(move || autostart::refresh(&handle));
            }
            // The tray's update item works in a release build (it has the
            // updater pubkey), or in --simulate with the simulated source; else it says this
            // build can't update itself, as the General tab does.
            let tray = tray::build(app.handle(), updater || simulate)?;
            if simulate {
                simulate::attach(
                    app.handle().clone(),
                    sim_store,
                    real_injector,
                    tray,
                    receiver,
                );
                simulate::open_window(app.handle())?;
                simulate::watch_main_thread(app.handle().clone());
            } else {
                spawn_device(app.handle().clone(), config, tray, receiver);
                spawn_permission_watch(app.handle().clone());
            }
            if smoke {
                smoke::arm_timeout(app.handle().clone());
            }
            Ok(())
        })
        .on_page_load(move |webview, payload| {
            // A new or reloading settings page has no listeners yet: what is
            // sent to it waits for its settings_ready (app_windows.rs).
            if webview.label() == app_windows::SETTINGS_WINDOW
                && payload.event() == tauri::webview::PageLoadEvent::Started
            {
                app_windows::not_ready(webview.app_handle());
            }
            if simulate
                && webview.label() == "main"
                && payload.event() == tauri::webview::PageLoadEvent::Finished
            {
                let _ = webview.eval(simulate::SIMULATE_FLAG_SCRIPT);
            }
            if smoke
                && webview.label() == "main"
                && payload.event() == tauri::webview::PageLoadEvent::Finished
            {
                webview.app_handle().exit(smoke::exit_code(true));
            }
        })
        // Closing a window destroys its webview (app_windows.rs); freewheel
        // stays in the tray, only Quit exits.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                app_windows::close_requested(window, api);
            }
            // Linux: with no window left, WebKit's network process and the
            // closed webview's heap go too (linux_idle.rs).
            #[cfg(target_os = "linux")]
            if let tauri::WindowEvent::Destroyed = event {
                linux_idle::last_window_closed(window.app_handle());
            }
        })
        .build(context)
        .expect("error while building freewheel");
    app.run(move |_app, event| {
        // The last window closing is not an exit (no code): the tray stays.
        if let tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } = &event
        {
            api.prevent_exit();
        }
        // Every exit, Quit included, releases held keys before the process ends.
        if let tauri::RunEvent::Exit = event
            && !device::shutdown(&quit_commands, SHUTDOWN_TIMEOUT)
        {
            tracing::warn!("device loop did not confirm the key release");
        }
    });
}

#[cfg(test)]
mod tests {
    /// #140: the device loop once authenticated with a stub that always
    /// answered 0, and nothing noticed. The manager the app builds must send
    /// bmd.py's response to a recorded challenge.
    #[test]
    fn the_device_loop_authenticates_with_the_bmd_py_response() {
        use freewheel_core::hid::{FakeTransport, HidError, Written};
        use std::sync::{Arc, Mutex};

        let vectors = include_str!("../../crates/freewheel-core/src/auth_vectors.txt");
        let pairs: Vec<(u64, u64)> = vectors
            .lines()
            .filter(|l| l.starts_with("0x"))
            .map(|l| {
                let mut f = l
                    .split_whitespace()
                    .map(|h| u64::from_str_radix(h.trim_start_matches("0x"), 16).unwrap());
                (f.next().unwrap(), f.next().unwrap())
            })
            .collect();
        assert_eq!(pairs.len(), 8);
        for (challenge, response) in pairs {
            assert_eq!(super::AUTH_RESPONSE(challenge), response);
            let fake = Arc::new(Mutex::new(FakeTransport::new()));
            for (sub, payload) in [(0u8, challenge), (2, 0), (4, 600)] {
                let mut r = vec![0x06, sub];
                r.extend_from_slice(&payload.to_le_bytes());
                fake.lock().unwrap().push_feature(r);
            }
            #[derive(Clone)]
            struct Dev(Arc<Mutex<FakeTransport>>);
            impl freewheel_core::hid::HidTransport for Dev {
                fn read_timeout(
                    &mut self,
                    b: &mut [u8],
                    t: std::time::Duration,
                ) -> Result<usize, HidError> {
                    self.0.lock().unwrap().read_timeout(b, t)
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
            let mut once = Some(Dev(Arc::clone(&fake)));
            let mut manager =
                super::device_manager(move || once.take().ok_or(HidError::Disconnected));
            manager.tick(std::time::Duration::ZERO);
            assert_eq!(
                manager.status(),
                Some(freewheel_core::connection::ConnectionStatus::Connected)
            );
            let mut sent = vec![0x06, 0x03];
            sent.extend_from_slice(&response.to_le_bytes());
            assert_eq!(
                fake.lock().unwrap().written()[2],
                Written::Feature(sent),
                "challenge {challenge:#018x}"
            );
        }
    }
    #[test]
    fn the_committed_config_has_no_updater_plugin_or_installer() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert!(conf["plugins"].get("updater").is_none());
        assert_eq!(conf["bundle"]["targets"], serde_json::json!(["app"]));
    }

    /// A21: tapped audio only goes to the playback IOProc.
    #[test]
    fn mac_app_volume_has_no_file_or_network_io() {
        let src = include_str!("mac_app_volume.rs");
        for io in [
            "std::fs", "std::net", "std::io", "File", "Tcp", "Udp", "Socket", "Write",
        ] {
            assert!(!src.contains(io), "mac_app_volume.rs uses {io}");
        }
        assert!(src.contains("play_at_gain(inp, unsafe { samples(out) }, gain)"));
    }

    #[test]
    fn info_plist_has_the_audio_capture_usage_string() {
        let plist = include_str!("../Info.plist");
        assert!(plist.contains("<key>NSAudioCaptureUsageDescription</key>\n\t<string>freewheel"));
    }
}
