use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

use alloc::boxed::Box;
use alloc::vec::Vec;
use rfvp::host_api::{
    AudioParams, AudioSampleFormat, AudioStreamDesc, AudioStreamId, EncodedAudioKind, RfvpAudio,
    RfvpError, RfvpResult, SoftAudioConfig, SoftAudioMixer, SoftAudioVorbis,
};

const OUTPUT_SAMPLE_RATE: u32 = 48_000;
const MIX_FRAMES: usize = 1024;

static PULLS: AtomicU32 = AtomicU32::new(0);
static NONZERO_BUFFERS: AtomicU32 = AtomicU32::new(0);
static MIX_ERRORS: AtomicU32 = AtomicU32::new(0);
static LOAD_ERRORS: AtomicU32 = AtomicU32::new(0);
static PLAY_ERRORS: AtomicU32 = AtomicU32::new(0);
static LAST_MIX_ERROR: AtomicU32 = AtomicU32::new(0);
static MAX_PULL_US: AtomicU32 = AtomicU32::new(0);
static MAX_LOCK_US: AtomicU32 = AtomicU32::new(0);

/// Cumulative, approximate snapshots: audio continues while the main thread reads these.
pub fn diagnostic_stats() -> [u32; 8] {
    [&PULLS, &NONZERO_BUFFERS, &MIX_ERRORS, &LOAD_ERRORS, &PLAY_ERRORS,
     &LAST_MIX_ERROR, &MAX_PULL_US, &MAX_LOCK_US].map(|v| v.load(Ordering::Relaxed))
}

fn error_code(error: RfvpError) -> u32 {
    error.code()
}

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

/// The mixer, shared with the audio thread (`rfvp_psv_audio_thread` in rfvp_psv_vitasdk.c), which
/// pulls a buffer from it every ~21 ms while the game loop changes what plays.
struct SharedMixer {
    locked: AtomicBool,
    mixer: UnsafeCell<SoftAudioMixer<PsvVorbisBackend>>,
    chunk: UnsafeCell<Vec<i16>>,
}

impl SharedMixer {
    fn with<R>(&self, f: impl FnOnce(&mut SoftAudioMixer<PsvVorbisBackend>, &mut Vec<i16>) -> R) -> R {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        // SAFETY: the lock above gives this thread sole access until it is released below.
        let result = f(unsafe { &mut *self.mixer.get() }, unsafe { &mut *self.chunk.get() });
        self.locked.store(false, Ordering::Release);
        result
    }
}

// SAFETY: every access to the cells goes through `with`, under the lock.
unsafe impl Sync for SharedMixer {}

/// The mixer the audio thread reads, or null when there is none.
static SHARED: AtomicPtr<SharedMixer> = AtomicPtr::new(core::ptr::null_mut());

extern "C" {
    fn rfvp_psv_audio_thread_start();
    fn rfvp_psv_audio_thread_stop();
}

/// Called by the audio thread: the next `frames` frames of 48 kHz stereo, silence when nothing
/// plays.
#[no_mangle]
pub unsafe extern "C" fn rfvp_psv_audio_pull(dst: *mut i16, frames: usize) {
    let started = rfvp::perf::start();
    PULLS.fetch_add(1, Ordering::Relaxed);
    let out = unsafe { core::slice::from_raw_parts_mut(dst, frames * 2) };
    let shared = SHARED.load(Ordering::Acquire);
    if shared.is_null() {
        out.fill(0);
        return;
    }
    unsafe { &*shared }.with(|mixer, chunk| {
        MAX_LOCK_US.fetch_max(rfvp::perf::start().saturating_sub(started).min(u32::MAX as u64) as u32, Ordering::Relaxed);
        for part in out.chunks_mut(MIX_FRAMES * 2) {
            match mixer.mix_next(chunk) {
                Ok(true) => part.copy_from_slice(&chunk[..part.len()]),
                Ok(false) => part.fill(0),
                Err(error) => {
                    MIX_ERRORS.fetch_add(1, Ordering::Relaxed);
                    LAST_MIX_ERROR.store(error_code(error), Ordering::Relaxed);
                    part.fill(0);
                }
            }
        }
    });
    if out.iter().any(|&sample| sample != 0) {
        NONZERO_BUFFERS.fetch_add(1, Ordering::Relaxed);
    }
    MAX_PULL_US.fetch_max(rfvp::perf::start().saturating_sub(started).min(u32::MAX as u64) as u32, Ordering::Relaxed);
}

