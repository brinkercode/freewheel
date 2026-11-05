//! The tray icon and its menu: the On/Off check item, the 'Needs permission'
//! status line (macOS), Open settings, Start at login and Quit.

use std::sync::mpsc::Sender;

use freewheel_core::config::ConfigStore;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_autostart::ManagerExt;

use freewheel_core::permissions::{Permissions, status_line};

use crate::device::{Command, TrayState};

const TOGGLE: &str = "toggle";
const STATUS: &str = "status";
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

/// The status line below the toggle, in the menu only while a permission is
/// missing; clicking it opens the settings window and its Permissions section.
#[derive(Clone)]
pub struct StatusLine<R: Runtime> {
    menu: Menu<R>,
    item: MenuItem<R>,
}

impl<R: Runtime> StatusLine<R> {
    /// Shows 'Needs permission (click to fix)' while one is missing.
    pub fn show(&self, permissions: &Permissions) {
        let shown = self.menu.get(STATUS).is_some();
        match status_line(permissions) {
            Some(text) => {
                let _ = self.item.set_text(text);
                if !shown {
                    let _ = self.menu.insert(&self.item, 1);
                }
            }
            None if shown => {
                let _ = self.menu.remove(&self.item);
            }
            None => {}
        }
    }
}

/// The menu, its On/Off toggle, its Start at login item and its status line.
type MenuParts<R> = (Menu<R>, CheckMenuItem<R>, CheckMenuItem<R>, StatusLine<R>);

/// The tray menu: the On/Off toggle, Start at login (checked as
/// `start_at_login` says) and the permission status line.
pub fn menu<R: Runtime>(app: &AppHandle<R>, start_at_login: bool) -> tauri::Result<MenuParts<R>> {
    let toggle = CheckMenuItem::with_id(app, TOGGLE, toggle_text(true), true, true, None::<&str>)?;
    let login = CheckMenuItem::with_id(
        app,
        START_AT_LOGIN,
        "Start at login",
        true,
        start_at_login,
        None::<&str>,
    )?;
    let item = MenuItem::with_id(app, STATUS, "", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &toggle,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, OPEN_SETTINGS, "Open settings...", true, None::<&str>)?,
            &login,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, QUIT, "Quit freewheel", true, None::<&str>)?,
        ],
    )?;
    let status = StatusLine {
        menu: menu.clone(),
        item,
    };
    Ok((menu, toggle, login, status))
}

/// Shows and focuses the settings window (Open settings, second launch).
pub fn show_settings<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Builds the tray. The toggle sends On/Off to the device loop; Quit exits
/// the app, whose exit hook releases every held key.
pub fn build<R: Runtime>(
    app: &AppHandle<R>,
    commands: Sender<Command>,
) -> tauri::Result<(TrayIcon<R>, CheckMenuItem<R>, StatusLine<R>)> {
    // The checkbox shows the OS login item, not the config, at startup.
    let (menu, toggle, login, status) = menu(app, autostart_enabled(app))?;
    let check = toggle.clone();
    let tray = TrayIconBuilder::with_id("main")
        .icon(icon(TrayState::NotConnected))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(freewheel_core::APP_NAME)
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            TOGGLE => {
                let on = check.is_checked().unwrap_or(true);
                let _ = check.set_text(toggle_text(on));
                let _ = commands.send(Command::SetEnabled(on));
            }
            STATUS | OPEN_SETTINGS => show_settings(app),
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
    Ok((tray, toggle, status))
}

/// Shows `state` on the tray icon and the toggle.
pub fn show_state<R: Runtime>(tray: &TrayIcon<R>, toggle: &CheckMenuItem<R>, state: TrayState) {
    let on = state != TrayState::Off;
    let _ = tray.set_icon(Some(icon(state)));
    let _ = tray.set_icon_as_template(cfg!(target_os = "macos"));
    let _ = toggle.set_checked(on);
    let _ = toggle.set_text(toggle_text(on));
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::menu::MenuItemKind;

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
        let app = tauri::test::mock_app();
        let (menu, toggle, login, _) = menu(app.handle(), false).unwrap();
        assert!(toggle.is_checked().unwrap());
        assert!(!login.is_checked().unwrap());
        assert_eq!(toggle.text().unwrap(), "freewheel is On");
        let items: Vec<(String, String)> = menu
            .items()
            .unwrap()
            .iter()
            .filter_map(|item| match item {
                MenuItemKind::Check(i) => Some((i.id().0.clone(), i.text().unwrap())),
                MenuItemKind::MenuItem(i) => Some((i.id().0.clone(), i.text().unwrap())),
                _ => None,
            })
            .collect();
        let pairs: Vec<(&str, &str)> = items
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                (TOGGLE, "freewheel is On"),
                (OPEN_SETTINGS, "Open settings..."),
                (START_AT_LOGIN, "Start at login"),
                (QUIT, "Quit freewheel"),
            ]
        );
    }

    fn ids<R: Runtime>(menu: &Menu<R>) -> Vec<String> {
        menu.items()
            .unwrap()
            .iter()
            .filter_map(|item| match item {
                MenuItemKind::Check(i) => Some(i.id().0.clone()),
                MenuItemKind::MenuItem(i) => Some(i.id().0.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_missing_permission_shows_needs_permission_below_the_toggle() {
        use freewheel_core::permissions::{Permission, check_all};
        let app = tauri::test::mock_app();
        let (menu, _, _, status) = menu(app.handle(), false).unwrap();
        status.show(&check_all(|p| p != Permission::Accessibility));
        assert_eq!(
            ids(&menu),
            [TOGGLE, STATUS, OPEN_SETTINGS, START_AT_LOGIN, QUIT]
        );
        let line = menu.get(STATUS).unwrap();
        let line = line.as_menuitem().unwrap();
        assert_eq!(line.text().unwrap(), "Needs permission (click to fix)");
        assert!(line.is_enabled().unwrap());
        status.show(&check_all(|p| p != Permission::InputMonitoring));
        assert_eq!(
            ids(&menu),
            [TOGGLE, STATUS, OPEN_SETTINGS, START_AT_LOGIN, QUIT]
        );
        status.show(&check_all(|_| true));
        assert_eq!(ids(&menu), [TOGGLE, OPEN_SETTINGS, START_AT_LOGIN, QUIT]);
        status.show(&Permissions::new());
        assert_eq!(ids(&menu), [TOGGLE, OPEN_SETTINGS, START_AT_LOGIN, QUIT]);
    }

    #[test]
    fn start_at_login_is_unchecked_on_a_fresh_install() {
        let dir = tempfile::tempdir().unwrap();
        let config = ConfigStore::new(dir.path()).load().unwrap();
        assert!(!config.settings.start_at_login);
        let app = tauri::test::mock_app();
        let (_, _, login, _) = menu(app.handle(), config.settings.start_at_login).unwrap();
        assert!(!login.is_checked().unwrap());
    }

    #[test]
    fn start_at_login_shows_the_state_it_is_given() {
        let app = tauri::test::mock_app();
        let (_, _, login, _) = menu(app.handle(), true).unwrap();
        assert!(login.is_checked().unwrap());
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
