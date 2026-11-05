/// Every app command; each window may call only those its capability file in
/// `capabilities/` allows (T8, `capabilities` test in main.rs).
const COMMANDS: &[&str] = &[
    "get_config",
    "save_config",
    "get_status",
    "get_permissions",
    "open_permission_pane",
    "simulate",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build script failed");
}
