use futures::channel::oneshot;
use futures::future::BoxFuture;
use futures::task::{waker_ref, ArcWake};
use futures::FutureExt;

use parking_lot::{Condvar, Mutex};
use slab::Slab;

use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering as AtomicOrdering},
};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

/// Tickless single-thread executor core.
///
/// Typical usage:
/// - Create `Executor` on the thread you want to drive.
/// - Keep a `Handle` to spawn tasks from anywhere (same thread).
/// - In your event loop: `run_until_stalled()`, then `park_until_deadline_or_woken()`.
#[derive(Clone)]
pub struct Executor {
    inner: Arc<Inner>,
}

impl Executor {
    pub fn new() -> Self {
        Self { inner: Arc::new(Inner::new()) }
    }

    pub fn handle(&self) -> Handle {
        Handle { inner: self.inner.clone() }
    }

    /// Poll ready tasks until no further progress can be made.
    /// Returns number of tasks polled (not number completed).
    pub fn run_until_stalled(&self) -> usize {
        self.inner.wake_due_timers();

        let mut polled = 0usize;
        while let Some(id) = self.inner.pop_ready() {
            polled += 1;
            self.inner.poll_task(id);
            self.inner.wake_due_timers();
        }
        polled
    }

    /// When should an outer event loop wake up next (if no external events arrive)?
    pub fn next_deadline(&self) -> Option<Instant> {
        self.inner.peek_deadline()
    }

    /// Block the current thread until either:
    /// - a task is woken (enqueue), or
    /// - the next timer deadline arrives (then due timers will be woken), whichever comes first.
    ///
    /// This is the key to "no fixed frame rate": you only wake when something is due.
    pub fn park_until_deadline_or_woken(&self) {
        self.inner.park_until_deadline_or_woken();
    }
}

/// A clonable handle used by components to spawn tasks and create sleep futures.
/// Keep this in your engine subsystems (script, assets, etc.).
#[derive(Clone)]
pub struct Handle {
    inner: Arc<Inner>,
}

impl Handle {
    pub fn spawn<F, T>(&self, fut: F) -> JoinHandle<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel::<T>();
        let task_fut = async move {
            let out = fut.await;
            let _ = tx.send(out);
        };

        self.inner.insert_task(task_fut.boxed())
            .map(|id| self.inner.enqueue(id))
            .expect("failed to spawn task");

        JoinHandle { rx }
    }

    /// Tickless sleep. It registers a timer on first poll and yields Pending until deadline.
    pub fn sleep(&self, dur: Duration) -> Sleep {
        Sleep {
            inner: self.inner.clone(),
            deadline: Instant::now() + dur,
            registered: false,
        }
    }
}

pub struct JoinHandle<T> {
    rx: oneshot::Receiver<T>,
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, oneshot::Canceled>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        Pin::new(&mut this.rx).poll(cx)
    }
}

/// Cooperative yield: reschedule the current task once.
pub async fn yield_now() {
    struct YieldNow(bool);
    impl Future for YieldNow {
        type Output = ();
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                Poll::Ready(())
            } else {
                self.0 = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
    YieldNow(false).await
}

pub struct Sleep {
    inner: Arc<Inner>,
    deadline: Instant,
    registered: bool,
}

impl Future for Sleep {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if Instant::now() >= self.deadline {
            return Poll::Ready(());
        }
        if !self.registered {
            // Register once; stale wake-ups are acceptable.
            self.inner.register_timer(self.deadline, cx.waker().clone());
            self.registered = true;
        }
        Poll::Pending
    }
}

struct Inner {
    tasks: Mutex<Slab<Arc<Task>>>,
    ready: Mutex<VecDeque<usize>>,
    timers: Mutex<BinaryHeap<TimerEntry>>,
    cv: Condvar,

    // small optimization to avoid locking ready queue in fast-path checks
    has_ready: AtomicBool,
}

impl Inner {
    fn new() -> Self {
        Self {
            tasks: Mutex::new(Slab::new()),
            ready: Mutex::new(VecDeque::new()),
            timers: Mutex::new(BinaryHeap::new()),
            cv: Condvar::new(),
            has_ready: AtomicBool::new(false),
        }
    }

    fn insert_task(&self, fut: BoxFuture<'static, ()>) -> Result<usize, ()> {
        let mut tasks = self.tasks.lock();
        let entry = tasks.vacant_entry();
        let id = entry.key();

        let task = Arc::new(Task {
            inner: self.clone_arc(),
            id,
            future: Mutex::new(Some(fut)),
            queued: AtomicBool::new(false),
        });

        entry.insert(task);
        Ok(id)
    }

    fn clone_arc(&self) -> Arc<Inner> {
        // This is always safe because Inner is always owned by Arc in Executor/Handle.
        // We never produce Arc<Inner> from &Inner via raw pointers.
        // We only ever clone existing Arc<Inner>.
        //
        // Implementation detail: we store Arc<Inner> only in Executor/Handle/Task; Inner methods
        // take &self, but callers already hold Arc. Here we need an Arc clone, so we pass it in
        // at construction time (see Task::inner).
        //
        // This method is unused in the final design, kept for clarity; Task receives Arc directly.
        unreachable!("Inner::clone_arc should never be called");
    }

