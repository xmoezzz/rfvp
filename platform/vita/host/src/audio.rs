use core::ffi::c_void;

use alloc::vec::Vec;
use rfvp::host_api::{
    AudioParams, AudioSampleFormat, AudioStreamDesc, AudioStreamId, EncodedAudioKind, RfvpAudio,
    RfvpError, RfvpResult, SoftAudioConfig, SoftAudioMixer, SoftAudioVorbis,
};

use crate::raw::{RawAudioParams, RawAudioSampleFormat, RawAudioStreamDesc, RawAudioVTable};
use crate::status::status_to_result;

const MASTER_STREAM_ID: AudioStreamId = AudioStreamId(0);
const OUTPUT_SAMPLE_RATE: u32 = 48_000;
const MIX_FRAMES: usize = 1024;

#[repr(C)]
struct RfvpOggVorbisInfo {
    sample_rate: u32,
    channels: u16,
}

enum RfvpOggVorbis {}

extern "C" {
    fn rfvp_ogg_open_memory(
        bytes: *const u8,
        byte_len: usize,
        out_info: *mut RfvpOggVorbisInfo,
        out_decoder: *mut *mut RfvpOggVorbis,
    ) -> i32;
    fn rfvp_ogg_decode_interleaved_i16(
        decoder: *mut RfvpOggVorbis,
        out_samples: *mut i16,
        max_interleaved_samples: i32,
    ) -> i32;
    fn rfvp_ogg_seek_start(decoder: *mut RfvpOggVorbis) -> i32;
    fn rfvp_ogg_close(decoder: *mut RfvpOggVorbis);
}

pub struct PsvAudio {
    ctx: *mut c_void,
    vtable: RawAudioVTable,
    mixer: SoftAudioMixer<PsvVorbisBackend>,
    master_created: bool,
    master_buffer: Vec<i16>,
}

impl PsvAudio {
    pub fn new(ctx: *mut c_void, vtable: RawAudioVTable) -> Self {
        let config = SoftAudioConfig {
            output_sample_rate: OUTPUT_SAMPLE_RATE,
            mix_frames: MIX_FRAMES,
            max_active_bgm: 2,
            max_active_se: 16,
            max_active_total: 24,
        };
        Self {
            ctx,
            vtable,
            mixer: SoftAudioMixer::new(PsvVorbisBackend, config),
            master_created: false,
            master_buffer: Vec::new(),
        }
    }

    fn ensure_master_stream(&mut self) -> RfvpResult<()> {
        if self.master_created {
            return Ok(());
        }
        let desc = AudioStreamDesc {
            sample_rate: OUTPUT_SAMPLE_RATE,
            channels: 2,
            sample_format: AudioSampleFormat::I16,
        };
        let status = unsafe {
            (self.vtable.create_stream)(
                self.ctx,
                MASTER_STREAM_ID.0,
                audio_stream_desc_to_raw(desc),
            )
        };
        status_to_result(status)?;
        self.master_created = true;
        Ok(())
    }

    fn submit_master(&mut self, samples: &[i16]) -> RfvpResult<()> {
        let status = unsafe {
            (self.vtable.submit_i16)(
                self.ctx,
                MASTER_STREAM_ID.0,
                samples.as_ptr(),
                samples.len(),
            )
        };
        status_to_result(status)?;
        let status = unsafe {
            (self.vtable.play)(
                self.ctx,
                MASTER_STREAM_ID.0,
                audio_params_to_raw(AudioParams {
                    volume: 1.0,
                    pan: 0.0,
                    repeat: false,
                }),
            )
        };
        status_to_result(status)
    }
}

impl Drop for PsvAudio {
    fn drop(&mut self) {
        self.mixer.shutdown();
        if self.master_created {
            unsafe {
                (self.vtable.destroy_stream)(self.ctx, MASTER_STREAM_ID.0);
            }
            self.master_created = false;
        }
    }
}

impl RfvpAudio for PsvAudio {
    fn load_encoded(
        &mut self,
        id: AudioStreamId,
        kind: EncodedAudioKind,
        bytes: &[u8],
    ) -> RfvpResult<()> {
        self.mixer.load_encoded(id, kind, bytes)
    }

    fn create_stream(&mut self, id: AudioStreamId, desc: AudioStreamDesc) -> RfvpResult<()> {
        self.mixer.create_stream(id, desc)
    }

    fn submit_i16(&mut self, id: AudioStreamId, samples: &[i16]) -> RfvpResult<()> {
        self.mixer.submit_i16(id, samples)
    }

    fn submit_f32(&mut self, id: AudioStreamId, samples: &[f32]) -> RfvpResult<()> {
        self.mixer.submit_f32(id, samples)
    }

