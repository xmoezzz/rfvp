// SPDX-License-Identifier: MPL-2.0
//! A legacy static name with one owned value per fixed session thread.
use alloc::rc::Rc;
use core::{
    cell::RefCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};
use host_std::thread::LocalKey;
pub struct SessionMutex<T: 'static>(&'static LocalKey<RefCell<Option<T>>>);
impl<T> SessionMutex<T> {
    pub const fn new(key: &'static LocalKey<RefCell<Option<T>>>) -> Self {
        Self(key)
    }
    pub fn lock(&self) -> Result<SessionGuard<T>, ()> {
        self.0.with(|s| {
            s.borrow_mut()
                .take()
                .map(|value| SessionGuard {
                    key: self.0,
                    value: Some(value),
                    owner: PhantomData,
                })
                .ok_or(())
        })
    }
}
pub struct SessionGuard<T: 'static> {
    key: &'static LocalKey<RefCell<Option<T>>>,
    value: Option<T>,
    owner: PhantomData<Rc<()>>,
}
impl<T> Deref for SessionGuard<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value.as_ref().unwrap()
    }
}
impl<T> DerefMut for SessionGuard<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value.as_mut().unwrap()
    }
}
impl<T> Drop for SessionGuard<T> {
    fn drop(&mut self) {
        self.key.with(|s| *s.borrow_mut() = self.value.take());
    }
}
