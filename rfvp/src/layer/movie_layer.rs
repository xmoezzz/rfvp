use std::{fmt::Debug, sync::Arc};

use glam::Mat4;
use rfvp_audio::AudioManager;
use rfvp_render::{GpuCommonResources, RenderTarget, Renderable};
use rfvp_video::VideoPlayer;

use crate::{
    asset::movie::Movie,
    layer::{Layer, LayerProperties},
    update::{Updatable, UpdateContext},
};

pub struct MovieLayer {
    props: LayerProperties,
    video_player: VideoPlayer,
    render_target: RenderTarget,
    movie_name: Option<String>,
}

impl MovieLayer {
    pub fn new(
        resources: &GpuCommonResources,
        audio_manager: &AudioManager,
        movie: Arc<Movie>,
        movie_name: Option<String>,
    ) -> Self {
        Self {
            props: LayerProperties::new(),
            video_player: movie
                .play(resources, audio_manager)
                .expect("Failed to play movie"),
            render_target: RenderTarget::new(
                resources,
                resources.current_render_buffer_size(),
                Some("MovieLayer RenderTarget"),
            ),
            movie_name,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.video_player.is_finished()
    }
}

impl Renderable for MovieLayer {
    fn render<'enc>(
        &'enc self,
        resources: &'enc GpuCommonResources,
        render_pass: &mut wgpu::RenderPass<'enc>,
        transform: Mat4,
        projection: Mat4,
    ) {
        // draw to a render target first because currently all our layer passes are in Srgb
        // TODO: I believe this will be changed, so we can remove this extra render pass
        {
            let mut encoder = resources.start_encoder();
            let mut render_pass = self
                .render_target
                .begin_raw_render_pass(&mut encoder, Some("MovieLayer RenderPass"));

            self.video_player.render(
                resources,
                &mut render_pass,
                transform,
                self.render_target.projection_matrix(),
            );
        }

        resources.draw_sprite(
            render_pass,
            self.render_target.vertex_source(),
            self.render_target.bind_group(),
            projection,
        );
    }

    fn resize(&mut self, resources: &GpuCommonResources) {
        self.render_target
            .resize(resources, resources.current_render_buffer_size());
    }
}

impl Updatable for MovieLayer {
    fn update(&mut self, ctx: &UpdateContext) {
        self.video_player
            .update(ctx.time_delta_ticks(), &ctx.gpu_resources.queue);
    }
}

impl Debug for MovieLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MovieLayer")
            .field(&self.movie_name.as_ref().map_or("<unnamed>", |v| v.as_str()))
            .finish()
    }
}

impl Layer for MovieLayer {
    fn properties(&self) -> &LayerProperties {
        &self.props
    }

    fn properties_mut(&mut self) -> &mut LayerProperties {
        &mut self.props
    }
}
