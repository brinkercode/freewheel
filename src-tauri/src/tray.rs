//! The tray icon and its menu, as the design handoff maps it (README, "Tray
//! menu"):
//!
//! 1. `freewheel is On` — check item: the same On/Off as the settings
//!    window's switch (`Settings::set_enabled`); either one moves the other
//! 2. Status line — disabled, except "Needs permission (click to fix)", which
//!    opens the permission screen
//! 3. ——
//! 4. Open settings… · Start at login (check)
//! 5. ——
//! 6. Download logs for support… (saves the bundle, opens Help & backup) ·
//!    Check for updates / Restart to update (drives the settings window's
//!    update flow, src/updater.js)
//! 7. ——
//! 8. Quit freewheel
//!
//! The icon shows On · Off/paused · Not connected · Needs attention
//! (permission missing or battery low). Every `status` event refreshes the
//! icon, the toggle and the status line ([`emit_status`]).

use std::sync::Mutex;

use freewheel_core::config::ConfigStore;
use freewheel_core::permissions::Grant;
use freewheel_core::simulator::Bus;
use freewheel_core::status::{STATUS_EVENT, StatusEvent};
use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_autostart::ManagerExt;

use freewheel_core::battery::{self, Battery, LowBatteryAlert};
use freewheel_core::connection::ConnectionStatus;

use crate::device::{TrayState, Update};
use crate::notify::{self, Notifier};

const TOGGLE: &str = "toggle";
const STATUS: &str = "status";
const OPEN_SETTINGS: &str = "open-settings";
const START_AT_LOGIN: &str = "start-at-login";
const DOWNLOAD_LOGS: &str = "download-logs";
const UPDATE: &str = "update";
const QUIT: &str = "quit";
/// Sent when the status line is clicked: the settings window shows the
/// permissions screen with Done (FIX_PERMISSIONS_EVENT in settingsApi.js).
const FIX_PERMISSIONS_EVENT: &str = "fix-permissions";
/// To the settings window: run the update flow's "check", "download" or
/// "restart" (TRAY_UPDATE_EVENT in settingsApi.js).
pub const TRAY_UPDATE_EVENT: &str = "tray-update";
/// To the settings window: the tray saved the log bundle (`{ path }`) or
/// could not (`{ error }`); it opens Help & backup and shows it
/// (LOGS_SAVED_EVENT in settingsApi.js).
pub const LOGS_SAVED_EVENT: &str = "logs-saved";

const OPEN_SETTINGS_TEXT: &str = "Open settings…";
const START_AT_LOGIN_TEXT: &str = "Start at login";
const DOWNLOAD_LOGS_TEXT: &str = "Download logs for support…";
const QUIT_TEXT: &str = "Quit freewheel";
/// The status line before the device loop has said anything.
const LOOKING: &str = "Looking for your Speed Editor…";
/// Below this battery level the icon needs attention (src/shell.js LOW_BATTERY).
pub const LOW_BATTERY: u8 = 15;

/// Applies a Start at login toggle: `os` enables or disables the OS login
/// item first, then `start_at_login` is saved to the config.
pub fn set_start_at_login(
    os: impl FnOnce(bool) -> Result<(), String>,
    store: &mut ConfigStore,
    on: bool,
) -> Result<(), String> {
    os(on)?;
    let (mut config, _) = store.load_or_default();
    config.settings.start_at_login = on;
    store.save(&config).map_err(|e| e.to_string())
}

/// Whether the OS login item is registered now (false when unknown). In
/// `--simulate` the login item is the simulator's own: the OS is untouched.
pub(crate) fn autostart_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    if let Some(sim) = app.try_state::<crate::simulate::SimHandle>() {
        return sim.start_at_login();
    }
    app.autolaunch().is_enabled().unwrap_or_else(|e| {
        tracing::warn!(error = %e, "cannot read the start-at-login state");
        false
    })
}

pub(crate) fn set_autostart<R: Runtime>(app: &AppHandle<R>, on: bool) -> Result<(), String> {
    if let Some(sim) = app.try_state::<crate::simulate::SimHandle>() {
        sim.set_start_at_login(on);
        crate::simulate::note(
            app,
            format!(
                "would turn start at login {} (the real login item is not touched)",
                if on { "on" } else { "off" }
            ),
        );
        return Ok(());
    }
    let autolaunch = app.autolaunch();
    let result = if on {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    result.map_err(|e| e.to_string())
}

/// The tray icon's four states (screens-and-components 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IconState {
    On,
    /// Off or paused for Resolve.
    Off,
    /// Not connected.
    Absent,
    /// Needs attention: a permission is missing or the battery is low.
    Attention,
}

/// The taskbar the tray icon sits on: its pink glyph is the darker accent
/// on a light taskbar, the brighter one on a dark taskbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taskbar {
    /// Only Windows reads a light taskbar ([`taskbar`]).
    #[cfg_attr(not(windows), allow(dead_code))]
    Light,
    Dark,
}

/// The Windows taskbar's theme now (`SystemUsesLightTheme`, which is the
/// taskbar's, not the apps'); dark when unknown and on other systems.
#[cfg(windows)]
pub fn taskbar() -> Taskbar {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    use windows::core::w;
    let mut light: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: the out pointers point at `light` and `size`, which live
    // across the call; RRF_RT_REG_DWORD writes at most 4 bytes.
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(std::ptr::from_mut(&mut light).cast()),
            Some(&mut size),
        )
    };
    if read.is_ok() && light == 1 {
        Taskbar::Light
    } else {
        Taskbar::Dark
    }
}

#[cfg(not(windows))]
pub fn taskbar() -> Taskbar {
    Taskbar::Dark
}

