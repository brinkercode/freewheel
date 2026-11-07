//! Edge-case suite for the engine and device layer. It runs on the host with
//! only the fake device ([`FakeTransport`] behind a [`ConnectionManager`]) and
//! the mock key injector ([`MockInjector`]): no hardware, no network.
//!
//! Scenario -> test:
//! - stuck keys: `stuck_keys_none_left_down_after_off_disconnect_or_quit_for_every_action_type`
//! - multi-key reports: `multi_key_report_presses_and_releases_every_bound_key`
//! - rapid toggles: `rapid_latched_toggles_end_in_the_expected_state`,
//!   `rapid_off_on_toggles_leave_no_key_down_and_reconnect`
//! - Resolve hand-off: `resolve_hand_off_auto_off_and_auto_on`
//! - auth: `auth_rejected_status_is_auth_failed_then_retries_after_3s`,
//!   `auth_reauth_capped_at_590s_and_failed_reauth_is_a_disconnect`
//! - reports: `malformed_and_unknown_reports_are_skipped_and_connection_stays_up`
//! - layers: `layers_held_key_survives_push_and_back_and_latched_persists`
//! - dial overrides: `dial_override_active_at_disconnect_stops_and_dial_returns_to_jog`
//! - config: `config_newer_version_is_rejected_and_kept_as_bad_on_first_save`
//! - reconnect: `reconnect_after_unplug_retries_every_3s_and_starts_with_fresh_key_state`

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::rc::Rc;
use std::time::{Duration, Instant};

use freewheel_core::clock::Clock;
use freewheel_core::config::{
    Action, BAD_PREFIX, CONFIG_FILE, Config, ConfigError, ConfigStore, JogMode, KeyCombo, Layer,
    Modifier, OverrideMode,
};
use freewheel_core::connection::{
    ConnectionEvent, ConnectionManager, ConnectionStatus, RETRY_INTERVAL,
};
use freewheel_core::engine::Engine;
use freewheel_core::hid::{FakeTransport, HidError, HidTransport};
use freewheel_core::inject::{InjectCall, Keystroke, MockInjector};
use freewheel_core::launch::MockLauncher;
use freewheel_core::report::{DeviceEvent, Key};
use freewheel_core::resolve::{FakeProcessLister, HandOff, REPLUG_NOTIFICATION, ResolveWatcher};

use ConnectionEvent::{Device as Dev, Status};
use ConnectionStatus::{Absent, AuthFailed, Connected, Off};
use DeviceEvent::{Press, Release};

const T: Duration = Duration::from_millis(10);

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

/// A [`FakeTransport`] the test keeps a handle to after the manager opens it.
#[derive(Clone, Default)]
struct Device(Rc<RefCell<FakeTransport>>);

impl HidTransport for Device {
    fn read_timeout(&mut self, buf: &mut [u8], t: Duration) -> Result<usize, HidError> {
        self.0.borrow_mut().read_timeout(buf, t)
    }
    fn write(&mut self, data: &[u8]) -> Result<usize, HidError> {
        self.0.borrow_mut().write(data)
    }
    fn get_feature_report(&mut self, buf: &mut [u8]) -> Result<usize, HidError> {
        self.0.borrow_mut().get_feature_report(buf)
    }
    fn send_feature_report(&mut self, data: &[u8]) -> Result<(), HidError> {
        self.0.borrow_mut().send_feature_report(data)
    }
    fn close(&mut self) {
        self.0.borrow_mut().close()
    }
}

impl Device {
    /// A device scripted to pass one handshake, plugged into `usb`.
    fn plugged(usb: &Usb, timeout_s: u16) -> Self {
        let d = Device::default();
        d.handshake(timeout_s);
        usb.plug(&d);
        d
    }
    /// Scripts the three feature-report-6 reads of one handshake (bmd.py).
    fn handshake(&self, timeout_s: u16) {
        self.feature(0, 0x1234)
            .feature(2, 0)
            .feature(4, timeout_s.into());
    }
    fn feature(&self, sub: u8, payload: u64) -> &Self {
        let mut r = vec![0x06, sub];
        r.extend_from_slice(&payload.to_le_bytes());
        self.0.borrow_mut().push_feature(r);
        self
    }
    fn input(&self, report: Vec<u8>) -> &Self {
        self.0.borrow_mut().push_input(report);
        self
    }
    fn timeout(&self) -> &Self {
        self.0.borrow_mut().push_timeout();
        self
    }
    fn writes(&self) -> usize {
        self.0.borrow().written().len()
    }
    fn closed(&self) -> bool {
        self.0.borrow().is_closed()
    }
}

