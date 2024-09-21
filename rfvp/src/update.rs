use std::{sync::Arc, time::Duration};

use enum_dispatch::enum_dispatch;
use rfvp_core::time::Ticks;
use rfvp_render::GpuCommonResources;

use crate::{asset::AnyAssetServer, input::RawInputState, layer::UserLayer, time::Time};

pub struct UpdateContext<'a> {
    pub time: &'a Time,
    pub gpu_resources: &'a Arc<GpuCommonResources>,
    pub asset_server: &'a Arc<AnyAssetServer>,
    pub raw_input_state: &'a RawInputState,
}

impl<'a> UpdateContext<'a> {
    pub fn time_delta(&self) -> Duration {
        self.time.delta()
    }
    pub fn time_delta_ticks(&self) -> Ticks {
        Ticks::from_seconds(self.time.delta_seconds())
    }
}

#[enum_dispatch]
pub trait Updatable {
    fn update(&mut self, context: &UpdateContext);
}
