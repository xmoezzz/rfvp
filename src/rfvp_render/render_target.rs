use glam::{mat4, vec4, Mat4, Vec4};
use wgpu::util::DeviceExt;

use super::{GpuCommonResources, TextureBindGroup};
use super::vertices::{PosColTexVertex, VertexSource};

pub struct RenderTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    bind_group: TextureBindGroup,
    size: (u32, u32),
    quad_vb: wgpu::Buffer,
    quad_vertices: std::ops::Range<u32>,
}

impl RenderTarget {
    pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

    pub fn new(resources: &GpuCommonResources, size: (u32, u32), label: Option<&str>) -> Self {
        let (w, h) = (size.0.max(1), size.1.max(1));
        let texture = resources.device.create_texture(&wgpu::TextureDescriptor {
            label,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = resources.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rfvp_render.rendertarget_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = resources.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rfvp_render.rendertarget_bind_group"),
            layout: &resources.bind_group_layouts.texture,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        // Fullscreen quad in virtual pixel space centered at (0,0).
        let vw = w as f32;
        let vh = h as f32;
        let x0 = -vw / 2.0;
        let y0 = -vh / 2.0;
        let x1 = vw / 2.0;
        let y1 = vh / 2.0;

        let white = Vec4::ONE;
        let v = [
            PosColTexVertex { position: glam::vec3(x0, y1, 0.0), color: white, texture_coordinate: glam::vec2(0.0, 1.0) },
            PosColTexVertex { position: glam::vec3(x0, y0, 0.0), color: white, texture_coordinate: glam::vec2(0.0, 0.0) },
            PosColTexVertex { position: glam::vec3(x1, y1, 0.0), color: white, texture_coordinate: glam::vec2(1.0, 1.0) },
            PosColTexVertex { position: glam::vec3(x1, y1, 0.0), color: white, texture_coordinate: glam::vec2(1.0, 1.0) },
            PosColTexVertex { position: glam::vec3(x0, y0, 0.0), color: white, texture_coordinate: glam::vec2(0.0, 0.0) },
            PosColTexVertex { position: glam::vec3(x1, y0, 0.0), color: white, texture_coordinate: glam::vec2(1.0, 0.0) },
        ];

        let quad_vb = resources.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rfvp_render.rendertarget_quad_vb"),
            contents: bytemuck::cast_slice(&v),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            texture,
            view,
            sampler,
            bind_group: TextureBindGroup::new(bind_group),
            size: (w, h),
            quad_vb,
            quad_vertices: 0..6,
        }
    }

    pub fn projection_matrix(&self) -> Mat4 {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        mat4(
            vec4(2.0 / w, 0.0, 0.0, 0.0),
            vec4(0.0, -2.0 / h, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(0.0, 0.0, 0.0, 1.0),
        )
    }

    pub fn bind_group(&self) -> &TextureBindGroup {
        &self.bind_group
    }

    pub fn vertex_source<'a>(&'a self) -> VertexSource<'a> {
        VertexSource::VertexBuffer {
            vertex_buffer: &self.quad_vb,
            vertices: self.quad_vertices.clone(),
            instances: 0..1,
        }
    }

    pub fn begin_srgb_render_pass<'a>(
        &'a self,
        encoder: &'a mut wgpu::CommandEncoder,
        label: Option<&'a str>,
    ) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        })
    }
}
