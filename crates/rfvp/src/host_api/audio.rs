use super::error::RfvpResult;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudioStreamId(pub u32);

pub const BGM_LOGICAL_SLOT_COUNT: usize = 256;
pub const SE_LOGICAL_SLOT_COUNT: usize = 256;
pub const SE_STREAM_ID_BASE: u32 = 0x1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSlotKind {
    Bgm,
    Se,
}

impl AudioStreamId {
    pub const fn bgm(slot: usize) -> Self {
        Self(slot as u32)
    }

    pub const fn se(slot: usize) -> Self {
        Self(SE_STREAM_ID_BASE + slot as u32)
    }

    pub const fn slot_kind(self) -> Option<AudioSlotKind> {
        if self.0 < BGM_LOGICAL_SLOT_COUNT as u32 {
            Some(AudioSlotKind::Bgm)
        } else if self.0 >= SE_STREAM_ID_BASE
            && self.0 < SE_STREAM_ID_BASE + SE_LOGICAL_SLOT_COUNT as u32
        {
            Some(AudioSlotKind::Se)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSampleFormat {
    I16,
    F32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedAudioKind {
    Unknown,
    Wav,
    Ogg,
    Mp3,
    Flac,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioStreamDesc {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: AudioSampleFormat,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioParams {
    pub volume: f32,
    pub pan: f32,
    pub repeat: bool,
}

impl Default for AudioParams {
    fn default() -> Self {
        Self {
            volume: 1.0,
            pan: 0.0,
            repeat: false,
        }
    }
}

pub trait RfvpAudio {
    fn load_encoded(
        &mut self,
        id: AudioStreamId,
        kind: EncodedAudioKind,
        bytes: &[u8],
    ) -> RfvpResult<()>;

    fn create_stream(&mut self, id: AudioStreamId, desc: AudioStreamDesc) -> RfvpResult<()>;

    fn submit_i16(&mut self, id: AudioStreamId, samples: &[i16]) -> RfvpResult<()>;

    fn submit_f32(&mut self, id: AudioStreamId, samples: &[f32]) -> RfvpResult<()>;

    fn play(&mut self, id: AudioStreamId, params: AudioParams, fade_in_ms: u32) -> RfvpResult<()>;

    fn stop(&mut self, id: AudioStreamId, fade_ms: u32) -> RfvpResult<()>;

    fn pause(&mut self, id: AudioStreamId) -> RfvpResult<()>;

    fn resume(&mut self, id: AudioStreamId) -> RfvpResult<()>;

    fn set_params(&mut self, id: AudioStreamId, params: AudioParams) -> RfvpResult<()>;

    fn set_master_volume(&mut self, volume: f32) -> RfvpResult<()>;

    fn destroy_stream(&mut self, id: AudioStreamId);

    fn tick(&mut self, delta_us: u64) -> RfvpResult<()>;
}

pub trait SoftAudioVorbis {
    type Decoder;

    fn open(&mut self, bytes: &[u8]) -> RfvpResult<(Self::Decoder, AudioStreamDesc)>;

    fn decode_interleaved_i16(
        &mut self,
        decoder: &mut Self::Decoder,
        out_samples: &mut [i16],
    ) -> RfvpResult<usize>;

    fn seek_start(&mut self, decoder: &mut Self::Decoder) -> RfvpResult<()>;

    fn close(&mut self, decoder: Self::Decoder);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoftAudioConfig {
    pub output_sample_rate: u32,
    pub mix_frames: usize,
    pub max_active_bgm: usize,
    pub max_active_se: usize,
    pub max_active_total: usize,
}

impl Default for SoftAudioConfig {
    fn default() -> Self {
        Self {
            output_sample_rate: 48_000,
            mix_frames: 1024,
            max_active_bgm: 2,
            max_active_se: 16,
            max_active_total: 24,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VoiceState {
    Playing,
    Paused,
    Stopping,
    Ended,
}

enum LoadedAudio {
    Pcm {
        desc: AudioStreamDesc,
        samples: Vec<i16>,
    },
    Ogg {
        desc: AudioStreamDesc,
        bytes: Vec<u8>,
    },
}

enum VoiceSource<D> {
    Pcm,
    Ogg {
        decoder: Option<D>,
        decoded: Vec<i16>,
    },
}

struct ActiveVoice<D> {
    id: AudioStreamId,
    kind: AudioSlotKind,
    desc: AudioStreamDesc,
    params: AudioParams,
    state: VoiceState,
    source: VoiceSource<D>,
    phase: u64,
    phase_step: u64,
    gain: f32,
    fade_start_gain: f32,
    fade_target_gain: f32,
    fade_total_frames: u64,
    fade_remaining_frames: u64,
}

pub struct SoftAudioMixer<B: SoftAudioVorbis> {
    backend: B,
    config: SoftAudioConfig,
    loaded: Vec<(AudioStreamId, LoadedAudio)>,
    voices: Vec<ActiveVoice<B::Decoder>>,
    master_volume: f32,
    accum: Vec<i32>,
    decode_scratch: Vec<i16>,
}

impl<B: SoftAudioVorbis> SoftAudioMixer<B> {
    pub fn new(backend: B, config: SoftAudioConfig) -> Self {
        Self {
            backend,
            config,
            loaded: Vec::new(),
            voices: Vec::new(),
            master_volume: 1.0,
            accum: Vec::new(),
            decode_scratch: Vec::new(),
        }
    }

    pub const fn config(&self) -> SoftAudioConfig {
        self.config
    }

    pub fn load_encoded(
        &mut self,
        id: AudioStreamId,
        kind: EncodedAudioKind,
        bytes: &[u8],
    ) -> RfvpResult<()> {
        if is_ogg(kind, bytes) {
            let (decoder, desc) = self.backend.open(bytes)?;
            self.backend.close(decoder);
            self.replace_loaded(
                id,
                LoadedAudio::Ogg {
                    desc,
                    bytes: bytes.to_vec(),
                },
            );
            return Ok(());
        }

        let (desc, samples) = decode_wav_i16(kind, bytes)?;
        self.replace_loaded(id, LoadedAudio::Pcm { desc, samples });
        Ok(())
    }

    pub fn create_stream(&mut self, id: AudioStreamId, desc: AudioStreamDesc) -> RfvpResult<()> {
        if desc.sample_rate == 0 || desc.channels == 0 || desc.channels > 2 {
            return Err(super::error::RfvpError::InvalidArgument);
        }
        self.replace_loaded(
            id,
            LoadedAudio::Pcm {
                desc,
                samples: Vec::new(),
            },
        );
        Ok(())
    }

    pub fn submit_i16(&mut self, id: AudioStreamId, samples: &[i16]) -> RfvpResult<()> {
        let Some((_, LoadedAudio::Pcm { desc, samples: dst })) =
            self.loaded.iter_mut().find(|(slot_id, _)| *slot_id == id)
        else {
            return Err(super::error::RfvpError::NotFound);
        };
        if usize::from(desc.channels) == 0 || samples.len() % usize::from(desc.channels) != 0 {
            return Err(super::error::RfvpError::InvalidArgument);
        }
        dst.extend_from_slice(samples);
        Ok(())
    }

    pub fn submit_f32(&mut self, id: AudioStreamId, samples: &[f32]) -> RfvpResult<()> {
        let mut converted = Vec::with_capacity(samples.len());
        for sample in samples {
            converted.push(float_to_i16(*sample));
        }
        self.submit_i16(id, &converted)
    }

    pub fn play(
        &mut self,
        id: AudioStreamId,
        params: AudioParams,
        fade_in_ms: u32,
    ) -> RfvpResult<()> {
        let kind = id
            .slot_kind()
            .ok_or(super::error::RfvpError::InvalidArgument)?;
        self.remove_voice(id);
        self.ensure_voice_capacity(kind)?;

        let loaded = self
            .loaded
            .iter()
            .find(|(slot_id, _)| *slot_id == id)
            .ok_or(super::error::RfvpError::NotFound)?;
        let (desc, source) = match &loaded.1 {
            LoadedAudio::Pcm { desc, samples } => {
                if samples.is_empty() {
                    return Err(super::error::RfvpError::InvalidData);
                }
                (*desc, VoiceSource::Pcm)
            }
            LoadedAudio::Ogg { desc, bytes } => {
                let (decoder, open_desc) = self.backend.open(bytes)?;
                if open_desc.sample_rate != desc.sample_rate || open_desc.channels != desc.channels
                {
                    self.backend.close(decoder);
                    return Err(super::error::RfvpError::InvalidData);
                }
                (
                    *desc,
                    VoiceSource::Ogg {
                        decoder: Some(decoder),
                        decoded: Vec::new(),
                    },
                )
            }
        };

        let fade_frames = frames_from_ms(fade_in_ms, self.config.output_sample_rate);
        self.voices.push(ActiveVoice {
            id,
            kind,
            desc,
            params,
            state: VoiceState::Playing,
            source,
            phase: 0,
            phase_step: phase_step(desc.sample_rate, self.config.output_sample_rate)?,
            gain: if fade_frames == 0 { 1.0 } else { 0.0 },
            fade_start_gain: 0.0,
            fade_target_gain: 1.0,
            fade_total_frames: fade_frames,
            fade_remaining_frames: fade_frames,
        });
        Ok(())
    }

    pub fn stop(&mut self, id: AudioStreamId, fade_ms: u32) -> RfvpResult<()> {
        let Some(index) = self.voice_index(id) else {
            return Ok(());
        };
        if fade_ms == 0 {
            self.remove_voice_at(index);
            return Ok(());
        }
        let frames = frames_from_ms(fade_ms, self.config.output_sample_rate);
        let voice = &mut self.voices[index];
        voice.state = VoiceState::Stopping;
        voice.fade_start_gain = voice.gain;
        voice.fade_target_gain = 0.0;
        voice.fade_total_frames = frames;
        voice.fade_remaining_frames = frames;
        Ok(())
    }

    pub fn pause(&mut self, id: AudioStreamId) -> RfvpResult<()> {
        if let Some(index) = self.voice_index(id) {
            if self.voices[index].state == VoiceState::Playing {
                self.voices[index].state = VoiceState::Paused;
            }
        }
        Ok(())
    }

    pub fn resume(&mut self, id: AudioStreamId) -> RfvpResult<()> {
        if let Some(index) = self.voice_index(id) {
            if self.voices[index].state == VoiceState::Paused {
                self.voices[index].state = VoiceState::Playing;
            }
        }
        Ok(())
    }

    pub fn set_params(&mut self, id: AudioStreamId, params: AudioParams) -> RfvpResult<()> {
        if let Some(index) = self.voice_index(id) {
            self.voices[index].params = params;
        }
        Ok(())
    }

    pub fn set_master_volume(&mut self, volume: f32) -> RfvpResult<()> {
        self.master_volume = volume.max(0.0);
        Ok(())
    }

    pub fn destroy_stream(&mut self, id: AudioStreamId) {
        self.remove_voice(id);
        if let Some(index) = self.loaded.iter().position(|(slot_id, _)| *slot_id == id) {
            self.loaded.swap_remove(index);
        }
    }

    pub fn mix_next(&mut self, out: &mut [i16]) -> RfvpResult<bool> {
        let frames = self.config.mix_frames;
        if out.len() != frames * 2 {
            return Err(super::error::RfvpError::InvalidArgument);
        }
        if self.accum.len() != out.len() {
            self.accum.resize(out.len(), 0);
        }
        self.accum.fill(0);

        let mut any_active = false;
        let mut index = 0usize;
        while index < self.voices.len() {
            match self.voices[index].state {
                VoiceState::Paused | VoiceState::Ended => {
                    index += 1;
                    continue;
                }
                VoiceState::Playing | VoiceState::Stopping => {}
            }
            any_active = true;
            self.mix_voice(index)?;
            if self.voices[index].state == VoiceState::Ended {
                self.remove_voice_at(index);
            } else {
                index += 1;
            }
        }

        for (dst, sample) in out.iter_mut().zip(self.accum.iter().copied()) {
            *dst = clamp_i32_to_i16(sample);
        }
        Ok(any_active)
    }

    pub fn shutdown(&mut self) {
        while !self.voices.is_empty() {
            self.remove_voice_at(self.voices.len() - 1);
        }
        self.loaded.clear();
    }

    fn mix_voice(&mut self, index: usize) -> RfvpResult<()> {
        let frames = self.config.mix_frames;
        for out_frame in 0..frames {
            if self.voices[index].state == VoiceState::Ended {
                break;
            }
            self.advance_fade(index);
            let src_frame = (self.voices[index].phase >> 32) as usize;
            let Some((left, right)) = self.read_voice_frame(index, src_frame)? else {
                break;
            };
            let (left_gain, right_gain) = channel_gains(
                self.voices[index].params.volume * self.voices[index].gain * self.master_volume,
                self.voices[index].params.pan,
            );
            self.accum[out_frame * 2] += (left as f32 * left_gain) as i32;
            self.accum[out_frame * 2 + 1] += (right as f32 * right_gain) as i32;
            self.voices[index].phase = self.voices[index]
                .phase
                .wrapping_add(self.voices[index].phase_step);
        }
        self.compact_voice(index);
        Ok(())
    }

    fn read_voice_frame(
        &mut self,
        index: usize,
        src_frame: usize,
    ) -> RfvpResult<Option<(i16, i16)>> {
        let channels = usize::from(self.voices[index].desc.channels);
        let repeat = self.voices[index].params.repeat;
        match &mut self.voices[index].source {
            VoiceSource::Pcm => {
                let samples = match self
                    .loaded
                    .iter()
                    .find(|(id, _)| *id == self.voices[index].id)
                {
                    Some((_, LoadedAudio::Pcm { samples, .. })) => samples,
                    _ => {
                        self.voices[index].state = VoiceState::Ended;
                        return Ok(None);
                    }
                };
                let source_frames = samples.len() / channels;
                if src_frame >= source_frames {
                    if repeat {
                        self.voices[index].phase = 0;
                        return self.read_voice_frame(index, 0);
                    }
                    self.voices[index].state = VoiceState::Ended;
                    return Ok(None);
                }
                let base = src_frame * channels;
                let left = samples[base];
                let right = if channels == 2 {
                    samples[base + 1]
                } else {
                    left
                };
                Ok(Some((left, right)))
            }
            VoiceSource::Ogg { decoder, decoded } => {
                while decoded.len() / channels <= src_frame {
                    let start = decoded.len();
                    let chunk_samples = self
                        .config
                        .mix_frames
                        .checked_mul(channels)
                        .ok_or(super::error::RfvpError::CapacityExceeded)?;
                    self.decode_scratch.resize(chunk_samples, 0);
                    let count = self.backend.decode_interleaved_i16(
                        decoder
                            .as_mut()
                            .ok_or(super::error::RfvpError::InvalidData)?,
                        &mut self.decode_scratch,
                    )?;
                    if count == 0 {
                        if repeat {
                            self.backend.seek_start(
                                decoder
                                    .as_mut()
                                    .ok_or(super::error::RfvpError::InvalidData)?,
                            )?;
                            continue;
                        }
                        self.voices[index].state = VoiceState::Ended;
                        return Ok(None);
                    }
                    decoded.extend_from_slice(&self.decode_scratch[..count]);
                    if decoded.len() == start {
                        self.voices[index].state = VoiceState::Ended;
                        return Err(super::error::RfvpError::InvalidData);
                    }
                }
                let base = src_frame * channels;
                let left = decoded[base];
                let right = if channels == 2 {
                    decoded[base + 1]
                } else {
                    left
                };
                Ok(Some((left, right)))
            }
        }
    }

    fn compact_voice(&mut self, index: usize) {
        let channels = usize::from(self.voices[index].desc.channels);
        let consumed_frames = (self.voices[index].phase >> 32) as usize;
        let VoiceSource::Ogg { decoded, .. } = &mut self.voices[index].source else {
            return;
        };
        if consumed_frames == 0 {
            return;
        }
        let consumed_samples = consumed_frames.saturating_mul(channels).min(decoded.len());
        if consumed_samples > 0 {
            decoded.drain(..consumed_samples);
            self.voices[index].phase -= (consumed_samples / channels) as u64 * (1u64 << 32);
        }
    }

    fn advance_fade(&mut self, index: usize) {
        let voice = &mut self.voices[index];
        if voice.fade_remaining_frames == 0 {
            return;
        }
        voice.fade_remaining_frames -= 1;
        let done = voice.fade_total_frames - voice.fade_remaining_frames;
        let t = done as f32 / voice.fade_total_frames.max(1) as f32;
        voice.gain = voice.fade_start_gain + (voice.fade_target_gain - voice.fade_start_gain) * t;
        if voice.fade_remaining_frames == 0 {
            voice.gain = voice.fade_target_gain;
            if voice.state == VoiceState::Stopping {
                voice.state = VoiceState::Ended;
            }
        }
    }

    fn replace_loaded(&mut self, id: AudioStreamId, audio: LoadedAudio) {
        self.destroy_stream(id);
        self.loaded.push((id, audio));
    }

    fn ensure_voice_capacity(&self, kind: AudioSlotKind) -> RfvpResult<()> {
        let bgm = self
            .voices
            .iter()
            .filter(|voice| voice.kind == AudioSlotKind::Bgm)
            .count();
        let se = self
            .voices
            .iter()
            .filter(|voice| voice.kind == AudioSlotKind::Se)
            .count();
        if self.voices.len() >= self.config.max_active_total {
            return Err(super::error::RfvpError::CapacityExceeded);
        }
        match kind {
            AudioSlotKind::Bgm if bgm >= self.config.max_active_bgm => {
                Err(super::error::RfvpError::CapacityExceeded)
            }
            AudioSlotKind::Se if se >= self.config.max_active_se => {
                Err(super::error::RfvpError::CapacityExceeded)
            }
            _ => Ok(()),
        }
    }

    fn voice_index(&self, id: AudioStreamId) -> Option<usize> {
        self.voices.iter().position(|voice| voice.id == id)
    }

    fn remove_voice(&mut self, id: AudioStreamId) {
        if let Some(index) = self.voice_index(id) {
            self.remove_voice_at(index);
        }
    }

    fn remove_voice_at(&mut self, index: usize) {
        let mut voice = self.voices.swap_remove(index);
        if let VoiceSource::Ogg { decoder, .. } = &mut voice.source {
            if let Some(decoder) = decoder.take() {
                self.backend.close(decoder);
            }
        }
    }
}

impl<B: SoftAudioVorbis> Drop for SoftAudioMixer<B> {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn is_ogg(kind: EncodedAudioKind, bytes: &[u8]) -> bool {
    kind == EncodedAudioKind::Ogg
        || (kind == EncodedAudioKind::Unknown && bytes.len() >= 4 && &bytes[0..4] == b"OggS")
}

fn phase_step(source_rate: u32, output_rate: u32) -> RfvpResult<u64> {
    if source_rate == 0 || output_rate == 0 {
        return Err(super::error::RfvpError::InvalidArgument);
    }
    Ok(((source_rate as u64) << 32) / output_rate as u64)
}

fn frames_from_ms(ms: u32, sample_rate: u32) -> u64 {
    (u64::from(ms) * u64::from(sample_rate)) / 1000
}

fn channel_gains(volume: f32, pan: f32) -> (f32, f32) {
    let volume = volume.max(0.0);
    let pan = pan.clamp(-1.0, 1.0);
    let left = volume * if pan > 0.0 { 1.0 - pan } else { 1.0 };
    let right = volume * if pan < 0.0 { 1.0 + pan } else { 1.0 };
    (left, right)
}

fn float_to_i16(sample: f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0);
    if clamped >= 0.0 {
        (clamped * i16::MAX as f32) as i16
    } else {
        (clamped * -(i16::MIN as f32)) as i16
    }
}

fn clamp_i32_to_i16(value: i32) -> i16 {
    value.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

fn decode_wav_i16(kind: EncodedAudioKind, bytes: &[u8]) -> RfvpResult<(AudioStreamDesc, Vec<i16>)> {
    if kind != EncodedAudioKind::Unknown && kind != EncodedAudioKind::Wav {
        return Err(super::error::RfvpError::Unsupported);
    }
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(super::error::RfvpError::Unsupported);
    }
    let mut offset = 12usize;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits_per_sample = 0u16;
    let mut format = 0u16;
    let mut data: Option<&[u8]> = None;
    while offset
        .checked_add(8)
        .ok_or(super::error::RfvpError::CapacityExceeded)?
        <= bytes.len()
    {
        let id = &bytes[offset..offset + 4];
        let size = read_u32_le(bytes, offset + 4)? as usize;
        offset += 8;
        let end = offset
            .checked_add(size)
            .ok_or(super::error::RfvpError::CapacityExceeded)?;
        if end > bytes.len() {
            return Err(super::error::RfvpError::InvalidData);
        }
        match id {
            b"fmt " => {
                if size < 16 {
                    return Err(super::error::RfvpError::InvalidData);
                }
                format = read_u16_le(bytes, offset)?;
                channels = read_u16_le(bytes, offset + 2)?;
                sample_rate = read_u32_le(bytes, offset + 4)?;
                bits_per_sample = read_u16_le(bytes, offset + 14)?;
            }
            b"data" => data = Some(&bytes[offset..end]),
            _ => {}
        }
        offset = end + (size & 1);
    }
    if channels == 0 || channels > 2 || sample_rate == 0 {
        return Err(super::error::RfvpError::Unsupported);
    }
    let data = data.ok_or(super::error::RfvpError::InvalidData)?;
    let samples = match (format, bits_per_sample) {
        (1, 8) => data
            .iter()
            .map(|sample| ((*sample as i16) - 128) << 8)
            .collect(),
        (1, 16) => {
            if data.len() % 2 != 0 {
                return Err(super::error::RfvpError::InvalidData);
            }
            let mut samples = Vec::with_capacity(data.len() / 2);
            for chunk in data.chunks_exact(2) {
                samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
            }
            samples
        }
        (3, 32) => {
            if data.len() % 4 != 0 {
                return Err(super::error::RfvpError::InvalidData);
            }
            let mut samples = Vec::with_capacity(data.len() / 4);
            for chunk in data.chunks_exact(4) {
                samples.push(float_to_i16(f32::from_le_bytes([
                    chunk[0], chunk[1], chunk[2], chunk[3],
                ])));
            }
            samples
        }
        _ => return Err(super::error::RfvpError::Unsupported),
    };
    Ok((
        AudioStreamDesc {
            sample_rate,
            channels,
            sample_format: AudioSampleFormat::I16,
        },
        samples,
    ))
}

fn read_u16_le(bytes: &[u8], offset: usize) -> RfvpResult<u16> {
    let end = offset
        .checked_add(2)
        .ok_or(super::error::RfvpError::CapacityExceeded)?;
    let slice = bytes
        .get(offset..end)
        .ok_or(super::error::RfvpError::EndOfFile)?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> RfvpResult<u32> {
    let end = offset
        .checked_add(4)
        .ok_or(super::error::RfvpError::CapacityExceeded)?;
    let slice = bytes
        .get(offset..end)
        .ok_or(super::error::RfvpError::EndOfFile)?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}
