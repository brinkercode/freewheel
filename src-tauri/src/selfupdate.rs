//! Portable self-update: freewheel is one folder (Windows: `freewheel.exe`;
//! macOS: `freewheel.app`) and updates itself in place.
//!
//! 1. Check: download the release manifest (`latest.json`, a release asset:
//!    `releases/latest/download/latest.json`, no API rate limit) and compare
//!    its version with the running one (semver).
//! 2. Download: the package for this OS and architecture
//!    (`freewheel-<ver>-windows-x64.zip`, `freewheel-<ver>-macos-universal.zip`)
//!    into a temp dir, with progress, then verify its minisign signature
//!    against the pubkey baked in at release build (`FREEWHEEL_UPDATER_PUBKEY`)
//!    and the version the signature is bound to. Nothing next to the app is
//!    touched before the signature verifies.
//! 3. Install: unpack the new `freewheel.exe` / `freewheel.app` next to the
//!    running one (`<name>.new`), rename the running one to `<name>.old`
//!    (Windows allows renaming a running exe), rename `.new` into its place,
//!    start it with [`AFTER_UPDATE_FLAG`] and exit. Any failure puts the old
//!    one back. Settings live in the user config dir and are never touched;
//!    the start-at-login entry keeps pointing at the same path.
//! 4. Next start: wait for the old process to exit, then delete `<name>.old`,
//!    a stale `<name>.new` and the temp dir ([`cleanup`]).
//!
//! `freewheel --simulate` uses a simulated source with the same states,
//! progress and signature check (a committed test package), and never
//! installs. The e2e-only `e2e` cargo feature lets the Windows update e2e
//! point the manifest at a local server (`FREEWHEEL_E2E_UPDATE_URL`);
//! shipped builds are built without it and the release workflow checks the
//! binary does not even contain that name.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Where shipped builds look for the newest release's manifest.
pub const MANIFEST_URL: &str =
    "https://github.com/brinkercode/freewheel/releases/latest/download/latest.json";

/// Update e2e builds only: the manifest URL override (a local server).
#[cfg(feature = "e2e")]
pub const E2E_MANIFEST_URL_ENV: &str = "FREEWHEEL_E2E_UPDATE_URL";

/// The flag the updated app is started with: `--after-update <old pid>`.
pub const AFTER_UPDATE_FLAG: &str = "--after-update";

/// The command-line flag for a headless update run (update-e2e.yml).
pub const HEADLESS_FLAG: &str = "--update-check-and-install";

/// The temp dir downloads go to (removed after the swap and at each start).
pub const TEMP_DIR_NAME: &str = "freewheel-update";

/// What a package holds at its root, and what is swapped.
#[cfg(windows)]
pub const PAYLOAD: &str = "freewheel.exe";
#[cfg(not(windows))]
pub const PAYLOAD: &str = "freewheel.app";

/// The updater pubkey baked in at release build (tauri signer format:
/// base64 of minisign's .pub text); `None` in dev and CI builds.
pub fn pubkey() -> Option<&'static str> {
    option_env!("FREEWHEEL_UPDATER_PUBKEY")
        .map(str::trim)
        .filter(|k| !k.is_empty())
}

/// True when this build can update itself.
pub fn configured() -> bool {
    pubkey().is_some()
}

/// The manifest URL: always [`MANIFEST_URL`] in shipped builds.
pub fn manifest_url() -> String {
    #[cfg(feature = "e2e")]
    if let Some(url) = std::env::var(E2E_MANIFEST_URL_ENV)
        .ok()
        .filter(|u| !u.trim().is_empty())
    {
        return url.trim().to_string();
    }
    MANIFEST_URL.to_string()
}

/// Only https downloads (and, with `loopback`, the local test server:
/// e2e builds and the tests only).
pub fn url_allowed(url: &str, loopback: bool) -> bool {
    url.starts_with("https://") || (loopback && url.starts_with("http://127.0.0.1:"))
}

/// Shipped builds never allow the loopback server.
const LOOPBACK: bool = cfg!(feature = "e2e");

/// Why an update step failed. `Display` starts with a code the settings
/// window reads (src/updater.js errorState).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// This build has no updater pubkey (dev and CI builds).
    NotConfigured,
    /// The manifest or the package did not download.
    Network(String),
    /// The release has no package for this OS/arch, or the manifest is bad.
    NoPackage(String),
    /// The package's signature did not verify: nothing was installed.
    BadSignature(String),
    /// The app's folder cannot be written (e.g. Program Files).
    NotWritable(PathBuf),
    /// Unpacking or swapping failed (the old version was put back).
    Install(String),
    /// Download or install called with nothing to download or install.
    Nothing,
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::NotConfigured => {
                write!(f, "not-configured: this build has no updater pubkey")
            }
            UpdateError::Network(e) => write!(f, "network: {e}"),
            UpdateError::NoPackage(e) => write!(f, "no-package: {e}"),
            UpdateError::BadSignature(e) => write!(f, "bad-signature: {e}"),
            UpdateError::NotWritable(dir) => write!(f, "not-writable: {}", dir.display()),
            UpdateError::Install(e) => write!(f, "install: {e}"),
            UpdateError::Nothing => write!(f, "nothing: no update to download or install"),
        }
    }
}

impl std::error::Error for UpdateError {}

/// A release's manifest (`latest.json`), as scripts/portable-release.mjs
/// writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub pub_date: Option<String>,
    pub platforms: BTreeMap<String, Platform>,
}

/// One OS/arch's package.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Platform {
    pub url: String,
    /// Tauri signer format: base64 of minisign's .sig text.
    pub signature: String,
}

/// The manifest key for an OS/arch (`std::env::consts` names).
pub fn platform_key(os: &str, arch: &str) -> String {
    let os = if os == "macos" { "darwin" } else { os };
    format!("{os}-{arch}")
}

/// The package for `os`/`arch`; Windows on Arm runs the x64 build.
pub fn select<'m>(
    manifest: &'m Manifest,
    os: &str,
    arch: &str,
) -> Result<&'m Platform, UpdateError> {
    let key = platform_key(os, arch);
    manifest
        .platforms
        .get(&key)
        .or_else(|| {
            (key == "windows-aarch64")
                .then(|| manifest.platforms.get("windows-x86_64"))
                .flatten()
        })
        .ok_or_else(|| {
            UpdateError::NoPackage(format!(
                "freewheel {} has no download for {key}",
                manifest.version
            ))
        })
}

/// True when `candidate` is a newer semver version than `current`.
pub fn is_newer(current: &str, candidate: &str) -> Result<bool, UpdateError> {
    let parse = |v: &str| {
        semver::Version::parse(v.trim().trim_start_matches('v'))
            .map_err(|e| UpdateError::NoPackage(format!("not a version '{v}': {e}")))
    };
    Ok(parse(candidate)? > parse(current)?)
}

fn decode_text(b64: &str, what: &str) -> Result<String, UpdateError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| UpdateError::BadSignature(format!("{what} is not base64: {e}")))?;
    String::from_utf8(bytes)
        .map_err(|e| UpdateError::BadSignature(format!("{what} is not text: {e}")))
}

