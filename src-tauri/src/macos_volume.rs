//! The macOS [`VolumeScalar`]: the Core Audio default output device's
//! `kAudioDevicePropertyVolumeScalar`, on the main element or, when the device
//! has none, on its first two channels. The default device is looked up on
//! every call, so switching outputs needs no restart.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr;

use freewheel_core::overrides::{ControlError, ScalarVolume, VolumeScalar};

const fn fourcc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

const SYSTEM_OBJECT: u32 = 1; // kAudioObjectSystemObject
const DEFAULT_OUTPUT_DEVICE: u32 = fourcc(b"dOut"); // kAudioHardwarePropertyDefaultOutputDevice
const VOLUME_SCALAR: u32 = fourcc(b"volm"); // kAudioDevicePropertyVolumeScalar
const SCOPE_GLOBAL: u32 = fourcc(b"glob"); // kAudioObjectPropertyScopeGlobal
const SCOPE_OUTPUT: u32 = fourcc(b"outp"); // kAudioDevicePropertyScopeOutput
const ELEMENT_MAIN: u32 = 0; // kAudioObjectPropertyElementMain

#[repr(C)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectHasProperty(id: u32, address: *const PropertyAddress) -> u8;
    fn AudioObjectIsPropertySettable(
        id: u32,
        address: *const PropertyAddress,
        settable: *mut u8,
    ) -> i32;
    fn AudioObjectGetPropertyData(
        id: u32,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn AudioObjectSetPropertyData(
        id: u32,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        data_size: u32,
        data: *const c_void,
    ) -> i32;
}

fn check(status: i32, what: &str) -> Result<(), ControlError> {
    match status {
        0 => Ok(()),
        s => Err(ControlError(format!("Core Audio {what} failed: OSStatus {s}"))),
    }
}

fn get<T: Default>(id: u32, address: &PropertyAddress, what: &str) -> Result<T, ControlError> {
    let mut value = T::default();
    let mut size = size_of::<T>() as u32;
    // SAFETY: `value` is a live `T` and `size` is its size in bytes.
    let status = unsafe {
        AudioObjectGetPropertyData(
            id,
            address,
            0,
            ptr::null(),
            &mut size,
            (&raw mut value).cast(),
        )
    };
    check(status, what)?;
    Ok(value)
}

fn volume(element: u32) -> PropertyAddress {
    PropertyAddress {
        selector: VOLUME_SCALAR,
        scope: SCOPE_OUTPUT,
        element,
    }
}

/// The default output device and its elements with a settable volume.
fn settable_volumes() -> Result<(u32, Vec<PropertyAddress>), ControlError> {
    let default_output = PropertyAddress {
        selector: DEFAULT_OUTPUT_DEVICE,
        scope: SCOPE_GLOBAL,
        element: ELEMENT_MAIN,
    };
    let device: u32 = get(SYSTEM_OBJECT, &default_output, "default output device")?;
    for elements in [&[ELEMENT_MAIN][..], &[1, 2]] {
        let mut found = Vec::new();
        for &element in elements {
            let address = volume(element);
            let mut settable = 0u8;
            // SAFETY: `address` and `settable` are live for the call.
            let ok = unsafe {
                AudioObjectHasProperty(device, &address) != 0
                    && AudioObjectIsPropertySettable(device, &address, &mut settable) == 0
            };
            if ok && settable != 0 {
                found.push(address);
            }
        }
        if !found.is_empty() {
            return Ok((device, found));
        }
    }
    Ok((device, Vec::new()))
}

pub struct CoreAudioVolume;

/// The system [`VolumeControl`](freewheel_core::overrides::VolumeControl) on macOS.
pub fn system_volume() -> ScalarVolume<CoreAudioVolume> {
    ScalarVolume(CoreAudioVolume)
}

impl VolumeScalar for CoreAudioVolume {
    fn get(&mut self) -> Result<Option<f32>, ControlError> {
        let (device, addresses) = settable_volumes()?;
        if addresses.is_empty() {
            return Ok(None);
        }
        let mut sum = 0.0;
        for address in &addresses {
            sum += get::<f32>(device, address, "volume read")?;
        }
        Ok(Some(sum / addresses.len() as f32))
    }

    fn set(&mut self, value: f32) -> Result<(), ControlError> {
        let (device, addresses) = settable_volumes()?;
        for address in &addresses {
            // SAFETY: `value` is a live f32 and the size passed is its size.
            let status = unsafe {
                AudioObjectSetPropertyData(
                    device,
                    address,
                    0,
                    ptr::null(),
                    size_of::<f32>() as u32,
                    (&raw const value).cast(),
                )
            };
            check(status, "volume write")?;
        }
        Ok(())
    }
}
