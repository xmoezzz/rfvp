use glam::Mat4;
use rfvp_render::{GpuCommonResources, RenderTarget, Renderable};

use crate::{
    layer::{screen_layer::ScreenLayer, Layer, LayerProperties, MessageLayer},
    update::{Updatable, UpdateContext},
};

pub struct RootLayerGroup {
    screen_layer: ScreenLayer,
    message_layer: MessageLayer,
    render_target: RenderTarget,
    properties: LayerProperties,
}

impl RootLayerGroup {
    pub fn new(
        resources: &GpuCommonResources,
        screen_layer: ScreenLayer,
        message_layer: MessageLayer,
    ) -> Self {
        let render_target = RenderTarget::new(
            resources,
            resources.current_render_buffer_size(),
            Some("LayerGroup RenderTarget"),
        );

        Self {
            screen_layer,
            message_layer,
            render_target,
            properties: LayerProperties::new(),
        }
    }

    pub fn screen_layer(&self) -> &ScreenLayer {
        &self.screen_layer
    }

    pub fn screen_layer_mut(&mut self) -> &mut ScreenLayer {
        &mut self.screen_layer
    }

    pub fn message_layer(&self) -> &MessageLayer {
        &self.message_layer
    }

    pub fn message_layer_mut(&mut self) -> &mut MessageLayer {
        &mut self.message_layer
    }
}

impl Updatable for RootLayerGroup {
    fn update(&mut self, context: &UpdateContext) {
        self.properties.update(context);
        self.screen_layer.update(context);
        self.message_layer.update(context);
    }
}

impl Renderable for RootLayerGroup {
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
                .begin_srgb_render_pass(&mut encoder, Some("RootLayerGroup RenderPass"));

            let transform = self.properties.compute_transform(transform);
            let projection = self.render_target.projection_matrix();

            render_pass.push_debug_group("ScreenLayer");
            self.screen_layer
                .render(resources, &mut render_pass, transform, projection);
            render_pass.pop_debug_group();

            render_pass.push_debug_group("MessageLayer");
            self.message_layer
                .render(resources, &mut render_pass, transform, projection);
            render_pass.pop_debug_group();
        }

        render_pass.push_debug_group("RootLayerGroup Render");
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
        self.render_target
            .resize(resources, resources.current_render_buffer_size());
    }
}

impl Layer for RootLayerGroup {
    fn properties(&self) -> &LayerProperties {
        &self.properties
    }

    fn properties_mut(&mut self) -> &mut LayerProperties {
        &mut self.properties
    }
}
