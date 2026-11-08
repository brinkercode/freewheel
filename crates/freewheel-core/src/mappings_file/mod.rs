//! `freewheel-mappings.json`, version 2: the short, hand-editable form of
//! the keys, pages, wheel and dial (SCHEMA.md is its reference, and
//! schema/freewheel-mappings.schema.json its JSON Schema).
//!
//! ```json
//! {
//!   "$schema": "https://raw.githubusercontent.com/brinkercode/freewheel/main/schema/freewheel-mappings.schema.json",
//!   "version": 2,
//!   "pages": {
//!     "Main": { "CUT": "Ctrl+Z", "CAM1": "open Spotify", "ESC": "page Music" },
//!     "Music": { "ESC": "back" }
//!   },
//!   "wheel": { "mode": "scroll", "speed": 5 }
//! }
//! ```
//!
//! Export always writes version 2. Import also reads the long version-1
//! files earlier builds wrote ([`Config::import_mappings`]). Errors name the
//! line, column and key ([`SaveError::InvalidFile`]).

pub mod json;
pub mod names;
pub mod open;
pub mod schema;

use std::collections::BTreeMap;

use crate::config::{
    Action, Appearance, CONFIG_VERSION, Config, DEFAULT_PROFILE_NAME, JogBinding, JogMode,
    JogSettings, KeyCombo, Layer, OverrideMode, OverrideSettings, PROFILE_COLORS, Profile,
    ProfileApp, SaveError,
};
use json::{Member, Node, Pos, Value};
use names::{closest, device_key, format_combo, key_id, parse_combo, printed_name};
pub use open::{Host, InstalledApp, Os, RealHost};
use open::{OpenSpec, Target};

/// The mappings file version export writes.
pub const MAPPINGS_VERSION: u32 = 2;
/// Where the JSON Schema is published; export writes it as `$schema`.
pub const SCHEMA_URL: &str = "https://raw.githubusercontent.com/brinkercode/freewheel/main/schema/freewheel-mappings.schema.json";

/// The fastest wheel speed: one wheel tick per step.
pub const MAX_SPEED: u32 = 10;

/// A field of the file, for errors, the schema and SCHEMA.md.
pub struct Field {
    pub path: &'static str,
    pub kind: &'static str,
    pub default: &'static str,
    pub range: &'static str,
    pub about: &'static str,
}

/// Every field the file reads, in SCHEMA.md order. `<mode>` is one of
/// [`WHEEL_MODES`].
pub const FIELDS: &[Field] = &[
    Field {
        path: "$schema",
        kind: "text",
        default: "(none)",
        range: "any URL",
        about: "Where editors find the JSON Schema; freewheel ignores it.",
    },
    Field {
        path: "version",
        kind: "number",
        default: "2",
        range: "1 or 2",
        about: "The file format. 1 is the long form earlier versions wrote; it still reads.",
    },
    Field {
        path: "pages",
        kind: "object",
        default: "one empty page \"Base\"",
        range: "1 or more pages",
        about: "Page name to its keys. The first page is where the Speed Editor starts.",
    },
    Field {
        path: "pages.<page>",
        kind: "object",
        default: "{}",
        range: "key name to action",
        about: "Key name (see Key names) to what the key does (see Actions). Keys left out are not set.",
    },
    Field {
        path: "profiles",
        kind: "list",
        default: "[] (none)",
        range: "profiles, each with a name, apps and pages",
        about: "App profiles: each has its own pages and is used while one of its apps is in front. The pages above are the Default profile, used while no profile's app is.",
    },
    Field {
        path: "profiles[].name",
        kind: "text",
        default: "(required)",
        range: "unique, not Default",
        about: "The profile's name.",
    },
    Field {
        path: "profiles[].color",
        kind: "text",
        default: "(none: the app's accent)",
        range: "rose, orange, amber, green, teal, blue, indigo, violet",
        about: "The profile's colour in the Keys tab.",
    },
    Field {
        path: "profiles[].icon",
        kind: "text",
        default: "(none: a plain profile icon)",
        range: "a Material Symbols Rounded icon name, like :edit: or :sports_esports: (see Profile icons)",
        about: "The profile's icon in the Keys tab, by its name in Google's icon catalogue (https://fonts.google.com/icons?icon.style=Rounded). Written :name: or name.",
    },
    Field {
        path: "profiles[].apps",
        kind: "list",
        default: "[] (switched to by no app)",
        range: "app names, or { \"name\", \"windows\", \"macos\", \"linux\" } objects",
        about: "The apps the profile is for (see App profiles).",
    },
    Field {
        path: "profiles[].pages",
        kind: "object",
        default: "(required)",
        range: "1 or more pages",
        about: "The profile's pages, written as pages is. The first page is where the profile starts. A page key goes to a page of the same profile.",
    },
    Field {
        path: "wheel",
        kind: "object",
        default: "scroll mode, speed 10",
        range: "",
        about: "What turning the wheel does.",
    },
    Field {
        path: "wheel.mode",
        kind: "text",
        default: "\"scroll\"",
        range: "jog, scroll, shuttle, volume, brightness",
        about: "The wheel button lit when freewheel starts. volume and brightness turn the wheel into that knob.",
    },
    Field {
        path: "wheel.speed",
        kind: "number",
        default: "10",
        range: "1 to 10",
        about: "How far the wheel turns per step, for every mode: 10 steps on every tick, 1 needs ten ticks.",
    },
    Field {
        path: "wheel.<mode>",
        kind: "object",
        default: "left \"scroll up\", right \"scroll down\"",
        range: "",
        about: "What turning the wheel does while <mode>'s button is lit.",
    },
    Field {
        path: "wheel.<mode>.left",
        kind: "action",
        default: "\"scroll up\"",
        range: "any action except back",
        about: "What one step to the left does.",
    },
    Field {
        path: "wheel.<mode>.right",
        kind: "action",
        default: "\"scroll down\"",
        range: "any action except back",
        about: "What one step to the right does.",
    },
    Field {
        path: "wheel.<mode>.speed",
        kind: "number",
        default: "wheel.speed",
        range: "1 to 10",
        about: "This mode's speed, instead of wheel.speed.",
    },
    Field {
        path: "wheel.<mode>.max_rate",
        kind: "number or null",
        default: "null (no limit)",
        range: "1 to 1000",
        about: "At most this many steps a second (useful for shuttle).",
    },
    Field {
        path: "dial",
        kind: "object",
        default: "2, 2, 5",
        range: "",
        about: "How much one wheel step changes while the wheel is a knob.",
    },
    Field {
        path: "dial.volume_step",
        kind: "number",
        default: "2",
        range: "0.1 to 100",
        about: "Percent of system volume per step.",
    },
    Field {
        path: "dial.app_volume_step",
        kind: "number",
        default: "2",
        range: "0.1 to 100",
        about: "Percent of the front app's volume per step.",
    },
    Field {
        path: "dial.brightness_step",
        kind: "number",
        default: "5",
        range: "1 to 100",
        about: "Percent of display brightness per step.",
    },
    Field {
        path: "settings",
        kind: "object",
        default: "(your current settings)",
        range: "",
        about: "Optional app settings. Restore changes only the ones written; export never writes them.",
    },
    Field {
        path: "settings.appearance",
        kind: "text",
        default: "\"system\"",
        range: "system, light, dark",
        about: "The settings window's colours.",
    },
    Field {
        path: "settings.update_automatically",
        kind: "true/false",
        default: "true",
        range: "",
        about: "Download new versions by themselves.",
    },
    Field {
        path: "settings.step_aside_for_resolve",
        kind: "true/false",
        default: "true",
        range: "",
        about: "Turn freewheel off while DaVinci Resolve is open.",
    },
    Field {
        path: "settings.resolve_program",
        kind: "text",
        default: "\"Resolve.exe\" (Windows), \"Resolve\" (macOS)",
        range: "not empty",
        about: "The program name freewheel watches for.",
    },
    Field {
        path: "settings.low_battery_warning",
        kind: "number",
        default: "15",
        range: "0 to 100",
        about: "Warn when the Speed Editor's battery is at or below this percent.",
    },
    Field {
        path: "settings.detailed_logs",
        kind: "true/false",
        default: "false",
        range: "",
        about: "Detailed logs for troubleshooting (they turn off after a day).",
    },
];

