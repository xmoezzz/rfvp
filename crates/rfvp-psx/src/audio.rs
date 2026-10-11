//! Sound: the game's Ogg Vorbis files decoded, resampled and coded for the PlayStation.
//!
//! The engine hands a sound's file to the host as it is (`RfvpAudio::load_encoded`), so each
//! pack entry is replaced by what the host plays:
//!
//! ```text
//! SPU sound (sound effects, voices), played from sound RAM:
//!   "PSXS", u32 sample rate, u32 samples, u32 data length, SPU-ADPCM data
//! XA stream (music), played by the CD drive from an interleaved XA file:
//!   "PSXA", u16 file, u8 channel, u8 coding info, u32 first sector, u32 sectors, u32 frames,
//!   u32 loop sector (where a repeat starts again, in the stream's sectors)
//! ```
//!
//! An XA stream's first sector is counted in the file's own sectors (the channel's sectors are
//! every `channels`-th one from `channel`).

use std::io::Cursor;

use anyhow::{Context, Result};
use vorbis_rs::VorbisDecoder;

pub fn is_ogg(bytes: &[u8]) -> bool {
    bytes.starts_with(b"OggS")
}

pub fn is_wav(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
}

/// A RIFF WAVE file of 8- or 16-bit PCM.
pub fn decode_wav(bytes: &[u8]) -> Result<Pcm> {
    let mut at = 12;
    let (mut channels, mut rate, mut bits) = (0usize, 0u32, 0u16);
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into()?) as usize;
        let body = &bytes[at + 8..(at + 8 + len).min(bytes.len())];
        if id == b"fmt " {
            anyhow::ensure!(body.len() >= 16, "short fmt chunk");
            anyhow::ensure!(u16::from_le_bytes([body[0], body[1]]) == 1, "WAVE is not PCM");
            channels = u16::from_le_bytes([body[2], body[3]]) as usize;
            rate = u32::from_le_bytes(body[4..8].try_into()?);
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"data" {
            anyhow::ensure!(channels > 0 && (bits == 8 || bits == 16), "unsupported WAVE format");
            let mut out = vec![Vec::new(); channels];
            let step = bits as usize / 8;
            for (i, s) in body.chunks_exact(step).enumerate() {
                let v = if bits == 16 {
                    i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0
                } else {
                    (s[0] as f32 - 128.0) / 128.0
                };
                out[i % channels].push(v);
            }
            let len = out.iter().map(Vec::len).min().unwrap_or(0);
            out.iter_mut().for_each(|c| c.truncate(len));
            return Ok(Pcm { rate, channels: out });
        }
        at += 8 + len + (len & 1);
    }
    anyhow::bail!("WAVE without data")
}

/// Where a looped recording repeats: (first sample of the loop, its length). Game music is
/// often stored with its loop played over and over for many minutes; only the intro and one
/// pass are worth keeping. Found by correlating the 10 ms energy envelope against itself, then
/// confirmed and refined on the waveform.
pub fn find_loop(x: &[f32], rate: u32) -> Option<(usize, usize)> {
    let hop = (rate / 100) as usize;
    let env: Vec<f32> = x.chunks(hop).map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32 + 1e-9).ln()).collect();
    let n = env.len();
    let start = n / 4; // a quarter in: past most intros
    let min_lag = 2000; // 20 s
    if n < start + min_lag * 2 {
        return None;
    }
    let corr = |a: &[f32], b: &[f32]| -> f32 {
        let (ma, mb) = (a.iter().sum::<f32>() / a.len() as f32, b.iter().sum::<f32>() / b.len() as f32);
        let (mut ab, mut aa, mut bb) = (0.0f32, 0.0f32, 0.0f32);
        for (x, y) in a.iter().zip(b) {
            let (dx, dy) = (x - ma, y - mb);
            ab += dx * dy;
            aa += dx * dx;
            bb += dy * dy;
        }
        ab / (aa * bb).sqrt().max(1e-9)
    };
    let window = 3000.min(n / 4);
    let mut best = (0.0f32, 0usize);
    for lag in min_lag..(n - start - window).min(n / 2 + 1) {
        let c = corr(&env[start..start + window], &env[start + lag..start + lag + window]);
        if c > best.0 + 0.002 {
            best = (c, lag);
        }
    }
    if std::env::var_os("RFVP_LOOP_DEBUG").is_some() {
        eprintln!("envelope: corr {} at lag {}", best.0, best.1);
    }
    // Only a candidate: the waveform below decides.
    if best.0 < 0.8 {
        return None;
    }
    let wave = |a: usize, b: usize, len: usize| -> f32 {
        if b + len > x.len() {
            return 0.0;
        }
        corr(&x[a..a + len], &x[b..b + len])
    };
    // The waveform: refine the period within two hops, on half a second at a few places (an
    // intro may still be playing at the first).
    let len = (rate / 2) as usize;
    let coarse = best.1 * hop;
    let span = x.len().saturating_sub(coarse + 2 * hop + len);
    let (mut score, mut period, mut probe) = (0.0f32, coarse, 0usize);
    for at in [span / 4, span / 2, span * 3 / 4] {
        for p in coarse.saturating_sub(2 * hop)..coarse + 2 * hop {
            let c = wave(at, at + p, len);
            if c > score {
                (score, period, probe) = (c, p, at);
            }
        }
    }
    if std::env::var_os("RFVP_LOOP_DEBUG").is_some() {
        eprintln!("waveform: corr {score} at period {period}, probe {probe}");
    }
    if score < 0.98 {
        return None;
    }
    // The loop starts where the recording first matches itself a period later.
    let step = (rate / 10) as usize;
    let mut intro = 0;
    while intro < probe && wave(intro, intro + period, len) < 0.98 {
        intro += step;
    }
    Some((intro, period))
}

