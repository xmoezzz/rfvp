//use core::arch::asm;

use crate::thread;

pub mod futex;
pub mod mutex;
pub mod rwlock;

#[inline(always)]
fn get_current_thread_handle() -> u32 {
    unsafe { (*thread::get_thread_local_region()).nx_thread_vars.handle }
}
