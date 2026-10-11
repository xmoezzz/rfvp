use alloc::alloc::{alloc, dealloc, Layout};
use alloc::collections::BTreeMap;
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
    /// Size the GE is told (powers of two; it samples no further than the stored rows).
    width: u32,
    height: u32,
    /// Pixels from one row to the next in memory, and the rows kept: only the picture and one
    /// row and column of transparent edge, not the whole power-of-two square.
    stride: u32,
    rows: u32,
    /// Size the engine sees; a texture bigger than `MAX_TEX_SIZE` is stored halved `shift` times.
    logical_w: u32,
    logical_h: u32,
    shift: u32,
    format: RawPixelFormat,
    pixels: *mut u32,
    bytes_len: usize,
}

impl TextureSlot {
    const fn empty() -> Self {
        Self {
            width: 0,
            height: 0,
            stride: 0,
            rows: 0,
            logical_w: 0,
            logical_h: 0,
            shift: 0,
            format: RawPixelFormat::Rgba8,
            pixels: ptr::null_mut(),
            bytes_len: 0,
        }
    }

    fn is_live(self) -> bool {
        !self.pixels.is_null()
    }
}

// Keyed by texture id: graph ids go up to 4095 and the white texture is `u32::MAX`.
static mut TEXTURES: BTreeMap<u32, TextureSlot> = BTreeMap::new();
static mut GU_LIST: psp::Align16<[u32; LIST_WORDS]> = psp::Align16([0; LIST_WORDS]);
static mut RENDERER_INITIALIZED: bool = false;
static mut FRAME_W: u32 = SCREEN_W;
static mut FRAME_H: u32 = SCREEN_H;
static mut INPUT: crate::input::PspInput = crate::input::PspInput::new_const();
/// The last controller reading (stick x, stick y, buttons), for the debug report.
static mut LAST_STICK: (u8, u8, u32) = (0, 0, 0);
/// Every button pressed since boot, and the frame of the first press, for the debug report.
static mut BUTTONS_SEEN: u32 = 0;
static mut FIRST_BUTTON_FRAME: u32 = u32::MAX;
static mut SHOULD_EXIT: bool = false;
static mut TICK_RESOLUTION: u32 = 1_000_000;
static mut START_TICK: u64 = 0;
// Debug counters for the report: sprites drawn, sprites skipped for a missing texture, and
// textures that could not be created (with the last failure's id, size and status).
static mut DRAWN: u32 = 0;
static mut SPRITE_LOG: String = String::new();
static mut SKIPPED: u32 = 0;
static mut CREATE_FAILED: u32 = 0;
static mut LAST_CREATE_FAIL: (u32, u32, u32, i32) = (0, 0, 0, 0);

