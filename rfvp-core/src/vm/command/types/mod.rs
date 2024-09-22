use num_derive::FromPrimitive;
use bitflags::bitflags;

mod id;

pub use id::{
    LayerId, LayerIdOpt, VLayerId, VLayerIdRepr, LAYERBANKS_COUNT, LAYERS_COUNT, PLANES_COUNT,
};

#[derive(FromPrimitive, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub enum MessageTextLayout {
    Left = 0,
    /// I _think_ this is the same as Left
    Layout1 = 1,
    Center = 2,
    Right = 3,
}

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
