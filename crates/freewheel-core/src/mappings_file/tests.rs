use super::open::tests::FakeHost;
use super::*;
use crate::config::{Modifier, Settings};
use crate::report::Key;

fn win() -> FakeHost {
    FakeHost::new(Os::Windows)
        .file(r"C:\Tools\Round Trip.exe")
        .file(r"C:\Tools\run.cmd")
        .var("LOCALAPPDATA", r"C:\Users\me\AppData\Local")
        .app(
            "Spotify",
            "explorer.exe",
            &[r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Spotify.lnk"],
        )
}

fn read(text: &str) -> Result<Imported, SaveError> {
    import(&Config::default(), text.as_bytes(), true, &win())
}

fn read_ok(text: &str) -> Imported {
    read(text).unwrap_or_else(|e| panic!("{e}\n{text}"))
}

fn err(text: &str) -> String {
    read(text).map(|_| ()).unwrap_err().to_string()
}

fn combo(m: &[Modifier], key: &str) -> KeyCombo {
    KeyCombo {
        modifiers: m.to_vec(),
        key: key.into(),
    }
}

fn la(path: &str, args: &[&str], enabled: bool) -> Action {
    Action::LaunchApp {
        path: path.into(),
        args: args.iter().map(|a| a.to_string()).collect(),
        enabled,
        cwd: None,
        per_os: None,
    }
}

const SCHEMA_EXAMPLE: &str = r#"{
  "$schema": "https://raw.githubusercontent.com/brinkercode/freewheel/main/schema/freewheel-mappings.schema.json",
  "version": 2,
  "pages": {
    "Main": {
      "CUT": "Ctrl+Z",
      "CAM1": "open Spotify",
      "CAM2": "https://youtube.com",
      "TRANS": "hold Shift",
      "SNAP": "toggle Alt",
      "ESC": "page Music",
      "STOP/PLAY": "Play/Pause",
      "AUDIO LEVEL": "wheel volume"
    },
    "Music": { "ESC": "back" }
  },
  "wheel": { "mode": "scroll", "speed": 5 }
}"#;

