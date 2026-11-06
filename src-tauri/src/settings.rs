//! The settings window's commands (`get_config`, `save_config`,
//! `get_status`) and the state behind the `status` event.
//!
//! `save_config` validates before it writes or applies anything (T8), then
//! hands the config to the running device loop: no restart.

use std::path::Path;
use std::sync::Mutex;
use std::sync::mpsc::Sender;

use freewheel_core::config::{
    Config, ConfigError, ConfigStore, MAPPINGS_FILE, SaveError, parse_for_save,
};
use freewheel_core::permissions::Permissions;
use freewheel_core::simulator::Bus;
use freewheel_core::status::StatusEvent;

use crate::device::{Command, Update};

/// The config and status the settings window reads, managed by Tauri.
pub struct Settings {
    store: Mutex<ConfigStore>,
    config: Mutex<Config>,
    status: Mutex<StatusEvent>,
    commands: Sender<Command>,
    /// Tells the Resolve watcher a switch was by hand, as the tray does.
    manual: Mutex<Sender<bool>>,
}

impl Settings {
    /// `store` is the one the running `config` was loaded from, so a bad
    /// file it rejected is still kept on the first save.
    pub fn new(
        store: ConfigStore,
        config: Config,
        commands: Sender<Command>,
        manual: Sender<bool>,
    ) -> Self {
        Settings {
            status: Mutex::new(StatusEvent::new(config.base_layer.clone())),
            store: Mutex::new(store),
            config: Mutex::new(config),
            commands,
            manual: Mutex::new(manual),
        }
    }

    /// The settings window's On/Off switch: switches the device loop, like
    /// the tray toggle, and tells the Resolve watcher it was by hand. The
    /// `status` event then shows the new state.
    pub fn set_enabled(&self, on: bool) -> Result<(), String> {
        tracing::info!(on, "settings window switched freewheel");
        let _ = lock(&self.manual).send(on);
        self.commands.send(Command::SetEnabled(on)).map_err(|_| {
            tracing::error!(on, "device loop is gone; cannot switch freewheel");
            "freewheel's device loop is not running".to_string()
        })
    }

    /// Records how the device that is connecting is attached; the next
    /// `Update::State` carries it in the `status` event.
    pub fn set_bus(&self, bus: Option<Bus>) {
        lock(&self.status).bus = bus;
    }

    /// Records whether freewheel stepped aside for Resolve; returns the new
    /// status when it changed.
    pub fn set_paused(&self, paused: bool) -> Option<StatusEvent> {
        self.change(|status| status.paused = paused)
    }

    pub fn config(&self) -> Config {
        lock(&self.config).clone()
    }

    pub fn status(&self) -> StatusEvent {
        lock(&self.status).clone()
    }

    /// Validates `value`, writes it to `config.json` and applies it to the
    /// running engine. An invalid config changes nothing.
    pub fn save(&self, value: &serde_json::Value) -> Result<(), SaveError> {
        self.replace(parse_for_save(value)?)
    }

    /// Writes the running config's mappings to `path` (freewheel-mappings.json).
    pub fn export_to(&self, path: &Path) -> Result<(), SaveError> {
        std::fs::write(path, self.config().export_mappings()).map_err(|e| {
            tracing::error!(error = %e, "exporting the mappings failed");
            SaveError::SaveFailed {
                message: e.to_string(),
            }
        })?;
        tracing::info!("mappings exported");
        Ok(())
    }

    /// Validates a mappings file, asks `confirm`, backs up `config.json`,
    /// then replaces the mappings. `Ok(false)` when the user kept their keys.
    /// LaunchApp bindings stay disabled unless `allow_launch` (T1).
    pub fn import(
        &self,
        bytes: &[u8],
        allow_launch: bool,
        confirm: impl FnOnce() -> bool,
    ) -> Result<bool, SaveError> {
        let config = self
            .config()
            .import_mappings(bytes, allow_launch)
            .inspect_err(|e| tracing::warn!(error = %e, "mappings file rejected"))?;
        if !confirm() {
            tracing::info!("mappings import cancelled");
            return Ok(false);
        }
        let failed = |e: ConfigError| {
            tracing::error!(error = %e, "backing up config.json failed; import stopped");
            SaveError::SaveFailed {
                message: format!("backing up config.json failed: {e}"),
            }
        };
        lock(&self.store).backup().map_err(failed)?;
        self.replace(config)?;
        tracing::info!(allow_launch, "mappings imported");
        Ok(true)
    }

