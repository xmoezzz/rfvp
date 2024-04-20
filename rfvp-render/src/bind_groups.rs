use std::ops::Deref;

use crate::common_resources::GpuCommonResources;

pub struct BindGroupLayouts {
    pub texture: wgpu::BindGroupLayout,
    pub yuv_texture: wgpu::BindGroupLayout,
}

impl BindGroupLayouts {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            texture: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("TextureBindGroup layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
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
            }),
            yuv_texture: device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("YuvTextureBindGroup layout"),
                entries: &[
                    // Y texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // U texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // V texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // texture sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            }),
        }
    }
}

pub struct TextureBindGroup(pub wgpu::BindGroup);
impl TextureBindGroup {
    pub fn new(
        resources: &GpuCommonResources,
        texture_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        label: Option<&str>,
    ) -> Self {
        let bind_group = resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label,
                layout: &resources.bind_group_layouts.texture,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            });
        Self(bind_group)
    }
}
impl Deref for TextureBindGroup {
    type Target = wgpu::BindGroup;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct YuvTextureBindGroup(pub wgpu::BindGroup);
impl YuvTextureBindGroup {
    pub fn new(
        resources: &GpuCommonResources,
        y_texture_view: &wgpu::TextureView,
        u_texture_view: &wgpu::TextureView,
        v_texture_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        label: Option<&str>,
    ) -> Self {
        let bind_group = resources
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label,
                layout: &resources.bind_group_layouts.yuv_texture,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(y_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(u_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(v_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            });
        Self(bind_group)
    }
}
