//! Fast RGBA8 downscale for the PSV renderer.
//!
//! The old host path converted the whole image to `u32`, then ran a per-destination-pixel
//! area filter with `u64` divisions. On a 1600x1600 background at 0.75 scale that costs
//! seconds on the Vita. This module keeps the same area/alpha weights, but works directly on
//! bytes, normalizes each axis once, and removes the per-pixel divisions with a reciprocal
//! table. Opaque images (backgrounds) take a cheaper path with no alpha arithmetic.
use alloc::vec::Vec;

struct Span {
    start: u32,
    weights: Vec<u32>,
}

/// Source spans of each destination pixel, with weights normalized to 65536 per axis.
/// Normalizing once keeps the 2D loop to shifts and reciprocals.
fn spans(source: u32, destination: u32, first_pixel: u32, count: u32) -> Vec<Span> {
    let source_fp = (source as u64) << 16;
    let ratio = source_fp / destination as u64;
    (first_pixel as u64..first_pixel as u64 + count as u64)
        .map(|i| {
            let start = i * ratio;
            let end = (start + ratio).min(source_fp);
            let first = (start >> 16) as u32;
            let last = (((end + 0xffff) >> 16) as u32).max(first + 1) - 1;
            let total = end.saturating_sub(start).max(1);
            let mut weights: Vec<u32> = (first..=last)
                .map(|j| {
                    let lo = ((j as u64) << 16).max(start);
                    let hi = (((j + 1) as u64) << 16).min(end);
                    ((hi.saturating_sub(lo) * 65536) / total) as u32
                })
                .collect();
            // Make each axis sum exactly 65536 so the 2D loop can use shifts, not divisions.
            let sum: u32 = weights.iter().sum();
            if let Some(last) = weights.last_mut() {
                if sum < 65536 {
                    *last += 65536 - sum;
                } else {
                    *last = last.saturating_sub(sum - 65536);
                }
            }
            Span { start: first, weights }
        })
        .collect()
}

/// `pr / pa` with `pr` premultiplied by alpha. `pa` is 0..=255; the table removes the
/// division from the pixel loop. 24 fractional bits keep 255/1 accurate.
const fn reciprocal() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 1usize;
    while i < 256 {
        table[i] = (((1u64 << 24) + i as u64 / 2) / i as u64) as u32;
        i += 1;
    }
    table
}

static RECIPROCAL: [u32; 256] = reciprocal();

/// Downscale straight-alpha RGBA8 pixels. Returns `None` for zero dimensions, upscaling,
/// short input, or an overflowing byte count. The output is `dst_width * dst_height * 4`.
pub fn downscale_rgba8(
    src: &[u8],
    src_width: u32,
    src_height: u32,
    dst_width: u32,
    dst_height: u32,
) -> Option<Vec<u8>> {
    downscale_rgba8_region(src, src_width, src_height, dst_width, dst_height,
        0, 0, dst_width, dst_height)
}

/// Recompute a destination rectangle using the SAME global spans and alpha math
/// as a full downscale. Source pixels include the complete image/row stride.
pub fn downscale_rgba8_region(
    src: &[u8], src_width: u32, src_height: u32, dst_width: u32, dst_height: u32,
    x: u32, y: u32, width: u32, height: u32,
) -> Option<Vec<u8>> {
    let source_bytes = (src_width as usize).checked_mul(src_height as usize)?.checked_mul(4)?;
    if src.len() < source_bytes { return None; }
    let opaque = src[..source_bytes].chunks_exact(4).all(|p| p[3] == 255);
    downscale_rgba8_region_with_opacity(src, src_width, src_height, dst_width, dst_height,
        x, y, width, height, opaque)
}

