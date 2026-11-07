//! Replay tests: representative Speed Editor byte sequences for every
//! report type, and the full connect -> auth -> keys -> jog -> battery ->
//! disconnect -> reconnect cycle, through the real connection manager,
//! report parser and engine on the fake transport.
//!
//! The byte layouts are the public reverse-engineered ones; there is no
//! capture from freewheel's own hardware in the repo yet (the live device
//! test, docs/device-testing.md, asks for one):
//! - smunaut/blackmagic-misc `bmd.py` (Apache-2.0): VID 0x1edb / PID 0xda0e,
//!   feature report 6 auth (reset, challenge, our challenge, reply, our
//!   response, status with the session timeout in bytes 2..4), input 0x04
//!   (6 LE16 held keycodes), 0x03 (mode u8, LE i32 value, u8), 0x07
//!   (charging u8, level u8), output 0x02 / 0x03 / 0x04.
//! - JamesBalazs/speed-editor-client (Go), puzzleemptym/Unbound-editor-device-customizer
//!   (Python, USB and Bluetooth) and John-Howard/PythonSpeedEditor: the same
//!   ids, handshake and layouts; re-auth at timeout - 10 s.
//! - libusb/hidapi (Windows backend): input reports come back padded with
//!   zeros to the device's longest input report, so every layout is also
//!   replayed padded.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use freewheel_core::auth::compute_response;
use freewheel_core::clock::Clock;
use freewheel_core::config::{Action, Config, JogMode, KeyCombo};
use freewheel_core::connection::{
    ConnectionEvent, ConnectionManager, ConnectionStatus, IDLE_REAUTH, RETRY_INTERVAL,
};
use freewheel_core::diagnostics::{AuthState, DeviceInfo, Direction};
use freewheel_core::engine::Engine;
use freewheel_core::hid::{FakeTransport, HidError, HidTransport, Written};
use freewheel_core::inject::{InjectCall, Keystroke, MockInjector};
use freewheel_core::launch::MockLauncher;
use freewheel_core::report::{
    DeviceEvent, DeviceJogMode, Key, ReportError, ReportParser, set_jog_mode,
};
use freewheel_core::simulator::Bus;

use ConnectionEvent::{Device as Dev, Status};
use ConnectionStatus::{Absent, Connected};

const T: Duration = Duration::from_millis(10);
const VECTORS: &str = include_str!("../src/auth_vectors.txt");

/// The first challenge -> response pair recorded from bmd.py.
fn vector() -> (u64, u64) {
    let line = VECTORS
        .lines()
        .find(|l| l.starts_with("0x"))
        .expect("a vector");
    let mut f = line
        .split_whitespace()
        .map(|h| u64::from_str_radix(h.trim_start_matches("0x"), 16).unwrap());
    (f.next().unwrap(), f.next().unwrap())
}

fn feature(sub: u8, payload: u64) -> Vec<u8> {
    let mut r = vec![0x06, sub];
    r.extend_from_slice(&payload.to_le_bytes());
    r
}

/// A key report holding `codes` (at most 6).
fn keys(codes: &[u16]) -> Vec<u8> {
    let mut r = vec![0x04];
    for i in 0..6 {
        r.extend_from_slice(&codes.get(i).copied().unwrap_or(0).to_le_bytes());
    }
    r
}

fn wheel(mode: u8, value: i32) -> Vec<u8> {
    let mut r = vec![0x03, mode];
    r.extend_from_slice(&value.to_le_bytes());
    r.push(0);
    r
}

/// As hidapi on Windows returns it: zero-padded to `len`.
fn padded(mut r: Vec<u8>, len: usize) -> Vec<u8> {
    r.resize(len, 0);
    r
}

#[derive(Clone)]
struct TestClock(Arc<Mutex<Instant>>);

impl TestClock {
    fn advance(&self, d: Duration) {
        *self.0.lock().unwrap() += d;
    }
}

impl Clock for TestClock {
    fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }
}

/// A fake Speed Editor the test keeps a handle to.
#[derive(Clone, Default)]
struct Device(Arc<Mutex<FakeTransport>>);