/// Verifies `signature` over `data` with `pubkey` (both tauri signer
/// format). With `version`, the signature must be bound to it (`tauri signer
/// sign --app-version`, in the signed trusted comment), so a manifest can't
/// pair a new version number with an older package.
pub fn verify(
    pubkey: &str,
    data: &[u8],
    signature: &str,
    version: Option<&str>,
) -> Result<(), UpdateError> {
    let key = minisign_verify::PublicKey::decode(&decode_text(pubkey, "the pubkey")?)
        .map_err(|e| UpdateError::BadSignature(format!("the pubkey: {e}")))?;
    let sig = minisign_verify::Signature::decode(&decode_text(signature, "the signature")?)
        .map_err(|e| UpdateError::BadSignature(format!("the signature: {e}")))?;
    key.verify(data, &sig, false).map_err(|e| {
        UpdateError::BadSignature(format!("the package does not match its signature: {e}"))
    })?;
    if let Some(v) = version {
        let bound = sig
            .trusted_comment()
            .split('\t')
            .find_map(|f| f.strip_prefix("version:"));
        if bound != Some(v) {
            return Err(UpdateError::BadSignature(format!(
                "the signature is for version {}, not {v}",
                bound.unwrap_or("(none)")
            )));
        }
    }
    Ok(())
}

/// `<path>.<suffix>`, next to `path`.
pub fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    path.with_file_name(name)
}

/// Removes a file or a dir tree; a missing one is fine.
pub fn remove_path(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// What gets swapped for the running `exe`: the exe itself, or (macOS,
/// `bundle`) the `.app` it runs from.
pub fn install_target(exe: &Path, bundle: bool) -> Result<PathBuf, UpdateError> {
    if !bundle {
        return Ok(exe.to_path_buf());
    }
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            UpdateError::Install(format!("{} is not inside freewheel.app", exe.display()))
        })
}

/// The running app's install target.
pub fn current_target() -> Result<PathBuf, UpdateError> {
    let exe =
        std::env::current_exe().map_err(|e| UpdateError::Install(format!("where am I: {e}")))?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    install_target(&dunce(exe), cfg!(target_os = "macos"))
}

/// `\\?\C:\x` -> `C:\x` (canonicalize on Windows), so the relaunched path
/// and the start-at-login entry stay the plain path.
fn dunce(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest.to_string()),
        _ => path,
    }
}

/// Fails with [`UpdateError::NotWritable`] when files can't be made in `dir`.
pub fn check_writable(dir: &Path) -> Result<(), UpdateError> {
    let probe = dir.join(format!(".freewheel-write-test-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(f) => {
            drop(f);
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(_) => Err(UpdateError::NotWritable(dir.to_path_buf())),
    }
}

/// Unpacks the package's `root` entry (a file, or a dir and everything in
/// it) to `dest`. Entries outside `root` (e.g. freewheel-simulate.cmd) and
/// macOS `__MACOSX` metadata are skipped; unix modes and symlinks are kept.
pub fn extract_payload(zip: &Path, root: &str, dest: &Path) -> Result<(), UpdateError> {
    let bad = |e: String| UpdateError::Install(format!("unpacking the update: {e}"));
    let file = std::fs::File::open(zip).map_err(|e| bad(e.to_string()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| bad(e.to_string()))?;
    let mut found = false;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| bad(e.to_string()))?;
        let Some(name) = entry.enclosed_name() else {
            return Err(bad(format!("unsafe path {:?}", entry.name_raw())));
        };
        let mut parts = name.components();
        if parts.next().and_then(|c| c.as_os_str().to_str()) != Some(root) {
            continue;
        }
        let rest = parts.as_path();
        if rest.components().any(|c| c.as_os_str() == "__MACOSX") {
            continue;
        }
        let out = if rest.as_os_str().is_empty() {
            dest.to_path_buf()
        } else {
            dest.join(rest)
        };
        found = true;
        let mode = entry.unix_mode();
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| bad(format!("{}: {e}", out.display())))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| bad(format!("{}: {e}", parent.display())))?;
        }
        #[cfg(unix)]
        if mode.is_some_and(|m| m & 0o170000 == 0o120000) {
            let mut link = String::new();
            std::io::Read::read_to_string(&mut entry, &mut link).map_err(|e| bad(e.to_string()))?;
            std::os::unix::fs::symlink(&link, &out)
                .map_err(|e| bad(format!("{}: {e}", out.display())))?;
            continue;
        }
        let mut f =
            std::fs::File::create(&out).map_err(|e| bad(format!("{}: {e}", out.display())))?;
        std::io::copy(&mut entry, &mut f).map_err(|e| bad(format!("{}: {e}", out.display())))?;
        drop(f);
        #[cfg(unix)]
        if let Some(m) = mode {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&out, std::fs::Permissions::from_mode(m & 0o7777))
                .map_err(|e| bad(format!("{}: {e}", out.display())))?;
        }
        #[cfg(not(unix))]
        let _ = mode;
    }
    if found {
        Ok(())
    } else {
        Err(bad(format!("the package has no {root}")))
    }
}

/// Swaps `staged` into `target`'s place: `target` -> `<target>.old`,
/// `staged` -> `target`. If the second rename fails, the first is undone.
pub fn swap(target: &Path, staged: &Path) -> Result<(), UpdateError> {
    swap_with(target, staged, |a, b| std::fs::rename(a, b))
}

/// [`swap`] with the rename injected (the tests make it fail).
pub fn swap_with(
    target: &Path,
    staged: &Path,
    rename: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), UpdateError> {
    let old = sibling(target, "old");
    remove_path(&old).map_err(|e| {
        UpdateError::Install(format!(
            "{} from the last update is still there: {e}",
            old.display()
        ))
    })?;
    rename(target, &old).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => {
            UpdateError::NotWritable(target.parent().unwrap_or(target).to_path_buf())
        }
        _ => UpdateError::Install(format!("moving {} aside: {e}", target.display())),
    })?;
    if let Err(e) = rename(staged, target) {
        return Err(match rename(&old, target) {
            Ok(()) => UpdateError::Install(format!("putting the new version in place: {e}")),
            Err(back) => UpdateError::Install(format!(
                "putting the new version in place: {e}; putting the old one back: {back}"
            )),
        });
    }
    Ok(())
}

/// Undoes a [`swap`]: the old version back in `target`'s place.
pub fn unswap(target: &Path) -> Result<(), UpdateError> {
    let old = sibling(target, "old");
    remove_path(target)
        .map_err(|e| UpdateError::Install(format!("removing the new version: {e}")))?;
    std::fs::rename(&old, target)
        .map_err(|e| UpdateError::Install(format!("putting the old version back: {e}")))
}

/// Unpacks the verified `zip` next to `target` and swaps it in. On any
/// failure nothing changes (the staged copy is removed, the old version
/// stays or is put back).
pub fn install_package(zip: &Path, target: &Path, root: &str) -> Result<(), UpdateError> {
    let dir = target
        .parent()
        .ok_or_else(|| UpdateError::Install(format!("{} has no folder", target.display())))?;
    check_writable(dir)?;
    let staged = sibling(target, "new");
    remove_path(&staged).map_err(|e| UpdateError::Install(format!("{}: {e}", staged.display())))?;
    let result = extract_payload(zip, root, &staged).and_then(|()| swap(target, &staged));
    if result.is_err() {
        let _ = remove_path(&staged);
    }
    result
}

/// What [`cleanup`] removed.
pub fn cleanup(target: &Path, temp: &Path) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    for path in [
        sibling(target, "old"),
        sibling(target, "new"),
        temp.to_path_buf(),
    ] {
        if std::fs::symlink_metadata(&path).is_err() {
            continue;
        }
        // Windows releases the old exe's file a moment after its process exits.
        for attempt in 0..25 {
            match remove_path(&path) {
                Ok(()) => {
                    removed.push(path.clone());
                    break;
                }
                Err(e) if attempt == 24 => {
                    tracing::warn!(path = %path.display(), error = %e, "update cleanup: not removed")
                }
                Err(_) => std::thread::sleep(Duration::from_millis(200)),
            }
        }
    }
    removed
}

