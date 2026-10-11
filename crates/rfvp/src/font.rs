//! Thin fontdue-compatible API over `ab_glyph`.
//!
//! Why a shim: `fontdue::Font::from_bytes` eagerly outlines every glyph in the
//! font (CJK fonts have 20000+ glyphs, costing 60–100 MB each). `ab_glyph`
//! only parses the font header and outlines glyphs lazily on demand. This
//! shim exposes the subset of the fontdue API the rest of the codebase uses,
//! letting us swap engines without touching all the coordinate math
//! (`baseline - (ymin + height)` etc.) at the call sites.
//!
//! Coordinate conventions kept identical to fontdue:
//!  * `LineMetrics::descent` is **negative** for typical fonts.
//!  * `Metrics::ymin` is the bitmap's bottom edge measured **upward** from
//!    the glyph baseline (positive = above baseline).
//!  * Bitmap row-major top-left origin, alpha 0..=255.
//!
//! `Font` wraps `ab_glyph::FontArc`, which is internally `Arc<dyn Font>`, so
//! cloning a `Font` is a refcount bump — cheap.

#[cfg(any(
    feature = "old_school",
    all(feature = "no_std", not(feature = "old_school"))
))]
use alloc::vec;
use alloc::vec::Vec;

#[cfg(all(feature = "old_school", not(feature = "psx")))]
use rfvp_bitmap::{
    OwnedBitmapFont, GLYPH_BYTES as BITMAP_GLYPH_BYTES, GLYPH_HEIGHT as BITMAP_GLYPH_HEIGHT,
    GLYPH_WIDTH as BITMAP_GLYPH_WIDTH,
};

#[cfg(not(feature = "old_school"))]
use ab_glyph::{Font as _, FontArc, PxScale, ScaleFont};
#[cfg(all(feature = "no_std", not(feature = "old_school")))]
use core_maths::CoreFloat;

#[derive(Clone, Copy, Debug, Default)]
pub struct FontSettings;