#[test]
fn the_schema_md_example_reads_as_the_keys_it_says() {
    let got = read_ok(SCHEMA_EXAMPLE);
    assert!(got.notes.is_empty(), "{:?}", got.notes);
    let c = got.config;
    assert_eq!(
        c.layers
            .iter()
            .map(|l| (l.id.as_str(), l.name.as_str()))
            .collect::<Vec<_>>(),
        [("base", "Main"), ("music", "Music")]
    );
    assert_eq!(c.base_layer, "base");
    let b = &c.layers[0].bindings;
    use Modifier::*;
    assert_eq!(
        b["Cut"],
        Action::Hotkey {
            combo: combo(&[Ctrl], "Z")
        }
    );
    assert_eq!(
        b["Cam1"],
        la(
            "explorer.exe",
            &[r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Spotify.lnk"],
            true
        )
    );
    assert_eq!(
        b["Cam2"],
        la("explorer.exe", &["https://youtube.com"], true)
    );
    assert_eq!(
        b["Trans"],
        Action::Held {
            combo: combo(&[Shift], "")
        }
    );
    assert_eq!(
        b["Snap"],
        Action::Latched {
            combo: combo(&[Alt], "")
        }
    );
    assert_eq!(
        b["Esc"],
        Action::LayerPush {
            target: "music".into()
        }
    );
    assert_eq!(
        b["StopPlay"],
        Action::Hotkey {
            combo: combo(&[], "PlayPause")
        }
    );
    assert_eq!(
        b["AudioLevel"],
        Action::OverrideToggle {
            mode: OverrideMode::SystemVolume
        }
    );
    assert!(
        c.layers[1].bindings.is_empty(),
        "back is the push key's own job"
    );
    assert_eq!(c.jog.default_mode, JogMode::Scroll);
    assert!(c.jog.per_mode.values().all(|b| b.threshold == 6));
    assert_eq!(c.overrides, OverrideSettings::default());
    assert_eq!(c.settings, Settings::default());
    // ...and writes back the same way.
    let again = String::from_utf8(export(&c, &win())).unwrap();
    assert!(again.contains("\"CAM1\": \"open Spotify\""), "{again}");
    assert!(again.contains("\"ESC\": \"back\""), "{again}");
    assert_eq!(read_ok(&again).config, c);
}

/// A config that uses every action kind and every key.
fn everything() -> Config {
    use Modifier::*;
    let kinds = [
        Action::Hotkey {
            combo: combo(&[Ctrl, Shift], "S"),
        },
        Action::Hotkey {
            combo: combo(&[], "F24"),
        },
        Action::Hotkey {
            combo: combo(&[Meta], "+"),
        },
        Action::Hotkey {
            combo: combo(&[Alt], "Num7"),
        },
        Action::Hotkey {
            combo: combo(&[], "Mute"),
        },
        Action::Held {
            combo: combo(&[Shift], ""),
        },
        Action::Held {
            combo: combo(&[Ctrl], "Space"),
        },
        Action::Latched {
            combo: combo(&[Alt], ""),
        },
        Action::Scroll { amount: -1 },
        Action::Scroll { amount: 1 },
        Action::Scroll { amount: -7 },
        la(r"C:\Tools\Round Trip.exe", &["--fw", "a b"], true),
        la(
            "explorer.exe",
            &[r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Spotify.lnk"],
            true,
        ),
        la("explorer.exe", &["https://example.com/a?b=c"], true),
        Action::LaunchApp {
            path: r"C:\Tools\run.cmd".into(),
            args: vec!["x".into()],
            enabled: true,
            cwd: Some(r"C:\Tools".into()),
            per_os: None,
        },
        Action::LayerPush {
            target: "studio".into(),
        },
        Action::SetJogMode {
            mode: JogMode::Shuttle,
        },
        Action::SetJogMode {
            mode: JogMode::Volume,
        },
        Action::OverrideToggle {
            mode: OverrideMode::AppVolume,
        },
        Action::OverrideToggle {
            mode: OverrideMode::Brightness,
        },
    ];
    let mut config = Config::default();
    config.layers[0].name = "Main".into();
    let mut studio = Layer {
        id: "studio".into(),
        name: "Studio".into(),
        bindings: BTreeMap::new(),
    };
    for (i, key) in Key::ALL.iter().enumerate() {
        let action = kinds[i % kinds.len()].clone();
        config.layers[0]
            .bindings
            .insert(key_id(*key), action.clone());
        if !matches!(action, Action::LayerPush { .. }) {
            studio.bindings.insert(key_id(*key), action);
        }
    }
    config.layers.push(studio);
    config.jog.default_mode = JogMode::Jog;
    let jog = config.jog.per_mode.get_mut(&JogMode::Jog).unwrap();
    jog.left = Some(Action::Hotkey {
        combo: combo(&[], "Left"),
    });
    jog.right = None;
    jog.threshold = 3;
    config
        .jog
        .per_mode
        .get_mut(&JogMode::Shuttle)
        .unwrap()
        .shuttle_max_rate_hz = Some(8);
    config.overrides = OverrideSettings {
        system_volume_step: 0.05,
        app_volume_step: 0.025,
        brightness_step: 7,
    };
    config
}

#[test]
fn every_action_kind_and_every_key_round_trips() {
    let config = everything();
    crate::config::validate(&config).unwrap();
    let file = String::from_utf8(export(&config, &win())).unwrap();
    for (_, printed) in names::DEVICE_KEYS {
        assert!(
            file.contains(&format!("\"{printed}\": ")),
            "{printed} missing:\n{file}"
        );
    }
    for part in [
        "\"Ctrl+Shift+S\"",
        "\"F24\"",
        "\"Win+Plus\"",
        "\"Alt+Num7\"",
        "\"Mute\"",
        "\"hold Shift\"",
        "\"hold Ctrl+Space\"",
        "\"toggle Alt\"",
        "\"scroll up\"",
        "\"scroll down\"",
        "\"scroll -7\"",
        "\"open Spotify\"",
        "\"https://example.com/a?b=c\"",
        "\"page Studio\"",
        "\"wheel mode shuttle\"",
        "\"wheel mode volume\"",
        "\"wheel app volume\"",
        "\"wheel brightness\"",
        "\"cwd\": \"C:\\\\Tools\"",
        "\"args\": [\"--fw\", \"a b\"]",
        "\"mode\": \"jog\"",
        "\"max_rate\": 8",
        "\"volume_step\": 5",
        "\"app_volume_step\": 2.5",
        "\"brightness_step\": 7",
        "\"right\": \"nothing\"",
    ] {
        assert!(file.contains(part), "{part} missing:\n{file}");
    }
    let back = read_ok(&file);
    assert_eq!(back.config, config, "\n{file}");
    // The file is valid JSON for any other tool too.
    let value: serde_json::Value = serde_json::from_str(&file).unwrap();
    assert_eq!(value["version"], 2);
    assert_eq!(value["$schema"], SCHEMA_URL);
}

#[test]
fn keys_read_by_printed_name_any_case_and_by_old_id_and_write_in_device_order() {
    let file = r#"{"pages": {"P": {"cam 1": "A", "Stop/Play": "B", "SmartInsrt": "C", "trim out": "D", "SCRL": "E"}}}"#;
    let c = read_ok(file).config;
    let b = &c.layers[0].bindings;
    for (id, k) in [
        ("Cam1", "A"),
        ("StopPlay", "B"),
        ("SmartInsrt", "C"),
        ("TrimOut", "D"),
        ("Scrl", "E"),
    ] {
        assert_eq!(
            b[id],
            Action::Hotkey {
                combo: combo(&[], k)
            }
        );
    }
    let out = String::from_utf8(export(&c, &win())).unwrap();
    let order: Vec<usize> = ["SMART INSRT", "TRIM OUT", "CAM1", "STOP/PLAY", "SCRL"]
        .iter()
        .map(|k| out.find(&format!("\"{k}\"")).unwrap())
        .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{out}");
}

#[test]
fn omitted_sections_take_the_defaults_and_settings_stay() {
    let mut current = Config::default();
    current.settings.appearance = Appearance::Dark;
    current.overrides.brightness_step = 9;
    let got = import(&current, b"{}", true, &win()).unwrap().config;
    assert_eq!(got.layers, Config::default().layers);
    assert_eq!(got.jog, JogSettings::default());
    assert_eq!(got.overrides, OverrideSettings::default());
    assert_eq!(got.settings, current.settings);
    let got = import(
        &current,
        br#"{"wheel": {"jog": {"left": "Left"}}}"#,
        true,
        &win(),
    )
    .unwrap()
    .config;
    let jog = &got.jog.per_mode[&JogMode::Jog];
    assert_eq!(
        jog.left,
        Some(Action::Hotkey {
            combo: combo(&[], "Left")
        })
    );
    assert_eq!(jog.right, Some(Action::Scroll { amount: 1 }));
    assert_eq!(
        got.jog.per_mode[&JogMode::Scroll],
        JogSettings::default().per_mode[&JogMode::Scroll]
    );
    // A default config writes a short file.
    let short = String::from_utf8(export(&Config::default(), &win())).unwrap();
    assert_eq!(
        short,
        format!(
            "{{\n  \"$schema\": \"{SCHEMA_URL}\",\n  \"version\": 2,\n  \"pages\": {{\n    \"Base\": {{}}\n  }},\n  \"wheel\": {{\n    \"mode\": \"scroll\",\n    \"speed\": 10\n  }}\n}}\n"
        )
    );
}

#[test]
fn settings_fields_change_only_what_they_name() {
    let file = r#"{"settings": {"appearance": "Dark", "update_automatically": false, "step_aside_for_resolve": false,
        "resolve_program": "Resolve Beta.exe", "low_battery_warning": 30, "detailed_logs": true}}"#;
    let s = read_ok(file).config.settings;
    assert_eq!(s.appearance, Appearance::Dark);
    assert!(!s.auto_update && !s.step_aside_for_resolve && s.detailed_logs);
    assert_eq!(s.resolve_process_name, "Resolve Beta.exe");
    assert_eq!(s.low_battery_threshold, 30);
    assert_eq!(s.enabled, Settings::default().enabled);
    let s = read_ok(r#"{"settings": {"appearance": "light"}}"#)
        .config
        .settings;
    assert_eq!(
        Settings {
            appearance: Appearance::Light,
            ..Settings::default()
        },
        s
    );
}

#[test]
fn a_version_1_file_still_reads_and_migrates_to_the_short_form() {
    let mut v1 = everything();
    // v1 files carry no cwd (it is new) and the long jog blocks.
    for l in &mut v1.layers {
        for a in l.bindings.values_mut() {
            if let Action::LaunchApp { cwd, .. } = a {
                *cwd = None;
            }
        }
    }
    let long = v1.export_mappings();
    assert!(String::from_utf8_lossy(&long).contains("\"Hotkey\""));
    let read = import(&Config::default(), &long, true, &win()).unwrap();
    assert_eq!(read.config.layers, v1.layers);
    assert_eq!(read.config.jog, v1.jog);
    let short = export(&read.config, &win());
    assert!(
        short.len() * 3 < long.len(),
        "{} vs {}",
        short.len(),
        long.len()
    );
    assert_eq!(
        import(&Config::default(), &short, true, &win())
            .unwrap()
            .config,
        read.config
    );
    // A v1 file without the opt-in still comes in off.
    let off = import(&Config::default(), &long, false, &win())
        .unwrap()
        .config;
    assert!(
        off.layers[0]
            .bindings
            .values()
            .all(|a| !matches!(a, Action::LaunchApp { enabled: true, .. }))
    );
}

#[test]
fn errors_name_the_line_column_and_key_and_suggest_a_fix() {
    let file = SCHEMA_EXAMPLE.replace("\"open Spotify\"", "\"opn Spotify\"");
    assert_eq!(
        err(&file),
        "line 7, column 15, CAM1 on Main: unknown action \"opn Spotify\" (did you mean \"open Spotify\"?)"
    );
    let cases: &[(&str, &str)] = &[
        (
            r#"{"pages": {"Main": {"CUTT": "Ctrl+Z"}}}"#,
            "line 1, column 21, page Main: \"CUTT\" is not a Speed Editor key (did you mean \"CUT\"?)",
        ),
        (
            r#"{"pages": {"Main": {"CUT": "Ctrl+Entr"}}}"#,
            "line 1, column 28, CUT on Main: \"Entr\" in \"Ctrl+Entr\" is not a key freewheel can press (did you mean \"Ctrl+Enter\"?)",
        ),
        (
            r#"{"pages": {"Main": {"CUT": "Crtl+Z"}}}"#,
            "line 1, column 28, CUT on Main: unknown action \"Crtl+Z\" (did you mean \"Ctrl+Z\"?)",
        ),
        (
            r#"{"pages": {"Main": {"CUT": "hold Shft"}}}"#,
            "line 1, column 28, CUT on Main: unknown shortcut \"Shft\" (did you mean \"hold Shift\"?)",
        ),
        (
            r#"{"pages": {"Main": {"CUT": "Shift"}}}"#,
            "line 1, column 28, CUT on Main: \"Shift\" has no key besides the modifiers (to keep it down while the key is held, write \"hold Shift\")",
        ),
        (
            r#"{"pages": {"Main": {"ESC": "page Musc"}, "Music": {}}}"#,
            "line 1, column 28, ESC on Main: there is no page \"Musc\" in the file (did you mean \"page music\"?)",
        ),
        (
            r#"{"pages": {"Main": {}, "Music": {"ESC": "back"}}}"#,
            "line 1, column 41, ESC on Music: \"back\" works only on the key that opens this page: write \"ESC\": \"page Music\" on another page, or pick another action",
        ),
        (
            r#"{"pages": {"Main": {"CUT": "scroll 0"}}}"#,
            "line 1, column 28, CUT on Main: \"scroll\" needs up, down or a whole number from -100 to 100 (not 0), like \"scroll -3\", not \"scroll 0\"",
        ),
        (
            r#"{"pages": {"Main": {"CUT": "wheel volum"}}}"#,
            "line 1, column 28, CUT on Main: unknown action \"wheel volum\" (did you mean \"wheel volume\"?); \"wheel\" takes volume, app volume, brightness or mode <mode>",
        ),
        (
            r#"{"pages": {"Main": {"CUT": 5}}}"#,
            "line 1, column 28, CUT on Main: an action is text like \"Ctrl+Z\" or an { \"open\": ... } object, not a number",
        ),
        (
            r#"{"pages": {"Main": {"CUT": "A", "cut": "B"}}}"#,
            "line 1, column 33, page Main: CUT is on this page twice (as \"CUT\" and \"cut\")",
        ),
        (
            r#"{"pages": {"Main": {}, "main": {}}}"#,
            "line 1, column 24, pages: the page \"main\" is in the file twice (\"Main\" before it; names ignore case)",
        ),
        (
            r#"{"pages": {}}"#,
            "line 1, column 11, pages: pages is empty: write at least one page, like \"Main\": {}",
        ),
        (
            r#"{"wheel": {"sped": 5}}"#,
            "line 1, column 12, wheel: unknown field \"sped\" (did you mean \"speed\"?)",
        ),
        (
            r#"{"wheel": {"speed": 11}}"#,
            "line 1, column 21, wheel.speed: wheel.speed must be a whole number from 1 to 10, not 11",
        ),
        (
            r#"{"wheel": {"mode": "jgo"}}"#,
            "line 1, column 20, wheel.mode: wheel.mode must be one of jog, scroll, shuttle, volume, brightness, not \"jgo\" (did you mean \"jog\"?)",
        ),
        (
            r#"{"wheel": {"jog": {"left": "back"}}}"#,
            "line 1, column 28, wheel.jog.left: \"back\" works only on a page's key, not on the wheel",
        ),
        (
            r#"{"dial": {"volume_step": 0}}"#,
            "line 1, column 26, dial.volume_step: dial.volume_step must be a number from 0.1 to 100, not 0",
        ),
        (
            r#"{"settings": {"appearance": "blue"}}"#,
            "line 1, column 29, settings.appearance: settings.appearance must be one of system, light, dark, not \"blue\"",
        ),
        (
            r#"{"setings": {}}"#,
            "line 1, column 2, the file: unknown field \"setings\" (did you mean \"settings\"?)",
        ),
        (
            r#"{"pages": {"Main": {"CUT": {"open": "x", "arg": []}}}}"#,
            "line 1, column 42, CUT on Main: unknown field \"arg\" (did you mean \"args\"?)",
        ),
        (
            r#"{"pages": {"Main": {"CUT": {"open": "x", "args": "y"}}}}"#,
            "line 1, column 50, CUT on Main: args must be a list of text, like [\"--flag\", \"file.txt\"], not a string",
        ),
        (
            r#"{"pages": {"Main": {"CUT": {"open": {"window": "x"}}}}}"#,
            "line 1, column 38, CUT on Main: unknown field \"window\" (did you mean \"windows\"?)",
        ),
        (
            r#"{"pages": {"Main": {"CUT": {"open": "https://a.org", "args": ["x"]}}}}"#,
            "line 1, column 28, CUT on Main: a web address cannot take args or cwd",
        ),
        (
            r#"{"pages": {"Main": {"CUT": "open "}}}"#,
            "line 1, column 28, CUT on Main: \"open\" needs an app name or a path, like \"open Spotify\"",
        ),
        (
            r#"{"pages": {"Main": {"CUT": "do something"}}}"#,
            "line 1, column 28, CUT on Main: unknown action \"do something\"; an action starts with open, hold, toggle, page, scroll or wheel, or is a shortcut like Ctrl+Z (see SCHEMA.md)",
        ),
        (
            r#"{"version": 0}"#,
            "line 1, column 13, version: version 0 does not exist: write 2",
        ),
    ];
    for (text, want) in cases {
        assert_eq!(&err(text), want, "{text}");
    }
    // Syntax errors are Malformed, with the line and column.
    assert_eq!(
        read("{\n  \"pages\": {\n    \"Main\": { \"CUT\": \"C:\\Tools\" }\n  }\n}"),
        Err(SaveError::Malformed { message: "line 3, column 25: a backslash in text must be doubled (write C:\\\\Tools, not C:\\Tools)".into() })
    );
    assert!(
        matches!(read("[1]"), Err(SaveError::Malformed { message }) if message.contains("one { ... } object"))
    );
    assert_eq!(
        read(r#"{"version": 3, "pages": "future"}"#),
        Err(SaveError::VersionMismatch {
            found: 3,
            expected: 2
        })
    );
    // The UI gets the parts, tagged.
    let e = read(&file).unwrap_err();
    assert_eq!(
        serde_json::to_value(&e).unwrap(),
        serde_json::json!({"kind": "InvalidFile", "line": 7, "column": 15, "at": "CAM1 on Main", "message": "unknown action \"opn Spotify\" (did you mean \"open Spotify\"?)"})
    );
}

#[test]
fn opening_apps_by_path_env_var_name_object_and_per_os() {
    let file = r#"{"pages": {"Main": {
        "CAM1": "open C:\\Tools\\Round Trip.exe",
        "CAM2": "open %LOCALAPPDATA%\\..\\..\\..\\..\\Tools\\run.cmd",
        "CAM3": {"open": "C:\\Tools\\run.cmd", "args": ["--flag", "file.txt"], "cwd": "%LOCALAPPDATA%"},
        "CAM4": {"open": {"windows": "Spotify", "macos": "com.spotify.client", "linux": "spotify.desktop"}},
        "CAM5": "open Photoshop",
        "CAM6": {"open": {"macos": "com.apple.Safari"}}
    }}}"#;
    let got = read_ok(file);
    let b = &got.config.layers[0].bindings;
    assert_eq!(b["Cam1"], la(r"C:\Tools\Round Trip.exe", &[], true));
    assert_eq!(
        b["Cam3"],
        Action::LaunchApp {
            path: r"C:\Tools\run.cmd".into(),
            args: vec!["--flag".into(), "file.txt".into()],
            enabled: true,
            cwd: Some(r"C:\Users\me\AppData\Local".into()),
            per_os: None,
        }
    );
    assert_eq!(
        b["Cam4"],
        Action::LaunchApp {
            path: "explorer.exe".into(),
            args: vec![r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Spotify.lnk".into()],
            enabled: true,
            cwd: None,
            per_os: Some(crate::config::PerOsTargets {
                windows: Some("Spotify".into()),
                macos: Some("com.spotify.client".into()),
                linux: Some("spotify.desktop".into()),
            }),
        }
    );
    assert_eq!(b["Cam5"], la("Photoshop", &[], false));
    assert!(matches!(
        &b["Cam6"],
        Action::LaunchApp { enabled: false, .. }
    ));
    assert_eq!(
        got.notes,
        [
            "CAM2 on Main: there is no app or file called %LOCALAPPDATA%\\..\\..\\..\\..\\Tools\\run.cmd (C:\\Users\\me\\AppData\\Local\\..\\..\\..\\..\\Tools\\run.cmd) on this computer. The key is off until you pick the app again.",
            "CAM5 on Main: there is no app or file called Photoshop on this computer. The key is off until you pick the app again.",
            "CAM6 on Main: the file has no \"windows\" app to open on this computer. The key is off.",
        ]
    );
    // An off key keeps what the file said, so a backup writes it back.
    let out = String::from_utf8(export(&got.config, &win())).unwrap();
    assert!(out.contains("\"CAM5\": \"open Photoshop\""), "{out}");
}

