/// A calendar date and time of day, as shown next to a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    /// 0 for Sunday.
    pub day_of_week: u8,
    pub hour: u8,
    pub minute: u8,
}

pub trait RfvpClock {
    fn ticks_us(&mut self) -> u64;

    /// The local date and time, if the host knows it.
    fn wall_time(&mut self) -> Option<WallTime> {
        None
    }
}
