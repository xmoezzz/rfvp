use alloc::alloc::{alloc, dealloc, Layout};
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::mem::{size_of, MaybeUninit};
use core::ptr;

use psp::sys;
use rfvp::{RfvpBootConfig, RfvpCoreConfig};

use crate::raw::{
    RawAudioParams, RawAudioVTable, RawBlendMode, RawClockVTable, RawColorRgba,
    RawDrawSolidCommand, RawDrawSpriteCommand, RawFileHandle, RawFileInfo, RawFileKind,
    RawFileSystemVTable, RawPixelFormat, RawPspHost, RawRectI32, RawRendererVTable, RawTextureDesc,
    RawTextureFilter, RawTextureRect,
};
use crate::status::{rfvp_error_to_status, PspStatus};
use crate::PspApp;

const SCREEN_W: u32 = 480;
const SCREEN_H: u32 = 272;
const STRIDE: i32 = 512;
const MAX_PATH: usize = 512;
const MAX_TEXTURES: usize = 256;
const LIST_WORDS: usize = 262_144 / size_of::<u32>();
const MAX_TEX_SIZE: u32 = 512;

#[repr(C)]
struct PspVertex {
    u: f32,
    v: f32,
    color: u32,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Clone, Copy)]
struct TextureSlot {
    width: u32,
    height: u32,
    format: RawPixelFormat,
    pixels: *mut u32,
    bytes_len: usize,
}

impl TextureSlot {
    const fn empty() -> Self {
        Self {
            width: 0,
            height: 0,
            format: RawPixelFormat::Rgba8,
            pixels: ptr::null_mut(),
            bytes_len: 0,
        }
    }

    fn is_live(self) -> bool {
        !self.pixels.is_null()
    }
}

static mut TEXTURES: [TextureSlot; MAX_TEXTURES] = [TextureSlot::empty(); MAX_TEXTURES];
static mut GU_LIST: psp::Align16<[u32; LIST_WORDS]> = psp::Align16([0; LIST_WORDS]);
static mut RENDERER_INITIALIZED: bool = false;
static mut FRAME_W: u32 = SCREEN_W;
static mut FRAME_H: u32 = SCREEN_H;
static mut PREV_BUTTONS: sys::CtrlButtons = sys::CtrlButtons::empty();
static mut SHOULD_EXIT: bool = false;
static mut TICK_RESOLUTION: u32 = 1_000_000;
static mut START_TICK: u64 = 0;

pub fn run() -> i32 {
    unsafe {
        let init_status = platform_init();
        if init_status != PspStatus::Ok.as_i32() {
            return init_status;
        }

        let raw_host = make_raw_host();
        let mut app = PspApp::new(raw_host, RfvpCoreConfig::default(), 128, SCREEN_W, SCREEN_H);
        if let Err(err) = app.boot_old_school(RfvpBootConfig {
            max_hcb_bytes: 2 * 1024 * 1024,
            max_manifest_entries: 128,
            ..RfvpBootConfig::default()
        }) {
            platform_fini();
            return rfvp_error_to_status(err);
        }

        loop {
            let poll_status = platform_poll(&mut app);
            if poll_status != PspStatus::Ok.as_i32() {
                platform_fini();
                return poll_status;
            }
            if let Err(err) = app.run_frame() {
                platform_fini();
                return rfvp_error_to_status(err);
            }
            if app.quit_requested() || SHOULD_EXIT {
                break;
            }
        }

        platform_fini();
        PspStatus::Ok.as_i32()
    }
}

unsafe fn make_raw_host() -> RawPspHost {
    RawPspHost {
        fs_ctx: ptr::null_mut(),
        fs: RawFileSystemVTable {
            open: fs_open,
            close: fs_close,
            read_at: fs_read_at,
            len: fs_len,
            metadata: fs_metadata,
            write_all: fs_write_all,
            enumerate_by_extension: Some(fs_enumerate_by_extension),
        },
        renderer_ctx: ptr::null_mut(),
        renderer: RawRendererVTable {
            init: renderer_init,
            shutdown: renderer_shutdown,
            create_texture: renderer_create_texture,
            update_texture: renderer_update_texture,
            destroy_texture: renderer_destroy_texture,
            begin_frame: renderer_begin_frame,
            draw_sprite: renderer_draw_sprite,
            draw_solid: renderer_draw_solid,
            end_frame: renderer_end_frame,
            present: renderer_present,
        },
        audio_ctx: ptr::null_mut(),
        audio: RawAudioVTable {
            load_native: audio_unsupported_load,
            play: audio_unsupported_play,
            stop: audio_unsupported_stop,
            pause: audio_unsupported_slot,
            resume: audio_unsupported_slot,
            set_params: audio_unsupported_params,
            destroy: audio_destroy,
            tick: audio_tick,
        },
        clock_ctx: ptr::null_mut(),
        clock: RawClockVTable {
            ticks_us: clock_ticks_us,
        },
        log_ctx: ptr::null_mut(),
        log: Some(psp_log),
        fatal_ctx: ptr::null_mut(),
        fatal: Some(psp_fatal),
    }
}