/// The wheel modes, as a file names them.
pub const WHEEL_MODES: [(JogMode, &str); 5] = [
    (JogMode::Jog, "jog"),
    (JogMode::Scroll, "scroll"),
    (JogMode::Shuttle, "shuttle"),
    (JogMode::Volume, "volume"),
    (JogMode::Brightness, "brightness"),
];

/// The knobs `wheel ...` turns the wheel into.
pub const KNOBS: [(OverrideMode, &str); 3] = [
    (OverrideMode::SystemVolume, "volume"),
    (OverrideMode::AppVolume, "app volume"),
    (OverrideMode::Brightness, "brightness"),
];

/// One action form, for SCHEMA.md and the schema's descriptions.
pub struct ActionForm {
    pub form: &'static str,
    pub example: &'static str,
    pub does: &'static str,
}

/// Every action a key or wheel step can have.
pub const ACTIONS: &[ActionForm] = &[
    ActionForm {
        form: "<shortcut>",
        example: "Ctrl+Shift+Z",
        does: "Presses the shortcut: modifiers and one key joined by +, or a media key like Play/Pause.",
    },
    ActionForm {
        form: "hold <shortcut>",
        example: "hold Shift",
        does: "Holds the shortcut down while the key is held. A single modifier alone is allowed.",
    },
    ActionForm {
        form: "toggle <shortcut>",
        example: "toggle Alt",
        does: "The first press holds the shortcut down, the next press lets it go (the key's light shows it).",
    },
    ActionForm {
        form: "open <app or path>",
        example: "open Spotify",
        does: "Opens an app by its name, a program, a file or a folder (see How open finds the app).",
    },
    ActionForm {
        form: "<web address>",
        example: "https://youtube.com",
        does: "Opens the page in your web browser. It must start with https:// or http://.",
    },
    ActionForm {
        form: "page <name>",
        example: "page Music",
        does: "Goes to another page. On that page, the same key goes back.",
    },
    ActionForm {
        form: "back",
        example: "back",
        does: "Says the key goes back: allowed only on the key that opened this page from another page. It is the same as leaving the key out.",
    },
    ActionForm {
        form: "scroll <n>",
        example: "scroll -3",
        does: "Scrolls n lines (negative: up). scroll up and scroll down are -1 and 1. n is -100 to 100, not 0.",
    },
    ActionForm {
        form: "wheel volume",
        example: "wheel volume",
        does: "Turns the wheel into a volume knob until you press the key again.",
    },
    ActionForm {
        form: "wheel app volume",
        example: "wheel app volume",
        does: "Turns the wheel into a volume knob for the app in front.",
    },
    ActionForm {
        form: "wheel brightness",
        example: "wheel brightness",
        does: "Turns the wheel into a brightness knob.",
    },
    ActionForm {
        form: "wheel mode <mode>",
        example: "wheel mode jog",
        does: "Switches the wheel to a mode, as pressing SHTL, JOG or SCRL does: jog, scroll, shuttle, volume or brightness.",
    },
    ActionForm {
        form: "nothing",
        example: "nothing",
        does: "The key does nothing (the same as leaving it out). SHTL, JOG and SCRL left unset switch the wheel to their mode.",
    },
    ActionForm {
        form: "{ \"open\": ..., \"args\": [...], \"cwd\": ... }",
        example: "{ \"open\": \"C:\\\\Tools\\\\run.cmd\", \"args\": [\"--fast\", \"file.txt\"], \"cwd\": \"%USERPROFILE%\" }",
        does: "open with arguments and a start folder. open may also be { \"windows\": ..., \"macos\": ..., \"linux\": ... } for one file shared by several computers; a backup writes every OS back.",
    },
];

/// The words that start an action, for "did you mean" suggestions.
const VERBS: [&str; 8] = [
    "open", "hold", "toggle", "page", "scroll", "wheel", "nothing", "back",
];

/// The result of an import: the new config and notes for the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub config: Config,
    pub notes: Vec<String>,
}

fn invalid(pos: Pos, at: impl Into<String>, message: impl Into<String>) -> SaveError {
    SaveError::InvalidFile {
        line: pos.line,
        column: pos.column,
        at: at.into(),
        message: message.into(),
    }
}

/// Reads a mappings file (version 2, or the long version 1) into a copy of
/// `current` with its keys, pages, wheel and dial replaced. Every `open`
/// and web address comes in off unless `allow_launch` (T1).
pub fn import(
    current: &Config,
    bytes: &[u8],
    allow_launch: bool,
    host: &dyn Host,
) -> Result<Imported, SaveError> {
    let text = std::str::from_utf8(bytes).map_err(|_| SaveError::Malformed {
        message: "the file is not text (UTF-8)".into(),
    })?;
    let root = json::parse(text).map_err(|e| SaveError::Malformed {
        message: e.to_string(),
    })?;
    let Value::Object(members) = &root.value else {
        return Err(SaveError::Malformed {
            message: format!(
                "line {}, column {}: the file must be one {{ ... }} object, not {}",
                root.pos.line,
                root.pos.column,
                root.value.kind()
            ),
        });
    };
    let get = |name: &str| members.iter().find(|m| m.key == name);
    let version = match get("version") {
        None if get("layers").is_some() || get("base_layer").is_some() => 1,
        None => MAPPINGS_VERSION as u64,
        Some(m) => match m.node.value {
            Value::Number(n) if n >= 0.0 && n.fract() == 0.0 => n as u64,
            _ => {
                return Err(invalid(
                    m.node.pos,
                    "version",
                    format!("version must be 2, not {}", m.node.value.kind()),
                ));
            }
        },
    };
    match version {
        v if v > u64::from(MAPPINGS_VERSION) => Err(SaveError::VersionMismatch {
            found: v,
            expected: MAPPINGS_VERSION,
        }),
        v if v == u64::from(CONFIG_VERSION) => {
            let config = current.import_mappings(bytes, allow_launch)?;
            Ok(Imported {
                config,
                notes: Vec::new(),
            })
        }
        2 => Reader::new(host).file(current, members, allow_launch),
        v => Err(invalid(
            get("version").map_or(root.pos, |m| m.node.pos),
            "version",
            format!("version {v} does not exist: write 2"),
        )),
    }
}

