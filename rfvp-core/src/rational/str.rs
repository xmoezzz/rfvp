use std::fmt::{Debug, Display};

use super::{Rational, Sign};

impl Debug for Rational {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (sign, integer, fraction) = self.into_parts();

        let sign = match sign {
            Sign::Positive => {
                if f.sign_plus() {
                    "+"
                } else {
                    ""
                }
            }
            Sign::Negative => "-",
        };

        // TODO: ideally, we should handle all the different formatting options here
        // unfortunately, rust doesn't help us with that...
        write!(f, "{sign}{integer}.{fraction:03}")
    }
}

impl Display for Rational {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}