impl Device {
    /// A device that passes one handshake with `challenge` and `session_s`.
    fn plugged(challenge: u64, session_s: u16, bus: Bus) -> Self {
        let d = Device::default();
        d.handshake(challenge, session_s);
        d.0.lock().unwrap().set_info(DeviceInfo {
            bus: Some(bus),
            vendor_id: 0x1edb,
            product_id: 0xda0e,
            ..DeviceInfo::default()
        });
        d
    }
    fn handshake(&self, challenge: u64, session_s: u16) {
        let mut f = self.0.lock().unwrap();
        f.push_feature(feature(0, challenge));
        f.push_feature(feature(2, 0x0bad_f00d));
        f.push_feature(feature(4, session_s.into()));
    }
    fn send(&self, report: Vec<u8>) {
        self.0.lock().unwrap().push_input(report);
    }
    fn written(&self) -> Vec<Written> {
        self.0.lock().unwrap().written().to_vec()
    }
}

impl HidTransport for Device {
    fn read_timeout(&mut self, buf: &mut [u8], t: Duration) -> Result<usize, HidError> {
        self.0.lock().unwrap().read_timeout(buf, t)
    }
    fn write(&mut self, data: &[u8]) -> Result<usize, HidError> {
        self.0.lock().unwrap().write(data)
    }
    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize, HidError> {
        self.0.lock().unwrap().get_feature_report(buf)
    }
    fn send_feature_report(&mut self, data: &[u8]) -> Result<(), HidError> {
        self.0.lock().unwrap().send_feature_report(data)
    }
    fn close(&mut self) {
        self.0.lock().unwrap().close()
    }
    fn info(&self) -> Option<DeviceInfo> {
        self.0.lock().unwrap().info()
    }
}

type Plug = Arc<Mutex<Option<Device>>>;
type Opener = Box<dyn FnMut() -> Result<Device, HidError>>;
type Manager = ConnectionManager<Device, Opener, TestClock, fn(u64) -> u64>;

/// The manager as the app builds it: the real auth response and the idle check.
fn manager(plug: &Plug, clock: &TestClock) -> Manager {
    let slot = Arc::clone(plug);
    let open: Opener = Box::new(move || {
        slot.lock()
            .unwrap()
            .take()
            .ok_or(HidError::Io("no Speed Editor".into()))
    });
    ConnectionManager::new(open, clock.clone(), compute_response as fn(u64) -> u64)
        .with_idle_reauth(IDLE_REAUTH)
}

fn setup() -> (Plug, TestClock) {
    (
        Arc::default(),
        TestClock(Arc::new(Mutex::new(Instant::now()))),
    )
}

fn device_events(events: Vec<ConnectionEvent>) -> Vec<DeviceEvent> {
    events
        .into_iter()
        .filter_map(|e| match e {
            Dev(d) => Some(d),
            Status(_) => None,
        })
        .collect()
}

#[test]
fn every_key_report_layout_decodes_plain_and_windows_padded() {
    for pad in [13, 64] {
        let mut p = ReportParser::default();
        for &key in Key::ALL {
            let down = padded(keys(&[key as u16]), pad);
            assert_eq!(p.parse(&down), Ok(vec![DeviceEvent::Press(key)]), "{key:?}");
            assert_eq!(
                p.parse(&padded(keys(&[]), pad)),
                Ok(vec![DeviceEvent::Release(key)])
            );
        }
        // Six at once (the report's limit), then a roll-over swap.
        let six = [0x07, 0x08, 0x0f, 0x10, 0x11, 0x3c];
        assert_eq!(p.parse(&padded(keys(&six), pad)).unwrap().len(), 6);
        assert_eq!(
            p.parse(&padded(keys(&[0x08, 0x0f, 0x10, 0x11, 0x3c, 0x33]), pad)),
            Ok(vec![
                DeviceEvent::Release(Key::In),
                DeviceEvent::Press(Key::Cam1)
            ])
        );
    }
}

#[test]
fn wheel_reports_decode_in_every_device_mode() {
    let mut p = ReportParser::default();
    for (mode, value) in [
        (0u8, 1),
        (0, -1),
        (0, 37),
        (1, 4096),
        (1, -4096),
        (2, -12),
        (3, 0),
        (3, 1500),
    ] {
        for r in [wheel(mode, value), padded(wheel(mode, value), 64)] {
            assert_eq!(p.parse(&r), Ok(vec![DeviceEvent::Jog { mode, value }]));
        }
    }
}