/// Checks an object's names: each known, none twice (case-insensitively).
fn check_fields(members: &[Member], known: &[&str], at: &str) -> Result<(), SaveError> {
    for (i, m) in members.iter().enumerate() {
        if !known.contains(&m.key.as_str()) {
            let hint = closest(&m.key, known.iter().copied())
                .map(|k| format!(" (did you mean \"{k}\"?)"))
                .unwrap_or_else(|| format!(" (it can have: {})", known.join(", ")));
            return Err(invalid(
                m.key_pos,
                at,
                format!("unknown field \"{}\"{hint}", m.key),
            ));
        }
        if members[..i].iter().any(|o| o.key == m.key) {
            return Err(invalid(
                m.key_pos,
                at,
                format!("\"{}\" is in the file twice", m.key),
            ));
        }
    }
    Ok(())
}

fn fields_of(prefix: &str) -> Vec<&'static str> {
    FIELDS
        .iter()
        .filter_map(|f| f.path.strip_prefix(prefix))
        .filter(|rest| !rest.contains('.') && !rest.starts_with('<'))
        .collect()
}

fn object<'a>(node: &'a Node, at: &str) -> Result<&'a [Member], SaveError> {
    match &node.value {
        Value::Object(m) => Ok(m),
        other => Err(invalid(
            node.pos,
            at,
            format!("{at} must be an object {{ ... }}, not {}", other.kind()),
        )),
    }
}

fn number(node: &Node, at: &str, min: f64, max: f64, whole: bool) -> Result<f64, SaveError> {
    let range = |n: f64| {
        if whole {
            format!("{at} must be a whole number from {min} to {max}, not {n}")
        } else {
            format!("{at} must be a number from {min} to {max}, not {n}")
        }
    };
    match node.value {
        Value::Number(n) if n >= min && n <= max && (!whole || n.fract() == 0.0) => Ok(n),
        Value::Number(n) => Err(invalid(node.pos, at, range(n))),
        ref other => Err(invalid(
            node.pos,
            at,
            format!("{at} must be a number, not {}", other.kind()),
        )),
    }
}

fn boolean(node: &Node, at: &str) -> Result<bool, SaveError> {
    match node.value {
        Value::Bool(b) => Ok(b),
        ref other => Err(invalid(
            node.pos,
            at,
            format!("{at} must be true or false, not {}", other.kind()),
        )),
    }
}

fn text<'a>(node: &'a Node, at: &str) -> Result<&'a str, SaveError> {
    match &node.value {
        Value::String(s) => Ok(s),
        other => Err(invalid(
            node.pos,
            at,
            format!("{at} must be text in double quotes, not {}", other.kind()),
        )),
    }
}

fn one_of<T: Copy>(node: &Node, at: &str, table: &[(T, &'static str)]) -> Result<T, SaveError> {
    let word = text(node, at)?;
    let names: Vec<&str> = table.iter().map(|(_, n)| *n).collect();
    table
        .iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(word.trim()))
        .map(|(v, _)| *v)
        .ok_or_else(|| {
            let hint = closest(word, names.iter().copied())
                .map(|k| format!(" (did you mean \"{k}\"?)"))
                .unwrap_or_default();
            invalid(
                node.pos,
                at,
                format!(
                    "{at} must be one of {}, not \"{word}\"{hint}",
                    names.join(", ")
                ),
            )
        })
}

/// A page key or wheel step's action, before page names are known.
enum Parsed {
    Action(Action),
    Nothing,
    Back,
    Page(String, Pos),
    Open(OpenSpec),
}

struct Reader<'a> {
    host: &'a dyn Host,
    notes: Vec<String>,
}

impl<'a> Reader<'a> {
    fn new(host: &'a dyn Host) -> Self {
        Reader {
            host,
            notes: Vec::new(),
        }
    }

    fn file(
        mut self,
        current: &Config,
        members: &[Member],
        allow_launch: bool,
    ) -> Result<Imported, SaveError> {
        check_fields(members, &fields_of(""), "the file")?;
        let get = |name: &str| members.iter().find(|m| m.key == name).map(|m| &m.node);
        if let Some(schema) = get("$schema") {
            text(schema, "$schema")?;
        }
        let mut config = Config {
            version: CONFIG_VERSION,
            settings: current.settings.clone(),
            ..Config::default()
        };
        if let Some(pages) = get("pages") {
            let layers = self.pages(
                pages,
                "pages",
                &current.pages_of(None),
                &[],
                Some("base"),
                "",
            )?;
            config.base_layer = layers[0].id.clone();
            config.layers = layers;
        }
        // The wheel's page actions go to the Default profile's pages.
        let ids: BTreeMap<String, String> = config
            .layers
            .iter()
            .map(|l| (l.name.to_lowercase(), l.id.clone()))
            .collect();
        if let Some(profiles) = get("profiles") {
            config.profiles = self.profiles(profiles, current, &mut config.layers)?;
        }
        if let Some(wheel) = get("wheel") {
            config.jog = self.wheel(wheel, &ids)?;
        }
        if let Some(dial) = get("dial") {
            config.overrides = dial_settings(dial)?;
        }
        if let Some(settings) = get("settings") {
            apply_settings(&mut config.settings, settings)?;
        }
        crate::config::validate(&config)?;
        let mut notes = self.notes;
        if !allow_launch {
            let turned_off = disable_launch_apps(&mut config);
            if turned_off > 0 {
                notes.push(format!(
                    "{turned_off} {} that open{} apps or web pages came in off: turn on “Let restored keys open apps” and restore again to keep them on.",
                    if turned_off == 1 { "key" } else { "keys" },
                    if turned_off == 1 { "s" } else { "" },
                ));
            }
        }
        Ok(Imported { config, notes })
    }

