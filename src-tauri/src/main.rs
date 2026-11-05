#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod app_volume;
#[cfg(windows)]
mod brightness;
#[cfg(any(target_os = "macos", all(unix, test)))]
mod builtin_brightness;
#[cfg(target_os = "macos")]
mod ddc_macos;
mod device;
mod hidapi_transport;
mod inject;
mod launch;
#[cfg(target_os = "macos")]
mod mac_app_volume;
mod resolve_watch;
mod simulate;
mod sysinfo_lister;
mod tray;
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
use tauri::Manager;
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

/// Starts the device loop on its own thread; the tray shows its state.
fn spawn_device(
    config: Config,
    tray: tauri::tray::TrayIcon<tauri::Wry>,
    toggle: tauri::menu::CheckMenuItem<tauri::Wry>,
    commands: mpsc::Receiver<device::Command>,
) {
    use freewheel_core::connection::ConnectionManager;
    use freewheel_core::engine::Engine;

    std::thread::spawn(move || {
        let (mut injector, api) = match (inject::EnigoInjector::new(), hidapi::HidApi::new()) {
            (Ok(injector), Ok(api)) => (injector, api),
            (Err(e), _) => return tracing::error!(error = %e, "no key injection"),
            (_, Err(e)) => return tracing::error!(error = %e, "no HID access"),
        };
        let open = move || hidapi_transport::HidapiTransport::open(&api);
        let manager = ConnectionManager::new(open, SystemClock, auth_response as fn(u64) -> u64);
        device::run(
            manager,
            Engine::new(&config),
            &SystemClock,
            &mut injector,
            &mut launch::ProcessLauncher,
            commands,
            |state| tray::show_state(&tray, &toggle, state),
        );
    });
}

/// Starts the Resolve hand-off on its own thread: it polls for the
/// configured process, switches the device loop Off or On and notifies.
fn spawn_resolve_watch(
    app: tauri::AppHandle<tauri::Wry>,
    config: &Config,
    manual: mpsc::Receiver<bool>,
    commands: mpsc::Sender<device::Command>,
) {
    let watcher = ResolveWatcher::from_settings(&config.settings);
    std::thread::spawn(move || {
        let notify = |text: &str| {
            let shown = app
                .notification()
                .builder()
                .title(freewheel_core::APP_NAME)
                .body(text)
                .show();
            if let Err(e) = shown {
                tracing::warn!(error = %e, "replug notification failed");
            }
        };
        resolve_watch::run(
            watcher,
            &mut sysinfo_lister::SysinfoLister::new(),
            true,
            POLL_INTERVAL,
            manual,
            commands,
            notify,
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

/// `--simulate`: the simulator session on its own config.
fn with_simulator(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    let real = ConfigStore::from_project_dirs()
        .unwrap_or_else(|| ConfigStore::new(std::env::temp_dir().join(freewheel_core::APP_NAME)));
    builder
        .manage(simulate::spawn(
            simulator::simulator_store(&real),
            real_injector,
        ))
        .invoke_handler(tauri::generate_handler![simulate::simulate])
}

/// `--simulate`: the simulator window. Called from the one `setup` hook,
/// since a second `Builder::setup` would replace the tray's.
fn open_simulator(app: &tauri::App) -> tauri::Result<()> {
    if let Some(settings) = app.get_webview_window("main") {
        settings.set_title(&simulator::title("freewheel settings", true))?;
    }
    let url = tauri::WebviewUrl::App("index.html#simulator".into());
    tauri::WebviewWindowBuilder::new(app, "simulator", url)
        .title(simulator::title(freewheel_core::APP_NAME, true))
        .inner_size(520.0, 720.0)
        .build()?;
    Ok(())
}

fn main() {
    let _logger = init_logging();
    let context = tauri::generate_context!();
    debug_assert_eq!(
        context.config().product_name.as_deref(),
        Some(freewheel_core::APP_NAME)
    );
    let (commands, receiver) = mpsc::channel();
    let quit_commands = commands.clone();
    let simulate = simulator::requested(std::env::args());
    // Registered first, so a second launch only focuses this one.
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tray::show_settings(app)
        }))
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ));
    if updater_pubkey_set(&context.config().plugins) {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }
    if simulate {
        builder = with_simulator(builder);
    }
    let app = builder
        .setup(move |app| {
            let config = ConfigStore::from_project_dirs()
                .map(|mut store| store.load_or_default().0)
                .unwrap_or_default();
            let (manual, manual_rx) = mpsc::channel();
            spawn_resolve_watch(app.handle().clone(), &config, manual_rx, commands.clone());
            let (tray, toggle) = tray::build(app.handle(), commands, manual)?;
            spawn_device(config, tray, toggle, receiver);
            if simulate {
                open_simulator(app)?;
            }
            Ok(())
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
