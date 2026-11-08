//! The Windows [`BrightnessControl`]: external monitors over DDC/CI with
//! Dxva2 (`GetMonitorBrightness` / `SetMonitorBrightness`) and the built-in
//! panel with WMI (`WmiMonitorBrightnessMethods.WmiSetBrightness`). Displays
//! are listed again on every step, so hot-plugged monitors are picked up.
//! Compiled only on Windows and checked by hand (README, manual checklist).

// Wired into the override worker by a later card.
#![allow(dead_code)]

use std::rc::Rc;

use freewheel_core::overrides::{BrightnessControl, ControlError, Level, Screen, step_displays};
use serde::{Deserialize, Serialize};
use windows_sys::Win32::Devices::Display::{
    DestroyPhysicalMonitor, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, PHYSICAL_MONITOR, SetMonitorBrightness,
};
use windows_sys::Win32::Foundation::{HANDLE, LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
use wmi::WMIConnection;

pub struct WindowsBrightness;

impl WindowsBrightness {
    fn step(percent: i32) -> Result<(), ControlError> {
        let mut screens = ddc_monitors();
        screens.extend(panels());
        step_displays(screens, percent)
    }
}

impl BrightnessControl for WindowsBrightness {
    fn step_up(&mut self, step: u8) -> Result<(), ControlError> {
        Self::step(step.into())
    }
    fn step_down(&mut self, step: u8) -> Result<(), ControlError> {
        Self::step(-i32::from(step))
    }
}

fn os_error(call: &str) -> ControlError {
    ControlError(format!("{call}: {}", std::io::Error::last_os_error()))
}

/// A physical monitor from Dxva2; its handle is destroyed on drop.
struct DdcMonitor {
    handle: HANDLE,
    name: String,
}

impl Screen for DdcMonitor {
    fn name(&self) -> String {
        self.name.clone()
    }
    fn level(&mut self) -> Result<Level, ControlError> {
        let (mut min, mut current, mut max) = (0, 0, 0);
        // SAFETY: the handle is a live physical monitor until drop.
        if unsafe { GetMonitorBrightness(self.handle, &mut min, &mut current, &mut max) } == 0 {
            return Err(os_error("GetMonitorBrightness"));
        }
        Ok(Level { min, current, max })
    }
    fn set(&mut self, value: u32) -> Result<(), ControlError> {
        // SAFETY: as above.
        if unsafe { SetMonitorBrightness(self.handle, value) } == 0 {
            return Err(os_error("SetMonitorBrightness"));
        }
        Ok(())
    }
}

impl Drop for DdcMonitor {
    fn drop(&mut self) {
        // SAFETY: the handle came from GetPhysicalMonitorsFromHMONITOR.
        unsafe { DestroyPhysicalMonitor(self.handle) };
    }
}

unsafe extern "system" fn collect(monitor: HMONITOR, _: HDC, _: *mut RECT, list: LPARAM) -> i32 {
    // SAFETY: `list` is the `Vec` passed to EnumDisplayMonitors below.
    unsafe { (*(list as *mut Vec<HMONITOR>)).push(monitor) };
    1
}

fn ddc_monitors() -> Vec<Result<Box<dyn Screen>, ControlError>> {
    let mut monitors: Vec<HMONITOR> = Vec::new();
    let list = &mut monitors as *mut Vec<HMONITOR> as LPARAM;
    // SAFETY: `collect` only pushes to `monitors`, which outlives the call.
    if unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(collect), list) }
        == 0
    {
        return vec![Err(os_error("EnumDisplayMonitors"))];
    }
    let mut screens: Vec<Result<Box<dyn Screen>, ControlError>> = Vec::new();
    for (i, monitor) in monitors.into_iter().enumerate() {
        match physical_monitors(monitor) {
            Ok(found) => screens.extend(
                found
                    .into_iter()
                    .map(|m| Ok(Box::new(m) as Box<dyn Screen>)),
            ),
            Err(e) => screens.push(Err(ControlError(format!("display {}: {}", i + 1, e.0)))),
        }
    }
    screens
}

