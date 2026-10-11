//! Sound on the PlayStation: what rfvp-psx-convert wrote in place of each sound file, played
//! by the hardware itself (no decoding, no mixing on the CPU):
//!
//! - `PSXS`: SPU-ADPCM, copied to sound RAM and played on an SPU voice;
//! - `PSXA`: a stream of `music.xa`, which the CD drive plays (main.c).

use alloc::collections::BTreeMap;

use rfvp::host_api::{AudioByteStream, AudioParams, AudioStreamDesc, AudioStreamId, EncodedAudioKind, RfvpAudio, RfvpResult};

extern "C" {
    // main.c
    fn rfvp_ps1_spu_upload(data: *const u8, len: u32) -> u32;
    fn rfvp_ps1_spu_alloc(len: u32) -> u32;
    fn rfvp_ps1_spu_write(addr: u32, data: *const u8, len: u32);
    fn rfvp_ps1_spu_play(voice: i32, addr: u32, rate: u32, vol_l: i32, vol_r: i32);
    fn rfvp_ps1_spu_volume(voice: i32, vol_l: i32, vol_r: i32);
    fn rfvp_ps1_spu_stop(voice: i32);
    fn rfvp_ps1_xa_play(start: u32, sectors: u32, loop_sector: u32, file: i32, channel: i32, channels: i32, repeat: i32);
    fn rfvp_ps1_xa_stop();
    fn rfvp_ps1_xa_volume(volume: i32);
}

const VOICES: i32 = 24;

#[derive(Clone, Copy)]
enum Sound {
    /// In sound RAM: its address, length and rate; its first and last blocks' flags are set for
    /// how it is played.
    Spu { addr: u32, len: u32, rate: u32, looped: bool },
    Xa { start: u32, sectors: u32, loop_sector: u32, file: u8, channel: u8, channels: u8 },
}

pub struct Ps1Audio {
    sounds: BTreeMap<u32, Sound>,
    voice_of: BTreeMap<u32, i32>,
    next_voice: i32,
    xa_owner: Option<u32>,
    master: f32,
    params: BTreeMap<u32, AudioParams>,
}

impl Ps1Audio {
    pub fn new() -> Self {
        Self {
            sounds: BTreeMap::new(),
            voice_of: BTreeMap::new(),
            next_voice: 0,
            xa_owner: None,
            master: 1.0,
            params: BTreeMap::new(),
        }
    }

    fn volumes(&self, params: AudioParams) -> (i32, i32) {
        let v = (params.volume * self.master).clamp(0.0, 1.0) * 0x3fff as f32;
        let pan = params.pan.clamp(-1.0, 1.0);
        let l = v * if pan > 0.0 { 1.0 - pan } else { 1.0 };
        let r = v * if pan < 0.0 { 1.0 + pan } else { 1.0 };
        (l as i32, r as i32)
    }

    fn voice(&mut self, id: u32) -> i32 {
        if let Some(&v) = self.voice_of.get(&id) {
            return v;
        }
        let v = self.next_voice;
        self.next_voice = (self.next_voice + 1) % VOICES;
        // The voice may have belonged to another sound: that one is now silent.
        self.voice_of.retain(|_, &mut other| other != v);
        self.voice_of.insert(id, v);
        v
    }
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// XA channels interleaved in a file of this coding at double speed (see rfvp-psx's xa.rs).
fn xa_channels(coding: u8) -> u8 {
    let stereo = coding & 1 != 0;
    let low_rate = coding & 4 != 0;
    match (stereo, low_rate) {
        (true, true) => 16,
        (true, false) => 8,
        (false, true) => 32,
        (false, false) => 16,
    }
}

/// The ADPCM block flags for a sound played once (end) or repeated (loop start, loop end).
fn set_loop_flags(data: &mut [u8], looped: bool) {
    if data.len() < 16 {
        return;
    }
    let last = data.len() / 16 * 16 - 16;
    data[1] = if looped { 0x04 } else { 0 };
    data[last + 1] = if looped { 0x03 } else { 0x01 };
    if last == 0 {
        data[1] = if looped { 0x07 } else { 0x01 };
    }
}

impl RfvpAudio for Ps1Audio {
    fn load_encoded(&mut self, id: AudioStreamId, _kind: EncodedAudioKind, bytes: &[u8]) -> RfvpResult<()> {
        let sound = match bytes.get(0..4) {
            Some(b"PSXS") => {
                let rate = u32_at(bytes, 4).unwrap_or(22050);
                let len = u32_at(bytes, 12).unwrap_or(0).min(bytes.len().saturating_sub(16) as u32);
                let mut data = bytes[16..16 + len as usize].to_vec();
                set_loop_flags(&mut data, false);
                let addr = unsafe { rfvp_ps1_spu_upload(data.as_ptr(), len) };
                if addr == 0 {
                    return Err(rfvp::host_api::RfvpError::CapacityExceeded);
                }
                Sound::Spu { addr, len, rate, looped: false }
            }
            Some(b"PSXA") => {
                let file = bytes.get(4).copied().unwrap_or(1);
                let channel = bytes.get(6).copied().unwrap_or(0);
                let coding = bytes.get(7).copied().unwrap_or(5);
                let first = u32_at(bytes, 8).unwrap_or(0);
                let sectors = u32_at(bytes, 12).unwrap_or(0);
                let loop_sector = u32_at(bytes, 20).unwrap_or(0);
                let mut size = 0u32;
                let handle = unsafe { crate::ffi::rfvp_ps1_file_find(c"music.xa".as_ptr(), &mut size) };
                if handle < 0 {
                    return Err(rfvp::host_api::RfvpError::NotFound);
                }
                let lba = unsafe { crate::ffi::rfvp_ps1_file_lba(handle) };
                Sound::Xa { start: lba + first, sectors, loop_sector, file, channel, channels: xa_channels(coding) }
            }
            _ => return Err(rfvp::host_api::RfvpError::Unsupported),
        };
        self.sounds.insert(id.0, sound);
        Ok(())
    }

