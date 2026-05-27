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

pub struct PsvRenderer {
    ctx: *mut c_void,
    vtable: RawRendererVTable,
}

impl PsvRenderer {
    pub const fn new(ctx: *mut c_void, vtable: RawRendererVTable) -> Self {
        Self { ctx, vtable }
    }
}

impl RfvpRenderer for PsvRenderer {
    fn create_texture(
        &mut self,
        id: TextureId,
        desc: TextureDesc,
        pixels: Option<&[u8]>,
    ) -> RfvpResult<()> {
        let raw_desc = texture_desc_to_raw(desc);
        let (ptr, len) = match pixels {
            Some(bytes) => (bytes.as_ptr(), bytes.len()),
            None => (ptr::null(), 0),
        };
        let status = unsafe { (self.vtable.create_texture)(self.ctx, id.0, raw_desc, ptr, len) };
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
                texture_rect_to_raw(rect),
                pixels.as_ptr(),
                pixels.len(),
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
        let raw_clear = clear.map(color_to_raw);
        let clear_ptr = match raw_clear.as_ref() {
            Some(value) => value as *const RawColorRgba,
            None => ptr::null(),
        };
        let status = unsafe { (self.vtable.begin_frame)(self.ctx, width, height, clear_ptr) };
        status_to_result(status)
    }

    fn draw_sprite(&mut self, command: &DrawSpriteCommand) -> RfvpResult<()> {
        let raw = draw_sprite_to_raw(command);
        let status = unsafe { (self.vtable.draw_sprite)(self.ctx, &raw) };
        status_to_result(status)
    }

    fn draw_solid(&mut self, command: &DrawSolidCommand) -> RfvpResult<()> {
        let raw = draw_solid_to_raw(command);
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

fn pixel_format_to_raw(value: PixelFormat) -> RawPixelFormat {
    match value {
        PixelFormat::Rgba8 => RawPixelFormat::Rgba8,
        PixelFormat::Bgra8 => RawPixelFormat::Bgra8,
        PixelFormat::Rgb8 => RawPixelFormat::Rgb8,
        PixelFormat::Luma8 => RawPixelFormat::Luma8,
        PixelFormat::LumaA8 => RawPixelFormat::LumaA8,
        PixelFormat::Alpha8 => RawPixelFormat::Alpha8,
    }
}

fn blend_mode_to_raw(value: BlendMode) -> RawBlendMode {
    match value {
        BlendMode::Opaque => RawBlendMode::Opaque,
        BlendMode::Alpha => RawBlendMode::Alpha,
        BlendMode::Add => RawBlendMode::Add,
        BlendMode::Multiply => RawBlendMode::Multiply,
        BlendMode::Screen => RawBlendMode::Screen,
    }
}

fn texture_filter_to_raw(value: TextureFilter) -> RawTextureFilter {
    match value {
        TextureFilter::Nearest => RawTextureFilter::Nearest,
        TextureFilter::Linear => RawTextureFilter::Linear,
    }
}

fn texture_desc_to_raw(value: TextureDesc) -> RawTextureDesc {
    RawTextureDesc {
        width: value.width,
        height: value.height,
        format: pixel_format_to_raw(value.format),
        mip_count: value.mip_count,
        _padding: [0; 3],
    }
}

fn texture_rect_to_raw(value: TextureRect) -> RawTextureRect {
    RawTextureRect {
        x: value.x,
        y: value.y,
        width: value.width,
        height: value.height,
    }
}

fn color_to_raw(value: ColorRgba) -> RawColorRgba {
    RawColorRgba {
        r: value.r,
        g: value.g,
        b: value.b,
        a: value.a,
    }
}

fn rect_to_raw(value: RectI32) -> RawRectI32 {
    RawRectI32 {
        x: value.x,
        y: value.y,
        width: value.width,
        height: value.height,
    }
}

fn vertex_to_raw(value: Vertex2D) -> RawVertex2D {
    RawVertex2D {
        position: value.position,
        tex_coord: value.tex_coord,
        color: color_to_raw(value.color),
    }
}

fn draw_sprite_to_raw(value: &DrawSpriteCommand) -> RawDrawSpriteCommand {
    let scissor = value.scissor.unwrap_or(RectI32 {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    });
    RawDrawSpriteCommand {
        texture_id: value.texture.0,
        vertices: [
            vertex_to_raw(value.vertices[0]),
            vertex_to_raw(value.vertices[1]),
            vertex_to_raw(value.vertices[2]),
            vertex_to_raw(value.vertices[3]),
        ],
        blend: blend_mode_to_raw(value.blend),
        filter: texture_filter_to_raw(value.filter),
        has_scissor: u8::from(value.scissor.is_some()),
        _padding: [0; 3],
        scissor: rect_to_raw(scissor),
    }
}

fn draw_solid_to_raw(value: &DrawSolidCommand) -> RawDrawSolidCommand {
    let scissor = value.scissor.unwrap_or(RectI32 {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    });
    RawDrawSolidCommand {
        rect: rect_to_raw(value.rect),
        color: color_to_raw(value.color),
        blend: blend_mode_to_raw(value.blend),
        has_scissor: u8::from(value.scissor.is_some()),
        _padding: [0; 3],
        scissor: rect_to_raw(scissor),
    }
}