    fn play(&mut self, id: AudioStreamId, params: AudioParams, fade_in_ms: u32) -> RfvpResult<()> {
        self.mixer.play(id, params, fade_in_ms)
    }

    fn stop(&mut self, id: AudioStreamId, fade_ms: u32) -> RfvpResult<()> {
        self.mixer.stop(id, fade_ms)
    }

    fn pause(&mut self, id: AudioStreamId) -> RfvpResult<()> {
        self.mixer.pause(id)
    }

    fn resume(&mut self, id: AudioStreamId) -> RfvpResult<()> {
        self.mixer.resume(id)
    }

    fn set_params(&mut self, id: AudioStreamId, params: AudioParams) -> RfvpResult<()> {
        self.mixer.set_params(id, params)
    }

    fn set_master_volume(&mut self, volume: f32) -> RfvpResult<()> {
        self.mixer.set_master_volume(volume)
    }

    fn destroy_stream(&mut self, id: AudioStreamId) {
        self.mixer.destroy_stream(id);
    }

    fn tick(&mut self, delta_us: u64) -> RfvpResult<()> {
        self.ensure_master_stream()?;
        let needed = MIX_FRAMES
            .checked_mul(2)
            .ok_or(RfvpError::CapacityExceeded)?;
        if self.master_buffer.len() != needed {
            self.master_buffer.resize(needed, 0);
        }
        let active = self.mixer.mix_next(&mut self.master_buffer)?;
        if active {
            let samples = self.master_buffer.clone();
            self.submit_master(&samples)?;
        }
        let status = unsafe { (self.vtable.tick)(self.ctx, delta_us) };
        status_to_result(status)
    }
}

struct PsvVorbisBackend;

struct PsvVorbisDecoder {
    ptr: *mut RfvpOggVorbis,
}

impl SoftAudioVorbis for PsvVorbisBackend {
    type Decoder = PsvVorbisDecoder;

    fn open(&mut self, bytes: &[u8]) -> RfvpResult<(Self::Decoder, AudioStreamDesc)> {
        if bytes.is_empty() {
            return Err(RfvpError::InvalidData);
        }
        let mut info = RfvpOggVorbisInfo {
            sample_rate: 0,
            channels: 0,
        };
        let mut decoder = core::ptr::null_mut();
        let status =
            unsafe { rfvp_ogg_open_memory(bytes.as_ptr(), bytes.len(), &mut info, &mut decoder) };
        if status != 0 || decoder.is_null() || info.sample_rate == 0 || info.channels == 0 {
            return Err(RfvpError::InvalidData);
        }
        Ok((
            PsvVorbisDecoder { ptr: decoder },
            AudioStreamDesc {
                sample_rate: info.sample_rate,
                channels: info.channels,
                sample_format: AudioSampleFormat::I16,
            },
        ))
    }

    fn decode_interleaved_i16(
        &mut self,
        decoder: &mut Self::Decoder,
        out_samples: &mut [i16],
    ) -> RfvpResult<usize> {
        let max_samples =
            i32::try_from(out_samples.len()).map_err(|_| RfvpError::CapacityExceeded)?;
        let decoded = unsafe {
            rfvp_ogg_decode_interleaved_i16(decoder.ptr, out_samples.as_mut_ptr(), max_samples)
        };
        if decoded < 0 {
            return Err(RfvpError::InvalidData);
        }
        Ok(decoded as usize)
    }

    fn seek_start(&mut self, decoder: &mut Self::Decoder) -> RfvpResult<()> {
        let status = unsafe { rfvp_ogg_seek_start(decoder.ptr) };
        if status == 0 {
            Ok(())
        } else {
            Err(RfvpError::InvalidData)
        }
    }

    fn close(&mut self, decoder: Self::Decoder) {
        if !decoder.ptr.is_null() {
            unsafe {
                rfvp_ogg_close(decoder.ptr);
            }
        }
    }
}

fn audio_sample_format_to_raw(value: AudioSampleFormat) -> RawAudioSampleFormat {
    match value {
        AudioSampleFormat::I16 => RawAudioSampleFormat::I16,
        AudioSampleFormat::F32 => RawAudioSampleFormat::F32,
    }
}

fn audio_stream_desc_to_raw(value: AudioStreamDesc) -> RawAudioStreamDesc {
    RawAudioStreamDesc {
        sample_rate: value.sample_rate,
        channels: value.channels,
        sample_format: audio_sample_format_to_raw(value.sample_format),
    }
}

fn audio_params_to_raw(value: AudioParams) -> RawAudioParams {
    RawAudioParams {
        volume: value.volume,
        pan: value.pan,
        repeat: u8::from(value.repeat),
        _padding: [0; 3],
    }
}
