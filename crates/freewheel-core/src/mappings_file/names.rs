//! The names a mappings file uses: Speed Editor keys (as printed on the
//! device), modifiers and the keys a shortcut can press. These tables are
//! the single source for the parser, the writer, the JSON Schema and the
//! SCHEMA.md lists (a test fails when SCHEMA.md misses one).

use crate::config::{KeyCombo, Modifier};
use crate::report::Key;

/// Every Speed Editor key in device order (left block row by row, then the
/// middle block, then the right block), with the name a file writes: the
/// text printed on the key, as the drawing's `data-key` shows it.
pub const DEVICE_KEYS: [(Key, &str); 43] = [
    (Key::SmartInsrt, "SMART INSRT"),
    (Key::Appnd, "APPND"),
    (Key::RiplOwr, "RIPL O/WR"),
    (Key::CloseUp, "CLOSE UP"),
    (Key::PlaceOnTop, "PLACE ON TOP"),
    (Key::SrcOwr, "SRC O/WR"),
    (Key::In, "IN"),
    (Key::Out, "OUT"),
    (Key::TrimIn, "TRIM IN"),
    (Key::TrimOut, "TRIM OUT"),
    (Key::Roll, "ROLL"),
    (Key::SlipSrc, "SLIP SRC"),
    (Key::SlipDest, "SLIP DEST"),
    (Key::TransDur, "TRANS DUR"),
    (Key::Cut, "CUT"),
    (Key::Dis, "DIS"),
    (Key::SmthCut, "SMTH CUT"),
    (Key::Esc, "ESC"),
    (Key::SyncBin, "SYNC BIN"),
    (Key::AudioLevel, "AUDIO LEVEL"),
    (Key::FullView, "FULL VIEW"),
    (Key::Trans, "TRANS"),
    (Key::Split, "SPLIT"),
    (Key::Snap, "SNAP"),
    (Key::RiplDel, "RIPL DEL"),
    (Key::Cam7, "CAM7"),
    (Key::Cam8, "CAM8"),
    (Key::Cam9, "CAM9"),
    (Key::LiveOwr, "LIVE O/WR"),
    (Key::Cam4, "CAM4"),
    (Key::Cam5, "CAM5"),
    (Key::Cam6, "CAM6"),
    (Key::VideoOnly, "VIDEO ONLY"),
    (Key::Cam1, "CAM1"),
    (Key::Cam2, "CAM2"),
    (Key::Cam3, "CAM3"),
    (Key::AudioOnly, "AUDIO ONLY"),
    (Key::StopPlay, "STOP/PLAY"),
    (Key::Source, "SOURCE"),
    (Key::Timeline, "TIMELINE"),
    (Key::Shtl, "SHTL"),
    (Key::Jog, "JOG"),
    (Key::Scrl, "SCRL"),
];

/// Letters and digits only, upper case: "Cam 1", "cam1", "CAM-1" and the
/// config id "Cam1" all read as the same key.
fn squash(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// The key a file names, by printed name or config id, any case or spacing.
pub fn device_key(name: &str) -> Option<Key> {
    let want = squash(name);
    DEVICE_KEYS
        .iter()
        .find(|(_, printed)| squash(printed) == want)
        .map(|(k, _)| *k)
}

/// The config id of a key ("Cam1"), as layer bindings store it.
pub fn key_id(key: Key) -> String {
    format!("{key:?}")
}

/// The name a file writes for a config key id, or `None` for a non-key.
pub fn printed_name(id: &str) -> Option<&'static str> {
    DEVICE_KEYS
        .iter()
        .find(|(k, _)| key_id(*k) == id)
        .map(|(_, p)| *p)
}

/// Modifier names: the first is what a file writes on Windows and Linux
/// (macOS writes `Cmd` for Meta); every alias reads, in any case.
pub const MODIFIERS: [(Modifier, &[&str]); 4] = [
    (Modifier::Ctrl, &["Ctrl", "Control", "Ctl"]),
    (Modifier::Shift, &["Shift"]),
    (Modifier::Alt, &["Alt", "Option", "Opt"]),
    (
        Modifier::Meta,
        &["Win", "Cmd", "Command", "Meta", "Super", "Windows"],
    ),
];

/// Which modifier `name` is.
pub fn modifier(name: &str) -> Option<Modifier> {
    let n = name.trim().to_ascii_lowercase();
    MODIFIERS
        .iter()
        .find(|(_, names)| names.iter().any(|a| a.to_ascii_lowercase() == n))
        .map(|(m, _)| *m)
}

