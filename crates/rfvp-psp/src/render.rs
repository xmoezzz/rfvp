use core::ffi::c_void;
use core::ptr;

use rfvp::host_api::{
    BlendMode, ColorRgba, DrawSolidCommand, DrawSpriteCommand, PixelFormat, RectI32, RfvpRenderer,
    RfvpResult, TextureDesc, TextureFilter, TextureId, TextureRect, Vertex2D,
};

use crate::raw::{
    RawBlendMode, RawColorRgba, RawDrawSolidCommand, RawDrawSpriteCommand, RawPixelFormat,
    RawRectI32, RawRendererVTable, RawTextureDesc, RawTextureFilter, RawTextureRect, RawVertex2D,
};
use crate::status::status_to_result;

pub struct PspRenderer {
    ctx: *mut c_void,
    vtable: RawRendererVTable,
    initialized: bool,
}

impl PspRenderer {
    pub const fn new(ctx: *mut c_void, vtable: RawRendererVTable) -> Self {
        Self {
            ctx,
            vtable,
            initialized: false,
        }
    }

    fn ensure_initialized(&mut self, width: u32, height: u32) -> RfvpResult<()> {
        if self.initialized {
            return Ok(());
        }
        let status = unsafe { (self.vtable.init)(self.ctx, width, height) };
        status_to_result(status)?;
        self.initialized = true;
        Ok(())
    }
}

impl Drop for PspRenderer {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                (self.vtable.shutdown)(self.ctx);
            }
        }
    }
}

impl RfvpRenderer for PspRenderer {
    fn create_texture(
        &mut self,
        id: TextureId,
        desc: TextureDesc,
        pixels: Option<&[u8]>,
    ) -> RfvpResult<()> {
        let (ptr, len) = pixels.map_or((ptr::null(), 0), |bytes| (bytes.as_ptr(), bytes.len()));
        let stride = desc.width as usize * bytes_per_pixel(desc.format);
        let status = unsafe {
            (self.vtable.create_texture)(self.ctx, id.0, texture_desc(desc), ptr, len, stride)
        };
        status_to_result(status)
    }

    fn update_texture(
        &mut self,
        id: TextureId,
        rect: TextureRect,
        pixels: &[u8],
    ) -> RfvpResult<()> {
        let status = unsafe {
            (self.vtable.update_texture)(
                self.ctx,
                id.0,
                texture_rect(rect),
                pixels.as_ptr(),
                pixels.len(),
                0,
            )
        };
        status_to_result(status)
    }

    fn destroy_texture(&mut self, id: TextureId) {
        unsafe {
            (self.vtable.destroy_texture)(self.ctx, id.0);
        }
    }

    fn begin_frame(&mut self, width: u32, height: u32, clear: Option<ColorRgba>) -> RfvpResult<()> {
        self.ensure_initialized(width, height)?;
        let raw_clear = clear.map(color);
        let clear_ptr = raw_clear.as_ref().map_or(ptr::null(), |value| value);
        let status = unsafe { (self.vtable.begin_frame)(self.ctx, width, height, clear_ptr) };
        status_to_result(status)
    }

    fn draw_sprite(&mut self, command: &DrawSpriteCommand) -> RfvpResult<()> {
        let raw = raw_sprite(command);
        let status = unsafe { (self.vtable.draw_sprite)(self.ctx, &raw) };
        status_to_result(status)
    }

    fn draw_solid(&mut self, command: &DrawSolidCommand) -> RfvpResult<()> {
        let raw = raw_solid(command);
        let status = unsafe { (self.vtable.draw_solid)(self.ctx, &raw) };
        status_to_result(status)
    }

    fn end_frame(&mut self) -> RfvpResult<()> {
        let status = unsafe { (self.vtable.end_frame)(self.ctx) };
        status_to_result(status)
    }

    fn present(&mut self) -> RfvpResult<()> {
        let status = unsafe { (self.vtable.present)(self.ctx) };
        status_to_result(status)
    }
}

fn bytes_per_pixel(value: PixelFormat) -> usize {
    match value {
        PixelFormat::Rgba8 | PixelFormat::Bgra8 => 4,
        PixelFormat::Rgb8 => 3,
        PixelFormat::Luma8 | PixelFormat::Alpha8 => 1,
        PixelFormat::LumaA8 => 2,
    }
}

fn pixel_format(value: PixelFormat) -> RawPixelFormat {
    match value {
        PixelFormat::Rgba8 => RawPixelFormat::Rgba8,
        PixelFormat::Bgra8 => RawPixelFormat::Bgra8,
        PixelFormat::Rgb8 => RawPixelFormat::Rgb8,
        PixelFormat::Luma8 => RawPixelFormat::Luma8,
        PixelFormat::LumaA8 => RawPixelFormat::LumaA8,
        PixelFormat::Alpha8 => RawPixelFormat::Alpha8,
    }
}

fn texture_desc(value: TextureDesc) -> RawTextureDesc {
    RawTextureDesc {
        width: value.width,
        height: value.height,
        format: pixel_format(value.format),
        mip_count: value.mip_count,
        _padding: [0; 3],
    }
}

fn texture_rect(value: TextureRect) -> RawTextureRect {
    RawTextureRect {
        x: value.x,
        y: value.y,
        width: value.width,
        height: value.height,
    }
}

fn color(value: ColorRgba) -> RawColorRgba {
    RawColorRgba {
        r: value.r,
        g: value.g,
        b: value.b,
        a: value.a,
    }
}

fn rect(value: RectI32) -> RawRectI32 {
    RawRectI32 {
        x: value.x,
        y: value.y,
        width: value.width,
        height: value.height,
    }
}

fn blend(value: BlendMode) -> RawBlendMode {
    match value {
        BlendMode::Opaque => RawBlendMode::Opaque,
        BlendMode::Alpha => RawBlendMode::Alpha,
        BlendMode::Add => RawBlendMode::Add,
        BlendMode::Multiply => RawBlendMode::Multiply,
        BlendMode::Screen => RawBlendMode::Screen,
    }
}

fn filter(value: TextureFilter) -> RawTextureFilter {
    match value {
        TextureFilter::Nearest => RawTextureFilter::Nearest,
        TextureFilter::Linear => RawTextureFilter::Linear,
    }
}

fn vertex(value: Vertex2D) -> RawVertex2D {
    RawVertex2D {
        position: value.position,
        tex_coord: value.tex_coord,
        color: color(value.color),
    }
}

fn raw_sprite(value: &DrawSpriteCommand) -> RawDrawSpriteCommand {
    let (has_scissor, scissor) = value.scissor.map(|scissor| (1, rect(scissor))).unwrap_or((
        0,
        RawRectI32 {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        },
    ));
    RawDrawSpriteCommand {
        texture_id: value.texture.0,
        vertices: [
            vertex(value.vertices[0]),
            vertex(value.vertices[1]),
            vertex(value.vertices[2]),
            vertex(value.vertices[3]),
        ],
        blend: blend(value.blend),
        filter: filter(value.filter),
        has_scissor,
        _padding: [0; 3],
        scissor,
    }
}

fn raw_solid(value: &DrawSolidCommand) -> RawDrawSolidCommand {
    let (has_scissor, scissor) = value.scissor.map(|scissor| (1, rect(scissor))).unwrap_or((
        0,
        RawRectI32 {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        },
    ));
    RawDrawSolidCommand {
        rect: rect(value.rect),
        color: color(value.color),
        blend: blend(value.blend),
        has_scissor,
        _padding: [0; 3],
        scissor,
    }
}