/// Hands out the plugged devices in order; nothing plugged means absent.
#[derive(Clone, Default)]
struct Usb {
    queue: Rc<RefCell<VecDeque<Device>>>,
    opens: Rc<Cell<usize>>,
}

type Opener = Box<dyn FnMut() -> Result<Device, HidError>>;
type Manager = ConnectionManager<Device, Opener, FakeClock, fn(u64) -> u64>;

impl Usb {
    fn plug(&self, d: &Device) {
        self.queue.borrow_mut().push_back(d.clone());
    }
    fn manager(&self, clock: &FakeClock) -> Manager {
        let usb = self.clone();
        let open: Opener = Box::new(move || {
            usb.opens.set(usb.opens.get() + 1);
            let next = usb.queue.borrow_mut().pop_front();
            next.ok_or(HidError::Io("no device".into()))
        });
        ConnectionManager::new(open, clock.clone(), (|c: u64| !c) as fn(u64) -> u64)
    }
}

/// The engine side of the device thread: every device event goes to the engine.
struct Rig {
    engine: Engine,
    mock: MockInjector,
    launcher: MockLauncher,
}

impl Rig {
    fn new(config: &Config) -> Self {
        Rig {
            engine: Engine::new(config),
            mock: MockInjector::new(),
            launcher: MockLauncher::new(),
        }
    }
    fn feed(&mut self, events: Vec<ConnectionEvent>) -> Vec<ConnectionEvent> {
        for e in &events {
            if let Dev(e) = e {
                self.engine
                    .handle(e, &mut self.mock, &mut self.launcher)
                    .unwrap();
            }
        }
        events
    }
    fn tick(&mut self, m: &mut Manager) -> Vec<ConnectionEvent> {
        self.feed(m.tick(T))
    }
}

fn combo(key: &str, modifiers: &[Modifier]) -> KeyCombo {
    KeyCombo {
        modifiers: modifiers.to_vec(),
        key: key.into(),
    }
}

