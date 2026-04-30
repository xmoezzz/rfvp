#![allow(unexpected_cfgs)]
#![cfg(any(target_os = "horizon", target_vendor = "nintendo", rfvp_switch))]
#![no_std]

pub const RFVP_SWITCH_AUDIO_API_VERSION: u32 = 1;
pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u32 = 2;
pub const RING_SAMPLES: usize = 48_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchAudioError {
    RingFull,
    RingEmpty,
}

pub struct SwitchAudioBackend {
    ring: [i16; RING_SAMPLES],
    read: usize,
    write: usize,
    len: usize,
    volume_q15: i32,
}

impl SwitchAudioBackend {
    pub const fn new() -> Self {
        Self {
            ring: [0; RING_SAMPLES],
            read: 0,
            write: 0,
            len: 0,
            volume_q15: 32767,
        }
    }

    pub fn reset(&mut self) {
        self.read = 0;
        self.write = 0;
        self.len = 0;
    }

    pub fn set_volume_q15(&mut self, volume_q15: i32) {
        self.volume_q15 = volume_q15.clamp(0, 32767);
    }

    pub fn queued_samples(&self) -> usize {
        self.len
    }

    pub fn available_samples(&self) -> usize {
        RING_SAMPLES - self.len
    }

    pub fn push_interleaved_i16(&mut self, samples: &[i16]) -> Result<usize, SwitchAudioError> {
        if samples.is_empty() {
            return Ok(0);
        }
        let mut written = 0usize;
        for &s in samples {
            if self.len == RING_SAMPLES {
                return if written == 0 {
                    Err(SwitchAudioError::RingFull)
                } else {
                    Ok(written)
                };
            }
            let scaled = ((s as i32 * self.volume_q15) >> 15)
                .clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            self.ring[self.write] = scaled;
            self.write = (self.write + 1) % RING_SAMPLES;
            self.len += 1;
            written += 1;
        }
        Ok(written)
    }

    pub fn push_interleaved_f32(&mut self, samples: &[f32]) -> Result<usize, SwitchAudioError> {
        let mut written = 0usize;
        for &s in samples {
            if self.len == RING_SAMPLES {
                return if written == 0 {
                    Err(SwitchAudioError::RingFull)
                } else {
                    Ok(written)
                };
            }
            let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
            self.push_interleaved_i16(core::slice::from_ref(&v))?;
            written += 1;
        }
        Ok(written)
    }

    pub fn pop_interleaved_i16(&mut self, out: &mut [i16]) -> Result<usize, SwitchAudioError> {
        if out.is_empty() {
            return Ok(0);
        }
        if self.len == 0 {
            return Err(SwitchAudioError::RingEmpty);
        }
        let mut read_count = 0usize;
        for slot in out.iter_mut() {
            if self.len == 0 {
                break;
            }
            *slot = self.ring[self.read];
            self.read = (self.read + 1) % RING_SAMPLES;
            self.len -= 1;
            read_count += 1;
        }
        Ok(read_count)
    }
}

impl Default for SwitchAudioBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_audio_api_version() -> u32 {
    RFVP_SWITCH_AUDIO_API_VERSION
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_audio_init(audio: *mut SwitchAudioBackend) -> i32 {
    if audio.is_null() {
        return -1;
    }
    unsafe {
        audio.write(SwitchAudioBackend::new());
    }
    0
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_audio_queued_samples(audio: *const SwitchAudioBackend) -> u32 {
    if audio.is_null() {
        return 0;
    }
    unsafe { (*audio).queued_samples() as u32 }
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_audio_push_i16(
    audio: *mut SwitchAudioBackend,
    samples: *const i16,
    len: usize,
) -> i32 {
    if audio.is_null() || samples.is_null() {
        return -1;
    }
    let input = unsafe { core::slice::from_raw_parts(samples, len) };
    match unsafe { (*audio).push_interleaved_i16(input) } {
        Ok(n) => n as i32,
        Err(_) => -2,
    }
}

#[cfg(feature = "ffi")]
#[no_mangle]
pub extern "C" fn rfvp_switch_audio_pop_i16(
    audio: *mut SwitchAudioBackend,
    out: *mut i16,
    len: usize,
) -> i32 {
    if audio.is_null() || out.is_null() {
        return -1;
    }
    let output = unsafe { core::slice::from_raw_parts_mut(out, len) };
    match unsafe { (*audio).pop_interleaved_i16(output) } {
        Ok(n) => n as i32,
        Err(_) => 0,
    }
}
