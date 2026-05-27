use rfvp::host_api::RfvpClock;

pub struct HorizonClock;

impl HorizonClock {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for HorizonClock {
    fn default() -> Self {
        Self::new()
    }
}

impl RfvpClock for HorizonClock {
    fn ticks_us(&mut self) -> u64 {
        nx::arm::get_system_tick_as_nanos() / 1_000
    }
}
