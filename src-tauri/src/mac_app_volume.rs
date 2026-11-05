//! The macOS [`AppVolumeControl`] for one PID (ADR-010): a private Core Audio
//! process tap mutes the app, and an IOProc on a private aggregate device
//! (the default output plus the tap) plays the tapped audio back at the dial's
//! gain. Its logic is [`TapAppVolume`], tested in the core.
//!
//! Tapped audio only goes from the IOProc's input to its output buffers; this
//! module does no file or network I/O (A21).
//!
//! [manual] macOS 15: with the dial on the app-volume override and music
//! playing in one app, turning the jog wheel down lowers only that app (to
//! silence at 0) and turning it up restores it; leaving the override restores
//! the app's own volume. The first use asks for the audio-capture permission;
//! with it denied the status reads 'permission needed'.

// Wired into the override worker by a later card.
#![allow(dead_code)]

use std::ffi::c_void;
use std::sync::Arc;

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use freewheel_core::overrides::{
    ControlError, Gain, NO_AUDIO_SESSION, TapAppVolume, TapOpener, play_at_gain, tap_refused,
};
use objc2::msg_send;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool};
use tracing::warn;

type AudioObjectId = u32;
type OsStatus = i32;

#[repr(C)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

#[repr(C)]
struct AudioBuffer {
    channels: u32,
    bytes: u32,
    data: *mut c_void,
}

#[repr(C)]
struct AudioBufferList {
    count: u32,
    buffers: [AudioBuffer; 1],
}

type IoProc = unsafe extern "C" fn(
    AudioObjectId,
    *const c_void,
    *const AudioBufferList,
    *const c_void,
    *mut AudioBufferList,
    *const c_void,
    *mut c_void,
) -> OsStatus;

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectGetPropertyData(
        id: AudioObjectId,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> OsStatus;
    fn AudioHardwareCreateProcessTap(desc: *mut AnyObject, tap: *mut AudioObjectId) -> OsStatus;
    fn AudioHardwareDestroyProcessTap(tap: AudioObjectId) -> OsStatus;
    fn AudioHardwareCreateAggregateDevice(
        desc: *const c_void,
        device: *mut AudioObjectId,
    ) -> OsStatus;
    fn AudioHardwareDestroyAggregateDevice(device: AudioObjectId) -> OsStatus;
    fn AudioDeviceCreateIOProcID(
        device: AudioObjectId,
        proc_: IoProc,
        client: *mut c_void,
        id: *mut Option<IoProc>,
    ) -> OsStatus;
    fn AudioDeviceDestroyIOProcID(device: AudioObjectId, id: Option<IoProc>) -> OsStatus;
    fn AudioDeviceStart(device: AudioObjectId, id: Option<IoProc>) -> OsStatus;
    fn AudioDeviceStop(device: AudioObjectId, id: Option<IoProc>) -> OsStatus;
}

const SYSTEM_OBJECT: AudioObjectId = 1;
const fn code(c: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*c)
}

/// One OS call's status, logged and turned into a [`ControlError`].
fn check(what: &str, status: OsStatus) -> Result<(), ControlError> {
    if status == 0 {
        return Ok(());
    }
    warn!(what, status, "Core Audio call failed");
    Err(ControlError(format!("{what} failed (OSStatus {status})")))
}

/// Reads a global property of `id` into `T`.
unsafe fn property<T>(
    id: AudioObjectId,
    selector: &[u8; 4],
    pid: Option<i32>,
) -> Result<T, ControlError> {
    let address = PropertyAddress {
        selector: code(selector),
        scope: code(b"glob"),
        element: 0,
    };
    let (qsize, q) = match &pid {
        Some(p) => (4, (p as *const i32).cast()),
        None => (0, std::ptr::null()),
    };
    let mut out = std::mem::MaybeUninit::<T>::zeroed();
    let mut size = size_of::<T>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(id, &address, qsize, q, &mut size, out.as_mut_ptr().cast())
    };
    check("AudioObjectGetPropertyData", status)?;
    Ok(unsafe { out.assume_init() })
}

unsafe fn string_property(id: AudioObjectId, selector: &[u8; 4]) -> Result<CFString, ControlError> {
    let s: CFStringRef = unsafe { property(id, selector, None)? };
    if s.is_null() {
        return Err(ControlError("Core Audio returned no UID".into()));
    }
    Ok(unsafe { CFString::wrap_under_create_rule(s) })
}

/// Plays the tap's input at the shared gain on the output device.
unsafe extern "C" fn play(
    _device: AudioObjectId,
    _now: *const c_void,
    input: *const AudioBufferList,
    _input_time: *const c_void,
    output: *mut AudioBufferList,
    _output_time: *const c_void,
    client: *mut c_void,
) -> OsStatus {
    let gain = unsafe { &*client.cast::<Gain>() }.get();
    let ins = unsafe { buffers(input) };
    for (i, out) in unsafe { buffers(output) }.iter().enumerate() {
        let inp: &[f32] = ins.get(i).map_or(&[], |b| unsafe { samples(b) });
        play_at_gain(inp, unsafe { samples(out) }, gain);
    }
    0
}

/// The buffers of a Core Audio buffer list (none for a null list).
unsafe fn buffers<'a>(list: *const AudioBufferList) -> &'a [AudioBuffer] {
    match unsafe { list.as_ref() } {
        Some(l) => unsafe { std::slice::from_raw_parts(l.buffers.as_ptr(), l.count as usize) },
        None => &[],
    }
}

