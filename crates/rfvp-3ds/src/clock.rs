use core::ffi::c_void;

use rfvp::host_api::RfvpClock;

use crate::raw::RawClockVTable;

pub struct ThreeDsClock {
    ctx: *mut c_void,
    vtable: RawClockVTable,
}

impl ThreeDsClock {
    pub const fn new(ctx: *mut c_void, vtable: RawClockVTable) -> Self {
        Self { ctx, vtable }
    }
}

impl RfvpClock for ThreeDsClock {
    fn ticks_us(&mut self) -> u64 {
        unsafe { (self.vtable.ticks_us)(self.ctx) }
    }
}
