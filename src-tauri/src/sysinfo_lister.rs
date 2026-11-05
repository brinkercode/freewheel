//! The production [`ProcessLister`]: the OS process list through sysinfo
//! (ADR-08), refreshing only process names.

use std::ffi::OsStr;

use freewheel_core::resolve::ProcessLister;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

pub struct SysinfoLister {
    system: System,
}

impl SysinfoLister {
    pub fn new() -> Self {
        Self {
            system: System::new(),
        }
    }
}

impl ProcessLister for SysinfoLister {
    /// Refreshes the process list once per call; the watcher asks for one
    /// name per poll, so this is one refresh per poll.
    fn is_running(&mut self, name: &str) -> bool {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing(),
        );
        self.system
            .processes_by_exact_name(OsStr::new(name))
            .next()
            .is_some()
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
