//! The boundary with the C runtime (`platform/ps1/runtime/main.c`): the services it offers the
//! engine and the entry points it calls.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use core::alloc::{GlobalAlloc, Layout};
use core::ffi::{c_char, c_int, c_void};
use core::panic::PanicInfo;

use rfvp::script::parser::Nls;
use rfvp::{RfvpBootConfig, RfvpCoreConfig};

use crate::app::Ps1App;
use crate::event::Ps1Input;

extern "C" {
    // main.c
    fn rfvp_ps1_log(message: *const u8, len: usize);
    fn rfvp_ps1_ticks_us() -> u64;
    fn rfvp_ps1_cycles() -> u32;
    pub fn rfvp_ps1_file_find(name: *const c_char, size: *mut u32) -> c_int;
    pub fn rfvp_ps1_file_read(handle: c_int, offset: u32, buf: *mut u8, len: u32) -> c_int;
    pub fn rfvp_ps1_file_lba(handle: c_int) -> u32;
    pub fn rfvp_ps1_file_count() -> c_int;
    pub fn rfvp_ps1_file_name(index: c_int) -> *const c_char;
}

pub fn log(message: &str) {
    unsafe { rfvp_ps1_log(message.as_ptr(), message.len()) };
}

pub fn ticks_us() -> u64 {
    unsafe { rfvp_ps1_ticks_us() }
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

/// The one heap of the console, for the engine and the C runtime alike: talc (bins of free
/// blocks, a word of overhead each) in place of PSn00bSDK's malloc, which walks a list of every
/// block on each call and, in 2 MiB, left too little in one piece next to separate pools.
/// main.c hands over the memory with `rfvp_ps1_heap_init`; its malloc and free (weak in
/// PSn00bSDK's libc) are the ones below.
static mut TALC: talc::Talc<talc::ErrOnOom> = talc::Talc::new(talc::ErrOnOom);

/// Bytes handed out, and the most at once (reported on the TTY).
pub static mut HEAP_IN_USE: usize = 0;
pub static mut HEAP_PEAK: usize = 0;

fn account(delta: isize) {
    unsafe {
        HEAP_IN_USE = (HEAP_IN_USE as isize + delta) as usize;
        if HEAP_IN_USE > HEAP_PEAK {
            HEAP_PEAK = HEAP_IN_USE;
        }
    }
}

fn talc() -> &'static mut talc::Talc<talc::ErrOnOom> {
    // SAFETY: one CPU thread, and no allocation from interrupt handlers.
    unsafe { &mut *core::ptr::addr_of_mut!(TALC) }
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_ps1_heap_init(base: *mut u8, size: usize) {
    let _ = unsafe { talc().claim(talc::Span::from_base_size(base, size)) };
}

unsafe fn allocate(layout: Layout) -> Option<core::ptr::NonNull<u8>> {
    unsafe { talc().malloc(layout).ok() }
}

/// Reports an allocation that failed, with the heap in use, without allocating.
fn trace_big(size: usize, _p: *mut u8) {
    let mut buf = [0u8; 96];
    let mut n = 0;
    let mut put = |bytes: &[u8], buf: &mut [u8; 96], n: &mut usize| {
        for b in bytes {
            buf[*n] = *b;
            *n += 1;
        }
    };
    let mut number = |v: usize, buf: &mut [u8; 96], n: &mut usize| {
        let mut digits = [0u8; 10];
        let (mut d, mut v) = (0, v);
        loop {
            digits[d] = b'0' + (v % 10) as u8;
            d += 1;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        while d > 0 {
            d -= 1;
            buf[*n] = digits[d];
            *n += 1;
        }
    };
    put(if _p.is_null() { b"rfvp: no memory for " } else { b"rfvp: big alloc of  " }, &mut buf, &mut n);
    number(size, &mut buf, &mut n);
    put(b" bytes, in use ", &mut buf, &mut n);
    number(unsafe { HEAP_IN_USE } / 1024, &mut buf, &mut n);
    put(b" KiB\n", &mut buf, &mut n);
    unsafe { rfvp_ps1_log(buf.as_ptr(), n) };
}

struct TalcAlloc;

unsafe impl GlobalAlloc for TalcAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match unsafe { allocate(layout) } {
            Some(p) => {
                account(layout.size() as isize);
                p.as_ptr()
            }
            None => {
                trace_big(layout.size(), core::ptr::null_mut());
                core::ptr::null_mut()
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        account(-(layout.size() as isize));
        unsafe { talc().free(core::ptr::NonNull::new_unchecked(ptr), layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { core::ptr::NonNull::new_unchecked(ptr) };
        let heap = talc();
        if new_size <= layout.size() {
            unsafe { heap.shrink(p, layout, new_size) };
            account(new_size as isize - layout.size() as isize);
            return ptr;
        }
        if let Ok(grown) = unsafe { heap.grow_in_place(p, layout, new_size) } {
            account(new_size as isize - layout.size() as isize);
            return grown.as_ptr();
        }
        // Moved: to the heap a block of the new size belongs in.
        let Ok(new_layout) = Layout::from_size_align(new_size, layout.align()) else {
            return core::ptr::null_mut();
        };
        match unsafe { allocate(new_layout) } {
            Some(moved) => {
                unsafe {
                    core::ptr::copy_nonoverlapping(ptr, moved.as_ptr(), layout.size());
                    heap.free(p, layout);
                }
                account(new_size as isize - layout.size() as isize);
                moved.as_ptr()
            }
            None => {
                trace_big(new_size, core::ptr::null_mut());
                core::ptr::null_mut()
            }
        }
    }
}

/// C's blocks carry their size in the 8 bytes before them, for `free`.
const C_HEADER: usize = 8;

#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    let Ok(layout) = Layout::from_size_align(size + C_HEADER, 8) else {
        return core::ptr::null_mut();
    };
    let p = unsafe { TalcAlloc.alloc(layout) };
    if p.is_null() {
        return p as *mut c_void;
    }
    unsafe {
        *(p as *mut usize) = size;
        p.add(C_HEADER) as *mut c_void
    }
}

#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        let p = (ptr as *mut u8).sub(C_HEADER);
        let size = *(p as *const usize);
        TalcAlloc.dealloc(p, Layout::from_size_align_unchecked(size + C_HEADER, 8));
    }
}

