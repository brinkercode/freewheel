//! What a key, a mapping or a config change is, in words for the log and
//! the support bundle: `CUT`, `Open app "Spotify"`, `Open website
//! example.com`, `CUT: Do nothing -> Press Ctrl+C`.
//!
//! These lines go to the user's own log file. App paths, shortcuts, page
//! names and website hosts are fine there (they are what the user needs to
//! see what happened); a website's full address only with detailed logs on
//! (`full`), as it may hold private query strings.

use std::collections::{BTreeMap, BTreeSet};

use crate::config::{Action, Config, JogBinding, JogMode, KeyCombo, Layer, Modifier, OverrideMode};
use crate::report::Key;
use crate::status::printed_label;

/// The programs that open a file or a web address the way a double-click
/// does: a website or Start Menu app is saved as one of these plus its target.
const OPENERS: [&str; 2] = ["explorer.exe", "/usr/bin/open"];

/// The key printed on the device for a binding name (`"Cam1"` -> `CAM 1`);
/// an unknown name as it is.
pub fn key_label(binding: &str) -> String {
    Key::ALL
        .iter()
        .find(|k| format!("{k:?}") == binding)
        .map_or_else(|| binding.to_string(), |&k| printed_label(k).to_string())
}

/// The page's name for `id`, or the id when no page has it.
pub fn page_name<'a>(layers: &'a [Layer], id: &'a str) -> &'a str {
    layers
        .iter()
        .find(|l| l.id == id)
        .map_or(id, |l| l.name.as_str())
}

/// `Ctrl+Shift+K`; a lone modifier is just its name (`Shift`).
pub fn combo(combo: &KeyCombo) -> String {
    let mut parts: Vec<String> = combo.modifiers.iter().map(modifier).collect();
    if !combo.key.is_empty() {
        parts.push(combo.key.clone());
    }
    if parts.is_empty() {
        return "nothing".into();
    }
    parts.join("+")
}

fn modifier(m: &Modifier) -> String {
    format!("{m:?}")
}

/// The last part of a Windows or Unix path without its extension
/// (`C:\Start Menu\Spotify.lnk` -> `Spotify`).
pub fn app_name(path: &str) -> String {
    let file = path
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path);
    match file.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => file.to_string(),
    }
}

/// The host of an http(s) address, or `None` when `arg` is not one.
pub fn website_host(arg: &str) -> Option<&str> {
    let rest = arg
        .strip_prefix("https://")
        .or_else(|| arg.strip_prefix("http://"))?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    // user:pass@host never reaches the log.
    let host = host.rsplit('@').next().unwrap_or(host);
    (!host.is_empty()).then_some(host)
}

fn is_opener(path: &str) -> bool {
    OPENERS
        .iter()
        .any(|o| path.eq_ignore_ascii_case(o) || path.to_lowercase().ends_with(&format!("\\{o}")))
}

/// What a LaunchApp opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target<'a> {
    /// A web address opened in the default browser through the OS opener.
    Website { host: &'a str, url: &'a str },
    /// An app (or file): its name and the path that is run or opened.
    App { name: String, path: &'a str },
}

/// What `path` + `args` (a LaunchApp) opens.
pub fn launch_target<'a>(path: &'a str, args: &'a [String]) -> Target<'a> {
    let first = args.first().map(String::as_str);
    if let Some(url) = first {
        if let Some(host) = website_host(url) {
            return Target::Website { host, url };
        }
        if is_opener(path) {
            return Target::App {
                name: app_name(url),
                path: url,
            };
        }
    }
    Target::App {
        name: app_name(path),
        path,
    }
}

/// The program and arguments a LaunchApp runs, quoted: `explorer.exe
/// "C:\\…\\Spotify.lnk"`. A web address shows only its host (`https://
/// example.com/…`) unless `full`.
pub fn launch_command(path: &str, args: &[String], full: bool) -> String {
    let mut out = path.to_string();
    for arg in args {
        let shown = match website_host(arg) {
            Some(host) if !full => {
                let scheme = if arg.starts_with("http://") {
                    "http"
                } else {
                    "https"
                };
                let bare = format!("{scheme}://{host}");
                if arg.trim_end_matches('/') == bare {
                    bare
                } else {
                    format!("{bare}/…")
                }
            }
            _ => arg.clone(),
        };
        out.push_str(&format!(" \"{shown}\""));
    }
    out
}

/// The short name of what a LaunchApp opens: `"Spotify"` or `example.com`.
pub fn launch_name(path: &str, args: &[String]) -> String {
    match launch_target(path, args) {
        Target::Website { host, .. } => host.to_string(),
        Target::App { name, .. } => format!("\"{name}\""),
    }
}

