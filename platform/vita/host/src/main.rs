#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;
mod diagnostics;

use alloc::format;
use core::fmt::Write;
use core::mem::MaybeUninit;
use core::panic::PanicInfo;

use rfvp::host_api::{RfvpFileSystem, RfvpHost};
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
/// This diagnostic build records automatically. This flag permits a comparison with timing off.
const DEBUG_OFF_FILE: &str = "diagnostics_off.txt";
const DEBUG_REPORT_FILE: &str = "debug_report.txt";

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
    let debug = !app.host().fs().exists(DEBUG_OFF_FILE);
    if debug { rfvp::perf::set_clock(diagnostics::clock_us); }
    let boot_started = diagnostics::clock_us();

    // A translated script (and its encoding) is picked by `config.toml` in the game directory.
    if let Err(err) = app.boot(RfvpBootConfig::default()) {
        report_error(&mut app, "the game could not be loaded");
        let _ = app.render_frame();
        unsafe { rfvp_psv_vitasdk_fini() };
        return err_to_status(err);
    }

    let header = format!("hcb={}\nboot_us={}\ndiagnostics_off.txt disables recording on next launch",
        app.core().loaded_game().map(|game| game.hcb_path.as_str()).unwrap_or("unknown"),
        diagnostics::clock_us().saturating_sub(boot_started));
    let mut diagnostics = diagnostics::Diagnostics::new(header.clone());
    if debug {
        let boot_report = format!("build={}\n{}\nboot stages (inclusive us) {:?}\n{:?}\n{}",
            diagnostics::BUILD, header, rfvp::perf::STAGES, rfvp::perf::take(), rfvp::ring_log::dump());
        let _ = app.host().fs().write_all("debug_boot.txt", boot_report.as_bytes());
        let _ = rfvp::perf::take_ops();
    }
    let mut frames = 0u64;
    let mut last_report_us = diagnostics::clock_us();
    let (mut unused_present, mut unused_draws) = (0, 0);
    unsafe { rfvp_psv_gpu_stats(&mut unused_present, &mut unused_draws) };

    loop {
        let started = diagnostics::clock_us();
        let poll_status = unsafe { rfvp_psv_platform_poll(&mut app) };
        if poll_status != 0 {
            unsafe { rfvp_psv_vitasdk_fini() };
            return poll_status;
        }

        let tick = match app.run_frame() {
            Ok(tick) => tick,
            Err(err) => {
                report_error(&mut app, "the game stopped with an error");
                if debug { let _ = app.host().fs().write_all(DEBUG_REPORT_FILE, diagnostics.report().as_bytes()); }
                unsafe { rfvp_psv_vitasdk_fini() };
                return err_to_status(err);
            }
        };
        let ended = diagnostics::clock_us();
        let quitting = app.quit_requested() || unsafe { rfvp_psv_platform_should_exit() } != 0;

        frames += 1;
        if debug {
            let (mut present_us, mut draws) = (0, 0);
            unsafe { rfvp_psv_gpu_stats(&mut present_us, &mut draws) };
            diagnostics.record(tick, started, ended, present_us, draws);
            if frames == 1 || frames % diagnostics::REPORT_EVERY == 0
                || ended.saturating_sub(last_report_us) >= 2_000_000 || quitting {
                let write_started = diagnostics::clock_us();
                let text = diagnostics.report();
                if app.host().fs().write_all(DEBUG_REPORT_FILE, text.as_bytes()).is_err() {
                    report_error(&mut app, "diagnostic report could not be written");
                }
                diagnostics.last_write_us = diagnostics::clock_us().saturating_sub(write_started);
                last_report_us = diagnostics::clock_us();
            }
        }
        if quitting { break; }
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