/// The temp dir downloads go to.
pub fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(TEMP_DIR_NAME)
}

/// Removes what the last update left (the old version, the download); run
/// once at start, after [`wait_after_update`].
pub fn cleanup_current() {
    match current_target() {
        Ok(target) => {
            for path in cleanup(&target, &temp_dir()) {
                tracing::info!(path = %path.display(), "update cleanup: removed");
            }
        }
        Err(e) => tracing::debug!(error = %e, "update cleanup: no install target"),
    }
}

/// The old process's pid when started with `--after-update <pid>`.
pub fn after_update_pid(args: impl IntoIterator<Item = String>) -> Option<u32> {
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        if a == AFTER_UPDATE_FLAG {
            return args.next()?.parse().ok();
        }
    }
    None
}

/// True when the command line asks for a headless update run.
pub fn headless_requested(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|a| a == HEADLESS_FLAG)
}

/// Started by an update: waits (at most `timeout`) for the old process to
/// exit, so the single-instance check and the cleanup don't see it.
pub fn wait_after_update(args: impl IntoIterator<Item = String>, timeout: Duration) {
    let Some(pid) = after_update_pid(args) else {
        return;
    };
    let pid = sysinfo::Pid::from_u32(pid);
    let mut sys = sysinfo::System::new();
    let end = std::time::Instant::now() + timeout;
    loop {
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        if sys.process(pid).is_none() {
            tracing::info!(%pid, "after update: the old version exited");
            return;
        }
        if std::time::Instant::now() > end {
            tracing::warn!(%pid, "after update: the old version is still running");
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Starts the swapped-in `target` with [`AFTER_UPDATE_FLAG`] and this pid.
pub fn relaunch(target: &Path) -> std::io::Result<()> {
    let pid = std::process::id().to_string();
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = std::process::Command::new("/usr/bin/open");
        c.arg("-n").arg(target).arg("--args");
        c
    };
    #[cfg(not(target_os = "macos"))]
    let mut command = std::process::Command::new(target);
    command.arg(AFTER_UPDATE_FLAG).arg(pid).spawn().map(drop)
}

/// The download's events for the settings window, in the shape
/// src/updater.js reads: `{event: "Started", data: {contentLength}}`,
/// `{event: "Progress", data: {chunkLength}}`, `{event: "Finished"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", content = "data", rename_all_fields = "camelCase")]
pub enum DownloadEvent {
    Started { content_length: Option<u64> },
    Progress { chunk_length: u64 },
    Finished,
}

/// What `update_check` returns for a newer version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    /// Release-please's notes (What's new after the restart).
    pub body: String,
}

/// What an install did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    /// The new version is in place of `target`; start it and exit.
    Swapped(PathBuf),
    /// `--simulate`: nothing changed on disk, the window shows `version`.
    Simulated(String),
}

#[derive(Debug, Clone)]
struct Offer {
    version: String,
    notes: String,
    platform: Platform,
}

#[derive(Debug, Clone, Default)]
enum Phase {
    #[default]
    Idle,
    Available(Offer),
    Ready(Offer, PathBuf),
}

/// Where updates come from.
enum Source {
    /// The release manifest; `pubkey` is baked in at release build.
    Release {
        url: String,
        pubkey: Option<String>,
        loopback: bool,
    },
    /// `--simulate`: the Simulator's "New version" publishes one, with the
    /// notes its "New version's notes" picked.
    Simulated {
        published: Mutex<Option<String>>,
        notes: Mutex<(NotesChoice, String)>,
    },
}

/// The notes a simulated update carries (the Simulator's "New version's notes").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotesChoice {
    /// The next release's real notes, embedded at build ([`SIM_NEXT_NOTES`]).
    Next,
    /// [`SIM_NOTES`].
    Sample,
    /// Whatever the user pasted.
    Paste,
}

/// What the Simulator shows about the simulated update's notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SimNotes {
    pub choice: NotesChoice,
    /// The markdown the next simulated update carries, verbatim.
    pub notes: String,
    /// Whether this build has the next release's notes.
    pub next_available: bool,
}

/// The longest pasted notes kept.
pub const SIM_PASTE_MAX: usize = 64 * 1024;

/// The simulated check takes as long as the prototype's.
pub const SIM_CHECK: Duration = Duration::from_millis(1500);
/// The simulated download takes as long as the prototype's.
pub const SIM_DOWNLOAD: Duration = Duration::from_millis(3200);
/// The simulated package's size: progress is counted against it.
pub const SIM_SIZE: u64 = 8_400_000;
const SIM_CHUNKS: u64 = 32;
/// The simulated update's signed bytes (a throwaway test key; the private
/// half was deleted after signing).
const SIM_PACKAGE: &[u8] = include_bytes!("../fixtures/update/package.bin");
const SIM_SIGNATURE: &str = include_str!("../fixtures/update/package.bin.sig");
const SIM_PUBKEY: &str = include_str!("../fixtures/update/test.pub");

/// The release notes the simulated update carries, release-please style.
pub const SIM_NOTES: &str = "## [{v}](https://github.com/brinkercode/freewheel/compare) (simulated)

### Features

* **wheel:** smoother scrolling when you turn the wheel fast
* **keys:** a shortcut can now open a folder

### Bug Fixes

* the Speed Editor reconnects by itself after your computer wakes up
";

/// The next release's user-facing notes for this build's commit
/// (`scripts/release-notes.mjs preview`: release-please's grouping of the
/// commits since the last tag, then the release's own filter), embedded by
/// build.rs from FREEWHEEL_NEXT_NOTES_FILE (the e2e and test builds); empty
/// in builds made without it.
pub const SIM_NEXT_NOTES: &str = include_str!(concat!(env!("OUT_DIR"), "/next-notes.md"));

/// Whether this build carries the next release's notes.
pub fn next_notes_available() -> bool {
    !SIM_NEXT_NOTES.trim().is_empty()
}

/// The next minor version: \"0.1.0\" -> \"0.2.0\".
pub fn next_minor(version: &str) -> String {
    match semver::Version::parse(version.trim_start_matches('v')) {
        Ok(v) => format!("{}.{}.0", v.major, v.minor + 1),
        Err(_) => "0.1.0".into(),
    }
}

/// The markdown a simulated update of `version` carries for `choice`.
fn sim_notes_text(choice: NotesChoice, pasted: &str, version: &str) -> String {
    match choice {
        NotesChoice::Next => SIM_NEXT_NOTES.to_string(),
        NotesChoice::Sample => SIM_NOTES.replace("{v}", version),
        NotesChoice::Paste => pasted.to_string(),
    }
}

