//! Dial overrides: system volume, foreground-app volume and brightness
//! behind control traits, so the engine can be tested without OS calls. The
//! OS backends live in the app crate. Controls can be slow, so they run on an
//! [`OverrideWorker`] thread fed by a coalescing queue and never stall key
//! handling; brightness is called at most once per [`BRIGHTNESS_INTERVAL`].

use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tracing::warn;

use crate::clock::Clock;
use crate::config::{OverrideMode, OverrideSettings};

/// Shortest gap between two brightness calls (A19).
pub const BRIGHTNESS_INTERVAL: Duration = Duration::from_millis(100);

/// An error from a dial override control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlError(pub String);

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dial override failed: {}", self.0)
    }
}

impl std::error::Error for ControlError {}

/// The system output volume; `step` is a fraction of full scale.
pub trait VolumeControl: Send {
    fn step_up(&mut self, step: f32) -> Result<(), ControlError>;
    fn step_down(&mut self, step: f32) -> Result<(), ControlError>;
}

/// An OS output device's volume scalar in 0..1, read and written by a backend.
pub trait VolumeScalar: Send {
    /// The current scalar, or `None` when the device has no settable volume.
    fn get(&mut self) -> Result<Option<f32>, ControlError>;
    fn set(&mut self, value: f32) -> Result<(), ControlError>;
}

/// A [`VolumeControl`] that steps a [`VolumeScalar`], clamped to 0..1.
pub struct ScalarVolume<S>(pub S);

impl<S: VolumeScalar> ScalarVolume<S> {
    fn add(&mut self, delta: f32) -> Result<(), ControlError> {
        let Some(current) = self.0.get()? else {
            return Err(ControlError(
                "the default output device has no settable volume".into(),
            ));
        };
        self.0.set((current + delta).clamp(0.0, 1.0))
    }
}

impl<S: VolumeScalar> VolumeControl for ScalarVolume<S> {
    fn step_up(&mut self, step: f32) -> Result<(), ControlError> {
        self.add(step)
    }
    fn step_down(&mut self, step: f32) -> Result<(), ControlError> {
        self.add(-step)
    }
}

/// The foreground app's volume; `step` is a fraction of full scale.
pub trait AppVolumeControl: Send {
    fn step_up(&mut self, step: f32) -> Result<(), ControlError>;
    fn step_down(&mut self, step: f32) -> Result<(), ControlError>;
    /// Called when the dial leaves its override mode and on release_all.
    fn release(&mut self) {}
    /// Called when the app-volume override is switched on.
    fn activate(&mut self) -> Result<(), ControlError> {
        Ok(())
    }
}

/// A gain of 0..1 shared with an audio thread.
#[derive(Debug)]
pub struct Gain(AtomicU32);

impl Gain {
    pub fn new(gain: f32) -> Self {
        Self(AtomicU32::new(gain.clamp(0.0, 1.0).to_bits()))
    }
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    /// Adds `delta` (negative is down), clamped to 0..1.
    pub fn step(&self, delta: f32) {
        let v = (self.get() + delta).clamp(0.0, 1.0);
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
}

/// Copies tapped samples to the output at `gain`; output past the input is
/// silence.
pub fn play_at_gain(input: &[f32], output: &mut [f32], gain: f32) {
    let n = input.len().min(output.len());
    for (o, i) in output[..n].iter_mut().zip(input) {
        *o = i * gain;
    }
    output[n..].fill(0.0);
}

/// Opens a muting tap of one process whose audio plays back at `gain`.
/// Dropping the tap tears it down.
pub trait TapOpener: Send {
    type Tap: Send;
    fn open(&mut self, pid: u32, gain: Arc<Gain>) -> Result<Self::Tap, ControlError>;
}

/// The macOS [`ProcessTap`] that [`FollowFocus`] drives: `retarget` opens a
/// tap of the pid at full gain (dropping the previous one), each step moves
/// the gain, and `release` drops the tap.
///
/// macOS has no public query for the audio-capture permission; it prompts
/// when the first tap is created. So asking only allows that attempt, and a
/// refused tap sets the permission back to `NotAsked`, so the next switch-on
/// tries again.
pub struct GainTap<O: TapOpener> {
    opener: O,
    permission: CapturePermission,
    tap: Option<(O::Tap, Arc<Gain>)>,
}

impl<O: TapOpener> GainTap<O> {
    pub fn new(opener: O) -> Self {
        Self {
            opener,
            permission: CapturePermission::NotAsked,
            tap: None,
        }
    }
}

impl<O: TapOpener> ProcessTap for GainTap<O> {
    fn permission(&self) -> CapturePermission {
        self.permission
    }
    fn request_permission(&mut self) {
        self.permission = CapturePermission::Granted;
    }
    fn retarget(&mut self, pid: u32) -> Result<(), ControlError> {
        self.tap = None;
        let gain = Arc::new(Gain::new(1.0));
        match self.opener.open(pid, Arc::clone(&gain)) {
            Ok(tap) => {
                self.tap = Some((tap, gain));
                Ok(())
            }
            Err(e) => {
                if e.0 == TAP_PERMISSION_NEEDED {
                    self.permission = CapturePermission::NotAsked;
                }
                Err(e)
            }
        }
    }
    fn step(&mut self, delta: f32) -> Result<(), ControlError> {
        let Some((_, gain)) = &self.tap else {
            return Err(ControlError(NO_AUDIO_SESSION.into()));
        };
        gain.step(delta);
        Ok(())
    }
    fn release(&mut self) {
        self.tap = None;
    }
}

/// The error when macOS refuses the audio-capture permission for a tap.
pub const TAP_PERMISSION_NEEDED: &str = PERMISSION_NEEDED;
/// The error when this Mac cannot tap app audio (the ADR-010 fallback).
pub const TAP_NOT_AVAILABLE: &str = "not available on this Mac yet";

/// Maps the OSStatus of a refused tap to its status text: the permission
/// errors ('!hog', 'nope') to [`TAP_PERMISSION_NEEDED`], anything else to
/// [`TAP_NOT_AVAILABLE`]. The caller logs the OS error.
pub fn tap_refused(status: i32) -> ControlError {
    const PERMISSIONS: i32 = i32::from_be_bytes(*b"!hog");
    const ILLEGAL_OPERATION: i32 = i32::from_be_bytes(*b"nope");
    let text = match status {
        PERMISSIONS | ILLEGAL_OPERATION => TAP_PERMISSION_NEEDED,
        _ => TAP_NOT_AVAILABLE,
    };
    ControlError(text.into())
}

/// The error while the audio-capture permission is not granted.
pub const PERMISSION_NEEDED: &str = "permission needed";

/// The macOS audio-capture permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturePermission {
    NotAsked,
    Denied,
    Granted,
}

