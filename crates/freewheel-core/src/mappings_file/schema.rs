//! The JSON Schema (draft 2020-12) of a version-2 mappings file and the
//! generated lists in SCHEMA.md, both built from the parser's own tables so
//! they cannot drift: tests compare them with the committed files
//! (`FREEWHEEL_WRITE_SCHEMA=1 cargo test -p freewheel-core mappings_file`
//! rewrites both).

use serde_json::{Value, json};

use super::names::{self, DEVICE_KEYS, MODIFIERS};
use super::{ACTIONS, FIELDS, KNOBS, MAX_SPEED, SCHEMA_URL, WHEEL_MODES};

/// `s` as a regex source: syntax characters escaped (valid with the `u` flag).
fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if "\\^$.|?*+()[]{}/".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A regex for `word`, any letter case, with `gap` allowed between characters.
fn any_case(word: &str, gap: &str) -> String {
    let mut out = String::new();
    for (i, c) in word.chars().enumerate() {
        if i > 0 {
            out.push_str(gap);
        }
        let (lo, up) = (c.to_ascii_lowercase(), c.to_ascii_uppercase());
        if lo != up {
            out.push_str(&format!("[{up}{lo}]"));
        } else {
            out.push_str(&escape(&c.to_string()));
        }
    }
    out
}

fn alternatives(items: impl Iterator<Item = String>) -> String {
    let all: Vec<String> = items.collect();
    format!("(?:{})", all.join("|"))
}

/// Any spelling of a Speed Editor key the parser reads ("CAM1", "cam 1", "Cam1").
pub fn key_name_pattern() -> String {
    let squash = |s: &str| -> String { s.chars().filter(char::is_ascii_alphanumeric).collect() };
    let keys = alternatives(
        DEVICE_KEYS
            .iter()
            .map(|(_, p)| any_case(&squash(p), "[^A-Za-z0-9]*")),
    );
    format!("^[^A-Za-z0-9]*{keys}[^A-Za-z0-9]*$")
}

fn modifier_pattern() -> String {
    alternatives(
        MODIFIERS
            .iter()
            .flat_map(|(_, n)| n.iter())
            .map(|n| any_case(n, "")),
    )
}

fn key_pattern() -> String {
    let named = names::named_keys()
        .flat_map(|k| {
            [k.stored, k.written]
                .into_iter()
                .chain(k.aliases.iter().copied())
        })
        .filter(|n| n.chars().count() > 1)
        .map(|n| {
            let letters: String = n
                .chars()
                .filter(|c| !c.is_whitespace() && *c != '_')
                .collect();
            any_case(&letters, "[\\s_]*")
        });
    // One character (not a space, not a control character) or a named key.
    format!("(?:{}|[^\\s\\p{{Cc}}])", alternatives(named))
}

/// Any action string the parser reads; `back` only on a page's key.
pub fn action_pattern(on_key: bool) -> String {
    let m = modifier_pattern();
    let k = key_pattern();
    let combo = format!("(?:{m}\\s*\\+\\s*)*(?:{k}|\\+)|(?:{m}\\s*\\+\\s*)+\\+");
    let lone = format!("(?:{m}\\s*\\+\\s*)*(?:{k}|{m})|(?:{m}\\s*\\+\\s*)*\\+");
    let s = "\\s+";
    let modes = alternatives(WHEEL_MODES.iter().map(|(_, n)| any_case(n, "")));
    let knobs = alternatives(KNOBS.iter().map(|(_, n)| {
        n.split(' ')
            .map(|w| any_case(w, ""))
            .collect::<Vec<_>>()
            .join(s)
    }));
    let mut forms = vec![
        any_case("nothing", ""),
        format!("{}{}\\S+", any_case("http", ""), "[Ss]?://"),
        format!("{}{s}\\S.*", any_case("open", "")),
        format!(
            "(?:{}|{}){s}(?:{lone})",
            any_case("hold", ""),
            any_case("toggle", "")
        ),
        format!("{}{s}\\S.*", any_case("page", "")),
        format!(
            "{}{s}(?:{}|{}|-?(?:100|[1-9][0-9]?))",
            any_case("scroll", ""),
            any_case("up", ""),
            any_case("down", "")
        ),
        format!(
            "{}{s}(?:{knobs}|{}{s}{modes})",
            any_case("wheel", ""),
            any_case("mode", "")
        ),
        combo,
    ];
    if on_key {
        forms.insert(1, any_case("back", ""));
    }
    format!("^\\s*(?:{})\\s*$", forms.join("|"))
}

