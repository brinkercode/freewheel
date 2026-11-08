//! `open ...`: what a key that opens an app or web page runs, resolved on
//! the computer that reads the file, and the reverse for export.
//!
//! Resolution order (SCHEMA.md "How open finds the app"): a web address;
//! a macOS bundle id (macOS); the exact path; the path with environment
//! variables and `~` expanded; a Linux .desktop id (Linux); an installed
//! app's name; a program on PATH. Anything else stays as a key that is off,
//! with a note saying why, never dropped.

use crate::config::{Action, PerOsTargets};

/// The OS a file is read or written on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    Linux,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        }
    }

    /// The field a per-OS `open` object uses for this OS.
    pub fn field(self) -> &'static str {
        match self {
            Os::Windows => "windows",
            Os::MacOs => "macos",
            Os::Linux => "linux",
        }
    }

    /// The program that opens a web page, file or folder as a double-click does.
    pub fn opener(self) -> &'static str {
        match self {
            Os::Windows => "explorer.exe",
            Os::MacOs => "/usr/bin/open",
            Os::Linux => "xdg-open",
        }
    }
}

/// One installed app the key panel lists (src-tauri apps.rs): its name and
/// what runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledApp {
    pub name: String,
    pub path: String,
    pub args: Vec<String>,
}

/// The computer a file is read on: its files, environment and apps.
pub trait Host {
    fn os(&self) -> Os;
    fn exists(&self, path: &str) -> bool;
    fn is_dir(&self, path: &str) -> bool;
    /// A file the OS runs as a program (Unix: an executable bit).
    fn is_executable(&self, path: &str) -> bool;
    fn var(&self, name: &str) -> Option<String>;
    /// The installed apps, by name (may scan folders: called only when a
    /// name needs it).
    fn installed(&self) -> Vec<InstalledApp>;
    /// The full path of a program found on PATH.
    fn which(&self, name: &str) -> Option<String>;
    /// Linux: the .desktop file for an id like `spotify.desktop`.
    fn desktop_entry(&self, id: &str) -> Option<String>;
}

/// The real computer; `installed` comes from the app crate (apps.rs).
pub struct RealHost<F: Fn() -> Vec<InstalledApp>> {
    installed: F,
    cache: std::sync::OnceLock<Vec<InstalledApp>>,
}

impl<F: Fn() -> Vec<InstalledApp>> RealHost<F> {
    pub fn new(installed: F) -> Self {
        RealHost {
            installed,
            cache: std::sync::OnceLock::new(),
        }
    }
}

impl<F: Fn() -> Vec<InstalledApp>> Host for RealHost<F> {
    fn os(&self) -> Os {
        Os::current()
    }
    fn exists(&self, path: &str) -> bool {
        std::path::Path::new(path).exists()
    }
    fn is_dir(&self, path: &str) -> bool {
        std::path::Path::new(path).is_dir()
    }
    fn is_executable(&self, path: &str) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(path)
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        }
        #[cfg(not(unix))]
        {
            has_ext(path, &["exe", "com", "bat", "cmd"]) && std::path::Path::new(path).is_file()
        }
    }
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    }
    fn installed(&self) -> Vec<InstalledApp> {
        self.cache.get_or_init(|| (self.installed)()).clone()
    }
    fn which(&self, name: &str) -> Option<String> {
        let path = std::env::var_os("PATH")?;
        let exts: Vec<String> = if cfg!(windows) {
            let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into());
            std::iter::once(String::new())
                .chain(
                    pathext
                        .split(';')
                        .filter(|e| !e.is_empty())
                        .map(str::to_string),
                )
                .collect()
        } else {
            vec![String::new()]
        };
        std::env::split_paths(&path).find_map(|dir| {
            exts.iter().find_map(|ext| {
                let candidate = dir.join(format!("{name}{ext}"));
                let text = candidate.to_string_lossy().into_owned();
                self.is_executable(&text).then_some(text)
            })
        })
    }
    fn desktop_entry(&self, id: &str) -> Option<String> {
        let mut dirs = Vec::new();
        if let Some(home) = self
            .var("XDG_DATA_HOME")
            .or_else(|| self.var("HOME").map(|h| format!("{h}/.local/share")))
        {
            dirs.push(home);
        }
        let data = self
            .var("XDG_DATA_DIRS")
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        dirs.extend(
            data.split(':')
                .filter(|d| !d.is_empty())
                .map(str::to_string),
        );
        dirs.into_iter()
            .map(|d| format!("{d}/applications/{id}"))
            .find(|p| self.exists(p))
    }
}