#[no_mangle]
pub unsafe extern "C" fn calloc(count: usize, size: usize) -> *mut c_void {
    let Some(total) = count.checked_mul(size) else {
        return core::ptr::null_mut();
    };
    let p = unsafe { malloc(total) };
    if !p.is_null() {
        unsafe { core::ptr::write_bytes(p as *mut u8, 0, total) };
    }
    p
}

#[no_mangle]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    if ptr.is_null() {
        return unsafe { malloc(size) };
    }
    let old = unsafe { *((ptr as *const u8).sub(C_HEADER) as *const usize) };
    let new = unsafe { malloc(size) };
    if !new.is_null() {
        unsafe { core::ptr::copy_nonoverlapping(ptr as *const u8, new as *mut u8, old.min(size)) };
        unsafe { free(ptr) };
    }
    new
}

#[cfg(target_arch = "mips")]
#[global_allocator]
static ALLOCATOR: TalcAlloc = TalcAlloc;

#[cfg(target_arch = "mips")]
#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    let (in_use, peak) = unsafe { (HEAP_IN_USE, HEAP_PEAK) };
    log(&format!("rfvp: panic (heap {} KiB, peak {} KiB): {info}\n", in_use / 1024, peak / 1024));
    loop {}
}

pub struct Session {
    app: Ps1App,
    frames: u32,
    window_started_us: u64,
    last_frame_us: u64,
    worst_us: u64,
}