/// How much of a launch target a summary shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// The app's name and the website's host (config changes, support bundle).
    Short,
    /// Also the app's path (the key-press log line).
    Path,
    /// Also a website's whole address (the key-press line with detailed logs on).
    Full,
}

/// What a mapping does, at `detail`. `layers` names the page a page key
/// goes to.
pub fn action(action: &Action, layers: &[Layer], detail: Detail) -> String {
    match action {
        Action::Hotkey { combo: c } => format!("Press {}", combo(c)),
        Action::Held { combo: c } => format!("Hold {} while held", combo(c)),
        Action::Latched { combo: c } => format!("Turn {} on and off", combo(c)),
        Action::Scroll { amount } => format!("Scroll {amount}"),
        Action::LaunchApp {
            path,
            args,
            enabled,
            ..
        } => {
            let mut s = match launch_target(path, args) {
                Target::Website { url, .. } if detail == Detail::Full => {
                    format!("Open website {url}")
                }
                Target::Website { host, .. } => format!("Open website {host}"),
                Target::App { name, path } if detail != Detail::Short => {
                    format!("Open app \"{name}\" ({path})")
                }
                Target::App { name, .. } => format!("Open app \"{name}\""),
            };
            if !enabled {
                s.push_str(" (off: restored from a file)");
            }
            s
        }
        Action::LayerPush { target } => format!("Go to page \"{}\"", page_name(layers, target)),
        Action::SetJogMode { mode } => format!("Wheel: {}", jog_mode(*mode)),
        Action::OverrideToggle { mode } => {
            format!("Turn the wheel into {} knob", override_mode(*mode))
        }
    }
}

/// `Scroll`, `Jog`, `Shuttle`, `Volume`, `Brightness`.
pub fn jog_mode(mode: JogMode) -> &'static str {
    match mode {
        JogMode::Jog => "Jog",
        JogMode::Scroll => "Scroll",
        JogMode::Shuttle => "Shuttle",
        JogMode::Volume => "Volume",
        JogMode::Brightness => "Brightness",
    }
}

/// `a volume`, `an app volume`, `a brightness`.
pub fn override_mode(mode: OverrideMode) -> &'static str {
    match mode {
        OverrideMode::SystemVolume => "a volume",
        OverrideMode::AppVolume => "an app volume",
        OverrideMode::Brightness => "a brightness",
    }
}

/// The control a dial tick steps: `volume`, `app volume`, `brightness`.
pub fn dial_control(mode: OverrideMode) -> &'static str {
    match mode {
        OverrideMode::SystemVolume => "volume",
        OverrideMode::AppVolume => "app volume",
        OverrideMode::Brightness => "brightness",
    }
}

/// A mapping or none: `Do nothing` when the key is not set.
pub fn maybe_action(a: Option<&Action>, layers: &[Layer]) -> String {
    a.map_or_else(|| "Do nothing".into(), |a| action(a, layers, Detail::Short))
}

/// Every difference between two configs, one line each:
/// `CUT: Do nothing -> Open website example.com`, `page renamed Music ->
/// Edit Bay`, `wheel Scroll threshold 1 -> 2`, `setting auto_update true ->
/// false`. Keys on a page other than the first say which page.
pub fn config_changes(old: &Config, new: &Config) -> Vec<String> {
    let mut out = Vec::new();
    let ids: Vec<&str> = old
        .layers
        .iter()
        .chain(&new.layers)
        .map(|l| l.id.as_str())
        .fold(Vec::new(), |mut v, id| {
            if !v.contains(&id) {
                v.push(id);
            }
            v
        });
    for id in ids {
        let before = old.layers.iter().find(|l| l.id == id);
        let after = new.layers.iter().find(|l| l.id == id);
        match (before, after) {
            (None, Some(l)) => out.push(format!("page added \"{}\"", l.name)),
            (Some(l), None) => out.push(format!("page deleted \"{}\"", l.name)),
            (Some(b), Some(a)) if b.name != a.name => {
                out.push(format!("page renamed {} -> {}", b.name, a.name));
            }
            _ => {}
        }
        let empty = BTreeMap::new();
        let b = before.map_or(&empty, |l| &l.bindings);
        let a = after.map_or(&empty, |l| &l.bindings);
        let keys: BTreeSet<&String> = b.keys().chain(a.keys()).collect();
        let on_page = if id == new.base_layer || after.is_none() {
            String::new()
        } else {
            format!(" on \"{}\"", page_name(&new.layers, id))
        };
        for key in keys {
            let (x, y) = (b.get(key), a.get(key));
            if x != y {
                let (from, to) = (maybe_action(x, &old.layers), maybe_action(y, &new.layers));
                let line = if from == to {
                    // Same app, other arguments (or turned on/off).
                    format!("{}{on_page}: {to} (its details changed)", key_label(key))
                } else {
                    format!("{}{on_page}: {from} -> {to}", key_label(key))
                };
                out.push(line);
            }
        }
    }
    if old.base_layer != new.base_layer {
        out.push(format!(
            "first page {} -> {}",
            page_name(&old.layers, &old.base_layer),
            page_name(&new.layers, &new.base_layer)
        ));
    }
    jog_changes(old, new, &mut out);
    value_changes("dial step", &old.overrides, &new.overrides, &mut out);
    value_changes("setting", &old.settings, &new.settings, &mut out);
    out
}

