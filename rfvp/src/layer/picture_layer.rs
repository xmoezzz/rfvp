use std::{fmt::Debug, sync::Arc};

use glam::Mat4;
use rfvp_render::{GpuCommonResources, Renderable};

use crate::{
    asset::picture::Picture,
    layer::{Layer, LayerProperties},
    update::{Updatable, UpdateContext},
};

pub struct PictureLayer {
    picture: Arc<Picture>,
    picture_name: Option<String>,

    props: LayerProperties,
}

impl PictureLayer {
    pub fn new(
        resources: &GpuCommonResources,
        picture: Arc<Picture>,
        picture_name: Option<String>,
    ) -> Self {
        // ensure the picture is loaded to gpu
        picture.gpu_image(resources);

        Self {
            picture,
            picture_name,
            props: LayerProperties::new(),
        }
    }
}

impl Renderable for PictureLayer {
    fn render<'enc>(
        &'enc self,
        resources: &'enc GpuCommonResources,
        render_pass: &mut wgpu::RenderPass<'enc>,
        transform: Mat4,
        projection: Mat4,
    ) {
        let total_transform = projection * self.props.compute_transform(transform);
        // TODO: there should be a generic function to render a layer (from texture?)
        let gpu_image = self.picture.gpu_image(resources);
        resources.draw_sprite(
            render_pass,
            gpu_image.vertex_source(),
            gpu_image.bind_group(),
            total_transform,
        );
    }

    fn resize(&mut self, _resources: &GpuCommonResources) {
        // no internal buffers to resize
    }
}

impl Updatable for PictureLayer {
    fn update(&mut self, ctx: &UpdateContext) {
        self.props.update(ctx);
    }
}

impl Debug for PictureLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PictureLayer")
            .field(
                &self
                    .picture_name
                    .as_ref()
                    .map_or("<unnamed>", |v| v.as_str()),
            )
            .finish()
    }
}

impl Layer for PictureLayer {
    fn properties(&self) -> &LayerProperties {
        &self.props
    }

    fn properties_mut(&mut self) -> &mut LayerProperties {
        &mut self.props
    }
}
