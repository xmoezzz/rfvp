use std::f32::consts::PI;

use crate::time::Ticks;

/// Curves the motion of a [`Tween`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    /// Maintains a constant speed for the duration of the [`Tween`].
    Linear,
    /// Causes the [`Tween`] to start slow and speed up.
    SineIn,
    /// Causes the [`Tween`] to start fast and slow down.
    SineOut,
    /// Causes the [`Tween`] to start slow, speed up, and then slow back down.
    SineInOut,

    /// Causes the [`Tween`] to hold its value for the duration
    ///   of the [`Tween`] and then jump to the end value.
    Jump,

    /// TODO: document
    /// This is some weird one, it uses power functions instead of sine/cosine
    Power(i32),
}

const HALF_PI: f32 = PI / 2.0;

impl Easing {
    fn apply(&self, x: f32) -> f32 {
        match *self {
            Easing::Linear => x,
            Easing::SineIn => 1.0 - (x * HALF_PI).cos(),
            Easing::SineOut => (x * HALF_PI).sin(),
            Easing::SineInOut => (1.0 - (PI * x).cos()) / 2.0,
            Easing::Jump => {
                if x < 1.0 {
                    0.0
                } else {
                    1.0
                }
            }
            Easing::Power(power) => {
                if power > 0 {
                    x.powi(power)
                } else if power != 0 {
                    1.0 - (1.0 - x).powi(-power)
                } else {
                    x
                }
            }
        }
    }
}

/// Describes a smooth transition between values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tween {
    /// The duration of the motion.
    pub duration: Ticks,
    /// The curve of the motion.
    pub easing: Easing,
}

impl Tween {
    pub const IMMEDIATE: Self = Self {
        duration: Ticks::ZERO,
        easing: Easing::Linear,
    };

    pub const MS_15: Self = Self {
        // I would use the function from_millis, but it can't be const yet (see https://github.com/rust-lang/rust/issues/57241)
        duration: Ticks::from_f32(Ticks::TICKS_PER_SECOND / 1000.0 * 15.0),
        easing: Easing::Linear,
    };

    pub fn linear(duration: Ticks) -> Self {
        Self {
            duration,
            easing: Easing::Linear,
        }
    }

    pub fn value(&self, time: Ticks) -> f32 {
        let x = time / self.duration;
        self.easing.apply(x)
    }
}
