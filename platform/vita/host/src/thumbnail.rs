//! Small save previews sampled directly from the opaque display buffer.
//! Bilinear sampling uses four source pixels per output pixel, no full-frame copy.
use alloc::vec::Vec;

fn axis(source: usize, destination: usize, index: usize) -> (usize, usize, u32) {
    // Pixel-center mapping, with 8 fractional bits; division is outside the pixel loop.
    let position = (((2 * index as u64 + 1) * source as u64 * 128) / destination as u64)
        .saturating_sub(128).min((source as u64 - 1) * 256);
    let low = (position >> 8) as usize;
    (low, (low + 1).min(source - 1), (position & 255) as u32)
}

/// Crop coordinates are (left, top, right, bottom) in a packed RGBA8 framebuffer.
/// Intended for opaque display pixels, not for transparent sprite resampling.
pub fn capture_rgba8(
    frame: &[u8], frame_width: usize, frame_height: usize,
    crop: (usize, usize, usize, usize), width: u32, height: u32,
) -> Option<Vec<u8>> {
    let frame_bytes = frame_width.checked_mul(frame_height)?.checked_mul(4)?;
    let (left, top, right, bottom) = crop;
    if width == 0 || height == 0 || frame.len() < frame_bytes
        || left >= right || top >= bottom || right > frame_width || bottom > frame_height {
        return None;
    }
    let (width, height) = (width as usize, height as usize);
    let output_bytes = width.checked_mul(height)?.checked_mul(4)?;
    let columns: Vec<_> = (0..width).map(|x| axis(right - left, width, x)).collect();
    let mut out = Vec::with_capacity(output_bytes);
    for y in 0..height {
        let (y0, y1, wy) = axis(bottom - top, height, y);
        let row0 = (top + y0) * frame_width;
        let row1 = (top + y1) * frame_width;
        for &(x0, x1, wx) in &columns {
            let p00 = (row0 + left + x0) * 4;
            let p01 = (row0 + left + x1) * 4;
            let p10 = (row1 + left + x0) * 4;
            let p11 = (row1 + left + x1) * 4;
            for channel in 0..4 {
                let upper = frame[p00 + channel] as u32 * (256 - wx) + frame[p01 + channel] as u32 * wx;
                let lower = frame[p10 + channel] as u32 * (256 - wx) + frame[p11 + channel] as u32 * wx;
                out.push(((upper * (256 - wy) + lower * wy + 32768) >> 16) as u8);
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
    fn constant_frame_and_alpha_are_preserved() {
        let frame = [31, 64, 128, 255].repeat(8 * 6);
        let output = capture_rgba8(&frame, 8, 6, (2, 1, 6, 5), 3, 3).unwrap();
        assert_eq!(output, [31, 64, 128, 255].repeat(9));
    }

    #[test]
    fn center_uses_all_four_neighbors_with_rounding() {
        let frame: Vec<u8> = [0, 64, 128, 255].iter().flat_map(|&v| [v, v, v, 255]).collect();
        assert_eq!(capture_rgba8(&frame, 2, 2, (0, 0, 2, 2), 1, 1).unwrap(), vec![112, 112, 112, 255]);
    }

    #[test]
    fn crop_respects_frame_stride_and_excludes_borders() {
        let frame: Vec<u8> = (0..16).flat_map(|v| [v, 0, 0, 255]).collect();
        let output = capture_rgba8(&frame, 4, 4, (1, 1, 3, 3), 2, 2).unwrap();
        let expected: Vec<u8> = [5, 6, 9, 10].iter().flat_map(|&v| [v, 0, 0, 255]).collect();
        assert_eq!(output, expected);
    }

    #[test]
    fn invalid_sizes_or_crop_return_none() {
        let frame = vec![255; 4 * 4 * 4];
        assert!(capture_rgba8(&frame, 4, 4, (0, 0, 4, 4), 0, 2).is_none());
        assert!(capture_rgba8(&frame, 4, 4, (3, 0, 2, 4), 2, 2).is_none());
        assert!(capture_rgba8(&frame, 4, 4, (0, 0, 5, 4), 2, 2).is_none());
        assert!(capture_rgba8(&frame[..10], 4, 4, (0, 0, 4, 4), 2, 2).is_none());
        assert!(capture_rgba8(&frame, usize::MAX, 4, (0, 0, 4, 4), 2, 2).is_none());
    }

    #[test]
    fn vita_frame_produces_the_existing_save_preview_size() {
        let frame = [20, 40, 60, 255].repeat(960 * 544);
        let output = capture_rgba8(&frame, 960, 544, (117, 0, 843, 544), 120, 90).unwrap();
        assert_eq!(output.len(), 120 * 90 * 4);
        assert_eq!(output, [20, 40, 60, 255].repeat(120 * 90));
    }
}
