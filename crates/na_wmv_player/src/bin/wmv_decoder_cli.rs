//! Simple CLI wrapper for the library.
//!
//! Usage:
//!   wmv-decoder <input.wmv> [output_dir] [--yuv] [--png]

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use wmv_decoder::{AsfWmv2Decoder, DecoderError, Result, YuvFrame};

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    let mut input_path: Option<String> = None;
    let mut output_dir: Option<PathBuf> = None;
    let mut dump_yuv = false;
    let mut dump_png = false;

    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--yuv" => dump_yuv = true,
            "--png" => dump_png = true,
            _ => {
                if input_path.is_none() {
                    input_path = Some(arg.clone());
                } else if output_dir.is_none() {
                    output_dir = Some(PathBuf::from(arg));
                } else {
                    eprintln!("Unexpected argument: {arg}");
                    eprintln!("Usage: {} <input.wmv> [output_dir] [--yuv] [--png]", args[0]);
                    std::process::exit(1);
                }
            }
        }
    }

    let Some(input_path) = input_path else {
        eprintln!("Usage: {} <input.wmv> [output_dir] [--yuv] [--png]", args[0]);
        std::process::exit(1);
    };

    if output_dir.is_some() && !dump_yuv && !dump_png {
        dump_yuv = true;
    }

    if let Err(e) = run(&input_path, output_dir.as_deref(), dump_yuv, dump_png) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run(input_path: &str, output_dir: Option<&Path>, dump_yuv: bool, dump_png: bool) -> Result<()> {
    let file = File::open(input_path)?;
    let reader = BufReader::new(file);

    let mut dec = AsfWmv2Decoder::open(reader)?;

    if let Some(dir) = output_dir {
        std::fs::create_dir_all(dir)?;
    }

    let mut idx: u64 = 0;
    while let Some(df) = dec.next_frame()? {
        idx += 1;

        if let Some(dir) = output_dir {
            if dump_yuv {
                let fname = dir.join(format!("frame_{:06}.yuv", idx));
                write_yuv_frame(&fname, &df.frame)?;
            }
            if dump_png {
                let fname = dir.join(format!("frame_{:06}.png", idx));
                write_png_frame(&fname, &df.frame)?;
            }
        }
    }

    Ok(())
}

fn write_yuv_frame(path: &Path, frame: &YuvFrame) -> Result<()> {
    let file = File::create(path)?;
    let mut w = BufWriter::new(file);
    w.write_all(&frame.y)?;
    w.write_all(&frame.cb)?;
    w.write_all(&frame.cr)?;
    w.flush()?;
    Ok(())
}

fn write_png_frame(path: &Path, frame: &YuvFrame) -> Result<()> {
    // Convert YUV420p (BT.601-ish) -> RGB for debug output.
    let w = frame.width as usize;
    let h = frame.height as usize;

    let mut rgb = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let yy = frame.y[y * w + x] as i32;
            let uv_idx = (y / 2) * (w / 2) + (x / 2);
            let cb = frame.cb[uv_idx] as i32;
            let cr = frame.cr[uv_idx] as i32;

            // Simple integer conversion.
            let c = yy - 16;
            let d = cb - 128;
            let e = cr - 128;

            let r = (298 * c + 409 * e + 128) >> 8;
            let g = (298 * c - 100 * d - 208 * e + 128) >> 8;
            let b = (298 * c + 516 * d + 128) >> 8;

            let r = r.clamp(0, 255) as u8;
            let g = g.clamp(0, 255) as u8;
            let b = b.clamp(0, 255) as u8;

            let o = (y * w + x) * 3;
            rgb[o] = r;
            rgb[o + 1] = g;
            rgb[o + 2] = b;
        }
    }

    let file = File::create(path)?;
    let mut enc = png::Encoder::new(file, frame.width, frame.height);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| DecoderError::InvalidData(format!("PNG header error: {e}")))?;
    writer
        .write_image_data(&rgb)
        .map_err(|e| DecoderError::InvalidData(format!("PNG write error: {e}")))?;
    Ok(())
}
