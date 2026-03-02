use std::fs;
use std::path::{Path, PathBuf};

use na_mpeg2_decoder::{
    frame_to_gray_rgba, frame_to_rgba_bt601_limited, Demuxer, Decoder, Frame, StreamType,
};

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "usage: na_mpeg2dec <input.(m2v|mpg|ts|ps)> [out_dir] [--max N] [--gray] [--strict]"
        );
        std::process::exit(2);
    }

    let input_path = &args[1];
    let mut out_dir: Option<PathBuf> = None;
    let mut max_frames: Option<usize> = None;
    let mut gray = false;
    let mut strict = false;

    // args[2] is treated as out_dir if it does not start with "--".
    let mut i = 2usize;
    if i < args.len() && !args[i].starts_with("--") {
        out_dir = Some(PathBuf::from(&args[i]));
        i += 1;
    }
    while i < args.len() {
        match args[i].as_str() {
            "--gray" => {
                gray = true;
                i += 1;
            }
            "--strict" => {
                strict = true;
                i += 1;
            }
            "--max" => {
                if i + 1 >= args.len() {
                    eprintln!("--max needs an integer argument");
                    std::process::exit(2);
                }
                max_frames = Some(args[i + 1].parse::<usize>().expect("--max N"));
                i += 2;
            }
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }

    if let Some(ref dir) = out_dir {
        fs::create_dir_all(dir).expect("create out_dir");
    }

    let data = fs::read(input_path).expect("read input");

    let mut demux = Demuxer::new_auto();
    let mut dec = Decoder::new();
    let mut frame_count: usize = 0;

    // Reuse a single RGBA buffer to reduce allocations.
    let mut rgba_scratch: Vec<u8> = Vec::new();

    for pkt in demux.push(&data, None) {
        if pkt.stream_type != StreamType::MpegVideo {
            continue;
        }

        match dec.decode_shared(&pkt.data, pkt.pts_90k) {
            Ok(frames) => {
                for f in frames {
                    frame_count += 1;
                    if let Some(ref dir) = out_dir {
                        let path = dir.join(format!("frame_{:06}.png", frame_count));
                        if let Err(e) = write_frame_png(&path, &f, gray, &mut rgba_scratch) {
                            eprintln!("png write error: {e}");
                            std::process::exit(1);
                        }
                    }
                    if let Some(max_n) = max_frames {
                        if frame_count >= max_n {
                            println!("decoded {} frame(s)", frame_count);
                            return;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("decode warn: {e}");
                if strict {
                    std::process::exit(1);
                }
            }
        }
    }

    match dec.flush_shared() {
        Ok(frames) => {
            for f in frames {
                frame_count += 1;
                if let Some(ref dir) = out_dir {
                    let path = dir.join(format!("frame_{:06}.png", frame_count));
                    if let Err(e) = write_frame_png(&path, &f, gray, &mut rgba_scratch) {
                        eprintln!("png write error: {e}");
                        std::process::exit(1);
                    }
                }
                if let Some(max_n) = max_frames {
                    if frame_count >= max_n {
                        break;
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("flush error: {e}");
            std::process::exit(1);
        }
    }

    println!("decoded {} frame(s)", frame_count);
}

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;

fn write_frame_png(path: &Path, frame: &Frame, gray: bool, rgba_scratch: &mut Vec<u8>) -> AnyResult<()> {
    let need = frame.width * frame.height * 4;
    if rgba_scratch.len() != need {
        rgba_scratch.resize(need, 0);
    }
    if gray {
        frame_to_gray_rgba(frame, rgba_scratch);
    } else {
        frame_to_rgba_bt601_limited(frame, rgba_scratch);
    }
    write_png_rgba(path, frame.width, frame.height, rgba_scratch)
}

fn write_png_rgba(path: &Path, w: usize, h: usize, rgba: &[u8]) -> AnyResult<()> {
    use std::io::BufWriter;

    let f = std::fs::File::create(path)?;
    let wtr = BufWriter::new(f);
    let mut enc = png::Encoder::new(wtr, w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header()?;
    writer.write_image_data(rgba)?;
    Ok(())
}
