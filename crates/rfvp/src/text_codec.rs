//! The script's text encodings (`Nls`): bytes to text and back.
//!
//! encoding_rs does it everywhere but on the PlayStation, where its tables for every encoding
//! (188 KiB, linked in whole) would take a tenth of the 2 MiB: there Shift-JIS has its own table
//! (22 KiB: `sjis_table.bin`, the two-byte codes of Windows-31J), and GBK is not supported.

use alloc::string::String;
use alloc::vec::Vec;

use crate::script::parser::Nls;

/// `bytes` as text, and whether some could not be decoded (they become U+FFFD).
pub fn decode(nls: Nls, bytes: &[u8]) -> (String, bool) {
    #[cfg(not(feature = "psx"))]
    {
        let encoding = match nls {
            Nls::ShiftJIS => encoding_rs::SHIFT_JIS,
            Nls::GBK => encoding_rs::GBK,
            Nls::UTF8 => encoding_rs::UTF_8,
        };
        let (text, _, errors) = encoding.decode(bytes);
        (text.into_owned(), errors)
    }
    #[cfg(feature = "psx")]
    match nls {
        Nls::ShiftJIS => sjis::decode(bytes),
        Nls::GBK | Nls::UTF8 => {
            let text = String::from_utf8_lossy(bytes);
            let errors = matches!(text, alloc::borrow::Cow::Owned(_));
            (text.into_owned(), errors)
        }
    }
}

/// `text` in the encoding; what it cannot hold becomes `?`.
pub fn encode(nls: Nls, text: &str) -> Vec<u8> {
    #[cfg(not(feature = "psx"))]
    {
        match nls {
            Nls::ShiftJIS => encoding_rs::SHIFT_JIS.encode(text).0.into_owned(),
            Nls::GBK => encoding_rs::GBK.encode(text).0.into_owned(),
            Nls::UTF8 => text.as_bytes().to_vec(),
        }
    }
    #[cfg(feature = "psx")]
    match nls {
        Nls::ShiftJIS => sjis::encode(text),
        Nls::GBK | Nls::UTF8 => text.as_bytes().to_vec(),
    }
}

#[cfg(feature = "psx")]
mod sjis {
    use alloc::string::String;
    use alloc::vec::Vec;

    /// Lead bytes 0x81..=0x9F and 0xE0..=0xFC, each with trail bytes 0x40..=0xFC: the character
    /// (UTF-16, little endian), 0 where none.
    static TABLE: &[u8] = include_bytes!("sjis_table.bin");
    const TRAILS: usize = 0xFD - 0x40;

    fn lead_index(lead: u8) -> Option<usize> {
        match lead {
            0x81..=0x9F => Some((lead - 0x81) as usize),
            0xE0..=0xFC => Some((lead - 0xE0) as usize + 0x1F),
            _ => None,
        }
    }

    fn lookup(lead: u8, trail: u8) -> Option<char> {
        let row = lead_index(lead)?;
        if !(0x40..=0xFC).contains(&trail) {
            return None;
        }
        let at = (row * TRAILS + (trail - 0x40) as usize) * 2;
        let code = u16::from_le_bytes([TABLE[at], TABLE[at + 1]]);
        if code == 0 {
            None
        } else {
            char::from_u32(code as u32)
        }
    }

    pub fn decode(bytes: &[u8]) -> (String, bool) {
        let mut out = String::with_capacity(bytes.len());
        let mut errors = false;
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            match b {
                0x00..=0x7F => {
                    out.push(b as char);
                    i += 1;
                }
                // Half-width katakana.
                0xA1..=0xDF => {
                    out.push(char::from_u32(0xFF61 + (b - 0xA1) as u32).unwrap_or('\u{FFFD}'));
                    i += 1;
                }
                _ => match bytes.get(i + 1).and_then(|&t| lookup(b, t)) {
                    Some(c) => {
                        out.push(c);
                        i += 2;
                    }
                    None => {
                        out.push('\u{FFFD}');
                        errors = true;
                        i += if lead_index(b).is_some() && i + 1 < bytes.len() { 2 } else { 1 };
                    }
                },
            }
        }
        (out, errors)
    }

    pub fn encode(text: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(text.len());
        for c in text.chars() {
            let code = c as u32;
            if code < 0x80 {
                out.push(code as u8);
                continue;
            }
            if (0xFF61..=0xFF9F).contains(&code) {
                out.push((code - 0xFF61) as u8 + 0xA1);
                continue;
            }
            // Saves' titles only: a search of the table is fast enough.
            let found = TABLE.chunks_exact(2).position(|p| u16::from_le_bytes([p[0], p[1]]) as u32 == code);
            match found {
                Some(index) => {
                    let (row, trail) = (index / TRAILS, index % TRAILS);
                    let lead = if row < 0x1F { 0x81 + row as u8 } else { 0xE0 + (row - 0x1F) as u8 };
                    out.push(lead);
                    out.push(0x40 + trail as u8);
                }
                None => out.push(b'?'),
            }
        }
        out
    }
}
