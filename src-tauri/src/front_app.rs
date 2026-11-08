//! Profiles per app: which app is in front, told by the OS when it changes
//! (never polled), and the name a profile keeps for an app the picker chose.
//!
//! - Windows: `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` on a thread of its
//!   own (its message loop sleeps until an event) -> the window's process ->
//!   its program file (`notepad.exe`).
//! - macOS: NSWorkspace's did-activate-application notification on the main
//!   run loop (no permission needed) -> the app's bundle id.
//! - Linux on X11: `PropertyNotify` of `_NET_ACTIVE_WINDOW` on the root
//!   window (the thread blocks on the X connection) -> the window's process
//!   and its `WM_CLASS`.
//! - Linux on Wayland: no portable way to know, so profiles switch by key
//!   only ([`supported`] is false and the Keys tab says so).
//!
//! Each change reaches the device loop (or the --simulate session) as
//! [`Command::FrontApp`](crate::device::Command::FrontApp).

use std::path::Path;
use std::sync::mpsc::Sender;

use freewheel_core::config::ProfileApp;
use freewheel_core::mappings_file::Os;
use freewheel_core::profiles::FrontApp;

use crate::apps::AppChoice;
use crate::device::Command;

/// Whether this computer tells freewheel which app is in front.
pub fn supported() -> bool {
    #[cfg(target_os = "linux")]
    {
        crate::linux_inject::current_session() != crate::linux_inject::Session::Wayland
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// The Keys tab: whether profiles can follow the app in front here.
#[tauri::command]
pub fn front_app_support() -> bool {
    supported()
}

/// The front app from a process's names: `own` when it is this process.
pub fn front_app(pid: u32, ids: Vec<String>, name: Option<String>) -> Option<FrontApp> {
    if pid == std::process::id() {
        return Some(FrontApp::own());
    }
    let mut unique: Vec<String> = Vec::new();
    for id in ids {
        let id = id.trim().to_string();
        if !id.is_empty() && !unique.contains(&id) {
            unique.push(id);
        }
    }
    let first = unique.first()?.clone();
    Some(FrontApp {
        name: name.filter(|n| !n.trim().is_empty()).unwrap_or(first),
        ids: unique,
        own: false,
    })
}

/// Starts watching the app in front; each change goes to `commands`.
/// On macOS this must run on the main thread (Tauri's setup does).
pub fn watch(commands: Sender<Command>) {
    if !supported() {
        tracing::info!("the app in front is not visible here (Wayland): profiles switch by key");
        return;
    }
    let send = move |app: FrontApp| {
        tracing::debug!(app = %app.name, own = app.own, "app in front");
        let _ = commands.send(Command::FrontApp(app));
    };
    #[cfg(windows)]
    win::watch(send);
    #[cfg(target_os = "macos")]
    mac::watch(send);
    #[cfg(target_os = "linux")]
    x11::watch(send);
}

#[cfg(windows)]
mod win {
    use std::sync::OnceLock;

    use freewheel_core::profiles::FrontApp;
    use windows::Win32::Foundation::{CloseHandle, HWND};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, GetMessageW,
        GetWindowThreadProcessId, MSG, WINEVENT_OUTOFCONTEXT,
    };

    type Sink = Box<dyn Fn(FrontApp) + Send + Sync>;
    static SINK: OnceLock<Sink> = OnceLock::new();

    /// The program file of the process that owns `hwnd`.
    fn app_of(hwnd: HWND) -> Option<FrontApp> {
        let mut pid = 0u32;
        // SAFETY: plain query on a window handle; a stale one yields pid 0.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut pid)) };
        if pid == 0 {
            return None;
        }
        if pid == std::process::id() {
            return Some(FrontApp::own());
        }
        // SAFETY: the handle is closed below; the buffer outlives the call.
        let path = unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &raw mut len,
            );
            let _ = CloseHandle(process);
            ok.ok()?;
            String::from_utf16_lossy(&buf[..len as usize])
        };
        let file = path.rsplit('\\').next().unwrap_or(&path).to_string();
        super::front_app(pid, vec![file], None)
    }

    unsafe extern "system" fn on_foreground(
        _hook: HWINEVENTHOOK,
        _event: u32,
        hwnd: HWND,
        _object: i32,
        _child: i32,
        _thread: u32,
        _time: u32,
    ) {
        if let (Some(sink), Some(app)) = (SINK.get(), app_of(hwnd)) {
            sink(app);
        }
    }

    pub fn watch(send: impl Fn(FrontApp) + Send + Sync + 'static) {
        if SINK.set(Box::new(send)).is_err() {
            return;
        }
        std::thread::spawn(|| {
            // SAFETY: the hook lives as long as this thread, whose message
            // loop (GetMessageW sleeps until a message) delivers its events.
            unsafe {
                let hook = SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND,
                    EVENT_SYSTEM_FOREGROUND,
                    None,
                    Some(on_foreground),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                );
                if hook.is_invalid() {
                    tracing::warn!("watching the app in front failed: profiles switch by key");
                    return;
                }
                if let (Some(sink), Some(app)) = (SINK.get(), app_of(GetForegroundWindow())) {
                    sink(app);
                }
                let mut msg = MSG::default();
                while GetMessageW(&raw mut msg, None, 0, 0).as_bool() {
                    DispatchMessageW(&raw const msg);
                }
            }
        });
    }

    /// The program a Start Menu shortcut starts.
    pub fn lnk_target(lnk: &std::path::Path) -> Option<std::path::PathBuf> {
        use windows::Win32::System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize, IPersistFile, STGM_READ,
        };
        use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
        use windows::core::{HSTRING, Interface};
        // SAFETY: COM is set up and torn down on this (blocking) thread.
        unsafe {
            let inited = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let target = (|| -> windows::core::Result<String> {
                let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
                link.cast::<IPersistFile>()?
                    .Load(&HSTRING::from(lnk.as_os_str()), STGM_READ)?;
                let mut buf = [0u16; 1024];
                link.GetPath(&mut buf, std::ptr::null_mut(), 0)?;
                let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                Ok(String::from_utf16_lossy(&buf[..len]))
            })();
            if inited {
                CoUninitialize();
            }
            target
                .ok()
                .filter(|t| !t.is_empty())
                .map(std::path::PathBuf::from)
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use freewheel_core::profiles::FrontApp;
    use objc2_app_kit::{
        NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey,
        NSWorkspaceDidActivateApplicationNotification,
    };
    use objc2_foundation::{NSBundle, NSNotification, NSString};

    fn app_of(app: &NSRunningApplication) -> Option<FrontApp> {
        let pid = u32::try_from(app.processIdentifier()).ok()?;
        let bundle = app.bundleIdentifier().map(|s| s.to_string());
        let exe = app
            .executableURL()
            .and_then(|u| u.lastPathComponent())
            .map(|s| s.to_string());
        let name = app.localizedName().map(|s| s.to_string());
        super::front_app(pid, bundle.into_iter().chain(exe).collect(), name)
    }

    pub fn watch(send: impl Fn(FrontApp) + Send + Sync + 'static) {
        let workspace = NSWorkspace::sharedWorkspace();
        if let Some(app) = workspace.frontmostApplication().as_deref().and_then(app_of) {
            send(app);
        }
        let block = RcBlock::new(move |note: NonNull<NSNotification>| {
            // SAFETY: AppKit passes a valid notification for the call.
            let note = unsafe { note.as_ref() };
            let running = note
                .userInfo()
                .and_then(|info| {
                    // SAFETY: the key is AppKit's own constant.
                    let key = unsafe { NSWorkspaceApplicationKey };
                    info.objectForKey(key)
                })
                .and_then(|o| o.downcast::<NSRunningApplication>().ok())
                .or_else(|| NSWorkspace::sharedWorkspace().frontmostApplication());
            if let Some(app) = running.as_deref().and_then(app_of) {
                send(app);
            }
        });
        // SAFETY: a main-thread observer for AppKit's notification; it is
        // kept for the app's lifetime (never removed).
        let observer = unsafe {
            workspace
                .notificationCenter()
                .addObserverForName_object_queue_usingBlock(
                    Some(NSWorkspaceDidActivateApplicationNotification),
                    None,
                    None,
                    &block,
                )
        };
        std::mem::forget(observer);
    }

    /// The bundle id of an app bundle.
    pub fn bundle_id(app: &std::path::Path) -> Option<String> {
        let path = NSString::from_str(&app.to_string_lossy());
        NSBundle::bundleWithPath(&path)
            .and_then(|b| b.bundleIdentifier())
            .map(|s| s.to_string())
    }
}