    fn replace(&self, config: Config) -> Result<(), SaveError> {
        lock(&self.store)
            .save(&config)
            .map_err(|e| SaveError::SaveFailed {
                message: e.to_string(),
            })?;
        *lock(&self.config) = config.clone();
        if self
            .commands
            .send(Command::Reload(Box::new(config)))
            .is_err()
        {
            tracing::warn!("device loop is gone; the saved config applies on the next start");
        }
        Ok(())
    }

    /// Folds a device loop update into the status; returns the new status
    /// when it changed, for the `status` event.
    pub fn apply(&self, update: &Update) -> Option<StatusEvent> {
        self.change(|status| match update {
            Update::State(connection) => status.set_connection(*connection),
            Update::Battery(battery) => status.battery = Some(*battery),
            Update::Layer(layer) => status.layer.clone_from(layer),
            Update::KeyPressed(_) => {}
        })
    }

    /// Records the macOS permissions; returns the new status when it changed.
    pub fn set_permissions(&self, permissions: &Permissions) -> Option<StatusEvent> {
        self.change(|status| status.set_permissions(permissions))
    }

    fn change(&self, f: impl FnOnce(&mut StatusEvent)) -> Option<StatusEvent> {
        let mut status = lock(&self.status);
        let before = status.clone();
        f(&mut status);
        (*status != before).then(|| status.clone())
    }
}

/// A poisoned lock still holds usable data: the writers never leave it torn.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[tauri::command]
pub fn get_config(settings: tauri::State<'_, Settings>) -> Config {
    settings.config()
}

#[tauri::command]
pub fn save_config(
    settings: tauri::State<'_, Settings>,
    config: serde_json::Value,
) -> Result<(), SaveError> {
    settings.save(&config)
}

/// Asks for a file in a save dialog and exports the mappings to it; `false`
/// when the user cancelled. Async so the blocking dialog is off the main thread.
#[tauri::command]
pub async fn export_mappings(
    app: tauri::AppHandle,
    settings: tauri::State<'_, Settings>,
) -> Result<bool, SaveError> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .set_file_name(MAPPINGS_FILE)
        .add_filter("freewheel mappings", &["json"])
        .blocking_save_file();
    match picked.map(|p| p.into_path()) {
        None => Ok(false),
        Some(Ok(path)) => settings.export_to(&path).map(|()| true),
        Some(Err(e)) => Err(SaveError::SaveFailed {
            message: e.to_string(),
        }),
    }
}

/// Asks for a mappings file, validates it, confirms "Replace your keys?",
/// backs up config.json and replaces the mappings; `false` when the user
/// cancelled or kept their keys.
#[tauri::command]
pub async fn import_mappings(
    app: tauri::AppHandle,
    settings: tauri::State<'_, Settings>,
    allow_launch: bool,
) -> Result<bool, SaveError> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("freewheel mappings", &["json"])
        .blocking_pick_file()
    else {
        return Ok(false);
    };
    let unreadable = |e: String| {
        tracing::warn!(error = %e, "mappings file cannot be read");
        SaveError::Malformed {
            message: format!("the file cannot be read: {e}"),
        }
    };
    let path = picked.into_path().map_err(|e| unreadable(e.to_string()))?;
    let bytes = std::fs::read(path).map_err(|e| unreadable(e.to_string()))?;
    settings.import(&bytes, allow_launch, || {
        app.dialog()
            .message("Your keys, wheel and dial settings are replaced by the ones in this file. A copy of your current settings is kept first.")
            .title("Replace your keys?")
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Replace my keys".into(),
                "Keep my keys".into(),
            ))
            .blocking_show()
    })
}