unsafe fn platform_init() -> i32 {
    sys::sceDisplaySetMode(sys::DisplayMode::Lcd, SCREEN_W as usize, SCREEN_H as usize);
    sys::sceCtrlSetSamplingCycle(0);
    sys::sceCtrlSetSamplingMode(sys::CtrlMode::Analog);
    TICK_RESOLUTION = sys::sceRtcGetTickResolution().max(1);
    let mut now = 0_u64;
    let status = sys::sceRtcGetCurrentTick(&mut now);
    if status < 0 {
        return PspStatus::Backend.as_i32();
    }
    START_TICK = now;
    SHOULD_EXIT = false;
    PspStatus::Ok.as_i32()
}

unsafe fn platform_fini() {
    renderer_shutdown(ptr::null_mut());
}

unsafe fn platform_poll(app: &mut PspApp) -> i32 {
    let mut data = MaybeUninit::<sys::SceCtrlData>::zeroed();
    let status = sys::sceCtrlReadBufferPositive(data.as_mut_ptr(), 1);
    if status < 0 {
        return PspStatus::Backend.as_i32();
    }
    let data = data.assume_init();
    let buttons = data.buttons;
    let prev = PREV_BUTTONS;
    for status in [
        push_button(app, prev, buttons, sys::CtrlButtons::CROSS, 1),
        push_button(app, prev, buttons, sys::CtrlButtons::CIRCLE, 2),
        push_button(app, prev, buttons, sys::CtrlButtons::TRIANGLE, 3),
        push_button(app, prev, buttons, sys::CtrlButtons::SQUARE, 4),
        push_button(app, prev, buttons, sys::CtrlButtons::LEFT, 5),
        push_button(app, prev, buttons, sys::CtrlButtons::RIGHT, 6),
        push_button(app, prev, buttons, sys::CtrlButtons::UP, 7),
        push_button(app, prev, buttons, sys::CtrlButtons::DOWN, 8),
        push_button(app, prev, buttons, sys::CtrlButtons::LTRIGGER, 9),
        push_button(app, prev, buttons, sys::CtrlButtons::RTRIGGER, 10),
    ] {
        if let Err(status) = status {
            return status;
        }
    }
    if !prev.contains(sys::CtrlButtons::START) && buttons.contains(sys::CtrlButtons::START) {
        if let Err(err) = app.push_event(rfvp::host_api::RfvpEvent::Quit) {
            return rfvp_error_to_status(err);
        }
    }
    PREV_BUTTONS = buttons;
    PspStatus::Ok.as_i32()
}

unsafe fn push_button(
    app: &mut PspApp,
    prev: sys::CtrlButtons,
    now: sys::CtrlButtons,
    button: sys::CtrlButtons,
    key_id: u32,
) -> Result<(), i32> {
    let was = prev.contains(button);
    let is = now.contains(button);
    if was != is {
        app.push_key(key_from_psp_id(key_id), is)
            .map_err(rfvp_error_to_status)?;
    }
    Ok(())
}

fn key_from_psp_id(key_id: u32) -> rfvp::host_api::KeyCode {
    match key_id {
        1 => rfvp::host_api::KeyCode::Return,
        2 => rfvp::host_api::KeyCode::Escape,
        3 => rfvp::host_api::KeyCode::Space,
        4 => rfvp::host_api::KeyCode::Backspace,
        5 => rfvp::host_api::KeyCode::Left,
        6 => rfvp::host_api::KeyCode::Right,
        7 => rfvp::host_api::KeyCode::Up,
        8 => rfvp::host_api::KeyCode::Down,
        9 => rfvp::host_api::KeyCode::PageUp,
        10 => rfvp::host_api::KeyCode::PageDown,
        other => rfvp::host_api::KeyCode::Unknown(other),
    }
}

