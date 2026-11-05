//! The settings window's commands (`get_config`, `save_config`,
//! `get_status`) and the state behind the `status` event.
//!
//! `save_config` validates before it writes or applies anything (T8), then
//! hands the config to the running device loop: no restart.

use std::sync::Mutex;
use std::sync::mpsc::Sender;

use freewheel_core::config::{Config, ConfigStore, SaveError, parse_for_save};
use freewheel_core::permissions::Permissions;
use freewheel_core::status::StatusEvent;

use crate::device::{Command, Update};

/// The config and status the settings window reads, managed by Tauri.
pub struct Settings {
    store: Mutex<ConfigStore>,
    config: Mutex<Config>,
    status: Mutex<StatusEvent>,
    commands: Sender<Command>,
}

impl Settings {
    /// `store` is the one the running `config` was loaded from, so a bad
    /// file it rejected is still kept on the first save.
    pub fn new(store: ConfigStore, config: Config, commands: Sender<Command>) -> Self {
        Settings {
            status: Mutex::new(StatusEvent::new(config.base_layer.clone())),
            store: Mutex::new(store),
            config: Mutex::new(config),
            commands,
        }
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
        let config = parse_for_save(value)?;
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

#[tauri::command]
pub fn get_status(settings: tauri::State<'_, Settings>) -> StatusEvent {
    settings.status()
}

#[cfg(test)]
mod tests {
    use super::*;
    use freewheel_core::battery::Battery;
    use freewheel_core::config::{Action, CONFIG_FILE};
    use freewheel_core::connection::ConnectionStatus;
    use freewheel_core::permissions::{Grant, check_all};
    use std::sync::mpsc::{self, Receiver};

    fn settings() -> (tempfile::TempDir, Settings, Receiver<Command>) {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let settings = Settings::new(ConfigStore::new(dir.path()), Config::default(), tx);
        (dir, settings, rx)
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
