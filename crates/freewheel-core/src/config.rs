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
/// The prefix of the copy an import keeps: `config.json.backup-<timestamp>`.
pub const BACKUP_PREFIX: &str = "config.json.backup-";
/// The file name export suggests: the mappings without the app settings.
pub const MAPPINGS_FILE: &str = "freewheel-mappings.json";

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
    /// Each wheel tick steps the system volume by `overrides.system_volume_step`.
    Volume,
    /// Each wheel tick steps display brightness by `overrides.brightness_step`.
    Brightness,
}

impl JogMode {
    /// The dial override control a turn of the wheel drives in this mode,
    /// or `None` when the wheel fires its jog actions.
    pub fn override_mode(self) -> Option<OverrideMode> {
        match self {
            JogMode::Volume => Some(OverrideMode::SystemVolume),
            JogMode::Brightness => Some(OverrideMode::Brightness),
            JogMode::Jog | JogMode::Scroll | JogMode::Shuttle => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum OverrideMode {
    SystemVolume,
    AppVolume,
    Brightness,
}

/// One `open` target per OS, as a mappings file wrote them (an app name,
/// path, bundle id, .desktop id or web address).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PerOsTargets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub windows: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub macos: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub linux: Option<String>,
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
        /// The folder the app starts in (a mappings file's `cwd`); `None`:
        /// freewheel's own. Left out of config.json when unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        cwd: Option<String>,
        /// What a mappings file's per-OS `open` said for each OS, kept so a
        /// backup writes every OS back; `path`/`args` are this computer's
        /// one. Left out of config.json when the file had a single target.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        per_os: Option<PerOsTargets>,
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
        // A fresh setup: every wheel button (SHTL, JOG, SCRL) scrolls, as
        // the design handoff's prototype starts.
        JogSettings {
            default_mode: JogMode::Scroll,
            per_mode: BTreeMap::from([
                (JogMode::Shuttle, scroll.clone()),
                (JogMode::Jog, scroll.clone()),
                (JogMode::Scroll, scroll),
            ]),
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
    // Resolve tab: switch Off while DaVinci Resolve is open so it gets the
    // Speed Editor; off keeps freewheel On alongside Resolve.
    #[serde(default = "yes")]
    pub step_aside_for_resolve: bool,
    // Help & backup: DEBUG lines in freewheel.log (they turn off after 24 h).
    #[serde(default)]
    pub detailed_logs: bool,
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
        // The process DaVinci Resolve runs as: Resolve.exe, Resolve.app's
        // Resolve, and /opt/resolve/bin/resolve on Linux.
        let resolve = if cfg!(windows) {
            "Resolve.exe"
        } else if cfg!(target_os = "linux") {
            "resolve"
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
            step_aside_for_resolve: true,
            detailed_logs: false,
        }
    }
}

/// One app a profile is for: the profile is active while this app is in
/// front. Each OS names apps its own way: the program file on Windows
/// (`notepad.exe`), the bundle id on macOS (`com.apple.TextEdit`), the
/// program or its window class on Linux (`gedit`, `org.gnome.gedit`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ProfileApp {
    /// What the Keys tab shows (`Notepad`).
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub windows: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub macos: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub linux: Option<String>,
}

/// A profile: its own pages, used while one of its apps is in front.
/// Pages not in any profile belong to the Default profile (the pages
/// 1.0.0 had), which is used while no profile's app is in front.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// The apps it is for. Left out of config.json when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(as = "Option<Vec<ProfileApp>>", optional))]
    pub apps: Vec<ProfileApp>,
    /// Its pages (layer ids, at least one): the first is where it starts.
    pub pages: Vec<LayerId>,
    /// Its colour in the Keys tab: one of [`PROFILE_COLORS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub color: Option<String>,
    /// Its icon in the Keys tab: a bundled icon's name (Google's Material
    /// icon font, Rounded),
    /// bare (`edit`; the mappings file writes `:edit:`). See [`crate::icons`].
    /// A name the font lacks draws the plain profile icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub icon: Option<String>,
}

