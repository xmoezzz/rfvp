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
        // `nx::arm::get_system_tick_as_nanos` divides by zero on its first call, so convert the
        // system tick here. The multiplication is done in 128 bits: the counter is large.
        let ticks = nx::arm::get_system_tick() as u128;
        let frequency = nx::arm::get_system_tick_frequency() as u128;
        (ticks * 1_000_000 / frequency.max(1)) as u64
    }
}