async fn sleep(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

/// The update state machine the settings window drives: check, download
/// (verify), install. Managed state; one per app.
pub struct Updater {
    source: Source,
    current: Mutex<String>,
    phase: Mutex<Phase>,
    temp: PathBuf,
    target: Option<PathBuf>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Updater {
    /// Real updates for the running app at `current` version.
    pub fn release(current: &str) -> Self {
        Self::with_source(
            current,
            Source::Release {
                url: manifest_url(),
                pubkey: pubkey().map(str::to_string),
                loopback: LOOPBACK,
            },
            temp_dir(),
            current_target().ok(),
        )
    }

    /// `--simulate`'s simulated source.
    pub fn simulated(current: &str) -> Self {
        Self::with_source(
            current,
            Source::Simulated {
                published: Mutex::new(None),
                notes: Mutex::new((
                    if next_notes_available() {
                        NotesChoice::Next
                    } else {
                        NotesChoice::Sample
                    },
                    String::new(),
                )),
            },
            temp_dir(),
            None,
        )
    }

    /// A release source for the tests: any URL, pubkey, temp dir and target.
    #[cfg(test)]
    fn for_test(
        current: &str,
        url: &str,
        pubkey: Option<&str>,
        temp: PathBuf,
        target: PathBuf,
    ) -> Self {
        Self::with_source(
            current,
            Source::Release {
                url: url.into(),
                pubkey: pubkey.map(str::to_string),
                loopback: true,
            },
            temp,
            Some(target),
        )
    }

    fn with_source(current: &str, source: Source, temp: PathBuf, target: Option<PathBuf>) -> Self {
        Self {
            source,
            current: Mutex::new(current.to_string()),
            phase: Mutex::default(),
            temp,
            target,
        }
    }

    /// True for `--simulate`'s source.
    pub fn is_simulated(&self) -> bool {
        matches!(self.source, Source::Simulated { .. })
    }

    /// The running version (in `--simulate`, the last one "installed").
    pub fn current(&self) -> String {
        lock(&self.current).clone()
    }

    /// `--simulate`: publishes a version newer than the running one (and
    /// than the last one published); returns it. `None` for real updates.
    pub fn sim_publish(&self) -> Option<String> {
        let Source::Simulated { published, .. } = &self.source else {
            return None;
        };
        let current = self.current();
        let mut published = lock(published);
        let base = match published.as_deref() {
            Some(p) if is_newer(&current, p).unwrap_or(false) => p.to_string(),
            _ => current,
        };
        let next = next_minor(&base);
        *published = Some(next.clone());
        Some(next)
    }

    /// `--simulate`: picks the notes the next simulated update carries
    /// (`None` keeps the choice; `text` is kept for [`NotesChoice::Paste`])
    /// and says what they are. `Err` for real updates, or for
    /// [`NotesChoice::Next`] in a build without them.
    pub fn sim_notes(
        &self,
        choice: Option<NotesChoice>,
        text: Option<String>,
    ) -> Result<SimNotes, String> {
        let Source::Simulated { notes, .. } = &self.source else {
            return Err("not simulating".into());
        };
        let mut notes = lock(notes);
        if choice == Some(NotesChoice::Next) && !next_notes_available() {
            return Err("this build has no next-release notes (FREEWHEEL_NEXT_NOTES_FILE)".into());
        }
        if let Some(text) = text {
            if text.len() > SIM_PASTE_MAX {
                return Err(format!("pasted notes are over {SIM_PASTE_MAX} bytes"));
            }
            notes.1 = text;
        }
        if let Some(c) = choice {
            notes.0 = c;
        }
        let version = next_minor(&self.current());
        Ok(SimNotes {
            choice: notes.0,
            notes: sim_notes_text(notes.0, &notes.1, &version),
            next_available: next_notes_available(),
        })
    }

    /// Checks for a newer version; remembers it for [`Self::download`].
    pub async fn check(&self) -> Result<Option<UpdateInfo>, UpdateError> {
        let current = self.current();
        let offer = match &self.source {
            Source::Release {
                url,
                pubkey,
                loopback,
            } => {
                if pubkey.is_none() {
                    return Err(UpdateError::NotConfigured);
                }
                match fetch_manifest(url, *loopback).await? {
                    Some(manifest) if is_newer(&current, &manifest.version)? => {
                        let platform =
                            select(&manifest, std::env::consts::OS, std::env::consts::ARCH)?;
                        if !url_allowed(&platform.url, *loopback) {
                            return Err(UpdateError::NoPackage(format!(
                                "not an https download: {}",
                                platform.url
                            )));
                        }
                        Some(Offer {
                            version: manifest.version.clone(),
                            notes: manifest.notes.clone(),
                            platform: platform.clone(),
                        })
                    }
                    _ => None,
                }
            }
            Source::Simulated { published, notes } => {
                sleep(SIM_CHECK).await;
                let published = lock(published).clone();
                let (choice, pasted) = lock(notes).clone();
                published
                    .filter(|p| is_newer(&current, p).unwrap_or(false))
                    .map(|version| Offer {
                        notes: sim_notes_text(choice, &pasted, &version),
                        version,
                        platform: Platform {
                            url: "simulated".into(),
                            signature: SIM_SIGNATURE.into(),
                        },
                    })
            }
        };
        let info = offer.as_ref().map(|o| UpdateInfo {
            version: o.version.clone(),
            current_version: current,
            body: o.notes.clone(),
        });
        *lock(&self.phase) = offer.map_or(Phase::Idle, Phase::Available);
        Ok(info)
    }

    /// Downloads the offered package to the temp dir and verifies it; then
    /// it is ready to install.
    pub async fn download(
        &self,
        mut on: impl FnMut(DownloadEvent) + Send,
    ) -> Result<(), UpdateError> {
        let offer = match lock(&self.phase).clone() {
            Phase::Available(o) | Phase::Ready(o, _) => o,
            Phase::Idle => return Err(UpdateError::Nothing),
        };
        match &self.source {
            Source::Release { pubkey, .. } => {
                let pubkey = pubkey.clone().ok_or(UpdateError::NotConfigured)?;
                let target = self
                    .target
                    .clone()
                    .ok_or_else(|| UpdateError::Install("no install target".into()))?;
                // Before any download: an update that can't be put in place is no use.
                check_writable(target.parent().unwrap_or(&target))?;
                let zip = self.temp.join(format!("freewheel-{}.zip", offer.version));
                let result = async {
                    std::fs::create_dir_all(&self.temp).map_err(|e| {
                        UpdateError::Install(format!("{}: {e}", self.temp.display()))
                    })?;
                    let data = fetch_package(&offer.platform.url, &zip, &mut on).await?;
                    verify(
                        &pubkey,
                        &data,
                        &offer.platform.signature,
                        Some(&offer.version),
                    )
                }
                .await;
                if let Err(e) = result {
                    let _ = remove_path(&self.temp);
                    return Err(e);
                }
                *lock(&self.phase) = Phase::Ready(offer, zip);
            }
            Source::Simulated { .. } => {
                on(DownloadEvent::Started {
                    content_length: Some(SIM_SIZE),
                });
                for _ in 0..SIM_CHUNKS {
                    sleep(SIM_DOWNLOAD / SIM_CHUNKS as u32).await;
                    on(DownloadEvent::Progress {
                        chunk_length: SIM_SIZE / SIM_CHUNKS,
                    });
                }
                on(DownloadEvent::Finished);
                verify(SIM_PUBKEY, SIM_PACKAGE, &offer.platform.signature, None)?;
                *lock(&self.phase) = Phase::Ready(offer, PathBuf::new());
            }
        }
        Ok(())
    }

    /// Installs the downloaded, verified package (see the module docs);
    /// the temp dir is removed. In `--simulate` only the version changes.
    pub fn install(&self) -> Result<Installed, UpdateError> {
        let Phase::Ready(offer, zip) = lock(&self.phase).clone() else {
            return Err(UpdateError::Nothing);
        };
        if self.is_simulated() {
            *lock(&self.current) = offer.version.clone();
            *lock(&self.phase) = Phase::Idle;
            return Ok(Installed::Simulated(offer.version));
        }
        let target = self
            .target
            .clone()
            .ok_or_else(|| UpdateError::Install("no install target".into()))?;
        install_package(&zip, &target, PAYLOAD)?;
        let _ = remove_path(&self.temp);
        *lock(&self.phase) = Phase::Idle;
        Ok(Installed::Swapped(target))
    }
}

fn client() -> Result<reqwest::Client, UpdateError> {
    reqwest::Client::builder()
        .user_agent(concat!("freewheel/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| UpdateError::Network(e.to_string()))
}

/// The manifest, or `None` when there is no release (204 or 404).
async fn fetch_manifest(url: &str, loopback: bool) -> Result<Option<Manifest>, UpdateError> {
    if !url_allowed(url, loopback) {
        return Err(UpdateError::Network(format!("not an https URL: {url}")));
    }
    let res = client()?
        .get(url)
        .send()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let status = res.status();
    if status == reqwest::StatusCode::NO_CONTENT || status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(UpdateError::Network(format!("{url}: HTTP {status}")));
    }
    let text = res
        .text()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| UpdateError::NoPackage(format!("the manifest is not valid: {e}")))
}

/// Streams `url` into `dest` with progress; returns the bytes.
async fn fetch_package(
    url: &str,
    dest: &Path,
    on: &mut (impl FnMut(DownloadEvent) + Send),
) -> Result<Vec<u8>, UpdateError> {
    let net = |e: reqwest::Error| UpdateError::Network(e.to_string());
    let mut res = client()?.get(url).send().await.map_err(net)?;
    if !res.status().is_success() {
        return Err(UpdateError::Network(format!(
            "{url}: HTTP {}",
            res.status()
        )));
    }
    on(DownloadEvent::Started {
        content_length: res.content_length(),
    });
    let mut file = std::fs::File::create(dest)
        .map_err(|e| UpdateError::Install(format!("{}: {e}", dest.display())))?;
    let mut data = Vec::with_capacity(res.content_length().unwrap_or(0) as usize);
    while let Some(chunk) = res.chunk().await.map_err(net)? {
        file.write_all(&chunk)
            .map_err(|e| UpdateError::Install(format!("{}: {e}", dest.display())))?;
        data.extend_from_slice(&chunk);
        on(DownloadEvent::Progress {
            chunk_length: chunk.len() as u64,
        });
    }
    on(DownloadEvent::Finished);
    Ok(data)
}

/// The headless update run (`freewheel --update-check-and-install`,
/// update-e2e.yml): check, download, verify, swap; no window, no relaunch.
/// Exit code: 0 installed, 2 up to date, 3 not configured, 1 failed.
pub fn run_headless(current: &str) -> i32 {
    cleanup_current();
    let updater = Updater::release(current);
    let outcome = tauri::async_runtime::block_on(async {
        match updater.check().await? {
            None => Ok(false),
            Some(info) => {
                tracing::info!(from = %info.current_version, to = %info.version, "update run: installing");
                updater.download(|_| {}).await?;
                updater.install().map(|_| true)
            }
        }
    });
    match outcome {
        Ok(true) => 0,
        Ok(false) => {
            tracing::info!("update run: up to date");
            2
        }
        Err(UpdateError::NotConfigured) => {
            tracing::error!("update run: this build has no updater pubkey");
            3
        }
        Err(e) => {
            tracing::error!(error = %e, "update run failed");
            1
        }
    }
}

/// Checks for a newer version (the settings window's Check for updates).
#[tauri::command]
pub async fn update_check(
    updater: tauri::State<'_, Updater>,
) -> Result<Option<UpdateInfo>, String> {
    updater.check().await.map_err(|e| {
        tracing::warn!(error = %e, "update check failed");
        e.to_string()
    })
}

/// Downloads and verifies the offered version, with progress on `on_event`.
#[tauri::command]
pub async fn update_download(
    updater: tauri::State<'_, Updater>,
    on_event: tauri::ipc::Channel<DownloadEvent>,
) -> Result<(), String> {
    updater
        .download(move |event| {
            let _ = on_event.send(event);
        })
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "update download failed");
            e.to_string()
        })
}

