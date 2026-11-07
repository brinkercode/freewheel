//! Linux volume through the PulseAudio protocol (PipeWire's pipewire-pulse
//! or PulseAudio itself) with `pactl`, or WirePlumber's `wpctl` when
//! `pactl` is missing. System volume steps the default sink, capped at 100 %;
//! app volume steps the sink inputs (streams) of the focused window's
//! process and its children (browsers play from a child process). The
//! focused window comes from X11 (`_NET_ACTIVE_WINDOW`, `_NET_WM_PID`);
//! Wayland does not tell other apps which window has the focus, so there
//! app volume says it is not available.
//!
//! [manual] Ubuntu 24.04 (PipeWire): the dial on Change the volume moves
//! the GNOME volume slider; on App volume with a video playing in Firefox
//! (X11 session), only Firefox's stream in pavucontrol moves.

use std::process::Command;

use freewheel_core::overrides::{
    AppVolumeControl, AudioSession, ControlError, VolumeControl, step_app_sessions,
};

/// Runs a program and returns its stdout (the tests answer with fixtures).
pub type Run = Box<dyn FnMut(&str, &[String]) -> Result<String, ControlError> + Send>;

/// Runs `program args` for real.
pub fn run_command() -> Run {
    Box::new(|program, args| {
        let out = Command::new(program)
            .args(args)
            .output()
            .map_err(|e| ControlError(format!("{program}: {e}")))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(ControlError(format!(
                "{program} {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
    })
}

fn strings(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// The first percentage in `pactl get-sink-volume` output
/// (`Volume: front-left: 32768 /  50% / -18.06 dB, ...`).
pub fn parse_percent(text: &str) -> Option<u32> {
    text.split('/')
        .map(str::trim)
        .find_map(|part| part.strip_suffix('%')?.trim().parse().ok())
}

/// `current` moved by `step` (a fraction of full scale), as a whole
/// percentage from 0 to 100.
pub fn stepped(current: u32, step: f32, up: bool) -> u32 {
    let by = (step * 100.0).round().max(1.0) as i64;
    let to = i64::from(current) + if up { by } else { -by };
    to.clamp(0, 100) as u32
}

/// The default sink's volume.
pub struct PulseVolume {
    run: Run,
}

impl PulseVolume {
    pub fn new(run: Run) -> Self {
        Self { run }
    }

    fn step(&mut self, step: f32, up: bool) -> Result<(), ControlError> {
        match (self.run)("pactl", &strings(&["get-sink-volume", "@DEFAULT_SINK@"])) {
            Ok(out) => {
                let now = parse_percent(&out)
                    .ok_or_else(|| ControlError(format!("system volume: no volume in {out:?}")))?;
                let to = stepped(now, step, up);
                (self.run)(
                    "pactl",
                    &strings(&["set-sink-volume", "@DEFAULT_SINK@", &format!("{to}%")]),
                )
                .map(drop)
            }
            // No pactl: WirePlumber's own tool, limited to 100 %.
            Err(pactl) => {
                let by = format!(
                    "{}%{}",
                    (step * 100.0).round().max(1.0) as u32,
                    if up { "+" } else { "-" }
                );
                (self.run)(
                    "wpctl",
                    &strings(&["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", &by]),
                )
                .map(drop)
                .map_err(|wpctl| ControlError(format!("system volume: {}; {}", pactl.0, wpctl.0)))
            }
        }
    }
}

impl VolumeControl for PulseVolume {
    fn step_up(&mut self, step: f32) -> Result<(), ControlError> {
        self.step(step, true)
    }
    fn step_down(&mut self, step: f32) -> Result<(), ControlError> {
        self.step(step, false)
    }
}

/// One stream (`pactl -f json list sink-inputs`).
#[derive(Debug, Clone, PartialEq)]
pub struct SinkInput {
    pub index: u32,
    pub pid: Option<u32>,
    /// Volume as a fraction (the loudest channel).
    pub volume: f32,
}

/// The streams in `pactl -f json list sink-inputs` output.
pub fn parse_sink_inputs(json: &str) -> Result<Vec<SinkInput>, ControlError> {
    let list: Vec<serde_json::Value> = serde_json::from_str(json)
        .map_err(|e| ControlError(format!("app volume: the stream list is not JSON: {e}")))?;
    Ok(list
        .iter()
        .filter_map(|s| {
            let index = u32::try_from(s.get("index")?.as_u64()?).ok()?;
            let pid = s
                .pointer("/properties/application.process.id")
                .and_then(|p| {
                    p.as_str()
                        .and_then(|t| t.parse().ok())
                        .or(p.as_u64().and_then(|n| u32::try_from(n).ok()))
                });
            let volume = s
                .get("volume")
                .and_then(serde_json::Value::as_object)
                .map(|channels| {
                    channels
                        .values()
                        .filter_map(|c| {
                            parse_percent(&format!("/{}", c.get("value_percent")?.as_str()?))
                        })
                        .max()
                        .unwrap_or(100)
                })
                .unwrap_or(100);
            Some(SinkInput {
                index,
                pid,
                volume: volume as f32 / 100.0,
            })
        })
        .collect())
}

/// A stream as the core's [`step_app_sessions`] sees it: its process is
/// reported as the focused one when it is that process or one of its
/// children.
struct Stream<'a> {
    input: SinkInput,
    focused: u32,
    is_child: &'a dyn Fn(u32, u32) -> bool,
    run: &'a std::cell::RefCell<&'a mut Run>,
}

impl AudioSession for Stream<'_> {
    fn process_id(&self) -> Result<u32, ControlError> {
        Ok(match self.input.pid {
            Some(p) if p == self.focused || (self.is_child)(p, self.focused) => self.focused,
            Some(p) => p,
            None => 0,
        })
    }
    fn volume(&self) -> Result<f32, ControlError> {
        Ok(self.input.volume)
    }
    fn set_volume(&mut self, volume: f32) -> Result<(), ControlError> {
        let percent = (volume * 100.0).round().clamp(0.0, 100.0) as u32;
        (self.run.borrow_mut())(
            "pactl",
            &strings(&[
                "set-sink-input-volume",
                &self.input.index.to_string(),
                &format!("{percent}%"),
            ]),
        )
        .map(drop)
    }
}

/// Steps every stream of `focused` (and its children) by `delta`.
pub fn step_streams(
    run: &mut Run,
    focused: u32,
    is_child: &dyn Fn(u32, u32) -> bool,
    delta: f32,
) -> Result<(), ControlError> {
    let json = run("pactl", &strings(&["-f", "json", "list", "sink-inputs"]))?;
    let inputs = parse_sink_inputs(&json)?;
    // One Run, shared by the streams in turn.
    let run = std::cell::RefCell::new(run);
    let streams = inputs.into_iter().map(|input| Stream {
        input,
        focused,
        is_child,
        run: &run,
    });
    step_app_sessions(focused, streams, delta)
}

/// True when `pid` descends from `ancestor` (sysinfo's parent links).
pub fn descends_from(pid: u32, ancestor: u32) -> bool {
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let mut at = sysinfo::Pid::from_u32(pid);
    for _ in 0..32 {
        match sys.process(at).and_then(sysinfo::Process::parent) {
            Some(parent) if parent.as_u32() == ancestor => return true,
            Some(parent) if parent.as_u32() > 1 => at = parent,
            _ => return false,
        }
    }
    false
}

/// The pid of the focused window (X11 `_NET_ACTIVE_WINDOW` / `_NET_WM_PID`).
pub fn focused_pid() -> Result<u32, ControlError> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
    if crate::linux_inject::current_session() == crate::linux_inject::Session::Wayland {
        return Err(ControlError(WAYLAND.into()));
    }
    let err = |e: &dyn std::fmt::Display| ControlError(format!("focused window: {e}"));
    let (conn, n) = x11rb::connect(None).map_err(|e| err(&e))?;
    let root = conn.setup().roots[n].root;
    let atom = |name: &[u8]| -> Result<u32, ControlError> {
        Ok(conn
            .intern_atom(false, name)
            .map_err(|e| err(&e))?
            .reply()
            .map_err(|e| err(&e))?
            .atom)
    };
    let active = conn
        .get_property(
            false,
            root,
            atom(b"_NET_ACTIVE_WINDOW")?,
            AtomEnum::WINDOW,
            0,
            1,
        )
        .map_err(|e| err(&e))?
        .reply()
        .map_err(|e| err(&e))?
        .value32()
        .and_then(|mut v| v.next())
        .filter(|w| *w != 0)
        .ok_or_else(|| ControlError("no foreground window".into()))?;
    conn.get_property(
        false,
        active,
        atom(b"_NET_WM_PID")?,
        AtomEnum::CARDINAL,
        0,
        1,
    )
    .map_err(|e| err(&e))?
    .reply()
    .map_err(|e| err(&e))?
    .value32()
    .and_then(|mut v| v.next())
    .filter(|p| *p != 0)
    .ok_or_else(|| ControlError("the focused window does not say its process".into()))
}

/// What app volume says on Wayland.
pub const WAYLAND: &str =
    "app volume needs an X11 session on Linux (Wayland keeps the focused window private)";

/// The focused app's streams.
pub struct PulseAppVolume {
    run: Run,
}

impl PulseAppVolume {
    pub fn new(run: Run) -> Self {
        Self { run }
    }
    fn step(&mut self, delta: f32) -> Result<(), ControlError> {
        let pid = focused_pid()?;
        step_streams(&mut self.run, pid, &descends_from, delta)
    }
}

impl AppVolumeControl for PulseAppVolume {
    fn step_up(&mut self, step: f32) -> Result<(), ControlError> {
        self.step(step)
    }
    fn step_down(&mut self, step: f32) -> Result<(), ControlError> {
        self.step(-step)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    type Calls = Arc<Mutex<Vec<String>>>;

    /// A Run answering from `answers` (by program + first arg) and recording calls.
    fn fake(answers: Vec<(&'static str, Result<&'static str, &'static str>)>) -> (Run, Calls) {
        let calls: Calls = Arc::default();
        let seen = Arc::clone(&calls);
        let run: Run = Box::new(move |program, args| {
            let line = format!("{program} {}", args.join(" "));
            seen.lock().unwrap().push(line.clone());
            match answers.iter().find(|(p, _)| line.starts_with(p)) {
                Some((_, Ok(out))) => Ok(out.to_string()),
                Some((_, Err(e))) => Err(ControlError(e.to_string())),
                None => Ok(String::new()),
            }
        });
        (run, calls)
    }

    #[test]
    fn percentages_parse_and_step_within_0_to_100() {
        let out = "Volume: front-left: 32768 /  50% / -18.06 dB,   front-right: 32768 /  50% / -18.06 dB\n        balance 0.00\n";
        assert_eq!(parse_percent(out), Some(50));
        assert_eq!(parse_percent("nothing"), None);
        assert_eq!(stepped(50, 0.05, true), 55);
        assert_eq!(stepped(98, 0.05, true), 100);
        assert_eq!(stepped(3, 0.05, false), 0);
        assert_eq!(stepped(50, 0.001, true), 51, "a tiny step still moves");
    }

    #[test]
    fn system_volume_reads_then_sets_the_default_sink() {
        let (run, calls) = fake(vec![(
            "pactl get-sink-volume",
            Ok("Volume: front-left: 1 /  40% / x"),
        )]);
        let mut v = PulseVolume::new(run);
        v.step_up(0.1).unwrap();
        assert_eq!(
            *calls.lock().unwrap(),
            [
                "pactl get-sink-volume @DEFAULT_SINK@",
                "pactl set-sink-volume @DEFAULT_SINK@ 50%"
            ]
        );
    }

    #[test]
    fn without_pactl_wpctl_steps_it_capped_at_100() {
        let (run, calls) = fake(vec![("pactl", Err("pactl: No such file"))]);
        let mut v = PulseVolume::new(run);
        v.step_down(0.05).unwrap();
        assert_eq!(
            calls.lock().unwrap()[1],
            "wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 5%-"
        );
        let (run, _) = fake(vec![("pactl", Err("no pactl")), ("wpctl", Err("no wpctl"))]);
        let e = PulseVolume::new(run).step_up(0.05).unwrap_err();
        assert_eq!(e.0, "system volume: no pactl; no wpctl");
    }

    const INPUTS: &str = r#"[
      {"index": 41, "properties": {"application.process.id": "1200", "application.name": "Firefox"},
       "volume": {"front-left": {"value": 39321, "value_percent": "60%", "db": "-13 dB"}, "front-right": {"value": 39321, "value_percent": "60%", "db": "-13 dB"}}},
      {"index": 42, "properties": {"application.process.id": "777"},
       "volume": {"mono": {"value": 65536, "value_percent": "100%", "db": "0 dB"}}},
      {"index": 43, "properties": {},
       "volume": {"mono": {"value_percent": "20%"}}}
    ]"#;

    #[test]
    fn streams_parse_from_pactls_json() {
        let s = parse_sink_inputs(INPUTS).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(
            s[0],
            SinkInput {
                index: 41,
                pid: Some(1200),
                volume: 0.6
            }
        );
        assert_eq!(s[2].pid, None);
        assert!(parse_sink_inputs("not json").is_err());
    }

    #[test]
    fn app_volume_steps_the_focused_apps_streams_and_its_childrens() {
        let (mut run, calls) = fake(vec![("pactl -f json list sink-inputs", Ok(INPUTS))]);
        // 1200 is a child of the focused 1000 (a browser's audio process).
        let child = |p: u32, a: u32| p == 1200 && a == 1000;
        step_streams(&mut run, 1000, &child, 0.1).unwrap();
        assert_eq!(
            calls.lock().unwrap()[1],
            "pactl set-sink-input-volume 41 70%"
        );
        assert_eq!(calls.lock().unwrap().len(), 2, "only Firefox's stream");
        let (mut run, _) = fake(vec![("pactl -f json", Ok(INPUTS))]);
        let e = step_streams(&mut run, 5, &|_, _| false, 0.1).unwrap_err();
        assert_eq!(e.0, freewheel_core::overrides::NO_AUDIO_SESSION);
    }
}
