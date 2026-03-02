use super::error::{DecodeError, Result};

/// MSB-first bitreader equivalent to `GetBitContext`.
#[derive(Clone, Debug)]
pub struct GetBits<'a> {
    buf: &'a [u8],
    size_in_bits: usize,
    bit_pos: usize,
}

impl<'a> GetBits<'a> {
    pub fn init(buf: &'a [u8]) -> Self {
        Self { buf, size_in_bits: buf.len() * 8, bit_pos: 0 }
    }

    #[inline]
    pub fn bits_left(&self) -> i32 {
        self.size_in_bits as i32 - self.bit_pos as i32
    }

    #[inline]
    pub fn bits_count(&self) -> usize {
        self.bit_pos
    }

    #[inline]
    pub fn align(&mut self) {
        self.bit_pos = (self.bit_pos + 7) & !7;
    }

    #[inline]
    pub fn skip_bits(&mut self, n: usize) {
        self.bit_pos = self.bit_pos.saturating_add(n);
    }

    #[inline]
    pub fn skip_bits1(&mut self) {
        self.skip_bits(1);
    }

    #[inline]
    pub fn get_bits1(&mut self) -> u32 {
        self.get_bits(1)
    }

    #[inline]
    pub fn show_bits(&self, n: usize) -> u32 {
        debug_assert!(n <= 32);
        if n == 0 {
            return 0;
        }
        let byte_pos = self.bit_pos >> 3;
        let bit_off = self.bit_pos & 7;

        // Load up to 8 bytes.
        let mut acc: u64 = 0;
        for i in 0..8 {
            let b = self.buf.get(byte_pos + i).copied().unwrap_or(0);
            acc = (acc << 8) | (b as u64);
        }
        // Align so that the current bit is at the top.
        let shift = 64 - 8 * 8 + bit_off; // = bit_off
        let acc = acc << shift;
        let val = (acc >> (64 - n)) as u32;
        val
    }

    #[inline]
    pub fn show_bits_long(&self, n: usize) -> u32 {
        self.show_bits(n)
    }

    #[inline]
    pub fn get_bits(&mut self, n: usize) -> u32 {
        let v = self.show_bits(n);
        self.skip_bits(n);
        v
    }

    /// Show signed bits (two's complement), equivalent to `show_sbits()`.
    #[inline]
    pub fn show_sbits(&self, n: usize) -> i32 {
        debug_assert!(n > 0 && n <= 32);
        let v = self.show_bits(n) as i32;
        Self::sign_extend(v, n)
    }

    /// Get signed bits (two's complement), equivalent to `get_sbits()`.
    #[inline]
    pub fn get_sbits(&mut self, n: usize) -> i32 {
        let v = self.show_sbits(n);
        self.skip_bits(n);
        v
    }

    /// Equivalent to `get_xbits()` for MPEG dc-style signed values.
    #[inline]
    pub fn get_xbits(&mut self, n: usize) -> i32 {
        debug_assert!(n > 0 && n <= 25);
        let v = self.get_bits(n) as i32;
        let thresh = 1 << (n - 1);
        if v < thresh {
            v - ((1 << n) - 1)
        } else {
            v
        }
    }

    #[inline]
    pub fn sign_extend(val: i32, bits: usize) -> i32 {
        debug_assert!(bits > 0 && bits <= 32);
        let shift = 32 - bits;
        (val << shift) >> shift
    }

    /// Equivalent to `skip_1stop_8data_bits()`.
    pub fn skip_1stop_8data_bits(&mut self) -> Result<()> {
        if self.bits_left() <= 0 {
            return Err(DecodeError::InvalidData("slice extra bits"));
        }
        while self.get_bits1() != 0 {
            self.skip_bits(8);
            if self.bits_left() <= 0 {
                return Err(DecodeError::InvalidData("slice extra bits"));
            }
        }
        Ok(())
    }
}
