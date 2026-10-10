//! Desktop-only WMV3 diagnostic frame dumper.
//!
//! This binary deliberately lives outside the Siglus playback path. It decodes
//! ASF/WMV3 through the same native decoder, records picture-header metadata,
//! and can either dump a selected decode-order frame with context or compare
//! presentation-order native frames against FFmpeg and dump the first large
//! mismatch.
//!
//! Usage:
//!   wmv3-frame-dump <input.wmv> <output_dir> [--frame N] [--context N]
//!                   [--threshold N]
//!
//! Modes:
//!   --frame N       Dump native decode-order frame N (zero based) and +/- context.
//!                   FFmpeg is not required in this mode.
//!   no --frame      Reorder native frames by ASF PTS, compare them against local
//!                   FFmpeg YUV420p output, and dump the first frame whose maximum
//!                   byte difference reaches --threshold, plus +/- context.
//!
//! The default threshold is 16 so tiny rounding/filter differences do not hide
//! the first structural decoding failure. Use --threshold 1 for byte-level
//! comparison.

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!("wmv3-frame-dump is a desktop-only diagnostic binary");

use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};

use wmv_decoder::asf::{AsfFile, VideoStreamInfo};
use wmv_decoder::vc1::{PictureHeader, SequenceHeader};
use wmv_decoder::{DecoderError, Result, Wmv3Decoder, YuvFrame};

const DEFAULT_CONTEXT: usize = 2;
const DEFAULT_THRESHOLD: u8 = 16;
const FNV64_OFFSET: u64 = 0xcbf29ce484222325;
const FNV64_PRIME: u64 = 0x100000001b3;

#[derive(Debug)]
struct Options {
    input: PathBuf,
    output_dir: PathBuf,
    frame: Option<u64>,
    context: usize,
    threshold: u8,
}

#[derive(Clone)]
struct NativeFrame {
    decode_index: u64,
    pts_ms: u32,
    is_key: bool,
    header: PictureHeader,
    payload: Vec<u8>,
    frame: YuvFrame,
}

#[derive(Clone, Copy, Debug, Default)]
struct PlaneDiff {
    mismatched: u64,
    max_abs: u8,
    sum_abs: u128,
}

#[derive(Clone, Debug, Default)]
struct FrameDiff {
    y: PlaneDiff,
    cb: PlaneDiff,
    cr: PlaneDiff,
    first_mismatch: Option<(&'static str, usize, usize, u8, u8)>,
}

impl FrameDiff {
    fn mismatched(&self) -> u64 {
        self.y.mismatched + self.cb.mismatched + self.cr.mismatched
    }

    fn max_abs(&self) -> u8 {
        self.y.max_abs.max(self.cb.max_abs).max(self.cr.max_abs)
    }

    fn sum_abs(&self) -> u128 {
        self.y.sum_abs + self.cb.sum_abs + self.cr.sum_abs
    }
}

struct ComparedFrame {
    presentation_index: u64,
    native: NativeFrame,
    ffmpeg: Vec<u8>,
    diff: FrameDiff,
}

struct FfmpegPipe {
    child: Child,
    stdout: ChildStdout,
}

fn usage(program: &str) {
    eprintln!(
        "Usage: {program} <input.wmv> <output_dir> [--frame N] [--context N] [--threshold N]\n\
         \n\
         Without --frame: compare presentation-order native WMV3 against FFmpeg and\n\
         dump the first structural mismatch.\n\
         With --frame: dump that zero-based native decode-order frame and context."
    );
}

fn parse_args() -> Options {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        usage(&args[0]);
        std::process::exit(2);
    }

    let input = PathBuf::from(&args[1]);
    let output_dir = PathBuf::from(&args[2]);
    let mut frame = None;
    let mut context = DEFAULT_CONTEXT;
    let mut threshold = DEFAULT_THRESHOLD;

