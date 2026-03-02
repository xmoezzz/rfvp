use super::utils::clip_u8;

// Constants for BIT_DEPTH=8 path.
const W1: i64 = 22725;
const W2: i64 = 21407;
const W3: i64 = 19266;
const W4: i64 = 16383;
const W5: i64 = 12873;
const W6: i64 = 8867;
const W7: i64 = 4520;

const ROW_SHIFT: i64 = 11;
const COL_SHIFT: i64 = 20;
const DC_SHIFT: i64 = 3;

#[inline]
fn idct_row_cond_dc(row: &mut [i16; 8]) {
    // This is the generic path (no DC-only shortcut).
    let r0 = row[0] as i64;
    let r1 = row[1] as i64;
    let r2 = row[2] as i64;
    let r3 = row[3] as i64;
    let r4 = row[4] as i64;
    let r5 = row[5] as i64;
    let r6 = row[6] as i64;
    let r7 = row[7] as i64;

    let mut a0 = W4 * r0 + (1 << (ROW_SHIFT + 0 - 1));
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

    if (r4 | r5 | r6 | r7) != 0 {
        a0 += W4 * r4 + W6 * r6;
        a1 += -W4 * r4 - W2 * r6;
        a2 += -W4 * r4 + W2 * r6;
        a3 += W4 * r4 - W6 * r6;

        b0 += W5 * r5 + W7 * r7;
        b1 += -W1 * r5 - W5 * r7;
        b2 += W7 * r5 + W3 * r7;
        b3 += W3 * r5 - W1 * r7;
    }

    row[0] = ((a0 + b0) >> ROW_SHIFT) as i16;
    row[7] = ((a0 - b0) >> ROW_SHIFT) as i16;
    row[1] = ((a1 + b1) >> ROW_SHIFT) as i16;
    row[6] = ((a1 - b1) >> ROW_SHIFT) as i16;
    row[2] = ((a2 + b2) >> ROW_SHIFT) as i16;
    row[5] = ((a2 - b2) >> ROW_SHIFT) as i16;
    row[3] = ((a3 + b3) >> ROW_SHIFT) as i16;
    row[4] = ((a3 - b3) >> ROW_SHIFT) as i16;
}

#[inline]
fn idct_cols(col: [i16; 8]) -> [i64; 8] {
    let c0 = col[0] as i64;
    let c1 = col[1] as i64;
    let c2 = col[2] as i64;
    let c3 = col[3] as i64;
    let c4 = col[4] as i64;
    let c5 = col[5] as i64;
    let c6 = col[6] as i64;
    let c7 = col[7] as i64;

    // a0 = W4 * (col0 + ((1<<(COL_SHIFT-1))/W4));
    let add = (1i64 << (COL_SHIFT - 1)) / W4;
    let mut a0 = W4 * (c0 + add);
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

    if (c4 | c5 | c6 | c7) != 0 {
        a0 += W4 * c4 + W6 * c6;
        a1 += -W4 * c4 - W2 * c6;
        a2 += -W4 * c4 + W2 * c6;
        a3 += W4 * c4 - W6 * c6;

        b0 += W5 * c5 + W7 * c7;
        b1 += -W1 * c5 - W5 * c7;
        b2 += W7 * c5 + W3 * c7;
        b3 += W3 * c5 - W1 * c7;
    }

    [a0 + b0, a1 + b1, a2 + b2, a3 + b3, a3 - b3, a2 - b2, a1 - b1, a0 - b0]
}

pub fn simple_idct_put(dest: &mut [u8], stride: usize, block: &mut [i16; 64]) {
    // Row transforms (in-place)
    for y in 0..8 {
        let mut row = [0i16; 8];
        for x in 0..8 {
            row[x] = block[y * 8 + x];
        }
        idct_row_cond_dc(&mut row);
        for x in 0..8 {
            block[y * 8 + x] = row[x];
        }
    }

    // Column transforms + store
    for x in 0..8 {
        let col = [
            block[0 * 8 + x],
            block[1 * 8 + x],
            block[2 * 8 + x],
            block[3 * 8 + x],
            block[4 * 8 + x],
            block[5 * 8 + x],
            block[6 * 8 + x],
            block[7 * 8 + x],
        ];
        let out = idct_cols(col);
        for y in 0..8 {
            let v = (out[y] >> COL_SHIFT) as i32;
            dest[y * stride + x] = clip_u8(v);
        }
    }
}

pub fn simple_idct_add(dest: &mut [u8], stride: usize, block: &mut [i16; 64]) {
    for y in 0..8 {
        let mut row = [0i16; 8];
        for x in 0..8 {
            row[x] = block[y * 8 + x];
        }
        idct_row_cond_dc(&mut row);
        for x in 0..8 {
            block[y * 8 + x] = row[x];
        }
    }

    for x in 0..8 {
        let col = [
            block[0 * 8 + x],
            block[1 * 8 + x],
            block[2 * 8 + x],
            block[3 * 8 + x],
            block[4 * 8 + x],
            block[5 * 8 + x],
            block[6 * 8 + x],
            block[7 * 8 + x],
        ];
        let out = idct_cols(col);
        for y in 0..8 {
            let add = (out[y] >> COL_SHIFT) as i32;
            let cur = dest[y * stride + x] as i32;
            dest[y * stride + x] = clip_u8(cur + add);
        }
    }
}

// For completeness; MPEG-2 DC scaling uses (1<<(3-dc_precision)) in the block decode.
#[allow(dead_code)]
pub fn dc_shift() -> i64 { DC_SHIFT }