unsafe extern "C" fn fs_open(
    _ctx: *mut c_void,
    path: *const u8,
    path_len: usize,
    out_handle: *mut RawFileHandle,
) -> i32 {
    if path.is_null() || out_handle.is_null() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let Some(path) = c_path(path, path_len) else {
        return PspStatus::InvalidArgument.as_i32();
    };
    let fd = sys::sceIoOpen(path.as_ptr(), sys::IoOpenFlags::RD_ONLY, 0);
    if fd.0 < 0 {
        return PspStatus::NotFound.as_i32();
    }
    (*out_handle).value = fd.0 as u64;
    PspStatus::Ok.as_i32()
}

unsafe extern "C" fn fs_close(_ctx: *mut c_void, handle: RawFileHandle) {
    if handle != RawFileHandle::INVALID {
        let _ = sys::sceIoClose(sys::SceUid(handle.value as i32));
    }
}

unsafe extern "C" fn fs_read_at(
    _ctx: *mut c_void,
    handle: RawFileHandle,
    offset: u64,
    buf: *mut u8,
    len: usize,
    out_read: *mut usize,
) -> i32 {
    if buf.is_null() || out_read.is_null() || offset > i32::MAX as u64 || len > u32::MAX as usize {
        return PspStatus::InvalidArgument.as_i32();
    }
    let fd = sys::SceUid(handle.value as i32);
    if sys::sceIoLseek32(fd, offset as i32, sys::IoWhence::Set) < 0 {
        return PspStatus::Io.as_i32();
    }
    let read = sys::sceIoRead(fd, buf.cast::<c_void>(), len as u32);
    if read < 0 {
        return PspStatus::Io.as_i32();
    }
    *out_read = read as usize;
    PspStatus::Ok.as_i32()
}

unsafe extern "C" fn fs_len(_ctx: *mut c_void, handle: RawFileHandle, out_len: *mut u64) -> i32 {
    if out_len.is_null() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let fd = sys::SceUid(handle.value as i32);
    let cur = sys::sceIoLseek32(fd, 0, sys::IoWhence::Cur);
    if cur < 0 {
        return PspStatus::Io.as_i32();
    }
    let end = sys::sceIoLseek32(fd, 0, sys::IoWhence::End);
    if end < 0 {
        return PspStatus::Io.as_i32();
    }
    let _ = sys::sceIoLseek32(fd, cur, sys::IoWhence::Set);
    *out_len = end as u64;
    PspStatus::Ok.as_i32()
}

unsafe extern "C" fn fs_metadata(
    _ctx: *mut c_void,
    path: *const u8,
    path_len: usize,
    out_info: *mut RawFileInfo,
) -> i32 {
    if path.is_null() || out_info.is_null() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let Some(path) = c_path(path, path_len) else {
        return PspStatus::InvalidArgument.as_i32();
    };
    let mut stat = MaybeUninit::<sys::SceIoStat>::zeroed();
    let status = sys::sceIoGetstat(path.as_ptr(), stat.as_mut_ptr());
    if status < 0 {
        return PspStatus::NotFound.as_i32();
    }
    *out_info = stat_to_info(stat.assume_init());
    PspStatus::Ok.as_i32()
}

unsafe extern "C" fn fs_write_all(
    _ctx: *mut c_void,
    path: *const u8,
    path_len: usize,
    bytes: *const u8,
    byte_len: usize,
) -> i32 {
    if path.is_null() || (byte_len != 0 && bytes.is_null()) {
        return PspStatus::InvalidArgument.as_i32();
    }
    let Some(path) = c_path(path, path_len) else {
        return PspStatus::InvalidArgument.as_i32();
    };
    let flags = sys::IoOpenFlags::WR_ONLY | sys::IoOpenFlags::CREAT | sys::IoOpenFlags::TRUNC;
    let fd = sys::sceIoOpen(path.as_ptr(), flags, 0o666);
    if fd.0 < 0 {
        return PspStatus::Io.as_i32();
    }
    let mut written = 0usize;
    while written < byte_len {
        let chunk = sys::sceIoWrite(fd, bytes.add(written).cast::<c_void>(), byte_len - written);
        if chunk <= 0 {
            let _ = sys::sceIoClose(fd);
            return PspStatus::Io.as_i32();
        }
        written += chunk as usize;
    }
    let close = sys::sceIoClose(fd);
    if close < 0 {
        return PspStatus::Io.as_i32();
    }
    PspStatus::Ok.as_i32()
}

