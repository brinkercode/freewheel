//! App profiles: which profile the app in front asks for.
//!
//! A profile ([`Profile`]) has its own pages and lists the apps it is for
//! ([`ProfileApp`]). The rule, as the Keys tab states it:
//!
//! - an app a profile lists comes to the front: that profile is active, on
//!   its first page;
//! - any other app comes to the front: the Default profile, on the page it
//!   was on before;
//! - freewheel's own window coming to the front changes nothing;
//! - page keys switch pages within the active profile.
//!
//! The OS watchers (src-tauri front_app.rs) say which app is in front as a
//! [`FrontApp`]; the engine applies it ([`crate::engine::Engine::front_app`]).

use crate::config::{Profile, ProfileApp};
use crate::mappings_file::Os;

/// The app that came to the front, as the OS names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontApp {
    /// What logs and the settings window call it (`Notepad`, `notepad.exe`).
    pub name: String,
    /// Every name a profile's app may match: the program file on Windows,
    /// the bundle id and program on macOS, the program and the window class
    /// (instance and class) on Linux.
    pub ids: Vec<String>,
    /// freewheel's own window: never switches.
    pub own: bool,
}

impl FrontApp {
    /// An app known by these names (the first is its display name).
    pub fn new(ids: &[&str]) -> Self {
        FrontApp {
            name: ids.first().copied().unwrap_or_default().to_string(),
            ids: ids.iter().map(|s| s.to_string()).collect(),
            own: false,
        }
    }

    /// freewheel's own window.
    pub fn own() -> Self {
        FrontApp {
            name: crate::APP_NAME.into(),
            ids: Vec::new(),
            own: true,
        }
    }

    /// Whether `other` is the same app (another window of it counts as the same).
    pub fn same_app(&self, other: &FrontApp) -> bool {
        self.own == other.own && self.ids == other.ids
    }
}

/// What `app` says for `os`.
pub fn entry_for(app: &ProfileApp, os: Os) -> Option<&str> {
    match os {
        Os::Windows => app.windows.as_deref(),
        Os::MacOs => app.macos.as_deref(),
        Os::Linux => app.linux.as_deref(),
    }
    .map(str::trim)
    .filter(|s| !s.is_empty())
}

/// A name as matching compares it: lower case, without its folder and, on
/// Windows, without `.exe`; on Linux without `.desktop`.
pub fn normalize(name: &str, os: Os) -> String {
    let name = name.trim();
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let lower = file.to_lowercase();
    let cut = match os {
        Os::Windows => lower.strip_suffix(".exe"),
        Os::Linux => lower.strip_suffix(".desktop"),
        Os::MacOs => lower.strip_suffix(".app"),
    };
    cut.map(str::to_string).unwrap_or(lower)
}

/// The program inside an AppImage, from its file name as AppImages are
/// named (`<name>-<version>-<arch>.AppImage`): `krita-5.2.6-x86_64.AppImage`
/// is `krita`, `GIMP_AppImage-git-2.10-x86_64.AppImage` is `gimp`. Lower
/// case; `None` for a name that is not an AppImage's.
///
/// The window in front belongs to that program (its process, its window
/// class), never to a process named after the file (#200).
pub fn appimage_program(name: &str) -> Option<String> {
    let file = name.trim().rsplit(['/', '\\']).next().unwrap_or(name);
    let lower = file.to_lowercase();
    let stem = lower.strip_suffix(".appimage")?;
    const ARCHES: [&str; 9] = [
        "x64", "amd64", "aarch64", "arm64", "armhf", "i386", "i686", "linux", "appimage",
    ];
    let mut words = Vec::new();
    // `x86_64` holds a separator: make it one word first.
    let stem = stem.replace("x86_64", "x64").replace("x86-64", "x64");
    for word in stem.split(['-', '_', ' ']) {
        let version = word.starts_with(|c: char| c.is_ascii_digit())
            || (word.starts_with('v') && word[1..].starts_with(|c: char| c.is_ascii_digit()));
        if word.is_empty() || version || ARCHES.contains(&word) || word == "git" {
            break;
        }
        words.push(word);
    }
    let program = words.join("-");
    (!program.is_empty()).then_some(program)
}

/// The names a profile's entry matches: the entry itself and, on Linux for
/// an AppImage, the program inside it ([`appimage_program`]).
fn entry_names(entry: &str, os: Os) -> Vec<String> {
    let mut names = vec![normalize(entry, os)];
    if os == Os::Linux
        && let Some(program) = appimage_program(entry)
    {
        names.push(program);
    }
    names.retain(|n| !n.is_empty());
    names
}

/// Whether `app` (a profile's) is the app `front`, on `os`.
pub fn app_matches(app: &ProfileApp, os: Os, front: &FrontApp) -> bool {
    if front.own {
        return false;
    }
    let Some(entry) = entry_for(app, os) else {
        return false;
    };
    let names = entry_names(entry, os);
    front
        .ids
        .iter()
        .any(|id| names.contains(&normalize(id, os)))
}

