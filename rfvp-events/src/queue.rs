use std::collections::VecDeque;
use std::sync::Mutex;

use crate::Notify;

#[derive(Default)]
pub struct EventQueue<E> {
    q: Mutex<VecDeque<E>>,
    notify: Notify,
}

impl<E> EventQueue<E> {
    pub fn new() -> Self {
        Self {
            q: Mutex::new(VecDeque::new()),
            notify: Notify::default(),
        }
    }

    pub fn push(&self, ev: E) {
        {
            let mut q = self.q.lock().unwrap();
            q.push_back(ev);
        }
        self.notify.notify_waiters();
    }

    pub fn pop(&self) -> Option<E> {
        self.q.lock().unwrap().pop_front()
    }

    pub fn drain(&self) -> Vec<E> {
        let mut q = self.q.lock().unwrap();
        q.drain(..).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.q.lock().unwrap().is_empty()
    }

    pub async fn wait_nonempty(&self) {
        loop {
            if !self.is_empty() {
                return;
            }
            self.notify.notified().await;
        }
    }

    pub async fn next(&self) -> E {
        loop {
            if let Some(ev) = self.pop() {
                return ev;
            }
            self.notify.notified().await;
        }
    }

    pub fn notifier(&self) -> &Notify {
        &self.notify
    }
}