    /// One set of pages (`pages`, or a profile's): `current` are the pages
    /// whose ids a page of the same name keeps, `taken` the ids already
    /// used, `first` the id of the first page (Default's is "base"), and
    /// new ids start with `prefix`.
    fn pages(
        &mut self,
        node: &Node,
        at_pages: &str,
        current: &[&Layer],
        taken: &[Layer],
        first: Option<&str>,
        prefix: &str,
    ) -> Result<Vec<Layer>, SaveError> {
        let pages = object(node, at_pages)?;
        if pages.is_empty() {
            return Err(invalid(
                node.pos,
                at_pages,
                format!("{at_pages} is empty: write at least one page, like \"Main\": {{}}"),
            ));
        }
        // Ids: the current page of the same name keeps its id (so a restore
        // on the same computer changes nothing); the first page is "base".
        let mut layers: Vec<Layer> = Vec::new();
        for (i, page) in pages.iter().enumerate() {
            let name = page.key.trim();
            if name.is_empty() {
                return Err(invalid(page.key_pos, at_pages, "a page needs a name"));
            }
            if let Some(other) = layers.iter().find(|l| l.name.eq_ignore_ascii_case(name)) {
                return Err(invalid(
                    page.key_pos,
                    at_pages,
                    format!(
                        "the page \"{name}\" is in the file twice (\"{}\" before it; names ignore case)",
                        other.name
                    ),
                ));
            }
            let used = |id: &str| layers.iter().chain(taken).any(|l| l.id == id);
            let id = current
                .iter()
                .find(|l| l.name.eq_ignore_ascii_case(name) && !used(&l.id))
                .map(|l| l.id.clone())
                .or_else(|| first.filter(|f| i == 0 && !used(f)).map(str::to_string))
                .unwrap_or_else(|| {
                    let slug = format!("{prefix}{}", slug(name));
                    let mut id = slug.clone();
                    let mut n = 2;
                    while used(&id) || id == "base" {
                        id = format!("{slug}-{n}");
                        n += 1;
                    }
                    id
                });
            layers.push(Layer {
                id,
                name: name.to_string(),
                bindings: BTreeMap::new(),
            });
        }
        let ids: BTreeMap<String, String> = layers
            .iter()
            .map(|l| (l.name.to_lowercase(), l.id.clone()))
            .collect();
        let mut backs: Vec<(usize, String, Pos, String)> = Vec::new();
        for (i, page) in pages.iter().enumerate() {
            let page_name = layers[i].name.clone();
            let keys = object(&page.node, &format!("the page {page_name}"))?;
            let mut seen: Vec<(crate::report::Key, &str)> = Vec::new();
            for m in keys {
                let Some(key) = device_key(&m.key) else {
                    let all = names::DEVICE_KEYS.iter().map(|(_, n)| *n);
                    let hint = closest(&m.key, all)
                        .map(|k| format!(" (did you mean \"{k}\"?)"))
                        .unwrap_or_else(|| " (see SCHEMA.md for the key names)".into());
                    return Err(invalid(
                        m.key_pos,
                        format!("page {page_name}"),
                        format!("\"{}\" is not a Speed Editor key{hint}", m.key),
                    ));
                };
                let printed = printed_name(&key_id(key)).unwrap_or_default();
                if let Some((_, first)) = seen.iter().find(|(k, _)| *k == key) {
                    return Err(invalid(
                        m.key_pos,
                        format!("page {page_name}"),
                        format!(
                            "{printed} is on this page twice (as \"{first}\" and \"{}\")",
                            m.key
                        ),
                    ));
                }
                seen.push((key, &m.key));
                let at = format!("{printed} on {page_name}");
                let action = match self.action(&m.node, &at, true)? {
                    Parsed::Action(a) => Some(a),
                    Parsed::Nothing => None,
                    Parsed::Back => {
                        backs.push((i, key_id(key), m.node.pos, at));
                        None
                    }
                    Parsed::Page(target, pos) => Some(Action::LayerPush {
                        target: page_id(&ids, &target, pos, &at)?,
                    }),
                    Parsed::Open(spec) => Some(self.open(&spec, &at)),
                };
                if let Some(action) = action {
                    layers[i].bindings.insert(key_id(key), action);
                }
            }
        }
        for (i, key, pos, at) in backs {
            let id = &layers[i].id;
            let opener = layers.iter().find(|l| {
                &l.id != id && matches!(l.bindings.get(&key), Some(Action::LayerPush { target }) if target == id)
            });
            if opener.is_none() {
                let printed = printed_name(&key).unwrap_or_default();
                return Err(invalid(
                    pos,
                    at,
                    format!(
                        "\"back\" works only on the key that opens this page: write \"{printed}\": \"page {}\" on another page, or pick another action",
                        layers[i].name
                    ),
                ));
            }
        }
        Ok(layers)
    }

