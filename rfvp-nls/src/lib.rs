use anyhow::Result;
use encoding_rs::{Encoding as RsEncoding, GB18030, SHIFT_JIS, UTF_8};
use std::borrow::Cow;

pub trait TextDecoder {
    fn decode<'a>(&self, bytes: &'a [u8]) -> Cow<'a, str>;

    /// Decode C-style string: stop at the first NUL (0x00).
    fn decode_cstr<'a>(&self, bytes: &'a [u8]) -> Cow<'a, str> {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        self.decode(&bytes[..end])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    ShiftJis,
    /// Treat GBK as GB18030 (superset). This is robust for legacy CN game assets.
    Gbk,
    Gb18030,
}

impl Encoding {
    #[inline]
    pub fn as_encoding_rs(self) -> &'static RsEncoding {
        match self {
            Encoding::Utf8 => UTF_8,
            Encoding::ShiftJis => SHIFT_JIS,
            Encoding::Gbk => GB18030,
            Encoding::Gb18030 => GB18030,
        }
    }
}

/// A simple decoder bound to one encoding.
#[derive(Debug, Clone, Copy)]
pub struct Decoder {
    enc: Encoding,
}

impl Decoder {
    #[inline]
    pub fn new(enc: Encoding) -> Self {
        Self { enc }
    }

    #[inline]
    pub fn encoding(&self) -> Encoding {
        self.enc
    }

    /// Encode a Rust string to bytes using the selected encoding.
    /// This is "best effort": unrepresentable chars will be replaced.
    pub fn encode<'a>(&self, s: &'a str) -> Cow<'a, [u8]> {
        let enc = self.enc.as_encoding_rs();
        let (cow, _had_errors, _) = enc.encode(s);
        cow
    }

    /// Same as encode(), but always returns an owned Vec<u8>.
    pub fn encode_owned(&self, s: &str) -> Vec<u8> {
        self.encode(s).into_owned()
    }
}

impl TextDecoder for Decoder {
    fn decode<'a>(&self, bytes: &'a [u8]) -> Cow<'a, str> {
        match self.enc {
            Encoding::Utf8 => match std::str::from_utf8(bytes) {
                Ok(s) => Cow::Borrowed(s),
                Err(_) => Cow::Owned(String::from_utf8_lossy(bytes).into_owned()),
            },
            Encoding::ShiftJis | Encoding::Gbk | Encoding::Gb18030 => {
                let enc = self.enc.as_encoding_rs();
                let (cow, _had_errors, _) = enc.decode(bytes);
                cow
            }
        }
    }
}

/// A convenience default.
impl Default for Decoder {
    fn default() -> Self {
        Self::new(Encoding::Utf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_decode_cstr() {
        let d = Decoder::new(Encoding::Utf8);
        let bytes = b"hello\0world";
        assert_eq!(d.decode_cstr(bytes), "hello");
    }

    #[test]
    fn encode_roundtrip_ascii_shiftjis() {
        let d = Decoder::new(Encoding::ShiftJis);
        let s = "ABCxyz123";
        let b = d.encode_owned(s);
        assert_eq!(d.decode(&b), s);
    }
}
