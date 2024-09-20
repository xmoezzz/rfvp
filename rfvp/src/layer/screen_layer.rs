use glam::Mat4;
use rfvp_render::{GpuCommonResources, RenderTarget, Renderable};

use crate::{
    layer::{page_layer::PageLayer, Layer, LayerProperties},
    update::{Updatable, UpdateContext},
};

pub struct ScreenLayer {
    page_layer: PageLayer,
    properties: LayerProperties,
    render_target: RenderTarget,
    // TODO: a TransitionLayer (two kinds??) should be here
}

impl ScreenLayer {
    pub fn new(resources: &GpuCommonResources) -> Self {
        Self {
            page_layer: PageLayer::new(resources),
            properties: LayerProperties::new(),
            render_target: RenderTarget::new(
                resources,
                resources.current_render_buffer_size(),
                Some("ScreenLayer RenderTarget"),
            ),
        }
    }

    pub fn page_layer(&self) -> &PageLayer {
        &self.page_layer
    }

    pub fn page_layer_mut(&mut self) -> &mut PageLayer {
        &mut self.page_layer
    }
}

impl Updatable for ScreenLayer {
    fn update(&mut self, context: &UpdateContext) {
        self.page_layer.update(context);
        self.properties.update(context);
    }
}

impl Renderable for ScreenLayer {
    fn render<'enc>(
        &'enc self,
        resources: &'enc GpuCommonResources,
        render_pass: &mut wgpu::RenderPass<'enc>,
        transform: Mat4,
        projection: Mat4,
    ) {
        {
            let mut encoder = resources.start_encoder();
            let mut render_pass = self
                .render_target
                .begin_srgb_render_pass(&mut encoder, Some("PageLayer RenderPass"));

            let transform = self.properties.compute_transform(transform);
            let projection = self.render_target.projection_matrix();

            self.page_layer
                .render(resources, &mut render_pass, transform, projection);
        }

        render_pass.push_debug_group("ScreenLayer Render");
        // TODO use layer pseudo-pipeline
        resources.draw_sprite(
            render_pass,
            self.render_target.vertex_source(),
            self.render_target.bind_group(),
            projection,
        );
        render_pass.pop_debug_group();
    }

    fn resize(&mut self, resources: &GpuCommonResources) {
        self.page_layer.resize(resources);
    }
}

impl Layer for ScreenLayer {
    fn properties(&self) -> &LayerProperties {
        &self.properties
    }

    fn properties_mut(&mut self) -> &mut LayerProperties {
        &mut self.properties
    }
}
