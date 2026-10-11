// Compiled directly with rustc; quant_snapshot.rs is copied into output/ at run start.
#[path = "output/quant_snapshot.rs"]
mod quant;
use std::{collections::HashSet, env, fs};

fn main() {
    let args: Vec<String> = env::args().collect();
    let src = fs::read(&args[1]).unwrap();
    let width: usize = args[3].parse().unwrap();
    assert!(width > 0 && src.len() % (width * 4) == 0);
    let small = quant::quantize(&src, width, 16, false);
    let distinct: HashSet<(u16, u8)> = src.chunks_exact(4)
        .filter(|p| quant::level_of(p[3]) != 0)
        .map(|p| ((p[0] as u16 >> 3) | (p[1] as u16 >> 3) << 5 | (p[2] as u16 >> 3) << 10, quant::level_of(p[3])))
        .collect();
    let exact = small.palette.len() <= 16 && quant::alpha_error(&src, &small) <= 8 && distinct.len() < 16;
    let dither = args[4] == "dither";
    let forced = quant::quantize(&src, width, 16, dither);
    let (q, colors) = if exact { (small, 16usize) }
        else if quant::alpha_error(&src, &forced) <= 64 { (forced, 16) }
        else { (quant::quantize(&src, width, 256, dither), 256) };
    let error = quant::alpha_error(&src, &q);
    // Probe payload: u16 count, u8 maximum alpha error, padded palette, padded alpha, indices.
    let mut bytes = (colors as u16).to_le_bytes().to_vec();
    bytes.push(error);
    for i in 0..colors { bytes.extend_from_slice(&q.palette.get(i).copied().unwrap_or(0).to_le_bytes()); }
    for i in 0..colors { bytes.push(q.alpha.get(i).copied().unwrap_or(0)); }
    bytes.extend(q.indices);
    fs::write(&args[2], bytes).unwrap();
}
