use alloc::alloc::{alloc, dealloc, Layout};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cmp;
use core::ffi::{c_int, c_void};

use nx::arm;
use nx::ipc::sf;
use nx::service;
use nx::service::audio::{
    AudioBuffer, AudioInterfaceName, AudioOut, AudioOutManagerService, AudioRequestParameters,
    IAudioOutClient, IAudioOutManagerClient, PcmFormat,
};
use nx::svc;
use nx::version::{self, Version};
use rfvp::host_api::{
    AudioParams, AudioSampleFormat, AudioStreamDesc, AudioStreamId, EncodedAudioKind, RfvpAudio,
    RfvpError, RfvpResult, SoftAudioConfig, SoftAudioMixer, SoftAudioVorbis,
};

const RELEASED_BUFFER_BATCH: usize = 32;
const OUTPUT_SAMPLE_RATE: u32 = 48_000;
const MIX_FRAMES: usize = 1024;
const OGG_ALLOC_ALIGN: usize = 16;
const OGG_ALLOC_HEADER_SIZE: usize = core::mem::size_of::<usize>();

type QsortCompareFn = unsafe extern "C" fn(*const c_void, *const c_void) -> c_int;

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

#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    if size == 0 {
        return core::ptr::null_mut();
    }
    let Some(total) = size.checked_add(OGG_ALLOC_HEADER_SIZE) else {
        return core::ptr::null_mut();
    };
    let Ok(layout) = Layout::from_size_align(total, OGG_ALLOC_ALIGN) else {
        return core::ptr::null_mut();
    };
    let base = unsafe { alloc(layout) };
    if base.is_null() {
        return core::ptr::null_mut();
    }
    unsafe {
        (base as *mut usize).write(total);
        base.add(OGG_ALLOC_HEADER_SIZE) as *mut c_void
    }
}

#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    let base = unsafe { (ptr as *mut u8).sub(OGG_ALLOC_HEADER_SIZE) };
    let total = unsafe { (base as *const usize).read() };
    let Ok(layout) = Layout::from_size_align(total, OGG_ALLOC_ALIGN) else {
        return;
    };
    unsafe {
        dealloc(base, layout);
    }
}

#[no_mangle]
pub unsafe extern "C" fn qsort(
    base: *mut c_void,
    count: usize,
    size: usize,
    compare: QsortCompareFn,
) {
    if base.is_null() || count < 2 || size == 0 {
        return;
    }
    let base = base as *mut u8;
    for index in 1..count {
        let mut current = index;
        while current > 0 {
            let left = unsafe { base.add((current - 1) * size) };
            let right = unsafe { base.add(current * size) };
            if unsafe { compare(left as *const c_void, right as *const c_void) } <= 0 {
                break;
            }
            for byte in 0..size {
                unsafe {
                    let a = left.add(byte);
                    let b = right.add(byte);
                    let tmp = a.read();
                    a.write(b.read());
                    b.write(tmp);
                }
            }
            current -= 1;
        }
    }
}

struct AudioChunk {
    bytes: Vec<u8>,
    buffer: AudioBuffer,
    queued: bool,
    completed: bool,
}

impl AudioChunk {
    fn new(mut bytes: Vec<u8>) -> Box<Self> {
        let sample_buffer = bytes.as_mut_ptr();
        let data_size = bytes.len();
        Box::new(Self {
            bytes,
            buffer: AudioBuffer {
                _unused_ptr: 0,
                sample_buffer,
                buffer_capacity: data_size,
                data_size,
                _data_offset: 0,
            },
            queued: false,
            completed: false,
        })
    }

    fn buffer_ptr(&self) -> usize {
        &self.buffer as *const AudioBuffer as usize
    }

    fn flush_cache(&mut self) {
        if !self.bytes.is_empty() {
            arm::cache_flush(self.bytes.as_mut_ptr(), self.bytes.len());
        }
    }
}

struct HorizonMasterOutput {
    out: AudioOut,
    actual_format: PcmFormat,
    chunks: Vec<Box<AudioChunk>>,
    playing: bool,
}

