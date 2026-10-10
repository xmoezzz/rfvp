use core::ffi::c_void;
use core::slice;

use alloc::format;
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
    pub fn with_game_root(root: &str) -> Self {
        Self {
            fs: HorizonFileSystem::with_root(root),
            renderer: HorizonRenderer::new(),
            audio: HorizonAudio::new(),
            clock: HorizonClock::new(),
        }
    }

    pub fn renderer_ref(&self) -> &HorizonRenderer {
        &self.renderer
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
        let level = match level {
            RfvpLogLevel::Error => "error",
            RfvpLogLevel::Warn => "warn",
            RfvpLogLevel::Info => "info",
            RfvpLogLevel::Debug | RfvpLogLevel::Trace => "debug",
        };
        crate::ffi::log(&format!("rfvp [{level}] {message}\n"));
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
    let message = if message_ptr.is_null() || message_len == 0 {
        ""
    } else {
        let bytes = unsafe { slice::from_raw_parts(message_ptr, message_len) };
        core::str::from_utf8(bytes).unwrap_or("")
    };
    // Unwinds into the panic handler, which writes the message next to the game.
    panic!("fatal error {code:?}: {message}");
}
