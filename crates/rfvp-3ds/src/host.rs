use core::ffi::c_void;

use rfvp::host_api::{FatalErrorCode, PlatformCallbacks, RfvpHost, RfvpLogLevel};

use crate::audio::ThreeDsAudio;
use crate::clock::ThreeDsClock;
use crate::fs::ThreeDsFileSystem;
use crate::raw::{RawThreeDsFatalFn, RawThreeDsHost, RawThreeDsLogFn};
use crate::render::ThreeDsRenderer;

pub struct ThreeDsHost {
    fs: ThreeDsFileSystem,
    renderer: ThreeDsRenderer,
    audio: ThreeDsAudio,
    clock: ThreeDsClock,
    log_ctx: *mut c_void,
    log: Option<RawThreeDsLogFn>,
    fatal_ctx: *mut c_void,
    fatal: Option<RawThreeDsFatalFn>,
}

impl ThreeDsHost {
    pub fn from_raw(raw: RawThreeDsHost) -> Self {
        Self {
            fs: ThreeDsFileSystem::new(raw.fs_ctx, raw.fs),
            renderer: ThreeDsRenderer::new(raw.renderer_ctx, raw.renderer),
            audio: ThreeDsAudio::new(raw.audio_ctx, raw.audio),
            clock: ThreeDsClock::new(raw.clock_ctx, raw.clock),
            log_ctx: raw.log_ctx,
            log: raw.log,
            fatal_ctx: raw.fatal_ctx,
            fatal: raw.fatal,
        }
    }
}

impl RfvpHost for ThreeDsHost {
    type FileSystem = ThreeDsFileSystem;
    type Renderer = ThreeDsRenderer;
    type Audio = ThreeDsAudio;
    type Clock = ThreeDsClock;

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
        let Some(log) = self.log else {
            return;
        };
        unsafe {
            log(
                self.log_ctx,
                log_level_to_raw(level),
                message.as_ptr(),
                message.len(),
            );
        }
    }

    fn platform_callbacks(&mut self) -> PlatformCallbacks {
        PlatformCallbacks {
            user_data: self as *mut Self as *mut c_void,
            fatal_error: Some(three_ds_fatal_error),
        }
    }
}

extern "C" fn three_ds_fatal_error(
    user_data: *mut c_void,
    code: FatalErrorCode,
    message_ptr: *const u8,
    message_len: usize,
) {
    if user_data.is_null() {
        return;
    }
    let host = unsafe { &mut *user_data.cast::<ThreeDsHost>() };
    if let Some(fatal) = host.fatal {
        unsafe {
            fatal(host.fatal_ctx, code as u32, message_ptr, message_len);
        }
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
