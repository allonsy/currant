pub use nix::sys::signal::Signal;
use std::sync::Arc;
use std::sync::Mutex;

use crate::kill_barrier::KillBarrier;

/// Provides a way to send signals to the underlying processes.
pub struct HandleControl {
    pids: Vec<Arc<(String, Mutex<Option<u32>>)>>,
    kill_barrier: KillBarrier,
}

impl HandleControl {
    /// Construct a new [HandleControl].
    /// This shouldn't really be called. Use [CommandHandle::get_signaler](crate::CommandHandle::get_signaler) and [ControlledCommandHandle::get_signaler](crate::ControlledCommandHandle::get_signaler) instead
    pub fn new(pids: Vec<Arc<(String, Mutex<Option<u32>>)>>, barrier: KillBarrier) -> Self {
        Self {
            pids,
            kill_barrier: barrier,
        }
    }

    /// Kills all running processes. This uses the kill barrier functionality and will work on all OS-es. This won't send a SIGTERM to all processes
    pub fn kill_all(&self) -> Result<(), String> {
        self.kill_barrier.initiate_kill()
    }

    /// UNIX-ONLY: Send a unix signal to a specific child process by name.
    /// See [Signal] for variants.
    /// On windows machines this will most likely just kill the child process.
    /// Returns `()` on success or an error message if the signal couldn't be sent
    pub fn signal_one(&self, cmd_name: &str, signal: Signal) -> Result<(), String> {
        for pid_arc in self.pids.iter() {
            let (name, lock) = &**pid_arc;
            if name == cmd_name {
                if let Ok(unlocked_pid) = lock.lock() {
                    if let Some(pid) = &*unlocked_pid {
                        return send_signal(*pid, signal);
                    } else {
                        return Err(format!("Unable to look up pid for cmd: {}", cmd_name));
                    }
                } else {
                    return Err(format!(
                        "Unable to acquire poisoned lock for pidlist for command: {}",
                        cmd_name
                    ));
                }
            }
        }

        Err(format!("process named: '{}' not found", cmd_name))
    }

    /// UNIX-ONLY: Send a unix signal to all child processes.
    /// See [Signal] for variants.
    /// On windows machines this will most likely just kill all the child processes.
    /// If an error occurs sending a message to a specific process, currant will silently move on to the next child process
    pub fn signal_all(&self, signal: Signal) {
        for pid_arc in self.pids.iter() {
            if let Ok(unlocked_pid) = pid_arc.1.lock() {
                if let Some(pid) = &*unlocked_pid {
                    let _ = send_signal(*pid, signal);
                }
            }
        }
    }
}

fn send_signal(pid: u32, signal: Signal) -> Result<(), String> {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), signal)
        .map_err(|e| e.to_string())
}