/// The colours a profile can have (src/styles/base.css draws each).
pub const PROFILE_COLORS: &[&str] = &[
    "rose", "orange", "amber", "green", "teal", "blue", "indigo", "violet",
];

/// The icons the profile editor suggests (any name of [`crate::icons`] works);
/// new profiles take them in turn.
pub const SUGGESTED_ICONS: &[&str] = &[
    "brush",
    "code",
    "music_note",
    "sports_esports",
    "movie",
    "edit",
    "photo_camera",
    "terminal",
];

/// What the Keys tab calls the pages no profile has.
pub const DEFAULT_PROFILE_NAME: &str = "Default";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Config {
    pub version: u32,
    pub settings: Settings,
    /// Every page: the Default profile's and each profile's.
    pub layers: Vec<Layer>,
    /// The Default profile's first page.
    pub base_layer: LayerId,
    pub jog: JogSettings,
    pub overrides: OverrideSettings,
    /// The app profiles. Left out of config.json when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(as = "Option<Vec<Profile>>", optional))]
    pub profiles: Vec<Profile>,
}

impl Config {
    /// The profile page `layer` belongs to (None: the Default profile).
    pub fn profile_of(&self, layer: &str) -> Option<&Profile> {
        self.profiles
            .iter()
            .find(|p| p.pages.iter().any(|l| l == layer))
    }

    /// The pages of profile `profile` (None: the Default profile's), in order.
    pub fn pages_of(&self, profile: Option<&str>) -> Vec<&Layer> {
        match profile.and_then(|id| self.profiles.iter().find(|p| p.id == id)) {
            Some(p) => p
                .pages
                .iter()
                .filter_map(|id| self.layers.iter().find(|l| &l.id == id))
                .collect(),
            None => self
                .layers
                .iter()
                .filter(|l| self.profile_of(&l.id).is_none())
                .collect(),
        }
    }
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
            profiles: Vec::new(),
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

    /// Copies `config.json` to `config.json.backup-<timestamp>`; `None` when
    /// there is no file yet (the app runs on defaults).
    pub fn backup(&self) -> Result<Option<PathBuf>, ConfigError> {
        let path = self.path();
        if !path.exists() {
            return Ok(None);
        }
        let backup = self.stamped_path(BACKUP_PREFIX);
        fs::copy(&path, &backup)?;
        Ok(Some(backup))
    }

    fn bad_path(&self) -> PathBuf {
        self.stamped_path(BAD_PREFIX)
    }

    fn stamped_path(&self, prefix: &str) -> PathBuf {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut candidate = self.dir.join(format!("{prefix}{secs}"));
        let mut n = 1;
        while candidate.exists() {
            candidate = self.dir.join(format!("{prefix}{secs}-{n}"));
            n += 1;
        }
        candidate
    }
}

/// `freewheel-mappings.json`: the versioned layers, jog and override
/// bindings, without the app settings.
#[derive(Serialize)]
struct MappingsExport<'a> {
    version: u32,
    layers: &'a [Layer],
    base_layer: &'a LayerId,
    jog: &'a JogSettings,
    overrides: &'a OverrideSettings,
}

impl Config {
    /// The `freewheel-mappings.json` bytes for this config.
    pub fn export_mappings(&self) -> Vec<u8> {
        let export = MappingsExport {
            version: CONFIG_VERSION,
            layers: &self.layers,
            base_layer: &self.base_layer,
            jog: &self.jog,
            overrides: &self.overrides,
        };
        serde_json::to_vec_pretty(&export).expect("a config always serializes")
    }