/// What a file asks to open: one target, or one per OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    One(String),
    PerOs(PerOsTargets),
}

/// The entry of a per-OS `open` that `os` runs (a pure function: tests pick
/// every OS on any computer).
pub fn select(targets: &PerOsTargets, os: Os) -> Option<&str> {
    match os {
        Os::Windows => targets.windows.as_deref(),
        Os::MacOs => targets.macos.as_deref(),
        Os::Linux => targets.linux.as_deref(),
    }
}

/// `targets` with `os`'s entry replaced (`None` removes it).
pub fn with_entry(targets: &PerOsTargets, os: Os, entry: Option<String>) -> PerOsTargets {
    let mut out = targets.clone();
    *match os {
        Os::Windows => &mut out.windows,
        Os::MacOs => &mut out.macos,
        Os::Linux => &mut out.linux,
    } = entry;
    out
}

/// The target an off key keeps when the file has no entry for `os`: the
/// first other OS's, so the key still says what it was.
pub fn fallback(targets: &PerOsTargets) -> String {
    [&targets.windows, &targets.macos, &targets.linux]
        .into_iter()
        .flatten()
        .next()
        .cloned()
        .unwrap_or_default()
}

/// `open` with its arguments and start folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSpec {
    pub target: Target,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

/// The LaunchApp an `open` resolves to, and a note when it is off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub action: Action,
    pub note: Option<String>,
}

pub fn is_url(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && lower.len() > 8
        && !lower.contains(char::is_whitespace)
}

/// `com.spotify.client`: reverse-DNS, no slashes, at least three parts.
pub fn is_bundle_id(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() >= 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
        && parts[0]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && !has_ext(text, &["exe", "app", "lnk", "bat", "cmd", "desktop", "com"])
}

fn has_ext(path: &str, exts: &[&str]) -> bool {
    path.rsplit_once('.').is_some_and(|(_, e)| {
        !e.contains(['/', '\\']) && exts.iter().any(|x| e.eq_ignore_ascii_case(x))
    })
}

fn has_separator(text: &str) -> bool {
    text.contains(['/', '\\'])
}

