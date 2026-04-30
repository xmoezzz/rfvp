use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;

use crate::platform_time::Duration;

#[derive(Clone, Copy, Debug, Default)]
pub struct Tween {
    pub duration: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioBus {
    Bgm,
    Se,
    Movie,
}

#[derive(Clone, Copy, Debug)]
pub struct PlayParams {
    pub bus: AudioBus,
    pub slot: i32,
    pub repeat: bool,
    pub volume: f32,
    pub pan: f32,
    pub fade_in: Tween,
}

#[cfg(not(rfvp_switch))]
mod backend {
    use super::*;
    use kira::{AudioManager as KiraAudioManager, AudioManagerSettings};
    use kira::sound::Region;
    use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
    use kira::track::{TrackBuilder, TrackHandle};

    #[derive(Clone)]
    pub struct SoundData {
        inner: StaticSoundData,
    }

    pub struct SoundHandle {
        inner: StaticSoundHandle,
    }

    pub struct AudioTrackHandle {
        _inner: TrackHandle,
    }

    pub struct AudioManager {
        manager: Mutex<KiraAudioManager>,
    }

    impl Debug for AudioManager {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("AudioManager").finish()
        }
    }

    impl AudioManager {
        pub fn new() -> Self {
            let mut settings = AudioManagerSettings::default();
            settings.capacities.sub_track_capacity = 512;

            let mgr = KiraAudioManager::new(settings)
                .expect("failed to create Kira AudioManager");
            Self {
                manager: Mutex::new(mgr),
            }
        }

        pub fn kira_manager(&self) -> &Mutex<KiraAudioManager> {
            &self.manager
        }

        pub fn play(&self, data: StaticSoundData) -> StaticSoundHandle {
            let mut mgr = self.manager.lock().unwrap();
            mgr.play(data).expect("failed to play sound")
        }

        pub fn create_track(&self) -> AudioTrackHandle {
            let mut mgr = self.manager.lock().unwrap();
            let track = mgr
                .add_sub_track(TrackBuilder::new())
                .expect("Failed to create audio sub track");
            AudioTrackHandle { _inner: track }
        }

        pub fn load_sound(&self, bytes: Vec<u8>) -> Result<SoundData> {
            let cursor = std::io::Cursor::new(bytes);
            let inner = StaticSoundData::from_cursor(cursor)?;
            Ok(SoundData { inner })
        }

        pub fn play_sound(&self, data: &SoundData, params: PlayParams) -> Result<SoundHandle> {
            let loop_region = params.repeat.then_some(Region::default());
            let settings = StaticSoundSettings::new()
                .panning(kira::Panning::from(params.pan))
                .volume(params.volume)
                .fade_in_tween(params.fade_in.into_kira())
                .loop_region(loop_region)
                .playback_rate(1.0);

            let sound = data.inner.clone().with_settings(settings);
            let mut mgr = self.manager.lock().unwrap();
            let inner = mgr.play(sound)?;
            Ok(SoundHandle { inner })
        }

        pub fn master_vol(&self, vol: f32) {
            let mut mgr = self.manager.lock().unwrap();
            mgr.main_track().set_volume(vol, crate::rfvp_audio::Tween::default().into_kira());
        }

        pub fn set_handle_volume(&self, handle: &mut SoundHandle, volume: f32, tween: Tween) {
            handle.set_volume(volume, tween);
        }

        pub fn set_handle_panning(&self, handle: &mut SoundHandle, pan: f32, tween: Tween) {
            handle.set_panning(pan, tween);
        }

        pub fn stop_handle(&self, handle: &mut SoundHandle, tween: Tween) {
            handle.stop(tween);
        }
    }

    impl Tween {
        pub fn into_kira(self) -> kira::Tween {
            kira::Tween {
                duration: self.duration,
                ..Default::default()
            }
        }
    }

    impl SoundHandle {
        pub fn set_volume(&mut self, volume: f32, tween: Tween) {
            self.inner.set_volume(volume, tween.into_kira());
        }

