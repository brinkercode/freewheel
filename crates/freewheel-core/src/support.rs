//! The support bundle behind "Download logs for support": every log file,
//! oldest first, after a header and a redacted summary of the config.
//!
//! The summary follows the logging privacy rules: no shortcuts, launch
//! targets, arguments, layer names or process names, only counts, kinds and
//! the app's own switches. The log lines are already scrubbed when written.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::{Action, Config, Settings};
use crate::logging::{LOG_FILE, MAX_FILES, hashed};

/// The bundle's file name prefix: `freewheel-logs-<stamp>.txt`.
pub const BUNDLE_PREFIX: &str = "freewheel-logs-";

/// The log files in `dir`, oldest (`freewheel.log.4`) to newest
/// (`freewheel.log`); missing ones are skipped.
pub fn log_files(dir: &Path) -> Vec<PathBuf> {
    (0..MAX_FILES)
        .rev()
        .map(|n| match n {
            0 => dir.join(LOG_FILE),
            n => dir.join(format!("{LOG_FILE}.{n}")),
        })
        .filter(|p| p.is_file())
        .collect()
}

fn action_kind(action: &Action) -> &'static str {
    match action {
        Action::Hotkey { .. } => "Hotkey",
        Action::Held { .. } => "Held",
        Action::Latched { .. } => "Latched",
        Action::Scroll { .. } => "Scroll",
        Action::LaunchApp { enabled: true, .. } => "LaunchApp (on)",
        Action::LaunchApp { enabled: false, .. } => "LaunchApp (off)",
        Action::LayerPush { .. } => "LayerPush",
        Action::SetJogMode { .. } => "SetJogMode",
        Action::OverrideToggle { .. } => "OverrideToggle",
    }
}

