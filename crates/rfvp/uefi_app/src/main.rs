#![no_main]

use core::time::Duration;

use rfvp::{
    rfvp_os_host::{init_uefi_platform, RfvpOsConfig, RfvpOsEvent, RfvpOsKey, RfvpOsPointerButton, RfvpOsRuntime},
    script::parser::Nls,
    soft_render::{PixelFormat, SoftFramebuffer},
};
use uefi::boot;
use uefi::boot::{OpenProtocolAttributes, OpenProtocolParams};
use uefi::prelude::*;
use uefi::proto::console::gop::PixelFormat as GopPixelFormat;
use uefi::proto::console::gop::{GraphicsOutput, Mode, PixelBitmask};
use uefi::proto::console::pointer::Pointer;
use uefi::proto::unsafe_protocol;
use uefi::proto::console::text::{Key, ScanCode};
use uefi::proto::media::file::{Directory, File, FileAttribute, FileMode, RegularFile};
use uefi::{CString16, cstr16, println, system, Char16};
use uefi_raw::protocol::console::{
    AbsolutePointerMode, AbsolutePointerProtocol as RawAbsolutePointerProtocol,
    AbsolutePointerState,
};
use uefi_raw::{Boolean, Status as RawStatus};

const DEFAULT_PROJECT_DIR: &str = r"\rfvp";
const DEFAULT_HCB_PATH: &str = r"\rfvp\Snow.hcb";
const FRAMEBUFFER_DIAGNOSTICS_ENABLED: bool = false;
const FRAMEBUFFER_DIAGNOSTIC_FRAMES: usize = 2;
const FRAMEBUFFER_DIAGNOSTIC_INTERVAL: usize = 300;
const POINTER_DIAGNOSTIC_LIMIT: usize = 16;
const POINTER_IDLE_DIAGNOSTICS_ENABLED: bool = false;
const POINTER_IDLE_DIAGNOSTIC_INTERVAL: usize = 120;
const PRESENTER_TEST_PATTERN: bool = false;
const UEFI_FRAME_STALL_MS: u64 = 0;

#[repr(transparent)]
#[unsafe_protocol("8d59d32b-c655-4ae9-9b15-f25904992a43")]
struct AbsolutePointer(RawAbsolutePointerProtocol);

#[entry]
fn main() -> Status {
    match main_impl() {
        Ok(()) => Status::SUCCESS,
        Err(status) => status,
    }
}

fn main_impl() -> Result<(), Status> {
    // Initialize UEFI helpers exactly once, before anything else.
    uefi::helpers::init().map_err(|err| err.status())?;
    // Calibrate the arch-specific monotonic clock (TSC on x86_64,
    // CNTPCT_EL0 on aarch64). This must run before any Instant::now() call.
    init_uefi_platform();

    // On aarch64 and x86_64 UEFI the firmware stack is shallow (often ≤128 KiB).
    // Video decode enters large frames that exhaust it, causing __chkstk faults.
    // Allocate a 16 MiB app-owned stack and run the entire runtime on it.
    #[cfg(all(target_os = "uefi", any(target_arch = "aarch64", target_arch = "x86_64")))]
    // Safety: allocates page-aligned memory via UEFI BootServices and switches
    // the stack pointer with a hand-written assembly trampoline before calling
    // run_initialized().  Boot services are active at this point.
    unsafe { return run_on_big_stack(); }

    // All other targets (UEFI arches without a trampoline, or non-UEFI builds)
    // run run_initialized() directly on the current stack.
    #[cfg(not(all(target_os = "uefi", any(target_arch = "aarch64", target_arch = "x86_64"))))]
    return run_initialized();
}

// The runtime body.  uefi::helpers::init() has already been called by
// main_impl(); do NOT call it again here.
fn run_initialized() -> Result<(), Status> {
    println!("RFVP UEFI host");
    println!("Project directory: {}", DEFAULT_PROJECT_DIR);
    println!("Esc exits the UEFI host.");
    println!("Keyboard, EFI_ABSOLUTE_POINTER_PROTOCOL, and EFI_SIMPLE_POINTER_PROTOCOL input are supported when firmware exposes them.");

    println!("[UEFI] before locating project hcb");
    let hcb_cstr = locate_project_hcb()?;
    println!("[UEFI] after locating project hcb");
    println!("[UEFI] before reading project hcb");
    let hcb_bytes = read_project_hcb(&hcb_cstr)?;
    println!("[UEFI] after reading project hcb bytes={}", hcb_bytes.len());

    println!("[UEFI] before get GOP");
    let gop_handle =
        boot::get_handle_for_protocol::<GraphicsOutput>().map_err(|err| err.status())?;
    println!("[UEFI] after get GOP");
    println!("[UEFI] before open GOP");
    let mut gop =
        boot::open_protocol_exclusive::<GraphicsOutput>(gop_handle).map_err(|err| err.status())?;
    println!("[UEFI] after open GOP");

    println!("[UEFI] before choose_mode");
    let selected_mode = choose_mode(&gop);
    println!("[UEFI] after choose_mode");
    println!("[UEFI] before set_mode");
    if let Some(mode) = selected_mode {
        if let Err(err) = gop.set_mode(&mode) {
            println!(
                "[UEFI] gop.set_mode failed with {:?}; continuing with current GOP mode",
                err.status()
            );
        }
    } else {
        println!("[UEFI] choose_mode returned no candidate; continuing with current GOP mode");
    }
    println!("[UEFI] after set_mode");

    let info = gop.current_mode_info();
    let (screen_width, screen_height) = info.resolution();
    println!(
        "[UEFI] current mode width={} height={} stride={} pixel format={:?}",
        screen_width,
        screen_height,
        info.stride(),
        info.pixel_format()
    );
    if info.pixel_format() == GopPixelFormat::BltOnly {
        println!("[UEFI] GOP BltOnly mode is unsupported by the RFVP UEFI presenter");
        return Err(Status::UNSUPPORTED);
    }

    println!(
        "[UEFI] before RfvpOsRuntime::new project_dir={} screen={}x{} nls={:?}",
        DEFAULT_PROJECT_DIR,
        screen_width,
        screen_height,
        Nls::ShiftJIS
    );
    println!("[UEFI] before RfvpOsConfig::new");
    let runtime_config = RfvpOsConfig::new(DEFAULT_PROJECT_DIR, Nls::ShiftJIS)
        .with_hcb_path(DEFAULT_HCB_PATH)
        .with_hcb_bytes(hcb_bytes);
    println!("[UEFI] after RfvpOsConfig::new");
    println!("[UEFI] before RfvpOsConfig::with_screen_size");
    let runtime_config = runtime_config.with_screen_size(screen_width as u32, screen_height as u32);
    println!("[UEFI] after RfvpOsConfig::with_screen_size");
    println!("[UEFI] calling RfvpOsRuntime::new");
    let mut runtime = match RfvpOsRuntime::new(runtime_config) {
        Ok(runtime) => {
            println!("[UEFI] after RfvpOsRuntime::new");
            runtime
        }
        Err(err) => {
            println!("Failed to initialize RFVP runtime: {:?}", err);
            return Err(Status::LOAD_ERROR);
        }
    };

    println!("[UEFI] before resize_screen");
    runtime.resize_screen(screen_width as u32, screen_height as u32);
    println!("[UEFI] after resize_screen");

    // Capture virtual (game-native) resolution for input coordinate mapping.
    let vs = runtime.virtual_size();
    let virtual_width = vs.0 as i32;
    let virtual_height = vs.1 as i32;
    println!(
        "[UEFI] virtual_size={}x{} screen={}x{}",
        virtual_width, virtual_height, screen_width, screen_height
    );

    let mut pointer_state = UefiPointerState::new(screen_width as i32, screen_height as i32);

    // Send an initial cursor position so the engine's hit-test logic has a valid
    // starting coordinate from frame 1 (cursor_x/cursor_y default to 0,0 otherwise).
    runtime.push_event(RfvpOsEvent::PointerMove {
        x: virtual_width / 2,
        y: virtual_height / 2,
        in_screen: true,
    });

    println!("[UEFI] before pointer detection");
    if boot::get_handle_for_protocol::<AbsolutePointer>().is_ok() {
        println!("EFI_ABSOLUTE_POINTER_PROTOCOL detected.");
    } else {
        println!("EFI_ABSOLUTE_POINTER_PROTOCOL not detected.");
    }
    if boot::get_handle_for_protocol::<Pointer>().is_ok() {
        println!("EFI_SIMPLE_POINTER_PROTOCOL detected.");
    } else {
        println!("EFI_SIMPLE_POINTER_PROTOCOL not detected; keyboard input remains available.");
    }

    if PRESENTER_TEST_PATTERN {
        println!("[UEFI] presenting diagnostic software-surface test pattern");
        let pattern = make_presenter_test_pattern(320, 240)?;
        log_framebuffer_diagnostics(usize::MAX, &pattern);
        present_framebuffer(&mut gop, &pattern, usize::MAX, None)?;
        boot::stall(Duration::from_millis(2000));
    }

    let mut frame_index = 0usize;
    loop {
        let input = poll_input_events(
            &mut pointer_state,
            screen_width as i32,
            screen_height as i32,
            virtual_width,
            virtual_height,
        );
        if input.exit_requested {
            break;
        }
        for event in input.events {
            runtime.push_event(event);
        }

        let fb = match runtime.step_frame() {
            Ok(fb) => fb,
            Err(err) => {
                println!("RFVP frame failed: {:?}", err);
                return Err(Status::DEVICE_ERROR);
            }
        };

        if should_log_frame_diagnostics(frame_index) {
            log_framebuffer_diagnostics(frame_index, fb);
        }

        present_framebuffer(&mut gop, fb, frame_index, Some(&pointer_state))?;
        frame_index = frame_index.saturating_add(1);

        if runtime.should_exit() {
            break;
        }

        if UEFI_FRAME_STALL_MS > 0 {
            boot::stall(Duration::from_millis(UEFI_FRAME_STALL_MS));
        }
    }

    println!("RFVP UEFI host exiting.");
    Ok(())
}

