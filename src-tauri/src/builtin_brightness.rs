//! The macOS built-in panel [`BrightnessControl`], through private symbols
//! loaded at runtime with `dlopen`/`dlsym` (best effort, as the
//! freewheel-m4-5 spike recorded):
//! - arm64: `DisplayServicesGetBrightness` / `DisplayServicesSetBrightness`
//!   from the private DisplayServices framework.
//! - x86_64: `CoreDisplay_Display_GetUserBrightness` /
//!   `CoreDisplay_Display_SetUserBrightness` from CoreDisplay.
//!
//! The built-in display is found with the public `CGGetOnlineDisplayList` and
//! `CGDisplayIsBuiltin`, also looked up with `dlsym`. A missing library or
//! symbol, a failed call or any other architecture gives the error
//! [`UNAVAILABLE`], which the settings window shows; the reason is logged.
//! The symbols are resolved once and cached.
//!
//! [manual] macOS 15 on arm64 and on x86_64 (release checklist
//! freewheel-m7-3): with the dial on the brightness override, turning the jog
//! wheel steps the built-in panel, or settings shows 'built-in display
//! brightness unavailable' and the log says why.

// Wired into the override worker by a later card.
#![allow(dead_code)]

use std::ffi::{CStr, c_char, c_int, c_void};

use freewheel_core::overrides::{BrightnessControl, ControlError};

/// The error shown in settings when the built-in panel can't be stepped.
pub const UNAVAILABLE: &str = "built-in display brightness unavailable";

const RTLD_LAZY: c_int = 1;

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
}

/// The last `dlerror`, or `fallback` when there is none.
fn dl_error(fallback: String) -> String {
    // SAFETY: dlerror returns null or a NUL-terminated string.
    let e = unsafe { dlerror() };
    if e.is_null() {
        fallback
    } else {
        unsafe { CStr::from_ptr(e) }.to_string_lossy().into_owned()
    }
}

/// Opens `path` (the program itself for `None`). The handle is kept for the
/// life of the process, so the symbols looked up in it stay valid.
fn open(path: Option<&CStr>) -> Result<*mut c_void, String> {
    let p = path.map_or(std::ptr::null(), CStr::as_ptr);
    // SAFETY: `p` is null or a NUL-terminated string.
    let handle = unsafe { dlopen(p, RTLD_LAZY) };
    if handle.is_null() {
        let name = path.map_or("<self>".into(), |p| p.to_string_lossy().into_owned());
        return Err(dl_error(format!("dlopen {name} failed")));
    }
    Ok(handle)
}

/// Looks up `name` in `handle`; `F` must be its C signature.
fn sym<F: Copy>(handle: *mut c_void, name: &CStr) -> Result<F, String> {
    // SAFETY: `handle` came from dlopen; `name` is NUL-terminated.
    let p = unsafe { dlsym(handle, name.as_ptr()) };
    if p.is_null() {
        return Err(format!("{} not found", name.to_string_lossy()));
    }
    // SAFETY: the caller names the symbol's own signature as F.
    Ok(unsafe { std::mem::transmute_copy(&p) })
}

type DisplayFn = Box<dyn Fn() -> Result<u32, String> + Send>;
type GetFn = Box<dyn Fn(u32) -> Result<f64, String> + Send>;
type SetFn = Box<dyn Fn(u32, f64) -> Result<(), String> + Send>;
type Resolve = Box<dyn FnMut() -> Result<Api, String> + Send>;

/// The resolved calls; brightness is 0..1.
pub struct Api {
    display: DisplayFn,
    get: GetFn,
    set: SetFn,
}

/// The built-in panel; `step` is in percent of the full range.
pub struct BuiltinBrightness {
    resolve: Resolve,
    api: Option<Result<Api, String>>,
}

impl BuiltinBrightness {
    fn new(resolve: Resolve) -> Self {
        Self { resolve, api: None }
    }

    fn step(&mut self, percent: i32) -> Result<(), ControlError> {
        let api = self.api.get_or_insert_with(&mut self.resolve);
        let stepped = match api {
            Ok(api) => step_panel(api, percent),
            Err(e) => Err(e.clone()),
        };
        stepped.map_err(|why| {
            tracing::warn!(reason = %why, "{UNAVAILABLE}");
            ControlError(UNAVAILABLE.into())
        })
    }
}

fn step_panel(api: &Api, percent: i32) -> Result<(), String> {
    let display = (api.display)()?;
    let current = (api.get)(display)?;
    let to = (current + f64::from(percent) / 100.0).clamp(0.0, 1.0);
    (api.set)(display, to)
}

impl BrightnessControl for BuiltinBrightness {
    fn step_up(&mut self, step: u8) -> Result<(), ControlError> {
        self.step(step.into())
    }
    fn step_down(&mut self, step: u8) -> Result<(), ControlError> {
        self.step(-i32::from(step))
    }
}

/// The built-in panel control of this Mac.
#[cfg(target_os = "macos")]
pub fn builtin_brightness() -> BuiltinBrightness {
    BuiltinBrightness::new(Box::new(mac::resolve))
}

#[cfg(target_os = "macos")]
mod mac {
    use super::*;

    const CORE_GRAPHICS: &CStr = c"/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics";

    type OnlineList = unsafe extern "C" fn(max: u32, ids: *mut u32, count: *mut u32) -> i32;
    type IsBuiltin = unsafe extern "C" fn(id: u32) -> u32;

