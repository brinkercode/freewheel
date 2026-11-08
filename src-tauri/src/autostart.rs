//! Start at login: the OS login item (`auto-launch`).
//!
//! `auto-launch` writes the program path into the login item as it is:
//! Linux's `Exec=` line in `~/.config/autostart/freewheel.desktop` and the
//! Windows `Run` value are both command lines, so a path with a space in it
//! (`~/My Apps/freewheel.AppImage`) started the program `~/My` (#197). Here
//! the path is quoted for each OS before it gets there.

use auto_launch::{AutoLaunch, AutoLaunchBuilder};
use tauri::{AppHandle, Runtime};

/// The login item's name: the `.desktop` file, the `Run` value and the
/// LaunchAgent's label all use it (the same name as freewheel 1.1.0 and
/// earlier used, so turning it off also removes their entry).
const NAME: &str = "freewheel";

/// The program the login item starts, as this OS's login item spells it.
///
/// - Linux: one quoted argument of a Desktop Entry `Exec` key. Inside the
///   quotes `"`, `` ` ``, `$` and `\` are escaped with `\`, then the whole
///   value is escaped as a desktop entry string (each `\` doubled), and `%`
///   becomes `%%` (it starts a field code).
/// - Windows: the path in double quotes (a Windows path can't contain `"`).
/// - macOS: the path as it is (the LaunchAgent lists program arguments one
///   by one, nothing is split).
pub fn login_item_path(os: &str, program: &str) -> Result<String, String> {
    match os {
        "linux" => {
            if program.chars().any(|c| c.is_control()) {
                return Err(format!(
                    "Start at login can't start {program:?}: the path has a line break or other control character"
                ));
            }
            let mut quoted = String::from("\"");
            for c in program.chars() {
                match c {
                    '"' | '`' | '$' => {
                        // `\"` inside the quotes, then the string escape of `\`.
                        quoted.push_str("\\\\");
                        quoted.push(c);
                    }
                    '\\' => quoted.push_str("\\\\\\\\"),
                    '%' => quoted.push_str("%%"),
                    c => quoted.push(c),
                }
            }
            quoted.push('"');
            Ok(quoted)
        }
        "windows" => Ok(format!("\"{program}\"")),
        _ => Ok(program.to_string()),
    }
}

/// The program to start at login: the AppImage file when running from one
/// (the binary inside it lives in a mount that is gone after a restart),
/// else this executable (on macOS its real path, as the LaunchAgent needs).
fn program<R: Runtime>(app: &AppHandle<R>) -> Result<String, String> {
    // Only Linux has AppImages (`Env::appimage` is Linux-only).
    #[cfg(target_os = "linux")]
    if let Some(image) = tauri::Manager::env(app)
        .appimage
        .as_deref()
        .and_then(std::ffi::OsStr::to_str)
    {
        return Ok(image.to_string());
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = if cfg!(target_os = "macos") {
        exe.canonicalize().map_err(|e| e.to_string())?
    } else {
        exe
    };
    Ok(exe.display().to_string())
}

fn login_item(program: &str) -> Result<AutoLaunch, String> {
    let mut builder = AutoLaunchBuilder::new();
    builder
        .set_app_name(NAME)
        .set_app_path(&login_item_path(std::env::consts::OS, program)?);
    #[cfg(target_os = "macos")]
    builder.set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent);
    builder.build().map_err(|e| e.to_string())
}

/// Whether the login item is there.
pub fn is_enabled<R: Runtime>(app: &AppHandle<R>) -> Result<bool, String> {
    login_item(&program(app)?)?
        .is_enabled()
        .map_err(|e| e.to_string())
}

