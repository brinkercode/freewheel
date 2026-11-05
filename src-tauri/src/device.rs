//! The device loop: one thread owns the connection manager, the mapping
//! engine and the key injector. The tray drives it with [`Command`]s and
//! hears back an [`Update`] when the connection status or the battery changes.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use freewheel_core::battery::Battery;
use freewheel_core::clock::Clock;
use freewheel_core::connection::{ConnectionEvent, ConnectionManager, ConnectionStatus};
use freewheel_core::engine::Engine;
use freewheel_core::hid::{HidError, HidTransport};
use freewheel_core::inject::KeyInjector;
use freewheel_core::launch::Launcher;
use freewheel_core::report::DeviceEvent;
use tracing::warn;

/// How long one loop turn waits for an input report or a command.
const POLL: Duration = Duration::from_millis(20);

pub enum Command {
    /// The tray's On/Off toggle.
    SetEnabled(bool),
    /// The quit hook: release every held key, then acknowledge and stop.
    Quit(Sender<()>),
}

/// What the tray icon shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    On,
    Off,
    NotConnected,
}

/// What the device loop tells the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Update {
    State(TrayState),
    Battery(Battery),
}

impl From<ConnectionStatus> for TrayState {
    fn from(status: ConnectionStatus) -> Self {
        match status {
            ConnectionStatus::Connected => TrayState::On,
            ConnectionStatus::Off => TrayState::Off,
            ConnectionStatus::Absent | ConnectionStatus::AuthFailed => TrayState::NotConnected,
        }
    }
}

/// Asks the device loop to release every held key and waits, at most
/// `timeout`, until it has. Returns whether it did.
pub fn shutdown(commands: &Sender<Command>, timeout: Duration) -> bool {
    let (done, released) = mpsc::channel();
    commands.send(Command::Quit(done)).is_ok() && released.recv_timeout(timeout).is_ok()
}

/// Runs the device loop until [`Command::Quit`] or until every sender is gone.
pub fn run<T, O, C, F>(
    mut manager: ConnectionManager<T, O, C, F>,
    mut engine: Engine,
    clock: &dyn Clock,
    injector: &mut dyn KeyInjector,
    launcher: &mut dyn Launcher,
    commands: Receiver<Command>,
    mut on_update: impl FnMut(Update),
) where
    T: HidTransport,
    O: FnMut() -> Result<T, HidError>,
    C: Clock + Clone,
    F: Fn(u64) -> u64,
{
    loop {
        // While connected the read itself waits; otherwise the command wait does.
        let wait = if manager.status() == Some(ConnectionStatus::Connected) {
            Duration::ZERO
        } else {
            POLL
        };
        let mut events = match commands.recv_timeout(wait) {
            Ok(Command::SetEnabled(on)) => manager.set_enabled(on),
            Ok(Command::Quit(done)) => {
                release(&mut engine, injector);
                let _ = done.send(());
                return;
            }
            Err(RecvTimeoutError::Timeout) => Vec::new(),
            Err(RecvTimeoutError::Disconnected) => return release(&mut engine, injector),
        };
        events.extend(manager.tick(POLL));
        for event in events {
            match event {
                ConnectionEvent::Status(status) => on_update(Update::State(status.into())),
                ConnectionEvent::Device(event) => {
                    if let DeviceEvent::Battery { level, charging } = event {
                        on_update(Update::Battery(Battery { level, charging }));
                    }
                    if let Err(e) = engine.handle(&event, injector, launcher) {
                        warn!(error = %e, "device event failed");
                    }
                }
            }
        }
        if let Err(e) = engine.tick(clock, injector) {
            warn!(error = %e, "shuttle repeat failed");
        }
        // LED and dial override output are not wired to the device yet; drop
        // what the engine queued so it cannot grow.
        engine.take_reports();
        engine.take_override_ticks();
        engine.take_launch_errors();
    }
}

