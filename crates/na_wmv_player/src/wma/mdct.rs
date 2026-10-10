//! MDCT/IMDCT implementation.
//!

use std::f64::consts::PI;

/// FFT-based inverse MDCT. The historical name is retained for API compatibility.
pub struct MdctNaive {
    /// Frame size (N). Full IMDCT output is 2N.
    pub len: usize,
    /// Scale factor (double precision in upstream).
    pub scale: f64,
    plan: Option<FftPlan>,
}

impl MdctNaive {
    pub fn new(len: usize, scale: f64) -> Self {
        Self {
            len,
            scale,
            plan: (len >= 2 && len.is_power_of_two()).then(|| FftPlan::new(len)),
        }
    }

    /// Half-length inverse MDCT.
    ///
    /// Input: N coefficients.
    /// Output: N samples (half IMDCT), matching upstream's MDCT semantics.
    pub fn imdct_half(&self, dst: &mut [f32], src: &[f32]) {
        if let Some(plan) = self
            .plan
            .as_ref()
            .filter(|plan| plan.phase.len() * 2 == self.len)
        {
            plan.imdct_half(dst, src, self.scale);
            return;
        }
        // Retain support for non-power-of-two lengths and callers changing the
        // public length field. WMA uses power-of-two lengths exclusively.
        self.imdct_half_naive(dst, src);
    }

    fn imdct_half_naive(&self, dst: &mut [f32], src: &[f32]) {
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

#[derive(Clone, Copy, Default)]
struct Complex {
    re: f64,
    im: f64,
}

impl Complex {
    fn rotation(angle: f64) -> Self {
        let (im, re) = angle.sin_cos();
        Self { re, im }
    }

    fn mul(self, other: Self) -> Self {
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }
}

struct FftPlan {
    phase: Vec<Complex>,
    twiddles: Vec<Complex>,
    bit_reversed: Vec<usize>,
}

impl FftPlan {
    fn new(len: usize) -> Self {
        let fft_len = len / 2;
        let phase_step = -PI / len as f64;
        Self {
            // Split the constant quarter-sample phase equally between the
            // input and output rotations so they can share one table.
            phase: (0..fft_len)
                .map(|i| Complex::rotation(phase_step * (i as f64 + 0.125)))
                .collect(),
            twiddles: (0..fft_len / 2)
                .map(|i| Complex::rotation(-2.0 * PI * i as f64 / fft_len as f64))
                .collect(),
            bit_reversed: (0..fft_len)
                .map(|i| {
                    if fft_len == 1 {
                        0
                    } else {
                        i.reverse_bits() >> (usize::BITS - fft_len.trailing_zeros())
                    }
                })
                .collect(),
        }
    }

    fn imdct_half(&self, dst: &mut [f32], src: &[f32], scale: f64) {
        let fft_len = self.phase.len();
        let len = 2 * fft_len;
        let mut work = vec![Complex::default(); fft_len];

        // The required half IMDCT is the reversed DCT-IV:
        // D[k] = sum_j x[j] cos(pi/N * (j + 1/2) * (k + 1/2)).
        // Pack z[j] = x[2j] + i*x[N-1-2j], j = 0..N/2, and rotate
        // by exp(-i*pi/N * (j + 1/8)). After an N/2-point forward FFT
        // and the same rotation at index k, the real part is D[2k]
        // and the imaginary part is -D[N-1-2k]. Both components are
        // useful; no zero padding or redundant FFT outputs are needed.
        for j in 0..fft_len {
            work[self.bit_reversed[j]] = Complex {
                re: src[2 * j] as f64,
                im: src[len - 1 - 2 * j] as f64,
            }
            .mul(self.phase[j]);
        }

        let mut width = 2;
        while width <= fft_len {
            let half = width / 2;
            let stride = fft_len / width;
            for block in work.chunks_exact_mut(width) {
                for j in 0..half {
                    let even = block[j];
                    let odd = block[j + half].mul(self.twiddles[j * stride]);
                    block[j] = Complex {
                        re: even.re + odd.re,
                        im: even.im + odd.im,
                    };
                    block[j + half] = Complex {
                        re: even.re - odd.re,
                        im: even.im - odd.im,
                    };
                }
            }
            width *= 2;
        }

        for k in 0..fft_len {
            let value = work[k].mul(self.phase[k]);
            dst[len - 1 - 2 * k] = (value.re * scale) as f32;
            dst[2 * k] = (-value.im * scale) as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MdctNaive;

    #[test]
    fn packed_imdct_preserves_each_coefficient_and_scale() {
        // Exercise both members of every complex pair, including the N=2
        // case whose one-point FFT has no butterfly stages.
        for len in [2, 4, 8, 16, 32] {
            for coefficient in 0..len {
                let mut input = vec![0.0; len];
                input[coefficient] = 1.0;
                for scale in [1.0, -0.25, 0.0] {
                    let transform = MdctNaive::new(len, scale);
                    let mut expected = vec![0.0; len];
                    transform.imdct_half_naive(&mut expected, &input);
                    let mut actual = vec![123.0; len + 3];
                    transform.imdct_half(&mut actual, &input);
                    for i in 0..len {
                        assert!(
                            (actual[i] - expected[i]).abs() < 1e-7,
                            "N={len} coefficient={coefficient} scale={scale} i={i}"
                        );
                    }
                    assert_eq!(&actual[len..], &[123.0; 3]);
                }
            }
        }
    }

    #[test]
    fn fft_matches_reference_half_and_full_imdct() {
        // Include every transform size used by WMAv1/v2 and WMA Pro.
        for bits in 1..=13 {
            let len = 1 << bits;
            let mut seed = 1234567u32;
            let noise: Vec<f32> = (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed as i32) as f32 / i32::MAX as f32
                })
                .collect();
            let mut impulse = vec![0.0; len];
            impulse[len / 3] = 1.0;
            for input in [noise, impulse, vec![0.0; len]] {
                for scale in [1.0, 1.0 / 32768.0] {
                    let transform = MdctNaive::new(len, scale);
                    let mut expected = vec![0.0; len];
                    transform.imdct_half_naive(&mut expected, &input);
                    let mut half = vec![0.0; len];
                    transform.imdct_half(&mut half, &input);
                    let mut full = vec![0.0; 2 * len];
                    transform.imdct_full(&mut full, &input);
                    for (i, (&actual, &reference)) in half.iter().zip(&expected).enumerate() {
                        let tolerance = (2e-6 * reference.abs()).max(2e-7 * scale as f32);
                        assert!(
                            (actual - reference).abs() <= tolerance,
                            "N={len} i={i} scale={scale}: {actual} != {reference}"
                        );
                        assert_eq!(full[len / 2 + i], actual);
                    }
                    for i in 0..len / 2 {
                        assert_eq!(full[i], -half[len / 2 - i - 1]);
                        assert_eq!(full[2 * len - i - 1], half[len / 2 + i]);
                    }
                }
            }
        }
    }
}
