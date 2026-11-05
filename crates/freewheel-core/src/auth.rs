//! The feature-report-6 auth handshake and re-auth scheduling.
//!
//! Handshake order and report layouts follow smunaut/blackmagic-misc
//! `bmd.py` (Apache-2.0, credited in NOTICE). The response algorithm is
//! passed in as a function so this module only drives the protocol;
//! [`compute_response`] is that function, ported from `bmd_kbd_auth` in
//! <https://github.com/smunaut/blackmagic-misc/blob/c0e71caa1b544026ef7f422b48a1b66b0288d91f/bmd.py>

use std::fmt;
use std::time::{Duration, Instant};

use crate::hid::{HidError, HidTransport};

const AUTH_EVEN_TBL: [u64; 8] = [
    0x3ae1206f97c10bc8,
    0x2a9ab32bebf244c6,
    0x20a6f8b8df9adf0a,
    0xaf80ece52cfc1719,
    0xec2ee2f7414fd151,
    0xb055adfd73344a15,
    0xa63d2e3059001187,
    0x751bf623f42e0dde,
];

const AUTH_ODD_TBL: [u64; 8] = [
    0x3e22b34f502e7fde,
    0x24656b981875ab1c,
    0xa17f3456df7bf8c3,
    0x6df72e1941aef698,
    0x72226f011e66ab94,
    0x3831a3c606296b42,
    0xfd7ff81881332c89,
    0x61a3f6474ff236c6,
];

const MASK: u64 = 0xa79a63f585d37bf0;

/// bmd.py's `rol8`: rotates by one byte (the low byte moves to the top).
fn rol8(v: u64) -> u64 {
    v.rotate_right(8)
}

/// Computes the response to a device challenge (bmd.py `bmd_kbd_auth`).
pub fn compute_response(challenge: u64) -> u64 {
    let n = (challenge & 7) as usize;
    let mut v = challenge.rotate_right(8 * n as u32);
    let k = if (v & 1) == ((0x78 >> n) & 1) {
        AUTH_EVEN_TBL[n]
    } else {
        v ^= rol8(v);
        AUTH_ODD_TBL[n]
    };
    v ^ (rol8(v) & MASK) ^ k
}

const REPORT_ID: u8 = 0x06;
const REPORT_LEN: usize = 10;
/// The longest session we trust the device to report (ADR-003 default).
const MAX_TIMEOUT: Duration = Duration::from_secs(600);
/// How long before the session timeout re-auth is due.
const REAUTH_MARGIN: Duration = Duration::from_secs(10);

/// A source of the current time, injectable for tests.
pub trait Clock {
    fn now(&self) -> Instant;
}

/// A failed handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// The transport failed.
    Hid(HidError),
    /// A report read during the handshake had an unexpected header.
    UnexpectedReport { expected: [u8; 2], got: Vec<u8> },
    /// The status report was malformed or rejected our response.
    BadStatus(Vec<u8>),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::Hid(e) => write!(f, "auth transport error: {e}"),
            AuthError::UnexpectedReport { expected, got } => {
                write!(f, "auth expected report {expected:02x?}, got {got:02x?}")
            }
            AuthError::BadStatus(got) => write!(f, "auth rejected, status {got:02x?}"),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<HidError> for AuthError {
    fn from(e: HidError) -> Self {
        AuthError::Hid(e)
    }
}

fn read_report(t: &mut dyn HidTransport, sub: u8) -> Result<[u8; REPORT_LEN], AuthError> {
    let mut buf = [0u8; REPORT_LEN];
    buf[0] = REPORT_ID;
    let n = t.get_feature_report(&mut buf)?;
    if n < REPORT_LEN || buf[..2] != [REPORT_ID, sub] {
        let got = buf[..n].to_vec();
        return Err(if sub == 4 {
            AuthError::BadStatus(got)
        } else {
            AuthError::UnexpectedReport {
                expected: [REPORT_ID, sub],
                got,
            }
        });
    }
    Ok(buf)
}

fn send_report(t: &mut dyn HidTransport, sub: u8, payload: u64) -> Result<(), AuthError> {
    let mut buf = [0u8; REPORT_LEN];
    buf[0] = REPORT_ID;
    buf[1] = sub;
    buf[2..].copy_from_slice(&payload.to_le_bytes());
    Ok(t.send_feature_report(&buf)?)
}

