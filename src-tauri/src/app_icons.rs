//! Real app icons for the key panel's app picker (`app_icons`), in place of
//! letter avatars: each app's icon as a PNG data URL.
//!
//! - Windows: the shell's own icon for the Start Menu shortcut (it follows the
//!   .lnk to its target) or the .exe, through `IShellItemImageFactory`.
//! - macOS: `NSWorkspace iconForFile:` drawn into a bitmap, or else the
//!   bundle's .icns converted with `sips`.
//!
//! Icons are cached in memory and on disk (`<cache dir>/app-icons`, keyed by
//! path, size and modified time), so the picker opens fast after the first time.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

/// The icon's size in pixels: the picker draws it at 28 CSS px, so 64 is
/// sharp on 2x displays.
pub const ICON_PX: u32 = 64;
/// The most icons one call extracts (the picker lists at most a few hundred).
const MAX_ICONS: usize = 600;

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(B64[(n >> shift) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A PNG as a data URL the webview shows in an `<img>`.
pub fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", base64(png))
}

/// Whether `bytes` start like a PNG file.
pub fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

/// Encodes `rgba` (width × height × 4 bytes, straight alpha) as a PNG.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || rgba.len() != (width * height * 4) as usize {
        return None;
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}

/// The shell's 32-bit BGRA pixels as straight-alpha RGBA. Bitmaps with no
/// alpha at all are opaque; premultiplied ones (no colour above its alpha)
/// are un-premultiplied.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let (px, _) = bgra.as_chunks::<4>();
    let no_alpha = px.iter().all(|p| p[3] == 0);
    let premultiplied = !no_alpha
        && px
            .iter()
            .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
    let mut out = Vec::with_capacity(bgra.len());
    for &[b, g, r, a] in px {
        if no_alpha {
            out.extend([r, g, b, 255]);
        } else if premultiplied && a > 0 && a < 255 {
            let un =
                |c: u8| ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
            out.extend([un(r), un(g), un(b), a]);
        } else {
            out.extend([r, g, b, a]);
        }
    }
    out
}

/// The disk cache file name for the icon of `path` with this size and
/// modified time (FNV-1a), so a changed app gets a fresh icon.
pub fn cache_key(path: &Path, len: u64, modified: u64) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let text = format!(
        "{}|{len}|{modified}|{ICON_PX}",
        path.to_string_lossy().to_lowercase()
    );
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}.png")
}

fn file_key(path: &Path) -> String {
    let meta = std::fs::metadata(path).ok();
    let len = meta.as_ref().map_or(0, |m| m.len());
    let modified = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    cache_key(path, len, modified)
}

/// Icons by path: in memory, then on disk, then `extract` (PNG bytes).
pub struct IconCache {
    dir: Option<PathBuf>,
    memory: Mutex<HashMap<PathBuf, Option<String>>>,
}

impl IconCache {
    pub fn new(dir: Option<PathBuf>) -> Self {
        IconCache {
            dir,
            memory: Mutex::new(HashMap::new()),
        }
    }

    /// The icon of `path` as a data URL, or None when it has none.
    pub fn get(&self, path: &Path, extract: &dyn Fn(&Path) -> Option<Vec<u8>>) -> Option<String> {
        let memory = |m: &Mutex<HashMap<PathBuf, Option<String>>>| {
            m.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(path)
                .cloned()
        };
        if let Some(known) = memory(&self.memory) {
            return known;
        }
        let file = self.dir.as_ref().map(|d| d.join(file_key(path)));
        let cached = file
            .as_ref()
            .and_then(|f| std::fs::read(f).ok())
            .filter(|b| is_png(b));
        let png = cached.or_else(|| {
            let png = extract(path).filter(|b| is_png(b))?;
            if let (Some(dir), Some(file)) = (&self.dir, &file) {
                let _ = std::fs::create_dir_all(dir);
                if let Err(e) = std::fs::write(file, &png) {
                    tracing::debug!(error = %e, "app icon not cached");
                }
            }
            Some(png)
        });
        let url = png.as_deref().map(data_url);
        self.memory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(path.to_path_buf(), url.clone());
        url
    }
}

