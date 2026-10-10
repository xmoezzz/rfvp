use core::ffi::c_void;

use rfvp::host_api::{FatalErrorCode, PlatformCallbacks, RfvpHost, RfvpLogLevel};

use crate::audio::PspAudio;
use crate::clock::PspClock;
use crate::fs::PspFileSystem;
use crate::raw::{RawPspFatalFn, RawPspHost, RawPspLogFn};
use crate::render::PspRenderer;

pub struct PspHost {
    fs: PspFileSystem,
    renderer: PspRenderer,
    audio: PspAudio,
    clock: PspClock,
    log_ctx: *mut c_void,
    log: Option<RawPspLogFn>,
    fatal_ctx: *mut c_void,
    fatal: Option<RawPspFatalFn>,
}

impl PspHost {
    pub fn from_raw(raw: RawPspHost) -> Self {
        Self {
            fs: PspFileSystem::new(raw.fs_ctx, raw.fs),
            renderer: PspRenderer::new(raw.renderer_ctx, raw.renderer),
            audio: PspAudio::new(raw.audio_ctx, raw.audio),
            clock: PspClock::new(raw.clock_ctx, raw.clock),
            log_ctx: raw.log_ctx,
            log: raw.log,
            fatal_ctx: raw.fatal_ctx,
            fatal: raw.fatal,
        }
    }
}

impl RfvpHost for PspHost {
    type FileSystem = PspFileSystem;
    type Renderer = PspRenderer;
    type Audio = PspAudio;
    type Clock = PspClock;

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
            fatal_error: Some(psp_fatal_error),
        }
    }
}

extern "C" fn psp_fatal_error(
    user_data: *mut c_void,
    code: FatalErrorCode,
    message_ptr: *const u8,
    message_len: usize,
) {
    if user_data.is_null() {
        return;
    }
    let host = unsafe { &mut *user_data.cast::<PspHost>() };
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