/// The frontmost app (macOS: `NSWorkspace.frontmostApplication`).
pub trait FocusSource: Send {
    fn frontmost_pid(&mut self) -> Result<u32, ControlError>;
}

/// A process tap with gain on one process (macOS: a Core Audio process tap).
pub trait ProcessTap: Send {
    fn permission(&self) -> CapturePermission;
    /// Shows the audio-capture prompt.
    fn request_permission(&mut self);
    /// Moves the tap to `pid`.
    fn retarget(&mut self, pid: u32) -> Result<(), ControlError>;
    /// Steps the tap's gain by `delta` (negative is down).
    fn step(&mut self, delta: f32) -> Result<(), ControlError>;
    /// Tears the tap down, so the app plays at its own volume again.
    fn release(&mut self);
}

/// The macOS [`AppVolumeControl`]: keeps a [`ProcessTap`] on the frontmost
/// app and asks for the audio-capture permission the first time it is
/// switched on. Until it is granted, every call fails with
/// [`PERMISSION_NEEDED`], which reaches the status event as an override error.
///
/// [manual] macOS 15: switching the app-volume override on for the first time
/// shows the audio-capture prompt, and the status shows 'permission needed'
/// until it is allowed; then, with audio playing in two apps, turning the jog
/// wheel changes only the frontmost one, and after switching focus to the
/// other app it changes only that one.
pub struct FollowFocus<F, T> {
    focus: F,
    tap: T,
    pid: Option<u32>,
}

impl<F: FocusSource, T: ProcessTap> FollowFocus<F, T> {
    pub fn new(focus: F, tap: T) -> Self {
        Self {
            focus,
            tap,
            pid: None,
        }
    }

    /// Moves the tap to the frontmost app if focus has changed; call it on
    /// every focus change.
    pub fn follow(&mut self) -> Result<(), ControlError> {
        if self.tap.permission() != CapturePermission::Granted {
            return Err(ControlError(PERMISSION_NEEDED.into()));
        }
        let pid = self.focus.frontmost_pid()?;
        if self.pid != Some(pid) {
            self.tap.retarget(pid)?;
            self.pid = Some(pid);
        }
        Ok(())
    }

    fn step(&mut self, delta: f32) -> Result<(), ControlError> {
        self.follow()?;
        self.tap.step(delta)
    }
}

impl<F: FocusSource, T: ProcessTap> AppVolumeControl for FollowFocus<F, T> {
    fn step_up(&mut self, step: f32) -> Result<(), ControlError> {
        self.step(step)
    }
    fn step_down(&mut self, step: f32) -> Result<(), ControlError> {
        self.step(-step)
    }
    fn activate(&mut self) -> Result<(), ControlError> {
        if self.tap.permission() == CapturePermission::NotAsked {
            self.tap.request_permission();
        }
        self.follow()
    }
    /// Tears the tap down on mode exit and release_all; the next step taps
    /// the frontmost app again.
    fn release(&mut self) {
        self.tap.release();
        self.pid = None;
    }
}

/// One OS audio session (Windows: an `IAudioSessionControl2` with its
/// `ISimpleAudioVolume`), so [`step_app_sessions`] is tested without OS calls.
pub trait AudioSession {
    fn process_id(&self) -> Result<u32, ControlError>;
    /// The session volume, 0..1.
    fn volume(&self) -> Result<f32, ControlError>;
    fn set_volume(&mut self, volume: f32) -> Result<(), ControlError>;
}

/// The error when the foreground app has no audio session.
pub const NO_AUDIO_SESSION: &str = "no audio session";

/// Steps every session owned by `pid` by `delta` (negative is down), clamped
/// to 0..1. Errors with [`NO_AUDIO_SESSION`] when no session matches.
pub fn step_app_sessions<S: AudioSession>(
    pid: u32,
    sessions: impl IntoIterator<Item = S>,
    delta: f32,
) -> Result<(), ControlError> {
    let mut found = false;
    for mut s in sessions {
        if s.process_id()? != pid {
            continue;
        }
        found = true;
        let v = (s.volume()? + delta).clamp(0.0, 1.0);
        s.set_volume(v)?;
    }
    if found {
        Ok(())
    } else {
        Err(ControlError(NO_AUDIO_SESSION.into()))
    }
}

/// Display brightness; `step` is in percent.
pub trait BrightnessControl: Send {
    fn step_up(&mut self, step: u8) -> Result<(), ControlError>;
    fn step_down(&mut self, step: u8) -> Result<(), ControlError>;
}

/// A display's brightness in its own units, from `min` to `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    pub min: u32,
    pub current: u32,
    pub max: u32,
}

/// One display a [`BrightnessControl`] backend can step.
pub trait Screen {
    fn name(&self) -> String;
    fn level(&mut self) -> Result<Level, ControlError>;
    fn set(&mut self, value: u32) -> Result<(), ControlError>;
}

