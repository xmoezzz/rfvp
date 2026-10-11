use rfvp::host_api::{RfvpClock, WallTime};

pub struct Ps2Clock;

impl Ps2Clock {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for Ps2Clock {
    fn default() -> Self {
        Self::new()
    }
}

impl RfvpClock for Ps2Clock {
    fn ticks_us(&mut self) -> u64 {
        crate::ffi::ticks_us()
    }

    /// The console's local time, stamped on saves.
    fn wall_time(&mut self) -> Option<WallTime> {
        crate::ffi::wall_time()
    }
}
