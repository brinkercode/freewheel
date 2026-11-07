//! The Resolve hand-off (ADR-08): poll the process list for the configured
//! Resolve process, switch Off with the replug notification when it starts,
//! and switch back On when it quits only if this watcher did the switching
//! [A10]. A manual Off or On always wins.

use std::collections::BTreeSet;
use std::time::Duration;

use crate::config::Settings;

/// How often the process list is polled (ADR-08 default).
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Shown when Resolve starts and freewheel switches Off. Until the hardware
/// spike confirms Resolve re-acquires the device on its own, it keeps the
/// replug hint (ADR-08 default).
pub const REPLUG_NOTIFICATION: &str = "DaVinci Resolve started, so freewheel switched Off. \
     If Resolve does not respond to the Speed Editor, unplug it and plug it back in.";

/// Answers whether a process with an exact name is running.
pub trait ProcessLister {
    fn is_running(&mut self, name: &str) -> bool;
}

/// A process list the tests start and stop processes in.
#[derive(Debug, Clone, Default)]
pub struct FakeProcessLister {
    running: BTreeSet<String>,
    polls: usize,
}

impl FakeProcessLister {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn start(&mut self, name: &str) {
        self.running.insert(name.to_owned());
    }
    pub fn stop(&mut self, name: &str) {
        self.running.remove(name);
    }
    /// How many times the list was asked.
    pub fn polls(&self) -> usize {
        self.polls
    }
}

impl ProcessLister for FakeProcessLister {
    fn is_running(&mut self, name: &str) -> bool {
        self.polls += 1;
        self.running.contains(name)
    }
}

/// What the app must do after a poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandOff {
    /// Resolve started: switch Off and show `notification`.
    SwitchOff { notification: &'static str },
    /// Resolve quit after it had switched freewheel Off: switch back On.
    SwitchOn,
}

/// Tracks Resolve and whether the current Off is its doing.
#[derive(Debug, Clone)]
pub struct ResolveWatcher {
    process_name: String,
    /// `settings.step_aside_for_resolve`: switch Off while Resolve runs.
    step_aside: bool,
    resolve_running: bool,
    off_by_resolve: bool,
}

impl ResolveWatcher {
    pub fn new(process_name: impl Into<String>) -> Self {
        Self {
            process_name: process_name.into(),
            step_aside: true,
            resolve_running: false,
            off_by_resolve: false,
        }
    }

    /// A watcher for the process name in the config.
    pub fn from_settings(settings: &Settings) -> Self {
        let mut watcher = Self::new(settings.resolve_process_name.clone());
        watcher.set_step_aside(settings.step_aside_for_resolve);
        watcher
    }

    /// Whether freewheel steps aside (switches Off) while Resolve runs.
    pub fn step_aside(&self) -> bool {
        self.step_aside
    }

    /// The Resolve tab's switch. Turned off while stepped aside, the next
    /// poll switches freewheel back On.
    pub fn set_step_aside(&mut self, on: bool) {
        self.step_aside = on;
    }

    pub fn process_name(&self) -> &str {
        &self.process_name
    }

    /// True while freewheel is Off because Resolve started.
    pub fn off_by_resolve(&self) -> bool {
        self.off_by_resolve
    }

    /// Polls once. `enabled` is whether freewheel is On right now.
    pub fn poll(&mut self, lister: &mut impl ProcessLister, enabled: bool) -> Option<HandOff> {
        let running = lister.is_running(&self.process_name);
        let was_running = std::mem::replace(&mut self.resolve_running, running);
        if !self.step_aside {
            if self.off_by_resolve {
                self.off_by_resolve = false;
                return Some(HandOff::SwitchOn);
            }
            return None;
        }
        match (was_running, running) {
            (false, true) if enabled => {
                self.off_by_resolve = true;
                Some(HandOff::SwitchOff {
                    notification: REPLUG_NOTIFICATION,
                })
            }
            (true, false) if self.off_by_resolve => {
                self.off_by_resolve = false;
                Some(HandOff::SwitchOn)
            }
            _ => None,
        }
    }