unsafe extern "C" fn fs_enumerate_by_extension(
    _ctx: *mut c_void,
    root: *const u8,
    root_len: usize,
    extension: *const u8,
    extension_len: usize,
    visitor_ctx: *mut c_void,
    visitor: crate::raw::RawEnumerateByExtensionVisitorFn,
) -> i32 {
    if root.is_null() || extension.is_null() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let Ok(root) = core::str::from_utf8(core::slice::from_raw_parts(root, root_len)) else {
        return PspStatus::InvalidArgument.as_i32();
    };
    let Ok(extension) = core::str::from_utf8(core::slice::from_raw_parts(extension, extension_len))
    else {
        return PspStatus::InvalidArgument.as_i32();
    };
    enumerate_dir(root, extension, visitor_ctx, visitor)
}

unsafe fn enumerate_dir(
    root: &str,
    extension: &str,
    visitor_ctx: *mut c_void,
    visitor: crate::raw::RawEnumerateByExtensionVisitorFn,
) -> i32 {
    let Some(path) = c_path(root.as_ptr(), root.len()) else {
        return PspStatus::InvalidArgument.as_i32();
    };
    let dir = sys::sceIoDopen(path.as_ptr());
    if dir.0 < 0 {
        return PspStatus::NotFound.as_i32();
    }
    loop {
        let mut entry = MaybeUninit::<sys::SceIoDirent>::zeroed();
        let status = sys::sceIoDread(dir, entry.as_mut_ptr());
        if status < 0 {
            let _ = sys::sceIoDclose(dir);
            return PspStatus::Io.as_i32();
        }
        if status == 0 {
            break;
        }
        let entry = entry.assume_init();
        let name = dirent_name(&entry);
        if name == "." || name == ".." {
            continue;
        }
        let mut full = String::from(root);
        if !full.is_empty() && !full.ends_with('/') {
            full.push('/');
        }
        full.push_str(name);
        let info = stat_to_info(entry.d_stat);
        if info.kind == RawFileKind::Directory {
            let nested_status = enumerate_dir(&full, extension, visitor_ctx, visitor);
            if nested_status != PspStatus::Ok.as_i32() {
                let _ = sys::sceIoDclose(dir);
                return nested_status;
            }
        } else if info.kind == RawFileKind::File && extension_matches(&full, extension) {
            let status = visitor(visitor_ctx, full.as_ptr(), full.len(), info);
            if status != PspStatus::Ok.as_i32() {
                let _ = sys::sceIoDclose(dir);
                return status;
            }
        }
    }
    let close = sys::sceIoDclose(dir);
    if close < 0 {
        return PspStatus::Io.as_i32();
    }
    PspStatus::Ok.as_i32()
}

unsafe extern "C" fn renderer_init(_ctx: *mut c_void, width: u32, height: u32) -> i32 {
    if RENDERER_INITIALIZED {
        return PspStatus::Ok.as_i32();
    }
    FRAME_W = width.max(1);
    FRAME_H = height.max(1);
    sys::sceGuInit();
    sys::sceGuStart(
        sys::GuContextType::Direct,
        core::ptr::addr_of_mut!(GU_LIST.0).cast(),
    );
    sys::sceGuDrawBuffer(sys::DisplayPixelFormat::Psm8888, ptr::null_mut(), STRIDE);
    sys::sceGuDispBuffer(
        SCREEN_W as i32,
        SCREEN_H as i32,
        frame_buffer_offset(0x88000),
        STRIDE,
    );
    sys::sceGuDepthBuffer(frame_buffer_offset(0x110000), STRIDE);
    sys::sceGuOffset(2048 - (SCREEN_W / 2), 2048 - (SCREEN_H / 2));
    sys::sceGuViewport(2048, 2048, SCREEN_W as i32, SCREEN_H as i32);
    sys::sceGuDisable(sys::GuState::DepthTest);
    sys::sceGuDisable(sys::GuState::CullFace);
    sys::sceGuEnable(sys::GuState::Blend);
    sys::sceGuEnable(sys::GuState::Texture2D);
    sys::sceGuEnable(sys::GuState::ScissorTest);
    sys::sceGuScissor(0, 0, SCREEN_W as i32, SCREEN_H as i32);
    sys::sceGuFinish();
    sys::sceGuSync(sys::GuSyncMode::Finish, sys::GuSyncBehavior::Wait);
    sys::sceDisplayWaitVblankStart();
    sys::sceGuDisplay(true);
    RENDERER_INITIALIZED = true;
    PspStatus::Ok.as_i32()
}

unsafe extern "C" fn renderer_shutdown(_ctx: *mut c_void) {
    let mut index = 0;
    while index < MAX_TEXTURES {
        free_texture(&mut *core::ptr::addr_of_mut!(TEXTURES[index]));
        index += 1;
    }
    if RENDERER_INITIALIZED {
        sys::sceGuDisplay(false);
        sys::sceGuTerm();
        RENDERER_INITIALIZED = false;
    }
}

