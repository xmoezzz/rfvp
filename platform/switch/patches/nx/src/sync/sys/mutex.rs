use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering::*;

//use super::*;

use crate::diag::abort;
use crate::svc;

use super::get_current_thread_handle;

const WAIT_MASK: u32 = 0x40_00_00_00;

pub struct Mutex {
    owner_thread_handle: AtomicU32,
}

impl Mutex {
    #[inline]
    pub const fn new() -> Self {
        Self {
            owner_thread_handle: AtomicU32::new(svc::INVALID_HANDLE),
        }
    }

    #[inline]
    pub fn is_locked(&self) -> bool {
        (self.owner_thread_handle.load(Relaxed) & !WAIT_MASK) != svc::INVALID_HANDLE
    }

    #[inline]
    pub fn try_lock(&self) -> bool {
        self.owner_thread_handle
            .compare_exchange(
                svc::INVALID_HANDLE,
                get_current_thread_handle(),
                Acquire,
                Relaxed,
            )
            .is_ok()
    }

    #[inline]
    pub fn lock(&self) {
        let current_thread = get_current_thread_handle();
        let _current_val = self.owner_thread_handle.load(Relaxed);
        if let Err(state) = self.owner_thread_handle.compare_exchange(
            svc::INVALID_HANDLE,
            current_thread,
            Acquire,
            Relaxed,
        ) {
            self.lock_contended(state, current_thread);
        }
    }

    #[cold]
    fn lock_contended(&self, mut state: u32, current_thread: u32) {
        loop {
            // If the state isn't marked as contended yet, mark it
            if state & WAIT_MASK == 0 {
                // check if we have just caught an unlocked mutex while setting the wait flag
                // this should only happen when an uncontended mutex unlocks, so we can just try to
                // lock without worrying about the kernel giving it to a waiter
                if self.owner_thread_handle.fetch_or(WAIT_MASK, Acquire) == svc::INVALID_HANDLE {
                    // we have found an unlocked mutex by chance, try to lock it
                    match self.owner_thread_handle.compare_exchange(
                        svc::INVALID_HANDLE | WAIT_MASK,
                        current_thread,
                        Acquire,
                        Relaxed,
                    ) {
                        Ok(_) => {
                            // we locked by replacing our written wait mask with the new handle value
                            return;
                        }
                        Err(s) => {
                            state = s;
                        }
                    }
                }

                // we didn't luck into a lock, and the value is not flagged for aribitration.
                // we can wait for the kernel to hand us the value
                if let Err(rc) = unsafe {
                    svc::arbitrate_lock(
                        state & !WAIT_MASK,
                        &self.owner_thread_handle as *const _ as _,
                        current_thread,
                    )
                } {
                    abort::abort(abort::AbortLevel::Panic(), rc);
                }

                // we should have the value here, but libnx and nnSdk check
                state = self.owner_thread_handle.load(Acquire);
                if state & !WAIT_MASK == current_thread {
                    return;
                }
            }
        }
    }

    /// # Safety
    ///
    /// This can only be called on a mutex that has been locked by the owner/borrower as it will directly overwrite
    /// the value without checking if it was already
    #[inline]
    pub unsafe fn unlock(&self) {
        let state = self.owner_thread_handle.swap(svc::INVALID_HANDLE, Acquire);

        //debug_assert!(state & !WAIT_MASK == 0, "Tried to unlock and unlocked mutex");

        // we're actually not going to check the assert below, as we want to support having a Mutex that is Sync,
        // and/or a MutexGuard that is Sync/Send
        // assert_eq!(current_thread, state & !WAIT_MASK, "We are unlocking a mutex that isn't owned by the current thread");

        if state & WAIT_MASK != 0
            && let Err(rc) =
                unsafe { svc::arbitrate_unlock(&self.owner_thread_handle as *const _ as _) }
        {
            abort::abort(abort::AbortLevel::Panic(), rc);
        }
    }
}

impl Default for Mutex {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(clippy::declare_interior_mutable_const)]
unsafe impl lock_api::RawMutex for Mutex {
    // mark that a mutex guard can be Send
    type GuardMarker = lock_api::GuardSend;

    // const initializer for the Mutex
    const INIT: Self = Self::new();

    fn is_locked(&self) -> bool {
        self.is_locked()
    }

    fn lock(&self) {
        self.lock()
    }

    fn try_lock(&self) -> bool {
        self.try_lock()
    }

    unsafe fn unlock(&self) {
        unsafe { self.unlock() }
    }
}
