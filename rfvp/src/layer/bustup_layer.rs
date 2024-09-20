use std::{fmt::Debug, sync::Arc};

use glam::Mat4;
use rfvp_render::{GpuCommonResources, GpuImage, Renderable};

use crate::{
    asset::bustup::Bustup,
    layer::{Layer, LayerProperties},
    update::{Updatable, UpdateContext},
};

pub struct BustupLayer {
    bustup: Arc<Bustup>,
    bustup_name: Option<String>,
    emotion: String,

    properties: LayerProperties,
}

impl BustupLayer {
    pub fn new(
        resources: &GpuCommonResources,
        bustup: Arc<Bustup>,
        bustup_name: Option<String>,
        emotion: &str,
    ) -> Self {
        // ensure the picture is loaded to gpu
        bustup.base_gpu_image(resources);

        Self {
            bustup,
            bustup_name,
            emotion: emotion.to_owned(),
            properties: LayerProperties::new(),
        }
    }
}

impl Renderable for BustupLayer {
    fn render<'enc>(
        &'enc self,
        resources: &'enc GpuCommonResources,
        render_pass: &mut wgpu::RenderPass<'enc>,
        transform: Mat4,
        projection: Mat4,
    ) {
        let transform = self.properties.compute_transform(transform);
        let total_transform = projection * transform;

        let mut draw_image = |image: &'enc GpuImage| {
            // TODO: there should be a generic function to render a layer (from texture?)
            resources.draw_sprite(
                render_pass,
                image.vertex_source(),
                image.bind_group(),
                total_transform,
            );
        };

        let base_gpu_image = self.bustup.base_gpu_image(resources);
        draw_image(base_gpu_image);

        if let Some(emotion_gpu_image) = self.bustup.face_gpu_image(resources, &self.emotion) {
            draw_image(emotion_gpu_image);
        }

        if let Some(mouth_gpu_image) = self.bustup.mouth_gpu_image(resources, &self.emotion, 0.0) {
            draw_image(mouth_gpu_image);
        }
    }

    fn resize(&mut self, _resources: &GpuCommonResources) {
        // no internal buffers to resize
    }
}

impl Updatable for BustupLayer {
    fn update(&mut self, ctx: &UpdateContext) {
        self.properties.update(ctx);
    }
}

impl Debug for BustupLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("BustupLayer")
            .field(
                &self
                    .bustup_name
                    .as_ref()
                    .map_or("<unnamed>", |v| v.as_str()),
            )
            .field(&self.emotion)
            .finish()
    }
}

impl Layer for BustupLayer {
    fn properties(&self) -> &LayerProperties {
        &self.properties
    }

    fn properties_mut(&mut self) -> &mut LayerProperties {
        &mut self.properties
    }
}
