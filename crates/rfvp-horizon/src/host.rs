use alloc::string::String;

use core::ffi::c_void;
use core::slice;
use nx::diag::abort::{abort, AbortLevel};
use nx::diag::log::lm::LmLogger;
use nx::diag::log::{log_with, LogMetadata, LogSeverity};
use nx::result::ResultCode;
use rfvp::host_api::{FatalErrorCode, PlatformCallbacks, RfvpHost, RfvpLogLevel};

use crate::audio::HorizonAudio;
use crate::clock::HorizonClock;
use crate::fs::HorizonFileSystem;
use crate::render::HorizonRenderer;

pub struct HorizonHost {
    fs: HorizonFileSystem,
    renderer: HorizonRenderer,
    audio: HorizonAudio,
    clock: HorizonClock,
}

impl HorizonHost {
    pub fn new() -> Self {
        Self {
            fs: HorizonFileSystem::new(),
            renderer: HorizonRenderer::new(),
            audio: HorizonAudio::new(),
            clock: HorizonClock::new(),
        }
    }
}

impl Default for HorizonHost {
    fn default() -> Self {
        Self::new()
    }
}

impl RfvpHost for HorizonHost {
    type FileSystem = HorizonFileSystem;
    type Renderer = HorizonRenderer;
    type Audio = HorizonAudio;
    type Clock = HorizonClock;

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
        let severity = match level {
            RfvpLogLevel::Error => LogSeverity::Error,
            RfvpLogLevel::Warn => LogSeverity::Warn,
            RfvpLogLevel::Info => LogSeverity::Info,
            RfvpLogLevel::Debug | RfvpLogLevel::Trace => LogSeverity::Trace,
        };
        let metadata = LogMetadata::new(
            severity,
            matches!(level, RfvpLogLevel::Trace),
            String::from(message),
            "crates/rfvp-horizon/src/host.rs",
            "HorizonHost::log",
            0,
        );
        log_with::<LmLogger>(&metadata);
    }

    fn platform_callbacks(&mut self) -> PlatformCallbacks {
        PlatformCallbacks {
            user_data: core::ptr::null_mut(),
            fatal_error: Some(horizon_fatal_error),
        }
    }
}

extern "C" fn horizon_fatal_error(
    _user_data: *mut c_void,
    code: FatalErrorCode,
    message_ptr: *const u8,
    message_len: usize,
) {
    if !message_ptr.is_null() && message_len != 0 {
        let bytes = unsafe { slice::from_raw_parts(message_ptr, message_len) };
        if let Ok(message) = core::str::from_utf8(bytes) {
            let metadata = LogMetadata::new(
                LogSeverity::Error,
                false,
                String::from(message),
                "crates/rfvp-horizon/src/host.rs",
                "horizon_fatal_error",
                0,
            );
            log_with::<LmLogger>(&metadata);
        }
    }

    let raw = 0xCA00_0000u32 | ((code as u32) & 0xFFFF);
    abort(AbortLevel::FatalThrow(), ResultCode::new(raw));
}
