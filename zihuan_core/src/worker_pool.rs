use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use log::warn;

type Task = Box<dyn FnOnce() + Send + 'static>;

struct WorkerPoolInner {
    sender: SyncSender<Task>,
    active: Arc<(Mutex<usize>, Condvar)>,
}

/// A fixed-size OS thread pool. Tasks submitted via [`WorkerPool::submit`] are executed
/// concurrently up to `num_threads`. When the queue is full the task is dropped with a
/// warning instead of blocking the caller.
///
/// Cheap to clone — all clones share the same underlying pool.
#[derive(Clone)]
pub struct WorkerPool {
    inner: Arc<WorkerPoolInner>,
}

impl WorkerPool {
    pub fn new(num_threads: usize, queue_capacity: usize) -> Self {
        assert!(num_threads > 0, "WorkerPool requires at least one thread");

        let (sender, receiver) = mpsc::sync_channel::<Task>(queue_capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let active: Arc<(Mutex<usize>, Condvar)> = Arc::new((Mutex::new(0usize), Condvar::new()));

        for i in 0..num_threads {
            let receiver = Arc::clone(&receiver);
            let active = Arc::clone(&active);
            thread::Builder::new()
                .name(format!("zihuan-worker-{i}"))
                .spawn(move || run_worker(receiver, active))
                .expect("failed to spawn worker thread");
        }

        Self {
            inner: Arc::new(WorkerPoolInner { sender, active }),
        }
    }

    /// Submit a task. Returns immediately. If the queue is full the task is dropped.
    pub fn submit<F: FnOnce() + Send + 'static>(&self, task: F) {
        {
            let (lock, _) = &*self.inner.active;
            *lock.lock().unwrap() += 1;
        }
        if self.inner.sender.try_send(Box::new(task)).is_err() {
            warn!("[worker_pool] queue full or disconnected, dropping task");
            let (lock, cvar) = &*self.inner.active;
            let mut count = lock.lock().unwrap();
            *count = count.saturating_sub(1);
            if *count == 0 {
                cvar.notify_all();
            }
        }
    }

    /// Block until all currently submitted tasks have finished.
    pub fn wait_idle(&self) {
        let (lock, cvar) = &*self.inner.active;
        let mut count = lock.lock().unwrap();
        while *count > 0 {
            count = cvar.wait(count).unwrap();
        }
    }
}

fn run_worker(receiver: Arc<Mutex<Receiver<Task>>>, active: Arc<(Mutex<usize>, Condvar)>) {
    loop {
        let task = {
            let Ok(rx) = receiver.lock() else { break };
            match rx.recv() {
                Ok(task) => task,
                Err(_) => break,
            }
        };

        task();

        let (lock, cvar) = &*active;
        let mut count = lock.lock().unwrap();
        *count = count.saturating_sub(1);
        if *count == 0 {
            cvar.notify_all();
        }
    }
}