/// Steps every display by `percent` of its range (negative is down),
/// clamped to it. A display that fails, or a source that could not list its
/// displays, gives its own error; the others are still stepped (A16).
pub fn step_displays(
    screens: Vec<Result<Box<dyn Screen>, ControlError>>,
    percent: i32,
) -> Result<(), ControlError> {
    let mut errors = Vec::new();
    for screen in screens {
        let mut s = match screen {
            Ok(s) => s,
            Err(e) => {
                errors.push(e.0);
                continue;
            }
        };
        let stepped = s.level().and_then(|l| {
            let range = i64::from(l.max.saturating_sub(l.min));
            let to = i64::from(l.current) + i64::from(percent) * range / 100;
            s.set(to.clamp(l.min.into(), l.max.into()) as u32)
        });
        if let Err(e) = stepped {
            errors.push(format!("{}: {}", s.name(), e.0));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ControlError(errors.join("; ")))
    }
}

/// The override-errors part of the status event: a control call that failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideError {
    pub mode: OverrideMode,
    pub error: ControlError,
}

/// One control per override mode.
pub struct Controls {
    pub volume: Box<dyn VolumeControl>,
    pub app_volume: Box<dyn AppVolumeControl>,
    pub brightness: Box<dyn BrightnessControl>,
}

/// Adds `ticks` to the queued ticks of `mode`.
fn coalesce(queue: &mut Vec<(OverrideMode, i32)>, mode: OverrideMode, ticks: i32) {
    match queue.iter_mut().find(|(m, _)| *m == mode) {
        Some((_, t)) => *t += ticks,
        None => queue.push((mode, ticks)),
    }
}

/// Turns queued dial ticks into control calls, one call per mode per run,
/// holding brightness back until [`BRIGHTNESS_INTERVAL`] has passed.
pub struct OverrideRunner<C> {
    controls: Controls,
    steps: OverrideSettings,
    clock: C,
    pending: Vec<(OverrideMode, i32)>,
    activated: Vec<OverrideMode>,
    last_brightness: Option<Instant>,
}

impl<C: Clock> OverrideRunner<C> {
    pub fn new(controls: Controls, steps: OverrideSettings, clock: C) -> Self {
        Self {
            controls,
            steps,
            clock,
            pending: Vec::new(),
            activated: Vec::new(),
            last_brightness: None,
        }
    }

    /// Queues `ticks` dial ticks (negative is down) for `mode`.
    pub fn push(&mut self, mode: OverrideMode, ticks: i32) {
        coalesce(&mut self.pending, mode, ticks);
    }

    /// Queues the switching on of `mode`.
    pub fn activate(&mut self, mode: OverrideMode) {
        self.activated.push(mode);
    }

    /// Calls the controls for the queued activations and ticks. Returns the
    /// failed calls and, when brightness is held back, how long until it is
    /// due.
    pub fn run(&mut self) -> (Vec<OverrideError>, Option<Duration>) {
        let now = self.clock.now();
        let mut errors = Vec::new();
        for mode in std::mem::take(&mut self.activated) {
            if mode == OverrideMode::AppVolume
                && let Err(error) = self.controls.app_volume.activate()
            {
                warn!(?mode, error = %error, "dial override failed");
                errors.push(OverrideError { mode, error });
            }
        }
        let mut wait = None;
        for (mode, ticks) in std::mem::take(&mut self.pending) {
            if ticks == 0 {
                continue;
            }
            if mode == OverrideMode::Brightness {
                if let Some(due) = self.last_brightness.map(|t| t + BRIGHTNESS_INTERVAL)
                    && now < due
                {
                    wait = Some(due - now);
                    self.pending.push((mode, ticks));
                    continue;
                }
                self.last_brightness = Some(now);
            }
            if let Err(error) = self.step(mode, ticks) {
                warn!(?mode, error = %error, "dial override failed");
                errors.push(OverrideError { mode, error });
            }
        }
        (errors, wait)
    }

    /// Releases what the controls hold (the app-volume tap).
    pub fn release(&mut self) {
        self.controls.app_volume.release();
    }

    /// Uses `steps` for the ticks run from now on (a saved config).
    pub fn set_steps(&mut self, steps: OverrideSettings) {
        self.steps = steps;
    }

    fn step(&mut self, mode: OverrideMode, ticks: i32) -> Result<(), ControlError> {
        let n = ticks.unsigned_abs();
        let up = ticks > 0;
        let c = &mut self.controls;
        match mode {
            OverrideMode::SystemVolume => {
                let s = self.steps.system_volume_step * n as f32;
                if up {
                    c.volume.step_up(s)
                } else {
                    c.volume.step_down(s)
                }
            }
            OverrideMode::AppVolume => {
                let s = self.steps.app_volume_step * n as f32;
                if up {
                    c.app_volume.step_up(s)
                } else {
                    c.app_volume.step_down(s)
                }
            }
            OverrideMode::Brightness => {
                let s = u8::try_from(n.saturating_mul(self.steps.brightness_step.into()))
                    .unwrap_or(u8::MAX);
                if up {
                    c.brightness.step_up(s)
                } else {
                    c.brightness.step_down(s)
                }
            }
        }
    }
}

#[derive(Default)]
struct Queue {
    steps: Vec<(OverrideMode, i32)>,
    release: bool,
    activated: Vec<OverrideMode>,
    settings: Option<OverrideSettings>,
    stop: bool,
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
}

/// Runs an [`OverrideRunner`] on its own thread. [`push`](Self::push) only
/// queues, so a slow control never delays the caller.
pub struct OverrideWorker {
    shared: Arc<Shared>,
    errors: Receiver<OverrideError>,
    thread: Option<JoinHandle<()>>,
}