/// Frames between two reports on the TTY.
const REPORT_EVERY: u32 = 30;

impl Session {
    fn report_error(&mut self, what: &str, err: Option<rfvp::host_api::RfvpError>) {
        let detail = self.app.core().last_error_detail().map(String::from).unwrap_or_default();
        log(&format!(
            "rfvp: {what}{} (at core mark {})\n{detail}\nlog:\n{}\n",
            err.map(|err| format!(" ({err:?})")).unwrap_or_default(),
            rfvp::perf::last_mark(),
            rfvp::ring_log::dump()
        ));
    }

    fn report(&mut self) {
        let now = ticks_us();
        self.worst_us = self.worst_us.max(now.saturating_sub(self.last_frame_us));
        self.last_frame_us = now;
        if self.frames % REPORT_EVERY != 0 {
            return;
        }
        let window_us = now.saturating_sub(self.window_started_us).max(1);
        let (in_use, peak) = unsafe { (HEAP_IN_USE, HEAP_PEAK) };
        log(&format!(
            "rfvp: frame {}: {:.1} fps, slowest {} ms, heap {} KiB (peak {} KiB)\n",
            self.app.core().frame_index(),
            REPORT_EVERY as f32 * 1_000_000.0 / window_us as f32,
            self.worst_us / 1000,
            in_use / 1024,
            peak / 1024,
        ));
        self.window_started_us = now;
        self.worst_us = 0;
    }
}

/// Boot the game on the disc. Returns null if it cannot start (the reason goes to the TTY).
#[no_mangle]
pub unsafe extern "C" fn rfvp_ps1_start() -> *mut Session {
    rfvp::ring_log::install();
    extern "C" fn cycles() -> u32 {
        unsafe { rfvp_ps1_cycles() }
    }
    rfvp::perf::set_cycle_counter(cycles);
    let app = Ps1App::new(RfvpCoreConfig::default(), 64);
    let now = ticks_us();
    let mut session = Box::new(Session {
        app,
        frames: 0,
        window_started_us: now,
        last_frame_us: now,
        worst_us: 0,
    });
    {
        use core::mem::size_of;
        use rfvp::subsystem::resources::*;
        log(&format!(
            "rfvp: sizes: prim {} graph {} text item {} save item {} motion manager {} game data {}\n",
            size_of::<prim::Prim>(),
            size_of::<graph_buff::GraphBuff>(),
            size_of::<text_manager::TextItem>(),
            size_of::<Option<save_manager::SaveItem>>(),
            size_of::<motion_manager::MotionManager>(),
            size_of::<rfvp::subsystem::world::GameData>(),
        ));
        log(&format!("rfvp: heap before boot {} KiB\n", unsafe { HEAP_IN_USE } / 1024));
    }
    log("rfvp: booting the game\n");
    let boot = RfvpBootConfig {
        nls: Nls::ShiftJIS,
        max_hcb_bytes: 4 * 1024 * 1024,
        // Pages of the script in RAM; the rest stays on the disc.
        hcb_cache_bytes: 48 * 1024,
        ..RfvpBootConfig::default()
    };
    if let Err(err) = session.app.boot(boot) {
        session.report_error("the game could not be loaded", Some(err));
        return core::ptr::null_mut();
    }
    log("rfvp: the game is loaded, running\n");
    Box::into_raw(session)
}

/// Run one frame and draw it. Returns 0 to go on, 1 when the game wants to quit and -1 when it
/// stopped with an error.
#[no_mangle]
pub unsafe extern "C" fn rfvp_ps1_frame(session: *mut Session, input: *const Ps1Input) -> c_int {
    let session = unsafe { &mut *session };
    let input = unsafe { &*input };
    if let Err(err) = session.app.run_frame(input) {
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
    session.report();
    0
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_ps1_stop(session: *mut Session) {
    if !session.is_null() {
        drop(unsafe { Box::from_raw(session) });
    }
}