fn field(path: &str) -> &'static super::Field {
    FIELDS
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("no field {path}"))
}

fn about(path: &str) -> String {
    let f = field(path);
    let range = if f.range.is_empty() {
        String::new()
    } else {
        format!(" Allowed: {}.", f.range)
    };
    format!("{}{range} Default: {}.", f.about, f.default)
}

fn choice(path: &str, names: &[&str]) -> Value {
    json!({
        "description": about(path),
        "anyOf": [
            { "enum": names },
            { "type": "string", "pattern": format!("^\\s*{}\\s*$", alternatives(names.iter().map(|n| any_case(n, "")))) }
        ]
    })
}

/// The JSON Schema.
pub fn schema() -> Value {
    let action_help: Vec<String> = ACTIONS
        .iter()
        .map(|a| format!("{}: {}", a.form, a.does))
        .collect();
    let examples: Vec<&str> = ACTIONS
        .iter()
        .filter(|a| !a.example.starts_with('{'))
        .map(|a| a.example)
        .collect();
    let page_keys: serde_json::Map<String, Value> = DEVICE_KEYS
        .iter()
        .map(|(_, p)| {
            (
                p.to_string(),
                json!({ "$ref": "#/$defs/keyAction", "description": format!("The {p} key.") }),
            )
        })
        .collect();
    let mode_names: Vec<&str> = WHEEL_MODES.iter().map(|(_, n)| *n).collect();
    let speed = |path: &str| json!({ "type": "integer", "minimum": 1, "maximum": MAX_SPEED, "description": about(path) });
    let mode_block = json!({
        "type": "object",
        "description": about("wheel.<mode>"),
        "additionalProperties": false,
        "properties": {
            "left": { "$ref": "#/$defs/wheelAction", "description": about("wheel.<mode>.left") },
            "right": { "$ref": "#/$defs/wheelAction", "description": about("wheel.<mode>.right") },
            "speed": speed("wheel.<mode>.speed"),
            "max_rate": { "type": ["integer", "null"], "minimum": 1, "maximum": 1000, "description": about("wheel.<mode>.max_rate") }
        }
    });
    let mut wheel_props = serde_json::Map::new();
    wheel_props.insert("mode".into(), choice("wheel.mode", &mode_names));
    wheel_props.insert("speed".into(), speed("wheel.speed"));
    for name in &mode_names {
        wheel_props.insert((*name).into(), mode_block.clone());
    }
    let text =
        |path: &str| json!({ "type": "string", "pattern": "\\S", "description": about(path) });
    let boolean = |path: &str| json!({ "type": "boolean", "description": about(path) });
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": SCHEMA_URL,
        "title": "freewheel mappings file",
        "description": "What each Speed Editor key, the wheel and the dial do in freewheel. Reference: SCHEMA.md in the freewheel repository.",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "$schema": { "type": "string", "description": about("$schema") },
            "version": { "const": 2, "description": about("version") },
            "pages": {
                "type": "object",
                "minProperties": 1,
                "description": about("pages"),
                "propertyNames": { "pattern": "\\S" },
                "additionalProperties": { "$ref": "#/$defs/page" }
            },
            "profiles": {
                "type": "array",
                "description": about("profiles"),
                "items": { "$ref": "#/$defs/profile" }
            },
            "wheel": {
                "type": "object",
                "description": about("wheel"),
                "additionalProperties": false,
                "properties": wheel_props
            },
            "dial": {
                "type": "object",
                "description": about("dial"),
                "additionalProperties": false,
                "properties": {
                    "volume_step": { "type": "number", "minimum": 0.1, "maximum": 100, "description": about("dial.volume_step") },
                    "app_volume_step": { "type": "number", "minimum": 0.1, "maximum": 100, "description": about("dial.app_volume_step") },
                    "brightness_step": { "type": "integer", "minimum": 1, "maximum": 100, "description": about("dial.brightness_step") }
                }
            },
            "settings": {
                "type": "object",
                "description": about("settings"),
                "additionalProperties": false,
                "properties": {
                    "appearance": choice("settings.appearance", &["system", "light", "dark"]),
                    "update_automatically": boolean("settings.update_automatically"),
                    "step_aside_for_resolve": boolean("settings.step_aside_for_resolve"),
                    "resolve_program": text("settings.resolve_program"),
                    "low_battery_warning": { "type": "integer", "minimum": 0, "maximum": 100, "description": about("settings.low_battery_warning") },
                    "detailed_logs": boolean("settings.detailed_logs")
                }
            }
        },
        "$defs": {
            "profile": {
                "type": "object",
                "required": ["name", "pages"],
                "additionalProperties": false,
                "properties": {
                    "name": { "type": "string", "pattern": "\\S", "not": { "pattern": format!("^\\s*{}\\s*$", any_case("default", "")) }, "description": about("profiles[].name") },
                    "color": choice("profiles[].color", crate::config::PROFILE_COLORS),
                    "icon": { "type": "string", "pattern": "^\\s*:?[A-Za-z0-9_]+:?\\s*$", "examples": crate::config::SUGGESTED_ICONS.iter().map(|n| crate::icons::written(n)).collect::<Vec<_>>(), "description": about("profiles[].icon") },
                    "apps": { "type": "array", "description": about("profiles[].apps"), "items": { "$ref": "#/$defs/profileApp" } },
                    "pages": {
                        "type": "object",
                        "minProperties": 1,
                        "description": about("profiles[].pages"),
                        "propertyNames": { "pattern": "\\S" },
                        "additionalProperties": { "$ref": "#/$defs/page" }
                    }
                }
            },
            "profileApp": {
                "description": "An app: its program name (matched on every OS), or one name per OS: the program file on Windows (notepad.exe), the bundle id on macOS (com.apple.TextEdit), the program or window class on Linux (gedit).",
                "anyOf": [
                    { "type": "string", "pattern": "\\S" },
                    {
                        "type": "object",
                        "additionalProperties": false,
                        "anyOf": [
                            { "required": ["windows"], "properties": { "windows": true } },
                            { "required": ["macos"], "properties": { "macos": true } },
                            { "required": ["linux"], "properties": { "linux": true } }
                        ],
                        "properties": {
                            "name": { "type": "string", "description": "What the Keys tab shows." },
                            "windows": { "type": "string", "pattern": "\\S" },
                            "macos": { "type": "string", "pattern": "\\S" },
                            "linux": { "type": "string", "pattern": "\\S" }
                        }
                    }
                ]
            },
            "page": {
                "type": "object",
                "description": about("pages.<page>"),
                "properties": page_keys,
                "patternProperties": { key_name_pattern(): { "$ref": "#/$defs/keyAction" } },
                "additionalProperties": false
            },
            "keyAction": {
                "description": action_help.join("\n"),
                "examples": examples,
                "anyOf": [
                    { "type": "string", "pattern": action_pattern(true) },
                    { "$ref": "#/$defs/open" },
                    { "type": "null" }
                ]
            },
            "wheelAction": {
                "description": "What one wheel step does: any action except back.",
                "anyOf": [
                    { "type": "string", "pattern": action_pattern(false) },
                    { "$ref": "#/$defs/open" },
                    { "type": "null" }
                ]
            },
            "openTarget": { "type": "string", "pattern": "\\S", "description": "An app name, a path (environment variables and ~ expand), a macOS bundle id, a Linux .desktop id, or a web address." },
            "open": {
                "type": "object",
                "description": "open with arguments, a start folder or one app per OS.",
                "required": ["open"],
                "additionalProperties": false,
                "properties": {
                    "open": {
                        "anyOf": [
                            { "$ref": "#/$defs/openTarget" },
                            {
                                "type": "object",
                                "minProperties": 1,
                                "additionalProperties": false,
                                "properties": {
                                    "windows": { "$ref": "#/$defs/openTarget" },
                                    "macos": { "$ref": "#/$defs/openTarget" },
                                    "linux": { "$ref": "#/$defs/openTarget" }
                                }
                            }
                        ]
                    },
                    "args": { "type": "array", "items": { "type": "string" }, "description": "Arguments, each passed as it is (no shell)." },
                    "cwd": { "type": "string", "description": "The folder the app starts in (environment variables and ~ expand)." }
                }
            }
        }
    })
}

