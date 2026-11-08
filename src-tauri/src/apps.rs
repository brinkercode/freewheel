//! The key panel's app picker: the installed apps (`list_apps`: the Start
//! Menu shortcuts on Windows, the .app bundles on macOS, the .desktop
//! entries on Linux) and 'Choose file…' (`choose_app`, the OS file dialog
//! filtered to programs). Each answer is the LaunchApp the key saves: what
//! to run and its arguments.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One app the key panel can pick: its name and the LaunchApp to save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppChoice {
    pub name: String,
    pub path: String,
    pub args: Vec<String>,
}

/// How deep the Start Menu folders are searched.
const MAX_DEPTH: usize = 4;

/// The Start Menu folders on Windows: everyone's and this user's.
pub fn start_menu_dirs(program_data: Option<&str>, app_data: Option<&str>) -> Vec<PathBuf> {
    let programs = Path::new("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs");
    [program_data, app_data]
        .into_iter()
        .flatten()
        .map(|root| Path::new(root).join(&programs))
        .collect()
}

/// The .app folders on macOS: /Applications (and its Utilities), the
/// system apps and the user's own.
pub fn mac_app_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/Applications/Utilities"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
    ];
    dirs.extend(home.map(|h| h.join("Applications")));
    dirs
}

/// Not apps: uninstallers, help files and readmes the Start Menu also lists.
fn is_clutter(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        "uninstall",
        "readme",
        "read me",
        "help",
        "documentation",
        "release notes",
        "website",
    ]
    .iter()
    .any(|w| n.contains(w))
}

/// Every file under `dir` (at most `depth` folders down) whose extension
/// is `ext`; a folder with that extension (a macOS .app) counts as one.
#[cfg(all(windows, test))]
pub fn find_files(dir: &Path, ext: &str, depth: usize, out: &mut Vec<PathBuf>) {
    find(dir, ext, depth, out);
}

fn find(dir: &Path, ext: &str, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let matches = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext));
        if matches {
            out.push(path);
        } else if depth > 0 && path.is_dir() {
            find(&path, ext, depth - 1, out);
        }
    }
}

