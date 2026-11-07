//! The real [`HidTransport`]: a Speed Editor opened through hidapi.
//! Never constructed in tests; only its error mapping is tested.

use std::time::Duration;

use freewheel_core::diagnostics::{self, DeviceInfo};
use freewheel_core::hid::{HidError, HidTransport};
use freewheel_core::simulator::Bus;
use hidapi::{BusType, HidApi, HidDevice};

/// Blackmagic Design vendor id.
pub const VID: u16 = 0x1edb;
/// DaVinci Resolve Speed Editor product id. The S1 hardware spike recorded
/// the same VID/PID over Bluetooth as over USB, so one pair opens both.
pub const PID: u16 = 0xda0e;

/// One HID interface (Windows: top-level collection) with the Speed
/// Editor's ids, as hidapi lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub bus: Option<Bus>,
    pub usage_page: u16,
    pub usage: u16,
    pub interface: i32,
}

/// The order to try the listed interfaces in: USB before Bluetooth (a
/// cable is the more reliable link when both are there), then
/// vendor-defined usage pages (0xff00..) first, where vendor feature
/// reports like the auth report 6 live, then by interface number. Returns
/// indexes into `candidates`.
pub fn try_order(candidates: &[Candidate]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by_key(|&i| {
        let c = &candidates[i];
        (
            c.bus != Some(Bus::Usb),
            c.usage_page < 0xff00,
            c.interface,
            c.usage_page,
            c.usage,
        )
    });
    order
}

/// A Speed Editor opened through hidapi.
pub struct HidapiTransport {
    device: Option<HidDevice>,
    bus: Option<Bus>,
    info: DeviceInfo,
}

impl HidapiTransport {
    /// Opens the Speed Editor over USB or Bluetooth. hidapi's list is
    /// refreshed on every call, so a device plugged in or paired after
    /// launch is found by the next 3 s retry. When the OS lists several
    /// interfaces with the Speed Editor's ids (Windows lists each HID
    /// top-level collection, and a paired Bluetooth one next to a cabled
    /// one), each is tried in [`try_order`] until one accepts the auth
    /// reset (feature report 6, step 0: the handshake's own first step).
    pub fn open(api: &mut HidApi) -> Result<Self, HidError> {
        if let Err(e) = api.refresh_devices() {
            tracing::warn!(error = %e, "cannot refresh the HID device list");
        }
        let listed: Vec<hidapi::DeviceInfo> = api
            .device_list()
            .filter(|d| d.vendor_id() == VID && d.product_id() == PID)
            .cloned()
            .collect();
        let candidates: Vec<Candidate> = listed
            .iter()
            .map(|d| Candidate {
                bus: bus(d.bus_type()),
                usage_page: d.usage_page(),
                usage: d.usage(),
                interface: d.interface_number(),
            })
            .collect();
        if listed.is_empty() {
            return Err(HidError::Io(
                "no Speed Editor (1edb:da0e) listed by the OS".into(),
            ));
        }
        tracing::debug!(?candidates, "Speed Editor interfaces listed");
        let mut first_error = None;
        for i in try_order(&candidates) {
            let d = &listed[i];
            let device = match api.open_path(d.path()) {
                Ok(device) => device,
                Err(e) => {
                    tracing::info!(candidate = ?candidates[i], error = %e, "cannot open this Speed Editor interface");
                    first_error.get_or_insert(map_error(e));
                    continue;
                }
            };
            if let Err(e) = device.send_feature_report(&AUTH_RESET) {
                tracing::info!(candidate = ?candidates[i], error = %e, "interface refused the auth report, trying the next");
                first_error.get_or_insert(map_error(e));
                continue;
            }
            let mut descriptor = vec![0u8; 4096];
            let report_descriptor = match device.get_report_descriptor(&mut descriptor) {
                Ok(n) => Some(diagnostics::hex(&descriptor[..n.min(descriptor.len())])),
                Err(e) => {
                    tracing::debug!(error = %e, "no report descriptor");
                    None
                }
            };
            let info = DeviceInfo {
                bus: candidates[i].bus,
                vendor_id: VID,
                product_id: PID,
                usage_page: d.usage_page(),
                usage: d.usage(),
                interface: d.interface_number(),
                product: d.product_string().map(str::to_owned),
                release: d.release_number(),
                candidates: u32::try_from(listed.len()).unwrap_or(u32::MAX),
                report_descriptor,
            };
            tracing::info!(
                bus = ?info.bus,
                usage_page = info.usage_page,
                usage = info.usage,
                interface = info.interface,
                candidates = info.candidates,
                "opened the Speed Editor"
            );
            if let Some(desc) = &info.report_descriptor {
                tracing::debug!(%desc, "Speed Editor report descriptor");
            }
            return Ok(Self {
                device: Some(device),
                bus: info.bus,
                info,
            });
        }
        Err(first_error.unwrap_or(HidError::Disconnected))
    }

