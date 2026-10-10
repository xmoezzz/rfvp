//! The boundary with the C runtime (`platform/switch/runtime/main.c`): the services it offers
//! the engine, the parts of newlib the engine uses, and the entry points it calls.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use core::alloc::{GlobalAlloc, Layout};
use core::ffi::{c_char, c_int, c_long, c_void, CStr};
use core::panic::PanicInfo;

use rfvp::host_api::{RfvpFile, RfvpFileSystem, RfvpHost, WallTime};
use rfvp::script::parser::Nls;
use rfvp::{RfvpBootConfig, RfvpCoreConfig};

use crate::app::HorizonApp;
use crate::event::SwitchInput;

/// `struct RfvpSwitchWallTime` in main.c.
#[repr(C)]
#[derive(Default)]
pub struct CWallTime {
    year: u16,
    month: u8,
    day: u8,
    day_of_week: u8,
    hour: u8,
    minute: u8,
}

pub type DirVisitor = unsafe extern "C" fn(*mut c_void, *const c_char, bool, u64);

/// newlib's `FILE`.
pub enum CFile {}

pub const SEEK_SET: c_int = 0;
pub const SEEK_END: c_int = 2;

extern "C" {
    // main.c
    fn rfvp_switch_log(message: *const u8, len: usize);
    fn rfvp_switch_ticks_us() -> u64;
    fn rfvp_switch_wall_time(out: *mut CWallTime) -> bool;
    pub fn rfvp_switch_list_dir(path: *const c_char, visit: DirVisitor, ctx: *mut c_void) -> c_int;
    pub fn rfvp_switch_path_kind(path: *const c_char, size: *mut u64) -> c_int;

    // newlib
    pub fn fopen(path: *const c_char, mode: *const c_char) -> *mut CFile;
    pub fn fclose(file: *mut CFile) -> c_int;
    pub fn fread(buf: *mut c_void, size: usize, count: usize, file: *mut CFile) -> usize;
    pub fn fwrite(buf: *const c_void, size: usize, count: usize, file: *mut CFile) -> usize;
    pub fn fseek(file: *mut CFile, offset: c_long, whence: c_int) -> c_int;
    pub fn ftell(file: *mut CFile) -> c_long;
    pub fn remove(path: *const c_char) -> c_int;
    pub fn mkdir(path: *const c_char, mode: u32) -> c_int;
    fn memalign(align: usize, size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn abort() -> !;
}

pub fn log(message: &str) {
    unsafe { rfvp_switch_log(message.as_ptr(), message.len()) };
}

pub fn ticks_us() -> u64 {
    unsafe { rfvp_switch_ticks_us() }
}

pub fn wall_time() -> Option<WallTime> {
    let mut out = CWallTime::default();
    unsafe { rfvp_switch_wall_time(&mut out) }.then(|| WallTime {
        year: out.year,
        month: out.month,
        day: out.day,
        day_of_week: out.day_of_week,
        hour: out.hour,
        minute: out.minute,
    })
}

/// A path as a NUL-terminated string for C.
pub fn c_path(path: &str) -> Option<alloc::vec::Vec<u8>> {
    if path.as_bytes().contains(&0) {
        return None;
    }
    let mut bytes = alloc::vec::Vec::with_capacity(path.len() + 1);
    bytes.extend_from_slice(path.as_bytes());
    bytes.push(0);
    Some(bytes)
}

/// The heap is libnx's: newlib's allocator over all the memory the process may use.
struct NewlibAlloc;

unsafe impl GlobalAlloc for NewlibAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { memalign(layout.align().max(16), layout.size()) as *mut u8 }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        unsafe { free(ptr as *mut c_void) }
    }
}

#[cfg(target_os = "horizon")]
#[global_allocator]
static ALLOCATOR: NewlibAlloc = NewlibAlloc;

#[cfg(target_os = "horizon")]
#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    let message = format!("rfvp: panic: {info}\n");
    log(&message);
    write_file(ERROR_FILE, message.as_bytes());
    unsafe { abort() }
}

/// Optional file next to the game selecting the script encoding: `sjis`, `gbk` or `utf8`.
const NLS_FILE: &str = "sdmc:/switch/rfvp/nls.txt";
/// Written when the game cannot start or stops, so the reason can be read off the SD card.
const ERROR_FILE: &str = "sdmc:/switch/rfvp/last_error.txt";
/// If this file exists the port reports how it is doing in `DEBUG_REPORT_FILE`, about once a
/// second: for finding out why a game is slow or stuck on a console.
const DEBUG_FLAG_FILE: &str = "sdmc:/switch/rfvp/debug.txt";
const DEBUG_REPORT_FILE: &str = "sdmc:/switch/rfvp/debug_report.txt";
/// Frames between two reports.
const DEBUG_REPORT_EVERY: u32 = 60;
/// The debug report also saves the screen here, every `DEBUG_SCREEN_EVERY` frames.
const DEBUG_SCREEN_FILE: &str = "sdmc:/switch/rfvp/debug_screen.bmp";
const DEBUG_SCREEN_EVERY: u32 = 300;

fn write_file(path: &str, bytes: &[u8]) {
    let Some(path) = c_path(path) else { return };
    unsafe {
        let file = fopen(path.as_ptr() as *const c_char, c"wb".as_ptr());
        if !file.is_null() {
            fwrite(bytes.as_ptr() as *const c_void, 1, bytes.len(), file);
            fclose(file);
        }
    }
}