/// The name a file writes for `m` on this OS.
pub fn modifier_name(m: Modifier, mac: bool) -> &'static str {
    match m {
        Modifier::Meta if mac => "Cmd",
        m => MODIFIERS
            .iter()
            .find(|(x, _)| *x == m)
            .map_or("?", |(_, n)| n[0]),
    }
}

/// A key a shortcut presses by name: what the config stores (and the
/// injector sends), what a file writes, and every other name that reads.
pub struct NamedKey {
    pub stored: &'static str,
    pub written: &'static str,
    pub aliases: &'static [&'static str],
    pub group: &'static str,
}

macro_rules! named {
    ($group:literal: $($stored:literal $(as $written:literal)? $([$($alias:literal),*])?),* $(,)?) => {
        [$(NamedKey {
            stored: $stored,
            written: named!(@w $stored $($written)?),
            aliases: &[$($($alias),*)?],
            group: $group,
        }),*]
    };
    (@w $stored:literal) => { $stored };
    (@w $stored:literal $written:literal) => { $written };
}

const EDITING: [NamedKey; 14] = named!("Editing and navigation":
    "Enter" ["Return"], "Tab", "Space" ["Spacebar"], "Escape" ["Esc"], "Backspace",
    "Delete" ["Del"], "Insert" ["Ins"], "Home", "End", "PageUp" ["PgUp", "Page Up"],
    "PageDown" ["PgDn", "Page Down"], "CapsLock" ["Caps Lock"], "PrintScreen" ["PrtSc", "Print Screen"],
    "Pause" ["Break"],
);
const ARROWS: [NamedKey; 4] = named!("Arrows":
    "Up" ["ArrowUp", "Up Arrow"], "Down" ["ArrowDown", "Down Arrow"],
    "Left" ["ArrowLeft", "Left Arrow"], "Right" ["ArrowRight", "Right Arrow"],
);
const FUNCTION: [NamedKey; 24] = named!("Function keys":
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
    "F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
);
const NUMPAD: [NamedKey; 18] = named!("Number pad":
    "Num0" ["Numpad0"], "Num1" ["Numpad1"], "Num2" ["Numpad2"], "Num3" ["Numpad3"],
    "Num4" ["Numpad4"], "Num5" ["Numpad5"], "Num6" ["Numpad6"], "Num7" ["Numpad7"],
    "Num8" ["Numpad8"], "Num9" ["Numpad9"], "NumAdd" ["NumPlus", "NumpadAdd"],
    "NumSubtract" ["NumMinus", "NumpadSubtract"], "NumMultiply" ["NumpadMultiply"],
    "NumDivide" ["NumpadDivide"], "NumDecimal" ["NumpadDecimal"], "NumEnter" ["NumpadEnter"],
    "NumLock", "ScrollLock" ["Scroll Lock"],
);
const MEDIA: [NamedKey; 6] = named!("Media keys":
    "PlayPause" as "Play/Pause" ["Play Pause", "Play"],
    "Mute" ["Volume Mute"],
    "VolumeUp" as "Volume up" ["Vol Up"],
    "VolumeDown" as "Volume down" ["Vol Down"],
    "NextTrack" as "Next track" ["Next"],
    "PrevTrack" as "Previous track" ["Prev track", "Previous", "Prev"],
);
/// Punctuation by name; the character itself reads too (`Ctrl+/`).
const PUNCTUATION: [NamedKey; 12] = named!("Punctuation":
    "+" as "Plus", "-" as "Minus" ["Dash", "Hyphen"], "=" as "Equals" ["Equal"],
    "," as "Comma", "." as "Period" ["Dot", "Full stop"], "/" as "Slash",
    "\\" as "Backslash", ";" as "Semicolon", "'" as "Quote" ["Apostrophe"],
    "`" as "Backquote" ["Grave", "Backtick"], "[" as "BracketLeft" ["Left bracket"],
    "]" as "BracketRight" ["Right bracket"],
);

/// Every key a shortcut can press by name, by group, as SCHEMA.md lists them.
pub fn named_keys() -> impl Iterator<Item = &'static NamedKey> {
    EDITING
        .iter()
        .chain(&ARROWS)
        .chain(&FUNCTION)
        .chain(&NUMPAD)
        .chain(&MEDIA)
        .chain(&PUNCTUATION)
}

fn fold(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// The stored name of the key `name` presses: a named key, or one
/// character (letters upper case, as the key panel records them; a letter
/// whose upper case is more than one letter, like `ß`, stays as it is).
pub fn shortcut_key(name: &str) -> Option<String> {
    let name = name.trim();
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_control() {
            return None;
        }
        let mut upper = c.to_uppercase();
        let key = match (upper.next(), upper.next()) {
            (Some(u), None) => u,
            _ => c,
        };
        return Some(key.to_string());
    }
    let want = fold(name);
    named_keys()
        .find(|k| {
            fold(k.stored) == want
                || fold(k.written) == want
                || k.aliases.iter().any(|a| fold(a) == want)
        })
        .map(|k| k.stored.to_string())
}

