use rfvp::host_api::{RfvpClock, WallTime};

pub struct Ps1Clock;

impl Ps1Clock {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for Ps1Clock {
    fn default() -> Self {
        Self::new()
    }
}

impl RfvpClock for Ps1Clock {
    fn ticks_us(&mut self) -> u64 {
        crate::ffi::ticks_us()
    }

    /// The PlayStation has no clock: saves carry no date.
    fn wall_time(&mut self) -> Option<WallTime> {
        None
    }
}
