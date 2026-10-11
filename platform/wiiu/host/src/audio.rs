//! Audio: the shared software mixer, pulled by the runtime (`audio_pump` in main.c) each
//! frame for the part of the AX voices' ring that has played out.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ffi::c_void;

use rfvp::host_api::{
    AudioByteStream, AudioParams, AudioSampleFormat, AudioStreamDesc, AudioStreamId, EncodedAudioKind, RfvpAudio,
    RfvpError, RfvpResult, SoftAudioConfig, SoftAudioMixer, SoftAudioVorbis,
};

const OUTPUT_SAMPLE_RATE: u32 = 48_000;
/// Frames the mixer produces at a time.
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
    fn rfvp_ogg_open_stream(
        read: unsafe extern "C" fn(*mut c_void, u64, *mut u8, i32) -> i32,
        user: *mut c_void,
        out_info: *mut RfvpOggVorbisInfo,
        out_decoder: *mut *mut RfvpOggVorbis,
    ) -> i32;
    fn rfvp_ogg_seek_start(decoder: *mut RfvpOggVorbis) -> i32;
    fn rfvp_ogg_close(decoder: *mut RfvpOggVorbis);
}

pub struct WiiuAudio {
    mixer: SoftAudioMixer<WiiuVorbisBackend>,
    chunk: Vec<i16>,
}

impl WiiuAudio {
    pub fn new() -> Self {
        Self {
            mixer: SoftAudioMixer::new(
                WiiuVorbisBackend,
                SoftAudioConfig {
                    output_sample_rate: OUTPUT_SAMPLE_RATE,
                    mix_frames: MIX_FRAMES,
                    max_active_bgm: 2,
                    max_active_se: 16,
                    max_active_total: 24,
                },
            ),
            chunk: alloc::vec![0; MIX_FRAMES * 2],
        }
    }

    /// Fill `out` (interleaved stereo) with what plays next; silence when nothing does.
    pub fn render(&mut self, out: &mut [i16]) {
        for part in out.chunks_mut(MIX_FRAMES * 2) {
            match self.mixer.mix_next(&mut self.chunk) {
                Ok(true) => part.copy_from_slice(&self.chunk[..part.len()]),
                _ => part.fill(0),
            }
        }
    }
}

impl Default for WiiuAudio {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WiiuAudio {
    fn drop(&mut self) {
        self.mixer.shutdown();
    }
}

impl RfvpAudio for WiiuAudio {
    fn load_encoded(
        &mut self,
        id: AudioStreamId,
        kind: EncodedAudioKind,
        bytes: &[u8],
    ) -> RfvpResult<()> {
        self.mixer.load_encoded(id, kind, bytes)
    }

    /// Music is read from the card as it plays: a song is several megabytes.
    fn load_stream(
        &mut self,
        id: AudioStreamId,
        kind: EncodedAudioKind,
        stream: AudioByteStream,
    ) -> RfvpResult<()> {
        self.mixer.load_stream(id, kind, stream)
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

    /// Mixing happens when the runtime asks for audio (`render`), at the device's pace.
    fn tick(&mut self, _delta_us: u64) -> RfvpResult<()> {
        Ok(())
    }
}

struct WiiuVorbisBackend;

struct WiiuVorbisDecoder {
    ptr: *mut RfvpOggVorbis,
    /// What a streamed decoder reads from: only the C side reads it (its address is `user`),
    /// it is kept here so it lives as long as the decoder.
    #[allow(dead_code)]
    stream: Option<Box<AudioByteStream>>,
}

/// The C decoder's read callback for a streamed sound.
unsafe extern "C" fn read_stream(user: *mut c_void, offset: u64, buf: *mut u8, len: i32) -> i32 {
    let stream = unsafe { &*user.cast::<AudioByteStream>() };
    let buf = unsafe { core::slice::from_raw_parts_mut(buf, len.max(0) as usize) };
    match stream.read_at(offset, buf) {
        Ok(n) => n as i32,
        Err(_) => -1,
    }
}

impl SoftAudioVorbis for WiiuVorbisBackend {
    type Decoder = WiiuVorbisDecoder;

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
            WiiuVorbisDecoder {
                ptr: decoder,
                stream: None,
            },
            AudioStreamDesc {
                sample_rate: info.sample_rate,
                channels: info.channels,
                sample_format: AudioSampleFormat::I16,
            },
        ))
    }

    fn open_stream(
        &mut self,
        stream: AudioByteStream,
    ) -> RfvpResult<(Self::Decoder, AudioStreamDesc)> {
        let stream = Box::new(stream);
        let mut info = RfvpOggVorbisInfo {
            sample_rate: 0,
            channels: 0,
        };
        let mut decoder = core::ptr::null_mut();
        let user = &*stream as *const AudioByteStream as *mut c_void;
        let status = unsafe { rfvp_ogg_open_stream(read_stream, user, &mut info, &mut decoder) };
        if status != 0 || decoder.is_null() || info.sample_rate == 0 || info.channels == 0 {
            return Err(RfvpError::InvalidData);
        }
        Ok((
            WiiuVorbisDecoder {
                ptr: decoder,
                stream: Some(stream),
            },
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