unsafe extern "C" fn renderer_create_texture(
    _ctx: *mut c_void,
    texture_id: u32,
    desc: RawTextureDesc,
    pixels: *const u8,
    pixels_len: usize,
    stride: usize,
) -> i32 {
    let Some(slot) = texture_slot_mut(texture_id) else {
        return PspStatus::InvalidArgument.as_i32();
    };
    if desc.width == 0 || desc.height == 0 || desc.mip_count > 1 {
        return PspStatus::Unsupported.as_i32();
    }
    let Some(bpp) = texture_bpp(desc.format) else {
        return PspStatus::Unsupported.as_i32();
    };
    let width = next_pow2(desc.width);
    let height = next_pow2(desc.height);
    if width > MAX_TEX_SIZE || height > MAX_TEX_SIZE {
        return PspStatus::Unsupported.as_i32();
    }
    free_texture(slot);
    let bytes_len = width as usize * height as usize * 4;
    let Ok(layout) = Layout::from_size_align(bytes_len, 16) else {
        return PspStatus::OutOfMemory.as_i32();
    };
    let data = alloc(layout).cast::<u32>();
    if data.is_null() {
        return PspStatus::OutOfMemory.as_i32();
    }
    ptr::write_bytes(data, 0, width as usize * height as usize);
    *slot = TextureSlot {
        width,
        height,
        format: desc.format,
        pixels: data,
        bytes_len,
    };
    if !pixels.is_null() && pixels_len != 0 {
        upload_texture_pixels(
            slot,
            0,
            0,
            desc.width,
            desc.height,
            pixels,
            pixels_len,
            stride,
            bpp,
        )
    } else {
        PspStatus::Ok.as_i32()
    }
}

unsafe extern "C" fn renderer_update_texture(
    _ctx: *mut c_void,
    texture_id: u32,
    rect: RawTextureRect,
    pixels: *const u8,
    pixels_len: usize,
    stride: usize,
) -> i32 {
    if pixels.is_null() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let Some(slot) = texture_slot_mut(texture_id) else {
        return PspStatus::InvalidArgument.as_i32();
    };
    if !slot.is_live() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let Some(bpp) = texture_bpp(slot.format) else {
        return PspStatus::Unsupported.as_i32();
    };
    upload_texture_pixels(
        slot,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        pixels,
        pixels_len,
        stride,
        bpp,
    )
}

unsafe extern "C" fn renderer_destroy_texture(_ctx: *mut c_void, texture_id: u32) {
    if let Some(slot) = texture_slot_mut(texture_id) {
        free_texture(slot);
    }
}

unsafe extern "C" fn renderer_begin_frame(
    _ctx: *mut c_void,
    width: u32,
    height: u32,
    clear: *const RawColorRgba,
) -> i32 {
    if !RENDERER_INITIALIZED {
        let status = renderer_init(ptr::null_mut(), width, height);
        if status != PspStatus::Ok.as_i32() {
            return status;
        }
    }
    FRAME_W = width.max(1);
    FRAME_H = height.max(1);
    sys::sceGuStart(
        sys::GuContextType::Direct,
        core::ptr::addr_of_mut!(GU_LIST.0).cast(),
    );
    let clear_color = if clear.is_null() {
        sys::rgba(0, 0, 0, 255)
    } else {
        pack_color(*clear)
    };
    sys::sceGuClearColor(clear_color);
    sys::sceGuClear(sys::ClearBuffer::COLOR_BUFFER_BIT | sys::ClearBuffer::FAST_CLEAR_BIT);
    PspStatus::Ok.as_i32()
}