pub fn run() -> i32 {
    rfvp::ring_log::install();
    unsafe {
        // Run at the PSP's top speed; it boots at 222 MHz.
        sys::scePowerSetClockFrequency(333, 333, 166);
        let init_status = platform_init();
        if init_status != PspStatus::Ok.as_i32() {
            return init_status;
        }

        let raw_host = make_raw_host();
        let mut app = PspApp::new(raw_host, RfvpCoreConfig::default(), 128, SCREEN_W, SCREEN_H);
        if let Err(err) = app.boot_old_school(RfvpBootConfig {
            max_hcb_bytes: 2 * 1024 * 1024,
            max_manifest_entries: 128,
            // 512 KiB of pages: a quarter of a typical script, enough that jumps rarely read.
            hcb_cache_bytes: 512 * 1024,
            ..RfvpBootConfig::default()
        }) {
            report_error(&app, "the game could not be loaded", err);
            platform_fini();
            return rfvp_error_to_status(err);
        }

        let debug = file_exists(b"debug.txt\0");
        if debug {
            start_watchdog();
            // `trace=syscall,motion` in debug.txt turns on those trace categories (into the log).
            let options = read_small_file(b"debug.txt\0");
            if let Some(list) = options.lines().find_map(|line| line.trim().strip_prefix("trace=")) {
                rfvp::trace::enable(list);
            }
            // `click=X,Y@FRAME` (repeatable): a left click at screen pixel (X, Y) on that frame,
            // to drive the game in an emulator without touching the controller.
            let clicks = &mut *core::ptr::addr_of_mut!(DEBUG_CLICKS);
            for line in options.lines() {
                if let Some(click) = line.trim().strip_prefix("click=").and_then(parse_click) {
                    clicks.push(click);
                }
            }
            rfvp::perf::set_clock(|| unsafe { sys::sceKernelGetSystemTimeWide() as u64 });
        }
        let mut frames = 0u32;
        let mut window_start = clock_ticks_us(ptr::null_mut());
        let mut last_frame = window_start;
        let mut worst = 0u64;
        let (mut vm_us, mut scene_us, mut render_us) = (0u64, 0u64, 0u64);
        const REPORT_EVERY: u32 = 10;

        loop {
            let poll_status = platform_poll(&mut app);
            if poll_status != PspStatus::Ok.as_i32() {
                platform_fini();
                return poll_status;
            }
            match app.run_frame() {
                Ok(tick) => {
                    vm_us += tick.vm_us;
                    scene_us += tick.scene_us;
                    render_us += tick.render_us;
                }
                Err(err) => {
                    report_error(&app, "the game stopped with an error", err);
                    platform_fini();
                    return rfvp_error_to_status(err);
                }
            }
            if app.quit_requested() || SHOULD_EXIT {
                break;
            }
            frames += 1;
            WATCHDOG_FRAMES.store(frames, core::sync::atomic::Ordering::Relaxed);
            if debug {
                crate::heap::note_peak();
                let now = clock_ticks_us(ptr::null_mut());
                worst = worst.max(now.saturating_sub(last_frame));
                last_frame = now;
                if frames % REPORT_EVERY == 0 {
                    let (used, peak, heap) = crate::heap::usage();
                    let window = now.saturating_sub(window_start).max(1);
                    let per = |us: u64| us as f64 / 1000.0 / REPORT_EVERY as f64;
                    let text = alloc::format!(
                        "frame {frames}: {:.1} fps over the last {REPORT_EVERY} frames, slowest frame {:.1} ms\n\
                         per frame: {:.1} ms scripts, {:.1} ms motions and text, {:.1} ms drawing\n\
                         stages (ms per frame):{}\n\
                         sprites per frame: {} drawn, {} skipped; textures failed: {} (last {:?})\n\
                         controller: stick {:?}, cursor {:?} shown {}, buttons seen {:#x} from frame {}\n\
                         heap: {} KiB used, {} KiB at most, of {} KiB\n\nlog:\n{}",
                        REPORT_EVERY as f64 * 1_000_000.0 / window as f64,
                        worst as f64 / 1000.0,
                        per(vm_us),
                        per(scene_us),
                        per(render_us),
                        stage_report(REPORT_EVERY),
                        DRAWN / REPORT_EVERY,
                        SKIPPED / REPORT_EVERY,
                        CREATE_FAILED,
                        LAST_CREATE_FAIL,
                        LAST_STICK,
                        (*core::ptr::addr_of!(INPUT)).cursor(),
                        (*core::ptr::addr_of!(INPUT)).cursor_shown(),
                        BUTTONS_SEEN,
                        FIRST_BUTTON_FRAME,
                        used / 1024,
                        peak / 1024,
                        heap / 1024,
                        rfvp::ring_log::dump()
                    );
                    write_file("debug_report.txt", text.as_bytes());
                    write_file("debug_sprites.txt", (*core::ptr::addr_of!(SPRITE_LOG)).as_bytes());
                    write_file("debug_threads.txt", app.core().debug_threads().as_bytes());
                    vm_us = 0;
                    DRAWN = 0;
                    SKIPPED = 0;
                    scene_us = 0;
                    render_us = 0;
                    if frames % 100 == 0 {
                        let name = alloc::format!("debug_screen_{:03}.bmp", frames / 100);
                        write_file(&name, &screen_bmp());
                    }
                    window_start = now;
                    worst = 0;
                }
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

/// Debug clicks from debug.txt: (x, y, frame).
static mut DEBUG_CLICKS: Vec<(i32, i32, u32)> = Vec::new();
static mut POLL_FRAME: u32 = 0;

fn parse_click(text: &str) -> Option<(i32, i32, u32)> {
    let (at, frame) = text.split_once('@')?;
    let (x, y) = at.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?, frame.trim().parse().ok()?))
}

/// Send the debug clicks due this frame: the cursor moves there ten frames before, the button
/// goes down on the frame and up three frames later.
unsafe fn push_debug_clicks(app: &mut PspApp) -> Result<(), rfvp::host_api::RfvpError> {
    let frame = POLL_FRAME;
    POLL_FRAME += 1;
    for &(x, y, at) in (*core::ptr::addr_of!(DEBUG_CLICKS)).iter() {
        if frame + 10 == at {
            app.push_pointer_move_physical(x, y)?;
        } else if frame == at {
            app.push_pointer_down_physical(rfvp::host_api::PointerButton::Left, x, y)?;
        } else if frame == at + 3 {
            app.push_pointer_up_physical(rfvp::host_api::PointerButton::Left, x, y)?;
        }
    }
    Ok(())
}

unsafe fn platform_poll(app: &mut PspApp) -> i32 {
    let mut data = MaybeUninit::<sys::SceCtrlData>::zeroed();
    let status = sys::sceCtrlReadBufferPositive(data.as_mut_ptr(), 1);
    if status < 0 {
        return PspStatus::Backend.as_i32();
    }
    let data = data.assume_init();
    if let Err(err) = push_debug_clicks(app) {
        return rfvp_error_to_status(err);
    }
    let input = &mut *core::ptr::addr_of_mut!(INPUT);
    let mut events = Vec::new();
    input.poll(data.buttons.bits(), data.lx, data.ly, &mut events);
    LAST_STICK = (data.lx, data.ly, data.buttons.bits());
    if data.buttons.bits() != 0 {
        BUTTONS_SEEN |= data.buttons.bits();
        if FIRST_BUTTON_FRAME == u32::MAX {
            FIRST_BUTTON_FRAME = POLL_FRAME;
        }
    }
    for event in events {
        // Pointer positions are in screen pixels; the game wants its own coordinates.
        let pushed = match event {
            rfvp::host_api::RfvpEvent::PointerMove { x, y, .. } => {
                app.push_pointer_move_physical(x, y)
            }
            rfvp::host_api::RfvpEvent::PointerDown { button, x, y } => {
                app.push_pointer_down_physical(button, x, y)
            }
            rfvp::host_api::RfvpEvent::PointerUp { button, x, y } => {
                app.push_pointer_up_physical(button, x, y)
            }
            other => app.push_event(other),
        };
        if let Err(err) = pushed {
            return rfvp_error_to_status(err);
        }
    }
    PspStatus::Ok.as_i32()
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
    make_parent_dirs(&path);
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

/// Create every directory on the way to the NUL-terminated `path` (the file itself is not
/// touched). Directories that already exist are fine.
unsafe fn make_parent_dirs(path: &[u8]) {
    let mut dir = path.to_vec();
    for i in 1..path.len().saturating_sub(1) {
        if path[i] == b'/' && path[i - 1] != b':' && path[i - 1] != b'/' {
            dir[i] = 0;
            let _ = sys::sceIoMkdir(dir.as_ptr(), 0o777);
            dir[i] = b'/';
        }
    }
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

/// Names of the files directly in `dir` (relative to the working directory, "" for itself).
pub fn list_files(dir: &str) -> Result<Vec<String>, i32> {
    let dir = if dir.is_empty() { "." } else { dir.trim_end_matches('/') };
    let Some(path) = c_path(dir.as_ptr(), dir.len()) else {
        return Err(PspStatus::InvalidArgument.as_i32());
    };
    let mut names = Vec::new();
    unsafe {
        let handle = sys::sceIoDopen(path.as_ptr());
        if handle.0 < 0 {
            return Err(PspStatus::NotFound.as_i32());
        }
        loop {
            let mut entry = MaybeUninit::<sys::SceIoDirent>::zeroed();
            if sys::sceIoDread(handle, entry.as_mut_ptr()) <= 0 {
                break;
            }
            let entry = entry.assume_init();
            if stat_to_info(entry.d_stat).kind == RawFileKind::File {
                names.push(String::from(dirent_name(&entry)));
            }
        }
        let _ = sys::sceIoDclose(handle);
    }
    Ok(names)
}

/// Written next to the EBOOT when the game cannot start or stops, so the reason can be read off
/// the memory stick.
unsafe fn report_error(app: &PspApp, what: &str, err: rfvp::host_api::RfvpError) {
    let text = alloc::format!(
        "{what} ({err:?})\n{}\n\nlog:\n{}",
        app.core().last_error_detail().unwrap_or(""),
        rfvp::ring_log::dump()
    );
    write_file("last_error.txt", text.as_bytes());
}

/// A small file's text, or empty when it is missing.
unsafe fn read_small_file(path: &[u8]) -> String {
    let fd = sys::sceIoOpen(path.as_ptr(), sys::IoOpenFlags::RD_ONLY, 0);
    if fd.0 < 0 {
        return String::new();
    }
    let mut buf = [0u8; 256];
    let n = sys::sceIoRead(fd, buf.as_mut_ptr().cast(), buf.len() as u32);
    sys::sceIoClose(fd);
    String::from_utf8_lossy(&buf[..n.max(0) as usize]).into_owned()
}

unsafe fn file_exists(path: &[u8]) -> bool {
    let mut stat = MaybeUninit::<sys::SceIoStat>::zeroed();
    sys::sceIoGetstat(path.as_ptr(), stat.as_mut_ptr()) >= 0
}

unsafe fn write_file(path: &str, bytes: &[u8]) {
    let _ = fs_write_all(ptr::null_mut(), path.as_ptr(), path.len(), bytes.as_ptr(), bytes.len());
}

/// The frame on screen as a 32-bit BMP, for the debug report.
static WATCHDOG_FRAMES: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Debug: a thread that writes `stall.txt` (where the core is, without allocating) when no
/// frame has finished for three seconds.
unsafe fn start_watchdog() {
    unsafe extern "C" fn watchdog(_args: usize, _argp: *mut c_void) -> i32 {
        let mut last = u32::MAX;
        let mut still = 0;
        loop {
            sys::sceKernelDelayThread(500_000);
            let now = WATCHDOG_FRAMES.load(core::sync::atomic::Ordering::Relaxed);
            if now != last {
                last = now;
                still = 0;
                continue;
            }
            still += 1;
            if still == 6 {
                let mut buf = [0u8; 128];
                let mut len = 0;
                for (label, value) in [
                    ("stalled at frame ", now),
                    (", core mark ", rfvp::perf::last_mark()),
                ] {
                    for b in label.bytes() {
                        buf[len] = b;
                        len += 1;
                    }
                    let mut digits = [0u8; 10];
                    let (mut n, mut count) = (value, 0);
                    loop {
                        digits[count] = b'0' + (n % 10) as u8;
                        count += 1;
                        n /= 10;
                        if n == 0 {
                            break;
                        }
                    }
                    while count > 0 {
                        count -= 1;
                        buf[len] = digits[count];
                        len += 1;
                    }
                }
                let fd = sys::sceIoOpen(
                    b"stall.txt\0".as_ptr(),
                    sys::IoOpenFlags::WR_ONLY | sys::IoOpenFlags::CREAT | sys::IoOpenFlags::TRUNC,
                    0o777,
                );
                if fd.0 >= 0 {
                    sys::sceIoWrite(fd, buf.as_ptr().cast(), len);
                    sys::sceIoClose(fd);
                }
            }
        }
    }
    let id = sys::sceKernelCreateThread(
        b"rfvp_watchdog\0".as_ptr(),
        watchdog,
        8,
        8 * 1024,
        sys::ThreadAttributes::USER,
        ptr::null_mut(),
    );
    if id.0 >= 0 {
        sys::sceKernelStartThread(id, 0, ptr::null_mut());
    }
}

fn stage_report(frames: u32) -> String {
    let mut text = String::new();
    for (name, us) in rfvp::perf::STAGES.iter().zip(rfvp::perf::take()) {
        if us >= frames * 50 {
            text += &alloc::format!(" {name} {:.1}", us as f64 / 1000.0 / frames as f64);
        }
    }
    text
}

unsafe fn screen_bmp() -> Vec<u8> {
    let mut top: *mut c_void = ptr::null_mut();
    let mut width = 0usize;
    let mut format = sys::DisplayPixelFormat::Psm8888;
    sys::sceDisplayGetFrameBuf(&mut top, &mut width, &mut format, sys::DisplaySetBufSync::Immediate);
    let (w, h) = (SCREEN_W as usize, SCREEN_H as usize);
    let mut bmp = Vec::with_capacity(54 + w * h * 4);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&((54 + w * h * 4) as u32).to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&(w as i32).to_le_bytes());
    bmp.extend_from_slice(&(-(h as i32)).to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&32u16.to_le_bytes());
    bmp.extend_from_slice(&[0; 24]);
    // Read through the uncached mirror: the GPU wrote it.
    let base = ((top as usize) | 0x4000_0000) as *const u32;
    for y in 0..h {
        for x in 0..w {
            let p = ptr::read_volatile(base.add(y * width.max(w) + x));
            let [r, g, b, _] = p.to_le_bytes();
            bmp.extend_from_slice(&[b, g, r, 255]);
        }
    }
    bmp
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
    let textures = &mut *core::ptr::addr_of_mut!(TEXTURES);
    for slot in textures.values_mut() {
        free_texture(slot);
    }
    textures.clear();
    if RENDERER_INITIALIZED {
        sys::sceGuDisplay(false);
        sys::sceGuTerm();
        RENDERER_INITIALIZED = false;
    }
}

unsafe extern "C" fn renderer_create_texture(
    ctx: *mut c_void,
    texture_id: u32,
    desc: RawTextureDesc,
    pixels: *const u8,
    pixels_len: usize,
    stride: usize,
) -> i32 {
    let status = create_texture_inner(ctx, texture_id, desc, pixels, pixels_len, stride);
    if status != PspStatus::Ok.as_i32() {
        CREATE_FAILED += 1;
        LAST_CREATE_FAIL = (texture_id, desc.width, desc.height, status);
    }
    status
}

unsafe fn create_texture_inner(
    _ctx: *mut c_void,
    texture_id: u32,
    desc: RawTextureDesc,
    pixels: *const u8,
    pixels_len: usize,
    stride: usize,
) -> i32 {
    let slot = (*core::ptr::addr_of_mut!(TEXTURES))
        .entry(texture_id)
        .or_insert(TextureSlot::empty());
    if desc.width == 0 || desc.height == 0 || desc.mip_count > 1 {
        return PspStatus::Unsupported.as_i32();
    }
    let Some(bpp) = texture_bpp(desc.format) else {
        return PspStatus::Unsupported.as_i32();
    };
    // The GE samples at most 512x512: store bigger pictures halved until they fit. Texture
    // coordinates are normalized, so drawing does not change.
    let mut shift = 0;
    while next_pow2(desc.width >> shift) > MAX_TEX_SIZE || next_pow2(desc.height >> shift) > MAX_TEX_SIZE {
        shift += 1;
    }
    let stored_w = (desc.width >> shift).max(1);
    let stored_h = (desc.height >> shift).max(1);
    let width = next_pow2(stored_w);
    let height = next_pow2(stored_h);
    // Room for one transparent column and row past the picture, which filtering at its edge
    // reads; the GE wants rows of a multiple of 4 pixels.
    let row_pixels = ((stored_w + 1).min(width) + 3) & !3;
    let rows = (stored_h + 1).min(height);
    free_texture(slot);
    let bytes_len = row_pixels as usize * rows as usize * 4;
    let Ok(layout) = Layout::from_size_align(bytes_len, 16) else {
        return PspStatus::OutOfMemory.as_i32();
    };
    let data = alloc(layout).cast::<u32>();
    if data.is_null() {
        return PspStatus::OutOfMemory.as_i32();
    }
    ptr::write_bytes(data, 0, row_pixels as usize * rows as usize);
    *slot = TextureSlot {
        width,
        height,
        stride: row_pixels,
        rows,
        logical_w: desc.width,
        logical_h: desc.height,
        shift,
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
    if let Some(mut slot) = (*core::ptr::addr_of_mut!(TEXTURES)).remove(&texture_id) {
        free_texture(&mut slot);
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
    (*core::ptr::addr_of_mut!(SPRITE_LOG)).clear();
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
    // A texture that failed to load (out of memory) is skipped rather than stopping the game.
    let Some(texture) = texture_slot(command.texture_id).filter(|t| t.is_live()) else {
        SKIPPED += 1;
        return PspStatus::Ok.as_i32();
    };
    DRAWN += 1;
    {
        let v = &command.vertices;
        let log = &mut *core::ptr::addr_of_mut!(SPRITE_LOG);
        if log.len() < 4000 {
            *log += &alloc::format!(
                "tex {} {}x{} at ({:.0},{:.0})-({:.0},{:.0}) uv ({:.2},{:.2})-({:.2},{:.2}) rgba {:?} {:?}\n",
                command.texture_id, texture.logical_w, texture.logical_h,
                v[0].position[0], v[0].position[1], v[3].position[0], v[3].position[1],
                v[0].tex_coord[0], v[0].tex_coord[1], v[3].tex_coord[0], v[3].tex_coord[1],
                v[0].color, command.blend,
            );
        }
    }
    let u_scale = texture.logical_w as f32 / (1u32 << texture.shift) as f32;
    let v_scale = texture.logical_h as f32 / (1u32 << texture.shift) as f32;
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
        texture.stride as i32,
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
        (*dst).u = src.tex_coord[0] * u_scale;
        (*dst).v = src.tex_coord[1] * v_scale;
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
    // Color and position only: the vertex type below has no texture coordinates.
    let vertices =
        sys::sceGuGetMemory((4 * size_of::<PspColorVertex>()) as i32).cast::<PspColorVertex>();
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
        *vertices.add(index) = PspColorVertex {
            color,
            x,
            y,
            z: 0.0,
        };
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
    let input = &*core::ptr::addr_of!(INPUT);
    if input.cursor_shown() {
        draw_cursor(input.cursor());
    }
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
    if width == 0 || height == 0 || x + width > slot.logical_w || y + height > slot.logical_h {
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
    if slot.shift == 0 {
        for row in 0..height as usize {
            let src = pixels.add(row * row_stride);
            let dst = slot
                .pixels
                .add((y as usize + row) * slot.stride as usize + x as usize);
            for col in 0..width as usize {
                *dst.add(col) = convert_pixel(slot.format, src.add(col * bpp));
            }
        }
    } else {
        // Point-sample the stored texels covered by the rectangle.
        let s = slot.shift;
        let stored_w = (slot.logical_w >> s).max(1).min(slot.stride);
        let stored_h = (slot.logical_h >> s).max(1).min(slot.rows);
        let dx0 = x >> s;
        let dy0 = y >> s;
        let dx1 = ((x + width + (1 << s) - 1) >> s).min(stored_w);
        let dy1 = ((y + height + (1 << s) - 1) >> s).min(stored_h);
        for dy in dy0..dy1 {
            let sy = ((dy << s).max(y) - y).min(height - 1) as usize;
            let src = pixels.add(sy * row_stride);
            let dst = slot.pixels.add(dy as usize * slot.stride as usize);
            for dx in dx0..dx1 {
                let sx = ((dx << s).max(x) - x).min(width - 1) as usize;
                *dst.add(dx as usize) = convert_pixel(slot.format, src.add(sx * bpp));
            }
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
    (*core::ptr::addr_of_mut!(TEXTURES)).get_mut(&texture_id)
}

unsafe fn texture_slot(texture_id: u32) -> Option<TextureSlot> {
    (*core::ptr::addr_of!(TEXTURES)).get(&texture_id).copied()
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
        // The GE has no alpha-aware multiply or screen; these ignore the source alpha.
        RawBlendMode::Multiply => {
            sys::sceGuEnable(sys::GuState::Blend);
            sys::sceGuBlendFunc(
                sys::BlendOp::Add,
                sys::BlendFactor::Color,
                sys::BlendFactor::Fix,
                0,
                0,
            );
            PspStatus::Ok.as_i32()
        }
        RawBlendMode::Screen => {
            sys::sceGuEnable(sys::GuState::Blend);
            sys::sceGuBlendFunc(
                sys::BlendOp::Add,
                sys::BlendFactor::OneMinusColor,
                sys::BlendFactor::Fix,
                0,
                0x00ff_ffff,
            );
            PspStatus::Ok.as_i32()
        }
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

#[repr(C)]
struct PspColorVertex {
    color: u32,
    x: f32,
    y: f32,
    z: f32,
}

/// The stick's cursor, over the frame: the engine draws none on consoles. An arrow, white with
/// a black edge, its tip at `(x, y)` in screen pixels.
unsafe fn draw_cursor((x, y): (i32, i32)) {
    let (x, y) = (x as f32, y as f32);
    let triangles = [
        (sys::rgba(0, 0, 0, 255), [(x - 1.0, y - 2.0), (x - 1.0, y + 17.0), (x + 13.0, y + 12.0)]),
        (sys::rgba(255, 255, 255, 255), [(x, y), (x, y + 14.0), (x + 10.0, y + 10.0)]),
    ];
    let vertices =
        sys::sceGuGetMemory((6 * size_of::<PspColorVertex>()) as i32).cast::<PspColorVertex>();
    if vertices.is_null() {
        return;
    }
    let mut index = 0;
    for (color, points) in triangles {
        for (px, py) in points {
            *vertices.add(index) = PspColorVertex {
                color,
                x: px,
                y: py,
                z: 0.0,
            };
            index += 1;
        }
    }
    sys::sceGuDisable(sys::GuState::Texture2D);
    sys::sceGuDisable(sys::GuState::Blend);
    sys::sceGuScissor(0, 0, SCREEN_W as i32, SCREEN_H as i32);
    sys::sceGuDrawArray(
        sys::GuPrimitive::Triangles,
        sys::VertexType::COLOR_8888 | sys::VertexType::VERTEX_32BITF | sys::VertexType::TRANSFORM_2D,
        6,
        ptr::null(),
        vertices.cast::<c_void>(),
    );
    sys::sceGuEnable(sys::GuState::Texture2D);
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