/// Decoded sound: planar channels at `rate`.
pub struct Pcm {
    pub rate: u32,
    pub channels: Vec<Vec<f32>>,
}

pub fn decode_ogg(bytes: &[u8]) -> Result<Pcm> {
    let mut decoder = VorbisDecoder::new(Cursor::new(bytes)).context("open Ogg Vorbis")?;
    let rate = decoder.sampling_frequency().get();
    let mut channels = vec![Vec::new(); decoder.channels().get() as usize];
    while let Some(block) = decoder.decode_audio_block().context("decode Ogg Vorbis")? {
        for (dst, src) in channels.iter_mut().zip(block.samples()) {
            dst.extend_from_slice(src);
        }
    }
    Ok(Pcm { rate, channels })
}

impl Pcm {
    pub fn seconds(&self) -> f64 {
        self.channels.first().map_or(0.0, |c| c.len() as f64 / self.rate as f64)
    }

    /// One channel, the average of all.
    pub fn mono(&self) -> Vec<f32> {
        let n = self.channels.len().max(1) as f32;
        let len = self.channels.first().map_or(0, Vec::len);
        (0..len).map(|i| self.channels.iter().map(|c| c[i]).sum::<f32>() / n).collect()
    }

    /// Two channels (mono duplicated, extra channels dropped).
    pub fn stereo(&self) -> [Vec<f32>; 2] {
        match self.channels.len() {
            0 => [Vec::new(), Vec::new()],
            1 => [self.channels[0].clone(), self.channels[0].clone()],
            _ => [self.channels[0].clone(), self.channels[1].clone()],
        }
    }
}

/// Resample with a windowed sinc (Blackman, 32 taps a side), low-passed at the lower of the two
/// Nyquist rates: sounds go down to 18.9 or 22 kHz.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let table = filter_table(from, to);
    let ratio = to as f64 / from as f64;
    let out_len = (input.len() as f64 * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let center = i as f64 / ratio;
        let base = center.floor() as i64;
        let phase = (((center - base as f64) * PHASES as f64) as usize).min(PHASES - 1);
        let taps = &table[phase * TAPS * 2..(phase + 1) * TAPS * 2];
        let first = base - TAPS as i64 + 1;
        let mut acc = 0.0f32;
        if first >= 0 && (base + TAPS as i64) < input.len() as i64 {
            let window = &input[first as usize..first as usize + TAPS * 2];
            for (s, w) in window.iter().zip(taps) {
                acc += s * w;
            }
        } else {
            for (j, w) in taps.iter().enumerate() {
                let k = first + j as i64;
                if k >= 0 && (k as usize) < input.len() {
                    acc += input[k as usize] * w;
                }
            }
        }
        out.push(acc);
    }
    out
}

const TAPS: usize = 32;
const PHASES: usize = 1024;

