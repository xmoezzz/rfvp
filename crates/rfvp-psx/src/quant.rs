//! Color quantization to a PlayStation CLUT, with the alpha kept.
//!
//! The GPU's texels are 15-bit colors with one more bit, STP. Color 0x0000 is not drawn (fully
//! transparent). With semi-transparency on for the primitive, a texel with STP set is blended
//! with what is behind it (half and half, in the mode the host uses), and one without is drawn
//! as it is. So the GPU sees three kinds of pixel: transparent, half, opaque.
//!
//! The engine also reads alpha itself (hit tests on buttons, parts drawn onto pictures, mask
//! prims), and needs more than three steps. So pixels are quantized by color and by alpha
//! level: each palette entry stands for one level and keeps its own 8-bit alpha (the mean of its
//! pixels) in a table beside the palette. The GPU gets the level's kind; the engine the alpha.

/// What the GPU makes of a pixel.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Class {
    Transparent,
    Half,
    Opaque,
}

/// Alpha levels: 0 is fully transparent (palette index 0); then faint (drawn as nothing),
/// two half levels, nearly opaque and opaque.
pub fn level_of(alpha: u8) -> u8 {
    match alpha {
        0..=7 => 0,
        8..=63 => 1,
        64..=127 => 2,
        128..=191 => 3,
        192..=247 => 4,
        _ => 5,
    }
}

pub fn class_of_level(level: u8) -> Class {
    match level {
        0 | 1 => Class::Transparent,
        2 | 3 => Class::Half,
        _ => Class::Opaque,
    }
}

pub fn class_of(alpha: u8) -> Class {
    class_of_level(level_of(alpha))
}

/// A 15-bit color with its STP bit: bit 15 STP, then blue, green, red in 5 bits each.
pub fn psx_color(r: u8, g: u8, b: u8, class: Class) -> u16 {
    let c = (r as u16 >> 3) | ((g as u16 >> 3) << 5) | ((b as u16 >> 3) << 10);
    match class {
        Class::Transparent => 0,
        Class::Half => c | 0x8000,
        // 0x0000 is transparent: opaque black becomes the darkest gray that is not.
        Class::Opaque if c == 0 => 0x0421,
        Class::Opaque => c,
    }
}

/// The result: a palette (index 0 transparent), each entry's alpha, an index per pixel.
pub struct Quantized {
    pub palette: Vec<u16>,
    pub alpha: Vec<u8>,
    pub indices: Vec<u8>,
}

/// One palette entry being built: color, alpha level, mean alpha.
#[derive(Clone, Copy)]
struct Entry {
    rgb: [u8; 3],
    level: u8,
    alpha: u8,
}

/// Quantize RGBA pixels to at most `max_colors` entries (index 0 included, kept for
/// transparency). `dither` diffuses the color error of opaque pixels (Floyd-Steinberg) across
/// `width` pixel rows.
pub fn quantize(rgba: &[u8], width: usize, max_colors: usize, dither: bool) -> Quantized {
    quantize_impl(rgba, width, max_colors, dither, true)
}

/// Reproduce the former allocation only to establish the existing 4-bit budget.
/// This palette is never written to the new image output.
pub fn legacy_clut4_budget(rgba: &[u8], width: usize) -> Quantized {
    quantize_impl(rgba, width, 16, true, false)
}