fn release(engine: &mut Engine, injector: &mut dyn KeyInjector) {
    if let Err(e) = engine.release_all(injector) {
        warn!(error = %e, "releasing held keys failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freewheel_core::clock::SystemClock;
    use freewheel_core::config::{Action, Config, KeyCombo};
    use freewheel_core::hid::FakeTransport;
    use freewheel_core::inject::MockInjector;
    use freewheel_core::launch::MockLauncher;
    use freewheel_core::report::{DeviceEvent, Key};
    use std::thread;

    const WAIT: Duration = Duration::from_secs(5);

    type Opener = fn() -> Result<FakeTransport, HidError>;

    /// A manager whose device is never there.
    fn absent_manager() -> ConnectionManager<FakeTransport, Opener, SystemClock, fn(u64) -> u64> {
        ConnectionManager::new(|| Err(HidError::Disconnected), SystemClock, |c| c)
    }

    fn engine_holding_in(injector: &mut MockInjector) -> Engine {
        let mut config = Config::default();
        let combo = KeyCombo {
            modifiers: vec![],
            key: "S".into(),
        };
        config.layers[0]
            .bindings
            .insert(format!("{:?}", Key::In), Action::Held { combo });
        let mut engine = Engine::new(&config);
        engine
            .handle(
                &DeviceEvent::Press(Key::In),
                injector,
                &mut MockLauncher::new(),
            )
            .unwrap();
        engine
    }

    #[test]
    fn tray_state_follows_the_connection_status() {
        assert_eq!(TrayState::from(ConnectionStatus::Connected), TrayState::On);
        assert_eq!(TrayState::from(ConnectionStatus::Off), TrayState::Off);
        assert_eq!(
            TrayState::from(ConnectionStatus::Absent),
            TrayState::NotConnected
        );
        assert_eq!(
            TrayState::from(ConnectionStatus::AuthFailed),
            TrayState::NotConnected
        );
    }

    #[test]
    fn toggle_switches_the_connection_manager_off_and_on() {
        let (tx, rx) = mpsc::channel();
        let (state_tx, states) = mpsc::channel();
        let mut injector = MockInjector::new();
        thread::scope(|s| {
            let injector = &mut injector;
            s.spawn(move || {
                let on_state = move |st| state_tx.send(st).unwrap();
                let mut launcher = MockLauncher::new();
                run(
                    absent_manager(),
                    Engine::new(&Config::default()),
                    &SystemClock,
                    injector,
                    &mut launcher,
                    rx,
                    on_state,
                );
            });
            assert_eq!(
                states.recv_timeout(WAIT),
                Ok(Update::State(TrayState::NotConnected))
            );
            tx.send(Command::SetEnabled(false)).unwrap();
            assert_eq!(states.recv_timeout(WAIT), Ok(Update::State(TrayState::Off)));
            tx.send(Command::SetEnabled(true)).unwrap();
            assert_eq!(
                states.recv_timeout(WAIT),
                Ok(Update::State(TrayState::NotConnected))
            );
            assert!(shutdown(&tx, WAIT));
        });
    }

    #[test]
    fn battery_reports_reach_the_tray() {
        let mut dev = FakeTransport::new();
        for (sub, payload) in [(0u8, 0x1234u64), (2, 0), (4, 300)] {
            let mut r = vec![0x06, sub];
            r.extend_from_slice(&payload.to_le_bytes());
            dev.push_feature(r);
        }
        dev.push_input([0x07, 1, 64]);
        let mut dev = Some(dev);
        let open = move || dev.take().ok_or(HidError::Disconnected);
        let manager = ConnectionManager::new(open, SystemClock, (|c: u64| !c) as fn(u64) -> u64);
        let (tx, rx) = mpsc::channel();
        let (update_tx, updates) = mpsc::channel();
        let mut injector = MockInjector::new();
        thread::scope(|s| {
            let injector = &mut injector;
            s.spawn(move || {
                let mut launcher = MockLauncher::new();
                run(
                    manager,
                    Engine::new(&Config::default()),
                    &SystemClock,
                    injector,
                    &mut launcher,
                    rx,
                    move |u| update_tx.send(u).unwrap(),
                );
            });
            assert_eq!(updates.recv_timeout(WAIT), Ok(Update::State(TrayState::On)));
            assert_eq!(
                updates.recv_timeout(WAIT),
                Ok(Update::Battery(Battery {
                    level: 64,
                    charging: true
                }))
            );
            assert!(shutdown(&tx, WAIT));
        });
    }

    #[test]
    fn quit_hook_releases_held_keys_before_acknowledging() {
        let mut injector = MockInjector::new();
        let engine = engine_holding_in(&mut injector);
        assert!(!injector.keys_down().is_empty());
        let (tx, rx) = mpsc::channel();
        thread::scope(|s| {
            let injector = &mut injector;
            s.spawn(move || {
                let mut launcher = MockLauncher::new();
                run(
                    absent_manager(),
                    engine,
                    &SystemClock,
                    injector,
                    &mut launcher,
                    rx,
                    |_| {},
                );
            });
            assert!(shutdown(&tx, WAIT));
        });
        assert!(injector.keys_down().is_empty());
    }

    #[test]
    fn shutdown_reports_a_loop_that_is_gone() {
        let (tx, rx) = mpsc::channel();
        drop(rx);
        assert!(!shutdown(&tx, WAIT));
    }
}
