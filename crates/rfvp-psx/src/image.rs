//! Pictures: the game's NVSG images, made smaller and turned into CLUT images.
//!
//! An NVSG file is an `hzc1` header (`hzc1`, u32 unpacked length, u32 NVSG header length), the
//! NVSG header (type, size, offsets, frame count) and zlib-packed pixels: BGR, BGRA (one or more
//! frames), 8-bit masks or 1-bit glyphs. Color pictures become types `CLUT8` and `CLUT4`, kept
//! uncompressed (the console has no time to inflate them):
//!
//! ```text
//! u16 colors (16 or 256), u16 stride (pixels per row, rounded up to what VRAM uploads need)
//! u16 palette[colors]     PlayStation colors; entry 0 is transparent (see quant.rs)
//! u8  alpha[colors]       each entry's real alpha, for the engine's own use (hit tests,
//!                         masks, parts); the GPU only knows transparent, STP and opaque
//! indices                 frames x height x stride; 4-bit ones two to a byte, low nibble first
//! ```
//!
//! The header keeps the game's metadata (offsets, `u`/`v`) unscaled, as the PSP repack does: the
//! engine scales it by the `scale` in `rfvp.toml`. Masks and glyphs are made smaller and stay
//! as they were (the engine reads their values).

use std::io::{Read, Write};

use anyhow::{Context, Result, bail, ensure};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};

use crate::game::{read_u16, read_u32};
use crate::quant;
use rfvp_image::{Filter, LayoutRule};

/// NVSG types written for the PlayStation.
pub const TYPE_CLUT8: u16 = 0x10;
pub const TYPE_CLUT4: u16 = 0x11;

pub fn is_nvsg(bytes: &[u8]) -> bool {
    bytes.len() >= 16 && &bytes[0..4] == b"hzc1" && &bytes[12..16] == b"NVSG"
}

/// Convert one NVSG file, scaled by `num / den`. `force4` makes every color picture 4-bit
/// (16 colors, dithered): half the VRAM of 8-bit, for prims and UI parts.
#[cfg(test)]
pub fn convert(bytes: &[u8], num: u32, den: u32, force4: bool) -> Result<Vec<u8>> {
    convert_with_options(bytes, num, den, force4, Filter::Area, None)
}

