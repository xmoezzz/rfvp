//! MDCT/IMDCT implementation.
//!


use std::f64::consts::PI;

pub struct MdctNaive {
    /// Frame size (N). Full IMDCT output is 2N.
    pub len: usize,
    /// Scale factor (double precision in upstream).
    pub scale: f64,
}

impl MdctNaive {
    pub fn new(len: usize, scale: f64) -> Self {
        Self { len, scale }
    }

    /// Half-length inverse MDCT.
    ///
    /// Input: N coefficients.
    /// Output: N samples (half IMDCT), matching upstream's MDCT semantics.
    pub fn imdct_half(&self, dst: &mut [f32], src: &[f32]) {
        // Translated from `ff_tx_mdct_naive_inv`.
        // In upstream: len = s->len >> 1; len2 = len*2 (== s->len)
        let len = self.len >> 1;
        let len2 = len * 2;
        let phase = PI / (4.0 * (len2 as f64));

        for i in 0..len {
            let mut sum_d: f64 = 0.0;
            let mut sum_u: f64 = 0.0;

            let i_d = phase * ((4 * len - 2 * i - 1) as f64);
            let i_u = phase * ((3 * len2 + 2 * i + 1) as f64);

            for j in 0..len2 {
                let a = (2 * j + 1) as f64;
                let a_d = (a * i_d).cos();
                let a_u = (a * i_u).cos();
                let val = src[j] as f64;
                sum_d += a_d * val;
                sum_u += a_u * val;
            }

            dst[i] = (sum_d * self.scale) as f32;
            dst[i + len] = (-(sum_u * self.scale)) as f32;
        }
    }

    /// Full IMDCT.
    ///
    /// Input: N coefficients.
    /// Output: 2N samples.
    pub fn imdct_full(&self, dst: &mut [f32], src: &[f32]) {
        // Translated from `ff_tx_mdct_inv_full`.
        let len = self.len * 2;
        let len2 = len / 2;
        let len4 = len / 4;

        // The half IMDCT is written into the middle of the output.
        self.imdct_half(&mut dst[len4..len4 + len2], src);

        for i in 0..len4 {
            dst[i] = -dst[len2 - i - 1];
            dst[len - i - 1] = dst[len2 + i];
        }
    }
}
