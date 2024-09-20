use std::io;

use binrw::{BinRead, BinResult, BinWrite, Endian};
use smallvec::SmallVec;

use super::read_sjis_string;
use crate::format::text::write_sjis_string;

/// A non-fixed-up shift-jis string array stored a bit more efficiently than a list of strings
///
/// Used only in [crate::vm::command::Command::SELECT]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringArray(pub SmallVec<String, 4>);

impl StringArray {
    pub fn new<T: Into<String>, I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self(iter.into_iter().map(Into::into).collect())
    }
}

impl BinRead for StringArray {
    type Args<'a> = ();

    fn read_options<R: io::Read + io::Seek>(
        reader: &mut R,
        endian: Endian,
        _: (),
    ) -> BinResult<Self> {
        let size = u16::read_options(reader, endian, ())?;
        let pos = reader.stream_position()?;
        let mut res = SmallVec::new();
        loop {
            let s = read_sjis_string(reader, None)?;
            // the end of array is marked by an additional null terminator
            // which we treat as an empty string
            // this is fiiiine
            if s.len() == 0 {
                break;
            }

            res.push(s);
        }
        let end = reader.stream_position()?;
        let read = end - pos;
        assert_eq!(read, size as u64);
        Ok(Self(res))
    }
}

impl BinWrite for StringArray {
    type Args<'a> = ();

    fn write_options<W: io::Write + io::Seek>(
        &self,
        writer: &mut W,
        endian: Endian,
        _: (),
    ) -> BinResult<()> {
        let mut buffer = Vec::new();

        for s in &self.0 {
            write_sjis_string(s, &mut buffer)?;
            buffer.push(0u8);
        }
        buffer.push(0u8);

        let size: u16 = buffer.len().try_into().expect("StringArray too large");
        size.write_options(writer, endian, ())?;

        writer.write_all(&buffer)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::iter;

    use super::StringArray;
    use crate::format::test_util::assert_enc_dec_pair;

    #[test]
    fn enc_dec() {
        assert_enc_dec_pair(&StringArray::new(iter::empty::<&str>()), "010000");
        assert_enc_dec_pair(
            &StringArray::new(["HELLO", "WORLD"]),
            "0d0048454c4c4f00574f524c440000",
        );
        assert_enc_dec_pair(
            &StringArray::new(["ミク", "かわいい"]),
            "0f00837e834e0082a982ed82a282a20000",
        );
        assert_enc_dec_pair(&StringArray::new(["日本"]), "060093fa967b0000");
    }
}