    /// This config with its mappings replaced by a `freewheel-mappings.json`,
    /// validated as `save_config` does (T8). The file is untrusted (T1): every
    /// imported LaunchApp is disabled unless `allow_launch` is set.
    pub fn import_mappings(&self, bytes: &[u8], allow_launch: bool) -> Result<Config, SaveError> {
        let malformed = |message: String| SaveError::Malformed { message };
        let mut value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| malformed(e.to_string()))?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| malformed("not a JSON object".into()))?;
        if let Some(found) = object.get("version").and_then(serde_json::Value::as_u64)
            && found > u64::from(CONFIG_VERSION)
        {
            return Err(SaveError::VersionMismatch {
                found,
                expected: CONFIG_VERSION,
            });
        }
        if object.contains_key("settings") {
            return Err(SaveError::UnknownField {
                path: "settings".into(),
            });
        }
        let settings =
            serde_json::to_value(&self.settings).map_err(|e| malformed(e.to_string()))?;
        object.insert("settings".into(), settings);
        let mut config = parse_for_save(&value)?;
        if !allow_launch {
            config.disable_launch_apps();
        }
        Ok(config)
    }

    /// Turns off every LaunchApp, on keys and on the jog wheel.
    fn disable_launch_apps(&mut self) {
        let keys = self.layers.iter_mut().flat_map(|l| l.bindings.values_mut());
        let jog = self
            .jog
            .per_mode
            .values_mut()
            .flat_map(|b| [b.left.as_mut(), b.right.as_mut()])
            .flatten();
        for action in keys.chain(jog) {
            if let Action::LaunchApp { enabled, .. } = action {
                *enabled = false;
            }
        }
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
    let mut config: Config =
        serde_json::from_slice(bytes).map_err(|e| ConfigError::Malformed(e.to_string()))?;
    // An icon the bundled font lacks (a hand edit) is dropped: the profile
    // shows the plain icon and the next save stays valid.
    for p in &mut config.profiles {
        if p.icon
            .as_deref()
            .is_some_and(|i| crate::icons::icon_name(i) != Some(i))
        {
            p.icon = None;
        }
    }
    Ok(config)
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
    /// A mappings file (version 2) with a mistake: where it is (`at`, like
    /// "CAM1 on Main" or "wheel.speed") and what is wrong.
    InvalidFile {
        line: u32,
        column: u32,
        at: String,
        message: String,
    },
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
            SaveError::InvalidFile {
                line,
                column,
                at,
                message,
            } if at.is_empty() => write!(f, "line {line}, column {column}: {message}"),
            SaveError::InvalidFile {
                line,
                column,
                at,
                message,
            } => write!(f, "line {line}, column {column}, {at}: {message}"),
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
    validate_profiles(config)?;
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
            // A page key goes to a page of the same profile.
            if let Action::LayerPush { target } = action {
                let from = config.profile_of(&layer.id).map(|p| &p.id);
                if config.profile_of(target).map(|p| &p.id) != from {
                    return Err(SaveError::Malformed {
                        message: format!(
                            "layer {} key {key} goes to page {target} of another profile",
                            layer.id
                        ),
                    });
                }
            }
        }
    }
    for (mode, binding) in &config.jog.per_mode {
        for (side, action) in [("left", &binding.left), ("right", &binding.right)] {
            if let Some(action) = action {
                check_action(format!("jog {mode:?} {side}"), action)?;
                if let Action::LayerPush { target } = action
                    && config.profile_of(target).is_some()
                {
                    return Err(SaveError::Malformed {
                        message: format!("jog {mode:?} {side} goes to page {target} of a profile"),
                    });
                }
            }
        }
    }
    Ok(())
}