    /// Finds the built-in display on every call: it goes away in clamshell.
    fn builtin_display() -> Result<DisplayFn, String> {
        let cg = open(Some(CORE_GRAPHICS))?;
        let list: OnlineList = sym(cg, c"CGGetOnlineDisplayList")?;
        let is_builtin: IsBuiltin = sym(cg, c"CGDisplayIsBuiltin")?;
        Ok(Box::new(move || {
            let mut ids = [0u32; 16];
            let mut n = 0;
            // SAFETY: `ids` holds 16 entries; `n` receives the count.
            let r = unsafe { list(16, ids.as_mut_ptr(), &mut n) };
            if r != 0 {
                return Err(format!("CGGetOnlineDisplayList returned {r}"));
            }
            ids[..n.min(16) as usize]
                .iter()
                .copied()
                .find(|&id| unsafe { is_builtin(id) } != 0)
                .ok_or_else(|| "no built-in display online".into())
        }))
    }

    #[cfg(target_arch = "aarch64")]
    pub fn resolve() -> Result<Api, String> {
        type Get = unsafe extern "C" fn(id: u32, level: *mut f32) -> i32;
        type Set = unsafe extern "C" fn(id: u32, level: f32) -> i32;
        let lib = open(Some(
            c"/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices",
        ))?;
        let get: Get = sym(lib, c"DisplayServicesGetBrightness")?;
        let set: Set = sym(lib, c"DisplayServicesSetBrightness")?;
        Ok(Api {
            display: builtin_display()?,
            get: Box::new(move |id| {
                let mut level = 0.0f32;
                match unsafe { get(id, &mut level) } {
                    0 => Ok(level.into()),
                    r => Err(format!("DisplayServicesGetBrightness returned {r}")),
                }
            }),
            set: Box::new(move |id, level| match unsafe { set(id, level as f32) } {
                0 => Ok(()),
                r => Err(format!("DisplayServicesSetBrightness returned {r}")),
            }),
        })
    }

    #[cfg(target_arch = "x86_64")]
    pub fn resolve() -> Result<Api, String> {
        type Get = unsafe extern "C" fn(id: u32) -> f64;
        type Set = unsafe extern "C" fn(id: u32, level: f64);
        let lib = open(Some(
            c"/System/Library/Frameworks/CoreDisplay.framework/CoreDisplay",
        ))?;
        let get: Get = sym(lib, c"CoreDisplay_Display_GetUserBrightness")?;
        let set: Set = sym(lib, c"CoreDisplay_Display_SetUserBrightness")?;
        Ok(Api {
            display: builtin_display()?,
            get: Box::new(move |id| Ok(unsafe { get(id) })),
            set: Box::new(move |id, level| {
                unsafe { set(id, level) };
                Ok(())
            }),
        })
    }

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    pub fn resolve() -> Result<Api, String> {
        Err(format!("no-go architecture {}", std::env::consts::ARCH))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn real_dlopen_of_a_missing_library_is_an_error() {
        let e = open(Some(c"/nonexistent/freewheel-no-such.dylib")).unwrap_err();
        assert!(e.contains("freewheel-no-such"), "{e}");
    }

    #[test]
    fn real_dlsym_of_a_missing_symbol_is_an_error_not_a_crash() {
        let me = open(None).unwrap();
        let e = sym::<*mut c_void>(me, c"freewheel_no_such_symbol").unwrap_err();
        assert_eq!(e, "freewheel_no_such_symbol not found");
    }

    #[test]
    fn real_dlsym_finds_and_calls_a_present_symbol() {
        let me = open(None).unwrap();
        let strlen: unsafe extern "C" fn(*const c_char) -> usize = sym(me, c"strlen").unwrap();
        assert_eq!(unsafe { strlen(c"four".as_ptr()) }, 4);
    }

    #[test]
    fn a_missing_library_shows_unavailable() {
        let mut b = BuiltinBrightness::new(Box::new(|| {
            open(Some(c"/nonexistent/DisplayServices")).map(|_| unreachable!())
        }));
        assert_eq!(b.step_up(10), Err(ControlError(UNAVAILABLE.into())));
    }

    fn fake(level: Arc<Mutex<f64>>, display: Result<u32, String>) -> Api {
        let l2 = Arc::clone(&level);
        Api {
            display: Box::new(move || display.clone()),
            get: Box::new(move |_| Ok(*level.lock().unwrap())),
            set: Box::new(move |_, v| {
                *l2.lock().unwrap() = v;
                Ok(())
            }),
        }
    }

    #[test]
    fn steps_and_clamps_the_panel() {
        let level = Arc::new(Mutex::new(0.5));
        let l = Arc::clone(&level);
        let mut b = BuiltinBrightness::new(Box::new(move || Ok(fake(Arc::clone(&l), Ok(1)))));
        b.step_up(10).unwrap();
        assert!((*level.lock().unwrap() - 0.6).abs() < 1e-9);
        b.step_down(20).unwrap();
        assert!((*level.lock().unwrap() - 0.4).abs() < 1e-9);
        b.step_up(100).unwrap();
        assert_eq!(*level.lock().unwrap(), 1.0);
        b.step_down(200).unwrap();
        assert_eq!(*level.lock().unwrap(), 0.0);
    }

    #[test]
    fn a_failed_call_shows_unavailable() {
        let level = Arc::new(Mutex::new(0.5));
        let mut b = BuiltinBrightness::new(Box::new(move || {
            Ok(fake(
                Arc::clone(&level),
                Err("no built-in display online".into()),
            ))
        }));
        assert_eq!(b.step_up(10), Err(ControlError(UNAVAILABLE.into())));
    }

    #[test]
    fn resolves_the_symbols_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&calls);
        let mut b = BuiltinBrightness::new(Box::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            Err("DisplayServicesSetBrightness not found".into())
        }));
        assert!(b.step_up(10).is_err());
        assert!(b.step_down(10).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