    /// `profiles`: each with its name, apps and pages; their pages join `layers`.
    fn profiles(
        &mut self,
        node: &Node,
        current: &Config,
        layers: &mut Vec<Layer>,
    ) -> Result<Vec<Profile>, SaveError> {
        let Value::Array(items) = &node.value else {
            return Err(invalid(
                node.pos,
                "profiles",
                format!(
                    "profiles must be a list [ ... ] of profiles, not {}",
                    node.value.kind()
                ),
            ));
        };
        let mut out: Vec<Profile> = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let at = format!("profile {}", i + 1);
            let members = object(item, &at)?;
            check_fields(members, &fields_of("profiles[]."), &at)?;
            let get = |n: &str| members.iter().find(|m| m.key == n).map(|m| &m.node);
            let Some(name_node) = get("name") else {
                return Err(invalid(item.pos, &at, "a profile needs a \"name\""));
            };
            let name = text(name_node, &format!("{at} name"))?.trim().to_string();
            if name.is_empty() {
                return Err(invalid(name_node.pos, &at, "a profile needs a name"));
            }
            if name.eq_ignore_ascii_case(DEFAULT_PROFILE_NAME)
                || out.iter().any(|p| p.name.eq_ignore_ascii_case(&name))
            {
                return Err(invalid(
                    name_node.pos,
                    &at,
                    format!(
                        "the profile name \"{name}\" is taken (names ignore case; Default is the pages above)"
                    ),
                ));
            }
            let at = format!("the profile {name}");
            let id = current
                .profiles
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&name) && !out.iter().any(|o| o.id == p.id))
                .map(|p| p.id.clone())
                .unwrap_or_else(|| {
                    let base = format!("profile-{}", slug(&name));
                    let mut id = base.clone();
                    let mut n = 2;
                    while out.iter().any(|o| o.id == id) {
                        id = format!("{base}-{n}");
                        n += 1;
                    }
                    id
                });
            let apps = match get("apps") {
                None => Vec::new(),
                Some(n) => profile_apps(n, &at)?,
            };
            let Some(pages_node) = get("pages") else {
                return Err(invalid(
                    item.pos,
                    &at,
                    "a profile needs \"pages\", like \"pages\": { \"Main\": {} }",
                ));
            };
            let before = current.pages_of(Some(&id));
            let before: Vec<&Layer> = if current.profiles.iter().any(|p| p.id == id) {
                before
            } else {
                Vec::new()
            };
            let pages = self.pages(
                pages_node,
                &format!("{at}: pages"),
                &before,
                layers,
                None,
                &format!("{id}-"),
            )?;
            out.push(Profile {
                id,
                name,
                apps,
                pages: pages.iter().map(|l| l.id.clone()).collect(),
                color: get("color")
                    .map(|n| pick(n, &format!("{at} color"), PROFILE_COLORS))
                    .transpose()?,
                icon: get("icon")
                    .map(|n| icon(n, &format!("{at} icon")))
                    .transpose()?,
            });
            layers.extend(pages);
        }
        Ok(out)
    }

    fn open(&mut self, spec: &OpenSpec, at: &str) -> Action {
        let resolved = open::resolve(spec, self.host);
        if let Some(note) = resolved.note {
            self.notes.push(format!("{at}: {note}."));
        }
        resolved.action
    }

    fn action(&mut self, node: &Node, at: &str, on_key: bool) -> Result<Parsed, SaveError> {
        match &node.value {
            Value::String(s) => match parse_action(s, on_key) {
                Ok(Parsed::Page(name, _)) => Ok(Parsed::Page(name, node.pos)),
                Ok(parsed) => Ok(parsed),
                Err(message) => Err(invalid(node.pos, at, message)),
            },
            Value::Object(members) => self.open_object(members, node.pos, at).map(Parsed::Open),
            Value::Null => Ok(Parsed::Nothing),
            other => Err(invalid(
                node.pos,
                at,
                format!(
                    "an action is text like \"Ctrl+Z\" or an {{ \"open\": ... }} object, not {}",
                    other.kind()
                ),
            )),
        }
    }

    fn open_object(
        &mut self,
        members: &[Member],
        pos: Pos,
        at: &str,
    ) -> Result<OpenSpec, SaveError> {
        check_fields(members, &["open", "args", "cwd"], at)?;
        let get = |name: &str| members.iter().find(|m| m.key == name).map(|m| &m.node);
        let Some(target) = get("open") else {
            return Err(invalid(
                pos,
                at,
                "an action object needs \"open\": the app, path or web address",
            ));
        };
        let target = match &target.value {
            Value::String(s) if !s.trim().is_empty() => Target::One(s.trim().to_string()),
            Value::Object(per_os) => {
                check_fields(per_os, &["windows", "macos", "linux"], at)?;
                if per_os.is_empty() {
                    return Err(invalid(
                        target.pos,
                        at,
                        "open needs at least one of windows, macos, linux",
                    ));
                }
                let pick = |os: &str| -> Result<Option<String>, SaveError> {
                    per_os
                        .iter()
                        .find(|m| m.key == os)
                        .map(|m| match &m.node.value {
                            Value::String(s) if !s.trim().is_empty() => Ok(s.trim().to_string()),
                            other => Err(invalid(
                                m.node.pos,
                                at,
                                format!(
                                    "open.{os} must be an app, path or web address, not {}",
                                    other.kind()
                                ),
                            )),
                        })
                        .transpose()
                };
                Target::PerOs(crate::config::PerOsTargets {
                    windows: pick("windows")?,
                    macos: pick("macos")?,
                    linux: pick("linux")?,
                })
            }
            Value::String(_) => {
                return Err(invalid(
                    target.pos,
                    at,
                    "open is empty: write the app, path or web address",
                ));
            }
            other => {
                return Err(invalid(
                    target.pos,
                    at,
                    format!(
                        "open must be text or a {{ \"windows\": ... }} object, not {}",
                        other.kind()
                    ),
                ));
            }
        };
        let args = match get("args") {
            None => Vec::new(),
            Some(node) => match &node.value {
                Value::Array(items) => items
                    .iter()
                    .map(|i| match &i.value {
                        Value::String(s) => Ok(s.clone()),
                        other => Err(invalid(
                            i.pos,
                            at,
                            format!("each of args must be text, not {}", other.kind()),
                        )),
                    })
                    .collect::<Result<_, _>>()?,
                other => {
                    return Err(invalid(
                        node.pos,
                        at,
                        format!(
                            "args must be a list of text, like [\"--flag\", \"file.txt\"], not {}",
                            other.kind()
                        ),
                    ));
                }
            },
        };
        let cwd = get("cwd")
            .map(|n| text(n, "cwd").map(str::to_string))
            .transpose()?;
        let is_url = matches!(&target, Target::One(t) if open::is_url(t));
        if is_url && (!args.is_empty() || cwd.is_some()) {
            return Err(invalid(pos, at, "a web address cannot take args or cwd"));
        }
        Ok(OpenSpec { target, args, cwd })
    }

    fn wheel(
        &mut self,
        node: &Node,
        ids: &BTreeMap<String, String>,
    ) -> Result<JogSettings, SaveError> {
        let members = object(node, "wheel")?;
        let mut known = fields_of("wheel.");
        known.extend(WHEEL_MODES.iter().map(|(_, n)| *n));
        check_fields(members, &known, "wheel")?;
        let get = |name: &str| members.iter().find(|m| m.key == name).map(|m| &m.node);
        let mut jog = JogSettings::default();
        if let Some(mode) = get("mode") {
            jog.default_mode = one_of(mode, "wheel.mode", &WHEEL_MODES)?;
        }
        let speed = get("speed")
            .map(|n| number(n, "wheel.speed", 1.0, f64::from(MAX_SPEED), true))
            .transpose()?;
        if let Some(speed) = speed {
            for b in jog.per_mode.values_mut() {
                b.threshold = threshold_of(speed as u32);
            }
        }
        for (mode, name) in WHEEL_MODES {
            let Some(block) = get(name) else { continue };
            let at = format!("wheel.{name}");
            let members = object(block, &at)?;
            check_fields(members, &["left", "right", "speed", "max_rate"], &at)?;
            let mut binding = jog.per_mode.get(&mode).cloned().unwrap_or(JogBinding {
                left: Some(Action::Scroll { amount: -1 }),
                right: Some(Action::Scroll { amount: 1 }),
                threshold: threshold_of(speed.map_or(MAX_SPEED, |s| s as u32)),
                shuttle_max_rate_hz: None,
            });
            for m in members {
                let at = format!("{at}.{}", m.key);
                match m.key.as_str() {
                    "left" | "right" => {
                        let action = match self.action(&m.node, &at, false)? {
                            Parsed::Action(a) => Some(a),
                            Parsed::Nothing => None,
                            Parsed::Back => unreachable!("back is refused off a page"),
                            Parsed::Page(target, pos) => Some(Action::LayerPush {
                                target: page_id(ids, &target, pos, &at)?,
                            }),
                            Parsed::Open(spec) => Some(self.open(&spec, &at)),
                        };
                        if m.key == "left" {
                            binding.left = action;
                        } else {
                            binding.right = action;
                        }
                    }
                    "speed" => {
                        binding.threshold =
                            threshold_of(
                                number(&m.node, &at, 1.0, f64::from(MAX_SPEED), true)? as u32
                            )
                    }
                    _ => {
                        binding.shuttle_max_rate_hz = match m.node.value {
                            Value::Null => None,
                            _ => Some(number(&m.node, &at, 1.0, 1000.0, true)? as u32),
                        };
                    }
                }
            }
            jog.per_mode.insert(mode, binding);
        }
        Ok(jog)
    }
}

fn page_id(
    ids: &BTreeMap<String, String>,
    name: &str,
    pos: Pos,
    at: &str,
) -> Result<String, SaveError> {
    ids.get(&name.trim().to_lowercase())
        .cloned()
        .ok_or_else(|| {
            let names: Vec<&str> = ids.keys().map(String::as_str).collect();
            let hint = closest(&name.to_lowercase(), names.iter().copied())
                .map(|k| format!(" (did you mean \"page {k}\"?)"))
                .unwrap_or_default();
            invalid(
                pos,
                at,
                format!("there is no page \"{name}\" in the file{hint}"),
            )
        })
}

