//! Audio decoding: WAV (RIFF/PCM) and OGG Vorbis → signed 16-bit stereo PCM.

use anyhow::{bail, Result};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::sync::Arc;

/// Decoded audio ready for playback.
pub struct DecodedPcm {
    /// Stereo interleaved signed 16-bit PCM at `sample_rate`.
    pub samples: Arc<Vec<i16>>,
    pub sample_rate: u32,
}

/// Decode an audio byte slice (WAV or OGG) to signed 16-bit stereo interleaved PCM.
pub fn decode_bytes(data: &[u8]) -> Result<DecodedPcm> {
    if data.len() >= 4 && &data[..4] == b"RIFF" {
        let (samples, sample_rate) = decode_wav(data)?;
        Ok(DecodedPcm { samples: Arc::new(samples), sample_rate })
    } else if data.len() >= 4 && &data[..4] == b"OggS" {
        let (samples, sample_rate) = decode_ogg(data)?;
        Ok(DecodedPcm { samples: Arc::new(samples), sample_rate })
    } else {
        bail!("anzu-hal: unrecognised audio format (not WAV/OGG)")
    }
}

// ─── WAV ─────────────────────────────────────────────────────────────────────

fn decode_wav(data: &[u8]) -> Result<(Vec<i16>, u32)> {
    let mut cur = Cursor::new(data);

    let mut tag = [0u8; 4];
    cur.read_exact(&mut tag)?;
    if &tag != b"RIFF" { bail!("WAV: missing RIFF"); }
    let mut _sz = [0u8; 4];
    cur.read_exact(&mut _sz)?;
    cur.read_exact(&mut tag)?;
    if &tag != b"WAVE" { bail!("WAV: missing WAVE"); }

    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut bits_per_sample = 0u16;
    let mut audio_format = 0u16;
    let mut samples: Vec<i16> = Vec::new();

    loop {
        let mut id = [0u8; 4];
        if cur.read_exact(&mut id).is_err() { break; }
        let mut sz_b = [0u8; 4];
        cur.read_exact(&mut sz_b)?;
        let sz = u32::from_le_bytes(sz_b) as usize;

        if &id == b"fmt " {
            let mut fmt = vec![0u8; sz.min(18)];
            cur.read_exact(&mut fmt)?;
            if fmt.len() < 16 { bail!("WAV: fmt chunk too short"); }
            audio_format    = u16::from_le_bytes([fmt[0],  fmt[1]]);
            channels        = u16::from_le_bytes([fmt[2],  fmt[3]]);
            sample_rate     = u32::from_le_bytes([fmt[4],  fmt[5],  fmt[6],  fmt[7]]);
            bits_per_sample = u16::from_le_bytes([fmt[14], fmt[15]]);
            if sz > fmt.len() {
                cur.seek(SeekFrom::Current((sz - fmt.len()) as i64))?;
            }
        } else if &id == b"data" {
            if audio_format != 1 {
                bail!("WAV: only PCM (format 1) supported, got {}", audio_format);
            }
            if channels == 0 || (bits_per_sample != 8 && bits_per_sample != 16) {
                bail!("WAV: unsupported ch={} bps={}", channels, bits_per_sample);
            }
            let mut raw = vec![0u8; sz];
            cur.read_exact(&mut raw)?;
            samples = match bits_per_sample {
                8  => raw.iter().map(|&b| ((b as i16) - 128) * 256).collect(),
                16 => raw.chunks_exact(2)
                         .map(|c| i16::from_le_bytes([c[0], c[1]]))
                         .collect(),
                _ => unreachable!(),
            };
            break; // data chunk is last
        } else {
            // Skip unknown chunk (word-aligned).
            cur.seek(SeekFrom::Current(sz as i64 + (sz & 1) as i64)).ok();
        }
    }

    if sample_rate == 0 { bail!("WAV: missing fmt chunk"); }
    if samples.is_empty() { bail!("WAV: empty data chunk"); }

    Ok((mono_to_stereo(samples, channels), sample_rate))
}

// ─── OGG Vorbis ──────────────────────────────────────────────────────────────

fn decode_ogg(data: &[u8]) -> Result<(Vec<i16>, u32)> {
    use lewton::inside_ogg::OggStreamReader;
    let mut reader = OggStreamReader::new(Cursor::new(data))
        .map_err(|e| anyhow::anyhow!("OGG header: {:?}", e))?;
    let rate     = reader.ident_hdr.audio_sample_rate;
    let channels = reader.ident_hdr.audio_channels;
    let mut out: Vec<i16> = Vec::new();
    loop {
        match reader.read_dec_packet_itl() {
            Ok(Some(pkt)) => out.extend_from_slice(&pkt),
            Ok(None) => break,
            Err(e) => return Err(anyhow::anyhow!("OGG decode: {:?}", e)),
        }
    }
    Ok((mono_to_stereo(out, channels as u16), rate))
}

// ─── Channel normalisation ────────────────────────────────────────────────────

fn mono_to_stereo(samples: Vec<i16>, channels: u16) -> Vec<i16> {
    match channels {
        1 => {
            let mut s = Vec::with_capacity(samples.len() * 2);
            for &v in &samples { s.push(v); s.push(v); }
            s
        }
        2 => samples,
        n => {
            // Keep first two channels.
            let mut s = Vec::with_capacity(samples.len() / n as usize * 2);
            for chunk in samples.chunks_exact(n as usize) {
                s.push(chunk[0]);
                s.push(chunk[1]);
            }
            s
        }
    }
}
