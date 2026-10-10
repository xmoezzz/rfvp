#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use alloc::format;
use core::fmt::Write;
use core::mem::MaybeUninit;
use core::panic::PanicInfo;

use rfvp::host_api::{RfvpClock, RfvpFileSystem, RfvpHost};
use rfvp::{RfvpBootConfig, RfvpCoreConfig};
use rfvp_psv::{PsvApp, RawPsvHost};

extern "C" {
    fn rfvp_psv_make_raw_host(out_host: *mut RawPsvHost) -> i32;
    fn rfvp_psv_platform_poll(app: *mut PsvApp) -> i32;
    fn rfvp_psv_platform_should_exit() -> i32;
    fn rfvp_psv_vitasdk_init();
    fn rfvp_psv_vitasdk_fini();
    fn rfvp_psv_vitasdk_abort() -> !;
    fn rfvp_psv_c_write_log(name: *const u8, text: *const u8, text_len: usize);
    fn rfvp_psv_gpu_stats(present_us: *mut u64, draw_calls: *mut u64);
}

/// newlib's default heap is far too small for the game's images (a 1080p CG alone is 8 MiB
/// decoded). The VPK runs in extended memory mode (`ATTRIBUTE2=12`), and the heap is reserved
/// whole at startup; 320 MiB, as siglus_rs's Vita port uses, leaves room for stacks and the
/// system.
#[used]
#[no_mangle]
pub static _newlib_heap_size_user: u32 = 320 * 1024 * 1024;

/// Written next to the game when it cannot start or stops, so the reason can be read off the
/// memory card.
const ERROR_FILE: &str = "last_error.txt";
/// If this file is in the game folder, the port reports how it is doing in `DEBUG_REPORT_FILE`
/// about once a second: frame rate, slowest frame and the recent log.
const DEBUG_FLAG_FILE: &str = "debug.txt";
const DEBUG_REPORT_FILE: &str = "debug_report.txt";
/// Frames between two reports.
const DEBUG_REPORT_EVERY: u32 = 60;

fn report_error(app: &mut PsvApp, what: &str) {
    let detail = app
        .core()
        .last_error_detail()
        .map(alloc::string::String::from)
        .unwrap_or_default();
    let text = format!(
        "{what}\n{detail}\n\
         The game folder (ux0:data/rfvp/game) needs the game's .hcb script and .bin packs, plus \
         a CJK font named default.ttf.\n\nlog:\n{}",
        rfvp::ring_log::dump()
    );
    let _ = app.host().fs().write_all(ERROR_FILE, text.as_bytes());
}