const THREE_OS: &str = r#"{
  "version": 2,
  "pages": {
    "Main": {
      "CAM1": { "open": { "windows": "C:\\Tools\\run.cmd", "macos": "com.spotify.client", "linux": "spotify.desktop" }, "args": ["--fast"], "cwd": "C:\\Work" },
      "CAM2": { "open": { "windows": "Spotify", "macos": "/Applications/Spotify.app", "linux": "/usr/bin/spotify" } },
      "CAM3": { "open": { "macos": "com.apple.Safari", "linux": "firefox" } }
    }
  }
}"#;

/// The `open` of every key on the first page of an exported file.
fn opens(file: &[u8]) -> Vec<(String, serde_json::Value)> {
    let v: serde_json::Value = serde_json::from_slice(file).unwrap();
    let page = v["pages"].as_object().unwrap().values().next().unwrap();
    page.as_object()
        .unwrap()
        .iter()
        .map(|(k, a)| (k.clone(), a.clone()))
        .collect()
}

#[test]
fn a_three_os_file_restored_on_windows_backs_up_with_every_os() {
    let got = read_ok(THREE_OS);
    let b = &got.config.layers[0].bindings;
    // This computer runs its own entry ...
    assert!(
        matches!(&b["Cam1"], Action::LaunchApp { path, args, enabled: true, .. }
        if path == r"C:\Tools\run.cmd" && args == &["--fast"])
    );
    assert!(
        matches!(&b["Cam2"], Action::LaunchApp { path, enabled: true, .. } if path == "explorer.exe")
    );
    assert!(matches!(
        &b["Cam3"],
        Action::LaunchApp { enabled: false, .. }
    ));
    // ... and config.json keeps the others, so a restart loses nothing.
    let saved = serde_json::to_string(&got.config).unwrap();
    let reloaded: Config = serde_json::from_str(&saved).unwrap();
    assert_eq!(reloaded, got.config);
    let out = export(&got.config, &win());
    let want: serde_json::Value = serde_json::from_str(THREE_OS).unwrap();
    let want: Vec<(String, serde_json::Value)> = want["pages"]["Main"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, a)| (k.clone(), a.clone()))
        .collect();
    assert_eq!(opens(&out), want, "{}", String::from_utf8_lossy(&out));
    // The exported file restores to the same keys, and exports the same again.
    let again = import(&Config::default(), &out, true, &win()).unwrap();
    assert_eq!(
        again.config.layers[0].bindings,
        got.config.layers[0].bindings
    );
    assert_eq!(export(&again.config, &win()), out);
}

