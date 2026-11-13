#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;
mod browser;
mod diagnostics;
mod launcher;
mod text;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;
use core::mem::MaybeUninit;
use core::panic::PanicInfo;

use rfvp::host_api::{RfvpFile, RfvpFileSystem, RfvpHost};
use rfvp::{RfvpBootConfig, RfvpCoreConfig};
use rfvp_psv::fs::PsvFileSystem;
use rfvp_psv::{PsvApp, RawPsvHost};

use browser::Outcome;
use launcher::LaunchRequest;

extern "C" {
    fn rfvp_psv_make_raw_host(out_host: *mut RawPsvHost) -> i32;
    fn rfvp_psv_platform_poll(app: *mut PsvApp) -> i32;
    fn rfvp_psv_platform_should_exit() -> i32;
    fn rfvp_psv_vitasdk_init();
    fn rfvp_psv_vitasdk_fini();
    fn rfvp_psv_vitasdk_abort() -> !;
    fn rfvp_psv_c_write_log(name: *const u8, text: *const u8, text_len: usize);
    fn rfvp_psv_gpu_stats(present_us: *mut u64, draw_calls: *mut u64);
    /// Sets the directory relative paths resolve in (config, packs, font, saves).
    fn rfvp_psv_c_set_asset_root(root: *const u8) -> i32;
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

/// Longest directory the platform layer can hold (its own limit is 1024 bytes including the
/// terminator). Checked here so an over-long path is refused with a message instead of cut.
const MAX_DIR_BYTES: usize = 1000;

/// How long the launcher waits for the confirming press to be released, in presented frames
/// (about ten seconds at 60 Hz). Running out cancels the launch instead of starting a game with
/// a held button.
const RELEASE_WAIT_FRAMES: u32 = 600;

fn report_error(app: &mut PsvApp, what: &str) {
    let detail = app
        .core()
        .last_error_detail()
        .map(String::from)
        .unwrap_or_default();
    let text = format!(
        "{what}\n{detail}\n\
         The chosen game folder needs the game's .hcb script and .bin packs, plus \
         a CJK font named default.ttf.\n\nlog:\n{}",
        rfvp::ring_log::dump()
    );
    let _ = app.host().fs().write_all(ERROR_FILE, text.as_bytes());
}

/// Read a file through the platform layer, bounded. Used by the launcher for its font and its
/// own state; the game's own reads go through the core.
pub fn load_file(fs: &mut PsvFileSystem, path: &str, limit: usize) -> rfvp::host_api::RfvpResult<Vec<u8>> {
    let mut file = RfvpFileSystem::open(fs, path)?;
    file.read_to_vec(limit)
}

fn set_asset_root(dir: &str) -> Result<(), i32> {
    if dir.is_empty() || dir.len() > MAX_DIR_BYTES {
        return Err(-7); // RFVP_PSV_CAPACITY_EXCEEDED
    }
    let mut bytes = Vec::with_capacity(dir.len() + 1);
    bytes.extend_from_slice(dir.as_bytes());
    bytes.push(0);
    let status = unsafe { rfvp_psv_c_set_asset_root(bytes.as_ptr()) };
    if status == 0 {
        Ok(())
    } else {
        Err(status)
    }
}

/// The launcher, for one selection: browse, draw, and return what the player chose.
///
/// Each call creates and drops its own browser, so the font, glyph cache, layer and texture are
/// gone before the game allocates anything.
fn run_launcher(app: &mut PsvApp, previous_error: Option<&str>) -> Option<LaunchRequest> {
    let mut browser = match browser::Browser::new(app.host().fs()) {
        Ok(browser) => browser,
        Err(message) => {
            // No font at all: nothing could be drawn, so fall back to the behaviour of earlier
            // builds (start whatever the fixed game folder holds) and leave the reason on the
            // card. Starting a game that then complains about the missing font is still better
            // than a blank screen.
            let text = format!(
                "the file browser could not start: {message}\n\
                 Fix the font path, then launch again. Falling back to {}.\n",
                launcher::DEFAULT_GAME_DIR
            );
            let _ = rfvp::host_api::RfvpFileSystem::write_all(
                app.host().fs(),
                launcher::LOG_FILE,
                text.as_bytes(),
            );
            return Some(LaunchRequest {
                asset_root: launcher::normalize_dir(launcher::DEFAULT_GAME_DIR),
                script: String::new(),
                nls: None,
                encoding: launcher::Encoding::Auto,
                encoding_source: "fallback",
            });
        }
    };
    if let Some(error) = previous_error {
        browser.show_error(error);
    }

    let request = loop {
        let input = browser::poll_input().unwrap_or_default();
        let outcome = browser.handle(app.host().fs(), &input);
        if let Outcome::Launch(request) = outcome {
            // Wait until the button or finger is really gone while the browser keeps the screen.
            // A failed read or a stuck input cancels the launch instead of pretending it was
            // released: the game's first poll must never see a finger that is still down.
            browser.show_note("starting the game\u{2026}");
            match wait_for_release(app, &mut browser) {
                Release::Released => break request,
                Release::StillHeld => {
                    browser.show_error(
                        "start cancelled: a button or the screen is still held; release it and choose the script again",
                    );
                    browser.write_log(app.host().fs());
                }
                Release::Unreadable => {
                    browser.show_error(
                        "start cancelled: the pad and touch screen could not be read",
                    );
                    browser.write_log(app.host().fs());
                }
            }
            continue;
        }
        if let Outcome::Quit = outcome {
            browser.release(app.host().renderer());
            return None;
        }
        if let Err(err) = browser.draw(app.host().renderer()) {
            // The renderer cannot draw the browser: stop pretending it can be used.
            browser.note(&format!("browser draw failed: {err:?}"));
            browser.write_log(app.host().fs());
            browser.release(app.host().renderer());
            return None;
        }
    };
    browser.release(app.host().renderer());
    Some(request)
}

enum Release {
    /// Nothing is held any more (and no finger is on the screen).
    Released,
    /// The wait ran out with input still down.
    StillHeld,
    /// The platform stopped answering.
    Unreadable,
}

/// Poll until no pad button is held and no finger is on the screen, at most a few seconds.
///
/// Each iteration presents a frame, which is what paces the wait; the screen keeps showing the
/// browser. Returning something other than [`Release::Released`] is not a release: the caller
/// must cancel the launch rather than let the game inherit a held input.
fn wait_for_release(app: &mut PsvApp, browser: &mut browser::Browser) -> Release {
    for _ in 0..RELEASE_WAIT_FRAMES {
        let Some(input) = browser::poll_input() else {
            return Release::Unreadable;
        };
        if input.held == 0 && !input.touch_held && !input.touch_down {
            return Release::Released;
        }
        let _ = browser.draw(app.host().renderer());
    }
    Release::StillHeld
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

    // Choose a game, then start it. A game that fails to start is reported in the browser and
    // the player can choose again in the same process: the core keeps nothing of a failed boot
    // (its parser, game data and VM are only installed once a boot succeeds).
    let mut previous_error: Option<String> = None;
    let header = loop {
        let Some(request) = run_launcher(&mut app, previous_error.as_deref()) else {
            unsafe { rfvp_psv_vitasdk_fini() };
            return 0;
        };

        if let Err(status) = set_asset_root(&request.asset_root) {
            previous_error = Some(format!(
                "this folder path cannot be used (status {status}); open a folder with a shorter path"
            ));
            continue;
        }
        // A script the player chose outranks the configuration; the resource root is the folder
        // the platform layer now points at, so the config, packs, font and saves are all there.
        let boot = RfvpBootConfig {
            asset_root: ".",
            hcb_file: if request.script.is_empty() {
                None
            } else {
                Some(request.script.as_str())
            },
            nls_override: request.nls,
            ..RfvpBootConfig::default()
        };
        app.clear_pending_events();

        let boot_started = diagnostics::clock_us();
        match app.boot(boot) {
            Ok(()) => {
                break format!(
                    "hcb={}\nasset_root={}\nchosen={}\nencoding={} ({})\nboot_us={}\ndiagnostics_off.txt disables recording on next launch",
                    app.core().loaded_game().map(|game| game.hcb_path.as_str()).unwrap_or("unknown"),
                    request.asset_root,
                    if request.script.is_empty() { "(configured search)" } else { request.script.as_str() },
                    request.encoding.label(),
                    request.encoding_source,
                    diagnostics::clock_us().saturating_sub(boot_started));
            }
            Err(err) => {
                report_error(&mut app, "the game could not be loaded");
                let detail = app
                    .core()
                    .last_error_detail()
                    .map(String::from)
                    .unwrap_or_else(|| format!("{err:?}"));
                previous_error = Some(format!(
                    "{} could not start: {detail}",
                    if request.script.is_empty() { "the configured game" } else { request.script.as_str() }
                ));
            }
        }
    };

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
