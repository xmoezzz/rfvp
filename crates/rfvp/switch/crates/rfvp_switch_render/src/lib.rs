#![allow(unexpected_cfgs)]
#![cfg(any(target_os = "horizon", target_vendor = "nintendo", rfvp_switch))]
#![no_std]

use core::ffi::c_void;

pub const RFVP_SWITCH_RENDER_API_VERSION: u32 = 2;
pub const MAX_RENDER_COMMANDS: usize = 8192;
pub const MAX_TEXTURES: usize = 2048;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureId(pub u32);

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TextureDesc {
    pub id: TextureId,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TextureUploadRgba8 {
    pub desc: TextureDesc,
    pub data: *const u8,
    pub byte_len: usize,
    pub generation: u64,
}

impl Default for TextureUploadRgba8 {
    fn default() -> Self {
        Self {
            desc: TextureDesc::default(),
            data: core::ptr::null(),
            byte_len: 0,
            generation: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RectF32 {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ColorF32 {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Mat4F32 {
    pub cols: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TexturedQuad {
    pub texture: TextureId,
    pub dst: RectF32,
    pub uv: RectF32,
    pub color: ColorF32,
    pub transform: Mat4F32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct FillQuad {
    pub dst: RectF32,
    pub color: ColorF32,
    pub transform: Mat4F32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderCommandKind {
    None = 0,
    BeginFrame = 1,
    EndFrame = 2,
    Clear = 3,
    UploadTextureRgba8 = 4,
    DrawTexturedQuad = 5,
    DrawFillQuad = 6,
}

impl Default for RenderCommandKind {
    fn default() -> Self {
        Self::None
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union RenderCommandPayload {
    pub color: ColorF32,
    pub texture: TextureDesc,
    pub texture_upload: TextureUploadRgba8,
    pub textured_quad: TexturedQuad,
    pub fill_quad: FillQuad,
    pub empty: [u8; 160],
}

impl Default for RenderCommandPayload {
    fn default() -> Self {
        Self { empty: [0; 160] }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RenderCommand {
    pub kind: RenderCommandKind,
    pub payload: RenderCommandPayload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchRenderError {
    CommandBufferFull,
    TextureTableFull,
    InvalidTexture,
    InvalidUpload,
}

pub struct SwitchRenderer {
    commands: [RenderCommand; MAX_RENDER_COMMANDS],
    command_len: usize,
    textures: [TextureDesc; MAX_TEXTURES],
    texture_len: usize,
    next_texture_id: u32,
    framebuffer_width: u32,
    framebuffer_height: u32,
}

impl SwitchRenderer {
    pub const fn new() -> Self {
        Self {
            commands: [RenderCommand {
                kind: RenderCommandKind::None,
                payload: RenderCommandPayload { empty: [0; 160] },
            }; MAX_RENDER_COMMANDS],
            command_len: 0,
            textures: [TextureDesc {
                id: TextureId(0),
                width: 0,
                height: 0,
            }; MAX_TEXTURES],
            texture_len: 0,
            next_texture_id: 1,
            framebuffer_width: 1280,
            framebuffer_height: 720,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.framebuffer_width = width.max(1);
        self.framebuffer_height = height.max(1);
    }

    pub fn framebuffer_size(&self) -> (u32, u32) {
        (self.framebuffer_width, self.framebuffer_height)
    }

    pub fn begin_frame(&mut self, clear: ColorF32) -> Result<(), SwitchRenderError> {
        self.command_len = 0;
        self.push(RenderCommand {
            kind: RenderCommandKind::BeginFrame,
            payload: RenderCommandPayload { empty: [0; 160] },
        })?;
        self.push(RenderCommand {
            kind: RenderCommandKind::Clear,
            payload: RenderCommandPayload { color: clear },
        })
    }

    pub fn end_frame(&mut self) -> Result<(), SwitchRenderError> {
        self.push(RenderCommand {
            kind: RenderCommandKind::EndFrame,
            payload: RenderCommandPayload { empty: [0; 160] },
        })
    }

    pub fn register_texture_rgba8(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<TextureId, SwitchRenderError> {
        self.register_texture_rgba8_with_pixels(width, height, core::ptr::null(), 0, 0)
    }

    pub fn register_texture_rgba8_with_pixels(
        &mut self,
        width: u32,
        height: u32,
        data: *const u8,
        byte_len: usize,
        generation: u64,
    ) -> Result<TextureId, SwitchRenderError> {
        if self.texture_len >= MAX_TEXTURES {
            return Err(SwitchRenderError::TextureTableFull);
        }
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(4))
            .ok_or(SwitchRenderError::InvalidUpload)?;
        if !data.is_null() && byte_len != expected {
            return Err(SwitchRenderError::InvalidUpload);
        }
        let id = TextureId(self.next_texture_id);
        self.next_texture_id = self.next_texture_id.wrapping_add(1).max(1);
        let desc = TextureDesc { id, width, height };
        self.textures[self.texture_len] = desc;
        self.texture_len += 1;
        self.push(RenderCommand {
            kind: RenderCommandKind::UploadTextureRgba8,
            payload: RenderCommandPayload {
                texture_upload: TextureUploadRgba8 {
                    desc,
                    data,
                    byte_len,
                    generation,
                },
            },
        })?;
        Ok(id)
    }

    pub fn upload_texture_rgba8(
        &mut self,
        id: TextureId,
        width: u32,
        height: u32,
        data: *const u8,
        byte_len: usize,
        generation: u64,
    ) -> Result<(), SwitchRenderError> {
        if id.0 == 0 || !self.has_texture(id) {
            return Err(SwitchRenderError::InvalidTexture);
        }
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(4))
            .ok_or(SwitchRenderError::InvalidUpload)?;
        if data.is_null() || byte_len != expected {
            return Err(SwitchRenderError::InvalidUpload);
        }
        let desc = TextureDesc { id, width, height };
        if let Some(slot) = self.textures[..self.texture_len].iter_mut().find(|t| t.id == id) {
            *slot = desc;
        }
        self.push(RenderCommand {
            kind: RenderCommandKind::UploadTextureRgba8,
            payload: RenderCommandPayload {
                texture_upload: TextureUploadRgba8 {
                    desc,
                    data,
                    byte_len,
                    generation,
                },
            },
        })
    }

    pub fn draw_textured_quad(&mut self, quad: TexturedQuad) -> Result<(), SwitchRenderError> {
        if !self.has_texture(quad.texture) {
            return Err(SwitchRenderError::InvalidTexture);
        }
        self.push(RenderCommand {
            kind: RenderCommandKind::DrawTexturedQuad,
            payload: RenderCommandPayload { textured_quad: quad },
        })
    }

    pub fn draw_fill_quad(&mut self, quad: FillQuad) -> Result<(), SwitchRenderError> {
        self.push(RenderCommand {
            kind: RenderCommandKind::DrawFillQuad,
            payload: RenderCommandPayload { fill_quad: quad },
        })
    }

    pub fn commands(&self) -> &[RenderCommand] {
        &self.commands[..self.command_len]
    }

    pub fn textures(&self) -> &[TextureDesc] {
        &self.textures[..self.texture_len]
    }

    fn push(&mut self, cmd: RenderCommand) -> Result<(), SwitchRenderError> {
        if self.command_len >= MAX_RENDER_COMMANDS {
            return Err(SwitchRenderError::CommandBufferFull);
        }
        self.commands[self.command_len] = cmd;
        self.command_len += 1;
        Ok(())
    }

    fn has_texture(&self, id: TextureId) -> bool {
        self.textures[..self.texture_len].iter().any(|t| t.id == id)
    }
}

impl Default for SwitchRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_render_api_version() -> u32 {
    RFVP_SWITCH_RENDER_API_VERSION
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_render_init(
    renderer: *mut SwitchRenderer,
    width: u32,
    height: u32,
) -> i32 {
    if renderer.is_null() {
        return -1;
    }
    unsafe {
        renderer.write(SwitchRenderer::new());
        (*renderer).resize(width, height);
    }
    0
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_render_begin_frame(
    renderer: *mut SwitchRenderer,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
) -> i32 {
    if renderer.is_null() {
        return -1;
    }
    match unsafe { (*renderer).begin_frame(ColorF32 { r, g, b, a }) } {
        Ok(()) => 0,
        Err(_) => -2,
    }
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_render_end_frame(renderer: *mut SwitchRenderer) -> i32 {
    if renderer.is_null() {
        return -1;
    }
    match unsafe { (*renderer).end_frame() } {
        Ok(()) => 0,
        Err(_) => -2,
    }
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_render_command_count(renderer: *const SwitchRenderer) -> usize {
    if renderer.is_null() {
        return 0;
    }
    unsafe { (*renderer).commands().len() }
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_render_commands(renderer: *const SwitchRenderer) -> *const c_void {
    if renderer.is_null() {
        return core::ptr::null();
    }
    unsafe { (*renderer).commands().as_ptr() as *const c_void }
}