    let mut i = 3usize;
    while i < args.len() {
        match args[i].as_str() {
            "--frame" => {
                i += 1;
                frame = Some(parse_u64_arg(&args, i, "--frame"));
            }
            "--context" => {
                i += 1;
                context = parse_u64_arg(&args, i, "--context") as usize;
            }
            "--threshold" => {
                i += 1;
                let v = parse_u64_arg(&args, i, "--threshold");
                if v > 255 {
                    eprintln!("--threshold must be in 0..=255");
                    std::process::exit(2);
                }
                threshold = v as u8;
            }
            "--help" | "-h" => {
                usage(&args[0]);
                std::process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {other}");
                usage(&args[0]);
                std::process::exit(2);
            }
        }
        i += 1;
    }

    Options {
        input,
        output_dir,
        frame,
        context,
        threshold,
    }
}

fn parse_u64_arg(args: &[String], i: usize, flag: &str) -> u64 {
    let Some(value) = args.get(i) else {
        eprintln!("Missing value for {flag}");
        std::process::exit(2);
    };
    match value.parse::<u64>() {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Invalid integer for {flag}: {value}");
            std::process::exit(2);
        }
    }
}

fn main() {
    env_logger::init();
    let opts = parse_args();
    if let Err(err) = run(&opts) {
        eprintln!("wmv3-frame-dump: {err}");
        std::process::exit(1);
    }
}

fn run(opts: &Options) -> Result<()> {
    fs::create_dir_all(&opts.output_dir)?;

    if let Some(target) = opts.frame {
        run_manual_dump(opts, target)
    } else {
        run_first_mismatch(opts)
    }
}

fn open_native(
    input: &Path,
) -> Result<(
    BufReader<File>,
    AsfFile,
    VideoStreamInfo,
    SequenceHeader,
    Wmv3Decoder,
)> {
    let file = File::open(input)?;
    let mut reader = BufReader::new(file);
    let asf = AsfFile::open(&mut reader)?;

    let info = asf
        .video_streams
        .iter()
        .find(|v| v.codec_four_cc.as_slice().eq_ignore_ascii_case(b"WMV3"))
        .cloned()
        .ok_or_else(|| DecoderError::Unsupported("no WMV3 video stream in ASF".into()))?;

    let mut seq = SequenceHeader::parse(&info.extra_data)?;
    seq.width = info.width;
    seq.height = info.height;
    seq.display_width = info.width;
    seq.display_height = info.height;

    let decoder = Wmv3Decoder::new(info.width, info.height, &info.extra_data)?;
    reader.seek(SeekFrom::Start(asf.data_offset))?;

    Ok((reader, asf, info, seq, decoder))
}

fn run_manual_dump(opts: &Options, target: u64) -> Result<()> {
    let (mut reader, mut asf, info, seq, mut decoder) = open_native(&opts.input)?;
    let mut manifest = create_manifest(&opts.output_dir)?;
    let start = target.saturating_sub(opts.context as u64);
    let end = target.saturating_add(opts.context as u64);
    let mut decode_index = 0u64;
    let mut dumped = 0usize;

    println!(
        "WMV3 {}x{}, MAXBFRAMES={}, manual decode-order dump {}..={} (target={})",
        info.width, info.height, seq.max_b_frames, start, end, target
    );

    'packets: loop {
        let payloads = match asf.read_packet(&mut reader) {
            Ok(v) => v,
            Err(DecoderError::EndOfStream) => break,
            Err(err) => return Err(err),
        };

        for payload in payloads {
            if payload.stream_number != info.stream_number {
                continue;
            }

            let native = decode_native_frame(
                &mut decoder,
                &seq,
                decode_index,
                payload.pts_ms,
                payload.is_key_frame,
                payload.data,
                info.width,
                info.height,
            )?;
            let Some(native) = native else {
                continue;
            };

            write_manifest_row(&mut manifest, &native)?;

            if decode_index >= start && decode_index <= end {
                dump_native(&opts.output_dir, &native, "selected")?;
                dumped += 1;
            }

            if decode_index >= end {
                break 'packets;
            }
            decode_index += 1;
        }
    }

    manifest.flush()?;
    println!(
        "dumped {dumped} native frame(s) to {}",
        opts.output_dir.display()
    );
    if dumped == 0 {
        return Err(DecoderError::InvalidData(format!(
            "decode-order frame {target} was not reached"
        )));
    }
    Ok(())
}

