use core::ffi::c_void;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawFileHandle {
    pub value: u64,
}

impl RawFileHandle {
    pub const INVALID: Self = Self { value: u64::MAX };
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawFileKind {
    File = 0,
    Directory = 1,
    Other = 2,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawFileInfo {
    pub len: u64,
    pub kind: RawFileKind,
}

pub type RawOpenFileFn = unsafe extern "C" fn(
    ctx: *mut c_void,
    path: *const u8,
    path_len: usize,
    out_handle: *mut RawFileHandle,
) -> i32;
pub type RawCloseFileFn = unsafe extern "C" fn(ctx: *mut c_void, handle: RawFileHandle);
pub type RawReadAtFn = unsafe extern "C" fn(
    ctx: *mut c_void,
    handle: RawFileHandle,
    offset: u64,
    buf: *mut u8,
    len: usize,
    out_read: *mut usize,
) -> i32;
pub type RawFileLenFn =
    unsafe extern "C" fn(ctx: *mut c_void, handle: RawFileHandle, out_len: *mut u64) -> i32;
pub type RawMetadataFn = unsafe extern "C" fn(
    ctx: *mut c_void,
    path: *const u8,
    path_len: usize,
    out_info: *mut RawFileInfo,
) -> i32;
pub type RawWriteAllFn = unsafe extern "C" fn(
    ctx: *mut c_void,
    path: *const u8,
    path_len: usize,
    bytes: *const u8,
    byte_len: usize,
) -> i32;
pub type RawEnumerateByExtensionVisitorFn = unsafe extern "C" fn(
    visitor_ctx: *mut c_void,
    path: *const u8,
    path_len: usize,
    info: RawFileInfo,
) -> i32;
pub type RawEnumerateByExtensionFn = unsafe extern "C" fn(
    ctx: *mut c_void,
    root: *const u8,
    root_len: usize,
    extension: *const u8,
    extension_len: usize,
    visitor_ctx: *mut c_void,
    visitor: RawEnumerateByExtensionVisitorFn,
) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawFileSystemVTable {
    pub open: RawOpenFileFn,
    pub close: RawCloseFileFn,
    pub read_at: RawReadAtFn,
    pub len: RawFileLenFn,
    pub metadata: RawMetadataFn,
    pub write_all: RawWriteAllFn,
    pub enumerate_by_extension: Option<RawEnumerateByExtensionFn>,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawPixelFormat {
    Rgba8 = 0,
    Bgra8 = 1,
    Rgb8 = 2,
    Luma8 = 3,
    LumaA8 = 4,
    Alpha8 = 5,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawTextureDesc {
    pub width: u32,
    pub height: u32,
    pub format: RawPixelFormat,
    pub mip_count: u8,
    pub _padding: [u8; 3],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawTextureRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawColorRgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawRectI32 {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawBlendMode {
    Opaque = 0,
    Alpha = 1,
    Add = 2,
    Multiply = 3,
    Screen = 4,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawTextureFilter {
    Nearest = 0,
    Linear = 1,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawVertex2D {
    pub position: [f32; 2],
    pub tex_coord: [f32; 2],
    pub color: RawColorRgba,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawDrawSpriteCommand {
    pub texture_id: u32,
    pub vertices: [RawVertex2D; 4],
    pub blend: RawBlendMode,
    pub filter: RawTextureFilter,
    pub has_scissor: u8,
    pub _padding: [u8; 3],
    pub scissor: RawRectI32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawDrawSolidCommand {
    pub rect: RawRectI32,
    pub color: RawColorRgba,
    pub blend: RawBlendMode,
    pub has_scissor: u8,
    pub _padding: [u8; 3],
    pub scissor: RawRectI32,
}

pub type RawRendererInitFn = unsafe extern "C" fn(ctx: *mut c_void, width: u32, height: u32) -> i32;
pub type RawRendererShutdownFn = unsafe extern "C" fn(ctx: *mut c_void);
pub type RawCreateTextureFn = unsafe extern "C" fn(
    ctx: *mut c_void,
    texture_id: u32,
    desc: RawTextureDesc,
    pixels: *const u8,
    pixels_len: usize,
    stride: usize,
) -> i32;
pub type RawUpdateTextureFn = unsafe extern "C" fn(
    ctx: *mut c_void,
    texture_id: u32,
    rect: RawTextureRect,
    pixels: *const u8,
    pixels_len: usize,
    stride: usize,
) -> i32;
pub type RawDestroyTextureFn = unsafe extern "C" fn(ctx: *mut c_void, texture_id: u32);
pub type RawBeginFrameFn = unsafe extern "C" fn(
    ctx: *mut c_void,
    width: u32,
    height: u32,
    clear: *const RawColorRgba,
) -> i32;
pub type RawDrawSpriteFn =
    unsafe extern "C" fn(ctx: *mut c_void, command: *const RawDrawSpriteCommand) -> i32;
pub type RawDrawSolidFn =
    unsafe extern "C" fn(ctx: *mut c_void, command: *const RawDrawSolidCommand) -> i32;
pub type RawEndFrameFn = unsafe extern "C" fn(ctx: *mut c_void) -> i32;
pub type RawPresentFn = unsafe extern "C" fn(ctx: *mut c_void) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawRendererVTable {
    pub init: RawRendererInitFn,
    pub shutdown: RawRendererShutdownFn,
    pub create_texture: RawCreateTextureFn,
    pub update_texture: RawUpdateTextureFn,
    pub destroy_texture: RawDestroyTextureFn,
    pub begin_frame: RawBeginFrameFn,
    pub draw_sprite: RawDrawSpriteFn,
    pub draw_solid: RawDrawSolidFn,
    pub end_frame: RawEndFrameFn,
    pub present: RawPresentFn,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawAudioParams {
    pub volume: f32,
    pub pan: f32,
    pub repeat: u8,
    pub _padding: [u8; 3],
}

pub type RawLoadNativeAudioFn = unsafe extern "C" fn(
    ctx: *mut c_void,
    stream_id: u32,
    bytes: *const u8,
    byte_len: usize,
) -> i32;
pub type RawPlayNativeAudioFn = unsafe extern "C" fn(
    ctx: *mut c_void,
    stream_id: u32,
    params: RawAudioParams,
    fade_in_ms: u32,
) -> i32;
pub type RawStopNativeAudioFn =
    unsafe extern "C" fn(ctx: *mut c_void, stream_id: u32, fade_ms: u32) -> i32;
pub type RawPauseNativeAudioFn = unsafe extern "C" fn(ctx: *mut c_void, stream_id: u32) -> i32;
pub type RawResumeNativeAudioFn = unsafe extern "C" fn(ctx: *mut c_void, stream_id: u32) -> i32;
pub type RawSetNativeAudioParamsFn =
    unsafe extern "C" fn(ctx: *mut c_void, stream_id: u32, params: RawAudioParams) -> i32;
pub type RawDestroyNativeAudioFn = unsafe extern "C" fn(ctx: *mut c_void, stream_id: u32);
pub type RawAudioTickFn = unsafe extern "C" fn(ctx: *mut c_void, delta_us: u64) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawAudioVTable {
    pub load_native: RawLoadNativeAudioFn,
    pub play: RawPlayNativeAudioFn,
    pub stop: RawStopNativeAudioFn,
    pub pause: RawPauseNativeAudioFn,
    pub resume: RawResumeNativeAudioFn,
    pub set_params: RawSetNativeAudioParamsFn,
    pub destroy: RawDestroyNativeAudioFn,
    pub tick: RawAudioTickFn,
}

pub type RawTicksUsFn = unsafe extern "C" fn(ctx: *mut c_void) -> u64;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawClockVTable {
    pub ticks_us: RawTicksUsFn,
}

pub type RawThreeDsLogFn =
    unsafe extern "C" fn(ctx: *mut c_void, level: u32, message: *const u8, message_len: usize);
pub type RawThreeDsFatalFn =
    unsafe extern "C" fn(ctx: *mut c_void, code: u32, message: *const u8, message_len: usize);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawThreeDsHost {
    pub fs_ctx: *mut c_void,
    pub fs: RawFileSystemVTable,
    pub renderer_ctx: *mut c_void,
    pub renderer: RawRendererVTable,
    pub audio_ctx: *mut c_void,
    pub audio: RawAudioVTable,
    pub clock_ctx: *mut c_void,
    pub clock: RawClockVTable,
    pub log_ctx: *mut c_void,
    pub log: Option<RawThreeDsLogFn>,
    pub fatal_ctx: *mut c_void,
    pub fatal: Option<RawThreeDsFatalFn>,
}
