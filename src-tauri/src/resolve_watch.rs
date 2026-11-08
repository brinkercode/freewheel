//! The Resolve hand-off loop: polls the process list every
//! [`POLL_INTERVAL`](freewheel_core::resolve::POLL_INTERVAL), turns each
//! [`HandOff`] into a [`Command::SetEnabled`] for the device loop and shows
//! the replug notification. The tray's manual toggle reaches it as
//! [`Message::Manual`] so a manual Off or On always wins [A10]; the Resolve
//! tab's step-aside switch as [`Message::StepAside`].

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use freewheel_core::resolve::{HandOff, ProcessLister, ResolveWatcher};

use crate::device::Command;

/// What the hand-off tells the app about stepping aside for Resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pause<'a> {
    /// Stepped aside for Resolve: show this replug notification.
    Started(&'a str),
    /// No longer stepped aside: Resolve quit or the user switched by hand.
    Ended,
}

/// What the tray and the settings window tell the hand-off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// The user switched freewheel On (`true`) or Off by hand.
    Manual(bool),
    /// `settings.step_aside_for_resolve` changed; applies from the next poll.
    StepAside(bool),
}

/// Runs until the message sender or the device loop is gone.
/// `enabled` is whether freewheel starts On; `pause` hears each [`Pause`].
pub fn run(
    mut watcher: ResolveWatcher,
    lister: &mut impl ProcessLister,
    mut enabled: bool,
    interval: Duration,
    manual: Receiver<Message>,
    commands: Sender<Command>,
    mut pause: impl FnMut(Pause<'_>),
) {
    let mut next_poll = Instant::now();
    loop {
        match manual.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
            Ok(Message::Manual(on)) => {
                enabled = on;
                watcher.manual_switch();
                pause(Pause::Ended);
            }
            Ok(Message::StepAside(on)) => {
                tracing::info!(on, "step aside for Resolve changed");
                watcher.set_step_aside(on);
                // Poll now, so turning it off while stepped aside switches
                // back On at once.
                next_poll = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {
                next_poll = Instant::now() + interval;
                let on = match watcher.poll(lister, enabled) {
                    Some(HandOff::SwitchOff { notification }) => {
                        pause(Pause::Started(notification));
                        false
                    }
                    Some(HandOff::SwitchOn) => {
                        pause(Pause::Ended);
                        true
                    }
                    None => continue,
                };
                enabled = on;
                if commands.send(Command::SetEnabled(on)).is_err() {
                    return;
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freewheel_core::resolve::REPLUG_NOTIFICATION;
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;

    const NAME: &str = "Resolve";
    const TICK: Duration = Duration::from_millis(5);
    const WAIT: Duration = Duration::from_secs(5);

    /// A lister the test thread starts and stops Resolve in.
    #[derive(Clone, Default)]
    struct SharedLister(Arc<Mutex<bool>>);

    impl ProcessLister for SharedLister {
        fn is_running(&mut self, name: &str) -> bool {
            name == NAME && *self.0.lock().unwrap()
        }
    }

    fn enabled(cmd: Command) -> bool {
        match cmd {
            Command::SetEnabled(on) => on,
            Command::Quit(_) => panic!("unexpected Quit"),
            Command::Reload(_) => panic!("unexpected Reload"),
            Command::ExitOverride => panic!("unexpected ExitOverride"),
            Command::FrontApp(_) => panic!("unexpected FrontApp"),
        }
    }

    #[test]
    fn resolve_start_switches_off_notifies_and_quit_switches_back_on() {
        let resolve = SharedLister::default();
        let (manual, manual_rx) = mpsc::channel();
        let (cmd_tx, commands) = mpsc::channel();
        let (note_tx, notes) = mpsc::channel();
        let mut lister = resolve.clone();
        let handle = thread::spawn(move || {
            run(
                ResolveWatcher::new(NAME),
                &mut lister,
                true,
                TICK,
                manual_rx,
                cmd_tx,
                |pause: Pause<'_>| {
                    let note = match pause {
                        Pause::Started(text) => Some(text.to_owned()),
                        Pause::Ended => None,
                    };
                    note_tx.send(note).unwrap()
                },
            )
        });
        *resolve.0.lock().unwrap() = true;
        assert!(!enabled(commands.recv_timeout(WAIT).unwrap()));
        assert_eq!(
            notes.recv_timeout(WAIT).unwrap().as_deref(),
            Some(REPLUG_NOTIFICATION)
        );
        *resolve.0.lock().unwrap() = false;
        assert!(enabled(commands.recv_timeout(WAIT).unwrap()));
        assert_eq!(notes.recv_timeout(WAIT).unwrap(), None, "pause not ended");
        drop(manual);
        handle.join().unwrap();
    }

    #[test]
    fn a_manual_toggle_while_resolve_runs_is_respected_on_quit() {
        let resolve = SharedLister::default();
        let (manual, manual_rx) = mpsc::channel();
        let (cmd_tx, commands) = mpsc::channel();
        let (pause_tx, pauses) = mpsc::channel();
        let mut lister = resolve.clone();
        let handle = thread::spawn(move || {
            run(
                ResolveWatcher::new(NAME),
                &mut lister,
                true,
                TICK,
                manual_rx,
                cmd_tx,
                |pause: Pause<'_>| pause_tx.send(matches!(pause, Pause::Started(_))).unwrap(),
            )
        });
        *resolve.0.lock().unwrap() = true;
        assert!(!enabled(commands.recv_timeout(WAIT).unwrap()));
        // The user switches On and Off again by hand from the tray.
        manual.send(Message::Manual(true)).unwrap();
        manual.send(Message::Manual(false)).unwrap();
        thread::sleep(TICK * 4);
        *resolve.0.lock().unwrap() = false;
        thread::sleep(TICK * 10);
        drop(manual);
        handle.join().unwrap();
        assert!(commands.try_recv().is_err(), "manual Off was overridden");
        // Paused for Resolve, then no longer once switched by hand.
        assert_eq!(pauses.try_iter().collect::<Vec<_>>(), [true, false, false]);
    }

    #[test]
    fn stops_when_the_device_loop_is_gone() {
        let resolve = SharedLister(Arc::new(Mutex::new(true)));
        let (_manual, manual_rx) = mpsc::channel();
        let (cmd_tx, commands) = mpsc::channel();
        drop(commands);
        let mut lister = resolve;
        run(
            ResolveWatcher::new(NAME),
            &mut lister,
            true,
            TICK,
            manual_rx,
            cmd_tx,
            |_| {},
        );
    }

    #[test]
    fn with_step_aside_off_resolve_starting_keeps_freewheel_on() {
        let resolve = SharedLister::default();
        let (manual, manual_rx) = mpsc::channel();
        let (cmd_tx, commands) = mpsc::channel();
        let mut lister = resolve.clone();
        let mut watcher = ResolveWatcher::new(NAME);
        watcher.set_step_aside(false);
        let handle = thread::spawn(move || {
            run(watcher, &mut lister, true, TICK, manual_rx, cmd_tx, |p| {
                panic!("paused: {p:?}")
            })
        });
        *resolve.0.lock().unwrap() = true;
        thread::sleep(TICK * 10);
        drop(manual);
        handle.join().unwrap();
        assert!(commands.try_recv().is_err(), "switched for Resolve");
    }

    #[test]
    fn turning_step_aside_off_while_stepped_aside_switches_back_on() {
        let resolve = SharedLister(Arc::new(Mutex::new(true)));
        let (manual, manual_rx) = mpsc::channel();
        let (cmd_tx, commands) = mpsc::channel();
        let (pause_tx, pauses) = mpsc::channel();
        let mut lister = resolve.clone();
        let handle = thread::spawn(move || {
            run(
                ResolveWatcher::new(NAME),
                &mut lister,
                true,
                // Long enough that only the message makes it poll again.
                Duration::from_secs(60),
                manual_rx,
                cmd_tx,
                |pause: Pause<'_>| pause_tx.send(matches!(pause, Pause::Started(_))).unwrap(),
            )
        });
        assert!(!enabled(commands.recv_timeout(WAIT).unwrap()));
        manual.send(Message::StepAside(false)).unwrap();
        assert!(enabled(commands.recv_timeout(WAIT).unwrap()));
        drop(manual);
        handle.join().unwrap();
        assert_eq!(pauses.try_iter().collect::<Vec<_>>(), [true, false]);
    }
}
