//! The connection manager: opens the device through a factory, keeps it
//! authenticated, retries every 3 s while it is absent and closes the
//! handle on Off. Connection changes come out as [`ConnectionEvent`]s.
//!
//! It also records the Device test view's [`DeviceDiagnostics`] and, with
//! detailed logs on, logs every raw report both ways.

use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

use crate::auth::{Authenticator, Clock};
use crate::diagnostics::{self, DeviceDiagnostics, Recorder};
use crate::hid::{HidError, HidTransport};
use crate::report::{DeviceEvent, ReportParser};

/// How often the manager tries to open the device while it is absent.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(3);
/// After this long without an input report the manager re-runs the auth
/// handshake ([`ConnectionManager::with_idle_reauth`]). A Speed Editor that
/// slept (Bluetooth) or lost power over a PC sleep comes back
/// unauthenticated and silent; without this it would stay dead until the
/// session timeout (up to 10 minutes). A device that is gone fails the
/// handshake, so it is reported as disconnected and reopened.
pub const IDLE_REAUTH: Duration = Duration::from_secs(5);
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
    idle_reauth: Option<Duration>,
    last_activity: Instant,
    diag: Recorder,
    /// Successful authentications (connects and re-auths) so far.
    session: u64,
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
        let now = clock.now();
        Self {
            open,
            auth: Authenticator::new(clock.clone(), respond),
            retry_at: now,
            clock,
            parser: ReportParser::default(),
            handle: None,
            enabled: true,
            status: None,
            idle_reauth: None,
            last_activity: now,
            diag: Recorder::new(now),
            session: 0,
        }
    }

    /// Counts successful authentications (connects and re-auths, idle ones
    /// included). A device that slept or was reset by another app comes back
    /// with its LEDs off, so whoever owns the output state sends it again
    /// when this changes.
    pub fn session(&self) -> u64 {
        self.session
    }

    /// Re-runs the handshake after `idle` without input (see [`IDLE_REAUTH`]).
    pub fn with_idle_reauth(mut self, idle: Duration) -> Self {
        self.idle_reauth = Some(idle);
        self
    }

    /// The last published status; `None` before the first tick.
    pub fn status(&self) -> Option<ConnectionStatus> {
        self.status
    }

    /// The Device test view's data now.
    pub fn diagnostics(&self) -> DeviceDiagnostics {
        let next =
            self.handle
                .as_ref()
                .and(self.auth.reauth_due())
                .map(|due| match self.idle_reauth {
                    Some(idle) => due.min(self.last_activity + idle),
                    None => due,
                });
        self.diag.snapshot(self.clock.now(), next)
    }

    /// Whether the diagnostics changed since the last call.
    pub fn take_diagnostics_changed(&mut self) -> bool {
        self.diag.take_changed()
    }

    /// Writes an output report (wheel mode, LEDs) to the open device.
    pub fn write(&mut self, report: &[u8]) -> Result<(), HidError> {
        let Some(t) = self.handle.as_mut() else {
            return Err(HidError::Disconnected);
        };
        let result = t.write(report).map(|_| ());
        let now = self.clock.now();
        match &result {
            Ok(()) => debug!(report = %diagnostics::hex(report), "device report out"),
            Err(e) => {
                warn!(report = %diagnostics::hex(report), error = %e, "device report out failed")
            }
        }
        self.diag.output(report, result.is_err(), now);
        result
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

    /// How long until the next attempt to open the device (zero when it is
    /// due); `None` while a device is open or the manager is Off.
    pub fn next_attempt_in(&self) -> Option<Duration> {
        (self.enabled && self.handle.is_none())
            .then(|| self.retry_at.saturating_duration_since(self.clock.now()))
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
        let now = self.clock.now();
        let idle = self
            .idle_reauth
            .is_some_and(|d| now.saturating_duration_since(self.last_activity) >= d);
        let mut read = Ok(());
        if self.auth.is_due() || idle {
            let why = if idle {
                "idle check"
            } else {
                "session timeout"
            };
            match self.auth.authenticate(t) {
                Ok(session) => {
                    let took = self.clock.now().saturating_duration_since(now);
                    if idle {
                        debug!(
                            session_s = session.as_secs(),
                            took_ms = took.as_millis() as u64,
                            "re-authenticated ({why})"
                        );
                    } else {
                        info!(
                            session_s = session.as_secs(),
                            took_ms = took.as_millis() as u64,
                            "re-authenticated ({why})"
                        );
                    }
                    self.diag.auth_passed(session, self.clock.now());
                    self.last_activity = self.clock.now();
                    self.session += 1;
                }
                Err(e) => {
                    let took = self.clock.now().saturating_duration_since(now);
                    warn!(error = %e, took_ms = took.as_millis() as u64, "re-auth failed ({why}): treating the device as gone");
                    self.diag.auth_failed(e.to_string());
                    read = Err(HidError::Disconnected);
                }
            }
        }
        let read = read.and_then(|()| {
            let mut buf = [0u8; REPORT_BUF];
            let n = t.read_timeout(&mut buf, read_timeout)?;
            Ok(buf[..n.min(REPORT_BUF)].to_vec())
        });
        match read {
            Ok(report) if report.is_empty() => {}
            // Reports we cannot decode are skipped, not fatal.
            Ok(report) => {
                let now = self.clock.now();
                self.last_activity = now;
                let parsed = self.parser.parse(&report);
                debug!(report = %diagnostics::hex(&report), "device report in");
                if let Err(e) = &parsed {
                    debug!(error = %e, "device report skipped");
                }
                self.diag.input(&report, &parsed, self.parser.held(), now);
                if let Ok(evs) = parsed {
                    events.extend(evs.into_iter().map(ConnectionEvent::Device));
                }
            }
            Err(e) => {
                info!(error = %e, "Speed Editor gone");
                self.drop_handle(&mut events);
                self.retry_later(ConnectionStatus::Absent, &mut events);
            }
        }
        events
    }

    fn connect(&mut self, events: &mut Vec<ConnectionEvent>) {
        let mut t = match (self.open)() {
            Ok(t) => t,
            Err(e) => {
                let text = e.to_string();
                if self.diagnostics().last_open_error.as_deref() != Some(text.as_str()) {
                    info!(error = %e, "no Speed Editor to open (retrying every 3 s)");
                }
                self.diag.open_failed(text);
                return self.retry_later(ConnectionStatus::Absent, events);
            }
        };
        let info = t.info();
        info!(?info, "Speed Editor opened, authenticating");
        self.diag.opened(info);
        let started = self.clock.now();
        match self.auth.authenticate(&mut t) {
            Ok(session) => {
                let took = self.clock.now().saturating_duration_since(started);
                info!(
                    session_s = session.as_secs(),
                    took_ms = took.as_millis() as u64,
                    "Speed Editor authenticated"
                );
                self.diag.auth_passed(session, self.clock.now());
            }
            Err(e) => {
                warn!(error = %e, "Speed Editor auth failed (retrying in 3 s)");
                self.diag.auth_failed(e.to_string());
                t.close();
                return self.retry_later(ConnectionStatus::AuthFailed, events);
            }
        }
        self.last_activity = self.clock.now();
        self.handle = Some(t);
        self.session += 1;
        self.set_status(ConnectionStatus::Connected, events);
        self.diag.connected();
        events.push(ConnectionEvent::Device(DeviceEvent::Connected));
    }

    fn drop_handle(&mut self, events: &mut Vec<ConnectionEvent>) {
        if let Some(mut t) = self.handle.take() {
            t.close();
            self.parser = ReportParser::default();
            self.diag.disconnected();
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
            self.diag.status(status);
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
        fn info(&self) -> Option<crate::diagnostics::DeviceInfo> {
            self.0.borrow().info()
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
    fn session_counts_connects_and_reauths_and_writes_need_a_device() {
        let (f, clock) = setup();
        let bad = Shared::default();
        bad.handshake(5);
        f.plug(&bad);
        let mut m = manager(&f, &clock);
        assert_eq!(m.tick(T), [Status(AuthFailed)]);
        assert_eq!(m.session(), 0);
        assert_eq!(m.write(&[0x02, 0, 0, 0, 0]), Err(HidError::Disconnected));
        // The next open attempt is a retry interval away, counting down.
        assert_eq!(m.next_attempt_in(), Some(RETRY_INTERVAL));
        clock.advance(Duration::from_secs(1));
        assert_eq!(
            m.next_attempt_in(),
            Some(RETRY_INTERVAL - Duration::from_secs(1))
        );
        clock.advance(RETRY_INTERVAL - Duration::from_secs(1));

        let dev = Shared::default();
        dev.handshake(300);
        f.plug(&dev);
        assert_eq!(m.next_attempt_in(), Some(Duration::ZERO));
        assert_eq!(m.tick(T), CONNECTED);
        assert_eq!(m.next_attempt_in(), None);
        assert_eq!(m.session(), 1);
        assert_eq!(m.write(&[0x02, 1, 0, 0, 0]), Ok(()));
        assert_eq!(dev.writes(), 4);

        // A read without a re-auth keeps the session; the re-auth starts one.
        dev.timeouts(1);
        m.tick(T);
        assert_eq!(m.session(), 1);
        clock.advance(Duration::from_secs(290));
        dev.handshake(300).timeouts(1);
        m.tick(T);
        assert_eq!(m.session(), 2);
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

    #[test]
    fn an_idle_device_is_re_authenticated_and_a_silent_one_reopened() {
        // Bluetooth sleep / PC sleep: the device comes back unauthenticated
        // and silent. The idle check re-runs the handshake; one that fails
        // reports the device gone, and the next open authenticates again.
        let (f, clock) = setup();
        let dev = Shared::default();
        dev.handshake(600);
        f.plug(&dev);
        let mut m = manager(&f, &clock).with_idle_reauth(IDLE_REAUTH);
        assert_eq!(m.tick(T), CONNECTED);

        // Input keeps it from being idle.
        clock.advance(IDLE_REAUTH - Duration::from_millis(1));
        dev.0.borrow_mut().push_input([0x07, 0, 50]);
        assert_eq!(m.tick(T).len(), 1);
        clock.advance(IDLE_REAUTH - Duration::from_millis(1));
        dev.timeouts(1);
        assert_eq!(m.tick(T), []);
        assert_eq!(dev.writes(), 3, "no handshake while input is recent");

        // Idle: the handshake runs again and passes.
        clock.advance(Duration::from_millis(1));
        dev.handshake(600).timeouts(1);
        assert_eq!(m.tick(T), []);
        assert_eq!(dev.writes(), 6);
        assert_eq!(m.diagnostics().auth.passed, 2);

        // Asleep: the next idle handshake fails, so it is gone.
        clock.advance(IDLE_REAUTH);
        assert_eq!(
            m.tick(T),
            [Device(DeviceEvent::Disconnected), Status(Absent)]
        );
        assert!(m.diagnostics().auth.last_error.is_some());

        // Awake again: reopened and authenticated on the next retry.
        let woke = Shared::default();
        woke.handshake(600);
        f.plug(&woke);
        clock.advance(RETRY_INTERVAL);
        assert_eq!(m.tick(T), CONNECTED);
        let d = m.diagnostics();
        assert_eq!((d.connects, d.disconnects, d.auth.passed), (2, 1, 3));
    }

    #[test]
    fn writes_go_to_the_open_device_and_into_the_diagnostics() {
        let (f, clock) = setup();
        let mut m = manager(&f, &clock);
        assert_eq!(
            m.write(&[0x03, 0, 0, 0, 0, 0, 0xff]),
            Err(HidError::Disconnected)
        );
        let dev = Shared::default();
        dev.handshake(600);
        f.plug(&dev);
        clock.advance(RETRY_INTERVAL);
        m.tick(T);
        assert_eq!(m.write(&[0x03, 3, 0, 0, 0, 0, 0xff]), Ok(()));
        assert_eq!(
            dev.0.borrow().written().last(),
            Some(&crate::hid::Written::Output(vec![
                0x03, 3, 0, 0, 0, 0, 0xff
            ]))
        );
        let d = m.diagnostics();
        assert_eq!(d.reports[0].direction, crate::diagnostics::Direction::Out);
        assert_eq!(
            d.reports[0].decoded,
            "set wheel mode 3 (absolute, dead zero)"
        );
        assert!(!d.reports[0].failed);
    }

    #[test]
    fn diagnostics_follow_open_auth_reports_and_disconnect() {
        let (f, clock) = setup();
        let mut m = manager(&f, &clock);
        m.tick(T);
        assert_eq!(
            m.diagnostics().last_open_error.as_deref(),
            Some("HID I/O error: no device")
        );
        assert!(m.take_diagnostics_changed());
        assert!(!m.take_diagnostics_changed());

        let bad = Shared::default();
        bad.handshake(5);
        f.plug(&bad);
        clock.advance(RETRY_INTERVAL);
        m.tick(T);
        let d = m.diagnostics();
        assert_eq!(d.auth.state, crate::diagnostics::AuthState::Failed);
        assert_eq!(
            d.auth.last_error.as_deref(),
            Some("auth rejected, status 06 04 (10 bytes), session 5 s")
        );

        let dev = Shared::default();
        dev.handshake(600);
        dev.0
            .borrow_mut()
            .set_info(crate::diagnostics::DeviceInfo {
                vendor_id: 0x1edb,
                product_id: 0xda0e,
                ..Default::default()
            })
            .push_input([0x04, 0x0f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        f.plug(&dev);
        clock.advance(RETRY_INTERVAL);
        m.tick(T);
        m.tick(T);
        let d = m.diagnostics();
        assert_eq!(d.status, Some(Connected));
        assert_eq!(d.device.as_ref().map(|i| i.product_id), Some(0xda0e));
        assert_eq!(d.auth.session_s, Some(600));
        assert_eq!(d.auth.next_in_s, Some(590));
        assert_eq!(d.held, ["CUT"]);
        assert_eq!(d.open_attempts, 3);
        m.tick(T); // empty script: gone
        let d = m.diagnostics();
        assert_eq!(d.status, Some(Absent));
        assert!(d.held.is_empty());
        assert_eq!(d.auth.next_in_s, None);
    }
}
