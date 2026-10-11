//! The boundary with the C runtime (`platform/ps3/runtime/main.c`): the services it offers the
//! engine, the parts of newlib the engine uses, and the entry points it calls.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use core::alloc::{GlobalAlloc, Layout};
use core::ffi::{c_char, c_int, c_long, c_void, CStr};
use core::panic::PanicInfo;

use rfvp::host_api::{RfvpFile, RfvpFileSystem, RfvpHost, WallTime};
use rfvp::script::parser::Nls;
use rfvp::{RfvpBootConfig, RfvpCoreConfig};

use crate::app::Ps3App;
use crate::event::Ps3Input;

/// `struct RfvpPs3WallTime` in main.c.
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
    fn rfvp_ps3_log(message: *const u8, len: usize);
    fn rfvp_ps3_ticks_us() -> u64;
    fn rfvp_ps3_wall_time(out: *mut CWallTime) -> bool;
    pub fn rfvp_ps3_list_dir(path: *const c_char, visit: DirVisitor, ctx: *mut c_void) -> c_int;
    pub fn rfvp_ps3_path_kind(path: *const c_char, size: *mut u64) -> c_int;
    pub fn rfvp_ps3_mkdir(path: *const c_char) -> c_int;

    // newlib
    pub fn fopen(path: *const c_char, mode: *const c_char) -> *mut CFile;
    pub fn fclose(file: *mut CFile) -> c_int;
    pub fn fread(buf: *mut c_void, size: usize, count: usize, file: *mut CFile) -> usize;
    pub fn fwrite(buf: *const c_void, size: usize, count: usize, file: *mut CFile) -> usize;
    pub fn fseek(file: *mut CFile, offset: c_long, whence: c_int) -> c_int;
    pub fn ftell(file: *mut CFile) -> c_long;
    pub fn remove(path: *const c_char) -> c_int;
    fn memalign(align: usize, size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn abort() -> !;
}

pub fn log(message: &str) {
    unsafe { rfvp_ps3_log(message.as_ptr(), message.len()) };
}

pub fn ticks_us() -> u64 {
    unsafe { rfvp_ps3_ticks_us() }
}

pub fn wall_time() -> Option<WallTime> {
    let mut out = CWallTime::default();
    unsafe { rfvp_ps3_wall_time(&mut out) }.then(|| WallTime {
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

/// The heap is newlib's.
struct NewlibAlloc;

unsafe impl GlobalAlloc for NewlibAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { memalign(layout.align().max(16), layout.size()) as *mut u8 }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        unsafe { free(ptr as *mut c_void) }
    }
}

#[cfg(target_arch = "powerpc64")]
#[global_allocator]
static ALLOCATOR: NewlibAlloc = NewlibAlloc;

/// Where the files written when something goes wrong go: the save directory (set by
/// `rfvp_ps3_start`; the engine runs on one thread).
static mut SAVE_ROOT: Option<String> = None;

fn game_file(name: &str) -> String {
    // SAFETY: only `rfvp_ps3_start` writes it, before anything reads it, on the one thread.
    match unsafe { (*core::ptr::addr_of!(SAVE_ROOT)).as_deref() } {
        Some(root) => format!("{root}/{name}"),
        None => String::from(name),
    }
}

#[cfg(target_arch = "powerpc64")]
#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    let message = format!("rfvp: panic: {info}\n");
    log(&message);
    write_file(&game_file(ERROR_FILE), message.as_bytes());
    unsafe { abort() }
}

/// Optional file in the game directory selecting the script encoding: `sjis`, `gbk` or `utf8`.
const NLS_FILE: &str = "nls.txt";
/// Written when the game cannot start or stops, so the reason can be read off the card.
const ERROR_FILE: &str = "last_error.txt";
/// If this file exists the port reports how it is doing in `DEBUG_REPORT_FILE`, about once a
/// second: for finding out why a game is slow or stuck on a console.
const DEBUG_FLAG_FILE: &str = "debug.txt";
const DEBUG_REPORT_FILE: &str = "debug_report.txt";
/// Frames between two reports.
const DEBUG_REPORT_EVERY: u32 = 60;

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

pub struct Session {
    app: Ps3App,
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
            "{what}{} (at core mark {})\n{detail}\n\
             The game directory needs a game repacked with `rfvp-rebuilder --platform psp` \
             (the .hcb script, rfvp.toml, defualt.tmap and the asset folders).\n\nlog:\n{}",
            err.map(|err| format!(" ({err:?})")).unwrap_or_default(),
            rfvp::perf::last_mark(),
            rfvp::ring_log::dump()
        );
        log(&format!("rfvp: {what}\n"));
        write_file(&game_file(ERROR_FILE), text.as_bytes());
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
             stages (ms per frame):{}\n\
             virtual frame {}x{}\n\nlog:\n{}",
            self.app.core().frame_index(),
            DEBUG_REPORT_EVERY as f64 * 1_000_000.0 / window_us as f64,
            DEBUG_REPORT_EVERY,
            self.worst_us as f64 / 1000.0,
            stage_report(DEBUG_REPORT_EVERY),
            self.app.core().config().virtual_width,
            self.app.core().config().virtual_height,
            rfvp::ring_log::dump()
        );
        write_file(&game_file(DEBUG_REPORT_FILE), text.as_bytes());
        // The first two lines also go to the console, to follow the rate over a whole run.
        let summary: alloc::vec::Vec<&str> = text.lines().take(2).collect();
        log(&format!("rfvp report: {}\n", summary.join(" | ")));
        self.window_started_us = now;
        self.worst_us = 0;
    }
}

