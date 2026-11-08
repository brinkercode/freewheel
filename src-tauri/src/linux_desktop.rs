//! Linux desktop entries (freedesktop.org Desktop Entry spec): the key
//! panel's installed apps are the `.desktop` files in the XDG data dirs,
//! a key saves the entry's path and freewheel runs its `Exec` line itself
//! (split into argv as the spec says, field codes dropped, never through a
//! shell), and the picker shows each entry's `Icon` from the icon theme.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The parts of a `[Desktop Entry]` freewheel uses.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub kind: String,
    pub no_display: bool,
    pub hidden: bool,
    pub try_exec: Option<String>,
}

/// A value's escapes (`\s \n \t \r \\`).
fn unescape_value(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Parses the `[Desktop Entry]` group of a .desktop file (unlocalised keys).
pub fn parse_entry(text: &str) -> Option<DesktopEntry> {
    let mut entry = DesktopEntry::default();
    let mut in_entry = false;
    let mut seen = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            seen |= in_entry;
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = unescape_value(value.trim());
        match key.trim() {
            "Name" => entry.name = value,
            "Exec" => entry.exec = value,
            "Icon" => entry.icon = Some(value).filter(|v| !v.is_empty()),
            "Type" => entry.kind = value,
            "NoDisplay" => entry.no_display = value == "true",
            "Hidden" => entry.hidden = value == "true",
            "TryExec" => entry.try_exec = Some(value).filter(|v| !v.is_empty()),
            _ => {}
        }
    }
    seen.then_some(entry)
}

/// The argv of an `Exec` value: quoted arguments unquoted (`\"`, `` \` ``,
/// `\$`, `\\` inside quotes), field codes (`%f %U %i %c %k`...) dropped,
/// `%%` kept as `%`. `None` when the quoting is broken or nothing is left.
pub fn exec_argv(exec: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut has = false;
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                has = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => match chars.next()? {
                            e @ ('"' | '`' | '$' | '\\') => cur.push(e),
                            e => {
                                cur.push('\\');
                                cur.push(e);
                            }
                        },
                        o => cur.push(o),
                    }
                }
            }
            ' ' | '\t' => {
                if has {
                    args.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            // `%%` is a `%`; any other field code (a file, URL, icon,
            // name...) passes nothing.
            '%' => {
                if chars.next() == Some('%') {
                    cur.push('%');
                    has = true;
                }
            }
            o => {
                cur.push(o);
                has = true;
            }
        }
    }
    if has {
        args.push(cur);
    }
    (!args.is_empty()).then_some(args)
}

/// The dirs .desktop files are read from, most important first:
/// `$XDG_DATA_HOME/applications` (default `~/.local/share`), then each of
/// `$XDG_DATA_DIRS` (default `/usr/local/share:/usr/share`), plus the
/// Flatpak and Snap exports when the data dirs leave them out.
pub fn application_dirs(var: impl Fn(&str) -> Option<String>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut data: Vec<PathBuf> = Vec::new();
    match var("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(d) => data.push(PathBuf::from(d)),
        None => data.extend(home.map(|h| h.join(".local/share"))),
    }
    let dirs = var("XDG_DATA_DIRS")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    data.extend(dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    data.extend(home.map(|h| h.join(".local/share/flatpak/exports/share")));
    data.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    data.push(PathBuf::from("/var/lib/snapd/desktop"));
    let mut out: Vec<PathBuf> = Vec::new();
    for d in data {
        let apps = d.join("applications");
        if !out.contains(&apps) {
            out.push(apps);
        }
    }
    out
}

/// One installed app: its name and its .desktop file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub name: String,
    pub desktop: PathBuf,
}

/// The desktop-file id of `path` under `dir` (`kde/x.desktop` -> `kde-x.desktop`).
fn desktop_id(dir: &Path, path: &Path) -> String {
    path.strip_prefix(dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('/', "-")
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "desktop") {
            out.push(p);
        } else if depth > 0 && p.is_dir() {
            walk(&p, depth - 1, out);
        }
    }
}