/// The bundled icon for each state: on Windows (and Linux) the app's pink
/// for the `taskbar` (scripts/tray-icons.mjs makes them from the design
/// handoff's glyphs); on macOS the handoff's template images, which the menu
/// bar tints itself (a template must stay black on transparent).
pub fn icon(state: IconState, taskbar: Taskbar) -> Image<'static> {
    #[cfg(target_os = "macos")]
    let [on, off, absent, attention] = {
        let _ = taskbar;
        [
            tauri::include_image!("icons/tray/macos/tray-onTemplate@2x.png"),
            tauri::include_image!("icons/tray/macos/tray-offTemplate@2x.png"),
            tauri::include_image!("icons/tray/macos/tray-not-connectedTemplate@2x.png"),
            tauri::include_image!("icons/tray/macos/tray-attentionTemplate@2x.png"),
        ]
    };
    #[cfg(not(target_os = "macos"))]
    let [on, off, absent, attention] = match taskbar {
        Taskbar::Dark => [
            tauri::include_image!("icons/tray/windows/dark-taskbar/tray-on-32.png"),
            tauri::include_image!("icons/tray/windows/dark-taskbar/tray-off-32.png"),
            tauri::include_image!("icons/tray/windows/dark-taskbar/tray-not-connected-32.png"),
            tauri::include_image!("icons/tray/windows/dark-taskbar/tray-attention-32.png"),
        ],
        Taskbar::Light => [
            tauri::include_image!("icons/tray/windows/light-taskbar/tray-on-32.png"),
            tauri::include_image!("icons/tray/windows/light-taskbar/tray-off-32.png"),
            tauri::include_image!("icons/tray/windows/light-taskbar/tray-not-connected-32.png"),
            tauri::include_image!("icons/tray/windows/light-taskbar/tray-attention-32.png"),
        ],
    };
    match state {
        IconState::On => on,
        IconState::Off => off,
        IconState::Absent => absent,
        IconState::Attention => attention,
    }
}

fn toggle_text(on: bool) -> &'static str {
    if on {
        "freewheel is On"
    } else {
        "freewheel is Off"
    }
}

/// The device status the tooltip shows while no battery data is known.
fn status_text(state: TrayState) -> &'static str {
    match state {
        TrayState::On => "Connected",
        TrayState::Off => "freewheel is Off",
        TrayState::NotConnected => "Looking for device",
    }
}

/// What the tray shows for a status: the toggle's check, the status line
/// (and whether clicking it fixes a permission) and the icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayStatus {
    pub on: bool,
    pub line: String,
    /// The line is "Needs permission (click to fix)": enabled, it opens the
    /// permission screen.
    pub fix: bool,
    pub icon: IconState,
}

/// The tray's view of a `status` event, first match wins as in the
/// toolbar (src/shell.js deriveStatus): Off → Needs permission → Looking for
/// device → Paused (Resolve) → Connected (USB / Bluetooth · N%).
pub fn tray_status(status: &StatusEvent, low_battery: u8) -> TrayStatus {
    let paused = status.paused;
    // The one On/Off (Settings::set_enabled); stepped aside for Resolve the
    // device loop is Off, yet freewheel is On.
    let on = status.enabled;
    let missing = status.permissions.values().any(|g| *g == Grant::Missing);
    let connected = paused || status.connection == ConnectionStatus::Connected;
    let level = status.battery.map(|b| b.level);
    let low = level.is_some_and(|l| l < low_battery);
    let (line, fix) = if !on {
        ("Off: Resolve can use it".to_string(), false)
    } else if missing {
        ("Needs permission (click to fix)".to_string(), true)
    } else if !connected {
        (LOOKING.to_string(), false)
    } else if paused {
        ("Paused: DaVinci Resolve is open".to_string(), false)
    } else {
        (connected_text(status.bus, level), false)
    };
    let icon = if !on || paused {
        IconState::Off
    } else if missing || (connected && low) {
        IconState::Attention
    } else if !connected {
        IconState::Absent
    } else {
        IconState::On
    };
    TrayStatus {
        on,
        line,
        fix,
        icon,
    }
}

/// "Connected by USB", "Connected by Bluetooth · 64%" (src/shell.js connectedText).
fn connected_text(bus: Option<Bus>, level: Option<u8>) -> String {
    let battery = level.map(|l| format!(" · {l}%")).unwrap_or_default();
    match bus {
        Some(Bus::Usb) => format!("Connected by USB{battery}"),
        Some(Bus::Bluetooth) => format!("Connected by Bluetooth{battery}"),
        None => format!("Connected{battery}"),
    }
}

/// The settings window's update state (src/updater.js), as it reports it
/// through `tray_update_state`; `None` status is idle.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct UpdateReport {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub during: Option<String>,
    #[serde(default, rename = "notConfigured")]
    pub not_configured: bool,
}

/// What clicking the update item asks the settings window to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateAction {
    Check,
    Download,
    Restart,
    /// Disabled: busy, or this build cannot update.
    Nothing,
}

impl UpdateAction {
    /// The TRAY_UPDATE_EVENT payload.
    fn event(self) -> Option<&'static str> {
        match self {
            UpdateAction::Check => Some("check"),
            UpdateAction::Download => Some("download"),
            UpdateAction::Restart => Some("restart"),
            UpdateAction::Nothing => None,
        }
    }
}

/// The text the General tab shows for a build without an updater
/// (src/General.jsx updateText); the tray says the same.
pub const NOT_IN_THIS_BUILD: &str = "This test build can’t update itself";

/// The update item for the update state: its text and what a click does.
/// `available`: this run has an update source (a release build with the
/// updater pubkey, or `--simulate`'s simulated source).
pub fn update_item(available: bool, report: &UpdateReport) -> (String, UpdateAction) {
    use UpdateAction::*;
    if !available || report.not_configured {
        return (NOT_IN_THIS_BUILD.into(), Nothing);
    }
    let v = report.version.as_deref().unwrap_or("");
    match report.status.as_deref() {
        Some("checking") => ("Checking for updates…".into(), Nothing),
        Some("current") => ("You’re up to date · Check again".into(), Check),
        Some("available") => (format!("Download freewheel {v}"), Download),
        Some("downloading") => ("Downloading update…".into(), Nothing),
        Some("ready") => ("Restart to update".into(), Restart),
        Some("restarting") => ("Restarting…".into(), Nothing),
        Some("error") => match report.during.as_deref() {
            Some("download") => (
                format!("freewheel {v} didn’t download · Try again"),
                Download,
            ),
            Some("install") => (format!("freewheel {v} didn’t install · Try again"), Restart),
            _ => ("Couldn’t check for updates · Try again".into(), Check),
        },
        _ => ("Check for updates".into(), Check),
    }
}

