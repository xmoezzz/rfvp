//! Interleaved XA files: many streams in one file, one per channel at a time.
//!
//! The CD drive plays one channel of an XA file: sector `n` belongs to channel `n % channels`.
//! With music at 18.9 kHz stereo the drive needs one sector in 16 at double speed, so 16 streams
//! fit side by side; each channel holds several tracks one after another. Sectors are written
//! raw (2336 bytes: subheader, data, EDC left to mkpsxiso), as mkpsxiso takes `mixed` files.

use crate::adpcm::XA_DATA;
use crate::audio::XaRef;

pub const RAW_SECTOR: usize = 2336;

/// Submode: audio, form 2, real time.
const SUBMODE_AUDIO: u8 = 0x64;
const SUBMODE_EOF: u8 = 0x80;

/// The coding info byte: stereo, 18.9 kHz (else 37.8), 4 bits per sample.
pub fn coding(stereo: bool, rate: u32) -> u8 {
    (stereo as u8) | if rate == 18900 { 0x04 } else { 0 }
}

/// Channels an XA file can interleave at double speed for a stream of this coding.
pub fn channels_for(stereo: bool, rate: u32) -> usize {
    // Sectors a second at 2x (150) over sectors the stream needs a second.
    let frames = crate::adpcm::xa_frames_per_sector(stereo) as f64;
    (150.0 / (rate as f64 / frames)).floor() as usize
}

/// Lay out `tracks` (each its sectors of sound data, frames and loop sector) in one file;
/// returns the file and where each track went, in the order given.
pub fn build(file_number: u16, tracks: &[(Vec<[u8; XA_DATA]>, u32, u32)], stereo: bool, rate: u32) -> (Vec<u8>, Vec<XaRef>) {
    let channels = channels_for(stereo, rate).clamp(1, 32);
    let coding = coding(stereo, rate);
    // Longest tracks first, each onto the emptiest channel.
    let mut order: Vec<usize> = (0..tracks.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(tracks[i].0.len()));
    let mut fill = vec![0usize; channels];
    let mut placed = vec![(0usize, 0usize); tracks.len()];
    for i in order {
        let c = (0..channels).min_by_key(|&c| fill[c]).unwrap();
        placed[i] = (c, fill[c]);
        fill[c] += tracks[i].0.len();
    }
    let rows = fill.iter().copied().max().unwrap_or(0);
    let total = rows * channels;
    let mut out = vec![0u8; total * RAW_SECTOR];
    for n in 0..total {
        let sector = &mut out[n * RAW_SECTOR..(n + 1) * RAW_SECTOR];
        let submode = if n + 1 == total { SUBMODE_AUDIO | SUBMODE_EOF } else { SUBMODE_AUDIO };
        let sub = [file_number as u8, (n % channels) as u8, submode, coding];
        sector[0..4].copy_from_slice(&sub);
        sector[4..8].copy_from_slice(&sub);
        // Data stays zero (silence) where no track is.
    }
    let mut refs = Vec::with_capacity(tracks.len());
    for (i, (sectors, frames, loop_sector)) in tracks.iter().enumerate() {
        let (channel, start) = placed[i];
        for (k, data) in sectors.iter().enumerate() {
            let n = (start + k) * channels + channel;
            out[n * RAW_SECTOR + 8..n * RAW_SECTOR + 8 + XA_DATA].copy_from_slice(data);
        }
        refs.push(XaRef {
            file: file_number,
            channel: channel as u8,
            coding,
            first_sector: (start * channels + channel) as u32,
            sectors: sectors.len() as u32,
            frames: *frames,
            loop_sector: *loop_sector,
        });
    }
    (out, refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_for_music_and_voice() {
        assert_eq!(channels_for(true, 18900), 16);
        assert_eq!(channels_for(true, 37800), 8);
        assert_eq!(channels_for(false, 18900), 32);
    }

    #[test]
    fn tracks_share_channels() {
        let a = (vec![[1u8; XA_DATA]; 3], 100, 0);
        let b = (vec![[2u8; XA_DATA]; 2], 50, 0);
        let (file, refs) = build(1, &[a, b], true, 18900);
        assert_eq!(file.len() % RAW_SECTOR, 0);
        assert_eq!(refs[0].channel, 0);
        assert_eq!(refs[1].channel, 1);
        assert_eq!(refs[1].first_sector, 1);
        // Track b's second sector: row 1, channel 1.
        let n = 16 + 1;
        assert_eq!(file[n * RAW_SECTOR + 8], 2);
        assert_eq!(file[n * RAW_SECTOR + 1], 1);
    }
}