    /// How the opened device is attached, when hidapi says.
    pub fn bus(&self) -> Option<Bus> {
        self.bus
    }

    fn device(&self) -> Result<&HidDevice, HidError> {
        self.device.as_ref().ok_or(HidError::Closed)
    }
}

/// Feature report 6, step 0: resets the device's auth state machine.
const AUTH_RESET: [u8; 10] = [0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0];

impl HidTransport for HidapiTransport {
    fn read_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, HidError> {
        let ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
        // hidapi already returns Ok(0) when the timeout elapses.
        self.device()?.read_timeout(buf, ms).map_err(map_error)
    }

    fn write(&mut self, data: &[u8]) -> Result<usize, HidError> {
        let device = self.device()?;
        // Over Bluetooth on Windows a plain write is an unacknowledged GATT
        // write that some adapters drop silently (libusb/hidapi Known
        // Issues); an output report set is acknowledged. Fall back to the
        // plain write if the device does not take it.
        if self.bus == Some(Bus::Bluetooth) {
            match device.send_output_report(data) {
                Ok(()) => return Ok(data.len()),
                Err(e) => tracing::debug!(error = %e, "output report set failed, writing instead"),
            }
        }
        device.write(data).map_err(map_error)
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

    fn info(&self) -> Option<DeviceInfo> {
        Some(self.info.clone())
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
            [
                "disconnect",
                "not connected",
                "no such device",
                "not found",
                // Windows: ERROR_DEVICE_NOT_CONNECTED, ERROR_GEN_FAILURE on a
                // Bluetooth device that went to sleep or out of range.
                "does not exist",
                "device attached to the system is not functioning",
            ]
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
    fn interfaces_are_tried_usb_first_then_vendor_pages() {
        let c = |bus, usage_page, interface| Candidate {
            bus: Some(bus),
            usage_page,
            usage: 1,
            interface,
        };
        let listed = [
            c(Bus::Bluetooth, 0xff00, -1),
            c(Bus::Usb, 0x0001, 0),
            c(Bus::Usb, 0xff00, 0),
            c(Bus::Bluetooth, 0x000c, -1),
        ];
        assert_eq!(try_order(&listed), [2, 1, 0, 3]);
        assert_eq!(try_order(&[]), Vec::<usize>::new());
    }

    #[test]
    fn windows_device_gone_messages_map_to_disconnected() {
        for message in [
            "The device does not exist.",
            "A device attached to the system is not functioning.",
        ] {
            assert_eq!(map_error(api_error(message)), HidError::Disconnected);
        }
    }

    #[test]
    fn the_open_probe_is_the_handshakes_own_reset() {
        assert_eq!(AUTH_RESET, [0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn other_errors_map_to_io() {
        assert_eq!(
            map_error(api_error("access denied")),
            HidError::Io("hidapi error: access denied".to_string())
        );
    }
}
