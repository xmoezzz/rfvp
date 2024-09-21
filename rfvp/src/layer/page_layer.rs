use glam::Mat4;
use rfvp_core::vm::command::types::PLANES_COUNT;
use rfvp_render::{GpuCommonResources, RenderTarget, Renderable};

use crate::{
    layer::{Layer, LayerGroup, LayerProperties},
    update::{Updatable, UpdateContext},
};

pub struct PageLayer {
    planes: [LayerGroup; PLANES_COUNT],
    properties: LayerProperties,
    render_target: RenderTarget,
}

impl PageLayer {
    pub fn new(resources: &GpuCommonResources) -> Self {
        let render_target = RenderTarget::new(
            resources,
            resources.current_render_buffer_size(),
            Some("LayerGroup RenderTarget"),
        );

        Self {
            planes: [
                LayerGroup::new(resources),
                LayerGroup::new(resources),
                LayerGroup::new(resources),
                LayerGroup::new(resources),
            ],
            render_target,
            properties: LayerProperties::new(),
        }
    }

    pub fn plane(&self, index: u32) -> &LayerGroup {
        &self.planes[index as usize]
    }

    pub fn plane_mut(&mut self, index: u32) -> &mut LayerGroup {
        &mut self.planes[index as usize]
    }
}

impl Updatable for PageLayer {
    fn update(&mut self, context: &UpdateContext) {
        for plane in self.planes.iter_mut() {
            plane.update(context);
        }
    }
}

impl Renderable for PageLayer {
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

            for (i, plane) in self.planes.iter().enumerate() {
                render_pass.push_debug_group(&format!("Plane {}", i));

                plane.render(resources, &mut render_pass, transform, projection);

                render_pass.pop_debug_group();
            }
        }

        render_pass.push_debug_group("PageLayer Render");
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

impl Layer for PageLayer {
    fn properties(&self) -> &LayerProperties {
        &self.properties
    }

    fn properties_mut(&mut self) -> &mut LayerProperties {
        &mut self.properties
    }
}
