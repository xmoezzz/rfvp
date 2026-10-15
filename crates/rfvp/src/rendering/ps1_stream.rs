//! Bounded RAM for large PS1 CLUT pictures. Pixel indices stay on the disc.
use alloc::{sync::Arc, vec, vec::Vec};
use anyhow::{Result, ensure};
use crate::subsystem::resources::vfs::Vfs;
use std::io::{Read, Seek, SeekFrom};

#[derive(Clone, Debug)]
pub(crate) struct StreamedClut {
    pub palette: crate::ClutImage,
    pub indices_at: u64,
    pub row_bytes: usize,
}

impl StreamedClut {
    pub fn load(vfs: &Vfs, path: &str) -> Result<Option<(Self, [u8; 44], Vec<u64>)>> {
        let (mut reader, length) = vfs.open_stream_with_len(path)?;
        if length.is_some_and(|length| length < 64 * 1024) { return Ok(None); }
        let mut header = [0u8; 44];
        reader.read_exact(&mut header)?;
        let u16_at = |at| u16::from_le_bytes([header[at], header[at + 1]]);
        let u32_at = |at| u32::from_le_bytes(header[at..at + 4].try_into().unwrap());
        let kind = u16_at(18);
        if &header[..4] != b"hzc1" || &header[12..16] != b"NVSG" || !matches!(kind, 16 | 17) {
            return Ok(None);
        }
        ensure!(u32_at(8) >= 32, "invalid streamed NVSG header");
        let width = u32::from(u16_at(20));
        let height = u32::from(u16_at(22));
        ensure!(width != 0 && height != 0, "empty streamed texture");
        let payload = 12 + u64::from(u32_at(8));
        reader.seek(SeekFrom::Start(payload))?;
        let mut fields = [0u8; 4];
        reader.read_exact(&mut fields)?;
        let colors = u16::from_le_bytes([fields[0], fields[1]]) as usize;
        let stride = u16::from_le_bytes([fields[2], fields[3]]) as usize;
        let bits = if kind == 17 { 4 } else { 8 };
        ensure!(colors == if bits == 4 { 16 } else { 256 }, "invalid streamed CLUT");
        ensure!(stride >= width as usize && stride % (if bits == 4 { 4 } else { 2 }) == 0, "invalid streamed stride");
        let row_bytes = if bits == 4 { stride / 2 } else { stride };
        let indices_at = payload + (4 + colors * 3) as u64;
        ensure!(length.is_none_or(|length| indices_at + row_bytes as u64 * u64::from(height) <= length), "truncated streamed CLUT");
        let mut bytes = vec![0u8; 4 + colors * 2];
        bytes[..4].copy_from_slice(&fields);
        reader.read_exact(&mut bytes[4..])?;
        let mut alpha = vec![0u8; colors];
        reader.read_exact(&mut alpha)?;
        let mut hits: Vec<u64> = Vec::new();
        // Only a palette with fully transparent colours can give a hit plane: an opaque picture
        // (most backgrounds) needs none of its pixels read here.
        if alpha.iter().any(|&a| a == 0) {
            let mut clear = [false; 256];
            for (i, &a) in alpha.iter().enumerate() {
                clear[i] = a == 0;
            }
            let words = (width as usize * height as usize).div_ceil(64);
            // The plane in 32-bit halves (the R3000 has no 64-bit shifts): little endian, so
            // bit k of the u64 plane is bit k % 32 of half k / 32.
            let mut halves: Vec<u32> = Vec::new();
            let mut row = vec![0u8; row_bytes];
            for y in 0..height as usize {
                reader.read_exact(&mut row)?;
                let base = y * width as usize;
                for x in 0..width as usize {
                    let index = if bits == 4 { (row[x / 2] >> ((x & 1) * 4)) & 15 } else { row[x] };
                    if clear[index as usize] {
                        // Allocated only once a transparent pixel is actually used.
                        if halves.is_empty() {
                            halves = vec![u32::MAX; words * 2];
                        }
                        let pixel = base + x;
                        halves[pixel / 32] &= !(1u32 << (pixel & 31));
                    }
                }
            }
            if !halves.is_empty() {
                hits = halves.chunks_exact(2).map(|h| h[0] as u64 | (h[1] as u64) << 32).collect();
            }
        }
        Ok(Some((Self {
            palette: crate::ClutImage { width, height, stride: stride as u32, bits, bytes, alpha: Arc::new(alpha) },
            indices_at, row_bytes,
        }, header, hits)))
    }
}