/// One tray menu entry, as plain data; `menu` turns it into a native item.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Check {
        id: &'static str,
        text: &'static str,
        checked: bool,
    },
    Item {
        id: &'static str,
        text: String,
        enabled: bool,
    },
    Separator,
}

fn item(id: &'static str, text: impl Into<String>, enabled: bool) -> Entry {
    Entry::Item {
        id,
        text: text.into(),
        enabled,
    }
}

/// The tray menu as plain data, before the device loop has said anything.
fn menu_model(start_at_login: bool, updates_available: bool) -> Vec<Entry> {
    let (update_text, update_action) = update_item(updates_available, &UpdateReport::default());
    vec![
        Entry::Check {
            id: TOGGLE,
            text: toggle_text(true),
            checked: true,
        },
        item(STATUS, LOOKING, false),
        Entry::Separator,
        item(OPEN_SETTINGS, OPEN_SETTINGS_TEXT, true),
        Entry::Check {
            id: START_AT_LOGIN,
            text: START_AT_LOGIN_TEXT,
            checked: start_at_login,
        },
        Entry::Separator,
        item(DOWNLOAD_LOGS, DOWNLOAD_LOGS_TEXT, true),
        item(UPDATE, update_text, update_action != UpdateAction::Nothing),
        Entry::Separator,
        item(QUIT, QUIT_TEXT, true),
    ]
}

/// The native menu and the items that change.
struct MenuParts<R: Runtime> {
    menu: Menu<R>,
    toggle: CheckMenuItem<R>,
    login: CheckMenuItem<R>,
    status: MenuItem<R>,
    update: MenuItem<R>,
}

/// Builds the native tray menu from `menu_model` (main thread only).
fn menu<R: Runtime>(
    app: &AppHandle<R>,
    start_at_login: bool,
    updates_available: bool,
) -> tauri::Result<MenuParts<R>> {
    let mut items: Vec<Box<dyn IsMenuItem<R>>> = Vec::new();
    let (mut toggle, mut login, mut status, mut update) = (None, None, None, None);
    for entry in menu_model(start_at_login, updates_available) {
        match entry {
            Entry::Check { id, text, checked } => {
                let item = CheckMenuItem::with_id(app, id, text, true, checked, None::<&str>)?;
                if id == TOGGLE {
                    toggle = Some(item.clone());
                } else {
                    login = Some(item.clone());
                }
                items.push(Box::new(item));
            }
            Entry::Item { id, text, enabled } => {
                let item = MenuItem::with_id(app, id, text, enabled, None::<&str>)?;
                match id {
                    STATUS => status = Some(item.clone()),
                    UPDATE => update = Some(item.clone()),
                    _ => {}
                }
                items.push(Box::new(item));
            }
            Entry::Separator => items.push(Box::new(PredefinedMenuItem::separator(app)?)),
        }
    }
    let refs: Vec<&dyn IsMenuItem<R>> = items.iter().map(|item| item.as_ref()).collect();
    let menu = Menu::with_items(app, &refs)?;
    let (Some(toggle), Some(login), Some(status), Some(update)) = (toggle, login, status, update)
    else {
        unreachable!("the menu model has every changing item");
    };
    Ok(MenuParts {
        menu,
        toggle,
        login,
        status,
        update,
    })
}

/// Shows and focuses the settings window (Open settings, second launch).
/// When it was closed it is built again (app_windows.rs); returns whether it
/// was there.
pub fn show_settings<R: Runtime>(app: &AppHandle<R>) -> bool {
    crate::app_windows::show_settings(app)
}

/// What changes in the menu, kept to answer clicks and the Simulator.
#[derive(Debug, Clone)]
struct Live {
    status: TrayStatus,
    update_action: UpdateAction,
}

/// The tray menu's live parts, managed by Tauri: what a menu click (or, in
/// `--simulate`, the Simulator window's Tray menu) acts on.
pub struct TrayMenu<R: Runtime> {
    toggle: CheckMenuItem<R>,
    login: CheckMenuItem<R>,
    status: MenuItem<R>,
    update: MenuItem<R>,
    updates_available: bool,
    low_battery: u8,
    live: Mutex<Live>,
    notifier: Mutex<Notifier>,
}

