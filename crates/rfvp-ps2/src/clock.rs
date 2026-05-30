use core::ffi::c_void;

use rfvp::host_api::RfvpClock;

use crate::raw::RawClockVTable;

pub struct Ps2Clock {
    ctx: *mut c_void,
    vtable: RawClockVTable,
}

impl Ps2Clock {
    pub const fn new(ctx: *mut c_void, vtable: RawClockVTable) -> Self {
        Self { ctx, vtable }
    }
}

impl RfvpClock for Ps2Clock {
    fn ticks_us(&mut self) -> u64 {
        unsafe { (self.vtable.ticks_us)(self.ctx) }
    }
}