#[test]
fn picking_another_app_here_changes_only_this_os_entry() {
    let mut got = read_ok(THREE_OS).config;
    if let Some(Action::LaunchApp { path, args, .. }) = got.layers[0].bindings.get_mut("Cam2") {
        *path = r"C:\Tools\Round Trip.exe".into();
        args.clear();
    }
    let out = opens(&export(&got, &win()));
    assert_eq!(
        out[1].1["open"],
        serde_json::json!({ "windows": "C:\\Tools\\Round Trip.exe", "macos": "/Applications/Spotify.app", "linux": "/usr/bin/spotify" })
    );
}

#[test]
fn without_the_opt_in_every_app_and_web_page_comes_in_off_with_a_note() {
    let got = import(&Config::default(), SCHEMA_EXAMPLE.as_bytes(), false, &win()).unwrap();
    let b = &got.config.layers[0].bindings;
    assert!(matches!(
        &b["Cam1"],
        Action::LaunchApp { enabled: false, .. }
    ));
    assert!(matches!(
        &b["Cam2"],
        Action::LaunchApp { enabled: false, .. }
    ));
    assert_eq!(
        got.notes,
        [
            "2 keys that open apps or web pages came in off: turn on “Let restored keys open apps” and restore again to keep them on."
        ]
    );
    let file = r#"{"wheel": {"jog": {"left": "https://a.org"}}}"#;
    let got = import(&Config::default(), file.as_bytes(), false, &win()).unwrap();
    assert!(matches!(
        got.config.jog.per_mode[&JogMode::Jog].left,
        Some(Action::LaunchApp { enabled: false, .. })
    ));
}

