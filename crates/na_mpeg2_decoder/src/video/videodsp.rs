/// Equivalent to `ff_emulated_edge_mc()` for 8-bit.
///
/// Copies a rectangle from `src` into `buf`, replicating edges when `src_x/src_y`
/// are outside the image bounds.
pub fn emulated_edge_mc(
    buf: &mut [u8],
    src: &[u8],
    buf_linesize: isize,
    src_linesize: isize,
    block_w: usize,
    block_h: usize,
    mut src_x: isize,
    mut src_y: isize,
    w: isize,
    h: isize,
) {
    if w == 0 || h == 0 {
        return;
    }

    // Clamp starting point similar to upstream.
    if src_y >= h {
        src_y = h - 1;
    } else if src_y <= -(block_h as isize) {
        src_y = 1 - block_h as isize;
    }
    if src_x >= w {
        src_x = w - 1;
    } else if src_x <= -(block_w as isize) {
        src_x = 1 - block_w as isize;
    }

    let start_y = 0.max(-src_y) as usize;
    let start_x = 0.max(-src_x) as usize;
    let end_y = (block_h as isize).min(h - src_y) as usize;
    let end_x = (block_w as isize).min(w - src_x) as usize;

    let copy_w = end_x - start_x;

    // Helper to read a pixel from src with absolute coordinates.
    let src_at = |x: isize, y: isize| -> u8 {
        let x = x.clamp(0, w - 1) as usize;
        let y = y.clamp(0, h - 1) as usize;
        src[y * (src_linesize as usize) + x]
    };

    // Fill rows.
    for by in 0..block_h {
        let yy = src_y + by as isize;
        for bx in 0..block_w {
            let xx = src_x + bx as isize;
            let v = src_at(xx, yy);
            let dst_index = by * (buf_linesize.unsigned_abs() as usize) + bx;
            buf[dst_index] = v;
        }
    }

    // The upstream implementation first copies valid interior region then extends
    // left/right. The above direct clamp per-pixel yields identical results.
    let _ = (start_y, start_x, end_y, end_x, copy_w); // keep variables referenced
}
