//! A lock for the PSP's threads.
//!
//! The PSP has one core and runs the highest-priority ready thread, so a spin lock deadlocks as
//! soon as the audio thread (above the game's priority) spins on a lock the game thread holds.
//! Polling with short sleeps works but wakes the waiter thousands of times while the holder
//! does something long (opening a song). This one takes the lock with an atomic when it is free
//! and otherwise sleeps on a kernel semaphore until the holder lets go, like a futex.

use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

use psp::sys;

const UNLOCKED: u32 = 0;
const LOCKED: u32 = 1;
/// Locked, and a thread may be asleep waiting for it.
const CONTENDED: u32 = 2;

pub struct YieldingMutex {
    state: AtomicU32,
    /// The semaphore waiters sleep on, made the first time one has to (0: none yet).
    sema: AtomicI32,
}

impl YieldingMutex {
    fn sema(&self) -> sys::SceUid {
        let id = self.sema.load(Ordering::Acquire);
        if id > 0 {
            return sys::SceUid(id);
        }
        let made = unsafe {
            sys::sceKernelCreateSema(b"rfvp_lock\0".as_ptr(), 0, 0, i32::MAX, core::ptr::null_mut())
        };
        if made.0 <= 0 {
            return made;
        }
        match self
            .sema
            .compare_exchange(0, made.0, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => made,
            Err(theirs) => {
                // Another thread made one first.
                unsafe { sys::sceKernelDeleteSema(made) };
                sys::SceUid(theirs)
            }
        }
    }
}

unsafe impl lock_api::RawMutex for YieldingMutex {
    #[allow(clippy::declare_interior_mutable_const)]
    const INIT: Self = Self {
        state: AtomicU32::new(UNLOCKED),
        sema: AtomicI32::new(0),
    };

    type GuardMarker = lock_api::GuardSend;

    fn lock(&self) {
        if self.try_lock() {
            return;
        }
        // The semaphore must exist before the lock is marked contended: the holder only signals
        // one it can see, and a wakeup sent before it existed would be lost.
        let sema = self.sema();
        // Mark the lock contended; whoever holds it signals the semaphore on unlock.
        while self.state.swap(CONTENDED, Ordering::Acquire) != UNLOCKED {
            if sema.0 > 0 {
                unsafe { sys::sceKernelWaitSema(sema, 1, core::ptr::null_mut()) };
            } else {
                // No semaphore (out of kernel objects): poll instead.
                unsafe { sys::sceKernelDelayThread(100) };
            }
        }
    }

    fn try_lock(&self) -> bool {
        self.state
            .compare_exchange(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    unsafe fn unlock(&self) {
        if self.state.swap(UNLOCKED, Ordering::Release) == CONTENDED {
            let sema = self.sema.load(Ordering::Acquire);
            if sema > 0 {
                sys::sceKernelSignalSema(sys::SceUid(sema), 1);
            }
        }
    }
}

pub type Mutex<T> = lock_api::Mutex<YieldingMutex, T>;