#[test]
fn page_ids_stay_when_restoring_onto_the_same_pages() {
    let mut current = everything();
    current.layers[1].name = "Edit Bay".into();
    let file = export(&current, &win());
    let back = import(&current, &file, true, &win()).unwrap().config;
    assert_eq!(back.layers, current.layers);
    // On a fresh computer the ids are made from the names; nothing else changes.
    let fresh = import(&Config::default(), &file, true, &win())
        .unwrap()
        .config;
    assert_eq!(fresh.layers[1].id, "edit-bay");
    assert!(fresh.layers[0].bindings.values().any(|a| *a
        == Action::LayerPush {
            target: "edit-bay".into()
        }));
    assert!(
        !fresh.layers[0]
            .bindings
            .values()
            .any(|a| matches!(a, Action::LayerPush { target } if target == "studio"))
    );
}

#[test]
fn export_on_a_mac_writes_cmd() {
    let mut c = Config::default();
    c.layers[0].bindings.insert(
        "Cut".into(),
        Action::Hotkey {
            combo: combo(&[Modifier::Meta], "Z"),
        },
    );
    let mac = FakeHost::new(Os::MacOs);
    let out = String::from_utf8(export(&c, &mac)).unwrap();
    assert!(out.contains("\"CUT\": \"Cmd+Z\""), "{out}");
    assert_eq!(
        import(&Config::default(), out.as_bytes(), true, &mac)
            .unwrap()
            .config
            .layers,
        c.layers
    );
}

