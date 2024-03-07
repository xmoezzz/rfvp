use image::{DynamicImage, GenericImageView};
use anyhow::{Result, anyhow};

fn copy_rect(
    src: &DynamicImage,
    src_x: u32,
    src_y: u32,
    src_w: u32,
    src_h: u32,
    dest: &mut DynamicImage,
    dest_x: u32,
    dest_y: u32,
) -> Result<()> {
    let src = src.view(src_x, src_y, src_w, src_h);
    let dest = match dest.as_mut_rgba8() {
        Some(dest) => dest,
        None => return Err(anyhow!("copy_rect: dest image is not in RGBA8 format")),
    };
    for y in 0..src_h {
        for x in 0..src_w {
            let src_pixel = src.get_pixel(x + src_x, y + src_y);
            let dest_pixel = dest.get_pixel_mut(x + dest_x, y + dest_y);
            *dest_pixel = src_pixel;
        }
    }
    
    Ok(())
}