unsafe extern "C" fn renderer_draw_sprite(
    _ctx: *mut c_void,
    command: *const RawDrawSpriteCommand,
) -> i32 {
    if command.is_null() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let command = &*command;
    let Some(texture) = texture_slot(command.texture_id) else {
        return PspStatus::InvalidArgument.as_i32();
    };
    if !texture.is_live() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let status = set_blend(command.blend);
    if status != PspStatus::Ok.as_i32() {
        return status;
    }
    set_scissor(command.has_scissor != 0, command.scissor);
    sys::sceGuEnable(sys::GuState::Texture2D);
    sys::sceGuTexMode(sys::TexturePixelFormat::Psm8888, 0, 0, 0);
    sys::sceGuTexImage(
        sys::MipmapLevel::None,
        texture.width as i32,
        texture.height as i32,
        texture.width as i32,
        texture.pixels.cast::<c_void>(),
    );
    sys::sceGuTexFunc(
        sys::TextureEffect::Modulate,
        sys::TextureColorComponent::Rgba,
    );
    let (min_filter, mag_filter) = match command.filter {
        RawTextureFilter::Nearest => (sys::TextureFilter::Nearest, sys::TextureFilter::Nearest),
        RawTextureFilter::Linear => (sys::TextureFilter::Linear, sys::TextureFilter::Linear),
    };
    sys::sceGuTexFilter(min_filter, mag_filter);
    sys::sceGuTexWrap(sys::GuTexWrapMode::Clamp, sys::GuTexWrapMode::Clamp);

    let vertices = sys::sceGuGetMemory((4 * size_of::<PspVertex>()) as i32).cast::<PspVertex>();
    if vertices.is_null() {
        return PspStatus::OutOfMemory.as_i32();
    }
    for (index, src) in command.vertices.iter().enumerate() {
        let dst = vertices.add(index);
        (*dst).u = src.tex_coord[0] * texture.width as f32;
        (*dst).v = src.tex_coord[1] * texture.height as f32;
        (*dst).color = pack_color(src.color);
        (*dst).x = scale_x(src.position[0]);
        (*dst).y = scale_y(src.position[1]);
        (*dst).z = 0.0;
    }
    sys::sceGuDrawArray(
        sys::GuPrimitive::TriangleStrip,
        sys::VertexType::TEXTURE_32BITF
            | sys::VertexType::COLOR_8888
            | sys::VertexType::VERTEX_32BITF
            | sys::VertexType::TRANSFORM_2D,
        4,
        ptr::null(),
        vertices.cast::<c_void>(),
    );
    PspStatus::Ok.as_i32()
}

unsafe extern "C" fn renderer_draw_solid(
    _ctx: *mut c_void,
    command: *const RawDrawSolidCommand,
) -> i32 {
    if command.is_null() {
        return PspStatus::InvalidArgument.as_i32();
    }
    let command = &*command;
    let status = set_blend(command.blend);
    if status != PspStatus::Ok.as_i32() {
        return status;
    }
    set_scissor(command.has_scissor != 0, command.scissor);
    sys::sceGuDisable(sys::GuState::Texture2D);
    let vertices = sys::sceGuGetMemory((4 * size_of::<PspVertex>()) as i32).cast::<PspVertex>();
    if vertices.is_null() {
        return PspStatus::OutOfMemory.as_i32();
    }
    let x0 = scale_x(command.rect.x as f32);
    let y0 = scale_y(command.rect.y as f32);
    let x1 = scale_x((command.rect.x + command.rect.width) as f32);
    let y1 = scale_y((command.rect.y + command.rect.height) as f32);
    let color = pack_color(command.color);
    let points = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)];
    for (index, (x, y)) in points.iter().copied().enumerate() {
        let dst = vertices.add(index);
        (*dst).u = 0.0;
        (*dst).v = 0.0;
        (*dst).color = color;
        (*dst).x = x;
        (*dst).y = y;
        (*dst).z = 0.0;
    }
    sys::sceGuDrawArray(
        sys::GuPrimitive::TriangleStrip,
        sys::VertexType::COLOR_8888
            | sys::VertexType::VERTEX_32BITF
            | sys::VertexType::TRANSFORM_2D,
        4,
        ptr::null(),
        vertices.cast::<c_void>(),
    );
    sys::sceGuEnable(sys::GuState::Texture2D);
    PspStatus::Ok.as_i32()
}

unsafe extern "C" fn renderer_end_frame(_ctx: *mut c_void) -> i32 {
    sys::sceGuFinish();
    sys::sceGuSync(sys::GuSyncMode::Finish, sys::GuSyncBehavior::Wait);
    PspStatus::Ok.as_i32()
}

unsafe extern "C" fn renderer_present(_ctx: *mut c_void) -> i32 {
    sys::sceDisplayWaitVblankStart();
    sys::sceGuSwapBuffers();
    PspStatus::Ok.as_i32()
}

unsafe extern "C" fn audio_unsupported_load(
    _ctx: *mut c_void,
    _stream_id: u32,
    _bytes: *const u8,
    _byte_len: usize,
) -> i32 {
    PspStatus::Unsupported.as_i32()
}

unsafe extern "C" fn audio_unsupported_play(
    _ctx: *mut c_void,
    _stream_id: u32,
    _params: RawAudioParams,
    _fade_in_ms: u32,
) -> i32 {
    PspStatus::Unsupported.as_i32()
}

