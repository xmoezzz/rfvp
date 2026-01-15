pub fn nv12_to_rgba_strided(
    width: u32,
    height: u32,
    y_stride: usize,
    uv_stride: usize,
    y_plane: &[u8],
    uv_plane: &[u8],
) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;

    let mut out = vec![0u8; w * h * 4];

    for y in 0..h {
        let y_row = &y_plane[y * y_stride..y * y_stride + w];
        let uv_row = &uv_plane[(y / 2) * uv_stride..(y / 2) * uv_stride + w];

        for x in 0..w {
            let yy = y_row[x] as i32;
            let uu = uv_row[x & !1] as i32;
            let vv = uv_row[(x & !1) + 1] as i32;

            let c = yy - 16;
            let d = uu - 128;
            let e = vv - 128;

            let r = (298 * c + 409 * e + 128) >> 8;
            let g = (298 * c - 100 * d - 208 * e + 128) >> 8;
            let b = (298 * c + 516 * d + 128) >> 8;

            let r = r.clamp(0, 255) as u8;
            let g = g.clamp(0, 255) as u8;
            let b = b.clamp(0, 255) as u8;

            let o = (y * w + x) * 4;
            out[o] = r;
            out[o + 1] = g;
            out[o + 2] = b;
            out[o + 3] = 255;
        }
    }

    out
}

pub fn bgra_to_rgba_inplace(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        let b = px[0];
        let r = px[2];
        px[0] = r;
        px[2] = b;
    }
}


/// Convert a YUV_420_888 style 3-plane image into RGBA.
pub fn yuv420_888_to_rgba(
    width: u32,
    height: u32,
    y_row_stride: usize,
    u_row_stride: usize,
    v_row_stride: usize,
    u_pixel_stride: usize,
    v_pixel_stride: usize,
    y_plane: &[u8],
    u_plane: &[u8],
    v_plane: &[u8],
) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;

    let mut out = vec![0u8; w * h * 4];

    for y in 0..h {
        let y_off = y * y_row_stride;
        let uv_y = y / 2;

        for x in 0..w {
            let uv_x = x / 2;

            let yy = y_plane[y_off + x] as i32;
            let u_idx = uv_y * u_row_stride + uv_x * u_pixel_stride;
            let v_idx = uv_y * v_row_stride + uv_x * v_pixel_stride;

            let uu = u_plane.get(u_idx).copied().unwrap_or(128) as i32;
            let vv = v_plane.get(v_idx).copied().unwrap_or(128) as i32;

            let c = yy - 16;
            let d = uu - 128;
            let e = vv - 128;

            let r = (298 * c + 409 * e + 128) >> 8;
            let g = (298 * c - 100 * d - 208 * e + 128) >> 8;
            let b = (298 * c + 516 * d + 128) >> 8;

            let r = r.clamp(0, 255) as u8;
            let g = g.clamp(0, 255) as u8;
            let b = b.clamp(0, 255) as u8;

            let o = (y * w + x) * 4;
            out[o] = r;
            out[o + 1] = g;
            out[o + 2] = b;
            out[o + 3] = 255;
        }
    }

    out
}
