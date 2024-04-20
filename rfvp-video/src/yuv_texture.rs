use rfvp_render::{GpuCommonResources, YuvTextureBindGroup};

use crate::h264_decoder::{BitsPerSample, Colorspace, Frame, FrameSize, PlaneSize};

pub struct YuvTexture {
    tex_y: wgpu::Texture,
    tex_u: wgpu::Texture,
    tex_v: wgpu::Texture,
    bind_group: YuvTextureBindGroup,
    size: FrameSize,
}

fn create_texture(device: &wgpu::Device, size: PlaneSize, label: &str) -> wgpu::Texture {
    assert_eq!(size.bits_per_sample, BitsPerSample::B8);
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn write_texture(texture: &wgpu::Texture, size: PlaneSize, data: &[u8], queue: &wgpu::Queue) {
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture,
            mip_level: 0,
            origin: Default::default(),
            aspect: Default::default(),
        },
        data,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(size.width),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth_or_array_layers: 1,
        },
    );
}

impl YuvTexture {
    pub fn new(resources: &GpuCommonResources, size: FrameSize) -> Self {
        // note that this assumes 4:2:0 chroma subsampling is used
        // as of now, this is the only subsampling supported by openh264 crate

        assert_eq!(size.colorspace, Colorspace::C420mpeg2);
        assert_eq!(size.plane_sizes[0].bits_per_sample, BitsPerSample::B8);

        let device = &resources.device;

        let tex_y = create_texture(device, size.plane_sizes[0], "VideoRenderer Y Texture");
        let tex_u = create_texture(device, size.plane_sizes[1], "VideoRenderer U Texture");
        let tex_v = create_texture(device, size.plane_sizes[2], "VideoRenderer V Texture");

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("VideoRenderer Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: Default::default(),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: Default::default(),
            lod_min_clamp: 0.0,
            lod_max_clamp: 0.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });

        let bind_group = YuvTextureBindGroup::new(
            resources,
            &tex_y.create_view(&Default::default()),
            &tex_u.create_view(&Default::default()),
            &tex_v.create_view(&Default::default()),
            &sampler,
            Some("VideoRenderer Bind Group"),
        );

        Self {
            tex_y,
            tex_u,
            tex_v,
            bind_group,
            size,
        }
    }

    pub fn write_data(&self, yuv: &Frame, queue: &wgpu::Queue) {
        // this, theoretically, supports different subsamplings, but has not been tested
        // also, there would definitely be problems with non-even sizes & shifted chroma planes
        let [size_y, size_u, size_v] = self.size.plane_sizes;

        write_texture(&self.tex_y, size_y, yuv.get_y_plane(), queue);
        write_texture(&self.tex_u, size_u, yuv.get_u_plane(), queue);
        write_texture(&self.tex_v, size_v, yuv.get_v_plane(), queue);
    }

    pub fn bind_group(&self) -> &YuvTextureBindGroup {
        &self.bind_group
    }
}
