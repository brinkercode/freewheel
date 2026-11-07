//! The Windows e2e's stand-ins for the OS dialogs (scripts/e2e-windows.mjs):
//! WebDriver cannot click a native file dialog, so in an e2e build
//! (`cargo build --features e2e`, only the e2e workflow does) running
//! `--simulate` with [`DIALOG_DIR_ENV`] set, each dialog answers with a
//! fixed file in that folder instead of opening. Shipped builds are built
//! without the feature: there [`dialog`] is always `None` and every dialog
//! opens as usual.

use std::path::PathBuf;

/// The folder the stubbed dialogs answer with files in.
#[cfg_attr(not(feature = "e2e"), allow(dead_code))]
pub const DIALOG_DIR_ENV: &str = "FREEWHEEL_E2E_DIALOG_DIR";

/// The dialogs the app opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialog {
    /// Back up: where to save freewheel-mappings.json.
    SaveMappings,
    /// Restore: the mappings file to read.
    OpenMappings,
    /// Restore's "Replace your keys?"; the stub says yes.
    ConfirmReplace,
    /// The key panel's 'Choose file…'.
    ChooseApp,
}

/// The program 'Choose file…' answers with in the e2e.
#[cfg_attr(not(feature = "e2e"), allow(dead_code))]
pub const CHOSEN_APP: &str = "E2E Example App.exe";

/// The stub's answer for `dialog` in `dir`: the file it picks (for the
/// confirmation, any `Some` is yes).
#[cfg_attr(not(feature = "e2e"), allow(dead_code))]
pub fn answer(dialog: Dialog, dir: &std::path::Path) -> PathBuf {
    match dialog {
        Dialog::SaveMappings | Dialog::OpenMappings | Dialog::ConfirmReplace => {
            dir.join(freewheel_core::config::MAPPINGS_FILE)
        }
        Dialog::ChooseApp => dir.join(CHOSEN_APP),
    }
}

/// The stubbed answer for `dialog`, when stubs are on: `Some(Some(path))`.
/// `None` (always, without the `e2e` feature): open the real dialog.
#[cfg(feature = "e2e")]
pub fn dialog<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    dialog: Dialog,
) -> Option<Option<PathBuf>> {
    if !crate::simulate::simulating(app) {
        return None;
    }
    let dir = std::env::var_os(DIALOG_DIR_ENV)?;
    let path = answer(dialog, std::path::Path::new(&dir));
    tracing::info!(?dialog, "e2e: dialog stubbed");
    crate::simulate::note(
        app,
        format!("e2e: the {dialog:?} dialog answered {}", path.display()),
    );
    Some(Some(path))
}

#[cfg(not(feature = "e2e"))]
pub fn dialog<R: tauri::Runtime>(
    _app: &tauri::AppHandle<R>,
    _dialog: Dialog,
) -> Option<Option<PathBuf>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_dialog_answers_with_a_fixed_file() {
        let dir = std::path::Path::new("/e2e");
        assert_eq!(
            answer(Dialog::SaveMappings, dir),
            dir.join("freewheel-mappings.json")
        );
        assert_eq!(
            answer(Dialog::OpenMappings, dir),
            answer(Dialog::SaveMappings, dir)
        );
        assert_eq!(answer(Dialog::ChooseApp, dir), dir.join(CHOSEN_APP));
    }

    /// Shipped builds never stub: the release workflow builds without the
    /// feature, and only the e2e workflow turns it on.
    #[test]
    fn only_the_e2e_workflow_builds_the_stubs() {
        let e2e = include_str!("../../.github/workflows/e2e-windows.yml");
        assert!(e2e.contains("--features tauri/custom-protocol,e2e"));
        assert!(include_str!("../../scripts/e2e-windows.mjs").contains(DIALOG_DIR_ENV));
        for dir in
            std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.github/workflows")).unwrap()
        {
            let path = dir.unwrap().path();
            if path.ends_with("e2e-windows.yml") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(
                !text.contains(",e2e") && !text.contains("features e2e"),
                "{path:?} builds the e2e stubs"
            );
        }
        let manifest = include_str!("../Cargo.toml");
        assert!(manifest.contains("e2e = []"));
        assert!(!manifest.contains("default = [\"e2e\""));
    }
}
