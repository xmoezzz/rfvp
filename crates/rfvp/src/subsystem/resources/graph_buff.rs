use image::{DynamicImage, GenericImageView, ImageBuffer};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use super::texture::NvsgTexture;
use super::vfs::Vfs;

/// How this [`GraphBuff`] was last populated.
///
/// Save/load needs this information because different NVSG payload types must be decoded
/// using different loaders.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub enum GraphBuffLoadKind {
    /// Unknown or unloaded.
    #[default]
    Unknown,
    /// Normal 24/32-bit NVSG texture.
    Texture,
    /// 8-bit mask NVSG texture.
    Mask,
    /// 1-bit gaiji glyph.
    GaijiGlyph,
    /// Raw RGBA8 pixel buffer.
    RawRgba,
}

#[derive(Debug, Clone)]
pub struct GraphBuff {
    pub texture: Option<DynamicImage>,
    pub r_value: u8,
    pub g_value: u8,
    pub b_value: u8,
    pub texture_ready: bool,
    pub texture_path: String,
    pub offset_x: u16,
    pub offset_y: u16,
    pub width: u16,
    pub height: u16,
    pub u: u16,
    pub v: u16,

    /// Monotonically increasing generation counter.
    ///
    /// Any CPU-side pixel mutation (including replacing the texture) must bump this counter so the
    /// GPU cache can refresh the corresponding texture.
    pub generation: u64,

    /// Track how this graph was populated so it can be restored correctly.
    pub load_kind: GraphBuffLoadKind,
}

impl GraphBuff {
    pub fn new() -> Self {
        Self {
            texture: None,
            r_value: 0,
            g_value: 0,
            b_value: 0,
            texture_ready: false,
            texture_path: String::new(),
            offset_x: 0,
            offset_y: 0,
            width: 0,
            height: 0,
            u: 0,
            v: 0,
            generation: 0,
            load_kind: GraphBuffLoadKind::Unknown,
        }
    }

    #[inline]
    pub fn get_generation(&self) -> u64 {
        self.generation
    }