/// Every action kind has a written form SCHEMA.md documents.
#[test]
fn every_action_kind_is_documented() {
    let w = Writer {
        host: &win(),
        name_of: &|_: &str| Some("P".into()),
    };
    for action in everything().layers[0].bindings.values() {
        // A new Action variant fails to compile here until it is handled.
        let form = match action {
            Action::Hotkey { .. } => "<shortcut>",
            Action::Held { .. } => "hold <shortcut>",
            Action::Latched { .. } => "toggle <shortcut>",
            Action::Scroll { .. } => "scroll <n>",
            Action::LaunchApp { .. } => "open <app or path>",
            Action::LayerPush { .. } => "page <name>",
            Action::SetJogMode { .. } => "wheel mode <mode>",
            Action::OverrideToggle { .. } => "wheel volume",
        };
        assert!(ACTIONS.iter().any(|a| a.form == form), "{form}");
        assert!(w.action(action).is_some());
    }
}

fn repo_file(rel: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

#[test]
fn the_committed_json_schema_is_up_to_date() {
    let path = repo_file("schema/freewheel-mappings.schema.json");
    let want = schema::schema_text();
    if std::env::var_os("FREEWHEEL_WRITE_SCHEMA").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &want).unwrap();
    }
    let have = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        have == want,
        "schema/freewheel-mappings.schema.json is stale: run FREEWHEEL_WRITE_SCHEMA=1 cargo test -p freewheel-core mappings_file"
    );
}

