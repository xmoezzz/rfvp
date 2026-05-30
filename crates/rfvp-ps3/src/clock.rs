use rfvp::host_api::RfvpClock;

unsafe extern "C" {
    fn rfvp_ps3_platform_ticks_us() -> u64;
}

pub struct PS3Clock;

impl PS3Clock {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for PS3Clock {
    fn default() -> Self {
        Self::new()
    }
}

impl RfvpClock for PS3Clock {
    fn ticks_us(&mut self) -> u64 {
        unsafe { rfvp_ps3_platform_ticks_us() }
    }
}
