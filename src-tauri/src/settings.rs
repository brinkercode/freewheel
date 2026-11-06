//! The settings window's commands (`get_config`, `save_config`,
//! `get_status`) and the state behind the `status` event.
//!
//! `save_config` validates before it writes or applies anything (T8), then
//! hands the config to the running device loop: no restart.

use std::sync::Mutex;
use std::sync::mpsc::Sender;

use freewheel_core::config::{Config, ConfigStore, SaveError, parse_for_save};
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

    /// The General tab's Start at login switch: `os` sets the OS login item
    /// first, then `start_at_login` is saved in the running config, so a
    /// later save from the window keeps it.
    pub fn set_start_at_login(
        &self,
        os: impl FnOnce(bool) -> Result<(), String>,
        on: bool,
    ) -> Result<(), String> {
        os(on)?;
        let mut config = lock(&self.config);
        config.settings.start_at_login = on;
        lock(&self.store).save(&config).map_err(|e| e.to_string())
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

#[tauri::command]
pub fn set_enabled(settings: tauri::State<'_, Settings>, on: bool) -> Result<(), String> {
    settings.set_enabled(on)
}

/// Whether the OS login item is registered now.
#[tauri::command]
pub fn get_start_at_login(app: tauri::AppHandle) -> bool {
    crate::tray::autostart_enabled(&app)
}

/// Sets the OS login item and the config; resolves to what the OS has now.
#[tauri::command]
pub fn set_start_at_login(
    app: tauri::AppHandle,
    settings: tauri::State<'_, Settings>,
    on: bool,
) -> Result<bool, String> {
    tracing::info!(on, "settings window switched start at login");
    settings
        .set_start_at_login(|on| crate::tray::set_autostart(&app, on), on)
        .inspect_err(|e| tracing::error!(error = %e, on, "cannot change start at login"))?;
    Ok(crate::tray::autostart_enabled(&app))
}

/// The app's version, for 'freewheel <version>' on the General tab.
#[tauri::command]
pub fn app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
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
    fn start_at_login_sets_the_os_item_then_the_running_config() {
        let (_dir, settings, _commands) = settings();
        let mut asked = None;
        settings
            .set_start_at_login(
                |on| {
                    asked = Some(on);
                    Ok(())
                },
                true,
            )
            .unwrap();
        assert_eq!(asked, Some(true));
        assert!(settings.config().settings.start_at_login);
        assert!(
            lock(&settings.store)
                .load()
                .unwrap()
                .settings
                .start_at_login
        );
        let err = settings.set_start_at_login(|_| Err("denied".into()), false);
        assert_eq!(err, Err("denied".to_string()));
        assert!(settings.config().settings.start_at_login);
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
