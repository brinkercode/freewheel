//! The settings window's commands (`get_config`, `save_config`,
//! `get_status`, `export_logs`, `set_detailed_logs`...) and the state behind
//! the `status` event.
//!
//! `save_config` validates before it writes or applies anything (T8), then
//! hands the config to the running device loop: no restart.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::Sender;
use std::time::{SystemTime, UNIX_EPOCH};

use freewheel_core::config::{
    Config, ConfigError, ConfigStore, MAPPINGS_FILE, SaveError, parse_for_save,
};
use freewheel_core::logging::Logger;
use freewheel_core::permissions::Permissions;
use freewheel_core::simulator::Bus;
use freewheel_core::status::StatusEvent;

use crate::device::{Command, Update};
use crate::resolve_watch::Message;
use freewheel_core::diagnostics::DeviceDiagnostics;

/// The config and status the settings window reads, managed by Tauri.
pub struct Settings {
    store: Mutex<ConfigStore>,
    config: Mutex<Config>,
    status: Mutex<StatusEvent>,
    commands: Sender<Command>,
    /// Tells the Resolve watcher a switch was by hand, as the tray does,
    /// and when the step-aside setting changes.
    manual: Mutex<Sender<Message>>,
    /// The session's file logger, when the log dir was usable.
    logger: Option<Logger>,
    /// The log bundle saved last, for Help & backup's 'Show file'.
    last_bundle: Mutex<Option<PathBuf>>,
    /// The Device test view's data, latest first heard from the device loop.
    diagnostics: Mutex<DeviceDiagnostics>,
}

impl Settings {
    /// `store` is the one the running `config` was loaded from, so a bad
    /// file it rejected is still kept on the first save.
    pub fn new(
        store: ConfigStore,
        config: Config,
        commands: Sender<Command>,
        manual: Sender<Message>,
    ) -> Self {
        Settings {
            status: Mutex::new(StatusEvent::new(config.base_layer.clone())),
            store: Mutex::new(store),
            config: Mutex::new(config),
            commands,
            manual: Mutex::new(manual),
            logger: None,
            last_bundle: Mutex::new(None),
            diagnostics: Mutex::new(DeviceDiagnostics::default()),
        }
    }

    /// The Device test view's data now.
    pub fn diagnostics(&self) -> DeviceDiagnostics {
        lock(&self.diagnostics).clone()
    }

    /// Uses `logger` for the detailed-logs switch and the log bundle, and
    /// applies the saved `detailed_logs` to it.
    pub fn with_logger(mut self, logger: Option<Logger>) -> Self {
        if let Some(logger) = &logger
            && lock(&self.config).settings.detailed_logs
        {
            logger.set_detailed(true);
        }
        self.logger = logger;
        self
    }

    /// Help & backup's Detailed logs switch: saves `detailed_logs` and
    /// raises (DEBUG, for 24 h) or lowers (INFO) the log level now.
    pub fn set_detailed_logs(&self, on: bool) -> Result<(), String> {
        let mut config = lock(&self.config);
        config.settings.detailed_logs = on;
        self.apply_detailed(on);
        lock(&self.store).save(&config).map_err(|e| {
            tracing::error!(error = %e, on, "saving the detailed-logs switch failed");
            e.to_string()
        })
    }

    fn apply_detailed(&self, on: bool) {
        match &self.logger {
            Some(logger) => logger.set_detailed(on),
            None => tracing::warn!(on, "no log file; detailed logs change nothing"),
        }
    }

    /// Writes the support bundle (every log file and a redacted config
    /// summary) to a new file in `out_dir` and returns its path.
    pub fn export_logs(&self, out_dir: &Path, about: &str, stamp: &str) -> Result<PathBuf, String> {
        let Some(logger) = &self.logger else {
            return Err("freewheel has no log file this session".into());
        };
        let path = freewheel_core::support::write_bundle(
            logger.dir(),
            &self.config(),
            about,
            out_dir,
            stamp,
        )
        .map_err(|e| {
            tracing::error!(error = %e, "writing the log bundle failed");
            format!("Saving the logs failed: {e}")
        })?;
        tracing::info!("log bundle saved");
        *lock(&self.last_bundle) = Some(path.clone());
        Ok(path)
    }

    /// The log bundle saved last this session, if it is still there.
    pub fn last_bundle(&self) -> Option<PathBuf> {
        lock(&self.last_bundle).clone().filter(|p| p.is_file())
    }

    /// The Wheel tab's 'Back to normal': the device loop leaves the dial
    /// override, as pressing its key again does.
    pub fn exit_override(&self) -> Result<(), String> {
        tracing::info!("settings window put the wheel back to normal");
        self.commands
            .send(Command::ExitOverride)
            .map_err(|_| "freewheel's device loop is not running".to_string())
    }