/// The caller has scanned this same source for alpha already. Reuse that result
/// instead of scanning the complete character image a second time for a face patch.
pub fn downscale_rgba8_region_with_opacity(
    src: &[u8], src_width: u32, src_height: u32, dst_width: u32, dst_height: u32,
    x: u32, y: u32, width: u32, height: u32, opaque: bool,
) -> Option<Vec<u8>> {
    if src_width == 0 || src_height == 0 || dst_width == 0 || dst_height == 0
        || dst_width > src_width || dst_height > src_height
    {
        return None;
    }
    if width == 0 || height == 0 || x.checked_add(width)? > dst_width
        || y.checked_add(height)? > dst_height { return None; }
    let source_bytes = (src_width as usize)
        .checked_mul(src_height as usize)?
        .checked_mul(4)?;
    if src.len() < source_bytes {
        return None;
    }
    let output_bytes = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    let sw = src_width as usize;
    let columns = spans(src_width, dst_width, x, width);
    let rows = spans(src_height, dst_height, y, height);
    let mut out = Vec::with_capacity(output_bytes);
    for row in &rows {
        for column in &columns {
            if opaque {
                let (mut sum_r, mut sum_g, mut sum_b) = (0u32, 0u32, 0u32);
                for (j, wy) in row.weights.iter().enumerate() {
                    let row_start = ((row.start as usize + j).min(src_height as usize - 1)) * sw;
                    for (i, wx) in column.weights.iter().enumerate() {
                        let p = (row_start + (column.start as usize + i).min(sw - 1)) * 4;
                        let w = ((*wy as u64 * *wx as u64) >> 16) as u32;
                        sum_r += src[p] as u32 * w;
                        sum_g += src[p + 1] as u32 * w;
                        sum_b += src[p + 2] as u32 * w;
                    }
                }
                out.push((sum_r.saturating_add(32768) >> 16) as u8);
                out.push((sum_g.saturating_add(32768) >> 16) as u8);
                out.push((sum_b.saturating_add(32768) >> 16) as u8);
                out.push(255);
            } else {
                let (mut sum_a, mut sum_r, mut sum_g, mut sum_b) = (0u32, 0u32, 0u32, 0u32);
                for (j, wy) in row.weights.iter().enumerate() {
                    let row_start = ((row.start as usize + j).min(src_height as usize - 1)) * sw;
                    for (i, wx) in column.weights.iter().enumerate() {
                        let p = (row_start + (column.start as usize + i).min(sw - 1)) * 4;
                        let a = src[p + 3] as u32;
                        if a == 0 {
                            continue;
                        }
                        let w = ((*wy as u64 * *wx as u64) >> 16) as u32 * a;
                        sum_a += w;
                        sum_r += src[p] as u32 * w;
                        sum_g += src[p + 1] as u32 * w;
                        sum_b += src[p + 2] as u32 * w;
                    }
                }
                let pa = ((sum_a.saturating_add(32768)) >> 16) as usize;
                if pa == 0 {
                    out.extend_from_slice(&[0, 0, 0, 0]);
                    continue;
                }
                let reciprocal = RECIPROCAL[pa] as u64;
                let unpremultiply = |sum: u32| -> u8 {
                    let premultiplied = (sum.saturating_add(32768) >> 16) as u64;
                    (((premultiplied * reciprocal + (1 << 23)) >> 24).min(255)) as u8
                };
                out.push(unpremultiply(sum_r));
                out.push(unpremultiply(sum_g));
                out.push(unpremultiply(sum_b));
                out.push(pa as u8);
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn constant_color_is_preserved() {
        let src = [31u8, 64, 128, 255].repeat(8 * 6);
        let out = downscale_rgba8(&src, 8, 6, 4, 3).unwrap();
        assert_eq!(out, [31u8, 64, 128, 255].repeat(4 * 3));
    }

    #[test]
    fn transparent_pixels_do_not_bleed_their_rgb() {
        // One transparent red pixel and three opaque blue pixels become one blue pixel.
        let mut src = [0u8; 2 * 2 * 4];
        src[0..4].copy_from_slice(&[255, 0, 0, 0]);
        for p in 1..4 {
            src[p * 4..p * 4 + 4].copy_from_slice(&[0, 0, 255, 255]);
        }
        let out = downscale_rgba8(&src, 2, 2, 1, 1).unwrap();
        assert_eq!(&out[0..3], &[0, 0, 255]);
        // One of four samples is transparent, so coverage is 3/4: no red bleed, but alpha 191.
        assert_eq!(out[3], 191);
    }

    #[test]
    fn fully_transparent_input_stays_transparent() {
        let src = vec![0u8; 4 * 4 * 4];
        assert_eq!(downscale_rgba8(&src, 4, 4, 2, 2).unwrap(), vec![0u8; 2 * 2 * 4]);
    }

    #[test]
    fn one_pixel_axes_and_odd_sizes_work() {
        let src = [10u8, 20, 30, 255].repeat(5);
        let out = downscale_rgba8(&src, 5, 1, 1, 1).unwrap();
        assert_eq!(out, [10u8, 20, 30, 255]);
        let src = [10u8, 20, 30, 255].repeat(7);
        let out = downscale_rgba8(&src, 1, 7, 1, 3).unwrap();
        assert_eq!(out.len(), 3 * 4);
        assert!(out.chunks_exact(4).all(|p| p[0..3] == [10, 20, 30] && p[3] == 255));
    }

    #[test]
    fn invalid_or_short_inputs_are_rejected() {
        let src = vec![0u8; 16];
        assert!(downscale_rgba8(&src, 0, 2, 1, 1).is_none());
        assert!(downscale_rgba8(&src, 2, 2, 3, 1).is_none());
        assert!(downscale_rgba8(&src[..15], 2, 2, 1, 1).is_none());
        assert!(downscale_rgba8(&src, u32::MAX, u32::MAX, 1, 1).is_none());
    }

    #[test]
    fn matches_the_old_area_filter_on_a_transparent_fixture() {
        // Reference: the shipped area filter with unnormalized 16.16 weights.
        fn spans(source: u32, destination: u32) -> Vec<(u32, Vec<u64>)> {
            let source_fp = (source as u64) << 16;
            let ratio = source_fp / destination as u64;
            (0..destination as u64)
                .map(|i| {
                    let start = i * ratio;
                    let end = (start + ratio).min(source_fp);
                    let first = (start >> 16) as u32;
                    let last = (((end + 0xffff) >> 16) as u32).max(first + 1) - 1;
                    let weights = (first..=last)
                        .map(|j| {
                            let lo = ((j as u64) << 16).max(start);
                            let hi = (((j + 1) as u64) << 16).min(end);
                            hi.saturating_sub(lo)
                        })
                        .collect();
                    (first, weights)
                })
                .collect()
        }
        fn reference(src: &[u8], width: u32, height: u32, dw: u32, dh: u32) -> Vec<u8> {
            let cols = spans(width, dw);
            let rows = spans(height, dh);
            let mut out = Vec::new();
            for (y0, ys) in &rows {
                for (x0, xs) in &cols {
                    let (mut sr, mut sg, mut sb, mut sa, mut sw) = (0u64, 0u64, 0u64, 0u64, 0u64);
                    for (j, wy) in ys.iter().enumerate() {
                        let row = ((*y0 as usize + j).min(height as usize - 1)) * width as usize;
                        for (i, wx) in xs.iter().enumerate() {
                            let p = (row + (*x0 as usize + i).min(width as usize - 1)) * 4;
                            let w = (wy * wx) >> 16;
                            let a = src[p + 3] as u64;
                            let wa = w * a;
                            sr += src[p] as u64 * wa;
                            sg += src[p + 1] as u64 * wa;
                            sb += src[p + 2] as u64 * wa;
                            sa += wa;
                            sw += w;
                        }
                    }
                    if sa == 0 || sw == 0 {
                        out.extend_from_slice(&[0, 0, 0, 0]);
                    } else {
                        out.push(((sr + sa / 2) / sa) as u8);
                        out.push(((sg + sa / 2) / sa) as u8);
                        out.push(((sb + sa / 2) / sa) as u8);
                        out.push((((sa + sw / 2) / sw).min(255)) as u8);
                    }
                }
            }
            out
        }
        let mut src = Vec::new();
        for i in 0..7 * 5 {
            src.extend_from_slice(&[
                (i * 37 % 256) as u8,
                (i * 53 % 256) as u8,
                (i * 71 % 256) as u8,
                if i % 3 == 0 { 0 } else { (i * 29 % 256) as u8 },
            ]);
        }
        let old = reference(&src, 7, 5, 5, 4);
        let new = downscale_rgba8(&src, 7, 5, 5, 4).unwrap();
        assert_eq!(old.len(), new.len());
        let max_diff = old.iter().zip(&new).map(|(a, b)| (*a).abs_diff(*b)).max().unwrap();
        assert!(max_diff <= 2, "max channel difference {max_diff}");
    }

    #[test]
    fn edge_pixels_stay_in_bounds() {
        let src: Vec<u8> = (0..4 * 4).flat_map(|i| [i as u8, 0, 0, 255]).collect();
        for (dw, dh) in [(1, 1), (3, 3), (4, 4), (2, 3), (3, 2)] {
            let out = downscale_rgba8(&src, 4, 4, dw, dh).unwrap();
            assert_eq!(out.len(), (dw * dh * 4) as usize);
        }
    }
}
