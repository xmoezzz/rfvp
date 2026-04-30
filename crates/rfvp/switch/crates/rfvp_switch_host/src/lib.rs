#![allow(unexpected_cfgs)]
#![cfg(any(target_os = "horizon", target_vendor = "nintendo", rfvp_switch))]
#![no_std]

pub use rfvp_switch_audio as audio;
pub use rfvp_switch_core_abi as core_abi;
pub use rfvp_switch_render as render;

use audio::SwitchAudioBackend;
use core::cell::UnsafeCell;
use core::ffi::{c_char, c_void};
use core::ptr::null_mut;
use core_abi::{RfvpSwitchCoreStats, RfvpSwitchInputFrame, RFVP_SWITCH_CORE_ABI_VERSION};
use render::{ColorF32, RenderCommand, SwitchRenderer, RFVP_SWITCH_RENDER_API_VERSION};

pub const RFVP_SWITCH_HOST_API_VERSION: u32 = 2;

#[inline]
const fn empty_core_stats() -> RfvpSwitchCoreStats {
    RfvpSwitchCoreStats {
        abi_version: RFVP_SWITCH_CORE_ABI_VERSION,
        frame_no: 0,
        last_status: 0,
        forced_yield: 0,
        forced_yield_contexts: 0,
        main_thread_exited: 0,
        game_should_exit: 0,
    }
}

pub struct SwitchHost {
    pub renderer: SwitchRenderer,
    pub audio: SwitchAudioBackend,
    pub frame_no: u64,
    pub core: *mut c_void,
    pub core_status: i32,
    pub core_stats: RfvpSwitchCoreStats,
}

impl SwitchHost {
    pub const fn new() -> Self {
        Self {
            renderer: SwitchRenderer::new(),
            audio: SwitchAudioBackend::new(),
            frame_no: 0,
            core: null_mut(),
            core_status: 0,
            core_stats: empty_core_stats(),
        }
    }

    pub fn begin_frame(&mut self) {
        let _ = self.renderer.begin_frame(ColorF32 {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        });
    }

    pub fn end_frame(&mut self) {
        let _ = self.renderer.end_frame();
        self.frame_no = self.frame_no.wrapping_add(1);
    }

    #[cfg(feature = "rfvp-core-link")]
    pub unsafe fn load_game_root(
        &mut self,
        game_root: *const c_char,
        nls: *const c_char,
        width: u32,
        height: u32,
    ) -> i32 {
        if !self.core.is_null() {
            rfvp_switch_core_destroy(self.core);
            self.core = null_mut();
        }

        let core = rfvp_switch_core_create(game_root, nls, width, height);
        if core.is_null() {
            self.core_status = -10;
            return self.core_status;
        }

        self.core = core;
        self.core_status = 0;
        self.refresh_core_stats();
        0
    }

    #[cfg(not(feature = "rfvp-core-link"))]
    pub unsafe fn load_game_root(
        &mut self,
        _game_root: *const c_char,
        _nls: *const c_char,
        _width: u32,
        _height: u32,
    ) -> i32 {
        self.core_status = -20;
        self.core_status
    }

    pub fn tick(&mut self, frame_time_ms: u32, input: &RfvpSwitchInputFrame) -> i32 {
        self.begin_frame();

        #[cfg(feature = "rfvp-core-link")]
        {
            if !self.core.is_null() {
                self.core_status = unsafe { rfvp_switch_core_tick(self.core, frame_time_ms, input as *const _) };
                self.refresh_core_stats();
            }
        }

        #[cfg(not(feature = "rfvp-core-link"))]
        {
            let _ = (frame_time_ms, input);
            self.core_status = -20;
        }

        self.end_frame();
        self.core_status
    }

    #[cfg(feature = "rfvp-core-link")]
    fn refresh_core_stats(&mut self) {
        if self.core.is_null() {
            self.core_stats = empty_core_stats();
            return;
        }
        let mut stats = empty_core_stats();
        let rc = unsafe { rfvp_switch_core_stats(self.core, &mut stats as *mut _) };
        if rc == 0 {
            self.core_stats = stats;
        } else {
            self.core_status = rc;
        }
    }

    #[cfg(not(feature = "rfvp-core-link"))]
    fn refresh_core_stats(&mut self) {
        self.core_stats = empty_core_stats();
    }

    pub unsafe fn destroy_core(&mut self) {
        #[cfg(feature = "rfvp-core-link")]
        {
            if !self.core.is_null() {
                rfvp_switch_core_destroy(self.core);
                self.core = null_mut();
            }
        }
        #[cfg(not(feature = "rfvp-core-link"))]
        {
            self.core = null_mut();
        }
        self.core_status = 0;
        self.core_stats = empty_core_stats();
    }
}