impl OverrideWorker {
    pub fn spawn<C: Clock + Send + 'static>(mut runner: OverrideRunner<C>) -> Self {
        let shared = Arc::new(Shared::default());
        let (tx, errors) = mpsc::channel();
        let s = Arc::clone(&shared);
        let thread = std::thread::spawn(move || {
            let mut wait = None;
            loop {
                let (settings, activated, steps, release) = {
                    let mut q = s.queue.lock().unwrap();
                    while q.steps.is_empty()
                        && q.activated.is_empty()
                        && q.settings.is_none()
                        && !q.release
                        && !q.stop
                    {
                        let Some(d) = wait else {
                            q = s.wake.wait(q).unwrap();
                            continue;
                        };
                        let (g, timeout) = s.wake.wait_timeout(q, d).unwrap();
                        q = g;
                        if timeout.timed_out() {
                            break;
                        }
                    }
                    if q.stop {
                        return;
                    }
                    (
                        q.settings.take(),
                        std::mem::take(&mut q.activated),
                        std::mem::take(&mut q.steps),
                        std::mem::take(&mut q.release),
                    )
                };
                if let Some(settings) = settings {
                    runner.set_steps(settings);
                }
                for mode in activated {
                    runner.activate(mode);
                }
                for (mode, ticks) in steps {
                    runner.push(mode, ticks);
                }
                let (errs, w) = runner.run();
                wait = w;
                if release {
                    runner.release();
                }
                for e in errs {
                    let _ = tx.send(e);
                }
            }
        });
        Self {
            shared,
            errors,
            thread: Some(thread),
        }
    }

    /// Queues `ticks` dial ticks for `mode` without calling any control.
    pub fn push(&self, mode: OverrideMode, ticks: i32) {
        coalesce(&mut self.shared.queue.lock().unwrap().steps, mode, ticks);
        self.shared.wake.notify_one();
    }

    /// Queues the switching on of `mode`, e.g. to prompt for a permission.
    pub fn activate(&self, mode: OverrideMode) {
        self.shared.queue.lock().unwrap().activated.push(mode);
        self.shared.wake.notify_one();
    }

    /// Drains the failed control calls, as override-error status events.
    pub fn take_errors(&self) -> Vec<OverrideError> {
        self.errors.try_iter().collect()
    }

    /// Releases what the controls hold, after the ticks already queued.
    pub fn release(&self) {
        self.shared.queue.lock().unwrap().release = true;
        self.shared.wake.notify_one();
    }

    /// Uses `steps` for the ticks queued from now on (a saved config).
    pub fn set_steps(&self, steps: OverrideSettings) {
        self.shared.queue.lock().unwrap().settings = Some(steps);
        self.shared.wake.notify_one();
    }
}

