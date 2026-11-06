//! What the settings window hears from the app: the `status` event
//! (connection, battery, layer, override errors, permissions) and the
//! `key-pressed` event that names each physical press by its printed key.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::battery::Battery;
use crate::config::{LayerId, OverrideMode};
use crate::connection::ConnectionStatus;
use crate::overrides::OverrideError;
use crate::permissions::{Grant, Permissions};
use crate::report::Key;
use crate::simulator::Bus;

/// The event name of [`StatusEvent`].
pub const STATUS_EVENT: &str = "status";
/// The event name of [`KeyPressed`].
pub const KEY_PRESSED_EVENT: &str = "key-pressed";

/// An override control call that failed, as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct OverrideErrorStatus {
    pub mode: OverrideMode,
    pub error: String,
}

impl From<&OverrideError> for OverrideErrorStatus {
    fn from(e: &OverrideError) -> Self {
        OverrideErrorStatus {
            mode: e.mode,
            error: e.error.0.clone(),
        }
    }
}

/// The settings window's view of the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct StatusEvent {
    pub connection: ConnectionStatus,
    pub battery: Option<Battery>,
    pub layer: LayerId,
    pub override_errors: Vec<OverrideErrorStatus>,
    /// Permission name (`Accessibility`, `InputMonitoring`) to its grant;
    /// empty outside macOS.
    pub permissions: BTreeMap<String, Grant>,
    /// How the connected device is attached; `None` while not connected or
    /// when the OS does not say.
    pub bus: Option<Bus>,
    /// freewheel stepped aside (switched Off) because DaVinci Resolve is open.
    pub paused: bool,
}

impl StatusEvent {
    /// The status before the device loop has said anything.
    pub fn new(layer: impl Into<LayerId>) -> Self {
        StatusEvent {
            connection: ConnectionStatus::Absent,
            battery: None,
            layer: layer.into(),
            override_errors: Vec::new(),
            permissions: BTreeMap::new(),
            bus: None,
            paused: false,
        }
    }

    /// Sets the connection; a new connection reports its battery and bus afresh.
    pub fn set_connection(&mut self, connection: ConnectionStatus) {
        if connection != ConnectionStatus::Connected {
            self.battery = None;
            self.bus = None;
        }
        self.connection = connection;
    }

    pub fn set_permissions(&mut self, permissions: &Permissions) {
        self.permissions = permissions
            .iter()
            .map(|(p, g)| (p.name().to_string(), *g))
            .collect();
    }

    pub fn set_override_errors(&mut self, errors: &[OverrideError]) {
        self.override_errors = errors.iter().map(OverrideErrorStatus::from).collect();
    }
}

/// One physical press: `key` is the binding name the config uses, `label`
/// what is printed on the key, so the Keys tab can say 'You pressed CUT'.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct KeyPressed {
    pub key: String,
    pub label: String,
}

impl From<Key> for KeyPressed {
    fn from(key: Key) -> Self {
        KeyPressed {
            key: format!("{key:?}"),
            label: printed_label(key).to_string(),
        }
    }
}

/// The text printed on a Speed Editor key.
pub fn printed_label(key: Key) -> &'static str {
    match key {
        Key::SmartInsrt => "SMART INSRT",
        Key::Appnd => "APPND",
        Key::RiplOwr => "RIPL O/WR",
        Key::CloseUp => "CLOSE UP",
        Key::PlaceOnTop => "PLACE ON TOP",
        Key::SrcOwr => "SRC O/WR",
        Key::In => "IN",
        Key::Out => "OUT",
        Key::TrimIn => "TRIM IN",
        Key::TrimOut => "TRIM OUT",
        Key::Roll => "ROLL",
        Key::SlipSrc => "SLIP SRC",
        Key::SlipDest => "SLIP DEST",
        Key::TransDur => "TRANS DUR",
        Key::Cut => "CUT",
        Key::Dis => "DIS",
        Key::SmthCut => "SMTH CUT",
        Key::Source => "SOURCE",
        Key::Timeline => "TIMELINE",
        Key::Shtl => "SHTL",
        Key::Jog => "JOG",
        Key::Scrl => "SCRL",
        Key::Esc => "ESC",
        Key::SyncBin => "SYNC BIN",
        Key::AudioLevel => "AUDIO LEVEL",
        Key::FullView => "FULL VIEW",
        Key::Trans => "TRANS",
        Key::Split => "SPLIT",
        Key::Snap => "SNAP",
        Key::RiplDel => "RIPL DEL",
        Key::Cam1 => "CAM 1",
        Key::Cam2 => "CAM 2",
        Key::Cam3 => "CAM 3",
        Key::Cam4 => "CAM 4",
        Key::Cam5 => "CAM 5",
        Key::Cam6 => "CAM 6",
        Key::Cam7 => "CAM 7",
        Key::Cam8 => "CAM 8",
        Key::Cam9 => "CAM 9",
        Key::LiveOwr => "LIVE O/WR",
        Key::VideoOnly => "VIDEO ONLY",
        Key::AudioOnly => "AUDIO ONLY",
        Key::StopPlay => "STOP/PLAY",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overrides::ControlError;
    use crate::permissions::{Permission, check_all};

    #[test]
    fn the_status_event_carries_every_part_the_ui_shows() {
        let mut status = StatusEvent::new("base");
        status.set_connection(ConnectionStatus::Connected);
        status.battery = Some(Battery {
            level: 64,
            charging: true,
        });
        status.layer = "edit".into();
        status.set_override_errors(&[OverrideError {
            mode: OverrideMode::Brightness,
            error: ControlError("DELL U2720Q: no DDC reply".into()),
        }]);
        status.set_permissions(&check_all(|p| p == Permission::Accessibility));
        status.bus = Some(Bus::Bluetooth);
        status.paused = true;
        assert_eq!(
            serde_json::to_value(&status).unwrap(),
            serde_json::json!({
                "connection": "Connected",
                "battery": {"level": 64, "charging": true},
                "layer": "edit",
                "override_errors": [
                    {"mode": "Brightness", "error": "DELL U2720Q: no DDC reply"}
                ],
                "permissions": {"Accessibility": "Granted", "InputMonitoring": "Missing"},
                "bus": "Bluetooth",
                "paused": true
            })
        );
    }

    #[test]
    fn losing_the_connection_forgets_the_battery() {
        let mut status = StatusEvent::new("base");
        status.set_connection(ConnectionStatus::Connected);
        status.battery = Some(Battery {
            level: 10,
            charging: false,
        });
        status.bus = Some(Bus::Usb);
        status.set_connection(ConnectionStatus::Absent);
        assert_eq!(status.battery, None);
        assert_eq!(status.bus, None);
        assert_eq!(status.connection, ConnectionStatus::Absent);
    }

    #[test]
    fn a_key_press_names_the_binding_key_and_its_printed_label() {
        assert_eq!(
            serde_json::to_value(KeyPressed::from(Key::Cut)).unwrap(),
            serde_json::json!({"key": "Cut", "label": "CUT"})
        );
        assert_eq!(KeyPressed::from(Key::SmartInsrt).label, "SMART INSRT");
        assert_eq!(KeyPressed::from(Key::StopPlay).label, "STOP/PLAY");
    }

    #[test]
    fn every_key_has_a_distinct_printed_label() {
        let mut labels: Vec<&str> = Key::ALL.iter().map(|&k| printed_label(k)).collect();
        assert!(labels.iter().all(|l| !l.is_empty()));
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), Key::ALL.len());
    }
}