    #[inline]
    pub fn mark_dirty(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn get_r_value(&self) -> u8 {
        self.r_value
    }

    pub fn get_g_value(&self) -> u8 {
        self.g_value
    }

    pub fn get_b_value(&self) -> u8 {
        self.b_value
    }

    pub fn get_texture_ready(&self) -> bool {
        self.texture_ready
    }

    pub fn get_texture_path(&self) -> &str {
        &self.texture_path
    }

    pub fn get_offset_x(&self) -> u16 {
        self.offset_x
    }

    pub fn get_offset_y(&self) -> u16 {
        self.offset_y
    }

    pub fn get_u(&self) -> u16 {
        self.u
    }

    pub fn get_v(&self) -> u16 {
        self.v
    }

    pub fn get_width(&self) -> u16 {
        self.width
    }

    pub fn get_height(&self) -> u16 {
        self.height
    }

    pub fn get_texture_mut(&mut self) -> &mut Option<DynamicImage> {
        &mut self.texture
    }

    pub fn get_texture(&self) -> &Option<DynamicImage> {
        &self.texture
    }

    pub fn unload(&mut self) {
        self.texture = None;
        self.texture_ready = false;
        self.texture_path = String::new();
        self.offset_x = 0;
        self.offset_y = 0;
        self.width = 0;
        self.height = 0;
        self.u = 0;
        self.v = 0;
        self.load_kind = GraphBuffLoadKind::Unknown;
        self.mark_dirty();
    }

    pub fn load_texture(&mut self, file_name: &str, buff: Vec<u8>) -> Result<()> {
        let mut nvsg_texture = NvsgTexture::new(file_name);
        nvsg_texture.read_texture(&buff, |typ| {
            typ == super::texture::TextureType::Single24Bit ||
            typ == super::texture::TextureType::Single32Bit
        })?;

        self.unload();
        // we don't need to split the texture into multiple 256x256 textures
        self.texture = Some(nvsg_texture.get_texture(0)?);
        self.r_value = 100;
        self.g_value = 100;
        self.b_value = 100;
        self.texture_ready = true;
        self.offset_x = nvsg_texture.get_offset_x();
        self.offset_y = nvsg_texture.get_offset_y();
        self.width = nvsg_texture.get_width();
        self.height = nvsg_texture.get_height();
        self.u = nvsg_texture.get_u();
        self.v = nvsg_texture.get_v();
        self.texture_path = file_name.to_string();
        self.load_kind = GraphBuffLoadKind::Texture;
        self.mark_dirty();
    
        Ok(())
    }

    pub fn load_gaiji_fontface_glyph(&mut self, file_name: &str, buff: Vec<u8>) -> Result<()> {
        let mut nvsg_texture = NvsgTexture::new(file_name);
        nvsg_texture.read_texture(&buff, |typ| {
            typ == super::texture::TextureType::Single1Bit
        })?;

        self.unload();
        self.texture = Some(nvsg_texture.get_texture(0)?);
        self.r_value = 100;
        self.g_value = 100;
        self.b_value = 100;
        self.texture_ready = true;
        self.offset_x = nvsg_texture.get_offset_x();
        self.offset_y = nvsg_texture.get_offset_y();
        self.width = nvsg_texture.get_width();
        self.height = nvsg_texture.get_height();
        self.u = nvsg_texture.get_u();
        self.v = nvsg_texture.get_v();
        self.texture_path = file_name.to_string();
        self.load_kind = GraphBuffLoadKind::GaijiGlyph;
        self.mark_dirty();
    
        Ok(())
    }

    pub fn load_mask(&mut self, file_name: &str, buff: Vec<u8>) -> Result<()> {
        let mut nvsg_texture = NvsgTexture::new(file_name);
        nvsg_texture.read_texture(&buff, |typ| {
            typ == super::texture::TextureType::Single8Bit
        })?;

        self.unload();
        self.texture = Some(nvsg_texture.get_texture(0)?);
        self.r_value = 100;
        self.g_value = 100;
        self.b_value = 100;
        self.texture_ready = true;
        self.offset_x = nvsg_texture.get_offset_x();
        self.offset_y = nvsg_texture.get_offset_y();
        self.width = nvsg_texture.get_width();
        self.height = nvsg_texture.get_height();
        self.u = nvsg_texture.get_u();
        self.v = nvsg_texture.get_v();
        self.texture_path = file_name.to_string();
        self.load_kind = GraphBuffLoadKind::Mask;
        self.mark_dirty();
    
        Ok(())
    }

    pub fn load_from_buff(&mut self, buff: Vec<u8>, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 {
            return Err(anyhow!("load_from_buff: invalid size {}x{}", width, height));
        }

        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(4))
            .ok_or_else(|| anyhow!("load_from_buff: size overflow ({}x{}x4)", width, height))?;

        if buff.len() != expected {
            return Err(anyhow!(
                "load_from_buff: invalid buffer length: got {}, expected {} ({}x{}x4)",
                buff.len(),
                expected,
                width,
                height
            ));
        }

        // Buffers provided through this interface are treated as RGBA8.
        let img = image::RgbaImage::from_raw(width, height, buff)
            .ok_or_else(|| anyhow!("load_from_buff: RgbaImage::from_raw failed ({}x{})", width, height))?;

        self.texture = Some(DynamicImage::ImageRgba8(img));
        self.texture_ready = true;

        self.texture_path.clear();
        self.offset_x = 0;
        self.offset_y = 0;

        if width > u16::MAX as u32 || height > u16::MAX as u32 {
            return Err(anyhow!(
                "load_from_buff: size too large for u16: {}x{}",
                width,
                height
            ));
        }
        self.width = width as u16;
        self.height = height as u16;

        self.u = 0;
        self.v = 0;

        self.mark_dirty();
        self.load_kind = GraphBuffLoadKind::RawRgba;
        Ok(())
    }

