//! The PlayStation's two 4-bit ADPCM formats.
//!
//! Both code 28 samples at a time with one of a few predictors (`s = d + (k0 * s1 + k1 * s2) /
//! 64`) and a shift for the residuals. The SPU's format (sounds in sound RAM) has five
//! predictors and 16-byte blocks; XA (CD streams) has four, in 128-byte sound groups of eight
//! units. The encoder tries every predictor and shift for each unit and keeps the one that
//! decodes closest, carrying the decoder's own state forward.

const FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];

/// Predictor state: the last two decoded samples.
#[derive(Clone, Copy, Default)]
pub struct State {
    s1: i32,
    s2: i32,
}

/// Code 28 samples with predictors `0..filters`; returns (filter, shift, nibbles).
fn encode_unit(samples: &[i16; 28], state: &mut State, filters: usize) -> (u8, u8, [u8; 28]) {
    let mut best: Option<(i64, u8, u8, [u8; 28], State)> = None;
    for f in 0..filters {
        let (k0, k1) = FILTERS[f];
        // The largest residual decides the smallest shift worth trying.
        let mut peak = 0i32;
        let mut probe = *state;
        for &s in samples {
            let predicted = (k0 * probe.s1 + k1 * probe.s2 + 32) >> 6;
            peak = peak.max((s as i32 - predicted).abs());
            probe = State { s1: s as i32, s2: probe.s1 };
        }
        // Residuals are coded as q * 2^e (q in -8..=7); the format stores 12 - e. Start at the
        // smallest e that fits the largest residual and try its neighbours.
        let mut fit = 0i32;
        while fit < 12 && (peak >> fit) > 7 {
            fit += 1;
        }
        for e in (fit - 1).max(0)..=(fit + 1).min(12) {
            let mut st = *state;
            let mut err = 0i64;
            let mut nibbles = [0u8; 28];
            let step = 1i32 << e;
            for (i, &s) in samples.iter().enumerate() {
                let predicted = (k0 * st.s1 + k1 * st.s2 + 32) >> 6;
                let residual = s as i32 - predicted;
                let q = ((residual + step / 2).div_euclid(step)).clamp(-8, 7);
                nibbles[i] = (q & 15) as u8;
                let decoded = ((q << 12) >> (12 - e)) + predicted;
                let decoded = decoded.clamp(-32768, 32767);
                err += ((s as i32 - decoded) as i64).pow(2);
                st = State { s1: decoded, s2: st.s1 };
            }
            if best.as_ref().is_none_or(|b| err < b.0) {
                best = Some((err, f as u8, (12 - e) as u8, nibbles, st));
            }
        }
    }
    let (_, filter, range, nibbles, st) = best.unwrap();
    *state = st;
    (filter, range, nibbles)
}

/// Samples as 28-sample units, the last padded with silence.
fn units(samples: &[i16]) -> impl Iterator<Item = [i16; 28]> + '_ {
    samples.chunks(28).map(|chunk| {
        let mut unit = [0i16; 28];
        unit[..chunk.len()].copy_from_slice(chunk);
        unit
    })
}

/// SPU-ADPCM of mono samples, played once: the last block's flags end the voice.
pub fn encode_spu(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len().div_ceil(28) * 16 + 16);
    let mut state = State::default();
    for unit in units(samples) {
        let (filter, range, nibbles) = encode_unit(&unit, &mut state, 5);
        out.push((filter << 4) | range);
        out.push(0);
        for pair in nibbles.chunks_exact(2) {
            out.push(pair[0] | (pair[1] << 4));
        }
    }
    if out.is_empty() {
        out.resize(16, 0);
    }
    // End flag (bit 0) without repeat (bit 1): the voice stops here. The host sets the loop
    // flags itself when a sound repeats.
    let last = out.len() - 16;
    out[last + 1] = 0x01;
    out
}

/// One XA sector's sound data: 18 sound groups of 128 bytes.
pub const XA_GROUPS: usize = 18;
pub const XA_DATA: usize = XA_GROUPS * 128;