/// One of `names`, in any case.
fn pick(node: &Node, at: &str, names: &[&'static str]) -> Result<String, SaveError> {
    let table: Vec<(&'static str, &'static str)> = names.iter().map(|n| (*n, *n)).collect();
    one_of(node, at, &table).map(str::to_string)
}

/// A profile's `icon`: a bundled Material Symbols name, `:edit:` or `edit`.
fn icon(node: &Node, at: &str) -> Result<String, SaveError> {
    let word = text(node, at)?;
    if let Some(name) = crate::icons::icon_name(word) {
        return Ok(name.to_string());
    }
    let bare = crate::icons::bare(word).to_ascii_lowercase();
    let hint = closest(&bare, crate::icons::names())
        .map(|k| format!(" (did you mean \"{}\"?)", crate::icons::written(k)))
        .unwrap_or_default();
    Err(invalid(
        node.pos,
        at,
        format!(
            "there is no icon called \"{word}\"{hint}. Use a Material Symbols Rounded name, like \":edit:\" (every name: {})",
            crate::icons::ICON_CATALOGUE
        ),
    ))
}

/// A profile's `apps`: names (matched on every OS) or per-OS objects.
fn profile_apps(node: &Node, at: &str) -> Result<Vec<ProfileApp>, SaveError> {
    let Value::Array(items) = &node.value else {
        return Err(invalid(
            node.pos,
            at,
            format!("apps must be a list [ ... ], not {}", node.value.kind()),
        ));
    };
    let mut apps = Vec::new();
    for item in items {
        let app = match &item.value {
            Value::String(s) if !s.trim().is_empty() => {
                let s = s.trim().to_string();
                ProfileApp {
                    name: s.clone(),
                    windows: Some(s.clone()),
                    macos: Some(s.clone()),
                    linux: Some(s),
                }
            }
            Value::Object(members) => {
                check_fields(members, &["name", "windows", "macos", "linux"], at)?;
                let get = |n: &str| -> Result<Option<String>, SaveError> {
                    match members.iter().find(|m| m.key == n) {
                        None => Ok(None),
                        Some(m) => Ok(Some(
                            text(&m.node, &format!("{at} app {n}"))?.trim().to_string(),
                        )
                        .filter(|s| !s.is_empty())),
                    }
                };
                let (windows, macos, linux) = (get("windows")?, get("macos")?, get("linux")?);
                let Some(first) = windows.clone().or(macos.clone()).or(linux.clone()) else {
                    return Err(invalid(
                        item.pos,
                        at,
                        "an app needs \"windows\", \"macos\" or \"linux\": the program name on that OS",
                    ));
                };
                ProfileApp {
                    name: get("name")?.unwrap_or(first),
                    windows,
                    macos,
                    linux,
                }
            }
            other => {
                return Err(invalid(
                    item.pos,
                    at,
                    format!(
                        "an app is its program name in double quotes or an object, not {}",
                        other.kind()
                    ),
                ));
            }
        };
        apps.push(app);
    }
    Ok(apps)
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "page".into() } else { out }
}

/// Wheel speed 1..=10 to ticks per step 10..=1.
pub fn threshold_of(speed: u32) -> u32 {
    MAX_SPEED + 1 - speed.clamp(1, MAX_SPEED)
}

/// Ticks per step to the speed a file writes (clamped to 1..=10).
pub fn speed_of(threshold: u32) -> u32 {
    MAX_SPEED + 1 - threshold.clamp(1, MAX_SPEED)
}

/// Reads one action string (a key's when `on_key`, else a wheel step's).
fn parse_action(text: &str, on_key: bool) -> Result<Parsed, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("the action is empty: write \"nothing\" for a key that does nothing".into());
    }
    if open::is_url(trimmed) {
        return Ok(Parsed::Open(OpenSpec {
            target: Target::One(trimmed.to_string()),
            args: Vec::new(),
            cwd: None,
        }));
    }
    let (verb, rest) = match trimmed.split_once(char::is_whitespace) {
        Some((v, r)) => (v.to_ascii_lowercase(), r.trim()),
        None => (trimmed.to_ascii_lowercase(), ""),
    };
    let needs = |what: &str| format!("\"{verb}\" needs {what}, like \"{}\"", example(&verb));
    match verb.as_str() {
        "nothing" if rest.is_empty() => return Ok(Parsed::Nothing),
        "back" if rest.is_empty() && on_key => return Ok(Parsed::Back),
        "back" if rest.is_empty() => {
            return Err("\"back\" works only on a page's key, not on the wheel".into());
        }
        "open" if rest.is_empty() => return Err(needs("an app name or a path")),
        "open" => {
            return Ok(Parsed::Open(OpenSpec {
                target: Target::One(rest.to_string()),
                args: Vec::new(),
                cwd: None,
            }));
        }
        "hold" | "toggle" if rest.is_empty() => return Err(needs("a shortcut")),
        "hold" | "toggle" => {
            let combo = parse_combo(rest, true).map_err(|e| {
                e.0.replace("unknown action \"", "unknown shortcut \"")
                    .replace("(did you mean \"", &format!("(did you mean \"{verb} "))
            })?;
            return Ok(Parsed::Action(if verb == "hold" {
                Action::Held { combo }
            } else {
                Action::Latched { combo }
            }));
        }
        "page" if rest.is_empty() => return Err(needs("a page name")),
        "page" => {
            return Ok(Parsed::Page(
                rest.to_string(),
                json::Pos { line: 0, column: 0 },
            ));
        }
        "scroll" => {
            let amount = match rest.to_ascii_lowercase().as_str() {
                "up" => -1,
                "down" => 1,
                n => match n.parse::<i32>() {
                    Ok(n) if n != 0 && (-100..=100).contains(&n) => n,
                    _ => {
                        return Err(format!(
                            "\"scroll\" needs up, down or a whole number from -100 to 100 (not 0), like \"scroll -3\", not \"{trimmed}\""
                        ));
                    }
                },
            };
            return Ok(Parsed::Action(Action::Scroll { amount }));
        }
        "wheel" => {
            let lower = rest.to_ascii_lowercase();
            let squashed = lower.split_whitespace().collect::<Vec<_>>().join(" ");
            if let Some(mode) = squashed.strip_prefix("mode ") {
                if let Some((m, _)) = WHEEL_MODES.iter().find(|(_, n)| *n == mode) {
                    return Ok(Parsed::Action(Action::SetJogMode { mode: *m }));
                }
                let names: Vec<&str> = WHEEL_MODES.iter().map(|(_, n)| *n).collect();
                return Err(format!(
                    "\"wheel mode\" needs one of {}, not \"{mode}\"",
                    names.join(", ")
                ));
            }
            if let Some((m, _)) = KNOBS.iter().find(|(_, n)| *n == squashed) {
                return Ok(Parsed::Action(Action::OverrideToggle { mode: *m }));
            }
            let options: Vec<String> = KNOBS
                .iter()
                .map(|(_, n)| format!("wheel {n}"))
                .chain(WHEEL_MODES.iter().map(|(_, n)| format!("wheel mode {n}")))
                .collect();
            let hint = closest(
                &format!("wheel {squashed}"),
                options.iter().map(String::as_str),
            )
            .map(|k| format!(" (did you mean \"{k}\"?)"))
            .unwrap_or_default();
            return Err(format!(
                "unknown action \"{trimmed}\"{hint}; \"wheel\" takes volume, app volume, brightness or mode <mode>"
            ));
        }
        _ => {}
    }
    match parse_combo(trimmed, false) {
        Ok(combo) => Ok(Parsed::Action(Action::Hotkey { combo })),
        Err(e) => {
            // "opn Spotify": a typo in the first word.
            if let Some(v) = closest(&verb, VERBS.iter().copied()) {
                let fixed = if rest.is_empty() {
                    v.to_string()
                } else {
                    format!("{v} {rest}")
                };
                return Err(format!(
                    "unknown action \"{trimmed}\" (did you mean \"{fixed}\"?)"
                ));
            }
            if !rest.is_empty() && !trimmed.contains('+') && names::shortcut_key(trimmed).is_none()
            {
                return Err(format!(
                    "unknown action \"{trimmed}\"; an action starts with open, hold, toggle, page, scroll or wheel, or is a shortcut like Ctrl+Z (see SCHEMA.md)"
                ));
            }
            Err(e.0)
        }
    }
}

fn example(verb: &str) -> &'static str {
    match verb {
        "open" => "open Spotify",
        "hold" => "hold Shift",
        "toggle" => "toggle Ctrl",
        "page" => "page Music",
        _ => "",
    }
}

