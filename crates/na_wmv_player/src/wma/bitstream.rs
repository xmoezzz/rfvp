//! Bitstream reader.
//!
//! This is a simplified but semantically equivalent translation of upstream's
//! `GetBitContext` (MSB-first bit order).

use crate::error::{DecoderError, Result};

#[derive(Clone)]
pub struct GetBitContext<'a> {
    buf: &'a [u8],
    size_in_bits: usize,
    size_in_bits_plus8: usize,
    bit_pos: usize,
}

impl<'a> GetBitContext<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        let size_in_bits = buf.len() * 8;
        Self {
            buf,
            size_in_bits,
            size_in_bits_plus8: size_in_bits.saturating_add(8),
            bit_pos: 0,
        }
    }

    pub fn new_bits(buf: &'a [u8], size_in_bits: usize) -> Result<Self> {
        if size_in_bits > buf.len() * 8 {
            return Err(DecoderError::InvalidData(
                "bitstream size exceeds buffer".into(),
            ));
        }
        Ok(Self {
            buf,
            size_in_bits,
            size_in_bits_plus8: size_in_bits.saturating_add(8),
            bit_pos: 0,
        })
    }

    pub fn new_window(buf: &'a [u8], bit_pos: usize, size_in_bits: usize) -> Result<Self> {
        if bit_pos > size_in_bits || size_in_bits > buf.len() * 8 {
            return Err(DecoderError::InvalidData("invalid bitstream window".into()));
        }
        Ok(Self {
            buf,
            size_in_bits,
            size_in_bits_plus8: size_in_bits.saturating_add(8),
            bit_pos,
        })
    }

    #[inline]
    pub fn bits_left(&self) -> isize {
        self.size_in_bits as isize - self.bit_pos as isize
    }

    #[inline]
    pub fn bits_read(&self) -> usize {
        self.bit_pos
    }

    #[inline]
    pub fn align_to_byte(&mut self) {
        let target = self.bit_pos.saturating_add(7) & !7;
        self.bit_pos = target.min(self.size_in_bits_plus8);
    }

    /// Advance the bit reader using FFmpeg's checked GetBitContext semantics.
    ///
    /// FFmpeg deliberately permits the read index to enter eight bits of zero
    /// padding past the logical end of the bitstream.  A large amount of its
    /// VLC code relies on that look-ahead behavior at frame boundaries.
    #[inline]
    pub fn skip_bits(&mut self, n: usize) -> Result<()> {
        self.bit_pos = self.bit_pos.saturating_add(n).min(self.size_in_bits_plus8);
        Ok(())
    }

    #[inline]
    pub fn get_bits1(&mut self) -> Result<u32> {
        self.get_bits(1)
    }

    /// Read up to 32 bits.
    ///
    /// This mirrors FFmpeg's checked GetBitContext: reads use zero-filled
    /// padding beyond `size_in_bits`, while the stored bit index is clamped to
    /// `size_in_bits + 8`.  It is intentionally not an EOF error by itself;
    /// callers that require a hard boundary check use `bits_left()` exactly as
    /// the upstream WMA decoders do.
    #[inline]
    pub fn get_bits(&mut self, n: usize) -> Result<u32> {
        if n == 0 {
            return Ok(0);
        }
        if n > 32 {
            return Err(DecoderError::InvalidData("get_bits > 32".into()));
        }

        let start = self.bit_pos;
        let mut pos = start;
        let mut remaining = n;
        let mut out: u32 = 0;
        while remaining > 0 {
            if pos >= self.size_in_bits {
                out <<= remaining;
                break;
            }

            let byte_idx = pos >> 3;
            let bit_in_byte = pos & 7;
            let byte_avail = 8 - bit_in_byte;
            let logical_avail = self.size_in_bits - pos;
            let take = remaining.min(byte_avail).min(logical_avail);
            let byte = self.buf[byte_idx] as u32;
            let shift = byte_avail - take;
            let mask = (1u32 << take) - 1;
            out = (out << take) | ((byte >> shift) & mask);
            pos += take;
            remaining -= take;
        }

        self.bit_pos = start.saturating_add(n).min(self.size_in_bits_plus8);
        Ok(out)
    }

    #[inline]
    pub fn show_bits(&self, n: usize) -> Result<u32> {
        let mut tmp = self.clone();
        tmp.get_bits(n)
    }

    #[inline]
    pub fn get_bits_long(&mut self, n: usize) -> Result<u32> {
        self.get_bits(n)
    }

    #[inline]
    pub fn get_sbits(&mut self, n: usize) -> Result<i32> {
        if n == 0 || n > 32 {
            return Err(DecoderError::InvalidData("invalid signed bit width".into()));
        }
        let raw = self.get_bits(n)?;
        let shift = 32 - n;
        Ok(((raw << shift) as i32) >> shift)
    }
}
