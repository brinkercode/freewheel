//! Device I/O behind the [`HidTransport`] trait, so protocol logic can be
//! tested without hardware. The hidapi-backed transport lives in the app
//! crate; this module has no OS or hidapi dependency.

use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

use crate::diagnostics::DeviceInfo;

/// An error from a HID transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HidError {
    /// The device went away (unplugged, powered off or out of range).
    Disconnected,
    /// The transport was closed by [`HidTransport::close`].
    Closed,
    /// Any other I/O failure reported by the backend.
    Io(String),
}

impl fmt::Display for HidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HidError::Disconnected => write!(f, "device disconnected"),
            HidError::Closed => write!(f, "transport closed"),
            HidError::Io(msg) => write!(f, "HID I/O error: {msg}"),
        }
    }
}

impl std::error::Error for HidError {}

/// Raw report I/O with one opened HID device.
pub trait HidTransport {
    /// Reads one input report into `buf`, waiting at most `timeout`.
    /// Returns the number of bytes read; `Ok(0)` means the timeout elapsed.
    fn read_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, HidError>;

    /// Writes one output report (first byte is the report id).
    /// Returns the number of bytes written.
    fn write(&mut self, data: &[u8]) -> Result<usize, HidError>;

    /// Gets a feature report. `buf[0]` holds the requested report id on
    /// entry; on return `buf` holds the report. Returns its length.
    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize, HidError>;

    /// Sends a feature report (first byte is the report id).
    fn send_feature_report(&mut self, data: &[u8]) -> Result<(), HidError>;

    /// Closes the device. Later calls fail with [`HidError::Closed`].
    fn close(&mut self);

    /// What the OS says about the opened device, for the Device test view.
    fn info(&self) -> Option<DeviceInfo> {
        None
    }
}

/// One scripted step of input for [`FakeTransport::read_timeout`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptedInput {
    /// An input report returned by the next read.
    Report(Vec<u8>),
    /// The next read times out (returns `Ok(0)`).
    Timeout,
}

/// Something the code under test sent to a [`FakeTransport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Written {
    /// An output report passed to `write`.
    Output(Vec<u8>),
    /// A feature report passed to `send_feature_report`.
    Feature(Vec<u8>),
}

/// A scriptable in-memory device for tests.
///
/// Reads replay the scripted input in order, `get_feature_report` replays
/// the scripted feature reports in order, and every write is recorded.
/// When a script runs out, the call fails with [`HidError::Disconnected`]
/// and the fake stays disconnected from then on.
#[derive(Debug, Default)]
pub struct FakeTransport {
    inputs: VecDeque<ScriptedInput>,
    features: VecDeque<Vec<u8>>,
    written: Vec<Written>,
    disconnected: bool,
    closed: bool,
    info: Option<DeviceInfo>,
}

impl FakeTransport {
    /// A fake with empty scripts.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an input report to the read script.
    pub fn push_input(&mut self, report: impl Into<Vec<u8>>) -> &mut Self {
        self.inputs.push_back(ScriptedInput::Report(report.into()));
        self
    }

    /// Appends a read timeout to the read script.
    pub fn push_timeout(&mut self) -> &mut Self {
        self.inputs.push_back(ScriptedInput::Timeout);
        self
    }

    /// Appends a feature report to the `get_feature_report` script.
    pub fn push_feature(&mut self, report: impl Into<Vec<u8>>) -> &mut Self {
        self.features.push_back(report.into());
        self
    }

    /// Everything written so far, in order.
    pub fn written(&self) -> &[Written] {
        &self.written
    }

    /// Sets what [`HidTransport::info`] returns.
    pub fn set_info(&mut self, info: DeviceInfo) -> &mut Self {
        self.info = Some(info);
        self
    }

    /// Whether `close` has been called.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    fn check_open(&self) -> Result<(), HidError> {
        if self.closed {
            Err(HidError::Closed)
        } else if self.disconnected {
            Err(HidError::Disconnected)
        } else {
            Ok(())
        }
    }

    fn disconnect(&mut self) -> HidError {
        self.disconnected = true;
        HidError::Disconnected
    }
}

fn copy_into(buf: &mut [u8], report: &[u8]) -> usize {
    let n = report.len().min(buf.len());
    buf[..n].copy_from_slice(&report[..n]);
    n
}

impl HidTransport for FakeTransport {
    fn read_timeout(&mut self, buf: &mut [u8], _timeout: Duration) -> Result<usize, HidError> {
        self.check_open()?;
        match self.inputs.pop_front() {
            Some(ScriptedInput::Report(report)) => Ok(copy_into(buf, &report)),
            Some(ScriptedInput::Timeout) => Ok(0),
            None => Err(self.disconnect()),
        }
    }