#[test]
fn schema_md_lists_every_key_action_name_and_field() {
    let path = repo_file("SCHEMA.md");
    let have = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n");
    let want = schema::with_blocks(&have).unwrap();
    let writing = std::env::var_os("FREEWHEEL_WRITE_SCHEMA").is_some();
    if writing && want != have {
        std::fs::write(&path, &want).unwrap();
    }
    assert!(
        writing || have == want,
        "SCHEMA.md's generated lists are stale: run FREEWHEEL_WRITE_SCHEMA=1 cargo test -p freewheel-core mappings_file"
    );
    // Beyond the generated tables: the prose covers each of these.
    for (_, printed) in names::DEVICE_KEYS {
        assert!(have.contains(&format!("`{printed}`")), "{printed}");
    }
    for k in names::named_keys() {
        assert!(
            have.contains(&format!("`{}`", k.written.replace('|', "\\|"))),
            "{}",
            k.written
        );
    }
    for f in FIELDS {
        assert!(have.contains(&format!("`{}`", f.path)), "{}", f.path);
    }
    for verb in VERBS {
        assert!(have.contains(&format!("`{verb}")), "{verb}");
    }
    for part in [
        "Let restored keys open apps",
        "How open finds the app",
        "line 7, column 15, CAM1 on Main",
        "%LOCALAPPDATA%",
        "com.spotify.client",
        ".desktop",
        "\"windows\"",
    ] {
        assert!(have.contains(part), "SCHEMA.md lacks {part}");
    }
}

#[test]
fn the_schema_patterns_are_valid_regexes_for_what_the_parser_reads() {
    // A light check here (no regex crate): the patterns are anchored and
    // built from the same tables. scripts/mappings-schema.test.js runs the
    // schema with ajv against fixtures.
    assert!(schema::action_pattern(true).starts_with("^\\s*(?:"));
    assert!(
        schema::key_name_pattern().contains("[Cc][^A-Za-z0-9]*[Aa][^A-Za-z0-9]*[Mm][^A-Za-z0-9]*1")
    );
}

#[test]
fn the_examples_in_schema_md_restore() {
    let doc = std::fs::read_to_string(repo_file("SCHEMA.md")).unwrap();
    let files: Vec<&str> = doc
        .split("```json\n")
        .skip(1)
        .filter_map(|b| b.split("\n```").next())
        .filter(|b| b.starts_with("{\n  \"$schema\""))
        .collect();
    assert_eq!(files.len(), 2);
    for file in files {
        let got = import(&Config::default(), file.as_bytes(), true, &win())
            .unwrap_or_else(|e| panic!("{e}\n{file}"));
        assert!(got.config.layers[0].bindings.len() >= 8);
    }
}

/// The fixtures the JSON Schema test (scripts/mappings-schema.test.js) runs
/// through ajv: the parser agrees with the schema on each, and the exported
/// one (every action kind and key) is what export writes now.
#[test]
fn the_parser_agrees_with_the_schema_fixtures() {
    let dir = repo_file("schema/fixtures");
    let exported = dir.join("valid/exported-everything.json");
    let want = String::from_utf8(export(&everything(), &win())).unwrap();
    if std::env::var_os("FREEWHEEL_WRITE_SCHEMA").is_some() {
        std::fs::write(&exported, &want).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&exported)
            .unwrap()
            .replace("\r\n", "\n"),
        want,
        "run FREEWHEEL_WRITE_SCHEMA=1 cargo test -p freewheel-core mappings_file"
    );
    let mut seen = 0;
    for (sub, ok) in [("valid", true), ("invalid", false)] {
        for entry in std::fs::read_dir(dir.join(sub)).unwrap() {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            let result = import(&Config::default(), text.as_bytes(), true, &win());
            assert_eq!(result.is_ok(), ok, "{}: {:?}", path.display(), result.err());
            seen += 1;
        }
    }
    assert!(seen >= 20, "{seen}");
}

