//! The OS controls behind the dial overrides (Wheel tab: "Change the volume",
//! "Change screen brightness"), handed to the device loop's override worker.
//! A control this OS has no backend for fails every step with
//! [`UNAVAILABLE`], which the worker logs; keys keep working.

use freewheel_core::overrides::{
    AppVolumeControl, BrightnessControl, ControlError, Controls, VolumeControl,
};

/// The error of a control this OS has no backend for.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))] // Windows has every backend; macOS uses its own reasons
pub const UNAVAILABLE: &str = "not available on this system";

/// A control that is not there: every step fails with `self.0`.
#[cfg_attr(windows, allow(dead_code))]
pub struct Unavailable(pub &'static str);

#[cfg_attr(windows, allow(dead_code))]
impl Unavailable {
    fn fail(&self) -> Result<(), ControlError> {
        Err(ControlError(self.0.into()))
    }
}

impl VolumeControl for Unavailable {
    fn step_up(&mut self, _: f32) -> Result<(), ControlError> {
        self.fail()
    }
    fn step_down(&mut self, _: f32) -> Result<(), ControlError> {
        self.fail()
    }
}

impl AppVolumeControl for Unavailable {
    fn step_up(&mut self, _: f32) -> Result<(), ControlError> {
        self.fail()
    }
    fn step_down(&mut self, _: f32) -> Result<(), ControlError> {
        self.fail()
    }
}

impl BrightnessControl for Unavailable {
    fn step_up(&mut self, _: u8) -> Result<(), ControlError> {
        self.fail()
    }
    fn step_down(&mut self, _: u8) -> Result<(), ControlError> {
        self.fail()
    }
}

/// Steps every brightness backend (macOS: the built-in panel and DDC
/// monitors). It fails only when every backend fails, with all their errors.
#[cfg(any(target_os = "macos", test))]
pub struct EveryDisplay(pub Vec<Box<dyn BrightnessControl>>);

#[cfg(any(target_os = "macos", test))]
impl EveryDisplay {
    fn each(
        &mut self,
        mut step: impl FnMut(&mut dyn BrightnessControl) -> Result<(), ControlError>,
    ) -> Result<(), ControlError> {
        let mut errors = Vec::new();
        for control in &mut self.0 {
            if let Err(e) = step(control.as_mut()) {
                errors.push(e.0);
            }
        }
        if errors.len() < self.0.len() {
            Ok(())
        } else {
            Err(ControlError(errors.join("; ")))
        }
    }
}

#[cfg(any(target_os = "macos", test))]
impl BrightnessControl for EveryDisplay {
    fn step_up(&mut self, step: u8) -> Result<(), ControlError> {
        self.each(|c| c.step_up(step))
    }
    fn step_down(&mut self, step: u8) -> Result<(), ControlError> {
        self.each(|c| c.step_down(step))
    }
}

/// This OS's controls.
#[cfg(windows)]
pub fn os_controls() -> Controls {
    Controls {
        volume: Box::new(crate::volume::EndpointVolume),
        app_volume: Box::new(crate::app_volume::WindowsAppVolume),
        brightness: Box::new(crate::brightness::WindowsBrightness),
    }
}

/// This OS's controls. App volume needs a frontmost-app source, which is not
/// built yet, so it reports the ADR-010 fallback text.
#[cfg(target_os = "macos")]
pub fn os_controls() -> Controls {
    Controls {
        volume: Box::new(crate::macos_volume::system_volume()),
        app_volume: Box::new(Unavailable(freewheel_core::overrides::TAP_NOT_AVAILABLE)),
        brightness: Box::new(EveryDisplay(vec![
            Box::new(crate::builtin_brightness::builtin_brightness()),
            Box::new(crate::ddc_macos::ddc_brightness()),
        ])),
    }
}

/// This OS's controls: none yet.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn os_controls() -> Controls {
    Controls {
        volume: Box::new(Unavailable(UNAVAILABLE)),
        app_volume: Box::new(Unavailable(UNAVAILABLE)),
        brightness: Box::new(Unavailable(UNAVAILABLE)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freewheel_core::overrides::ControlCall::{Down, Up};
    use freewheel_core::overrides::FakeControl;

    #[test]
    fn every_display_steps_each_backend_and_fails_only_when_all_do() {
        let (panel, monitor) = (FakeControl::new(), FakeControl::failing());
        let mut both = EveryDisplay(vec![Box::new(panel.clone()), Box::new(monitor.clone())]);
        assert_eq!(both.step_up(5), Ok(()));
        assert_eq!(both.step_down(10), Ok(()));
        assert_eq!(panel.calls(), [Up(5.0), Down(10.0)]);
        assert_eq!(monitor.calls(), [Up(5.0), Down(10.0)]);

        let mut none = EveryDisplay(vec![
            Box::new(Unavailable("panel off")),
            Box::new(Unavailable("no monitor")),
        ]);
        assert_eq!(
            none.step_up(5),
            Err(ControlError("panel off; no monitor".into()))
        );
    }

    #[test]
    fn a_control_without_a_backend_says_so() {
        let mut controls = os_controls();
        if cfg!(not(any(windows, target_os = "macos"))) {
            assert_eq!(
                controls.volume.step_up(0.1),
                Err(ControlError(UNAVAILABLE.into()))
            );
            assert_eq!(
                controls.brightness.step_down(5),
                Err(ControlError(UNAVAILABLE.into()))
            );
        }
    }
}
