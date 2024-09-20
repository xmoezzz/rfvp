use std::{
    iter::Peekable,
    str::{Chars, FromStr},
};

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};

use crate::sanitization::RATIONAL;

// this basically copies the impl of FromStr for Rational from rfvp-core
// shin-code depends on shin-derive, so we can't use it here
// this is not ideal... Do we need even more core-ry crate?

struct Rational(i32);

pub enum Sign {
    Positive,
    Negative,
}

impl Rational {
    pub const DENOM: i32 = 1000;

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
}

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

impl ToTokens for Rational {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let value = self.0;
        tokens.extend(quote!(#RATIONAL::from_raw(#value)))
    }
}

pub fn impl_rational(lit: syn::Lit) -> TokenStream {
    let mut errors = Vec::new();

    let lit = match &lit {
        syn::Lit::Int(lit) => {
            if lit.suffix() != "" {
                errors.push("Rational literal should not have a suffix");
            }

            lit.base10_digits()
        }
        syn::Lit::Float(lit) => {
            if lit.suffix() != "" {
                errors.push("Rational literal should not have a suffix");
            }
            if lit.base10_digits().contains(['e', 'E']) {
                errors.push("Rational literal should not have an exponent");
            }

            lit.base10_digits()
        }
        _ => {
            return quote!(compile_error!(
                "Rational literal should be an integer or a float"
            ));
        }
    };

    let parsed = match Rational::from_str(lit) {
        Ok(r) => r,
        Err(e) => {
            errors.push(match e {
                DecimalParseError::Empty => "Rational literal should not be empty",
                DecimalParseError::InvalidDigit => {
                    "Rational literal should only contain digits, a decimal point, and a sign"
                }
                DecimalParseError::AbsoluteValueTooBig => "Rational literal is too big",
                DecimalParseError::FractionalPartUnrepresentable => {
                    "Rational literal has too many digits in the fractional part"
                }
            });
            Rational(0)
        }
    };

    if errors.is_empty() {
        quote!(#parsed)
    } else {
        let errors = errors.iter().map(|e| quote!(compile_error!(#e);));
        quote!(
            {
                #(#errors)*
                #parsed
            }
        )
    }
}
