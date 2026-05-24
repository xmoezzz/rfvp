//! Null audio driver: discards all samples silently.

use super::AudioDriver;

pub struct NullDriver;

impl AudioDriver for NullDriver {
    fn is_available(&self) -> bool { false }
    fn frames_available(&self) -> usize { 0 }
    fn write_frames(&mut self, _buf: &[i16], _n_frames: usize) {}
}