impl HorizonMasterOutput {
    fn open() -> RfvpResult<Self> {
        let manager = service::new_service_object::<AudioOutManagerService>()
            .map_err(|_| RfvpError::Backend)?;
        let in_name = AudioInterfaceName::new();
        let mut out_name = AudioInterfaceName::new();
        let params = AudioRequestParameters {
            sample_rate: OUTPUT_SAMPLE_RATE,
            channel_count: 2,
        };
        let (out, response) = manager
            .open_audio_out(
                sf::InMapAliasBuffer::from_var(&in_name),
                sf::OutMapAliasBuffer::from_mut_var(&mut out_name),
                params,
                sf::AppletResourceUserId::new(0),
                sf::CopyHandle::from(svc::CURRENT_PROCESS_PSEUDO_HANDLE),
            )
            .map_err(|_| RfvpError::Backend)?;
        let actual_format = match response.sample_format {
            PcmFormat::Int16 | PcmFormat::Float => response.sample_format,
            _ => return Err(RfvpError::InvalidData),
        };
        Ok(Self {
            out,
            actual_format,
            chunks: Vec::new(),
            playing: false,
        })
    }

    fn submit_stereo_i16(&mut self, samples: &[i16]) -> RfvpResult<()> {
        self.collect_released_buffers()?;
        let bytes = encode_master(samples, self.actual_format);
        let mut chunk = AudioChunk::new(bytes);
        Self::append_chunk(&self.out, &mut chunk)?;
        self.chunks.push(chunk);
        if !self.playing {
            self.out.start().map_err(|_| RfvpError::Backend)?;
            self.playing = true;
        }
        Ok(())
    }

    fn append_chunk(out: &AudioOut, chunk: &mut AudioChunk) -> RfvpResult<()> {
        if chunk.bytes.is_empty() || chunk.queued || chunk.completed {
            return Ok(());
        }
        chunk.flush_cache();
        unsafe {
            if version::get_version() >= Version::new(3, 0, 0) {
                out.append_buffer_auto(
                    sf::InAutoSelectBuffer::from_var(&chunk.buffer),
                    chunk.buffer_ptr(),
                )
                .map_err(|_| RfvpError::Backend)?;
            } else {
                out.append_buffer(
                    sf::InMapAliasBuffer::from_var(&chunk.buffer),
                    chunk.buffer_ptr(),
                )
                .map_err(|_| RfvpError::Backend)?;
            }
        }
        chunk.queued = true;
        Ok(())
    }

    fn collect_released_buffers(&mut self) -> RfvpResult<()> {
        loop {
            let count = if version::get_version() >= Version::new(3, 0, 0) {
                let empty = AudioBuffer {
                    _unused_ptr: 0,
                    sample_buffer: core::ptr::null_mut(),
                    buffer_capacity: 0,
                    data_size: 0,
                    _data_offset: 0,
                };
                let mut released: [AudioBuffer; RELEASED_BUFFER_BATCH] =
                    [empty; RELEASED_BUFFER_BATCH];
                let count = self
                    .out
                    .get_released_buffers_auto(sf::OutAutoSelectBuffer::from_mut_array(
                        &mut released,
                    ))
                    .map_err(|_| RfvpError::Backend)? as usize;
                for buffer in released.iter().take(cmp::min(count, RELEASED_BUFFER_BATCH)) {
                    if buffer.sample_buffer.is_null() {
                        continue;
                    }
                    if let Some(chunk) = self.chunks.iter_mut().find(|chunk| {
                        chunk.buffer.sample_buffer == buffer.sample_buffer
                            && chunk.buffer.data_size == buffer.data_size
                            && chunk.buffer.buffer_capacity == buffer.buffer_capacity
                    }) {
                        chunk.queued = false;
                        chunk.completed = true;
                    }
                }
                count
            } else {
                let mut released: [*mut AudioBuffer; RELEASED_BUFFER_BATCH] =
                    [core::ptr::null_mut(); RELEASED_BUFFER_BATCH];
                let count = self
                    .out
                    .get_released_buffers(sf::OutMapAliasBuffer::from_mut_array(&mut released))
                    .map_err(|_| RfvpError::Backend)? as usize;
                for ptr in released
                    .iter()
                    .copied()
                    .take(cmp::min(count, RELEASED_BUFFER_BATCH))
                {
                    if ptr.is_null() {
                        continue;
                    }
                    let ptr_value = ptr as usize;
                    if let Some(chunk) = self
                        .chunks
                        .iter_mut()
                        .find(|chunk| chunk.buffer_ptr() == ptr_value)
                    {
                        chunk.queued = false;
                        chunk.completed = true;
                    }
                }
                count
            };
            if count < RELEASED_BUFFER_BATCH {
                break;
            }
        }
        self.chunks.retain(|chunk| !chunk.completed);
        Ok(())
    }

