//! Let the GPU handle small reductions. Avoid the CPU area filter for 800x600 games.
//! Keep the existing reduction for large pictures; transparent sprites have a bounded exception.
pub const NATIVE_PIXEL_LIMIT: u64 = 2 * 1024 * 1024;
/// A narrow exception for transparent character sprites just over the native cap. The
/// reported 1424x1853 sprite is 2,638,672 pixels, just 0.66% over the former
/// 2,621,440 cap, and still spends ~1.1 s in the CPU filter on a full upload.
/// The exception is now bounded at 2,700,000 pixels (10,800,000 RGBA bytes),
/// <= 2048 per side, and only when the source actually has transparency. Opaque backgrounds
/// keep the normal 2 MiB limit.
pub const NEAR_CAP_PIXEL_LIMIT: u64 = 2_700_000;

pub fn kept_size(scale: f32, width: u32, height: u32) -> (u32, u32) {
    policy(scale, width, height).0
}

/// `kept_size` plus the reason, for the bounded texture diagnostics. New labels are
/// deliberately stable strings so a device log can be matched to this code.
pub fn policy(scale: f32, width: u32, height: u32) -> ((u32, u32), &'static str) {
    policy_with_alpha(scale, width, height, false)
}

/// Like [`policy`], but a transparent source may keep native pixels up to
/// [`NEAR_CAP_PIXEL_LIMIT`] on either side <= 2048. Used for character sprites whose source
/// alpha is known before the upload; opaque backgrounds pass `false`.
pub fn policy_with_alpha(
    scale: f32,
    width: u32,
    height: u32,
    transparent: bool,
) -> ((u32, u32), &'static str) {
    if scale >= 0.999 {
        return ((width, height), "scale-1");
    }
    if scale < 0.75 {
        return (scaled_size(scale, width, height), "scale-below-0.75");
    }
    if width > 2048 || height > 2048 {
        return (scaled_size(scale, width, height), "dimension-limit");
    }
    let pixels = width as u64 * height as u64;
    if pixels <= NATIVE_PIXEL_LIMIT {
        return ((width, height), "native-eligible");
    }
    if transparent && pixels <= NEAR_CAP_PIXEL_LIMIT {
        return ((width, height), "native-transparent-near-cap");
    }
    (scaled_size(scale, width, height), "pixel-limit")
}

/// Whether any pixel's alpha channel is not 255. `bpp`/`alpha_index` describe the source
/// format (RGBA/BGRA 4/3, LumaA 2/1, Alpha 1/0). Bounded by `count * bpp`; short/invalid
/// input returns false so the caller keeps the old scaled policy.
pub fn has_transparent_alpha(
    pixels: &[u8],
    count: usize,
    bpp: usize,
    alpha_index: usize,
) -> bool {
    if bpp == 0 || alpha_index >= bpp {
        return false;
    }
    let Some(len) = count.checked_mul(bpp) else {
        return false;
    };
    if pixels.len() < len {
        return false;
    }
    pixels[..len].chunks_exact(bpp).any(|p| p[alpha_index] != 255)
}

/// The original screen-scale policy, also used if a native allocation fails.
pub fn scaled_size(scale: f32, width: u32, height: u32) -> (u32, u32) {
    if scale >= 0.999 { return (width, height); }
    (((width as f32 * scale + 0.5) as u32).max(1),
     ((height as f32 * scale + 0.5) as u32).max(1))
}
