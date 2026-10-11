use rfvp::host_api::{RfvpClock, WallTime};

pub struct WiiClock;

impl WiiClock {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for WiiClock {
    fn default() -> Self {
        Self::new()
    }
}

impl RfvpClock for WiiClock {
    fn ticks_us(&mut self) -> u64 {
        crate::ffi::ticks_us()
    }

    /// The console's local time, stamped on saves.
    fn wall_time(&mut self) -> Option<WallTime> {
        crate::ffi::wall_time()
    }
}