/// Installs the downloaded version and restarts into it: `"restarting"`
/// (this process exits right after answering), or in `--simulate`
/// `"simulated"` (nothing changed on disk).
#[tauri::command]
pub async fn update_install(
    app: tauri::AppHandle,
    updater: tauri::State<'_, Updater>,
) -> Result<String, String> {
    let installed = updater.install().map_err(|e| {
        tracing::warn!(error = %e, "update install failed");
        e.to_string()
    })?;
    match installed {
        Installed::Simulated(_) => Ok("simulated".into()),
        Installed::Swapped(target) => {
            if let Err(e) = relaunch(&target) {
                let undo = unswap(&target);
                tracing::error!(error = %e, ?undo, "update: the new version did not start; the old one is back");
                return Err(
                    UpdateError::Install(format!("starting the new version: {e}")).to_string(),
                );
            }
            tracing::info!(target = %target.display(), "update: installed, restarting");
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                app.exit(0);
            });
            Ok("restarting".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    const TEST_PUBKEY: &str = include_str!("../fixtures/update/test.pub");
    const OTHER_PUBKEY: &str = include_str!("../fixtures/update/other.pub");

    fn manifest(platforms: &[&str]) -> Manifest {
        Manifest {
            version: "0.2.0".into(),
            notes: "notes".into(),
            pub_date: None,
            platforms: platforms
                .iter()
                .map(|k| {
                    (
                        k.to_string(),
                        Platform {
                            url: format!("https://x/{k}.zip"),
                            signature: "s".into(),
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn versions_compare_as_semver() {
        assert!(is_newer("0.1.0", "0.1.1").unwrap());
        assert!(is_newer("0.1.9", "0.1.10").unwrap());
        assert!(is_newer("0.9.0", "0.10.0").unwrap());
        assert!(is_newer("v0.1.0", "v1.0.0").unwrap());
        assert!(is_newer("1.0.0-beta.1", "1.0.0").unwrap());
        assert!(!is_newer("0.2.0", "0.2.0").unwrap());
        assert!(!is_newer("0.2.0", "0.1.9").unwrap());
        assert!(!is_newer("1.0.0", "1.0.0-rc.1").unwrap());
        assert!(is_newer("0.1.0", "nope").is_err());
    }

    #[test]
    fn the_package_is_picked_by_os_and_arch() {
        let m = manifest(&["windows-x86_64", "darwin-aarch64", "darwin-x86_64"]);
        assert_eq!(
            select(&m, "windows", "x86_64").unwrap().url,
            "https://x/windows-x86_64.zip"
        );
        assert_eq!(
            select(&m, "macos", "aarch64").unwrap().url,
            "https://x/darwin-aarch64.zip"
        );
        assert_eq!(
            select(&m, "macos", "x86_64").unwrap().url,
            "https://x/darwin-x86_64.zip"
        );
        // Windows on Arm runs the x64 build.
        assert_eq!(
            select(&m, "windows", "aarch64").unwrap().url,
            "https://x/windows-x86_64.zip"
        );
        let e = select(&m, "linux", "x86_64").unwrap_err();
        assert!(e.to_string().starts_with("no-package:"), "{e}");
        assert!(select(&manifest(&["windows-x86_64"]), "macos", "aarch64").is_err());
    }

    #[test]
    fn the_manifest_parses_as_the_release_writes_it() {
        let json = include_str!("../../scripts/fixtures/portable-latest.json");
        let m: Manifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.version, "0.1.1");
        assert!(m.notes.contains("Features"));
        for key in ["windows-x86_64", "darwin-aarch64", "darwin-x86_64"] {
            assert!(m.platforms[key].url.starts_with(
                "https://github.com/brinkercode/freewheel/releases/download/v0.1.1/freewheel-0.1.1-"
            ));
        }
    }

    #[test]
    fn a_good_signature_verifies() {
        let data = include_bytes!("../fixtures/update/package.bin");
        let sig = include_str!("../fixtures/update/package.bin.sig");
        verify(TEST_PUBKEY, data, sig, None).unwrap();
        verify(TEST_PUBKEY, data, sig, Some("0.2.0")).unwrap();
    }

    #[test]
    fn a_tampered_package_or_another_key_or_version_is_rejected() {
        let data = include_bytes!("../fixtures/update/package.bin");
        let sig = include_str!("../fixtures/update/package.bin.sig");
        let mut tampered = data.to_vec();
        tampered[3] ^= 1;
        for (what, result) in [
            ("tampered", verify(TEST_PUBKEY, &tampered, sig, None)),
            ("truncated", verify(TEST_PUBKEY, &data[..10], sig, None)),
            ("another key", verify(OTHER_PUBKEY, data, sig, None)),
            (
                "another version",
                verify(TEST_PUBKEY, data, sig, Some("0.3.0")),
            ),
            ("no signature", verify(TEST_PUBKEY, data, "", None)),
            (
                "garbage",
                verify(TEST_PUBKEY, data, "bm90IGEgc2lnbmF0dXJl", None),
            ),
        ] {
            let e = result.expect_err(what);
            assert!(e.to_string().starts_with("bad-signature:"), "{what}: {e}");
        }
    }

    fn write_zip(path: &Path, entries: &[(&str, Option<&[u8]>, u32)]) {
        let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, data, mode) in entries {
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(*mode);
            match data {
                Some(d) => {
                    z.start_file(*name, opts).unwrap();
                    z.write_all(d).unwrap();
                }
                None => z.add_directory(*name, opts).unwrap(),
            }
        }
        z.finish().unwrap();
    }

    fn read(path: &Path) -> String {
        let mut s = String::new();
        std::fs::File::open(path)
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        s
    }

    #[test]
    fn a_windows_package_swaps_the_exe_and_keeps_the_old_one_aside() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("portable");
        std::fs::create_dir(&app).unwrap();
        let exe = app.join("freewheel.exe");
        std::fs::write(&exe, "A").unwrap();
        let zip = dir.path().join("b.zip");
        write_zip(
            &zip,
            &[
                ("freewheel.exe", Some(b"B"), 0o755),
                ("freewheel-simulate.cmd", Some(b"cmd"), 0o644),
            ],
        );
        install_package(&zip, &exe, "freewheel.exe").unwrap();
        assert_eq!(read(&exe), "B");
        assert_eq!(read(&sibling(&exe, "old")), "A");
        assert!(!sibling(&exe, "new").exists());
        // Only the exe: nothing else in the folder changes.
        assert!(!app.join("freewheel-simulate.cmd").exists());
        // The next start removes the old version and the download.
        let temp = dir.path().join(TEMP_DIR_NAME);
        std::fs::create_dir(&temp).unwrap();
        std::fs::write(temp.join("freewheel-0.2.0.zip"), "zip").unwrap();
        let removed = cleanup(&exe, &temp);
        assert_eq!(removed, vec![sibling(&exe, "old"), temp.clone()]);
        assert!(!sibling(&exe, "old").exists() && !temp.exists());
        assert_eq!(read(&exe), "B");
        assert_eq!(std::fs::read_dir(&app).unwrap().count(), 1);
        // Nothing left: cleanup is a no-op.
        assert!(cleanup(&exe, &temp).is_empty());
    }

    #[test]
    fn a_macos_package_swaps_the_whole_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("freewheel.app");
        std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        std::fs::write(bundle.join("Contents/MacOS/freewheel"), "A").unwrap();
        std::fs::write(bundle.join("Contents/OnlyInA"), "a").unwrap();
        let zip = dir.path().join("b.zip");
        write_zip(
            &zip,
            &[
                ("freewheel.app/", None, 0o755),
                ("freewheel.app/Contents/", None, 0o755),
                ("freewheel.app/Contents/MacOS/", None, 0o755),
                ("freewheel.app/Contents/MacOS/freewheel", Some(b"B"), 0o755),
                ("freewheel.app/Contents/Info.plist", Some(b"plist"), 0o644),
                ("__MACOSX/freewheel.app/._Info.plist", Some(b"junk"), 0o644),
            ],
        );
        install_package(&zip, &bundle, "freewheel.app").unwrap();
        assert_eq!(read(&bundle.join("Contents/MacOS/freewheel")), "B");
        assert_eq!(read(&bundle.join("Contents/Info.plist")), "plist");
        assert!(!bundle.join("Contents/OnlyInA").exists());
        assert_eq!(
            read(&sibling(&bundle, "old").join("Contents/MacOS/freewheel")),
            "A"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(bundle.join("Contents/MacOS/freewheel"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        cleanup(&bundle, &dir.path().join(TEMP_DIR_NAME));
        assert!(!sibling(&bundle, "old").exists());
    }

    #[test]
    fn a_failed_swap_puts_the_old_version_back() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("freewheel.exe");
        let staged = sibling(&exe, "new");
        std::fs::write(&exe, "A").unwrap();
        std::fs::write(&staged, "B").unwrap();
        let e = swap_with(&exe, &staged, |from, to| {
            if from == staged {
                Err(std::io::Error::other("disk full"))
            } else {
                std::fs::rename(from, to)
            }
        })
        .unwrap_err();
        assert!(e.to_string().contains("disk full"), "{e}");
        assert_eq!(read(&exe), "A");
        assert!(!sibling(&exe, "old").exists());
    }

    #[test]
    fn a_broken_package_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("freewheel.exe");
        std::fs::write(&exe, "A").unwrap();
        let not_zip = dir.path().join("x.zip");
        std::fs::write(&not_zip, "not a zip").unwrap();
        let wrong = dir.path().join("wrong.zip");
        write_zip(&wrong, &[("something-else.exe", Some(b"B"), 0o755)]);
        for zip in [&not_zip, &wrong] {
            let e = install_package(zip, &exe, "freewheel.exe").unwrap_err();
            assert!(e.to_string().starts_with("install:"), "{e}");
            assert_eq!(read(&exe), "A");
            assert_eq!(
                std::fs::read_dir(dir.path()).unwrap().count(),
                3,
                "only exe and the two zips"
            );
        }
    }

    #[test]
    fn unsafe_paths_in_a_package_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("app").join("freewheel.exe");
        std::fs::create_dir(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "A").unwrap();
        let zip = dir.path().join("evil.zip");
        write_zip(&zip, &[("../evil.exe", Some(b"x"), 0o755)]);
        assert!(install_package(&zip, &exe, "freewheel.exe").is_err());
        assert!(!dir.path().join("evil.exe").exists());
        assert_eq!(read(&exe), "A");
    }

    #[test]
    fn a_leftover_new_copy_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("freewheel.exe");
        std::fs::write(&exe, "A").unwrap();
        std::fs::write(sibling(&exe, "new"), "stale").unwrap();
        std::fs::write(sibling(&exe, "old"), "older").unwrap();
        let zip = dir.path().join("b.zip");
        write_zip(&zip, &[("freewheel.exe", Some(b"B"), 0o755)]);
        install_package(&zip, &exe, "freewheel.exe").unwrap();
        assert_eq!(read(&exe), "B");
        assert_eq!(read(&sibling(&exe, "old")), "A");
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_that_cannot_be_written_says_so_and_changes_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Program Files");
        std::fs::create_dir(&app).unwrap();
        let exe = app.join("freewheel.exe");
        std::fs::write(&exe, "A").unwrap();
        let zip = dir.path().join("b.zip");
        write_zip(&zip, &[("freewheel.exe", Some(b"B"), 0o755)]);
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = install_package(&zip, &exe, "freewheel.exe");
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
        if std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(app.join("root"))
            .is_ok()
        {
            return; // running as root: permissions are not enforced
        }
        assert_eq!(result.unwrap_err(), UpdateError::NotWritable(app.clone()));
        assert_eq!(read(&exe), "A");
        assert_eq!(
            UpdateError::NotWritable(app.clone()).to_string(),
            format!("not-writable: {}", app.display())
        );
    }

    #[test]
    fn the_bundle_is_the_install_target_on_macos() {
        let exe = Path::new("/Applications/freewheel.app/Contents/MacOS/freewheel");
        assert_eq!(
            install_target(exe, true).unwrap(),
            Path::new("/Applications/freewheel.app")
        );
        assert_eq!(install_target(exe, false).unwrap(), exe);
        assert!(install_target(Path::new("/usr/local/bin/freewheel"), true).is_err());
        assert_eq!(
            dunce(PathBuf::from(r"\\?\C:\fw\freewheel.exe")),
            PathBuf::from(r"C:\fw\freewheel.exe")
        );
    }

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_flags_parse() {
        assert_eq!(
            after_update_pid(args(&["fw", AFTER_UPDATE_FLAG, "1234"])),
            Some(1234)
        );
        assert_eq!(after_update_pid(args(&["fw", AFTER_UPDATE_FLAG])), None);
        assert_eq!(after_update_pid(args(&["fw", "--simulate"])), None);
        assert!(headless_requested(args(&["fw", HEADLESS_FLAG])));
        assert!(!headless_requested(args(&["fw", "--simulate", "--smoke"])));
        // Not started by an update: no wait.
        let t = std::time::Instant::now();
        wait_after_update(args(&["fw"]), Duration::from_secs(5));
        assert!(t.elapsed() < Duration::from_secs(1));
        // An exited pid: no wait either.
        wait_after_update(
            args(&["fw", AFTER_UPDATE_FLAG, "4000000000"]),
            Duration::from_secs(5),
        );
        assert!(t.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn progress_events_have_the_shape_the_window_reads() {
        let json = |e| serde_json::to_value(e).unwrap();
        assert_eq!(
            json(DownloadEvent::Started {
                content_length: Some(9)
            }),
            serde_json::json!({"event": "Started", "data": {"contentLength": 9}})
        );
        assert_eq!(
            json(DownloadEvent::Progress { chunk_length: 3 }),
            serde_json::json!({"event": "Progress", "data": {"chunkLength": 3}})
        );
        assert_eq!(
            json(DownloadEvent::Finished),
            serde_json::json!({"event": "Finished"})
        );
    }

    #[test]
    fn shipped_builds_only_ask_github_over_https() {
        // No e2e feature: no override, whatever the environment says.
        #[cfg(not(feature = "e2e"))]
        {
            const NAME: &str = "FREEWHEEL_E2E_UPDATE_URL";
            // SAFETY: tests touching this variable run in this test only.
            unsafe { std::env::set_var(NAME, "http://127.0.0.1:1/latest.json") };
            assert_eq!(manifest_url(), MANIFEST_URL);
            const { assert!(!LOOPBACK) };
            let u = Updater::release("0.1.0");
            assert!(
                matches!(&u.source, Source::Release { url, loopback: false, .. } if url == MANIFEST_URL)
            );
            unsafe { std::env::remove_var(NAME) };
        }
        assert!(
            MANIFEST_URL
                .starts_with("https://github.com/brinkercode/freewheel/releases/latest/download/")
        );
        assert!(url_allowed("https://github.com/x", false));
        assert!(!url_allowed("http://127.0.0.1:1/latest.json", false));
        assert!(url_allowed("http://127.0.0.1:1/latest.json", true));
        assert!(!url_allowed("http://github.com/x", true));
        assert!(!url_allowed("file:///C:/evil.zip", true));
    }

    #[test]
    fn a_build_without_a_pubkey_does_not_update() {
        let dir = tempfile::tempdir().unwrap();
        let u = Updater::for_test(
            "0.1.0",
            MANIFEST_URL,
            None,
            dir.path().join("t"),
            dir.path().join("freewheel.exe"),
        );
        let e = tauri::async_runtime::block_on(u.check()).unwrap_err();
        assert_eq!(e, UpdateError::NotConfigured);
        assert!(e.to_string().starts_with("not-configured:"));
        assert_eq!(u.install().unwrap_err(), UpdateError::Nothing);
    }

    #[test]
    fn the_simulated_update_carries_the_notes_the_simulator_picked() {
        let u = Updater::simulated("0.1.0");
        let start = u.sim_notes(None, None).unwrap();
        assert_eq!(start.next_available, next_notes_available());
        assert_eq!(
            start.choice,
            if next_notes_available() {
                NotesChoice::Next
            } else {
                NotesChoice::Sample
            }
        );
        let check = |u: &Updater| {
            u.sim_publish().unwrap();
            tauri::async_runtime::block_on(u.check())
                .unwrap()
                .unwrap()
                .body
        };
        let sample = u.sim_notes(Some(NotesChoice::Sample), None).unwrap();
        assert_eq!(sample.notes, SIM_NOTES.replace("{v}", "0.2.0"));
        assert_eq!(check(&u), sample.notes);
        let pasted = "### Features\n\n* **keys:** pasted\n";
        let p = u
            .sim_notes(Some(NotesChoice::Paste), Some(pasted.into()))
            .unwrap();
        assert_eq!((p.choice, p.notes.as_str()), (NotesChoice::Paste, pasted));
        assert_eq!(check(&u), pasted);
        // The choice stays until changed; the paste is kept.
        assert_eq!(u.sim_notes(None, None).unwrap().notes, pasted);
        assert!(
            u.sim_notes(
                Some(NotesChoice::Paste),
                Some("x".repeat(SIM_PASTE_MAX + 1))
            )
            .is_err()
        );
        match u.sim_notes(Some(NotesChoice::Next), None) {
            Ok(n) => assert_eq!(n.notes, SIM_NEXT_NOTES),
            Err(e) => assert!(!next_notes_available(), "{e}"),
        }
        // Real updates have no simulated notes.
        let dir = tempfile::tempdir().unwrap();
        let real = Updater::for_test(
            "0.1.0",
            "http://127.0.0.1:1/latest.json",
            None,
            dir.path().into(),
            dir.path().join("x"),
        );
        assert!(real.sim_notes(None, None).is_err());
    }

    #[test]
    fn the_simulated_source_goes_through_check_download_verify_install() {
        let u = Updater::simulated("0.1.0");
        assert!(u.is_simulated());
        // Nothing published: up to date.
        assert_eq!(tauri::async_runtime::block_on(u.check()).unwrap(), None);
        assert_eq!(u.sim_publish().as_deref(), Some("0.2.0"));
        assert_eq!(u.sim_publish().as_deref(), Some("0.3.0"));
        let info = tauri::async_runtime::block_on(u.check()).unwrap().unwrap();
        assert_eq!(
            (info.version.as_str(), info.current_version.as_str()),
            ("0.3.0", "0.1.0")
        );
        assert!(info.body.contains("## [0.3.0]"));
        let mut events = Vec::new();
        tauri::async_runtime::block_on(u.download(|e| events.push(e))).unwrap();
        assert_eq!(
            events.first(),
            Some(&DownloadEvent::Started {
                content_length: Some(SIM_SIZE)
            })
        );
        assert_eq!(events.last(), Some(&DownloadEvent::Finished));
        assert_eq!(events.len() as u64, SIM_CHUNKS + 2);
        assert_eq!(u.install().unwrap(), Installed::Simulated("0.3.0".into()));
        assert_eq!(u.current(), "0.3.0");
        // Installed: up to date again; the next one is newer than that.
        assert_eq!(tauri::async_runtime::block_on(u.check()).unwrap(), None);
        assert_eq!(u.sim_publish().as_deref(), Some("0.4.0"));
        // Real updates never publish.
        assert_eq!(Updater::release("0.1.0").sim_publish(), None);
    }

    /// A one-thread HTTP server for the tests: `routes` maps a path to
    /// (status, body); every request is answered once per connection.
    fn serve(routes: Vec<(&'static str, u16, Vec<u8>)>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (status, body) = routes
                    .iter()
                    .find(|(p, _, _)| *p == path)
                    .map(|(_, s, b)| (*s, b.clone()))
                    .unwrap_or((404, Vec::new()));
                let head = format!(
                    "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        base
    }

    fn served_manifest(base: &str, package: &str) -> Vec<u8> {
        let key = platform_key(std::env::consts::OS, std::env::consts::ARCH);
        let sig = include_str!("../fixtures/update/package.bin.sig").trim();
        serde_json::to_vec(&serde_json::json!({
            "version": "0.2.0",
            "notes": "## 0.2.0\n\n* new",
            "platforms": { key: { "url": format!("{base}{package}"), "signature": sig } }
        }))
        .unwrap()
    }

    #[test]
    fn check_download_and_verify_over_http_then_a_bad_package_is_not_installed() {
        let package = include_bytes!("../fixtures/update/package.bin").to_vec();
        let mut tampered = package.clone();
        tampered[0] ^= 1;
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("freewheel.exe");
        std::fs::write(&exe, "A").unwrap();
        let temp = dir.path().join(TEMP_DIR_NAME);
        let run = |manifest: &str, routes: Vec<(&'static str, u16, Vec<u8>)>| {
            let base = serve(routes);
            Updater::for_test(
                "0.1.0",
                &format!("{base}{manifest}"),
                Some(TEST_PUBKEY),
                temp.clone(),
                exe.clone(),
            )
        };
        // 204: up to date; 500: a check error.
        let u = run("/none.json", vec![("/none.json", 204, Vec::new())]);
        assert_eq!(tauri::async_runtime::block_on(u.check()).unwrap(), None);
        let u = run("/fail.json", vec![("/fail.json", 500, b"no".to_vec())]);
        let e = tauri::async_runtime::block_on(u.check()).unwrap_err();
        assert!(e.to_string().starts_with("network:"), "{e}");
        // A tampered package: rejected, nothing downloaded is kept, A intact.
        let routes = |pkg: Vec<u8>| served_manifest(&serve(vec![("/pkg", 200, pkg)]), "/pkg");
        let manifest = routes(tampered);
        let u = run("/latest.json", vec![("/latest.json", 200, manifest)]);
        let info = tauri::async_runtime::block_on(u.check()).unwrap().unwrap();
        assert_eq!(
            (info.version.as_str(), info.body.as_str()),
            ("0.2.0", "## 0.2.0\n\n* new")
        );
        let mut events = Vec::new();
        let e = tauri::async_runtime::block_on(u.download(|ev| events.push(ev))).unwrap_err();
        assert!(e.to_string().starts_with("bad-signature:"), "{e}");
        assert!(!temp.exists(), "the rejected download is gone");
        assert_eq!(u.install().unwrap_err(), UpdateError::Nothing);
        assert_eq!(read(&exe), "A");
        // The signed package: downloads with progress and verifies; it is
        // not a zip, so the install fails and A stays.
        let manifest = routes(package.clone());
        let u = run("/latest.json", vec![("/latest.json", 200, manifest)]);
        tauri::async_runtime::block_on(u.check()).unwrap().unwrap();
        let mut events = Vec::new();
        tauri::async_runtime::block_on(u.download(|ev| events.push(ev))).unwrap();
        assert_eq!(
            events.first(),
            Some(&DownloadEvent::Started {
                content_length: Some(package.len() as u64)
            })
        );
        assert_eq!(events.last(), Some(&DownloadEvent::Finished));
        assert_eq!(
            std::fs::read(temp.join("freewheel-0.2.0.zip")).unwrap(),
            package
        );
        let e = u.install().unwrap_err();
        assert!(e.to_string().starts_with("install:"), "{e}");
        assert_eq!(read(&exe), "A");
        assert!(!sibling(&exe, "old").exists() && !sibling(&exe, "new").exists());
    }

    /// The packagers the release uses (scripts/portable-release.mjs), and
    /// this unpacker, agree: run on the OS that packs.
    #[test]
    fn the_release_packagers_make_what_the_updater_unpacks() {
        let script = include_str!("../../scripts/portable-release.mjs");
        assert!(script.contains(r#"["-c", "-k", "--norsrc", "--keepParent""#));
        assert!(script.contains(r#"["-a", "-c", "-f""#));
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let app = dir.path().join("app");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        let zip = dir.path().join("package.zip");
        if cfg!(target_os = "macos") {
            let bundle = src.join("freewheel.app");
            std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
            std::fs::create_dir_all(bundle.join("Contents/Resources")).unwrap();
            std::fs::write(bundle.join("Contents/MacOS/freewheel"), "B").unwrap();
            std::fs::write(bundle.join("Contents/Info.plist"), "plist").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let exe = bundle.join("Contents/MacOS/freewheel");
                std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
                std::os::unix::fs::symlink("../Info.plist", bundle.join("Contents/Resources/link"))
                    .unwrap();
            }
            let ok = std::process::Command::new("/usr/bin/ditto")
                .args(["-c", "-k", "--norsrc", "--keepParent"])
                .arg(&bundle)
                .arg(&zip)
                .status()
                .unwrap();
            assert!(ok.success());
            let target = app.join("freewheel.app");
            std::fs::create_dir_all(target.join("Contents/MacOS")).unwrap();
            std::fs::write(target.join("Contents/MacOS/freewheel"), "A").unwrap();
            install_package(&zip, &target, "freewheel.app").unwrap();
            assert_eq!(read(&target.join("Contents/MacOS/freewheel")), "B");
            assert_eq!(read(&target.join("Contents/Resources/link")), "plist");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let meta = std::fs::metadata(target.join("Contents/MacOS/freewheel")).unwrap();
                assert_eq!(meta.permissions().mode() & 0o777, 0o755);
                let link = std::fs::read_link(target.join("Contents/Resources/link")).unwrap();
                assert_eq!(link, Path::new("../Info.plist"));
            }
        } else if cfg!(windows) {
            std::fs::write(src.join("freewheel.exe"), "B").unwrap();
            std::fs::write(src.join("freewheel-simulate.cmd"), "cmd").unwrap();
            let tar = Path::new(&std::env::var("SystemRoot").unwrap()).join(r"System32\tar.exe");
            let ok = std::process::Command::new(tar)
                .args(["-a", "-c", "-f"])
                .arg(&zip)
                .arg("-C")
                .arg(&src)
                .args(["freewheel.exe", "freewheel-simulate.cmd"])
                .status()
                .unwrap();
            assert!(ok.success());
            let target = app.join("freewheel.exe");
            std::fs::write(&target, "A").unwrap();
            install_package(&zip, &target, "freewheel.exe").unwrap();
            assert_eq!(read(&target), "B");
            assert_eq!(read(&sibling(&target, "old")), "A");
        }
    }
}
