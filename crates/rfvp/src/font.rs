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

#[cfg(feature = "old_school")]
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

#[cfg(feature = "old_school")]
#[derive(Clone)]
pub struct Font {
    bitmap: OwnedBitmapFont,
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

#[cfg(feature = "old_school")]
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

    pub fn metrics(&self, ch: char, size: f32) -> Metrics {
        let glyph = self.bitmap.lookup_glyph(ch as u32);
        let advance = if glyph.data.len() == Self::GLYPH_BYTES {
            size
        } else {
            0.0
        };
        Metrics {
            xmin: 0,
            ymin: 0,
            width: if advance > 0.0 { Self::GLYPH_WIDTH } else { 0 },
            height: if advance > 0.0 { Self::GLYPH_HEIGHT } else { 0 },
            advance_width: advance,
            advance_height: 0.0,
        }
    }

    pub fn rasterize(&self, ch: char, size: f32) -> (Metrics, Vec<u8>) {
        let metrics = self.metrics(ch, size);
        let glyph = self.bitmap.lookup_glyph(ch as u32);
        if glyph.data.len() != Self::GLYPH_BYTES {
            return (metrics, Vec::new());
        }
        let mut bitmap = vec![0; Self::GLYPH_WIDTH * Self::GLYPH_HEIGHT];
        for y in 0..Self::GLYPH_HEIGHT {
            for x_pair in 0..(Self::GLYPH_WIDTH / 2) {
                let byte = glyph.data[y * 8 + x_pair];
                let left = byte >> 4;
                let right = byte & 0x0f;
                bitmap[y * Self::GLYPH_WIDTH + x_pair * 2] = coverage_to_alpha(left);
                bitmap[y * Self::GLYPH_WIDTH + x_pair * 2 + 1] = coverage_to_alpha(right);
            }
        }
        (metrics, bitmap)
    }
}

#[cfg(feature = "old_school")]
fn coverage_to_alpha(value: u8) -> u8 {
    value.saturating_mul(17)
}

#[cfg(all(test, feature = "old_school"))]
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
