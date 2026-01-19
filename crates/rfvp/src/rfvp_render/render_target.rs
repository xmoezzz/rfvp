use glam::{mat4, vec4, Mat4, Vec4};
use wgpu::util::DeviceExt;

use super::{GpuCommonResources, TextureBindGroup};
use super::vertices::{PosColTexVertex, VertexSource};



#[derive(Debug)]
pub struct RenderTargetReadback {
    pub buffer: wgpu::Buffer,
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: u32,
    pub padded_bytes_per_row: u32,
}

impl RenderTargetReadback {
    // get pixels as RGBA8 slice
pub fn map_to_rgba8(&self, device: &wgpu::Device) -> Vec<u8> {
        let height = self.height as usize;
        let dst_bpr = self.bytes_per_row as usize;
        let src_bpr = self.padded_bytes_per_row as usize;

        if height == 0 || dst_bpr == 0 {
            return Vec::new();
        }

        let slice = self.buffer.slice(..);

        // wgpu::Buffer::map_async is async; block here without extra crates.
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });

        // Ensure the mapping completes.
        device.poll(wgpu::Maintain::Wait);

        // Propagate mapping failure loudly (you can change to Result if preferred).
        rx.recv().expect("map_async callback dropped").expect("buffer map failed");

        let mapped = slice.get_mapped_range();

        // Strip per-row padding.
        let mut out = vec![0u8; height * dst_bpr];
        for y in 0..height {
            let src0 = y * src_bpr;
            let src1 = src0 + dst_bpr;
            let dst0 = y * dst_bpr;
            let dst1 = dst0 + dst_bpr;
            out[dst0..dst1].copy_from_slice(&mapped[src0..src1]);
        }

        drop(mapped);
        self.buffer.unmap();

        out
    }
}

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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
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

        // Fullscreen quad in virtual pixel space.
        // Coordinate system: origin at top-left, x right, y down.
        let vw = w as f32;
        let vh = h as f32;
        let x0 = 0.0;
        let y0 = 0.0;
        let x1 = vw;
        let y1 = vh;

        let white = Vec4::ONE;
        let v = [
            // Two triangles (0,1,2) (2,1,3)
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
        // Virtual space: origin at top-left, x right, y down.
        // Maps pixel coordinates (0..w, 0..h) into NDC (-1..1, 1..-1).
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        mat4(
            vec4(2.0 / w, 0.0, 0.0, 0.0),
            vec4(0.0, -2.0 / h, 0.0, 0.0),
            vec4(0.0, 0.0, 1.0, 0.0),
            vec4(-1.0, 1.0, 0.0, 1.0),
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


    /// Encode a GPU -> CPU readback of the full render target into an internal buffer.
    /// The returned buffer is MAP_READ and must be mapped after submission.
    pub fn encode_readback_rgba8(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) -> RenderTargetReadback {
        let width = self.size.0;
        let height = self.size.1;
        let bytes_per_row = 4u32.saturating_mul(width);
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = ((bytes_per_row + align - 1) / align) * align;

        let buffer_size = (padded_bytes_per_row as u64) * (height as u64);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rfvp_render.render_target.readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        RenderTargetReadback {
            buffer,
            width,
            height,
            bytes_per_row,
            padded_bytes_per_row,
        }
    }
}