pub fn convert_with_options(
    bytes: &[u8], num: u32, den: u32, force4: bool,
    filter: Filter, layout: Option<&LayoutRule>,
) -> Result<Vec<u8>> {
    let unpacked_len = read_u32(bytes, 4)? as usize;
    let header_len = read_u32(bytes, 8)? as usize;
    let nvsg = &bytes[12..];
    ensure!(header_len >= 32 && header_len <= nvsg.len(), "bad NVSG header length");
    let kind = read_u16(nvsg, 6)?;
    let width = read_u16(nvsg, 8)? as usize;
    let height = read_u16(nvsg, 10)? as usize;
    let frames = (read_u32(nvsg, 20)? as usize).max(1);
    let depth = match kind {
        0 => 3,
        1 | 2 => 4,
        3 | 4 => 1,
        _ => bail!("unknown NVSG type {kind}"),
    };
    let expected = width * height * depth * frames;
    ensure!(unpacked_len == expected, "NVSG length {unpacked_len}, expected {expected}");
    let mut pixels = vec![0u8; expected];
    ZlibDecoder::new(&nvsg[header_len..])
        .read_exact(&mut pixels)
        .context("inflate NVSG pixels")?;

    // A tiny picture (a line, a dot) keeps at least one pixel.
    let factor = if depth == 1 { 1.0 } else { layout.map_or(1.0, |r| r.scale) };
    let new_w = if factor == 1.0 { scale(width, num, den).max(1) }
        else { (width as f64 * num as f64 / den as f64 * factor).round().max(1.0) as usize };
    let new_h = if factor == 1.0 { scale(height, num, den).max(1) }
        else { (height as f64 * num as f64 / den as f64 * factor).round().max(1.0) as usize };
    ensure!(new_w <= u16::MAX as usize && new_h <= u16::MAX as usize, "layout dimensions exceed u16");
    let mut header = bytes[12..12 + header_len].to_vec();
    if depth != 1 {
        if let Some(rule) = layout { rule.patch_header(&mut header); }
    }

    if depth == 1 {
        // Masks and glyphs: smaller, same format.
        let mut out = Vec::with_capacity(new_w * new_h * frames);
        for f in 0..frames {
            let frame = &pixels[f * width * height..(f + 1) * width * height];
            out.extend(resize_gray(frame, width, height, new_w, new_h, kind == 4));
        }
        let mut encoded = write_zlib(&header, new_w, new_h, kind, &out)?;
        if kind == 3 {
            append_mask_coverage(&mut encoded, &out, new_w, new_h, frames)?;
        }
        return Ok(encoded);
    }

    // Color: RGBA, resized premultiplied (as NVSG's 32-bit pixels are stored) so transparent
    // pixels lend no color, then made straight: a half-transparent texel is blended half and
    // half on the console, so it needs its own color.
    let mut rgba = Vec::with_capacity(new_w * new_h * 4 * frames);
    for f in 0..frames {
        let frame = &pixels[f * width * height * depth..(f + 1) * width * height * depth];
        let mut premul = Vec::with_capacity(width * height * 4);
        for p in frame.chunks_exact(depth) {
            let a = if depth == 4 { p[3] } else { 255 };
            premul.extend_from_slice(&[p[2], p[1], p[0], a]);
        }
        let resized = match filter {
            Filter::Area => resize_rgba(&premul, width, height, new_w, new_h),
            Filter::DpidBalanced => rfvp_image::resize_rgba(&premul, width, height, new_w, new_h),
        };
        for p in resized.chunks_exact(4) {
            let a = p[3] as u32;
            if a == 0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let un = |c: u8| ((c as u32 * 255 + a / 2) / a).min(255) as u8;
                rgba.extend_from_slice(&[un(p[0]), un(p[1]), un(p[2]), a as u8]);
            }
        }
    }

    // 4 bits when 15 colors say it all, or when asked (prims) and the mask survives 16 colors
    // (no alpha off by more than a level); 8 bits (dithered) otherwise.
    let small = quant::quantize(&rgba, new_w, 16, false);
    let exact16 = small.palette.len() <= 16 && quant::alpha_error(&rgba, &small) <= 8 && {
        let distinct: std::collections::HashSet<(u16, u8)> = rgba
            .chunks_exact(4)
            .filter(|p| quant::level_of(p[3]) != 0)
            .map(|p| ((p[0] as u16 >> 3) | (p[1] as u16 >> 3) << 5 | (p[2] as u16 >> 3) << 10, quant::level_of(p[3])))
            .collect();
        distinct.len() < 16
    };
    let forced = if force4 && !exact16 {
        let q = quant::quantize(&rgba, new_w, 16, true);
        (quant::alpha_error(&rgba, &q) <= 64).then_some(q)
    } else {
        None
    };
    let (q, colors, kind_out, stride) = if exact16 {
        (small, 16, TYPE_CLUT4, new_w.next_multiple_of(4))
    } else if let Some(q) = forced {
        (q, 16, TYPE_CLUT4, new_w.next_multiple_of(4))
    } else {
        (quant::quantize(&rgba, new_w, 256, true), 256, TYPE_CLUT8, new_w.next_multiple_of(2))
    };

    let mut payload = Vec::new();
    payload.extend_from_slice(&(colors as u16).to_le_bytes());
    payload.extend_from_slice(&(stride as u16).to_le_bytes());
    for i in 0..colors {
        payload.extend_from_slice(&q.palette.get(i).copied().unwrap_or(0).to_le_bytes());
    }
    for i in 0..colors {
        payload.push(q.alpha.get(i).copied().unwrap_or(0));
    }
    for row in q.indices.chunks_exact(new_w) {
        let mut padded = row.to_vec();
        padded.resize(stride, 0);
        if kind_out == TYPE_CLUT4 {
            payload.extend(padded.chunks_exact(2).map(|p| p[0] | (p[1] << 4)));
        } else {
            payload.extend_from_slice(&padded);
        }
    }
    let encoded = write_raw(&header, new_w, new_h, kind_out, &payload);
    if filter == Filter::DpidBalanced {
        // Compare the actual palette, packed rows and DMA-padded GPU tiles.
        // A layout can intentionally spend more pixels; filtering cannot add
        // cost beyond Area encoded with that same layout.
        let baseline = convert_with_options(bytes, num, den, force4, Filter::Area, layout)?;
        if encoded.len() > baseline.len() || vram_bytes(&encoded)? > vram_bytes(&baseline)? {
            return Ok(baseline);
        }
    }
    Ok(encoded)
}

