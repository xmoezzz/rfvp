use core::ffi::c_void;
use core::slice;

use rfvp::host_api::{FatalErrorCode, PlatformCallbacks, RfvpHost, RfvpLogLevel};

use crate::audio::PS3Audio;
use crate::clock::PS3Clock;
use crate::fs::PS3FileSystem;
use crate::render::PS3Renderer;

unsafe extern "C" {
    fn rfvp_ps3_platform_log(level: u32, message: *const u8, message_len: usize);
    pub(crate) fn rfvp_ps3_platform_fatal(code: u32, message: *const u8, message_len: usize) -> !;
}

pub struct PS3Host {
    fs: PS3FileSystem,
    renderer: PS3Renderer,
    audio: PS3Audio,
    clock: PS3Clock,
}

impl PS3Host {
    pub fn new() -> Self {
        Self {
            fs: PS3FileSystem::new(),
            renderer: PS3Renderer::new(),
            audio: PS3Audio::new(),
            clock: PS3Clock::new(),
        }
    }
}

impl Default for PS3Host {
    fn default() -> Self {
        Self::new()
    }
}

impl RfvpHost for PS3Host {
    type FileSystem = PS3FileSystem;
    type Renderer = PS3Renderer;
    type Audio = PS3Audio;
    type Clock = PS3Clock;

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
            rfvp_ps3_platform_log(log_level_to_raw(level), message.as_ptr(), message.len());
        }
    }

    fn platform_callbacks(&mut self) -> PlatformCallbacks {
        PlatformCallbacks {
            user_data: core::ptr::null_mut(),
            fatal_error: Some(ps3_fatal_error),
        }
    }
}

extern "C" fn ps3_fatal_error(
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
        rfvp_ps3_platform_fatal(code as u32, message_ptr, message_len);
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