/// The tray menu as the Simulator window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrayMenuState {
    /// The On/Off item's check.
    pub on: bool,
    pub start_at_login: bool,
    /// The status line's text.
    pub status_line: String,
    /// Whether the status line is clickable (a missing permission).
    pub status_fix: bool,
    /// The update item's text and whether it is enabled.
    pub update: String,
    pub update_enabled: bool,
    pub icon: IconState,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl<R: Runtime> TrayMenu<R> {
    pub fn state(&self) -> TrayMenuState {
        // The menu first, then `live`, never both: off the main thread (the
        // async sim_tray) a menu getter waits for the main thread, and the
        // main thread may be waiting for `live` (show_update from
        // tray_update_state, show_status): holding it across the getter
        // deadlocked the app.
        let on = self.toggle.is_checked().unwrap_or(true);
        let start_at_login = self.login.is_checked().unwrap_or(false);
        let update = self.update.text().unwrap_or_default();
        let live = lock(&self.live).clone();
        TrayMenuState {
            on,
            start_at_login,
            status_line: live.status.line,
            status_fix: live.status.fix,
            update,
            update_enabled: live.update_action != UpdateAction::Nothing,
            icon: live.status.icon,
        }
    }

    /// Shows `status` on the icon, the toggle and the status line; notifies
    /// when freewheel stepped aside for Resolve.
    fn show_status(&self, app: &AppHandle<R>, status: &StatusEvent) {
        let view = tray_status(status, self.low_battery);
        let _ = self.toggle.set_checked(view.on);
        let _ = self.toggle.set_text(toggle_text(view.on));
        let _ = self.status.set_text(&view.line);
        let _ = self.status.set_enabled(view.fix);
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_icon(Some(icon(view.icon, taskbar())));
            let _ = tray.set_icon_as_template(cfg!(target_os = "macos"));
        }
        lock(&self.live).status = view;
        self.changed(app);
        let stepped_aside = lock(&self.notifier).paused(status.paused);
        if let Some(notice) = stepped_aside {
            notify::show(app, &notice);
        }
    }

    /// The settings window's update state: the item's text and action; the
    /// update-ready notification.
    fn show_update(&self, app: &AppHandle<R>, report: &UpdateReport) {
        let (text, action) = update_item(self.updates_available, report);
        let _ = self.update.set_text(&text);
        let _ = self.update.set_enabled(action != UpdateAction::Nothing);
        lock(&self.live).update_action = action;
        self.changed(app);
        let in_front = app
            .get_webview_window("main")
            .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false));
        let decision = lock(&self.notifier).update(
            report.status.as_deref(),
            report.version.as_deref(),
            in_front,
        );
        if let Some(decision) = decision {
            notify::apply(app, decision);
        }
    }

    /// A click on the item `id`, after the OS flipped a check item's check.
    pub fn clicked(&self, app: &AppHandle<R>, id: &str) {
        match id {
            TOGGLE => {
                // The OS flipped the check: switch to it, through the same
                // On/Off as the settings window's switch, then show the
                // result everywhere (the window hears the `status` event).
                let on = self.toggle.is_checked().unwrap_or(true);
                let settings = app.state::<crate::settings::Settings>();
                let status = settings.set_enabled(on, "tray").unwrap_or_else(|e| {
                    tracing::error!(error = %e, on, "the tray cannot switch freewheel");
                    settings.status()
                });
                emit_status(app, &status);
            }
            STATUS => {
                show_settings(app);
                let fix = lock(&self.live).status.fix;
                if fix {
                    crate::app_windows::emit_to_settings(app, FIX_PERMISSIONS_EVENT, ());
                }
            }
            OPEN_SETTINGS => {
                show_settings(app);
            }
            START_AT_LOGIN => {
                let login = &self.login;
                let on = login.is_checked().unwrap_or(false);
                // Through the running config when the window has one, so its
                // next save keeps this choice.
                let result = match app.try_state::<crate::settings::Settings>() {
                    Some(settings) => settings.set_start_at_login(|on| set_autostart(app, on), on),
                    None => match ConfigStore::from_project_dirs() {
                        Some(mut store) => {
                            set_start_at_login(|on| set_autostart(app, on), &mut store, on)
                        }
                        None => set_autostart(app, on),
                    },
                };
                if let Err(e) = result {
                    tracing::error!(error = %e, on, "cannot change start at login");
                }
                // Show what the OS really has, whether or not the change took.
                let _ = login.set_checked(autostart_enabled(app));
            }
            DOWNLOAD_LOGS => {
                let app = app.clone();
                // Off the main thread: it reads up to 10 MB of logs.
                std::thread::spawn(move || download_logs(&app));
            }
            UPDATE => {
                let action = lock(&self.live).update_action;
                match action.event() {
                    // The flow lives in the settings window's script. A
                    // closed one opens and, once its page listens, runs the
                    // action: a check shows Checking… then the answer, as in
                    // an open window (its own startup check stays silent).
                    Some(what) => {
                        let closed = app.get_webview_window("main").is_none();
                        tracing::info!(what, closed, "tray: update");
                        if closed {
                            show_settings(app);
                        }
                        crate::app_windows::emit_to_settings(app, TRAY_UPDATE_EVENT, what);
                    }
                    None => tracing::info!("tray: the update item is disabled now"),
                }
            }
            QUIT => app.exit(0),
            _ => {}
        }
    }

    /// `--simulate`: the Simulator window clicks tray item `id`, as the OS
    /// menu does (a check item flips its check first; a disabled item does
    /// nothing, as the OS menu does not deliver its clicks).
    pub fn click(&self, app: &AppHandle<R>, id: &str) -> Result<TrayMenuState, String> {
        let text = match id {
            TOGGLE | START_AT_LOGIN => {
                let item = if id == TOGGLE {
                    &self.toggle
                } else {
                    &self.login
                };
                let checked = item.is_checked().map_err(|e| e.to_string())?;
                item.set_checked(!checked).map_err(|e| e.to_string())?;
                item.text().unwrap_or_default()
            }
            OPEN_SETTINGS => OPEN_SETTINGS_TEXT.into(),
            DOWNLOAD_LOGS => DOWNLOAD_LOGS_TEXT.into(),
            QUIT => QUIT_TEXT.into(),
            STATUS | UPDATE => {
                let state = self.state();
                let (text, enabled) = if id == STATUS {
                    (state.status_line, state.status_fix)
                } else {
                    (state.update, state.update_enabled)
                };
                if !enabled {
                    crate::simulate::note(app, format!("Tray: {text} (disabled, nothing happens)"));
                    return Ok(self.state());
                }
                text
            }
            other => return Err(format!("the tray menu has no item {other}")),
        };
        crate::simulate::note(app, format!("Tray: {text}"));
        self.clicked(app, id);
        Ok(self.state())
    }
}

/// `--simulate`: tells the Simulator window the tray menu changed (its
/// Tray menu section redraws; SIM_TRAY_EVENT in src/simulateEvents.js).
pub const SIM_TRAY_EVENT: &str = "sim-tray";