#[tauri::command]
pub fn get_status(settings: tauri::State<'_, Settings>) -> StatusEvent {
    settings.status()
}

#[tauri::command]
pub fn set_enabled(settings: tauri::State<'_, Settings>, on: bool) -> Result<(), String> {
    settings.set_enabled(on)
}

#[cfg(test)]
mod tests {
    use super::*;
    use freewheel_core::battery::Battery;
    use freewheel_core::config::{Action, BACKUP_PREFIX, CONFIG_FILE};
    use freewheel_core::connection::ConnectionStatus;
    use freewheel_core::permissions::{Grant, check_all};
    use std::sync::mpsc::{self, Receiver};

    fn settings() -> (tempfile::TempDir, Settings, Receiver<Command>) {
        let (dir, settings, rx, _manual) = settings_with_manual();
        (dir, settings, rx)
    }

    type WithManual = (
        tempfile::TempDir,
        Settings,
        Receiver<Command>,
        Receiver<bool>,
    );

    fn settings_with_manual() -> WithManual {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let (manual, manual_rx) = mpsc::channel();
        let settings = Settings::new(ConfigStore::new(dir.path()), Config::default(), tx, manual);
        (dir, settings, rx, manual_rx)
    }

    #[test]
    fn the_switch_turns_the_device_loop_off_and_on_by_hand() {
        let (_dir, settings, commands, manual) = settings_with_manual();
        for on in [false, true] {
            settings.set_enabled(on).unwrap();
            assert!(matches!(commands.try_recv(), Ok(Command::SetEnabled(o)) if o == on));
            assert_eq!(manual.try_recv(), Ok(on));
        }
        drop(commands);
        assert!(settings.set_enabled(false).is_err());
    }

    #[test]
    fn the_status_carries_the_bus_and_the_resolve_pause() {
        let (_dir, settings, _commands) = settings();
        settings.set_bus(Some(Bus::Bluetooth));
        let status = settings
            .apply(&Update::State(ConnectionStatus::Connected))
            .unwrap();
        assert_eq!(status.bus, Some(Bus::Bluetooth));
        let paused = settings.set_paused(true).unwrap();
        assert!(paused.paused);
        assert_eq!(settings.set_paused(true), None);
        let off = settings
            .apply(&Update::State(ConnectionStatus::Off))
            .unwrap();
        assert_eq!(off.bus, None);
        assert!(off.paused);
        assert!(!settings.set_paused(false).unwrap().paused);
    }

    #[test]
    fn save_config_writes_and_applies_without_a_restart() {
        let (dir, settings, commands) = settings();
        let mut config = Config::default();
        config.layers[0]
            .bindings
            .insert("Cut".into(), Action::Scroll { amount: 3 });
        settings
            .save(&serde_json::to_value(&config).unwrap())
            .unwrap();

        assert_eq!(settings.config(), config);
        match commands.try_recv() {
            Ok(Command::Reload(applied)) => assert_eq!(*applied, config),
            _ => panic!("the device loop did not get the config"),
        }
        let saved = ConfigStore::new(dir.path()).load().unwrap();
        assert_eq!(saved, config);
    }

    #[test]
    fn save_config_rejects_an_invalid_config_and_changes_nothing() {
        let (dir, settings, commands) = settings();
        let mut value = serde_json::to_value(Config::default()).unwrap();
        value["version"] = 2.into();
        assert_eq!(
            settings.save(&value),
            Err(SaveError::VersionMismatch {
                found: 2,
                expected: 1
            })
        );
        value["version"] = 1.into();
        value["layers"][0]["bindings"]["Turbo"] = serde_json::json!({"Scroll": {"amount": 1}});
        assert!(matches!(
            settings.save(&value),
            Err(SaveError::UnknownKey { .. })
        ));
        value["layers"][0]["bindings"] = serde_json::json!({});
        value["base_layer"] = "gone".into();
        assert!(matches!(
            settings.save(&value),
            Err(SaveError::BadLayerReference { .. })
        ));

        assert_eq!(settings.config(), Config::default());
        assert!(commands.try_recv().is_err());
        assert!(!dir.path().join(CONFIG_FILE).exists());
    }

