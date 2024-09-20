use bevy_utils::hashbrown::HashMap;
use glam::Mat4;
use itertools::Itertools;
use rfvp_core::vm::command::types::LayerId;
use rfvp_render::{GpuCommonResources, RenderTarget, Renderable};

use crate::{
    adv::LayerSelection,
    layer::{Layer, LayerProperties, UserLayer},
    update::{Updatable, UpdateContext},
};

pub struct LayerGroup {
    layers: HashMap<LayerId, UserLayer>,
    render_target: RenderTarget,
    properties: LayerProperties,
}

impl LayerGroup {
    pub fn new(resources: &GpuCommonResources) -> Self {
        let render_target = RenderTarget::new(
            resources,
            resources.current_render_buffer_size(),
            Some("LayerGroup RenderTarget"),
        );

        Self {
            layers: HashMap::new(),
            render_target,
            properties: LayerProperties::new(),
        }
    }

    pub fn get_layer_ids(&self) -> impl Iterator<Item = LayerId> + '_ {
        self.layers.keys().cloned()
    }

    pub fn add_layer(&mut self, id: LayerId, layer: UserLayer) {
        self.layers.insert(id, layer);
    }

    pub fn remove_layer(&mut self, id: LayerId) {
        if self.layers.remove(&id).is_none() {
            // this warning is too noisy
            // needs to be more specific to be useful
            // warn!("LayerGroup::remove_layer: layer not found");
        }
    }

    pub fn get_layer(&self, id: LayerId) -> Option<&UserLayer> {
        self.layers.get(&id)
    }

    pub fn get_layers(&self, selection: LayerSelection) -> impl Iterator<Item = &UserLayer> {
        self.layers
            .iter()
            .filter(move |&(&id, _)| selection.contains(id))
            .map(|(_, v)| v)
    }

    pub fn get_layer_mut(&mut self, id: LayerId) -> Option<&mut UserLayer> {
        self.layers.get_mut(&id)
    }

    pub fn get_layers_mut(
        &mut self,
        selection: LayerSelection,
    ) -> impl Iterator<Item = &mut UserLayer> {
        self.layers
            .iter_mut()
            .filter(move |&(&id, _)| selection.contains(id))
            .map(|(_, v)| v)
    }
}

impl Updatable for LayerGroup {
    fn update(&mut self, context: &UpdateContext) {
        self.properties.update(context);
        for layer in self.layers.values_mut() {
            layer.update(context);
        }
    }
}

impl Renderable for LayerGroup {
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
                .begin_srgb_render_pass(&mut encoder, Some("LayerGroup RenderPass"));

            let ordered_layers = self
                .layers
                .iter()
                .sorted_by_key(|&(id, _)| {
                    // TODO: use render order property
                    *id
                })
                .collect::<Vec<_>>();

            let transform = self.properties.compute_transform(transform);
            let projection = self.render_target.projection_matrix();

            for (id, l) in ordered_layers {
                render_pass.push_debug_group(&format!("Layer {:?}", id));
                l.render(resources, &mut render_pass, transform, projection);
                render_pass.pop_debug_group();
            }
        }

        render_pass.push_debug_group("LayerGroup Render");
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

impl Layer for LayerGroup {
    fn properties(&self) -> &LayerProperties {
        &self.properties
    }

    fn properties_mut(&mut self) -> &mut LayerProperties {
        &mut self.properties
    }
}
