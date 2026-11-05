//! The Windows [`AppVolumeControl`]: steps the `ISimpleAudioVolume` of every
//! audio session on the default output device that the foreground window's
//! process owns. Its logic is [`step_app_sessions`], tested in the core.
//!
//! [manual] Windows 11: with the dial on the app-volume override and a video
//! playing in the focused app, turning the jog wheel moves only that app's
//! slider in the Volume Mixer; focusing an app with no audio reports
//! 'no audio session'.

// Wired into the override worker by a later card.
#![allow(dead_code)]

use freewheel_core::overrides::{AppVolumeControl, AudioSession, ControlError, step_app_sessions};
use windows::Win32::Media::Audio::{
    IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
    MMDeviceEnumerator, eMultimedia, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
use windows::core::Interface;

fn err(e: windows::core::Error) -> ControlError {
    ControlError(e.to_string())
}

struct Session {
    control: IAudioSessionControl2,
    volume: ISimpleAudioVolume,
}

impl AudioSession for Session {
    fn process_id(&self) -> Result<u32, ControlError> {
        unsafe { self.control.GetProcessId() }.map_err(err)
    }
    fn volume(&self) -> Result<f32, ControlError> {
        unsafe { self.volume.GetMasterVolume() }.map_err(err)
    }
    fn set_volume(&mut self, volume: f32) -> Result<(), ControlError> {
        unsafe { self.volume.SetMasterVolume(volume, std::ptr::null()) }.map_err(err)
    }
}

/// The process ID of the foreground window.
fn foreground_pid() -> Result<u32, ControlError> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&raw mut pid)) };
    if pid == 0 {
        return Err(ControlError("no foreground window".into()));
    }
    Ok(pid)
}

/// The audio sessions of the default output device.
fn sessions() -> Result<Vec<Session>, ControlError> {
    unsafe {
        // Already initialised on this thread is fine; a real failure surfaces
        // from CoCreateInstance.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let devices: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(err)?;
        let device = devices
            .GetDefaultAudioEndpoint(eRender, eMultimedia)
            .map_err(err)?;
        let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None).map_err(err)?;
        let list = manager.GetSessionEnumerator().map_err(err)?;
        let mut out = Vec::new();
        for i in 0..list.GetCount().map_err(err)? {
            let control = list.GetSession(i).map_err(err)?;
            out.push(Session {
                control: control.cast().map_err(err)?,
                volume: control.cast().map_err(err)?,
            });
        }
        Ok(out)
    }
}

pub struct WindowsAppVolume;

impl WindowsAppVolume {
    fn step(delta: f32) -> Result<(), ControlError> {
        step_app_sessions(foreground_pid()?, sessions()?, delta)
    }
}

impl AppVolumeControl for WindowsAppVolume {
    fn step_up(&mut self, step: f32) -> Result<(), ControlError> {
        Self::step(step)
    }
    fn step_down(&mut self, step: f32) -> Result<(), ControlError> {
        Self::step(-step)
    }
}
