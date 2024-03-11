use std::path::Path;

use image::{DynamicImage, ImageBuffer, ImageFormat};

use crate::{
    subsystem::components::color::Color,
    utils::file::read_file,
};

/// Component used by the 2D Renderer to know which material to use when rendering a renderable object.
#[derive(Clone)]
pub enum Material {
    /// Fill with a color
    Color(Color),
    /// Use a texture. Note that this means the target object will need to have uv maps.
    Texture(String),
}

#[derive(Debug)]
pub(crate) struct Texture {
    pub(crate) bytes: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl Texture {
    pub fn from_png(file_path: &Path) -> Texture {
        if let Ok(bytes) = read_file(&file_path) {
            let converted_image = image::load_from_memory_with_format(&bytes, ImageFormat::Png);
            if let Ok(image) = converted_image {
                return Texture::create_texture_from_dynamic_image(image);
            }
        }
        log::error!("Error while loading your texture, loading fallback texture instead.");
        Texture::fallback_texture()
    }

    pub fn from_raw_buffer(bytes: Vec<u8>, width: u32, height: u32) -> Texture {
        Texture { bytes, width, height }
    }

    pub fn from_color(color: &Color) -> Texture {
        let img = ImageBuffer::from_fn(1, 1, |_x, _y| {
            image::Rgba([color.red(), color.green(), color.blue(), (color.alpha() * 255.) as u8])
        });
        Texture { bytes: img.into_raw(), width: 1, height: 1 }
    }

    fn fallback_texture() -> Texture {
        return Texture::from_color(&Color::color_white());
    }

    fn create_texture_from_dynamic_image(dynamic_image: DynamicImage) -> Texture {
        let image = dynamic_image.to_rgba8();
        let width = image.width();
        let height = image.height();
        let bytes = image.into_raw();

        Texture { bytes, width, height }
    }
}
