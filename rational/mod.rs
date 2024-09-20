pub use rfvp_derive::rat;

mod conv;
mod ops;
mod parse;
mod str;

/// Implements a fixed-point decimal number with 3 digits of precision.
///
/// This type is commonly used for fractional numbers in shin.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rational(i32);

pub enum Sign {
    Positive,
    Negative,
}

impl Rational {
    pub const DENOM: i32 = 1000;

    pub const MAX: Self = Self(i32::MAX);
    pub const MIN: Self = Self(i32::MIN);

    pub const ZERO: Self = rat!(0);
    pub const ONE: Self = rat!(1);
    pub const PI: Self = rat!(3.141);
    pub const DOUBLE_PI: Self = rat!(6.283);

    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    pub const fn into_raw(self) -> i32 {
        self.0
    }

    pub fn try_from_parts(sign: Sign, integer: u32, fraction: u16) -> Result<Self, ()> {
        let (max_int, max_frac) = match sign {
            Sign::Positive => (2147483, 647),
            Sign::Negative => (2147483, 648),
        };

        if integer > max_int || integer == max_int && fraction > max_frac {
            return Err(());
        }
        
        let fraction = fraction as u32;

        let value = integer * Self::DENOM as u32 + fraction;

        if cfg!(debug_assertions) {
            match sign {
                Sign::Positive => assert!(value <= i32::MAX as u32),
                Sign::Negative => assert!(value <= i32::MAX as u32 + 1),
            }
        }

        Ok(Self(match sign {
            Sign::Positive => value as i32,
            Sign::Negative => (value as i32).wrapping_neg(),
        }))
    }

    pub fn into_parts(self) -> (Sign, u32, u16) {
        let sign = if self.0 < 0 {
            Sign::Negative
        } else {
            Sign::Positive
        };
        let integer = (self.0.abs() / Self::DENOM) as u32;
        let fraction = (self.0.abs() % Self::DENOM) as u16;

        (sign, integer, fraction)
    }
}