#[test]
fn battery_reports_decode_plain_and_padded() {
    let mut p = ReportParser::default();
    for (charging, level) in [(0u8, 100u8), (1, 87), (0, 14), (1, 0)] {
        for r in [
            vec![0x07, charging, level],
            padded(vec![0x07, charging, level], 13),
        ] {
            assert_eq!(
                p.parse(&r),
                Ok(vec![DeviceEvent::Battery {
                    charging: charging != 0,
                    level
                }])
            );
        }
    }
}

#[test]
fn unknown_and_short_reports_are_errors_the_manager_skips() {
    let mut p = ReportParser::default();
    assert_eq!(
        p.parse(&[0x05, 1, 2, 3]),
        Err(ReportError::UnknownReport(0x05))
    );
    assert_eq!(
        p.parse(&[0x03, 0, 1]),
        Err(ReportError::Short { id: 0x03, len: 3 })
    );

    let (plug, clock) = setup();
    let (challenge, _) = vector();
    let dev = Device::plugged(challenge, 600, Bus::Bluetooth);
    *plug.lock().unwrap() = Some(dev.clone());
    let mut m = manager(&plug, &clock);
    assert_eq!(m.tick(T), [Status(Connected), Dev(DeviceEvent::Connected)]);
    dev.send(vec![0x05, 0xaa]);
    dev.send(vec![0x03, 0]);
    dev.send(keys(&[0x0f]));
    assert_eq!(m.tick(T), []);
    assert_eq!(m.tick(T), []);
    assert_eq!(m.tick(T), [Dev(DeviceEvent::Press(Key::Cut))]);
    let d = m.diagnostics();
    assert_eq!(d.undecoded, 2);
    assert_eq!(d.status, Some(Connected));
    assert_eq!(d.reports[2].decoded, "unknown report id 0x05");
}

