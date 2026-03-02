#[inline]
pub fn clip_u8(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

#[inline]
pub fn avg_u8(a: u8, b: u8) -> u8 {
    ((a as u16 + b as u16 + 1) >> 1) as u8
}
