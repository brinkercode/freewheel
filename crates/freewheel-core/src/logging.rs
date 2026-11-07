//! Diagnostic file logging: `freewheel.log` in a log directory the app crate
//! resolves, rotated at 2 MB with 5 files kept, INFO by default.
//!
//! What the log holds: every key press and release with its page and
//! mapping, and every action's result (engine.rs, describe.rs), so a log
//! shows what happened. Mapped shortcuts, app paths, page names and website
//! hosts are written (they are the user's own setup, in the user's own log);
//! a website's whole address only with detailed logs on. Never written: what
//! the user types, window titles, audio session names, device serial numbers
//! or auth bytes (logged only as their kind and a hash, [`hashed`]). The home
//! path is replaced by `~` in every line written.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tracing::dispatcher::{self, SetGlobalDefaultError};
use tracing::{Dispatch, Level, debug, error, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{Layer, filter, fmt as tfmt};

use crate::clock::Clock;

/// Name of the current log file; rotated files get `.1` to `.4`.
pub const LOG_FILE: &str = "freewheel.log";
/// A log file is rotated before it would grow past this size.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Log files kept, the current one included.
pub const MAX_FILES: usize = 5;
/// Detailed (DEBUG) logs turn themselves off after this long.
pub const DETAILED_LOGS_FOR: Duration = Duration::from_secs(24 * 60 * 60);

/// Appends to `freewheel.log`, rotating it by size and scrubbing the home path.
struct RotatingFile {
    dir: PathBuf,
    file: File,
    size: u64,
    home: Option<String>,
}

impl RotatingFile {
    fn open(dir: &Path, home: Option<&Path>) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let file = Self::append(&dir.join(LOG_FILE))?;
        let size = file.metadata()?.len();
        let home = home
            .map(|h| h.to_string_lossy().into_owned())
            .filter(|h| !h.is_empty());
        Ok(Self {
            dir: dir.to_path_buf(),
            file,
            size,
            home,
        })
    }

    fn append(path: &Path) -> io::Result<File> {
        OpenOptions::new().create(true).append(true).open(path)
    }

    fn path(&self, n: usize) -> PathBuf {
        match n {
            0 => self.dir.join(LOG_FILE),
            n => self.dir.join(format!("{LOG_FILE}.{n}")),
        }
    }

    fn rotate(&mut self) -> io::Result<()> {
        for n in (1..MAX_FILES).rev() {
            let from = self.path(n - 1);
            if from.exists() {
                fs::rename(from, self.path(n))?;
            }
        }
        self.file = Self::append(&self.path(0))?;
        self.size = 0;
        Ok(())
    }
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        let text = match &self.home {
            Some(home) => text.replace(home.as_str(), "~"),
            None => text.into_owned(),
        };
        let len = text.len() as u64;
        if self.size > 0 && self.size + len > MAX_FILE_BYTES {
            self.rotate()?;
        }
        self.file.write_all(text.as_bytes())?;
        self.size += len;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// The session-only detailed-logs switch.
struct DetailedLogs {
    clock: Arc<dyn Clock + Send + Sync>,
    since: Mutex<Option<Instant>>,
}

impl DetailedLogs {
    fn max_level(&self) -> Level {
        let mut since = self.since.lock().unwrap_or_else(|e| e.into_inner());
        match *since {
            Some(t) if self.clock.now().saturating_duration_since(t) < DETAILED_LOGS_FOR => {
                Level::DEBUG
            }
            _ => {
                *since = None;
                Level::INFO
            }
        }
    }
}

/// The file logger of one app session. Clones share the file and the
/// detailed-logs switch.
#[derive(Clone)]
pub struct Logger {
    dispatch: Dispatch,
    detailed: Arc<DetailedLogs>,
    dir: PathBuf,
}

impl Logger {
    /// Opens (appending) the log in `dir`. `home` is scrubbed from every line.
    pub fn init(
        dir: &Path,
        home: Option<&Path>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> io::Result<Self> {
        let writer = Mutex::new(RotatingFile::open(dir, home)?);
        let detailed = Arc::new(DetailedLogs {
            clock,
            since: Mutex::new(None),
        });
        let level = Arc::clone(&detailed);
        let layer = tfmt::layer()
            .with_ansi(false)
            .with_writer(writer)
            .with_filter(filter::dynamic_filter_fn(move |meta, _| {
                *meta.level() <= level.max_level()
            }));
        let dispatch = Dispatch::new(tracing_subscriber::registry().with(layer));
        Ok(Self {
            dispatch,
            detailed,
            dir: dir.to_path_buf(),
        })
    }

    /// The directory holding `freewheel.log` and its rotated files.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Makes this logger the process-wide default.
    pub fn install_global(&self) -> Result<(), SetGlobalDefaultError> {
        dispatcher::set_global_default(self.dispatch.clone())
    }

    /// Logs every panic's message and location, then runs the previous hook.
    pub fn install_panic_hook(&self) {
        let dispatch = self.dispatch.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let message = info.payload_as_str().unwrap_or("non-string panic payload");
            let location = info.location().map(ToString::to_string).unwrap_or_default();
            dispatcher::with_default(&dispatch, || error!("panic at {location}: {message}"));
            previous(info);
        }));
    }

    /// Writes the startup line: app version, OS name/version and architecture.
    pub fn log_startup(&self, version: &str, os: &str) {
        dispatcher::with_default(&self.dispatch, || {
            info!(
                "freewheel {version} starting on {os} ({})",
                std::env::consts::ARCH
            )
        });
    }

    /// Turns detailed (DEBUG) logs on for [`DETAILED_LOGS_FOR`], or off.
    /// The logger itself keeps nothing: every launch starts at INFO until
    /// the app applies the saved `settings.detailed_logs`.
    pub fn set_detailed(&self, on: bool) {
        *self
            .detailed
            .since
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = on.then(|| self.detailed.clock.now());
        dispatcher::with_default(&self.dispatch, || {
            info!("detailed logs {}", if on { "on" } else { "off" })
        });
    }

    /// Whether detailed (DEBUG) logs are on right now.
    pub fn detailed(&self) -> bool {
        self.detailed.max_level() == Level::DEBUG
    }
}

