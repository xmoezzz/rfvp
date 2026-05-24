//! Software PCM mixer.
//!
//! Fixed output format: 48 000 Hz, 16-bit signed, stereo interleaved.
//! Input: any sample rate (linear-interpolation resampling), 16-bit signed stereo.

use std::sync::Arc;

pub const OUTPUT_RATE: u32 = 48_000;

/// Maximum simultaneous channels (4 BGM + up to 28 SE).
pub const MAX_CHANNELS: usize = 32;

/// Per-sample buffer size written to the DMA ring at one time (must be even).
pub const MIX_BUFFER_FRAMES: usize = 1024;

#[derive(Clone)]
struct Channel {
    /// 16-bit signed stereo interleaved PCM at `sample_rate`.
    samples: Arc<Vec<i16>>,
    /// Read position in stereo frames (sub-frame accuracy as fixed-point ×2^16).
    pos_fp: u64,
    /// Advance per output frame in fixed-point (pos_fp units = sample_rate/OUTPUT_RATE × 2^16).
    step_fp: u64,
    volume: f32,
    pan: f32,
    looping: bool,
    loop_start_frame: u64,
    active: bool,
}

impl Channel {
    fn frames(&self) -> u64 {
        self.samples.len() as u64 / 2
    }

    fn sample_at(&self, frame: u64) -> (i16, i16) {
        let frame = (frame % self.frames().max(1)) as usize;
        let idx = frame * 2;
        if idx + 1 >= self.samples.len() {
            return (0, 0);
        }
        (self.samples[idx], self.samples[idx + 1])
    }

    /// Linear interpolation between frame `f0` and `f1`, weight `frac` ∈ [0, 65535].
    fn lerp_sample(&self, f0: u64, f1: u64, frac: u32) -> (i16, i16) {
        let (l0, r0) = self.sample_at(f0);
        let (l1, r1) = self.sample_at(f1);
        let lerp = |a: i16, b: i16| -> i16 {
            let a = a as i32;
            let b = b as i32;
            ((a * (65536 - frac as i32) + b * frac as i32) >> 16) as i16
        };
        (lerp(l0, l1), lerp(r0, r1))
    }

    fn next_sample(&mut self) -> Option<(i16, i16)> {
        if !self.active {
            return None;
        }
        let total_fp = self.frames() << 16;
        if total_fp == 0 {
            self.active = false;
            return None;
        }

        let frame_int = self.pos_fp >> 16;
        let frac = (self.pos_fp & 0xFFFF) as u32;
        let frame_next = frame_int + 1;

        let (l, r) = if frame_next < self.frames() {
            self.lerp_sample(frame_int, frame_next, frac)
        } else if self.looping {
            self.lerp_sample(frame_int, self.loop_start_frame, frac)
        } else {
            self.sample_at(frame_int)
        };

        self.pos_fp += self.step_fp;

        if self.pos_fp >= total_fp {
            if self.looping {
                let loop_start_fp = self.loop_start_frame << 16;
                self.pos_fp = loop_start_fp + (self.pos_fp - total_fp);
            } else {
                self.active = false;
            }
        }

        Some((l, r))
    }
}

pub struct SoftMixer {
    channels: [Option<Channel>; MAX_CHANNELS],
    master_volume: f32,
}

impl SoftMixer {
    pub fn new() -> Self {
        Self {
            channels: [(); MAX_CHANNELS].map(|_| None),
            master_volume: 1.0,
        }
    }

    /// Allocate the next free channel slot. Returns `None` if all channels are in use.
    fn alloc_channel(&mut self) -> Option<usize> {
        self.channels.iter().position(|c| {
            c.as_ref().map(|ch| !ch.active).unwrap_or(true)
        })
    }

    /// Start playing `samples` (Arc<Vec<i16>>, stereo 16-bit) on a new channel.
    pub fn play(
        &mut self,
        samples: Arc<Vec<i16>>,
        sample_rate: u32,
        volume: f32,
        pan: f32,
        looping: bool,
        loop_start_frame: u64,
    ) -> Option<usize> {
        let id = self.alloc_channel()?;
        let step_fp = ((sample_rate as u64) << 16) / OUTPUT_RATE as u64;
        self.channels[id] = Some(Channel {
            samples,
            pos_fp: 0,
            step_fp,
            volume,
            pan,
            looping,
            loop_start_frame,
            active: true,
        });
        Some(id)
    }

    pub fn stop_channel(&mut self, id: usize) {
        if let Some(ch) = self.channels.get_mut(id) {
            if let Some(c) = ch.as_mut() {
                c.active = false;
            }
        }
    }

    pub fn set_channel_volume(&mut self, id: usize, vol: f32) {
        if let Some(Some(ch)) = self.channels.get_mut(id) {
            ch.volume = vol;
        }
    }

    pub fn set_channel_pan(&mut self, id: usize, pan: f32) {
        if let Some(Some(ch)) = self.channels.get_mut(id) {
            ch.pan = pan;
        }
    }

    pub fn is_channel_active(&self, id: usize) -> bool {
        self.channels
            .get(id)
            .and_then(|c| c.as_ref())
            .map(|c| c.active)
            .unwrap_or(false)
    }

    pub fn channel_volume(&self, id: usize) -> f32 {
        self.channels.get(id).and_then(|c| c.as_ref()).map(|c| c.volume).unwrap_or(0.0)
    }

    pub fn channel_pan(&self, id: usize) -> f32 {
        self.channels.get(id).and_then(|c| c.as_ref()).map(|c| c.pan).unwrap_or(0.5)
    }

    pub fn set_master_volume(&mut self, vol: f32) {
        self.master_volume = vol.clamp(0.0, 1.0);
    }

    /// Mix `n_frames` stereo output frames into `out` (16-bit signed interleaved).
    /// `out` must have length ≥ n_frames × 2.
    pub fn mix_into(&mut self, out: &mut [i16], n_frames: usize) {
        let master = self.master_volume;
        let n = n_frames.min(out.len() / 2);
        let buf = &mut out[..n * 2];
        buf.fill(0);

        for ch_slot in self.channels.iter_mut() {
            let Some(ch) = ch_slot.as_mut() else { continue };
            if !ch.active {
                continue;
            }
            let vol = (ch.volume * master).clamp(0.0, 1.0);
            // Pan: 0.0=left, 0.5=center, 1.0=right (matches kira Panning semantics with shift)
            let pan = ch.pan.clamp(0.0, 1.0);
            let gain_l = vol * (1.0 - pan).sqrt();
            let gain_r = vol * pan.sqrt();

            for frame in 0..n {
                let Some((sl, sr)) = ch.next_sample() else { break };
                let l = (sl as f32 * gain_l) as i32;
                let r = (sr as f32 * gain_r) as i32;
                let oi = frame * 2;
                buf[oi] = (buf[oi] as i32 + l).clamp(-32768, 32767) as i16;
                buf[oi + 1] = (buf[oi + 1] as i32 + r).clamp(-32768, 32767) as i16;
            }
        }
    }

}