fn quantize_impl(rgba: &[u8], width: usize, max_colors: usize, dither: bool, reserve_alpha: bool) -> Quantized {
    let count = rgba.len() / 4;
    // Histogram of 15-bit colors per alpha level, with the alpha summed.
    let mut histogram = std::collections::HashMap::<(u16, u8), (u32, u64)>::new();
    for p in rgba.chunks_exact(4) {
        let level = level_of(p[3]);
        if level == 0 {
            continue;
        }
        let e = histogram.entry((r5g5b5(p[0], p[1], p[2]), level)).or_default();
        e.0 += 1;
        e.1 += p[3] as u64;
    }

    let exact = histogram.len() < max_colors;
    let mut entries: Vec<Entry> = if exact {
        let mut keys: Vec<_> = histogram.iter().map(|(&k, &v)| (k, v)).collect();
        keys.sort_by_key(|(k, _)| *k);
        keys.into_iter()
            .map(|((c, level), (n, sum))| Entry { rgb: expand(c), level, alpha: (sum / n as u64) as u8 })
            .collect()
    } else {
        if reserve_alpha { median_cut(&histogram, max_colors - 1) }
        else { median_cut_legacy(&histogram, max_colors - 1) }
    };
    entries.truncate(max_colors - 1);
    let mut palette = Vec::with_capacity(entries.len() + 1);
    let mut alpha = Vec::with_capacity(entries.len() + 1);
    palette.push(0);
    alpha.push(0);
    for e in &entries {
        palette.push(psx_color(e.rgb[0], e.rgb[1], e.rgb[2], class_of_level(e.level)));
        alpha.push(e.alpha);
    }

    let mut indices = vec![0u8; count];
    let mut cache = std::collections::HashMap::<(u16, u8), u8>::new();
    let mut nearest = |r: i32, g: i32, b: i32, level: u8| -> u8 {
        let key = (r5g5b5(clamp8(r), clamp8(g), clamp8(b)), level);
        *cache.entry(key).or_insert_with(|| {
            let pick = |same_level: bool| {
                let mut best = (u32::MAX, 0u8);
                for (i, e) in entries.iter().enumerate() {
                    if same_level && e.level != level {
                        continue;
                    }
                    let d = sq(e.rgb[0] as i32 - r) + sq(e.rgb[1] as i32 - g) + sq(e.rgb[2] as i32 - b)
                        + if same_level { 0 } else { sq((e.level as i32 - level as i32) * 48) };
                    if d < best.0 {
                        best = (d, (i + 1) as u8);
                    }
                }
                best
            };
            let best = pick(true);
            // No entry at this level (it was outvoted): the closest in color and alpha.
            if best.0 == u32::MAX { pick(false).1 } else { best.1 }
        })
    };

    let dither = dither && !exact;
    let height = if width == 0 { 0 } else { count / width };
    let mut next_row = vec![[0i32; 3]; width + 2];
    let mut this_row = vec![[0i32; 3]; width + 2];
    for y in 0..height {
        std::mem::swap(&mut this_row, &mut next_row);
        next_row.iter_mut().for_each(|e| *e = [0; 3]);
        for x in 0..width {
            let i = y * width + x;
            let p = &rgba[i * 4..i * 4 + 4];
            let level = level_of(p[3]);
            if level == 0 {
                continue;
            }
            let diffuse = dither && level >= 4;
            let carried = if diffuse { this_row[x + 1] } else { [0; 3] };
            let want = [p[0] as i32 + carried[0] / 16, p[1] as i32 + carried[1] / 16, p[2] as i32 + carried[2] / 16];
            let index = nearest(want[0], want[1], want[2], level);
            indices[i] = index;
            if diffuse && index > 0 {
                let got = entries[index as usize - 1].rgb;
                for c in 0..3 {
                    let err = want[c] - got[c] as i32;
                    this_row[x + 2][c] += err * 7;
                    next_row[x][c] += err * 3;
                    next_row[x + 1][c] += err * 5;
                    next_row[x + 2][c] += err;
                }
            }
        }
    }
    Quantized { palette, alpha, indices }
}

/// Median cut with a guaranteed slot for each alpha level, then weighted color slots.
fn median_cut(histogram: &std::collections::HashMap<(u16, u8), (u32, u64)>, colors: usize) -> Vec<Entry> {
    let mut groups = Vec::new();
    for level in 1..=5 {
        let mut points: Vec<([u8; 3], u32, u64)> = histogram.iter()
            .filter(|((_, l), _)| *l == level)
            .map(|(&(color, _), &(n, sum))| (expand(color), n, sum))
            .collect();
        if points.is_empty() { continue; }
        // Hash iteration must not change palette ties or error diffusion.
        points.sort_unstable_by_key(|p| p.0);
        let weight: u64 = points.iter().map(|p| p.1 as u64).sum();
        let alpha_sum: u64 = points.iter().map(|p| p.2).sum();
        groups.push((level, points, weight, (alpha_sum / weight) as u8));
    }
    // Reserve one entry for every alpha level before distributing color slots.
    // A rare antialiased edge must not become opaque or disappear because a
    // large opaque fill won the proportional allocation first.
    assert!(colors >= groups.len());
    let mut slots = vec![1usize; groups.len()];
    for _ in groups.len()..colors {
        let pick = (0..groups.len()).filter(|&i| slots[i] < groups[i].1.len())
            .max_by(|&a, &b| {
                (groups[a].2 * (slots[b] + 1) as u64)
                    .cmp(&(groups[b].2 * (slots[a] + 1) as u64))
                    .then_with(|| groups[a].0.cmp(&groups[b].0))
            });
        let Some(i) = pick else { break };
        slots[i] += 1;
    }
    let mut out = Vec::new();
    for ((level, points, _, alpha), count) in groups.into_iter().zip(slots) {
        for rgb in cut(points.iter().map(|p| (p.0, p.1)).collect(), count) {
            out.push(Entry { rgb, level, alpha });
        }
    }
    out
}

