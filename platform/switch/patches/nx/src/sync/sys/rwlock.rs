use core::sync::atomic::Ordering::*;

use super::futex::Futex;

// We're just going to steal the Rust stdlib implementation since it's a compatible license (MIT-apache dual license).
pub struct RwLock {
    state: Futex,
    writer_notifier: Futex,
}
const READ_LOCKED: u32 = 1;
const MASK: u32 = (1 << 30) - 1;
const WRITE_LOCKED: u32 = MASK;
const DOWNGRADE: u32 = READ_LOCKED.wrapping_sub(WRITE_LOCKED); // READ_LOCKED - WRITE_LOCKED
const MAX_READERS: u32 = MASK - 1;
const READERS_WAITING: u32 = 1 << 30;
const WRITERS_WAITING: u32 = 1 << 31;

#[inline]
fn is_unlocked(state: u32) -> bool {
    state & MASK == 0
}

#[inline]
fn is_write_locked(state: u32) -> bool {
    state & MASK == WRITE_LOCKED
}

#[inline]
fn has_readers_waiting(state: u32) -> bool {
    state & READERS_WAITING != 0
}

#[inline]
fn has_writers_waiting(state: u32) -> bool {
    state & WRITERS_WAITING != 0
}

impl RwLock {
    pub const fn new() -> Self {
        Self {
            state: Futex::new(),
            writer_notifier: Futex::new(),
        }
    }

    #[inline]
    pub fn try_read(&self) -> bool {
        self.state
            .0
            .fetch_update(Acquire, Relaxed, |s| {
                is_read_lockable(s).then(|| s + READ_LOCKED)
            })
            .is_ok()
    }

    #[inline]
    pub fn read(&self) {
        let state = self.state.0.load(Relaxed);
        if !is_read_lockable(state)
            || self
                .state
                .0
                .compare_exchange_weak(state, state + READ_LOCKED, Acquire, Relaxed)
                .is_err()
        {
            self.read_contended();
        }
    }

    /// # Safety
    ///
    /// The `RwLock` must be read-locked (N readers) in order to call this.
    #[inline]
    pub unsafe fn read_unlock(&self) {
        let state = self.state.0.fetch_sub(READ_LOCKED, Release) - READ_LOCKED;

        // It's impossible for a reader to be waiting on a read-locked RwLock,
        // except if there is also a writer waiting.
        debug_assert!(!has_readers_waiting(state) || has_writers_waiting(state));

        // Wake up a writer if we were the last reader and there's a writer waiting.
        if is_unlocked(state) && has_writers_waiting(state) {
            self.wake_writer_or_readers(state);
        }
    }

    #[cold]
    fn read_contended(&self) {
        let mut has_slept = false;
        let mut state = self.state.0.load(Relaxed);

        loop {
            // If we have just been woken up, first check for a `downgrade` call.
            // Otherwise, if we can read-lock it, lock it.
            if (has_slept && is_read_lockable_after_wakeup(state)) || is_read_lockable(state) {
                match self.state.0.compare_exchange_weak(
                    state,
                    state + READ_LOCKED,
                    Acquire,
                    Relaxed,
                ) {
                    Ok(_) => return, // Locked!
                    Err(s) => {
                        state = s;
                        continue;
                    }
                }
            }

            // Check for overflow.
            assert!(
                !has_reached_max_readers(state),
                "too many active read locks on RwLock"
            );

            // Make sure the readers waiting bit is set before we go to sleep.
            if !has_readers_waiting(state)
                && let Err(s) =
                    self.state
                        .0
                        .compare_exchange(state, state | READERS_WAITING, Relaxed, Relaxed)
            {
                state = s;
                continue;
            }

            // Wait for the state to change.
            self.state.wait(state | READERS_WAITING, -1);
            has_slept = true;

            // Read again after waking up.
            state = self.state.0.load(Relaxed);
        }
    }

    fn wake_writer(&self) {
        self.writer_notifier.0.fetch_add(1, Release);
        self.writer_notifier.signal_one()
    }

    #[inline]
    pub fn try_write(&self) -> bool {
        self.state
            .0
            .fetch_update(Acquire, Relaxed, |s| {
                is_unlocked(s).then(|| s + WRITE_LOCKED)
            })
            .is_ok()
    }

    #[inline]
    pub fn write(&self) {
        if self
            .state
            .0
            .compare_exchange_weak(0, WRITE_LOCKED, Acquire, Relaxed)
            .is_err()
        {
            self.write_contended();
        }
    }

    /// # Safety
    ///
    /// The `RwLock` must be write-locked (single writer) in order to call this.
    #[inline]
    pub unsafe fn write_unlock(&self) {
        let state = self.state.0.fetch_sub(WRITE_LOCKED, Release) - WRITE_LOCKED;

        debug_assert!(is_unlocked(state));

        if has_writers_waiting(state) || has_readers_waiting(state) {
            self.wake_writer_or_readers(state);
        }
    }

    /// # Safety
    ///
    /// The `RwLock` must be write-locked (single writer) in order to call this.
    #[inline]
    pub unsafe fn downgrade(&self) {
        // Removes all write bits and adds a single read bit.
        let state = self.state.0.fetch_add(DOWNGRADE, Release);
        debug_assert!(
            is_write_locked(state),
            "RwLock must be write locked to call `downgrade`"
        );

        if has_readers_waiting(state) {
            // Since we had the exclusive lock, nobody else can unset this bit.
            self.state.0.fetch_sub(READERS_WAITING, Relaxed);
            self.state.signal_all();
        }
    }

