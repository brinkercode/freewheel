#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use freewheel_core::clock::SystemClock;
use freewheel_core::logging::Logger;

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

fn main() {
    let _logger = init_logging();
    let context = tauri::generate_context!();
    debug_assert_eq!(
        context.config().product_name.as_deref(),
        Some(freewheel_core::APP_NAME)
    );
    tauri::Builder::default()
        .run(context)
        .expect("error while running freewheel");
}
