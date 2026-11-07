//! What the Help & backup "Device test" view shows: the connection, the
//! auth handshake, the battery, the last raw reports both ways, the keys
//! held and pressed and the wheel. The connection manager records it as it
//! goes, so the real device loop and `--simulate` fill it the same way.
//!
//! Privacy: auth feature reports are never kept (only their step and
//! header), so a copied diagnostics text holds no auth bytes.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::battery::Battery;
use crate::connection::ConnectionStatus;
use crate::report::{DeviceEvent, Key, ReportError};
use crate::simulator::Bus;
use crate::status::printed_label;

/// The event name the device loop emits a [`DeviceDiagnostics`] under.
pub const DEVICE_DIAGNOSTICS_EVENT: &str = "device-diagnostics";
/// Raw reports kept, newest first.
pub const MAX_REPORTS: usize = 24;
/// Recent key presses kept, newest first.
pub const MAX_PRESSES: usize = 16;

/// Which way a raw report went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum Direction {
    /// An input report the device sent.
    In,
    /// An output report freewheel wrote (wheel mode, LEDs).
    Out,
}

/// One raw report as the Device test view lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RawReport {
    /// Counts every report recorded since the app started.
    #[cfg_attr(test, ts(type = "number"))]
    pub seq: u64,
    /// Milliseconds since the device loop started.
    #[cfg_attr(test, ts(type = "number"))]
    pub at_ms: u64,
    pub direction: Direction,
    /// The report id (first byte); 0 for an empty report.
    pub id: u8,
    /// Every byte, the report id included, as spaced hex (`04 0f 00 ...`).
    pub hex: String,
    /// What freewheel made of it, e.g. `keys: CUT` or `unknown report id 0x09`.
    pub decoded: String,
    /// Whether a write failed (output reports only).
    pub failed: bool,
}

/// How the auth handshake is going.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum AuthState {
    /// No handshake has run on this connection yet.
    NotYet,
    /// The last handshake passed.
    Passed,
    /// The last handshake failed (`last_error` says how).
    Failed,
}

/// The auth part of the diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AuthDiagnostics {
    pub state: AuthState,
    /// Handshakes passed since the app started (first auth and re-auths).
    pub passed: u32,
    pub failed: u32,
    /// The session the device granted in its last status report (seconds).
    pub session_s: Option<u32>,
    /// Seconds until the next re-auth is due (timeout or idle check).
    #[cfg_attr(test, ts(type = "number | null"))]
    pub next_in_s: Option<u64>,
    /// Why the last handshake failed, without any auth bytes.
    pub last_error: Option<String>,
    /// When the last handshake passed (ms since the device loop started).
    #[cfg_attr(test, ts(type = "number | null"))]
    pub last_passed_ms: Option<u64>,
}

/// The wheel part of the diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct WheelDiagnostics {
    /// The mode byte of the last wheel report (0/2 relative, 1/3 absolute).
    pub device_mode: Option<u8>,
    /// The value of the last wheel report.
    pub last_value: Option<i32>,
    /// The sum of relative values, or the last absolute position.
    #[cfg_attr(test, ts(type = "number"))]
    pub total: i64,
    #[cfg_attr(test, ts(type = "number"))]
    pub reports: u64,
}

/// What the OS says about the opened device (USB or Bluetooth).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DeviceInfo {
    pub bus: Option<Bus>,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub interface: i32,
    pub product: Option<String>,
    pub release: u16,
    /// HID interfaces/collections with the Speed Editor's ids that the OS listed.
    pub candidates: u32,
    /// The HID report descriptor as spaced hex, when the OS gives it.
    pub report_descriptor: Option<String>,
}