/// The stages that took a measurable time, per frame (see `rfvp::perf`).
fn stage_report(frames: u32) -> String {
    let mut text = format!(" ops {}", rfvp::perf::take_ops() / frames.max(1));
    for (op, count, cycles) in rfvp::perf::take_op_cycles(6) {
        text += &format!(" op{op:02x}:{count}x{}", cycles / count.max(1));
    }
    for (name, us) in rfvp::perf::STAGES.iter().zip(rfvp::perf::take()) {
        if us >= frames * 50 {
            text += &format!(" {name} {:.1}", us as f64 / 1000.0 / frames as f64);
        }
    }
    text
}

fn read_nls(app: &mut Ps3App) -> Nls {
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

/// Load the game in `game_root` (`/dev_hdd0/game/RFVP00001/USRDIR/game`, or `game` next to
/// the ELF), saving in `save_root`. Returns null if it cannot start; the reason is
/// written to `last_error.txt` in `save_root`.
#[no_mangle]
pub unsafe extern "C" fn rfvp_ps3_start(game_root: *const c_char, save_root: *const c_char) -> *mut Session {
    rfvp::ring_log::install();
    let game_root = unsafe { CStr::from_ptr(game_root) }
        .to_str()
        .unwrap_or("/dev_hdd0/game/RFVP00001/USRDIR/game");
    let save_root = unsafe { CStr::from_ptr(save_root) }
        .to_str()
        .unwrap_or(game_root);
    unsafe { *core::ptr::addr_of_mut!(SAVE_ROOT) = Some(String::from(save_root)) };
    let mut app = Ps3App::new(RfvpCoreConfig::default(), 256, game_root, save_root);
    let nls = read_nls(&mut app);
    let debug = app.host().fs().exists(DEBUG_FLAG_FILE);
    if debug {
        rfvp::perf::set_clock(ticks_us);
        extern "C" {
            fn rfvp_ps3_cycles() -> u32;
        }
        extern "C" fn cycles() -> u32 {
            unsafe { rfvp_ps3_cycles() }
        }
        rfvp::perf::set_cycle_counter(cycles);
        // `trace=syscall,motion` in debug.txt turns those trace categories on (into the log).
        if let Ok(mut file) = app.host().fs().open(DEBUG_FLAG_FILE) {
            if let Ok(bytes) = file.read_to_vec(4096) {
                let text = String::from_utf8_lossy(&bytes);
                if let Some(list) = text.lines().find_map(|line| line.trim().strip_prefix("trace=")) {
                    rfvp::trace::enable(list);
                }
            }
        }
    }
    let now = ticks_us();
    let mut session = Box::new(Session {
        app,
        debug,
        frames: 0,
        window_started_us: now,
        last_frame_us: now,
        worst_us: 0,
    });
    log("rfvp: booting the game\n");
    let boot = RfvpBootConfig {
        nls,
        max_hcb_bytes: 16 * 1024 * 1024,
        // The whole script: the PS3 has 256 MiB.
        hcb_cache_bytes: 16 * 1024 * 1024,
        ..RfvpBootConfig::default()
    };
    if let Err(err) = session.app.boot(boot) {
        session.report_error("the game could not be loaded", Some(err));
        return core::ptr::null_mut();
    }
    log("rfvp: the game is loaded, running\n");
    Box::into_raw(session)
}

/// Run one frame and draw it (main.c shows it afterwards). Returns 0 to go on,
/// 1 when the game wants to quit and -1 when it stopped with an error.
#[no_mangle]
pub unsafe extern "C" fn rfvp_ps3_frame(session: *mut Session, input: *const Ps3Input) -> c_int {
    let session = unsafe { &mut *session };
    let input = unsafe { &*input };
    let result = session.app.run_frame(input);
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

/// The pointer cursor, in screen pixels (see event.rs); false when it is hidden
/// (the engine draws none).
#[no_mangle]
pub unsafe extern "C" fn rfvp_ps3_cursor(session: *mut Session, x: *mut i32, y: *mut i32) -> bool {
    let session = unsafe { &*session };
    match session.app.cursor() {
        Some((cx, cy)) => {
            unsafe {
                *x = cx;
                *y = cy;
            }
            true
        }
        None => false,
    }
}

/// Mix the next `frames` frames of 48 kHz stereo into `dst`.
#[no_mangle]
pub unsafe extern "C" fn rfvp_ps3_audio_render(session: *mut Session, dst: *mut i16, frames: usize) {
    let session = unsafe { &mut *session };
    let out = unsafe { core::slice::from_raw_parts_mut(dst, frames * 2) };
    session.app.host().audio().render(out);
}

/// The last place the core passed (`rfvp::perf::mark`), for the runtime's watchdog.
#[no_mangle]
pub extern "C" fn rfvp_ps3_last_mark() -> u32 {
    rfvp::perf::last_mark()
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_ps3_stop(session: *mut Session) {
    if !session.is_null() {
        drop(unsafe { Box::from_raw(session) });
    }
}
