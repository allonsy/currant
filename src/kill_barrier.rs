use std::sync::{mpsc, Arc, Barrier, Mutex};
use std::thread;

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
