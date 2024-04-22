use wgpu::{CommandEncoder, Device, Queue, SurfaceConfiguration, TextureFormat, TextureView};

use crate::config::app_config::AppConfig;
use crate::subsystem::world::GameData;

pub(crate) mod renderer_state;
pub(crate) mod shinku2d;

/// Trait to implement in order to create a renderer to use in the application
pub trait Renderer {
    fn start(
        &mut self,
        device: &Device,
        queue: &Queue,
        surface_config: &SurfaceConfiguration,
        texture_format: &TextureFormat,
        window_size: (u32, u32),
    );

    /// Will be called first, before render, each time the window request redraw.
    fn update(
        &mut self,
        data: &mut GameData,
        device: &Device,
        surface_config: &SurfaceConfiguration,
        queue: &mut Queue,
    );

    /// Will be called after render, each time the window request redraw.
    fn render(
        &mut self,
        data: &mut GameData,
        config: &AppConfig,
        texture_view: &TextureView,
        encoder: &mut CommandEncoder,
    );
}