fn layer(id: &str, bindings: &[(Key, Action)]) -> Layer {
    Layer {
        id: id.into(),
        name: id.into(),
        bindings: bindings
            .iter()
            .map(|(k, a)| (format!("{k:?}"), a.clone()))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn config_with(base: &[(Key, Action)], more: Vec<Layer>) -> Config {
    let mut config = Config::default();
    config.layers[0] = layer("base", base);
    config.layers.extend(more);
    config
}

/// Input report 0x04 with up to six raw keycodes (bmd.py layout).
fn raw_keys(codes: &[u16]) -> Vec<u8> {
    let mut r = vec![0x04];
    for i in 0..6 {
        r.extend_from_slice(&codes.get(i).copied().unwrap_or(0).to_le_bytes());
    }
    r
}

fn keys(keys: &[Key]) -> Vec<u8> {
    raw_keys(&keys.iter().map(|&k| k as u16).collect::<Vec<_>>())
}

/// Input report 0x03: mode byte and a signed le32 value (bmd.py layout).
fn jog(value: i32) -> Vec<u8> {
    let mut r = vec![0x03, 0];
    r.extend_from_slice(&value.to_le_bytes());
    r
}

fn key(name: &str) -> Keystroke {
    Keystroke::Key(name.into())
}

fn modifier(m: Modifier) -> Keystroke {
    Keystroke::Modifier(m)
}

/// One action of each type; the match stops compiling when a type is added.
/// No combo shares a modifier with the held (Ctrl) or latched (Meta) key.
fn every_action_type() -> Vec<Action> {
    let all = vec![
        Action::Hotkey {
            combo: combo("H", &[]),
        },
        Action::Held {
            combo: combo("D", &[Modifier::Shift]),
        },
        Action::Latched {
            combo: combo("L", &[Modifier::Alt]),
        },
        Action::Scroll { amount: 3 },
        Action::LaunchApp {
            path: "/usr/bin/app".into(),
            args: vec![],
            enabled: true,
            cwd: None,
            per_os: Some(freewheel_core::config::PerOsTargets {
                windows: Some("app.exe".into()),
                macos: None,
                linux: Some("/usr/bin/app".into()),
            }),
        },
        Action::LayerPush {
            target: "l1".into(),
        },
        Action::SetJogMode {
            mode: JogMode::Shuttle,
        },
        Action::OverrideToggle {
            mode: OverrideMode::SystemVolume,
        },
    ];
    for a in &all {
        match a {
            Action::Hotkey { .. }
            | Action::Held { .. }
            | Action::Latched { .. }
            | Action::Scroll { .. }
            | Action::LaunchApp { .. }
            | Action::LayerPush { .. }
            | Action::SetJogMode { .. }
            | Action::OverrideToggle { .. } => {}
        }
    }
    all
}

#[derive(Debug, Clone, Copy)]
enum End {
    Off,
    Disconnect,
    Quit,
}

#[test]
fn stuck_keys_none_left_down_after_off_disconnect_or_quit_for_every_action_type() {
    for action in every_action_type() {
        for end in [End::Off, End::Disconnect, End::Quit] {
            let config = config_with(
                &[
                    (
                        Key::In,
                        Action::Held {
                            combo: combo("I", &[Modifier::Ctrl]),
                        },
                    ),
                    (
                        Key::Snap,
                        Action::Latched {
                            combo: combo("S", &[Modifier::Meta]),
                        },
                    ),
                    (Key::Cam2, action.clone()),
                ],
                vec![layer("l1", &[])],
            );
            let (usb, clock) = (Usb::default(), FakeClock::new());
            let mut m = usb.manager(&clock);
            let mut rig = Rig::new(&config);
            let dev = Device::plugged(&usb, 600);
            // In held, Snap latched, then the action key pressed and held.
            dev.input(keys(&[Key::In]))
                .input(keys(&[Key::In, Key::Snap]))
                .input(keys(&[Key::In]))
                .input(keys(&[Key::In, Key::Cam2]));
            for _ in 0..5 {
                rig.tick(&mut m);
            }
            let held_and_latched = [
                modifier(Modifier::Ctrl),
                key("I"),
                modifier(Modifier::Meta),
                key("S"),
            ];
            assert!(
                held_and_latched
                    .iter()
                    .all(|k| rig.mock.keys_down().contains(k)),
                "{action:?} before {end:?}: {:?}",
                rig.mock.keys_down()
            );
            match end {
                End::Off => {
                    rig.feed(m.set_enabled(false));
                    assert_eq!(m.status(), Some(Off));
                }
                // The input script is empty: the next read is a disconnect.
                End::Disconnect => {
                    rig.tick(&mut m);
                    assert_eq!(m.status(), Some(Absent));
                }
                // Quit: the device thread releases everything before it exits.
                End::Quit => rig.engine.release_all(&mut rig.mock).unwrap(),
            }
            assert!(
                rig.mock.keys_down().is_empty(),
                "{action:?} after {end:?}: {:?}",
                rig.mock.keys_down()
            );
        }
    }
}

#[test]
fn lone_modifier_held_or_latched_is_released_on_off_disconnect_or_quit() {
    for end in [End::Off, End::Disconnect, End::Quit] {
        let config = config_with(
            &[
                (
                    Key::In,
                    Action::Held {
                        combo: combo("", &[Modifier::Shift]),
                    },
                ),
                (
                    Key::Snap,
                    Action::Latched {
                        combo: combo("", &[Modifier::Alt]),
                    },
                ),
            ],
            vec![],
        );
        let (usb, clock) = (Usb::default(), FakeClock::new());
        let mut m = usb.manager(&clock);
        let mut rig = Rig::new(&config);
        let dev = Device::plugged(&usb, 600);
        // Snap latched, then In pressed and held.
        dev.input(keys(&[Key::Snap]))
            .input(keys(&[]))
            .input(keys(&[Key::In]));
        for _ in 0..4 {
            rig.tick(&mut m);
        }
        assert_eq!(
            rig.mock.keys_down(),
            [modifier(Modifier::Alt), modifier(Modifier::Shift)],
            "before {end:?}"
        );
        match end {
            End::Off => {
                rig.feed(m.set_enabled(false));
            }
            End::Disconnect => {
                rig.tick(&mut m);
            }
            End::Quit => rig.engine.release_all(&mut rig.mock).unwrap(),
        }
        assert!(
            rig.mock.keys_down().is_empty(),
            "after {end:?}: {:?}",
            rig.mock.keys_down()
        );
    }
}

#[test]
fn multi_key_report_presses_and_releases_every_bound_key() {
    let held = |k: &str| Action::Held {
        combo: combo(k, &[]),
    };
    let config = config_with(
        &[
            (Key::In, held("I")),
            (Key::Out, held("O")),
            (Key::Cut, held("C")),
            (
                Key::Cam1,
                Action::Hotkey {
                    combo: combo("X", &[]),
                },
            ),
        ],
        vec![],
    );
    let (usb, clock) = (Usb::default(), FakeClock::new());
    let mut m = usb.manager(&clock);
    let mut rig = Rig::new(&config);
    let dev = Device::plugged(&usb, 600);
    dev.input(keys(&[Key::In, Key::Out, Key::Cut, Key::Cam1]))
        .input(keys(&[Key::Out]))
        .input(keys(&[]));
    rig.tick(&mut m);

    assert_eq!(
        rig.tick(&mut m),
        [
            Dev(Press(Key::In)),
            Dev(Press(Key::Out)),
            Dev(Press(Key::Cut)),
            Dev(Press(Key::Cam1)),
        ]
    );
    assert_eq!(rig.mock.keys_down(), [key("I"), key("O"), key("C")]);
    let tap = [InjectCall::Down(key("X")), InjectCall::Up(key("X"))];
    assert_eq!(rig.mock.calls()[3..], tap);

    // One report releases three keys while Out stays down.
    assert_eq!(
        rig.tick(&mut m),
        [
            Dev(Release(Key::In)),
            Dev(Release(Key::Cut)),
            Dev(Release(Key::Cam1)),
        ]
    );
    assert_eq!(rig.mock.keys_down(), [key("O")]);
    rig.tick(&mut m);
    assert!(rig.mock.keys_down().is_empty());
}

#[test]
fn rapid_latched_toggles_end_in_the_expected_state() {
    let config = config_with(
        &[(
            Key::Snap,
            Action::Latched {
                combo: combo("L", &[Modifier::Ctrl]),
            },
        )],
        vec![],
    );
    let (usb, clock) = (Usb::default(), FakeClock::new());
    let mut m = usb.manager(&clock);
    let mut rig = Rig::new(&config);
    let dev = Device::plugged(&usb, 600);
    for _ in 0..101 {
        dev.input(keys(&[Key::Snap])).input(keys(&[]));
    }
    for _ in 0..1 + 2 * 101 {
        rig.tick(&mut m);
    }
    // An odd number of presses leaves it latched, every down matched once.
    assert_eq!(rig.mock.keys_down(), [modifier(Modifier::Ctrl), key("L")]);
    assert_eq!(rig.mock.calls().len(), 101 * 2);
    dev.input(keys(&[Key::Snap]));
    rig.tick(&mut m);
    assert!(rig.mock.keys_down().is_empty());
    assert_eq!(rig.mock.calls().len(), 102 * 2);
}

#[test]
fn rapid_off_on_toggles_leave_no_key_down_and_reconnect() {
    let config = config_with(
        &[(
            Key::In,
            Action::Held {
                combo: combo("I", &[Modifier::Shift]),
            },
        )],
        vec![],
    );
    let (usb, clock) = (Usb::default(), FakeClock::new());
    let mut m = usb.manager(&clock);
    let mut rig = Rig::new(&config);
    let first = Device::plugged(&usb, 600);
    first.input(keys(&[Key::In]));
    rig.tick(&mut m);
    rig.tick(&mut m);
    assert_eq!(rig.mock.keys_down().len(), 2);

    let mut events = Vec::new();
    for _ in 0..20 {
        events.extend(rig.feed(m.set_enabled(false)));
        events.extend(rig.feed(m.set_enabled(true)));
    }
    assert_eq!(events, [Dev(DeviceEvent::Disconnected), Status(Off)]);
    assert!(first.closed());
    assert!(rig.mock.keys_down().is_empty());
    assert_eq!(usb.opens.get(), 1);

    let second = Device::plugged(&usb, 600);
    assert_eq!(
        rig.tick(&mut m),
        [Status(Connected), Dev(DeviceEvent::Connected)]
    );
    assert_eq!(usb.opens.get(), 2);
    assert_eq!(second.writes(), 3);
}

#[test]
fn resolve_hand_off_auto_off_and_auto_on() {
    let (usb, clock) = (Usb::default(), FakeClock::new());
    let mut m = usb.manager(&clock);
    let mut rig = Rig::new(&Config::default());
    let mut ps = FakeProcessLister::new();
    let mut w = ResolveWatcher::from_settings(&Config::default().settings);
    let mut enabled = true;
    let first = Device::plugged(&usb, 600);
    rig.tick(&mut m);

    // Resolve starts: auto-off closes the device and notifies.
    ps.start(w.process_name());
    let action = w.poll(&mut ps, enabled);
    assert_eq!(
        action,
        Some(HandOff::SwitchOff {
            notification: REPLUG_NOTIFICATION
        })
    );
    enabled = false;
    assert_eq!(
        rig.feed(m.set_enabled(enabled)),
        [Dev(DeviceEvent::Disconnected), Status(Off)]
    );
    assert!(first.closed());

    // Resolve quits: auto-on, because auto-off did the switching.
    ps.stop(w.process_name());
    assert_eq!(w.poll(&mut ps, enabled), Some(HandOff::SwitchOn));
    enabled = true;
    rig.feed(m.set_enabled(enabled));
    Device::plugged(&usb, 600);
    assert_eq!(
        rig.tick(&mut m),
        [Status(Connected), Dev(DeviceEvent::Connected)]
    );

    // A manual Off is respected: Resolve coming and going leaves it Off.
    w.manual_switch();
    enabled = false;
    rig.feed(m.set_enabled(enabled));
    ps.start(w.process_name());
    assert_eq!(w.poll(&mut ps, enabled), None);
    ps.stop(w.process_name());
    assert_eq!(w.poll(&mut ps, enabled), None);
    assert_eq!(m.status(), Some(Off));
}

#[test]
fn auth_rejected_status_is_auth_failed_then_retries_after_3s() {
    let (usb, clock) = (Usb::default(), FakeClock::new());
    let mut m = usb.manager(&clock);
    let mut rig = Rig::new(&Config::default());
    // The status report comes back with the wrong sub-id: rejected.
    let bad = Device::default();
    bad.feature(0, 0x1234).feature(2, 0).feature(5, 600);
    usb.plug(&bad);
    assert_eq!(rig.tick(&mut m), [Status(AuthFailed)]);
    assert!(bad.closed());

    let good = Device::plugged(&usb, 600);
    clock.advance(RETRY_INTERVAL - Duration::from_millis(1));
    assert_eq!(rig.tick(&mut m), []);
    assert_eq!(usb.opens.get(), 1);
    clock.advance(Duration::from_millis(1));
    assert_eq!(
        rig.tick(&mut m),
        [Status(Connected), Dev(DeviceEvent::Connected)]
    );
    assert_eq!(good.writes(), 3);
    assert!(rig.mock.calls().is_empty());
}

#[test]
fn auth_reauth_capped_at_590s_and_failed_reauth_is_a_disconnect() {
    let config = config_with(
        &[(
            Key::In,
            Action::Held {
                combo: combo("I", &[]),
            },
        )],
        vec![],
    );
    let (usb, clock) = (Usb::default(), FakeClock::new());
    let mut m = usb.manager(&clock);
    let mut rig = Rig::new(&config);
    // The device reports a 3600 s session: ADR-03 caps it at 600 s - 10 s.
    let dev = Device::plugged(&usb, 3600);
    dev.input(keys(&[Key::In]));
    rig.tick(&mut m);
    rig.tick(&mut m);
    assert_eq!(rig.mock.keys_down(), [key("I")]);

    clock.advance(Duration::from_secs(589));
    dev.timeout();
    assert_eq!(rig.tick(&mut m), []);
    assert_eq!(dev.writes(), 3);

    // Due at 590 s; the device stays silent, so re-auth fails: a disconnect.
    clock.advance(Duration::from_secs(1));
    assert_eq!(
        rig.tick(&mut m),
        [Dev(DeviceEvent::Disconnected), Status(Absent)]
    );
    assert_eq!(dev.writes(), 4);
    assert!(dev.closed());
    assert!(rig.mock.keys_down().is_empty());
}

#[test]
fn malformed_and_unknown_reports_are_skipped_and_connection_stays_up() {
    let config = config_with(
        &[(
            Key::In,
            Action::Hotkey {
                combo: combo("I", &[]),
            },
        )],
        vec![],
    );
    let (usb, clock) = (Usb::default(), FakeClock::new());
    let mut m = usb.manager(&clock);
    let mut rig = Rig::new(&config);
    let dev = Device::plugged(&usb, 600);
    dev.input(vec![0x09, 0, 0]) // unknown report id
        .input(vec![0x04, 0x07]) // short key report
        .input(vec![0x03, 0, 1]) // short jog report
        .input(raw_keys(&[0x07, 0x99])) // a known key next to an unknown one
        .input(vec![0x07, 1, 50]) // battery
        .input(keys(&[Key::In]));
    rig.tick(&mut m);
    for _ in 0..4 {
        assert_eq!(rig.tick(&mut m), []);
    }
    assert_eq!(m.status(), Some(Connected));
    assert_eq!(
        rig.tick(&mut m),
        [Dev(DeviceEvent::Battery {
            charging: true,
            level: 50
        })]
    );
    assert!(rig.mock.calls().is_empty());
    // The dropped report left no key state behind: In is a fresh press.
    assert_eq!(rig.tick(&mut m), [Dev(Press(Key::In))]);
    assert_eq!(rig.mock.calls().len(), 2);
}

#[test]
fn layers_held_key_survives_push_and_back_and_latched_persists() {
    let config = config_with(
        &[
            (
                Key::In,
                Action::Held {
                    combo: combo("I", &[Modifier::Ctrl]),
                },
            ),
            (
                Key::Snap,
                Action::Latched {
                    combo: combo("S", &[Modifier::Shift]),
                },
            ),
            (
                Key::Cam1,
                Action::LayerPush {
                    target: "l1".into(),
                },
            ),
        ],
        vec![layer(
            "l1",
            &[
                (
                    Key::In,
                    Action::Hotkey {
                        combo: combo("X", &[]),
                    },
                ),
                (
                    Key::Cam1,
                    Action::Hotkey {
                        combo: combo("Y", &[]),
                    },
                ),
            ],
        )],
    );
    let (usb, clock) = (Usb::default(), FakeClock::new());
    let mut m = usb.manager(&clock);
    let mut rig = Rig::new(&config);
    let dev = Device::plugged(&usb, 600);
    let latched = [modifier(Modifier::Shift), key("S")];
    dev.input(keys(&[Key::Snap]))
        .input(keys(&[]))
        .input(keys(&[Key::In]))
        .input(keys(&[Key::In, Key::Cam1]))
        .input(keys(&[Key::Cam1]))
        .input(keys(&[]))
        .input(keys(&[Key::Cam1]))
        .input(keys(&[]))
        .input(keys(&[Key::Snap]));
    for _ in 0..5 {
        rig.tick(&mut m);
    }
    // ADR-07: on l1, the held key and the latch are still down.
    assert_eq!(rig.engine.current_layer(), "l1");
    assert_eq!(rig.mock.keys_down().len(), 4);
    // Its physical release on l1 releases it; the latch persists.
    rig.tick(&mut m);
    assert_eq!(rig.mock.keys_down(), latched);
    rig.tick(&mut m);
    // The push button pops back instead of firing l1's binding.
    rig.tick(&mut m);
    assert_eq!(rig.engine.current_layer(), "base");
    assert_eq!(rig.mock.keys_down(), latched);
    rig.tick(&mut m);
    rig.tick(&mut m);
    assert!(rig.mock.keys_down().is_empty());
    let fired_l1 = |k| rig.mock.calls().contains(&InjectCall::Down(key(k)));
    assert!(!fired_l1("X") && !fired_l1("Y"));
}

#[test]
fn dial_override_active_at_disconnect_stops_and_dial_returns_to_jog() {
    let config = config_with(
        &[(
            Key::Cam1,
            Action::OverrideToggle {
                mode: OverrideMode::SystemVolume,
            },
        )],
        vec![],
    );
    let (usb, clock) = (Usb::default(), FakeClock::new());
    let mut m = usb.manager(&clock);
    let mut rig = Rig::new(&config);
    let first = Device::plugged(&usb, 600);
    first
        .input(keys(&[Key::Cam1]))
        .input(keys(&[]))
        .input(jog(2));
    for _ in 0..4 {
        rig.tick(&mut m);
    }
    assert_eq!(
        rig.engine.active_override(),
        Some(OverrideMode::SystemVolume)
    );
    assert_eq!(
        rig.engine.take_override_ticks(),
        [(OverrideMode::SystemVolume, 2)]
    );
    assert!(rig.mock.calls().is_empty());

    first.input(jog(3));
    rig.tick(&mut m); // the jog report
    rig.tick(&mut m); // script ends: disconnect
    assert_eq!(m.status(), Some(Absent));
    assert_eq!(rig.engine.active_override(), None);
    assert!(rig.engine.take_override_ticks().is_empty());

    let second = Device::plugged(&usb, 600);
    second.input(jog(1));
    clock.advance(RETRY_INTERVAL);
    rig.tick(&mut m);
    rig.tick(&mut m);
    assert_eq!(rig.mock.calls(), [InjectCall::Scroll(1)]);
    assert!(rig.engine.take_override_ticks().is_empty());
}

#[test]
fn config_newer_version_is_rejected_and_kept_as_bad_on_first_save() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = ConfigStore::new(dir.path());
    let newer = br#"{"version":2,"layers":[]}"#;
    fs::write(store.path(), newer).unwrap();

    let (config, err) = store.load_or_default();
    assert!(matches!(err, Some(ConfigError::UnsupportedVersion(2))));
    assert_eq!(config, Config::default());
    // The default config binds no key: a press injects nothing.
    let mut rig = Rig::new(&config);
    rig.feed(vec![Dev(Press(Key::Cut)), Dev(Release(Key::Cut))]);
    assert!(rig.mock.calls().is_empty());

    store.save(&config).unwrap();
    let mut names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(names.len(), 2, "{names:?}");
    assert_eq!(names[0], CONFIG_FILE);
    assert!(names[1].starts_with(BAD_PREFIX), "{names:?}");
    assert_eq!(fs::read(dir.path().join(&names[1])).unwrap(), newer);
    assert_eq!(store.load().unwrap(), config);
}

#[test]
fn reconnect_after_unplug_retries_every_3s_and_starts_with_fresh_key_state() {
    let config = config_with(
        &[(
            Key::In,
            Action::Held {
                combo: combo("I", &[]),
            },
        )],
        vec![],
    );
    let (usb, clock) = (Usb::default(), FakeClock::new());
    let mut m = usb.manager(&clock);
    let mut rig = Rig::new(&config);
    let first = Device::plugged(&usb, 600);
    first.input(keys(&[Key::In]));
    rig.tick(&mut m);
    rig.tick(&mut m);
    // Unplugged with In still down on the device.
    assert_eq!(
        rig.tick(&mut m),
        [Dev(DeviceEvent::Disconnected), Status(Absent)]
    );
    assert!(rig.mock.keys_down().is_empty());

    let second = Device::plugged(&usb, 600);
    second.input(keys(&[Key::In])).input(keys(&[]));
    clock.advance(RETRY_INTERVAL - Duration::from_millis(1));
    assert_eq!(rig.tick(&mut m), []);
    assert_eq!(usb.opens.get(), 1);
    clock.advance(Duration::from_millis(1));
    assert_eq!(
        rig.tick(&mut m),
        [Status(Connected), Dev(DeviceEvent::Connected)]
    );
    // The parser was reset: In, still down, is a new press, not a no-op.
    assert_eq!(rig.tick(&mut m), [Dev(Press(Key::In))]);
    assert_eq!(rig.mock.keys_down(), [key("I")]);
    assert_eq!(rig.tick(&mut m), [Dev(Release(Key::In))]);
    assert!(rig.mock.keys_down().is_empty());
    let downs = rig
        .mock
        .calls()
        .iter()
        .filter(|c| **c == InjectCall::Down(key("I")))
        .count();
    assert_eq!(downs, 2);
}