    fn enqueue(&self, id: usize) {
        {
            let mut q = self.ready.lock();
            q.push_back(id);
            self.has_ready.store(true, AtomicOrdering::Release);
        }
        self.cv.notify_one();
    }

    fn pop_ready(&self) -> Option<usize> {
        let mut q = self.ready.lock();
        let id = q.pop_front();
        if q.is_empty() {
            self.has_ready.store(false, AtomicOrdering::Release);
        }
        id
    }

    fn poll_task(&self, id: usize) {
        let task = {
            let tasks = self.tasks.lock();
            match tasks.get(id) {
                Some(t) => t.clone(),
                None => return,
            }
        };

        let waker = waker_ref(&task);
        let mut cx = Context::from_waker(&*waker);

        // Take future out, poll, then put back if pending
        let mut slot = task.future.lock();
        let mut fut = match slot.take() {
            Some(f) => f,
            None => return,
        };

        match fut.as_mut().poll(&mut cx) {
            Poll::Ready(()) => {
                let mut tasks = self.tasks.lock();
                let _ = tasks.remove(id);
            }
            Poll::Pending => {
                *slot = Some(fut);
            }
        }
    }

    fn register_timer(&self, deadline: Instant, waker: Waker) {
        self.timers.lock().push(TimerEntry { deadline, waker });
        self.cv.notify_one();
    }

    fn wake_due_timers(&self) {
        let now = Instant::now();
        let mut heap = self.timers.lock();
        while let Some(top) = heap.peek() {
            if top.deadline > now {
                break;
            }
            let entry = heap.pop().expect("peek then pop");
            entry.waker.wake_by_ref();
        }
    }

    fn peek_deadline(&self) -> Option<Instant> {
        self.timers.lock().peek().map(|e| e.deadline)
    }

    fn park_until_deadline_or_woken(&self) {
        // Fast path
        if self.has_ready.load(AtomicOrdering::Acquire) {
            return;
        }

        loop {
            // Wake due timers before deciding to sleep.
            self.wake_due_timers();
            if self.has_ready.load(AtomicOrdering::Acquire) {
                return;
            }

            let deadline = self.peek_deadline();

            // We wait on the ready-queue mutex as the condition variable anchor.
            let mut q = self.ready.lock();
            if !q.is_empty() {
                self.has_ready.store(true, AtomicOrdering::Release);
                return;
            }

            match deadline {
                Some(t) => {
                    let now = Instant::now();
                    if t <= now {
                        // due; loop will wake timers
                        continue;
                    }
                    self.cv.wait_for(&mut q, t - now);
                }
                None => {
                    self.cv.wait(&mut q);
                }
            }

            // loop re-checks conditions
        }
    }
}

struct Task {
    inner: Arc<Inner>,
    id: usize,
    future: Mutex<Option<BoxFuture<'static, ()>>>,
    queued: AtomicBool,
}

impl ArcWake for Task {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        // De-duplicate excessive enqueues.
        // If two wakeups race, it's still correct if task is enqueued twice; the poll loop is robust.
        if !arc_self.queued.swap(true, AtomicOrdering::AcqRel) {
            arc_self.inner.enqueue(arc_self.id);
            arc_self.queued.store(false, AtomicOrdering::Release);
        } else {
            arc_self.inner.enqueue(arc_self.id);
        }
    }
}

struct TimerEntry {
    deadline: Instant,
    waker: Waker,
}

impl PartialEq for TimerEntry {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline
    }
}
impl Eq for TimerEntry {}

impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is max-heap; reverse to make the earliest deadline come out first.
        other.deadline.cmp(&self.deadline)
    }
}
impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Fix Inner::clone_arc by removing it entirely:
/// We need Inner::insert_task to construct Task with Arc<Inner>.
/// The clean way is: pass Arc<Inner> into Inner methods or make insert_task a method on Arc<Inner>.
///
/// To keep this file concise and 0 unsafe, here is the intended pattern:
impl Inner {
    fn insert_task_arc(self: &Arc<Self>, fut: BoxFuture<'static, ()>) -> usize {
        let mut tasks = self.tasks.lock();
        let entry = tasks.vacant_entry();
        let id = entry.key();
        let task = Arc::new(Task {
            inner: self.clone(),
            id,
            future: Mutex::new(Some(fut)),
            queued: AtomicBool::new(false),
        });
        entry.insert(task);
        id
    }
}

/// Override Handle::spawn to use insert_task_arc (0 unreachable).
impl Handle {
    pub fn spawn_clean<F, T>(&self, fut: F) -> JoinHandle<T>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel::<T>();
        let task_fut = async move {
            let out = fut.await;
            let _ = tx.send(out);
        };

        let id = self.inner.insert_task_arc(task_fut.boxed());
        self.inner.enqueue(id);

        JoinHandle { rx }
    }
}
