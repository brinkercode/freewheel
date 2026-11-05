#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[allow(dead_code)] // wired up by the connection manager
mod hidapi_transport;

fn main() {
    let context = tauri::generate_context!();
    debug_assert_eq!(
        context.config().product_name.as_deref(),
        Some(freewheel_core::APP_NAME)
    );
    tauri::Builder::default()
        .run(context)
        .expect("error while running freewheel");
}
