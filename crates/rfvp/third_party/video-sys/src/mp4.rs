use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail, Context, Result};
use mp4::{FourCC, Mp4Reader, TrackType};

use crate::h264::H264Config;

#[derive(Debug, Clone)]
pub struct EncodedSample {
    pub data_avcc: Vec<u8>,
    pub pts_us: i64,
    pub dur_us: i64,
}

#[derive(Debug)]
struct Prefetched {
    start_time: u64,
    duration: u32,
    rendering_offset: i32,
    bytes: Vec<u8>,
}

pub struct Mp4H264Source {
    path: PathBuf,
    reader: Mp4Reader<BufReader<File>>,
    track_id: u32,
    timescale: u32,
    sample_count: u32,
    next_sample_id: u32,
    prefetched: Option<Prefetched>,

    pub config: H264Config,
}

impl Mp4H264Source {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        let f = File::open(&path).with_context(|| format!("open mp4: {}", path.display()))?;
        let size = f
            .metadata()
            .with_context(|| format!("stat mp4: {}", path.display()))?
            .len();

        let reader = BufReader::new(f);
        let mut mp4 = Mp4Reader::read_header(reader, size).context("mp4::read_header")?;

        let (track_id, timescale, sample_count, width, height, sps, pps) =
            select_h264_video_track(&mp4).context("select H.264 track")?;

        // Prefetch the first sample to:
        // 1) validate we can read samples;
        // 2) infer NAL length field size (usually 4, but not guaranteed).
        let prefetched = mp4
            .read_sample(track_id, 1)
            .context("read first sample")?
            .map(|s| Prefetched {
                start_time: s.start_time,
                duration: s.duration,
                rendering_offset: s.rendering_offset,
                bytes: s.bytes.to_vec(),
            });

        let nal_len_size = prefetched
            .as_ref()
            .map(|p| detect_nal_length_size(&p.bytes))
            .unwrap_or(4);

        let avcc = build_avcc_record(&sps, &pps, nal_len_size)?;
        let config = H264Config::parse_from_avcc(width, height, &avcc)
            .context("parse avcC from SPS/PPS")?;

