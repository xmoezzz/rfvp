use core::ffi::c_void;

use rfvp::host_api::{FatalErrorCode, PlatformCallbacks, RfvpHost, RfvpLogLevel};

use crate::audio::PsvAudio;
use crate::clock::PsvClock;
use crate::fs::PsvFileSystem;
use crate::raw::{RawPsvHost, RawPsvLogFn};
use crate::render::PsvRenderer;

pub struct PsvHost {
    fs: PsvFileSystem,
    renderer: PsvRenderer,
    audio: PsvAudio,
    clock: PsvClock,
    log_ctx: *mut c_void,
    log: Option<RawPsvLogFn>,
}

impl PsvHost {
    pub fn from_raw(raw: RawPsvHost) -> Self {
        Self {
            fs: PsvFileSystem::new(raw.fs_ctx, raw.fs),
            renderer: PsvRenderer::new(raw.renderer_ctx, raw.renderer),
            audio: PsvAudio::new(raw.audio_ctx, raw.audio),
            clock: PsvClock::new(raw.clock_ctx, raw.clock),
            log_ctx: raw.log_ctx,
            log: raw.log,
        }
    }

    pub fn into_parts(self) -> (PsvFileSystem, PsvRenderer, PsvAudio, PsvClock) {
        (self.fs, self.renderer, self.audio, self.clock)
    }
}

impl RfvpHost for PsvHost {
    type FileSystem = PsvFileSystem;
    type Renderer = PsvRenderer;
    type Audio = PsvAudio;
    type Clock = PsvClock;

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
            user_data: core::ptr::null_mut(),
            fatal_error: Some(psv_fatal_error),
        }
    }
}

extern "C" {
    fn rfvp_psv_platform_fatal_error(code: u32, message: *const u8, message_len: usize);
}

extern "C" fn psv_fatal_error(
    _user_data: *mut c_void,
    code: FatalErrorCode,
    message_ptr: *const u8,
    message_len: usize,
) {
    unsafe {
        rfvp_psv_platform_fatal_error(code as u32, message_ptr, message_len);
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