    fn write(&mut self, data: &[u8]) -> Result<usize, HidError> {
        self.check_open()?;
        self.written.push(Written::Output(data.to_vec()));
        Ok(data.len())
    }

    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize, HidError> {
        self.check_open()?;
        match self.features.pop_front() {
            Some(report) => Ok(copy_into(buf, &report)),
            None => Err(self.disconnect()),
        }
    }

    fn send_feature_report(&mut self, data: &[u8]) -> Result<(), HidError> {
        self.check_open()?;
        self.written.push(Written::Feature(data.to_vec()));
        Ok(())
    }

    fn close(&mut self) {
        self.closed = true;
    }

    fn info(&self) -> Option<DeviceInfo> {
        self.info.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Duration = Duration::from_millis(10);

    #[test]
    fn fake_replays_scripted_input_in_order() {
        let mut fake = FakeTransport::new();
        fake.push_input([0x04, 1, 2])
            .push_timeout()
            .push_input([0x03, 9]);
        let mut buf = [0u8; 8];
        assert_eq!(fake.read_timeout(&mut buf, T), Ok(3));
        assert_eq!(&buf[..3], &[0x04, 1, 2]);
        assert_eq!(fake.read_timeout(&mut buf, T), Ok(0));
        assert_eq!(fake.read_timeout(&mut buf, T), Ok(2));
        assert_eq!(&buf[..2], &[0x03, 9]);
    }

    #[test]
    fn fake_replays_scripted_feature_reports() {
        let mut fake = FakeTransport::new();
        fake.push_feature([0x06, 0, 1, 2])
            .push_feature([0x06, 2, 3]);
        let mut buf = [0x06u8, 0, 0, 0, 0];
        assert_eq!(fake.get_feature_report(&mut buf), Ok(4));
        assert_eq!(&buf[..4], &[0x06, 0, 1, 2]);
        assert_eq!(fake.get_feature_report(&mut buf), Ok(3));
        assert_eq!(&buf[..3], &[0x06, 2, 3]);
    }

    #[test]
    fn fake_records_every_write() {
        let mut fake = FakeTransport::new();
        assert_eq!(fake.write(&[0x02, 0xff]), Ok(2));
        assert_eq!(fake.send_feature_report(&[0x06, 0, 0]), Ok(()));
        assert_eq!(fake.write(&[0x03, 1]), Ok(2));
        assert_eq!(
            fake.written(),
            &[
                Written::Output(vec![0x02, 0xff]),
                Written::Feature(vec![0x06, 0, 0]),
                Written::Output(vec![0x03, 1]),
            ]
        );
    }

    #[test]
    fn fake_returns_disconnect_after_input_script_ends() {
        let mut fake = FakeTransport::new();
        fake.push_input([0x04]);
        let mut buf = [0u8; 4];
        assert_eq!(fake.read_timeout(&mut buf, T), Ok(1));
        assert_eq!(fake.read_timeout(&mut buf, T), Err(HidError::Disconnected));
        // It stays disconnected for every call afterwards.
        assert_eq!(fake.read_timeout(&mut buf, T), Err(HidError::Disconnected));
        assert_eq!(fake.write(&[0x02]), Err(HidError::Disconnected));
    }

    #[test]
    fn fake_returns_disconnect_after_feature_script_ends() {
        let mut fake = FakeTransport::new();
        let mut buf = [0x06u8; 4];
        assert_eq!(
            fake.get_feature_report(&mut buf),
            Err(HidError::Disconnected)
        );
        assert_eq!(
            fake.send_feature_report(&[0x06]),
            Err(HidError::Disconnected)
        );
    }

    #[test]
    fn fake_fails_with_closed_after_close() {
        let mut fake = FakeTransport::new();
        fake.push_input([0x04]).push_feature([0x06]);
        fake.close();
        assert!(fake.is_closed());
        let mut buf = [0u8; 4];
        assert_eq!(fake.read_timeout(&mut buf, T), Err(HidError::Closed));
        assert_eq!(fake.write(&[0x02]), Err(HidError::Closed));
        assert_eq!(fake.get_feature_report(&mut buf), Err(HidError::Closed));
        assert_eq!(fake.send_feature_report(&[0x06]), Err(HidError::Closed));
        assert!(fake.written().is_empty());
    }

    #[test]
    fn transport_is_usable_as_a_trait_object() {
        let mut fake = FakeTransport::new();
        fake.push_input([0x07, 50]);
        let transport: &mut dyn HidTransport = &mut fake;
        let mut buf = [0u8; 2];
        assert_eq!(transport.read_timeout(&mut buf, T), Ok(2));
        transport.close();
        assert!(fake.is_closed());
    }
}
