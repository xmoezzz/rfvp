use std::{fmt::Debug, hash::Hash, io, marker::PhantomData};

use binrw::{BinRead, BinResult, BinWrite, Endian};
use rfvp_core::format::text::{measure_sjis_string, write_sjis_string};

use crate::format::text;

pub trait StringFixup {
    fn encode(string: String) -> String;
    fn decode(string: String) -> String;
}

#[derive(Debug)]
pub struct NoFixup;
impl StringFixup for NoFixup {
    fn encode(string: String) -> String {
        string
    }
    fn decode(string: String) -> String {
        string
    }
}

#[derive(Debug)]
pub struct WithFixup;
impl StringFixup for WithFixup {
    fn encode(string: String) -> String {
        text::encode_string_fixup(&string)
    }

    fn decode(string: String) -> String {
        text::decode_string_fixup(&string)
    }
}

pub trait StringLengthDesc:
    for<'a> BinRead<Args<'a> = ()> + for<'a> BinWrite<Args<'a> = ()> + Sized + 'static
{
    /// Should return the length of the string, in bytes, including the null terminator.
    fn get_length(&self) -> Option<usize>;

    fn from_length(length: usize) -> Option<Self>;
}

impl StringLengthDesc for u8 {
    fn get_length(&self) -> Option<usize> {
        Some(*self as usize)
    }

    fn from_length(length: usize) -> Option<Self> {
        length.try_into().ok()
    }
}

impl StringLengthDesc for u16 {
    fn get_length(&self) -> Option<usize> {
        Some(*self as usize)
    }

    fn from_length(length: usize) -> Option<Self> {
        length.try_into().ok()
    }
}

impl StringLengthDesc for () {
    fn get_length(&self) -> Option<usize> {
        None
    }

    fn from_length(_: usize) -> Option<Self> {
        Some(())
    }
}

/// A string that is encoded in Shift-JIS when written to a file.
///
/// `L` specifies how the length will be encoded in the file.
/// `F` describes the fixup to be applied to the string (an additional transform that is applied to the string before it is written to the file).
///     Entergram uses it to convert hiragana to half-width katakana in some places, probably saving a bit of space.
pub struct SJisString<L: StringLengthDesc, F: StringFixup + 'static = NoFixup>(
    pub String,
    pub PhantomData<(L, F)>,
);

/// A zero-terminated Shift-JIS string.
pub type ZeroString = SJisString<()>;
/// A Shift-JIS string with a u8 length descriptor.
pub type U8String = SJisString<u8>;
/// A Shift-JIS string with a u16 length descriptor.
pub type U16String = SJisString<u16>;
/// A Shift-JIS string with a u8 length descriptor and fixup applied.
pub type U8FixupString = SJisString<u8, WithFixup>;
/// A Shift-JIS string with a u16 length descriptor and fixup applied.
pub type U16FixupString = SJisString<u16, WithFixup>;

impl<L: StringLengthDesc, F: StringFixup + 'static> SJisString<L, F> {
    pub fn new(string: impl Into<String>) -> Self {
        Self(string.into(), PhantomData)
    }
}

impl<L: StringLengthDesc, F: StringFixup + 'static> PartialEq for SJisString<L, F> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<L: StringLengthDesc, F: StringFixup + 'static> Eq for SJisString<L, F> {}

impl<L: StringLengthDesc, F: StringFixup + 'static> Clone for SJisString<L, F> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}

impl<L: StringLengthDesc, F: StringFixup + 'static> Hash for SJisString<L, F> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl<L: StringLengthDesc, F: StringFixup> BinRead for SJisString<L, F> {
    type Args<'a> = ();

    fn read_options<R: io::Read + io::Seek>(
        reader: &mut R,
        endian: Endian,
        _: (),
    ) -> BinResult<Self> {
        let len = L::read_options(reader, endian, ())?;
        // "- 1" to strip the null terminator

        let res = Self(
            // TODO: extra allocation in case of fixup
            // this will consume the null terminator
            F::decode(text::read_sjis_string(reader, len.get_length())?),
            PhantomData,
        );

        // read the null terminator
        // let _ = u8::read_options(reader, options, ())?;

        Ok(res)
    }
}
impl<L: StringLengthDesc, F: StringFixup> BinWrite for SJisString<L, F> {
    type Args<'a> = ();