    fn mappings() -> Config {
        let mut config = Config::default();
        config.layers[0].bindings.insert(
            "Snap".into(),
            Action::LaunchApp {
                path: "/bin/app".into(),
                args: vec![],
                enabled: true,
            },
        );
        config
    }

    #[test]
    fn export_writes_the_mappings_file() {
        let (dir, settings, _commands) = settings();
        let path = dir.path().join(MAPPINGS_FILE);
        settings.export_to(&path).unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            Config::default().export_mappings()
        );
        assert!(matches!(
            settings.export_to(dir.path()),
            Err(SaveError::SaveFailed { .. })
        ));
    }

    #[test]
    fn import_confirms_then_backs_up_then_replaces_and_applies() {
        let (dir, settings, commands) = settings();
        settings
            .save(&serde_json::to_value(Config::default()).unwrap())
            .unwrap();
        let _ = commands.try_recv();
        let before = std::fs::read(dir.path().join(CONFIG_FILE)).unwrap();
        let bytes = mappings().export_mappings();

        let mut asked = false;
        assert_eq!(
            settings.import(&bytes, true, || {
                asked = true;
                false
            }),
            Ok(false)
        );
        assert!(asked);
        assert_eq!(settings.config(), Config::default());
        assert!(commands.try_recv().is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

        assert_eq!(settings.import(&bytes, true, || true), Ok(true));
        assert_eq!(settings.config(), mappings());
        assert!(matches!(commands.try_recv(), Ok(Command::Reload(c)) if *c == mappings()));
        let backup = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.to_string_lossy().contains(BACKUP_PREFIX))
            .expect("a backup of config.json");
        assert_eq!(std::fs::read(backup).unwrap(), before);
        assert_eq!(ConfigStore::new(dir.path()).load().unwrap(), mappings());
    }

    #[test]
    fn import_rejects_a_bad_file_before_asking_and_changes_nothing() {
        let (dir, settings, commands) = settings();
        for bytes in [&b"{oops"[..], br#"{"version": 2}"#] {
            let result = settings.import(bytes, true, || panic!("asked for a bad file"));
            assert!(matches!(
                result,
                Err(SaveError::Malformed { .. } | SaveError::VersionMismatch { .. })
            ));
        }
        assert_eq!(settings.config(), Config::default());
        assert!(commands.try_recv().is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn import_without_the_opt_in_enables_no_launch_app() {
        let (_dir, settings, _commands) = settings();
        let bytes = mappings().export_mappings();
        assert_eq!(settings.import(&bytes, false, || true), Ok(true));
        assert!(matches!(
            settings.config().layers[0].bindings["Snap"],
            Action::LaunchApp { enabled: false, .. }
        ));
    }

    #[test]
    fn status_follows_the_device_loop_and_the_permissions() {
        let (_dir, settings, _commands) = settings();
        assert_eq!(settings.status().layer, "base");
        let changed = settings
            .apply(&Update::State(ConnectionStatus::Connected))
            .unwrap();
        assert_eq!(changed.connection, ConnectionStatus::Connected);
        let battery = Battery {
            level: 40,
            charging: false,
        };
        assert_eq!(
            settings.apply(&Update::Battery(battery)).unwrap().battery,
            Some(battery)
        );
        assert_eq!(
            settings.apply(&Update::Layer("edit".into())).unwrap().layer,
            "edit"
        );
        let missing = check_all(|_| false);
        let status = settings.set_permissions(&missing).unwrap();
        assert_eq!(status.permissions["Accessibility"], Grant::Missing);
        assert_eq!(status.permissions["InputMonitoring"], Grant::Missing);
        // Nothing new, no event.
        assert_eq!(settings.set_permissions(&missing), None);
        assert_eq!(settings.apply(&Update::Layer("edit".into())), None);
        assert_eq!(settings.status(), status);
    }
}
