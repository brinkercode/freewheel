//! The macOS DDC/CI transport behind [`DdcBrightness`], following
//! MonitorControl's approach (written from the public DDC/CI and IOKit
//! headers, not its code):
//! - arm64: the `DCPAVServiceProxy` services whose `Location` is `External`,
//!   through the private `IOAVService` I2C calls.
//! - x86_64: each `IOFramebuffer` with an I2C bus, through `IOI2CInterface`.
//!
//! Every call is looked up with `dlsym` first; a missing symbol makes the probe
//! fail, so the path reports unavailable instead of crashing. OS errors end up
//! in the display's error, which the core logs.
//!
//! [manual] macOS 15 on arm64 and on x86_64 (the release
//! checklist in README.md): with one or more DDC monitors plugged in and the dial on
//! the brightness override, turning the jog wheel steps every monitor;
//! unplugging the DDC cable of one monitor gives that monitor its own error
//! while the others still step.

// Wired into the override worker by a later card.
#![allow(dead_code)]

use std::ffi::{CStr, c_char, c_void};
use std::time::Duration;

use freewheel_core::ddc::{DDC_ADDRESS, DdcBrightness, DdcDisplay};
use freewheel_core::overrides::ControlError;

type IoObject = u32;
type CfRef = *const c_void;

/// DDC/CI: the host waits this long before reading a reply.
const REPLY_DELAY: Duration = Duration::from_millis(40);
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;
const UTF8: u32 = 0x0800_0100;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingServices(port: u32, matching: *mut c_void, it: *mut IoObject) -> i32;
    fn IOIteratorNext(it: IoObject) -> IoObject;
    fn IOObjectRelease(o: IoObject) -> i32;
    fn IORegistryEntryCreateCFProperty(e: IoObject, key: CfRef, a: CfRef, o: u32) -> CfRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(a: CfRef, s: *const c_char, encoding: u32) -> CfRef;
    fn CFEqual(a: CfRef, b: CfRef) -> u8;
    fn CFRelease(cf: CfRef);
}

unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

/// The DDC brightness control of this Mac.
pub fn ddc_brightness() -> DdcBrightness {
    DdcBrightness::new(Box::new(probe))
}

/// Looks up `name`; `F` must be its C signature.
fn symbol<F: Copy>(name: &CStr) -> Result<F, ControlError> {
    let p = unsafe { dlsym(RTLD_DEFAULT, name.as_ptr()) };
    if p.is_null() {
        return Err(ControlError(format!(
            "{} not found",
            name.to_string_lossy()
        )));
    }
    // SAFETY: the caller names the symbol's own signature as F.
    Ok(unsafe { std::mem::transmute_copy(&p) })
}

fn check(r: i32, what: &str) -> Result<(), ControlError> {
    match r {
        0 => Ok(()),
        _ => Err(ControlError(format!("{what} failed: IOReturn {r:#x}"))),
    }
}

/// An IOKit object released on drop.
struct Service(IoObject);

impl Drop for Service {
    fn drop(&mut self) {
        unsafe { IOObjectRelease(self.0) };
    }
}

/// The services of IOKit class `class`.
fn services(class: &CStr) -> Result<Vec<Service>, ControlError> {
    let mut it = 0;
    let r = unsafe { IOServiceGetMatchingServices(0, IOServiceMatching(class.as_ptr()), &mut it) };
    check(r, "IOServiceGetMatchingServices")?;
    let it = Service(it);
    Ok(
        std::iter::from_fn(|| match unsafe { IOIteratorNext(it.0) } {
            0 => None,
            s => Some(Service(s)),
        })
        .collect(),
    )
}

#[cfg(target_arch = "aarch64")]
use arm64::probe;
#[cfg(target_arch = "x86_64")]
use x86_64::probe;

#[cfg(target_arch = "aarch64")]
mod arm64 {
    use std::ptr::null;

    use super::*;

    type Create = unsafe extern "C" fn(alloc: CfRef, service: IoObject) -> CfRef;
    type I2c =
        unsafe extern "C" fn(av: CfRef, chip: u32, offset: u32, buf: *mut u8, len: u32) -> i32;

    struct AvDisplay {
        name: String,
        av: CfRef,
        read: I2c,
        write: I2c,
    }

    impl Drop for AvDisplay {
        fn drop(&mut self) {
            unsafe { CFRelease(self.av) };
        }
    }

    impl DdcDisplay for AvDisplay {
        fn name(&self) -> String {
            self.name.clone()
        }
        fn write(&mut self, packet: &[u8]) -> Result<(), ControlError> {
            let mut data = packet[1..].to_vec();
            let (chip, offset) = (DDC_ADDRESS.into(), packet[0].into());
            let r = unsafe {
                (self.write)(self.av, chip, offset, data.as_mut_ptr(), data.len() as u32)
            };
            check(r, "IOAVServiceWriteI2C")
        }
        fn read(&mut self, request: &[u8], reply: &mut [u8]) -> Result<(), ControlError> {
            self.write(request)?;
            std::thread::sleep(REPLY_DELAY);
            let len = reply.len() as u32;
            let r = unsafe { (self.read)(self.av, DDC_ADDRESS.into(), 0, reply.as_mut_ptr(), len) };
            check(r, "IOAVServiceReadI2C")
        }
    }