        pub fn set_panning(&mut self, pan: f32, tween: Tween) {
            self.inner.set_panning(kira::Panning::from(pan), tween.into_kira());
        }

        pub fn stop(&mut self, tween: Tween) {
            self.inner.stop(tween.into_kira());
        }

        pub fn is_advancing(&self) -> bool {
            self.inner.state().is_advancing()
        }
    }
}

#[cfg(rfvp_switch)]
mod backend {
    use super::*;
    use std::collections::HashMap;
    use std::io::Cursor;

    use anyhow::{anyhow, bail};
    use lewton::inside_ogg::OggStreamReader;
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::errors::Error as SymphoniaError;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    use symphonia::default::{get_codecs, get_probe};
    use rfvp_switch_audio::{SwitchAudioBackend, SAMPLE_RATE};

    #[derive(Clone, Debug)]
    pub struct SoundData {
        pub id: u64,
        pub bytes: Arc<[u8]>,
    }

    #[derive(Clone, Copy, Debug)]
    pub struct SoundHandle {
        pub id: u64,
        pub stopped: bool,
    }

    #[derive(Clone, Copy, Debug)]
    pub struct AudioTrackHandle {
        pub id: u64,
    }

    #[derive(Clone, Debug)]
    pub enum AudioCommand {
        CreateTrack {
            track_id: u64,
        },
        LoadSound {
            sound_id: u64,
            bytes: Arc<[u8]>,
        },
        PlaySound {
            handle_id: u64,
            sound_id: u64,
            bus: AudioBus,
            slot: i32,
            repeat: bool,
            volume: f32,
            pan: f32,
            fade_in_samples: u64,
        },
        SetVolume {
            handle_id: u64,
            volume: f32,
            tween_samples: u64,
        },
        SetPanning {
            handle_id: u64,
            pan: f32,
            tween_samples: u64,
        },
        Stop {
            handle_id: u64,
            tween_samples: u64,
        },
        MasterVolume {
            volume: f32,
        },
    }

    #[derive(Clone, Debug)]
    struct Pcm16Sound {
        channels: u16,
        sample_rate: u32,
        samples: Vec<i16>,
    }

    #[derive(Clone, Debug)]
    struct ScalarRamp {
        current: f32,
        target: f32,
        step: f32,
        remaining_samples: u64,
    }

    impl ScalarRamp {
        fn new(value: f32) -> Self {
            Self {
                current: value,
                target: value,
                step: 0.0,
                remaining_samples: 0,
            }
        }

        fn set_target(&mut self, target: f32, samples: u64) {
            self.target = target;
            if samples == 0 {
                self.current = target;
                self.step = 0.0;
                self.remaining_samples = 0;
                return;
            }

            self.step = (target - self.current) / samples as f32;
            self.remaining_samples = samples;
        }

        fn next_sample_value(&mut self) -> f32 {
            let value = self.current;
            if self.remaining_samples != 0 {
                self.current += self.step;
                self.remaining_samples -= 1;
                if self.remaining_samples == 0 {
                    self.current = self.target;
                    self.step = 0.0;
                }
            }
            value
        }

        fn is_finished(&self) -> bool {
            self.remaining_samples == 0
        }
    }

    #[derive(Clone, Debug)]
    struct PlayingSound {
        handle_id: u64,
        sound_id: u64,
        repeat: bool,
        volume: ScalarRamp,
        pan: ScalarRamp,
        pos_frames: f64,
        active: bool,
        stop_when_volume_ramp_finishes: bool,
    }

    pub struct AudioManager {
        next_id: AtomicU64,
        commands: Mutex<Vec<AudioCommand>>,
        loaded: Mutex<HashMap<u64, Pcm16Sound>>,
        playing: Mutex<Vec<PlayingSound>>,
        backend: Mutex<SwitchAudioBackend>,
        master_volume: Mutex<f32>,
    }

