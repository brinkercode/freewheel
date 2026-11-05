#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[allow(dead_code)] // wired up by the connection manager
mod hidapi_transport;
#[cfg(not(test))]
mod inject;

fn main() {
    let context = tauri::generate_context!();
    debug_assert_eq!(
        context.config().product_name.as_deref(),
        Some(freewheel_core::APP_NAME)
    );
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_process::init());
    if updater_pubkey_set(&context.config().plugins) {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }
    builder.run(context).expect("error while running freewheel");
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
}
