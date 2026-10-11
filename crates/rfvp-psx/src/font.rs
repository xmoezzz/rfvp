//! The text font: one fixed size, one bit per pixel, only the characters the game shows.
//!
//! ```text
//! "PSXF", u8 version (1), u8 cell size in pixels, u16 glyph count
//! u16 codepoints[count]     sorted (all are in the Basic Multilingual Plane)
//! u8  advances[count]       pixels the pen moves (the cell for full width, half for half)
//! glyphs[count]             cell rows of ceil(cell / 8) bytes, most significant bit leftmost
//! ```
//!
//! Glyphs are drawn as they are: no scaling, outline, shadow or ruby.

use std::collections::BTreeSet;

use anyhow::{Result, anyhow};
use fontdue::{Font, FontSettings};

const OVERSAMPLE: usize = 4;

/// Characters every game needs, whatever its script says: ASCII, kana, CJK punctuation, full
/// width forms, the replacement glyph.
fn base_set() -> BTreeSet<u32> {
    let mut set = BTreeSet::new();
    for range in [0x20..=0x7e, 0x3000..=0x30ff, 0xff01..=0xff9f] {
        set.extend(range);
    }
    set.insert(0x25a1);
    set
}

pub fn build(ttf: &[u8], cell: usize, strings: &[String]) -> Result<Vec<u8>> {
    let font = Font::from_bytes(ttf, FontSettings::default()).map_err(|e| anyhow!("font: {e}"))?;
    let mut set = base_set();
    for s in strings {
        set.extend(s.chars().map(|c| c as u32).filter(|&c| c >= 0x20 && c <= 0xffff));
    }
    // Only what the font can draw (and the fallback box).
    let codepoints: Vec<u32> = set
        .into_iter()
        .filter(|&c| c == 0x25a1 || c == 0x20 || font.lookup_glyph_index(char::from_u32(c).unwrap()) != 0)
        .collect();
    let row_bytes = cell.div_ceil(8);
    let mut out = Vec::new();
    out.extend_from_slice(b"PSXF");
    out.push(1);
    out.push(cell as u8);
    out.extend_from_slice(&(codepoints.len() as u16).to_le_bytes());
    for &c in &codepoints {
        out.extend_from_slice(&(c as u16).to_le_bytes());
    }
    let glyphs: Vec<(u8, Vec<u8>)> = codepoints
        .iter()
        .map(|&c| render(&font, char::from_u32(c).unwrap(), cell, row_bytes))
        .collect();
    out.extend(glyphs.iter().map(|g| g.0));
    for (_, bits) in &glyphs {
        out.extend_from_slice(bits);
    }
    Ok(out)
}

/// One glyph: its advance and its bits. Full-width glyphs fill the cell; narrower ones take
/// half of it. Every glyph sits on one baseline, the font's ascent and descent spanning the
/// cell.
fn render(font: &Font, ch: char, cell: usize, row_bytes: usize) -> (u8, Vec<u8>) {
    let large = cell * OVERSAMPLE;
    let mut size = large as f32;
    if let Some(line) = font.horizontal_line_metrics(size) {
        let span = line.ascent - line.descent;
        if span > 0.0 {
            size *= large as f32 / span;
        }
    }
    let (metrics, bitmap) = font.rasterize(ch, size);
    let (ascent, descent) = font
        .horizontal_line_metrics(size)
        .map(|l| (l.ascent, l.descent))
        .unwrap_or((large as f32 * 0.88, -(large as f32) * 0.12));
    let baseline = ((large as f32 - (ascent - descent)) / 2.0 + ascent).round() as i32;
    let full = metrics.advance_width > large as f32 * 0.75;
    let advance = if full { cell } else { cell.div_ceil(2) };
    let box_w = advance * OVERSAMPLE;
    let x_origin = ((box_w as f32 - metrics.advance_width) / 2.0).round() as i32 + metrics.xmin;
    let y_top = baseline - (metrics.ymin + metrics.height as i32);
    let mut canvas = vec![0u8; large * large];
    for y in 0..metrics.height {
        for x in 0..metrics.width {
            let (dx, dy) = (x as i32 + x_origin, y as i32 + y_top);
            if (0..large as i32).contains(&dx) && (0..large as i32).contains(&dy) {
                canvas[dy as usize * large + dx as usize] = bitmap[y * metrics.width + x];
            }
        }
    }
    let mut bits = vec![0u8; cell * row_bytes];
    for y in 0..cell {
        for x in 0..cell {
            let mut sum = 0u32;
            for oy in 0..OVERSAMPLE {
                for ox in 0..OVERSAMPLE {
                    sum += canvas[(y * OVERSAMPLE + oy) * large + x * OVERSAMPLE + ox] as u32;
                }
            }
            // On when a little over a third covered: thin strokes stay, blobs do not form.
            if sum * 100 >= 255 * (OVERSAMPLE * OVERSAMPLE) as u32 * 38 {
                bits[y * row_bytes + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    (advance as u8, bits)
}
