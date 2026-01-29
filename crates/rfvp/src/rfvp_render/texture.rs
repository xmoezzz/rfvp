use image::DynamicImage;
use wgpu::util::DeviceExt;
use image::GenericImageView;

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
        Self::new_with_format(resources, img, label, wgpu::TextureFormat::Rgba8UnormSrgb)
    }

    /// Create a non-sRGB RGBA8 texture.
    ///
    /// This is used for linear data such as 8-bit mask textures (e.g., dissolve masks).
    pub fn new_rgba8_unorm(
        resources: &GpuCommonResources,
        img: &DynamicImage,
        label: Option<&str>,
    ) -> Self {
        Self::new_with_format(resources, img, label, wgpu::TextureFormat::Rgba8Unorm)
    }

    fn new_with_format(
        resources: &GpuCommonResources,
        img: &DynamicImage,
        label: Option<&str>,
        format: wgpu::TextureFormat,
    ) -> Self {
        // Avoid an extra allocation when the source is already RGBA8.
        let (rgba, w, h): (std::borrow::Cow<'_, [u8]>, u32, u32) = match img {
            DynamicImage::ImageRgba8(rgba) => {
                let (w, h) = rgba.dimensions();
                (std::borrow::Cow::Borrowed(rgba.as_raw()), w, h)
            }
            _ => {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                (std::borrow::Cow::Owned(rgba.into_raw()), w, h)
            }
        };

        let texture_desc = wgpu::TextureDescriptor {
            label,
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };

        let texture = resources.device.create_texture_with_data(
            &resources.queue,
            &texture_desc,
            wgpu::util::TextureDataOrder::LayerMajor,
            &rgba,
        );

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
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
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

    /// Update the whole texture with an RGBA8 image.
    ///
    /// This is intended for frequently-updated graphs (movie/text buffers) to avoid recreating
    /// GPU textures and bind groups every frame.
    ///
    /// Returns `false` if the image size does not match this texture.
    pub fn update_rgba8(&mut self, resources: &GpuCommonResources, img: &DynamicImage) -> bool {
        let (src_w, src_h) = img.dimensions();
        if (src_w, src_h) != self.size {
            return false;
        }

        let raw: std::borrow::Cow<'_, [u8]> = match img {
            DynamicImage::ImageRgba8(rgba) => std::borrow::Cow::Borrowed(rgba.as_raw()),
            _ => std::borrow::Cow::Owned(img.to_rgba8().into_raw()),
        };

        let bytes_per_row = 4u32.saturating_mul(src_w);
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = ((bytes_per_row + align - 1) / align) * align;

        let data: std::borrow::Cow<'_, [u8]> = if padded_bytes_per_row == bytes_per_row {
            raw
        } else {
            // Pad each row to meet wgpu's alignment requirement.
            let mut out = vec![0u8; (padded_bytes_per_row as usize) * (src_h as usize)];
            for y in 0..(src_h as usize) {
                let src_off = y * (bytes_per_row as usize);
                let dst_off = y * (padded_bytes_per_row as usize);
                out[dst_off..dst_off + (bytes_per_row as usize)]
                    .copy_from_slice(&raw[src_off..src_off + (bytes_per_row as usize)]);
            }
            std::borrow::Cow::Owned(out)
        };

        resources.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(src_h),
            },
            wgpu::Extent3d {
                width: src_w,
                height: src_h,
                depth_or_array_layers: 1,
            },
        );

        true
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
