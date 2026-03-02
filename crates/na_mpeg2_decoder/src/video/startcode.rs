/// Equivalent to `avpriv_find_start_code()`.
///
/// - `state` must be preserved across calls.
/// - Returns the index in `data` to continue from.
/// - Updates `state` to the last 4 bytes (big-endian) observed.
pub fn find_start_code(data: &[u8], state: &mut u32) -> usize {
    let end = data.len();
    if end == 0 {
        return 0;
    }

    let mut p: usize = 0;

    for _ in 0..3 {
        let tmp = (*state) << 8;
        *state = tmp.wrapping_add(data[p] as u32);
        p += 1;
        if tmp == 0x100 || p == end {
            return p;
        }
    }

    while p < end {
        let b_1 = data[p - 1];
        if b_1 > 1 {
            p += 3;
        } else if data[p - 2] != 0 {
            p += 2;
        } else if (data[p - 3] | (b_1.wrapping_sub(1))) != 0 {
            p += 1;
        } else {
            p += 1;
            break;
        }
    }

    let p2 = std::cmp::min(p, end);
    let p_rb = p2.saturating_sub(4);
    let mut v: u32 = 0;
    for i in 0..4 {
        v = (v << 8) | data.get(p_rb + i).copied().unwrap_or(0) as u32;
    }
    *state = v;

    p_rb + 4
}
