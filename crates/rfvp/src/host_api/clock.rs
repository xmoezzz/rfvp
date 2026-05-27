pub trait RfvpClock {
    fn ticks_us(&mut self) -> u64;
}
