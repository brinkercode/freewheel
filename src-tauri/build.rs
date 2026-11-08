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
    "release_notes",
    "simulate",
    "sim_jump",
    "sim_tray",
    "sim_notes",
    "show_logs_file",
    "exit_wheel_override",
    "list_apps",
    "choose_app",
    "app_icons",
    "tray_update_state",
    "settings_ready",
    "update_check",
    "update_download",
    "update_install",
    "desktop_hints",
    "profile_app",
    "front_app_support",
];

/// The env var naming the next release's notes file (scripts/release-notes.mjs
/// preview), embedded for the Simulator's "Next release (real notes)".
const NEXT_NOTES_ENV: &str = "FREEWHEEL_NEXT_NOTES_FILE";

/// Writes `$OUT_DIR/next-notes.md`: the file `FREEWHEEL_NEXT_NOTES_FILE` names
/// (the e2e and test builds), else empty (selfupdate.rs SIM_NEXT_NOTES).
fn next_notes() {
    println!("cargo:rerun-if-env-changed={NEXT_NOTES_ENV}");
    let notes = match std::env::var(NEXT_NOTES_ENV) {
        Ok(path) if !path.trim().is_empty() => {
            println!("cargo:rerun-if-changed={path}");
            std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{NEXT_NOTES_ENV}={path}: {e}"))
        }
        _ => String::new(),
    };
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    std::fs::write(out.join("next-notes.md"), notes).expect("write next-notes.md");
}

fn main() {
    next_notes();
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build script failed");
}
