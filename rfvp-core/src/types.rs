use bitflags::bitflags;
use proc_bitfield::bitfield;

bitflags! {
    /// Represents a status of a playing audio that can be awaited on
    ///
    /// Used in [BGMWAIT](super::super::runtime::BGMWAIT), [SEWAIT](super::super::runtime::SEWAIT) and [VOICEWAIT](super::super::runtime::VOICEWAIT) commands
    #[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
    pub struct AudioWaitStatus: i32 {
        const PLAYING = 1;
        const STOPPED = 2;
        const VOLUME_TWEENER_IDLE = 4;
        const PANNING_TWEENER_IDLE = 8;
        const PLAY_SPEED_TWEENER_IDLE = 16;
    }
}


/// A volume value, in the range [0.0, 1.0].
#[derive(Debug, Copy, Clone)]
pub struct Volume(pub f32);

impl Default for Volume {
    fn default() -> Self {
        Self(1.0)
    }
}

impl PartialEq for Volume {
    fn eq(&self, other: &Self) -> bool {
        self.0.total_cmp(&other.0) == std::cmp::Ordering::Equal
    }
}

impl Eq for Volume {}


/// Defines a pan value in the range [-1.0, 1.0], where `0.0` is the center and `-1.0` is the hard left and `1.0` is the hard right.
#[derive(Debug, Copy, Clone)]
pub struct Pan(pub f32);

impl Default for Pan {
    fn default() -> Self {
        Self(0.0)
    }
}

impl PartialEq for Pan {
    fn eq(&self, other: &Self) -> bool {
        self.0.total_cmp(&other.0) == std::cmp::Ordering::Equal
    }
}

impl Eq for Pan {}