/// What a file writes for a stored key.
pub fn written_key(stored: &str) -> String {
    named_keys()
        .find(|k| k.stored == stored)
        .map_or_else(|| stored.to_string(), |k| k.written.to_string())
}

/// Writes a combo: `Ctrl+Shift+Z`, `Play/Pause`, `Ctrl+Plus`, `Shift`.
pub fn format_combo(combo: &KeyCombo, mac: bool) -> String {
    let mut parts: Vec<String> = combo
        .modifiers
        .iter()
        .map(|m| modifier_name(*m, mac).to_string())
        .collect();
    if !combo.key.is_empty() {
        parts.push(written_key(&combo.key));
    }
    parts.join("+")
}

/// Why a combo string does not read, with a suggestion when one is close.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComboError(pub String);

/// Reads `Ctrl+Shift+Z`; `lone_modifier` allows `Shift` alone (hold/toggle).
pub fn parse_combo(text: &str, lone_modifier: bool) -> Result<KeyCombo, ComboError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(ComboError("the shortcut is empty".into()));
    }
    // "Ctrl++" and a lone "+" press the + key.
    let (body, plus) = if text == "+" {
        ("", true)
    } else if let Some(rest) = text.strip_suffix("++") {
        (rest, true)
    } else {
        (text, false)
    };
    let mut parts: Vec<&str> = if body.is_empty() {
        Vec::new()
    } else {
        body.split('+').map(str::trim).collect()
    };
    if parts.iter().any(|p| p.is_empty()) {
        return Err(ComboError(format!(
            "\"{text}\" has an empty part between the + signs"
        )));
    }
    let key_part = if plus { None } else { parts.pop() };
    let mut modifiers = Vec::new();
    for part in &parts {
        match modifier(part) {
            Some(m) if modifiers.contains(&m) => {
                return Err(ComboError(format!("{part} is in \"{text}\" twice")));
            }
            Some(m) => modifiers.push(m),
            None => return Err(unknown_part(part, text, true)),
        }
    }
    let key = match key_part {
        None => "+".to_string(),
        Some(last) => match (shortcut_key(last), modifier(last)) {
            (Some(k), _) => k,
            (None, Some(m)) => {
                if modifiers.contains(&m) {
                    return Err(ComboError(format!("{last} is in \"{text}\" twice")));
                }
                modifiers.push(m);
                String::new()
            }
            (None, None) => return Err(unknown_part(last, text, parts.is_empty())),
        },
    };
    if key.is_empty() && !(lone_modifier && modifiers.len() == 1) {
        let hint = if modifiers.len() == 1 {
            format!(" (to keep it down while the key is held, write \"hold {text}\")")
        } else {
            String::new()
        };
        return Err(ComboError(format!(
            "\"{text}\" has no key besides the modifiers{hint}"
        )));
    }
    Ok(KeyCombo { modifiers, key })
}

fn unknown_part(part: &str, text: &str, lone: bool) -> ComboError {
    let mut names: Vec<&str> = MODIFIERS
        .iter()
        .flat_map(|(_, n)| n.iter().copied())
        .collect();
    names.extend(
        named_keys().flat_map(|k| std::iter::once(k.written).chain(k.aliases.iter().copied())),
    );
    let suggestion = closest(part, names.iter().copied())
        .map(|best| text.replacen(part, best, 1))
        .map(|fixed| format!(" (did you mean \"{fixed}\"?)"))
        .unwrap_or_default();
    if lone {
        ComboError(format!("unknown action \"{text}\"{suggestion}"))
    } else {
        ComboError(format!(
            "\"{part}\" in \"{text}\" is not a key freewheel can press{suggestion}"
        ))
    }
}

/// Edit distance between two strings, ignoring case; swapping two
/// neighbouring letters ("Crtl") counts as one edit.
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.to_lowercase().chars().collect();
    let b: Vec<char> = b.to_lowercase().chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = best;
        }
    }
    d[a.len()][b.len()]
}