fn kinds<'a>(actions: impl Iterator<Item = &'a Action>) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for action in actions {
        *counts.entry(action_kind(action)).or_default() += 1;
    }
    if counts.is_empty() {
        return "none".into();
    }
    counts
        .iter()
        .map(|(kind, n)| format!("{kind} {n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The config without anything private: switches, counts and kinds.
pub fn config_summary(config: &Config) -> String {
    let s = &config.settings;
    let process = if s.resolve_process_name == Settings::default().resolve_process_name {
        "default".to_string()
    } else {
        format!("custom ({})", hashed("process", &s.resolve_process_name))
    };
    let mut out = String::new();
    let _ = writeln!(out, "config version: {}", config.version);
    let _ = writeln!(out, "enabled: {}", s.enabled);
    let _ = writeln!(out, "start_at_login: {}", s.start_at_login);
    let _ = writeln!(out, "auto_update: {}", s.auto_update);
    let _ = writeln!(out, "appearance: {:?}", s.appearance);
    let _ = writeln!(out, "step_aside_for_resolve: {}", s.step_aside_for_resolve);
    let _ = writeln!(out, "resolve_process_name: {process}");
    let _ = writeln!(out, "detailed_logs: {}", s.detailed_logs);
    let _ = writeln!(out, "low_battery_threshold: {}%", s.low_battery_threshold);
    for (i, layer) in config.layers.iter().enumerate() {
        let base = if layer.id == config.base_layer {
            " (base)"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "page {}{base}: {} keys: {}",
            i + 1,
            layer.bindings.len(),
            kinds(layer.bindings.values())
        );
    }
    let _ = writeln!(out, "wheel default mode: {:?}", config.jog.default_mode);
    for (mode, binding) in &config.jog.per_mode {
        let _ = writeln!(
            out,
            "wheel {mode:?}: threshold {}, actions: {}",
            binding.threshold,
            kinds(binding.left.iter().chain(&binding.right))
        );
    }
    let o = &config.overrides;
    let _ = writeln!(
        out,
        "dial steps: system volume {}, app volume {}, brightness {}",
        o.system_volume_step, o.app_volume_step, o.brightness_step
    );
    out
}

/// The bundle's text: `about` (app version, OS), the config summary, then
/// every log file in `log_dir`, oldest first.
pub fn bundle(log_dir: &Path, config: &Config, about: &str) -> String {
    let mut out = String::from("freewheel support bundle\n");
    let _ = writeln!(out, "{about}\n\n== config (redacted) ==");
    out.push_str(&config_summary(config));
    let files = log_files(log_dir);
    if files.is_empty() {
        out.push_str("\n== no log files ==\n");
    }
    for file in files {
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        let _ = writeln!(out, "\n== {name} ==");
        match fs::read(&file) {
            Ok(bytes) => out.push_str(&String::from_utf8_lossy(&bytes)),
            Err(e) => {
                let _ = writeln!(out, "(cannot be read: {e})");
            }
        }
    }
    out
}

/// A free bundle path in `out_dir`: `freewheel-logs-<stamp>.txt`, or with
/// `-2`, `-3`... when that file exists.
pub fn bundle_path(out_dir: &Path, stamp: &str) -> PathBuf {
    let first = out_dir.join(format!("{BUNDLE_PREFIX}{stamp}.txt"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| out_dir.join(format!("{BUNDLE_PREFIX}{stamp}-{n}.txt")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Writes the bundle to a new file in `out_dir` and returns its path.
pub fn write_bundle(
    log_dir: &Path,
    config: &Config,
    about: &str,
    out_dir: &Path,
    stamp: &str,
) -> io::Result<PathBuf> {
    fs::create_dir_all(out_dir)?;
    let path = bundle_path(out_dir, stamp);
    fs::write(&path, bundle(log_dir, config, about))?;
    Ok(path)
}

/// `YYYYMMDD-HHMMSS` (UTC) for `secs` since the Unix epoch.
pub fn stamp(secs: u64) -> String {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Howard Hinnant's civil_from_days.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{JogMode, KeyCombo, Modifier};

    fn private_config() -> Config {
        let mut config = Config::default();
        config.settings.resolve_process_name = "SecretResolve".into();
        config.settings.detailed_logs = true;
        let bindings = &mut config.layers[0].bindings;
        bindings.insert(
            "Cut".into(),
            Action::Hotkey {
                combo: KeyCombo {
                    modifiers: vec![Modifier::Ctrl],
                    key: "F13".into(),
                },
            },
        );
        bindings.insert(
            "Snap".into(),
            Action::LaunchApp {
                path: "/Users/alice/SecretApp.app".into(),
                args: vec!["--diary".into()],
                enabled: true,
                cwd: None,
            },
        );
        config.jog.default_mode = JogMode::Volume;
        config
    }

    #[test]
    fn the_summary_keeps_switches_and_counts_but_nothing_private() {
        let summary = config_summary(&private_config());
        for private in ["SecretResolve", "F13", "SecretApp", "--diary", "alice"] {
            assert!(!summary.contains(private), "{private} leaked: {summary}");
        }
        assert!(summary.contains("detailed_logs: true"), "{summary}");
        assert!(
            summary.contains("step_aside_for_resolve: true"),
            "{summary}"
        );
        assert!(summary.contains("resolve_process_name: custom (process#"));
        assert!(
            summary.contains("page 1 (base): 2 keys: Hotkey 1, LaunchApp (on) 1"),
            "{summary}"
        );
        assert!(summary.contains("wheel default mode: Volume"), "{summary}");
        assert!(config_summary(&Config::default()).contains("resolve_process_name: default"));
    }

    #[test]
    fn the_bundle_holds_every_log_oldest_first_in_a_new_file() {
        let logs = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        fs::write(logs.path().join(LOG_FILE), "newest line\n").unwrap();
        fs::write(logs.path().join(format!("{LOG_FILE}.2")), "oldest line\n").unwrap();
        fs::write(logs.path().join(format!("{LOG_FILE}.1")), "middle line\n").unwrap();
        fs::write(logs.path().join("other.txt"), "not a log\n").unwrap();

        let about = "freewheel 0.1.0 on TestOS";
        let path = write_bundle(
            logs.path(),
            &private_config(),
            about,
            out.path(),
            "20261106-215800",
        )
        .unwrap();
        assert_eq!(path, out.path().join("freewheel-logs-20261106-215800.txt"));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("freewheel support bundle\nfreewheel 0.1.0 on TestOS\n"));
        let at = |s: &str| {
            text.find(s)
                .unwrap_or_else(|| panic!("{s} missing: {text}"))
        };
        assert!(at("== config (redacted) ==") < at("oldest line"));
        assert!(at("oldest line") < at("middle line"));
        assert!(at("middle line") < at("newest line"));
        assert!(!text.contains("not a log"));
        assert!(!text.contains("SecretApp"));

        // A second export the same second never overwrites the first.
        let again = write_bundle(
            logs.path(),
            &Config::default(),
            about,
            out.path(),
            "20261106-215800",
        )
        .unwrap();
        assert_eq!(
            again,
            out.path().join("freewheel-logs-20261106-215800-2.txt")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn a_bundle_without_logs_says_so() {
        let logs = tempfile::tempdir().unwrap();
        let text = bundle(&logs.path().join("missing"), &Config::default(), "about");
        assert!(text.contains("== no log files =="), "{text}");
    }

    #[test]
    fn stamps_are_utc_calendar_times() {
        assert_eq!(stamp(0), "19700101-000000");
        assert_eq!(stamp(951_782_400), "20000229-000000");
        assert_eq!(stamp(1_794_002_280), "20261106-215800");
    }
}