/// The running game and what the debug report needs between frames.
pub struct Session {
    app: HorizonApp,
    game_root: String,
    debug: bool,
    frames: u32,
    window_started_us: u64,
    last_frame_us: u64,
    worst_us: u64,
}

impl Session {
    fn report_error(&mut self, what: &str, err: Option<rfvp::host_api::RfvpError>) {
        let detail = self
            .app
            .core()
            .last_error_detail()
            .map(String::from)
            .unwrap_or_default();
        let text = format!(
            "{what}{}\ngame directory: {}\n{detail}\n\
             The game directory needs the game's .hcb script and .bin packs, plus a CJK font \
             named default.ttf.\n\nlog:\n{}",
            err.map(|err| format!(" ({err:?})")).unwrap_or_default(),
            self.game_root,
            rfvp::ring_log::dump()
        );
        log(&format!("rfvp: {what}\n"));
        write_file(ERROR_FILE, text.as_bytes());
    }

    fn debug_report(&mut self) {
        let now = ticks_us();
        self.worst_us = self.worst_us.max(now.saturating_sub(self.last_frame_us));
        self.last_frame_us = now;
        if self.frames % DEBUG_REPORT_EVERY != 0 {
            return;
        }
        let window_us = now.saturating_sub(self.window_started_us).max(1);
        let text = format!(
            "frame {}: {:.1} fps over the last {} frames, slowest frame {:.1} ms\n\
             virtual frame {}x{}\n\nlog:\n{}",
            self.app.core().frame_index(),
            DEBUG_REPORT_EVERY as f64 * 1_000_000.0 / window_us as f64,
            DEBUG_REPORT_EVERY,
            self.worst_us as f64 / 1000.0,
            self.app.core().config().virtual_width,
            self.app.core().config().virtual_height,
            rfvp::ring_log::dump()
        );
        write_file(DEBUG_REPORT_FILE, text.as_bytes());
        if self.frames % DEBUG_SCREEN_EVERY == 0 {
            write_file(DEBUG_SCREEN_FILE, &self.app.host().renderer_ref().screen_bmp());
        }
        self.window_started_us = now;
        self.worst_us = 0;
    }
}

fn read_nls(app: &mut HorizonApp) -> Nls {
    let Ok(mut file) = app.host().fs().open(NLS_FILE) else {
        return Nls::ShiftJIS;
    };
    let Ok(bytes) = file.read_to_vec(64) else {
        return Nls::ShiftJIS;
    };
    match core::str::from_utf8(&bytes).map(str::trim) {
        Ok("gbk") => Nls::GBK,
        Ok("utf8") => Nls::UTF8,
        _ => Nls::ShiftJIS,
    }
}

/// Load the game in `game_root`. Returns null if it cannot start; the reason is written to
/// `ERROR_FILE`.
#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_start(game_root: *const c_char) -> *mut Session {
    rfvp::ring_log::install();
    let game_root = unsafe { CStr::from_ptr(game_root) }
        .to_str()
        .unwrap_or("sdmc:/switch/rfvp/game");
    let mut app = HorizonApp::new(RfvpCoreConfig::default(), 256, game_root);
    let nls = read_nls(&mut app);
    let debug = app.host().fs().exists(DEBUG_FLAG_FILE);
    let now = ticks_us();
    let mut session = Box::new(Session {
        app,
        game_root: String::from(game_root),
        debug,
        frames: 0,
        window_started_us: now,
        last_frame_us: now,
        worst_us: 0,
    });
    log("rfvp: booting the game\n");
    let boot = RfvpBootConfig {
        asset_root: game_root,
        nls,
        ..RfvpBootConfig::default()
    };
    if let Err(err) = session.app.boot(boot) {
        session.report_error("the game could not be loaded", Some(err));
        return core::ptr::null_mut();
    }
    log("rfvp: the game is loaded, running\n");
    Box::into_raw(session)
}

/// Run one frame and draw it into `framebuffer` (RGBA rows, `stride_bytes` apart). Returns 0 to
/// go on, 1 when the game wants to quit and -1 when it stopped with an error.
#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_frame(
    session: *mut Session,
    input: *const SwitchInput,
    framebuffer: *mut u32,
    stride_bytes: u32,
) -> c_int {
    let session = unsafe { &mut *session };
    let input = unsafe { &*input };
    session
        .app
        .host()
        .renderer()
        .set_target(framebuffer, stride_bytes as usize / 4);
    let result = session.app.run_frame(input);
    session.app.host().renderer().set_target(core::ptr::null_mut(), 0);
    if let Err(err) = result {
        session.report_error("the game stopped with an error", Some(err));
        return -1;
    }
    if session.app.quit_requested() {
        return 1;
    }
    session.frames += 1;
    if session.frames == 1 {
        log("rfvp: the first frame was drawn\n");
    }
    if session.debug {
        session.debug_report();
    }
    0
}

/// Mix the next `frames` frames of 48 kHz stereo into `dst`.
#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_audio_render(session: *mut Session, dst: *mut i16, frames: usize) {
    let session = unsafe { &mut *session };
    let out = unsafe { core::slice::from_raw_parts_mut(dst, frames * 2) };
    session.app.host().audio().render(out);
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_switch_stop(session: *mut Session) {
    if !session.is_null() {
        drop(unsafe { Box::from_raw(session) });
    }
}

