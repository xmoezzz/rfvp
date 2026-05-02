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

use ab_glyph::{Font as _, FontArc, PxScale, ScaleFont};

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
pub struct Font {
    inner: FontArc,
}

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
    pub fn from_bytes_static(bytes: &'static [u8], _settings: FontSettings)
        -> Result<Self, ab_glyph::InvalidFont>
    {
        Self::from_static(bytes)
    }

    pub fn from_bytes_owned(bytes: Vec<u8>, _settings: FontSettings)
        -> Result<Self, ab_glyph::InvalidFont>
    {
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

    /// Glyph metrics without rasterizing.
    pub fn metrics(&self, ch: char, size: f32) -> Metrics {
        self.glyph_metrics_internal(ch, size, false).0
    }

    /// Rasterize a glyph and return its metrics + alpha bitmap (row-major,
    /// top-left, 0..=255).
    pub fn rasterize(&self, ch: char, size: f32) -> (Metrics, Vec<u8>) {
        self.glyph_metrics_internal(ch, size, true)
    }

    fn glyph_metrics_internal(&self, ch: char, size: f32, want_bitmap: bool)
        -> (Metrics, Vec<u8>)
    {
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
        let ymax_top_down = bb.max.y.ceil() as i32;  // positive for descenders
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
