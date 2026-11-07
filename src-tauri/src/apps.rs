//! The key panel's app picker: the installed apps (`list_apps`: the Start
//! Menu shortcuts on Windows, the .app bundles on macOS) and 'Choose
//! file…' (`choose_app`, the OS file dialog filtered to programs). Each
//! answer is the LaunchApp the key saves: what to run and its arguments.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// One app the key panel can pick: its name and the LaunchApp to save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

/// A file picked with 'Choose file…': an .exe runs as it is; a macOS .app
/// (or any other file) opens through the OS opener.
pub fn chosen(path: &Path) -> AppChoice {
    let name = path.file_stem().map_or_else(
        || path.to_string_lossy().into_owned(),
        |s| s.to_string_lossy().into_owned(),
    );
    let is_exe = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    let (program, args) = if is_exe {
        (path.to_string_lossy().into_owned(), Vec::new())
    } else if cfg!(windows) {
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
        Vec::new()
    }
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

/// The file dialog's filter for programs on this OS.
pub fn program_filter() -> (&'static str, &'static [&'static str]) {
    if cfg!(target_os = "macos") {
        ("Apps", &["app"])
    } else {
        ("Programs", &["exe"])
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
            let (name, ext) = program_filter();
            app.dialog()
                .file()
                .add_filter(name, ext)
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
        let c = chosen(Path::new("C:/Tools/My Editor.exe"));
        assert_eq!(c.name, "My Editor");
        assert_eq!(c.path, "C:/Tools/My Editor.exe");
        assert!(c.args.is_empty());
        let app = chosen(Path::new("/Applications/Notes.app"));
        assert_eq!(app.name, "Notes");
        assert_eq!(app.args, ["/Applications/Notes.app"]);
    }
}
