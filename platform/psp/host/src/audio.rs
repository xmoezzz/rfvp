use core::ffi::c_void;

use rfvp::host_api::{
    AudioParams, AudioSampleFormat, AudioStreamDesc, AudioStreamId, EncodedAudioKind, RfvpAudio,
    RfvpError, RfvpResult,
};

use crate::raw::{RawAudioParams, RawAudioVTable};
use crate::status::status_to_result;

pub const ACTIVE_BGM_VOICES: usize = 1;
pub const ACTIVE_SE_VOICES: usize = 8;
pub const ACTIVE_TOTAL_VOICES: usize = 12;

pub struct PspAudio {
    ctx: *mut c_void,
    vtable: RawAudioVTable,
}

impl PspAudio {
    pub const fn new(ctx: *mut c_void, vtable: RawAudioVTable) -> Self {
        Self { ctx, vtable }
    }
}

impl RfvpAudio for PspAudio {
    fn load_encoded(
        &mut self,
        id: AudioStreamId,
        kind: EncodedAudioKind,
        bytes: &[u8],
    ) -> RfvpResult<()> {
        match kind {
            EncodedAudioKind::Ogg | EncodedAudioKind::Mp3 | EncodedAudioKind::Flac => {
                Err(RfvpError::Unsupported)
            }
            EncodedAudioKind::Unknown | EncodedAudioKind::Wav => {
                let status = unsafe {
                    (self.vtable.load_native)(self.ctx, id.0, bytes.as_ptr(), bytes.len())
                };
                status_to_result(status)
            }
        }
    }

    fn create_stream(&mut self, _id: AudioStreamId, _desc: AudioStreamDesc) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn submit_i16(&mut self, _id: AudioStreamId, _samples: &[i16]) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn submit_f32(&mut self, _id: AudioStreamId, _samples: &[f32]) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn play(&mut self, id: AudioStreamId, params: AudioParams, fade_in_ms: u32) -> RfvpResult<()> {
        let status =
            unsafe { (self.vtable.play)(self.ctx, id.0, audio_params(params), fade_in_ms) };
        status_to_result(status)
    }

    fn stop(&mut self, id: AudioStreamId, fade_ms: u32) -> RfvpResult<()> {
        let status = unsafe { (self.vtable.stop)(self.ctx, id.0, fade_ms) };
        status_to_result(status)
    }

    fn pause(&mut self, id: AudioStreamId) -> RfvpResult<()> {
        let status = unsafe { (self.vtable.pause)(self.ctx, id.0) };
        status_to_result(status)
    }

    fn resume(&mut self, id: AudioStreamId) -> RfvpResult<()> {
        let status = unsafe { (self.vtable.resume)(self.ctx, id.0) };
        status_to_result(status)
    }

    fn set_params(&mut self, id: AudioStreamId, params: AudioParams) -> RfvpResult<()> {
        let status = unsafe { (self.vtable.set_params)(self.ctx, id.0, audio_params(params)) };
        status_to_result(status)
    }

    fn set_master_volume(&mut self, _volume: f32) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn destroy_stream(&mut self, id: AudioStreamId) {
        unsafe {
            (self.vtable.destroy)(self.ctx, id.0);
        }
    }

    fn tick(&mut self, delta_us: u64) -> RfvpResult<()> {
        let status = unsafe { (self.vtable.tick)(self.ctx, delta_us) };
        status_to_result(status)
    }
}

fn audio_params(value: AudioParams) -> RawAudioParams {
    RawAudioParams {
        volume: value.volume,
        pan: value.pan,
        repeat: u8::from(value.repeat),
        _padding: [0; 3],
    }
}

#[allow(dead_code)]
fn _stream_desc_supported(desc: AudioStreamDesc) -> bool {
    desc.sample_format == AudioSampleFormat::I16 && desc.channels <= 2 && desc.sample_rate != 0
}
