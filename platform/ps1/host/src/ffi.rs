//! The boundary with the C runtime (`platform/ps1/runtime/main.c`): the services it offers the
//! engine and the entry points it calls.

use alloc::boxed::Box;
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
    fn rfvp_ps1_oom(size: usize);
    #[cfg(feature = "diagnostics")]
    fn rfvp_ps1_stats(frame: u32, ops: u32, vm_us: u32, tree_us: u32, submit_us: u32, syscall_us: u32, script_io_us: u32,
                      used: usize, peak: usize, free: usize);
    pub fn rfvp_ps1_file_find(name: *const c_char, size: *mut u32) -> c_int;
    pub fn rfvp_ps1_file_read(handle: c_int, offset: u32, buf: *mut u8, len: u32) -> c_int;
    pub fn rfvp_ps1_file_lba(handle: c_int) -> u32;
    pub fn rfvp_ps1_file_count() -> c_int;
    pub fn rfvp_ps1_file_name(index: c_int) -> *const c_char;
}

pub fn log(message: &str) {
    unsafe { rfvp_ps1_log(message.as_ptr(), message.len()) };
}

#[cfg(feature = "diagnostics")]
fn profile_us() -> u64 {
    extern "C" {
        fn rfvp_ps1_profile_us() -> u64;
    }
    unsafe { rfvp_ps1_profile_us() }
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
#[cfg(feature = "diagnostics")]
static mut PEAK_HEAP: usize = 0;

#[inline]
fn track_heap() {
    #[cfg(feature = "diagnostics")]
    unsafe {
        let counters = talc().get_counters();
        PEAK_HEAP = PEAK_HEAP.max(counters.claimed_bytes - counters.available_bytes);
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

struct TalcAlloc;

unsafe impl GlobalAlloc for TalcAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match unsafe { talc().malloc(layout) } {
            Ok(p) => { track_heap(); p.as_ptr() },
            Err(_) => {
                unsafe { rfvp_ps1_oom(layout.size()) };
                core::ptr::null_mut()
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { talc().free(core::ptr::NonNull::new_unchecked(ptr), layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { core::ptr::NonNull::new_unchecked(ptr) };
        if new_size <= layout.size() {
            unsafe { talc().shrink(p, layout, new_size) };
            return ptr;
        }
        let result = unsafe { talc().grow(p, layout, new_size) };
        track_heap();
        result.map_or(core::ptr::null_mut(), |p| p.as_ptr())
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
fn panic(_info: &PanicInfo<'_>) -> ! {
    // No message: formatting one would put the whole formatting machinery in 2 MiB.
    log("rfvp: panic\n");
    #[cfg(feature = "diagnostics")]
    {
        use core::fmt::Write;
        struct Tty;
        impl Write for Tty {
            fn write_str(&mut self, value: &str) -> core::fmt::Result {
                log(value);
                Ok(())
            }
        }
        let _ = writeln!(Tty, "{}", _info);
    }
    loop {}
}

pub struct Session {
    app: Ps1App,
    #[cfg(feature = "diagnostics")]
    frames: u32,
}

/// Boot the game on the disc. Returns null if it cannot start.
#[no_mangle]
pub unsafe extern "C" fn rfvp_ps1_start() -> *mut Session {
    #[cfg(feature = "diagnostics")]
    rfvp::perf::set_clock(profile_us);
    let mut session = Box::new(Session {
        app: Ps1App::new(RfvpCoreConfig::default(), 64),
        #[cfg(feature = "diagnostics")]
        frames: 0,
    });
    let boot = RfvpBootConfig {
        nls: Nls::ShiftJIS,
        max_hcb_bytes: 4 * 1024 * 1024,
        // The compiled script's code in RAM (crates/rfvp/src/aot.rs); the rest stays on the
        // disc. A miss costs a CD seek (and the music's), over half a second: 128 KiB holds the
        // code that runs every frame (no more RAM in all than the interpreter took, with its
        // 96 KiB of script pages and its own code).
        hcb_cache_bytes: 128 * 1024,
        ..RfvpBootConfig::default()
    };
    if session.app.boot(boot).is_err() {
        log("rfvp: the game could not be loaded\n");
        return core::ptr::null_mut();
    }
    Box::into_raw(session)
}

/// Run one frame and draw it. Returns 0 to go on, 1 when the game wants to quit and -1 when it
/// stopped with an error.
#[no_mangle]
pub unsafe extern "C" fn rfvp_ps1_frame(session: *mut Session, input: *const Ps1Input) -> c_int {
    let session = unsafe { &mut *session };
    let input = unsafe { &*input };
    if session.app.run_frame(input).is_err() {
        log("rfvp: the game stopped with an error\n");
        return -1;
    }
    #[cfg(feature = "diagnostics")]
    {
        session.frames += 1;
        if session.frames % 100 == 0 {
            let totals = rfvp::perf::take();
            {
                let mut m = alloc::string::String::from("perf:");
                for (name, us) in rfvp::perf::STAGES.iter().zip(totals.iter()) {
                    if *us > 0 {
                        m += &alloc::format!(" {}={}", name, us / 100);
                    }
                }
                m.push('\n');
                log(&m);
            }
            let ops = rfvp::perf::take_ops();
            let counters = talc().get_counters();
            unsafe { rfvp_ps1_stats(session.frames, ops / 100, totals[17] / 100,
                totals[11] / 100, totals[12] / 100, totals[20] / 100, totals[22] / 100,
                counters.claimed_bytes - counters.available_bytes, PEAK_HEAP,
                counters.available_bytes) };
        }
    }
    if session.app.quit_requested() {
        return 1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn rfvp_ps1_stop(session: *mut Session) {
    if !session.is_null() {
        drop(unsafe { Box::from_raw(session) });
    }
}