/// True when `program` (a path, or a name looked up in `path`) exists.
pub fn program_exists(program: &str, path: &std::ffi::OsStr) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    std::env::split_paths(path).any(|d| d.join(program).is_file())
}

/// The apps the menus show: `Type=Application` entries that are not
/// `NoDisplay`/`Hidden` and whose `TryExec` exists; the first file with a
/// desktop-file id wins (the user's own over the system's).
pub fn installed_apps(dirs: &[PathBuf], path: &std::ffi::OsStr) -> Vec<App> {
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut apps = Vec::new();
    for dir in dirs {
        let mut files = Vec::new();
        walk(dir, 2, &mut files);
        files.sort();
        for file in files {
            let id = desktop_id(dir, &file);
            if seen.insert(id, ()).is_some() {
                continue;
            }
            let Some(entry) = std::fs::read_to_string(&file)
                .ok()
                .and_then(|t| parse_entry(&t))
            else {
                continue;
            };
            if entry.kind != "Application"
                || entry.no_display
                || entry.hidden
                || entry.name.is_empty()
                || exec_argv(&entry.exec).is_none()
                || entry
                    .try_exec
                    .as_deref()
                    .is_some_and(|t| !program_exists(t, path))
            {
                continue;
            }
            apps.push(App {
                name: entry.name,
                desktop: file,
            });
        }
    }
    apps
}

/// The argv that launches the entry in `desktop`, plus `extra` arguments.
pub fn launch_argv(desktop: &Path, extra: &[String]) -> Result<Vec<String>, String> {
    let text =
        std::fs::read_to_string(desktop).map_err(|e| format!("{}: {e}", desktop.display()))?;
    let entry =
        parse_entry(&text).ok_or_else(|| format!("{}: not a desktop entry", desktop.display()))?;
    let mut argv = exec_argv(&entry.exec)
        .ok_or_else(|| format!("{}: no Exec line freewheel can run", desktop.display()))?;
    argv.extend(extra.iter().cloned());
    Ok(argv)
}

/// Where icon themes live: `~/.icons`, then `<data dir>/icons` for each
/// data dir, then `/usr/share/pixmaps` (searched last, unthemed).
pub fn icon_dirs(var: impl Fn(&str) -> Option<String>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = home.map(|h| h.join(".icons")).into_iter().collect();
    for apps in application_dirs(var, home) {
        if let Some(data) = apps.parent() {
            out.push(data.join("icons"));
        }
    }
    out
}

/// The icon theme the desktop uses, from GTK's settings.ini; `None` leaves
/// just hicolor (every app installs its icon there).
pub fn current_theme(home: Option<&Path>) -> Option<String> {
    let ini = std::fs::read_to_string(home?.join(".config/gtk-3.0/settings.ini")).ok()?;
    ini.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "gtk-icon-theme-name").then(|| v.trim().trim_matches('"').to_string())
    })
}

/// Sizes searched, best first for a 64 px icon.
const SIZES: [&str; 10] = [
    "scalable", "64x64", "128x128", "96x96", "256x256", "48x48", "512x512", "72x72", "32x32",
    "symbolic",
];

/// The file of icon `name`: an absolute path as is; else the first match in
/// `themes` (then hicolor) under `dirs`, by size, then `/usr/share/pixmaps`.
pub fn find_icon(name: &str, themes: &[String], dirs: &[PathBuf]) -> Option<PathBuf> {
    let p = Path::new(name);
    if p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }
    let mut order: Vec<&str> = themes.iter().map(String::as_str).collect();
    for fallback in ["hicolor", "Adwaita", "breeze"] {
        if !order.contains(&fallback) {
            order.push(fallback);
        }
    }
    for theme in order {
        for dir in dirs {
            let base = dir.join(theme);
            if !base.is_dir() {
                continue;
            }
            for size in SIZES {
                for ext in ["svg", "png"] {
                    let f = base.join(size).join("apps").join(format!("{name}.{ext}"));
                    if f.is_file() {
                        return Some(f);
                    }
                    // Some themes put the category first (apps/64).
                    let f = base.join("apps").join(size).join(format!("{name}.{ext}"));
                    if f.is_file() {
                        return Some(f);
                    }
                }
            }
        }
    }
    for dir in [Path::new("/usr/share/pixmaps")]
        .into_iter()
        .chain(dirs.iter().map(PathBuf::as_path))
    {
        for ext in ["svg", "png", "xpm"] {
            let f = dir.join(format!("{name}.{ext}"));
            if f.is_file() {
                return Some(f);
            }
        }
    }
    None
}