        Ok(Self {
            path,
            reader: mp4,
            track_id,
            timescale,
            sample_count,
            next_sample_id: 1,
            prefetched,
            config,
        })
    }

    pub fn next_sample(&mut self) -> Result<Option<EncodedSample>> {
        if self.next_sample_id == 0 {
            bail!("internal error: sample ids are 1-based");
        }

        if self.next_sample_id > self.sample_count {
            return Ok(None);
        }

        let (start_time, duration, rendering_offset, bytes) = if self.next_sample_id == 1 {
            if let Some(p) = self.prefetched.take() {
                (p.start_time, p.duration, p.rendering_offset, p.bytes)
            } else {
                let s = self
                    .reader
                    .read_sample(self.track_id, 1)
                    .context("read sample #1")?
                    .ok_or_else(|| anyhow!("sample #1 missing"))?;
                (s.start_time, s.duration, s.rendering_offset, s.bytes.to_vec())
            }
        } else {
            let s = self
                .reader
                .read_sample(self.track_id, self.next_sample_id)
                .with_context(|| format!("read sample #{}", self.next_sample_id))?
                .ok_or_else(|| anyhow!("sample #{} missing", self.next_sample_id))?;
            (s.start_time, s.duration, s.rendering_offset, s.bytes.to_vec())
        };

        self.next_sample_id += 1;

        let pts_ticks = (start_time as i128) + (rendering_offset as i128);
        let pts_us = ticks_to_us(pts_ticks, self.timescale);
        let dur_us = ticks_to_us(duration as i128, self.timescale);

        Ok(Some(EncodedSample {
            data_avcc: bytes,
            pts_us,
            dur_us,
        }))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn ticks_to_us(ticks: i128, timescale: u32) -> i64 {
    if timescale == 0 {
        return 0;
    }
    // microseconds = ticks * 1_000_000 / timescale
    let us = ticks.saturating_mul(1_000_000i128) / (timescale as i128);
    if us > (i64::MAX as i128) {
        i64::MAX
    } else if us < (i64::MIN as i128) {
        i64::MIN
    } else {
        us as i64
    }
}

fn select_h264_video_track(
    mp4: &Mp4Reader<BufReader<File>>,
) -> Result<(u32, u32, u32, u32, u32, Vec<u8>, Vec<u8>)> {
    let avc1 = FourCC::from_str("avc1").unwrap();
    let avc3 = FourCC::from_str("avc3").unwrap();

    for (track_id, track) in mp4.tracks().iter() {
        let tt = track.track_type().context("track_type")?;
        if tt != TrackType::Video {
            continue;
        }

        let bt = track.box_type().context("box_type")?;
        if bt != avc1 && bt != avc3 {
            continue;
        }

        let timescale = track.timescale();
        let sample_count = track.sample_count();
        let width = track.width() as u32;
        let height = track.height() as u32;

        let sps = track
            .sequence_parameter_set()
            .context("sequence_parameter_set")?
            .to_vec();
        let pps = track
            .picture_parameter_set()
            .context("picture_parameter_set")?
            .to_vec();

        if sps.is_empty() || pps.is_empty() {
            bail!("H.264 track is missing SPS/PPS (avcC)");
        }

        return Ok((*track_id, timescale, sample_count, width, height, sps, pps));
    }

    bail!("no H.264 (avc1/avc3) video track found");
}

/// Heuristically infer NAL length prefix size (1..=4 bytes) for AVCC samples.
fn detect_nal_length_size(sample: &[u8]) -> usize {
    for &n in &[4usize, 3, 2, 1] {
        if looks_like_length_prefixed_nals(sample, n) {
            return n;
        }
    }
    4
}

fn looks_like_length_prefixed_nals(sample: &[u8], n: usize) -> bool {
    if n == 0 || n > 4 {
        return false;
    }
    let mut off = 0usize;
    let mut nal_count = 0usize;

    while off + n <= sample.len() && nal_count < 8 {
        let len = read_be_len(&sample[off..off + n]);
        if len == 0 {
            return false;
        }
        let next = off + n + len;
        if next > sample.len() {
            return false;
        }
        off = next;
        nal_count += 1;

        if off == sample.len() {
            return nal_count >= 1;
        }
    }

    // Accept if we could parse at least one NAL and did not violate bounds.
    nal_count >= 1
}

fn read_be_len(b: &[u8]) -> usize {
    let mut v = 0usize;
    for &x in b {
        v = (v << 8) | (x as usize);
    }
    v
}

/// Build an AVCDecoderConfigurationRecord (avcC) from SPS/PPS.
/// This is sufficient for system decoders that require codec private data.
fn build_avcc_record(sps: &[u8], pps: &[u8], nal_len_size: usize) -> Result<Vec<u8>> {
    if nal_len_size < 1 || nal_len_size > 4 {
        bail!("invalid nal_len_size={}", nal_len_size);
    }
    if sps.len() < 4 {
        bail!("SPS too short (len={})", sps.len());
    }
    if pps.is_empty() {
        bail!("PPS empty");
    }

    let profile_idc = sps[1];
    let constraint = sps[2];
    let level_idc = sps[3];

    let mut out = Vec::with_capacity(64 + sps.len() + pps.len());

    // AVCDecoderConfigurationRecord
    out.push(1); // configurationVersion
    out.push(profile_idc);
    out.push(constraint);
    out.push(level_idc);

    // 6 bits reserved (111111) + 2 bits lengthSizeMinusOne
    let length_size_minus_one = (nal_len_size - 1) as u8;
    out.push(0b1111_1100 | (length_size_minus_one & 0b11));

    // 3 bits reserved (111) + 5 bits numOfSequenceParameterSets
    out.push(0b1110_0000 | 1);

    // SPS
    out.extend_from_slice(&(sps.len() as u16).to_be_bytes());
    out.extend_from_slice(sps);

    // numOfPictureParameterSets
    out.push(1);

    // PPS
    out.extend_from_slice(&(pps.len() as u16).to_be_bytes());
    out.extend_from_slice(pps);

    Ok(out)
}
