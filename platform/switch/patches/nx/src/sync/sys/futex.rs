use core::{
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{
    result::ResultBase,
    svc::{self, rc::ResultTimedOut},
};

#[repr(transparent)]
pub struct Futex(pub(super) AtomicU32);

impl Futex {
    pub const fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    pub fn wait(&self, expected: u32, timeout: i64) -> bool {
        // No need to wait if the value already changed.
        if self.0.load(Ordering::Relaxed) != expected {
            return true;
        }

        match unsafe {
            svc::wait_for_address(
                ptr::from_ref(self).cast(),
                svc::ArbitrationType::WaitIfEqual,
                expected,
                timeout,
            )
        } {
            Ok(_) => true,
            Err(rc) if ResultTimedOut::matches(rc) => false,
            Err(rc) => {
                panic!(
                    "Error waiting for valid pointer at {:#X}, err : {}-{}",
                    ptr::from_ref(self).addr(),
                    rc.get_module(),
                    rc.get_value()
                );
            }
        }
    }

    #[inline(always)]
    pub fn signal_one(&self) {
        self.signal_n(1);
    }

    #[inline(always)]
    pub fn signal_all(&self) {
        self.signal_n(-1);
    }

    pub fn signal_n(&self, count: i32) {
        if let Err(rc) = unsafe {
            svc::signal_to_address(
                ptr::from_ref(self).cast(),
                svc::SignalType::Signal,
                0,
                count,
            )
        } {
            panic!(
                "Error signaling for valid pointer at {:#X}, err : {}-{}",
                ptr::from_ref(self).addr(),
                rc.get_module(),
                rc.get_value()
            )
        }
    }
}

impl Default for Futex {
    fn default() -> Self {
        Self::new()
    }
}