const PROFILES_FILE: &str = r#"{
  "version": 2,
  "pages": {
    "Main": { "CUT": "Ctrl+Z", "ESC": "page Music" },
    "Music": { "ESC": "back" }
  },
  "profiles": [
    {
      "name": "Writing",
      "apps": ["notepad.exe", { "name": "Word", "windows": "WINWORD.EXE", "macos": "com.microsoft.Word" }],
      "pages": {
        "Main": { "CUT": "Ctrl+Shift+X", "ESC": "page Review" },
        "Review": { "ESC": "back" }
      }
    },
    { "name": "Paint", "apps": [{ "windows": "mspaint.exe" }], "pages": { "Canvas": {} } }
  ]
}"#;

#[test]
fn profiles_read_with_their_own_pages_and_apps_and_round_trip() {
    let c = read_ok(PROFILES_FILE).config;
    // Default: the pages as 1.0.0 had them.
    let default: Vec<&str> = c.pages_of(None).iter().map(|l| l.name.as_str()).collect();
    assert_eq!(default, ["Main", "Music"]);
    assert_eq!(c.base_layer, "base");
    let writing = &c.profiles[0];
    assert_eq!(
        (writing.id.as_str(), writing.name.as_str()),
        ("profile-writing", "Writing")
    );
    assert_eq!(
        writing.pages,
        ["profile-writing-main", "profile-writing-review"]
    );
    assert_eq!(
        writing.apps,
        [
            ProfileApp {
                name: "notepad.exe".into(),
                windows: Some("notepad.exe".into()),
                macos: Some("notepad.exe".into()),
                linux: Some("notepad.exe".into()),
            },
            ProfileApp {
                name: "Word".into(),
                windows: Some("WINWORD.EXE".into()),
                macos: Some("com.microsoft.Word".into()),
                linux: None,
            },
        ]
    );
    // "page Review" on Writing's Main is Writing's Review, not a Default page.
    let main = c
        .layers
        .iter()
        .find(|l| l.id == "profile-writing-main")
        .unwrap();
    assert_eq!(
        main.bindings.get("Esc"),
        Some(&Action::LayerPush {
            target: "profile-writing-review".into()
        })
    );
    assert_eq!(c.profiles[1].apps[0].name, "mspaint.exe");
    let file = String::from_utf8(export(&c, &win())).unwrap();
    assert!(file.contains(r#""profiles": ["#), "{file}");
    assert!(file.contains("\"notepad.exe\",\n"), "{file}");
    let again = read_ok(&file).config;
    assert_eq!(again, c, "{file}");
    // Restored again here: the same ids.
    let same = import(&c, file.as_bytes(), true, &win()).unwrap().config;
    assert_eq!(same, c);
}

#[test]
fn a_1_0_0_file_reads_unchanged_and_writes_no_profiles() {
    let c = read_ok(SCHEMA_EXAMPLE).config;
    assert!(c.profiles.is_empty());
    let file = String::from_utf8(export(&c, &win())).unwrap();
    assert!(!file.contains("profiles"), "{file}");
    let value: serde_json::Value = serde_json::to_value(&c).unwrap();
    assert!(
        value.get("profiles").is_none(),
        "config.json stays as 1.0.0 wrote it"
    );
}

#[test]
fn profile_mistakes_say_what_to_fix() {
    let with = |profiles: &str| format!(r#"{{"pages": {{"Main": {{}}}}, "profiles": {profiles}}}"#);
    for (profiles, want) in [
        (r#"{}"#, "profiles must be a list"),
        (r#"[{"pages": {"A": {}}}]"#, "a profile needs a \"name\""),
        (r#"[{"name": "W"}]"#, "a profile needs \"pages\""),
        (r#"[{"name": "default", "pages": {"A": {}}}]"#, "is taken"),
        (
            r#"[{"name": "W", "pages": {"A": {}}}, {"name": "w", "pages": {"B": {}}}]"#,
            "is taken",
        ),
        (r#"[{"name": "W", "pages": {}}]"#, "is empty"),
        (
            r#"[{"name": "W", "apps": [{"name": "x"}], "pages": {"A": {}}}]"#,
            "an app needs",
        ),
        (
            r#"[{"name": "W", "apps": "x", "pages": {"A": {}}}]"#,
            "apps must be a list",
        ),
        (
            r#"[{"name": "W", "colour": 1, "pages": {"A": {}}}]"#,
            "unknown field \"colour\"",
        ),
        // A page key goes to a page of its own profile only.
        (
            r#"[{"name": "W", "pages": {"A": {"CUT": "page Main"}}}]"#,
            "there is no page \"Main\"",
        ),
    ] {
        let e = err(&with(profiles));
        assert!(e.contains(want), "{profiles}: {e}");
    }
}