/// The OS icon of an app (.lnk, .exe, .app) as PNG bytes.
pub fn extract(path: &Path) -> Option<Vec<u8>> {
    #[cfg(windows)]
    {
        let (w, h, bgra) = win::shell_icon(path, ICON_PX as i32)?;
        encode_png(w, h, &bgra_to_rgba(&bgra))
    }
    #[cfg(target_os = "macos")]
    {
        mac::workspace_icon(path, ICON_PX).or_else(|| mac::sips_icon(path, ICON_PX))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = path;
        None
    }
}

/// Runs `f` on a thread of its own, set up for the shell (COM, STA on
/// Windows), and waits for it.
fn on_icon_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    std::thread::spawn(move || {
        #[cfg(windows)]
        let _com = win::Com::init();
        f()
    })
    .join()
    .ok()
}

/// The icons of `paths` (each an app's .lnk/.exe/.app), by path; apps
/// without an icon are left out.
pub fn icons(cache: &'static IconCache, paths: Vec<String>) -> HashMap<String, String> {
    on_icon_thread(move || {
        paths
            .into_iter()
            .take(MAX_ICONS)
            .filter_map(|p| {
                let url = cache.get(Path::new(&p), &extract)?;
                Some((p, url))
            })
            .collect()
    })
    .unwrap_or_default()
}

/// The process-wide cache, on disk under the OS cache dir.
fn cache() -> &'static IconCache {
    static CACHE: std::sync::OnceLock<IconCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let dir = directories::ProjectDirs::from("", "", freewheel_core::APP_NAME)
            .map(|d| d.cache_dir().join("app-icons"));
        IconCache::new(dir)
    })
}

