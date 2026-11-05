//! The connection manager: opens the device through a factory, keeps it
//! authenticated, retries every 3 s while it is absent and closes the
//! handle on Off. Connection changes come out as [`ConnectionEvent`]s.

use std::time::{Duration, Instant};

use crate::auth::{Authenticator, Clock};
use crate::hid::{HidError, HidTransport};
use crate::report::{DeviceEvent, ReportParser};

/// How often the manager tries to open the device while it is absent.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(3);
const REPORT_BUF: usize = 64;

/// The connection part of the status event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum ConnectionStatus {
    Absent,
    Connected,
    Off,
    AuthFailed,
}

/// Something the manager publishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionEvent {
    /// The connection status changed.
    Status(ConnectionStatus),
    /// A device event, including `Connected` and `Disconnected`.
    Device(DeviceEvent),
}

/// Drives one device over any [`HidTransport`] that `open` returns.
pub struct ConnectionManager<T, O, C, F> {
    open: O,
    clock: C,
    auth: Authenticator<C, F>,
    parser: ReportParser,
    handle: Option<T>,
    enabled: bool,
    retry_at: Instant,
    status: Option<ConnectionStatus>,
}

impl<T, O, C, F> ConnectionManager<T, O, C, F>
where
    T: HidTransport,
    O: FnMut() -> Result<T, HidError>,
    C: Clock + Clone,
    F: Fn(u64) -> u64,
{
    /// A manager that is On; the first [`tick`](Self::tick) opens the device.
    pub fn new(open: O, clock: C, respond: F) -> Self {
        Self {
            open,
            auth: Authenticator::new(clock.clone(), respond),
            retry_at: clock.now(),
            clock,
            parser: ReportParser::default(),
            handle: None,
            enabled: true,
            status: None,
        }
    }

    /// The last published status; `None` before the first tick.
    pub fn status(&self) -> Option<ConnectionStatus> {
        self.status
    }

    /// On re-opens the device on the next tick; Off closes the handle and
    /// stops retrying.
    pub fn set_enabled(&mut self, enabled: bool) -> Vec<ConnectionEvent> {
        let mut events = Vec::new();
        if enabled && !self.enabled {
            self.retry_at = self.clock.now();
        } else if !enabled {
            self.drop_handle(&mut events);
            self.set_status(ConnectionStatus::Off, &mut events);
        }
        self.enabled = enabled;
        events
    }

    /// Opens the device when a retry is due, or re-authenticates when due
    /// and reads one input report, waiting at most `read_timeout`.
    pub fn tick(&mut self, read_timeout: Duration) -> Vec<ConnectionEvent> {
        let mut events = Vec::new();
        if !self.enabled {
            return events;
        }
        let Some(t) = self.handle.as_mut() else {
            if self.clock.now() >= self.retry_at {
                self.connect(&mut events);
            }
            return events;
        };
        let read = self.auth.maintain(t).and_then(|()| {
            let mut buf = [0u8; REPORT_BUF];
            let n = t.read_timeout(&mut buf, read_timeout)?;
            Ok(buf[..n].to_vec())
        });
        match read {
            Ok(report) if report.is_empty() => {}
            // Reports we cannot decode are skipped, not fatal.
            Ok(report) => {
                if let Ok(evs) = self.parser.parse(&report) {
                    events.extend(evs.into_iter().map(ConnectionEvent::Device));
                }
            }
            Err(_) => {
                self.drop_handle(&mut events);
                self.retry_later(ConnectionStatus::Absent, &mut events);
            }
        }
        events
    }

    fn connect(&mut self, events: &mut Vec<ConnectionEvent>) {
        let mut t = match (self.open)() {
            Ok(t) => t,
            Err(_) => return self.retry_later(ConnectionStatus::Absent, events),
        };
        if self.auth.authenticate(&mut t).is_err() {
            t.close();
            return self.retry_later(ConnectionStatus::AuthFailed, events);
        }
        self.handle = Some(t);
        self.set_status(ConnectionStatus::Connected, events);
        events.push(ConnectionEvent::Device(DeviceEvent::Connected));
    }

    fn drop_handle(&mut self, events: &mut Vec<ConnectionEvent>) {
        if let Some(mut t) = self.handle.take() {
            t.close();
            self.parser = ReportParser::default();
            events.push(ConnectionEvent::Device(DeviceEvent::Disconnected));
        }
    }

    fn retry_later(&mut self, status: ConnectionStatus, events: &mut Vec<ConnectionEvent>) {
        self.retry_at = self.clock.now() + RETRY_INTERVAL;
        self.set_status(status, events);
    }

    fn set_status(&mut self, status: ConnectionStatus, events: &mut Vec<ConnectionEvent>) {
        if self.status != Some(status) {
            self.status = Some(status);
            events.push(ConnectionEvent::Status(status));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ConnectionEvent::{Device, Status};
    use super::ConnectionStatus::*;
    use super::*;
    use crate::hid::FakeTransport;
    use crate::report::Key;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::rc::Rc;

    const T: Duration = Duration::from_millis(10);

    #[derive(Clone)]
    struct FakeClock(Rc<Cell<Instant>>);

    impl FakeClock {
        fn advance(&self, d: Duration) {
            self.0.set(self.0.get() + d);
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Instant {
            self.0.get()
        }
    }

    /// A fake the test keeps a handle to after the manager opens it.
    #[derive(Clone, Default)]
    struct Shared(Rc<RefCell<FakeTransport>>);

    impl HidTransport for Shared {
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

    impl Shared {
        fn handshake(&self, timeout_s: u16) -> &Self {
            let mut f = self.0.borrow_mut();
            for (sub, payload) in [(0u8, 0x1234u64), (2, 0), (4, timeout_s.into())] {
                let mut r = vec![0x06, sub];
                r.extend_from_slice(&payload.to_le_bytes());
                f.push_feature(r);
            }
            drop(f);
            self
        }
        fn timeouts(&self, n: usize) -> &Self {
            for _ in 0..n {
                self.0.borrow_mut().push_timeout();
            }
            self
        }
        fn writes(&self) -> usize {
            self.0.borrow().written().len()
        }
        fn closed(&self) -> bool {
            self.0.borrow().is_closed()
        }
    }

    /// Hands out the queued devices in order; an empty queue or `None`
    /// means the device is absent. Counts every open attempt.
    #[derive(Clone, Default)]
    struct Factory {
        queue: Rc<RefCell<VecDeque<Option<Shared>>>>,
        opens: Rc<Cell<usize>>,
    }

    impl Factory {
        fn plug(&self, dev: &Shared) {
            self.queue.borrow_mut().push_back(Some(dev.clone()));
        }
        fn opener(&self) -> Opener {
            let f = self.clone();
            Box::new(move || {
                f.opens.set(f.opens.get() + 1);
                f.queue
                    .borrow_mut()
                    .pop_front()
                    .flatten()
                    .ok_or(HidError::Io("no device".into()))
            })
        }
    }

    type Opener = Box<dyn FnMut() -> Result<Shared, HidError>>;
    type Manager = ConnectionManager<Shared, Opener, FakeClock, fn(u64) -> u64>;

    fn setup() -> (Factory, FakeClock) {
        (
            Factory::default(),
            FakeClock(Rc::new(Cell::new(Instant::now()))),
        )
    }

    fn manager(f: &Factory, c: &FakeClock) -> Manager {
        ConnectionManager::new(f.opener(), c.clone(), (|c: u64| !c) as fn(u64) -> u64)
    }

    const CONNECTED: [ConnectionEvent; 2] = [Status(Connected), Device(DeviceEvent::Connected)];

    #[test]
    fn opens_authenticates_and_reauths_when_due() {
        let (f, clock) = setup();
        let dev = Shared::default();
        dev.handshake(300);
        f.plug(&dev);
        let mut m = manager(&f, &clock);
        assert_eq!(m.tick(T), CONNECTED);
        assert_eq!(dev.writes(), 3);

        dev.0
            .borrow_mut()
            .push_input([0x04, 0x07, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(m.tick(T), [Device(DeviceEvent::Press(Key::In))]);

        clock.advance(Duration::from_secs(289));
        dev.timeouts(1);
        assert_eq!(m.tick(T), []);
        assert_eq!(dev.writes(), 3);

        clock.advance(Duration::from_secs(1));
        dev.handshake(300).timeouts(1);
        assert_eq!(m.tick(T), []);
        assert_eq!(dev.writes(), 6);
        assert_eq!(m.status(), Some(Connected));
    }

    #[test]
    fn retries_every_3s_while_absent() {
        let (f, clock) = setup();
        let mut m = manager(&f, &clock);
        assert_eq!(m.tick(T), [Status(Absent)]);
        assert_eq!(f.opens.get(), 1);

        clock.advance(Duration::from_millis(2999));
        assert_eq!(m.tick(T), []);
        assert_eq!(f.opens.get(), 1);

        clock.advance(Duration::from_millis(1));
        assert_eq!(m.tick(T), []);
        assert_eq!(f.opens.get(), 2);

        let dev = Shared::default();
        dev.handshake(600);
        f.plug(&dev);
        clock.advance(Duration::from_secs(1));
        assert_eq!(m.tick(T), []);
        assert_eq!(f.opens.get(), 2);
        clock.advance(Duration::from_secs(2));
        assert_eq!(m.tick(T), CONNECTED);
        assert_eq!(f.opens.get(), 3);
    }

    #[test]
    fn off_closes_and_stops_retrying_on_reopens_and_reauths() {
        let (f, clock) = setup();
        let first = Shared::default();
        first.handshake(600);
        f.plug(&first);
        let mut m = manager(&f, &clock);
        assert_eq!(m.tick(T), CONNECTED);

        assert_eq!(
            m.set_enabled(false),
            [Device(DeviceEvent::Disconnected), Status(Off)]
        );
        assert!(first.closed());
        clock.advance(Duration::from_secs(30));
        assert_eq!(m.tick(T), []);
        assert_eq!(f.opens.get(), 1);

        let second = Shared::default();
        second.handshake(600);
        f.plug(&second);
        assert_eq!(m.set_enabled(true), []);
        assert_eq!(m.tick(T), CONNECTED);
        assert_eq!(f.opens.get(), 2);
        assert_eq!(second.writes(), 3);
    }

    #[test]
    fn off_while_absent_stops_retrying() {
        let (f, clock) = setup();
        let mut m = manager(&f, &clock);
        assert_eq!(m.tick(T), [Status(Absent)]);
        assert_eq!(m.set_enabled(false), [Status(Off)]);
        clock.advance(Duration::from_secs(30));
        assert_eq!(m.tick(T), []);
        assert_eq!(f.opens.get(), 1);
    }

    #[test]
    fn connect_disconnect_and_auth_failure_are_status_events() {
        let (f, clock) = setup();
        let bad = Shared::default();
        bad.handshake(5); // too short: rejected
        f.plug(&bad);
        let mut m = manager(&f, &clock);
        assert_eq!(m.tick(T), [Status(AuthFailed)]);
        assert!(bad.closed());

        let good = Shared::default();
        good.handshake(600);
        f.plug(&good);
        clock.advance(RETRY_INTERVAL);
        assert_eq!(m.tick(T), CONNECTED);

        // The input script is empty, so the read reports a disconnect.
        assert_eq!(
            m.tick(T),
            [Device(DeviceEvent::Disconnected), Status(Absent)]
        );
        assert!(good.closed());
        assert_eq!(m.status(), Some(Absent));
    }

    #[test]
    fn engine_releases_everything_on_off_and_on_disconnect() {
        use crate::config::{Action, Config, KeyCombo};
        use crate::engine::Engine;
        use crate::inject::MockInjector;
        use crate::launch::MockLauncher;

        let mut config = Config::default();
        let combo = KeyCombo {
            modifiers: vec![],
            key: "A".into(),
        };
        config.layers[0]
            .bindings
            .insert("In".into(), Action::Held { combo });
        let press_in = [0x04, 0x07, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let (f, clock) = setup();
        let mut m = manager(&f, &clock);
        let mut engine = Engine::new(&config);
        let mut mock = MockInjector::new();
        let mut feed = |events: Vec<ConnectionEvent>, mock: &mut MockInjector| {
            for e in events {
                if let Device(e) = e {
                    engine.handle(&e, mock, &mut MockLauncher::new()).unwrap();
                }
            }
        };

        // Off with the key held.
        let dev = Shared::default();
        dev.handshake(600);
        dev.0.borrow_mut().push_input(press_in);
        f.plug(&dev);
        feed(m.tick(T), &mut mock);
        feed(m.tick(T), &mut mock);
        assert_eq!(mock.keys_down().len(), 1);
        feed(m.set_enabled(false), &mut mock);
        assert!(mock.keys_down().is_empty());

        // The device goes away with the key held.
        let dev = Shared::default();
        dev.handshake(600);
        dev.0.borrow_mut().push_input(press_in);
        f.plug(&dev);
        feed(m.set_enabled(true), &mut mock);
        feed(m.tick(T), &mut mock);
        feed(m.tick(T), &mut mock);
        assert_eq!(mock.keys_down().len(), 1);
        feed(m.tick(T), &mut mock); // empty input script: disconnect
        assert_eq!(m.status(), Some(Absent));
        assert!(mock.keys_down().is_empty());
    }

    #[test]
    fn transport_errors_emit_disconnected_without_panicking() {
        let (f, clock) = setup();
        let dev = Shared::default();
        dev.handshake(600);
        f.plug(&dev);
        let mut m = manager(&f, &clock);
        assert_eq!(m.tick(T), CONNECTED);

        // A failed re-auth is a disconnect.
        clock.advance(Duration::from_secs(590));
        assert_eq!(
            m.tick(T),
            [Device(DeviceEvent::Disconnected), Status(Absent)]
        );

        // A handle closed under us is a disconnect too.
        let dev = Shared::default();
        dev.handshake(600);
        f.plug(&dev);
        clock.advance(RETRY_INTERVAL);
        assert_eq!(m.tick(T), CONNECTED);
        dev.0.borrow_mut().close();
        assert_eq!(
            m.tick(T),
            [Device(DeviceEvent::Disconnected), Status(Absent)]
        );
    }
}