fn jog_changes(old: &Config, new: &Config, out: &mut Vec<String>) {
    let (o, n) = (&old.jog, &new.jog);
    if o.default_mode != n.default_mode {
        out.push(format!(
            "wheel starts in {} -> {}",
            jog_mode(o.default_mode),
            jog_mode(n.default_mode)
        ));
    }
    let modes: BTreeSet<&JogMode> = o.per_mode.keys().chain(n.per_mode.keys()).collect();
    for mode in modes {
        let m = jog_mode(*mode);
        let (b, a) = (o.per_mode.get(mode), n.per_mode.get(mode));
        let side = |j: Option<&JogBinding>, left: bool, layers: &[Layer]| {
            let a = j.and_then(|j| {
                if left {
                    j.left.as_ref()
                } else {
                    j.right.as_ref()
                }
            });
            maybe_action(a, layers)
        };
        for (name, left) in [("left", true), ("right", false)] {
            let (x, y) = (side(b, left, &old.layers), side(a, left, &new.layers));
            if x != y {
                out.push(format!("wheel {m} {name}: {x} -> {y}"));
            }
        }
        let threshold =
            |j: Option<&JogBinding>| j.map_or("none".into(), |j| j.threshold.to_string());
        if threshold(b) != threshold(a) {
            out.push(format!(
                "wheel {m} threshold {} -> {}",
                threshold(b),
                threshold(a)
            ));
        }
        let rate = |j: Option<&JogBinding>| {
            j.and_then(|j| j.shuttle_max_rate_hz)
                .map_or("default".into(), |r| format!("{r} Hz"))
        };
        if rate(b) != rate(a) {
            out.push(format!("wheel {m} top speed {} -> {}", rate(b), rate(a)));
        }
    }
}