/// Adds (or rewrites) or removes the login item.
pub fn set<R: Runtime>(app: &AppHandle<R>, on: bool) -> Result<(), String> {
    let item = login_item(&program(app)?)?;
    if on {
        item.enable().map_err(|e| e.to_string())
    } else {
        match item.disable() {
            Err(auto_launch::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result.map_err(|e| e.to_string()),
        }
    }
}

/// At startup: a login item that is on is written again, so one an older
/// freewheel wrote with an unquoted path (or for an AppImage since moved)
/// starts this freewheel from now on.
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    match is_enabled(app) {
        Ok(true) => {
            if let Err(e) = set(app, true) {
                tracing::warn!(error = %e, "cannot rewrite the start-at-login item");
            }
        }
        Ok(false) => {}
        Err(e) => tracing::warn!(error = %e, "cannot read the start-at-login state"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Splits a Desktop Entry `Exec` value into the program and its
    /// arguments, the way the Desktop Entry spec says a session does: undo
    /// the string escapes, then split on spaces outside double quotes, where
    /// `\` escapes the next character; `%%` is a literal `%`.
    fn desktop_exec_argv(value: &str) -> Vec<String> {
        let mut unescaped = String::new();
        let mut chars = value.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('s') => unescaped.push(' '),
                    Some('n') => unescaped.push('\n'),
                    Some('t') => unescaped.push('\t'),
                    Some('r') => unescaped.push('\r'),
                    Some(other) => unescaped.push(other),
                    None => {}
                }
            } else {
                unescaped.push(c);
            }
        }
        let mut args = Vec::new();
        let mut arg = String::new();
        let mut started = false;
        let mut quoted = false;
        let mut chars = unescaped.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' => {
                    quoted = !quoted;
                    started = true;
                }
                '\\' if quoted => {
                    if let Some(next) = chars.next() {
                        arg.push(next);
                    }
                }
                '%' if chars.peek() == Some(&'%') => {
                    chars.next();
                    arg.push('%');
                    started = true;
                }
                ' ' if !quoted => {
                    if started {
                        args.push(std::mem::take(&mut arg));
                        started = false;
                    }
                }
                c => {
                    arg.push(c);
                    started = true;
                }
            }
        }
        if started {
            args.push(arg);
        }
        args
    }

    /// The `Exec=` value `auto-launch` writes for this login item (its
    /// `build_xdg_autostart_data`: `Exec={app_path} {args joined}`, no args).
    fn exec_value(program: &str) -> String {
        format!("{} ", login_item_path("linux", program).unwrap())
    }

    #[test]
    fn linux_login_item_starts_an_appimage_in_a_folder_with_a_space() {
        let program = "/home/me/My Apps/freewheel-1.1.0-linux-x86_64.AppImage";
        assert_eq!(
            desktop_exec_argv(&exec_value(program)),
            vec![program.to_string()]
        );
    }

    #[test]
    fn linux_login_item_keeps_every_reserved_character_of_the_path() {
        for program in [
            "/home/me/Apps/freewheel.AppImage",
            "/home/me/it's \"mine\"/fw $HOME `x` 100% \\ back/freewheel.AppImage",
            "/home/me/a  b/(c) & d; e|f <g> #h*?.AppImage",
        ] {
            assert_eq!(
                desktop_exec_argv(&exec_value(program)),
                vec![program.to_string()],
                "Exec={}",
                exec_value(program)
            );
        }
    }

    #[test]
    fn linux_login_item_refuses_a_path_with_a_line_break() {
        assert!(login_item_path("linux", "/home/me/a\nb/freewheel").is_err());
    }

    #[test]
    fn windows_run_value_quotes_the_program() {
        // auto-launch's Run value is `{app_path} {args joined}`: Windows
        // reads the first quoted part as the program.
        let path = login_item_path("windows", r"C:\Users\me\My Apps\freewheel.exe").unwrap();
        let value = format!("{path} ");
        assert_eq!(value, "\"C:\\Users\\me\\My Apps\\freewheel.exe\" ");
    }

    #[test]
    fn macos_launch_agent_gets_the_path_as_it_is() {
        let program = "/Applications/My Apps/freewheel.app/Contents/MacOS/freewheel";
        assert_eq!(login_item_path("macos", program).unwrap(), program);
    }

    #[test]
    fn this_os_login_item_is_built_with_the_quoted_path() {
        let program = if cfg!(windows) {
            r"C:\My Apps\freewheel.exe"
        } else {
            "/home/me/My Apps/freewheel.AppImage"
        };
        let item = login_item(program).unwrap();
        assert_eq!(
            item.get_app_path(),
            login_item_path(std::env::consts::OS, program).unwrap()
        );
        assert_eq!(item.get_app_name(), NAME);
        assert!(item.get_args().is_empty());
    }
}