#[cfg(target_os = "linux")]
mod x11 {
    use freewheel_core::profiles::FrontApp;
    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::{
        AtomEnum, ChangeWindowAttributesAux, ConnectionExt, EventMask, Window,
    };

    /// The program of process `pid` and its short name (`/proc`), and the
    /// AppImage file it runs from, if any: a profile that picked
    /// `krita-5.2.6-x86_64.AppImage` matches its window by that name too.
    pub fn process_names(pid: u32) -> Vec<String> {
        let exe = std::fs::read_link(format!("/proc/{pid}/exe"))
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()));
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .ok()
            .map(|s| s.trim().to_string());
        let image = std::fs::read(format!("/proc/{pid}/environ"))
            .ok()
            .and_then(|raw| super::appimage_of_environ(&raw));
        exe.into_iter().chain(comm).chain(image).collect()
    }

    /// `WM_CLASS`'s instance and class (two NUL-ended strings).
    pub fn wm_class(raw: &[u8]) -> Vec<String> {
        raw.split(|b| *b == 0)
            .map(|s| String::from_utf8_lossy(s).trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn run(send: &dyn Fn(FrontApp)) -> Result<(), String> {
        let err = |e: &dyn std::fmt::Display| e.to_string();
        let (conn, n) = x11rb::connect(None).map_err(|e| err(&e))?;
        let root = conn.setup().roots[n].root;
        let atom = |name: &[u8]| -> Result<u32, String> {
            Ok(conn
                .intern_atom(false, name)
                .map_err(|e| err(&e))?
                .reply()
                .map_err(|e| err(&e))?
                .atom)
        };
        let active = atom(b"_NET_ACTIVE_WINDOW")?;
        let wm_pid = atom(b"_NET_WM_PID")?;
        conn.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .map_err(|e| err(&e))?;
        conn.flush().map_err(|e| err(&e))?;
        let report = |window: Window| -> Option<FrontApp> {
            let pid = conn
                .get_property(false, window, wm_pid, AtomEnum::CARDINAL, 0, 1)
                .ok()?
                .reply()
                .ok()?
                .value32()
                .and_then(|mut v| v.next())
                .unwrap_or(0);
            let class = conn
                .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
                .ok()?
                .reply()
                .ok()
                .map(|r| wm_class(&r.value))
                .unwrap_or_default();
            let mut ids = if pid == 0 {
                Vec::new()
            } else {
                process_names(pid)
            };
            ids.extend(class.iter().cloned());
            if ids.is_empty() {
                // A window that names no program is still another app in
                // front (it goes back to Default): known by its title.
                let title = conn
                    .get_property(false, window, AtomEnum::WM_NAME, AtomEnum::ANY, 0, 256)
                    .ok()?
                    .reply()
                    .ok()
                    .map(|r| String::from_utf8_lossy(&r.value).into_owned())
                    .unwrap_or_default();
                let title = if title.trim().is_empty() {
                    format!("window 0x{window:x}")
                } else {
                    title
                };
                return super::front_app(pid, vec![title], None);
            }
            super::front_app(pid, ids, class.last().cloned())
        };
        let active_window = || -> Option<Window> {
            conn.get_property(false, root, active, AtomEnum::WINDOW, 0, 1)
                .ok()?
                .reply()
                .ok()?
                .value32()
                .and_then(|mut v| v.next())
                .filter(|w| *w != 0)
        };
        if let Some(app) = active_window().and_then(report) {
            send(app);
        }
        loop {
            // Blocks until the X server sends something: no polling.
            match conn.wait_for_event().map_err(|e| err(&e))? {
                Event::PropertyNotify(e) if e.window == root && e.atom == active => {
                    if let Some(app) = active_window().and_then(report) {
                        send(app);
                    }
                }
                _ => {}
            }
        }
    }

    pub fn watch(send: impl Fn(FrontApp) + Send + 'static) {
        std::thread::spawn(move || {
            if let Err(e) = run(&send) {
                tracing::warn!(error = %e, "watching the app in front failed: profiles switch by key");
            }
        });
    }
}

/// The AppImage file (its name) a process runs from: the AppImage runtime
/// sets `APPIMAGE` to the file's path for the program inside it. `raw` is
/// `/proc/<pid>/environ` (`NAME=value` entries, each NUL-ended).
#[cfg(any(target_os = "linux", test))]
pub fn appimage_of_environ(raw: &[u8]) -> Option<String> {
    raw.split(|b| *b == 0)
        .find_map(|entry| entry.strip_prefix(b"APPIMAGE="))
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .and_then(|path| {
            let file = path.rsplit('/').next().unwrap_or(&path).trim().to_string();
            (!file.is_empty()).then_some(file)
        })
}

/// The app a profile keeps for a picker choice on `os`, by the name that
/// OS's front-app watcher reports: the program file on Windows (a Start
/// Menu shortcut's target), the bundle id on macOS, the window class or
/// program on Linux (from a .desktop entry). `resolve` reads shortcuts,
/// bundles and desktop entries.
pub fn profile_app_for(choice: &AppChoice, os: Os, resolve: &dyn Resolve) -> ProfileApp {
    // Either separator: a file restored from another OS keeps its own.
    let file_name = |p: &str| p.rsplit(['/', '\\']).next().unwrap_or(p).to_string();
    // The thing the choice opens: the opener's argument or the program.
    let opened = choice
        .args
        .first()
        .filter(|_| {
            let p = file_name(&choice.path).to_lowercase();
            p == "explorer.exe" || p == "open" || p == "xdg-open"
        })
        .cloned()
        .unwrap_or_else(|| choice.path.clone());
    let lower = opened.to_lowercase();
    let id = match os {
        Os::Windows if lower.ends_with(".lnk") => resolve
            .lnk_target(Path::new(&opened))
            .map(|t| file_name(&t.to_string_lossy()))
            .unwrap_or_else(|| format!("{}.exe", choice.name)),
        Os::Windows => file_name(&opened),
        Os::MacOs => resolve
            .bundle_id(Path::new(&opened))
            .unwrap_or_else(|| choice.name.clone()),
        Os::Linux if lower.ends_with(".desktop") => resolve
            .desktop_program(Path::new(&opened))
            .unwrap_or_else(|| file_name(&opened).trim_end_matches(".desktop").to_string()),
        Os::Linux => file_name(&opened),
    };
    let mut app = ProfileApp {
        name: choice.name.clone(),
        ..ProfileApp::default()
    };
    match os {
        Os::Windows => app.windows = Some(id),
        Os::MacOs => app.macos = Some(id),
        Os::Linux => app.linux = Some(id),
    }
    app
}

/// What [`profile_app_for`] reads from the computer.
pub trait Resolve {
    fn lnk_target(&self, lnk: &Path) -> Option<std::path::PathBuf>;
    fn bundle_id(&self, app: &Path) -> Option<String>;
    /// A desktop entry's window class, else its program's file name.
    fn desktop_program(&self, desktop: &Path) -> Option<String>;
}

/// This computer.
pub struct RealResolve;

impl Resolve for RealResolve {
    fn lnk_target(&self, lnk: &Path) -> Option<std::path::PathBuf> {
        #[cfg(windows)]
        {
            win::lnk_target(lnk)
        }
        #[cfg(not(windows))]
        {
            let _ = lnk;
            None
        }
    }

    fn bundle_id(&self, app: &Path) -> Option<String> {
        #[cfg(target_os = "macos")]
        {
            mac::bundle_id(app)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = app;
            None
        }
    }

    fn desktop_program(&self, desktop: &Path) -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            let text = std::fs::read_to_string(desktop).ok()?;
            desktop_program(&text)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = desktop;
            None
        }
    }
}

