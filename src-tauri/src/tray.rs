//! The tray icon and its menu: the On/Off check item, Open settings,
//! Start at login and Quit.

use std::sync::mpsc::Sender;

use freewheel_core::config::ConfigStore;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_autostart::ManagerExt;

use freewheel_core::battery::{self, Battery, LowBatteryAlert};
use tauri_plugin_notification::NotificationExt;

use crate::device::{Command, TrayState, Update};

const TOGGLE: &str = "toggle";
const OPEN_SETTINGS: &str = "open-settings";
const START_AT_LOGIN: &str = "start-at-login";
const QUIT: &str = "quit";

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

/// Whether the OS login item is registered now (false when unknown).
fn autostart_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.autolaunch().is_enabled().unwrap_or_else(|e| {
        tracing::warn!(error = %e, "cannot read the start-at-login state");
        false
    })
}

fn set_autostart<R: Runtime>(app: &AppHandle<R>, on: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    let result = if on {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    result.map_err(|e| e.to_string())
}

/// The bundled icon for each state (macOS template images, else the
/// Windows dark-taskbar set).
pub fn icon(state: TrayState) -> Image<'static> {
    #[cfg(target_os = "macos")]
    let [on, off, absent] = [
        tauri::include_image!("icons/tray/macos/tray-onTemplate@2x.png"),
        tauri::include_image!("icons/tray/macos/tray-offTemplate@2x.png"),
        tauri::include_image!("icons/tray/macos/tray-not-connectedTemplate@2x.png"),
    ];
    #[cfg(not(target_os = "macos"))]
    let [on, off, absent] = [
        tauri::include_image!("icons/tray/windows/dark-taskbar/tray-on-32.png"),
        tauri::include_image!("icons/tray/windows/dark-taskbar/tray-off-32.png"),
        tauri::include_image!("icons/tray/windows/dark-taskbar/tray-not-connected-32.png"),
    ];
    match state {
        TrayState::On => on,
        TrayState::Off => off,
        TrayState::NotConnected => absent,
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

/// One tray menu entry, as plain data; `menu` turns it into a native item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Check {
        id: &'static str,
        text: &'static str,
        checked: bool,
    },
    Item {
        id: &'static str,
        text: &'static str,
    },
    Separator,
}

/// The tray menu as plain data: the On/Off toggle, Open settings, Start at
/// login (checked as `start_at_login` says) and Quit.
fn menu_model(start_at_login: bool) -> [Entry; 6] {
    [
        Entry::Check {
            id: TOGGLE,
            text: toggle_text(true),
            checked: true,
        },
        Entry::Separator,
        Entry::Item {
            id: OPEN_SETTINGS,
            text: "Open settings...",
        },
        Entry::Check {
            id: START_AT_LOGIN,
            text: "Start at login",
            checked: start_at_login,
        },
        Entry::Separator,
        Entry::Item {
            id: QUIT,
            text: "Quit freewheel",
        },
    ]
}

/// Builds the native tray menu from `menu_model` (main thread only); the
/// returned check items are the On/Off toggle and Start at login.
pub fn menu<R: Runtime>(
    app: &AppHandle<R>,
    start_at_login: bool,
) -> tauri::Result<(Menu<R>, CheckMenuItem<R>, CheckMenuItem<R>)> {
    let mut items: Vec<Box<dyn IsMenuItem<R>>> = Vec::new();
    let (mut toggle, mut login) = (None, None);
    for entry in menu_model(start_at_login) {
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
            Entry::Item { id, text } => {
                items.push(Box::new(MenuItem::with_id(
                    app,
                    id,
                    text,
                    true,
                    None::<&str>,
                )?));
            }
            Entry::Separator => items.push(Box::new(PredefinedMenuItem::separator(app)?)),
        }
    }
    let refs: Vec<&dyn IsMenuItem<R>> = items.iter().map(|item| item.as_ref()).collect();
    let menu = Menu::with_items(app, &refs)?;
    let (Some(toggle), Some(login)) = (toggle, login) else {
        unreachable!("the menu model has both check items");
    };
    Ok((menu, toggle, login))
}