/// The icon file of the app `source` (a .desktop file) in this session.
pub fn icon_file(source: &Path) -> Option<PathBuf> {
    let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
    icon_file_in(source, |n: &str| std::env::var(n).ok(), home.as_deref())
}

/// [`icon_file`] with the session's env vars and home given (tests).
pub fn icon_file_in(
    source: &Path,
    var: impl Fn(&str) -> Option<String>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    let entry = parse_entry(&std::fs::read_to_string(source).ok()?)?;
    let themes: Vec<String> = current_theme(home).into_iter().collect();
    find_icon(&entry.icon?, &themes, &icon_dirs(var, home))
}

/// The icon of the app `source` as PNG bytes at `px` (gdk-pixbuf draws SVG
/// and XPM too), or the PNG or SVG file itself when gdk-pixbuf can't draw
/// it (no loader installed): the window scales those.
pub fn icon_bytes(source: &Path, px: u32) -> Option<Vec<u8>> {
    draw_icon(&icon_file(source)?, px)
}

/// Draws the icon `file` as PNG bytes at `px` (see [`icon_bytes`]).
pub fn draw_icon(file: &Path, px: u32) -> Option<Vec<u8>> {
    let size = i32::try_from(px).ok()?;
    match gdk_pixbuf::Pixbuf::from_file_at_scale(file, size, size, true) {
        Ok(pixbuf) => pixbuf.save_to_bufferv("png", &[]).ok(),
        Err(e) => {
            tracing::debug!(file = %file.display(), error = %e, "icon not drawn");
            let bytes = std::fs::read(file).ok()?;
            (crate::app_icons::is_svg(&bytes) || crate::app_icons::is_png(&bytes)).then_some(bytes)
        }
    }
}

/// The D-Bus name of the StatusNotifierItem host freewheel's tray icon
/// (libayatana-appindicator) shows in.
pub const TRAY_WATCHER: &str = "org.kde.StatusNotifierWatcher";

/// True when the session has a tray host (KDE, Cinnamon, XFCE..., or GNOME
/// with the AppIndicator extension); `None` when the session bus can't be asked.
pub fn tray_host() -> Option<bool> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let reply = conn
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "NameHasOwner",
            &(TRAY_WATCHER,),
        )
        .ok()?;
    reply.body().deserialize::<bool>().ok()
}

/// What the General tab says when there is no tray host.
pub const NO_TRAY: &str = "Your desktop shows no tray icons, so freewheel's tray menu is hidden. \
On GNOME, add the AppIndicator extension. Until then, open freewheel again to see these settings.";

