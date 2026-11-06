//! The version-1 config model and its persistence as `config.json`.
//!
//! Saves are atomic (temp file in the config directory, then rename). A
//! malformed or newer-version file is never lost: the first save after such
//! a load renames it to `config.json.bad-<timestamp>` before writing.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The config format version this build reads and writes.
pub const CONFIG_VERSION: u32 = 1;
/// The config file name inside the config directory.
pub const CONFIG_FILE: &str = "config.json";
/// The temp file a save writes before renaming it to [`CONFIG_FILE`].
pub const TEMP_FILE: &str = "config.json.tmp";
/// The prefix of a preserved bad file: `config.json.bad-<timestamp>`.
pub const BAD_PREFIX: &str = "config.json.bad-";

/// A layer identifier.
pub type LayerId = String;
/// A Speed Editor key name.
pub type Key = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Meta,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeyCombo {
    pub modifiers: Vec<Modifier>,
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum JogMode {
    Jog,
    Scroll,
    Shuttle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum OverrideMode {
    SystemVolume,
    AppVolume,
    Brightness,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum Action {
    Hotkey {
        combo: KeyCombo,
    },
    Held {
        combo: KeyCombo,
    },
    Latched {
        combo: KeyCombo,
    },
    Scroll {
        amount: i32,
    },
    LaunchApp {
        path: String,
        args: Vec<String>,
        enabled: bool,
    },
    LayerPush {
        target: LayerId,
    },
    SetJogMode {
        mode: JogMode,
    },
    OverrideToggle {
        mode: OverrideMode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub bindings: BTreeMap<Key, Action>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct JogBinding {
    pub left: Option<Action>,
    pub right: Option<Action>,
    pub threshold: u32,
    pub shuttle_max_rate_hz: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct JogSettings {
    pub default_mode: JogMode,
    pub per_mode: BTreeMap<JogMode, JogBinding>,
}

impl Default for JogSettings {
    fn default() -> Self {
        let scroll = JogBinding {
            left: Some(Action::Scroll { amount: -1 }),
            right: Some(Action::Scroll { amount: 1 }),
            threshold: 1,
            shuttle_max_rate_hz: None,
        };
        JogSettings {
            default_mode: JogMode::Scroll,
            per_mode: BTreeMap::from([(JogMode::Scroll, scroll)]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct OverrideSettings {
    pub system_volume_step: f32,
    pub app_volume_step: f32,
    pub brightness_step: u8,
}

impl Default for OverrideSettings {
    fn default() -> Self {
        OverrideSettings {
            system_volume_step: 0.02,
            app_volume_step: 0.02,
            brightness_step: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Settings {
    pub enabled: bool,
    #[serde(default)]
    pub start_at_login: bool,
    pub resolve_process_name: String,
    pub low_battery_threshold: u8,
    #[serde(default)]
    pub appearance: Appearance,
    // New versions download by themselves; the user chooses when to restart.
    #[serde(default = "yes")]
    pub auto_update: bool,
}

fn yes() -> bool {
    true
}

/// The settings window's colour scheme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum Appearance {
    // Follows the computer's light or dark setting.
    #[default]
    System,
    Light,
    Dark,
}

impl Default for Settings {
    fn default() -> Self {
        let resolve = if cfg!(windows) {
            "Resolve.exe"
        } else {
            "Resolve"
        };
        Settings {
            enabled: true,
            start_at_login: false,
            resolve_process_name: resolve.to_string(),
            low_battery_threshold: 15,
            appearance: Appearance::System,
            auto_update: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Config {
    pub version: u32,
    pub settings: Settings,
    pub layers: Vec<Layer>,
    pub base_layer: LayerId,
    pub jog: JogSettings,
    pub overrides: OverrideSettings,
}

impl Default for Config {
    fn default() -> Self {
        let base = Layer {
            id: "base".into(),
            name: "Base".into(),
            bindings: BTreeMap::new(),
        };
        Config {
            version: CONFIG_VERSION,
            settings: Settings::default(),
            base_layer: base.id.clone(),
            layers: vec![base],
            jog: JogSettings::default(),
            overrides: OverrideSettings::default(),
        }
    }
}

/// An error loading or saving the config.
#[derive(Debug)]
pub enum ConfigError {
    /// The file declares a version newer than [`CONFIG_VERSION`].
    UnsupportedVersion(u32),
    /// The file is not a valid version-1 config.
    Malformed(String),
    /// A filesystem operation failed.
    Io(io::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::UnsupportedVersion(v) => {
                write!(f, "config version {v} is newer than {CONFIG_VERSION}")
            }
            ConfigError::Malformed(msg) => write!(f, "malformed config: {msg}"),
            ConfigError::Io(e) => write!(f, "config I/O error: {e}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<io::Error> for ConfigError {
    fn from(e: io::Error) -> Self {
        ConfigError::Io(e)
    }
}

/// Loads and saves `config.json` in one config directory.
#[derive(Debug)]
pub struct ConfigStore {
    dir: PathBuf,
    /// Set when the last load rejected the file, so the next save keeps it.
    preserve_bad: bool,
}

impl ConfigStore {
    /// A store for an injected config directory.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        ConfigStore {
            dir: dir.into(),
            preserve_bad: false,
        }
    }

    /// A store for the user's config directory from `directories::ProjectDirs`.
    pub fn from_project_dirs() -> Option<Self> {
        directories::ProjectDirs::from("", "", crate::APP_NAME).map(|d| Self::new(d.config_dir()))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(CONFIG_FILE)
    }

    /// Loads the config; a missing file yields the default config.
    pub fn load(&mut self) -> Result<Config, ConfigError> {
        let bytes = match fs::read(self.path()) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
            Err(e) => return Err(e.into()),
        };
        let result = parse(&bytes);
        self.preserve_bad = matches!(
            result,
            Err(ConfigError::Malformed(_) | ConfigError::UnsupportedVersion(_))
        );
        result
    }

    /// The load the app uses: any error yields the default config.
    pub fn load_or_default(&mut self) -> (Config, Option<ConfigError>) {
        match self.load() {
            Ok(c) => (c, None),
            Err(e) => (Config::default(), Some(e)),
        }
    }

    /// Saves atomically: a rejected file is first renamed to
    /// `config.json.bad-<timestamp>`, then a temp file is written and renamed.
    pub fn save(&mut self, config: &Config) -> Result<(), ConfigError> {
        fs::create_dir_all(&self.dir)?;
        let path = self.path();
        if self.preserve_bad && path.exists() {
            fs::rename(&path, self.bad_path())?;
        }
        self.preserve_bad = false;
        let json =
            serde_json::to_vec_pretty(config).map_err(|e| ConfigError::Malformed(e.to_string()))?;
        let tmp = self.dir.join(TEMP_FILE);
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&json)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn bad_path(&self) -> PathBuf {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut candidate = self.dir.join(format!("{BAD_PREFIX}{secs}"));
        let mut n = 1;
        while candidate.exists() {
            candidate = self.dir.join(format!("{BAD_PREFIX}{secs}-{n}"));
            n += 1;
        }
        candidate
    }
}

fn parse(bytes: &[u8]) -> Result<Config, ConfigError> {
    #[derive(Deserialize)]
    struct Probe {
        version: u32,
    }
    let probe: Probe =
        serde_json::from_slice(bytes).map_err(|e| ConfigError::Malformed(e.to_string()))?;
    if probe.version > CONFIG_VERSION {
        return Err(ConfigError::UnsupportedVersion(probe.version));
    }
    serde_json::from_slice(bytes).map_err(|e| ConfigError::Malformed(e.to_string()))
}

/// Why `save_config` rejected a config from the settings window (T8). It
/// reaches the UI as `{"kind": ..., ...}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind")]
pub enum SaveError {
    /// `version` is not [`CONFIG_VERSION`].
    VersionMismatch {
        #[cfg_attr(test, ts(type = "number"))]
        found: u64,
        expected: u32,
    },
    /// A field this version does not know, as a JSON path (`settings.colour`).
    UnknownField { path: String },
    /// A binding on a key the Speed Editor does not have.
    UnknownKey { layer: LayerId, key: Key },
    /// `base_layer` or a `LayerPush` names a layer that does not exist.
    BadLayerReference { location: String, target: LayerId },
    /// Not a version-1 config at all (wrong types, missing fields).
    Malformed { message: String },
    /// The config was valid but writing `config.json` failed.
    SaveFailed { message: String },
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::VersionMismatch { found, expected } => {
                write!(f, "config version {found} is not {expected}")
            }
            SaveError::UnknownField { path } => write!(f, "unknown field {path}"),
            SaveError::UnknownKey { layer, key } => {
                write!(f, "layer {layer} binds unknown key {key}")
            }
            SaveError::BadLayerReference { location, target } => {
                write!(f, "{location} refers to missing layer {target}")
            }
            SaveError::Malformed { message } => write!(f, "malformed config: {message}"),
            SaveError::SaveFailed { message } => write!(f, "saving the config failed: {message}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// Parses and validates a config the settings window sends: the version, no
/// unknown fields, only Speed Editor keys and only existing layers.
pub fn parse_for_save(value: &serde_json::Value) -> Result<Config, SaveError> {
    let found = value.get("version").and_then(serde_json::Value::as_u64);
    if found != Some(u64::from(CONFIG_VERSION)) {
        return match found {
            Some(found) => Err(SaveError::VersionMismatch {
                found,
                expected: CONFIG_VERSION,
            }),
            None => Err(SaveError::Malformed {
                message: "version is missing or not a number".into(),
            }),
        };
    }
    let config: Config =
        serde_json::from_value(value.clone()).map_err(|e| SaveError::Malformed {
            message: e.to_string(),
        })?;
    let known = serde_json::to_value(&config).map_err(|e| SaveError::Malformed {
        message: e.to_string(),
    })?;
    if let Some(path) = unknown_field(value, &known, "") {
        return Err(SaveError::UnknownField { path });
    }
    validate(&config)?;
    Ok(config)
}

/// The first object key in `sent` that the parsed config does not keep.
fn unknown_field(sent: &serde_json::Value, known: &serde_json::Value, at: &str) -> Option<String> {
    use serde_json::Value;
    let join = |k: &str| {
        if at.is_empty() {
            k.to_string()
        } else {
            format!("{at}.{k}")
        }
    };
    match (sent, known) {
        (Value::Object(s), Value::Object(k)) => s.iter().find_map(|(name, v)| match k.get(name) {
            Some(kv) => unknown_field(v, kv, &join(name)),
            // A null optional field round-trips as null, so only a value is extra.
            None => Some(join(name)),
        }),
        (Value::Array(s), Value::Array(k)) => s
            .iter()
            .zip(k)
            .enumerate()
            .find_map(|(i, (sv, kv))| unknown_field(sv, kv, &format!("{at}[{i}]"))),
        _ => None,
    }
}

/// Checks the references inside a parsed config.
pub fn validate(config: &Config) -> Result<(), SaveError> {
    let has_layer = |id: &str| config.layers.iter().any(|l| l.id == id);
    let check_action = |location: String, action: &Action| match action {
        Action::LayerPush { target } if !has_layer(target) => Err(SaveError::BadLayerReference {
            location,
            target: target.clone(),
        }),
        _ => Ok(()),
    };
    if !has_layer(&config.base_layer) {
        return Err(SaveError::BadLayerReference {
            location: "base_layer".into(),
            target: config.base_layer.clone(),
        });
    }
    let key_names: Vec<String> = crate::report::Key::ALL
        .iter()
        .map(|k| format!("{k:?}"))
        .collect();
    for layer in &config.layers {
        for (key, action) in &layer.bindings {
            if !key_names.contains(key) {
                return Err(SaveError::UnknownKey {
                    layer: layer.id.clone(),
                    key: key.clone(),
                });
            }
            check_action(format!("layer {} key {key}", layer.id), action)?;
        }
    }
    for (mode, binding) in &config.jog.per_mode {
        for (side, action) in [("left", &binding.left), ("right", &binding.right)] {
            if let Some(action) = action {
                check_action(format!("jog {mode:?} {side}"), action)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo(key: &str) -> KeyCombo {
        KeyCombo {
            modifiers: vec![Modifier::Ctrl, Modifier::Shift],
            key: key.into(),
        }
    }

    fn roundtrip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + fmt::Debug>(value: &T) {
        let json = serde_json::to_string(value).unwrap();
        let back: T = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, value);
    }

    fn store() -> (tempfile::TempDir, ConfigStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path());
        (dir, store)
    }

    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn serde_roundtrips_every_config_type() {
        let actions = vec![
            Action::Hotkey { combo: combo("C") },
            Action::Held { combo: combo("H") },
            Action::Latched { combo: combo("L") },
            Action::Scroll { amount: -3 },
            Action::LaunchApp {
                path: "/bin/app".into(),
                args: vec!["-x".into()],
                enabled: false,
            },
            Action::LaunchApp {
                path: "app.exe".into(),
                args: vec![],
                enabled: true,
            },
            Action::LayerPush {
                target: "edit".into(),
            },
            Action::SetJogMode {
                mode: JogMode::Shuttle,
            },
            Action::OverrideToggle {
                mode: OverrideMode::Brightness,
            },
        ];
        for a in &actions {
            roundtrip(a);
        }
        let layer = Layer {
            id: "edit".into(),
            name: "Edit".into(),
            bindings: actions
                .iter()
                .enumerate()
                .map(|(i, a)| (format!("K{i}"), a.clone()))
                .collect(),
        };
        roundtrip(&layer);
        let mut jog = JogSettings::default();
        jog.per_mode.insert(
            JogMode::Shuttle,
            JogBinding {
                left: None,
                right: Some(actions[0].clone()),
                threshold: 4,
                shuttle_max_rate_hz: Some(20),
            },
        );
        roundtrip(&jog);
        let settings = Settings {
            enabled: false,
            start_at_login: true,
            resolve_process_name: "R".into(),
            low_battery_threshold: 9,
            appearance: Appearance::Dark,
            auto_update: false,
        };
        roundtrip(&settings);
        let mut config = Config::default();
        config.layers.push(layer);
        config.jog = jog;
        config.settings = settings;
        roundtrip(&config);
    }

    #[test]
    fn update_automatically_is_on_by_default_and_persists() {
        assert!(Settings::default().auto_update);
        assert_eq!(Settings::default().appearance, Appearance::System);
        // A config saved before the General tab keeps updating by itself.
        let old: Settings = serde_json::from_value(serde_json::json!({
            "enabled": true, "resolve_process_name": "Resolve", "low_battery_threshold": 15
        }))
        .unwrap();
        assert!(old.auto_update);
        assert_eq!(old.appearance, Appearance::System);
        let (_dir, mut s) = store();
        let mut config = Config::default();
        config.settings.auto_update = false;
        config.settings.appearance = Appearance::Light;
        s.save(&config).unwrap();
        let loaded = s.load().unwrap().settings;
        assert!(!loaded.auto_update);
        assert_eq!(loaded.appearance, Appearance::Light);
    }

    #[test]
    fn default_config_has_no_bindings_and_scrolls() {
        let c = Config::default();
        assert_eq!(c.version, 1);
        assert!(c.layers.iter().all(|l| l.bindings.is_empty()));
        assert_eq!(c.jog.default_mode, JogMode::Scroll);
        let scroll = &c.jog.per_mode[&JogMode::Scroll];
        assert_eq!(scroll.left, Some(Action::Scroll { amount: -1 }));
        assert_eq!(scroll.right, Some(Action::Scroll { amount: 1 }));
    }

    #[test]
    fn save_writes_temp_in_dir_and_renames_to_config_json() {
        let (dir, mut s) = store();
        let config = Config::default();
        s.save(&config).unwrap();
        assert_eq!(entries(dir.path()), vec![CONFIG_FILE.to_string()]);
        assert_eq!(s.load().unwrap(), config);

        // The temp file lives in the config dir: block it and the save fails
        // while the existing config.json stays intact.
        fs::create_dir(dir.path().join(TEMP_FILE)).unwrap();
        let mut changed = config.clone();
        changed.settings.enabled = false;
        assert!(matches!(s.save(&changed), Err(ConfigError::Io(_))));
        assert_eq!(s.load().unwrap(), config);
    }

    #[test]
    fn load_missing_returns_default() {
        let (_dir, mut s) = store();
        assert_eq!(s.load().unwrap(), Config::default());
    }

    #[test]
    fn load_newer_version_is_typed_error_and_app_gets_default() {
        let (_dir, mut s) = store();
        fs::write(s.path(), br#"{"version":2}"#).unwrap();
        assert!(matches!(s.load(), Err(ConfigError::UnsupportedVersion(2))));
        let (config, err) = s.load_or_default();
        assert_eq!(config, Config::default());
        assert!(matches!(err, Some(ConfigError::UnsupportedVersion(2))));
    }

    #[test]
    fn load_malformed_is_typed_error_and_app_gets_default() {
        let (_dir, mut s) = store();
        fs::write(s.path(), b"{not json").unwrap();
        assert!(matches!(s.load(), Err(ConfigError::Malformed(_))));
        let (config, err) = s.load_or_default();
        assert_eq!(config, Config::default());
        assert!(matches!(err, Some(ConfigError::Malformed(_))));
    }

    #[test]
    fn first_save_after_malformed_load_preserves_bad_file() {
        let (dir, mut s) = store();
        let bad = b"{\"version\":1, oops".to_vec();
        fs::write(s.path(), &bad).unwrap();
        let (config, _) = s.load_or_default();
        s.save(&config).unwrap();

        let names = entries(dir.path());
        assert_eq!(names.len(), 2, "{names:?}");
        assert_eq!(names[0], CONFIG_FILE);
        let stamp = names[1].strip_prefix(BAD_PREFIX).expect("bad file name");
        assert!(
            !stamp.is_empty() && stamp.chars().all(|c| c.is_ascii_digit()),
            "{stamp}"
        );
        assert_eq!(fs::read(dir.path().join(&names[1])).unwrap(), bad);
        assert_eq!(s.load().unwrap(), config);

        // Only the first save renames: the second overwrites config.json.
        s.save(&config).unwrap();
        assert_eq!(entries(dir.path()).len(), 2);
    }

    fn valid_json() -> serde_json::Value {
        let mut config = Config::default();
        config.layers.push(Layer {
            id: "edit".into(),
            name: "Edit".into(),
            bindings: BTreeMap::from([("Cut".into(), Action::Hotkey { combo: combo("X") })]),
        });
        config.layers[0].bindings.insert(
            "Cam1".into(),
            Action::LayerPush {
                target: "edit".into(),
            },
        );
        serde_json::to_value(config).unwrap()
    }

    #[test]
    fn save_accepts_a_valid_config() {
        let value = valid_json();
        let config = parse_for_save(&value).unwrap();
        assert_eq!(serde_json::to_value(config).unwrap(), value);
    }

    #[test]
    fn save_rejects_a_version_mismatch() {
        for version in [0, 2] {
            let mut value = valid_json();
            value["version"] = version.into();
            assert_eq!(
                parse_for_save(&value),
                Err(SaveError::VersionMismatch {
                    found: version,
                    expected: CONFIG_VERSION
                })
            );
        }
    }

    #[test]
    fn save_rejects_an_unknown_field() {
        let mut value = valid_json();
        value["settings"]["colour"] = "red".into();
        assert_eq!(
            parse_for_save(&value),
            Err(SaveError::UnknownField {
                path: "settings.colour".into()
            })
        );
        let mut value = valid_json();
        value["layers"][1]["hotkey"] = true.into();
        assert_eq!(
            parse_for_save(&value),
            Err(SaveError::UnknownField {
                path: "layers[1].hotkey".into()
            })
        );
    }

    #[test]
    fn save_rejects_a_binding_on_an_unknown_key() {
        let mut value = valid_json();
        value["layers"][1]["bindings"]["Turbo"] = value["layers"][1]["bindings"]["Cut"].clone();
        assert_eq!(
            parse_for_save(&value),
            Err(SaveError::UnknownKey {
                layer: "edit".into(),
                key: "Turbo".into()
            })
        );
    }

    #[test]
    fn save_rejects_a_bad_layer_reference() {
        let mut value = valid_json();
        value["layers"][0]["bindings"]["Cam1"]["LayerPush"]["target"] = "gone".into();
        assert_eq!(
            parse_for_save(&value),
            Err(SaveError::BadLayerReference {
                location: "layer base key Cam1".into(),
                target: "gone".into()
            })
        );
        let mut value = valid_json();
        value["base_layer"] = "gone".into();
        assert!(matches!(
            parse_for_save(&value),
            Err(SaveError::BadLayerReference { location, .. }) if location == "base_layer"
        ));
        let mut value = valid_json();
        value["jog"]["per_mode"]["Scroll"]["left"] =
            serde_json::json!({"LayerPush": {"target": "gone"}});
        assert!(matches!(
            parse_for_save(&value),
            Err(SaveError::BadLayerReference { target, .. }) if target == "gone"
        ));
    }

    #[test]
    fn save_errors_reach_the_ui_tagged_by_kind() {
        let err = SaveError::UnknownKey {
            layer: "base".into(),
            key: "Turbo".into(),
        };
        assert_eq!(
            serde_json::to_value(err).unwrap(),
            serde_json::json!({"kind": "UnknownKey", "layer": "base", "key": "Turbo"})
        );
        let mut value = valid_json();
        value["layers"] = "none".into();
        assert!(matches!(
            parse_for_save(&value),
            Err(SaveError::Malformed { .. })
        ));
    }

    #[test]
    fn config_path_is_injectable_and_project_dirs_names_freewheel() {
        let (dir, s) = store();
        assert_eq!(s.path(), dir.path().join(CONFIG_FILE));
        if let Some(real) = ConfigStore::from_project_dirs() {
            assert!(
                real.dir()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains("freewheel")
            );
            assert!(real.path().ends_with(CONFIG_FILE));
        }
    }
}
