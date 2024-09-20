use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

use super::Rational;

impl Add for Rational {
    type Output = Rational;

    fn add(self, rhs: Self) -> Self::Output {
        let (Self(lhs), Self(rhs)) = (self, rhs);
        Self(lhs + rhs)
    }
}

impl Sub for Rational {
    type Output = Rational;

    fn sub(self, rhs: Self) -> Self::Output {
        let (Self(lhs), Self(rhs)) = (self, rhs);
        Self(lhs - rhs)
    }
}

impl Mul for Rational {
    type Output = Rational;

    fn mul(self, rhs: Self) -> Self::Output {
        let (Self(lhs), Self(rhs)) = (self, rhs);
        // take care to not overflow when not necessary
        let result = lhs as i64 * rhs as i64 / Rational::DENOM as i64;

        Self(result.try_into().expect("overflow"))
    }
}

impl Div for Rational {
    type Output = Rational;

    fn div(self, rhs: Self) -> Self::Output {
        let (Self(lhs), Self(rhs)) = (self, rhs);
        // take care to not overflow when not necessary
        let result = lhs as i64 * Rational::DENOM as i64 / rhs as i64;

        Self(result.try_into().expect("overflow"))
    }
}

impl AddAssign for Rational {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl SubAssign for Rational {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl MulAssign for Rational {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl DivAssign for Rational {
    fn div_assign(&mut self, rhs: Self) {
        *self = *self / rhs;
    }
}
