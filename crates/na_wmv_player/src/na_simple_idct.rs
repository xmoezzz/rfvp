#[inline(always)]
fn clip_u8(x: i32) -> u8 {
    if x < 0 { 0 } else if x > 255 { 255 } else { x as u8 }
}

// --- Constants (BIT_DEPTH=8) ---
const W1: i64 = 22725;
const W2: i64 = 21407;
const W3: i64 = 19266;
const W4: i64 = 16383;
const W5: i64 = 12873;
const W6: i64 = 8867;
const W7: i64 = 4520;

const ROW_SHIFT: i32 = 11;
const COL_SHIFT: i32 = 20;
const DC_SHIFT: i32 = 3;

// ((1<<(COL_SHIFT-1))/W4) in upstream (integer division)
const COL_RND_W4_DIV: i16 = ((1i64 << (COL_SHIFT - 1)) / W4) as i16;

#[inline(always)]
fn idct_row_cond_dc_int16_8bit(row: &mut [i16; 8]) {
    // DC-only shortcut (matches upstream's int16 path semantics).
    if row[1] == 0
        && row[2] == 0
        && row[3] == 0
        && row[4] == 0
        && row[5] == 0
        && row[6] == 0
        && row[7] == 0
    {
        let t: i16 = (((row[0] as i32) << DC_SHIFT) as i16);
        *row = [t; 8];
        return;
    }

    // Use i64 for safety; upstream uses carefully-sized unsigned intermediates.
    let r0 = row[0] as i64;
    let r1 = row[1] as i64;
    let r2 = row[2] as i64;
    let r3 = row[3] as i64;
    let r4 = row[4] as i64;
    let r5 = row[5] as i64;
    let r6 = row[6] as i64;
    let r7 = row[7] as i64;

    let mut a0 = W4 * r0 + (1i64 << (ROW_SHIFT - 1));
    let mut a1 = a0;
    let mut a2 = a0;
    let mut a3 = a0;

    a0 += W2 * r2;
    a1 += W6 * r2;
    a2 -= W6 * r2;
    a3 -= W2 * r2;

    let mut b0 = W1 * r1 + W3 * r3;
    let mut b1 = W3 * r1 - W7 * r3;
    let mut b2 = W5 * r1 - W1 * r3;
    let mut b3 = W7 * r1 - W5 * r3;

    if r4 != 0 || r5 != 0 || r6 != 0 || r7 != 0 {
        a0 += W4 * r4 + W6 * r6;
        a1 += -W4 * r4 - W2 * r6;
        a2 += -W4 * r4 + W2 * r6;
        a3 += W4 * r4 - W6 * r6;

        b0 += W5 * r5 + W7 * r7;
        b1 += -W1 * r5 - W5 * r7;
        b2 += W7 * r5 + W3 * r7;
        b3 += W3 * r5 - W1 * r7;
    }

    let rs = ROW_SHIFT as i64;
    row[0] = ((a0 + b0) >> rs) as i16;
    row[7] = ((a0 - b0) >> rs) as i16;
    row[1] = ((a1 + b1) >> rs) as i16;
    row[6] = ((a1 - b1) >> rs) as i16;
    row[2] = ((a2 + b2) >> rs) as i16;
    row[5] = ((a2 - b2) >> rs) as i16;
    row[3] = ((a3 + b3) >> rs) as i16;
    row[4] = ((a3 - b3) >> rs) as i16;
}

#[inline(always)]
fn idct_sparse_col_int16_8bit(block: &mut [i16; 64], col: usize) {
    // Column elements are block[col + 8*r]
    let c0 = block[col + 8 * 0] as i64;
    let c1 = block[col + 8 * 1] as i64;
    let c2 = block[col + 8 * 2] as i64;
    let c3 = block[col + 8 * 3] as i64;
    let c4 = block[col + 8 * 4] as i64;
    let c5 = block[col + 8 * 5] as i64;
    let c6 = block[col + 8 * 6] as i64;
    let c7 = block[col + 8 * 7] as i64;

    let mut a0 = W4 * (c0 + COL_RND_W4_DIV as i64);
    let mut a1 = a0;
    let mut a2 = a0;
    let mut a3 = a0;

    a0 += W2 * c2;
    a1 += W6 * c2;
    a2 -= W6 * c2;
    a3 -= W2 * c2;

    let mut b0 = W1 * c1 + W3 * c3;
    let mut b1 = W3 * c1 - W7 * c3;
    let mut b2 = W5 * c1 - W1 * c3;
    let mut b3 = W7 * c1 - W5 * c3;

    if c4 != 0 {
        a0 += W4 * c4;
        a1 += -W4 * c4;
        a2 += -W4 * c4;
        a3 += W4 * c4;
    }
    if c5 != 0 {
        b0 += W5 * c5;
        b1 += -W1 * c5;
        b2 += W7 * c5;
        b3 += W3 * c5;
    }
    if c6 != 0 {
        a0 += W6 * c6;
        a1 += -W2 * c6;
        a2 += W2 * c6;
        a3 += -W6 * c6;
    }
    if c7 != 0 {
        b0 += W7 * c7;
        b1 += -W5 * c7;
        b2 += W3 * c7;
        b3 += -W1 * c7;
    }

    let cs = COL_SHIFT as i64;
    block[col + 8 * 0] = ((a0 + b0) >> cs) as i16;
    block[col + 8 * 1] = ((a1 + b1) >> cs) as i16;
    block[col + 8 * 2] = ((a2 + b2) >> cs) as i16;
    block[col + 8 * 3] = ((a3 + b3) >> cs) as i16;
    block[col + 8 * 4] = ((a3 - b3) >> cs) as i16;
    block[col + 8 * 5] = ((a2 - b2) >> cs) as i16;
    block[col + 8 * 6] = ((a1 - b1) >> cs) as i16;
    block[col + 8 * 7] = ((a0 - b0) >> cs) as i16;
}