/// Shows and focuses the settings window (Open settings, second launch).
pub fn show_settings<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Builds the tray. The toggle sends On/Off to the device loop and tells the
/// Resolve watcher (`manual`) the switch was by hand; Quit exits the app,
/// whose exit hook releases every held key.
pub fn build<R: Runtime>(
    app: &AppHandle<R>,
    commands: Sender<Command>,
    manual: Sender<bool>,
) -> tauri::Result<(TrayIcon<R>, CheckMenuItem<R>)> {
    // The checkbox shows the OS login item, not the config, at startup.
    let (menu, toggle, login) = menu(app, autostart_enabled(app))?;
    let check = toggle.clone();
    let tray = TrayIconBuilder::with_id("main")
        .icon(icon(TrayState::NotConnected))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(status_text(TrayState::NotConnected))
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            TOGGLE => {
                let on = check.is_checked().unwrap_or(true);
                let _ = check.set_text(toggle_text(on));
                let _ = commands.send(Command::SetEnabled(on));
                let _ = manual.send(on);
            }
            OPEN_SETTINGS => show_settings(app),
            START_AT_LOGIN => {
                let on = login.is_checked().unwrap_or(false);
                let store = ConfigStore::from_project_dirs();
                let result = match store {
                    Some(mut store) => {
                        set_start_at_login(|on| set_autostart(app, on), &mut store, on)
                    }
                    None => set_autostart(app, on),
                };
                if let Err(e) = result {
                    tracing::error!(error = %e, on, "cannot change start at login");
                }
                // Show what the OS really has, whether or not the change took.
                let _ = login.set_checked(autostart_enabled(app));
            }
            QUIT => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok((tray, toggle))
}

/// Shows `state` on the tray icon and the toggle.
pub fn show_state<R: Runtime>(tray: &TrayIcon<R>, toggle: &CheckMenuItem<R>, state: TrayState) {
    let on = state != TrayState::Off;
    let _ = tray.set_icon(Some(icon(state)));
    let _ = tray.set_icon_as_template(cfg!(target_os = "macos"));
    let _ = toggle.set_checked(on);
    let _ = toggle.set_text(toggle_text(on));
}

/// What the tray shows: the state icon, the battery tooltip and the
/// once-only low-battery notification.
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
    pub fn apply<R: Runtime>(
        &mut self,
        tray: &TrayIcon<R>,
        toggle: &CheckMenuItem<R>,
        update: Update,
    ) {
        match update {
            Update::State(state) => {
                self.state = state;
                // A new connection reports its battery afresh.
                self.battery = None;
                show_state(tray, toggle, state);
            }
            Update::Battery(b) => {
                self.battery = Some(b);
                if self.alert.update(b, self.threshold) {
                    let _ = tray
                        .app_handle()
                        .notification()
                        .builder()
                        .title("Speed Editor battery low")
                        .body(format!("{}% left. Charge it soon.", b.level))
                        .show();
                }
            }
        }
        let _ = tray.set_tooltip(Some(self.tooltip()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_state_has_its_own_bundled_icon() {
        let on = icon(TrayState::On);
        let off = icon(TrayState::Off);
        let absent = icon(TrayState::NotConnected);
        assert_ne!(on.rgba(), off.rgba());
        assert_ne!(on.rgba(), absent.rgba());
        assert_ne!(off.rgba(), absent.rgba());
    }

    #[test]
    fn menu_has_a_checked_toggle_open_settings_start_at_login_and_quit() {
        assert_eq!(
            menu_model(false),
            [
                Entry::Check {
                    id: TOGGLE,
                    text: "freewheel is On",
                    checked: true,
                },
                Entry::Separator,
                Entry::Item {
                    id: OPEN_SETTINGS,
                    text: "Open settings...",
                },
                Entry::Check {
                    id: START_AT_LOGIN,
                    text: "Start at login",
                    checked: false,
                },
                Entry::Separator,
                Entry::Item {
                    id: QUIT,
                    text: "Quit freewheel",
                },
            ]
        );
    }

    /// Whether the Start at login entry of the model is checked.
    fn login_checked(model: [Entry; 6]) -> bool {
        model
            .into_iter()
            .find_map(|entry| match entry {
                Entry::Check {
                    id: START_AT_LOGIN,
                    checked,
                    ..
                } => Some(checked),
                _ => None,
            })
            .unwrap()
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
        assert!(!login_checked(menu_model(config.settings.start_at_login)));
    }

    #[test]
    fn start_at_login_shows_the_state_it_is_given() {
        assert!(login_checked(menu_model(true)));
        assert!(!login_checked(menu_model(false)));
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
    fn toggle_text_names_the_state() {
        assert_eq!(toggle_text(true), "freewheel is On");
        assert_eq!(toggle_text(false), "freewheel is Off");
    }
}
