//! The real [`HidTransport`]: a Speed Editor opened through hidapi.
//! Never constructed in tests; only its error mapping is tested.

use std::time::Duration;

use freewheel_core::hid::{HidError, HidTransport};
use freewheel_core::simulator::Bus;
use hidapi::{BusType, HidApi, HidDevice};

/// Blackmagic Design vendor id.
pub const VID: u16 = 0x1edb;
/// DaVinci Resolve Speed Editor product id. The S1 hardware spike recorded
/// the same VID/PID over Bluetooth as over USB, so one pair opens both.
pub const PID: u16 = 0xda0e;

/// A Speed Editor opened through hidapi.
pub struct HidapiTransport {
    device: Option<HidDevice>,
    bus: Option<Bus>,
}

impl HidapiTransport {
    /// Opens the first Speed Editor hidapi can see, over USB or Bluetooth.
    pub fn open(api: &HidApi) -> Result<Self, HidError> {
        let device = api.open(VID, PID).map_err(map_error)?;
        let bus = match device.get_device_info() {
            Ok(info) => bus(info.bus_type()),
            Err(e) => {
                tracing::warn!(error = %e, "cannot read the Speed Editor's bus");
                None
            }
        };
        tracing::info!(vid = VID, pid = PID, ?bus, "opened the Speed Editor");
        Ok(Self {
            device: Some(device),
            bus,
        })
    }

    /// How the opened device is attached, when hidapi says.
    pub fn bus(&self) -> Option<Bus> {
        self.bus
    }

    fn device(&self) -> Result<&HidDevice, HidError> {
        self.device.as_ref().ok_or(HidError::Closed)
    }
}

impl HidTransport for HidapiTransport {
    fn read_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, HidError> {
        let ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
        // hidapi already returns Ok(0) when the timeout elapses.
        self.device()?.read_timeout(buf, ms).map_err(map_error)
    }

    fn write(&mut self, data: &[u8]) -> Result<usize, HidError> {
        self.device()?.write(data).map_err(map_error)
    }

    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize, HidError> {
        self.device()?.get_feature_report(buf).map_err(map_error)
    }

    fn send_feature_report(&mut self, data: &[u8]) -> Result<(), HidError> {
        self.device()?.send_feature_report(data).map_err(map_error)
    }

    fn close(&mut self) {
        self.device = None;
    }
}

/// Maps a hidapi error to the kinds [`freewheel_core::hid::FakeTransport`]
/// returns: a gone device is [`HidError::Disconnected`], anything else `Io`.
fn map_error(err: hidapi::HidError) -> HidError {
    use std::io::ErrorKind;
    let gone = match &err {
        hidapi::HidError::IoError { error } => {
            matches!(
                error.kind(),
                ErrorKind::NotConnected | ErrorKind::BrokenPipe
            ) || error.raw_os_error() == Some(19) // ENODEV
        }
        hidapi::HidError::HidApiError { message } => {
            let m = message.to_ascii_lowercase();
            ["disconnect", "not connected", "no such device", "not found"]
                .iter()
                .any(|s| m.contains(s))
        }
        _ => false,
    };
    if gone {
        HidError::Disconnected
    } else {
        HidError::Io(err.to_string())
    }
}

/// The status bus for hidapi's bus type; other buses are not a Speed Editor's.
fn bus(bus: BusType) -> Option<Bus> {
    match bus {
        BusType::Usb => Some(Bus::Usb),
        BusType::Bluetooth => Some(Bus::Bluetooth),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_error(message: &str) -> hidapi::HidError {
        hidapi::HidError::HidApiError {
            message: message.to_string(),
        }
    }

    #[test]
    fn opens_the_speed_editor_usb_and_bluetooth_ids() {
        assert_eq!((VID, PID), (0x1edb, 0xda0e));
    }

    #[test]
    fn usb_and_bluetooth_buses_name_the_status_bus() {
        assert_eq!(bus(BusType::Usb), Some(Bus::Usb));
        assert_eq!(bus(BusType::Bluetooth), Some(Bus::Bluetooth));
        assert_eq!(bus(BusType::Unknown), None);
    }

    #[test]
    fn device_gone_messages_map_to_disconnected() {
        for message in [
            "The device is not connected.",
            "hid_read_timeout: device disconnected",
            "No such device",
            "hid_open: device not found",
        ] {
            assert_eq!(map_error(api_error(message)), HidError::Disconnected);
        }
    }

    #[test]
    fn device_gone_io_errors_map_to_disconnected() {
        let enodev = std::io::Error::from_raw_os_error(19);
        let not_connected = std::io::Error::from(std::io::ErrorKind::NotConnected);
        for error in [enodev, not_connected] {
            assert_eq!(
                map_error(hidapi::HidError::IoError { error }),
                HidError::Disconnected
            );
        }
    }

    #[test]
    fn other_errors_map_to_io() {
        assert_eq!(
            map_error(api_error("access denied")),
            HidError::Io("hidapi error: access denied".to_string())
        );
    }
}