impl FontSettings {
    pub fn default() -> Self {
        FontSettings
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LineMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub new_line_size: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Metrics {
    /// Bitmap left edge offset from glyph origin (pen position), in pixels.
    pub xmin: i32,
    /// Bitmap bottom edge measured **upward** from the baseline.
    /// Positive = above baseline. Negative = below baseline (descender).
    pub ymin: i32,
    pub width: usize,
    pub height: usize,
    pub advance_width: f32,
    pub advance_height: f32,
}

#[derive(Clone)]
#[cfg(not(feature = "old_school"))]
pub struct Font {
    inner: FontArc,
}

#[cfg(all(feature = "old_school", not(feature = "psx")))]
#[derive(Clone)]
pub struct Font {
    bitmap: OwnedBitmapFont,
}

/// The PlayStation's font (`font.psxf`, written by rfvp-psx-convert): glyphs of one fixed
/// size, one bit per pixel, only those the game uses. It is drawn at that size whatever the
/// script asks for.
#[cfg(feature = "psx")]
#[derive(Clone)]
pub struct Font {
    data: alloc::sync::Arc<Vec<u8>>,
    cell: usize,
    count: usize,
}

/// The PlayStation font's glyph size (12 until the font is loaded).
#[cfg(feature = "psx")]
static PSX_CELL: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(12);

#[cfg(feature = "psx")]
pub fn psx_cell() -> usize {
    PSX_CELL.load(core::sync::atomic::Ordering::Relaxed)
}

#[cfg(feature = "psx")]
impl Font {
    /// `"PSXF", u8 version, u8 cell, u16 count, u16 codepoints[count] (sorted), u8
    /// advances[count], glyphs (cell rows of ceil(cell / 8) bytes, MSB leftmost)`.
    pub fn from_psxf(data: Vec<u8>) -> Result<Self, ()> {
        if data.len() < 8 || &data[0..4] != b"PSXF" || data[4] != 1 {
            return Err(());
        }
        let cell = data[5] as usize;
        let count = u16::from_le_bytes([data[6], data[7]]) as usize;
        let need = 8 + count * 3 + count * cell * cell.div_ceil(8);
        if cell == 0 || data.len() < need {
            return Err(());
        }
        PSX_CELL.store(cell, core::sync::atomic::Ordering::Relaxed);
        Ok(Self { data: alloc::sync::Arc::new(data), cell, count })
    }

    fn index(&self, ch: char) -> Option<usize> {
        let c = ch as u32;
        if c > 0xffff {
            return None;
        }
        let (mut lo, mut hi) = (0usize, self.count);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let at = 8 + mid * 2;
            let v = u16::from_le_bytes([self.data[at], self.data[at + 1]]) as u32;
            if v == c {
                return Some(mid);
            }
            if v < c {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        None
    }

    pub fn from_static(_bytes: &'static [u8]) -> Result<Self, ()> {
        Err(())
    }

    pub fn from_vec(bytes: Vec<u8>) -> Result<Self, ()> {
        Self::from_psxf(bytes)
    }

    pub fn from_bytes_static(_bytes: &'static [u8], _settings: FontSettings) -> Result<Self, ()> {
        Err(())
    }

    pub fn from_bytes_owned(bytes: Vec<u8>, _settings: FontSettings) -> Result<Self, ()> {
        Self::from_psxf(bytes)
    }

    pub fn horizontal_line_metrics(&self, _size: f32) -> Option<LineMetrics> {
        let cell = self.cell as f32;
        Some(LineMetrics { ascent: cell, descent: 0.0, line_gap: 0.0, new_line_size: cell })
    }

    pub fn has_glyph(&self, ch: char) -> bool {
        self.index(ch).is_some()
    }

    pub fn metrics(&self, ch: char, _size: f32) -> Metrics {
        match self.index(ch) {
            Some(i) => Metrics {
                xmin: 0,
                ymin: 0,
                width: self.cell,
                height: self.cell,
                advance_width: self.data[8 + self.count * 2 + i] as f32,
                advance_height: 0.0,
            },
            None => Metrics::default(),
        }
    }

    pub fn rasterize(&self, ch: char, size: f32) -> (Metrics, Vec<u8>) {
        let metrics = self.metrics(ch, size);
        let Some(i) = self.index(ch) else {
            return (metrics, Vec::new());
        };
        let row = self.cell.div_ceil(8);
        let start = 8 + self.count * 3 + i * self.cell * row;
        let bits = &self.data[start..start + self.cell * row];
        let mut out = vec![0u8; self.cell * self.cell];
        for y in 0..self.cell {
            for x in 0..self.cell {
                if bits[y * row + x / 8] & (0x80 >> (x % 8)) != 0 {
                    out[y * self.cell + x] = 255;
                }
            }
        }
        (metrics, out)
    }
}

#[cfg(not(feature = "old_school"))]
impl Font {
    /// Parse a font from a `'static` byte slice (e.g. `include_bytes!`).
    /// Avoids copying the byte slice — uses `ab_glyph::FontRef` internally.
    pub fn from_static(bytes: &'static [u8]) -> Result<Self, ab_glyph::InvalidFont> {
        Ok(Self {
            inner: FontArc::try_from_slice(bytes)?,
        })
    }

    /// Parse a font from owned bytes (e.g. `fs::read`).
    pub fn from_vec(bytes: Vec<u8>) -> Result<Self, ab_glyph::InvalidFont> {
        Ok(Self {
            inner: FontArc::try_from_vec(bytes)?,
        })
    }

    /// fontdue-compatible entry point. Settings are ignored; ab_glyph has no
    /// directly equivalent rendering hint cache.
    pub fn from_bytes_static(
        bytes: &'static [u8],
        _settings: FontSettings,
    ) -> Result<Self, ab_glyph::InvalidFont> {
        Self::from_static(bytes)
    }

    pub fn from_bytes_owned(
        bytes: Vec<u8>,
        _settings: FontSettings,
    ) -> Result<Self, ab_glyph::InvalidFont> {
        Self::from_vec(bytes)
    }

    pub fn horizontal_line_metrics(&self, size: f32) -> Option<LineMetrics> {
        let scaled = self.inner.as_scaled(PxScale::from(size));
        let ascent = scaled.ascent();
        // ab_glyph::ScaleFont::descent returns a value with fontdue's sign
        // convention (negative for typical fonts), so no flip needed.
        let descent = scaled.descent();
        let line_gap = scaled.line_gap();
        Some(LineMetrics {
            ascent,
            descent,
            line_gap,
            new_line_size: ascent - descent + line_gap,
        })
    }

    /// Return whether the font has a real cmap entry for this character.
    pub fn has_glyph(&self, ch: char) -> bool {
        self.inner.glyph_id(ch).0 != 0
    }

    /// Glyph metrics without rasterizing.
    pub fn metrics(&self, ch: char, size: f32) -> Metrics {
        self.glyph_metrics_internal(ch, size, false).0
    }

    /// Rasterize a glyph and return its metrics + alpha bitmap (row-major,
    /// top-left, 0..=255).
    pub fn rasterize(&self, ch: char, size: f32) -> (Metrics, Vec<u8>) {
        self.glyph_metrics_internal(ch, size, true)
    }

    fn glyph_metrics_internal(&self, ch: char, size: f32, want_bitmap: bool) -> (Metrics, Vec<u8>) {
        let scale = PxScale::from(size);
        let scaled = self.inner.as_scaled(scale);
        let glyph_id = self.inner.glyph_id(ch);
        let advance_width = scaled.h_advance(glyph_id);

        let glyph = glyph_id.with_scale_and_position(scale, ab_glyph::point(0.0, 0.0));

        // Whitespace / control chars / unmapped glyphs have no outline.
        let outlined = match self.inner.outline_glyph(glyph) {
            Some(o) => o,
            None => {
                return (
                    Metrics {
                        xmin: 0,
                        ymin: 0,
                        width: 0,
                        height: 0,
                        advance_width,
                        advance_height: 0.0,
                    },
                    Vec::new(),
                );
            }
        };

        let bb = outlined.px_bounds();
        let xmin_i = bb.min.x.floor() as i32;
        let xmax_i = bb.max.x.ceil() as i32;
        let ymin_top_down = bb.min.y.floor() as i32; // negative for ascenders
        let ymax_top_down = bb.max.y.ceil() as i32; // positive for descenders
        let width = (xmax_i - xmin_i).max(0) as usize;
        let height = (ymax_top_down - ymin_top_down).max(0) as usize;

        // Convert ab_glyph's y-down bbox to fontdue's y-up bitmap-bottom anchor:
        //   fontdue.ymin = -bbox.max.y  (the lowest pixel below baseline,
        //                                 flipped to positive-up)
        let metrics = Metrics {
            xmin: xmin_i,
            ymin: -ymax_top_down,
            width,
            height,
            advance_width,
            advance_height: 0.0,
        };

        let bitmap = if want_bitmap && width > 0 && height > 0 {
            let mut buf = vec![0u8; width * height];
            outlined.draw(|x, y, c| {
                let xu = x as usize;
                let yu = y as usize;
                if xu < width && yu < height {
                    buf[yu * width + xu] = (c * 255.0).clamp(0.0, 255.0) as u8;
                }
            });
            buf
        } else {
            Vec::new()
        };

        (metrics, bitmap)
    }
}

#[cfg(all(feature = "old_school", not(feature = "psx")))]
impl Font {
    pub const GLYPH_WIDTH: usize = BITMAP_GLYPH_WIDTH as usize;
    pub const GLYPH_HEIGHT: usize = BITMAP_GLYPH_HEIGHT as usize;
    pub const GLYPH_BYTES: usize = BITMAP_GLYPH_BYTES;

    pub fn from_old_school_tmap(bytes: Vec<u8>) -> Result<Self, rfvp_bitmap::BitmapFontError> {
        Ok(Self {
            bitmap: OwnedBitmapFont::from_bytes(bytes)?,
        })
    }

    pub fn from_static(_bytes: &'static [u8]) -> Result<Self, ()> {
        Err(())
    }

    pub fn from_vec(_bytes: Vec<u8>) -> Result<Self, ()> {
        Err(())
    }

    pub fn from_bytes_static(_bytes: &'static [u8], _settings: FontSettings) -> Result<Self, ()> {
        Err(())
    }

    pub fn from_bytes_owned(_bytes: Vec<u8>, _settings: FontSettings) -> Result<Self, ()> {
        Err(())
    }

    pub fn horizontal_line_metrics(&self, size: f32) -> Option<LineMetrics> {
        Some(LineMetrics {
            ascent: size,
            descent: 0.0,
            line_gap: 0.0,
            new_line_size: size,
        })
    }

    pub fn has_glyph(&self, ch: char) -> bool {
        self.bitmap.lookup_glyph(ch as u32).data.len() == Self::GLYPH_BYTES
    }

    /// The side of a glyph drawn at `size`: the font's square cells shown `size` pixels high,
    /// so a glyph never sticks out of its line or into the next one.
    fn cell_pixels(size: f32) -> usize {
        ((size + 0.5) as usize).max(1)
    }

    pub fn metrics(&self, ch: char, size: f32) -> Metrics {
        let glyph = self.bitmap.lookup_glyph(ch as u32);
        let present = glyph.data.len() == Self::GLYPH_BYTES;
        let side = if present { Self::cell_pixels(size) } else { 0 };
        Metrics {
            // The cell spans the line: from the baseline (ascent = size, no descent) up.
            xmin: 0,
            ymin: 0,
            width: side,
            height: side,
            advance_width: if present { size } else { 0.0 },
            advance_height: 0.0,
        }
    }

    pub fn rasterize(&self, ch: char, size: f32) -> (Metrics, Vec<u8>) {
        let metrics = self.metrics(ch, size);
        let glyph = self.bitmap.lookup_glyph(ch as u32);
        if glyph.data.len() != Self::GLYPH_BYTES {
            return (metrics, Vec::new());
        }
        let (w, h) = (Self::GLYPH_WIDTH, Self::GLYPH_HEIGHT);
        let mut cell = vec![0u8; w * h];
        for y in 0..h {
            for x_pair in 0..(w / 2) {
                let byte = glyph.data[y * (w / 2) + x_pair];
                cell[y * w + x_pair * 2] = coverage_to_alpha(byte >> 4);
                cell[y * w + x_pair * 2 + 1] = coverage_to_alpha(byte & 0x0f);
            }
        }
        let side = metrics.width;
        if side == w {
            return (metrics, cell);
        }
        let mut out = vec![0u8; side * side];
        if side > w {
            // Larger: each cell pixel becomes a block, which keeps the dots sharp.
            for y in 0..side {
                let sy = y * h / side;
                for x in 0..side {
                    out[y * side + x] = cell[sy * w + x * w / side];
                }
            }
        } else {
            // Smaller: the average of the cell pixels each output pixel covers (in 1/side
            // units, so it is exact).
            for y in 0..side {
                for x in 0..side {
                    let mut sum = 0u32;
                    let mut weight = 0u32;
                    for cy in 0..h {
                        let wy = overlap(cy * side, (cy + 1) * side, y * h, (y + 1) * h);
                        if wy == 0 {
                            continue;
                        }
                        for cx in 0..w {
                            let wx = overlap(cx * side, (cx + 1) * side, x * w, (x + 1) * w);
                            sum += cell[cy * w + cx] as u32 * (wx * wy) as u32;
                            weight += (wx * wy) as u32;
                        }
                    }
                    out[y * side + x] = if weight == 0 { 0 } else { ((sum + weight / 2) / weight) as u8 };
                }
            }
        }
        (metrics, out)
    }
}

/// Length of the overlap of `[a0, a1)` and `[b0, b1)`.
#[cfg(feature = "old_school")]
fn overlap(a0: usize, a1: usize, b0: usize, b1: usize) -> usize {
    a1.min(b1).saturating_sub(a0.max(b0))
}

#[cfg(feature = "old_school")]
fn coverage_to_alpha(value: u8) -> u8 {
    value.saturating_mul(17)
}

#[cfg(all(test, feature = "old_school", not(feature = "psx")))]
mod old_school_tests {
    use super::*;

    fn minimal_font_bytes() -> Vec<u8> {
        let mut entries = vec![0xffffu16; 0x1100];
        entries[0] = 0;
        let header_len = 76usize;
        let entries_len = entries.len() * 2;
        let records_offset = header_len + entries_len;
        let glyphs_offset = records_offset + 36;

        let mut bytes = Vec::new();
        bytes.extend_from_slice(rfvp_bitmap::MAGIC);
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&(0x1100u32).to_le_bytes());
        bytes.extend_from_slice(&(header_len as u32).to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&(records_offset as u32).to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&(glyphs_offset as u32).to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 32]);
        for entry in entries {
            bytes.extend_from_slice(&entry.to_le_bytes());
        }
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let mut flags = [0u8; 32];
        flags[0x3f / 8] |= 1 << (0x3f % 8);
        flags[0x41 / 8] |= 1 << (0x41 % 8);
        bytes.extend_from_slice(&flags);
        bytes.extend_from_slice(&[0x00; 128]);
        bytes.extend_from_slice(&[0xff; 128]);
        bytes
    }

    #[test]
    fn old_school_font_loads_bitmap_and_rasterizes_unicode_lookup() {
        let font = Font::from_old_school_tmap(minimal_font_bytes()).unwrap();
        let (_metrics, bitmap) = font.rasterize('A', 16.0);
        assert_eq!(bitmap.len(), 16 * 16);
        assert_eq!(bitmap[0], 255);
    }
}
