#[cfg(feature = "random")]
pub fn next_u32() -> u32 {
    rand::random::<u32>()
}

#[cfg(not(feature = "random"))]
#[cfg(any(not(feature = "no_std"), target_has_atomic = "64"))]
pub fn next_u32() -> u32 {
    use std::sync::atomic::{AtomicU64, Ordering};

    static STATE: AtomicU64 = AtomicU64::new(0x9e37_79b9_7f4a_7c15);
    let mut old = STATE.load(Ordering::Relaxed);
    loop {
        let mut x = old;
        x ^= x << 7;
        x ^= x >> 9;
        x = x.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        match STATE.compare_exchange_weak(old, x, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return (x >> 32) as u32,
            Err(current) => old = current,
        }
    }
}

#[cfg(not(feature = "random"))]
#[cfg(all(feature = "no_std", not(target_has_atomic = "64")))]
pub fn next_u32() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};

    static STATE: AtomicU32 = AtomicU32::new(0x9e37_79b9);
    let mut old = STATE.load(Ordering::Relaxed);
    loop {
        let mut x = old;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        match STATE.compare_exchange_weak(old, x, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return x,
            Err(current) => old = current,
        }
    }
}

pub fn next_f32() -> f32 {
    const SCALE: f32 = 1.0 / 4_294_967_296.0;
    (next_u32() as f32) * SCALE
}

pub fn range_i32(start: i32, end_exclusive: i32) -> i32 {
    if end_exclusive <= start {
        return start;
    }
    let span = (end_exclusive - start) as u32;
    start + (next_u32() % span) as i32
}

pub fn range_i32_inclusive(start: i32, end_inclusive: i32) -> i32 {
    if end_inclusive <= start {
        return start;
    }
    let span = (end_inclusive - start + 1) as u32;
    start + (next_u32() % span) as i32
}

pub fn range_f32(start: f32, end: f32) -> f32 {
    if !(end > start) {
        return start;
    }
    start + (end - start) * next_f32()
}
