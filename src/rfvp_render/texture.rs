use image::DynamicImage;
use wgpu::util::DeviceExt;

use super::GpuCommonResources;

#[derive(Debug)]
pub struct TextureBindGroup {
    bind_group: wgpu::BindGroup,
}

impl TextureBindGroup {
    // Requested: constructor instead of struct literal initialization.
    pub fn new(bind_group: wgpu::BindGroup) -> Self {
        Self { bind_group }
    }

    pub fn raw(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}

pub struct BindGroupLayouts {
    pub texture: wgpu::BindGroupLayout,
}

impl BindGroupLayouts {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rfvp_render.texture_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        Self { texture }
    }
}

#[derive(Debug)]
pub struct GpuTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    bind_group: TextureBindGroup,
    size: (u32, u32),
}

impl GpuTexture {
    pub fn new(resources: &GpuCommonResources, img: &DynamicImage, label: Option<&str>) -> Self {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();

                let texture_desc = wgpu::TextureDescriptor {
            label,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };

        let texture = resources
            .device
            .create_texture_with_data(&resources.queue, &texture_desc, wgpu::util::TextureDataOrder::LayerMajor, &rgba);

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = resources.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("rfvp_render.texture_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = resources.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rfvp_render.texture_bind_group"),
            layout: &resources.bind_group_layouts.texture,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        Self {
            texture,
            view,
            sampler,
            bind_group: TextureBindGroup::new(bind_group),
            size: (w, h),
        }
    }

    pub fn bind_group(&self) -> &TextureBindGroup {
        &self.bind_group
    }

    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    pub fn raw_view(&self) -> &wgpu::TextureView {
        &self.view
    }
}