    /// The settings window's On/Off switch: switches the device loop, like
    /// the tray toggle, and tells the Resolve watcher it was by hand. The
    /// `status` event then shows the new state.
    pub fn set_enabled(&self, on: bool) -> Result<(), String> {
        tracing::info!(on, "settings window switched freewheel");
        let _ = lock(&self.manual).send(Message::Manual(on));
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

    /// The running config. `detailed_logs` reads false once the logger
    /// turned them off by itself after 24 h.
    pub fn config(&self) -> Config {
        let mut config = lock(&self.config).clone();
        if let Some(logger) = &self.logger {
            config.settings.detailed_logs &= logger.detailed();
        }
        config
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
        let before = std::mem::replace(&mut *lock(&self.config), config.clone()).settings;
        let after = &config.settings;
        if before.step_aside_for_resolve != after.step_aside_for_resolve {
            let _ = lock(&self.manual).send(Message::StepAside(after.step_aside_for_resolve));
        }
        if before.detailed_logs != after.detailed_logs {
            self.apply_detailed(after.detailed_logs);
        }
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
            Update::Override(mode) => status.wheel_override = *mode,
            Update::Diagnostics(d) => *lock(&self.diagnostics) = (**d).clone(),
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
    let picked = match crate::e2e::dialog(&app, crate::e2e::Dialog::SaveMappings) {
        Some(stub) => stub.map(Ok),
        None => app
            .dialog()
            .file()
            .set_file_name(MAPPINGS_FILE)
            .add_filter("freewheel mappings", &["json"])
            .blocking_save_file()
            .map(|p| p.into_path().map_err(|e| e.to_string())),
    };
    match picked {
        None => Ok(false),
        Some(Ok(path)) => {
            settings.export_to(&path)?;
            crate::simulate::note(&app, format!("Backed up the keys to {}", path.display()));
            Ok(true)
        }
        Some(Err(message)) => Err(SaveError::SaveFailed { message }),
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
    use crate::e2e::Dialog;
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
    let unreadable = |e: String| {
        tracing::warn!(error = %e, "mappings file cannot be read");
        SaveError::Malformed {
            message: format!("the file cannot be read: {e}"),
        }
    };
    let picked = match crate::e2e::dialog(&app, Dialog::OpenMappings) {
        Some(stub) => stub,
        None => app
            .dialog()
            .file()
            .add_filter("freewheel mappings", &["json"])
            .blocking_pick_file()
            .map(|p| p.into_path().map_err(|e| unreadable(e.to_string())))
            .transpose()?,
    };
    let Some(path) = picked else {
        return Ok(false);
    };
    let bytes = std::fs::read(&path).map_err(|e| unreadable(e.to_string()))?;
    let restored = settings.import(&bytes, allow_launch, || {
        if let Some(yes) = crate::e2e::dialog(&app, Dialog::ConfirmReplace) {
            return yes.is_some();
        }
        app.dialog()
            .message("Your keys, wheel and dial settings are replaced by the ones in this file. A copy of your current settings is kept first.")
            .title("Replace your keys?")
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Replace my keys".into(),
                "Keep my keys".into(),
            ))
            .blocking_show()
    })?;
    if restored {
        crate::simulate::note(&app, format!("Restored the keys from {}", path.display()));
    }
    Ok(restored)
}

/// The Help & backup Device test view's data (also sent as the
/// `device-diagnostics` event as it changes).
#[tauri::command]
pub fn get_device_diagnostics(settings: tauri::State<'_, Settings>) -> DeviceDiagnostics {
    settings.diagnostics()
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

/// Where log bundles go: the user's Downloads folder, else their home.
pub fn downloads_dir() -> PathBuf {
    let dirs = directories::UserDirs::new();
    dirs.as_ref()
        .and_then(|d| d.download_dir().map(Path::to_path_buf))
        .or_else(|| dirs.as_ref().map(|d| d.home_dir().to_path_buf()))
        .unwrap_or_else(std::env::temp_dir)
}

/// Saves the log bundle to the Downloads folder; its absolute path. Help &
/// backup's Download logs and the tray's 'Download logs for support...'.
pub fn save_log_bundle<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<String, String> {
    use tauri::Manager;
    let settings = app.state::<Settings>();
    let about = format!(
        "freewheel {} on {} ({})",
        app.package_info().version,
        os_info::get(),
        std::env::consts::ARCH
    );
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let stamp = freewheel_core::support::stamp(secs);
    let path = settings.export_logs(&downloads_dir(), &about, &stamp)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Saves the log bundle to the Downloads folder; resolves to its path.
#[tauri::command]
pub async fn export_logs(app: tauri::AppHandle) -> Result<String, String> {
    let path = save_log_bundle(&app)?;
    crate::simulate::note(&app, format!("Saved the logs to {path}"));
    Ok(path)
}

/// Help & backup's 'Show file': the saved log bundle, selected in Explorer
/// (the Finder on macOS). Only the bundle this session saved, never a path
/// the window names.
#[tauri::command]
pub async fn show_logs_file(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let path = app
        .state::<Settings>()
        .last_bundle()
        .ok_or("The log file is gone. Download the logs again.")?;
    crate::simulate::note(&app, format!("Showed {} in its folder", path.display()));
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|e| {
        tracing::error!(error = %e, "showing the log file failed");
        format!("The folder could not be opened: {e}")
    })
}

/// The Wheel tab's 'Back to normal': leaves the dial override.
#[tauri::command]
pub fn exit_wheel_override(settings: tauri::State<'_, Settings>) -> Result<(), String> {
    settings.exit_override()
}

/// Saves `detailed_logs` and raises or lowers the log level now.
#[tauri::command]
pub fn set_detailed_logs(settings: tauri::State<'_, Settings>, on: bool) -> Result<(), String> {
    tracing::info!(on, "settings window switched detailed logs");
    settings.set_detailed_logs(on)
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
        Receiver<Message>,
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
            assert_eq!(manual.try_recv(), Ok(Message::Manual(on)));
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

    fn logger_in(dir: &Path) -> Logger {
        Logger::init(
            dir,
            None,
            std::sync::Arc::new(freewheel_core::clock::SystemClock),
        )
        .unwrap()
    }

    #[test]
    fn the_detailed_logs_switch_persists_and_changes_the_level_now() {
        let (dir, settings, _commands) = settings();
        let logs = tempfile::tempdir().unwrap();
        let logger = logger_in(logs.path());
        let settings = settings.with_logger(Some(logger.clone()));
        assert!(!logger.detailed());
        settings.set_detailed_logs(true).unwrap();
        assert!(logger.detailed());
        assert!(settings.config().settings.detailed_logs);
        assert!(
            ConfigStore::new(dir.path())
                .load()
                .unwrap()
                .settings
                .detailed_logs
        );
        settings.set_detailed_logs(false).unwrap();
        assert!(!logger.detailed());
        assert!(
            !ConfigStore::new(dir.path())
                .load()
                .unwrap()
                .settings
                .detailed_logs
        );
    }

    #[test]
    fn a_saved_detailed_logs_applies_at_launch_and_through_save_config() {
        let dir = tempfile::tempdir().unwrap();
        let logs = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.settings.detailed_logs = true;
        let (tx, _rx) = mpsc::channel();
        let (manual, _manual_rx) = mpsc::channel();
        let logger = logger_in(logs.path());
        let settings = Settings::new(ConfigStore::new(dir.path()), config.clone(), tx, manual)
            .with_logger(Some(logger.clone()));
        assert!(logger.detailed());
        config.settings.detailed_logs = false;
        settings
            .save(&serde_json::to_value(&config).unwrap())
            .unwrap();
        assert!(!logger.detailed());
    }

    #[test]
    fn saving_a_new_step_aside_tells_the_resolve_watcher() {
        let (_dir, settings, _commands, manual) = settings_with_manual();
        let mut config = Config::default();
        settings
            .save(&serde_json::to_value(&config).unwrap())
            .unwrap();
        assert!(manual.try_recv().is_err(), "unchanged, nothing sent");
        config.settings.step_aside_for_resolve = false;
        settings
            .save(&serde_json::to_value(&config).unwrap())
            .unwrap();
        assert_eq!(manual.try_recv(), Ok(Message::StepAside(false)));
    }

    #[test]
    fn export_logs_writes_a_bundle_and_returns_its_path() {
        let (_dir, settings, _commands) = settings();
        let out = tempfile::tempdir().unwrap();
        assert!(settings.export_logs(out.path(), "about", "s").is_err());

        let logs = tempfile::tempdir().unwrap();
        let logger = logger_in(logs.path());
        logger.log_startup("9.9.9", "TestOS");
        let settings = settings.with_logger(Some(logger));
        let path = settings
            .export_logs(out.path(), "freewheel 9.9.9 on TestOS", "20261106-215800")
            .unwrap();
        assert!(path.is_absolute());
        assert_eq!(path.parent(), Some(out.path()));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("freewheel 9.9.9 starting on TestOS"),
            "{text}"
        );
        assert!(text.contains("== config (redacted) =="), "{text}");
    }
}