unsafe extern "C" fn audio_unsupported_stop(
    _ctx: *mut c_void,
    _stream_id: u32,
    _fade_ms: u32,
) -> i32 {
    PspStatus::Unsupported.as_i32()
}

unsafe extern "C" fn audio_unsupported_slot(_ctx: *mut c_void, _stream_id: u32) -> i32 {
    PspStatus::Unsupported.as_i32()
}

unsafe extern "C" fn audio_unsupported_params(
    _ctx: *mut c_void,
    _stream_id: u32,
    _params: RawAudioParams,
) -> i32 {
    PspStatus::Unsupported.as_i32()
}

unsafe extern "C" fn audio_destroy(_ctx: *mut c_void, _stream_id: u32) {}

unsafe extern "C" fn audio_tick(_ctx: *mut c_void, _delta_us: u64) -> i32 {
    PspStatus::Ok.as_i32()
}

unsafe extern "C" fn clock_ticks_us(_ctx: *mut c_void) -> u64 {
    let mut now = 0_u64;
    if sys::sceRtcGetCurrentTick(&mut now) < 0 {
        return 0;
    }
    let elapsed = now.saturating_sub(START_TICK);
    elapsed.saturating_mul(1_000_000) / TICK_RESOLUTION as u64
}

unsafe extern "C" fn psp_log(
    _ctx: *mut c_void,
    _level: u32,
    message: *const u8,
    message_len: usize,
) {
    if message.is_null() {
        return;
    }
    let bytes = core::slice::from_raw_parts(message, message_len);
    for byte in bytes {
        psp::dprint!("{}", *byte as char);
    }
    psp::dprint!("\n");
}

unsafe extern "C" fn psp_fatal(
    _ctx: *mut c_void,
    code: u32,
    message: *const u8,
    message_len: usize,
) {
    psp::dprintln!("Fatal error {}", code);
    psp_log(ptr::null_mut(), 1, message, message_len);
}

unsafe fn upload_texture_pixels(
    slot: &mut TextureSlot,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    pixels: *const u8,
    pixels_len: usize,
    stride: usize,
    bpp: usize,
) -> i32 {
    if width == 0 || height == 0 || x + width > slot.width || y + height > slot.height {
        return PspStatus::InvalidArgument.as_i32();
    }
    let row_stride = if stride == 0 {
        width as usize * bpp
    } else {
        stride
    };
    if row_stride < width as usize * bpp {
        return PspStatus::InvalidArgument.as_i32();
    }
    if pixels_len < row_stride.saturating_mul(height as usize) {
        return PspStatus::InvalidArgument.as_i32();
    }
    for row in 0..height as usize {
        let src = pixels.add(row * row_stride);
        let dst = slot
            .pixels
            .add((y as usize + row) * slot.width as usize + x as usize);
        for col in 0..width as usize {
            *dst.add(col) = convert_pixel(slot.format, src.add(col * bpp));
        }
    }
    sys::sceKernelDcacheWritebackInvalidateRange(
        slot.pixels.cast::<c_void>(),
        slot.bytes_len as u32,
    );
    PspStatus::Ok.as_i32()
}

unsafe fn convert_pixel(format: RawPixelFormat, pixel: *const u8) -> u32 {
    match format {
        RawPixelFormat::Rgba8 => sys::rgba(*pixel, *pixel.add(1), *pixel.add(2), *pixel.add(3)),
        RawPixelFormat::Bgra8 => sys::rgba(*pixel.add(2), *pixel.add(1), *pixel, *pixel.add(3)),
        RawPixelFormat::LumaA8 => sys::rgba(255, 255, 255, *pixel.add(1)),
        _ => 0,
    }
}

fn texture_bpp(format: RawPixelFormat) -> Option<usize> {
    match format {
        RawPixelFormat::Rgba8 | RawPixelFormat::Bgra8 => Some(4),
        RawPixelFormat::LumaA8 => Some(2),
        _ => None,
    }
}

unsafe fn free_texture(slot: &mut TextureSlot) {
    if slot.pixels.is_null() {
        return;
    }
    if let Ok(layout) = Layout::from_size_align(slot.bytes_len, 16) {
        dealloc(slot.pixels.cast::<u8>(), layout);
    }
    *slot = TextureSlot::empty();
}

unsafe fn texture_slot_mut(texture_id: u32) -> Option<&'static mut TextureSlot> {
    if texture_id as usize >= MAX_TEXTURES {
        None
    } else {
        Some(&mut TEXTURES[texture_id as usize])
    }
}

