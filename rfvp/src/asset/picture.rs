use anyhow::Result;
use glam::vec2;
use rfvp_core::format::pic::NvsgTexture;
use rfvp_render::{GpuCommonResources, GpuImage, LazyGpuImage};

use crate::asset::Asset;

/// A Picture, uploaded to GPU on demand (because doing it in the asset loading context is awkward)
pub struct Picture {
    picture: LazyGpuImage,
    nvsg_texture: NvsgTexture,
}

impl Picture {
    pub fn gpu_image(&self, resources: &GpuCommonResources) -> &GpuImage {
        self.picture.gpu_image(resources)
    }
}

impl Asset for Picture {
    fn load_from_bytes(data: Vec<u8>) -> Result<Self> {
        let mut container = NvsgTexture::new();
        container.read_texture(&buffer, |typ: TextureType| {true})?;
        let pic = container.get_texture(0)?;
        let image = pic.to_rgba8()?;

        let picture = LazyGpuImage::new(
            image,
            vec2(container.get_offset_x() as f32, container.get_offset_y() as f32),
            None,
        );

        Ok(Self { picture, nvsg_texture })
    }
}