#[inline(always)]
fn idct_sparse_col_add_int16_8bit(dest: &mut [u8], dest_off: usize, line_size: usize, block: &[i16; 64], col: usize) {
    let c0 = block[col + 8 * 0] as i64;
    let c1 = block[col + 8 * 1] as i64;
    let c2 = block[col + 8 * 2] as i64;
    let c3 = block[col + 8 * 3] as i64;
    let c4 = block[col + 8 * 4] as i64;
    let c5 = block[col + 8 * 5] as i64;
    let c6 = block[col + 8 * 6] as i64;
    let c7 = block[col + 8 * 7] as i64;

    let mut a0 = W4 * (c0 + COL_RND_W4_DIV as i64);
    let mut a1 = a0;
    let mut a2 = a0;
    let mut a3 = a0;

    a0 += W2 * c2;
    a1 += W6 * c2;
    a2 -= W6 * c2;
    a3 -= W2 * c2;

    let mut b0 = W1 * c1 + W3 * c3;
    let mut b1 = W3 * c1 - W7 * c3;
    let mut b2 = W5 * c1 - W1 * c3;
    let mut b3 = W7 * c1 - W5 * c3;

    if c4 != 0 {
        a0 += W4 * c4;
        a1 += -W4 * c4;
        a2 += -W4 * c4;
        a3 += W4 * c4;
    }
    if c5 != 0 {
        b0 += W5 * c5;
        b1 += -W1 * c5;
        b2 += W7 * c5;
        b3 += W3 * c5;
    }
    if c6 != 0 {
        a0 += W6 * c6;
        a1 += -W2 * c6;
        a2 += W2 * c6;
        a3 += -W6 * c6;
    }
    if c7 != 0 {
        b0 += W7 * c7;
        b1 += -W5 * c7;
        b2 += W3 * c7;
        b3 += -W1 * c7;
    }

    let cs = COL_SHIFT as i64;
    let vals = [
        ((a0 + b0) >> cs) as i32,
        ((a1 + b1) >> cs) as i32,
        ((a2 + b2) >> cs) as i32,
        ((a3 + b3) >> cs) as i32,
        ((a3 - b3) >> cs) as i32,
        ((a2 - b2) >> cs) as i32,
        ((a1 - b1) >> cs) as i32,
        ((a0 - b0) >> cs) as i32,
    ];

    for r in 0..8usize {
        let off = dest_off + r * line_size + col;
        if off < dest.len() {
            let cur = dest[off] as i32;
            dest[off] = clip_u8(cur + vals[r]);
        }
    }
}

/// In-place 8x8 IDCT: equivalent to upstream `ff_simple_idct_int16_8bit`.
pub fn ff_simple_idct_int16_8bit(block: &mut [i16; 64]) {
    for r in 0..8usize {
        let mut row = [0i16; 8];
        for c in 0..8usize {
            row[c] = block[r * 8 + c];
        }
        idct_row_cond_dc_int16_8bit(&mut row);
        for c in 0..8usize {
            block[r * 8 + c] = row[c];
        }
    }
    for c in 0..8usize {
        idct_sparse_col_int16_8bit(block, c);
    }
}

