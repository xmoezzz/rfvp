use alloc::vec::Vec;

use super::error::{RfvpError, RfvpResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderTargetId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Rgba8,
    Bgra8,
    Rgb8,
    Luma8,
    LumaA8,
    Alpha8,
    /// PlayStation CLUT pictures: u16 colors, u16 stride (pixels a row), the palette (15-bit
    /// colors, bit 15 STP, 0 not drawn), then 4-bit (two a byte, low first) or 8-bit indices.
    #[cfg(feature = "psx")]
    Clut4,
    #[cfg(feature = "psx")]
    Clut8,
    /// PS1-only offline mask thresholds, animated through a three-state CLUT.
    #[cfg(feature = "psx")]
    Ps1Mask8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Opaque,
    Alpha,
    Add,
    Multiply,
    Screen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFilter {
    Nearest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureDesc {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub mip_count: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorRgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl ColorRgba {
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RectI32 {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex2D {
    pub position: [f32; 2],
    pub tex_coord: [f32; 2],
    pub color: ColorRgba,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawSpriteCommand {
    pub texture: TextureId,
    /// The four corners of the sprite in triangle-strip order: the sprite is the triangles
    /// `(0, 1, 2)` and `(2, 1, 3)`. For an axis-aligned sprite that is left/bottom, left/top,
    /// right/bottom, right/top. Drawing `(0, 2, 3)` as the second triangle leaves a hole.
    pub vertices: [Vertex2D; 4],
    pub blend: BlendMode,
    pub filter: TextureFilter,
    pub scissor: Option<RectI32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawSolidCommand {
    pub rect: RectI32,
    pub color: ColorRgba,
    pub blend: BlendMode,
    pub scissor: Option<RectI32>,
}

pub trait RfvpRenderer {
    /// PS1-only bounded row uploads for immutable CLUT resources.
    #[cfg(feature = "psx")]
    fn create_texture_rows(&mut self, _id: TextureId, _desc: TextureDesc, _palette: &[u8], _read: &mut TextureRowReader<'_>) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }
    fn create_texture(
        &mut self,
        id: TextureId,
        desc: TextureDesc,
        pixels: Option<&[u8]>,
    ) -> RfvpResult<()>;

    fn update_texture(&mut self, id: TextureId, rect: TextureRect, pixels: &[u8])
        -> RfvpResult<()>;

    /// Pixels are the COMPLETE image at `desc` size; `rect` marks the changed source
    /// area. The host may resample in global coordinates and reuse its texture.
    /// False means unsupported/incompatible and asks the caller for full recreation.
    fn update_texture_region_from_full(&mut self, _id: TextureId, _desc: TextureDesc,
        _rect: TextureRect, _pixels: &[u8]) -> RfvpResult<bool> { Ok(false) }

    fn destroy_texture(&mut self, id: TextureId);

    fn begin_frame(&mut self, width: u32, height: u32, clear: Option<ColorRgba>) -> RfvpResult<()>;

    fn draw_sprite(&mut self, command: &DrawSpriteCommand) -> RfvpResult<()>;

    fn draw_solid(&mut self, command: &DrawSolidCommand) -> RfvpResult<()>;

    fn end_frame(&mut self) -> RfvpResult<()>;

    fn present(&mut self) -> RfvpResult<()>;

    /// The game frame drawn last, scaled to `width` x `height` and returned as RGBA8, for the
    /// thumbnail of a save. A renderer that cannot read its frame back returns `None` and the
    /// thumbnail is left blank.
    fn capture_thumbnail(&mut self, _width: u32, _height: u32) -> Option<Vec<u8>> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RectI16 {
    pub x: i16,
    pub y: i16,
    pub w: i16,
    pub h: i16,
}

impl RectI16 {
    pub fn contains(self, x: i16, y: i16) -> bool {
        if self.w <= 0 || self.h <= 0 {
            return false;
        }
        let right = self.x.saturating_add(self.w);
        let bottom = self.y.saturating_add(self.h);
        x >= self.x && x < right && y >= self.y && y < bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RectU16 {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    fn to_color_rgba(self) -> ColorRgba {
        ColorRgba {
            r: self.r as f32 / 255.0,
            g: self.g as f32 / 255.0,
            b: self.b as f32 / 255.0,
            a: self.a as f32 / 255.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrimId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandBlendMode {
    Normal,
    Add,
    Sub,
    Mul,
}

impl CommandBlendMode {
    fn to_host_blend(self) -> BlendMode {
        match self {
            Self::Normal => BlendMode::Alpha,
            Self::Add => BlendMode::Add,
            Self::Sub => BlendMode::Alpha,
            Self::Mul => BlendMode::Multiply,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawImageCmd {
    pub texture: TextureHandle,
    pub src: RectU16,
    pub dst: RectI16,
    pub color: Rgba8,
    pub blend: CommandBlendMode,
    pub effect_id: u16,
    pub clip: Option<RectI16>,
    pub vertices: [Vertex2D; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawGlyphCmd {
    pub texture: TextureHandle,
    pub src: RectU16,
    pub dst: RectI16,
    pub color: Rgba8,
    pub clip: Option<RectI16>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RenderCommand {
    DrawImage(DrawImageCmd),
    DrawGlyph(DrawGlyphCmd),
    SetClip(RectI16),
    ClearClip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFormat {
    Rgba8,
    Rgb565,
    Rgba4444,
    Indexed4,
    Indexed8,
    LumaA8,
    Native(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortableTextureDesc {
    pub width: u16,
    pub height: u16,
    pub format: TextureFormat,
}

pub trait TextureBackend {
    type Error;

    fn create_texture(
        &mut self,
        handle: TextureHandle,
        desc: PortableTextureDesc,
        data: &[u8],
    ) -> Result<(), Self::Error>;

    fn destroy_texture(&mut self, handle: TextureHandle);

    #[cfg(feature = "psx")]
    fn create_texture_rows(&mut self, _handle: TextureHandle, _desc: PortableTextureDesc, _palette: &[u8], _read: &mut TextureRowReader<'_>) -> Option<Result<(), Self::Error>> {
        None
    }

    /// Update a complete RGBA texture when supported, otherwise ask the caller to recreate it.
    fn update_texture_data(&mut self, _handle: TextureHandle, _desc: PortableTextureDesc, _data: &[u8]) -> Result<bool, Self::Error> {
        Ok(false)
    }

    fn update_texture_region_from_full(&mut self, _handle: TextureHandle, _desc: PortableTextureDesc,
        _rect: RectU16, _data: &[u8]) -> Result<bool, Self::Error> { Ok(false) }
}

#[cfg(feature = "psx")]
pub type TextureRowReader<'a> = dyn FnMut(u32, u32, u32, u32, &mut [u8], u32) -> RfvpResult<()> + 'a;

pub trait RenderBackend {
    type Error;

    fn begin_frame(&mut self, width: u16, height: u16) -> Result<(), Self::Error>;

    fn submit_commands(&mut self, commands: &[RenderCommand]) -> Result<(), Self::Error>;

    fn end_frame(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitProxy {
    pub prim_id: PrimId,
    pub rect: RectI16,
    pub enabled: bool,
    pub visible: bool,
    pub order: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HitProxyTable {
    pub proxies: Vec<HitProxy>,
}

impl HitProxyTable {
    pub fn clear(&mut self) {
        self.proxies.clear();
    }

    pub fn push(&mut self, proxy: HitProxy) {
        self.proxies.push(proxy);
    }

    pub fn hit_test(&self, x: i16, y: i16) -> Option<PrimId> {
        let mut best: Option<HitProxy> = None;
        for proxy in &self.proxies {
            if !proxy.enabled || !proxy.visible || !proxy.rect.contains(x, y) {
                continue;
            }
            let replace = match best {
                None => true,
                Some(current) => proxy.order > current.order,
            };
            if replace {
                best = Some(*proxy);
            }
        }
        best.map(|proxy| proxy.prim_id)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderFrame {
    pub commands: Vec<RenderCommand>,
    pub hit_proxies: HitProxyTable,
}

fn portable_texture_format(format: TextureFormat) -> Result<PixelFormat, RfvpError> {
    match format {
        TextureFormat::Rgba8 => Ok(PixelFormat::Rgba8),
        TextureFormat::LumaA8 => Ok(PixelFormat::LumaA8),
        #[cfg(feature = "psx")]
        TextureFormat::Indexed4 => Ok(PixelFormat::Clut4),
        #[cfg(feature = "psx")]
        TextureFormat::Indexed8 => Ok(PixelFormat::Clut8),
        #[cfg(feature = "psx")]
        TextureFormat::Native(crate::ps1_mask::NATIVE_FORMAT) => Ok(PixelFormat::Ps1Mask8),
        _ => Err(RfvpError::Unsupported),
    }
}

fn command_rect_to_host(rect: RectI16) -> RectI32 {
    RectI32 {
        x: rect.x as i32,
        y: rect.y as i32,
        width: rect.w as i32,
        height: rect.h as i32,
    }
}

impl<T: RfvpRenderer> TextureBackend for T {
    type Error = RfvpError;

    #[cfg(feature = "psx")]
    fn create_texture_rows(&mut self, handle: TextureHandle, desc: PortableTextureDesc, palette: &[u8], read: &mut TextureRowReader<'_>) -> Option<Result<(), Self::Error>> {
        Some(RfvpRenderer::create_texture_rows(self, TextureId(handle.0), TextureDesc {
            width: u32::from(desc.width), height: u32::from(desc.height),
            format: match portable_texture_format(desc.format) { Ok(format) => format, Err(error) => return Some(Err(error)) },
            mip_count: 1,
        }, palette, read))
    }

    fn create_texture(
        &mut self,
        handle: TextureHandle,
        desc: PortableTextureDesc,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        RfvpRenderer::create_texture(
            self,
            TextureId(handle.0),
            TextureDesc {
                width: desc.width as u32,
                height: desc.height as u32,
                format: portable_texture_format(desc.format)?,
                mip_count: 1,
            },
            Some(data),
        )
    }

    fn destroy_texture(&mut self, handle: TextureHandle) {
        RfvpRenderer::destroy_texture(self, TextureId(handle.0));
    }

    fn update_texture_region_from_full(&mut self, handle: TextureHandle, desc: PortableTextureDesc,
        rect: RectU16, data: &[u8]) -> Result<bool, Self::Error> {
        if desc.format != TextureFormat::Rgba8 { return Ok(false); }
        RfvpRenderer::update_texture_region_from_full(self, TextureId(handle.0), TextureDesc {
            width: u32::from(desc.width), height: u32::from(desc.height),
            format: PixelFormat::Rgba8, mip_count: 1,
        }, TextureRect {
            x: u32::from(rect.x), y: u32::from(rect.y),
            width: u32::from(rect.w), height: u32::from(rect.h),
        }, data)
    }

    fn update_texture_data(&mut self, handle: TextureHandle, desc: PortableTextureDesc, data: &[u8]) -> Result<bool, Self::Error> {
        if desc.format != TextureFormat::Rgba8 { return Ok(false); }
        match RfvpRenderer::update_texture(self, TextureId(handle.0), TextureRect {
            x: 0, y: 0, width: u32::from(desc.width), height: u32::from(desc.height),
        }, data) {
            Ok(()) => Ok(true),
            Err(RfvpError::Unsupported) => Ok(false),
            Err(error) => Err(error),
        }
    }
}

impl<T: RfvpRenderer> RenderBackend for T {
    type Error = RfvpError;

    fn begin_frame(&mut self, width: u16, height: u16) -> Result<(), Self::Error> {
        RfvpRenderer::begin_frame(self, width as u32, height as u32, None)
    }

    fn submit_commands(&mut self, commands: &[RenderCommand]) -> Result<(), Self::Error> {
        let mut clip: Option<RectI16> = None;
        for command in commands {
            match *command {
                RenderCommand::SetClip(rect) => {
                    clip = Some(rect);
                }
                RenderCommand::ClearClip => {
                    clip = None;
                }
                RenderCommand::DrawImage(cmd) => {
                    let scissor = cmd.clip.or(clip).map(command_rect_to_host);
                    RfvpRenderer::draw_sprite(
                        self,
                        &DrawSpriteCommand {
                            texture: TextureId(cmd.texture.0),
                            vertices: cmd.vertices,
                            blend: cmd.blend.to_host_blend(),
                            filter: TextureFilter::Linear,
                            scissor,
                        },
                    )?;
                }
                RenderCommand::DrawGlyph(cmd) => {
                    let scissor = cmd.clip.or(clip).map(command_rect_to_host);
                    let color = cmd.color.to_color_rgba();
                    let x0 = cmd.dst.x as f32;
                    let y0 = cmd.dst.y as f32;
                    let x1 = cmd.dst.x.saturating_add(cmd.dst.w) as f32;
                    let y1 = cmd.dst.y.saturating_add(cmd.dst.h) as f32;
                    RfvpRenderer::draw_sprite(
                        self,
                        &DrawSpriteCommand {
                            texture: TextureId(cmd.texture.0),
                            vertices: [
                                Vertex2D {
                                    position: [x0, y1],
                                    tex_coord: [0.0, 1.0],
                                    color,
                                },
                                Vertex2D {
                                    position: [x0, y0],
                                    tex_coord: [0.0, 0.0],
                                    color,
                                },
                                Vertex2D {
                                    position: [x1, y1],
                                    tex_coord: [1.0, 1.0],
                                    color,
                                },
                                Vertex2D {
                                    position: [x1, y0],
                                    tex_coord: [1.0, 0.0],
                                    color,
                                },
                            ],
                            blend: BlendMode::Alpha,
                            filter: TextureFilter::Linear,
                            scissor,
                        },
                    )?;
                }
            }
        }
        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), Self::Error> {
        RfvpRenderer::end_frame(self)?;
        RfvpRenderer::present(self)
    }
}