/// Normalized filter taps for each of `PHASES` fractional positions (`TAPS` a side).
fn filter_table(from: u32, to: u32) -> std::sync::Arc<Vec<f32>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static TABLES: OnceLock<Mutex<HashMap<(u32, u32), Arc<Vec<f32>>>>> = OnceLock::new();
    let tables = TABLES.get_or_init(Default::default);
    if let Some(t) = tables.lock().unwrap().get(&(from, to)) {
        return t.clone();
    }
    let cutoff = (to as f64 / from as f64).min(1.0) * 0.95;
    let mut table = Vec::with_capacity(PHASES * TAPS * 2);
    for p in 0..PHASES {
        let frac = p as f64 / PHASES as f64;
        let mut taps = Vec::with_capacity(TAPS * 2);
        for j in 0..TAPS * 2 {
            // Distance from the sample at base - TAPS + 1 + j to the output position.
            let x = frac + TAPS as f64 - 1.0 - j as f64;
            let sinc = if x.abs() < 1e-9 {
                1.0
            } else {
                let a = std::f64::consts::PI * x * cutoff;
                a.sin() / a
            };
            let t = ((x / TAPS as f64) + 1.0) / 2.0;
            let window = if (0.0..=1.0).contains(&t) {
                0.42 - 0.5 * (2.0 * std::f64::consts::PI * t).cos()
                    + 0.08 * (4.0 * std::f64::consts::PI * t).cos()
            } else {
                0.0
            };
            taps.push(sinc * window);
        }
        let sum: f64 = taps.iter().sum();
        table.extend(taps.iter().map(|w| (w / sum) as f32));
    }
    let table = Arc::new(table);
    tables.lock().unwrap().insert((from, to), table.clone());
    table
}

pub fn to_i16(samples: &[f32]) -> Vec<i16> {
    samples.iter().map(|&s| (s * 32767.0).round().clamp(-32768.0, 32767.0) as i16).collect()
}

/// A sound for sound RAM, mono at `rate`.
pub fn spu_sound(pcm: &Pcm, rate: u32) -> Vec<u8> {
    let samples = to_i16(&resample(&pcm.mono(), pcm.rate, rate));
    let data = crate::adpcm::encode_spu(&samples);
    let mut out = Vec::with_capacity(16 + data.len());
    out.extend_from_slice(b"PSXS");
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// Where an XA stream sits; see the module notes.
#[derive(Clone, Copy, Debug)]
pub struct XaRef {
    pub file: u16,
    pub channel: u8,
    pub coding: u8,
    pub first_sector: u32,
    pub sectors: u32,
    pub frames: u32,
    /// Where a repeat starts again, in the stream's own sectors (0: from the start).
    pub loop_sector: u32,
}

pub fn xa_stub(r: &XaRef) -> Vec<u8> {
    let mut out = Vec::with_capacity(20);
    out.extend_from_slice(b"PSXA");
    out.extend_from_slice(&r.file.to_le_bytes());
    out.push(r.channel);
    out.push(r.coding);
    out.extend_from_slice(&r.first_sector.to_le_bytes());
    out.extend_from_slice(&r.sectors.to_le_bytes());
    out.extend_from_slice(&r.frames.to_le_bytes());
    out.extend_from_slice(&r.loop_sector.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_loop_after_an_intro() {
        let rate = 8000u32;
        // A deterministic "melody": notes changing every 0.4 s over a 25 s pattern.
        let note = |t: usize| -> f32 {
            let k = (t / 3200) as u32;
            let f = 200.0 + ((k * 2654435761) % 600) as f32;
            let amp = 0.2 + ((k * 40503) % 7) as f32 / 10.0;
            (t as f32 * f * 2.0 * std::f32::consts::PI / rate as f32).sin() * amp
        };
        let intro = 7 * rate as usize;
        let period = 25 * rate as usize;
        let x: Vec<f32> = (0..intro + period * 4)
            .map(|t| if t < intro { note(t + 999_999) * 0.5 } else { note((t - intro) % period) })
            .collect();
        let (start, p) = find_loop(&x, rate).expect("loop");
        assert!((p as i64 - period as i64).abs() <= 2, "period {p}");
        assert!(start >= intro.saturating_sub(rate as usize) && start <= intro + rate as usize, "start {start}");
    }

    #[test]
    fn resample_keeps_a_low_tone() {
        let input: Vec<f32> = (0..44100).map(|i| (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / 44100.0).sin() * 0.5).collect();
        let out = resample(&input, 44100, 18900);
        assert!((out.len() as i64 - 18900).abs() <= 1);
        let peak = out[1000..17000].iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        assert!((peak - 0.5).abs() < 0.03, "peak {peak}");
    }
}
