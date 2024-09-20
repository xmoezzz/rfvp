use super::Rational;

impl From<f32> for Rational {
    fn from(value: f32) -> Self {
        Self((value * 1000.0).round() as i32)
    }
}

impl From<f64> for Rational {
    fn from(value: f64) -> Self {
        Self((value * 1000.0).round() as i32)
    }
}

impl From<Rational> for f32 {
    fn from(value: Rational) -> Self {
        value.0 as f32 / 1000.0
    }
}

impl From<Rational> for f64 {
    fn from(value: Rational) -> Self {
        value.0 as f64 / 1000.0
    }
}
