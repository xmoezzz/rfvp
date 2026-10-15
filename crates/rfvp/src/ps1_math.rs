//! PS1 visual approximations: no lookup tables, heap allocations or cached state.

/// Sine/cosine for the engine's tenths-of-a-degree angles. Reduce once to +/- pi/4
/// and use fifth/sixth-degree Taylor polynomials. Analytic truncation error is
/// below 3.7e-5 for either output (under 0.03 pixels at radius 640).
pub(crate) fn sin_cos_angle(angle: i32) -> (f32, f32) {
    let angle = angle.rem_euclid(3600);
    let quadrant = (angle + 450) / 900;
    let residual = angle - quadrant * 900;
    let x = residual as f32 * (core::f32::consts::PI / 1800.0);
    let z = x * x;
    let sine = x * (1.0 + z * (-1.0 / 6.0 + z * (1.0 / 120.0)));
    let cosine = 1.0 + z * (-0.5 + z * (1.0 / 24.0 - z * (1.0 / 720.0)));
    match quadrant & 3 {
        0 => (sine, cosine),
        1 => (cosine, -sine),
        2 => (-sine, -cosine),
        _ => (-cosine, sine),
    }
}

/// Shared Q16 progress for visual motion. Typical durations use one native 32-bit
/// division; longer durations retain a safe 64-bit fallback. No division per axis.
pub(crate) fn progress(elapsed: i32, duration: i32, mode: u8) -> (i32, u32) {
    let e = elapsed.max(0) as u32;
    let d = duration.max(1) as u32;
    let half = d / 2;
    let (e, d, curve, segment) = match mode {
        4 | 5 if half > 0 && e <= half => (e, half, if mode == 4 { 2 } else { 3 }, 1),
        4 | 5 => (e.saturating_sub(half), d - half, if mode == 4 { 3 } else { 2 }, 2),
        _ => (e, d, mode, 0),
    };
    let ratio = if e >= d { 65536 } else if e <= 65535 {
        (e << 16) / d
    } else {
        ((u64::from(e) << 16) / u64::from(d)) as u32
    };
    let square = |t: u32| ((u64::from(t) * u64::from(t)) >> 16) as u32;
    let weight = match curve {
        2 => square(ratio),
        3 => 65536 - square(65536 - ratio),
        _ => ratio,
    };
    (segment, weight)
}

/// Truncate toward zero as the original interpolator does, with exact endpoints.
pub(crate) fn interpolate(src: i32, dst: i32, progress: (i32, u32)) -> i32 {
    let delta = i64::from(dst) - i64::from(src);
    let half = delta / 2;
    let (base, delta) = match progress.0 {
        1 => (i64::from(src), half),
        2 => (i64::from(src) + half, delta - half),
        _ => (i64::from(src), delta),
    };
    (base + delta * i64::from(progress.1) / 65536) as i32
}