    impl Debug for AudioManager {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("AudioManager").finish()
        }
    }

    impl AudioManager {
        pub fn new() -> Self {
            Self {
                next_id: AtomicU64::new(1),
                commands: Mutex::new(Vec::new()),
                loaded: Mutex::new(HashMap::new()),
                playing: Mutex::new(Vec::new()),
                backend: Mutex::new(SwitchAudioBackend::new()),
                master_volume: Mutex::new(1.0),
            }
        }

        fn alloc_id(&self) -> u64 {
            self.next_id.fetch_add(1, Ordering::Relaxed)
        }

        fn push_command(&self, command: AudioCommand) {
            self.commands.lock().unwrap().push(command);
        }

        pub fn create_track(&self) -> AudioTrackHandle {
            let id = self.alloc_id();
            self.push_command(AudioCommand::CreateTrack { track_id: id });
            AudioTrackHandle { id }
        }

        pub fn load_sound(&self, bytes: Vec<u8>) -> Result<SoundData> {
            let id = self.alloc_id();
            let bytes: Arc<[u8]> = Arc::<[u8]>::from(bytes);
            self.push_command(AudioCommand::LoadSound {
                sound_id: id,
                bytes: bytes.clone(),
            });
            Ok(SoundData { id, bytes })
        }

        pub fn play_sound(&self, data: &SoundData, params: PlayParams) -> Result<SoundHandle> {
            let id = self.alloc_id();
            self.push_command(AudioCommand::PlaySound {
                handle_id: id,
                sound_id: data.id,
                bus: params.bus,
                slot: params.slot,
                repeat: params.repeat,
                volume: params.volume,
                pan: params.pan,
                fade_in_samples: tween_to_samples(params.fade_in),
            });
            Ok(SoundHandle { id, stopped: false })
        }

        pub fn master_vol(&self, vol: f32) {
            self.push_command(AudioCommand::MasterVolume { volume: vol });
        }

        pub fn set_handle_volume(&self, handle: &mut SoundHandle, volume: f32, tween: Tween) {
            self.push_command(AudioCommand::SetVolume {
                handle_id: handle.id,
                volume,
                tween_samples: tween_to_samples(tween),
            });
        }

        pub fn set_handle_panning(&self, handle: &mut SoundHandle, pan: f32, tween: Tween) {
            self.push_command(AudioCommand::SetPanning {
                handle_id: handle.id,
                pan,
                tween_samples: tween_to_samples(tween),
            });
        }

        pub fn stop_handle(&self, handle: &mut SoundHandle, tween: Tween) {
            handle.stopped = true;
            self.push_command(AudioCommand::Stop {
                handle_id: handle.id,
                tween_samples: tween_to_samples(tween),
            });
        }

        pub fn drain_commands(&self) -> Vec<AudioCommand> {
            let mut commands = self.commands.lock().unwrap();
            std::mem::take(&mut *commands)
        }

        fn process_pending_commands(&self) {
            let commands = self.drain_commands();
            if commands.is_empty() {
                return;
            }

            let mut loaded = self.loaded.lock().unwrap();
            let mut playing = self.playing.lock().unwrap();
            let mut master_volume = self.master_volume.lock().unwrap();

            for command in commands {
                match command {
                    AudioCommand::CreateTrack { .. } => {}
                    AudioCommand::LoadSound { sound_id, bytes } => match decode_sound_payload(&bytes) {
                        Ok(sound) => {
                            loaded.insert(sound_id, sound);
                        }
                        Err(e) => {
                            log::warn!("Switch audio: unsupported sound payload id={sound_id}: {e:#}");
                        }
                    },
                    AudioCommand::PlaySound {
                        handle_id,
                        sound_id,
                        repeat,
                        volume,
                        pan,
                        fade_in_samples,
                        ..
                    } => {
                        if loaded.contains_key(&sound_id) {
                            playing.retain(|p| p.handle_id != handle_id);
                            let mut volume_ramp = ScalarRamp::new(if fade_in_samples == 0 { volume } else { 0.0 });
                            volume_ramp.set_target(volume, fade_in_samples);
                            playing.push(PlayingSound {
                                handle_id,
                                sound_id,
                                repeat,
                                volume: volume_ramp,
                                pan: ScalarRamp::new(pan),
                                pos_frames: 0.0,
                                active: true,
                                stop_when_volume_ramp_finishes: false,
                            });
                        } else {
                            log::warn!("Switch audio: play ignored for unloaded sound id={sound_id}");
                        }
                    }
                    AudioCommand::SetVolume { handle_id, volume, tween_samples } => {
                        for p in playing.iter_mut().filter(|p| p.handle_id == handle_id) {
                            p.volume.set_target(volume, tween_samples);
                            p.stop_when_volume_ramp_finishes = false;
                        }
                    }
                    AudioCommand::SetPanning { handle_id, pan, tween_samples } => {
                        for p in playing.iter_mut().filter(|p| p.handle_id == handle_id) {
                            p.pan.set_target(pan, tween_samples);
                        }
                    }
                    AudioCommand::Stop { handle_id, tween_samples } => {
                        if tween_samples == 0 {
                            playing.retain(|p| p.handle_id != handle_id);
                        } else {
                            for p in playing.iter_mut().filter(|p| p.handle_id == handle_id) {
                                p.volume.set_target(0.0, tween_samples);
                                p.stop_when_volume_ramp_finishes = true;
                            }
                        }
                    }
                    AudioCommand::MasterVolume { volume } => {
                        *master_volume = volume;
                    }
                }
            }
        }

        pub fn mix_to_ring(&self, duration_ms: u32) -> usize {
            self.process_pending_commands();
            if self.playing.lock().unwrap().is_empty() {
                return 0;
            }

            let frame_count = ((SAMPLE_RATE as u64)
                .saturating_mul(duration_ms.max(1) as u64)
                / 1000)
                .max(1) as usize;
            let mut mix = vec![0i32; frame_count.saturating_mul(2)];
            let master_volume = *self.master_volume.lock().unwrap();
            let loaded = self.loaded.lock().unwrap();
            let mut playing = self.playing.lock().unwrap();

            for p in playing.iter_mut() {
                if !p.active {
                    continue;
                }
                let Some(sound) = loaded.get(&p.sound_id) else {
                    p.active = false;
                    continue;
                };
                let channels = sound.channels.max(1) as usize;
                let source_frame_count = sound.samples.len() / channels;
                if source_frame_count == 0 || sound.sample_rate == 0 {
                    p.active = false;
                    continue;
                }

                let step = sound.sample_rate as f64 / SAMPLE_RATE as f64;

                for frame in 0..frame_count {
                    let mut src_frame = p.pos_frames.floor() as usize;
                    if src_frame >= source_frame_count {
                        if p.repeat {
                            p.pos_frames = 0.0;
                            src_frame = 0;
                        } else {
                            p.active = false;
                            break;
                        }
                    }

                    let volume = p.volume.next_sample_value();
                    let pan = p.pan.next_sample_value().clamp(0.0, 1.0);
                    let left_gain = if pan <= 0.5 { 1.0 } else { (1.0 - pan) * 2.0 };
                    let right_gain = if pan >= 0.5 { 1.0 } else { pan * 2.0 };
                    let gain_l = volume * master_volume * left_gain;
                    let gain_r = volume * master_volume * right_gain;

                    let base = src_frame * channels;
                    let (l, r) = if channels == 1 {
                        let s = sound.samples[base] as f32;
                        (s, s)
                    } else {
                        (sound.samples[base] as f32, sound.samples[base + 1] as f32)
                    };
                    let out = frame * 2;
                    mix[out] = mix[out].saturating_add((l * gain_l).round() as i32);
                    mix[out + 1] = mix[out + 1].saturating_add((r * gain_r).round() as i32);
                    p.pos_frames += step;

                    if p.stop_when_volume_ramp_finishes && p.volume.is_finished() {
                        p.active = false;
                        break;
                    }
                }
            }

            playing.retain(|p| p.active);
            drop(playing);
            drop(loaded);

            let mut out = Vec::with_capacity(mix.len());
            for s in mix {
                out.push(s.clamp(i16::MIN as i32, i16::MAX as i32) as i16);
            }

            self.backend
                .lock()
                .unwrap()
                .push_interleaved_i16(&out)
                .unwrap_or(0)
        }

        pub fn pop_interleaved_i16(&self, out: &mut [i16]) -> usize {
            self.backend
                .lock()
                .unwrap()
                .pop_interleaved_i16(out)
                .unwrap_or(0)
        }

        pub fn queued_samples(&self) -> usize {
            self.backend.lock().unwrap().queued_samples()
        }
    }

    impl SoundHandle {
        pub fn set_volume(&mut self, _volume: f32, _tween: Tween) {}

        pub fn set_panning(&mut self, _pan: f32, _tween: Tween) {}

        pub fn stop(&mut self, _tween: Tween) {
            self.stopped = true;
        }

        pub fn is_advancing(&self) -> bool {
            !self.stopped
        }
    }

    fn tween_to_samples(tween: Tween) -> u64 {
        duration_to_samples(tween.duration)
    }

    fn duration_to_samples(duration: Duration) -> u64 {
        let nanos = duration.as_nanos();
        if nanos == 0 {
            return 0;
        }
        let samples = (nanos
            .saturating_mul(SAMPLE_RATE as u128)
            .saturating_add(500_000_000)
            / 1_000_000_000) as u64;
        samples.max(1)
    }

    fn decode_sound_payload(bytes: &[u8]) -> Result<Pcm16Sound> {
        match decode_symphonia_pcm16(bytes) {
            Ok(sound) => Ok(sound),
            Err(sym_err) => match decode_wav_pcm16le(bytes) {
                Ok(sound) => Ok(sound),
                Err(wav_err) => match decode_ogg_vorbis_pcm16(bytes) {
                    Ok(sound) => Ok(sound),
                    Err(ogg_err) => Err(anyhow!(
                        "unsupported audio payload; Symphonia error: {sym_err:#}; WAV error: {wav_err:#}; OGG/Vorbis error: {ogg_err:#}"
                    )),
                },
            },
        }
    }

    fn decode_symphonia_pcm16(bytes: &[u8]) -> Result<Pcm16Sound> {
        let cursor = Cursor::new(bytes.to_vec());
        let media_source = MediaSourceStream::new(Box::new(cursor), Default::default());
        let hint = Hint::new();
        let probed = get_probe().format(
            &hint,
            media_source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?;
        let mut format = probed.format;
        let track = format
            .default_track()
            .ok_or_else(|| anyhow!("audio stream has no default track"))?;
        let track_id = track.id;
        let codec_params = &track.codec_params;
        let mut decoder = get_codecs().make(codec_params, &DecoderOptions::default())?;

        let mut samples: Vec<i16> = Vec::new();
        let mut channels: Option<u16> = None;
        let mut sample_rate: Option<u32> = None;

        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(err)) if err.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(SymphoniaError::ResetRequired) => bail!("Symphonia decoder reset required"),
                Err(err) => return Err(err.into()),
            };

            if packet.track_id() != track_id {
                continue;
            }

            let decoded = match decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(err) => return Err(err.into()),
            };
            let spec = *decoded.spec();
            if spec.channels.count() == 0 || spec.rate == 0 {
                bail!(
                    "invalid Symphonia decoded spec: channels={} sample_rate={}",
                    spec.channels.count(),
                    spec.rate
                );
            }

            if let Some(existing) = channels {
                if existing != spec.channels.count() as u16 {
                    bail!(
                        "audio stream changed channel count: {} -> {}",
                        existing,
                        spec.channels.count()
                    );
                }
            } else {
                channels = Some(spec.channels.count() as u16);
            }

            if let Some(existing) = sample_rate {
                if existing != spec.rate {
                    bail!("audio stream changed sample rate: {} -> {}", existing, spec.rate);
                }
            } else {
                sample_rate = Some(spec.rate);
            }

            let mut sample_buf = SampleBuffer::<i16>::new(decoded.capacity() as u64, spec);
            sample_buf.copy_interleaved_ref(decoded);
            samples.extend_from_slice(sample_buf.samples());
        }

        let channels = channels.ok_or_else(|| anyhow!("audio stream produced no channel layout"))?;
        let sample_rate = sample_rate.ok_or_else(|| anyhow!("audio stream produced no sample rate"))?;
        if samples.is_empty() {
            bail!("audio stream produced no samples");
        }

        Ok(Pcm16Sound {
            channels,
            sample_rate,
            samples,
        })
    }

    fn decode_ogg_vorbis_pcm16(bytes: &[u8]) -> Result<Pcm16Sound> {
        let cursor = Cursor::new(bytes);
        let mut reader = OggStreamReader::new(cursor)?;
        let channels = reader.ident_hdr.audio_channels as u16;
        let sample_rate = reader.ident_hdr.audio_sample_rate;
        if channels == 0 || sample_rate == 0 {
            bail!("invalid OGG/Vorbis stream: channels={} sample_rate={}", channels, sample_rate);
        }

        let mut samples = Vec::new();
        while let Some(packet) = reader.read_dec_packet_itl()? {
            samples.extend(packet);
        }

        Ok(Pcm16Sound {
            channels,
            sample_rate,
            samples,
        })
    }

    fn decode_wav_pcm16le(bytes: &[u8]) -> Result<Pcm16Sound> {
        if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            bail!("not a RIFF/WAVE file");
        }

        let mut pos = 12usize;
        let mut channels = 0u16;
        let mut sample_rate = 0u32;
        let mut bits_per_sample = 0u16;
        let mut audio_format = 0u16;
        let mut data_range = None;

        while pos.checked_add(8).map(|v| v <= bytes.len()).unwrap_or(false) {
            let id = &bytes[pos..pos + 4];
            let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
            pos += 8;
            let end = pos.checked_add(size).ok_or_else(|| anyhow!("WAV chunk size overflow"))?;
            if end > bytes.len() {
                bail!("WAV chunk exceeds input length");
            }

            match id {
                b"fmt " => {
                    if size < 16 {
                        bail!("WAV fmt chunk too small");
                    }
                    audio_format = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
                    channels = u16::from_le_bytes(bytes[pos + 2..pos + 4].try_into().unwrap());
                    sample_rate = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap());
                    bits_per_sample = u16::from_le_bytes(bytes[pos + 14..pos + 16].try_into().unwrap());
                }
                b"data" => {
                    data_range = Some(pos..end);
                }
                _ => {}
            }

            pos = end + (size & 1);
        }

        if audio_format != 1 || bits_per_sample != 16 {
            bail!("unsupported WAV format: format={} bits={}", audio_format, bits_per_sample);
        }
        if channels == 0 || sample_rate == 0 {
            bail!("invalid WAV stream: channels={} sample_rate={}", channels, sample_rate);
        }
        let range = data_range.ok_or_else(|| anyhow!("missing WAV data chunk"))?;
        let data = &bytes[range];
        if data.len() % 2 != 0 {
            bail!("WAV PCM16 data length is odd");
        }

        let mut samples = Vec::with_capacity(data.len() / 2);
        for chunk in data.chunks_exact(2) {
            samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
        }

        Ok(Pcm16Sound {
            channels,
            sample_rate,
            samples,
        })
    }
}
pub use backend::{AudioManager, AudioTrackHandle, SoundData, SoundHandle};
#[cfg(rfvp_switch)]
pub use backend::AudioCommand;