fn dial_settings(node: &Node) -> Result<OverrideSettings, SaveError> {
    let members = object(node, "dial")?;
    check_fields(members, &fields_of("dial."), "dial")?;
    let mut dial = OverrideSettings::default();
    for m in members {
        let at = format!("dial.{}", m.key);
        match m.key.as_str() {
            "volume_step" => {
                dial.system_volume_step = (number(&m.node, &at, 0.1, 100.0, false)? / 100.0) as f32
            }
            "app_volume_step" => {
                dial.app_volume_step = (number(&m.node, &at, 0.1, 100.0, false)? / 100.0) as f32
            }
            _ => dial.brightness_step = number(&m.node, &at, 1.0, 100.0, true)? as u8,
        }
    }
    Ok(dial)
}

const APPEARANCES: [(Appearance, &str); 3] = [
    (Appearance::System, "system"),
    (Appearance::Light, "light"),
    (Appearance::Dark, "dark"),
];

fn apply_settings(settings: &mut crate::config::Settings, node: &Node) -> Result<(), SaveError> {
    let members = object(node, "settings")?;
    check_fields(members, &fields_of("settings."), "settings")?;
    for m in members {
        let at = format!("settings.{}", m.key);
        match m.key.as_str() {
            "appearance" => settings.appearance = one_of(&m.node, &at, &APPEARANCES)?,
            "update_automatically" => settings.auto_update = boolean(&m.node, &at)?,
            "step_aside_for_resolve" => settings.step_aside_for_resolve = boolean(&m.node, &at)?,
            "detailed_logs" => settings.detailed_logs = boolean(&m.node, &at)?,
            "low_battery_warning" => {
                settings.low_battery_threshold = number(&m.node, &at, 0.0, 100.0, true)? as u8
            }
            _ => {
                let name = text(&m.node, &at)?.trim();
                if name.is_empty() {
                    return Err(invalid(
                        m.node.pos,
                        at,
                        "settings.resolve_program cannot be empty",
                    ));
                }
                settings.resolve_process_name = name.to_string();
            }
        }
    }
    Ok(())
}

/// Turns off every LaunchApp; returns how many were on.
fn disable_launch_apps(config: &mut Config) -> usize {
    let keys = config
        .layers
        .iter_mut()
        .flat_map(|l| l.bindings.values_mut());
    let jog = config
        .jog
        .per_mode
        .values_mut()
        .flat_map(|b| [b.left.as_mut(), b.right.as_mut()])
        .flatten();
    let mut n = 0;
    for action in keys.chain(jog) {
        if let Action::LaunchApp { enabled, .. } = action {
            n += usize::from(*enabled);
            *enabled = false;
        }
    }
    n
}

// ---- Writing ---------------------------------------------------------------

/// A JSON value with its object keys in order, for a readable file.
enum Out {
    Str(String),
    Num(String),
    List(Vec<Out>),
    Obj(Vec<(String, Out)>),
}

impl Out {
    fn write(&self, out: &mut String, indent: usize) {
        let pad = |n: usize| "  ".repeat(n);
        match self {
            Out::Str(s) => out.push_str(&serde_json::to_string(s).expect("a string serializes")),
            Out::Num(n) => out.push_str(n),
            Out::List(items) if items.is_empty() => out.push_str("[]"),
            Out::List(items)
                if items
                    .iter()
                    .any(|i| matches!(i, Out::Obj(m) if !m.is_empty())) =>
            {
                // A list of objects (profiles): one per line block.
                out.push_str("[\n");
                for (i, item) in items.iter().enumerate() {
                    out.push_str(&pad(indent + 1));
                    item.write(out, indent + 1);
                    if i + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&pad(indent));
                out.push(']');
            }
            Out::List(items) => {
                // Short lists of text stay on one line: "args": ["--x", "y"].
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    item.write(out, indent);
                }
                out.push(']');
            }
            Out::Obj(members) if members.is_empty() => out.push_str("{}"),
            Out::Obj(members) => {
                out.push_str("{\n");
                for (i, (k, v)) in members.iter().enumerate() {
                    out.push_str(&pad(indent + 1));
                    out.push_str(&serde_json::to_string(k).expect("a string serializes"));
                    out.push_str(": ");
                    v.write(out, indent + 1);
                    if i + 1 < members.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&pad(indent));
                out.push('}');
            }
        }
    }
}

fn num(n: f64) -> Out {
    let rounded = (n * 10_000.0).round() / 10_000.0;
    Out::Num(if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    })
}

/// The short (version 2) file for `config`, as Back up writes it. On
/// `host`, an app is written by its installed name when that name opens
/// the same thing again.
pub fn export(config: &Config, host: &dyn Host) -> Vec<u8> {
    // The Default profile's pages, its first page first.
    let mut layers: Vec<&Layer> = config
        .layers
        .iter()
        .filter(|l| l.id == config.base_layer)
        .collect();
    layers.extend(
        config
            .pages_of(None)
            .into_iter()
            .filter(|l| l.id != config.base_layer),
    );
    let pages = pages_out(&layers, host);
    let profiles: Vec<Out> = config
        .profiles
        .iter()
        .map(|p| {
            let mut members = vec![("name".to_string(), Out::Str(p.name.clone()))];
            if let Some(c) = &p.color {
                members.push(("color".into(), Out::Str(c.clone())));
            }
            if let Some(i) = &p.icon {
                members.push(("icon".into(), Out::Str(crate::icons::written(i))));
            }
            let apps: Vec<Out> = p.apps.iter().map(app_out).collect();
            if !apps.is_empty() {
                members.push(("apps".into(), Out::List(apps)));
            }
            members.push((
                "pages".into(),
                pages_out(&config.pages_of(Some(&p.id)), host),
            ));
            Out::Obj(members)
        })
        .collect();
    let default_names = page_names(&layers);
    let default_name = |id: &str| {
        default_names
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, n)| n.clone())
    };
    let wheel_writer = Writer {
        host,
        name_of: &default_name,
    };
    let mut top = vec![
        ("$schema".to_string(), Out::Str(SCHEMA_URL.into())),
        (
            "version".to_string(),
            Out::Num(MAPPINGS_VERSION.to_string()),
        ),
        ("pages".to_string(), pages),
    ];
    if !profiles.is_empty() {
        top.push(("profiles".to_string(), Out::List(profiles)));
    }
    top.push(("wheel".to_string(), wheel_writer.wheel(&config.jog)));
    let dial = &config.overrides;
    if *dial != OverrideSettings::default() {
        top.push((
            "dial".into(),
            Out::Obj(vec![
                (
                    "volume_step".into(),
                    num(f64::from(dial.system_volume_step) * 100.0),
                ),
                (
                    "app_volume_step".into(),
                    num(f64::from(dial.app_volume_step) * 100.0),
                ),
                (
                    "brightness_step".into(),
                    Out::Num(dial.brightness_step.to_string()),
                ),
            ]),
        ));
    }
    let mut text = String::new();
    Out::Obj(top).write(&mut text, 0);
    text.push('\n');
    text.into_bytes()
}

/// Each page's written name: its own, unique ignoring case.
fn page_names(layers: &[&Layer]) -> Vec<(String, String)> {
    let mut names: Vec<(String, String)> = Vec::new();
    for (i, layer) in layers.iter().enumerate() {
        let base = if layer.name.trim().is_empty() {
            format!("Page {}", i + 1)
        } else {
            layer.name.trim().to_string()
        };
        let mut name = base.clone();
        let mut n = 2;
        while names.iter().any(|(_, w)| w.eq_ignore_ascii_case(&name)) {
            name = format!("{base} ({n})");
            n += 1;
        }
        names.push((layer.id.clone(), name));
    }
    names
}

