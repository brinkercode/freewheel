#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod app_volume;
mod apps;
#[cfg(windows)]
mod brightness;
#[cfg(any(target_os = "macos", all(unix, test)))]
mod builtin_brightness;
mod controls;
#[cfg(target_os = "macos")]
mod ddc_macos;
mod device;
mod e2e;
mod hidapi_transport;
mod inject;
mod launch;
#[cfg(target_os = "macos")]
mod mac_app_volume;
#[cfg(target_os = "macos")]
#[allow(dead_code)] // wired into the override worker by a later card
mod macos_volume;
mod permissions;
mod resolve_watch;
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
use freewheel_core::inject::{KeyInjector, MockInjector};
use freewheel_core::logging::Logger;
use freewheel_core::resolve::{POLL_INTERVAL, ResolveWatcher};
use freewheel_core::simulator;
use freewheel_core::status::{KEY_PRESSED_EVENT, STATUS_EVENT};
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

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

/// The Speed Editor's auth response algorithm is not in freewheel-core yet,
/// so the handshake is rejected and the tray shows Not connected until it is.
fn auth_response(_challenge: u64) -> u64 {
    0
}

/// Starts the device loop on its own thread; the tray shows its state and
/// the settings window hears the `status` and `key-pressed` events.
fn spawn_device(
    app: tauri::AppHandle<tauri::Wry>,
    config: Config,
    tray: tauri::tray::TrayIcon<tauri::Wry>,
    toggle: tauri::menu::CheckMenuItem<tauri::Wry>,
    commands: mpsc::Receiver<device::Command>,
) {
    use freewheel_core::connection::{ConnectionManager, ConnectionStatus};
    use freewheel_core::engine::Engine;
    use freewheel_core::overrides::{OverrideRunner, OverrideWorker};

    std::thread::spawn(move || {
        let (mut injector, api) = match (inject::EnigoInjector::new(), hidapi::HidApi::new()) {
            (Ok(injector), Ok(api)) => (injector, api),
            (Err(e), _) => return tracing::error!(error = %e, "no key injection"),
            (_, Err(e)) => return tracing::error!(error = %e, "no HID access"),
        };
        // The bus of the device opened last, for the `status` event.
        let bus = Arc::new(std::sync::Mutex::new(None));
        let open = {
            let bus = Arc::clone(&bus);
            move || {
                let device = hidapi_transport::HidapiTransport::open(&api)?;
                *bus.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = device.bus();
                Ok(device)
            }
        };
        let manager = ConnectionManager::new(open, SystemClock, auth_response as fn(u64) -> u64);
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
                injector: &mut injector,
                launcher: &mut launch::ProcessLauncher,
                overrides: &overrides,
            },
            commands,
            {
                let mut view = tray::TrayView::new(config.settings.low_battery_threshold);
                move |update| {
                    if let device::Update::KeyPressed(key) = &update {
                        let _ = app.emit(KEY_PRESSED_EVENT, key);
                    }
                    let settings = app.state::<settings::Settings>();
                    if update == device::Update::State(ConnectionStatus::Connected) {
                        settings.set_bus(
                            *bus.lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner),
                        );
                    }
                    if let Some(status) = settings.apply(&update) {
                        let _ = app.emit(STATUS_EVENT, status);
                    }
                    view.apply(&tray, &toggle, update)
                }
            },
        );
    });
}

/// How often the tray re-checks the macOS permissions.
const PERMISSION_POLL: Duration = Duration::from_secs(2);

/// Keeps the tray's 'Needs permission' line and the status event's
/// permissions current, checking every [`PERMISSION_POLL`] (macOS only;
/// elsewhere nothing is needed).
fn spawn_permission_watch(app: tauri::AppHandle<tauri::Wry>, status: tray::StatusLine<tauri::Wry>) {
    if !cfg!(target_os = "macos") {
        return;
    }
    std::thread::spawn(move || {
        loop {
            let current = permissions::current();
            status.show(&current);
            if let Some(event) = app.state::<settings::Settings>().set_permissions(&current) {
                let _ = app.emit(STATUS_EVENT, event);
            }
            std::thread::sleep(PERMISSION_POLL);
        }
    });
}

