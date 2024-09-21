use anyhow::Result;
pub use rfvp_derive::TextureArchive;
use rfvp_render::LazyGpuTexture;

use crate::asset::Asset;

// TODO: add strong typing with derive or smth

pub trait TextureArchiveBuilder {
    type Output;

    fn new() -> Self;
    fn add_texture(&mut self, name: &str, texture: LazyGpuTexture);
    fn build(self) -> Self::Output;
}

pub trait TextureArchive: Sync + Send + 'static {
    type Builder: TextureArchiveBuilder<Output = Self>;
}

impl<T: TextureArchive> Asset for T {
    fn load_from_bytes(data: Vec<u8>) -> Result<Self> {
        let archive = rfvp_core::format::texture_archive::read_texture_archive(&data)?;

        let mut builder = T::Builder::new();
        let mut textures = archive.textures.into_iter().map(Some).collect::<Vec<_>>();

        for (name, index) in archive.name_to_index.into_iter() {
            let texture = textures[index].take().unwrap();
            let image = LazyGpuTexture::new(texture, Some(&format!("TextureArchive[{:?}]", name)));
            builder.add_texture(&name, image);
        }

        Ok(builder.build())
    }
}