/// Runs the handshake: reset, read challenge, send our challenge, read the
/// device's reply, send our response, read status. `respond` maps the
/// device challenge to our response. Returns the session timeout.
pub fn handshake(
    t: &mut dyn HidTransport,
    respond: impl Fn(u64) -> u64,
) -> Result<Duration, AuthError> {
    send_report(t, 0, 0)?;
    let challenge = read_report(t, 0)?;
    let challenge = u64::from_le_bytes(challenge[2..].try_into().expect("8 bytes"));
    // We do not check the device, so our challenge is zero, as in bmd.py.
    send_report(t, 1, 0)?;
    read_report(t, 2)?;
    send_report(t, 3, respond(challenge))?;
    let status = read_report(t, 4)?;
    let timeout = Duration::from_secs(u16::from_le_bytes([status[2], status[3]]).into());
    if timeout <= REAUTH_MARGIN {
        return Err(AuthError::BadStatus(status.to_vec()));
    }
    Ok(timeout)
}

/// Keeps a device authenticated: re-runs the handshake at
/// min(timeout, 600 s) minus 10 s.
pub struct Authenticator<C, F> {
    clock: C,
    respond: F,
    reauth_due: Option<Instant>,
}

impl<C: Clock, F: Fn(u64) -> u64> Authenticator<C, F> {
    pub fn new(clock: C, respond: F) -> Self {
        Self {
            clock,
            respond,
            reauth_due: None,
        }
    }

    /// When re-auth is next due; `None` before the first success.
    pub fn reauth_due(&self) -> Option<Instant> {
        self.reauth_due
    }

    /// Authenticates now and schedules the next re-auth.
    pub fn authenticate(&mut self, t: &mut dyn HidTransport) -> Result<(), AuthError> {
        self.reauth_due = None;
        let timeout = handshake(t, &self.respond)?;
        self.reauth_due = Some(self.clock.now() + timeout.min(MAX_TIMEOUT) - REAUTH_MARGIN);
        Ok(())
    }

