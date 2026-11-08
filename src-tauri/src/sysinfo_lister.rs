//! The production [`ProcessLister`]: the OS process list through sysinfo
//! (ADR-08), refreshing only process names.

use freewheel_core::resolve::ProcessLister;
#[cfg(not(target_os = "linux"))]
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

pub struct SysinfoLister {
    /// None when sysinfo can't read the process list here; then nothing
    /// counts as running.
    #[cfg(not(target_os = "linux"))]
    system: Option<System>,
}

impl SysinfoLister {
    pub fn new() -> Self {
        Self {
            #[cfg(not(target_os = "linux"))]
            system: System::new()
                .inspect_err(|e| tracing::warn!(error = %e, "can't read the process list"))
                .ok(),
        }
    }
}

impl ProcessLister for SysinfoLister {
    /// Refreshes the process list once per call; the watcher asks for one
    /// name per poll, so this is one refresh per poll.
    #[cfg(not(target_os = "linux"))]
    fn is_running(&mut self, name: &str) -> bool {
        use std::ffi::OsStr;
        let Some(system) = self.system.as_mut() else {
            return false;
        };
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing(),
        );
        system
            .processes_by_exact_name(OsStr::new(name))
            .next()
            .is_some()
    }

    /// Linux: one read of each `/proc/<pid>/stat` (linux_idle.rs), not a
    /// sysinfo refresh: the 2 s poll was most of an idle freewheel's CPU.
    #[cfg(target_os = "linux")]
    fn is_running(&mut self, name: &str) -> bool {
        crate::linux_idle::running(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_process_is_not_running() {
        let mut lister = SysinfoLister::new();
        assert!(!lister.is_running("freewheel-no-such-process.exe"));
    }
}