// Compatibility budget: proportional allocation used before alpha reservation.
fn median_cut_legacy(histogram: &std::collections::HashMap<(u16, u8), (u32, u64)>, colors: usize) -> Vec<Entry> {
    let mut out = Vec::new();
    let total: u64 = histogram.values().map(|v| v.0 as u64).sum();
    let mut levels: Vec<u8> = histogram.keys().map(|k| k.1).collect();
    levels.sort_unstable_by(|a, b| b.cmp(a));
    levels.dedup();
    for level in levels {
        let points: Vec<([u8; 3], u32, u64)> = histogram
            .iter()
            .filter(|((_, l), _)| *l == level)
            .map(|(&(color, _), &(n, sum))| (expand(color), n, sum))
            .collect();
        let weight: u64 = points.iter().map(|p| p.1 as u64).sum();
        let left = colors.saturating_sub(out.len());
        if left == 0 {
            break;
        }
        let share = ((colors as u64 * weight).div_ceil(total.max(1)) as usize).clamp(1, left);
        let alpha_sum: u64 = points.iter().map(|p| p.2).sum();
        let alpha = (alpha_sum / weight.max(1)) as u8;
        for rgb in cut(points.iter().map(|p| (p.0, p.1)).collect(), share) {
            out.push(Entry { rgb, level, alpha });
        }
    }
    out.truncate(colors);
    out
}

/// Split the points into `n` boxes, each time the box with the most weight times range, along
/// its longest channel at the weighted median; each box's weighted mean is a color.
fn cut(points: Vec<([u8; 3], u32)>, n: usize) -> Vec<[u8; 3]> {
    let mut boxes = vec![points];
    while boxes.len() < n {
        let pick = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .max_by_key(|(_, b)| {
                let (_, range) = longest_channel(b);
                let weight: u64 = b.iter().map(|p| p.1 as u64).sum();
                weight * range as u64
            })
            .map(|(i, _)| i);
        let Some(i) = pick else { break };
        let mut b = boxes.swap_remove(i);
        let (channel, _) = longest_channel(&b);
        b.sort_by_key(|p| (p.0[channel], p.0));
        let half: u64 = b.iter().map(|p| p.1 as u64).sum::<u64>() / 2;
        let mut acc = 0u64;
        let mut split = 1;
        for (k, p) in b.iter().enumerate() {
            acc += p.1 as u64;
            if acc >= half {
                split = (k + 1).clamp(1, b.len() - 1);
                break;
            }
        }
        let rest = b.split_off(split);
        boxes.push(b);
        boxes.push(rest);
    }
    boxes
        .iter()
        .map(|b| {
            let weight: u64 = b.iter().map(|p| p.1 as u64).sum::<u64>().max(1);
            let mut sum = [0u64; 3];
            for p in b {
                for c in 0..3 {
                    sum[c] += p.0[c] as u64 * p.1 as u64;
                }
            }
            [(sum[0] / weight) as u8, (sum[1] / weight) as u8, (sum[2] / weight) as u8]
        })
        .collect()
}

fn longest_channel(points: &[([u8; 3], u32)]) -> (usize, u8) {
    let mut best = (0, 0);
    for c in 0..3 {
        let lo = points.iter().map(|p| p.0[c]).min().unwrap_or(0);
        let hi = points.iter().map(|p| p.0[c]).max().unwrap_or(0);
        if hi - lo > best.1 {
            best = (c, hi - lo);
        }
    }
    best
}

fn r5g5b5(r: u8, g: u8, b: u8) -> u16 {
    (r as u16 >> 3) | ((g as u16 >> 3) << 5) | ((b as u16 >> 3) << 10)
}

