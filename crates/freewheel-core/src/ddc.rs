//! Brightness of external displays over DDC/CI (VESA MCCS VCP 0x10), the way
//! MonitorControl does it on macOS: read the current and maximum value, then
//! write the stepped value. Each display is stepped on its own, so one that
//! does not answer reports its own [`DisplayError`] and the others still
//! step (A16). The OS I2C transport is a [`DdcDisplay`] in the app crate.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::{debug, warn};

use crate::overrides::{BrightnessControl, ControlError};

/// The MCCS VCP code of luminance (brightness).
pub const VCP_BRIGHTNESS: u8 = 0x10;
/// The I2C address of DDC/CI (7-bit; 0x6E/0x6F as 8-bit write/read).
pub const DDC_ADDRESS: u8 = 0x37;
/// The host's source address, the first byte of every packet.
pub const HOST_ADDRESS: u8 = 0x51;
/// Length of a Get VCP Feature reply, starting with the display's address.
pub const REPLY_LEN: usize = 11;

/// XOR checksum over `seed` and `bytes`.
fn checksum(seed: u8, bytes: &[u8]) -> u8 {
    bytes.iter().fold(seed, |c, b| c ^ b)
}

/// A Set VCP Feature packet, source byte first.
pub fn set_vcp_packet(vcp: u8, value: u16) -> [u8; 7] {
    let [hi, lo] = value.to_be_bytes();
    let mut p = [HOST_ADDRESS, 0x84, 0x03, vcp, hi, lo, 0];
    p[6] = checksum(0x6E, &p[..6]);
    p
}

/// A Get VCP Feature request packet, source byte first.
pub fn get_vcp_packet(vcp: u8) -> [u8; 5] {
    let mut p = [HOST_ADDRESS, 0x82, 0x01, vcp, 0];
    p[4] = checksum(0x6E, &p[..4]);
    p
}

/// Parses a Get VCP Feature reply into (current, maximum).
pub fn parse_vcp_reply(vcp: u8, r: &[u8]) -> Result<(u16, u16), ControlError> {
    let bad = |why: &str| Err(ControlError(format!("bad DDC reply ({why}): {r:02x?}")));
    if r.len() < REPLY_LEN {
        return bad("short");
    }
    if checksum(0x50, &r[..REPLY_LEN - 1]) != r[REPLY_LEN - 1] {
        return bad("checksum");
    }
    if r[2] != 0x02 || r[4] != vcp {
        return bad("not a VCP reply");
    }
    if r[3] != 0 {
        return bad("unsupported VCP code");
    }
    let max = u16::from_be_bytes([r[6], r[7]]);
    let cur = u16::from_be_bytes([r[8], r[9]]);
    if max == 0 {
        return bad("maximum is 0");
    }
    Ok((cur, max))
}

/// One external display reached over its DDC/CI I2C bus.
pub trait DdcDisplay {
    /// A name for errors and logs.
    fn name(&self) -> String;
    /// Sends `packet` (source byte first).
    fn write(&mut self, packet: &[u8]) -> Result<(), ControlError>;
    /// Sends `request` and reads the reply into `reply`.
    fn read(&mut self, request: &[u8], reply: &mut [u8]) -> Result<(), ControlError>;
}

/// The error of one display, for the override-errors status event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayError {
    pub display: String,
    pub error: ControlError,
}

/// Moves the brightness of `d` by `delta` percent of its maximum, clamped.
fn step_one(d: &mut dyn DdcDisplay, delta: i32) -> Result<(), ControlError> {
    let mut reply = [0u8; REPLY_LEN];
    d.read(&get_vcp_packet(VCP_BRIGHTNESS), &mut reply)?;
    let (cur, max) = parse_vcp_reply(VCP_BRIGHTNESS, &reply)?;
    let by = (i32::from(max) * delta.abs() + 50) / 100;
    let by = by.max(1) * delta.signum();
    let value = (i32::from(cur) + by).clamp(0, i32::from(max)) as u16;
    debug!(display = %d.name(), cur, max, value, "DDC brightness write");
    d.write(&set_vcp_packet(VCP_BRIGHTNESS, value))
}

