//! Conservative alpha coverage preservation for explicitly selected cutout UI.
//! Inspired by DirectXTex's ScaleMipMapsAlphaForCoverage; this is an independent
//! integer implementation with a 1.5x cap and premultiplied color adjustment.

/// Keep the source's fraction of texels passing `threshold` as closely as the
/// target grid allows. One gain is shared across all animation frames. Never
/// creates pixels outside existing nonzero alpha, reduces alpha, or grows images.
/// Returns the applied gain; RGB/BGR order does not matter for premultiplied data.
pub fn preserve_alpha_coverage(
    source: &[u8],
    output: &mut [u8],
    channels: usize,
    threshold: u8,
) -> f64 {
    assert!(matches!(channels, 3 | 4));
    assert_eq!(source.len() % channels, 0);
    assert_eq!(output.len() % channels, 0);
    assert!((1..=254).contains(&threshold));
    if channels == 3 || source.is_empty() || output.is_empty() {
        return 1.0;
    }
    let source_count = source.chunks_exact(4).filter(|p| p[3] >= threshold).count();
    let wanted = ((source_count as u64 * (output.len() / 4) as u64 + (source.len() / 4) as u64 / 2)
        / (source.len() / 4) as u64) as usize;
    let mut histogram = [0usize; 256];
    for p in output.chunks_exact(4) {
        histogram[p[3] as usize] += 1;
    }
    let count = |gain: u32| -> usize {
        histogram
            .iter()
            .enumerate()
            .filter(|(alpha, _)| ((*alpha as u32 * gain + 128) / 256).min(255) >= threshold as u32)
            .map(|(_, &n)| n)
            .sum()
    };
    let mut best = 256;
    let mut error = count(best).abs_diff(wanted);
    // Discrete search accounts for the actual rounded bytes and threshold ties.
    // Equal errors retain the smaller gain, so a flat-alpha panel stays stable.
    for gain in 257..=384 {
        let candidate_error = count(gain).abs_diff(wanted);
        if candidate_error < error {
            best = gain;
            error = candidate_error;
        }
    }
    if best == 256 {
        return 1.0;
    }
    for p in output.chunks_exact_mut(4) {
        let old = p[3] as u32;
        if old == 0 {
            p[..3].fill(0);
            continue;
        }
        let alpha = ((old * best + 128) / 256).min(255);
        for color in &mut p[..3] {
            *color = ((*color as u32 * alpha + old / 2) / old).min(alpha) as u8;
        }
        p[3] = alpha as u8;
    }
    best as f64 / 256.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thin_edges_survive_without_coloring_empty_texels() {
        let source = [255, 128, 64, 255, 0, 0, 0, 0];
        let mut output = [100, 50, 25, 100, 0, 0, 0, 0];
        let gain = preserve_alpha_coverage(&source, &mut output, 4, 128);
        assert!((1.0..=1.5).contains(&gain));
        assert_eq!(output[3], 128);
        assert_eq!(&output[4..], &[0; 4]);
        assert_eq!(output[0], 128);
        assert_eq!(output[1], 64);
    }
    #[test]
    fn opaque_and_flat_translucent_images_are_unchanged() {
        for alpha in [0, 40, 128, 255] {
            let source = [alpha / 2, alpha / 3, alpha / 4, alpha].repeat(64);
            let mut output = source[..16].to_vec();
            let original = output.clone();
            assert_eq!(preserve_alpha_coverage(&source, &mut output, 4, 128), 1.0);
            assert_eq!(output, original);
        }
    }
    #[test]
    fn animation_frames_share_one_gain_and_preserve_premultiplication() {
        let source = [255, 255, 255, 255].repeat(16);
        let mut output = [80, 40, 20, 100].repeat(4);
        preserve_alpha_coverage(&source, &mut output, 4, 128);
        assert!(
            output
                .chunks_exact(4)
                .all(|p| p == &output[..4] && p[..3].iter().all(|&c| c <= p[3]))
        );
    }
}
