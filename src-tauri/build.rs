/// Every app command; each window may call only those its capability file in
/// `capabilities/` allows (T8, `capabilities` test in main.rs).
const COMMANDS: &[&str] = &[
    "get_config",
    "save_config",
    "get_status",
    "get_device_diagnostics",
    "set_enabled",
    "get_start_at_login",
    "set_start_at_login",
    "app_version",
    "export_mappings",
    "import_mappings",
    "get_permissions",
    "open_permission_pane",
    "open_bluetooth_settings",
    "export_logs",
    "set_detailed_logs",
    "open_release_page",
    "simulate",
    "sim_jump",
    "sim_tray",
    "show_logs_file",
    "exit_wheel_override",
    "list_apps",
    "choose_app",
    "app_icons",
    "tray_update_state",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build script failed");
}