fn choose_mode(gop: &GraphicsOutput) -> Option<Mode> {
    let mut selected: Option<Mode> = None;
    let mut selected_score = 0usize;

    for mode in gop.modes() {
        let info = mode.info();
        if info.pixel_format() == GopPixelFormat::BltOnly {
            continue;
        }

        let (w, h) = info.resolution();
        let score = w.saturating_mul(h);
        if score > selected_score && w >= 640 && h >= 480 {
            selected = Some(mode);
            selected_score = score;
        }
    }

    selected
}

/// Returns true if `s` ends with `.hcb` (case-insensitive ASCII).
fn cstr16_ends_with_hcb(s: &uefi::CStr16) -> bool {
    let chars: Vec<u16> = s.iter().map(|c| u16::from(*c)).collect();
    let n = chars.len();
    if n < 4 {
        return false;
    }
    chars[n - 4] == b'.' as u16
        && (chars[n - 3] == b'h' as u16 || chars[n - 3] == b'H' as u16)
        && (chars[n - 2] == b'c' as u16 || chars[n - 2] == b'C' as u16)
        && (chars[n - 1] == b'b' as u16 || chars[n - 1] == b'B' as u16)
}

/// Enumerate `\rfvp` and return a `CString16` path for the first `.hcb` found.
fn find_first_hcb_in_dir() -> Result<CString16, Status> {
    let image_handle = boot::image_handle();
    let mut fs = boot::get_image_file_system(image_handle).map_err(|e| {
        println!("[UEFI] find_hcb: get_image_file_system failed: {:?}", e.status());
        e.status()
    })?;
    let mut root = fs.open_volume().map_err(|e| {
        println!("[UEFI] find_hcb: open_volume failed: {:?}", e.status());
        e.status()
    })?;
    let dir_handle = root
        .open(cstr16!("\\rfvp"), FileMode::Read, FileAttribute::empty())
        .map_err(|e| {
            println!("[UEFI] find_hcb: open \\rfvp failed: {:?}", e.status());
            e.status()
        })?;
    let mut dir = unsafe { Directory::new(dir_handle) };
    loop {
        let entry = match dir.read_entry_boxed() {
            Ok(Some(e)) => e,
            Ok(None) => {
                println!("[UEFI] find_hcb: no .hcb file found in \\rfvp");
                return Err(Status::NOT_FOUND);
            }
            Err(e) => {
                println!("[UEFI] find_hcb: read_entry_boxed failed: {:?}", e.status());
                return Err(e.status());
            }
        };
        let name = entry.file_name();
        if cstr16_ends_with_hcb(name) {
            // Build "\rfvp\<name>" as a CString16.
            let mut path = CString16::new();
            for c in cstr16!("\\rfvp\\").iter() {
                path.push(*c);
            }
            for c in name.iter() {
                path.push(*c);
            }
            println!("[UEFI] find_hcb: found .hcb file in \\rfvp");
            return Ok(path);
        }
    }
}

/// Find the project HCB by enumerating the project directory.
/// Falls back to the hardcoded default if dynamic discovery fails.
fn locate_project_hcb() -> Result<CString16, Status> {
    match find_first_hcb_in_dir() {
        Ok(path) => Ok(path),
        Err(e) => {
            println!(
                "[UEFI] locate hcb: directory enum failed ({:?}), falling back to {}",
                e, DEFAULT_HCB_PATH
            );
            // Construct the fallback path as a runtime CString16.
            let mut path = CString16::new();
            for c in cstr16!("\\rfvp\\Snow.hcb").iter() {
                path.push(*c);
            }
            Ok(path)
        }
    }
}

fn read_project_hcb(hcb_cstr: &uefi::CStr16) -> Result<Vec<u8>, Status> {
    println!("[UEFI] read hcb: before image_handle");
    let image_handle = boot::image_handle();
    println!("[UEFI] read hcb: after image_handle");
    println!("[UEFI] read hcb: before get_image_file_system");
    let mut fs = boot::get_image_file_system(image_handle).map_err(|err| {
        println!(
            "[UEFI] failed to open boot image file system while reading hcb: {:?}",
            err.status()
        );
        err.status()
    })?;
    println!("[UEFI] read hcb: after get_image_file_system");
    println!("[UEFI] read hcb: before open_volume");
    let mut root = fs.open_volume().map_err(|err| {
        println!(
            "[UEFI] failed to open boot image volume while reading hcb: {:?}",
            err.status()
        );
        err.status()
    })?;
    println!("[UEFI] read hcb: after open_volume");
    println!("[UEFI] read hcb: before root.open");
    let hcb_handle = root
        .open(hcb_cstr, FileMode::Read, FileAttribute::empty())
        .map_err(|err| {
            println!("[UEFI] failed to open hcb: {:?}", err.status());
            err.status()
        })?;
    println!("[UEFI] read hcb: after root.open");

    let mut hcb_file = unsafe { RegularFile::new(hcb_handle) };
    let mut hcb_bytes = Vec::new();
    let mut next_progress_log = 1024 * 1024;
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let read_len = hcb_file.read(&mut chunk).map_err(|err| {
            println!("[UEFI] failed to read hcb: {:?}", err.status());
            err.status()
        })?;
        if read_len == 0 {
            break;
        }
        hcb_bytes.try_reserve(read_len).map_err(|_| {
            println!(
                "[UEFI] failed to reserve hcb bytes len={} add={}",
                hcb_bytes.len(),
                read_len
            );
            Status::OUT_OF_RESOURCES
        })?;
        hcb_bytes.extend_from_slice(&chunk[..read_len]);
        if hcb_bytes.len() >= next_progress_log {
            println!("[UEFI] read hcb: progress bytes={}", hcb_bytes.len());
            next_progress_log += 1024 * 1024;
        }
    }

    if hcb_bytes.is_empty() {
        println!("[UEFI] hcb is empty");
        Err(Status::LOAD_ERROR)
    } else {
        Ok(hcb_bytes)
    }
}

