//! The Windows [`VolumeControl`]: steps the default render endpoint with
//! `IAudioEndpointVolume::VolumeStepUp`/`VolumeStepDown`. The endpoint is
//! looked up on every call, so it follows a change of default device and no
//! COM pointer is held across threads.

#[cfg(windows)]
use freewheel_core::overrides::{ControlError, VolumeControl};
#[cfg(windows)]
use windows::Win32::{
    Media::Audio::{
        Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender,
    },
    System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    },
};

/// How many system volume steps make `step` (a fraction of full scale) on an
/// endpoint with `step_count` levels: at least one, at most the full range.
fn step_calls(step: f32, step_count: u32) -> u32 {
    let range = step_count.saturating_sub(1).max(1);
    ((step * range as f32).round() as u32).clamp(1, range)
}

/// The system output volume of the default render endpoint.
#[cfg(windows)]
pub struct EndpointVolume;

#[cfg(windows)]
impl EndpointVolume {
    fn step(&self, up: bool, step: f32) -> Result<(), ControlError> {
        let err = |e: windows::core::Error| ControlError(format!("system volume: {e}"));
        // SAFETY: plain COM calls on interfaces owned by this scope; COM is
        // initialised for this thread and uninitialised after they drop.
        unsafe {
            let init = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let result = (|| {
                let devices: IMMDeviceEnumerator =
                    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(err)?;
                let device = devices
                    .GetDefaultAudioEndpoint(eRender, eConsole)
                    .map_err(err)?;
                let volume: IAudioEndpointVolume =
                    device.Activate(CLSCTX_ALL, None).map_err(err)?;
                let (mut current, mut count) = (0, 0);
                volume
                    .GetVolumeStepInfo(&mut current, &mut count)
                    .map_err(err)?;
                for _ in 0..step_calls(step, count) {
                    if up {
                        volume.VolumeStepUp(std::ptr::null()).map_err(err)?;
                    } else {
                        volume.VolumeStepDown(std::ptr::null()).map_err(err)?;
                    }
                }
                Ok(())
            })();
            if init {
                CoUninitialize();
            }
            result
        }
    }
}

#[cfg(windows)]
impl VolumeControl for EndpointVolume {
    fn step_up(&mut self, step: f32) -> Result<(), ControlError> {
        self.step(true, step)
    }
    fn step_down(&mut self, step: f32) -> Result<(), ControlError> {
        self.step(false, step)
    }
}

#[cfg(test)]
mod tests {
    use super::step_calls;

    #[test]
    fn step_fraction_becomes_a_count_of_system_volume_steps() {
        // 51 levels (Windows default): 50 steps of 2%.
        assert_eq!(step_calls(0.02, 51), 1);
        assert_eq!(step_calls(0.1, 51), 5);
        // A tiny step still moves; a huge one stops at the full range.
        assert_eq!(step_calls(0.001, 51), 1);
        assert_eq!(step_calls(5.0, 51), 50);
        // A device reporting no range steps once.
        assert_eq!(step_calls(0.5, 0), 1);
    }
}