/// The apps in `dirs` with extension `ext`, by name (first one wins),
/// sorted case-insensitively. `launch` makes each one's LaunchApp.
pub fn scan(
    dirs: &[PathBuf],
    ext: &str,
    depth: usize,
    launch: impl Fn(&Path) -> (String, Vec<String>),
) -> Vec<AppChoice> {
    let mut found = Vec::new();
    for dir in dirs {
        find(dir, ext, depth, &mut found);
    }
    let mut apps: Vec<AppChoice> = Vec::new();
    for path in found {
        let Some(name) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        if is_clutter(&name) || apps.iter().any(|a| a.name.eq_ignore_ascii_case(&name)) {
            continue;
        }
        let (program, args) = launch(&path);
        apps.push(AppChoice {
            name,
            path: program,
            args,
        });
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// A Start Menu shortcut opens through Explorer, as a double-click does
/// (a .lnk is not a program CreateProcess can run).
pub fn windows_launch(path: &Path) -> (String, Vec<String>) {
    (
        "explorer.exe".into(),
        vec![path.to_string_lossy().into_owned()],
    )
}

/// A .app bundle opens through `open`, as a double-click does.
pub fn mac_launch(path: &Path) -> (String, Vec<String>) {
    (
        "/usr/bin/open".into(),
        vec![path.to_string_lossy().into_owned()],
    )
}

/// Linux's opener: `xdg-open` opens a file or a website in the user's
/// default app.
pub const LINUX_OPENER: &str = "xdg-open";

/// A file opens through `xdg-open`, as a double-click does.
pub fn linux_open(path: &Path) -> (String, Vec<String>) {
    (
        LINUX_OPENER.into(),
        vec![path.to_string_lossy().into_owned()],
    )
}

/// True for a file this user may run (any execute bit; Unix only).
fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// A file picked with 'Choose file…': an .exe runs as it is; a macOS .app
/// (or any other file) opens through the OS opener; on Linux a .desktop
/// entry is kept (freewheel runs its Exec line), a program or an AppImage
/// runs as it is and anything else opens with `xdg-open`.
pub fn chosen(path: &Path) -> AppChoice {
    chosen_on(path, std::env::consts::OS)
}

/// [`chosen`] as `os` would answer.
pub fn chosen_on(path: &Path, os: &str) -> AppChoice {
    let name = path.file_stem().map_or_else(
        || path.to_string_lossy().into_owned(),
        |s| s.to_string_lossy().into_owned(),
    );
    let has_ext = |x: &str| path.extension().is_some_and(|e| e.eq_ignore_ascii_case(x));
    let as_is = || (path.to_string_lossy().into_owned(), Vec::new());
    let (program, args) = if os == "linux" {
        if has_ext("desktop") || has_ext("appimage") || executable(path) {
            as_is()
        } else {
            linux_open(path)
        }
    } else if has_ext("exe") {
        as_is()
    } else if os == "windows" {
        windows_launch(path)
    } else {
        mac_launch(path)
    };
    AppChoice {
        name,
        path: program,
        args,
    }
}

/// The installed apps on this computer; empty where freewheel does not
/// know where they are.
pub fn installed() -> Vec<AppChoice> {
    if cfg!(windows) {
        let program_data = std::env::var("ProgramData").ok();
        let app_data = std::env::var("APPDATA").ok();
        let dirs = start_menu_dirs(program_data.as_deref(), app_data.as_deref());
        scan(&dirs, "lnk", MAX_DEPTH, windows_launch)
    } else if cfg!(target_os = "macos") {
        let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
        scan(&mac_app_dirs(home.as_deref()), "app", 0, mac_launch)
    } else {
        linux_installed()
    }
}

/// The Linux menus' apps: each .desktop entry, saved as its path (the
/// launcher runs its Exec line; the picker shows its Icon).
#[cfg(target_os = "linux")]
fn linux_installed() -> Vec<AppChoice> {
    let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
    let dirs = crate::linux_desktop::application_dirs(|n| std::env::var(n).ok(), home.as_deref());
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut apps: Vec<AppChoice> = Vec::new();
    for app in crate::linux_desktop::installed_apps(&dirs, &path) {
        if is_clutter(&app.name) || apps.iter().any(|a| a.name.eq_ignore_ascii_case(&app.name)) {
            continue;
        }
        apps.push(AppChoice {
            name: app.name,
            path: app.desktop.to_string_lossy().into_owned(),
            args: Vec::new(),
        });
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

#[cfg(not(target_os = "linux"))]
fn linux_installed() -> Vec<AppChoice> {
    Vec::new()
}

/// The installed apps for the key panel's list. Async: the folders are read
/// off the main thread.
#[tauri::command]
pub async fn list_apps() -> Vec<AppChoice> {
    let apps = tauri::async_runtime::spawn_blocking(installed)
        .await
        .unwrap_or_default();
    tracing::info!(count = apps.len(), "installed apps listed");
    apps
}

/// The file dialog's filter for programs on this OS (none on Linux, where
/// programs have no extension: any file, a .desktop, an AppImage).
pub fn program_filter() -> Option<(&'static str, &'static [&'static str])> {
    if cfg!(target_os = "linux") {
        None
    } else if cfg!(target_os = "macos") {
        Some(("Apps", &["app"]))
    } else {
        Some(("Programs", &["exe"]))
    }
}

/// 'Choose file…': the OS file dialog filtered to programs; `None` when
/// cancelled.
#[tauri::command]
pub async fn choose_app(app: tauri::AppHandle) -> Result<Option<AppChoice>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = match crate::e2e::dialog(&app, crate::e2e::Dialog::ChooseApp) {
        Some(stub) => stub,
        None => {
            let mut dialog = app.dialog().file();
            if let Some((name, ext)) = program_filter() {
                dialog = dialog.add_filter(name, ext);
            }
            dialog
                .blocking_pick_file()
                .map(|p| p.into_path().map_err(|e| e.to_string()))
                .transpose()?
        }
    };
    let choice = picked.as_deref().map(chosen);
    if let Some(c) = &choice {
        tracing::info!("app chosen from a file");
        crate::simulate::note(&app, format!("Chose the app {}", c.name));
    }
    Ok(choice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_start_menu_shortcuts_are_the_apps_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let programs = dir.path().join("Programs");
        std::fs::create_dir_all(programs.join("Spotify Folder")).unwrap();
        for f in [
            "Notepad.lnk",
            "Spotify Folder/Spotify.lnk",
            "Spotify Folder/Uninstall Spotify.lnk",
            "Readme.txt",
            "notepad.lnk",
        ] {
            std::fs::write(programs.join(f), b"").unwrap();
        }
        let apps = scan(
            &[programs.clone(), dir.path().join("missing")],
            "lnk",
            4,
            windows_launch,
        );
        let names: Vec<_> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names.contains(&"Spotify"));
        assert_eq!(apps[1].name, "Spotify");
        assert_eq!(apps[1].path, "explorer.exe");
        assert!(apps[1].args[0].ends_with("Spotify.lnk"));
    }

    #[test]
    fn mac_apps_are_bundles_opened_with_open() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Notes.app/Contents")).unwrap();
        let apps = scan(&[dir.path().to_path_buf()], "app", 0, mac_launch);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Notes");
        assert_eq!(apps[0].path, "/usr/bin/open");
    }

    #[test]
    fn the_start_menu_folders_are_everyones_and_the_users() {
        let dirs = start_menu_dirs(Some("C:/ProgramData"), Some("C:/Users/a/AppData/Roaming"));
        assert_eq!(dirs.len(), 2);
        assert!(dirs[0].ends_with("Microsoft/Windows/Start Menu/Programs"));
        assert!(dirs[1].starts_with("C:/Users/a/AppData/Roaming"));
        assert_eq!(start_menu_dirs(None, None), Vec::<PathBuf>::new());
        assert!(
            mac_app_dirs(Some(Path::new("/Users/a")))
                .contains(&PathBuf::from("/Users/a/Applications"))
        );
    }

    #[test]
    fn a_chosen_exe_runs_as_it_is() {
        let c = chosen_on(Path::new("C:/Tools/My Editor.exe"), "windows");
        assert_eq!(c.name, "My Editor");
        assert_eq!(c.path, "C:/Tools/My Editor.exe");
        assert!(c.args.is_empty());
        let app = chosen_on(Path::new("/Applications/Notes.app"), "macos");
        assert_eq!(app.name, "Notes");
        assert_eq!(app.args, ["/Applications/Notes.app"]);
        let lnk = chosen_on(Path::new("C:/x/Spotify.lnk"), "windows");
        assert_eq!((lnk.path.as_str(), lnk.args.len()), ("explorer.exe", 1));
    }

    #[test]
    fn on_linux_a_chosen_entry_or_program_runs_and_a_file_opens() {
        let dir = tempfile::tempdir().unwrap();
        let entry = chosen_on(
            Path::new("/usr/share/applications/org.gnome.Calculator.desktop"),
            "linux",
        );
        assert_eq!(
            entry.path,
            "/usr/share/applications/org.gnome.Calculator.desktop"
        );
        assert!(entry.args.is_empty());
        let img = chosen_on(Path::new("/home/a/Apps/Krita-5.2.AppImage"), "linux");
        assert_eq!((img.name.as_str(), img.args.len()), ("Krita-5.2", 0));
        let doc = dir.path().join("notes.txt");
        std::fs::write(&doc, "x").unwrap();
        let opened = chosen_on(&doc, "linux");
        assert_eq!(opened.path, LINUX_OPENER);
        assert_eq!(opened.args, [doc.to_string_lossy()]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let tool = dir.path().join("tool");
            std::fs::write(&tool, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(chosen_on(&tool, "linux").path, tool.to_string_lossy());
        }
    }
}