    /// Whether registry entry `s` has `Location` = `External`.
    fn is_external(s: &Service) -> bool {
        unsafe {
            let key = CFStringCreateWithCString(null(), c"Location".as_ptr(), UTF8);
            let ext = CFStringCreateWithCString(null(), c"External".as_ptr(), UTF8);
            let v = IORegistryEntryCreateCFProperty(s.0, key, null(), 0);
            let yes = !v.is_null() && CFEqual(v, ext) != 0;
            if !v.is_null() {
                CFRelease(v);
            }
            CFRelease(key);
            CFRelease(ext);
            yes
        }
    }

    pub fn probe() -> Result<Vec<Box<dyn DdcDisplay>>, ControlError> {
        let create: Create = symbol(c"IOAVServiceCreateWithService")?;
        let read: I2c = symbol(c"IOAVServiceReadI2C")?;
        let write: I2c = symbol(c"IOAVServiceWriteI2C")?;
        let mut out: Vec<Box<dyn DdcDisplay>> = Vec::new();
        for s in services(c"DCPAVServiceProxy")?
            .iter()
            .filter(|s| is_external(s))
        {
            let av = unsafe { create(null(), s.0) };
            if av.is_null() {
                continue;
            }
            let name = format!("external display {}", out.len() + 1);
            out.push(Box::new(AvDisplay {
                name,
                av,
                read,
                write,
            }));
        }
        Ok(out)
    }
}

#[cfg(target_arch = "x86_64")]
mod x86_64 {
    use super::*;

    const SIMPLE: u32 = 1; // kIOI2CSimpleTransactionType
    const DDC_REPLY: u32 = 2; // kIOI2CDDCciReplyTransactionType

    /// `IOI2CRequest` from IOKit/i2c/IOI2CInterface.h, LP64 layout.
    #[repr(C)]
    #[derive(Default)]
    struct Request {
        send_transaction_type: u32,
        reply_transaction_type: u32,
        send_address: u32,
        reply_address: u32,
        send_sub_address: u8,
        reply_sub_address: u8,
        reserved_a: [u8; 2],
        min_reply_delay: u64,
        result: i32,
        comm_flags: u32,
        pad_a: u32,
        send_bytes: u32,
        reserved_b: [u32; 2],
        pad_b: u32,
        reply_bytes: u32,
        completion: usize,
        send_buffer: usize,
        reply_buffer: usize,
        reserved_c: [u32; 10],
    }

    type Count = unsafe extern "C" fn(fb: IoObject, count: *mut u32) -> i32;
    type CopyIface = unsafe extern "C" fn(fb: IoObject, bus: u32, iface: *mut IoObject) -> i32;
    type Open = unsafe extern "C" fn(iface: IoObject, o: u32, c: *mut *mut c_void) -> i32;
    type Close = unsafe extern "C" fn(c: *mut c_void, o: u32) -> i32;
    type SendRequest = unsafe extern "C" fn(c: *mut c_void, o: u32, r: *mut Request) -> i32;

    struct Fb {
        name: String,
        iface: Service,
        open: Open,
        close: Close,
        send: SendRequest,
    }

    impl Fb {
        fn request(&self, mut req: Request) -> Result<(), ControlError> {
            let mut c = std::ptr::null_mut();
            check(
                unsafe { (self.open)(self.iface.0, 0, &mut c) },
                "IOI2CInterfaceOpen",
            )?;
            let r = unsafe { (self.send)(c, 0, &mut req) };
            unsafe { (self.close)(c, 0) };
            check(r, "IOI2CSendRequest")?;
            check(req.result, "I2C request")
        }
    }

    impl DdcDisplay for Fb {
        fn name(&self) -> String {
            self.name.clone()
        }
        fn write(&mut self, packet: &[u8]) -> Result<(), ControlError> {
            self.request(Request {
                send_transaction_type: SIMPLE,
                send_address: u32::from(DDC_ADDRESS) << 1,
                send_buffer: packet.as_ptr() as usize,
                send_bytes: packet.len() as u32,
                ..Request::default()
            })
        }
        fn read(&mut self, request: &[u8], reply: &mut [u8]) -> Result<(), ControlError> {
            self.write(request)?;
            std::thread::sleep(REPLY_DELAY);
            self.request(Request {
                reply_transaction_type: DDC_REPLY,
                reply_address: (u32::from(DDC_ADDRESS) << 1) | 1,
                reply_sub_address: request[0],
                reply_buffer: reply.as_mut_ptr() as usize,
                reply_bytes: reply.len() as u32,
                ..Request::default()
            })
        }
    }

    pub fn probe() -> Result<Vec<Box<dyn DdcDisplay>>, ControlError> {
        let count: Count = symbol(c"IOFBGetI2CInterfaceCount")?;
        let copy: CopyIface = symbol(c"IOFBCopyI2CInterfaceForBus")?;
        let open: Open = symbol(c"IOI2CInterfaceOpen")?;
        let close: Close = symbol(c"IOI2CInterfaceClose")?;
        let send: SendRequest = symbol(c"IOI2CSendRequest")?;
        let mut out: Vec<Box<dyn DdcDisplay>> = Vec::new();
        for fb in services(c"IOFramebuffer")? {
            let mut n = 0;
            if unsafe { count(fb.0, &mut n) } != 0 || n == 0 {
                continue;
            }
            let mut iface = 0;
            if unsafe { copy(fb.0, 0, &mut iface) } != 0 {
                continue;
            }
            let name = format!("framebuffer {}", out.len() + 1);
            let iface = Service(iface);
            out.push(Box::new(Fb {
                name,
                iface,
                open,
                close,
                send,
            }));
        }
        Ok(out)
    }
}