/// A sensitive value as it may appear in a log: its kind and a hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hashed {
    kind: &'static str,
    hash: u64,
}

impl fmt::Display for Hashed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{:016x}", self.kind, self.hash)
    }
}

/// Hashes a sensitive value (FNV-1a, stable across builds) for logging.
pub fn hashed(kind: &'static str, value: impl AsRef<[u8]>) -> Hashed {
    let hash = value
        .as_ref()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, &b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        });
    Hashed { kind, hash }
}

/// Logs a button press at DEBUG with the action's type only, e.g.
/// `CUT pressed -> action hotkey`.
pub fn button_pressed(button: &str, action_kind: &str) {
    debug!("{button} pressed -> action {action_kind}");
}

/// Tests only (it installs a global subscriber): runs `f` with this thread's log lines at `level` going to a buffer;
/// returns what was written. One global subscriber routes each thread's
/// lines (a per-thread default subscriber races with the callsite cache
/// while other tests run).
pub fn capture(level: Level, f: impl FnOnce()) -> String {
    use std::cell::RefCell;
    use std::sync::Once;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::{Layer, filter};
    thread_local! {
        static CAPTURE: RefCell<Option<(Level, Vec<u8>)>> = const { RefCell::new(None) };
    }
    struct Routed;
    impl std::io::Write for Routed {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            CAPTURE.with(|c| {
                if let Some((_, buf)) = &mut *c.borrow_mut() {
                    buf.extend_from_slice(b);
                }
            });
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(|| Routed)
            .with_filter(filter::dynamic_filter_fn(|meta, _| {
                CAPTURE.with(|c| c.borrow().as_ref().is_some_and(|(l, _)| meta.level() <= l))
            }));
        let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry().with(layer));
    });
    CAPTURE.with(|c| *c.borrow_mut() = Some((level, Vec::new())));
    f();
    let (_, buf) = CAPTURE.with(|c| c.borrow_mut().take()).unwrap();
    String::from_utf8(buf).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeClock(Mutex<Instant>);

    impl FakeClock {
        fn new() -> Arc<Self> {
            Arc::new(Self(Mutex::new(Instant::now())))
        }

        fn advance(&self, by: Duration) {
            *self.0.lock().unwrap() += by;
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Instant {
            *self.0.lock().unwrap()
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("freewheel-logging-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn log_files(dir: &Path) -> Vec<PathBuf> {
        let mut files: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        files.sort();
        files
    }

    fn read_logs(dir: &Path) -> String {
        log_files(dir)
            .iter()
            .map(|p| fs::read_to_string(p).unwrap())
            .collect()
    }

    fn logger(dir: &Path, clock: Arc<FakeClock>) -> Logger {
        Logger::init(dir, None, clock).unwrap()
    }

    #[test]
    fn startup_line_is_info_and_restart_appends() {
        let dir = temp_dir("startup");
        let expected = format!(
            "freewheel 0.1.0 starting on TestOS 1.2 ({})",
            std::env::consts::ARCH
        );
        logger(&dir, FakeClock::new()).log_startup("0.1.0", "TestOS 1.2");
        logger(&dir, FakeClock::new()).log_startup("0.1.0", "TestOS 1.2");

        let log = fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        let lines: Vec<_> = log.lines().filter(|l| l.contains(&expected)).collect();
        assert_eq!(lines.len(), 2, "{log}");
        assert!(lines.iter().all(|l| l.contains(" INFO ")), "{log}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotation_keeps_five_files_of_at_most_2_mb() {
        let dir = temp_dir("rotation");
        let logger = logger(&dir, FakeClock::new());
        let line = "x".repeat(1000);
        dispatcher::with_default(&logger.dispatch, || {
            for _ in 0..11_000 {
                info!("{line}");
            }
        });

        let files = log_files(&dir);
        assert_eq!(files.len(), MAX_FILES, "{files:?}");
        for file in &files {
            assert!(
                fs::metadata(file).unwrap().len() <= MAX_FILE_BYTES,
                "{file:?}"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn info_by_default_and_button_events_only_at_debug() {
        let dir = temp_dir("default-level");
        let logger = logger(&dir, FakeClock::new());
        assert!(!logger.detailed());
        dispatcher::with_default(&logger.dispatch, || {
            info!("info line");
            button_pressed("CUT", "hotkey");
        });
        let log = read_logs(&dir);
        assert!(log.contains("info line"));
        assert!(!log.contains("CUT pressed"), "{log}");

        logger.set_detailed(true);
        dispatcher::with_default(&logger.dispatch, || button_pressed("CUT", "hotkey"));
        assert!(read_logs(&dir).contains("CUT pressed -> action hotkey"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detailed_logs_revert_to_info_after_24_hours() {
        let dir = temp_dir("detailed-revert");
        let clock = FakeClock::new();
        let logger = logger(&dir, Arc::clone(&clock));
        logger.set_detailed(true);
        assert!(logger.detailed());

        clock.advance(DETAILED_LOGS_FOR - Duration::from_secs(1));
        dispatcher::with_default(&logger.dispatch, || debug!("still detailed"));
        assert!(logger.detailed());

        clock.advance(Duration::from_secs(1));
        dispatcher::with_default(&logger.dispatch, || debug!("after a day"));
        assert!(!logger.detailed());

        let log = read_logs(&dir);
        assert!(log.contains("still detailed"));
        assert!(!log.contains("after a day"), "{log}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detailed_logs_are_off_at_every_launch() {
        let dir = temp_dir("detailed-launch");
        logger(&dir, FakeClock::new()).set_detailed(true);
        let relaunched = logger(&dir, FakeClock::new());
        assert!(!relaunched.detailed());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn logs_hold_no_private_values_at_info_or_debug() {
        let home = Path::new("/home/alice-private");
        let private_values: [(&'static str, &str); 7] = [
            ("shortcut", "ctrl+alt+shift+F13"),
            ("launch target", "C:\\Users\\alice\\secret-app.exe"),
            ("uri", "https://example.test/private/diary"),
            ("window title", "Bank statement - Secret Browser"),
            ("app", "SecretChat.exe"),
            ("audio session", "SecretMusicPlayer"),
            ("serial", "SN-PRIVATE-123456"),
        ];
        let auth = [0xde_u8, 0xad, 0xbe, 0xef, 0x13, 0x37];

        for detailed in [false, true] {
            let dir = temp_dir(&format!("privacy-{detailed}"));
            let logger = Logger::init(&dir, Some(home), FakeClock::new()).unwrap();
            logger.set_detailed(detailed);
            logger.log_startup("0.1.0", "TestOS 1.2");
            dispatcher::with_default(&logger.dispatch, || {
                button_pressed("CUT", "hotkey");
                for (kind, value) in private_values {
                    info!("sent {}", hashed(kind, value));
                    debug!("sent {}", hashed(kind, value));
                }
                debug!("auth challenge {}", hashed("auth", auth));
                info!("config at {}", home.join("cfg").display());
            });

            let log = read_logs(&dir);
            for (kind, value) in private_values {
                assert!(!log.contains(value), "{value} leaked: {log}");
                assert!(log.contains(&hashed(kind, value).to_string()));
            }
            assert!(!log.contains(home.to_str().unwrap()), "{log}");
            assert!(log.contains("config at ~"));
            assert!(
                !log.contains("deadbeef") && !log.contains("222, 173"),
                "{log}"
            );
            assert_eq!(log.contains("CUT pressed -> action hotkey"), detailed);
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn panic_hook_logs_message_and_location() {
        let dir = temp_dir("panic");
        let logger = logger(&dir, FakeClock::new());
        logger.install_panic_hook();
        let line = line!() + 1;
        let result = std::thread::spawn(|| panic!("wheel fell off")).join();
        assert!(result.is_err());

        let log = read_logs(&dir);
        assert!(log.contains("wheel fell off"), "{log}");
        assert!(log.contains(&format!("{}:{line}:", file!())), "{log}");
        let _ = fs::remove_dir_all(&dir);
    }
}
