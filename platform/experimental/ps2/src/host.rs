use core::ffi::c_void;

use rfvp::host_api::{FatalErrorCode, PlatformCallbacks, RfvpHost, RfvpLogLevel};

use crate::audio::Ps2Audio;
use crate::clock::Ps2Clock;
use crate::fs::Ps2FileSystem;
use crate::raw::{RawPs2FatalFn, RawPs2Host, RawPs2LogFn};
use crate::render::Ps2Renderer;

pub struct Ps2Host {
    fs: Ps2FileSystem,
    renderer: Ps2Renderer,
    audio: Ps2Audio,
    clock: Ps2Clock,
    log_ctx: *mut c_void,
    log: Option<RawPs2LogFn>,
    fatal_ctx: *mut c_void,
    fatal: Option<RawPs2FatalFn>,
}

impl Ps2Host {
    pub fn from_raw(raw: RawPs2Host) -> Self {
        Self {
            fs: Ps2FileSystem::new(raw.fs_ctx, raw.fs),
            renderer: Ps2Renderer::new(raw.renderer_ctx, raw.renderer),
            audio: Ps2Audio::new(raw.audio_ctx, raw.audio),
            clock: Ps2Clock::new(raw.clock_ctx, raw.clock),
            log_ctx: raw.log_ctx,
            log: raw.log,
            fatal_ctx: raw.fatal_ctx,
            fatal: raw.fatal,
        }
    }
}

impl RfvpHost for Ps2Host {
    type FileSystem = Ps2FileSystem;
    type Renderer = Ps2Renderer;
    type Audio = Ps2Audio;
    type Clock = Ps2Clock;

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
            fatal_error: Some(ps2_fatal_error),
        }
    }
}

extern "C" fn ps2_fatal_error(
    user_data: *mut c_void,
    code: FatalErrorCode,
    message_ptr: *const u8,
    message_len: usize,
) {
    if user_data.is_null() {
        return;
    }
    let host = unsafe { &mut *user_data.cast::<Ps2Host>() };
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
