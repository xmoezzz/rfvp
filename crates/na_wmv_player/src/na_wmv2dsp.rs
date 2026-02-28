

#[inline(always)]
fn clip_u8(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

const W0: i32 = 2048;
const W1: i32 = 2841;
const W2: i32 = 2676;
const W3: i32 = 2408;
const W4: i32 = 2048;
const W5: i32 = 1609;
const W6: i32 = 1108;
const W7: i32 = 565;

#[inline(always)]
fn wmv2_idct_row(b: &mut [i16]) {
    debug_assert!(b.len() == 8);
    let (b0, b1, b2, b3, b4, b5, b6, b7) = (
        b[0] as i32,
        b[1] as i32,
        b[2] as i32,
        b[3] as i32,
        b[4] as i32,
        b[5] as i32,
        b[6] as i32,
        b[7] as i32,
    );

    // step 1
    let a1 = W1 * b1 + W7 * b7;
    let a7 = W7 * b1 - W1 * b7;
    let a5 = W5 * b5 + W3 * b3;
    let a3 = W3 * b5 - W5 * b3;
    let a2 = W2 * b2 + W6 * b6;
    let a6 = W6 * b2 - W2 * b6;
    let a0 = W0 * b0 + W0 * b4;
    let a4 = W0 * b0 - W0 * b4;

    // step 2
    let s1 = ((181i32 * (a1 - a5 + a7 - a3) + 128) >> 8) as i32;
    let s2 = ((181i32 * (a1 - a5 - a7 + a3) + 128) >> 8) as i32;

    // step 3
    b[0] = ((a0 + a2 + a1 + a5 + (1 << 7)) >> 8) as i16;
    b[1] = ((a4 + a6 + s1 + (1 << 7)) >> 8) as i16;
    b[2] = ((a4 - a6 + s2 + (1 << 7)) >> 8) as i16;
    b[3] = ((a0 - a2 + a7 + a3 + (1 << 7)) >> 8) as i16;
    b[4] = ((a0 - a2 - a7 - a3 + (1 << 7)) >> 8) as i16;
    b[5] = ((a4 - a6 - s2 + (1 << 7)) >> 8) as i16;
    b[6] = ((a4 + a6 - s1 + (1 << 7)) >> 8) as i16;
    b[7] = ((a0 + a2 - a1 - a5 + (1 << 7)) >> 8) as i16;
}

#[inline(always)]
fn wmv2_idct_col(block: &mut [i16; 64], col: usize) {
    // step 1, with extended precision
    let b1 = block[8 * 1 + col] as i32;
    let b7 = block[8 * 7 + col] as i32;
    let b5 = block[8 * 5 + col] as i32;
    let b3 = block[8 * 3 + col] as i32;
    let b2 = block[8 * 2 + col] as i32;
    let b6 = block[8 * 6 + col] as i32;
    let b0 = block[8 * 0 + col] as i32;
    let b4 = block[8 * 4 + col] as i32;

    let a1 = (W1 * b1 + W7 * b7 + 4) >> 3;
    let a7 = (W7 * b1 - W1 * b7 + 4) >> 3;
    let a5 = (W5 * b5 + W3 * b3 + 4) >> 3;
    let a3 = (W3 * b5 - W5 * b3 + 4) >> 3;
    let a2 = (W2 * b2 + W6 * b6 + 4) >> 3;
    let a6 = (W6 * b2 - W2 * b6 + 4) >> 3;
    let a0 = (W0 * b0 + W0 * b4) >> 3;
    let a4 = (W0 * b0 - W0 * b4) >> 3;

    // step 2
    let s1 = (181i32 * (a1 - a5 + a7 - a3) + 128) >> 8;
    let s2 = (181i32 * (a1 - a5 - a7 + a3) + 128) >> 8;

    // step 3
    block[8 * 0 + col] = ((a0 + a2 + a1 + a5 + (1 << 13)) >> 14) as i16;
    block[8 * 1 + col] = ((a4 + a6 + s1 + (1 << 13)) >> 14) as i16;
    block[8 * 2 + col] = ((a4 - a6 + s2 + (1 << 13)) >> 14) as i16;
    block[8 * 3 + col] = ((a0 - a2 + a7 + a3 + (1 << 13)) >> 14) as i16;

    block[8 * 4 + col] = ((a0 - a2 - a7 - a3 + (1 << 13)) >> 14) as i16;
    block[8 * 5 + col] = ((a4 - a6 - s2 + (1 << 13)) >> 14) as i16;
    block[8 * 6 + col] = ((a4 + a6 - s1 + (1 << 13)) >> 14) as i16;
    block[8 * 7 + col] = ((a0 + a2 - a1 - a5 + (1 << 13)) >> 14) as i16;
}

pub fn wmv2_idct_add(dest: &mut [u8], dest_off: usize, stride: usize, block: &mut [i16; 64]) {
    // row pass
    for i in (0..64).step_by(8) {
        wmv2_idct_row(&mut block[i..i + 8]);
    }
    // col pass
    for c in 0..8 {
        wmv2_idct_col(block, c);
    }

    // add
    for r in 0..8 {
        let d = dest_off + r * stride;
        let b = r * 8;
        for c in 0..8 {
            let idx = d + c;
            if idx >= dest.len() {
                continue;
            }
            let v = dest[idx] as i32 + block[b + c] as i32;
            dest[idx] = clip_u8(v);
        }
    }
}

pub fn wmv2_idct_put(dest: &mut [u8], dest_off: usize, stride: usize, block: &mut [i16; 64]) {
    // row pass
    for i in (0..64).step_by(8) {
        wmv2_idct_row(&mut block[i..i + 8]);
    }
    // col pass
    for c in 0..8 {
        wmv2_idct_col(block, c);
    }

    // put
    for r in 0..8 {
        let d = dest_off + r * stride;
        let b = r * 8;
        for c in 0..8 {
            let idx = d + c;
            if idx >= dest.len() {
                continue;
            }
            dest[idx] = clip_u8(block[b + c] as i32);
        }
    }
}
