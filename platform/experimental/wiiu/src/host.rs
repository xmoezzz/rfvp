use core::ffi::c_void;
use core::slice;

use rfvp::host_api::{FatalErrorCode, PlatformCallbacks, RfvpHost, RfvpLogLevel};

use crate::audio::WiiUAudio;
use crate::clock::WiiUClock;
use crate::fs::WiiUFileSystem;
use crate::render::WiiURenderer;

unsafe extern "C" {
    fn rfvp_wiiu_platform_log(level: u32, message: *const u8, message_len: usize);
    pub(crate) fn rfvp_wiiu_platform_fatal(code: u32, message: *const u8, message_len: usize) -> !;
}

pub struct WiiUHost {
    fs: WiiUFileSystem,
    renderer: WiiURenderer,
    audio: WiiUAudio,
    clock: WiiUClock,
}

impl WiiUHost {
    pub fn new() -> Self {
        Self {
            fs: WiiUFileSystem::new(),
            renderer: WiiURenderer::new(),
            audio: WiiUAudio::new(),
            clock: WiiUClock::new(),
        }
    }
}

impl Default for WiiUHost {
    fn default() -> Self {
        Self::new()
    }
}

impl RfvpHost for WiiUHost {
    type FileSystem = WiiUFileSystem;
    type Renderer = WiiURenderer;
    type Audio = WiiUAudio;
    type Clock = WiiUClock;

    fn fs(&mut self) -> &mut Self::FileSystem {
        &mut self.fs
    }

    fn renderer(&mut self) -> &mut Self::Renderer {
        &mut self.renderer
    }

    fn audio(&mut self) -> &mut Self::Audio {
        &mut self.audio
    }

    fn clock(&mut self) -> &mut Self::Clock {
        &mut self.clock
    }

    fn log(&mut self, level: RfvpLogLevel, message: &str) {
        unsafe {
            rfvp_wiiu_platform_log(log_level_to_raw(level), message.as_ptr(), message.len());
        }
    }

    fn platform_callbacks(&mut self) -> PlatformCallbacks {
        PlatformCallbacks {
            user_data: core::ptr::null_mut(),
            fatal_error: Some(wiiu_fatal_error),
        }
    }
}

extern "C" fn wiiu_fatal_error(
    _user_data: *mut c_void,
    code: FatalErrorCode,
    message_ptr: *const u8,
    message_len: usize,
) {
    let (message_ptr, message_len) = if message_ptr.is_null() {
        (b"rfvp fatal error".as_ptr(), b"rfvp fatal error".len())
    } else {
        let _ = unsafe { slice::from_raw_parts(message_ptr, message_len) };
        (message_ptr, message_len)
    };
    unsafe {
        rfvp_wiiu_platform_fatal(code as u32, message_ptr, message_len);
    }
}

fn log_level_to_raw(level: RfvpLogLevel) -> u32 {
    match level {
        RfvpLogLevel::Error => 1,
        RfvpLogLevel::Warn => 2,
        RfvpLogLevel::Info => 3,
        RfvpLogLevel::Debug => 4,
        RfvpLogLevel::Trace => 5,
    }
}