unsafe fn texture_slot(texture_id: u32) -> Option<TextureSlot> {
    if texture_id as usize >= MAX_TEXTURES {
        None
    } else {
        Some(TEXTURES[texture_id as usize])
    }
}

unsafe fn set_blend(blend: RawBlendMode) -> i32 {
    match blend {
        RawBlendMode::Opaque => {
            sys::sceGuDisable(sys::GuState::Blend);
            PspStatus::Ok.as_i32()
        }
        RawBlendMode::Alpha => {
            sys::sceGuEnable(sys::GuState::Blend);
            sys::sceGuBlendFunc(
                sys::BlendOp::Add,
                sys::BlendFactor::SrcAlpha,
                sys::BlendFactor::OneMinusSrcAlpha,
                0,
                0,
            );
            PspStatus::Ok.as_i32()
        }
        RawBlendMode::Add => {
            sys::sceGuEnable(sys::GuState::Blend);
            sys::sceGuBlendFunc(
                sys::BlendOp::Add,
                sys::BlendFactor::SrcAlpha,
                sys::BlendFactor::Fix,
                0,
                0x00ff_ffff,
            );
            PspStatus::Ok.as_i32()
        }
        RawBlendMode::Multiply | RawBlendMode::Screen => PspStatus::Unsupported.as_i32(),
    }
}

unsafe fn set_scissor(has_scissor: bool, rect: RawRectI32) {
    if has_scissor {
        let x0 = scale_x(rect.x as f32).clamp(0.0, SCREEN_W as f32) as i32;
        let y0 = scale_y(rect.y as f32).clamp(0.0, SCREEN_H as f32) as i32;
        let x1 = scale_x((rect.x + rect.width) as f32).clamp(0.0, SCREEN_W as f32) as i32;
        let y1 = scale_y((rect.y + rect.height) as f32).clamp(0.0, SCREEN_H as f32) as i32;
        sys::sceGuScissor(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0));
    } else {
        sys::sceGuScissor(0, 0, SCREEN_W as i32, SCREEN_H as i32);
    }
}

fn scale_x(x: f32) -> f32 {
    unsafe { x * SCREEN_W as f32 / FRAME_W.max(1) as f32 }
}

fn scale_y(y: f32) -> f32 {
    unsafe { y * SCREEN_H as f32 / FRAME_H.max(1) as f32 }
}

fn pack_color(color: RawColorRgba) -> u32 {
    sys::rgba(
        unit_to_u8(color.r),
        unit_to_u8(color.g),
        unit_to_u8(color.b),
        unit_to_u8(color.a),
    )
}

fn unit_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn next_pow2(mut value: u32) -> u32 {
    value = value.saturating_sub(1);
    value |= value >> 1;
    value |= value >> 2;
    value |= value >> 4;
    value |= value >> 8;
    value |= value >> 16;
    value.saturating_add(1)
}

fn frame_buffer_offset(offset: usize) -> *mut c_void {
    offset as *mut c_void
}

fn c_path(path: *const u8, path_len: usize) -> Option<Vec<u8>> {
    if path.is_null() || path_len >= MAX_PATH {
        return None;
    }
    let bytes = unsafe { core::slice::from_raw_parts(path, path_len) };
    if bytes.contains(&0) {
        return None;
    }
    let mut out = Vec::with_capacity(path_len + 1);
    out.extend_from_slice(bytes);
    out.push(0);
    Some(out)
}

fn stat_to_info(stat: sys::SceIoStat) -> RawFileInfo {
    let kind = if stat.st_mode.contains(sys::IoStatMode::IFDIR)
        || stat.st_attr.contains(sys::IoStatAttr::IFDIR)
    {
        RawFileKind::Directory
    } else if stat.st_mode.contains(sys::IoStatMode::IFREG)
        || stat.st_attr.contains(sys::IoStatAttr::IFREG)
    {
        RawFileKind::File
    } else {
        RawFileKind::Other
    };
    RawFileInfo {
        len: stat.st_size.max(0) as u64,
        kind,
    }
}

fn dirent_name(entry: &sys::SceIoDirent) -> &str {
    let len = entry
        .d_name
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(entry.d_name.len());
    core::str::from_utf8(&entry.d_name[..len]).unwrap_or("")
}

fn extension_matches(path: &str, extension: &str) -> bool {
    let Some(pos) = path.rfind('.') else {
        return false;
    };
    path[pos + 1..].eq_ignore_ascii_case(extension)
}
