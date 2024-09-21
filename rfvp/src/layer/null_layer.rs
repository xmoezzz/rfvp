use std::fmt::Debug;

use glam::Mat4;
use rfvp_render::{GpuCommonResources, Renderable};

use crate::{
    layer::{Layer, LayerProperties},
    update::{Updatable, UpdateContext},
};

pub struct NullLayer {
    props: LayerProperties,
}

impl NullLayer {
    pub fn new() -> Self {
        Self {
            props: LayerProperties::new(),
        }
    }
}

impl Renderable for NullLayer {
    fn render<'enc>(
        &'enc self,
        _resources: &'enc GpuCommonResources,
        _render_pass: &mut wgpu::RenderPass<'enc>,
        _transform: Mat4,
        _projection: Mat4,
    ) {
    }

    fn resize(&mut self, _resources: &GpuCommonResources) {
        // no internal buffers to resize
    }
}

impl Updatable for NullLayer {
    fn update(&mut self, _ctx: &UpdateContext) {}
}

impl Debug for NullLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("NullLayer").finish()
    }
}

impl Layer for NullLayer {
    fn properties(&self) -> &LayerProperties {
        &self.props
    }

    fn properties_mut(&mut self) -> &mut LayerProperties {
        &mut self.props
    }
}
