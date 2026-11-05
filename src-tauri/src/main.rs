#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod app_volume;
mod device;
mod hidapi_transport;
mod inject;
mod launch;
mod tray;

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use freewheel_core::clock::SystemClock;
use freewheel_core::logging::Logger;

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
    tray: tauri::tray::TrayIcon<tauri::Wry>,
    toggle: tauri::menu::CheckMenuItem<tauri::Wry>,
    commands: mpsc::Receiver<device::Command>,
) {
    use freewheel_core::config::ConfigStore;
    use freewheel_core::connection::ConnectionManager;
    use freewheel_core::engine::Engine;

    std::thread::spawn(move || {
        let config = ConfigStore::from_project_dirs()
            .map(|mut store| store.load_or_default().0)
            .unwrap_or_default();
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

fn main() {
    let _logger = init_logging();
    let context = tauri::generate_context!();
    debug_assert_eq!(
        context.config().product_name.as_deref(),
        Some(freewheel_core::APP_NAME)
    );
    let (commands, receiver) = mpsc::channel();
    let quit_commands = commands.clone();
    let app = tauri::Builder::default()
        // Registered first, so a second launch only focuses this one.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tray::show_settings(app)
        }))
        .setup(move |app| {
            let (tray, toggle) = tray::build(app.handle(), commands)?;
            spawn_device(tray, toggle, receiver);
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
