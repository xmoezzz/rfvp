//! Let the GPU handle small reductions. Avoid the CPU area filter for 800x600 games.
//! Keep the existing reduction for large pictures; native textures are capped at 8 MiB.
pub fn kept_size(scale: f32, width: u32, height: u32) -> (u32, u32) {
    if scale >= 0.999 || (scale >= 0.75 && width <= 2048 && height <= 2048
        && width as u64 * height as u64 <= 2 * 1024 * 1024) {
        (width, height)
    } else {
        scaled_size(scale, width, height)
    }
}

/// The original screen-scale policy, also used if a native allocation fails.
pub fn scaled_size(scale: f32, width: u32, height: u32) -> (u32, u32) {
    if scale >= 0.999 { return (width, height); }
    (((width as f32 * scale + 0.5) as u32).max(1),
     ((height as f32 * scale + 0.5) as u32).max(1))
}