/// Steps every display by `delta` percent; returns the displays that failed.
pub fn step_displays(displays: &mut [Box<dyn DdcDisplay>], delta: i32) -> Vec<DisplayError> {
    let mut errors = Vec::new();
    for d in displays.iter_mut() {
        if let Err(error) = step_one(d.as_mut(), delta) {
            warn!(display = %d.name(), error = %error, "DDC brightness failed");
            errors.push(DisplayError {
                display: d.name(),
                error,
            });
        }
    }
    errors
}

/// Finds the external displays; an error means DDC is unavailable here.
pub type Probe = Box<dyn FnMut() -> Result<Vec<Box<dyn DdcDisplay>>, ControlError> + Send>;

/// [`BrightnessControl`] over DDC/CI. Probes the displays on every step, so
/// plugged and unplugged displays are picked up; a failed probe clears the
/// shared [`available`](Self::available) flag, which hides the path in
/// settings.
pub struct DdcBrightness {
    probe: Probe,
    available: Arc<AtomicBool>,
}

impl DdcBrightness {
    pub fn new(probe: Probe) -> Self {
        Self {
            probe,
            available: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Whether DDC brightness works on this Mac, as last probed.
    pub fn available(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.available)
    }

    fn step(&mut self, delta: i32) -> Result<(), ControlError> {
        let mut displays = match (self.probe)() {
            Ok(d) => d,
            Err(e) => {
                warn!(error = %e, "DDC probe failed: external display brightness unavailable");
                self.available.store(false, Ordering::Relaxed);
                return Err(e);
            }
        };
        self.available.store(true, Ordering::Relaxed);
        debug!(displays = displays.len(), "DDC probe");
        let errors = step_displays(&mut displays, delta);
        if errors.is_empty() {
            return Ok(());
        }
        let parts: Vec<String> = errors
            .iter()
            .map(|e| format!("{}: {}", e.display, e.error.0))
            .collect();
        Err(ControlError(parts.join("; ")))
    }
}

impl BrightnessControl for DdcBrightness {
    fn step_up(&mut self, step: u8) -> Result<(), ControlError> {
        self.step(step.into())
    }
    fn step_down(&mut self, step: u8) -> Result<(), ControlError> {
        self.step(-i32::from(step))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A fake monitor: brightness (cur, max), or `None` when it never answers.
    /// Shared so the test reads it back after handing it over.
    #[derive(Clone)]
    struct Monitor(&'static str, Arc<Mutex<Option<(u16, u16)>>>);

    impl Monitor {
        fn new(name: &'static str, state: Option<(u16, u16)>) -> Self {
            Self(name, Arc::new(Mutex::new(state)))
        }
        fn brightness(&self) -> Option<u16> {
            self.1.lock().unwrap().map(|(c, _)| c)
        }
    }

    impl DdcDisplay for Monitor {
        fn name(&self) -> String {
            self.0.into()
        }
        fn write(&mut self, p: &[u8]) -> Result<(), ControlError> {
            assert_eq!(p[..4], [0x51, 0x84, 0x03, VCP_BRIGHTNESS]);
            assert_eq!(checksum(0x6E, p), 0, "set packet checksum");
            let mut s = self.1.lock().unwrap();
            let (cur, _) = s.as_mut().ok_or(ControlError("no answer".into()))?;
            *cur = u16::from_be_bytes([p[4], p[5]]);
            Ok(())
        }
        fn read(&mut self, req: &[u8], reply: &mut [u8]) -> Result<(), ControlError> {
            assert_eq!(req, get_vcp_packet(VCP_BRIGHTNESS));
            let (cur, max) = self
                .1
                .lock()
                .unwrap()
                .ok_or(ControlError("no answer".into()))?;
            let [mh, ml] = max.to_be_bytes();
            let [ch, cl] = cur.to_be_bytes();
            let mut r = [
                0x6E,
                0x88,
                0x02,
                0x00,
                VCP_BRIGHTNESS,
                0x00,
                mh,
                ml,
                ch,
                cl,
                0,
            ];
            r[10] = checksum(0x50, &r[..10]);
            reply.copy_from_slice(&r);
            Ok(())
        }
    }

    fn boxed(ms: &[Monitor]) -> Vec<Box<dyn DdcDisplay>> {
        ms.iter()
            .map(|m| Box::new(m.clone()) as Box<dyn DdcDisplay>)
            .collect()
    }

    #[test]
    fn packets_follow_ddc_ci_with_their_checksums() {
        // Set brightness to 50: checksum 0x6E^0x51^0x84^0x03^0x10^0x00^0x32.
        assert_eq!(
            set_vcp_packet(0x10, 50),
            [0x51, 0x84, 0x03, 0x10, 0x00, 0x32, 0x9A]
        );
        assert_eq!(get_vcp_packet(0x10), [0x51, 0x82, 0x01, 0x10, 0xAC]);
    }

    #[test]
    fn replies_parse_and_bad_ones_are_errors() {
        let mut r = [
            0x6E, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x64, 0x00, 0x28, 0,
        ];
        r[10] = checksum(0x50, &r[..10]);
        assert_eq!(parse_vcp_reply(0x10, &r), Ok((40, 100)));
        assert!(parse_vcp_reply(0x10, &r[..10]).is_err());
        assert!(parse_vcp_reply(0x12, &r).is_err());
        let mut corrupt = r;
        corrupt[9] = 0x29;
        let e = parse_vcp_reply(0x10, &corrupt).unwrap_err();
        assert!(e.0.contains("checksum"), "{e}");
    }

    #[test]
    fn steps_each_display_by_percent_of_its_max_clamped() {
        let a = Monitor::new("A", Some((50, 100)));
        let b = Monitor::new("B", Some((10, 200)));
        let mut ds = boxed(&[a.clone(), b.clone()]);
        assert_eq!(step_displays(&mut ds, 5), []);
        assert_eq!((a.brightness(), b.brightness()), (Some(55), Some(20)));
        assert_eq!(step_displays(&mut ds, -10), []);
        assert_eq!((a.brightness(), b.brightness()), (Some(45), Some(0)));
        assert_eq!(step_displays(&mut ds, 100), []);
        assert_eq!((a.brightness(), b.brightness()), (Some(100), Some(200)));
    }

    #[test]
    fn a_display_that_does_not_answer_has_its_own_error_and_others_still_step() {
        let a = Monitor::new("A", Some((50, 100)));
        let dead = Monitor::new("Dead", None);
        let c = Monitor::new("C", Some((50, 100)));
        let mut ds = boxed(&[a.clone(), dead, c.clone()]);
        assert_eq!(
            step_displays(&mut ds, 5),
            [DisplayError {
                display: "Dead".into(),
                error: ControlError("no answer".into()),
            }]
        );
        assert_eq!((a.brightness(), c.brightness()), (Some(55), Some(55)));
    }

    #[test]
    fn brightness_control_names_each_failed_display_in_its_error() {
        let a = Monitor::new("A", Some((50, 100)));
        let ms = [a.clone(), Monitor::new("Dead", None)];
        let mut ddc = DdcBrightness::new(Box::new(move || Ok(boxed(&ms))));
        let e = ddc.step_down(5).unwrap_err();
        assert_eq!(e, ControlError("Dead: no answer".into()));
        assert_eq!(a.brightness(), Some(45));
        assert!(ddc.available().load(Ordering::Relaxed));
    }

    #[test]
    fn a_failed_probe_reports_unavailable_until_a_probe_works() {
        let works = Arc::new(AtomicBool::new(false));
        let w = Arc::clone(&works);
        let mut ddc = DdcBrightness::new(Box::new(move || {
            if w.load(Ordering::Relaxed) {
                Ok(Vec::new())
            } else {
                Err(ControlError("IOAVService not found".into()))
            }
        }));
        let available = ddc.available();
        assert_eq!(
            ddc.step_up(5),
            Err(ControlError("IOAVService not found".into()))
        );
        assert!(!available.load(Ordering::Relaxed));
        works.store(true, Ordering::Relaxed);
        assert_eq!(ddc.step_up(5), Ok(()));
        assert!(available.load(Ordering::Relaxed));
    }
}
