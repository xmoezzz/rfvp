//! Movies: STR files (MDEC video, XA sound) at 15 frames a second, for a double-speed drive.
//!
//! At double speed the drive reads 150 sectors a second. Sound (37.8 kHz stereo XA) takes one
//! sector in eight; the other seven carry video, so each frame has 8 or 9 sectors (35 for
//! every four frames), and is coded to fit them. A video sector is a 32-byte STR header (0x0160,
//! 0x8001, chunk, chunks, frame, frame length, width, height, the frame's own header) and
//! 2016 bytes of the frame. The player shows frames as they arrive, so the stream keeps time by
//! itself.
//!
//! The source is decoded by ffmpeg (which must be on PATH): frames scaled to the console's
//! picture and cropped to whole macroblocks, sound resampled for XA.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};
use rayon::prelude::*;

use crate::adpcm;
use crate::mdec;
use crate::xa::RAW_SECTOR;

pub const FPS: u32 = 15;
const AUDIO_RATE: u32 = 37800;
/// Sectors in a group: seven of video, then one of sound.
const GROUP: usize = 8;
const VIDEO_PER_GROUP: usize = 7;
const CHUNK: usize = 2016;

/// Convert `source` to an STR of `width` x `height` (multiples of 16).
pub fn convert(source: &Path, width: usize, height: usize, viewport_h: usize) -> Result<Vec<u8>> {
    ensure!(width % 16 == 0 && height % 16 == 0, "STR size must be whole macroblocks");
    let audio = decode_audio(source)?;
    // Frames scaled to the viewport and cropped (or padded) to whole macroblocks.
    let filter = format!(
        "fps={FPS},scale={width}:{viewport_h}:flags=area,crop={width}:{}:0:{},pad={width}:{height}:0:(oh-ih)/2",
        height.min(viewport_h),
        viewport_h.saturating_sub(height) / 2
    );
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(source)
        .args(["-vf", &filter, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .stdout(Stdio::piped())
        .spawn()
        .context("run ffmpeg (is it installed?)")?;
    let mut stdout = child.stdout.take().unwrap();
    let frame_len = width * height * 3;

    // Sound sectors, in the order they go out.
    let mut audio_sectors = adpcm::encode_xa(&audio, true).into_iter();
    let coding = crate::xa::coding(true, AUDIO_RATE);
    let mut out = Vec::new();
    let mut frame_index = 0u32;
    let mut sector_index = 0usize;
    loop {
        // A batch of frames coded in parallel.
        let mut batch = Vec::new();
        for _ in 0..64 {
            let mut rgb = vec![0u8; frame_len];
            match stdout.read_exact(&mut rgb) {
                Ok(()) => batch.push(rgb),
                Err(_) => break,
            }
        }
        if batch.is_empty() {
            break;
        }
        let first = frame_index;
        let coded: Vec<(Vec<u8>, usize)> = batch
            .par_iter()
            .enumerate()
            .map(|(i, rgb)| {
                let f = first as usize + i;
                let sectors = (f + 1) * 35 / 4 - f * 35 / 4;
                let planes = mdec::Planes::from_rgb(rgb, width, height);
                (mdec::encode_within(&planes, sectors * CHUNK), sectors)
            })
            .collect();
        for (bitstream, sectors) in coded {
            for chunk in 0..sectors {
                // A sound sector every eighth sector.
                while sector_index % GROUP == VIDEO_PER_GROUP {
                    let data = audio_sectors.next().unwrap_or([0u8; adpcm::XA_DATA]);
                    out.extend_from_slice(&audio_sector(&data, coding));
                    sector_index += 1;
                }
                out.extend_from_slice(&video_sector(&bitstream, frame_index, chunk, sectors, width, height));
                sector_index += 1;
            }
            frame_index += 1;
        }
    }
    let status = child.wait()?;
    ensure!(status.success(), "ffmpeg failed on {}", source.display());
    ensure!(frame_index > 0, "no frames in {}", source.display());
    Ok(out)
}

fn decode_audio(source: &Path) -> Result<Vec<i16>> {
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(source)
        .args(["-vn", "-ac", "2", "-ar", &AUDIO_RATE.to_string(), "-f", "s16le", "-"])
        .output()
        .context("run ffmpeg (is it installed?)")?;
    // A movie without sound gets silence.
    Ok(output.stdout.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect())
}

fn video_sector(bitstream: &[u8], frame: u32, chunk: usize, chunks: usize, width: usize, height: usize) -> Vec<u8> {
    let mut s = vec![0u8; RAW_SECTOR];
    // Subheader: file 1, channel 1, data + real time, form 1.
    let sub = [1u8, 1, 0x48, 0];
    s[0..4].copy_from_slice(&sub);
    s[4..8].copy_from_slice(&sub);
    let h = &mut s[8..8 + 32];
    h[0..2].copy_from_slice(&0x0160u16.to_le_bytes());
    h[2..4].copy_from_slice(&0x8001u16.to_le_bytes());
    h[4..6].copy_from_slice(&(chunk as u16).to_le_bytes());
    h[6..8].copy_from_slice(&(chunks as u16).to_le_bytes());
    h[8..12].copy_from_slice(&(frame + 1).to_le_bytes());
    h[12..16].copy_from_slice(&(bitstream.len() as u32).to_le_bytes());
    h[16..18].copy_from_slice(&(width as u16).to_le_bytes());
    h[18..20].copy_from_slice(&(height as u16).to_le_bytes());
    h[20..28].copy_from_slice(&bitstream[0..8]);
    let start = chunk * CHUNK;
    if start < bitstream.len() {
        let end = (start + CHUNK).min(bitstream.len());
        s[8 + 32..8 + 32 + end - start].copy_from_slice(&bitstream[start..end]);
    }
    s
}

fn audio_sector(data: &[u8; adpcm::XA_DATA], coding: u8) -> Vec<u8> {
    let mut s = vec![0u8; RAW_SECTOR];
    let sub = [1u8, 1, 0x64, coding];
    s[0..4].copy_from_slice(&sub);
    s[4..8].copy_from_slice(&sub);
    s[8..8 + adpcm::XA_DATA].copy_from_slice(data);
    s
}
