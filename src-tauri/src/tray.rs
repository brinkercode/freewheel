//! The tray icon and its menu: the On/Off check item, Open settings and Quit.

use std::sync::mpsc::Sender;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Runtime};

use freewheel_core::battery::{self, Battery, LowBatteryAlert};
use tauri_plugin_notification::NotificationExt;

use crate::device::{Command, TrayState, Update};

const TOGGLE: &str = "toggle";
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

/// The device status the tooltip shows while no battery data is known.
fn status_text(state: TrayState) -> &'static str {
    match state {
        TrayState::On => "Connected",
        TrayState::Off => "freewheel is Off",
        TrayState::NotConnected => "Looking for device",
    }
}

/// The tray menu; the returned check item is the On/Off toggle.
pub fn menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<(Menu<R>, CheckMenuItem<R>)> {
    let toggle = CheckMenuItem::with_id(app, TOGGLE, toggle_text(true), true, true, None::<&str>)?;
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
    Ok((menu, toggle))
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
) -> tauri::Result<(TrayIcon<R>, CheckMenuItem<R>)> {
    let (menu, toggle) = menu(app)?;
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
            }
            OPEN_SETTINGS => show_settings(app),
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
        let (menu, toggle) = menu(app.handle()).unwrap();
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
    fn toggle_text_names_the_state() {
        assert_eq!(toggle_text(true), "freewheel is On");
        assert_eq!(toggle_text(false), "freewheel is Off");
    }
}