/// Starts the Resolve hand-off on its own thread: it polls for the
/// configured process, switches the device loop Off or On and notifies.
fn spawn_resolve_watch(
    app: tauri::AppHandle<tauri::Wry>,
    config: &Config,
    manual: mpsc::Receiver<resolve_watch::Message>,
    commands: mpsc::Sender<device::Command>,
) {
    let watcher = ResolveWatcher::from_settings(&config.settings);
    std::thread::spawn(move || {
        let pause = |pause: resolve_watch::Pause<'_>| {
            if let resolve_watch::Pause::Started(text) = pause {
                let shown = app
                    .notification()
                    .builder()
                    .title(freewheel_core::APP_NAME)
                    .body(text)
                    .show();
                if let Err(e) = shown {
                    tracing::warn!(error = %e, "replug notification failed");
                }
            }
            let paused = pause != resolve_watch::Pause::Ended;
            if let Some(status) = app.state::<settings::Settings>().set_paused(paused) {
                tracing::info!(paused, "Resolve hand-off");
                let _ = app.emit(STATUS_EVENT, status);
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

/// The injector "send for real" uses in simulator mode.
#[cfg(not(test))]
fn real_injector() -> Box<dyn KeyInjector> {
    match inject::EnigoInjector::new() {
        Ok(i) => Box::new(i),
        Err(e) => {
            tracing::warn!(error = %e, "no key injector, send for real records only");
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
            tray::sim_tray,
            settings::show_logs_file,
            settings::exit_wheel_override,
            apps::list_apps,
            apps::choose_app,
            settings::get_config,
            settings::save_config,
            settings::get_status,
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
            updates::open_release_page
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
    if updates::requested(std::env::args()) {
        let configured = updater_pubkey_set(&context.config().plugins);
        std::process::exit(updates::run(context, configured));
    }
    let (commands, receiver) = mpsc::channel();
    let quit_commands = commands.clone();
    let simulate = simulator::requested(std::env::args());
    if let Some(args) = simulate::browser_args(simulate || cfg!(feature = "e2e")) {
        // The Windows e2e (scripts/e2e-windows.mjs, and with the e2e-only
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
            tray::show_settings(app)
        }));
    }
    let mut builder = builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ));
    // `--simulate` never updates for real: its settings window uses the
    // simulated update source (src/simUpdate.js) instead.
    if !simulate && updater_pubkey_set(&context.config().plugins) {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }
    builder = if simulate {
        with_simulator(builder)
    } else {
        builder.invoke_handler(tauri::generate_handler![
            settings::get_config,
            settings::save_config,
            settings::get_status,
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
            settings::show_logs_file,
            settings::exit_wheel_override,
            apps::list_apps,
            apps::choose_app,
            updates::open_release_page
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
            app.manage(
                settings::Settings::new(store, config.clone(), commands.clone(), manual.clone())
                    .with_logger(logger.clone()),
            );
            spawn_resolve_watch(app.handle().clone(), &config, manual_rx, commands.clone());
            let (tray, toggle, status) = tray::build(app.handle(), commands, manual)?;
            if simulate {
                simulate::attach(
                    app.handle().clone(),
                    sim_store,
                    config.settings.low_battery_threshold,
                    real_injector,
                    tray,
                    toggle,
                    status,
                    receiver,
                );
                simulate::open_window(app)?;
            } else {
                spawn_device(app.handle().clone(), config, tray, toggle, receiver);
                spawn_permission_watch(app.handle().clone(), status);
            }
            if smoke {
                smoke::arm_timeout(app.handle().clone());
            }
            Ok(())
        })
        .on_page_load(move |webview, payload| {
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
        // Closing the settings window hides it; only Quit exits.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(context)
        .expect("error while building freewheel");
    app.run(move |_app, event| {
        // Every exit, Quit included, releases held keys before the process ends.
        if let tauri::RunEvent::Exit = event
            && !device::shutdown(&quit_commands, SHUTDOWN_TIMEOUT)
        {
            tracing::warn!("device loop did not confirm the key release");
        }
    });
}

/// Only release builds get the updater pubkey (`tauri build --config` in the
/// release workflow); CI and dev builds have none and skip the updater.
fn updater_pubkey_set(plugins: &tauri::utils::config::PluginConfig) -> bool {
    plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|k| !k.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::updater_pubkey_set;
    use tauri::utils::config::PluginConfig;

    fn plugins(updater: serde_json::Value) -> PluginConfig {
        PluginConfig([("updater".to_string(), updater)].into_iter().collect())
    }

    #[test]
    fn dev_and_ci_builds_without_a_pubkey_skip_the_updater() {
        assert!(!updater_pubkey_set(&PluginConfig::default()));
        assert!(!updater_pubkey_set(&plugins(serde_json::json!({}))));
        assert!(!updater_pubkey_set(&plugins(
            serde_json::json!({ "pubkey": "" })
        )));
    }

    #[test]
    fn a_release_build_with_the_injected_pubkey_runs_the_updater() {
        assert!(updater_pubkey_set(&plugins(
            serde_json::json!({ "pubkey": "injected" })
        )));
    }

    #[test]
    fn the_committed_config_has_no_pubkey() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let plugins = PluginConfig(serde_json::from_value(conf["plugins"].clone()).unwrap());
        assert!(!updater_pubkey_set(&plugins));
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