fn physical_monitors(monitor: HMONITOR) -> Result<Vec<DdcMonitor>, ControlError> {
    let mut n = 0;
    // SAFETY: `monitor` came from EnumDisplayMonitors; `list` holds `n` entries.
    if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, &mut n) } == 0 {
        return Err(os_error("GetNumberOfPhysicalMonitorsFromHMONITOR"));
    }
    let mut list = vec![unsafe { std::mem::zeroed::<PHYSICAL_MONITOR>() }; n as usize];
    if unsafe { GetPhysicalMonitorsFromHMONITOR(monitor, n, list.as_mut_ptr()) } == 0 {
        return Err(os_error("GetPhysicalMonitorsFromHMONITOR"));
    }
    Ok(list
        .into_iter()
        .map(|m| {
            // PHYSICAL_MONITOR is packed: copy the array out instead of borrowing it.
            let desc = m.szPhysicalMonitorDescription;
            let len = desc.iter().position(|&c| c == 0).unwrap_or(desc.len());
            DdcMonitor {
                handle: m.hPhysicalMonitor,
                name: String::from_utf16_lossy(&desc[..len]),
            }
        })
        .collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WmiMonitorBrightness {
    instance_name: String,
    current_brightness: u8,
}

#[derive(Deserialize)]
struct WmiMonitorBrightnessMethods {
    #[serde(rename = "__Path")]
    path: String,
    #[serde(rename = "InstanceName")]
    instance_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct WmiSetBrightness {
    timeout: u32,
    brightness: u8,
}

/// The built-in panel, in percent, through WMI.
struct Panel {
    wmi: Rc<WMIConnection>,
    path: String,
    name: String,
    current: u8,
}

impl Screen for Panel {
    fn name(&self) -> String {
        format!("built-in panel {}", self.name)
    }
    fn level(&mut self) -> Result<Level, ControlError> {
        let current = self.current.into();
        Ok(Level {
            min: 0,
            current,
            max: 100,
        })
    }
    fn set(&mut self, value: u32) -> Result<(), ControlError> {
        let args = WmiSetBrightness {
            timeout: 0,
            brightness: value.min(100) as u8,
        };
        self.wmi
            .exec_instance_method::<WmiMonitorBrightnessMethods, ()>(
                &self.path,
                "WmiSetBrightness",
                args,
            )
            .map_err(|e| ControlError(format!("WmiSetBrightness: {e}")))
    }
}

/// The built-in panels; a desktop with no WMI brightness class gives an error.
fn panels() -> Vec<Result<Box<dyn Screen>, ControlError>> {
    match wmi_panels() {
        Ok(found) => found
            .into_iter()
            .map(|p| Ok(Box::new(p) as Box<dyn Screen>))
            .collect(),
        Err(e) => vec![Err(ControlError(format!("built-in panel: {e}")))],
    }
}

fn wmi_panels() -> Result<Vec<Panel>, wmi::WMIError> {
    // wmi initialises COM (MTA) and its default security on a thread that
    // has no COM yet, and reuses whatever this thread already set up.
    let wmi = Rc::new(WMIConnection::with_namespace_path(r"ROOT\WMI")?);
    let levels: Vec<WmiMonitorBrightness> = wmi.raw_query("SELECT * FROM WmiMonitorBrightness")?;
    let methods: Vec<WmiMonitorBrightnessMethods> =
        wmi.raw_query("SELECT * FROM WmiMonitorBrightnessMethods")?;
    Ok(methods
        .into_iter()
        .filter_map(|m| {
            let level = levels.iter().find(|l| l.instance_name == m.instance_name)?;
            Some(Panel {
                wmi: Rc::clone(&wmi),
                path: m.path,
                name: m.instance_name,
                current: level.current_brightness,
            })
        })
        .collect())
}
