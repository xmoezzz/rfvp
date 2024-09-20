//! Encoding and decoding of Shift-JIS variant used by the shin engine.

use std::{collections::HashMap, io};

use once_cell::sync::Lazy;

pub mod string;
mod string_array;

pub use string::{SJisString, U16FixupString, U16String, U8FixupString, U8String, ZeroString};
pub use string_array::StringArray;

include!("decode_tables.rs");
include!("encode_tables.rs");

#[inline]
fn decode_single_sjis_char(c: u8) -> char {
    if c < 0x20 {
        // SAFETY: c < 0x20, so it is safe to construct such a char
        unsafe { char::from_u32_unchecked(c as u32) }
    } else if (0x20..0x80).contains(&c) {
        let index = (c - 0x20) as usize;
        // SAFETY: index < 0x60, so it is safe to access the table
        unsafe { *ASCII_TABLE.get_unchecked(index) }
    } else if (0xa0..0xe0).contains(&c) {
        let index = (c - 0xa0) as usize;
        // SAFETY: index < 0x40, so it is safe to access the table
        unsafe { *KATAKANA_TABLE.get_unchecked(index) }
    } else {
        // unmapped, no such first byte
        '\0'
    }
}

#[inline]
fn decode_double_sjis_char(first: u8, second: u8) -> char {
    // column actually spans two JIS rows
    // so, it's in range 0-193
    let column = if matches!(second, 0x40..=0x7e | 0x80..=0xfc) {
        if (0x40..=0x7e).contains(&second) {
            second - 0x40
        } else {
            second - 0x41
        }
    } else {
        return '\0';
    } as usize;

    let row = match first {
        0x81..=0xa0 => (first - 0x81) * 2, // 64 JIS rows (each HI byte value spans 2 rows)
        0xe0..=0xfc => (first - 0xe0) * 2 + 62, // 58 JIS rows (each HI byte value spans 2 rows)
        _ => return '\0',
    } as usize;

    // row \in [0; 121]
    // column \in [0; 193]
    // addr \in [0; 121*94 + 193] = [0; 11567]
    let addr = row * 94 + column;

    // SAFETY: addr < 11567, so it is safe to access the table
    unsafe { *JIS_TABLE.get_unchecked(addr) }
}

fn is_extended(c: u8) -> bool {
    matches!(c, 0x81..=0x9f | 0xe0..=0xfc)
}

/// The game engine files are encoded in (a variant of) Shift-JIS
/// But the game engine itself uses UTF-8
/// This function converts (a variant of) Shift-JIS to UTF-8
/// This function stops reading either at the first null byte or when byte_size bytes have been read
pub fn read_sjis_string<T: io::Read>(s: &mut T, byte_size: Option<usize>) -> io::Result<String> {
    use io::Read;

    let mut res = String::new();
    // TODO: maybe there is a better estimation
    if let Some(size) = byte_size {
        res.reserve(size);
    }
    let mut b = s
        .bytes()
        .take_while(|c| c.as_ref().map_or(true, |&c| c != 0))
        .take(byte_size.unwrap_or(usize::MAX));

    while let Some(c1) = b.next() {
        let c1 = c1?;
        let utf8_c = if is_extended(c1) {
            let c2 = b.next().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unexpected end of string when reading double-byte char",
                )
            })??;
            let utf8_c = decode_double_sjis_char(c1, c2);

            if utf8_c == '\0' {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unmappable sjis char: 0x{:02x}, 0x{:02x}", c1, c2),
                ));
            }
            utf8_c
        } else {
            let utf8_c = decode_single_sjis_char(c1);
            if utf8_c == '\0' {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid single-byte char: 0x{:02x}", c1),
                ));
            }
            utf8_c
        };

        res.push(utf8_c);
    }

    Ok(res)
}

fn map_char_to_sjis(c: char) -> Option<u16> {
    if c < '\u{0020}' {
        return Some(c as u16);
    }

    if c >= '\u{10000}' {
        return None;
    }
    let c = c as u16;
    let lo = (c & 0x1f) as usize;
    let hi = (c >> 5) as usize;

    let block_index = UNICODE_SJIS_COARSE_MAP[hi];
    if block_index < 0 {
        return None;
    }

    let mapped_char = UNICODE_SJIS_FINE_MAP[block_index as usize][lo];
    if mapped_char == 0 {
        return None;
    }

    Some(mapped_char)
}