#[test]
fn full_cycle_connect_auth_keys_jog_battery_disconnect_reconnect() {
    let (challenge, response) = vector();
    let (plug, clock) = setup();
    let mut config = Config::default();
    config.jog.default_mode = JogMode::Shuttle;
    let x = KeyCombo {
        modifiers: vec![],
        key: "X".into(),
    };
    config.layers[0]
        .bindings
        .insert("Cut".into(), Action::Hotkey { combo: x });
    let mut engine = Engine::new(&config);
    let mut injector = MockInjector::new();
    let mut m = manager(&plug, &clock);

    // The device loop's turn: tick, feed the engine, write what it queued.
    let run = |m: &mut Manager, engine: &mut Engine, injector: &mut MockInjector| {
        let events = m.tick(T);
        for e in &events {
            if let Dev(e) = e {
                engine
                    .handle(e, injector, &mut MockLauncher::new())
                    .unwrap();
            }
        }
        for r in engine.take_reports() {
            m.write(&r).unwrap();
        }
        events
    };

    // Absent until plugged, retried every 3 s.
    assert_eq!(run(&mut m, &mut engine, &mut injector), [Status(Absent)]);

    // Plugged over USB: the handshake carries the bmd.py response.
    let usb = Device::plugged(challenge, 600, Bus::Usb);
    *plug.lock().unwrap() = Some(usb.clone());
    clock.advance(RETRY_INTERVAL);
    assert_eq!(
        run(&mut m, &mut engine, &mut injector),
        [Status(Connected), Dev(DeviceEvent::Connected)]
    );
    assert_eq!(
        usb.written(),
        [
            Written::Feature(feature(0, 0)),
            Written::Feature(feature(1, 0)),
            Written::Feature(feature(3, response)),
            // The engine set the device wheel mode for Shuttle at once.
            Written::Output(set_jog_mode(DeviceJogMode::AbsoluteDeadzero).to_vec()),
        ]
    );
    let d = m.diagnostics();
    assert_eq!(d.auth.state, AuthState::Passed);
    assert_eq!(d.device.as_ref().and_then(|i| i.bus), Some(Bus::Usb));
    assert_eq!(d.reports[0].direction, Direction::Out);

    // Keys: CUT down (Windows-padded) and up taps X once.
    usb.send(padded(keys(&[0x0f]), 13));
    usb.send(padded(keys(&[]), 13));
    assert_eq!(
        device_events(run(&mut m, &mut engine, &mut injector)),
        [DeviceEvent::Press(Key::Cut)]
    );
    assert_eq!(
        device_events(run(&mut m, &mut engine, &mut injector)),
        [DeviceEvent::Release(Key::Cut)]
    );
    let xk = Keystroke::Key("X".into());
    assert_eq!(
        injector.calls(),
        [InjectCall::Down(xk.clone()), InjectCall::Up(xk)]
    );

    // Jog: an absolute dial position, then battery.
    usb.send(wheel(3, 800));
    usb.send(vec![0x07, 1, 64]);
    assert_eq!(
        device_events(run(&mut m, &mut engine, &mut injector)),
        [DeviceEvent::Jog {
            mode: 3,
            value: 800
        }]
    );
    assert_eq!(
        device_events(run(&mut m, &mut engine, &mut injector)),
        [DeviceEvent::Battery {
            charging: true,
            level: 64
        }]
    );
    let d = m.diagnostics();
    assert_eq!(d.wheel.total, 800);
    assert_eq!(d.battery.map(|b| b.level), Some(64));
    assert_eq!(d.presses, ["CUT"]);

    // Unplugged: the read fails, everything is released, then retried.
    assert_eq!(
        run(&mut m, &mut engine, &mut injector),
        [Dev(DeviceEvent::Disconnected), Status(Absent)]
    );
    assert!(injector.keys_down().is_empty());
    assert_eq!(m.diagnostics().disconnects, 1);

    // Reconnected over Bluetooth with a new challenge: a fresh handshake
    // and the wheel mode set again (the device forgot it).
    let (challenge2, response2) = (challenge ^ 0x0101, compute_response(challenge ^ 0x0101));
    let bt = Device::plugged(challenge2, 600, Bus::Bluetooth);
    *plug.lock().unwrap() = Some(bt.clone());
    clock.advance(RETRY_INTERVAL);
    assert_eq!(
        run(&mut m, &mut engine, &mut injector),
        [Status(Connected), Dev(DeviceEvent::Connected)]
    );
    assert_eq!(bt.written()[2], Written::Feature(feature(3, response2)));
    assert_eq!(
        bt.written()[3],
        Written::Output(set_jog_mode(DeviceJogMode::AbsoluteDeadzero).to_vec())
    );
    bt.send(keys(&[0x33]));
    assert_eq!(
        device_events(run(&mut m, &mut engine, &mut injector)),
        [DeviceEvent::Press(Key::Cam1)]
    );
    let d = m.diagnostics();
    assert_eq!((d.connects, d.disconnects), (2, 1));
    assert_eq!(d.device.as_ref().and_then(|i| i.bus), Some(Bus::Bluetooth));
    assert_eq!(d.auth.passed, 2);
}

#[test]
fn a_session_reauths_at_timeout_minus_10s_with_the_real_response() {
    let (challenge, response) = vector();
    let (plug, clock) = setup();
    let dev = Device::plugged(challenge, 300, Bus::Usb);
    *plug.lock().unwrap() = Some(dev.clone());
    // No idle check here: a busy device re-auths on the session timeout.
    let slot = Arc::clone(&plug);
    let open: Opener = Box::new(move || slot.lock().unwrap().take().ok_or(HidError::Disconnected));
    let mut m: Manager =
        ConnectionManager::new(open, clock.clone(), compute_response as fn(u64) -> u64);
    assert_eq!(m.tick(T)[0], Status(Connected));
    assert_eq!(m.diagnostics().auth.next_in_s, Some(290));
    clock.advance(Duration::from_secs(289));
    dev.send(wheel(0, 1));
    assert_eq!(m.tick(T).len(), 1);
    assert_eq!(dev.written().len(), 3);
    clock.advance(Duration::from_secs(1));
    dev.handshake(challenge, 300);
    dev.send(wheel(0, -1));
    assert_eq!(m.tick(T), [Dev(DeviceEvent::Jog { mode: 0, value: -1 })]);
    assert_eq!(dev.written()[5], Written::Feature(feature(3, response)));
    assert_eq!(m.diagnostics().auth.passed, 2);
}