/// A 15-bit color back to 8 bits per channel (the middle of each step).
fn expand(c: u16) -> [u8; 3] {
    let e = |v: u16| ((v & 31) << 3 | (v & 31) >> 2) as u8;
    [e(c), e(c >> 5), e(c >> 10)]
}

fn clamp8(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

fn sq(v: i32) -> u32 {
    (v * v) as u32
}

/// The largest alpha error (0-255) the quantized picture makes against the original, for
/// deciding whether 16 colors keep a picture's mask.
pub fn alpha_error(rgba: &[u8], q: &Quantized) -> u8 {
    rgba.chunks_exact(4)
        .zip(&q.indices)
        .map(|(p, &i)| (p[3] as i32 - q.alpha[i as usize] as i32).unsigned_abs() as u8)
        .max()
        .unwrap_or(0)
}

/// Alpha-weighted RGB mean square error in the actual RGB555 palette. Hidden
/// pixels contribute nothing; antialiased edges contribute less than solid ink.
pub fn color_mse(rgba: &[u8], q: &Quantized) -> u64 {
    let mut error = 0u64;
    let mut weight = 0u64;
    for (p, &index) in rgba.chunks_exact(4).zip(&q.indices) {
        if class_of(p[3]) == Class::Transparent { continue; }
        let w = p[3] as u64 * p[3] as u64;
        let rgb = expand(q.palette[index as usize] & 0x7fff);
        error += (0..3).map(|c| sq(p[c] as i32 - rgb[c] as i32) as u64).sum::<u64>() * w;
        weight += w;
    }
    error.div_ceil((weight * 3).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rare_alpha_levels_keep_their_classes_in_a_busy_palette() {
        let mut rgba = Vec::new();
        for i in 0..2048u32 {
            rgba.extend_from_slice(&[(i * 7) as u8, (i * 13) as u8, (i * 29) as u8, 255]);
        }
        for alpha in [30, 100, 160, 220] { rgba.extend_from_slice(&[180, 200, 240, alpha]); }
        let q = quantize(&rgba, 2052, 16, true);
        for (p, &index) in rgba.chunks_exact(4).zip(&q.indices) {
            assert_eq!(level_of(q.alpha[index as usize]), level_of(p[3]));
            assert_eq!(class_of(q.alpha[index as usize]), class_of(p[3]));
        }
        assert!(q.palette.len() <= 16);
        assert!(alpha_error(&rgba, &q) <= 63);
        let repeat = quantize(&rgba, 2052, 16, true);
        assert_eq!(q.palette, repeat.palette);
        assert_eq!(q.alpha, repeat.alpha);
        assert_eq!(q.indices, repeat.indices);
    }

    #[test]
    fn few_colors_are_kept_exactly() {
        let rgba = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0, 255, 0, 0, 128];
        let q = quantize(&rgba, 4, 16, true);
        assert_eq!(q.palette[0], 0);
        assert_eq!(q.indices[2], 0, "transparent pixel uses index 0");
        assert_eq!(q.palette[q.indices[0] as usize], 0x001f);
        assert_eq!(q.palette[q.indices[1] as usize], 0x03e0);
        assert_eq!(q.palette[q.indices[3] as usize], 0x801f, "half alpha sets STP");
        assert_eq!(q.alpha[q.indices[3] as usize], 128, "the engine keeps the real alpha");
    }

    #[test]
    fn faint_pixels_keep_their_alpha_but_are_not_drawn() {
        let rgba = [200, 200, 200, 30, 200, 200, 200, 255];
        let q = quantize(&rgba, 2, 16, false);
        assert_eq!(q.palette[q.indices[0] as usize], 0);
        assert_eq!(q.alpha[q.indices[0] as usize], 30);
        assert_ne!(q.indices[0], 0);
    }

    #[test]
    fn many_colors_fit_the_palette() {
        let mut rgba = Vec::new();
        for i in 0..4096u32 {
            rgba.extend_from_slice(&[(i * 7) as u8, (i * 13) as u8, (i * 29) as u8, 255]);
        }
        let q = quantize(&rgba, 64, 256, true);
        assert!(q.palette.len() <= 256);
        assert!(q.indices.iter().all(|&i| i != 0 && (i as usize) < q.palette.len()));
    }

    #[test]
    fn opaque_black_is_not_transparent() {
        assert_ne!(psx_color(0, 0, 0, Class::Opaque), 0);
    }
}
