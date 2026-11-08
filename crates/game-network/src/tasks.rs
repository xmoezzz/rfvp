// Copyright (c) 2026 CrySirror contributors. MPL-2.0.
//! Bounded blocking transport operations. Dropping a result cancels queued work.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
type Job = Box<dyn FnOnce() + Send>;
#[derive(Clone)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn check(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.0.load(Ordering::Acquire),
            "network operation cancelled"
        );
        Ok(())
    }
}
pub struct TaskPool {
    sender: mpsc::SyncSender<Job>,
}
impl TaskPool {
    pub fn new(name: &str, workers: usize, capacity: usize) -> anyhow::Result<Self> {
        anyhow::ensure!(
            (1..=16).contains(&workers) && (1..=1024).contains(&capacity),
            "network worker bounds"
        );
        let (sender, receiver) = mpsc::sync_channel::<Job>(capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..workers {
            let rx = receiver.clone();
            std::thread::Builder::new()
                .name(format!("{name}-{index}"))
                .spawn(move || loop {
                    let job = {
                        let Ok(guard) = rx.lock() else { break };
                        guard.recv()
                    };
                    let Ok(job) = job else { break };
                    job();
                })?;
        }
        Ok(Self { sender })
    }
    pub fn spawn<T: Send + 'static>(
        &self,
        f: impl FnOnce(Cancellation) -> anyhow::Result<T> + Send + 'static,
    ) -> anyhow::Result<Task<T>> {
        let (tx, rx) = mpsc::sync_channel(1);
        let cancel = Cancellation(Arc::new(AtomicBool::new(false)));
        let work_cancel = cancel.clone();
        self.sender
            .try_send(Box::new(move || {
                let result = work_cancel.check().and_then(|_| f(work_cancel));
                let _ = tx.send(result);
            }))
            .map_err(|_| anyhow::anyhow!("network worker queue unavailable"))?;
        Ok(Task { rx, cancel })
    }
}
pub struct Task<T> {
    rx: mpsc::Receiver<anyhow::Result<T>>,
    cancel: Cancellation,
}
impl<T> Task<T> {
    pub fn try_take(&mut self) -> anyhow::Result<Option<T>> {
        match self.rx.try_recv() {
            Ok(v) => v.map(Some),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => anyhow::bail!("network worker disconnected"),
        }
    }
}
impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        self.cancel.0.store(true, Ordering::Release);
    }
}