    /// The user switched On or Off by hand: the current state is theirs,
    /// so Resolve quitting must not change it.
    pub fn manual_switch(&mut self) {
        self.off_by_resolve = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: &str = "Resolve";

    #[test]
    fn process_name_is_read_from_config() {
        let settings = Settings {
            resolve_process_name: "MyResolve".into(),
            ..Settings::default()
        };
        assert_eq!(
            ResolveWatcher::from_settings(&settings).process_name(),
            "MyResolve"
        );
    }

    #[test]
    fn default_process_name_matches_the_os() {
        let expected = if cfg!(windows) {
            "Resolve.exe"
        } else {
            "Resolve"
        };
        let w = ResolveWatcher::from_settings(&Settings::default());
        assert_eq!(w.process_name(), expected);
    }

    #[test]
    fn resolve_start_switches_off_with_replug_notification() {
        let (mut ps, mut w) = (FakeProcessLister::new(), ResolveWatcher::new(NAME));
        assert_eq!(w.poll(&mut ps, true), None);
        ps.start(w.process_name());
        let action = w.poll(&mut ps, true);
        assert_eq!(
            action,
            Some(HandOff::SwitchOff {
                notification: REPLUG_NOTIFICATION
            })
        );
        assert!(REPLUG_NOTIFICATION.contains("unplug"));
        assert!(w.off_by_resolve());
        // Still running: nothing more to do.
        assert_eq!(w.poll(&mut ps, false), None);
        assert_eq!(ps.polls(), 3);
    }

    #[test]
    fn resolve_quit_switches_on_when_auto_off_did_it() {
        let (mut ps, mut w) = (FakeProcessLister::new(), ResolveWatcher::new(NAME));
        ps.start(w.process_name());
        assert!(matches!(
            w.poll(&mut ps, true),
            Some(HandOff::SwitchOff { .. })
        ));
        ps.stop(NAME);
        assert_eq!(w.poll(&mut ps, false), Some(HandOff::SwitchOn));
        assert!(!w.off_by_resolve());
        assert_eq!(w.poll(&mut ps, true), None);
    }

    #[test]
    fn manual_off_before_resolve_is_respected() {
        let (mut ps, mut w) = (FakeProcessLister::new(), ResolveWatcher::new(NAME));
        w.manual_switch();
        ps.start(w.process_name());
        assert_eq!(w.poll(&mut ps, false), None);
        ps.stop(NAME);
        assert_eq!(w.poll(&mut ps, false), None);
    }

    #[test]
    fn manual_switch_while_resolve_runs_is_respected_on_quit() {
        let (mut ps, mut w) = (FakeProcessLister::new(), ResolveWatcher::new(NAME));
        ps.start(w.process_name());
        assert!(w.poll(&mut ps, true).is_some());
        // The user switches On, then Off again, by hand.
        w.manual_switch();
        ps.stop(NAME);
        assert_eq!(w.poll(&mut ps, false), None);
    }

    #[test]
    fn other_processes_are_ignored() {
        let (mut ps, mut w) = (FakeProcessLister::new(), ResolveWatcher::new(NAME));
        ps.start("Resolve Helper");
        assert_eq!(w.poll(&mut ps, true), None);
    }

    #[test]
    fn step_aside_follows_the_setting() {
        assert!(ResolveWatcher::new(NAME).step_aside());
        assert!(ResolveWatcher::from_settings(&Settings::default()).step_aside());
        let off = Settings {
            step_aside_for_resolve: false,
            ..Settings::default()
        };
        assert!(!ResolveWatcher::from_settings(&off).step_aside());
    }

    #[test]
    fn with_step_aside_off_resolve_starting_leaves_freewheel_on() {
        let (mut ps, mut w) = (FakeProcessLister::new(), ResolveWatcher::new(NAME));
        w.set_step_aside(false);
        ps.start(NAME);
        assert_eq!(w.poll(&mut ps, true), None);
        assert!(!w.off_by_resolve());
        ps.stop(NAME);
        assert_eq!(w.poll(&mut ps, true), None);
    }

    #[test]
    fn turning_step_aside_off_while_stepped_aside_switches_back_on() {
        let (mut ps, mut w) = (FakeProcessLister::new(), ResolveWatcher::new(NAME));
        ps.start(NAME);
        assert!(matches!(
            w.poll(&mut ps, true),
            Some(HandOff::SwitchOff { .. })
        ));
        w.set_step_aside(false);
        assert_eq!(w.poll(&mut ps, false), Some(HandOff::SwitchOn));
        assert_eq!(w.poll(&mut ps, true), None);
        // Turned back on while Resolve still runs: no surprise Off, only
        // the next Resolve start steps aside.
        w.set_step_aside(true);
        assert_eq!(w.poll(&mut ps, true), None);
        ps.stop(NAME);
        assert_eq!(w.poll(&mut ps, true), None);
        ps.start(NAME);
        assert!(matches!(
            w.poll(&mut ps, true),
            Some(HandOff::SwitchOff { .. })
        ));
    }
}