impl Drop for OverrideWorker {
    fn drop(&mut self) {
        self.shared.queue.lock().unwrap().stop = true;
        self.shared.wake.notify_one();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A call made on a [`FakeControl`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlCall {
    Up(f32),
    Down(f32),
}

/// A recording fake for all three control traits: it changes nothing. Clones
/// share their record, so a test keeps one after handing one to a worker.
#[derive(Debug, Clone, Default)]
pub struct FakeControl {
    calls: Arc<Mutex<Vec<ControlCall>>>,
    gate: Arc<Mutex<()>>,
    fail: bool,
}

impl FakeControl {
    pub fn new() -> Self {
        Self::default()
    }

    /// A control whose every call fails (after recording it).
    pub fn failing() -> Self {
        Self {
            fail: true,
            ..Self::default()
        }
    }

    /// Every call so far, in order.
    pub fn calls(&self) -> Vec<ControlCall> {
        self.calls.lock().unwrap().clone()
    }

    /// Makes every call block until the guard is dropped: a slow control.
    pub fn hold(&self) -> MutexGuard<'_, ()> {
        self.gate.lock().unwrap()
    }

    fn record(&self, call: ControlCall) -> Result<(), ControlError> {
        drop(self.gate.lock().unwrap());
        self.calls.lock().unwrap().push(call);
        if self.fail {
            return Err(ControlError(format!("cannot apply {call:?}")));
        }
        Ok(())
    }
}

impl VolumeControl for FakeControl {
    fn step_up(&mut self, step: f32) -> Result<(), ControlError> {
        self.record(ControlCall::Up(step))
    }
    fn step_down(&mut self, step: f32) -> Result<(), ControlError> {
        self.record(ControlCall::Down(step))
    }
}

impl AppVolumeControl for FakeControl {
    fn step_up(&mut self, step: f32) -> Result<(), ControlError> {
        self.record(ControlCall::Up(step))
    }
    fn step_down(&mut self, step: f32) -> Result<(), ControlError> {
        self.record(ControlCall::Down(step))
    }
}

impl BrightnessControl for FakeControl {
    fn step_up(&mut self, step: u8) -> Result<(), ControlError> {
        self.record(ControlCall::Up(step.into()))
    }
    fn step_down(&mut self, step: u8) -> Result<(), ControlError> {
        self.record(ControlCall::Down(step.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::ControlCall::{Down, Up};
    use super::*;
    use crate::clock::SystemClock;

    #[derive(Clone)]
    struct FakeClock(Arc<Mutex<Instant>>);

    impl FakeClock {
        fn advance(&self, ms: u64) {
            *self.0.lock().unwrap() += Duration::from_millis(ms);
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Instant {
            *self.0.lock().unwrap()
        }
    }

    /// Fakes for (volume, app volume, brightness) and controls sharing them.
    fn fakes(brightness: FakeControl) -> (FakeControl, FakeControl, FakeControl, Controls) {
        let (v, a) = (FakeControl::new(), FakeControl::new());
        let controls = Controls {
            volume: Box::new(v.clone()),
            app_volume: Box::new(a.clone()),
            brightness: Box::new(brightness.clone()),
        };
        (v, a, brightness, controls)
    }

    fn steps() -> OverrideSettings {
        OverrideSettings {
            system_volume_step: 0.5,
            app_volume_step: 0.25,
            brightness_step: 5,
        }
    }

    #[test]
    fn ticks_step_the_control_of_their_mode_up_or_down() {
        let (v, a, b, controls) = fakes(FakeControl::new());
        let clock = FakeClock(Arc::new(Mutex::new(Instant::now())));
        let mut runner = OverrideRunner::new(controls, steps(), clock);
        runner.push(OverrideMode::SystemVolume, 1);
        runner.push(OverrideMode::AppVolume, -2);
        runner.push(OverrideMode::Brightness, 3);
        assert_eq!(runner.run(), (vec![], None));
        assert_eq!(v.calls(), [Up(0.5)]);
        assert_eq!(a.calls(), [Down(0.5)]);
        assert_eq!(b.calls(), [Up(15.0)]);
    }

    #[test]
    fn brightness_runs_at_most_once_per_100ms_and_coalesces() {
        let (v, _, b, controls) = fakes(FakeControl::new());
        let clock = FakeClock(Arc::new(Mutex::new(Instant::now())));
        let mut runner = OverrideRunner::new(controls, steps(), clock.clone());
        runner.push(OverrideMode::Brightness, 1);
        assert_eq!(runner.run(), (vec![], None));
        clock.advance(40);
        runner.push(OverrideMode::Brightness, 1);
        runner.push(OverrideMode::SystemVolume, 1);
        assert_eq!(runner.run(), (vec![], Some(Duration::from_millis(60))));
        // Volume is not throttled; brightness is held back.
        assert_eq!(v.calls(), [Up(0.5)]);
        assert_eq!(b.calls(), [Up(5.0)]);
        clock.advance(59);
        runner.push(OverrideMode::Brightness, 2);
        assert_eq!(runner.run(), (vec![], Some(Duration::from_millis(1))));
        assert_eq!(b.calls().len(), 1);
        clock.advance(1);
        assert_eq!(runner.run(), (vec![], None));
        // The three held-back ticks become one call.
        assert_eq!(b.calls(), [Up(5.0), Up(15.0)]);
    }

    #[test]
    fn control_errors_become_override_error_events() {
        let (_, _, _, controls) = fakes(FakeControl::failing());
        let mut runner = OverrideRunner::new(controls, steps(), SystemClock);
        runner.push(OverrideMode::Brightness, -1);
        let (errors, _) = runner.run();
        assert_eq!(
            errors,
            [OverrideError {
                mode: OverrideMode::Brightness,
                error: ControlError("cannot apply Down(5.0)".into()),
            }]
        );
    }

    /// A device volume scalar; `None` is a device without a settable volume.
    struct FakeScalar(Option<f32>);

    impl VolumeScalar for FakeScalar {
        fn get(&mut self) -> Result<Option<f32>, ControlError> {
            Ok(self.0)
        }
        fn set(&mut self, value: f32) -> Result<(), ControlError> {
            self.0 = Some(value);
            Ok(())
        }
    }

    /// A display with a level that records what it is set to.
    struct FakeScreen {
        name: &'static str,
        level: Result<Level, ControlError>,
        set_to: Arc<Mutex<Vec<u32>>>,
    }

    impl Screen for FakeScreen {
        fn name(&self) -> String {
            self.name.into()
        }
        fn level(&mut self) -> Result<Level, ControlError> {
            self.level.clone()
        }
        fn set(&mut self, value: u32) -> Result<(), ControlError> {
            self.set_to.lock().unwrap().push(value);
            Ok(())
        }
    }

    /// A fake session: (owning pid, volume), shared so the test reads it back.
    struct FakeSession(u32, Arc<Mutex<f32>>);

    impl AudioSession for FakeSession {
        fn process_id(&self) -> Result<u32, ControlError> {
            Ok(self.0)
        }
        fn volume(&self) -> Result<f32, ControlError> {
            Ok(*self.1.lock().unwrap())
        }
        fn set_volume(&mut self, volume: f32) -> Result<(), ControlError> {
            *self.1.lock().unwrap() = volume;
            Ok(())
        }
    }

    #[test]
    fn scalar_volume_steps_the_scalar_clamped_to_0_1() {
        let mut v = ScalarVolume(FakeScalar(Some(0.5)));
        v.step_up(0.25).unwrap();
        assert_eq!(v.0.0, Some(0.75));
        v.step_up(0.5).unwrap();
        assert_eq!(v.0.0, Some(1.0));
        v.step_down(0.25).unwrap();
        assert_eq!(v.0.0, Some(0.75));
        v.step_down(2.0).unwrap();
        assert_eq!(v.0.0, Some(0.0));
    }

    #[test]
    fn scalar_volume_without_a_settable_volume_is_an_error() {
        let mut v = ScalarVolume(FakeScalar(None));
        assert_eq!(
            v.step_up(0.1),
            Err(ControlError(
                "the default output device has no settable volume".into()
            ))
        );
        assert_eq!(v.0.0, None);
    }

    type Opened = Result<Box<dyn Screen>, ControlError>;

    fn screen(
        name: &'static str,
        level: Result<Level, ControlError>,
    ) -> (Arc<Mutex<Vec<u32>>>, Opened) {
        let set_to = Arc::new(Mutex::new(Vec::new()));
        let s = FakeScreen {
            name,
            level,
            set_to: Arc::clone(&set_to),
        };
        (set_to, Ok(Box::new(s)))
    }

    #[test]
    fn displays_step_by_percent_of_their_own_range_and_clamp() {
        let lvl = |min, current, max| Ok(Level { min, current, max });
        let (ddc, a) = screen("DDC", lvl(0, 50, 200));
        let (panel, b) = screen("panel", lvl(0, 98, 100));
        assert_eq!(step_displays(vec![a, b], 5), Ok(()));
        assert_eq!(*ddc.lock().unwrap(), [60]);
        assert_eq!(*panel.lock().unwrap(), [100]);
        let (low, c) = screen("low", lvl(10, 12, 110));
        assert_eq!(step_displays(vec![c], -5), Ok(()));
        assert_eq!(*low.lock().unwrap(), [10]);
    }

    #[test]
    fn each_failing_display_gives_its_own_error_and_the_others_still_step() {
        let lvl = Ok(Level {
            min: 0,
            current: 40,
            max: 100,
        });
        let (first, a) = screen("Dell", lvl.clone());
        let (_, b) = screen("LG", Err(ControlError("DDC/CI off".into())));
        let no_wmi = Err(ControlError("built-in panel: class not found".into()));
        let (last, d) = screen("HP", lvl);
        let err = step_displays(vec![a, b, no_wmi, d], -10).unwrap_err();
        assert_eq!(err.0, "LG: DDC/CI off; built-in panel: class not found");
        assert_eq!(*first.lock().unwrap(), [30]);
        assert_eq!(*last.lock().unwrap(), [30]);
    }

    fn sessions(list: &[(u32, f32)]) -> Vec<(u32, Arc<Mutex<f32>>)> {
        list.iter()
            .map(|&(pid, v)| (pid, Arc::new(Mutex::new(v))))
            .collect()
    }

    fn fake(list: &[(u32, Arc<Mutex<f32>>)]) -> Vec<FakeSession> {
        list.iter()
            .map(|(pid, v)| FakeSession(*pid, Arc::clone(v)))
            .collect()
    }

    fn volumes(list: &[(u32, Arc<Mutex<f32>>)]) -> Vec<f32> {
        list.iter().map(|(_, v)| *v.lock().unwrap()).collect()
    }

    #[test]
    fn app_volume_steps_only_the_sessions_of_the_pid() {
        let list = sessions(&[(7, 0.5), (8, 0.5), (7, 0.25)]);
        assert_eq!(step_app_sessions(7, fake(&list), 0.25), Ok(()));
        assert_eq!(volumes(&list), [0.75, 0.5, 0.5]);
        assert_eq!(step_app_sessions(8, fake(&list), -0.25), Ok(()));
        assert_eq!(volumes(&list), [0.75, 0.25, 0.5]);
    }

    #[test]
    fn app_volume_is_clamped_to_0_1() {
        let list = sessions(&[(7, 0.75), (7, 0.25)]);
        step_app_sessions(7, fake(&list), 0.5).unwrap();
        assert_eq!(volumes(&list), [1.0, 0.75]);
        step_app_sessions(7, fake(&list), -2.0).unwrap();
        assert_eq!(volumes(&list), [0.0, 0.0]);
    }

    #[test]
    fn app_volume_without_a_matching_session_is_a_no_audio_session_error() {
        let list = sessions(&[(8, 0.5)]);
        let err = step_app_sessions(7, fake(&list), 0.25).unwrap_err();
        assert_eq!(err, ControlError("no audio session".into()));
        assert_eq!(step_app_sessions(7, fake(&[]), 0.25), Err(err));
        assert_eq!(volumes(&list), [0.5]);
    }

    /// A fake tap opener: records the opened pids and their gains; a tap is
    /// live until dropped.
    type OpenedTap = (u32, Arc<Gain>);

    #[derive(Clone, Default)]
    struct FakeTaps {
        opened: Arc<Mutex<Vec<OpenedTap>>>,
        live: Arc<Mutex<u32>>,
        refuse: Option<i32>,
    }

    struct LiveTap(Arc<Mutex<u32>>);

    impl Drop for LiveTap {
        fn drop(&mut self) {
            *self.0.lock().unwrap() -= 1;
        }
    }

    impl TapOpener for FakeTaps {
        type Tap = LiveTap;
        fn open(&mut self, pid: u32, gain: Arc<Gain>) -> Result<LiveTap, ControlError> {
            if let Some(status) = self.refuse {
                return Err(tap_refused(status));
            }
            self.opened.lock().unwrap().push((pid, gain));
            *self.live.lock().unwrap() += 1;
            Ok(LiveTap(Arc::clone(&self.live)))
        }
    }

    impl FakeTaps {
        fn gain(&self) -> f32 {
            self.opened.lock().unwrap().last().unwrap().1.get()
        }
        fn live(&self) -> u32 {
            *self.live.lock().unwrap()
        }
    }

    /// A fake focus source: the frontmost pid, shared so a test can change it.
    #[derive(Clone)]
    struct FakeFocus(Arc<Mutex<u32>>);

    impl FocusSource for FakeFocus {
        fn frontmost_pid(&mut self) -> Result<u32, ControlError> {
            Ok(*self.0.lock().unwrap())
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    enum TapCall {
        Prompt,
        Retarget(u32),
        Step(f32),
        Release,
    }

    /// A fake tap recording its calls; the prompt sets the permission to
    /// `answer`.
    #[derive(Clone)]
    struct FakeTap {
        permission: Arc<Mutex<CapturePermission>>,
        answer: CapturePermission,
        calls: Arc<Mutex<Vec<TapCall>>>,
    }

    impl FakeTap {
        fn new(permission: CapturePermission, answer: CapturePermission) -> Self {
            Self {
                permission: Arc::new(Mutex::new(permission)),
                answer,
                calls: Arc::default(),
            }
        }
        fn calls(&self) -> Vec<TapCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl ProcessTap for FakeTap {
        fn permission(&self) -> CapturePermission {
            *self.permission.lock().unwrap()
        }
        fn request_permission(&mut self) {
            self.calls.lock().unwrap().push(TapCall::Prompt);
            *self.permission.lock().unwrap() = self.answer;
        }
        fn retarget(&mut self, pid: u32) -> Result<(), ControlError> {
            self.calls.lock().unwrap().push(TapCall::Retarget(pid));
            Ok(())
        }
        fn step(&mut self, delta: f32) -> Result<(), ControlError> {
            self.calls.lock().unwrap().push(TapCall::Step(delta));
            Ok(())
        }
        fn release(&mut self) {
            self.calls.lock().unwrap().push(TapCall::Release);
        }
    }

    /// The shipped macOS path: FollowFocus over a GainTap, focused on `pid`.
    fn gain_tap_app(taps: &FakeTaps, pid: u32) -> FollowFocus<FakeFocus, GainTap<FakeTaps>> {
        FollowFocus::new(
            FakeFocus(Arc::new(Mutex::new(pid))),
            GainTap::new(taps.clone()),
        )
    }

    #[test]
    fn gain_tap_taps_the_frontmost_pid_once_and_steps_its_gain_clamped_to_0_1() {
        let taps = FakeTaps::default();
        let mut app = gain_tap_app(&taps, 42);
        app.activate().unwrap();
        assert_eq!(taps.live(), 1);
        app.step_down(0.25).unwrap();
        assert_eq!(taps.gain(), 0.75);
        app.step_down(0.5).unwrap();
        assert_eq!(taps.gain(), 0.25);
        app.step_down(1.0).unwrap();
        assert_eq!(taps.gain(), 0.0);
        app.step_up(0.5).unwrap();
        assert_eq!(taps.gain(), 0.5);
        app.step_up(2.0).unwrap();
        assert_eq!(taps.gain(), 1.0);
        let opened = taps.opened.lock().unwrap();
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].0, 42);
    }

    #[test]
    fn gain_tap_retarget_drops_the_old_tap_and_opens_the_new_pid_at_full_gain() {
        let taps = FakeTaps::default();
        let focus = FakeFocus(Arc::new(Mutex::new(7)));
        let mut app = FollowFocus::new(focus.clone(), GainTap::new(taps.clone()));
        app.activate().unwrap();
        app.step_down(0.5).unwrap();
        *focus.0.lock().unwrap() = 8;
        app.step_down(0.25).unwrap();
        assert_eq!((taps.live(), taps.gain()), (1, 0.75));
        let pids: Vec<u32> = taps.opened.lock().unwrap().iter().map(|o| o.0).collect();
        assert_eq!(pids, [7, 8]);
    }

    #[test]
    fn follow_focus_release_tears_the_tap_down_and_the_next_step_retaps() {
        let focus = FakeFocus(Arc::new(Mutex::new(7)));
        let tap = FakeTap::new(CapturePermission::Granted, CapturePermission::Granted);
        let mut control = FollowFocus::new(focus, tap.clone());
        control.step_up(0.25).unwrap();
        control.release();
        control.step_up(0.25).unwrap();
        assert_eq!(
            tap.calls(),
            [
                TapCall::Retarget(7),
                TapCall::Step(0.25),
                TapCall::Release,
                TapCall::Retarget(7),
                TapCall::Step(0.25),
            ]
        );
    }

    #[test]
    fn gain_tap_release_through_follow_focus_drops_the_tap_and_reopens_at_full_gain() {
        let taps = FakeTaps::default();
        let mut app = gain_tap_app(&taps, 42);
        app.release();
        app.activate().unwrap();
        app.step_down(0.5).unwrap();
        app.release();
        assert_eq!(taps.live(), 0);
        app.release();
        assert_eq!(taps.live(), 0);
        app.step_down(0.25).unwrap();
        assert_eq!((taps.live(), taps.gain()), (1, 0.75));
        assert_eq!(taps.opened.lock().unwrap().len(), 2);
    }

    #[test]
    fn refused_tap_reports_permission_needed_and_asks_again_next_time() {
        let mut taps = FakeTaps {
            refuse: Some(i32::from_be_bytes(*b"!hog")),
            ..FakeTaps::default()
        };
        let mut tap = GainTap::new(taps.clone());
        tap.request_permission();
        assert_eq!(
            tap.retarget(42),
            Err(ControlError("permission needed".into()))
        );
        assert_eq!(tap.permission(), CapturePermission::NotAsked);
        assert_eq!(taps.live(), 0);
        taps.refuse = Some(-50);
        let mut tap = GainTap::new(taps);
        tap.request_permission();
        assert_eq!(tap.retarget(42).unwrap_err().0, TAP_NOT_AVAILABLE);
        assert_eq!(tap.permission(), CapturePermission::Granted);
        assert_eq!(tap.step(0.1), Err(ControlError(NO_AUDIO_SESSION.into())));
        assert_eq!(
            tap_refused(i32::from_be_bytes(*b"nope")).0,
            TAP_PERMISSION_NEEDED
        );
        assert_eq!(tap_refused(-50).0, "not available on this Mac yet");
    }

    #[test]
    fn play_at_gain_scales_samples_and_silences_the_rest() {
        let mut out = [9.0; 4];
        play_at_gain(&[1.0, -0.5], &mut out, 0.5);
        assert_eq!(out, [0.5, -0.25, 0.0, 0.0]);
        let mut out = [9.0; 1];
        play_at_gain(&[1.0, 1.0], &mut out, 0.0);
        assert_eq!(out, [0.0]);
    }

    #[test]
    fn gain_is_clamped_to_0_1() {
        assert_eq!(Gain::new(2.0).get(), 1.0);
        let g = Gain::new(0.5);
        g.step(-0.75);
        assert_eq!(g.get(), 0.0);
        g.step(0.25);
        assert_eq!(g.get(), 0.25);
    }

    #[test]
    fn worker_release_runs_queued_ticks_then_drops_the_tap() {
        let taps = FakeTaps::default();
        let controls = Controls {
            volume: Box::new(FakeControl::new()),
            app_volume: Box::new(gain_tap_app(&taps, 7)),
            brightness: Box::new(FakeControl::new()),
        };
        let worker = OverrideWorker::spawn(OverrideRunner::new(controls, steps(), SystemClock));
        worker.activate(OverrideMode::AppVolume);
        worker.push(OverrideMode::AppVolume, -1);
        wait_for(|| taps.live() == 1);
        assert_eq!(taps.gain(), 0.75);
        worker.push(OverrideMode::AppVolume, -1);
        worker.release();
        wait_for(|| taps.live() == 0);
        assert_eq!(taps.gain(), 0.5);
    }

    #[test]
    fn app_volume_tap_moves_to_the_frontmost_app_when_focus_changes() {
        let focus = FakeFocus(Arc::new(Mutex::new(7)));
        let tap = FakeTap::new(CapturePermission::Granted, CapturePermission::Granted);
        let mut control = FollowFocus::new(focus.clone(), tap.clone());
        control.activate().unwrap();
        control.step_up(0.25).unwrap();
        *focus.0.lock().unwrap() = 8;
        control.follow().unwrap();
        control.step_down(0.5).unwrap();
        assert_eq!(
            tap.calls(),
            [
                TapCall::Retarget(7),
                TapCall::Step(0.25),
                TapCall::Retarget(8),
                TapCall::Step(-0.5),
            ]
        );
    }

    #[test]
    fn app_volume_prompts_for_audio_capture_only_the_first_time_it_is_switched_on() {
        let focus = FakeFocus(Arc::new(Mutex::new(7)));
        let tap = FakeTap::new(CapturePermission::NotAsked, CapturePermission::Granted);
        let mut control = FollowFocus::new(focus, tap.clone());
        control.activate().unwrap();
        control.activate().unwrap();
        assert_eq!(tap.calls(), [TapCall::Prompt, TapCall::Retarget(7)]);
    }

    #[test]
    fn app_volume_reports_permission_needed_until_it_is_granted() {
        let focus = FakeFocus(Arc::new(Mutex::new(7)));
        let tap = FakeTap::new(CapturePermission::NotAsked, CapturePermission::Denied);
        let (v, b) = (FakeControl::new(), FakeControl::new());
        let controls = Controls {
            volume: Box::new(v),
            app_volume: Box::new(FollowFocus::new(focus, tap.clone())),
            brightness: Box::new(b),
        };
        let mut runner = OverrideRunner::new(controls, steps(), SystemClock);
        let needed = OverrideError {
            mode: OverrideMode::AppVolume,
            error: ControlError(PERMISSION_NEEDED.into()),
        };
        runner.activate(OverrideMode::AppVolume);
        assert_eq!(runner.run(), (vec![needed.clone()], None));
        runner.push(OverrideMode::AppVolume, 1);
        assert_eq!(runner.run(), (vec![needed], None));
        assert_eq!(tap.calls(), [TapCall::Prompt]);
        *tap.permission.lock().unwrap() = CapturePermission::Granted;
        runner.push(OverrideMode::AppVolume, 1);
        assert_eq!(runner.run(), (vec![], None));
        assert_eq!(
            tap.calls(),
            [TapCall::Prompt, TapCall::Retarget(7), TapCall::Step(0.25)]
        );
    }

    #[test]
    fn worker_activation_reaches_the_app_volume_control() {
        let focus = FakeFocus(Arc::new(Mutex::new(7)));
        let tap = FakeTap::new(CapturePermission::NotAsked, CapturePermission::NotAsked);
        let controls = Controls {
            volume: Box::new(FakeControl::new()),
            app_volume: Box::new(FollowFocus::new(focus, tap.clone())),
            brightness: Box::new(FakeControl::new()),
        };
        let worker = OverrideWorker::spawn(OverrideRunner::new(controls, steps(), SystemClock));
        worker.activate(OverrideMode::AppVolume);
        let mut errors = Vec::new();
        wait_for(|| {
            errors.extend(worker.take_errors());
            !errors.is_empty()
        });
        assert_eq!(errors[0].error, ControlError(PERMISSION_NEEDED.into()));
        assert_eq!(tap.calls(), [TapCall::Prompt]);
    }

    fn wait_for(mut f: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(5);
        while !f() {
            assert!(Instant::now() < end, "timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn worker_runs_controls_and_reports_their_errors() {
        let (v, _, b, controls) = fakes(FakeControl::failing());
        let worker = OverrideWorker::spawn(OverrideRunner::new(controls, steps(), SystemClock));
        worker.push(OverrideMode::Brightness, 1);
        worker.push(OverrideMode::SystemVolume, 2);
        wait_for(|| v.calls() == [Up(1.0)] && b.calls() == [Up(5.0)]);
        let mut errors = Vec::new();
        wait_for(|| {
            errors.extend(worker.take_errors());
            !errors.is_empty()
        });
        assert_eq!(errors[0].mode, OverrideMode::Brightness);
    }

    #[test]
    fn worker_steps_by_a_saved_config_from_then_on() {
        let (v, _, _, controls) = fakes(FakeControl::new());
        let worker = OverrideWorker::spawn(OverrideRunner::new(controls, steps(), SystemClock));
        worker.push(OverrideMode::SystemVolume, 1);
        wait_for(|| v.calls() == [Up(0.5)]);
        worker.set_steps(OverrideSettings {
            system_volume_step: 0.25,
            ..steps()
        });
        worker.push(OverrideMode::SystemVolume, -1);
        wait_for(|| v.calls() == [Up(0.5), Down(0.25)]);
    }
}