fn run_first_mismatch(opts: &Options) -> Result<()> {
    let (mut reader, mut asf, info, seq, mut decoder) = open_native(&opts.input)?;
    let mut manifest = create_manifest(&opts.output_dir)?;
    let mut compare_csv = BufWriter::new(File::create(opts.output_dir.join("comparison.csv"))?);
    writeln!(
        compare_csv,
        "presentation_index,decode_index,pts_ms,frame_type,mismatch_y,mismatch_cb,mismatch_cr,max_abs,mean_abs,first_plane,first_x,first_y"
    )?;

    let mut ffmpeg = spawn_ffmpeg_video(&opts.input)?;
    let frame_bytes = frame_size_yuv420(info.width, info.height)?;
    let reorder_keep = seq.max_b_frames as usize + 2;
    let mut reorder: Vec<NativeFrame> = Vec::with_capacity(reorder_keep + 2);
    let mut history: VecDeque<ComparedFrame> = VecDeque::with_capacity(opts.context + 1);
    let mut presentation_index = 0u64;
    let mut decode_index = 0u64;
    let mut found = false;
    let mut after_remaining = 0usize;
    let mut stop = false;
    let mut worst_max = 0u8;
    let mut worst_presentation = 0u64;
    let mut worst_decode = 0u64;

    println!(
        "WMV3 {}x{}, MAXBFRAMES={}, FFmpeg comparison threshold={}, context={}",
        info.width, info.height, seq.max_b_frames, opts.threshold, opts.context
    );

    while !stop {
        let payloads = match asf.read_packet(&mut reader) {
            Ok(v) => v,
            Err(DecoderError::EndOfStream) => break,
            Err(err) => return Err(err),
        };

        for payload in payloads {
            if payload.stream_number != info.stream_number {
                continue;
            }

            let native = decode_native_frame(
                &mut decoder,
                &seq,
                decode_index,
                payload.pts_ms,
                payload.is_key_frame,
                payload.data,
                info.width,
                info.height,
            )?;
            let Some(native) = native else {
                continue;
            };
            write_manifest_row(&mut manifest, &native)?;
            decode_index += 1;

            reorder.push(native);
            reorder.sort_by_key(|f| (f.pts_ms, f.decode_index));

            while reorder.len() > reorder_keep {
                let native = reorder.remove(0);
                stop = compare_one(
                    opts,
                    &mut ffmpeg.stdout,
                    frame_bytes,
                    presentation_index,
                    native,
                    &mut compare_csv,
                    &mut history,
                    &mut found,
                    &mut after_remaining,
                    &mut worst_max,
                    &mut worst_presentation,
                    &mut worst_decode,
                )?;
                presentation_index += 1;
                if stop {
                    break;
                }
            }

            if stop {
                break;
            }
        }
    }

    if !stop {
        reorder.sort_by_key(|f| (f.pts_ms, f.decode_index));
        for native in reorder.drain(..) {
            stop = compare_one(
                opts,
                &mut ffmpeg.stdout,
                frame_bytes,
                presentation_index,
                native,
                &mut compare_csv,
                &mut history,
                &mut found,
                &mut after_remaining,
                &mut worst_max,
                &mut worst_presentation,
                &mut worst_decode,
            )?;
            presentation_index += 1;
            if stop {
                break;
            }
        }
    }

    manifest.flush()?;
    compare_csv.flush()?;

    if stop {
        let _ = ffmpeg.child.kill();
        let _ = ffmpeg.child.wait();
    } else {
        drop(ffmpeg.stdout);
        let status = ffmpeg.child.wait()?;
        if !status.success() {
            return Err(DecoderError::InvalidData(format!(
                "FFmpeg reference process failed with {status}"
            )));
        }
    }

    if found {
        println!(
            "dumped first structural mismatch and context to {}",
            opts.output_dir.display()
        );
    } else {
        println!(
            "no frame reached threshold {}; worst max_abs={} at presentation={} decode={}",
            opts.threshold, worst_max, worst_presentation, worst_decode
        );
        println!("see comparison.csv for per-frame differences");
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn compare_one(
    opts: &Options,
    ffmpeg: &mut ChildStdout,
    frame_bytes: usize,
    presentation_index: u64,
    native: NativeFrame,
    compare_csv: &mut BufWriter<File>,
    history: &mut VecDeque<ComparedFrame>,
    found: &mut bool,
    after_remaining: &mut usize,
    worst_max: &mut u8,
    worst_presentation: &mut u64,
    worst_decode: &mut u64,
) -> Result<bool> {
    let mut reference = vec![0u8; frame_bytes];
    let n = read_exact_or_eof(ffmpeg, &mut reference)?;
    if n != frame_bytes {
        return Err(DecoderError::InvalidData(format!(
            "FFmpeg reference ended at presentation frame {presentation_index}: {n}/{frame_bytes} bytes"
        )));
    }

    let diff = compare_frame(&native.frame, &reference);
    let total_bytes = native.frame.y.len() + native.frame.cb.len() + native.frame.cr.len();
    let mean_abs = if total_bytes == 0 {
        0.0
    } else {
        diff.sum_abs() as f64 / total_bytes as f64
    };
    let (first_plane, first_x, first_y) = match diff.first_mismatch {
        Some((plane, x, y, _, _)) => (plane, x.to_string(), y.to_string()),
        None => ("", String::new(), String::new()),
    };
    writeln!(
        compare_csv,
        "{},{},{},{},{},{},{},{},{:.6},{},{},{}",
        presentation_index,
        native.decode_index,
        native.pts_ms,
        native.header.frame_type,
        diff.y.mismatched,
        diff.cb.mismatched,
        diff.cr.mismatched,
        diff.max_abs(),
        mean_abs,
        first_plane,
        first_x,
        first_y
    )?;

    if diff.max_abs() > *worst_max {
        *worst_max = diff.max_abs();
        *worst_presentation = presentation_index;
        *worst_decode = native.decode_index;
    }

    let current = ComparedFrame {
        presentation_index,
        native,
        ffmpeg: reference,
        diff,
    };

    if *found {
        dump_compared(&opts.output_dir, &current)?;
        if *after_remaining == 0 {
            return Ok(true);
        }
        *after_remaining -= 1;
        return Ok(*after_remaining == 0);
    }

    let significant = if opts.threshold == 0 {
        current.diff.mismatched() != 0
    } else {
        current.diff.max_abs() >= opts.threshold
    };

    if significant {
        println!(
            "first structural mismatch: presentation={} decode={} pts={} type={} max_abs={} mismatched={} first={:?}",
            current.presentation_index,
            current.native.decode_index,
            current.native.pts_ms,
            current.native.header.frame_type,
            current.diff.max_abs(),
            current.diff.mismatched(),
            current.diff.first_mismatch
        );
        while let Some(prev) = history.pop_front() {
            dump_compared(&opts.output_dir, &prev)?;
        }
        dump_compared(&opts.output_dir, &current)?;
        *found = true;
        *after_remaining = opts.context;
        return Ok(opts.context == 0);
    }

    history.push_back(current);
    while history.len() > opts.context {
        history.pop_front();
    }
    Ok(false)
}

fn decode_native_frame(
    decoder: &mut Wmv3Decoder,
    seq: &SequenceHeader,
    decode_index: u64,
    pts_ms: u32,
    is_key: bool,
    payload: Vec<u8>,
    width: u32,
    height: u32,
) -> Result<Option<NativeFrame>> {
    let mb_w = width.div_ceil(16) as usize;
    let mb_h = height.div_ceil(16) as usize;
    let header = PictureHeader::parse(&payload, seq, pts_ms, mb_w, mb_h)?;
    let frame = decoder.decode_frame_owned(&payload, is_key, pts_ms)?;
    Ok(frame.map(|frame| NativeFrame {
        decode_index,
        pts_ms,
        is_key,
        header,
        payload,
        frame,
    }))
}

fn create_manifest(dir: &Path) -> Result<BufWriter<File>> {
    let mut out = BufWriter::new(File::create(dir.join("native_manifest.csv"))?);
    writeln!(
        out,
        "decode_index,pts_ms,key,frame_type,pqindex,pquant,halfqp,rangeredfrm,mvrange,mv_mode,mv_mode2,mvtab,cbptab,ttmbf,ttfrm,transacfrm,transacfrm2,dctab,dquant_enabled,dquant_profile,dquant_edge,dquant_bilevel,dquant_alt_pquant,header_bits,payload_bytes,y_hash,cb_hash,cr_hash"
    )?;
    Ok(out)
}

fn write_manifest_row(out: &mut BufWriter<File>, f: &NativeFrame) -> Result<()> {
    let h = &f.header;
    writeln!(
        out,
        "{},{},{},{},{},{},{},{},{},{:?},{:?},{},{},{},{},{},{},{},{},{},{},{},{},{},{},0x{:016x},0x{:016x},0x{:016x}",
        f.decode_index,
        f.pts_ms,
        f.is_key as u8,
        h.frame_type,
        h.pqindex,
        h.pquant,
        h.halfqp as u8,
        h.rangeredfrm as u8,
        h.mvrange,
        h.mv_mode,
        h.mv_mode2,
        h.mvtab,
        h.cbptab,
        h.ttmbf as u8,
        h.ttfrm,
        h.transacfrm,
        h.transacfrm2,
        h.dctab as u8,
        h.dquant.enabled as u8,
        h.dquant.profile,
        h.dquant.edge,
        h.dquant.bi_level as u8,
        h.dquant.alt_pquant,
        h.header_bits,
        f.payload.len(),
        fnv1a(&f.frame.y),
        fnv1a(&f.frame.cb),
        fnv1a(&f.frame.cr),
    )?;
    Ok(())
}

fn dump_native(dir: &Path, f: &NativeFrame, tag: &str) -> Result<()> {
    let stem = format!(
        "{tag}_decode_{:06}_pts_{:010}_{}",
        f.decode_index, f.pts_ms, f.header.frame_type
    );
    write_yuv(&dir.join(format!("{stem}_native.yuv")), &f.frame)?;
    fs::write(dir.join(format!("{stem}.wmv3_payload")), &f.payload)?;
    write_pgm(
        &dir.join(format!("{stem}_native_Y.pgm")),
        f.frame.width as usize,
        f.frame.height as usize,
        &f.frame.y,
    )?;
    write_header_text(&dir.join(format!("{stem}_header.txt")), f)?;
    Ok(())
}

fn dump_compared(dir: &Path, c: &ComparedFrame) -> Result<()> {
    let f = &c.native;
    let stem = format!(
        "present_{:06}_decode_{:06}_pts_{:010}_{}",
        c.presentation_index, f.decode_index, f.pts_ms, f.header.frame_type
    );
    dump_native(dir, f, &format!("present_{:06}", c.presentation_index))?;
    fs::write(dir.join(format!("{stem}_ffmpeg.yuv")), &c.ffmpeg)?;

    let y_len = f.frame.y.len();
    write_pgm(
        &dir.join(format!("{stem}_ffmpeg_Y.pgm")),
        f.frame.width as usize,
        f.frame.height as usize,
        &c.ffmpeg[..y_len],
    )?;

    let y_diff: Vec<u8> = f
        .frame
        .y
        .iter()
        .zip(c.ffmpeg[..y_len].iter())
        .map(|(&a, &b)| a.abs_diff(b))
        .collect();
    write_pgm(
        &dir.join(format!("{stem}_Y_absdiff.pgm")),
        f.frame.width as usize,
        f.frame.height as usize,
        &y_diff,
    )?;
    write_mb_diff_csv(&dir.join(format!("{stem}_mb_diff.csv")), f, &c.ffmpeg)?;

    let mut summary = BufWriter::new(File::create(dir.join(format!("{stem}_compare.txt")))?);
    writeln!(summary, "presentation_index={}", c.presentation_index)?;
    writeln!(summary, "decode_index={}", f.decode_index)?;
    writeln!(summary, "pts_ms={}", f.pts_ms)?;
    writeln!(summary, "frame_type={}", f.header.frame_type)?;
    writeln!(summary, "mismatch_y={}", c.diff.y.mismatched)?;
    writeln!(summary, "mismatch_cb={}", c.diff.cb.mismatched)?;
    writeln!(summary, "mismatch_cr={}", c.diff.cr.mismatched)?;
    writeln!(summary, "max_abs={}", c.diff.max_abs())?;
    writeln!(summary, "first_mismatch={:?}", c.diff.first_mismatch)?;
    summary.flush()?;
    Ok(())
}

fn write_header_text(path: &Path, f: &NativeFrame) -> Result<()> {
    let h = &f.header;
    let mut out = BufWriter::new(File::create(path)?);
    writeln!(out, "decode_index={}", f.decode_index)?;
    writeln!(out, "pts_ms={}", f.pts_ms)?;
    writeln!(out, "is_key={}", f.is_key)?;
    writeln!(out, "frame_type={}", h.frame_type)?;
    writeln!(out, "pqindex={}", h.pqindex)?;
    writeln!(out, "pquant={}", h.pquant)?;
    writeln!(out, "halfqp={}", h.halfqp)?;
    writeln!(out, "rangeredfrm={}", h.rangeredfrm)?;
    writeln!(out, "mvrange={}", h.mvrange)?;
    writeln!(out, "mv_mode={:?}", h.mv_mode)?;
    writeln!(out, "mv_mode2={:?}", h.mv_mode2)?;
    writeln!(out, "lumscale={}", h.lumscale)?;
    writeln!(out, "lumshift={}", h.lumshift)?;
    writeln!(out, "mvtab={}", h.mvtab)?;
    writeln!(out, "cbptab={}", h.cbptab)?;
    writeln!(out, "ttmbf={}", h.ttmbf)?;
    writeln!(out, "ttfrm={}", h.ttfrm)?;
    writeln!(out, "transacfrm={}", h.transacfrm)?;
    writeln!(out, "transacfrm2={}", h.transacfrm2)?;
    writeln!(out, "dctab={}", h.dctab)?;
    writeln!(out, "dquant_enabled={}", h.dquant.enabled)?;
    writeln!(out, "dquant_profile={}", h.dquant.profile)?;
    writeln!(out, "dquant_edge={}", h.dquant.edge)?;
    writeln!(out, "dquant_bilevel={}", h.dquant.bi_level)?;
    writeln!(out, "dquant_alt_pquant={}", h.dquant.alt_pquant)?;
    writeln!(out, "header_bits={}", h.header_bits)?;
    writeln!(out, "payload_bytes={}", f.payload.len())?;
    out.flush()?;
    Ok(())
}

fn compare_frame(native: &YuvFrame, reference: &[u8]) -> FrameDiff {
    let y_len = native.y.len();
    let cb_len = native.cb.len();
    let mut out = FrameDiff::default();
    compare_plane(
        "Y",
        native.width as usize,
        &native.y,
        &reference[..y_len],
        &mut out.y,
        &mut out.first_mismatch,
    );
    compare_plane(
        "Cb",
        native.width as usize / 2,
        &native.cb,
        &reference[y_len..y_len + cb_len],
        &mut out.cb,
        &mut out.first_mismatch,
    );
    compare_plane(
        "Cr",
        native.width as usize / 2,
        &native.cr,
        &reference[y_len + cb_len..],
        &mut out.cr,
        &mut out.first_mismatch,
    );
    out
}

fn compare_plane(
    name: &'static str,
    width: usize,
    native: &[u8],
    reference: &[u8],
    stats: &mut PlaneDiff,
    first: &mut Option<(&'static str, usize, usize, u8, u8)>,
) {
    for (i, (&a, &b)) in native.iter().zip(reference.iter()).enumerate() {
        if a == b {
            continue;
        }
        let d = a.abs_diff(b);
        stats.mismatched += 1;
        stats.max_abs = stats.max_abs.max(d);
        stats.sum_abs += d as u128;
        if first.is_none() {
            let x = if width == 0 { 0 } else { i % width };
            let y = i.checked_div(width).unwrap_or(0);
            *first = Some((name, x, y, a, b));
        }
    }
}

fn write_mb_diff_csv(path: &Path, native: &NativeFrame, reference: &[u8]) -> Result<()> {
    let width = native.frame.width as usize;
    let height = native.frame.height as usize;
    let y_ref = &reference[..native.frame.y.len()];
    let mb_w = width.div_ceil(16);
    let mb_h = height.div_ceil(16);
    let mut out = BufWriter::new(File::create(path)?);
    writeln!(out, "mb_x,mb_y,mismatched_pixels,max_abs,mean_abs")?;

    for mb_y in 0..mb_h {
        for mb_x in 0..mb_w {
            let x0 = mb_x * 16;
            let y0 = mb_y * 16;
            let x1 = (x0 + 16).min(width);
            let y1 = (y0 + 16).min(height);
            let mut mismatched = 0u64;
            let mut max_abs = 0u8;
            let mut sum_abs = 0u64;
            let mut count = 0u64;
            for y in y0..y1 {
                let row = y * width;
                for x in x0..x1 {
                    let d = native.frame.y[row + x].abs_diff(y_ref[row + x]);
                    if d != 0 {
                        mismatched += 1;
                    }
                    max_abs = max_abs.max(d);
                    sum_abs += d as u64;
                    count += 1;
                }
            }
            let mean_abs = if count == 0 {
                0.0
            } else {
                sum_abs as f64 / count as f64
            };
            writeln!(out, "{mb_x},{mb_y},{mismatched},{max_abs},{mean_abs:.6}")?;
        }
    }
    out.flush()?;
    Ok(())
}

fn write_yuv(path: &Path, frame: &YuvFrame) -> Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    out.write_all(&frame.y)?;
    out.write_all(&frame.cb)?;
    out.write_all(&frame.cr)?;
    out.flush()?;
    Ok(())
}

fn write_pgm(path: &Path, width: usize, height: usize, pixels: &[u8]) -> Result<()> {
    let expected = width.saturating_mul(height);
    if pixels.len() < expected {
        return Err(DecoderError::InvalidData(format!(
            "PGM input too small for {}x{}: {} bytes",
            width,
            height,
            pixels.len()
        )));
    }
    let mut out = BufWriter::new(File::create(path)?);
    write!(out, "P5\n{} {}\n255\n", width, height)?;
    out.write_all(&pixels[..expected])?;
    out.flush()?;
    Ok(())
}

fn frame_size_yuv420(width: u32, height: u32) -> Result<usize> {
    let y = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| DecoderError::InvalidData("video dimensions overflow".into()))?;
    y.checked_add(y / 2)
        .ok_or_else(|| DecoderError::InvalidData("video frame size overflow".into()))
}

fn spawn_ffmpeg_video(input: &Path) -> Result<FfmpegPipe> {
    let input = input.to_string_lossy().into_owned();
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            &input,
            "-map",
            "0:v:0",
            "-an",
            "-sn",
            "-dn",
            "-pix_fmt",
            "yuv420p",
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|err| {
            DecoderError::Io(std::io::Error::new(
                err.kind(),
                format!("failed to start ffmpeg: {err}"),
            ))
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| DecoderError::InvalidData("ffmpeg stdout pipe was not created".into()))?;
    Ok(FfmpegPipe { child, stdout })
}

fn read_exact_or_eof(reader: &mut impl Read, buf: &mut [u8]) -> Result<usize> {
    let mut pos = 0usize;
    while pos < buf.len() {
        match reader.read(&mut buf[pos..]) {
            Ok(0) => break,
            Ok(n) => pos += n,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(DecoderError::Io(err)),
        }
    }
    Ok(pos)
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut h = FNV64_OFFSET;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(FNV64_PRIME);
    }
    h
}