    pub fn set_color_tone(
        &mut self,
        red_value: i32,
        green_value: i32,
        blue_value: i32
    ) {
        // IDA: GraphRGB -> color_tone_texture -> apply_color_tone.
        // apply_color_tone operates on premultiplied-alpha BGRA pixels and clamps channels to alpha.
        // For RGBA8 in Rust, we apply the same math per channel, clamping to A.

        // Match the original engine: no-op if the texture is not ready.
        if !self.texture_ready {
            return;
        };

        let Some(texture) = &mut self.texture else {
            return;
        };
        let Some(texture) = texture.as_mut_rgba8() else {
            return;
        };

        // In the original engine, (100,100,100) is a no-op on pixels.
        // Avoid bumping generation in this specific case to prevent misleading debug noise.
        if red_value == 100 && green_value == 100 && blue_value == 100 {
            self.r_value = 100;
            self.g_value = 100;
            self.b_value = 100;
            return;
        }

        // Clamp inputs to the script contract (0..=200). This matches the syscall layer.
        let r_adj = red_value.clamp(0, 200) as u32;
        let g_adj = green_value.clamp(0, 200) as u32;
        let b_adj = blue_value.clamp(0, 200) as u32;

        #[inline]
        fn apply_one(chan: u8, alpha: u8, adj: u32) -> u8 {
            if adj == 100 {
                return chan;
            }
            let c = chan as u32;
            let a = alpha as u32;
            if adj < 100 {
                // Darken: C = adj * C / 100
                ((adj * c) / 100).min(255) as u8
            } else {
                // Brighten: C = min(A, C + A * (adj - 100) / 255)
                let inc = (a * (adj - 100)) / 255;
                let out = c + inc;
                out.min(a).min(255) as u8
            }
        }

        for px in texture.pixels_mut() {
            let a = px.0[3];
            // Clamp each channel to alpha to preserve premultiplied-alpha invariant.
            px.0[0] = apply_one(px.0[0], a, r_adj).min(a);
            px.0[1] = apply_one(px.0[1], a, g_adj).min(a);
            px.0[2] = apply_one(px.0[2], a, b_adj).min(a);
        }

        self.r_value = r_adj as u8;
        self.g_value = g_adj as u8;
        self.b_value = b_adj as u8;
        self.mark_dirty();
    }
}


pub fn copy_rect(
    src: &DynamicImage,
    src_x: u32,
    src_y: u32,
    src_w: u32,
    src_h: u32,
    dest: &mut DynamicImage,
    dest_x: u32,
    dest_y: u32,
) -> Result<()> {
    let src = src.view(src_x, src_y, src_w, src_h);
    let dest = match dest.as_mut_rgba8() {
        Some(dest) => dest,
        None => return Err(anyhow!("copy_rect: dest image is not in RGBA8 format")),
    };
    for y in 0..src_h {
        for x in 0..src_w {
            // NOTE: `src` is already a sub-view starting at (src_x, src_y).
            // Coordinates here are view-local.
            let src_pixel = src.get_pixel(x, y);
            let dest_pixel = dest.get_pixel_mut(x + dest_x, y + dest_y);
            *dest_pixel = src_pixel;
        }
    }
    
    Ok(())
}

/// Copy a source rectangle into destination with clipping.
///
/// - `dest_x`/`dest_y` are allowed to be negative.
/// - Pixels outside the destination are silently dropped.
/// - This is the semantic required by the original engine's Parts overlay path.
pub fn copy_rect_clipped(
    src: &DynamicImage,
    src_x: u32,
    src_y: u32,
    src_w: u32,
    src_h: u32,
    dest: &mut DynamicImage,
    dest_x: i32,
    dest_y: i32,
) -> Result<()> {
    let dest_rgba = match dest.as_mut_rgba8() {
        Some(dest) => dest,
        None => return Err(anyhow!("copy_rect_clipped: dest image is not in RGBA8 format")),
    };

    let dw = dest_rgba.width() as i32;
    let dh = dest_rgba.height() as i32;
    let sw = src_w as i32;
    let sh = src_h as i32;

    // Destination rectangle in dest space.
    let dx0 = dest_x;
    let dy0 = dest_y;
    let dx1 = dest_x + sw;
    let dy1 = dest_y + sh;

    // Clip against destination bounds.
    let cx0 = dx0.max(0);
    let cy0 = dy0.max(0);
    let cx1 = dx1.min(dw);
    let cy1 = dy1.min(dh);

    if cx1 <= cx0 || cy1 <= cy0 {
        return Ok(());
    }

    // Source start point after clipping.
    let sx0 = src_x as i32 + (cx0 - dx0);
    let sy0 = src_y as i32 + (cy0 - dy0);

    let copy_w = cx1 - cx0;
    let copy_h = cy1 - cy0;

    // Sanity: ensure we never read outside the declared source rectangle.
    if sx0 < src_x as i32
        || sy0 < src_y as i32
        || sx0 + copy_w > (src_x + src_w) as i32
        || sy0 + copy_h > (src_y + src_h) as i32
    {
        return Err(anyhow!(
            "copy_rect_clipped: computed source rect out of bounds (sx0={},sy0={},w={},h={}, src=({}, {}, {}, {}))",
            sx0,
            sy0,
            copy_w,
            copy_h,
            src_x,
            src_y,
            src_w,
            src_h
        ));
    }

    for y in 0..copy_h {
        for x in 0..copy_w {
            let sx = (sx0 + x) as u32;
            let sy = (sy0 + y) as u32;
            let dx = (cx0 + x) as u32;
            let dy = (cy0 + y) as u32;
            let src_pixel = src.get_pixel(sx, sy);
            let dest_pixel = dest_rgba.get_pixel_mut(dx, dy);
            *dest_pixel = src_pixel;
        }
    }

    Ok(())
}