impl<R: Runtime> TrayMenu<R> {
    fn changed(&self, app: &AppHandle<R>) {
        if crate::simulate::simulating(app) {
            let _ = app.emit_to(
                crate::simulate::SIMULATOR_WINDOW,
                SIM_TRAY_EVENT,
                self.state(),
            );
        }
    }
}

/// Emits the `status` event to the windows and shows it in the tray.
pub fn emit_status<R: Runtime>(app: &AppHandle<R>, status: &StatusEvent) {
    let _ = app.emit(STATUS_EVENT, status);
    if let Some(menu) = app.try_state::<TrayMenu<R>>() {
        menu.show_status(app, status);
    }
}

/// Builds the tray. The toggle switches freewheel through
/// [`crate::settings::Settings::set_enabled`] (managed before the tray), as
/// the settings window's switch does; Quit exits the app, whose exit hook
/// releases every held key. The menu's parts are managed as [`TrayMenu`].
/// `updates_available`: this run has an update source.
pub fn build<R: Runtime>(
    app: &AppHandle<R>,
    updates_available: bool,
    low_battery: u8,
) -> tauri::Result<TrayIcon<R>> {
    let menu = manage_menu(app, updates_available, low_battery)?;
    TrayIconBuilder::with_id("main")
        .icon(icon(IconState::Absent, taskbar()))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(status_text(TrayState::NotConnected))
        .menu(&menu)
        .on_menu_event(move |app, event| {
            app.state::<TrayMenu<R>>().clicked(app, event.id().as_ref());
        })
        .build(app)
}

/// Builds the tray menu and manages its live parts as [`TrayMenu`].
fn manage_menu<R: Runtime>(
    app: &AppHandle<R>,
    updates_available: bool,
    low_battery: u8,
) -> tauri::Result<Menu<R>> {
    // The checkbox shows the OS login item, not the config, at startup.
    let parts = menu(app, autostart_enabled(app), updates_available)?;
    let (_, update_action) = update_item(updates_available, &UpdateReport::default());
    app.manage(TrayMenu {
        toggle: parts.toggle,
        login: parts.login,
        status: parts.status,
        update: parts.update,
        updates_available,
        low_battery,
        live: Mutex::new(Live {
            status: TrayStatus {
                on: true,
                line: LOOKING.into(),
                fix: false,
                icon: IconState::Absent,
            },
            update_action,
        }),
        notifier: Mutex::new(Notifier::default()),
    });
    Ok(parts.menu)
}

/// `--simulate` only: the Simulator window's Tray menu clicks a tray item
/// (`State` just reads the menu). Async: the menu lives on the main thread.
#[tauri::command]
pub async fn sim_tray(app: AppHandle, item: String) -> Result<TrayMenuState, String> {
    let menu = app.state::<TrayMenu<tauri::Wry>>();
    if item == "State" {
        return Ok(menu.state());
    }
    menu.click(&app, &item)
}

/// The settings window reports its update state (src/updater.js states, or
/// null when idle): the tray's update item follows it.
#[tauri::command]
pub fn tray_update_state(app: AppHandle, state: Option<UpdateReport>) {
    let state = state.unwrap_or_default();
    if let Some(menu) = app.try_state::<TrayMenu<tauri::Wry>>() {
        menu.show_update(&app, &state);
    }
    crate::app_windows::update_state(&app, state.status.as_deref());
}

/// What LOGS_SAVED_EVENT carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogsSaved {
    Path(String),
    Error(String),
}

/// Saves the log bundle to Downloads, then opens Help & backup, which says
/// where (or why it failed) and offers Show file.
fn download_logs<R: Runtime>(app: &AppHandle<R>) {
    let saved = match crate::settings::save_log_bundle(app) {
        Ok(path) => {
            crate::simulate::note(app, format!("Saved the logs to {path}"));
            LogsSaved::Path(path)
        }
        Err(e) => {
            crate::simulate::note(app, format!("The logs were not saved: {e}"));
            LogsSaved::Error(e)
        }
    };
    show_settings(app);
    crate::app_windows::emit_to_settings(app, LOGS_SAVED_EVENT, &saved);
}

/// What the tray shows from the device loop: the battery tooltip and the
/// once-only low-battery notification. (The icon, the toggle and the status
/// line follow the `status` event: [`emit_status`].)
pub struct TrayView {
    state: TrayState,
    battery: Option<Battery>,
    alert: LowBatteryAlert,
    threshold: u8,
}

impl TrayView {
    pub fn new(threshold: u8) -> Self {
        Self {
            state: TrayState::NotConnected,
            battery: None,
            alert: LowBatteryAlert::default(),
            threshold,
        }
    }

    /// The tooltip for what is known now.
    fn tooltip(&self) -> String {
        battery::tooltip(status_text(self.state), self.battery)
    }

