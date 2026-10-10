//! Synchronization support and utils

use core::cell::UnsafeCell;

pub mod sys;
use sys::mutex::Mutex as RawMutex;
use sys::rwlock::RwLock as RawRwLock;

#[macro_export]
macro_rules! acquire {
    ($x:expr) => {
        atomic::fence(Acquire)
    };
}

/// Represents a type which will lock a given [`Mutex`] on creation and unlock it on destruction, effectively guarding it
pub struct ScopedLock<'a> {
    lock: &'a mut RawMutex,
}

impl<'a> ScopedLock<'a> {
    /// Creates a new [`ScopedLock`] for a given [`Mutex`]
    ///
    /// # Arguments
    ///
    /// * `lock`: The [`Mutex`] to guard
    pub fn new(lock: &'a mut RawMutex) -> Self {
        lock.lock();
        Self { lock }
    }
}

impl Drop for ScopedLock<'_> {
    /// Unlocks the [`Mutex`] as the [`ScopedLock`] is destroyed (likely out of scope)
    fn drop(&mut self) {
        // SAFETY: variant upheld that the lock should actually be locked
        unsafe { self.lock.unlock() };
    }
}

//////////// MUTEX

/// Represents a value whose access is controlled by an inner [`Mutex`]
pub struct Mutex<T: ?Sized> {
    pub(self) raw_lock: RawMutex,
    pub(self) object_cell: UnsafeCell<T>,
}

impl<T> Mutex<T> {
    pub fn is_locked(&self) -> bool {
        self.raw_lock.is_locked()
    }
    /// Creates a new [`RwLock`] with a value
    ///
    /// # Arguments
    ///
    /// * `is_recursive`: Whether the inner [`Mutex`] is recursive
    /// * `t`: The value to store
    #[inline]
    pub const fn new(t: T) -> Self {
        Self {
            raw_lock: RawMutex::new(),
            object_cell: UnsafeCell::new(t),
        }
    }

    /// Sets a value, doing a lock-unlock operation in the process
    pub fn set(&self, t: T) {
        unsafe {
            self.raw_lock.lock();
            let _to_drop = core::mem::replace(self.object_cell.get().as_mut().unwrap(), t);
            self.raw_lock.unlock();
        }
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Locks the Mutex and returns a guarded reference to the inner value
    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.raw_lock.lock();
        MutexGuard { lock: self }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if self.raw_lock.try_lock() {
            Some(MutexGuard { lock: self })
        } else {
            None
        }
    }
}

impl<T: Copy> Mutex<T> {
    /// Gets a copy of the value, doing a lock-unlock operation in the process
    pub fn get_val(&self) -> T {
        unsafe {
            self.raw_lock.lock();
            let obj_copy = *self.object_cell.get();
            self.raw_lock.unlock();
            obj_copy
        }
    }
}

// we only have a bound on Sync instead of Send, because we don't implement into_inner
unsafe impl<T: ?Sized + Sync> Sync for Mutex<T> {}
unsafe impl<T: ?Sized + Sync> Send for Mutex<T> {}

pub struct MutexGuard<'borrow, T: ?Sized> {
    pub(self) lock: &'borrow Mutex<T>,
}

unsafe impl<T: ?Sized + Sync> Sync for MutexGuard<'_, T> {}

impl<'borrow, T: ?Sized> MutexGuard<'borrow, T> {
    pub fn new(lock: &'borrow Mutex<T>) -> Self {
        lock.raw_lock.lock();
        Self { lock }
    }
}

impl<T: ?Sized> core::ops::Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { self.lock.object_cell.get().as_ref().unwrap_unchecked() }
    }
}

impl<T: ?Sized> core::ops::DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe {
            self.lock
                .object_cell
                .get()
                .as_mut()
                // We know the pointer is valid as we have a valid ref to the parent
                .unwrap()
        }
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        unsafe { self.lock.raw_lock.unlock() };
    }
}
//////////// MUTEX

//////////// RWLOCK

pub struct RwLock<T: ?Sized> {
    pub(self) raw_lock: RawRwLock,
    pub(self) object_cell: UnsafeCell<T>,
}

impl<T: ?Sized> core::fmt::Debug for RwLock<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("RwLock<")?;
        f.write_str(core::any::type_name::<T>())?;
        f.write_str(">(...)")
    }
}

impl<T> RwLock<T> {
    /// Creates a new [`RwLock`] with a value
    ///
    /// # Arguments
    ///
    /// * `is_recursive`: Whether the inner [`Mutex`] is recursive
    /// * `t`: The value to store
    #[inline]
    pub const fn new(t: T) -> Self {
        Self {
            raw_lock: RawRwLock::new(),
            object_cell: UnsafeCell::new(t),
        }
    }

    /// Sets a value, doing a lock-unlock operation in the process
    pub fn set(&mut self, t: T) {
        unsafe {
            self.raw_lock.write();
            self.object_cell = UnsafeCell::new(t);
            self.raw_lock.write_unlock();
        }
    }

    /// Locks the value for writing and returns a guarded reference to the inner value
    pub fn write(&self) -> WriteGuard<'_, T> {
        self.raw_lock.write();
        WriteGuard { lock: self }
    }

    /// Locks the value for reading and returns a guarded reference to the inner value
    pub fn read(&self) -> ReadGuard<'_, T> {
        self.raw_lock.read();
        ReadGuard { lock: self }
    }
}

impl<T: Copy> RwLock<T> {
    /// Gets a copy of the value, doing a lock-unlock operation in the process
    pub fn get_val(&self) -> T {
        unsafe {
            self.raw_lock.read();
            let obj_copy = *self.object_cell.get();
            self.raw_lock.read_unlock();
            obj_copy
        }
    }
}
unsafe impl<T: ?Sized + Send> Sync for RwLock<T> {}
unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}

pub struct ReadGuard<'borrow, T: ?Sized> {
    pub(self) lock: &'borrow RwLock<T>,
}

pub struct WriteGuard<'borrow, T: ?Sized> {
    pub(self) lock: &'borrow RwLock<T>,
}

unsafe impl<T: ?Sized + Sync> Sync for ReadGuard<'_, T> {}
unsafe impl<T: ?Sized + Sync> Sync for WriteGuard<'_, T> {}

impl<'borrow, T: ?Sized> ReadGuard<'borrow, T> {
    pub fn new(lock: &'borrow RwLock<T>) -> Self {
        lock.raw_lock.read();
        Self { lock }
    }
}

impl<T> core::ops::Deref for ReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.object_cell.get() }
    }
}

impl<T> core::ops::Deref for WriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.object_cell.get() }
    }
}

impl<T> core::ops::DerefMut for WriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.object_cell.get() }
    }
}

impl<T: ?Sized> Drop for ReadGuard<'_, T> {
    fn drop(&mut self) {
        unsafe { self.lock.raw_lock.read_unlock() };
    }
}

impl<T: ?Sized> Drop for WriteGuard<'_, T> {
    fn drop(&mut self) {
        unsafe { self.lock.raw_lock.write_unlock() };
    }
}
