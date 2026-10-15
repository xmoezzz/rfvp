use core::ffi::c_void;

use rfvp::host_api::{FatalErrorCode, PlatformCallbacks, RfvpHost, RfvpLogLevel};

use crate::audio::Ps1Audio;
use crate::clock::Ps1Clock;
use crate::fs::Ps1FileSystem;
use crate::render::Ps1Renderer;

pub struct Ps1Host {
    fs: Ps1FileSystem,
    renderer: Ps1Renderer,
    audio: Ps1Audio,
    clock: Ps1Clock,
}

impl Ps1Host {
    pub fn new() -> Self {
        Self {
            fs: Ps1FileSystem::new(),
            renderer: Ps1Renderer::new(),
            audio: Ps1Audio::new(),
            clock: Ps1Clock::new(),
        }
    }

    pub fn renderer_ref(&self) -> &Ps1Renderer {
        &self.renderer
    }
}

impl RfvpHost for Ps1Host {
    type FileSystem = Ps1FileSystem;
    type Renderer = Ps1Renderer;
    type Audio = Ps1Audio;
    type Clock = Ps1Clock;

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

    /// The PlayStation keeps no log: the text would cost memory and the TTY time.
    fn log(&mut self, _level: RfvpLogLevel, _message: &str) {
        #[cfg(feature = "diagnostics")]
        {
            crate::ffi::log(_message);
            crate::ffi::log("\n");
        }
    }

    fn platform_callbacks(&mut self) -> PlatformCallbacks {
        PlatformCallbacks {
            user_data: core::ptr::null_mut(),
            fatal_error: Some(ps1_fatal_error),
        }
    }
}

extern "C" fn ps1_fatal_error(
    _user_data: *mut c_void,
    _code: FatalErrorCode,
    _message_ptr: *const u8,
    _message_len: usize,
) {
    panic!();
}