/// The profiles: unique ids and names, at least one page each, every page
/// in one profile at most (the Default profile's first page in none), and
/// every app named for some OS.
fn validate_profiles(config: &Config) -> Result<(), SaveError> {
    let bad = |message: String| Err(SaveError::Malformed { message });
    let mut seen_pages: Vec<&str> = Vec::new();
    for (i, p) in config.profiles.iter().enumerate() {
        if p.id.trim().is_empty() || p.name.trim().is_empty() {
            return bad(format!("profile {} needs an id and a name", i + 1));
        }
        let before = &config.profiles[..i];
        if before.iter().any(|o| o.id == p.id) {
            return bad(format!("the profile id {} is used twice", p.id));
        }
        if before
            .iter()
            .any(|o| o.name.trim().eq_ignore_ascii_case(p.name.trim()))
            || p.name.trim().eq_ignore_ascii_case(DEFAULT_PROFILE_NAME)
        {
            return bad(format!("the profile name {} is used twice", p.name));
        }
        if p.pages.is_empty() {
            return bad(format!("the profile {} has no page", p.name));
        }
        for page in &p.pages {
            if !config.layers.iter().any(|l| &l.id == page) {
                return Err(SaveError::BadLayerReference {
                    location: format!("profile {}", p.id),
                    target: page.clone(),
                });
            }
            if *page == config.base_layer || seen_pages.contains(&page.as_str()) {
                return bad(format!(
                    "page {page} is in more than one profile (or is the first page)"
                ));
            }
            seen_pages.push(page);
        }
        if let Some(c) = &p.color
            && !PROFILE_COLORS.contains(&c.as_str())
        {
            return bad(format!(
                "the profile {} has the colour {c}: pick one of {}",
                p.name,
                PROFILE_COLORS.join(", ")
            ));
        }
        if let Some(i) = &p.icon
            && crate::icons::icon_name(i) != Some(i.as_str())
        {
            return bad(format!(
                "the profile {} has the icon {i}: use a Material Symbols Rounded name, like edit ({})",
                p.name,
                crate::icons::ICON_CATALOGUE
            ));
        }
        for app in &p.apps {
            let named = [&app.windows, &app.macos, &app.linux]
                .into_iter()
                .any(|o| o.as_deref().is_some_and(|s| !s.trim().is_empty()));
            if !named {
                return bad(format!(
                    "the app \"{}\" of profile {} names no program for any OS",
                    app.name, p.name
                ));
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
                cwd: None,
                per_os: None,
            },
            Action::LaunchApp {
                path: "app.exe".into(),
                args: vec![],
                enabled: true,
                cwd: None,
                per_os: None,
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
            step_aside_for_resolve: false,
            detailed_logs: true,
        };
        roundtrip(&settings);
        for mode in [JogMode::Volume, JogMode::Brightness] {
            roundtrip(&Action::SetJogMode { mode });
        }
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
    fn step_aside_is_on_and_detailed_logs_off_by_default_and_both_persist() {
        let defaults = Settings::default();
        assert!(defaults.step_aside_for_resolve);
        assert!(!defaults.detailed_logs);
        // A config saved before these switches existed still loads, with
        // the old behaviour: step aside for Resolve, INFO logs.
        let old: Settings = serde_json::from_value(serde_json::json!({
            "enabled": true, "resolve_process_name": "Resolve", "low_battery_threshold": 15,
            "start_at_login": false, "appearance": "Dark", "auto_update": true
        }))
        .unwrap();
        assert!(old.step_aside_for_resolve);
        assert!(!old.detailed_logs);
        let (_dir, mut s) = store();
        let mut config = Config::default();
        config.settings.step_aside_for_resolve = false;
        config.settings.detailed_logs = true;
        s.save(&config).unwrap();
        let loaded = s.load().unwrap().settings;
        assert!(!loaded.step_aside_for_resolve);
        assert!(loaded.detailed_logs);
    }

    #[test]
    fn an_old_config_file_without_the_new_settings_loads_and_saves() {
        let (dir, mut s) = store();
        let mut old = serde_json::to_value(Config::default()).unwrap();
        let settings = old["settings"].as_object_mut().unwrap();
        settings.remove("step_aside_for_resolve");
        settings.remove("detailed_logs");
        fs::write(dir.path().join(CONFIG_FILE), old.to_string()).unwrap();
        assert_eq!(s.load().unwrap(), Config::default());
        // The settings window sends what it got; an old window's save works too.
        assert_eq!(parse_for_save(&old).unwrap(), Config::default());
    }

    #[test]
    fn volume_and_brightness_wheel_modes_drive_their_dial_controls() {
        assert_eq!(
            JogMode::Volume.override_mode(),
            Some(OverrideMode::SystemVolume)
        );
        assert_eq!(
            JogMode::Brightness.override_mode(),
            Some(OverrideMode::Brightness)
        );
        for mode in [JogMode::Jog, JogMode::Scroll, JogMode::Shuttle] {
            assert_eq!(mode.override_mode(), None);
        }
        assert_eq!(
            serde_json::to_value(JogMode::Volume).unwrap(),
            serde_json::json!("Volume")
        );
        let mut config = Config::default();
        config.jog.default_mode = JogMode::Brightness;
        config.layers[0].bindings.insert(
            "Cam1".into(),
            Action::SetJogMode {
                mode: JogMode::Volume,
            },
        );
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(parse_for_save(&value).unwrap(), config);
    }

    #[test]
    fn default_config_has_no_bindings_and_scrolls() {
        let c = Config::default();
        assert_eq!(c.version, 1);
        assert!(c.layers.iter().all(|l| l.bindings.is_empty()));
        assert_eq!(c.jog.default_mode, JogMode::Scroll);
        for mode in [JogMode::Shuttle, JogMode::Jog] {
            assert_eq!(c.jog.per_mode[&mode], c.jog.per_mode[&JogMode::Scroll]);
        }
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
    fn lone_modifier_held_and_latched_save_load_and_round_trip() {
        let (_dir, mut s) = store();
        let mut config = Config::default();
        for (key, m) in [("In", Modifier::Shift), ("Out", Modifier::Ctrl)] {
            let combo = KeyCombo {
                modifiers: vec![m],
                key: String::new(),
            };
            config
                .layers
                .first_mut()
                .unwrap()
                .bindings
                .insert(key.into(), Action::Held { combo });
        }
        let alt = KeyCombo {
            modifiers: vec![Modifier::Alt],
            key: String::new(),
        };
        config.layers[0]
            .bindings
            .insert("Snap".into(), Action::Latched { combo: alt });
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(parse_for_save(&value).unwrap(), config);
        s.save(&config).unwrap();
        assert_eq!(s.load().unwrap(), config);
        roundtrip(&config);
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

    fn with_launch_apps() -> Config {
        let launch = Action::LaunchApp {
            path: "/bin/app".into(),
            args: vec!["-x".into()],
            enabled: true,
            cwd: None,
            per_os: None,
        };
        let mut config = parse_for_save(&valid_json()).unwrap();
        config.layers[1]
            .bindings
            .insert("Snap".into(), launch.clone());
        config.jog.per_mode.get_mut(&JogMode::Scroll).unwrap().right = Some(launch);
        config.overrides.brightness_step = 9;
        config.settings.low_battery_threshold = 20;
        config
    }

    fn enabled_launch_apps(config: &Config) -> usize {
        let keys = config.layers.iter().flat_map(|l| l.bindings.values());
        let jog = config
            .jog
            .per_mode
            .values()
            .flat_map(|b| [b.left.as_ref(), b.right.as_ref()])
            .flatten();
        keys.chain(jog)
            .filter(|a| matches!(a, Action::LaunchApp { enabled: true, .. }))
            .count()
    }

    #[test]
    fn export_writes_the_versioned_mappings_without_the_settings() {
        let config = with_launch_apps();
        let value: serde_json::Value = serde_json::from_slice(&config.export_mappings()).unwrap();
        let keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        assert_eq!(
            keys,
            ["base_layer", "jog", "layers", "overrides", "version"]
        );
        assert_eq!(value["version"], CONFIG_VERSION);
        assert_eq!(value["overrides"]["brightness_step"], 9);
        assert_eq!(
            value["layers"][1]["bindings"]["Cut"]["Hotkey"]["combo"]["key"],
            "X"
        );
    }

    #[test]
    fn export_then_import_gives_an_identical_config() {
        let config = with_launch_apps();
        let back = Config::default()
            .import_mappings(&config.export_mappings(), true)
            .unwrap();
        // The app settings are not in the file: the importer keeps its own.
        assert_eq!(back.settings, Config::default().settings);
        assert_eq!(
            Config {
                settings: config.settings.clone(),
                ..back
            },
            config
        );
        assert_eq!(
            config
                .import_mappings(&config.export_mappings(), true)
                .unwrap(),
            config
        );
    }

    #[test]
    fn imported_launch_apps_stay_disabled_without_the_opt_in() {
        let config = with_launch_apps();
        assert_eq!(enabled_launch_apps(&config), 2);
        let imported = Config::default()
            .import_mappings(&config.export_mappings(), false)
            .unwrap();
        assert_eq!(enabled_launch_apps(&imported), 0);
        assert!(matches!(
            &imported.layers[1].bindings["Snap"],
            Action::LaunchApp { path, enabled: false, .. } if path == "/bin/app"
        ));
        let opted_in = Config::default()
            .import_mappings(&config.export_mappings(), true)
            .unwrap();
        assert_eq!(enabled_launch_apps(&opted_in), 2);
    }

    #[test]
    fn import_rejects_malformed_files() {
        let current = Config::default();
        for bytes in [&b"{not json"[..], b"[1, 2]", b"\"layers\"", b""] {
            assert!(
                matches!(
                    current.import_mappings(bytes, false),
                    Err(SaveError::Malformed { .. })
                ),
                "{}",
                String::from_utf8_lossy(bytes)
            );
        }
        let mut value: serde_json::Value =
            serde_json::from_slice(&with_launch_apps().export_mappings()).unwrap();
        value["layers"] = "none".into();
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            current.import_mappings(&bytes, false),
            Err(SaveError::Malformed { .. })
        ));
        let mut value: serde_json::Value =
            serde_json::from_slice(&current.export_mappings()).unwrap();
        value["settings"] = serde_json::to_value(Settings::default()).unwrap();
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            current.import_mappings(&bytes, false),
            Err(SaveError::UnknownField {
                path: "settings".into()
            })
        );
        value.as_object_mut().unwrap().remove("settings");
        value["base_layer"] = "gone".into();
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            current.import_mappings(&bytes, false),
            Err(SaveError::BadLayerReference { .. })
        ));
    }

    /// A backup keeps everything a user set up: page names and order, every
    /// kind of key, the wheel modes and the dial steps.
    #[test]
    fn a_backup_restores_every_page_key_wheel_mode_and_dial_step() {
        let mut config = with_launch_apps();
        config.layers[1].name = "Edit Bay (renamed)".into();
        let base = &mut config.layers[0].bindings;
        let combo = |modifiers: Vec<Modifier>, key: &str| KeyCombo {
            modifiers,
            key: key.into(),
        };
        base.insert(
            "Trans".into(),
            Action::Held {
                combo: combo(vec![Modifier::Shift], ""),
            },
        );
        base.insert(
            "Snap".into(),
            Action::Latched {
                combo: combo(vec![Modifier::Alt], ""),
            },
        );
        base.insert(
            "StopPlay".into(),
            Action::Hotkey {
                combo: combo(vec![], "PlayPause"),
            },
        );
        base.insert(
            "Shtl".into(),
            Action::SetJogMode {
                mode: JogMode::Volume,
            },
        );
        base.insert(
            "FullView".into(),
            Action::OverrideToggle {
                mode: OverrideMode::Brightness,
            },
        );
        config.jog.default_mode = JogMode::Jog;
        if let Some(jog) = config.jog.per_mode.get_mut(&JogMode::Jog) {
            jog.threshold = 7;
        }
        config.overrides.brightness_step = 3;
        config.overrides.system_volume_step = 0.07;
        let file = config.export_mappings();
        let restored = Config::default().import_mappings(&file, true).unwrap();
        assert_eq!(restored.layers, config.layers);
        assert_eq!(restored.base_layer, config.base_layer);
        assert_eq!(restored.jog, config.jog);
        assert_eq!(restored.overrides, config.overrides);
        // The file itself carries the renamed page and every binding.
        let text = String::from_utf8(file).unwrap();
        for part in [
            "Edit Bay (renamed)",
            "PlayPause",
            "Volume",
            "Latched",
            "Held",
        ] {
            assert!(text.contains(part), "{part}");
        }
    }

    /// Someone else's file never brings a key that runs a program unless
    /// the user lets it, on pages and on the wheel alike.
    #[test]
    fn someone_elses_launch_keys_and_wheel_actions_come_in_off() {
        let mut theirs = with_launch_apps();
        let launch = Action::LaunchApp {
            path: "C:/Their/Tool.exe".into(),
            args: vec!["--x".into()],
            enabled: true,
            cwd: None,
            per_os: None,
        };
        if let Some(jog) = theirs.jog.per_mode.values_mut().next() {
            jog.left = Some(launch.clone());
        }
        let file = theirs.export_mappings();
        let off = Config::default().import_mappings(&file, false).unwrap();
        assert_eq!(enabled_launch_apps(&off), 0);
        let on = Config::default().import_mappings(&file, true).unwrap();
        assert_eq!(enabled_launch_apps(&on), enabled_launch_apps(&theirs));
        // Turning them off changes nothing else.
        let mut expected = theirs.clone();
        expected.disable_launch_apps();
        assert_eq!(off.layers, expected.layers);
        assert_eq!(off.jog, expected.jog);
    }

    #[test]
    fn import_rejects_unknown_keys_fields_and_leaves_the_current_config_alone() {
        let current = with_launch_apps();
        let before = current.clone();
        let mut value: serde_json::Value =
            serde_json::from_slice(&current.export_mappings()).unwrap();
        value["layers"][0]["bindings"]["NoSuchKey"] =
            serde_json::json!({"Hotkey": {"combo": {"modifiers": [], "key": "A"}}});
        assert!(matches!(
            current.import_mappings(&serde_json::to_vec(&value).unwrap(), true),
            Err(SaveError::UnknownKey { .. })
        ));
        let mut value: serde_json::Value =
            serde_json::from_slice(&current.export_mappings()).unwrap();
        value["surprise"] = true.into();
        assert!(matches!(
            current.import_mappings(&serde_json::to_vec(&value).unwrap(), true),
            Err(SaveError::UnknownField { .. })
        ));
        let mut value: serde_json::Value =
            serde_json::from_slice(&current.export_mappings()).unwrap();
        value["layers"][0]["bindings"]["Cut"] =
            serde_json::json!({"LayerPush": {"target": "gone"}});
        assert!(matches!(
            current.import_mappings(&serde_json::to_vec(&value).unwrap(), true),
            Err(SaveError::BadLayerReference { .. })
        ));
        assert_eq!(current, before);
    }

    #[test]
    fn import_rejects_a_newer_version_before_reading_the_rest() {
        let bytes = br#"{"version": 2, "layers": "from the future"}"#;
        assert_eq!(
            Config::default().import_mappings(bytes, false),
            Err(SaveError::VersionMismatch {
                found: 2,
                expected: CONFIG_VERSION
            })
        );
    }

    #[test]
    fn backup_copies_config_json_and_keeps_it() {
        let (dir, mut s) = store();
        assert!(s.backup().unwrap().is_none());
        let config = with_launch_apps();
        s.save(&config).unwrap();
        let backup = s.backup().unwrap().unwrap();
        let name = backup.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with(BACKUP_PREFIX), "{name}");
        assert_eq!(fs::read(&backup).unwrap(), fs::read(s.path()).unwrap());
        assert_eq!(entries(dir.path()), vec![CONFIG_FILE.to_string(), name]);
        // A second backup in the same second gets its own name.
        assert_ne!(s.backup().unwrap().unwrap(), backup);
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

    /// The Linux update e2e's config (scripts/e2e-update-linux.mjs) is a
    /// valid config with keys and a second page, so its byte-for-byte check
    /// proves an update keeps a real setup.
    #[test]
    fn the_update_e2e_config_is_a_valid_setup() {
        let json = include_str!("../../../scripts/fixtures/update-e2e-config.json");
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.layers.len(), 2);
        assert!(!config.settings.auto_update);
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn profiles_are_checked_on_save_and_left_out_when_there_are_none() {
        let page = |id: &str, b: &[(&str, &str)]| Layer {
            id: id.into(),
            name: id.into(),
            bindings: b
                .iter()
                .map(|(k, t)| {
                    (
                        k.to_string(),
                        Action::LayerPush {
                            target: t.to_string(),
                        },
                    )
                })
                .collect(),
        };
        let mut config = Config::default();
        config.layers.push(page("music", &[]));
        config.layers.push(page("draft", &[("Cam1", "review")]));
        config.layers.push(page("review", &[]));
        config.profiles.push(Profile {
            id: "writing".into(),
            name: "Writing".into(),
            apps: vec![ProfileApp {
                name: "Notepad".into(),
                windows: Some("notepad.exe".into()),
                macos: None,
                linux: None,
            }],
            pages: vec!["draft".into(), "review".into()],
            color: None,
            icon: None,
        });
        validate(&config).unwrap();
        assert_eq!(
            config.profile_of("review").map(|p| p.id.as_str()),
            Some("writing")
        );
        let default: Vec<&str> = config
            .pages_of(None)
            .iter()
            .map(|l| l.id.as_str())
            .collect();
        assert_eq!(default, ["base", "music"]);
        // config.json round-trips through the settings window's save.
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(parse_for_save(&value).unwrap(), config);
        let bad = |f: &dyn Fn(&mut Config)| {
            let mut c = config.clone();
            f(&mut c);
            validate(&c).unwrap_err().to_string()
        };
        // A page key to another profile's page.
        assert!(
            bad(&|c| {
                c.layers[0].bindings.insert(
                    "Cam2".into(),
                    Action::LayerPush {
                        target: "draft".into(),
                    },
                );
            })
            .contains("another profile")
        );
        assert!(
            bad(&|c| c.profiles[0].pages.push("draft".into())).contains("more than one profile")
        );
        assert!(
            bad(&|c| c.profiles[0].pages.push("base".into())).contains("more than one profile")
        );
        assert!(bad(&|c| c.profiles[0].pages.clear()).contains("no page"));
        assert!(bad(&|c| c.profiles[0].name = "default".into()).contains("used twice"));
        assert!(bad(&|c| c.profiles[0].apps[0].windows = None).contains("names no program"));
        assert!(bad(&|c| c.profiles[0].color = Some("pink".into())).contains("colour pink"));
        assert!(bad(&|c| c.profiles[0].icon = Some("star_wars".into())).contains("icon star_wars"));
        // Stored bare: the editor strips the colons before saving.
        assert!(bad(&|c| c.profiles[0].icon = Some(":edit:".into())).contains("icon :edit:"));
        let mut any = config.clone();
        any.profiles[0].icon = Some("sports_esports".into());
        validate(&any).unwrap();
        // config.json with an icon the font lacks loads, with the plain icon.
        any.profiles[0].icon = Some("star_wars".into());
        let loaded = parse(&serde_json::to_vec(&any).unwrap()).unwrap();
        assert_eq!(loaded.profiles[0].icon, None);
        validate(&loaded).unwrap();
        assert!(bad(&|c| c.profiles[0].pages.push("gone".into())).contains("gone"));
        // No profiles: config.json is as 1.0.0 wrote it.
        let plain = serde_json::to_value(Config::default()).unwrap();
        assert!(plain.get("profiles").is_none());
    }
}
