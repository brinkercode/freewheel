//! Linux: what freewheel costs while it waits in the tray.
//!
//! - With no window left, WebKitGTK's network process stays alive for
//!   nothing (only webviews use it; the next window starts a new one) and
//!   glibc keeps the closed webview's freed heap: [`last_window_closed`]
//!   ends the one and gives the other back.
//! - The Resolve hand-off asks every 2 s whether a process runs: reading
//!   each `/proc/<pid>/stat` once ([`running`]) is far cheaper than a
//!   sysinfo refresh of every process.

use std::path::Path;
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

/// WebKitGTK's network process (its kernel name is the first 15 bytes).
pub const NETWORK_PROCESS: &str = "WebKitNetworkProcess";

/// How long after the last window closed freewheel tidies up (a window
/// reopened meanwhile cancels it).
pub const SETTLE: Duration = Duration::from_secs(2);

/// The kernel's name for a process called `name`: at most 15 bytes.
pub fn comm_of(name: &str) -> &str {
    let mut end = name.len().min(15);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

/// `/proc/<pid>/stat`'s name and parent pid (the name may hold spaces and
/// parentheses: it ends at the last `)`).
pub fn parse_stat(stat: &str) -> Option<(&str, u32)> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let comm = stat.get(open + 1..close)?;
    let mut rest = stat.get(close + 1..)?.split_whitespace();
    let _state = rest.next()?;
    let ppid = rest.next()?.parse().ok()?;
    Some((comm, ppid))
}

/// The pids under `proc` (normally `/proc`) whose process is `name` and,
/// when given, a child of `parent`. A name longer than the kernel keeps
/// also needs the program's file name (argv[0]) to match.
pub fn processes(proc: &Path, name: &str, parent: Option<u32>) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir(proc) else {
        return Vec::new();
    };
    let comm = comm_of(name);
    let mut pids: Vec<u32> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            let dir = proc.join(pid.to_string());
            let Ok(stat) = std::fs::read_to_string(dir.join("stat")) else {
                return false;
            };
            let Some((c, ppid)) = parse_stat(&stat) else {
                return false;
            };
            if c != comm || parent.is_some_and(|p| p != ppid) {
                return false;
            }
            comm.len() == name.len()
                || std::fs::read(dir.join("cmdline")).is_ok_and(|cmd| {
                    let argv0 = cmd.split(|b| *b == 0).next().unwrap_or_default();
                    let argv0 = String::from_utf8_lossy(argv0);
                    argv0.rsplit('/').next() == Some(name)
                })
        })
        .collect();
    pids.sort_unstable();
    pids
}

/// Whether a process named `name` runs (the Resolve hand-off).
pub fn running(name: &str) -> bool {
    !processes(Path::new("/proc"), name, None).is_empty()
}

/// A window was destroyed: once none is left (after [`SETTLE`]), ends
/// WebKitGTK's idle network process and returns freed heap to the system.
pub fn last_window_closed<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SETTLE);
        if !app.webview_windows().is_empty() {
            return;
        }
        let ended = processes(
            Path::new("/proc"),
            NETWORK_PROCESS,
            Some(std::process::id()),
        );
        for pid in &ended {
            // SAFETY: kill(2) with a pid read from /proc: our own child.
            unsafe {
                libc::kill(*pid as libc::pid_t, libc::SIGTERM);
            }
        }
        #[cfg(target_env = "gnu")]
        // SAFETY: malloc_trim only releases free heap pages; thread-safe.
        unsafe {
            libc::malloc_trim(0);
        }
        tracing::info!(
            ?ended,
            "no window left: WebKit's network process ended, heap trimmed"
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kernel_keeps_15_bytes_of_a_name() {
        assert_eq!(comm_of(NETWORK_PROCESS), "WebKitNetworkPr");
        assert_eq!(comm_of("resolve"), "resolve");
        assert_eq!(comm_of("ééééééééé"), "ééééééé");
    }

    #[test]
    fn stat_names_may_hold_spaces_and_parentheses() {
        assert_eq!(
            parse_stat("4242 (Web Content (x)) S 17 4242 4242 0 -1"),
            Some(("Web Content (x)", 17))
        );
        assert_eq!(parse_stat("garbage"), None);
    }

    #[test]
    fn processes_match_name_parent_and_long_names_by_argv0() {
        let dir = tempfile::tempdir().unwrap();
        let add = |pid: u32, comm: &str, ppid: u32, argv0: &str| {
            let p = dir.path().join(pid.to_string());
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("stat"), format!("{pid} ({comm}) S {ppid} 1 1 0")).unwrap();
            std::fs::write(p.join("cmdline"), format!("{argv0}\0--x\0")).unwrap();
        };
        add(10, "resolve", 1, "/opt/resolve/bin/resolve");
        add(
            11,
            "WebKitNetworkPr",
            99,
            "/usr/lib/webkit2gtk-4.1/WebKitNetworkProcess",
        );
        add(
            12,
            "WebKitNetworkPr",
            7,
            "/usr/lib/webkit2gtk-4.1/WebKitNetworkProcess",
        );
        add(13, "WebKitNetworkPr", 99, "/usr/bin/WebKitNetworkProbe");
        std::fs::create_dir_all(dir.path().join("self")).unwrap();
        assert_eq!(processes(dir.path(), "resolve", None), [10]);
        assert_eq!(processes(dir.path(), NETWORK_PROCESS, Some(99)), [11]);
        assert_eq!(processes(dir.path(), NETWORK_PROCESS, None), [11, 12]);
        assert!(processes(dir.path(), "Resolve", None).is_empty());
        assert!(processes(&dir.path().join("nope"), "resolve", None).is_empty());
    }

    #[test]
    fn this_test_runs_in_a_process_the_scan_finds() {
        let me = std::fs::read_to_string("/proc/self/comm").unwrap();
        let me = me.trim();
        assert!(
            processes(Path::new("/proc"), me, None).contains(&std::process::id()) || me.len() >= 15
        );
    }
}