    /// Re-authenticates if it is due. A failed re-auth means the device is
    /// gone, so it is reported as [`HidError::Disconnected`].
    pub fn maintain(&mut self, t: &mut dyn HidTransport) -> Result<(), HidError> {
        match self.reauth_due {
            Some(due) if self.clock.now() < due => Ok(()),
            _ => self.authenticate(t).map_err(|_| HidError::Disconnected),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::{FakeTransport, Written};
    use std::cell::Cell;
    use std::rc::Rc;

    const VECTORS: &str = include_str!("auth_vectors.txt");

    fn hex(s: &str) -> u64 {
        u64::from_str_radix(s.trim_start_matches("0x"), 16).expect("hex u64")
    }

    #[test]
    fn matches_every_recorded_bmd_py_vector() {
        let pairs: Vec<(u64, u64)> = VECTORS
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .map(|l| {
                let mut f = l.split_whitespace();
                (hex(f.next().unwrap()), hex(f.next().unwrap()))
            })
            .collect();
        assert_eq!(pairs.len(), 8);
        for (challenge, response) in pairs {
            assert_eq!(
                compute_response(challenge),
                response,
                "challenge {challenge:#018x}"
            );
        }
    }

    #[test]
    fn vectors_file_records_its_provenance() {
        for needle in [
            "c0e71caa1b544026ef7f422b48a1b66b0288d91f",
            "963d2d3c278b9df2b5f3f1ac369038a5c062d2db1d8b2a167f7d4ffaec89ddac",
            "sed -n 134,177p bmd.py",
            "python3 -I gen_vectors.py",
        ] {
            assert!(VECTORS.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn notice_credits_smunaut() {
        let notice = include_str!("../../../NOTICE");
        assert!(notice.contains("smunaut/blackmagic-misc"));
        assert!(notice.contains("Apache License, Version 2.0"));
    }

    #[derive(Clone)]
    struct FakeClock(Rc<Cell<Instant>>);

    impl FakeClock {
        fn new() -> Self {
            Self(Rc::new(Cell::new(Instant::now())))
        }
        fn advance(&self, d: Duration) {
            self.0.set(self.0.get() + d);
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Instant {
            self.0.get()
        }
    }

    fn report(sub: u8, payload: u64) -> Vec<u8> {
        let mut r = vec![REPORT_ID, sub];
        r.extend_from_slice(&payload.to_le_bytes());
        r
    }

    fn script(fake: &mut FakeTransport, timeout_s: u16) {
        fake.push_feature(report(0, 0x1122_3344_5566_7788))
            .push_feature(report(2, 0xdead))
            .push_feature(report(4, timeout_s.into()));
    }

    fn respond(c: u64) -> u64 {
        !c
    }

    #[test]
    fn handshake_writes_in_order() {
        let mut fake = FakeTransport::new();
        script(&mut fake, 600);
        assert_eq!(handshake(&mut fake, respond), Ok(Duration::from_secs(600)));
        assert_eq!(
            fake.written(),
            &[
                Written::Feature(report(0, 0)),
                Written::Feature(report(1, 0)),
                Written::Feature(report(3, !0x1122_3344_5566_7788)),
            ]
        );
        // All three scripted reads (challenge, reply, status) were consumed.
        let mut buf = [REPORT_ID; REPORT_LEN];
        assert_eq!(
            fake.get_feature_report(&mut buf),
            Err(HidError::Disconnected)
        );
    }

    #[test]
    fn reauth_is_due_at_timeout_minus_10s() {
        let clock = FakeClock::new();
        let start = clock.now();
        let mut auth = Authenticator::new(clock.clone(), respond);
        let mut fake = FakeTransport::new();
        script(&mut fake, 300);
        auth.authenticate(&mut fake).unwrap();
        assert_eq!(auth.reauth_due(), Some(start + Duration::from_secs(290)));

        // Not due yet: nothing is written.
        clock.advance(Duration::from_secs(289));
        assert_eq!(auth.maintain(&mut fake), Ok(()));
        assert_eq!(fake.written().len(), 3);

        // Due: the handshake runs again and reschedules.
        clock.advance(Duration::from_secs(1));
        script(&mut fake, 300);
        assert_eq!(auth.maintain(&mut fake), Ok(()));
        assert_eq!(fake.written().len(), 6);
        assert_eq!(auth.reauth_due(), Some(start + Duration::from_secs(580)));
    }

    #[test]
    fn reauth_timeout_is_capped_at_600s() {
        let clock = FakeClock::new();
        let start = clock.now();
        let mut auth = Authenticator::new(clock, respond);
        let mut fake = FakeTransport::new();
        script(&mut fake, 3600);
        auth.authenticate(&mut fake).unwrap();
        assert_eq!(auth.reauth_due(), Some(start + Duration::from_secs(590)));
    }

    #[test]
    fn bad_status_is_a_typed_error() {
        let mut fake = FakeTransport::new();
        fake.push_feature(report(0, 1))
            .push_feature(report(2, 0))
            .push_feature(report(5, 600));
        assert_eq!(
            handshake(&mut fake, respond),
            Err(AuthError::BadStatus(report(5, 600)))
        );
    }

    #[test]
    fn status_timeout_too_short_is_a_bad_status() {
        let mut fake = FakeTransport::new();
        script(&mut fake, 10);
        assert_eq!(
            handshake(&mut fake, respond),
            Err(AuthError::BadStatus(report(4, 10)))
        );
    }

    #[test]
    fn unexpected_challenge_header_is_a_typed_error() {
        let mut fake = FakeTransport::new();
        fake.push_feature(report(2, 0));
        assert_eq!(
            handshake(&mut fake, respond),
            Err(AuthError::UnexpectedReport {
                expected: [REPORT_ID, 0],
                got: report(2, 0)
            })
        );
    }

    #[test]
    fn reauth_failure_is_reported_as_disconnect() {
        let clock = FakeClock::new();
        let mut auth = Authenticator::new(clock.clone(), respond);
        let mut fake = FakeTransport::new();
        script(&mut fake, 600);
        auth.authenticate(&mut fake).unwrap();

        clock.advance(Duration::from_secs(590));
        fake.push_feature(report(0, 1))
            .push_feature(report(2, 0))
            .push_feature(report(4, 0));
        assert_eq!(auth.maintain(&mut fake), Err(HidError::Disconnected));
        assert_eq!(auth.reauth_due(), None);
    }
}
