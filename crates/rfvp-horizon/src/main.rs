#![no_std]
#![no_main]

extern crate alloc;
extern crate nx;

use alloc::format;
use alloc::string::String;
use core::panic::PanicInfo;

use nx::diag::abort;
use nx::diag::log::lm::LmLogger;
use nx::diag::log::{log_with, LogMetadata, LogSeverity};
use nx::result::Result;
use nx::util;
use nx::util::PointerAndSize;
use rfvp::host_api::{RfvpClock, RfvpFile, RfvpFileSystem, RfvpHost};
use rfvp::script::parser::Nls;
use rfvp::{RfvpBootConfig, RfvpCoreConfig};
use rfvp_horizon::fs::DEFAULT_GAME_ROOT;
use rfvp_horizon::{horizon_status_to_result_code, HorizonApp};

nx::rrt0_define_module_name!("rfvp-horizon");

/// Optional file next to the game selecting the script encoding: `sjis`, `gbk` or `utf8`.
const NLS_FILE: &str = "sdmc:/switch/rfvp/nls.txt";
/// Written when the game cannot start, so the reason can be read off the SD card.
const ERROR_FILE: &str = "sdmc:/switch/rfvp/last_error.txt";
/// If this file exists the port reports how it is doing in `DEBUG_REPORT_FILE`, about once a
/// second: for finding out why a game is slow or stuck on a console.
const DEBUG_FLAG_FILE: &str = "sdmc:/switch/rfvp/debug.txt";
const DEBUG_REPORT_FILE: &str = "sdmc:/switch/rfvp/debug_report.txt";
/// Frames between two reports.
const DEBUG_REPORT_EVERY: u32 = 60;

/// A line in the system log, which an emulator shows. Used to follow how far startup got.
fn trace(message: &str) {
    let metadata = LogMetadata::new(
        LogSeverity::Info,
        false,
        String::from(message),
        "crates/rfvp-horizon/src/main.rs",
        "main",
        0,
    );
    log_with::<LmLogger>(&metadata);
}

/// Heap sizes to ask the kernel for when the loader did not give one, largest first.
const HEAP_SIZES: [usize; 5] = [
    1024 * 1024 * 1024,
    768 * 1024 * 1024,
    512 * 1024 * 1024,
    384 * 1024 * 1024,
    256 * 1024 * 1024,
];

#[no_mangle]
pub fn initialize_heap(hbl_heap: PointerAndSize) -> PointerAndSize {
    if hbl_heap.is_valid() {
        return hbl_heap;
    }
    // Homebrew Loader normally hands over a heap. Without one, nx would ask for all the memory
    // the process may have, which the kernel refuses when that is a lot (an emulator, or a
    // process with a big allowance); and a refusal here is a panic before anything can report
    // it. So try sizes from large to small instead, capped by what is available.
    let available = nx::svc::get_info(
        nx::svc::InfoId::TotalMemorySize,
        nx::svc::CURRENT_PROCESS_PSEUDO_HANDLE,
        0,
    )
    .ok()
    .zip(
        nx::svc::get_info(
            nx::svc::InfoId::UsedMemorySize,
            nx::svc::CURRENT_PROCESS_PSEUDO_HANDLE,
            0,
        )
        .ok(),
    )
    .map(|(total, used)| total.saturating_sub(used).saturating_sub(0x20_0000) as usize)
    .unwrap_or(usize::MAX);
    for size in HEAP_SIZES {
        let size = size.min(available) & !0x1F_FFFF;
        if size == 0 {
            continue;
        }
        if let Ok(address) = nx::svc::set_heap_size(size) {
            if !address.is_null() {
                return PointerAndSize::new(address, size);
            }
        }
    }
    nx::mem::alloc::configure_heap(hbl_heap)
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

fn report_error(app: &mut HorizonApp, what: &str) {
    let detail = app
        .core()
        .last_error_detail()
        .map(String::from)
        .unwrap_or_default();
    let text = format!(
        "{what}\ngame directory: {DEFAULT_GAME_ROOT}\n{detail}\n\
         The game directory needs the game's .hcb script and .bin packs, plus a CJK font \
         named default.ttf.\n\nlog:\n{}",
        rfvp::ring_log::dump()
    );
    let _ = app.host().fs().write_all(ERROR_FILE, text.as_bytes());
}

#[no_mangle]
pub fn main() -> Result<()> {
    rfvp::ring_log::install();
    trace("rfvp: starting");
    let mut app = match HorizonApp::new(RfvpCoreConfig::default(), 256) {
        Ok(app) => app,
        Err(err) => {
            trace("rfvp: could not set up the app");
            return Err(horizon_status_to_result_code(err));
        }
    };
    trace("rfvp: app is up, reading the encoding setting");

    let nls = read_nls(&mut app);
    trace("rfvp: booting the game");
    let boot = RfvpBootConfig {
        asset_root: DEFAULT_GAME_ROOT,
        nls,
        ..RfvpBootConfig::default()
    };
    if let Err(err) = app.boot(boot) {
        trace("rfvp: the game could not be loaded");
        report_error(&mut app, "the game could not be loaded");
        return Err(horizon_status_to_result_code(err));
    }
    trace("rfvp: the game is loaded, running");

    let debug = app.host().fs().open(DEBUG_FLAG_FILE).is_ok();
    let mut frames = 0u32;
    let mut window_started_us = app.host().clock().ticks_us();
    let mut worst_us = 0u64;
    let mut last_frame_us = window_started_us;

    // The core draws and presents a frame per tick; presenting waits for the display, which
    // paces this loop.
    loop {
        if let Err(err) = app.run_frame() {
            report_error(&mut app, "the game stopped with an error");
            return Err(horizon_status_to_result_code(err));
        }
        if app.quit_requested() {
            break;
        }

        frames += 1;
        if frames == 1 {
                    trace("rfvp: the first frame was drawn");
        }
        if debug {
            let now = app.host().clock().ticks_us();
            worst_us = worst_us.max(now.saturating_sub(last_frame_us));
            last_frame_us = now;
            if frames % DEBUG_REPORT_EVERY == 0 {
                let window_us = now.saturating_sub(window_started_us).max(1);
                let text = format!(
                    "frame {}: {:.1} fps over the last {} frames, slowest frame {:.1} ms\n\
                     virtual frame {}x{}\n\nlog:\n{}",
                    app.core().frame_index(),
                    DEBUG_REPORT_EVERY as f64 * 1_000_000.0 / window_us as f64,
                    DEBUG_REPORT_EVERY,
                    worst_us as f64 / 1000.0,
                    app.core().config().virtual_width,
                    app.core().config().virtual_height,
                    rfvp::ring_log::dump()
                );
                let _ = app.host().fs().write_all(DEBUG_REPORT_FILE, text.as_bytes());
                window_started_us = now;
                worst_us = 0;
            }
        }
    }

    Ok(())
}

#[panic_handler]
fn panic_handler(info: &PanicInfo<'_>) -> ! {
    util::simple_panic_handler::<LmLogger>(info, abort::AbortLevel::FatalThrow())
}
