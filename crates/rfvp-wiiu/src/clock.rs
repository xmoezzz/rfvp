use rfvp::host_api::RfvpClock;

unsafe extern "C" {
    fn rfvp_wiiu_platform_ticks_us() -> u64;
}

pub struct WiiUClock;

impl WiiUClock {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for WiiUClock {
    fn default() -> Self {
        Self::new()
    }
}

impl RfvpClock for WiiUClock {
    fn ticks_us(&mut self) -> u64 {
        unsafe { rfvp_wiiu_platform_ticks_us() }
    }
}