    /// Every sound arrives this way (see the engine's `STREAM_FROM_BYTES`): an SPU sound is
    /// copied to sound RAM a piece at a time, an XA reference is read whole (it is small).
    fn load_stream(&mut self, id: AudioStreamId, kind: EncodedAudioKind, stream: AudioByteStream) -> RfvpResult<()> {
        let mut header = [0u8; 16];
        let n = stream.read_at(0, &mut header)?;
        if n < 16 || &header[0..4] != b"PSXS" {
            let bytes = stream.read_all()?;
            return self.load_encoded(id, kind, &bytes);
        }
        let rate = u32_at(&header, 4).unwrap_or(22050);
        let len = u32_at(&header, 12).unwrap_or(0);
        let addr = unsafe { rfvp_ps1_spu_alloc(len) };
        if addr == 0 || len < 16 {
            return Err(rfvp::host_api::RfvpError::CapacityExceeded);
        }
        let mut chunk = [0u8; 2048];
        let mut done = 0u32;
        while done < len {
            let want = (len - done).min(chunk.len() as u32) as usize;
            let got = stream.read_at(16 + done as u64, &mut chunk[..want])?;
            if got == 0 {
                break;
            }
            chunk[got..].fill(0);
            let padded = (got + 63) & !63;
            unsafe { rfvp_ps1_spu_write(addr + done, chunk.as_ptr(), padded as u32) };
            done += got as u32;
        }
        patch_loop(addr, len, false);
        self.sounds.insert(id.0, Sound::Spu { addr, len, rate, looped: false });
        Ok(())
    }

    fn create_stream(&mut self, _id: AudioStreamId, _desc: AudioStreamDesc) -> RfvpResult<()> {
        // PCM streams (a movie's sound) are not played: movies carry their own XA.
        Ok(())
    }

    fn submit_i16(&mut self, _id: AudioStreamId, _samples: &[i16]) -> RfvpResult<()> {
        Ok(())
    }

    fn submit_f32(&mut self, _id: AudioStreamId, _samples: &[f32]) -> RfvpResult<()> {
        Ok(())
    }

    fn play(&mut self, id: AudioStreamId, params: AudioParams, _fade_in_ms: u32) -> RfvpResult<()> {
        let Some(sound) = self.sounds.get(&id.0).copied() else {
            return Ok(());
        };
        self.params.insert(id.0, params);
        match sound {
            Sound::Spu { addr, len, rate, looped } => {
                if looped != params.repeat {
                    self.sounds.insert(id.0, Sound::Spu { addr, len, rate, looped: params.repeat });
                    patch_loop(addr, len, params.repeat);
                }
                let (l, r) = self.volumes(params);
                let voice = self.voice(id.0);
                unsafe { rfvp_ps1_spu_play(voice, addr, rate, l, r) };
            }
            Sound::Xa { start, sectors, loop_sector, file, channel, channels } => {
                self.xa_owner = Some(id.0);
                let (l, _) = self.volumes(AudioParams { pan: 0.0, ..params });
                unsafe {
                    rfvp_ps1_xa_volume(l);
                    rfvp_ps1_xa_play(start, sectors, loop_sector, file as i32, channel as i32, channels as i32, params.repeat as i32)
                };
            }
        }
        Ok(())
    }

    fn stop(&mut self, id: AudioStreamId, _fade_ms: u32) -> RfvpResult<()> {
        if self.xa_owner == Some(id.0) {
            self.xa_owner = None;
            unsafe { rfvp_ps1_xa_stop() };
        }
        if let Some(&voice) = self.voice_of.get(&id.0) {
            unsafe { rfvp_ps1_spu_stop(voice) };
        }
        Ok(())
    }

    fn pause(&mut self, id: AudioStreamId) -> RfvpResult<()> {
        self.stop(id, 0)
    }

    fn resume(&mut self, id: AudioStreamId) -> RfvpResult<()> {
        let params = self.params.get(&id.0).copied().unwrap_or_default();
        self.play(id, params, 0)
    }

    fn set_params(&mut self, id: AudioStreamId, params: AudioParams) -> RfvpResult<()> {
        self.params.insert(id.0, params);
        let (l, r) = self.volumes(params);
        if self.xa_owner == Some(id.0) {
            unsafe { rfvp_ps1_xa_volume(l.max(r)) };
        } else if let Some(&voice) = self.voice_of.get(&id.0) {
            unsafe { rfvp_ps1_spu_volume(voice, l, r) };
        }
        Ok(())
    }

    fn set_master_volume(&mut self, volume: f32) -> RfvpResult<()> {
        self.master = volume.clamp(0.0, 1.0);
        Ok(())
    }

    fn destroy_stream(&mut self, id: AudioStreamId) {
        let _ = self.stop(id, 0);
        self.sounds.remove(&id.0);
        self.voice_of.remove(&id.0);
        self.params.remove(&id.0);
    }

    fn tick(&mut self, _delta_us: u64) -> RfvpResult<()> {
        Ok(())
    }
}

extern "C" {
    fn rfvp_ps1_spu_patch_loop(addr: u32, len: u32, looped: i32);
}

/// Set the loop flags of a sound already in sound RAM (main.c rewrites its first and last
/// blocks' flag bytes).
fn patch_loop(addr: u32, len: u32, looped: bool) {
    unsafe { rfvp_ps1_spu_patch_loop(addr, len, looped as i32) };
}