/// Add an 8x8 IDCT block into destination: equivalent to upstream `ff_simple_idct_add_int16_8bit`.
pub fn ff_simple_idct_add_int16_8bit(dest: &mut [u8], dest_off: usize, line_size: usize, block: &mut [i16; 64]) {
    // Row transform in-place
    for r in 0..8usize {
        let mut row = [0i16; 8];
        for c in 0..8usize {
            row[c] = block[r * 8 + c];
        }
        idct_row_cond_dc_int16_8bit(&mut row);
        for c in 0..8usize {
            block[r * 8 + c] = row[c];
        }
    }
    // Column add (without overwriting block, like upstream idctSparseColAdd)
    let tmp = *block;
    for c in 0..8usize {
        idct_sparse_col_add_int16_8bit(dest, dest_off, line_size, &tmp, c);
    }
}

// --- WMV2 ABT helpers (ported from upstream simple_idct.c) ---

const CN_SHIFT: i32 = 12;
const RN_SHIFT: i32 = 15;
const C_SHIFT: i32 = 17; // (4+1+12)
const R_SHIFT: i32 = 11;

// Values computed exactly as upstream C_FIX/R_FIX with M_SQRT2 and +0.5 rounding.
const C1: i64 = 3784;
const C2: i64 = 1567;
const C3: i64 = 2896;

const R1: i64 = 30274;
const R2: i64 = 12540;
const R3: i64 = 23170;

#[inline(always)]
fn idct4col_add(dest: &mut [u8], dest_off: usize, line_size: usize, col: &[i16; 64], col_idx: usize) {
    // col points to block + i (column i), but in upstream idct4col_add reads col[8*0..8*3]
    let a0 = col[col_idx + 8 * 0] as i64;
    let a1 = col[col_idx + 8 * 1] as i64;
    let a2 = col[col_idx + 8 * 2] as i64;
    let a3 = col[col_idx + 8 * 3] as i64;

    let c0 = (a0 + a2) * C3 + (1i64 << (C_SHIFT - 1));
    let c2 = (a0 - a2) * C3 + (1i64 << (C_SHIFT - 1));
    let c1 = a1 * C1 + a3 * C2;
    let c3 = a1 * C2 - a3 * C1;

    let out = [
        ((c0 + c1) >> C_SHIFT) as i32,
        ((c2 + c3) >> C_SHIFT) as i32,
        ((c2 - c3) >> C_SHIFT) as i32,
        ((c0 - c1) >> C_SHIFT) as i32,
    ];

    for r in 0..4usize {
        let off = dest_off + r * line_size;
        if off < dest.len() {
            let cur = dest[off] as i32;
            dest[off] = clip_u8(cur + out[r]);
        }
    }
}

#[inline(always)]
fn idct4row(row: &mut [i16; 8]) {
    // Operates on row[0..3] only (upstream's idct4row)
    let a0 = row[0] as i64;
    let a1 = row[1] as i64;
    let a2 = row[2] as i64;
    let a3 = row[3] as i64;

    let c0 = (a0 + a2) * R3 + (1i64 << (R_SHIFT - 1));
    let c2 = (a0 - a2) * R3 + (1i64 << (R_SHIFT - 1));
    let c1 = a1 * R1 + a3 * R2;
    let c3 = a1 * R2 - a3 * R1;

    row[0] = ((c0 + c1) >> R_SHIFT) as i16;
    row[1] = ((c2 + c3) >> R_SHIFT) as i16;
    row[2] = ((c2 - c3) >> R_SHIFT) as i16;
    row[3] = ((c0 - c1) >> R_SHIFT) as i16;
}

/// WMV2 ABT: add an 8x4 IDCT block (top or bottom half). Equivalent to upstream `ff_simple_idct84_add`.
pub fn ff_simple_idct84_add(dest: &mut [u8], dest_off: usize, line_size: usize, block: &mut [i16; 64]) {
    // IDCT8 on each of the first 4 rows
    for r in 0..4usize {
        let mut row = [0i16; 8];
        for c in 0..8usize {
            row[c] = block[r * 8 + c];
        }
        idct_row_cond_dc_int16_8bit(&mut row);
        for c in 0..8usize {
            block[r * 8 + c] = row[c];
        }
    }
    let snap = *block;
    for c in 0..8usize {
        idct4col_add(dest, dest_off + c, line_size, &snap, c);
    }
}

/// WMV2 ABT: add a 4x8 IDCT block (left or right half). Equivalent to upstream `ff_simple_idct48_add`.
pub fn ff_simple_idct48_add(dest: &mut [u8], dest_off: usize, line_size: usize, block: &mut [i16; 64]) {
    // IDCT4 on each line (8 rows)
    for r in 0..8usize {
        let mut row = [0i16; 8];
        for c in 0..8usize {
            row[c] = block[r * 8 + c];
        }
        idct4row(&mut row);
        for c in 0..8usize {
            block[r * 8 + c] = row[c];
        }
    }
    // IDCT8 and store for first 4 columns
    let snap = *block;
    for c in 0..4usize {
        idct_sparse_col_add_int16_8bit(dest, dest_off, line_size, &snap, c);
    }
}
