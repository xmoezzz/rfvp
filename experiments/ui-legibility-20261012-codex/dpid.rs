//! CPU port of mergian/dpid's matab/dpid.m (SIGGRAPH Asia 2016).
//! Copyright (c) 2016, Nicolas Weber, Sandra C. Amend / GCC / TU Darmstadt.
//! BSD-3-Clause; see DPID-LICENSE.txt. Experiment port/adaptation: 2026.
//!
//! The original handles RGB. This adapter uses premultiplied RGBA for both
//! the shared distance weight and accumulation. Opaque input is equivalent
//! to RGB DPID (the constant normalization factor cancels). Transparent
//! input is an explicit extension, NOT the authors' or VRChat's alpha code.
//! All filtering is in stored byte space, matching the existing converter.
//! No semantic labels, OCR, fonts, neural models, or platform dependencies.

fn overlap(out: usize, source: usize, input: usize, target: usize) -> f64 {
    let lo = (out * source).max(input * target);
    let hi = ((out + 1) * source).min((input + 1) * target);
    hi.saturating_sub(lo) as f64
}

/// Input and output are premultiplied RGBA8; `lambda` controls detail emphasis.
/// `mix` interpolates area -> DPID in premultiplied space. mix is an additional
/// experiment control, not a parameter from the paper / VRChat implementation.
pub fn resize(p: &[u8], sw: usize, sh: usize, dw: usize, dh: usize,
              lambda: f64, mix: f64) -> Vec<u8> {
    assert!(sw > 0 && sh > 0 && dw > 0 && dh > 0);
    assert_eq!(p.len(), sw * sh * 4);
    assert!(lambda.is_finite() && lambda >= 0.0);
    assert!(mix.is_finite() && (0.0..=1.0).contains(&mix));
    let mut average = vec![[0.0; 4]; dw * dh];
    for y in 0..dh { for x in 0..dw {
        let dest = &mut average[y * dw + x];
        for iy in y*sh/dh..((y+1)*sh).div_ceil(dh).min(sh) {
            for ix in x*sw/dw..((x+1)*sw).div_ceil(dw).min(sw) {
                let weight = overlap(y, sh, iy, dh) * overlap(x, sw, ix, dw);
                for c in 0..4 { dest[c] += p[(iy*sw+ix)*4+c] as f64 * weight; }
            }
        }
        for v in dest.iter_mut() { *v /= (sw * sh) as f64; }
    }}
    let mut out = vec![0u8; dw * dh * 4];
    for y in 0..dh { for x in 0..dw {
        let base = average[y * dw + x];
        let mut result = base;
        if lambda > 0.0 && mix > 0.0 {
            // Authors' clipped 3x3 binomial guide, renormalized at the borders.
            let mut guide = [0.0; 4]; let mut total = 0.0;
            for oy in -1isize..=1 { for ox in -1isize..=1 {
                let gy = y as isize + oy; let gx = x as isize + ox;
                if gy < 0 || gx < 0 || gy >= dh as isize || gx >= dw as isize { continue; }
                let weight = (if oy == 0 { 2.0 } else { 1.0 })
                           * (if ox == 0 { 2.0 } else { 1.0 });
                total += weight;
                for c in 0..4 { guide[c] += average[gy as usize*dw+gx as usize][c] * weight; }
            }}
            for v in &mut guide { *v /= total; }
            let mut sum = [0.0; 4]; let mut total = 0.0;
            for iy in y*sh/dh..((y+1)*sh).div_ceil(dh).min(sh) {
                for ix in x*sw/dw..((x+1)*sw).div_ceil(dw).min(sw) {
                    let source = &p[(iy*sw+ix)*4..][..4];
                    let distance = (0..4).map(|c| (guide[c] - source[c] as f64).powi(2)).sum::<f64>().sqrt() / 510.0;
                    let weight = distance.powf(lambda) * overlap(y, sh, iy, dh) * overlap(x, sw, ix, dw);
                    total += weight;
                    for c in 0..4 { sum[c] += source[c] as f64 * weight; }
                }
            }
            if total > 0.0 { for c in 0..4 { result[c] = base[c]*(1.0-mix) + sum[c]/total*mix; } }
        }
        let dest = &mut out[(y*dw+x)*4..][..4];
        for c in 0..4 { dest[c] = result[c].round().clamp(0.0, 255.0) as u8; }
        for c in 0..3 { dest[c] = dest[c].min(dest[3]); }
    }}
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn constant_and_identity() {
        let p=[20,30,40,80].repeat(77);
        for l in [0.0,0.25,0.5,1.0] { assert_eq!(resize(&p,11,7,4,3,l,1.0),[20,30,40,80].repeat(12)); }
        let p:Vec<u8>=(0..35).flat_map(|i| [i*3,i*2,i,255]).collect();
        assert_eq!(resize(&p,7,5,7,5,1.0,1.0),p);
    }
    #[test] fn fractional_area() {
        let p:Vec<u8>=(0..77).flat_map(|i| [(i*13%256) as u8,(i*7%256) as u8,(i*3%256) as u8,255]).collect();
        let a=resize(&p,11,7,4,3,0.0,1.0);
        for y in 0..3 { for x in 0..4 { for c in 0..4 {
            let mut sum=0usize; let mut n=0usize;
            for iy in 0..7 { for ix in 0..11 {
                let wt=overlap(y,7,iy,3) as usize*overlap(x,11,ix,4) as usize;
                n+=wt;sum+=wt*p[(iy*11+ix)*4+c] as usize;
            }}
            assert_eq!(a[(y*4+x)*4+c],((2*sum+n)/(2*n)) as u8);
        }}}
        assert_eq!(a,resize(&p,11,7,4,3,1.0,0.0));
    }
    #[test] fn alpha_contract_and_degenerate() {
        assert_eq!(resize(&[0;64],4,4,2,2,1.0,1.0),[0;16]);
        let p:Vec<u8>=(0..81).flat_map(|i| { let a=(i*3) as u8;[a/2,a/3,a/4,a] }).collect();
        for l in [0.25,0.5,1.0] {
            let o=resize(&p,9,9,4,3,l,1.0);
            assert!(o.chunks_exact(4).all(|c| c[..3].iter().all(|v| *v<=c[3])));
        }
    }
}

#[cfg(not(test))]
fn main() {
    let a:Vec<String>=std::env::args().collect();
    assert_eq!(a.len(),9,"dpid input output source_w source_h dest_w dest_h lambda mix");
    let p=std::fs::read(&a[1]).unwrap();
    let out=resize(&p,a[3].parse().unwrap(),a[4].parse().unwrap(),a[5].parse().unwrap(),a[6].parse().unwrap(),a[7].parse().unwrap(),a[8].parse().unwrap());
    std::fs::write(&a[2],out).unwrap();
}
