/// VC-1 / WMV9 Bitstream Reader

#[derive(Clone)]
pub struct BitReader<'a> {
    data:     &'a [u8],
    byte_pos: usize,
    bit_pos:  u8,
    current:  u8,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let current = data.first().copied().unwrap_or(0);
        BitReader { data, byte_pos: 0, bit_pos: 8, current }
    }

    /// Create a reader positioned at an arbitrary bit offset (MSB-first).
    ///
    /// `bit_offset=0` is identical to `BitReader::new(data)`.
    pub fn new_at(data: &'a [u8], bit_offset: usize) -> Self {
        let byte_pos = bit_offset / 8;
        let bit_in_byte = (bit_offset % 8) as u8;
        // bit_pos = how many bits remain unread in the current byte.
        let bit_pos = if bit_in_byte == 0 { 8 } else { 8 - bit_in_byte };
        let current = data.get(byte_pos).copied().unwrap_or(0);
        BitReader { data, byte_pos, bit_pos, current }
    }

    pub fn read_bits(&mut self, mut n: u8) -> Option<u32> {
        if n == 0 { return Some(0); }
        let mut result = 0u32;
        while n > 0 {
            if self.byte_pos >= self.data.len() { return None; }
            let avail = self.bit_pos.min(n);
            let shift = self.bit_pos - avail;
            let mask  = ((1u16 << avail) - 1) as u8;
            result    = (result << avail) | ((self.current >> shift) & mask) as u32;
            self.bit_pos -= avail;
            n            -= avail;
            if self.bit_pos == 0 {
                self.byte_pos += 1;
                self.current   = self.data.get(self.byte_pos).copied().unwrap_or(0);
                self.bit_pos   = 8;
            }
        }
        Some(result)
    }

    #[inline]
    pub fn read_bit(&mut self) -> Option<bool> {
        self.read_bits(1).map(|b| b != 0)
    }

    pub fn read_bits_signed(&mut self, n: u8) -> Option<i32> {
        let v = self.read_bits(n)? as i32;
        if n == 0 { return Some(0); }
        let sign = 1i32 << (n - 1);
        Some(if v & sign != 0 { v - (sign << 1) } else { v })
    }

    /// Peek up to 24 bits without advancing.
    pub fn peek_bits(&self, n: u8) -> Option<u32> {
        if n == 0 { return Some(0); }
        let mut result    = 0u32;
        let mut bits_left = n;
        let mut bpos      = self.byte_pos;
        let mut boff      = self.bit_pos;

        while bits_left > 0 {
            let avail = boff.min(bits_left);
            let shift = boff - avail;
            let mask  = ((1u16 << avail) - 1) as u8;
            let byte  = if bpos < self.data.len() { self.data[bpos] } else { 0 };
            result    = (result << avail) | ((byte >> shift) & mask) as u32;
            bits_left -= avail;
            boff      -= avail;
            if boff == 0 { bpos += 1; boff = 8; }
        }
        Some(result)
    }

    #[inline]
    pub fn skip_bits(&mut self, n: u8) {
        let _ = self.read_bits(n);
    }

    /// Skip an arbitrary number of bits.
    #[inline]
    pub fn skip_bits_usize(&mut self, mut n: usize) {
        while n >= 32 {
            let _ = self.read_bits(32);
            n -= 32;
        }
        if n > 0 {
            let _ = self.read_bits(n as u8);
        }
    }

    pub fn read_ue(&mut self) -> Option<u32> {
        let mut zeros = 0u8;
        while !self.read_bit()? {
            zeros += 1;
            if zeros > 31 { return None; }
        }
        let suffix = self.read_bits(zeros)?;
        Some((1u32 << zeros) - 1 + suffix)
    }

    pub fn read_se(&mut self) -> Option<i32> {
        let ue = self.read_ue()?;
        let v  = ((ue + 1) >> 1) as i32;
        Some(if ue & 1 == 0 { -v } else { v })
    }

    pub fn byte_align(&mut self) {
        if self.bit_pos < 8 {
            self.byte_pos += 1;
            self.current   = self.data.get(self.byte_pos).copied().unwrap_or(0);
            self.bit_pos   = 8;
        }
    }

    pub fn remaining_bytes(&self) -> usize {
        self.data.len().saturating_sub(self.byte_pos)
    }

    pub fn bits_read(&self) -> usize {
        self.byte_pos * 8 + (8 - self.bit_pos as usize)
    }

    /// Remaining bits in the underlying buffer.
    pub fn bits_left(&self) -> isize {
        let total = (self.data.len() * 8) as isize;
        let used = self.bits_read() as isize;
        total - used
    }

    pub fn is_empty(&self) -> bool {
        self.byte_pos >= self.data.len()
    }
}
