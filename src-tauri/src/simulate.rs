//! `freewheel --simulate`: one thread owns the simulator session (ADR-04) and
//! the Simulator window drives it through the `simulate` command.

use std::sync::mpsc::{self, Sender};

use freewheel_core::config::ConfigStore;
use freewheel_core::inject::KeyInjector;
use freewheel_core::simulator::{Control, SimStatus, Simulator};

use crate::launch::ProcessLauncher;

type Request = (Control, Sender<SimStatus>);

/// Tauri state: the channel to the simulator thread.
pub struct SimHandle(Sender<Request>);

/// Starts the session on its own thread. `injector` is built there and is
/// only used while "send for real" is on.
pub fn spawn(
    store: ConfigStore,
    injector: impl FnOnce() -> Box<dyn KeyInjector> + Send + 'static,
) -> SimHandle {
    let (tx, rx) = mpsc::channel::<Request>();
    std::thread::spawn(move || {
        let mut sim = Simulator::new(store, injector(), Box::new(ProcessLauncher));
        for (control, reply) in rx {
            let _ = reply.send(sim.apply(control));
        }
    });
    SimHandle(tx)
}

impl SimHandle {
    pub fn apply(&self, control: Control) -> Result<SimStatus, String> {
        let (tx, rx) = mpsc::channel();
        self.0.send((control, tx)).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn simulate(control: Control, sim: tauri::State<'_, SimHandle>) -> Result<SimStatus, String> {
    sim.apply(control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use freewheel_core::connection::ConnectionStatus;
    use freewheel_core::inject::MockInjector;
    use freewheel_core::simulator::Bus;

    #[test]
    fn session_thread_answers_each_control() {
        let dir = tempfile::tempdir().unwrap();
        let sim = spawn(ConfigStore::new(dir.path()), || {
            Box::new(MockInjector::new())
        });
        let s = sim.apply(Control::Status).unwrap();
        assert_eq!(s.connection, Some(ConnectionStatus::Absent));
        let s = sim.apply(Control::Connect { bus: Bus::Usb }).unwrap();
        assert_eq!(s.connection, Some(ConnectionStatus::Connected));
        assert_eq!(s.bus, Some(Bus::Usb));
    }
}