    /// Takes an update from the device loop and shows it.
    pub fn apply<R: Runtime>(&mut self, tray: &TrayIcon<R>, update: Update) {
        match update {
            Update::State(status) => {
                self.state = status.into();
                // A new connection reports its battery afresh.
                self.battery = None;
            }
            Update::Battery(b) => {
                self.battery = Some(b);
                if self.alert.update(b, self.threshold) {
                    notify::show(tray.app_handle(), &notify::low_battery(b.level));
                }
            }
            // Only the settings window shows these.
            Update::Layer(_)
            | Update::KeyPressed(_)
            | Update::Override(_)
            | Update::Diagnostics(_)
            | Update::Leds(_) => return,
        }
        let _ = tray.set_tooltip(Some(self.tooltip()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Tray item ids, as the Simulator window names them.
    const SIM_ITEMS: [&str; 7] = [
        TOGGLE,
        STATUS,
        OPEN_SETTINGS,
        START_AT_LOGIN,
        DOWNLOAD_LOGS,
        UPDATE,
        QUIT,
    ];

    #[test]
    fn the_simulator_window_clicks_every_tray_item() {
        let js = include_str!("../../src/Simulator.jsx");
        for id in SIM_ITEMS {
            assert!(js.contains(&format!("\"{id}\"")), "{id}");
        }
    }

    #[test]
    fn the_simulator_window_hears_the_tray_changes() {
        let js = include_str!("../../src/simulateEvents.js");
        assert!(js.contains(&format!("SIM_TRAY_EVENT = \"{SIM_TRAY_EVENT}\"")));
        assert!(include_str!("../../src/Simulator.jsx").contains("SIM_TRAY_EVENT"));
    }

    /// No lock on the menu's live state is held across a menu call: off the
    /// main thread a menu call waits for the main thread, which may be
    /// waiting for that lock (the tray's Restart to update froze the app).
    #[test]
    fn no_menu_call_runs_under_the_live_lock() {
        let src = include_str!("tray.rs");
        let src = &src[..src.find("#[cfg(test)]").unwrap()];
        let menu_call = [
            ".is_checked()",
            ".set_checked(",
            ".text()",
            ".set_text(",
            ".set_enabled(",
            "set_icon",
        ];
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            // A guard kept in a binding lives to the end of its block.
            if t.starts_with("let") && t.contains("= lock(&self.live)") && !t.contains(".clone()") {
                let rest: Vec<&str> = src
                    .lines()
                    .skip(i + 1)
                    .take_while(|l| !l.trim().starts_with('}'))
                    .collect();
                for l in rest {
                    assert!(
                        !menu_call.iter().any(|c| l.contains(c)),
                        "line {}: a menu call under the live lock: {l}",
                        i + 1
                    );
                }
            }
            if t.contains("lock(&self.live)") {
                assert!(
                    !menu_call.iter().any(|c| t.contains(c)),
                    "line {}: {t}",
                    i + 1
                );
            }
        }
    }

    #[test]
    fn the_settings_window_hears_the_tray_events() {
        let js = include_str!("../../src/settingsApi.js");
        for event in [TRAY_UPDATE_EVENT, LOGS_SAVED_EVENT, FIX_PERMISSIONS_EVENT] {
            assert!(js.contains(&format!("\"{event}\"")), "{event}");
        }
    }

    const STATES: [IconState; 4] = [
        IconState::On,
        IconState::Off,
        IconState::Absent,
        IconState::Attention,
    ];

    #[test]
    fn each_state_has_its_own_bundled_icon() {
        for taskbar in [Taskbar::Light, Taskbar::Dark] {
            let all = STATES.map(|s| icon(s, taskbar));
            for (i, a) in all.iter().enumerate() {
                for b in &all[i + 1..] {
                    assert_ne!(a.rgba(), b.rgba());
                }
            }
        }
    }

    /// The most used colour among the icon's visible pixels, and how many
    /// pixels are amber (the attention badge).
    #[cfg(not(target_os = "macos"))]
    fn ink(image: &Image<'_>) -> ([u8; 3], usize) {
        let mut counts = BTreeMap::<[u8; 3], usize>::new();
        let mut amber = 0;
        for px in image.rgba().chunks(4) {
            if px[3] < 24 {
                continue;
            }
            if px[..3] == [240, 160, 32] {
                amber += 1;
            } else {
                *counts.entry([px[0], px[1], px[2]]).or_default() += 1;
            }
        }
        let top = counts.into_iter().max_by_key(|(_, n)| *n).unwrap().0;
        (top, amber)
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_windows_tray_icons_are_the_app_pink_for_the_taskbar() {
        // scripts/tray-icons.mjs: the light accent on a light taskbar, the
        // brighter pink on a dark one.
        for (taskbar, pink) in [
            (Taskbar::Light, [0xc4, 0x46, 0x7f]),
            (Taskbar::Dark, [0xf0, 0x7f, 0xae]),
        ] {
            for state in STATES {
                let (top, amber) = ink(&icon(state, taskbar));
                assert_eq!(top, pink, "{taskbar:?} {state:?}");
                // Only Needs attention keeps the handoff's amber badge.
                assert_eq!(
                    amber > 0,
                    state == IconState::Attention,
                    "{taskbar:?} {state:?}"
                );
            }
            // Off is the dimmed glyph: less ink than On.
            let alpha = |s| -> u32 {
                icon(s, taskbar)
                    .rgba()
                    .chunks(4)
                    .map(|p| u32::from(p[3]))
                    .sum()
            };
            assert!(
                alpha(IconState::Off) < alpha(IconState::On) * 3 / 4,
                "{taskbar:?}"
            );
        }
        // Off Windows there is no taskbar theme to read: dark.
        #[cfg(not(windows))]
        assert_eq!(taskbar(), Taskbar::Dark);
        // On Windows it is read from the registry (either is fine on a runner).
        #[cfg(windows)]
        let _ = taskbar();
    }

    #[test]
    fn the_tray_check_is_the_one_on_off_not_the_connection() {
        // Just switched Off: the device loop has not said Off yet.
        let mut s = status(ConnectionStatus::Absent);
        s.enabled = false;
        let v = tray_status(&s, LOW_BATTERY);
        assert!(!v.on);
        assert_eq!(v.icon, IconState::Off);
        assert_eq!(v.line, "Off: Resolve can use it");
        // Just switched On: still Off in the device loop, the check is on.
        let mut s = status(ConnectionStatus::Off);
        s.enabled = true;
        let v = tray_status(&s, LOW_BATTERY);
        assert!(v.on);
        assert_eq!(v.line, LOOKING);
        // Stepped aside for Resolve: On, paused, never Off.
        s.paused = true;
        let v = tray_status(&s, LOW_BATTERY);
        assert!(v.on);
        assert_eq!(v.icon, IconState::Off);
        assert_eq!(v.line, "Paused: DaVinci Resolve is open");
    }

    /// A tray built on a mock app with the real Settings behind it. (Not
    /// on macOS: AppKit menus only exist on the main thread, which a test
    /// does not run on; the Windows and Linux menus are the same code.)
    #[cfg(not(target_os = "macos"))]
    fn mock_tray() -> (
        tauri::App<tauri::test::MockRuntime>,
        tempfile::TempDir,
        std::sync::mpsc::Receiver<crate::device::Command>,
    ) {
        let app = tauri::test::mock_app();
        let dir = tempfile::tempdir().unwrap();
        let (commands, rx) = std::sync::mpsc::channel();
        let (manual, _) = std::sync::mpsc::channel();
        let mut store = ConfigStore::new(dir.path());
        let config = store.load_or_default().0;
        app.manage(crate::settings::Settings::new(
            store, config, commands, manual,
        ));
        // The simulator's login item: the OS one is not touched.
        app.manage(crate::simulate::SimHandle::new());
        // The menu without the OS tray icon (no system tray in a test).
        manage_menu(app.handle(), false, LOW_BATTERY).unwrap();
        (app, dir, rx)
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_tray_and_the_window_switch_the_same_on_off_both_ways() {
        use crate::device::Command;
        let (app, _dir, commands) = mock_tray();
        let app = app.handle();
        let menu = app.state::<TrayMenu<tauri::test::MockRuntime>>();
        let settings = app.state::<crate::settings::Settings>();
        assert!(menu.state().on);
        // The tray's item, as the OS delivers it: the check flips, then the click.
        menu.toggle.set_checked(false).unwrap();
        menu.clicked(app, TOGGLE);
        assert!(matches!(
            commands.try_recv(),
            Ok(Command::SetEnabled(false))
        ));
        assert!(!settings.status().enabled, "the window reads Off");
        assert!(!menu.state().on);
        assert_eq!(menu.toggle.text().unwrap(), "freewheel is Off");
        // The window's switch (the set_enabled command's path): the check follows.
        let status = settings.set_enabled(true, "settings window").unwrap();
        emit_status(app, &status);
        assert!(matches!(commands.try_recv(), Ok(Command::SetEnabled(true))));
        assert!(menu.state().on);
        assert!(menu.toggle.is_checked().unwrap());
        assert_eq!(menu.toggle.text().unwrap(), "freewheel is On");
        // (Stepped aside for Resolve keeps the check on:
        // the_tray_check_is_the_one_on_off_not_the_connection.)
        // A device loop that is gone: the check goes back to what is true.
        drop(commands);
        menu.toggle.set_checked(false).unwrap();
        menu.clicked(app, TOGGLE);
        assert!(menu.toggle.is_checked().unwrap());
        assert!(settings.status().enabled);
    }

    fn texts(model: &[Entry]) -> Vec<String> {
        model
            .iter()
            .map(|e| match e {
                Entry::Check { text, checked, .. } => {
                    format!("[{}] {text}", if *checked { "x" } else { " " })
                }
                Entry::Item { text, enabled, .. } => {
                    format!("{text}{}", if *enabled { "" } else { " (disabled)" })
                }
                Entry::Separator => "——".into(),
            })
            .collect()
    }

    #[test]
    fn the_menu_is_the_designs_mapping() {
        assert_eq!(
            texts(&menu_model(false, true)),
            [
                "[x] freewheel is On",
                "Looking for your Speed Editor… (disabled)",
                "——",
                "Open settings…",
                "[ ] Start at login",
                "——",
                "Download logs for support…",
                "Check for updates",
                "——",
                "Quit freewheel",
            ]
        );
    }

    #[test]
    fn a_build_without_an_updater_says_so_in_the_menu() {
        let model = texts(&menu_model(true, false));
        assert!(model.contains(&"[x] Start at login".to_string()));
        assert!(model.contains(&format!("{NOT_IN_THIS_BUILD} (disabled)")));
        // Even a window that thinks it can check gets the same answer.
        let report = UpdateReport {
            status: Some("current".into()),
            ..Default::default()
        };
        assert_eq!(
            update_item(false, &report),
            (NOT_IN_THIS_BUILD.into(), UpdateAction::Nothing)
        );
        let not_configured = UpdateReport {
            status: Some("error".into()),
            during: Some("check".into()),
            not_configured: true,
            ..Default::default()
        };
        assert_eq!(update_item(true, &not_configured).1, UpdateAction::Nothing);
    }

    fn report(status: &str, during: Option<&str>) -> UpdateReport {
        UpdateReport {
            status: Some(status.into()),
            version: Some("0.2.0".into()),
            during: during.map(Into::into),
            not_configured: false,
        }
    }

    #[test]
    fn the_update_item_follows_the_update_flow() {
        use UpdateAction::*;
        let cases = [
            (UpdateReport::default(), "Check for updates", Check),
            (report("checking", None), "Checking for updates…", Nothing),
            (
                report("current", None),
                "You’re up to date · Check again",
                Check,
            ),
            (
                report("available", None),
                "Download freewheel 0.2.0",
                Download,
            ),
            (report("downloading", None), "Downloading update…", Nothing),
            (report("ready", None), "Restart to update", Restart),
            (report("restarting", None), "Restarting…", Nothing),
            (
                report("error", Some("check")),
                "Couldn’t check for updates · Try again",
                Check,
            ),
            (
                report("error", Some("download")),
                "freewheel 0.2.0 didn’t download · Try again",
                Download,
            ),
            (
                report("error", Some("install")),
                "freewheel 0.2.0 didn’t install · Try again",
                Restart,
            ),
        ];
        for (r, text, action) in cases {
            assert_eq!(update_item(true, &r), (text.to_string(), action), "{r:?}");
        }
        assert_eq!(Check.event(), Some("check"));
        assert_eq!(Download.event(), Some("download"));
        assert_eq!(Restart.event(), Some("restart"));
        assert_eq!(Nothing.event(), None);
    }

    #[test]
    fn the_window_reports_updater_js_states() {
        let r: UpdateReport = serde_json::from_str(
            r#"{"status":"error","during":"check","message":"plugin updater not found","notConfigured":true}"#,
        )
        .unwrap();
        assert!(r.not_configured);
        let idle: UpdateReport = serde_json::from_str("{}").unwrap();
        assert_eq!(idle, UpdateReport::default());
    }

    /// A status from the device loop; Off here is switched Off.
    fn status(connection: ConnectionStatus) -> StatusEvent {
        let mut s = StatusEvent::new("base");
        s.set_connection(connection);
        s.enabled = connection != ConnectionStatus::Off;
        s
    }

    fn view(s: &StatusEvent) -> (bool, String, bool, IconState) {
        let v = tray_status(s, LOW_BATTERY);
        (v.on, v.line, v.fix, v.icon)
    }

    #[test]
    fn the_status_line_and_icon_follow_the_status() {
        use IconState::*;
        assert_eq!(
            view(&status(ConnectionStatus::Absent)),
            (true, LOOKING.into(), false, Absent)
        );
        assert_eq!(
            view(&status(ConnectionStatus::Off)),
            (false, "Off: Resolve can use it".into(), false, Off)
        );
        let mut usb = status(ConnectionStatus::Connected);
        usb.bus = Some(Bus::Usb);
        assert_eq!(view(&usb), (true, "Connected by USB".into(), false, On));
        let mut bt = status(ConnectionStatus::Connected);
        bt.bus = Some(Bus::Bluetooth);
        bt.battery = Some(Battery {
            level: 64,
            charging: false,
        });
        assert_eq!(
            view(&bt),
            (true, "Connected by Bluetooth · 64%".into(), false, On)
        );
        bt.battery = Some(Battery {
            level: 12,
            charging: false,
        });
        assert_eq!(
            view(&bt),
            (
                true,
                "Connected by Bluetooth · 12%".into(),
                false,
                Attention
            )
        );
        // Stepped aside: the device loop is Off, freewheel is still On.
        let mut paused = status(ConnectionStatus::Off);
        paused.enabled = true;
        paused.paused = true;
        assert_eq!(
            view(&paused),
            (true, "Paused: DaVinci Resolve is open".into(), false, Off)
        );
    }

    #[test]
    fn a_missing_permission_makes_the_line_a_fix_and_the_icon_attention() {
        let mut s = status(ConnectionStatus::Connected);
        s.permissions = BTreeMap::from([
            ("Accessibility".to_string(), Grant::Missing),
            ("InputMonitoring".to_string(), Grant::Granted),
        ]);
        assert_eq!(
            view(&s),
            (
                true,
                "Needs permission (click to fix)".into(),
                true,
                IconState::Attention
            )
        );
        // Off wins over the permission, as in the toolbar.
        s.set_connection(ConnectionStatus::Off);
        s.enabled = false;
        assert_eq!(view(&s).1, "Off: Resolve can use it");
        s.permissions.insert("Accessibility".into(), Grant::Granted);
        s.set_connection(ConnectionStatus::Connected);
        s.enabled = true;
        assert!(!view(&s).2);
    }

    #[test]
    fn the_status_line_reads_like_the_toolbar() {
        let shell = include_str!("../../src/shell.js");
        for text in [
            "Off: Resolve can use it",
            "Needs permission (click to fix)",
            LOOKING,
            "Paused: DaVinci Resolve is open",
            "Connected by ${bus}${battery}",
        ] {
            assert!(shell.contains(text), "{text}");
        }
        assert!(shell.contains(&format!("LOW_BATTERY = {LOW_BATTERY};")));
        let general = include_str!("../../src/General.jsx");
        assert!(general.contains(&format!("{NOT_IN_THIS_BUILD}.")));
    }

    #[test]
    fn tooltip_shows_the_battery_or_else_the_device_status() {
        let mut view = TrayView::new(15);
        assert_eq!(view.tooltip(), "Looking for device");
        view.state = TrayState::On;
        assert_eq!(view.tooltip(), "Connected");
        view.battery = Some(Battery {
            level: 64,
            charging: true,
        });
        assert_eq!(view.tooltip(), "Speed Editor 64% (charging)");
        view.state = TrayState::Off;
        view.battery = None;
        assert_eq!(view.tooltip(), "freewheel is Off");
    }

    #[test]
    fn start_at_login_is_unchecked_on_a_fresh_install() {
        let dir = tempfile::tempdir().unwrap();
        let config = ConfigStore::new(dir.path()).load().unwrap();
        assert!(!config.settings.start_at_login);
        let model = texts(&menu_model(config.settings.start_at_login, true));
        assert!(model.contains(&"[ ] Start at login".to_string()));
    }

    #[test]
    fn toggling_start_at_login_sets_the_os_item_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = ConfigStore::new(dir.path());
        let mut calls = Vec::new();
        set_start_at_login(
            |on| {
                calls.push(on);
                Ok(())
            },
            &mut store,
            true,
        )
        .unwrap();
        assert!(store.load().unwrap().settings.start_at_login);
        set_start_at_login(
            |on| {
                calls.push(on);
                Ok(())
            },
            &mut store,
            false,
        )
        .unwrap();
        assert!(!store.load().unwrap().settings.start_at_login);
        assert_eq!(calls, [true, false]);
    }

    #[test]
    fn a_failed_os_change_leaves_the_config_alone() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = ConfigStore::new(dir.path());
        let err = set_start_at_login(|_| Err("denied".into()), &mut store, true).unwrap_err();
        assert_eq!(err, "denied");
        assert!(!store.path().exists());
    }

    #[test]
    fn the_logs_event_carries_the_path_or_the_error() {
        assert_eq!(
            serde_json::to_string(&LogsSaved::Path("C:/d/l.txt".into())).unwrap(),
            r#"{"path":"C:/d/l.txt"}"#
        );
        assert_eq!(
            serde_json::to_string(&LogsSaved::Error("full".into())).unwrap(),
            r#"{"error":"full"}"#
        );
    }

    #[test]
    fn toggle_text_names_the_state() {
        assert_eq!(toggle_text(true), "freewheel is On");
        assert_eq!(toggle_text(false), "freewheel is Off");
    }
}