/// `prefix field old -> new` for every top-level field of two values that
/// serialize to JSON objects.
fn value_changes<T: serde::Serialize>(prefix: &str, old: &T, new: &T, out: &mut Vec<String>) {
    let (Ok(serde_json::Value::Object(o)), Ok(serde_json::Value::Object(n))) =
        (serde_json::to_value(old), serde_json::to_value(new))
    else {
        return;
    };
    let fields: BTreeSet<&String> = o.keys().chain(n.keys()).collect();
    for field in fields {
        let (x, y) = (o.get(field), n.get(field));
        if x != y {
            let show = |v: Option<&serde_json::Value>| match v {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => "none".into(),
            };
            out.push(format!("{prefix} {field} {} -> {}", show(x), show(y)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::JogBinding;

    fn hotkey(m: &[Modifier], key: &str) -> Action {
        Action::Hotkey {
            combo: KeyCombo {
                modifiers: m.to_vec(),
                key: key.into(),
            },
        }
    }

    fn launch(path: &str, args: &[&str]) -> Action {
        Action::LaunchApp {
            path: path.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            enabled: true,
            cwd: None,
        }
    }

    #[test]
    fn keys_are_named_as_printed() {
        assert_eq!(key_label("Cut"), "CUT");
        assert_eq!(key_label("Cam1"), "CAM 1");
        assert_eq!(key_label("SmartInsrt"), "SMART INSRT");
        assert_eq!(key_label("Nope"), "Nope");
    }

    #[test]
    fn apps_and_websites_in_words() {
        let layers = Config::default().layers;
        let spotify = launch("explorer.exe", &[r"C:\ProgramData\Start Menu\Spotify.lnk"]);
        assert_eq!(
            action(&spotify, &layers, Detail::Path),
            r#"Open app "Spotify" (C:\ProgramData\Start Menu\Spotify.lnk)"#
        );
        assert_eq!(
            action(&spotify, &layers, Detail::Short),
            r#"Open app "Spotify""#
        );
        let site = launch("explorer.exe", &["https://user:pw@example.com/private?q=1"]);
        assert_eq!(
            action(&site, &layers, Detail::Short),
            "Open website example.com"
        );
        assert_eq!(
            action(&site, &layers, Detail::Path),
            "Open website example.com"
        );
        assert_eq!(
            action(&site, &layers, Detail::Full),
            "Open website https://user:pw@example.com/private?q=1"
        );
        let mac = launch("/usr/bin/open", &["/Applications/Notes.app"]);
        assert_eq!(action(&mac, &layers, Detail::Short), r#"Open app "Notes""#);
        let exe = Action::LaunchApp {
            path: r"C:\Tools\Round Trip.exe".into(),
            args: vec!["--fw".into()],
            enabled: false,
            cwd: None,
        };
        assert_eq!(
            action(&exe, &layers, Detail::Path),
            r#"Open app "Round Trip" (C:\Tools\Round Trip.exe) (off: restored from a file)"#
        );
        assert_eq!(
            launch_name("explorer.exe", &["https://example.com".into()]),
            "example.com"
        );
        let args = vec!["https://example.com/private?q=1".to_string()];
        assert_eq!(
            launch_command("explorer.exe", &args, false),
            r#"explorer.exe "https://example.com/…""#
        );
        assert_eq!(
            launch_command("explorer.exe", &args, true),
            r#"explorer.exe "https://example.com/private?q=1""#
        );
        assert_eq!(
            launch_command("explorer.exe", &["https://example.com".into()], false),
            r#"explorer.exe "https://example.com""#
        );
    }

    #[test]
    fn every_other_kind_in_words() {
        let mut config = Config::default();
        config.layers.push(Layer {
            id: "edit".into(),
            name: "Edit Bay".into(),
            bindings: BTreeMap::new(),
        });
        let l = &config.layers;
        let shift = KeyCombo {
            modifiers: vec![Modifier::Shift],
            key: String::new(),
        };
        for (a, want) in [
            (
                hotkey(&[Modifier::Ctrl, Modifier::Shift], "K"),
                "Press Ctrl+Shift+K",
            ),
            (
                Action::Held {
                    combo: shift.clone(),
                },
                "Hold Shift while held",
            ),
            (Action::Latched { combo: shift }, "Turn Shift on and off"),
            (Action::Scroll { amount: -3 }, "Scroll -3"),
            (
                Action::LayerPush {
                    target: "edit".into(),
                },
                "Go to page \"Edit Bay\"",
            ),
            (
                Action::SetJogMode {
                    mode: JogMode::Volume,
                },
                "Wheel: Volume",
            ),
            (
                Action::OverrideToggle {
                    mode: OverrideMode::SystemVolume,
                },
                "Turn the wheel into a volume knob",
            ),
        ] {
            assert_eq!(action(&a, l, Detail::Path), want);
        }
    }

    #[test]
    fn config_changes_list_each_key_page_and_setting() {
        let old = Config::default();
        let mut new = old.clone();
        new.layers[0].bindings.insert(
            "Cut".into(),
            launch("explorer.exe", &["https://example.com/a?secret"]),
        );
        new.layers[0].name = "Main".into();
        new.layers.push(Layer {
            id: "music".into(),
            name: "Music".into(),
            bindings: BTreeMap::from([("In".to_string(), hotkey(&[], "X"))]),
        });
        new.jog
            .per_mode
            .get_mut(&JogMode::Scroll)
            .unwrap()
            .threshold = 2;
        new.jog.per_mode.insert(
            JogMode::Jog,
            JogBinding {
                left: Some(hotkey(&[], "Left")),
                right: Some(Action::Scroll { amount: 1 }),
                threshold: 1,
                shuttle_max_rate_hz: None,
            },
        );
        new.settings.auto_update = false;
        new.overrides.brightness_step = 7;
        let changes = config_changes(&old, &new);
        assert_eq!(
            changes,
            [
                "page renamed Base -> Main",
                "CUT: Do nothing -> Open website example.com",
                "page added \"Music\"",
                "IN on \"Music\": Do nothing -> Press X",
                "wheel Jog left: Scroll -1 -> Press Left",
                "wheel Scroll threshold 1 -> 2",
                "dial step brightness_step 5 -> 7",
                "setting auto_update true -> false",
            ]
        );
        assert!(config_changes(&new, &new).is_empty());
        let mut args = new.clone();
        args.layers[0].bindings.insert(
            "Cut".into(),
            launch("explorer.exe", &["https://example.com/b"]),
        );
        assert_eq!(
            config_changes(&new, &args),
            ["CUT: Open website example.com (its details changed)"]
        );
        let back = config_changes(&new, &old);
        assert!(
            back.contains(&"page deleted \"Music\"".to_string()),
            "{back:?}"
        );
        assert!(
            back.contains(&"IN: Press X -> Do nothing".to_string()),
            "{back:?}"
        );
        assert!(!changes.iter().any(|c| c.contains("secret")));
    }
}
