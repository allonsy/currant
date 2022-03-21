use std::sync::{mpsc, Arc, Barrier, Mutex};
use std::thread;

/// Synchronizes threads with a barrier-like effect.
/// All threads wait at the barrier until any one of the threads unlocks the barrier.
/// At that point, all waiting threads and any future threads are let through the barrier.
/// An application of this is all supervisor threads need to know whether or not to kill their worker thread.
/// In this scenario, all supervisor threads are locked at the barrier, but, if any threads need to tell all other threads to die
/// (if RestartOptions::Kill are set and one of the threads dies for example) all threads are unlocked from the barrier and the threads
/// will know to die.
/// This differs from a traditional Barrier in that it isn't based on the number of threads at the barrier but rather a condition.
/// This differs from a conditional variable (condvar) in that all future threads also are unlocked and not just a one at a time unlock for current threads.
/// This is more of combination between a condvar and a barrier.
pub struct KillBarrier {
    kill_chan: mpsc::Sender<()>,
    lock: Arc<Mutex<()>>,
}

impl KillBarrier {
    pub fn new() -> KillBarrier {
        let start_barrier = Arc::new(Barrier::new(2));

        let sync_lock = Arc::new(Mutex::new(()));
        let (send, recv) = mpsc::channel();

        let lock_clone = sync_lock.clone();
        let barrier_clone = start_barrier.clone();

        thread::spawn(move || {
            let _res = lock_clone.lock();
            barrier_clone.wait();

            let _ = recv.recv();
        });

        start_barrier.wait();

        KillBarrier {
            kill_chan: send,
            lock: sync_lock,
        }
    }

    pub fn wait(&self) -> Result<(), String> {
        let lock_res = self.lock.lock();
        match lock_res {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("{}", e)),
        }
    }

    pub fn initiate_kill(&self) -> Result<(), String> {
        let res = self.kill_chan.send(());

        match res {
            Ok(()) => Ok(()),
            Err(e) => Err(format!("{}", e)),
        }
    }
}

impl Default for KillBarrier {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for KillBarrier {
    fn clone(&self) -> KillBarrier {
        KillBarrier {
            kill_chan: self.kill_chan.clone(),
            lock: self.lock.clone(),
        }
    }
}