/// The first profile listing `front`, in the profiles' order.
pub fn profile_for<'a>(profiles: &'a [Profile], os: Os, front: &FrontApp) -> Option<&'a Profile> {
    profiles
        .iter()
        .find(|p| p.apps.iter().any(|a| app_matches(a, os, front)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(windows: &str, macos: &str, linux: &str) -> ProfileApp {
        let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
        ProfileApp {
            name: "App".into(),
            windows: some(windows),
            macos: some(macos),
            linux: some(linux),
        }
    }

    fn profile(id: &str, apps: Vec<ProfileApp>) -> Profile {
        Profile {
            id: id.into(),
            name: id.into(),
            apps,
            pages: vec![format!("{id}-main")],
            color: None,
            icon: None,
        }
    }

    #[test]
    fn each_os_matches_its_own_name_for_the_app() {
        let notepad = app("notepad.exe", "com.apple.TextEdit", "gedit");
        let win = FrontApp::new(&["Notepad.EXE"]);
        assert!(app_matches(&notepad, Os::Windows, &win));
        assert!(!app_matches(&notepad, Os::Linux, &win));
        let full_path = FrontApp::new(&[r"C:\Windows\System32\notepad.exe"]);
        assert!(app_matches(&notepad, Os::Windows, &full_path));
        // Written without .exe still matches.
        assert!(app_matches(
            &app("notepad", "", ""),
            Os::Windows,
            &FrontApp::new(&["notepad.exe"])
        ));
        let mac = FrontApp::new(&["com.apple.textedit", "TextEdit"]);
        assert!(app_matches(&notepad, Os::MacOs, &mac));
        assert!(!app_matches(&notepad, Os::Windows, &mac));
        // Linux: the program or either part of the window class.
        let linux = FrontApp::new(&["gedit", "org.gnome.gedit", "Gedit"]);
        assert!(app_matches(&notepad, Os::Linux, &linux));
        let by_class = app("", "", "org.gnome.gedit.desktop");
        assert!(app_matches(&by_class, Os::Linux, &linux));
        assert!(!app_matches(
            &notepad,
            Os::Linux,
            &FrontApp::new(&["gedit-helper"])
        ));
    }

    #[test]
    fn an_appimage_is_known_by_the_program_inside_it() {
        for (file, program) in [
            ("krita-5.2.6-x86_64.AppImage", Some("krita")),
            (
                "/home/me/My Apps/freewheel-1.1.0-linux-x86_64.AppImage",
                Some("freewheel"),
            ),
            ("Obsidian-1.5.3.AppImage", Some("obsidian")),
            ("balenaEtcher-1.18.11-x64.AppImage", Some("balenaetcher")),
            (
                "GIMP_AppImage-git-2.10.25-withPython2-x86_64.AppImage",
                Some("gimp"),
            ),
            ("kdenlive-v24.02-x86_64.appimage", Some("kdenlive")),
            ("Some-Tool-aarch64.AppImage", Some("some-tool")),
            ("Tool.AppImage", Some("tool")),
            ("2048-x86_64.AppImage", None),
            ("krita", None),
            ("krita.desktop", None),
        ] {
            assert_eq!(appimage_program(file).as_deref(), program, "{file}");
        }
        // The profile keeps the file; the window in front is its program
        // and window class (#200).
        let krita = app("", "", "krita-5.2.6-x86_64.AppImage");
        assert!(app_matches(
            &krita,
            Os::Linux,
            &FrontApp::new(&["krita", "Krita"])
        ));
        assert!(app_matches(
            &krita,
            Os::Linux,
            &FrontApp::new(&["AppRun", "krita-5.2.6-x86_64.AppImage"])
        ));
        assert!(!app_matches(
            &krita,
            Os::Linux,
            &FrontApp::new(&["gimp", "Gimp"])
        ));
        // Only on Linux: an AppImage is no Windows or macOS app.
        let both = app("krita-5.2.6-x86_64.AppImage", "", "");
        assert!(!app_matches(
            &both,
            Os::Windows,
            &FrontApp::new(&["krita.exe"])
        ));
    }

    #[test]
    fn freewheel_itself_and_an_app_with_no_entry_for_this_os_never_match() {
        let any = app("freewheel.exe", "freewheel", "freewheel");
        assert!(!app_matches(&any, Os::Windows, &FrontApp::own()));
        assert!(!app_matches(
            &app("", "x", ""),
            Os::Windows,
            &FrontApp::new(&[""])
        ));
    }

    #[test]
    fn the_first_profile_listing_the_app_wins() {
        let profiles = vec![
            profile("none", vec![]),
            profile("a", vec![app("code.exe", "", "")]),
            profile("b", vec![app("Code.exe", "", "")]),
        ];
        let front = FrontApp::new(&["code.exe"]);
        assert_eq!(
            profile_for(&profiles, Os::Windows, &front).map(|p| p.id.as_str()),
            Some("a")
        );
        assert!(profile_for(&profiles, Os::Windows, &FrontApp::new(&["x.exe"])).is_none());
        assert!(profile_for(&profiles, Os::Linux, &front).is_none());
    }

    #[test]
    fn another_window_of_the_same_app_is_the_same_app() {
        let a = FrontApp::new(&["notepad.exe"]);
        assert!(a.same_app(&FrontApp::new(&["notepad.exe"])));
        assert!(!a.same_app(&FrontApp::new(&["code.exe"])));
        assert!(!a.same_app(&FrontApp::own()));
    }
}