// ----------------------------
// Save/Load snapshots
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphBuffSnapshotV1 {
    pub id: u16,
    pub r_value: u8,
    pub g_value: u8,
    pub b_value: u8,
    pub texture_ready: bool,
    pub texture_path: String,
    pub offset_x: u16,
    pub offset_y: u16,
    pub width: u16,
    pub height: u16,
    pub u: u16,
    pub v: u16,
    pub load_kind: GraphBuffLoadKind,
    /// Raw RGBA8 pixels (width*height*4). Only present for non-VFS textures.
    pub rgba: Option<Vec<u8>>,
}

impl GraphBuff {
    pub fn capture_snapshot_with_id(&self, id: u16) -> GraphBuffSnapshotV1 {
        // Skip empty graphs.
        if !self.texture_ready && self.texture.is_none() && self.texture_path.is_empty() {
            return GraphBuffSnapshotV1 {
                id,
                r_value: self.r_value,
                g_value: self.g_value,
                b_value: self.b_value,
                texture_ready: self.texture_ready,
                texture_path: self.texture_path.clone(),
                offset_x: self.offset_x,
                offset_y: self.offset_y,
                width: self.width,
                height: self.height,
                u: self.u,
                v: self.v,
                load_kind: self.load_kind,
                rgba: None,
            };
        }

        GraphBuffSnapshotV1 {
            id,
            r_value: self.r_value,
            g_value: self.g_value,
            b_value: self.b_value,
            texture_ready: self.texture_ready,
            texture_path: self.texture_path.clone(),
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            width: self.width,
            height: self.height,
            u: self.u,
            v: self.v,
            load_kind: self.load_kind,
            rgba: if self.texture_path.is_empty() {
                // In-memory textures (text buffers, intermediate results). Persist raw RGBA.
                self.texture.as_ref().map(|img| img.to_rgba8().into_raw())
            } else {
                None
            },
        }
    }

    pub fn apply_snapshot_v1(&mut self, snap: &GraphBuffSnapshotV1, vfs: &Vfs) -> Result<()> {
        // Always reset, then rehydrate.
        self.unload();

        self.r_value = snap.r_value;
        self.g_value = snap.g_value;
        self.b_value = snap.b_value;

        // Prefer VFS re-load if we have a path.
        if !snap.texture_path.is_empty() {
            let bytes = match vfs.read_file(&snap.texture_path) {
                Ok(b) => b,
                Err(e) => {
                    // Fall back to embedded pixels if provided.
                    if let Some(rgba) = &snap.rgba {
                        self.load_from_buff(rgba.clone(), snap.width as u32, snap.height as u32)?;
                        self.offset_x = snap.offset_x;
                        self.offset_y = snap.offset_y;
                        self.u = snap.u;
                        self.v = snap.v;
                        self.texture_path = snap.texture_path.clone();
                        self.texture_ready = snap.texture_ready;
                        self.load_kind = snap.load_kind;
                        self.mark_dirty();
                        return Ok(());
                    }
                    return Err(anyhow!(
                        "apply_snapshot_v1: failed to read {} from vfs: {}",
                        snap.texture_path,
                        e
                    ));
                }
            };

            match snap.load_kind {
                GraphBuffLoadKind::Mask => self.load_mask(&snap.texture_path, bytes)?,
                GraphBuffLoadKind::GaijiGlyph => self.load_gaiji_fontface_glyph(&snap.texture_path, bytes)?,
                _ => self.load_texture(&snap.texture_path, bytes)?,
            }

            // load_* already sets offsets/u/v/size/ready/path/kind.
            return Ok(());
        }

        // No path: require pixels.
        if let Some(rgba) = &snap.rgba {
            self.load_from_buff(rgba.clone(), snap.width as u32, snap.height as u32)?;
            self.offset_x = snap.offset_x;
            self.offset_y = snap.offset_y;
            self.u = snap.u;
            self.v = snap.v;
            self.texture_ready = snap.texture_ready;
            self.load_kind = snap.load_kind;
            self.mark_dirty();
            return Ok(());
        }

        Ok(())
    }
}