/// The key panel's app icons: `paths` → PNG data URL. Async and off the
/// main thread: the first time it asks the OS for each icon.
#[tauri::command]
pub async fn app_icons(paths: Vec<String>) -> HashMap<String, String> {
    let asked = paths.len();
    let found = tauri::async_runtime::spawn_blocking(move || icons(cache(), paths))
        .await
        .unwrap_or_default();
    tracing::info!(asked, found = found.len(), "app icons");
    found
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::path::Path;

    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDIBits, GetObjectW, HBITMAP,
    };
    use windows::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize, IBindCtx,
    };
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
    };
    use windows::core::HSTRING;

    /// COM for this thread (single-threaded apartment, as the shell wants).
    pub struct Com(bool);

    impl Com {
        pub fn init() -> Self {
            // SAFETY: paired with CoUninitialize in Drop on the same thread.
            Com(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok())
        }
    }

    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: CoInitializeEx succeeded on this thread.
                unsafe { CoUninitialize() };
            }
        }
    }

    /// The shell's icon of `path` (a .lnk resolves to its target's icon) as
    /// top-down BGRA pixels: width, height, bytes.
    pub fn shell_icon(path: &Path, size: i32) -> Option<(u32, u32, Vec<u8>)> {
        let name = HSTRING::from(path.as_os_str());
        // SAFETY: plain shell calls; the bitmap is deleted once read.
        unsafe {
            let factory: IShellItemImageFactory =
                SHCreateItemFromParsingName(&name, None::<&IBindCtx>).ok()?;
            let bitmap = factory
                .GetImage(
                    SIZE { cx: size, cy: size },
                    SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
                )
                .ok()?;
            let pixels = read_bitmap(bitmap);
            let _ = DeleteObject(bitmap.into());
            pixels
        }
    }

    /// The pixels of a 32-bit bitmap, top-down.
    unsafe fn read_bitmap(bitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
        let mut info = BITMAP::default();
        // SAFETY: `info` is a BITMAP of the size given.
        let got = unsafe {
            GetObjectW(
                bitmap.into(),
                size_of::<BITMAP>() as i32,
                Some((&raw mut info).cast::<c_void>()),
            )
        };
        let (w, h) = (info.bmWidth, info.bmHeight.abs());
        if got == 0 || w <= 0 || h <= 0 || w > 1024 || h > 1024 {
            return None;
        }
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        // SAFETY: `pixels` holds h rows of w 32-bit pixels, as `bmi` asks.
        let lines = unsafe {
            let dc = CreateCompatibleDC(None);
            let lines = GetDIBits(
                dc,
                bitmap,
                0,
                h as u32,
                Some(pixels.as_mut_ptr().cast()),
                &mut bmi,
                DIB_RGB_COLORS,
            );
            let _ = DeleteDC(dc);
            lines
        };
        (lines == h).then_some((w as u32, h as u32, pixels))
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::path::Path;

    use objc2::AnyThread;
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSDeviceRGBColorSpace, NSGraphicsContext,
        NSWorkspace,
    };
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

    /// The Finder's icon for `path`, drawn at `px` × `px`, as PNG bytes.
    pub fn workspace_icon(path: &Path, px: u32) -> Option<Vec<u8>> {
        objc2::rc::autoreleasepool(|_| {
            let image = NSWorkspace::sharedWorkspace()
                .iconForFile(&NSString::from_str(&path.to_string_lossy()));
            let side = f64::from(px);
            image.setSize(NSSize::new(side, side));
            // SAFETY: a fresh RGBA bitmap the rep allocates itself.
            let rep = unsafe {
                NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                    NSBitmapImageRep::alloc(),
                    std::ptr::null_mut(),
                    px as isize,
                    px as isize,
                    8,
                    4,
                    true,
                    false,
                    NSDeviceRGBColorSpace,
                    0,
                    0,
                )
            }?;
            let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
            NSGraphicsContext::saveGraphicsState_class();
            NSGraphicsContext::setCurrentContext(Some(&context));
            image.drawInRect(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(side, side)));
            context.flushGraphics();
            NSGraphicsContext::restoreGraphicsState_class();
            // SAFETY: an empty properties dictionary.
            let data = unsafe {
                rep.representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &NSDictionary::new(),
                )
            }?;
            Some(data.to_vec())
        })
    }

    /// The bundle's .icns (Info.plist CFBundleIconFile) as a `px` PNG,
    /// converted by `sips`.
    pub fn sips_icon(path: &Path, px: u32) -> Option<Vec<u8>> {
        let resources = path.join("Contents/Resources");
        let plist = path.join("Contents/Info.plist");
        let named = std::process::Command::new("/usr/bin/plutil")
            .args(["-extract", "CFBundleIconFile", "raw", "-o", "-"])
            .arg(&plist)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|n| !n.is_empty());
        let icns = super::icns_file(&resources, named.as_deref())?;
        let out = std::env::temp_dir().join(format!(
            "freewheel-icon-{}-{}.png",
            std::process::id(),
            super::cache_key(path, 0, 0)
        ));
        let ok = std::process::Command::new("/usr/bin/sips")
            .args(["-s", "format", "png", "-Z", &px.to_string()])
            .arg(&icns)
            .arg("--out")
            .arg(&out)
            .output()
            .is_ok_and(|o| o.status.success());
        let png = ok.then(|| std::fs::read(&out).ok()).flatten();
        let _ = std::fs::remove_file(&out);
        png
    }
}