/// Everything the Device test view shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DeviceDiagnostics {
    /// The data comes from `--simulate`'s simulated Speed Editor.
    pub simulated: bool,
    pub status: Option<ConnectionStatus>,
    pub device: Option<DeviceInfo>,
    pub auth: AuthDiagnostics,
    pub battery: Option<Battery>,
    /// Printed labels of the keys held down now.
    pub held: Vec<String>,
    /// Printed labels of the latest presses, newest first.
    pub presses: Vec<String>,
    pub wheel: WheelDiagnostics,
    /// Input reports seen per report id (`"0x04"`).
    #[cfg_attr(test, ts(type = "Record<string, number>"))]
    pub counts: BTreeMap<String, u64>,
    /// Input reports freewheel could not decode (unknown id, too short).
    #[cfg_attr(test, ts(type = "number"))]
    pub undecoded: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub open_attempts: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub connects: u64,
    #[cfg_attr(test, ts(type = "number"))]
    pub disconnects: u64,
    /// Why the device could not be opened, last time it could not.
    pub last_open_error: Option<String>,
    /// The latest raw reports, newest first.
    pub reports: Vec<RawReport>,
    #[cfg_attr(test, ts(type = "number"))]
    pub uptime_ms: u64,
}

impl Default for DeviceDiagnostics {
    fn default() -> Self {
        DeviceDiagnostics {
            simulated: false,
            status: None,
            device: None,
            auth: AuthDiagnostics {
                state: AuthState::NotYet,
                passed: 0,
                failed: 0,
                session_s: None,
                next_in_s: None,
                last_error: None,
                last_passed_ms: None,
            },
            battery: None,
            held: Vec::new(),
            presses: Vec::new(),
            wheel: WheelDiagnostics {
                device_mode: None,
                last_value: None,
                total: 0,
                reports: 0,
            },
            counts: BTreeMap::new(),
            undecoded: 0,
            open_attempts: 0,
            connects: 0,
            disconnects: 0,
            last_open_error: None,
            reports: Vec::new(),
            uptime_ms: 0,
        }
    }
}

/// Bytes as lowercase spaced hex: `04 0f 00`.
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Windows pads every input report to the longest one with zeros; the
/// view shows them without that padding (at least `min` bytes).
fn trimmed(report: &[u8], min: usize) -> &[u8] {
    let end = report.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    &report[..end.max(min).min(report.len())]
}

