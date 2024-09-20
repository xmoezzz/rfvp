use std::{
    iter::Peekable,
    str::{Chars, FromStr},
};

use super::{Rational, Sign};

#[derive(Debug, PartialEq, Eq)]
pub enum DecimalParseError {
    Empty,
    InvalidDigit,
    AbsoluteValueTooBig,
    FractionalPartUnrepresentable,
}

fn collect_int(cursor: &mut Peekable<Chars>) -> Result<u32, DecimalParseError> {
    let mut int = 0u32;

    if cursor.peek().is_none() {
        return Err(DecimalParseError::Empty);
    }
    for c in cursor.by_ref() {
        match c {
            '0'..='9' => {
                let digit = c as u32 - '0' as u32;

                int = int
                    .checked_mul(10)
                    .and_then(|int| int.checked_add(digit))
                    .ok_or(DecimalParseError::AbsoluteValueTooBig)?;
            }
            '.' => break,
            _ => return Err(DecimalParseError::InvalidDigit),
        }
    }

    Ok(int)
}

fn collect_frac(cursor: &mut Peekable<Chars>) -> Result<u16, DecimalParseError> {
    let mut frac = 0u16;
    let mut position = 100u16;

    if cursor.peek().is_none() {
        return Err(DecimalParseError::Empty);
    }
    for c in cursor.by_ref() {
        match c {
            '0'..='9' => {
                let digit = c as u16 - '0' as u16;

                if digit != 0 && position == 0 {
                    return Err(DecimalParseError::FractionalPartUnrepresentable);
                }

                frac += digit * position;
                position /= 10;
            }
            _ => return Err(DecimalParseError::InvalidDigit),
        }
    }

    Ok(frac)
}

impl FromStr for Rational {
    type Err = DecimalParseError;

    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(DecimalParseError::Empty);
        }

        let sign = {
            let mut cur = s.chars();
            let first_char = cur.next().unwrap();
            match first_char {
                '+' => {
                    s = cur.as_str();
                    Sign::Positive
                }
                '-' => {
                    s = cur.as_str();
                    Sign::Negative
                }
                _ => Sign::Positive,
            }
        };

        let mut cursor = s.chars().peekable();
        let integer_part = collect_int(&mut cursor)?;

        if cursor.peek().is_none() {
            return Rational::try_from_parts(sign, integer_part, 0)
                .map_err(|_| DecimalParseError::AbsoluteValueTooBig);
        }

        let fractional_part = collect_frac(&mut cursor)?;

        Rational::try_from_parts(sign, integer_part, fractional_part)
            .map_err(|_| DecimalParseError::AbsoluteValueTooBig)
    }
}

#[cfg(test)]
mod tests {
    use super::{DecimalParseError, Rational};

    #[test]
    fn parse_basic() {
        assert_eq!("42".parse(), Ok(Rational(42_000)));
        assert_eq!("42.0".parse(), Ok(Rational(42_000)));
        assert_eq!("42.00".parse(), Ok(Rational(42_000)));
        assert_eq!("42.000".parse(), Ok(Rational(42_000)));
        assert_eq!("42.0000".parse(), Ok(Rational(42_000)));
        assert_eq!("42.00000".parse(), Ok(Rational(42_000)));
        assert_eq!("42.000000".parse(), Ok(Rational(42_000)));

        assert_eq!("42.1".parse(), Ok(Rational(42_100)));
        assert_eq!("42.101".parse(), Ok(Rational(42_101)));
        assert_eq!("42.001".parse(), Ok(Rational(42_001)));

        assert_eq!("+42".parse(), Ok(Rational(42_000)));
        assert_eq!("-42".parse(), Ok(Rational(-42_000)));
        assert_eq!("-42.123".parse(), Ok(Rational(-42_123)));
    }

    #[test]
    fn parse_big() {
        assert_eq!("2147483.647".parse(), Ok(Rational(2147483_647)));
        assert_eq!(
            "2147483.648".parse::<Rational>(),
            Err(DecimalParseError::AbsoluteValueTooBig)
        );
        assert_eq!("-2147483.648".parse(), Ok(Rational(-2147483_648)));
        assert_eq!(
            "-2147483.649".parse::<Rational>(),
            Err(DecimalParseError::AbsoluteValueTooBig)
        );
    }
}
