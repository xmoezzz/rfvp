use std::borrow::Cow;

use glam::{vec4, Vec2};
use image::RgbaImage;
use once_cell::sync::OnceCell;

use crate::{
    vertices::{PosColTexVertex, VertexSource},
    GpuCommonResources, SpriteVertexBuffer, TextureBindGroup, SRGB_TEXTURE_FORMAT,
};

pub struct LazyGpuImage {
    image: RgbaImage,
    origin: Vec2,
    label: Option<String>,
    gpu_image: OnceCell<GpuImage>,
}

impl LazyGpuImage {
    pub fn new(image: RgbaImage, origin: Vec2, label: Option<&str>) -> Self {
        Self {
            image,
            origin,
            label: label.map(|s| s.to_owned()),
            gpu_image: OnceCell::new(),
        }
    }

    pub fn gpu_image(&self, resources: &GpuCommonResources) -> &GpuImage {
        self.gpu_image.get_or_init(|| {
            GpuImage::load(resources, &self.image, self.origin, self.label.as_deref())
        })
    }
}

pub struct LazyGpuTexture {
    image: RgbaImage,
    label: Option<String>,
    gpu_texture: OnceCell<GpuTexture>,
}

impl LazyGpuTexture {
    pub fn new(image: RgbaImage, label: Option<&str>) -> Self {
        Self {
            image,
            label: label.map(|s| s.to_owned()),
            gpu_texture: OnceCell::new(),
        }
    }

    pub fn gpu_texture(&self, resources: &GpuCommonResources) -> &GpuTexture {
        self.gpu_texture
            .get_or_init(|| GpuTexture::load(resources, &self.image, self.label.as_deref()))
    }
}

/// Gpu picture, ready to be drawn
/// Includes a texture, a sampler, a bind group, and a vertex buffer
pub struct GpuImage {
    pub texture: GpuTexture,
    pub vertex_buffer: SpriteVertexBuffer,
}

impl GpuImage {
    pub fn load(
        resources: &GpuCommonResources,
        image: &RgbaImage,
        origin: Vec2,
        label: Option<&str>,
    ) -> Self {
        let label = label
            .map(|s| Cow::from(s.to_owned()))
            .unwrap_or_else(|| Cow::from("Unnamed GpuPicture"));

        let texture = GpuTexture::load(resources, image, Some(&label));

        let origin_translate = -origin.extend(0.0);

        let vertex_buffer = SpriteVertexBuffer::new(
            resources,
            (
                origin_translate.x,
                origin_translate.y,
                origin_translate.x + image.width() as f32,
                origin_translate.y + image.height() as f32,
            ),
            // TODO: do we even want colored vertices?..
            vec4(1.0, 1.0, 1.0, 1.0),
        );

        GpuImage {
            texture,
            vertex_buffer,
        }
    }

    pub fn bind_group(&self) -> &TextureBindGroup {
        &self.texture.bind_group
    }

    pub fn vertex_source(&self) -> VertexSource<PosColTexVertex> {
        self.vertex_buffer.vertex_source()
    }
}

/// Gpu texture
/// Includes a texture, a sampler, and a bind group (no vertex buffer)
pub struct GpuTexture {
    pub texture: wgpu::Texture,
    pub sampler: wgpu::Sampler,
    pub bind_group: TextureBindGroup,
    pub width: u32,
    pub height: u32,
}

impl GpuTexture {
    pub fn load(resources: &GpuCommonResources, image: &RgbaImage, label: Option<&str>) -> Self {
        let label = label
            .map(|s| Cow::from(s.to_owned()))
            .unwrap_or_else(|| Cow::from("Unnamed GpuTexture"));

        let size = wgpu::Extent3d {
            width: image.width(),
            height: image.height(),
            depth_or_array_layers: 1,
        };

        assert_eq!(
            SRGB_TEXTURE_FORMAT,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "Only Rgba8UnormSrgb is supported for now"
        );

        let texture = resources.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("{} Texture", label)),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: SRGB_TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        resources.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: Default::default(),
                aspect: wgpu::TextureAspect::All,
            },
            image,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * image.width()),
                rows_per_image: Some(image.height()),
            },
            wgpu::Extent3d {
                width: image.width(),
                height: image.height(),
                depth_or_array_layers: 1,
            },
        );

        let sampler = resources.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&format!("{} Sampler", label)),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = TextureBindGroup::new(
            resources,
            &texture_view,
            &sampler,
            Some(&format!("{} BindGroup", label)),
        );

        Self {
            texture,
            sampler,
            bind_group,
            width: image.width(),
            height: image.height(),
        }
    }

    pub fn bind_group(&self) -> &TextureBindGroup {
        &self.bind_group
    }
}
