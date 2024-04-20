mod tween;
mod tweener;

use std::{
    fmt::{Debug, Display},
    ops::Div,
    time::Duration,
};

use derive_more::{Add, AddAssign, Sub, SubAssign};
use float_ord::FloatOrd;
use tracing::warn;
pub use tween::{Easing, Tween};
pub use tweener::Tweener;


#[derive(
    Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable, Add, AddAssign, Sub, SubAssign,
)]
#[repr(transparent)]
pub struct Ticks(f32);

const TICKS_PER_SECOND: f32 = 60.0;

impl Ticks {
    pub const ZERO: Self = Self(0.0);
    pub const TICKS_PER_SECOND: f32 = TICKS_PER_SECOND;

    pub const fn from_f32(ticks: f32) -> Self {
        Self(ticks)
    }

    pub const fn from_u32(ticks: u32) -> Self {
        Self(ticks as f32)
    }
    pub fn from_i32(ticks: i32) -> Self {
        if ticks < 0 {
            warn!("Ticks::from_i32: negative value: {}", ticks);
        }
        Self(ticks.clamp(0, i32::MAX) as f32)
    }

    pub fn from_seconds(seconds: f32) -> Self {
        Self(seconds * TICKS_PER_SECOND)
    }

    pub fn from_millis(millis: f32) -> Self {
        Self::from_seconds(millis / 1000.0)
    }

    pub fn from_duration(duration: Duration) -> Self {
        Self::from_seconds(duration.as_secs_f32())
    }

    pub fn as_f32(&self) -> f32 {
        self.0
    }

    pub fn as_seconds(&self) -> f32 {
        self.0 / TICKS_PER_SECOND
    }

    pub fn as_duration(&self) -> Duration {
        Duration::from_secs_f32(self.as_seconds())
    }
}

impl Div for Ticks {
    type Output = f32;

    fn div(self, rhs: Self) -> Self::Output {
        self.0 / rhs.0
    }
}

impl PartialEq for Ticks {
    fn eq(&self, other: &Self) -> bool {
        FloatOrd(self.0).eq(&FloatOrd(other.0))
    }
}

impl Eq for Ticks {}

impl PartialOrd for Ticks {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        FloatOrd(self.0).partial_cmp(&FloatOrd(other.0))
    }
}

impl Ord for Ticks {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        FloatOrd(self.0).cmp(&FloatOrd(other.0))
    }
}

impl Debug for Ticks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.01}t", self.0)
    }
}

impl Display for Ticks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

