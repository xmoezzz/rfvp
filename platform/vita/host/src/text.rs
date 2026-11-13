//! Text and rectangle drawing for the file browser.
//!
//! The browser runs before any game exists, so it cannot use the engine's own text layout. It
//! rasterizes glyphs with the same font code the games use ([`rfvp::font::Font`]) into one
//! screen-sized RGBA layer, which the caller uploads as a single texture. Rebuilding that layer
//! only happens when something on screen changes; an idle browser costs nothing.
//!
//! Bitmaps are cached per (character, size) so a repaint after a key press does not rasterize
//! Chinese directory names again.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use rfvp::font::{Font, Metrics};

/// How many rasterized glyphs are kept. A screenful of Chinese names needs a few hundred;
/// beyond this the cache is dropped whole rather than grown for a whole card's worth of text.
const MAX_GLYPHS: usize = 1536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

struct Glyph {
    metrics: Metrics,
    bitmap: Vec<u8>,
    /// The font has no entry for this character at all. A space or other blank glyph has an
    /// empty bitmap too, and must not be drawn as a missing-character box.
    missing: bool,
}

/// `f32::round` and `f32::ceil`, which `core` does not have (same helpers as `render.rs`).
fn round_f32(v: f32) -> i32 {
    let t = v as i64 as f32;
    if v - t >= 0.5 {
        (t as i64 + 1) as i32
    } else if t - v > 0.5 {
        (t as i64 - 1) as i32
    } else {
        t as i32
    }
}

fn ceil_f32(v: f32) -> i32 {
    let t = v as i64 as f32;
    if v > t {
        (t as i64 + 1) as i32
    } else {
        t as i32
    }
}

/// An RGBA8 image the browser paints into.
pub struct Layer {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl Layer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: alloc::vec![0u8; width * height * 4],
        }
    }

    pub fn clear(&mut self, color: Rgba) {
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel[0] = color.0;
            pixel[1] = color.1;
            pixel[2] = color.2;
            pixel[3] = color.3;
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: Rgba) {
        if width <= 0 || height <= 0 || color.3 == 0 {
            return;
        }
        let x0 = x.max(0) as usize;
        let y0 = y.max(0) as usize;
        let x1 = (x.saturating_add(width)).min(self.width as i32).max(0) as usize;
        let y1 = (y.saturating_add(height)).min(self.height as i32).max(0) as usize;
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        if color.3 == 255 {
            for row in y0..y1 {
                let base = (row * self.width + x0) * 4;
                for pixel in self.pixels[base..base + (x1 - x0) * 4].chunks_exact_mut(4) {
                    pixel.copy_from_slice(&[color.0, color.1, color.2, 255]);
                }
            }
            return;
        }
        for row in y0..y1 {
            for column in x0..x1 {
                self.blend(column, row, color, 255);
            }
        }
    }

    /// Mix `color` into one pixel; `coverage` is 0..=255.
    fn blend(&mut self, x: usize, y: usize, color: Rgba, coverage: u8) {
        if x >= self.width || y >= self.height {
            return;
        }
        let alpha = (u32::from(color.3) * u32::from(coverage) + 127) / 255;
        if alpha == 0 {
            return;
        }
        let at = (y * self.width + x) * 4;
        let keep = 255 - alpha;
        self.pixels[at] = ((u32::from(color.0) * alpha + u32::from(self.pixels[at]) * keep) / 255) as u8;
        self.pixels[at + 1] =
            ((u32::from(color.1) * alpha + u32::from(self.pixels[at + 1]) * keep) / 255) as u8;
        self.pixels[at + 2] =
            ((u32::from(color.2) * alpha + u32::from(self.pixels[at + 2]) * keep) / 255) as u8;
        self.pixels[at + 3] = 255;
    }
}

/// A font plus the glyphs it has already produced.
pub struct TextPainter {
    font: Font,
    cache: BTreeMap<u64, Glyph>,
}

fn key(ch: char, size: f32) -> u64 {
    // Sizes are whole pixels here; 1/16 px steps keep distinct entries for distinct sizes.
    let scaled = (size * 16.0) as u32 as u64;
    (scaled << 32) | u64::from(ch as u32)
}

impl TextPainter {
    pub fn new(font: Font) -> Self {
        Self {
            font,
            cache: BTreeMap::new(),
        }
    }

