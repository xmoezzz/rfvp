use core::ffi::c_void;

use rfvp::host_api::{RfvpClock, WallTime};

use crate::raw::RawClockVTable;

pub struct PsvClock {
    ctx: *mut c_void,
    vtable: RawClockVTable,
}

impl PsvClock {
    pub const fn new(ctx: *mut c_void, vtable: RawClockVTable) -> Self {
        Self { ctx, vtable }
    }
}

extern "C" {
    /// year, month, day, day of week (Sunday = 0), hour, minute; 0 on success.
    fn rfvp_psv_vitasdk_wall_time(out: *mut u16) -> i32;
}

impl RfvpClock for PsvClock {
    fn ticks_us(&mut self) -> u64 {
        unsafe { (self.vtable.ticks_us)(self.ctx) }
    }

    fn wall_time(&mut self) -> Option<WallTime> {
        let mut parts = [0u16; 6];
        if unsafe { rfvp_psv_vitasdk_wall_time(parts.as_mut_ptr()) } != 0 {
            return None;
        }
        Some(WallTime {
            year: parts[0],
            month: parts[1] as u8,
            day: parts[2] as u8,
            day_of_week: parts[3] as u8,
            hour: parts[4] as u8,
            minute: parts[5] as u8,
        })
    }
}
