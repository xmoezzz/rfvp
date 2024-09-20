mod prng;

use std::f32::consts::PI;

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use rfvp_core::time::Ticks;
use tracing::warn;

#[derive(FromPrimitive, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum WobbleMode {
    Disabled = 0,
    /// Jumps to a random position every `period` ticks.
    Random = 1,
    /// Goes from 0 to 1 at 0 to .25, then from 1 to -1 at .25 to .75, then from -1 to 0 at .75 to 1.
    /// Results in kind-of choppy sine, as it's basically a triangular wave.
    /// https://www.desmos.com/calculator/j2wohvz0jt
    Triangular = 2,
    /// Jumps between -1 at 0 to .5 and 1 at .5 to 1.
    /// https://www.desmos.com/calculator/ntmtsrmh5j
    Square = 3,
    /// Plain old `sin(2pi * t)` wave.
    Sine = 4,
    /// Plain old `cos(2pi * t)` wave.
    Cosine = 5,
    /// `abs(sin(2pi * t))` wave.
    AbsSine = 6,
    /// Sawtooth: just goes from 0 to 1 each period, then jumps back.
    Sawtooth = 7,
    /// Sawtooth, but goes from 1 to 0.
    InvSawtooth = 8,
}

pub struct Wobbler {
    mode: WobbleMode,
    #[allow(unused)]
    seed: i32,
    period: Ticks,
    /// Time, measured in periods
    time: f32,
}
impl Wobbler {
    pub fn new() -> Self {
        Self {
            mode: WobbleMode::Disabled,
            seed: 0, // TODO: initialize to a non-zero seed
            period: Ticks::ZERO,
            time: 0.0,
        }
    }

    pub fn value(&self) -> f32 {
        if !self.active() {
            return 0.0;
        }

        let t = self.time % 1.0;
        match self.mode {
            WobbleMode::Disabled => 0.0,
            WobbleMode::Random => prng::prng(t, (self.time - t) as i32, self.seed),
            WobbleMode::Triangular => {
                if t < 0.25 {
                    t * 4.0
                } else if t < 0.75 {
                    2.0 - t * 4.0
                } else {
                    t * 4.0 - 4.0
                }
            }
            WobbleMode::Square => {
                if t < 0.5 {
                    -1.0
                } else {
                    1.0
                }
            }
            WobbleMode::Sine => (t * 2.0 * PI).sin(),
            WobbleMode::Cosine => (t * 2.0 * PI).cos(),
            WobbleMode::AbsSine => (t * 2.0 * PI).sin().abs(),
            WobbleMode::Sawtooth => t,
            WobbleMode::InvSawtooth => 1.0 - t,
        }
    }

    pub fn active(&self) -> bool {
        self.mode != WobbleMode::Disabled && self.period > Ticks::ZERO
    }

    pub fn update(&mut self, delta_time: Ticks, mode: f32, period: Ticks) {
        let mode = mode as i32;
        let mode = WobbleMode::from_i32(mode).unwrap_or_else(|| {
            // TODO: this should be printed once per invalid mode
            warn!("Invalid wobble mode: {}", mode);
            WobbleMode::Disabled
        });

        if mode != self.mode || period != self.period {
            self.mode = mode;
            self.period = period;
            self.time = 0.0;
        }

        if !self.active() {
            return;
        }

        let time = self.time + delta_time / period;
        let time_int = time.floor();
        let time_frac = time - time_int;

        let time_frac = if time_frac < 0.0 {
            // could this actually happen??
            1.0 + time_frac
        } else {
            time_frac
        };

        // round the integral part at 1000 periods
        let time_int = time_int % 1000.0;
        let time_int = if time_int < 0.0 {
            1000.0 + time_int
        } else {
            time_int
        };

        self.time = time_int + time_frac;
    }
}