/// Calculate the size of a string in Shift-JIS
pub fn measure_sjis_string(s: &str) -> io::Result<usize> {
    let mut result = 0;

    for c in s.chars() {
        let sjis = map_char_to_sjis(c).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unmappable char: {} (U+{:04X})", c, c as u32),
            )
        })?;

        match sjis {
            0x00..=0xff => {
                // single-byte
                result += 1;
            }
            0x100..=0xffff => {
                // double-byte
                result += 2;
            }
            // work around rust-intellij bug
            #[allow(unreachable_patterns)]
            _ => unreachable!(),
        }
    }

    Ok(result)
}

/// Encode a string in Shift-JIS
pub fn write_sjis_string<T: io::Write>(s: &str, dest: &mut T) -> io::Result<()> {
    for c in s.chars() {
        // NOTE: the game impl emits ※ (81A6 in Shift-JIS) for unmappable chars
        // we are more conservative and just error out
        let sjis = map_char_to_sjis(c).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unmappable char: {} (U+{:04X})", c, c as u32),
            )
        })?;

        match sjis {
            0x00..=0xff => {
                // single-byte
                dest.write_all(&[sjis as u8])?;
            }
            0x100..=0xffff => {
                // double-byte
                let hi = (sjis >> 8) as u8;
                let lo = (sjis & 0xff) as u8;
                dest.write_all(&[hi, lo])?;
            }
            // work around rust-intellij bug
            #[allow(unreachable_patterns)]
            _ => unreachable!(),
        }
    }

    Ok(())
}

const FIXUP_ENCODED: &str = "｢｣ｧｨｩｪｫｬｭｮｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜｦﾝｰｯ､ﾟﾞ･?｡";
const FIXUP_DECODED: &str = "「」ぁぃぅぇぉゃゅょあいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわをんーっ、？！…　。";

static FIXUP_DECODE_TABLE: Lazy<HashMap<char, char>> =
    Lazy::new(|| FIXUP_ENCODED.chars().zip(FIXUP_DECODED.chars()).collect());

static FIXUP_ENCODE_TABLE: Lazy<HashMap<char, char>> =
    Lazy::new(|| FIXUP_DECODED.chars().zip(FIXUP_ENCODED.chars()).collect());

/// Apply transformations that the game does to some strings
/// This basically involves replacing hiragana with half-width katakana (and some other chars), which is encoded as one byte in Shift-JIS
pub fn encode_string_fixup(s: &str) -> String {
    s.chars()
        .map(|c| FIXUP_ENCODE_TABLE.get(&c).copied().unwrap_or(c))
        .collect()
}

/// Apply transformations that the game does to some strings
/// This basically involves replacing  
pub fn decode_string_fixup(s: &str) -> String {
    s.chars()
        .map(|c| FIXUP_DECODE_TABLE.get(&c).copied().unwrap_or(c))
        .collect()
}

#[cfg(test)]
mod tests {
    #[allow(unused)]
    use super::*;

    #[test]
    fn test_sjis() {
        let s = b"\x82\xa0\x82\xa2\x82\xa4\x82\xa6\x82\xa8";
        let s = read_sjis_string(&mut io::Cursor::new(s), Some(s.len())).unwrap();
        assert_eq!(s, "あいうえお");
        let mut encoded = Vec::new();
        write_sjis_string(&s, &mut encoded).unwrap();
        assert_eq!(encoded, b"\x82\xa0\x82\xa2\x82\xa4\x82\xa6\x82\xa8");
    }

    // TODO: cover the fix-ups with tests

    // these files were auto-generated by a script
    // they check that the Shift_JIS decoder works the same way the original engine does it
    // to be more precise, it was tested against the Higirashi version
    include!("sjis_decode_tests.rs");
    include!("sjis_decode_unmapped_tests.rs");

    // this file was semi-automatically generated from the JIS table
    // it checks whether we can round-trip all the chars in the JIS table via Shift-JIS
    // (surprise: we can't, because of some private-use-area shenanigans. see the comment in the file for more details)
    include!("sjis_round_trip_tests.rs");
}