/// Preserve the Area alpha plane and append offline, GPU-ready Bayer coverage.
fn append_mask_coverage(encoded: &mut Vec<u8>, alpha: &[u8], width: usize, height: usize, frames: usize) -> Result<()> {
    use crate::mask_format as mask;
    let stride = width.next_multiple_of(2);
    ensure!(stride <= u16::MAX as usize, "PS1 mask stride exceeds u16");
    let start = encoded.len();
    encoded.extend_from_slice(mask::HEADER_MAGIC);
    encoded.extend_from_slice(&(width as u16).to_le_bytes());
    encoded.extend_from_slice(&(height as u16).to_le_bytes());
    encoded.extend_from_slice(&(frames as u32).to_le_bytes());
    encoded.extend_from_slice(&(stride as u16).to_le_bytes());
    encoded.extend_from_slice(&[0, 0]);
    for frame in alpha.chunks_exact(width * height) {
        encoded.extend_from_slice(&256u16.to_le_bytes());
        encoded.extend_from_slice(&(stride as u16).to_le_bytes());
        for index in 0..256 {
            encoded.extend_from_slice(&mask::PALETTE[mask::palette_state(index as u8, 255) as usize].to_le_bytes());
        }
        for y in 0..height {
            for x in 0..stride {
                encoded.push(if x < width { mask::coverage_index(frame[y * width + x], x, y) } else { 0 });
            }
        }
    }
    let length = u32::try_from(encoded.len() - start).context("PS1 mask trailer exceeds u32")?;
    encoded.extend_from_slice(&length.to_le_bytes());
    encoded.extend_from_slice(mask::FOOTER_MAGIC);
    Ok(())
}

pub fn vram_bytes(encoded: &[u8]) -> Result<usize> {
    if let Some(mask) = crate::mask_format::read(encoded).map_err(anyhow::Error::msg)? {
        let rows: usize = (0..usize::from(mask.width)).step_by(126).map(|x| {
            (usize::from(mask.width) - x).min(126).div_ceil(2).next_multiple_of(2) * 2
        }).sum();
        return Ok((rows * usize::from(mask.height) + 512) * mask.frames as usize);
    }
    let h = &encoded[12..];
    let kind = read_u16(h, 6)?;
    let width = read_u16(h, 8)? as usize;
    let height = read_u16(h, 10)? as usize;
    let frames = (read_u32(h, 20)? as usize).max(1);
    let (per, tile_w, colors) = if kind == TYPE_CLUT4 { (4, 236, 16) }
        else if kind == TYPE_CLUT8 { (2, 126, 256) } else { return Ok(encoded.len()); };
    let rows: usize = (0..width).step_by(tile_w).map(|x| {
        let texels = (width - x).min(tile_w);
        texels.div_ceil(per).next_multiple_of(2) * 2
    }).sum();
    Ok((rows * height + colors * 4) * frames)
}

