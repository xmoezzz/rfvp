//! Sound on the PSP: the engine's software mixer, Ogg Vorbis through stb_vorbis (built by
//! build.rs), and a thread that hands the mix to the hardware.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use psp::sys;
use rfvp::host_api::{
    AudioByteStream, AudioParams, AudioSampleFormat, AudioStreamDesc, AudioStreamId, EncodedAudioKind, RfvpAudio,
    RfvpError, RfvpResult, SoftAudioConfig, SoftAudioMixer, SoftAudioVorbis,
};

use crate::lock::Mutex;

/// The PSP's output rate.
const OUTPUT_SAMPLE_RATE: u32 = 44_100;
/// Frames per hardware buffer (a multiple of 64): ~23 ms.
const MIX_FRAMES: usize = 1024;
/// Above the game thread's priority (32), so a busy frame does not starve the sound.
const AUDIO_THREAD_PRIORITY: i32 = 16;

struct Shared {
    mixer: Mutex<(SoftAudioMixer<VorbisBackend>, Vec<i16>)>,
    running: AtomicBool,
}

/// What the audio thread reads; set while the thread runs.
static SHARED: AtomicPtr<Shared> = AtomicPtr::new(ptr::null_mut());

pub struct PspAudio {
    shared: Box<Shared>,
    thread: Option<sys::SceUid>,
}

impl PspAudio {
    pub fn new() -> Self {
        // Few voices: every one decodes Vorbis on the 333 MHz CPU.
        let config = SoftAudioConfig {
            output_sample_rate: OUTPUT_SAMPLE_RATE,
            mix_frames: MIX_FRAMES,
            max_active_bgm: 1,
            max_active_se: 6,
            max_active_total: 8,
        };
        Self {
            shared: Box::new(Shared {
                mixer: Mutex::new((
                    SoftAudioMixer::new(VorbisBackend, config),
                    alloc::vec![0; MIX_FRAMES * 2],
                )),
                running: AtomicBool::new(false),
            }),
            thread: None,
        }
    }

    fn mixer<R>(&mut self, f: impl FnOnce(&mut SoftAudioMixer<VorbisBackend>) -> R) -> R {
        f(&mut self.shared.mixer.lock().0)
    }

    fn start_thread(&mut self) {
        SHARED.store(&*self.shared as *const Shared as *mut Shared, Ordering::Release);
        self.shared.running.store(true, Ordering::Release);
        unsafe {
            let id = sys::sceKernelCreateThread(
                b"rfvp_audio\0".as_ptr(),
                audio_thread,
                AUDIO_THREAD_PRIORITY,
                64 * 1024,
                sys::ThreadAttributes::USER,
                ptr::null_mut(),
            );
            if id.0 < 0 {
                // No sound, but the game goes on.
                return;
            }
            sys::sceKernelStartThread(id, 0, ptr::null_mut());
            self.thread = Some(id);
        }
    }
}

impl Default for PspAudio {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PspAudio {
    fn drop(&mut self) {
        if let Some(id) = self.thread.take() {
            self.shared.running.store(false, Ordering::Release);
            unsafe {
                sys::sceKernelWaitThreadEnd(id, ptr::null_mut());
                sys::sceKernelDeleteThread(id);
            }
        }
        SHARED.store(ptr::null_mut(), Ordering::Release);
        self.mixer(|m| m.shutdown());
    }
}

unsafe extern "C" fn audio_thread(_args: usize, _argp: *mut c_void) -> i32 {
    let shared = &*SHARED.load(Ordering::Acquire);
    let channel = sys::sceAudioChReserve(-1, MIX_FRAMES as i32, sys::AudioFormat::Stereo);
    if channel < 0 {
        return channel;
    }
    // Two buffers: one plays while the other is mixed.
    let mut buffers = [
        alloc::vec![0i16; MIX_FRAMES * 2],
        alloc::vec![0i16; MIX_FRAMES * 2],
    ];
    let mut current = 0;
    while shared.running.load(Ordering::Acquire) {
        {
            let mut guard = shared.mixer.lock();
            let (mixer, chunk) = &mut *guard;
            let started = rfvp::perf::start();
            let mixed = mixer.mix_next(chunk);
            rfvp::perf::stop(16, started);
            match mixed {
                Ok(true) => buffers[current].copy_from_slice(&chunk[..MIX_FRAMES * 2]),
                _ => buffers[current].fill(0),
            }
        }
        sys::sceAudioOutputBlocking(channel, 0x8000, buffers[current].as_mut_ptr().cast());
        current ^= 1;
    }
    sys::sceAudioChRelease(channel);
    0
}

impl RfvpAudio for PspAudio {
    fn load_encoded(
        &mut self,
        id: AudioStreamId,
        kind: EncodedAudioKind,
        bytes: &[u8],
    ) -> RfvpResult<()> {
        self.mixer(|m| m.load_encoded(id, kind, bytes))
    }