#[derive(Default)]
struct UefiInputBatch {
    events: Vec<RfvpOsEvent>,
    exit_requested: bool,
}

fn poll_input_events(
    pointer_state: &mut UefiPointerState,
    screen_width: i32,
    screen_height: i32,
    virtual_width: i32,
    virtual_height: i32,
) -> UefiInputBatch {
    let mut out = system::with_stdin(|stdin| {
        let mut out = UefiInputBatch::default();
        loop {
            match stdin.read_key() {
                Ok(Some(Key::Special(code))) => {
                    if code == ScanCode::ESCAPE {
                        out.exit_requested = true;
                        push_key_pulse(&mut out, RfvpOsKey::Esc);
                        break;
                    }
                    if let Some(key) = map_special_key(code) {
                        push_key_pulse(&mut out, key);
                    }
                }
                Ok(Some(Key::Printable(ch))) => {
                    if let Some(key) = map_printable_key(ch) {
                        push_key_pulse(&mut out, key);
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(_) => break,
            }
        }
        out
    });

    poll_absolute_pointer(pointer_state, screen_width, screen_height, virtual_width, virtual_height, &mut out);
    poll_simple_pointer(pointer_state, screen_width, screen_height, virtual_width, virtual_height, &mut out);
    out
}

fn map_special_key(code: ScanCode) -> Option<RfvpOsKey> {
    if code == ScanCode::LEFT {
        Some(RfvpOsKey::Left)
    } else if code == ScanCode::RIGHT {
        Some(RfvpOsKey::Right)
    } else if code == ScanCode::UP {
        Some(RfvpOsKey::Up)
    } else if code == ScanCode::DOWN {
        Some(RfvpOsKey::Down)
    } else if code == ScanCode::FUNCTION_1 {
        Some(RfvpOsKey::F1)
    } else if code == ScanCode::FUNCTION_2 {
        Some(RfvpOsKey::F2)
    } else if code == ScanCode::FUNCTION_3 {
        Some(RfvpOsKey::F3)
    } else if code == ScanCode::FUNCTION_4 {
        Some(RfvpOsKey::F4)
    } else if code == ScanCode::FUNCTION_5 {
        Some(RfvpOsKey::F5)
    } else if code == ScanCode::FUNCTION_6 {
        Some(RfvpOsKey::F6)
    } else if code == ScanCode::FUNCTION_7 {
        Some(RfvpOsKey::F7)
    } else if code == ScanCode::FUNCTION_8 {
        Some(RfvpOsKey::F8)
    } else if code == ScanCode::FUNCTION_9 {
        Some(RfvpOsKey::F9)
    } else if code == ScanCode::FUNCTION_10 {
        Some(RfvpOsKey::F10)
    } else if code == ScanCode::FUNCTION_11 {
        Some(RfvpOsKey::F11)
    } else if code == ScanCode::FUNCTION_12 {
        Some(RfvpOsKey::F12)
    } else {
        None
    }
}

fn map_printable_key(ch: Char16) -> Option<RfvpOsKey> {
    if char16_eq(ch, '\r') || char16_eq(ch, '\n') {
        Some(RfvpOsKey::Enter)
    } else if char16_eq(ch, ' ') {
        Some(RfvpOsKey::Space)
    } else if char16_eq(ch, '\t') {
        Some(RfvpOsKey::Tab)
    } else {
        None
    }
}

fn char16_eq(value: Char16, ch: char) -> bool {
    match Char16::try_from(ch) {
        Ok(expected) => value == expected,
        Err(_) => false,
    }
}

#[derive(Debug, Clone)]
struct UefiPointerState {
    x: i32,
    y: i32,
    left_down: bool,
    right_down: bool,
    simple_reset_done: bool,
    absolute_reset_done: bool,
    simple_protocol_detected_logged: bool,
    simple_protocol_missing_logged: bool,
    simple_open_error_logged: bool,
    absolute_protocol_detected_logged: bool,
    absolute_protocol_missing_logged: bool,
    absolute_open_error_logged: bool,
    absolute_mode_error_logged: bool,
    absolute_last_valid: bool,
    absolute_last_x: u64,
    absolute_last_y: u64,
    absolute_last_buttons: u32,
    samples_seen: usize,
    movement_logs: usize,
    idle_polls: usize,
    /// Cached simple-pointer handles.  Populated on first successful find;
    /// never re-queried unless empty (handle vec cleared on open error).
    simple_handles: Vec<Handle>,
    /// Cached absolute-pointer handles.  Same caching strategy.
    absolute_handles: Vec<Handle>,
}

impl UefiPointerState {
    fn new(screen_width: i32, screen_height: i32) -> Self {
        Self {
            x: screen_width.saturating_sub(1) / 2,
            y: screen_height.saturating_sub(1) / 2,
            left_down: false,
            right_down: false,
            simple_reset_done: false,
            absolute_reset_done: false,
            simple_protocol_detected_logged: false,
            simple_protocol_missing_logged: false,
            simple_open_error_logged: false,
            absolute_protocol_detected_logged: false,
            absolute_protocol_missing_logged: false,
            absolute_open_error_logged: false,
            absolute_mode_error_logged: false,
            absolute_last_valid: false,
            absolute_last_x: 0,
            absolute_last_y: 0,
            absolute_last_buttons: 0,
            samples_seen: 0,
            movement_logs: 0,
            idle_polls: 0,
            simple_handles: Vec::new(),
            absolute_handles: Vec::new(),
        }
    }

    fn set_screen_position(
        &mut self,
        x: i32,
        y: i32,
        screen_width: i32,
        screen_height: i32,
    ) -> bool {
        let max_x = screen_width.saturating_sub(1).max(0);
        let max_y = screen_height.saturating_sub(1).max(0);
        let new_x = x.clamp(0, max_x);
        let new_y = y.clamp(0, max_y);
        let changed = new_x != self.x || new_y != self.y;
        self.x = new_x;
        self.y = new_y;
        changed
    }

    fn move_relative(&mut self, dx: i32, dy: i32, screen_width: i32, screen_height: i32) -> bool {
        self.set_screen_position(
            self.x.saturating_add(dx),
            self.y.saturating_add(dy),
            screen_width,
            screen_height,
        )
    }

    fn log_pointer_event(&mut self, message: core::fmt::Arguments<'_>) {
        if self.movement_logs < POINTER_DIAGNOSTIC_LIMIT {
            println!("{}", message);
            self.movement_logs += 1;
        }
    }
}

/// Map a screen-space position (GOP pixels) to the game's virtual coordinate space,
/// applying the same letterbox-inverse transform used by the desktop event handler.
fn screen_to_virtual(
    sx: i32,
    sy: i32,
    screen_w: i32,
    screen_h: i32,
    virtual_w: i32,
    virtual_h: i32,
) -> (i32, i32, bool) {
    let sw = screen_w.max(1) as f64;
    let sh = screen_h.max(1) as f64;
    let vw = virtual_w.max(1) as f64;
    let vh = virtual_h.max(1) as f64;
    let scale = (sw / vw).min(sh / vh);
    let dst_w = vw * scale;
    let dst_h = vh * scale;
    let off_x = (sw - dst_w) * 0.5;
    let off_y = (sh - dst_h) * 0.5;
    let px = sx as f64;
    let py = sy as f64;
    let in_screen =
        px >= off_x && px < off_x + dst_w && py >= off_y && py < off_y + dst_h;
    let vx = ((px - off_x) / scale) as i32;
    let vy = ((py - off_y) / scale) as i32;
    let max_x = (vw as i32).saturating_sub(1);
    let max_y = (vh as i32).saturating_sub(1);
    (vx.clamp(0, max_x), vy.clamp(0, max_y), in_screen)
}

fn push_pointer_move(
    state: &UefiPointerState,
    out: &mut UefiInputBatch,
    screen_w: i32,
    screen_h: i32,
    virtual_w: i32,
    virtual_h: i32,
) {
    let (vx, vy, in_screen) =
        screen_to_virtual(state.x, state.y, screen_w, screen_h, virtual_w, virtual_h);
    out.events.push(RfvpOsEvent::PointerMove {
        x: vx,
        y: vy,
        in_screen,
    });
}

fn push_pointer_button_events(
    state: &mut UefiPointerState,
    left_down: bool,
    right_down: bool,
    out: &mut UefiInputBatch,
) -> bool {
    let mut changed = false;

    if left_down != state.left_down {
        state.left_down = left_down;
        state.log_pointer_event(format_args!("[UEFI][POINTER] left_down={}", left_down));
        out.events.push(if left_down {
            RfvpOsEvent::PointerDown {
                button: RfvpOsPointerButton::Left,
            }
        } else {
            RfvpOsEvent::PointerUp {
                button: RfvpOsPointerButton::Left,
            }
        });
        changed = true;
    }

    if right_down != state.right_down {
        state.right_down = right_down;
        state.log_pointer_event(format_args!("[UEFI][POINTER] right_down={}", right_down));
        out.events.push(if right_down {
            RfvpOsEvent::PointerDown {
                button: RfvpOsPointerButton::Right,
            }
        } else {
            RfvpOsEvent::PointerUp {
                button: RfvpOsPointerButton::Right,
            }
        });
        changed = true;
    }

    changed
}

fn poll_absolute_pointer(
    state: &mut UefiPointerState,
    screen_width: i32,
    screen_height: i32,
    virtual_width: i32,
    virtual_height: i32,
    out: &mut UefiInputBatch,
) {
    // Populate handle cache on first call; skip the UEFI syscall every subsequent frame.
    if state.absolute_handles.is_empty() {
        match boot::find_handles::<AbsolutePointer>() {
            Ok(h) => state.absolute_handles = h,
            Err(_) => {
                if !state.absolute_protocol_missing_logged {
                    println!("[UEFI][POINTER] EFI_ABSOLUTE_POINTER_PROTOCOL not found by poller");
                    state.absolute_protocol_missing_logged = true;
                }
                return;
            }
        }
    }

    if !state.absolute_protocol_detected_logged {
        println!(
            "[UEFI][POINTER] EFI_ABSOLUTE_POINTER_PROTOCOL handles={}",
            state.absolute_handles.len()
        );
        state.absolute_protocol_detected_logged = true;
    }

    let reset_absolute_handles = !state.absolute_reset_done;
    if reset_absolute_handles {
        println!(
            "[UEFI][POINTER] resetting all absolute handles count={}",
            state.absolute_handles.len()
        );
    }

    let mut saw_sample = false;
    for handle_index in 0..state.absolute_handles.len() {
        let handle = state.absolute_handles[handle_index];
        let Ok(mut pointer) = (unsafe {
            boot::open_protocol::<AbsolutePointer>(
                OpenProtocolParams {
                    handle,
                    agent: boot::image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
        }) else {
            if !state.absolute_open_error_logged {
                println!("[UEFI][POINTER] failed to open EFI_ABSOLUTE_POINTER_PROTOCOL with GetProtocol");
                state.absolute_open_error_logged = true;
            }
            continue;
        };

        let protocol = &mut pointer.0 as *mut RawAbsolutePointerProtocol;
        if protocol.is_null() {
            continue;
        }

        if reset_absolute_handles {
            let status = unsafe { (pointer.0.reset)(protocol, Boolean::FALSE) };
            println!(
                "[UEFI][POINTER] absolute handle={} reset status={:?}",
                handle_index, status
            );
        }

        let mode_ptr = pointer.0.mode;
        if mode_ptr.is_null() {
            if !state.absolute_mode_error_logged {
                println!("[UEFI][POINTER] absolute mode pointer is null");
                state.absolute_mode_error_logged = true;
            }
            continue;
        }
        let mode = unsafe { *mode_ptr };

        let mut sample = AbsolutePointerState::default();
        let status = unsafe {
            (pointer.0.get_state)(protocol as *const RawAbsolutePointerProtocol, &mut sample)
        };
        if status == RawStatus::NOT_READY {
            continue;
        }
        if status.is_error() {
            state.log_pointer_event(format_args!(
                "[UEFI][POINTER] absolute handle={} get_state status={:?}",
                handle_index, status
            ));
            continue;
        }

        saw_sample = true;
        let raw_buttons = sample.active_buttons;
        let raw_changed = !state.absolute_last_valid
            || sample.current_x != state.absolute_last_x
            || sample.current_y != state.absolute_last_y
            || raw_buttons != state.absolute_last_buttons;
        state.absolute_last_valid = true;
        state.absolute_last_x = sample.current_x;
        state.absolute_last_y = sample.current_y;
        state.absolute_last_buttons = raw_buttons;

        if !raw_changed {
            continue;
        }

        state.samples_seen = state.samples_seen.saturating_add(1);

        let (screen_x, screen_y) = map_absolute_to_screen(&mode, &sample, screen_width, screen_height);
        let moved = state.set_screen_position(screen_x, screen_y, screen_width, screen_height);
        if moved {
            let screen_x_now = state.x;
            let screen_y_now = state.y;
            state.log_pointer_event(format_args!(
                "[UEFI][POINTER] source=absolute handle={} raw=({}, {}) screen={}x{}",
                handle_index, sample.current_x, sample.current_y, screen_x_now, screen_y_now
            ));
        }
        // Always emit PointerMove so the engine has the current cursor position
        // even when only button state changed (moved == false).
        push_pointer_move(state, out, screen_width, screen_height, virtual_width, virtual_height);

        let left_down = (raw_buttons & 0x1) != 0;
        let right_down = (raw_buttons & 0x2) != 0;
        push_pointer_button_events(state, left_down, right_down, out);
    }

    if reset_absolute_handles {
        state.absolute_reset_done = true;
    }

    if !saw_sample {
        log_pointer_idle(state, "absolute-no-sample");
    }
}

fn map_absolute_to_screen(
    mode: &AbsolutePointerMode,
    sample: &AbsolutePointerState,
    screen_width: i32,
    screen_height: i32,
) -> (i32, i32) {
    let x = map_absolute_axis(
        sample.current_x,
        mode.absolute_min_x,
        mode.absolute_max_x,
        screen_width.saturating_sub(1).max(0) as u64,
    );
    let y = map_absolute_axis(
        sample.current_y,
        mode.absolute_min_y,
        mode.absolute_max_y,
        screen_height.saturating_sub(1).max(0) as u64,
    );
    (x as i32, y as i32)
}

fn map_absolute_axis(value: u64, min: u64, max: u64, dst_max: u64) -> u64 {
    if dst_max == 0 || max <= min {
        return 0;
    }
    let clamped = value.clamp(min, max).saturating_sub(min);
    let range = max - min;
    ((clamped as u128 * dst_max as u128 + (range as u128 / 2)) / range as u128) as u64
}

fn poll_simple_pointer(
    state: &mut UefiPointerState,
    screen_width: i32,
    screen_height: i32,
    virtual_width: i32,
    virtual_height: i32,
    out: &mut UefiInputBatch,
) {
    // Populate handle cache on first call; skip the UEFI syscall every subsequent frame.
    if state.simple_handles.is_empty() {
        match boot::find_handles::<Pointer>() {
            Ok(h) => state.simple_handles = h,
            Err(_) => {
                if !state.simple_protocol_missing_logged {
                    println!("[UEFI][POINTER] EFI_SIMPLE_POINTER_PROTOCOL not found by poller");
                    state.simple_protocol_missing_logged = true;
                }
                return;
            }
        }
    }

    if !state.simple_protocol_detected_logged {
        println!(
            "[UEFI][POINTER] EFI_SIMPLE_POINTER_PROTOCOL handles={}",
            state.simple_handles.len()
        );
        state.simple_protocol_detected_logged = true;
    }

    let reset_simple_handles = !state.simple_reset_done;
    if reset_simple_handles {
        println!(
            "[UEFI][POINTER] resetting all simple handles count={}",
            state.simple_handles.len()
        );
    }

    let mut saw_sample = false;
    for handle_index in 0..state.simple_handles.len() {
        let handle = state.simple_handles[handle_index];
        let Ok(mut pointer) = (unsafe {
            boot::open_protocol::<Pointer>(
                OpenProtocolParams {
                    handle,
                    agent: boot::image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
        }) else {
            if !state.simple_open_error_logged {
                println!("[UEFI][POINTER] failed to open EFI_SIMPLE_POINTER_PROTOCOL with GetProtocol");
                state.simple_open_error_logged = true;
            }
            continue;
        };

        if reset_simple_handles {
            match pointer.reset(false) {
                Ok(()) => println!("[UEFI][POINTER] simple handle={} reset ok", handle_index),
                Err(err) => println!(
                    "[UEFI][POINTER] simple handle={} reset failed: {:?}",
                    handle_index,
                    err.status()
                ),
            }
        }

        loop {
            let pointer_sample = match pointer.read_state() {
                Ok(Some(sample)) => sample,
                Ok(None) => break,
                Err(err) => {
                    state.log_pointer_event(format_args!(
                        "[UEFI][POINTER] simple handle={} read_state failed: {:?}",
                        handle_index,
                        err.status()
                    ));
                    break;
                }
            };

            saw_sample = true;
            state.samples_seen = state.samples_seen.saturating_add(1);

            let dx = pointer_sample.relative_movement[0];
            let dy = pointer_sample.relative_movement[1];
            let dz = pointer_sample.relative_movement[2];

            if dx != 0 || dy != 0 {
                let moved = state.move_relative(dx, dy, screen_width, screen_height);
                let screen_x_now = state.x;
                let screen_y_now = state.y;
                state.log_pointer_event(format_args!(
                    "[UEFI][POINTER] source=simple handle={} dx={} dy={} dz={} screen={}x{}",
                    handle_index, dx, dy, dz, screen_x_now, screen_y_now
                ));
                if moved {
                    push_pointer_move(state, out, screen_width, screen_height, virtual_width, virtual_height);
                }
            }

            let left_down = pointer_sample.button[0];
            let right_down = pointer_sample.button[1];
            push_pointer_button_events(state, left_down, right_down, out);

            if dz != 0 {
                out.events.push(RfvpOsEvent::Wheel { delta: dz });
            }
        }
    }

    if reset_simple_handles {
        state.simple_reset_done = true;
    }

    if !saw_sample {
        log_pointer_idle(state, "simple-no-sample");
    }
}

fn log_pointer_idle(state: &mut UefiPointerState, reason: &str) {
    if !POINTER_IDLE_DIAGNOSTICS_ENABLED {
        return;
    }
    state.idle_polls = state.idle_polls.saturating_add(1);
    if state.idle_polls % POINTER_IDLE_DIAGNOSTIC_INTERVAL == 0 {
        println!(
            "[UEFI][POINTER] idle reason={} polls={} samples_seen={} screen={}x{}",
            reason, state.idle_polls, state.samples_seen, state.x, state.y
        );
    }
}

fn push_key_pulse(out: &mut UefiInputBatch, key: RfvpOsKey) {
    out.events.push(RfvpOsEvent::KeyDown { key, repeat: false });
    out.events.push(RfvpOsEvent::KeyUp { key });
}

fn present_framebuffer(
    gop: &mut GraphicsOutput,
    framebuffer: &SoftFramebuffer,
    frame_index: usize,
    pointer_state: Option<&UefiPointerState>,
) -> Result<(), Status> {
    let info = gop.current_mode_info();
    match info.pixel_format() {
        GopPixelFormat::Rgb | GopPixelFormat::Bgr | GopPixelFormat::Bitmask => {
            present_direct(gop, framebuffer, frame_index, pointer_state)
        }
        GopPixelFormat::BltOnly => {
            println!("GOP BltOnly mode is not supported by the RFVP direct framebuffer presenter.");
            Err(Status::UNSUPPORTED)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PresenterLayout {
    screen_width: usize,
    screen_height: usize,
    src_width: usize,
    src_height: usize,
    dst_width: usize,
    dst_height: usize,
    offset_x: usize,
    offset_y: usize,
}

fn compute_presenter_layout(
    screen_width: usize,
    screen_height: usize,
    src_width: usize,
    src_height: usize,
) -> PresenterLayout {
    let scale_w_num = screen_width.max(1);
    let scale_w_den = src_width.max(1);
    let scale_h_num = screen_height.max(1);
    let scale_h_den = src_height.max(1);

    let use_width_scale = scale_w_num.saturating_mul(scale_h_den)
        <= scale_h_num.saturating_mul(scale_w_den);

    let (dst_width, dst_height) = if use_width_scale {
        let dst_width = screen_width;
        let dst_height = src_height
            .saturating_mul(dst_width)
            .checked_div(src_width.max(1))
            .unwrap_or(1)
            .max(1)
            .min(screen_height.max(1));
        (dst_width, dst_height)
    } else {
        let dst_height = screen_height;
        let dst_width = src_width
            .saturating_mul(dst_height)
            .checked_div(src_height.max(1))
            .unwrap_or(1)
            .max(1)
            .min(screen_width.max(1));
        (dst_width, dst_height)
    };

    PresenterLayout {
        screen_width,
        screen_height,
        src_width,
        src_height,
        dst_width,
        dst_height,
        offset_x: screen_width.saturating_sub(dst_width) / 2,
        offset_y: screen_height.saturating_sub(dst_height) / 2,
    }
}

fn present_direct(
    gop: &mut GraphicsOutput,
    framebuffer: &SoftFramebuffer,
    frame_index: usize,
    pointer_state: Option<&UefiPointerState>,
) -> Result<(), Status> {
    let info = gop.current_mode_info();
    let (screen_width, screen_height) = info.resolution();
    let screen_stride = info.stride();
    let format = info.pixel_format();
    let bitmask = info.pixel_bitmask();
    let mut screen = gop.frame_buffer();

    let src_width = framebuffer.width().max(1) as usize;
    let src_height = framebuffer.height().max(1) as usize;
    let src_stride = framebuffer.stride() as usize;
    let pixels = framebuffer.pixels();
    let layout = compute_presenter_layout(screen_width, screen_height, src_width, src_height);

    if should_log_frame_diagnostics(frame_index) || frame_index == usize::MAX {
        println!(
            "[UEFI] presenter frame={} gop={}x{} stride={} format={:?} bitmask={:?} src={}x{} dst={}x{} offset={}x{}",
            frame_index,
            screen_width,
            screen_height,
            screen_stride,
            format,
            bitmask,
            src_width,
            src_height,
            layout.dst_width,
            layout.dst_height,
            layout.offset_x,
            layout.offset_y
        );
    }

    if frame_index == 0 || frame_index == usize::MAX {
        clear_letterbox(&mut screen, screen_stride, format, bitmask, layout);
    }

    if layout.dst_width == src_width && layout.dst_height == src_height {
        present_no_scale(
            &mut screen,
            screen_stride,
            format,
            bitmask,
            framebuffer.format(),
            pixels,
            src_stride,
            layout,
        );
    } else {
        present_scaled(
            &mut screen,
            screen_stride,
            format,
            bitmask,
            framebuffer.format(),
            pixels,
            src_stride,
            layout,
        );
    }

    if let Some(pointer) = pointer_state {
        draw_cursor_overlay(
            &mut screen,
            screen_stride,
            format,
            bitmask,
            layout.screen_width,
            layout.screen_height,
            pointer.x,
            pointer.y,
        );
    }

    Ok(())
}

fn clear_letterbox(
    screen: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    screen_stride: usize,
    format: GopPixelFormat,
    bitmask: Option<PixelBitmask>,
    layout: PresenterLayout,
) {
    if layout.offset_y > 0 {
        clear_rect(screen, screen_stride, format, bitmask, 0, 0, layout.screen_width, layout.offset_y);
        clear_rect(
            screen,
            screen_stride,
            format,
            bitmask,
            0,
            layout.offset_y + layout.dst_height,
            layout.screen_width,
            layout.screen_height.saturating_sub(layout.offset_y + layout.dst_height),
        );
    }

    if layout.offset_x > 0 {
        clear_rect(
            screen,
            screen_stride,
            format,
            bitmask,
            0,
            layout.offset_y,
            layout.offset_x,
            layout.dst_height,
        );
        clear_rect(
            screen,
            screen_stride,
            format,
            bitmask,
            layout.offset_x + layout.dst_width,
            layout.offset_y,
            layout.screen_width.saturating_sub(layout.offset_x + layout.dst_width),
            layout.dst_height,
        );
    }
}

fn clear_rect(
    screen: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    screen_stride: usize,
    format: GopPixelFormat,
    bitmask: Option<PixelBitmask>,
    x0: usize,
    y0: usize,
    width: usize,
    height: usize,
) {
    if width == 0 || height == 0 {
        return;
    }
    for y in y0..y0.saturating_add(height) {
        for x in x0..x0.saturating_add(width) {
            write_gop_pixel(screen, screen_stride, x, y, format, bitmask, 0, 0, 0);
        }
    }
}

fn present_no_scale(
    screen: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    screen_stride: usize,
    format: GopPixelFormat,
    bitmask: Option<PixelBitmask>,
    src_format: PixelFormat,
    pixels: &[u8],
    src_stride: usize,
    layout: PresenterLayout,
) {
    // Direct copy: RGBA8→RGB or BGRA8→BGR — byte layout is [C0,C1,C2,?] in both
    // source and GOP, so we can bulk-copy each row without touching individual pixels.
    let direct_copy = matches!(
        (format, src_format),
        (GopPixelFormat::Rgb, PixelFormat::Rgba8) | (GopPixelFormat::Bgr, PixelFormat::Bgra8)
    );
    if direct_copy {
        let copy_bytes = layout.dst_width * 4;
        for y in 0..layout.dst_height {
            let src_off = y * src_stride;
            if src_off + copy_bytes > pixels.len() {
                break;
            }
            let dst_off = ((layout.offset_y + y) * screen_stride + layout.offset_x) * 4;
            // Safety: src and dst are non-overlapping (separate allocations).
            // The GOP framebuffer is write-combining or WB memory; copy_nonoverlapping
            // (= memcpy) is correct and far faster than per-pixel write_volatile loops.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    pixels.as_ptr().add(src_off),
                    screen.as_mut_ptr().add(dst_off),
                    copy_bytes,
                );
            }
        }
        return;
    }

    // Channel-swap: RGBA8→BGR or BGRA8→RGB. Use row buffer for bulk MMIO write.
    let swap_rb = matches!(
        (format, src_format),
        (GopPixelFormat::Bgr, PixelFormat::Rgba8) | (GopPixelFormat::Rgb, PixelFormat::Bgra8)
    );
    if swap_rb {
        let mut row_buf: Vec<u32> = vec![0u32; layout.dst_width];
        for y in 0..layout.dst_height {
            let src_row = y * src_stride;
            let dst_off = ((layout.offset_y + y) * screen_stride + layout.offset_x) * 4;
            let mut len = layout.dst_width;
            for x in 0..layout.dst_width {
                let s = src_row + x * 4;
                if s + 2 >= pixels.len() {
                    len = x;
                    break;
                }
                row_buf[x] = u32::from_le_bytes([pixels[s + 2], pixels[s + 1], pixels[s], 0]);
            }
            unsafe {
                core::ptr::copy_nonoverlapping(
                    row_buf.as_ptr() as *const u8,
                    screen.as_mut_ptr().add(dst_off),
                    len * 4,
                );
            }
        }
        return;
    }

    // Bitmask slow path (rare; pull decode_source_rgb inline).
    if let (GopPixelFormat::Bitmask, Some(mask)) = (format, bitmask) {
        for y in 0..layout.dst_height {
            let src_row = y * src_stride;
            let dst_y = layout.offset_y + y;
            for x in 0..layout.dst_width {
                let src = src_row + x * 4;
                if src + 2 >= pixels.len() {
                    break;
                }
                let (r, g, b) = decode_source_rgb(src_format, pixels, src);
                write_gop_pixel(screen, screen_stride, layout.offset_x + x, dst_y, format, Some(mask), r, g, b);
            }
        }
    }
}

fn present_scaled(
    screen: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    screen_stride: usize,
    format: GopPixelFormat,
    bitmask: Option<PixelBitmask>,
    src_format: PixelFormat,
    pixels: &[u8],
    src_stride: usize,
    layout: PresenterLayout,
) {
    let dst_w = layout.dst_width.max(1);
    let dst_h = layout.dst_height.max(1);
    let src_w = layout.src_width.max(1);
    let src_h = layout.src_height.max(1);

    // Precompute x-to-source-x mapping to eliminate division from the inner loop.
    let x_map: Vec<usize> = (0..dst_w)
        .map(|x| (x * src_w / dst_w).min(src_w - 1))
        .collect();

    // Pull format dispatch entirely outside the loops.
    let direct_copy = matches!(
        (format, src_format),
        (GopPixelFormat::Rgb, PixelFormat::Rgba8) | (GopPixelFormat::Bgr, PixelFormat::Bgra8)
    );
    let swap_rb = !direct_copy && matches!(
        (format, src_format),
        (GopPixelFormat::Bgr, PixelFormat::Rgba8) | (GopPixelFormat::Rgb, PixelFormat::Bgra8)
    );

    // Row accumulation buffer: pixels are assembled here (fast normal RAM stores),
    // then bulk-copied to the GOP framebuffer (MMIO / write-combining) once per row.
    // This reduces MMIO write_volatile calls from O(width×height) to O(height),
    // which is significantly faster on write-combining framebuffer memory.
    let mut row_buf: Vec<u32> = vec![0u32; dst_w];

    for y in 0..dst_h {
        let src_y = (y * src_h / dst_h).min(src_h - 1);
        let src_row = src_y * src_stride;
        let dst_off = ((layout.offset_y + y) * screen_stride + layout.offset_x) * 4;

        if direct_copy {
            let mut len = dst_w;
            for x in 0..dst_w {
                let s = src_row + x_map[x] * 4;
                if s + 3 > pixels.len() {
                    len = x;
                    break;
                }
                row_buf[x] = u32::from_le_bytes([pixels[s], pixels[s + 1], pixels[s + 2], 0]);
            }
            unsafe {
                core::ptr::copy_nonoverlapping(
                    row_buf.as_ptr() as *const u8,
                    screen.as_mut_ptr().add(dst_off),
                    len * 4,
                );
            }
        } else if swap_rb {
            let mut len = dst_w;
            for x in 0..dst_w {
                let s = src_row + x_map[x] * 4;
                if s + 2 >= pixels.len() {
                    len = x;
                    break;
                }
                row_buf[x] = u32::from_le_bytes([pixels[s + 2], pixels[s + 1], pixels[s], 0]);
            }
            unsafe {
                core::ptr::copy_nonoverlapping(
                    row_buf.as_ptr() as *const u8,
                    screen.as_mut_ptr().add(dst_off),
                    len * 4,
                );
            }
        } else if let (GopPixelFormat::Bitmask, Some(mask)) = (format, bitmask) {
            let mut len = dst_w;
            for x in 0..dst_w {
                let src = src_row + x_map[x] * 4;
                if src + 2 >= pixels.len() {
                    len = x;
                    break;
                }
                let (r, g, b) = decode_source_rgb(src_format, pixels, src);
                row_buf[x] = pack_bitmask(mask, r, g, b);
            }
            unsafe {
                core::ptr::copy_nonoverlapping(
                    row_buf.as_ptr() as *const u8,
                    screen.as_mut_ptr().add(dst_off),
                    len * 4,
                );
            }
        }
    }
}

fn decode_source_rgb(format: PixelFormat, pixels: &[u8], offset: usize) -> (u8, u8, u8) {
    match format {
        PixelFormat::Rgba8 => (pixels[offset], pixels[offset + 1], pixels[offset + 2]),
        PixelFormat::Bgra8 => (pixels[offset + 2], pixels[offset + 1], pixels[offset]),
    }
}

fn draw_cursor_overlay(
    screen: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    screen_stride: usize,
    format: GopPixelFormat,
    bitmask: Option<PixelBitmask>,
    screen_width: usize,
    screen_height: usize,
    x: i32,
    y: i32,
) {
    const CURSOR: [&str; 18] = [
        "B.................",
        "BB................",
        "BWB...............",
        "BWWB..............",
        "BWWWB.............",
        "BWWWWB............",
        "BWWWWWB...........",
        "BWWWWWWB..........",
        "BWWWWWWWB.........",
        "BWWWWWWWWB........",
        "BWWWWBBBBB........",
        "BWWBWB............",
        "BWB.BWB...........",
        "BB..BWB...........",
        "B....BWB..........",
        ".....BWB..........",
        "......BWB.........",
        "......BBB.........",
    ];

    for (cy, row) in CURSOR.iter().enumerate() {
        for (cx, ch) in row.as_bytes().iter().enumerate() {
            let (r, g, b) = match *ch {
                b'B' => (0, 0, 0),
                b'W' => (255, 255, 255),
                _ => continue,
            };
            let px = x.saturating_add(cx as i32);
            let py = y.saturating_add(cy as i32);
            if px < 0 || py < 0 {
                continue;
            }
            let px = px as usize;
            let py = py as usize;
            if px >= screen_width || py >= screen_height {
                continue;
            }
            write_gop_pixel(
                screen,
                screen_stride,
                px,
                py,
                format,
                bitmask,
                r,
                g,
                b,
            );
        }
    }
}

fn should_log_frame_diagnostics(frame_index: usize) -> bool {
    FRAMEBUFFER_DIAGNOSTICS_ENABLED
        && (frame_index < FRAMEBUFFER_DIAGNOSTIC_FRAMES
            || frame_index % FRAMEBUFFER_DIAGNOSTIC_INTERVAL == 0)
}

fn log_framebuffer_diagnostics(frame_index: usize, framebuffer: &SoftFramebuffer) {
    let pixels = framebuffer.pixels();
    let stride = framebuffer.stride() as usize;
    let width = framebuffer.width() as usize;
    let height = framebuffer.height() as usize;
    let mut first = [[0u8; 4]; 16];
    let mut copied = 0usize;
    for y in 0..height {
        let row = y.saturating_mul(stride);
        for x in 0..width {
            let off = row + x.saturating_mul(4);
            if off + 3 >= pixels.len() {
                break;
            }
            if copied < first.len() {
                first[copied].copy_from_slice(&pixels[off..off + 4]);
                copied += 1;
            }
            if copied >= first.len() {
                break;
            }
        }
        if copied >= first.len() {
            break;
        }
    }

    let mut sampled_pixels = 0usize;
    let mut non_black = 0usize;
    let mut non_zero_alpha = 0usize;
    for y in 0..height {
        let row = y.saturating_mul(stride);
        for x in 0..width {
            let off = row + x.saturating_mul(4);
            if off + 3 >= pixels.len() {
                break;
            }
            let px = &pixels[off..off + 4];
            let (r, g, b, a) = match framebuffer.format() {
                PixelFormat::Rgba8 => (px[0], px[1], px[2], px[3]),
                PixelFormat::Bgra8 => (px[2], px[1], px[0], px[3]),
            };
            sampled_pixels += 1;
            if r != 0 || g != 0 || b != 0 {
                non_black += 1;
            }
            if a != 0 {
                non_zero_alpha += 1;
            }
        }
    }

    println!(
        "[UEFI] framebuffer frame={} width={} height={} stride={} format={:?} bytes={} first16={:?} sampled={} non_black={} non_zero_alpha={} checksum={:016x}",
        frame_index,
        framebuffer.width(),
        framebuffer.height(),
        framebuffer.stride(),
        framebuffer.format(),
        pixels.len(),
        first,
        sampled_pixels,
        non_black,
        non_zero_alpha,
        checksum_bytes(pixels)
    );
}

fn checksum_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn make_presenter_test_pattern(width: u32, height: u32) -> Result<SoftFramebuffer, Status> {
    let mut fb = SoftFramebuffer::new(width, height, PixelFormat::Rgba8).map_err(|err| {
        println!(
            "[UEFI] failed to allocate presenter test framebuffer: {:?}",
            err
        );
        Status::OUT_OF_RESOURCES
    })?;
    fb.clear_rgba(255, 255, 255, 255);

    let w = fb.width() as usize;
    let h = fb.height() as usize;
    let stride = fb.stride() as usize;
    let pixels = fb.pixels_mut();

    for y in 0..h {
        for x in 0..w {
            let color = if x < w / 4 && y < h / 4 {
                [255, 0, 0, 255]
            } else if x >= w.saturating_mul(3) / 4 && y < h / 4 {
                [0, 255, 0, 255]
            } else if x < w / 4 && y >= h.saturating_mul(3) / 4 {
                [0, 0, 255, 255]
            } else if x >= w.saturating_mul(7) / 16
                && x < w.saturating_mul(9) / 16
                && y >= h.saturating_mul(7) / 16
                && y < h.saturating_mul(9) / 16
            {
                [0, 0, 0, 255]
            } else {
                continue;
            };
            let off = y.saturating_mul(stride) + x.saturating_mul(4);
            if off + 3 < pixels.len() {
                pixels[off..off + 4].copy_from_slice(&color);
            }
        }
    }

    Ok(fb)
}

#[inline(always)]
fn write_gop_pixel(
    fb: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    stride: usize,
    x: usize,
    y: usize,
    format: GopPixelFormat,
    bitmask: Option<PixelBitmask>,
    r: u8,
    g: u8,
    b: u8,
) {
    let offset = (y * stride + x) * 4;
    let bytes = match format {
        GopPixelFormat::Rgb => [r, g, b, 0],
        GopPixelFormat::Bgr => [b, g, r, 0],
        GopPixelFormat::Bitmask => {
            let Some(mask) = bitmask else {
                return;
            };
            let value = pack_bitmask(mask, r, g, b);
            [
                value as u8,
                (value >> 8) as u8,
                (value >> 16) as u8,
                (value >> 24) as u8,
            ]
        }
        GopPixelFormat::BltOnly => return,
    };
    unsafe {
        core::ptr::write_volatile(fb.as_mut_ptr().add(offset) as *mut u32, u32::from_le_bytes(bytes));
    }
}

fn pack_bitmask(mask: PixelBitmask, r: u8, g: u8, b: u8) -> u32 {
    pack_channel(r, mask.red) | pack_channel(g, mask.green) | pack_channel(b, mask.blue)
}

fn pack_channel(value: u8, mask: u32) -> u32 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let bits = mask.count_ones();
    let max = if bits >= 32 {
        u32::MAX
    } else {
        (1u32 << bits) - 1
    };
    (((value as u32 * max + 127) / 255) << shift) & mask
}

// ─── UEFI big-stack implementation (aarch64 + x86_64) ────────────────────────
//
// The firmware-provided stack on UEFI is typically ≤128 KiB.  Video decode
// (MPEG-2 / WMV2) enters functions whose combined local frames exceed that
// limit.  LLVM emits __chkstk probes for large frames; when the probe address
// is unmapped the CPU faults.
//
// Fix: allocate 16 MiB via UEFI BootServices, switch SP into it with a tiny
// assembly trampoline, and run the entire rfvp runtime there.  The allocation
// is intentionally leaked; UEFI reclaims all LOADER_DATA pages on app exit.

#[cfg(all(target_os = "uefi", any(target_arch = "aarch64", target_arch = "x86_64")))]
const BIG_STACK_SIZE: usize = 16 * 1024 * 1024;

#[cfg(all(target_os = "uefi", any(target_arch = "aarch64", target_arch = "x86_64")))]
const BIG_STACK_PAGES: usize = BIG_STACK_SIZE / 4096;

#[cfg(all(target_os = "uefi", any(target_arch = "aarch64", target_arch = "x86_64")))]
struct BigStackResult {
    value: Option<Result<(), Status>>,
}

/// Entry point invoked on the big stack.  Calls `run_initialized()` and
/// stores the result through `arg`.
///
/// `extern "efiapi"` maps to AAPCS64 on aarch64 and Win64 on x86_64,
/// matching the respective trampoline calling conventions.
#[cfg(all(target_os = "uefi", any(target_arch = "aarch64", target_arch = "x86_64")))]
unsafe extern "efiapi" fn big_stack_entry(arg: *mut ()) {
    let out = &mut *(arg as *mut BigStackResult);
    out.value = Some(run_initialized());
}

/// Allocate a 16 MiB UEFI page-backed stack, switch SP to it, and run the
/// rfvp runtime.  Called from `main_impl()` after `uefi::helpers::init()`.
#[cfg(all(target_os = "uefi", any(target_arch = "aarch64", target_arch = "x86_64")))]
unsafe fn run_on_big_stack() -> Result<(), Status> {
    let ptr = boot::allocate_pages(
        boot::AllocateType::AnyPages,
        boot::MemoryType::LOADER_DATA,
        BIG_STACK_PAGES,
    )
    .map_err(|e| e.status())?;

    let stack_base = ptr.as_ptr() as usize;
    // UEFI pages are 4 KiB-aligned so the top is naturally 16-byte aligned.
    let stack_top = (stack_base + BIG_STACK_SIZE) as *mut u8;

    println!(
        "[UEFI] big stack base=0x{:016x} size=0x{:x} top=0x{:016x}",
        stack_base, BIG_STACK_SIZE, stack_top as usize,
    );
    println!("[UEFI] entering rfvp runtime on big stack");

    let mut result = BigStackResult { value: None };
    let arg = &mut result as *mut BigStackResult as *mut ();

    #[cfg(target_arch = "aarch64")]
    rfvp_aarch64_stack_switch(stack_top, big_stack_entry, arg);

    #[cfg(target_arch = "x86_64")]
    rfvp_x86_64_stack_switch(stack_top, big_stack_entry, arg);

    // Intentionally do NOT free ptr: the allocation must remain valid for the
    // entire duration of the call above.  UEFI reclaims all LOADER_DATA pages
    // when the application exits.

    match result.value {
        Some(r) => r,
        None => {
            println!("[UEFI] big-stack entry did not produce a result");
            Err(Status::LOAD_ERROR)
        }
    }
}

// ─── AArch64 stack-switch trampoline ─────────────────────────────────────────
//
// AAPCS64 (= extern "efiapi" on aarch64):
//   x0 = new_stack_top  – top of the new stack (must be 16-byte aligned)
//   x1 = func           – `unsafe extern "efiapi" fn(*mut ())`
//   x2 = arg            – forwarded to func as its sole argument
#[cfg(all(target_os = "uefi", target_arch = "aarch64"))]
core::arch::global_asm!(
    r#"
.globl rfvp_aarch64_stack_switch
rfvp_aarch64_stack_switch:
    sub  sp,  sp,  #32
    str  x19, [sp, #0]
    stp  x29, x30, [sp, #16]
    add  x29, sp,  #16
    mov  x19, sp
    mov  sp,  x0
    mov  x0,  x2
    blr  x1
    mov  sp,  x19
    ldr  x19, [sp, #0]
    ldp  x29, x30, [sp, #16]
    add  sp,  sp,  #32
    ret
"#
);

#[cfg(all(target_os = "uefi", target_arch = "aarch64"))]
extern "efiapi" {
    fn rfvp_aarch64_stack_switch(
        new_stack_top: *mut u8,
        func: unsafe extern "efiapi" fn(*mut ()),
        arg: *mut (),
    );
}

// ─── x86_64 stack-switch trampoline ──────────────────────────────────────────
//
// Microsoft x64 / Win64 (= extern "efiapi" on x86_64):
//   RCX = new_stack_top  – top of the new stack (16-byte aligned)
//   RDX = func           – `unsafe extern "efiapi" fn(*mut ())`
//   R8  = arg            – forwarded to func as its sole argument (→ RCX)
//
// RSP alignment: Win64 requires RSP % 16 == 0 before a CALL instruction.
// At entry to this trampoline RSP % 16 == 8 (return address already pushed).
// After `push rbx` RSP is 16-byte aligned.  We preserve that alignment when
// we switch to the new stack (page-aligned) and reserve 32 bytes of shadow
// space, keeping RSP % 16 == 0 at the point of `call rdx`.
#[cfg(all(target_os = "uefi", target_arch = "x86_64"))]
core::arch::global_asm!(
    r#"
.globl rfvp_x86_64_stack_switch
rfvp_x86_64_stack_switch:
    push rbx
    mov  rbx, rsp
    mov  rsp, rcx
    mov  rcx, r8
    sub  rsp, 32
    call rdx
    mov  rsp, rbx
    pop  rbx
    ret
"#
);

#[cfg(all(target_os = "uefi", target_arch = "x86_64"))]
extern "efiapi" {
    fn rfvp_x86_64_stack_switch(
        new_stack_top: *mut u8,
        func: unsafe extern "efiapi" fn(*mut ()),
        arg: *mut (),
    );
}