fn write_raw(header: &[u8], width: usize, height: usize, kind: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + header.len() + payload.len());
    out.extend_from_slice(b"hzc1");
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out.extend_from_slice(header);
    out[12 + 6..12 + 8].copy_from_slice(&kind.to_le_bytes());
    out[12 + 8..12 + 10].copy_from_slice(&(width as u16).to_le_bytes());
    out[12 + 10..12 + 12].copy_from_slice(&(height as u16).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

fn write_zlib(header: &[u8], width: usize, height: usize, kind: u16, pixels: &[u8]) -> Result<Vec<u8>> {
    let mut z = ZlibEncoder::new(Vec::new(), Compression::best());
    z.write_all(pixels)?;
    let packed = z.finish()?;
    let mut out = write_raw(header, width, height, kind, &packed);
    out[4..8].copy_from_slice(&(pixels.len() as u32).to_le_bytes());
    Ok(out)
}

fn scale(v: usize, num: u32, den: u32) -> usize {
    ((v as u64 * num as u64 * 2 + den as u64) / (den as u64 * 2)) as usize
}

/// Area-averaging resize of RGBA pixels.
fn resize_rgba(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    let mut out = vec![0u8; dw * dh * 4];
    for dy in 0..dh {
        let (y0, y1) = (dy * sh, (dy + 1) * sh);
        for dx in 0..dw {
            let (x0, x1) = (dx * sw, (dx + 1) * sw);
            let mut sum = [0u64; 4];
            let mut total = 0u64;
            for sy in y0 / dh..y1.div_ceil(dh) {
                let wy = (y1.min((sy + 1) * dh) - y0.max(sy * dh)) as u64;
                for sx in x0 / dw..x1.div_ceil(dw) {
                    let wx = (x1.min((sx + 1) * dw) - x0.max(sx * dw)) as u64;
                    let w = wx * wy;
                    let i = (sy * sw + sx) * 4;
                    for c in 0..4 {
                        sum[c] += src[i + c] as u64 * w;
                    }
                    total += w;
                }
            }
            let o = (dy * dw + dx) * 4;
            for c in 0..4 {
                out[o + c] = ((sum[c] + total / 2) / total.max(1)) as u8;
            }
        }
    }
    out
}

/// Area-averaging resize of one-byte pixels; `binary` keeps 1-bit glyphs at 0 or 1.
fn resize_gray(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize, binary: bool) -> Vec<u8> {
    let mut out = vec![0u8; dw * dh];
    for dy in 0..dh {
        let (y0, y1) = (dy * sh, (dy + 1) * sh);
        for dx in 0..dw {
            let (x0, x1) = (dx * sw, (dx + 1) * sw);
            let (mut sum, mut total) = (0u64, 0u64);
            for sy in y0 / dh..y1.div_ceil(dh) {
                let wy = (y1.min((sy + 1) * dh) - y0.max(sy * dh)) as u64;
                for sx in x0 / dw..x1.div_ceil(dw) {
                    let wx = (x1.min((sx + 1) * dw) - x0.max(sx * dw)) as u64;
                    let v = src[sy * sw + sx] as u64;
                    sum += if binary { (v != 0) as u64 } else { v } * wx * wy;
                    total += wx * wy;
                }
            }
            out[dy * dw + dx] = if binary {
                (sum * 2 >= total) as u8
            } else {
                ((sum + total / 2) / total.max(1)) as u8
            };
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nvsg(kind: u16, width: u16, height: u16, frames: u32, pixels: &[u8]) -> Vec<u8> {
        let mut header = vec![0u8; 32];
        header[0..4].copy_from_slice(b"NVSG");
        header[6..8].copy_from_slice(&kind.to_le_bytes());
        header[8..10].copy_from_slice(&width.to_le_bytes());
        header[10..12].copy_from_slice(&height.to_le_bytes());
        header[20..24].copy_from_slice(&frames.to_le_bytes());
        write_zlib(&header, width as usize, height as usize, kind, pixels).unwrap()
    }

    #[test]
    fn two_color_picture_becomes_clut4() {
        // 8x4 premultiplied BGRA, left half red, right half transparent; scaled by 1/2.
        let mut px = Vec::new();
        for _y in 0..4 {
            for x in 0..8 {
                px.extend_from_slice(if x < 4 { &[0, 0, 255, 255] } else { &[0, 0, 0, 0] });
            }
        }
        let out = convert(&nvsg(1, 8, 4, 1, &px), 1, 2, false).unwrap();
        assert_eq!(read_u16(&out[12..], 6).unwrap(), TYPE_CLUT4);
        assert_eq!(read_u16(&out[12..], 8).unwrap(), 4);
        assert_eq!(read_u16(&out[12..], 10).unwrap(), 2);
        let payload = &out[12 + 32..];
        assert_eq!(read_u16(payload, 0).unwrap(), 16);
        assert_eq!(read_u16(payload, 2).unwrap(), 4);
        let red = read_u16(payload, 4 + 2).unwrap();
        assert_eq!(red, 0x001f);
        assert_eq!(payload[4 + 32 + 1], 255, "its alpha");
        // First row: two red pixels, two transparent.
        assert_eq!(payload[4 + 48], 0x11);
        assert_eq!(payload[4 + 49], 0x00);
    }

    #[test]
    fn photo_becomes_clut8() {
        let mut px = Vec::new();
        for y in 0..32u32 {
            for x in 0..32u32 {
                px.extend_from_slice(&[(x * 8) as u8, (y * 8) as u8, ((x + y) * 4) as u8]);
            }
        }
        let out = convert(&nvsg(0, 32, 32, 1, &px), 1, 1, false).unwrap();
        assert_eq!(read_u16(&out[12..], 6).unwrap(), TYPE_CLUT8);
        let payload = &out[12 + 32..];
        assert_eq!(read_u16(payload, 0).unwrap(), 256);
        assert_eq!(payload.len(), 4 + 512 + 256 + 32 * 32);
    }

    #[test]
    fn dpid_preserves_metadata_frames_and_encoding_budget() {
        let mut px = Vec::new();
        for frame in 0..2 {
            for i in 0..77 {
                let a = ((i * 17 + frame * 53) % 256) as u8;
                px.extend_from_slice(&[a / 3, a / 2, a, a]);
            }
        }
        let mut input = nvsg(2, 11, 7, 2, &px);
        input[24..32].copy_from_slice(&[3, 0, 4, 0, 5, 0, 6, 0]);
        let area = convert_with_options(&input, 1, 2, true, Filter::Area, None).unwrap();
        let dpid = convert_with_options(&input, 1, 2, true, Filter::DpidBalanced, None).unwrap();
        assert_eq!(&dpid[24..36], &input[24..36]);
        assert!(dpid.len() <= area.len());
        assert!(vram_bytes(&dpid).unwrap() <= vram_bytes(&area).unwrap());
    }

    #[test]
    fn layout_changes_geometry_but_never_masks() {
        let rule = LayoutRule { scale: 2.0, offset_x: Some(90), offset_y: Some(120) };
        let input = nvsg(1, 12, 8, 1, &[0, 0, 255, 255].repeat(96));
        let out = convert_with_options(&input, 1, 2, true, Filter::DpidBalanced, Some(&rule)).unwrap();
        assert_eq!(read_u16(&out, 20).unwrap(), 12);
        assert_eq!(read_u16(&out, 22).unwrap(), 8);
        assert_eq!(read_u16(&out, 24).unwrap(), 90);
        assert_eq!(read_u16(&out, 26).unwrap(), 120);
        let mask = nvsg(3, 12, 8, 1, &[128; 96]);
        let out = convert_with_options(&mask, 1, 2, true, Filter::DpidBalanced, Some(&rule)).unwrap();
        assert_eq!(out, convert(&mask, 1, 2, true).unwrap());
    }
}