/// The hint for a tray host answer (`None`: could not tell, say nothing).
pub fn tray_hint(host: Option<bool>) -> Option<&'static str> {
    (host == Some(false)).then_some(NO_TRAY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_lines_split_like_the_spec_says() {
        let a = |s: &str| exec_argv(s).map(|v| v.join("|"));
        assert_eq!(a("firefox %u").as_deref(), Some("firefox"));
        assert_eq!(
            a("/usr/bin/code --new-window %F").as_deref(),
            Some("/usr/bin/code|--new-window")
        );
        assert_eq!(
            a(r#""/opt/My App/run" --name "a \"b\" \$c" 100%%"#).as_deref(),
            Some(r#"/opt/My App/run|--name|a "b" $c|100%"#)
        );
        assert_eq!(
            a("flatpak run --branch=stable org.gimp.GIMP @@ %F @@").as_deref(),
            Some("flatpak|run|--branch=stable|org.gimp.GIMP|@@|@@")
        );
        assert_eq!(
            a(r#"sh -c "echo; rm -rf /""#).as_deref(),
            Some("sh|-c|echo; rm -rf /")
        );
        assert_eq!(exec_argv("%U"), None);
        assert_eq!(exec_argv(r#""unclosed"#), None);
        assert_eq!(exec_argv(""), None);
    }

    #[test]
    fn only_the_desktop_entry_group_counts() {
        let e = parse_entry(
            "# c\n[Desktop Entry]\nName=Text\\sEditor\nName[de]=Texteditor\nExec=gedit %U\nIcon=org.gnome.gedit\nType=Application\nNoDisplay=false\n\
             [Desktop Action new]\nName=New\nExec=gedit --new\n",
        )
        .unwrap();
        assert_eq!(e.name, "Text Editor");
        assert_eq!(e.exec, "gedit %U");
        assert_eq!(e.icon.as_deref(), Some("org.gnome.gedit"));
        assert_eq!(e.kind, "Application");
        assert!(!e.no_display);
        assert_eq!(parse_entry("[Other]\nName=x\n"), None);
    }

    fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }

    #[test]
    fn the_menus_apps_are_listed_once_users_first() {
        let root = tempfile::tempdir().unwrap();
        let user = root.path().join("user/applications");
        let sys = root.path().join("sys/applications");
        let app = |name: &str, extra: &str| {
            format!(
                "[Desktop Entry]\nType=Application\nName={name}\nExec={}\n{extra}",
                name.to_lowercase()
            )
        };
        write(&sys, "gimp.desktop", &app("GIMP", "Icon=gimp"));
        write(&user, "gimp.desktop", &app("My GIMP", ""));
        write(&sys, "hidden.desktop", &app("Hidden", "NoDisplay=true"));
        write(
            &sys,
            "gone.desktop",
            &app("Gone", "TryExec=/nonexistent/gone"),
        );
        write(
            &sys,
            "link.desktop",
            "[Desktop Entry]\nType=Link\nName=Web\nURL=https://x\n",
        );
        write(&sys, "kde/dolphin.desktop", &app("Dolphin", ""));
        write(&sys, "notes.txt", "x");
        let apps = installed_apps(
            &[user.clone(), sys.clone(), root.path().join("none")],
            std::ffi::OsStr::new(""),
        );
        let names: Vec<&str> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names.contains(&"My GIMP") && names.contains(&"Dolphin"));
        let gimp = apps.iter().find(|a| a.name == "My GIMP").unwrap();
        assert_eq!(gimp.desktop, user.join("gimp.desktop"));
        assert_eq!(
            launch_argv(&gimp.desktop, &["--x".into()]).unwrap(),
            ["my", "gimp", "--x"]
        );
        assert!(launch_argv(&sys.join("link.desktop"), &[]).is_err());
    }

    #[test]
    fn the_xdg_data_dirs_are_searched_in_order() {
        let home = Path::new("/home/a");
        let env = |vars: &'static [(&'static str, &'static str)]| {
            move |n: &str| {
                vars.iter()
                    .find(|(k, _)| *k == n)
                    .map(|(_, v)| v.to_string())
            }
        };
        let dirs = application_dirs(env(&[]), Some(home));
        assert_eq!(dirs[0], PathBuf::from("/home/a/.local/share/applications"));
        assert_eq!(dirs[1], PathBuf::from("/usr/local/share/applications"));
        assert_eq!(dirs[2], PathBuf::from("/usr/share/applications"));
        assert!(dirs.contains(&PathBuf::from(
            "/var/lib/flatpak/exports/share/applications"
        )));
        let dirs = application_dirs(
            env(&[("XDG_DATA_HOME", "/d"), ("XDG_DATA_DIRS", "/x:/usr/share")]),
            Some(home),
        );
        assert_eq!(
            &dirs[..3],
            [
                PathBuf::from("/d/applications"),
                PathBuf::from("/x/applications"),
                PathBuf::from("/usr/share/applications")
            ]
        );
        assert_eq!(
            icon_dirs(env(&[("XDG_DATA_HOME", "/d")]), Some(home))[..2],
            [PathBuf::from("/home/a/.icons"), PathBuf::from("/d/icons")]
        );
    }

    #[test]
    fn icons_come_from_the_theme_then_hicolor_then_pixmaps() {
        let root = tempfile::tempdir().unwrap();
        let icons = root.path().join("icons");
        let hi = write(&icons, "hicolor/48x48/apps/gimp.png", "png");
        let svg = write(&icons, "hicolor/scalable/apps/gimp.svg", "<svg/>");
        assert_eq!(
            find_icon("gimp", &[], std::slice::from_ref(&icons)),
            Some(svg.clone())
        );
        let themed = write(&icons, "Papirus/64x64/apps/gimp.svg", "<svg/>");
        assert_eq!(
            find_icon("gimp", &["Papirus".into()], std::slice::from_ref(&icons)),
            Some(themed)
        );
        assert_eq!(find_icon(hi.to_str().unwrap(), &[], &[]), Some(hi));
        assert_eq!(find_icon("nope", &[], &[icons]), None);
        let home = root.path().join("home");
        write(
            &home,
            ".config/gtk-3.0/settings.ini",
            "[Settings]\ngtk-icon-theme-name=Yaru\n",
        );
        assert_eq!(current_theme(Some(&home)).as_deref(), Some("Yaru"));
    }

    #[test]
    fn no_tray_host_gets_the_gnome_hint() {
        assert_eq!(tray_hint(Some(false)), Some(NO_TRAY));
        assert!(NO_TRAY.contains("AppIndicator"));
        assert_eq!(tray_hint(Some(true)), None);
        assert_eq!(tray_hint(None), None);
    }

    /// A tiny icon theme shipped with the tests (tests/fixtures/icon-theme:
    /// hicolor with a PNG and an SVG app icon), found through XDG_DATA_DIRS
    /// given to the lookup, so this runs the same on any machine.
    #[test]
    fn a_desktop_entrys_icon_draws_as_a_png() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/icon-theme");
        let data_dirs = fixture.display().to_string();
        let var = |n: &str| match n {
            "XDG_DATA_DIRS" | "XDG_DATA_HOME" => Some(data_dirs.clone()),
            _ => None,
        };
        let root = tempfile::tempdir().unwrap();
        for (name, ext) in [
            ("freewheel-fixture-png", "png"),
            ("freewheel-fixture-svg", "svg"),
        ] {
            let desktop = write(
                root.path(),
                &format!("{name}.desktop"),
                &format!("[Desktop Entry]\nType=Application\nName=F\nExec=f\nIcon={name}\n"),
            );
            let file = icon_file_in(&desktop, var, None).expect("the fixture theme's icon");
            assert!(file.starts_with(&fixture), "{}", file.display());
            assert_eq!(file.extension().unwrap(), ext);
            let drawn = draw_icon(&file, 64).expect("an icon");
            if crate::app_icons::is_png(&drawn) {
                let decoded = png::Decoder::new(std::io::Cursor::new(&drawn))
                    .read_info()
                    .unwrap();
                let size = (decoded.info().width, decoded.info().height);
                // gdk-pixbuf drew it at 64 px, or (no loader) the PNG itself.
                assert!(
                    size == (64, 64) || drawn == std::fs::read(&file).unwrap(),
                    "{size:?}"
                );
            } else {
                assert_eq!(ext, "svg");
                assert!(crate::app_icons::is_svg(&drawn));
            }
        }
        // An absolute Icon= path needs no theme.
        let png = crate::app_icons::encode_png(2, 2, &[255, 0, 0, 255].repeat(4)).unwrap();
        let icon = root.path().join("red.png");
        std::fs::write(&icon, &png).unwrap();
        let desktop = write(
            root.path(),
            "red.desktop",
            &format!(
                "[Desktop Entry]\nType=Application\nName=Red\nExec=red\nIcon={}\n",
                icon.display()
            ),
        );
        assert_eq!(icon_file_in(&desktop, var, None), Some(icon.clone()));
        assert!(crate::app_icons::is_png(
            &draw_icon(&icon, 64).expect("an icon")
        ));
    }
}