/// The candidate closest to `word`, if it is close enough to be a typo.
pub fn closest<'a>(word: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let limit = match word.chars().count() {
        0..=4 => 1,
        5..=8 => 2,
        _ => 3,
    };
    candidates
        .map(|c| (distance(word, c), c))
        .filter(|(d, _)| *d <= limit && *d > 0)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_keys_cover_every_key_once_and_read_in_any_spelling() {
        assert_eq!(DEVICE_KEYS.len(), Key::ALL.len());
        for key in Key::ALL {
            let printed = printed_name(&key_id(*key)).unwrap();
            assert_eq!(device_key(printed), Some(*key));
            assert_eq!(device_key(&printed.to_lowercase()), Some(*key));
            assert_eq!(device_key(&key_id(*key)), Some(*key), "old id");
            // The label printed on the device (status.rs) reads too.
            assert_eq!(device_key(crate::status::printed_label(*key)), Some(*key));
        }
        assert_eq!(device_key("cam 1"), Some(Key::Cam1));
        assert_eq!(device_key("Stop/Play"), Some(Key::StopPlay));
        assert_eq!(device_key("TURBO"), None);
    }

    #[test]
    fn combos_read_and_write_back() {
        let c = |m: &[Modifier], k: &str| KeyCombo {
            modifiers: m.to_vec(),
            key: k.into(),
        };
        use Modifier::*;
        let cases = [
            ("Ctrl+Z", c(&[Ctrl], "Z"), "Ctrl+Z"),
            ("ctrl + shift + z", c(&[Ctrl, Shift], "Z"), "Ctrl+Shift+Z"),
            (
                "Cmd+Option+Esc",
                c(&[Meta, Alt], "Escape"),
                "Win+Alt+Escape",
            ),
            ("F24", c(&[], "F24"), "F24"),
            ("Ctrl++", c(&[Ctrl], "+"), "Ctrl+Plus"),
            ("Ctrl+Plus", c(&[Ctrl], "+"), "Ctrl+Plus"),
            ("+", c(&[], "+"), "Plus"),
            ("Ctrl+/", c(&[Ctrl], "/"), "Ctrl+Slash"),
            ("Play/Pause", c(&[], "PlayPause"), "Play/Pause"),
            ("playpause", c(&[], "PlayPause"), "Play/Pause"),
            ("Shift+ArrowLeft", c(&[Shift], "Left"), "Shift+Left"),
            ("Alt+numpad7", c(&[Alt], "Num7"), "Alt+Num7"),
            ("Win+PgDn", c(&[Meta], "PageDown"), "Win+PageDown"),
            ("Ctrl+é", c(&[Ctrl], "É"), "Ctrl+É"),
        ];
        for (text, combo, written) in cases {
            assert_eq!(parse_combo(text, false).unwrap(), combo, "{text}");
            assert_eq!(format_combo(&combo, false), written);
            assert_eq!(parse_combo(written, false).unwrap(), combo);
        }
        assert_eq!(format_combo(&c(&[Meta], "Q"), true), "Cmd+Q");
        assert_eq!(parse_combo("Shift", true).unwrap(), c(&[Shift], ""));
    }

    #[test]
    fn a_one_character_key_stays_one_character() {
        assert_eq!(shortcut_key("k").as_deref(), Some("K"));
        assert_eq!(shortcut_key("ä").as_deref(), Some("Ä"));
        // ß upper-cases to the two letters "SS", which is no key.
        assert_eq!(shortcut_key("ß").as_deref(), Some("ß"));
        assert_eq!(shortcut_key("ẞ").as_deref(), Some("ẞ"));
        assert_eq!(shortcut_key("ŉ").as_deref(), Some("ŉ"));
    }

    #[test]
    fn every_named_key_reads_by_each_of_its_names() {
        for k in named_keys() {
            for name in std::iter::once(k.written).chain(k.aliases.iter().copied()) {
                assert_eq!(shortcut_key(name).as_deref(), Some(k.stored), "{name}");
            }
        }
        let mut stored: Vec<_> = named_keys().map(|k| k.stored).collect();
        stored.sort();
        stored.dedup();
        assert_eq!(stored.len(), named_keys().count());
        for m in crate::inject::MEDIA_KEYS {
            assert!(named_keys().any(|k| k.stored == m), "{m}");
        }
    }

    #[test]
    fn bad_combos_say_why_and_suggest() {
        let err = |t: &str| parse_combo(t, false).unwrap_err().0;
        assert_eq!(
            err("Ctrl+Entr"),
            "\"Entr\" in \"Ctrl+Entr\" is not a key freewheel can press (did you mean \"Ctrl+Enter\"?)"
        );
        assert_eq!(
            err("Crtl+Z"),
            "unknown action \"Crtl+Z\" (did you mean \"Ctrl+Z\"?)"
        );
        assert!(err("Shift").contains("write \"hold Shift\""));
        assert!(err("Ctrl+Ctrl+Z").contains("twice"));
        assert!(err("Ctrl++Z").contains("empty part"));
        assert!(parse_combo("Ctrl+Shift", true).is_err());
    }
}