    fn shutdown(&mut self) {
        let _ = self.out.stop();
        if version::get_version() >= Version::new(4, 0, 0) {
            let _ = self.out.flush_buffers();
        }
        self.chunks.clear();
        self.playing = false;
    }
}

pub struct HorizonAudio {
    mixer: SoftAudioMixer<HorizonVorbisBackend>,
    output: Option<HorizonMasterOutput>,
    master_buffer: Vec<i16>,
}

impl HorizonAudio {
    pub fn new() -> Self {
        Self {
            mixer: SoftAudioMixer::new(
                HorizonVorbisBackend,
                SoftAudioConfig {
                    output_sample_rate: OUTPUT_SAMPLE_RATE,
                    mix_frames: MIX_FRAMES,
                    max_active_bgm: 2,
                    max_active_se: 16,
                    max_active_total: 24,
                },
            ),
            output: None,
            master_buffer: Vec::new(),
        }
    }

    fn output_mut(&mut self) -> RfvpResult<&mut HorizonMasterOutput> {
        if self.output.is_none() {
            self.output = Some(HorizonMasterOutput::open()?);
        }
        Ok(self.output.as_mut().expect("output was just initialized"))
    }
}

impl Default for HorizonAudio {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for HorizonAudio {
    fn drop(&mut self) {
        self.mixer.shutdown();
        if let Some(output) = &mut self.output {
            output.shutdown();
        }
    }
}

impl RfvpAudio for HorizonAudio {
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

    fn tick(&mut self, _delta_us: u64) -> RfvpResult<()> {
        let needed = MIX_FRAMES
            .checked_mul(2)
            .ok_or(RfvpError::CapacityExceeded)?;
        if self.master_buffer.len() != needed {
            self.master_buffer.resize(needed, 0);
        }
        let active = self.mixer.mix_next(&mut self.master_buffer)?;
        if active {
            let samples = self.master_buffer.clone();
            self.output_mut()?.submit_stereo_i16(&samples)?;
        } else if let Some(output) = &mut self.output {
            output.collect_released_buffers()?;
        }
        Ok(())
    }
}

struct HorizonVorbisBackend;

struct HorizonVorbisDecoder {
    ptr: *mut RfvpOggVorbis,
}

impl SoftAudioVorbis for HorizonVorbisBackend {
    type Decoder = HorizonVorbisDecoder;

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
            HorizonVorbisDecoder { ptr: decoder },
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

fn encode_master(samples: &[i16], actual_format: PcmFormat) -> Vec<u8> {
    match actual_format {
        PcmFormat::Float => {
            let mut out = Vec::with_capacity(samples.len() * core::mem::size_of::<f32>());
            for sample in samples {
                let value = *sample as f32 / i16::MAX as f32;
                out.extend_from_slice(&value.clamp(-1.0, 1.0).to_le_bytes());
            }
            out
        }
        _ => {
            let mut out = Vec::with_capacity(samples.len() * core::mem::size_of::<i16>());
            for sample in samples {
                out.extend_from_slice(&sample.to_le_bytes());
            }
            out
        }
    }
}