    /// Distance from the top of a line to its baseline, in pixels.
    ///
    /// `&self`: the metrics are only read, and a caller inside `draw(...)` arguments must be able
    /// to ask for them without taking a second mutable borrow of the painter.
    pub fn ascent(&self, size: f32) -> i32 {
        self.font
            .horizontal_line_metrics(size)
            .map(|metrics| ceil_f32(metrics.ascent))
            .unwrap_or(size as i32)
    }

    fn ensure(&mut self, ch: char, size: f32) {
        let entry = key(ch, size);
        if self.cache.contains_key(&entry) {
            return;
        }
        if self.cache.len() >= MAX_GLYPHS {
            self.cache.clear();
        }
        let present = self.font.has_glyph(ch);
        let (metrics, bitmap) = if present {
            self.font.rasterize(ch, size)
        } else {
            (self.font.metrics(ch, size), Vec::new())
        };
        self.cache.insert(entry, Glyph { metrics, bitmap, missing: !present });
    }

    pub fn advance(&mut self, ch: char, size: f32) -> i32 {
        self.ensure(ch, size);
        let advance = self.cache[&key(ch, size)].metrics.advance_width;
        if advance <= 0.0 {
            (size * 0.5) as i32
        } else {
            round_f32(advance)
        }
    }

    pub fn measure(&mut self, text: &str, size: f32) -> i32 {
        let mut width = 0;
        for ch in text.chars() {
            width += self.advance(ch, size);
        }
        width
    }

    /// `text` cut to `max_width`, with an ellipsis when something had to go. The cut text is
    /// only for display: the path it came from is never rebuilt from what is shown.
    pub fn truncate(&mut self, text: &str, size: f32, max_width: i32) -> String {
        if self.measure(text, size) <= max_width {
            return String::from(text);
        }
        let ellipsis = "\u{2026}";
        let room = max_width - self.measure(ellipsis, size);
        if room <= 0 {
            return String::from(ellipsis);
        }
        let mut out = String::new();
        let mut used = 0;
        for ch in text.chars() {
            let step = self.advance(ch, size);
            if used + step > room {
                break;
            }
            out.push(ch);
            used += step;
        }
        out.push_str(ellipsis);
        out
    }

    /// Draw `text` with its baseline at `baseline_y` starting at `x`, never past `right`.
    pub fn draw(
        &mut self,
        layer: &mut Layer,
        x: i32,
        baseline_y: i32,
        text: &str,
        size: f32,
        color: Rgba,
        right: i32,
    ) {
        let mut pen = x;
        for ch in text.chars() {
            let entry = key(ch, size);
            self.ensure(ch, size);
            let glyph = &self.cache[&entry];
            let advance = if glyph.metrics.advance_width <= 0.0 {
                (size * 0.5) as i32
            } else {
                round_f32(glyph.metrics.advance_width)
            };
            if pen + advance > right + size as i32 {
                break;
            }
            if glyph.bitmap.is_empty() {
                // Blank glyphs (space, tab, ideographic space) have no outline and must stay
                // blank. Only a character the font genuinely lacks gets a box, so a name is not
                // silently mangled into something that looks like another name.
                if glyph.missing && !ch.is_whitespace() {
                    let box_w = (advance - 2).max(2).min(size as i32 - 2);
                    let box_h = (size * 0.5) as i32;
                    layer.fill_rect(pen + 1, baseline_y - box_h, box_w, 1, color);
                    layer.fill_rect(pen + 1, baseline_y - 1, box_w, 1, color);
                    layer.fill_rect(pen + 1, baseline_y - box_h, 1, box_h, color);
                    layer.fill_rect(pen + box_w, baseline_y - box_h, 1, box_h, color);
                }
                pen += advance;
                continue;
            }
            let glyph_w = glyph.metrics.width;
            let glyph_h = glyph.metrics.height;
            let left = pen + glyph.metrics.xmin;
            // `ymin` is the bitmap's bottom edge measured upward from the baseline.
            let top = baseline_y - (glyph.metrics.ymin + glyph_h as i32);
            for row in 0..glyph_h {
                let y = top + row as i32;
                if y < 0 || y >= layer.height as i32 {
                    continue;
                }
                for column in 0..glyph_w {
                    let coverage = glyph.bitmap[row * glyph_w + column];
                    if coverage == 0 {
                        continue;
                    }
                    let px = left + column as i32;
                    if px < x || px > right {
                        continue;
                    }
                    if px < 0 || px >= layer.width as i32 {
                        continue;
                    }
                    layer.blend(px as usize, y as usize, color, coverage);
                }
            }
            pen += advance;
        }
    }
}
