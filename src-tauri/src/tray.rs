//! The tray icon and its menu: the On/Off check item, the 'Needs permission'
//! status line (macOS), Open settings and Quit.

use std::sync::mpsc::Sender;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Runtime};

use freewheel_core::permissions::{Permissions, status_line};

use crate::device::{Command, TrayState};

const TOGGLE: &str = "toggle";
const STATUS: &str = "status";
const OPEN_SETTINGS: &str = "open-settings";
const QUIT: &str = "quit";

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

/// The tray menu; the returned check item is the On/Off toggle.
pub fn menu<R: Runtime>(
    app: &AppHandle<R>,
) -> tauri::Result<(Menu<R>, CheckMenuItem<R>, StatusLine<R>)> {
    let toggle = CheckMenuItem::with_id(app, TOGGLE, toggle_text(true), true, true, None::<&str>)?;
    let item = MenuItem::with_id(app, STATUS, "", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &toggle,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, OPEN_SETTINGS, "Open settings...", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, QUIT, "Quit freewheel", true, None::<&str>)?,
        ],
    )?;
    let status = StatusLine {
        menu: menu.clone(),
        item,
    };
    Ok((menu, toggle, status))
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
    let (menu, toggle, status) = menu(app)?;
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
    fn menu_has_a_checked_toggle_open_settings_and_quit() {
        let app = tauri::test::mock_app();
        let (menu, toggle, _) = menu(app.handle()).unwrap();
        assert!(toggle.is_checked().unwrap());
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
        let (menu, _, status) = menu(app.handle()).unwrap();
        status.show(&check_all(|p| p != Permission::Accessibility));
        assert_eq!(ids(&menu), [TOGGLE, STATUS, OPEN_SETTINGS, QUIT]);
        let line = menu.get(STATUS).unwrap();
        let line = line.as_menuitem().unwrap();
        assert_eq!(line.text().unwrap(), "Needs permission (click to fix)");
        assert!(line.is_enabled().unwrap());
        status.show(&check_all(|p| p != Permission::InputMonitoring));
        assert_eq!(ids(&menu), [TOGGLE, STATUS, OPEN_SETTINGS, QUIT]);
        status.show(&check_all(|_| true));
        assert_eq!(ids(&menu), [TOGGLE, OPEN_SETTINGS, QUIT]);
        status.show(&Permissions::new());
        assert_eq!(ids(&menu), [TOGGLE, OPEN_SETTINGS, QUIT]);
    }

    #[test]
    fn toggle_text_names_the_state() {
        assert_eq!(toggle_text(true), "freewheel is On");
        assert_eq!(toggle_text(false), "freewheel is Off");
    }
}