/// `%LOCALAPPDATA%`, `$HOME`, `${HOME}` and a leading `~` expanded; an
/// unknown variable stays as written.
pub fn expand(text: &str, host: &dyn Host) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    if chars.first() == Some(&'~')
        && matches!(chars.get(1), None | Some('/' | '\\'))
        && let Some(home) = host.var("HOME").or_else(|| host.var("USERPROFILE"))
    {
        out.push_str(&home);
        i = 1;
    }
    while i < chars.len() {
        let c = chars[i];
        if c == '%' {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '%') {
                let name: String = chars[i + 1..i + 1 + end].iter().collect();
                if !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '(' || c == ')')
                    && let Some(v) = host.var(&name)
                {
                    out.push_str(&v);
                    i += end + 2;
                    continue;
                }
            }
        } else if c == '$' {
            let (name, len) = if chars.get(i + 1) == Some(&'{') {
                match chars[i + 2..].iter().position(|&c| c == '}') {
                    Some(end) => (
                        chars[i + 2..i + 2 + end].iter().collect::<String>(),
                        end + 3,
                    ),
                    None => (String::new(), 0),
                }
            } else {
                let name: String = chars[i + 1..]
                    .iter()
                    .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
                    .collect();
                let len = name.chars().count() + 1;
                (name, len)
            };
            if !name.is_empty()
                && let Some(v) = host.var(&name)
            {
                out.push_str(&v);
                i += len;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn launch(path: impl Into<String>, args: Vec<String>, cwd: &Option<String>) -> Action {
    Action::LaunchApp {
        path: path.into(),
        args,
        enabled: true,
        cwd: cwd.clone(),
        per_os: None,
    }
}

fn off(action: Action, note: String) -> Resolved {
    let action = match action {
        Action::LaunchApp {
            path,
            args,
            cwd,
            per_os,
            ..
        } => Action::LaunchApp {
            path,
            args,
            enabled: false,
            cwd,
            per_os,
        },
        other => other,
    };
    Resolved {
        action,
        note: Some(note),
    }
}

fn on(action: Action) -> Resolved {
    Resolved { action, note: None }
}

/// What runs a file or folder that exists at `path`.
fn for_path(path: &str, args: &[String], cwd: &Option<String>, host: &dyn Host) -> Resolved {
    let os = host.os();
    let args = args.to_vec();
    let opened = |extra: Vec<String>| {
        launch(
            os.opener(),
            std::iter::once(path.to_string()).chain(extra).collect(),
            cwd,
        )
    };
    let no_args = |what: &str| {
        off(
            opened(Vec::new()),
            format!(
                "{what} cannot take args; open the program itself (its full path) instead. The key is off"
            ),
        )
    };
    match os {
        Os::Windows => {
            if has_ext(path, &["exe", "com", "bat", "cmd"]) && !host.is_dir(path) {
                on(launch(path, args, cwd))
            } else if args.is_empty() {
                on(opened(Vec::new()))
            } else {
                no_args(&format!(
                    "{path} is not a program (.exe, .bat or .cmd), so it"
                ))
            }
        }
        Os::MacOs => {
            if has_ext(path, &["app"]) {
                let extra = if args.is_empty() {
                    Vec::new()
                } else {
                    std::iter::once("--args".to_string()).chain(args).collect()
                };
                on(opened(extra))
            } else if host.is_executable(path) {
                on(launch(path, args, cwd))
            } else if args.is_empty() {
                on(opened(Vec::new()))
            } else {
                no_args(&format!("{path} is not a program or .app, so it"))
            }
        }
        Os::Linux => {
            if has_ext(path, &["desktop"]) {
                on(launch(
                    "gio",
                    ["launch".to_string(), path.to_string()]
                        .into_iter()
                        .chain(args)
                        .collect(),
                    cwd,
                ))
            } else if host.is_executable(path) {
                on(launch(path, args, cwd))
            } else if args.is_empty() {
                on(opened(Vec::new()))
            } else {
                no_args(&format!("{path} is not a program or .desktop file, so it"))
            }
        }
    }
}

fn strip_app_ext(name: &str) -> &str {
    for ext in [".exe", ".app", ".lnk", ".desktop"] {
        if name.len() > ext.len() && name[name.len() - ext.len()..].eq_ignore_ascii_case(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
    name
}

/// Resolves an `open` on `host` (see the module docs for the order). A
/// per-OS `open` runs this OS's entry and keeps every entry in `per_os`.
pub fn resolve(spec: &OpenSpec, host: &dyn Host) -> Resolved {
    let os = host.os();
    let cwd = spec.cwd.as_ref().map(|c| expand(c, host));
    let targets = match &spec.target {
        Target::One(t) => return resolve_one(t.trim().to_string(), spec, &cwd, host),
        Target::PerOs(targets) => targets,
    };
    let mut resolved = match select(targets, os) {
        Some(t) => resolve_one(t.trim().to_string(), spec, &cwd, host),
        None => off(
            launch(fallback(targets), spec.args.clone(), &cwd),
            format!(
                "the file has no \"{}\" app to open on this computer. The key is off",
                os.field()
            ),
        ),
    };
    if let Action::LaunchApp { per_os, .. } = &mut resolved.action {
        *per_os = Some(targets.clone());
    }
    resolved
}

fn resolve_one(target: String, spec: &OpenSpec, cwd: &Option<String>, host: &dyn Host) -> Resolved {
    let os = host.os();
    let cwd = cwd.clone();
    let args = spec.args.clone();
    if is_url(&target) {
        return on(launch(os.opener(), vec![target], &None));
    }
    if os == Os::MacOs && !has_separator(&target) && is_bundle_id(&target) {
        let mut all = vec!["-b".to_string(), target];
        if !args.is_empty() {
            all.push("--args".into());
            all.extend(args);
        }
        return on(launch(os.opener(), all, &cwd));
    }
    if host.exists(&target) {
        return for_path(&target, &args, &cwd, host);
    }
    let expanded = expand(&target, host);
    if expanded != target && host.exists(&expanded) {
        return for_path(&expanded, &args, &cwd, host);
    }
    if !has_separator(&target) {
        if os == Os::Linux
            && has_ext(&target, &["desktop"])
            && let Some(file) = host.desktop_entry(&target)
        {
            return for_path(&file, &args, &cwd, host);
        }
        let name = strip_app_ext(&target);
        if let Some(app) = host
            .installed()
            .into_iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
        {
            if args.is_empty() {
                return on(launch(app.path, app.args, &cwd));
            }
            let opener = app.path == os.opener();
            return match (os, opener) {
                (Os::MacOs, true) => {
                    let all = app
                        .args
                        .into_iter()
                        .chain(std::iter::once("--args".to_string()))
                        .chain(args)
                        .collect();
                    on(launch(app.path, all, &cwd))
                }
                (_, true) => off(
                    launch(app.path, app.args, &cwd),
                    format!(
                        "{} is a Start Menu shortcut, which cannot take args; open the program's .exe by its full path instead. The key is off",
                        app.name
                    ),
                ),
                (_, false) => on(launch(
                    app.path,
                    app.args.into_iter().chain(args).collect(),
                    &cwd,
                )),
            };
        }
        if let Some(program) = host.which(&target) {
            return on(launch(program, args, &cwd));
        }
    }
    let shown = if expanded != target {
        format!("{target} ({expanded})")
    } else {
        target.clone()
    };
    off(
        launch(expanded, args, &cwd),
        format!(
            "there is no app or file called {shown} on this computer. The key is off until you pick the app again"
        ),
    )
}

/// What an `open` writes for a LaunchApp: the target and its own args.
pub fn unresolve(path: &str, args: &[String], os: Os) -> (String, Vec<String>) {
    let rest_after_args = |rest: &[String]| -> Vec<String> {
        match rest.split_first() {
            Some((first, more)) if first == "--args" => more.to_vec(),
            _ => rest.to_vec(),
        }
    };
    match os {
        Os::Windows if path.eq_ignore_ascii_case("explorer.exe") && args.len() == 1 => {
            (args[0].clone(), Vec::new())
        }
        Os::MacOs if path == "/usr/bin/open" => match args {
            [flag, id, rest @ ..] if flag == "-b" => (id.clone(), rest_after_args(rest)),
            [target, rest @ ..] if !target.starts_with('-') => {
                (target.clone(), rest_after_args(rest))
            }
            _ => (path.to_string(), args.to_vec()),
        },
        Os::Linux if path == "xdg-open" && args.len() == 1 => (args[0].clone(), Vec::new()),
        Os::Linux if path == "gio" && args.len() >= 2 && args[0] == "launch" => {
            (args[1].clone(), args[2..].to_vec())
        }
        _ => (path.to_string(), args.to_vec()),
    }
}

/// The shortest `open` target that resolves back to exactly this
/// LaunchApp on `host`: an installed app's name, else the path.
pub fn written_target(
    path: &str,
    args: &[String],
    cwd: &Option<String>,
    host: &dyn Host,
) -> (String, Vec<String>) {
    let (target, own) = unresolve(path, args, host.os());
    if is_url(&target) && own.is_empty() {
        return (target, own);
    }
    let same = |t: &str, a: &[String]| {
        let spec = OpenSpec {
            target: Target::One(t.to_string()),
            args: a.to_vec(),
            cwd: cwd.clone(),
        };
        matches!(resolve(&spec, host).action, Action::LaunchApp { path: p, args: x, .. } if p == path && x == args)
    };
    if host.os() != Os::Linux || !target.contains('/') {
        let named: Vec<InstalledApp> = host
            .installed()
            .into_iter()
            .filter(|app| app.path == path && args.starts_with(&app.args))
            .collect();
        for app in named {
            let extra = &args[app.args.len()..];
            let extra = if host.os() == Os::MacOs {
                rest_args(extra)
            } else {
                extra.to_vec()
            };
            if same(&app.name, &extra) {
                return (app.name, extra);
            }
        }
    }
    (target, own)
}

/// What a per-OS `open` writes for a LaunchApp that came from one: every
/// entry as the file had it, with this OS's entry kept when it still
/// resolves to exactly this LaunchApp, else the key's current app (it was
/// picked again here). An OS the file had no entry for stays without one
/// while the key still holds the stand-in a restore gave it.
pub fn written_per_os(
    path: &str,
    args: &[String],
    cwd: &Option<String>,
    targets: &PerOsTargets,
    host: &dyn Host,
) -> (PerOsTargets, Vec<String>) {
    let os = host.os();
    let (target, own) = written_target(path, args, cwd, host);
    let entry = match select(targets, os) {
        None if path == fallback(targets) => {
            return (targets.clone(), args.to_vec());
        }
        Some(raw) => {
            let spec = OpenSpec {
                target: Target::One(raw.to_string()),
                args: own.clone(),
                cwd: cwd.clone(),
            };
            let same = matches!(resolve(&spec, host).action,
                Action::LaunchApp { path: p, args: a, .. } if p == path && a == args);
            if same { raw.to_string() } else { target }
        }
        None => target,
    };
    (with_entry(targets, os, Some(entry)), own)
}

fn rest_args(extra: &[String]) -> Vec<String> {
    match extra.split_first() {
        Some((first, more)) if first == "--args" => more.to_vec(),
        _ => extra.to_vec(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// A made-up computer for the resolution tests.
    #[derive(Default)]
    pub struct FakeHost {
        pub os: Option<Os>,
        pub files: BTreeSet<String>,
        pub dirs: BTreeSet<String>,
        pub executables: BTreeSet<String>,
        pub vars: BTreeMap<String, String>,
        pub apps: Vec<InstalledApp>,
        pub path: BTreeMap<String, String>,
        pub desktop: BTreeMap<String, String>,
    }

    impl FakeHost {
        pub fn new(os: Os) -> Self {
            FakeHost {
                os: Some(os),
                ..Default::default()
            }
        }
        pub fn file(mut self, p: &str) -> Self {
            self.files.insert(p.into());
            self
        }
        pub fn exe(mut self, p: &str) -> Self {
            self.files.insert(p.into());
            self.executables.insert(p.into());
            self
        }
        pub fn dir(mut self, p: &str) -> Self {
            self.dirs.insert(p.into());
            self
        }
        pub fn var(mut self, k: &str, v: &str) -> Self {
            self.vars.insert(k.into(), v.into());
            self
        }
        pub fn app(mut self, name: &str, path: &str, args: &[&str]) -> Self {
            self.apps.push(InstalledApp {
                name: name.into(),
                path: path.into(),
                args: args.iter().map(|a| a.to_string()).collect(),
            });
            self
        }
    }

    impl Host for FakeHost {
        fn os(&self) -> Os {
            self.os.unwrap_or(Os::Windows)
        }
        fn exists(&self, p: &str) -> bool {
            self.files.contains(p) || self.dirs.contains(p)
        }
        fn is_dir(&self, p: &str) -> bool {
            self.dirs.contains(p)
        }
        fn is_executable(&self, p: &str) -> bool {
            self.executables.contains(p)
        }
        fn var(&self, n: &str) -> Option<String> {
            self.vars.get(n).cloned()
        }
        fn installed(&self) -> Vec<InstalledApp> {
            self.apps.clone()
        }
        fn which(&self, n: &str) -> Option<String> {
            self.path.get(n).cloned()
        }
        fn desktop_entry(&self, id: &str) -> Option<String> {
            self.desktop.get(id).cloned()
        }
    }

    fn spec(t: &str, args: &[&str], cwd: Option<&str>) -> OpenSpec {
        OpenSpec {
            target: Target::One(t.into()),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: cwd.map(str::to_string),
        }
    }

    fn la(path: &str, args: &[&str], enabled: bool, cwd: Option<&str>) -> Action {
        Action::LaunchApp {
            path: path.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            enabled,
            cwd: cwd.map(str::to_string),
            per_os: None,
        }
    }

    fn windows() -> FakeHost {
        FakeHost::new(Os::Windows)
            .file(r"C:\Program Files\Spotify\Spotify.exe")
            .file(r"C:\Users\me\AppData\Local\Tools\run.cmd")
            .file(r"C:\Users\me\notes.txt")
            .var("LOCALAPPDATA", r"C:\Users\me\AppData\Local")
            .var("USERPROFILE", r"C:\Users\me")
            .app(
                "Spotify",
                "explorer.exe",
                &[r"C:\ProgramData\Start Menu\Spotify.lnk"],
            )
            .app("Notepad", r"C:\Windows\notepad.exe", &[])
    }

    #[test]
    fn windows_paths_env_vars_names_and_urls() {
        let h = windows();
        let r = |t: &str, a: &[&str]| resolve(&spec(t, a, None), &h);
        assert_eq!(
            r(r"C:\Program Files\Spotify\Spotify.exe", &[]).action,
            la(r"C:\Program Files\Spotify\Spotify.exe", &[], true, None)
        );
        assert_eq!(
            r(r"%LOCALAPPDATA%\Tools\run.cmd", &["a b", "c"]).action,
            la(
                r"C:\Users\me\AppData\Local\Tools\run.cmd",
                &["a b", "c"],
                true,
                None
            )
        );
        assert_eq!(
            r(r"~\notes.txt", &[]).action,
            la("explorer.exe", &[r"C:\Users\me\notes.txt"], true, None)
        );
        assert_eq!(
            r("spotify", &[]).action,
            la(
                "explorer.exe",
                &[r"C:\ProgramData\Start Menu\Spotify.lnk"],
                true,
                None
            )
        );
        assert_eq!(r("Spotify.exe", &[]).action, r("spotify", &[]).action);
        assert_eq!(
            r("Notepad", &["x.txt"]).action,
            la(r"C:\Windows\notepad.exe", &["x.txt"], true, None)
        );
        assert_eq!(
            r("https://youtube.com", &[]).action,
            la("explorer.exe", &["https://youtube.com"], true, None)
        );
        // A shortcut cannot take args: off, with a note.
        let lnk = r("Spotify", &["--x"]);
        assert!(matches!(
            lnk.action,
            Action::LaunchApp { enabled: false, .. }
        ));
        assert!(lnk.note.unwrap().contains("cannot take args"));
        let txt = r(r"C:\Users\me\notes.txt", &["x"]);
        assert!(txt.note.unwrap().contains("not a program"));
    }

    #[test]
    fn an_unknown_app_stays_as_an_off_key_with_a_note() {
        let h = windows();
        let r = resolve(&spec("Photoshop", &["-x"], Some("%USERPROFILE%")), &h);
        assert_eq!(
            r.action,
            la("Photoshop", &["-x"], false, Some(r"C:\Users\me"))
        );
        assert_eq!(
            r.note.as_deref(),
            Some(
                "there is no app or file called Photoshop on this computer. The key is off until you pick the app again"
            )
        );
        let r = resolve(&spec(r"%NOPE%\x.exe", &[], None), &h);
        assert!(r.note.unwrap().contains(r"%NOPE%\x.exe"));
        let r = resolve(&spec(r"%LOCALAPPDATA%\gone.exe", &[], None), &h);
        assert!(
            r.note
                .unwrap()
                .contains(r"(C:\Users\me\AppData\Local\gone.exe)")
        );
    }

    #[test]
    fn macos_bundles_ids_executables_and_home() {
        let h = FakeHost::new(Os::MacOs)
            .dir("/Users/me/Applications/Spotify.app")
            .exe("/usr/local/bin/tool")
            .var("HOME", "/Users/me")
            .app("Safari", "/usr/bin/open", &["/Applications/Safari.app"]);
        let r = |t: &str, a: &[&str]| resolve(&spec(t, a, None), &h).action;
        assert_eq!(
            r("~/Applications/Spotify.app", &[]),
            la(
                "/usr/bin/open",
                &["/Users/me/Applications/Spotify.app"],
                true,
                None
            )
        );
        assert_eq!(
            r("$HOME/Applications/Spotify.app", &["--a"]),
            la(
                "/usr/bin/open",
                &["/Users/me/Applications/Spotify.app", "--args", "--a"],
                true,
                None
            )
        );
        assert_eq!(
            r("com.spotify.client", &[]),
            la("/usr/bin/open", &["-b", "com.spotify.client"], true, None)
        );
        assert_eq!(
            r("com.spotify.client", &["x"]),
            la(
                "/usr/bin/open",
                &["-b", "com.spotify.client", "--args", "x"],
                true,
                None
            )
        );
        assert_eq!(
            r("/usr/local/bin/tool", &["-v"]),
            la("/usr/local/bin/tool", &["-v"], true, None)
        );
        assert_eq!(
            r("safari", &["https://a.b"]),
            la(
                "/usr/bin/open",
                &["/Applications/Safari.app", "--args", "https://a.b"],
                true,
                None
            )
        );
        assert_eq!(
            r("https://x.org/a?b=1", &[]),
            la("/usr/bin/open", &["https://x.org/a?b=1"], true, None)
        );
    }

    #[test]
    fn linux_desktop_ids_executables_and_path() {
        let mut h = FakeHost::new(Os::Linux)
            .exe("/opt/app/run")
            .var("HOME", "/home/me");
        h.desktop.insert(
            "spotify.desktop".into(),
            "/usr/share/applications/spotify.desktop".into(),
        );
        h.path.insert("gimp".into(), "/usr/bin/gimp".into());
        let r = |t: &str, a: &[&str]| resolve(&spec(t, a, Some("~/work")), &h).action;
        assert_eq!(
            r("spotify.desktop", &[]),
            la(
                "gio",
                &["launch", "/usr/share/applications/spotify.desktop"],
                true,
                Some("/home/me/work")
            )
        );
        assert_eq!(
            r("/opt/app/run", &["x"]),
            la("/opt/app/run", &["x"], true, Some("/home/me/work"))
        );
        assert_eq!(
            r("gimp", &["a.png"]),
            la("/usr/bin/gimp", &["a.png"], true, Some("/home/me/work"))
        );
        assert_eq!(
            r("https://x.org", &[]),
            la("xdg-open", &["https://x.org"], true, None)
        );
        assert!(matches!(
            r("com.spotify.client", &[]),
            Action::LaunchApp { enabled: false, .. }
        ));
    }

    fn three() -> PerOsTargets {
        PerOsTargets {
            windows: Some("W".into()),
            macos: Some("M".into()),
            linux: Some("L".into()),
        }
    }

    #[test]
    fn select_picks_windows_entry() {
        assert_eq!(select(&three(), Os::Windows), Some("W"));
        let none = PerOsTargets {
            windows: None,
            ..three()
        };
        assert_eq!(select(&none, Os::Windows), None);
    }

    #[test]
    fn select_picks_macos_entry() {
        assert_eq!(select(&three(), Os::MacOs), Some("M"));
        let none = PerOsTargets {
            macos: None,
            ..three()
        };
        assert_eq!(select(&none, Os::MacOs), None);
    }

    #[test]
    fn select_picks_linux_entry() {
        assert_eq!(select(&three(), Os::Linux), Some("L"));
        let none = PerOsTargets {
            linux: None,
            ..three()
        };
        assert_eq!(select(&none, Os::Linux), None);
    }

    #[test]
    fn with_entry_changes_only_that_os() {
        let all = three();
        for os in [Os::Windows, Os::MacOs, Os::Linux] {
            let changed = with_entry(&all, os, Some("X".into()));
            for other in [Os::Windows, Os::MacOs, Os::Linux] {
                let want = if other == os {
                    Some("X")
                } else {
                    select(&all, other)
                };
                assert_eq!(select(&changed, other), want);
            }
            assert_eq!(select(&with_entry(&all, os, None), os), None);
        }
        assert_eq!(
            fallback(&PerOsTargets {
                windows: None,
                ..three()
            }),
            "M"
        );
        assert_eq!(fallback(&PerOsTargets::default()), "");
    }

    fn with_per_os(action: Action, targets: &PerOsTargets) -> Action {
        match action {
            Action::LaunchApp {
                path,
                args,
                enabled,
                cwd,
                ..
            } => Action::LaunchApp {
                path,
                args,
                enabled,
                cwd,
                per_os: Some(targets.clone()),
            },
            other => other,
        }
    }

    #[test]
    fn per_os_variants_pick_this_computers_one() {
        let targets = PerOsTargets {
            windows: Some(r"C:\Program Files\Spotify\Spotify.exe".into()),
            macos: Some("com.spotify.client".into()),
            linux: None,
        };
        let s = OpenSpec {
            target: Target::PerOs(targets.clone()),
            args: vec![],
            cwd: None,
        };
        assert_eq!(
            resolve(&s, &windows()).action,
            with_per_os(
                la(r"C:\Program Files\Spotify\Spotify.exe", &[], true, None),
                &targets
            )
        );
        assert_eq!(
            resolve(&s, &FakeHost::new(Os::MacOs)).action,
            with_per_os(
                la("/usr/bin/open", &["-b", "com.spotify.client"], true, None),
                &targets
            )
        );
        let linux = resolve(&s, &FakeHost::new(Os::Linux));
        assert!(matches!(
            linux.action,
            Action::LaunchApp { enabled: false, .. }
        ));
        assert!(linux.note.unwrap().contains("no \"linux\" app"));
        assert_eq!(
            linux.action,
            with_per_os(
                la(r"C:\Program Files\Spotify\Spotify.exe", &[], false, None),
                &targets
            )
        );
    }

    #[test]
    fn a_per_os_export_keeps_every_entry_on_every_os() {
        let targets = PerOsTargets {
            windows: Some(r"%LOCALAPPDATA%\Tools\run.cmd".into()),
            macos: Some("com.spotify.client".into()),
            linux: Some("spotify.desktop".into()),
        };
        let args = vec!["--x".to_string()];
        let s = OpenSpec {
            target: Target::PerOs(targets.clone()),
            args: args.clone(),
            cwd: None,
        };
        let mut linux = FakeHost::new(Os::Linux);
        linux.desktop.insert(
            "spotify.desktop".into(),
            "/usr/share/applications/spotify.desktop".into(),
        );
        for host in [&windows() as &dyn Host, &FakeHost::new(Os::MacOs), &linux] {
            let Action::LaunchApp {
                path,
                args: a,
                cwd,
                per_os,
                enabled,
            } = resolve(&s, host).action
            else {
                unreachable!()
            };
            assert!(enabled, "{:?}", host.os());
            assert_eq!(per_os.as_ref(), Some(&targets));
            assert_eq!(
                written_per_os(&path, &a, &cwd, &targets, host),
                (targets.clone(), args.clone()),
                "{:?}",
                host.os()
            );
        }
        // Picked again here: this OS's entry follows, the others stay.
        let (out, own) =
            written_per_os(r"C:\Windows\notepad.exe", &[], &None, &targets, &windows());
        assert_eq!(
            out,
            PerOsTargets {
                windows: Some("Notepad".into()),
                ..targets.clone()
            }
        );
        assert!(own.is_empty());
        // No entry for this OS: the stand-in is not written as one.
        let partial = PerOsTargets {
            linux: None,
            ..targets.clone()
        };
        let s = OpenSpec {
            target: Target::PerOs(partial.clone()),
            args: vec![],
            cwd: None,
        };
        let Action::LaunchApp {
            path, args, cwd, ..
        } = resolve(&s, &linux).action
        else {
            unreachable!()
        };
        assert_eq!(
            written_per_os(&path, &args, &cwd, &partial, &linux).0,
            partial
        );
    }

    #[test]
    fn export_writes_the_name_when_it_resolves_back_else_the_path() {
        let h = windows();
        let back = |path: &str, args: &[&str]| {
            let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            written_target(path, &args, &None, &h)
        };
        assert_eq!(
            back("explorer.exe", &[r"C:\ProgramData\Start Menu\Spotify.lnk"]),
            ("Spotify".into(), vec![])
        );
        assert_eq!(
            back(r"C:\Windows\notepad.exe", &["a.txt"]),
            ("Notepad".into(), vec!["a.txt".into()])
        );
        assert_eq!(
            back(r"C:\Program Files\Spotify\Spotify.exe", &[]),
            (r"C:\Program Files\Spotify\Spotify.exe".into(), vec![])
        );
        assert_eq!(
            back("explorer.exe", &["https://a.org"]),
            ("https://a.org".into(), vec![])
        );
        assert_eq!(
            unresolve(
                "/usr/bin/open",
                &["-b".into(), "com.x.y".into(), "--args".into(), "1".into()],
                Os::MacOs
            ),
            ("com.x.y".into(), vec!["1".into()])
        );
        assert_eq!(
            unresolve("gio", &["launch".into(), "/a.desktop".into()], Os::Linux),
            ("/a.desktop".into(), vec![])
        );
    }

    #[test]
    fn expansion_rules() {
        let h = FakeHost::new(Os::Linux)
            .var("HOME", "/h")
            .var("ProgramFiles", r"C:\PF")
            .var("X_1", "x");
        assert_eq!(expand("~/a", &h), "/h/a");
        assert_eq!(expand("a~/b", &h), "a~/b");
        assert_eq!(
            expand("$HOME/${X_1}/$NOPE/%ProgramFiles%\\y/100%", &h),
            r"/h/x/$NOPE/C:\PF\y/100%"
        );
        assert!(is_bundle_id("com.spotify.client"));
        assert!(
            !is_bundle_id("Spotify.exe") && !is_bundle_id("a.b") && !is_bundle_id("my.app.exe")
        );
    }
}