#[no_mangle]
pub unsafe extern "C" fn main(_argc: i32, _argv: *mut *mut u8) -> i32 {
    rfvp::ring_log::install();
    unsafe { rfvp_psv_vitasdk_init() };

    let mut raw_host = MaybeUninit::<RawPsvHost>::uninit();
    let status = unsafe { rfvp_psv_make_raw_host(raw_host.as_mut_ptr()) };
    if status != 0 {
        unsafe { rfvp_psv_vitasdk_fini() };
        return status;
    }

    let raw_host = unsafe { raw_host.assume_init() };
    let mut app = PsvApp::new(raw_host, RfvpCoreConfig::default(), 256);

    if let Err(err) = app.boot(RfvpBootConfig::default()) {
        report_error(&mut app, "the game could not be loaded");
        let _ = app.render_frame();
        unsafe { rfvp_psv_vitasdk_fini() };
        return err_to_status(err);
    }

    let debug = app.host().fs().exists(DEBUG_FLAG_FILE);
    let mut frames = 0u32;
    let mut window_started_us = app.host().clock().ticks_us();
    let mut last_frame_us = window_started_us;
    let mut worst_us = 0u64;

    loop {
        let poll_status = unsafe { rfvp_psv_platform_poll(&mut app) };
        if poll_status != 0 {
            unsafe { rfvp_psv_vitasdk_fini() };
            return poll_status;
        }

        if let Err(err) = app.run_frame() {
            report_error(&mut app, "the game stopped with an error");
            unsafe { rfvp_psv_vitasdk_fini() };
            return err_to_status(err);
        }

        if app.quit_requested() || unsafe { rfvp_psv_platform_should_exit() } != 0 {
            break;
        }

        frames += 1;
        if debug {
            let now = app.host().clock().ticks_us();
            worst_us = worst_us.max(now.saturating_sub(last_frame_us));
            last_frame_us = now;
            if frames % DEBUG_REPORT_EVERY == 0 {
                let window_us = now.saturating_sub(window_started_us).max(1);
                let (mut present_us, mut draw_calls) = (0u64, 0u64);
                unsafe { rfvp_psv_gpu_stats(&mut present_us, &mut draw_calls) };
                let per_frame = |us: u64| us as f64 / 1000.0 / DEBUG_REPORT_EVERY as f64;
                let text = format!(
                    "frame {}: {:.1} fps over the last {} frames, slowest frame {:.1} ms\n\
                     per frame: {:.1} ms engine and GPU, {:.1} ms waiting for vblank, {} draw calls\n\n\
                     log:\n{}",
                    app.core().frame_index(),
                    DEBUG_REPORT_EVERY as f64 * 1_000_000.0 / window_us as f64,
                    DEBUG_REPORT_EVERY,
                    worst_us as f64 / 1000.0,
                    per_frame(window_us.saturating_sub(present_us)),
                    per_frame(present_us),
                    draw_calls / DEBUG_REPORT_EVERY as u64,
                    rfvp::ring_log::dump()
                );
                let _ = app.host().fs().write_all(DEBUG_REPORT_FILE, text.as_bytes());
                window_started_us = now;
                worst_us = 0;
            }
        }
    }

    unsafe { rfvp_psv_vitasdk_fini() };
    0
}

/// A fixed buffer to format into without allocating: a panic can be an out-of-memory.
struct PanicText {
    bytes: [u8; 512],
    len: usize,
}

impl Write for PanicText {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let room = self.bytes.len() - self.len;
        let n = s.len().min(room);
        self.bytes[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
        self.len += n;
        Ok(())
    }
}

fn die(text: &PanicText) -> ! {
    unsafe {
        rfvp_psv_c_write_log(b"panic.txt\0".as_ptr(), text.bytes.as_ptr(), text.len);
        rfvp_psv_vitasdk_abort()
    }
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    let mut text = PanicText { bytes: [0; 512], len: 0 };
    let _ = write!(text, "{info}");
    die(&text)
}

#[alloc_error_handler]
fn alloc_error(layout: core::alloc::Layout) -> ! {
    let mut text = PanicText { bytes: [0; 512], len: 0 };
    let _ = write!(text, "out of memory allocating {} bytes", layout.size());
    die(&text)
}

fn err_to_status(err: rfvp::host_api::RfvpError) -> i32 {
    match err {
        rfvp::host_api::RfvpError::Io => rfvp_psv::PsvStatus::Io.as_i32(),
        rfvp::host_api::RfvpError::NotFound => rfvp_psv::PsvStatus::NotFound.as_i32(),
        rfvp::host_api::RfvpError::InvalidData => rfvp_psv::PsvStatus::InvalidData.as_i32(),
        rfvp::host_api::RfvpError::InvalidArgument => rfvp_psv::PsvStatus::InvalidArgument.as_i32(),
        rfvp::host_api::RfvpError::Unsupported => rfvp_psv::PsvStatus::Unsupported.as_i32(),
        rfvp::host_api::RfvpError::OutOfMemory => rfvp_psv::PsvStatus::OutOfMemory.as_i32(),
        rfvp::host_api::RfvpError::CapacityExceeded => {
            rfvp_psv::PsvStatus::CapacityExceeded.as_i32()
        }
        rfvp::host_api::RfvpError::EndOfFile => rfvp_psv::PsvStatus::EndOfFile.as_i32(),
        rfvp::host_api::RfvpError::Backend => rfvp_psv::PsvStatus::Backend.as_i32(),
    }
}