    #[cold]
    fn write_contended(&self) {
        let mut state = self.state.0.load(Relaxed);

        let mut other_writers_waiting = 0;

        loop {
            // If it's unlocked, we try to lock it.
            if is_unlocked(state) {
                match self.state.0.compare_exchange_weak(
                    state,
                    state | WRITE_LOCKED | other_writers_waiting,
                    Acquire,
                    Relaxed,
                ) {
                    Ok(_) => return, // Locked!
                    Err(s) => {
                        state = s;
                        continue;
                    }
                }
            }

            // Set the waiting bit indicating that we're waiting on it.
            if !has_writers_waiting(state)
                && let Err(s) =
                    self.state
                        .0
                        .compare_exchange(state, state | WRITERS_WAITING, Relaxed, Relaxed)
            {
                state = s;
                continue;
            }

            // Other writers might be waiting now too, so we should make sure
            // we keep that bit on once we manage lock it.
            other_writers_waiting = WRITERS_WAITING;

            // Examine the notification counter before we check if `state` has changed,
            // to make sure we don't miss any notifications.
            let seq = self.writer_notifier.0.load(Acquire);

            // Don't go to sleep if the lock has become available,
            // or if the writers waiting bit is no longer set.
            state = self.state.0.load(Relaxed);
            if is_unlocked(state) || !has_writers_waiting(state) {
                continue;
            }

            // Wait for the state to change.
            self.writer_notifier.wait(seq, -1);

            // Read again after waking up.
            state = self.state.0.load(Relaxed);
        }
    }

    /// Wakes up waiting threads after unlocking.
    ///
    /// If both are waiting, this will wake up only one writer, but will fall
    /// back to waking up readers if there was no writer to wake up.
    #[cold]
    fn wake_writer_or_readers(&self, mut state: u32) {
        assert!(is_unlocked(state));

        // The readers waiting bit might be turned on at any point now,
        // since readers will block when there's anything waiting.
        // Writers will just lock the lock though, regardless of the waiting bits,
        // so we don't have to worry about the writer waiting bit.
        //
        // If the lock gets locked in the meantime, we don't have to do
        // anything, because then the thread that locked the lock will take
        // care of waking up waiters when it unlocks.

        // If only writers are waiting, wake one of them up.
        if state == WRITERS_WAITING {
            match self.state.0.compare_exchange(state, 0, Relaxed, Relaxed) {
                Ok(_) => {
                    self.wake_writer();
                    return;
                }
                Err(s) => {
                    // Maybe some readers are now waiting too. So, continue to the next `if`.
                    state = s;
                }
            }
        }

        // If both writers and readers are waiting, leave the readers waiting
        // and only wake up one writer.
        if state == READERS_WAITING + WRITERS_WAITING {
            if self
                .state
                .0
                .compare_exchange(state, READERS_WAITING, Relaxed, Relaxed)
                .is_err()
            {
                // The lock got locked. Not our problem anymore.
                return;
            }
            self.wake_writer();
            // No writers were actually blocked on futex_wait, so we continue
            // to wake up readers instead, since we can't be sure if we notified a writer.
            state = READERS_WAITING;
        }

        // If readers are waiting, wake them all up.
        if state == READERS_WAITING
            && self
                .state
                .0
                .compare_exchange(state, 0, Relaxed, Relaxed)
                .is_ok()
        {
            self.state.signal_all();
        }
    }
}

#[inline]
fn is_read_lockable(state: u32) -> bool {
    // This also returns false if the counter could overflow if we tried to read lock it.
    //
    // We don't allow read-locking if there's readers waiting, even if the lock is unlocked
    // and there's no writers waiting. The only situation when this happens is after unlocking,
    // at which point the unlocking thread might be waking up writers, which have priority over readers.
    // The unlocking thread will clear the readers waiting bit and wake up readers, if necessary.
    state & MASK < MAX_READERS && !has_readers_waiting(state) && !has_writers_waiting(state)
}

#[inline]
fn is_read_lockable_after_wakeup(state: u32) -> bool {
    // We make a special case for checking if we can read-lock _after_ a reader thread that went to
    // sleep has been woken up by a call to `downgrade`.
    //
    // `downgrade` will wake up all readers and place the lock in read mode. Thus, there should be
    // no readers waiting and the lock should be read-locked (not write-locked or unlocked).
    //
    // Note that we do not check if any writers are waiting. This is because a call to `downgrade`
    // implies that the caller wants other readers to read the value protected by the lock. If we
    // did not allow readers to acquire the lock before writers after a `downgrade`, then only the
    // original writer would be able to read the value, thus defeating the purpose of `downgrade`.
    state & MASK < MAX_READERS
        && !has_readers_waiting(state)
        && !is_write_locked(state)
        && !is_unlocked(state)
}

#[inline]
fn has_reached_max_readers(state: u32) -> bool {
    state & MASK == MAX_READERS
}

impl Default for RwLock {
    fn default() -> Self {
        Self::new()
    }
}