/// Frames (per channel) one XA sector holds.
pub fn xa_frames_per_sector(stereo: bool) -> usize {
    XA_GROUPS * 8 * 28 / if stereo { 2 } else { 1 }
}

/// XA-ADPCM (4-bit) of interleaved 16-bit samples: one `XA_DATA`-byte block per sector.
pub fn encode_xa(samples: &[i16], stereo: bool) -> Vec<[u8; XA_DATA]> {
    let per_sector = xa_frames_per_sector(stereo) * if stereo { 2 } else { 1 };
    let mut states = [State::default(); 2];
    let mut sectors = Vec::new();
    for chunk in samples.chunks(per_sector) {
        let mut sector = [0u8; XA_DATA];
        let mut padded = chunk.to_vec();
        padded.resize(per_sector, 0);
        for g in 0..XA_GROUPS {
            let group = &mut sector[g * 128..(g + 1) * 128];
            // Eight units of 28 samples: mono units follow each other; stereo ones alternate
            // left and right.
            let group_samples = &padded[g * 224..(g + 1) * 224];
            for u in 0..8 {
                let mut unit = [0i16; 28];
                let channel = if stereo { u & 1 } else { 0 };
                for (i, s) in unit.iter_mut().enumerate() {
                    *s = if stereo {
                        group_samples[((u / 2) * 28 + i) * 2 + channel]
                    } else {
                        group_samples[u * 28 + i]
                    };
                }
                let (filter, range, nibbles) = encode_unit(&unit, &mut states[channel], 4);
                let param = (filter << 4) | range;
                // Parameters: units 0-3 at bytes 0-3 and 4-7, units 4-7 at 8-11 and 12-15.
                let base = if u < 4 { u } else { 8 + (u - 4) };
                group[base] = param;
                group[base + 4] = param;
                // Data: 28 words; byte u/2 of word i holds unit u's sample i, odd units high.
                for (i, &n) in nibbles.iter().enumerate() {
                    let byte = &mut group[16 + i * 4 + u / 2];
                    *byte |= if u & 1 == 0 { n } else { n << 4 };
                }
            }
        }
        sectors.push(sector);
    }
    sectors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_spu(data: &[u8]) -> Vec<i16> {
        let mut out = Vec::new();
        let mut st = State::default();
        for block in data.chunks_exact(16) {
            let (k0, k1) = FILTERS[(block[0] >> 4) as usize];
            let shift = (block[0] & 15) as i32;
            for i in 0..28 {
                let n = (block[2 + i / 2] >> ((i & 1) * 4)) & 15;
                let d = ((n as i32) << 28 >> 16) >> shift;
                let s = (d + ((k0 * st.s1 + k1 * st.s2 + 32) >> 6)).clamp(-32768, 32767);
                out.push(s as i16);
                st = State { s1: s, s2: st.s1 };
            }
        }
        out
    }

    #[test]
    fn spu_round_trip_is_close() {
        let samples: Vec<i16> = (0..2800).map(|i| ((i as f32 * 0.05).sin() * 12000.0) as i16).collect();
        let data = encode_spu(&samples);
        assert_eq!(data.len(), 100 * 16);
        assert_eq!(data[data.len() - 15], 0x01);
        let back = decode_spu(&data);
        let err: f64 = samples.iter().zip(&back).map(|(a, b)| ((*a as f64) - (*b as f64)).powi(2)).sum::<f64>()
            / samples.len() as f64;
        assert!(err.sqrt() < 300.0, "rms error {}", err.sqrt());
    }

    #[test]
    fn xa_sector_layout() {
        let samples = vec![1000i16; xa_frames_per_sector(true) * 2];
        let sectors = encode_xa(&samples, true);
        assert_eq!(sectors.len(), 1);
        let g = &sectors[0][..128];
        assert_eq!(&g[0..4], &g[4..8]);
        assert_eq!(&g[8..12], &g[12..16]);
    }
}