/// The float32 samples of one buffer.
unsafe fn samples<'a>(b: &AudioBuffer) -> &'a mut [f32] {
    if b.data.is_null() {
        return Default::default();
    }
    unsafe { std::slice::from_raw_parts_mut(b.data.cast(), b.bytes as usize / size_of::<f32>()) }
}

/// A live muting tap, its aggregate device and the playback IOProc.
pub struct CoreAudioTap {
    tap: AudioObjectId,
    device: AudioObjectId,
    io_proc: Option<IoProc>,
    gain: Arc<Gain>,
}

impl Drop for CoreAudioTap {
    fn drop(&mut self) {
        unsafe {
            if self.io_proc.is_some() {
                AudioDeviceStop(self.device, self.io_proc);
                AudioDeviceDestroyIOProcID(self.device, self.io_proc);
            }
            if self.device != 0 {
                AudioHardwareDestroyAggregateDevice(self.device);
            }
            AudioHardwareDestroyProcessTap(self.tap);
        }
    }
}

/// A private, muting stereo-mixdown tap of the process object `process`.
unsafe fn create_tap(process: AudioObjectId) -> Result<AudioObjectId, ControlError> {
    let Some(class) = AnyClass::get(c"CATapDescription") else {
        warn!("CATapDescription is missing");
        return Err(tap_refused(0));
    };
    let processes = CFArray::from_CFTypes(&[CFNumber::from(process as i32)]);
    let processes = unsafe { &*processes.as_concrete_TypeRef().cast::<AnyObject>() };
    let desc: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
    let desc: Option<Retained<AnyObject>> =
        unsafe { msg_send![desc, initStereoMixdownOfProcesses: processes] };
    let Some(desc) = desc else {
        warn!("CATapDescription init failed");
        return Err(tap_refused(0));
    };
    // CATapMuted: the app is heard only through the playback below.
    let _: () = unsafe { msg_send![&*desc, setMuteBehavior: 1isize] };
    let _: () = unsafe { msg_send![&*desc, setPrivate: Bool::YES] };
    let mut tap = 0;
    let status =
        unsafe { AudioHardwareCreateProcessTap(Retained::as_ptr(&desc).cast_mut(), &mut tap) };
    if status != 0 {
        warn!(status, "AudioHardwareCreateProcessTap refused");
        return Err(tap_refused(status));
    }
    Ok(tap)
}

/// The aggregate of the default output device and the tap.
fn aggregate(pid: u32, output: &CFString, tap: &CFString) -> CFDictionary<CFString, CFType> {
    let key = CFString::from_static_string;
    let sub = CFDictionary::from_CFType_pairs(&[(key("uid"), output.as_CFType())]);
    let tap = CFDictionary::from_CFType_pairs(&[
        (key("uid"), tap.as_CFType()),
        (key("drift"), CFBoolean::true_value().as_CFType()),
    ]);
    let uid = format!("com.brinkercode.freewheel.tap.{}.{pid}", std::process::id());
    CFDictionary::from_CFType_pairs(&[
        (
            key("name"),
            CFString::new("freewheel app volume").as_CFType(),
        ),
        (key("uid"), CFString::new(&uid).as_CFType()),
        (key("master"), output.as_CFType()),
        (key("private"), CFBoolean::true_value().as_CFType()),
        (key("stacked"), CFBoolean::false_value().as_CFType()),
        (key("tapautostart"), CFBoolean::true_value().as_CFType()),
        (key("subdevices"), CFArray::from_CFTypes(&[sub]).as_CFType()),
        (key("taps"), CFArray::from_CFTypes(&[tap]).as_CFType()),
    ])
}

/// Opens [`CoreAudioTap`]s.
pub struct CoreAudioTaps;

impl TapOpener for CoreAudioTaps {
    type Tap = CoreAudioTap;

    fn open(&mut self, pid: u32, gain: Arc<Gain>) -> Result<CoreAudioTap, ControlError> {
        unsafe {
            let pid_i32 = i32::try_from(pid).map_err(|_| ControlError(NO_AUDIO_SESSION.into()))?;
            let process: AudioObjectId = property(SYSTEM_OBJECT, b"id2p", Some(pid_i32))?;
            if process == 0 {
                return Err(ControlError(NO_AUDIO_SESSION.into()));
            }
            let mut t = CoreAudioTap {
                tap: create_tap(process)?,
                device: 0,
                io_proc: None,
                gain,
            };
            let tap_uid = string_property(t.tap, b"tuid")?;
            let output: AudioObjectId = property(SYSTEM_OBJECT, b"dOut", None)?;
            let output_uid = string_property(output, b"uid ")?;
            let desc = aggregate(pid, &output_uid, &tap_uid);
            check(
                "AudioHardwareCreateAggregateDevice",
                AudioHardwareCreateAggregateDevice(
                    desc.as_concrete_TypeRef().cast(),
                    &mut t.device,
                ),
            )?;
            let client = Arc::as_ptr(&t.gain).cast_mut().cast();
            let mut io_proc = None;
            check(
                "AudioDeviceCreateIOProcID",
                AudioDeviceCreateIOProcID(t.device, play, client, &mut io_proc),
            )?;
            t.io_proc = io_proc;
            check("AudioDeviceStart", AudioDeviceStart(t.device, t.io_proc))?;
            Ok(t)
        }
    }
}

/// App volume for one PID.
pub type MacAppVolume = TapAppVolume<CoreAudioTaps>;

pub fn mac_app_volume(pid: u32) -> MacAppVolume {
    TapAppVolume::new(CoreAudioTaps, pid)
}