    fn write_options<W: io::Write + io::Seek>(
        &self,
        writer: &mut W,
        endian: Endian,
        _: (),
    ) -> BinResult<()> {
        let pos = writer.stream_position()?;

        // TODO: extra allocation ALWAYS
        let fixed_up = F::encode(self.0.clone());

        let len = measure_sjis_string(&fixed_up)?;

        // we ALWAYS add a null terminator, even with length-prefixed strings
        let len = L::from_length(len + 1)
            .ok_or_else(|| binrw::Error::AssertFail {
                pos,
                message: "Failed to convert string length to the encoded representation. This is probably due to the string being too long.".to_string(),
            })?;

        len.write_options(writer, endian, ())?;

        write_sjis_string(&fixed_up, writer)?;

        // write the null terminator
        let _ = 0u8.write_options(writer, endian, ())?;

        Ok(())
    }
}
impl<L: StringLengthDesc, F: StringFixup> AsRef<str> for SJisString<L, F> {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl<L: StringLengthDesc, F: StringFixup> Debug for SJisString<L, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}
impl<L: StringLengthDesc, F: StringFixup> std::fmt::Display for SJisString<L, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}
impl<L: StringLengthDesc, F: StringFixup> SJisString<L, F> {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}


#[cfg(test)]
mod tests {
    use binrw::{io::NoSeek, BinWrite};

    use super::{U16FixupString, U16String, U8FixupString, U8String, ZeroString};
    use crate::format::test_util::assert_enc_dec_pair;

    #[test]
    fn enc_dec_zero_string() {
        assert_enc_dec_pair(&ZeroString::new(""), "00");
        assert_enc_dec_pair(&ZeroString::new("HELLO"), "48454c4c4f00");
        assert_enc_dec_pair(&ZeroString::new("ミク"), "837e834e00");
        assert_enc_dec_pair(&ZeroString::new("かわいい"), "82a982ed82a282a200");
        assert_enc_dec_pair(&ZeroString::new("日本"), "93fa967b00");
    }

    #[test]
    fn enc_dec_u8() {
        assert_enc_dec_pair(&U8String::new(""), "0100");
        assert_enc_dec_pair(&U8String::new("HELLO"), "0648454c4c4f00");
        assert_enc_dec_pair(
            &U8String::new("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
            "ff414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414100",
        );
        assert_enc_dec_pair(&U8String::new("ミク"), "05837e834e00");
        assert_enc_dec_pair(&U8FixupString::new("ミク"), "05837e834e00");
        assert_enc_dec_pair(&U8String::new("かわいい"), "0982a982ed82a282a200");
        assert_enc_dec_pair(&U8FixupString::new("かわいい"), "05b6dcb2b200");
        assert_enc_dec_pair(&U8FixupString::new("日本"), "0593fa967b00");
    }

    #[test]
    fn enc_u8_overflow() {
        let err = U8String::new("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
            .write_le(&mut NoSeek::new(Vec::new()))
            .unwrap_err();
        assert_eq!(format!("{:?}", err), "Failed to convert string length to the encoded representation. This is probably due to the string being too long. at 0x0");
    }

    #[test]
    fn enc_dec_u16() {
        assert_enc_dec_pair(&U16String::new(""), "010000");
        assert_enc_dec_pair(&U16String::new("HELLO"), "060048454c4c4f00");
        assert_enc_dec_pair(
            &U16String::new("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
            "000141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414100",
        );
        assert_enc_dec_pair(&U16String::new("ミク"), "0500837e834e00");
        assert_enc_dec_pair(&U16FixupString::new("ミク"), "0500837e834e00");
        assert_enc_dec_pair(&U16String::new("かわいい"), "090082a982ed82a282a200");
        assert_enc_dec_pair(&U16FixupString::new("かわいい"), "0500b6dcb2b200");
        assert_enc_dec_pair(&U16FixupString::new("日本"), "050093fa967b00");
    }
}
