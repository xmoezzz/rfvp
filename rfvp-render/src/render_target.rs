use std::borrow::Cow;

use glam::Mat4;

use super::TextureBindGroup;
use crate::{
    common_resources::GpuCommonResources,
    vertices::{PosColTexVertex, VertexSource},
    SpriteVertexBuffer, 
};

/// Describes a fullscreen intermediate render target.
pub struct RenderTarget {
    texture: wgpu::Texture,
    srgb_view: wgpu::TextureView,
    raw_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    bind_group: TextureBindGroup,
    vertices: SpriteVertexBuffer,
    label: Cow<'static, str>,
    size: (u32, u32),
}

impl RenderTarget {
    const SRGB_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
    const RAW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    pub fn new(resources: &GpuCommonResources, size: (u32, u32), label: Option<&str>) -> Self {
        let label = label
            .map(|s| Cow::from(s.to_owned()))
            .unwrap_or_else(|| Cow::from("Unnamed RenderTarget"));

        let texture = resources.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("{} Texture", label)),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::SRGB_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[Self::RAW_FORMAT],
        });
        let srgb_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(&format!("{} Srgb TextureView", label)),
            format: Some(Self::SRGB_FORMAT),
            ..Default::default()
        });
        let raw_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(&format!("{} Raw TextureView", label)),
            format: Some(Self::RAW_FORMAT),
            ..Default::default()
        });
        let sampler = resources.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&format!("{} Sampler", label)),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bind_group = TextureBindGroup::new(
            resources,
            &srgb_view,
            &sampler,
            Some(&format!("{} TextureBindGroup", label)),
        );
        let vertices = SpriteVertexBuffer::new_fullscreen(resources, size.0, size.1);
        Self {
            texture,
            srgb_view,
            raw_view,
            sampler,
            bind_group,
            vertices,
            label,
            size,
        }
    }

    pub fn resize(&mut self, resources: &GpuCommonResources, size: (u32, u32)) {
        self.texture = resources.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("{} Texture", self.label)),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::SRGB_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.srgb_view = self.texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(&format!("{} TextureView", self.label)),
            ..Default::default()
        });
        self.bind_group = TextureBindGroup::new(
            resources,
            &self.srgb_view,
            &self.sampler,
            Some(&format!("{} TextureBindGroup", self.label)),
        );
    }

    pub fn projection_matrix(&self) -> Mat4 {
        let mut projection = Mat4::IDENTITY;
        projection.x_axis.x = 2.0 / self.size.0 as f32;
        projection.y_axis.y = -2.0 / self.size.1 as f32; // in wgpu y is up, so we need to flip the y axis
        projection.z_axis.z = 1.0 / 1000.0;
        projection.w_axis.w = 1.0;

        projection
    }

    pub fn vertex_source(&self) -> VertexSource<PosColTexVertex> {
        self.vertices.vertex_source()
    }

    pub fn begin_srgb_render_pass<'a>(
        &'a self,
        encoder: &'a mut wgpu::CommandEncoder,
        label: Option<&str>,
    ) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.srgb_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None
        })
    }

    pub fn begin_raw_render_pass<'a>(
        &'a self,
        encoder: &'a mut wgpu::CommandEncoder,
        label: Option<&str>,
    ) -> wgpu::RenderPass<'a> {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.raw_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None
        })
    }

    pub fn bind_group(&self) -> &TextureBindGroup {
        &self.bind_group
    }
}
