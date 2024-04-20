mod fill;
mod sprite;
mod text;
mod text_outline;
mod yuv_sprite;

use fill::FillPipeline;
use sprite::SpritePipeline;
use text::TextPipeline;
use text_outline::TextOutlinePipeline;
use yuv_sprite::YuvSpritePipeline;

use crate::{bind_groups::BindGroupLayouts, RAW_TEXTURE_FORMAT, SRGB_TEXTURE_FORMAT};

// TODO: make a builder?
fn make_pipeline(
    device: &wgpu::Device,
    texture_format: wgpu::TextureFormat,
    shader_module: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    vertex_buffer_layout: wgpu::VertexBufferLayout,
    blend: Option<wgpu::BlendState>,
    label: &str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader_module,
            entry_point: "vertex_main",
            buffers: &[vertex_buffer_layout],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Cw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: Default::default(),
            conservative: false,
        },
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader_module,
            entry_point: "fragment_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: texture_format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
    })
}

pub struct Pipelines {
    pub sprite: SpritePipeline,
    pub yuv_sprite: YuvSpritePipeline,
    pub fill: FillPipeline,
    pub text: TextPipeline,
    pub text_outline: TextOutlinePipeline,
    // those are pipelines using screen's texture format (not our preferred RGBA format)
    // they are only used for the final render pass
    pub sprite_screen: SpritePipeline,
    pub fill_screen: FillPipeline,
}

impl Pipelines {
    pub fn new(
        device: &wgpu::Device,
        bind_group_layouts: &BindGroupLayouts,
        surface_texture_format: wgpu::TextureFormat,
    ) -> Pipelines {
        Pipelines {
            sprite: SpritePipeline::new(device, bind_group_layouts, SRGB_TEXTURE_FORMAT),
            yuv_sprite: YuvSpritePipeline::new(device, bind_group_layouts, RAW_TEXTURE_FORMAT),
            fill: FillPipeline::new(device, bind_group_layouts, SRGB_TEXTURE_FORMAT),
            text: TextPipeline::new(device, bind_group_layouts, SRGB_TEXTURE_FORMAT),
            text_outline: TextOutlinePipeline::new(device, bind_group_layouts, SRGB_TEXTURE_FORMAT),

            sprite_screen: SpritePipeline::new(device, bind_group_layouts, surface_texture_format),
            fill_screen: FillPipeline::new(device, bind_group_layouts, surface_texture_format),
        }
    }
}