pub struct PsvAudio {
    shared: Box<SharedMixer>,
    started: bool,
}

impl PsvAudio {
    pub fn new() -> Self {
        let config = SoftAudioConfig {
            output_sample_rate: OUTPUT_SAMPLE_RATE,
            mix_frames: MIX_FRAMES,
            max_active_bgm: 2,
            max_active_se: 16,
            max_active_total: 24,
        };
        Self {
            shared: Box::new(SharedMixer {
                locked: AtomicBool::new(false),
                mixer: UnsafeCell::new(SoftAudioMixer::new(PsvVorbisBackend, config)),
                chunk: UnsafeCell::new(alloc::vec![0; MIX_FRAMES * 2]),
            }),
            started: false,
        }
    }

    fn mixer<R>(&mut self, f: impl FnOnce(&mut SoftAudioMixer<PsvVorbisBackend>) -> R) -> R {
        let started = rfvp::perf::start();
        self.shared.with(|mixer, _| {
            rfvp::perf::stop(rfvp::perf::MAIN_AUDIO_LOCK, started);
            f(mixer)
        })
    }
}

impl Default for PsvAudio {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PsvAudio {
    fn drop(&mut self) {
        if self.started {
            unsafe { rfvp_psv_audio_thread_stop() };
            SHARED.store(core::ptr::null_mut(), Ordering::Release);
        }
        self.mixer(|mixer| mixer.shutdown());
    }
}

impl RfvpAudio for PsvAudio {
    fn load_encoded(
        &mut self,
        id: AudioStreamId,
        kind: EncodedAudioKind,
        bytes: &[u8],
    ) -> RfvpResult<()> {
        let result = self.mixer(|m| m.load_encoded(id, kind, bytes));
        if result.is_err() { LOAD_ERRORS.fetch_add(1, Ordering::Relaxed); }
        rfvp::perf::event(format_args!("audio load id={} kind={kind:?} bytes={} result={result:?}", id.0, bytes.len()));
        result
    }

    fn create_stream(&mut self, id: AudioStreamId, desc: AudioStreamDesc) -> RfvpResult<()> {
        self.mixer(|m| m.create_stream(id, desc))
    }

    fn submit_i16(&mut self, id: AudioStreamId, samples: &[i16]) -> RfvpResult<()> {
        self.mixer(|m| m.submit_i16(id, samples))
    }

    fn submit_f32(&mut self, id: AudioStreamId, samples: &[f32]) -> RfvpResult<()> {
        self.mixer(|m| m.submit_f32(id, samples))
    }

    fn play(&mut self, id: AudioStreamId, params: AudioParams, fade_in_ms: u32) -> RfvpResult<()> {
        let result = self.mixer(|m| m.play(id, params, fade_in_ms));
        if result.is_err() { PLAY_ERRORS.fetch_add(1, Ordering::Relaxed); }
        rfvp::perf::event(format_args!("audio play id={} params={params:?} fade={fade_in_ms} result={result:?}", id.0));
        result
    }

    fn stop(&mut self, id: AudioStreamId, fade_ms: u32) -> RfvpResult<()> {
        self.mixer(|m| m.stop(id, fade_ms))
    }

    fn pause(&mut self, id: AudioStreamId) -> RfvpResult<()> {
        self.mixer(|m| m.pause(id))
    }

    fn resume(&mut self, id: AudioStreamId) -> RfvpResult<()> {
        self.mixer(|m| m.resume(id))
    }

    fn set_params(&mut self, id: AudioStreamId, params: AudioParams) -> RfvpResult<()> {
        self.mixer(|m| m.set_params(id, params))
    }

    fn set_master_volume(&mut self, volume: f32) -> RfvpResult<()> {
        rfvp::perf::event(format_args!("audio master volume={volume}"));
        self.mixer(|m| m.set_master_volume(volume))
    }

    fn destroy_stream(&mut self, id: AudioStreamId) {
        self.mixer(|m| m.destroy_stream(id));
    }

    /// The audio thread mixes at the device's pace; this only starts it, the first time.
    fn tick(&mut self, _delta_us: u64) -> RfvpResult<()> {
        if !self.started {
            SHARED.store(&*self.shared as *const SharedMixer as *mut SharedMixer, Ordering::Release);
            unsafe { rfvp_psv_audio_thread_start() };
            self.started = true;
        }
        Ok(())
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

