use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

#[derive(Default)]
pub struct Notify {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    gen: u64,
    wakers: Vec<Waker>,
}

impl Notify {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn notify_waiters(&self) {
        let wakers = {
            let mut inner = self.inner.lock().unwrap();
            inner.gen = inner.gen.wrapping_add(1);
            std::mem::take(&mut inner.wakers)
        };
        for w in wakers {
            w.wake();
        }
    }

    pub fn notified(&self) -> Notified<'_> {
        let gen = self.inner.lock().unwrap().gen;
        Notified {
            notify: self,
            target_gen: gen.wrapping_add(1),
            done: false,
        }
    }

    fn register(&self, waker: &Waker, target_gen: u64) -> Poll<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.gen.wrapping_sub(target_gen) as i64 >= 0 {
            return Poll::Ready(());
        }

        if let Some(slot) = inner.wakers.iter_mut().find(|w| w.will_wake(waker)) {
            *slot = waker.clone();
        } else {
            inner.wakers.push(waker.clone());
        }
        Poll::Pending
    }
}

pub struct Notified<'a> {
    notify: &'a Notify,
    target_gen: u64,
    done: bool,
}

impl Future for Notified<'_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.done {
            return Poll::Ready(());
        }
        match self.notify.register(cx.waker(), self.target_gen) {
            Poll::Ready(()) => {
                self.done = true;
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