/// The .icns in `resources` that Info.plist names (with or without the
/// extension), else AppIcon.icns, else the first .icns there.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn icns_file(resources: &Path, named: Option<&str>) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(name) = named {
        candidates.push(resources.join(name));
        candidates.push(resources.join(format!("{name}.icns")));
    }
    candidates.push(resources.join("AppIcon.icns"));
    if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
        return Some(found);
    }
    let mut all: Vec<PathBuf> = std::fs::read_dir(resources)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("icns"))
        })
        .collect();
    all.sort();
    all.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn base64_matches_the_rfc_vectors() {
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), coded, "{plain}");
        }
        assert_eq!(data_url(b"foo"), "data:image/png;base64,Zm9v");
    }

    #[test]
    fn rgba_pixels_become_a_png_that_decodes_back() {
        let rgba: Vec<u8> = (0..4u8 * 3 * 2).collect();
        let png_bytes = encode_png(3, 2, &rgba).unwrap();
        assert!(is_png(&png_bytes));
        let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (3, 2));
        assert_eq!(&buf[..info.buffer_size()], &rgba[..]);
        assert_eq!(encode_png(3, 3, &rgba), None);
        assert_eq!(encode_png(0, 0, &[]), None);
    }

    #[test]
    fn shell_pixels_turn_into_straight_rgba() {
        // No alpha anywhere: an opaque bitmap.
        assert_eq!(bgra_to_rgba(&[10, 20, 30, 0]), [30, 20, 10, 255]);
        // Premultiplied: half-transparent white is (128,128,128,128).
        assert_eq!(
            bgra_to_rgba(&[128, 128, 128, 128, 0, 0, 0, 0]),
            [255, 255, 255, 128, 0, 0, 0, 0]
        );
        // A colour above its alpha: already straight, kept as is.
        assert_eq!(
            bgra_to_rgba(&[200, 100, 50, 128, 1, 2, 3, 255]),
            [50, 100, 200, 128, 3, 2, 1, 255]
        );
    }

    #[test]
    fn the_cache_key_changes_with_the_app() {
        let p = Path::new("C:/ProgramData/Start Menu/Notepad.lnk");
        let k = cache_key(p, 1200, 1_700_000_000);
        assert!(k.ends_with(".png") && k.len() == 20, "{k}");
        assert_eq!(
            k,
            cache_key(
                Path::new("c:/programdata/start menu/notepad.lnk"),
                1200,
                1_700_000_000
            )
        );
        assert_ne!(k, cache_key(p, 1201, 1_700_000_000));
        assert_ne!(k, cache_key(p, 1200, 1_700_000_001));
    }

    fn tiny_png() -> Vec<u8> {
        encode_png(1, 1, &[1, 2, 3, 255]).unwrap()
    }

    #[test]
    fn icons_are_extracted_once_then_cached_in_memory_and_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Notepad.lnk");
        std::fs::write(&app, b"lnk").unwrap();
        let calls = AtomicUsize::new(0);
        let extract = |_: &Path| {
            calls.fetch_add(1, Ordering::SeqCst);
            Some(tiny_png())
        };
        let cache = IconCache::new(Some(dir.path().join("icons")));
        let url = cache.get(&app, &extract).unwrap();
        assert!(
            url.starts_with("data:image/png;base64,iVBORw0KGgo"),
            "{url}"
        );
        assert_eq!(cache.get(&app, &extract).as_deref(), Some(url.as_str()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // A new process: the disk copy, no extraction.
        let again = IconCache::new(Some(dir.path().join("icons")));
        assert_eq!(again.get(&app, &extract).as_deref(), Some(url.as_str()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            std::fs::read_dir(dir.path().join("icons")).unwrap().count(),
            1
        );
    }

    #[test]
    fn an_app_without_an_icon_or_with_garbage_has_none() {
        let cache = IconCache::new(None);
        let calls = AtomicUsize::new(0);
        let none = |_: &Path| {
            calls.fetch_add(1, Ordering::SeqCst);
            None
        };
        assert_eq!(cache.get(Path::new("/nope.app"), &none), None);
        assert_eq!(cache.get(Path::new("/nope.app"), &none), None);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "the miss is remembered");
        let garbage = |_: &Path| Some(b"not a png".to_vec());
        assert_eq!(cache.get(Path::new("/bad.app"), &garbage), None);
    }

    #[test]
    fn the_icns_is_the_one_info_plist_names() {
        let dir = tempfile::tempdir().unwrap();
        let res = dir.path();
        assert_eq!(icns_file(res, Some("Thing")), None);
        std::fs::write(res.join("Other.icns"), b"").unwrap();
        assert_eq!(icns_file(res, None), Some(res.join("Other.icns")));
        std::fs::write(res.join("AppIcon.icns"), b"").unwrap();
        assert_eq!(icns_file(res, None), Some(res.join("AppIcon.icns")));
        std::fs::write(res.join("Thing.icns"), b"").unwrap();
        assert_eq!(icns_file(res, Some("Thing")), Some(res.join("Thing.icns")));
        assert_eq!(
            icns_file(res, Some("Thing.icns")),
            Some(res.join("Thing.icns"))
        );
    }

    /// The OS's real icon for an app every runner has (CI: windows-2025,
    /// macos-15): a PNG of ICON_PX that is not blank.
    #[cfg(any(windows, target_os = "macos"))]
    fn assert_real_icon(path: &Path) {
        assert!(path.exists(), "{} is missing", path.display());
        let png_bytes = on_icon_thread({
            let path = path.to_path_buf();
            move || extract(&path)
        })
        .flatten()
        .unwrap_or_else(|| panic!("no icon for {}", path.display()));
        assert!(is_png(&png_bytes));
        let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert!(
            info.width >= 16 && info.height >= 16,
            "{}x{}",
            info.width,
            info.height
        );
        let px = &buf[..info.buffer_size()];
        let visible = px.chunks(4).filter(|p| p.len() == 4 && p[3] > 0).count();
        assert!(
            visible > (info.width * info.height / 8) as usize,
            "{} looks blank",
            path.display()
        );
        let distinct: std::collections::HashSet<&[u8]> = px.chunks(4).collect();
        assert!(distinct.len() > 8, "{} is one flat colour", path.display());
    }

    #[cfg(windows)]
    #[test]
    fn windows_gives_the_real_icon_of_an_exe_and_a_start_menu_shortcut() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        assert_real_icon(&Path::new(&windir).join("System32").join("notepad.exe"));
        // A shortcut every Windows has in the Start Menu (it resolves to its target's icon).
        let program_data = std::env::var("ProgramData").unwrap();
        let app_data = std::env::var("APPDATA").ok();
        let dirs = crate::apps::start_menu_dirs(Some(&program_data), app_data.as_deref());
        let mut lnks = Vec::new();
        for d in &dirs {
            crate::apps::find_files(d, "lnk", 4, &mut lnks);
        }
        let known = lnks
            .iter()
            .find(|p| {
                let n = p.file_stem().unwrap().to_string_lossy().to_lowercase();
                n.contains("powershell") || n.contains("notepad") || n.contains("paint")
            })
            .or(lnks.first())
            .unwrap_or_else(|| panic!("no Start Menu shortcuts in {dirs:?}"))
            .clone();
        eprintln!("shortcut: {}", known.display());
        assert_real_icon(&known);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_gives_the_real_icon_of_an_app_bundle() {
        for app in [
            "/System/Applications/Calculator.app",
            "/System/Applications/TextEdit.app",
        ] {
            assert_real_icon(Path::new(app));
        }
        let png_bytes =
            mac::workspace_icon(Path::new("/System/Applications/Calculator.app"), ICON_PX).unwrap();
        assert!(is_png(&png_bytes));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_sips_fallback_converts_a_bundles_icns() {
        let found = [
            "/System/Applications/Calculator.app",
            "/System/Applications/TextEdit.app",
            "/System/Applications/Utilities/Terminal.app",
        ]
        .iter()
        .find_map(|a| mac::sips_icon(Path::new(a), ICON_PX));
        assert!(
            found.is_some_and(|b| is_png(&b)),
            "sips made no PNG from any system app"
        );
    }
}