impl Default for SwitchHost {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "rfvp-core-link")]
unsafe extern "C" {
    fn rfvp_switch_core_create(
        game_root_utf8: *const c_char,
        nls_utf8: *const c_char,
        width: u32,
        height: u32,
    ) -> *mut c_void;
    fn rfvp_switch_core_tick(
        core: *mut c_void,
        frame_time_ms: u32,
        input: *const RfvpSwitchInputFrame,
    ) -> i32;
    fn rfvp_switch_core_stats(core: *const c_void, out: *mut RfvpSwitchCoreStats) -> i32;
    fn rfvp_switch_core_destroy(core: *mut c_void);
    fn rfvp_switch_core_render_command_count(core: *const c_void) -> usize;
    fn rfvp_switch_core_render_commands(core: *const c_void) -> *const RenderCommand;
    fn rfvp_switch_core_audio_queued_samples(core: *const c_void) -> usize;
    fn rfvp_switch_core_audio_pop_i16(core: *const c_void, out: *mut i16, len: usize) -> usize;
}

#[cfg(feature = "ffi")]
struct GlobalHost(UnsafeCell<SwitchHost>);

#[cfg(feature = "ffi")]
unsafe impl Sync for GlobalHost {}

#[cfg(feature = "ffi")]
static GLOBAL_HOST: GlobalHost = GlobalHost(UnsafeCell::new(SwitchHost::new()));

#[cfg(feature = "ffi")]
#[inline]
fn global_host_mut() -> &'static mut SwitchHost {
    unsafe { &mut *GLOBAL_HOST.0.get() }
}

#[cfg(feature = "ffi")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_api_version() -> u32 {
    RFVP_SWITCH_HOST_API_VERSION
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_render_api_version() -> u32 {
    RFVP_SWITCH_RENDER_API_VERSION
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_audio_api_version() -> u32 {
    audio::RFVP_SWITCH_AUDIO_API_VERSION
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_core_abi_version() -> u32 {
    RFVP_SWITCH_CORE_ABI_VERSION
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_init(host: *mut SwitchHost) -> i32 {
    if host.is_null() {
        return -1;
    }
    unsafe {
        host.write(SwitchHost::new());
    }
    0
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_begin_frame(host: *mut SwitchHost) -> i32 {
    if host.is_null() {
        return -1;
    }
    unsafe {
        (*host).begin_frame();
    }
    0
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_end_frame(host: *mut SwitchHost) -> i32 {
    if host.is_null() {
        return -1;
    }
    unsafe {
        (*host).end_frame();
    }
    0
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_global_init() -> i32 {
    unsafe {
        global_host_mut().destroy_core();
    }
    *global_host_mut() = SwitchHost::new();
    0
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_global_begin_frame() -> i32 {
    global_host_mut().begin_frame();
    0
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_global_end_frame() -> i32 {
    global_host_mut().end_frame();
    0
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_global_frame_no() -> u64 {
    global_host_mut().frame_no
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_global_load_game_root(
    game_root_utf8: *const c_char,
    nls_utf8: *const c_char,
    width: u32,
    height: u32,
) -> i32 {
    unsafe { global_host_mut().load_game_root(game_root_utf8, nls_utf8, width, height) }
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_global_tick(frame_time_ms: u32, input: *const RfvpSwitchInputFrame) -> i32 {
    let fallback = RfvpSwitchInputFrame::default();
    let input_ref = if input.is_null() {
        &fallback
    } else {
        unsafe { &*input }
    };
    global_host_mut().tick(frame_time_ms, input_ref)
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_global_core_status() -> i32 {
    global_host_mut().core_status
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_global_core_stats(out: *mut RfvpSwitchCoreStats) -> i32 {
    if out.is_null() {
        return -1;
    }
    unsafe {
        out.write(global_host_mut().core_stats);
    }
    0
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_global_destroy_core() {
    unsafe {
        global_host_mut().destroy_core();
    }
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_global_render_command_count() -> usize {
    #[cfg(feature = "rfvp-core-link")]
    {
        let host = global_host_mut();
        if host.core.is_null() {
            return 0;
        }
        unsafe { rfvp_switch_core_render_command_count(host.core as *const c_void) }
    }
    #[cfg(not(feature = "rfvp-core-link"))]
    {
        0
    }
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_global_render_commands() -> *const RenderCommand {
    #[cfg(feature = "rfvp-core-link")]
    {
        let host = global_host_mut();
        if host.core.is_null() {
            return core::ptr::null();
        }
        unsafe { rfvp_switch_core_render_commands(host.core as *const c_void) }
    }
    #[cfg(not(feature = "rfvp-core-link"))]
    {
        core::ptr::null()
    }
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_global_audio_queued_samples() -> usize {
    #[cfg(feature = "rfvp-core-link")]
    {
        let host = global_host_mut();
        if host.core.is_null() {
            return 0;
        }
        unsafe { rfvp_switch_core_audio_queued_samples(host.core as *const c_void) }
    }
    #[cfg(not(feature = "rfvp-core-link"))]
    {
        0
    }
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_host_global_audio_pop_i16(out: *mut i16, len: usize) -> usize {
    if out.is_null() || len == 0 {
        return 0;
    }
    #[cfg(feature = "rfvp-core-link")]
    {
        let host = global_host_mut();
        if host.core.is_null() {
            return 0;
        }
        unsafe { rfvp_switch_core_audio_pop_i16(host.core as *const c_void, out, len) }
    }
    #[cfg(not(feature = "rfvp-core-link"))]
    {
        0
    }
}
