use core::ffi::c_void;
use core::slice;

use alloc::format;
use rfvp::host_api::{FatalErrorCode, PlatformCallbacks, RfvpHost, RfvpLogLevel};

use crate::audio::WiiAudio;
use crate::clock::WiiClock;
use crate::fs::WiiFileSystem;
use crate::render::WiiRenderer;

pub struct WiiHost {
    fs: WiiFileSystem,
    renderer: WiiRenderer,
    audio: WiiAudio,
    clock: WiiClock,
}

impl WiiHost {
    pub fn with_roots(root: &str, save_root: &str) -> Self {
        Self {
            fs: WiiFileSystem::with_roots(root, save_root),
            renderer: WiiRenderer::new(),
            audio: WiiAudio::new(),
            clock: WiiClock::new(),
        }
    }

    pub fn renderer_ref(&self) -> &WiiRenderer {
        &self.renderer
    }
}

impl RfvpHost for WiiHost {
    type FileSystem = WiiFileSystem;
    type Renderer = WiiRenderer;
    type Audio = WiiAudio;
    type Clock = WiiClock;

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
            fatal_error: Some(wii_fatal_error),
        }
    }
}

extern "C" fn wii_fatal_error(
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