/// One set of pages (Default's, or a profile's), as `pages` is written.
fn pages_out(layers: &[&Layer], host: &dyn Host) -> Out {
    let names = page_names(layers);
    let name_of = |id: &str| names.iter().find(|(i, _)| i == id).map(|(_, n)| n.clone());
    let writer = Writer {
        host,
        name_of: &name_of,
    };
    let mut pages = Vec::new();
    for (layer, (_, name)) in layers.iter().zip(&names) {
        let mut keys = Vec::new();
        for (key, printed) in names::DEVICE_KEYS {
            let id = key_id(key);
            match layer.bindings.get(&id) {
                Some(action) => {
                    if let Some(out) = writer.action(action) {
                        keys.push((printed.to_string(), out));
                    }
                }
                None => {
                    let opened_here = layers.iter().any(|l| {
                        l.id != layer.id && matches!(l.bindings.get(&id), Some(Action::LayerPush { target }) if *target == layer.id)
                    });
                    if opened_here {
                        keys.push((printed.to_string(), Out::Str("back".into())));
                    }
                }
            }
        }
        pages.push((name.clone(), Out::Obj(keys)));
    }
    Out::Obj(pages)
}

/// A profile app: its name when that is its program on every OS, else an object.
fn app_out(app: &ProfileApp) -> Out {
    let every = [&app.windows, &app.macos, &app.linux];
    if every
        .iter()
        .all(|o| o.as_deref() == Some(app.name.as_str()))
    {
        return Out::Str(app.name.clone());
    }
    let mut members = vec![("name".to_string(), Out::Str(app.name.clone()))];
    for (os, v) in [
        ("windows", &app.windows),
        ("macos", &app.macos),
        ("linux", &app.linux),
    ] {
        if let Some(v) = v {
            members.push((os.to_string(), Out::Str(v.clone())));
        }
    }
    Out::Obj(members)
}

struct Writer<'a> {
    host: &'a dyn Host,
    name_of: &'a dyn Fn(&str) -> Option<String>,
}

impl Writer<'_> {
    fn mac(&self) -> bool {
        self.host.os() == Os::MacOs
    }

    fn combo(&self, combo: &KeyCombo) -> String {
        format_combo(combo, self.mac())
    }

    fn action(&self, action: &Action) -> Option<Out> {
        Some(Out::Str(match action {
            Action::Hotkey { combo } => self.combo(combo),
            Action::Held { combo } => format!("hold {}", self.combo(combo)),
            Action::Latched { combo } => format!("toggle {}", self.combo(combo)),
            Action::Scroll { amount: -1 } => "scroll up".into(),
            Action::Scroll { amount: 1 } => "scroll down".into(),
            Action::Scroll { amount } => format!("scroll {amount}"),
            Action::LayerPush { target } => format!("page {}", (self.name_of)(target)?),
            Action::SetJogMode { mode } => format!(
                "wheel mode {}",
                WHEEL_MODES
                    .iter()
                    .find(|(m, _)| m == mode)
                    .map_or("scroll", |(_, n)| n)
            ),
            Action::OverrideToggle { mode } => format!(
                "wheel {}",
                KNOBS
                    .iter()
                    .find(|(m, _)| m == mode)
                    .map_or("volume", |(_, n)| n)
            ),
            Action::LaunchApp {
                path,
                args,
                cwd,
                per_os: Some(targets),
                ..
            } => return Some(self.open_per_os(path, args, cwd, targets)),
            Action::LaunchApp {
                path, args, cwd, ..
            } => return Some(self.open(path, args, cwd)),
        }))
    }

    fn open(&self, path: &str, args: &[String], cwd: &Option<String>) -> Out {
        let (target, own) = open::written_target(path, args, cwd, self.host);
        if open::is_url(&target) && own.is_empty() && cwd.is_none() {
            return Out::Str(target);
        }
        // A target with spaces at its ends would not survive "open <t>".
        if own.is_empty() && cwd.is_none() && target.trim() == target && !target.is_empty() {
            return Out::Str(format!("open {target}"));
        }
        let mut members = vec![("open".to_string(), Out::Str(target))];
        if !own.is_empty() {
            members.push((
                "args".into(),
                Out::List(own.into_iter().map(Out::Str).collect()),
            ));
        }
        if let Some(cwd) = cwd {
            members.push(("cwd".into(), Out::Str(cwd.clone())));
        }
        Out::Obj(members)
    }

    /// A per-OS `open` from a restored file: the object form, every OS.
    fn open_per_os(
        &self,
        path: &str,
        args: &[String],
        cwd: &Option<String>,
        targets: &crate::config::PerOsTargets,
    ) -> Out {
        let (targets, own) = open::written_per_os(path, args, cwd, targets, self.host);
        let per_os = [Os::Windows, Os::MacOs, Os::Linux]
            .into_iter()
            .filter_map(|os| {
                open::select(&targets, os)
                    .map(|t| (os.field().to_string(), Out::Str(t.to_string())))
            })
            .collect();
        let mut members = vec![("open".to_string(), Out::Obj(per_os))];
        if !own.is_empty() {
            members.push((
                "args".into(),
                Out::List(own.into_iter().map(Out::Str).collect()),
            ));
        }
        if let Some(cwd) = cwd {
            members.push(("cwd".into(), Out::Str(cwd.clone())));
        }
        Out::Obj(members)
    }

    fn wheel(&self, jog: &JogSettings) -> Out {
        let mode = WHEEL_MODES
            .iter()
            .find(|(m, _)| *m == jog.default_mode)
            .map_or("scroll", |(_, n)| n);
        let speed_mode = jog
            .per_mode
            .get(&jog.default_mode)
            .or_else(|| jog.per_mode.values().next());
        let speed = speed_mode.map_or(MAX_SPEED, |b| speed_of(b.threshold));
        let mut members = vec![
            ("mode".to_string(), Out::Str(mode.into())),
            ("speed".to_string(), Out::Num(speed.to_string())),
        ];
        for (m, name) in WHEEL_MODES {
            let Some(b) = jog.per_mode.get(&m) else {
                continue;
            };
            let mut block = Vec::new();
            let step = |a: &Option<Action>| {
                a.as_ref()
                    .and_then(|a| self.action(a))
                    .unwrap_or(Out::Str("nothing".into()))
            };
            if b.left != Some(Action::Scroll { amount: -1 }) {
                block.push(("left".to_string(), step(&b.left)));
            }
            if b.right != Some(Action::Scroll { amount: 1 }) {
                block.push(("right".to_string(), step(&b.right)));
            }
            if speed_of(b.threshold) != speed {
                block.push((
                    "speed".to_string(),
                    Out::Num(speed_of(b.threshold).to_string()),
                ));
            }
            if let Some(rate) = b.shuttle_max_rate_hz {
                block.push(("max_rate".to_string(), Out::Num(rate.to_string())));
            }
            if !block.is_empty() {
                members.push((name.to_string(), Out::Obj(block)));
            }
        }
        Out::Obj(members)
    }
}

#[cfg(test)]
mod tests;
