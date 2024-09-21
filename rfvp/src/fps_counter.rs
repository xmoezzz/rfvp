use std::{collections::VecDeque, time::Duration};

use crate::{
    render::overlay::{OverlayCollector, OverlayVisitable},
    update::{Updatable, UpdateContext},
};

const WINDOW_SIZE: usize = 60;

pub struct FpsCounter {
    values: VecDeque<Duration>,
}

impl FpsCounter {
    pub fn new() -> Self {
        Self {
            values: VecDeque::new(),
        }
    }
}

impl Updatable for FpsCounter {
    fn update(&mut self, context: &UpdateContext) {
        self.values.push_back(context.time_delta());
        if self.values.len() > WINDOW_SIZE {
            self.values.pop_front();
        }
    }
}

impl OverlayVisitable for FpsCounter {
    fn visit_overlay(&self, collector: &mut OverlayCollector) {
        collector.overlay(
            "FPS",
            |_ctx, top_left| {
                let sum: Duration = self.values.iter().cloned().sum();
                let avg = sum
                    .checked_div(self.values.len() as u32)
                    .unwrap_or(Duration::ZERO);
                let fps = 1.0 / avg.as_secs_f32();

                top_left.label(format!("FPS: {:.2}", fps));
            },
            true,
        )
    }
}