/// A desktop entry's `StartupWMClass`, else its `Exec` program's file name
/// (past `env VAR=x` and `flatpak run`, which name the app last).
#[cfg(target_os = "linux")]
pub fn desktop_program(text: &str) -> Option<String> {
    let entry = crate::linux_desktop::parse_entry(text)?;
    if let Some(class) = entry.startup_wm_class.filter(|c| !c.trim().is_empty()) {
        return Some(class.trim().to_string());
    }
    let argv = crate::linux_desktop::exec_argv(&entry.exec)?;
    let mut args = argv.iter().map(String::as_str).peekable();
    let base = |s: &str| s.rsplit('/').next().unwrap_or(s).to_string();
    while let Some(a) = args.next() {
        match base(a).as_str() {
            "env" => {
                while args
                    .peek()
                    .is_some_and(|n| n.contains('=') || n.starts_with('-'))
                {
                    args.next();
                }
            }
            "flatpak" => {
                return args.rfind(|n| !n.starts_with('-') && *n != "run").map(base);
            }
            program => return Some(program.to_string()),
        }
    }
    None
}

/// The Keys tab's profile apps: what a profile keeps for a picked app.
/// Async: shortcuts and bundles are read off the main thread.
#[tauri::command]
pub async fn profile_app(choice: AppChoice) -> Result<ProfileApp, String> {
    let app = tauri::async_runtime::spawn_blocking(move || {
        profile_app_for(&choice, Os::current(), &RealResolve)
    })
    .await
    .map_err(|e| e.to_string())?;
    tracing::info!("app picked for a profile");
    Ok(app)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Fake;

    impl Resolve for Fake {
        fn lnk_target(&self, lnk: &Path) -> Option<PathBuf> {
            lnk.to_string_lossy()
                .contains("Notepad")
                .then(|| PathBuf::from(r"C:\Windows\System32\notepad.exe"))
        }
        fn bundle_id(&self, app: &Path) -> Option<String> {
            app.ends_with("TextEdit.app")
                .then(|| "com.apple.TextEdit".to_string())
        }
        fn desktop_program(&self, desktop: &Path) -> Option<String> {
            desktop
                .ends_with("org.gnome.gedit.desktop")
                .then(|| "gedit".to_string())
        }
    }

    fn choice(name: &str, path: &str, args: &[&str]) -> AppChoice {
        AppChoice {
            name: name.into(),
            path: path.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    }

    /// #200: the window of a running AppImage belongs to the program inside
    /// its mount (/proc/<pid>/exe = /tmp/.mount_xxx/usr/bin/krita, comm
    /// "krita", WM_CLASS "krita"/"Krita"), never to a process named after
    /// the file.
    #[test]
    fn a_picked_appimage_matches_its_window_in_front() {
        use freewheel_core::profiles::app_matches;
        let image = choice("Krita", "/home/me/Apps/krita-5.2.6-x86_64.AppImage", &[]);
        let kept = profile_app_for(&image, Os::Linux, &Fake);
        // What the X11 watcher reports for the AppImage's window.
        let front = front_app(
            1,
            vec![
                "krita".into(),
                "krita".into(),
                "krita".into(),
                "Krita".into(),
            ],
            Some("Krita".into()),
        )
        .unwrap();
        assert!(
            app_matches(&kept, Os::Linux, &front),
            "profile keeps {:?}; the window in front is {:?}",
            kept.linux,
            front.ids
        );
        // Any other app in front: no match.
        let other = front_app(1, vec!["gimp".into(), "Gimp".into()], None).unwrap();
        assert!(!app_matches(&kept, Os::Linux, &other));
    }

    /// An AppImage whose program isn't named like its file still matches:
    /// the watcher also reports the AppImage file from the process's
    /// `APPIMAGE`.
    #[test]
    fn the_window_of_an_appimage_is_also_known_by_the_appimage_file() {
        use freewheel_core::profiles::app_matches;
        let environ = b"HOME=/home/me\0APPIMAGE=/home/me/My Apps/Odd.Name-x86_64.AppImage\0APPDIR=/tmp/.mount_Odd\0";
        assert_eq!(
            appimage_of_environ(environ).as_deref(),
            Some("Odd.Name-x86_64.AppImage")
        );
        assert_eq!(appimage_of_environ(b"HOME=/home/me\0PATH=/usr/bin\0"), None);
        assert_eq!(appimage_of_environ(b"NOT_APPIMAGE=/x.AppImage\0"), None);
        let image = choice("Odd", "/home/me/My Apps/Odd.Name-x86_64.AppImage", &[]);
        let kept = profile_app_for(&image, Os::Linux, &Fake);
        let ids = vec![
            "electron".into(),
            "electron".into(),
            appimage_of_environ(environ).unwrap(),
            "unexpected".into(),
        ];
        let front = front_app(1, ids, None).unwrap();
        assert!(app_matches(&kept, Os::Linux, &front), "{:?}", front.ids);
    }

    #[test]
    fn a_picked_app_is_kept_by_the_name_its_os_reports_in_front() {
        let lnk = choice(
            "Notepad",
            "explorer.exe",
            &[r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Notepad.lnk"],
        );
        let app = profile_app_for(&lnk, Os::Windows, &Fake);
        assert_eq!(
            (app.name.as_str(), app.windows.as_deref(), app.macos),
            ("Notepad", Some("notepad.exe"), None)
        );
        let exe = choice("Tool", r"C:\Tools\Tool.exe", &[]);
        assert_eq!(
            profile_app_for(&exe, Os::Windows, &Fake).windows.as_deref(),
            Some("Tool.exe")
        );
        // A shortcut that cannot be read: the app's name as a program.
        let odd = choice("Odd", "explorer.exe", &[r"C:\x\Odd.lnk"]);
        assert_eq!(
            profile_app_for(&odd, Os::Windows, &Fake).windows.as_deref(),
            Some("Odd.exe")
        );
        let mac = choice(
            "TextEdit",
            "/usr/bin/open",
            &["/System/Applications/TextEdit.app"],
        );
        assert_eq!(
            profile_app_for(&mac, Os::MacOs, &Fake).macos.as_deref(),
            Some("com.apple.TextEdit")
        );
        let desktop = choice(
            "Text Editor",
            "/usr/share/applications/org.gnome.gedit.desktop",
            &[],
        );
        assert_eq!(
            profile_app_for(&desktop, Os::Linux, &Fake).linux.as_deref(),
            Some("gedit")
        );
        let image = choice("App", "/home/me/Apps/Tool.AppImage", &[]);
        assert_eq!(
            profile_app_for(&image, Os::Linux, &Fake).linux.as_deref(),
            Some("Tool.AppImage")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_desktop_entry_names_its_window_class_or_program() {
        let entry = |extra: &str, exec: &str| {
            format!("[Desktop Entry]\nType=Application\nName=X\nExec={exec}\n{extra}")
        };
        assert_eq!(
            desktop_program(&entry("StartupWMClass=Gimp-2.10", "gimp %U")).as_deref(),
            Some("Gimp-2.10")
        );
        assert_eq!(
            desktop_program(&entry("", "/usr/bin/gedit --new-window %U")).as_deref(),
            Some("gedit")
        );
        assert_eq!(
            desktop_program(&entry("", "env GDK_BACKEND=x11 /opt/app/bin/tool")).as_deref(),
            Some("tool")
        );
        assert_eq!(
            desktop_program(&entry(
                "",
                "/usr/bin/flatpak run --branch=stable --arch=x86_64 org.gimp.GIMP %U"
            ))
            .as_deref(),
            Some("org.gimp.GIMP")
        );
    }

    #[test]
    fn this_process_is_freewheels_own_window() {
        let own = front_app(std::process::id(), vec!["freewheel".into()], None).unwrap();
        assert!(own.own);
        let other = front_app(
            1,
            vec![" gedit ".into(), "gedit".into(), "Gedit".into()],
            None,
        )
        .unwrap();
        assert_eq!(other.ids, ["gedit", "Gedit"]);
        assert_eq!(other.name, "gedit");
        assert!(front_app(1, vec![], None).is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wm_class_is_its_instance_and_class() {
        assert_eq!(
            x11::wm_class(b"xmessage\0Xmessage\0"),
            ["xmessage", "Xmessage"]
        );
        assert_eq!(
            x11::process_names(std::process::id()).len(),
            2,
            "this test's own program and short name"
        );
    }
}
