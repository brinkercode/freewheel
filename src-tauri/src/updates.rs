//! Updates outside the settings window.
//!
//! `freewheel --update-check-and-install` is the update-e2e workflow's way to
//! run the real updater on an installed build: no window, tray or device. It
//! checks the configured endpoint, downloads the update, verifies it against
//! the embedded pubkey and installs it, then exits with [`Outcome::code`].
//! On Windows the plugin itself exits 0 once it has launched the NSIS
//! installer, which then replaces the installed app.
//!
//! `open_release_page` opens a version's GitHub release page (the What's new
//! notice's link) in the default browser.

use tauri_plugin_updater::UpdaterExt;

/// The command-line flag for a headless update run.
pub const FLAG: &str = "--update-check-and-install";

/// Where freewheel's releases live.
pub const RELEASES: &str = "https://github.com/brinkercode/freewheel/releases";

/// What a headless update run did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A newer version was downloaded, verified and installed.
    Installed,
    /// The endpoint has no newer version.
    UpToDate,
    /// This build has no updater pubkey (dev and CI builds).
    NotConfigured,
    /// The check, the download, the signature or the install failed.
    Failed,
}

impl Outcome {
    /// The process exit code: 0 installed, 2 up to date, 3 not configured, 1 failed.
    pub fn code(self) -> i32 {
        match self {
            Outcome::Installed => 0,
            Outcome::Failed => 1,
            Outcome::UpToDate => 2,
            Outcome::NotConfigured => 3,
        }
    }
}

/// True when the command line asks for a headless update run.
pub fn requested(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|a| a == FLAG)
}

/// Runs the headless update and returns the exit code. `configured` is
/// whether this build has an updater pubkey.
pub fn run(mut context: tauri::Context<tauri::Wry>, configured: bool) -> i32 {
    if let Some(outcome) = precheck(configured) {
        return outcome.code();
    }
    // No settings window: the run must not start the UI's own update check.
    context.config_mut().app.windows.clear();
    let app = match tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
    {
        Ok(app) => app,
        Err(e) => {
            tracing::error!(error = %e, "update run: the app did not build");
            return Outcome::Failed.code();
        }
    };
    let outcome = tauri::async_runtime::block_on(check_and_install(app.handle()));
    tracing::info!(?outcome, "update run finished");
    outcome.code()
}

/// The outcome decided before any network call: a build without an updater
/// pubkey never updates.
fn precheck(configured: bool) -> Option<Outcome> {
    if configured {
        return None;
    }
    tracing::error!("update run: this build has no updater pubkey");
    Some(Outcome::NotConfigured)
}

async fn check_and_install(app: &tauri::AppHandle<tauri::Wry>) -> Outcome {
    let updater = match app.updater_builder().restart_after_install(false).build() {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(error = %e, "update run: no updater");
            return Outcome::Failed;
        }
    };
    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => {
            tracing::info!("update run: up to date");
            return Outcome::UpToDate;
        }
        Err(e) => {
            tracing::error!(error = %e, "update run: check failed");
            return Outcome::Failed;
        }
    };
    tracing::info!(from = %update.current_version, to = %update.version, "update run: installing");
    match update.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => Outcome::Installed,
        Err(e) => {
            tracing::error!(error = %e, "update run: download or install failed");
            Outcome::Failed
        }
    }
}

/// The GitHub release page of `version` ("0.1.1" or "v0.1.1"), or None when
/// it is not a plain semver version (so no other URL can be opened).
pub fn release_page(version: &str) -> Option<String> {
    let v = version.strip_prefix('v').unwrap_or(version);
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (v, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    let numeric = parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    let pre_ok = pre.is_none_or(|p| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    });
    (numeric && pre_ok).then(|| format!("{RELEASES}/tag/v{v}"))
}

/// Opens `version`'s GitHub release page in the default browser (no shell).
#[tauri::command]
pub fn open_release_page(version: String) -> Result<(), String> {
    let url = release_page(&version).ok_or_else(|| format!("not a version: {version}"))?;
    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("/usr/bin/open", vec![&url])
    } else if cfg!(windows) {
        ("rundll32.exe", vec!["url.dll,FileProtocolHandler", &url])
    } else {
        ("xdg-open", vec![&url])
    };
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(drop)
        .map_err(|e| format!("{program} {url}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_update_run_needs_its_flag() {
        assert!(requested(args(&["freewheel.exe", FLAG])));
        assert!(!requested(args(&["freewheel.exe"])));
        assert!(!requested(args(&[
            "freewheel.exe",
            "--simulate",
            "--smoke"
        ])));
    }

    #[test]
    fn exit_codes_tell_the_outcomes_apart() {
        let codes = [
            Outcome::Installed,
            Outcome::Failed,
            Outcome::UpToDate,
            Outcome::NotConfigured,
        ]
        .map(Outcome::code);
        assert_eq!(codes, [0, 1, 2, 3]);
    }

    #[test]
    fn a_build_without_a_pubkey_does_not_update() {
        // (tauri::generate_context! may be expanded once per binary, so the
        // check before the app is built is tested on its own.)
        assert_eq!(precheck(false), Some(Outcome::NotConfigured));
        assert_eq!(precheck(true), None);
    }

    #[test]
    fn the_release_page_is_the_tag_page() {
        let page = "https://github.com/brinkercode/freewheel/releases/tag/v0.1.1";
        assert_eq!(release_page("0.1.1").as_deref(), Some(page));
        assert_eq!(release_page("v0.1.1").as_deref(), Some(page));
        assert_eq!(
            release_page("1.2.3-beta.1").as_deref(),
            Some("https://github.com/brinkercode/freewheel/releases/tag/v1.2.3-beta.1")
        );
    }

    #[test]
    fn only_versions_make_a_release_page() {
        for bad in [
            "",
            "1.2",
            "1.2.3.4",
            "1.x.3",
            "1.2.3-",
            "1.2.3/../../evil",
            "1.2.3-a b",
            "https://evil.example",
            "1.2.3?x=1",
        ] {
            assert_eq!(release_page(bad), None, "{bad}");
        }
    }
}