fn key_labels(keys: &[Key]) -> String {
    if keys.is_empty() {
        return "none".into();
    }
    keys.iter()
        .map(|&k| printed_label(k))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A short description of an input report from its decode result.
pub fn describe_input(
    report: &[u8],
    held_after: &[Key],
    parsed: &Result<Vec<DeviceEvent>, ReportError>,
) -> String {
    match (report.first(), parsed) {
        (_, Err(e)) => e.to_string(),
        (Some(0x04), Ok(_)) => format!("keys held: {}", key_labels(held_after)),
        (Some(0x03), Ok(events)) => match events.first() {
            Some(DeviceEvent::Jog { mode, value }) => {
                let kind = match mode {
                    0 | 2 => "relative",
                    1 | 3 => "absolute",
                    _ => "unknown mode",
                };
                format!("wheel mode {mode} ({kind}) value {value:+}")
            }
            _ => "wheel".into(),
        },
        (Some(0x07), Ok(events)) => match events.first() {
            Some(DeviceEvent::Battery { charging, level }) => {
                let charging = if *charging { ", charging" } else { "" };
                format!("battery {level}%{charging}")
            }
            _ => "battery".into(),
        },
        _ => "report".into(),
    }
}

/// A short description of an output report freewheel writes.
pub fn describe_output(report: &[u8]) -> String {
    match report {
        [0x02, m @ ..] if m.len() >= 4 => {
            let mask = u32::from_le_bytes([m[0], m[1], m[2], m[3]]);
            format!("key LEDs mask {mask:#07x}")
        }
        [0x03, mode, ..] => {
            let name = match mode {
                0 => "relative",
                1 => "absolute",
                2 => "relative 2",
                3 => "absolute, dead zero",
                _ => "unknown",
            };
            format!("set wheel mode {mode} ({name})")
        }
        [0x04, leds, ..] => format!("wheel LEDs {leds:#04x}"),
        [id, ..] => format!("output report {id:#04x}"),
        [] => "empty output report".into(),
    }
}

/// Records diagnostics as the connection manager runs; [`Self::snapshot`]
/// is what the view gets.
#[derive(Debug)]
pub struct Recorder {
    start: Instant,
    data: DeviceDiagnostics,
    seq: u64,
    changed: bool,
}

impl Recorder {
    pub fn new(start: Instant) -> Self {
        Recorder {
            start,
            data: DeviceDiagnostics::default(),
            seq: 0,
            changed: true,
        }
    }

    fn ms(&self, now: Instant) -> u64 {
        u64::try_from(now.saturating_duration_since(self.start).as_millis()).unwrap_or(u64::MAX)
    }

    fn touch(&mut self) {
        self.changed = true;
    }

    /// Whether anything changed since the last call.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    pub fn status(&mut self, status: ConnectionStatus) {
        self.data.status = Some(status);
        self.touch();
    }

    pub fn open_failed(&mut self, error: String) {
        self.data.open_attempts += 1;
        if self.data.last_open_error.as_deref() != Some(&error) {
            self.data.last_open_error = Some(error);
        }
        self.touch();
    }

    pub fn opened(&mut self, info: Option<DeviceInfo>) {
        self.data.open_attempts += 1;
        self.data.device = info;
        self.touch();
    }

    pub fn auth_passed(&mut self, session: Duration, now: Instant) {
        let at = self.ms(now);
        let auth = &mut self.data.auth;
        auth.state = AuthState::Passed;
        auth.passed += 1;
        auth.session_s = Some(u32::try_from(session.as_secs()).unwrap_or(u32::MAX));
        auth.last_error = None;
        auth.last_passed_ms = Some(at);
        self.touch();
    }

    pub fn auth_failed(&mut self, error: String) {
        let auth = &mut self.data.auth;
        auth.state = AuthState::Failed;
        auth.failed += 1;
        auth.last_error = Some(error);
        self.touch();
    }

    pub fn connected(&mut self) {
        self.data.connects += 1;
        self.touch();
    }

    pub fn disconnected(&mut self) {
        self.data.disconnects += 1;
        self.data.held.clear();
        self.data.battery = None;
        self.data.auth.state = AuthState::NotYet;
        self.touch();
    }

    fn push(&mut self, report: RawReport) {
        self.data.reports.insert(0, report);
        self.data.reports.truncate(MAX_REPORTS);
        self.touch();
    }

    /// One input report, its decode result and the keys held after it.
    pub fn input(
        &mut self,
        report: &[u8],
        parsed: &Result<Vec<DeviceEvent>, ReportError>,
        held_after: &[Key],
        now: Instant,
    ) {
        let id = report.first().copied().unwrap_or(0);
        let min = match id {
            0x03 => 7,
            0x04 => 13,
            0x07 => 3,
            _ => 1,
        };
        *self.data.counts.entry(format!("{id:#04x}")).or_default() += 1;
        match parsed {
            Err(_) => self.data.undecoded += 1,
            Ok(events) => {
                for event in events {
                    match event {
                        DeviceEvent::Press(k) => {
                            self.data.presses.insert(0, printed_label(*k).to_string());
                            self.data.presses.truncate(MAX_PRESSES);
                        }
                        DeviceEvent::Jog { mode, value } => {
                            let w = &mut self.data.wheel;
                            w.reports += 1;
                            w.total = if matches!(mode, 1 | 3) {
                                i64::from(*value)
                            } else {
                                w.total.saturating_add(i64::from(*value))
                            };
                            w.device_mode = Some(*mode);
                            w.last_value = Some(*value);
                        }
                        DeviceEvent::Battery { charging, level } => {
                            self.data.battery = Some(Battery {
                                level: *level,
                                charging: *charging,
                            });
                        }
                        _ => {}
                    }
                }
                if id == 0x04 {
                    self.data.held = held_after
                        .iter()
                        .map(|&k| printed_label(k).to_string())
                        .collect();
                }
            }
        }
        self.seq += 1;
        let raw = RawReport {
            seq: self.seq,
            at_ms: self.ms(now),
            direction: Direction::In,
            id,
            hex: hex(trimmed(report, min)),
            decoded: describe_input(report, held_after, parsed),
            failed: false,
        };
        self.push(raw);
    }

    /// One output report freewheel wrote, and whether the write failed.
    pub fn output(&mut self, report: &[u8], failed: bool, now: Instant) {
        self.seq += 1;
        let raw = RawReport {
            seq: self.seq,
            at_ms: self.ms(now),
            direction: Direction::Out,
            id: report.first().copied().unwrap_or(0),
            hex: hex(report),
            decoded: describe_output(report),
            failed,
        };
        self.push(raw);
    }

    /// The view's data; `next_auth` is when the next handshake is due.
    pub fn snapshot(&self, now: Instant, next_auth: Option<Instant>) -> DeviceDiagnostics {
        let mut d = self.data.clone();
        d.uptime_ms = self.ms(now);
        d.auth.next_in_s = next_auth.map(|t| t.saturating_duration_since(now).as_secs());
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::ReportParser;

    fn record(rec: &mut Recorder, parser: &mut ReportParser, report: &[u8], now: Instant) {
        let parsed = parser.parse(report);
        rec.input(report, &parsed, parser.held(), now);
    }

    #[test]
    fn hex_is_lowercase_and_spaced() {
        assert_eq!(hex(&[0x04, 0x0f, 0x00, 0xab]), "04 0f 00 ab");
        assert_eq!(hex(&[]), "");
    }

    #[test]
    fn windows_padding_is_trimmed_but_the_report_layout_stays() {
        let mut padded = vec![0x07, 0x01, 0x40];
        padded.resize(64, 0);
        assert_eq!(trimmed(&padded, 3), &[0x07, 0x01, 0x40]);
        let mut keys = vec![0x04, 0x0f];
        keys.resize(64, 0);
        assert_eq!(trimmed(&keys, 13).len(), 13);
    }

    #[test]
    fn inputs_fill_keys_wheel_battery_counts_and_the_raw_list() {
        let t0 = Instant::now();
        let mut rec = Recorder::new(t0);
        let mut p = ReportParser::default();
        let mut cut = vec![0x04, 0x0f, 0x00];
        cut.resize(13, 0);
        record(&mut rec, &mut p, &cut, t0 + Duration::from_millis(5));
        record(&mut rec, &mut p, &[0x03, 0, 2, 0, 0, 0, 0], t0);
        record(&mut rec, &mut p, &[0x03, 0, 0xfd, 0xff, 0xff, 0xff, 0], t0);
        record(&mut rec, &mut p, &[0x07, 1, 87], t0);
        record(&mut rec, &mut p, &[0x09, 1, 2], t0);
        let d = rec.snapshot(t0 + Duration::from_secs(1), None);
        assert_eq!(d.held, ["CUT"]);
        assert_eq!(d.presses, ["CUT"]);
        assert_eq!(d.wheel.total, -1);
        assert_eq!(d.wheel.reports, 2);
        assert_eq!(d.wheel.device_mode, Some(0));
        assert_eq!(
            d.battery,
            Some(Battery {
                level: 87,
                charging: true
            })
        );
        assert_eq!(d.undecoded, 1);
        assert_eq!(d.counts["0x04"], 1);
        assert_eq!(d.counts["0x03"], 2);
        assert_eq!(d.counts["0x09"], 1);
        assert_eq!(d.reports.len(), 5);
        assert_eq!(d.reports[0].decoded, "unknown report id 0x09");
        assert_eq!(d.reports[1].decoded, "battery 87%, charging");
        assert_eq!(d.reports[2].decoded, "wheel mode 0 (relative) value -3");
        assert_eq!(d.reports[4].decoded, "keys held: CUT");
        assert_eq!(d.reports[4].hex, "04 0f 00 00 00 00 00 00 00 00 00 00 00");
        assert_eq!(d.reports[4].at_ms, 5);
        assert_eq!(d.uptime_ms, 1000);
    }

    #[test]
    fn absolute_wheel_reports_show_the_position() {
        let t0 = Instant::now();
        let mut rec = Recorder::new(t0);
        let mut p = ReportParser::default();
        let mut r = vec![0x03, 3];
        r.extend_from_slice(&(-900i32).to_le_bytes());
        r.push(0);
        record(&mut rec, &mut p, &r, t0);
        record(&mut rec, &mut p, &r, t0);
        let d = rec.snapshot(t0, None);
        assert_eq!(d.wheel.total, -900);
        assert_eq!(d.reports[0].decoded, "wheel mode 3 (absolute) value -900");
    }

    #[test]
    fn the_raw_list_keeps_only_the_newest() {
        let t0 = Instant::now();
        let mut rec = Recorder::new(t0);
        let mut p = ReportParser::default();
        for level in 0..(MAX_REPORTS as u8 + 5) {
            record(&mut rec, &mut p, &[0x07, 0, level], t0);
        }
        let d = rec.snapshot(t0, None);
        assert_eq!(d.reports.len(), MAX_REPORTS);
        assert_eq!(d.reports[0].hex, format!("07 00 {:02x}", MAX_REPORTS + 4));
        assert_eq!(d.reports[0].seq, MAX_REPORTS as u64 + 5);
    }

    #[test]
    fn outputs_are_described() {
        assert_eq!(
            describe_output(&[0x03, 0, 0, 0, 0, 0, 0xff]),
            "set wheel mode 0 (relative)"
        );
        assert_eq!(
            describe_output(&[0x03, 3, 0, 0, 0, 0, 0xff]),
            "set wheel mode 3 (absolute, dead zero)"
        );
        assert_eq!(
            describe_output(&[0x02, 1, 0, 0, 0]),
            "key LEDs mask 0x00001"
        );
        assert_eq!(describe_output(&[0x04, 2]), "wheel LEDs 0x02");
        let t0 = Instant::now();
        let mut rec = Recorder::new(t0);
        rec.output(&[0x03, 0, 0, 0, 0, 0, 0xff], true, t0);
        let d = rec.snapshot(t0, None);
        assert_eq!(d.reports[0].direction, Direction::Out);
        assert!(d.reports[0].failed);
    }

    #[test]
    fn auth_and_connection_counters() {
        let t0 = Instant::now();
        let mut rec = Recorder::new(t0);
        rec.open_failed("no device".into());
        rec.opened(None);
        rec.auth_failed("status report 06 05".into());
        rec.opened(None);
        rec.auth_passed(Duration::from_secs(600), t0 + Duration::from_secs(2));
        rec.connected();
        let d = rec.snapshot(
            t0 + Duration::from_secs(3),
            Some(t0 + Duration::from_secs(12)),
        );
        assert_eq!(d.open_attempts, 3);
        assert_eq!(d.last_open_error.as_deref(), Some("no device"));
        assert_eq!(d.auth.state, AuthState::Passed);
        assert_eq!((d.auth.passed, d.auth.failed), (1, 1));
        assert_eq!(d.auth.session_s, Some(600));
        assert_eq!(d.auth.next_in_s, Some(9));
        assert_eq!(d.auth.last_passed_ms, Some(2000));
        assert_eq!(d.auth.last_error, None);
        rec.disconnected();
        let d = rec.snapshot(t0, None);
        assert_eq!(d.disconnects, 1);
        assert_eq!(d.auth.state, AuthState::NotYet);
    }
}