    fn load_stream(
        &mut self,
        id: AudioStreamId,
        kind: EncodedAudioKind,
        stream: AudioByteStream,
    ) -> RfvpResult<()> {
        self.mixer(|m| m.load_stream(id, kind, stream))
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
        self.mixer(|m| m.play(id, params, fade_in_ms))
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
        self.mixer(|m| m.set_master_volume(volume))
    }

    fn destroy_stream(&mut self, id: AudioStreamId) {
        self.mixer(|m| m.destroy_stream(id));
    }

    /// The audio thread mixes at the hardware's pace; this only starts it, the first time.
    fn tick(&mut self, _delta_us: u64) -> RfvpResult<()> {
        if self.thread.is_none() && !self.shared.running.load(Ordering::Acquire) {
            self.start_thread();
        }
        Ok(())
    }
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
    fn rfvp_ogg_open_stream(
        read: unsafe extern "C" fn(*mut c_void, u64, *mut u8, i32) -> i32,
        user: *mut c_void,
        out_info: *mut RfvpOggVorbisInfo,
        out_decoder: *mut *mut RfvpOggVorbis,
    ) -> i32;
    fn rfvp_ogg_seek_start(decoder: *mut RfvpOggVorbis) -> i32;
    fn rfvp_ogg_close(decoder: *mut RfvpOggVorbis);
}

struct VorbisBackend;

struct VorbisDecoder {
    ptr: *mut RfvpOggVorbis,
    /// What a streamed decoder reads from (its address is the C side's `user`).
    stream: Option<Box<AudioByteStream>>,
}

/// The C decoder's read callback for a streamed sound.
unsafe extern "C" fn read_stream(user: *mut c_void, offset: u64, buf: *mut u8, len: i32) -> i32 {
    let stream = &*user.cast::<AudioByteStream>();
    let buf = core::slice::from_raw_parts_mut(buf, len.max(0) as usize);
    match stream.read_at(offset, buf) {
        Ok(n) => n as i32,
        Err(_) => -1,
    }
}

impl SoftAudioVorbis for VorbisBackend {
    type Decoder = VorbisDecoder;

    fn open(&mut self, bytes: &[u8]) -> RfvpResult<(Self::Decoder, AudioStreamDesc)> {
        if bytes.is_empty() {
            return Err(RfvpError::InvalidData);
        }
        let mut info = RfvpOggVorbisInfo {
            sample_rate: 0,
            channels: 0,
        };
        let mut decoder = ptr::null_mut();
        let status =
            unsafe { rfvp_ogg_open_memory(bytes.as_ptr(), bytes.len(), &mut info, &mut decoder) };
        if status != 0 || decoder.is_null() || info.sample_rate == 0 || info.channels == 0 {
            return Err(RfvpError::InvalidData);
        }
        Ok((
            VorbisDecoder {
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
        let mut decoder = ptr::null_mut();
        let user = &*stream as *const AudioByteStream as *mut c_void;
        let status = unsafe { rfvp_ogg_open_stream(read_stream, user, &mut info, &mut decoder) };
        if status != 0 || decoder.is_null() || info.sample_rate == 0 || info.channels == 0 {
            return Err(RfvpError::InvalidData);
        }
        Ok((
            VorbisDecoder {
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
        if unsafe { rfvp_ogg_seek_start(decoder.ptr) } == 0 {
            Ok(())
        } else {
            Err(RfvpError::InvalidData)
        }
    }

    fn close(&mut self, decoder: Self::Decoder) {
        if !decoder.ptr.is_null() {
            unsafe { rfvp_ogg_close(decoder.ptr) };
        }
    }
}