/// The schema file's text.
pub fn schema_text() -> String {
    let mut text = serde_json::to_string_pretty(&schema()).expect("the schema serializes");
    text.push('\n');
    text
}

fn md(s: &str) -> String {
    s.replace('|', "\\|")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn code(s: &str) -> String {
    let s = s.replace('|', "\\|");
    if s.contains('`') {
        format!("`` {s} ``")
    } else {
        format!("`{s}`")
    }
}

/// The generated blocks of SCHEMA.md, by name.
pub fn docs_blocks() -> Vec<(&'static str, String)> {
    let mut keys = String::from("| # | Key name | Also reads |\n|---|---|---|\n");
    for (i, (key, printed)) in DEVICE_KEYS.iter().enumerate() {
        let mut also = vec![names::key_id(*key)];
        let label = crate::status::printed_label(*key);
        if label != *printed {
            also.insert(0, label.to_string());
        }
        let also: Vec<String> = also.iter().map(|a| code(a)).collect();
        keys.push_str(&format!(
            "| {} | {} | {} |\n",
            i + 1,
            code(printed),
            also.join(", ")
        ));
    }
    let mut actions = String::from("| Action | Example | What it does |\n|---|---|---|\n");
    for a in ACTIONS {
        actions.push_str(&format!(
            "| {} | {} | {} |\n",
            code(a.form),
            code(a.example),
            md(a.does)
        ));
    }
    let mut modifiers = String::from("| Modifier | Names that read |\n|---|---|\n");
    for (m, names) in MODIFIERS {
        let all: Vec<String> = names.iter().map(|n| code(n)).collect();
        modifiers.push_str(&format!("| {m:?} | {} |\n", all.join(", ")));
    }
    let mut shortcut_keys =
        String::from("| Group | Key (as written) | Also reads |\n|---|---|---|\n");
    for k in names::named_keys() {
        let mut also: Vec<String> = Vec::new();
        if k.stored != k.written {
            also.push(code(k.stored));
        }
        also.extend(k.aliases.iter().map(|a| code(a)));
        shortcut_keys.push_str(&format!(
            "| {} | {} | {} |\n",
            k.group,
            code(k.written),
            also.join(", ")
        ));
    }
    let mut fields =
        String::from("| Field | Type | Default | Allowed | What it is |\n|---|---|---|---|---|\n");
    for f in FIELDS {
        let range = if f.range.is_empty() {
            "—".to_string()
        } else {
            md(f.range)
        };
        fields.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            code(f.path),
            md(f.kind),
            md(f.default),
            range,
            md(f.about)
        ));
    }
    let modes: Vec<String> = WHEEL_MODES.iter().map(|(_, n)| code(n)).collect();
    vec![
        ("keys", keys),
        ("actions", actions),
        ("modifiers", modifiers),
        ("shortcut-keys", shortcut_keys),
        ("fields", fields),
        ("wheel-modes", format!("{}\n", modes.join(", "))),
    ]
}

/// `doc` with each generated block replaced by its current text.
pub fn with_blocks(doc: &str) -> Result<String, String> {
    let mut out = doc.to_string();
    for (name, body) in docs_blocks() {
        let start = format!("<!-- generated: {name} -->\n");
        let end = format!("<!-- end generated: {name} -->");
        let a = out
            .find(&start)
            .ok_or(format!("SCHEMA.md has no {start:?}"))?
            + start.len();
        let b = a + out[a..]
            .find(&end)
            .ok_or(format!("SCHEMA.md has no {end:?}"))?;
        out.replace_range(a..b, &body);
    }
    Ok(out)
}
