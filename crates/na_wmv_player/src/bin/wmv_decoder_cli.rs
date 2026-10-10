//! Deterministic WMV3 / WMA Pro decoder validation CLI.
//!
//! The important property of this binary is that codec validation does not go
//! through Kira, CPAL, WGPU, or the Siglus movie integration.  It consumes the
//! ASF file directly through `AsfWmv2Decoder` / `AsfWmaDecoder`, can dump the
//! native decoder output, and can compare that output with FFmpeg.
//!
//! Usage:
//!   wmv-decoder <input.wmv> [output_dir] [options]
//!
//! Modes (video-only is kept as the default for backwards compatibility):
//!   --audio-only          Decode only the WMA stream.
//!   --both                Decode both video and audio streams.
//!
//! Native decoder dumps:
//!   --yuv                 Write one YUV420p file per decoded video frame.
//!   --png                 Write one PNG file per decoded video frame.
//!   --audio-f32           Write interleaved native-endian-independent f32le PCM.
//!   --audio-wav           Write an IEEE-float WAV containing the same PCM.
//!
//! Reference validation:
//!   --verify-ffmpeg       Compare native output against the local `ffmpeg` binary.
//!
//! `--verify-ffmpeg` compares WMV output byte-for-byte in YUV420p.  WMA Pro is
//! compared sample-for-sample as f32 with numerical error statistics; independent
//! RMS/peak/non-zero statistics are also printed so a silent native decoder is
//! immediately distinguishable from an audio-output/player problem.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};

use wmv_decoder::asf::AsfFile;
use wmv_decoder::{AsfWmaDecoder, AsfWmv2Decoder, DecoderError, Result, YuvFrame};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecodeMode {
    VideoOnly,
    AudioOnly,
    Both,
}

#[derive(Debug)]
struct Options {
    input_path: String,
    output_dir: Option<PathBuf>,
    mode: DecodeMode,
    dump_yuv: bool,
    dump_png: bool,
    dump_audio_f32: bool,
    dump_audio_wav: bool,
    verify_ffmpeg: bool,
}

fn usage(program: &str) {
    eprintln!(
        "Usage: {program} <input.wmv> [output_dir] [--yuv] [--png] \\\n         [--audio-f32] [--audio-wav] [--audio-only|--both] [--verify-ffmpeg]"
    );
}

fn parse_args() -> Options {
    let args: Vec<String> = std::env::args().collect();
    let mut input_path: Option<String> = None;
    let mut output_dir: Option<PathBuf> = None;
    let mut mode = DecodeMode::VideoOnly;
    let mut dump_yuv = false;
    let mut dump_png = false;
    let mut dump_audio_f32 = false;
    let mut dump_audio_wav = false;
    let mut verify_ffmpeg = false;

    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--yuv" => dump_yuv = true,
            "--png" => dump_png = true,
            "--audio-f32" => dump_audio_f32 = true,
            "--audio-wav" => dump_audio_wav = true,
            "--audio-only" => mode = DecodeMode::AudioOnly,
            "--both" => mode = DecodeMode::Both,
            "--verify-ffmpeg" => verify_ffmpeg = true,
            "--help" | "-h" => {
                usage(&args[0]);
                std::process::exit(0);
            }
            _ if arg.starts_with('-') => {
                eprintln!("Unexpected option: {arg}");
                usage(&args[0]);
                std::process::exit(1);
            }
            _ => {
                if input_path.is_none() {
                    input_path = Some(arg.clone());
                } else if output_dir.is_none() {
                    output_dir = Some(PathBuf::from(arg));
                } else {
                    eprintln!("Unexpected argument: {arg}");
                    usage(&args[0]);
                    std::process::exit(1);
                }
            }
        }
    }

    let Some(input_path) = input_path else {
        usage(&args[0]);
        std::process::exit(1);
    };

    if output_dir.is_some() && !dump_yuv && !dump_png && !dump_audio_f32 && !dump_audio_wav {
        // Preserve the old CLI behavior: an output directory by itself means
        // per-frame YUV dump.
        dump_yuv = true;
    }

    if (dump_audio_f32 || dump_audio_wav) && mode == DecodeMode::VideoOnly {
        mode = DecodeMode::Both;
    }

    if (dump_yuv || dump_png || dump_audio_f32 || dump_audio_wav) && output_dir.is_none() {
        eprintln!("An output_dir is required when a dump option is used.");
        usage(&args[0]);
        std::process::exit(1);
    }

    Options {
        input_path,
        output_dir,
        mode,
        dump_yuv,
        dump_png,
        dump_audio_f32,
        dump_audio_wav,
        verify_ffmpeg,
    }
}

fn main() {
    env_logger::init();
    let opts = parse_args();

    if let Err(e) = run(&opts) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run(opts: &Options) -> Result<()> {
    if let Some(dir) = opts.output_dir.as_deref() {
        std::fs::create_dir_all(dir)?;
    }

    print_stream_info(&opts.input_path)?;

    if matches!(opts.mode, DecodeMode::VideoOnly | DecodeMode::Both) {
        decode_video(opts)?;
    }
    if matches!(opts.mode, DecodeMode::AudioOnly | DecodeMode::Both) {
        decode_audio(opts)?;
    }

    Ok(())
}

fn print_stream_info(input_path: &str) -> Result<()> {
    let file = File::open(input_path)?;
    let mut reader = BufReader::new(file);
    let asf = AsfFile::open(&mut reader)?;

    println!(
        "ASF: packet_size={} packets={} duration_ms={:?}",
        asf.packet_size, asf.packet_count, asf.play_duration_ms
    );

    for (idx, v) in asf.video_streams.iter().enumerate() {
        let fourcc = String::from_utf8_lossy(&v.codec_four_cc);
        println!(
            "video[{idx}]: stream={} codec={} {}x{} extradata={} bytes",
            v.stream_number,
            fourcc,
            v.width,
            v.height,
            v.extra_data.len()
        );
    }

    for (idx, a) in asf.audio_streams.iter().enumerate() {
        println!(
            "audio[{idx}]: stream={} tag=0x{:04X} channels={} rate={} bit_rate={} block_align={} bits={} extradata={} bytes{}",
            a.stream_number,
            a.format_tag,
            a.channels,
            a.sample_rate,
            a.bit_rate,
            a.block_align,
            a.bits_per_sample,
            a.extra_data.len(),
            if a.format_tag == 0x0162 {
                " [WMA Pro]"
            } else {
                ""
            }
        );
    }

    Ok(())
}

#[derive(Default)]
struct VideoStats {
    frames: u64,
    bytes: u64,
    hash: u64,
    y_min: u8,
    y_max: u8,
    cb_min: u8,
    cb_max: u8,
    cr_min: u8,
    cr_max: u8,
    initialized: bool,
}

impl VideoStats {
    fn update(&mut self, frame: &YuvFrame) {
        if !self.initialized {
            self.y_min = 255;
            self.cb_min = 255;
            self.cr_min = 255;
            self.initialized = true;
            self.hash = FNV64_OFFSET;
        }
        self.frames += 1;
        self.bytes += (frame.y.len() + frame.cb.len() + frame.cr.len()) as u64;
        for &v in &frame.y {
            self.y_min = self.y_min.min(v);
            self.y_max = self.y_max.max(v);
            fnv1a_byte(&mut self.hash, v);
        }
        for &v in &frame.cb {
            self.cb_min = self.cb_min.min(v);
            self.cb_max = self.cb_max.max(v);
            fnv1a_byte(&mut self.hash, v);
        }
        for &v in &frame.cr {
            self.cr_min = self.cr_min.min(v);
            self.cr_max = self.cr_max.max(v);
            fnv1a_byte(&mut self.hash, v);
        }
    }
}

#[derive(Default)]
struct VideoCompareStats {
    frames_compared: u64,
    reference_short_frames: u64,
    mismatched_bytes: u64,
    max_abs_diff: u8,
    sum_abs_diff: u128,
    first_mismatch: Option<String>,
}

impl VideoCompareStats {
    fn compare_frame(&mut self, idx: u64, frame: &YuvFrame, reference: &[u8]) {
        self.frames_compared += 1;
        let y_len = frame.y.len();
        let cb_len = frame.cb.len();
        let expected_len = y_len + cb_len + frame.cr.len();
        if reference.len() != expected_len {
            self.reference_short_frames += 1;
            return;
        }

        self.compare_plane(
            idx,
            "Y",
            frame.width as usize,
            &frame.y,
            &reference[..y_len],
        );
        let cw = frame.width as usize / 2;
        self.compare_plane(idx, "Cb", cw, &frame.cb, &reference[y_len..y_len + cb_len]);
        self.compare_plane(idx, "Cr", cw, &frame.cr, &reference[y_len + cb_len..]);
    }

    fn compare_plane(
        &mut self,
        frame_idx: u64,
        plane: &str,
        width: usize,
        got: &[u8],
        reference: &[u8],
    ) {
        for (i, (&a, &b)) in got.iter().zip(reference.iter()).enumerate() {
            if a == b {
                continue;
            }
            self.mismatched_bytes += 1;
            let d = a.abs_diff(b);
            self.max_abs_diff = self.max_abs_diff.max(d);
            self.sum_abs_diff += d as u128;
            if self.first_mismatch.is_none() {
                let x = if width == 0 { 0 } else { i % width };
                let y = i.checked_div(width).unwrap_or(0);
                self.first_mismatch = Some(format!(
                    "frame={} plane={} x={} y={} native={} ffmpeg={}",
                    frame_idx, plane, x, y, a, b
                ));
            }
        }
    }
}

fn decode_video(opts: &Options) -> Result<()> {
    let file = File::open(&opts.input_path)?;
    let reader = BufReader::new(file);
    let mut dec = AsfWmv2Decoder::open(reader)?;
    let info = dec.video_stream_info().clone();
    let fourcc = String::from_utf8_lossy(&info.codec_four_cc).to_uppercase();

    println!(
        "video decoder: codec={fourcc} {}x{}",
        info.width, info.height
    );
    if fourcc != "WMV3" && fourcc != "WVC1" {
        println!(
            "video note: this file is neither WMV3 nor WVC1; no VC-1-specific conclusion should be drawn from it"
        );
    }

    let mut ffmpeg = if opts.verify_ffmpeg {
        Some(spawn_ffmpeg_video(&opts.input_path)?)
    } else {
        None
    };
    let frame_bytes = (info.width as usize)
        .checked_mul(info.height as usize)
        .and_then(|y| y.checked_add(y / 2))
        .ok_or_else(|| DecoderError::InvalidData("video dimensions overflow".into()))?;
    let mut reference = vec![0u8; frame_bytes];

    let mut stats = VideoStats::default();
    let mut compare = VideoCompareStats::default();
    let mut reference_exhausted = false;
    let mut idx: u64 = 0;

    while let Some(df) = dec.next_frame()? {
        idx += 1;
        stats.update(&df.frame);

        if let Some(dir) = opts.output_dir.as_deref() {
            if opts.dump_yuv {
                let fname = dir.join(format!("frame_{idx:06}.yuv"));
                write_yuv_frame(&fname, &df.frame)?;
            }
            if opts.dump_png {
                let fname = dir.join(format!("frame_{idx:06}.png"));
                write_png_frame(&fname, &df.frame)?;
            }
        }

        if let Some(proc) = ffmpeg.as_mut()
            && !reference_exhausted
        {
            let n = read_exact_or_eof(&mut proc.stdout, &mut reference)?;
            if n != reference.len() {
                compare.reference_short_frames += 1;
                reference_exhausted = true;
                eprintln!(
                    "FFmpeg video reference ended inside frame {idx}: got {n}/{} bytes",
                    reference.len()
                );
            } else {
                compare.compare_frame(idx, &df.frame, &reference);
            }
        }
    }

    println!(
        "video native: frames={} bytes={} hash=0x{:016X} Y={}..{} Cb={}..{} Cr={}..{}",
        stats.frames,
        stats.bytes,
        stats.hash,
        stats.y_min,
        stats.y_max,
        stats.cb_min,
        stats.cb_max,
        stats.cr_min,
        stats.cr_max
    );

    if let Some(mut proc) = ffmpeg {
        let mut extra = [0u8; 1];
        let extra_reference = if reference_exhausted {
            false
        } else {
            proc.stdout.read(&mut extra)? != 0
        };
        if extra_reference {
            drain_to_eof(&mut proc.stdout)?;
        }
        let status = proc.child.wait()?;
        if !status.success() {
            return Err(DecoderError::InvalidData(format!(
                "FFmpeg video reference process failed with {status}"
            )));
        }
        let mean_abs = if compare.mismatched_bytes == 0 {
            0.0
        } else {
            compare.sum_abs_diff as f64 / compare.mismatched_bytes as f64
        };
        println!(
            "video ffmpeg compare: frames={} mismatched_bytes={} max_abs_diff={} mean_abs_diff={:.6} reference_has_extra_frame_data={}",
            compare.frames_compared,
            compare.mismatched_bytes,
            compare.max_abs_diff,
            mean_abs,
            extra_reference
        );
        if let Some(first) = compare.first_mismatch {
            println!("video first mismatch: {first}");
        }
        if compare.reference_short_frames == 0 && !extra_reference && compare.mismatched_bytes == 0
        {
            println!("video verdict: EXACT MATCH with FFmpeg YUV420p output");
        } else {
            println!("video verdict: MISMATCH with FFmpeg YUV420p output");
        }
    }

    Ok(())
}

#[derive(Default)]
struct AudioStats {
    chunks: u64,
    samples: u64,
    sample_frames: u64,
    first_pts_ms: Option<u32>,
    last_pts_ms: Option<u32>,
    min: f32,
    max: f32,
    peak: f32,
    sum_sq: f64,
    nonzero: u64,
    non_finite: u64,
    hash: u64,
    initialized: bool,
}

impl AudioStats {
    fn update(&mut self, pts_ms: u32, channels: u16, samples: &[f32]) {
        if !self.initialized {
            self.min = f32::INFINITY;
            self.max = f32::NEG_INFINITY;
            self.hash = FNV64_OFFSET;
            self.initialized = true;
        }
        self.chunks += 1;
        self.first_pts_ms.get_or_insert(pts_ms);
        self.last_pts_ms = Some(pts_ms);
        self.samples += samples.len() as u64;
        if channels != 0 {
            self.sample_frames += (samples.len() / channels as usize) as u64;
        }
        for &s in samples {
            for b in s.to_bits().to_le_bytes() {
                fnv1a_byte(&mut self.hash, b);
            }
            if !s.is_finite() {
                self.non_finite += 1;
                continue;
            }
            self.min = self.min.min(s);
            self.max = self.max.max(s);
            self.peak = self.peak.max(s.abs());
            self.sum_sq += (s as f64) * (s as f64);
            if s != 0.0 {
                self.nonzero += 1;
            }
        }
    }

    fn rms(&self) -> f64 {
        let finite = self.samples.saturating_sub(self.non_finite);
        if finite == 0 {
            0.0
        } else {
            (self.sum_sq / finite as f64).sqrt()
        }
    }
}

#[derive(Default)]
struct AudioCompareStats {
    compared_samples: u64,
    missing_reference_samples: u64,
    abs_error_sum: f64,
    sq_error_sum: f64,
    max_abs_error: f32,
    first_large_error: Option<String>,
}

impl AudioCompareStats {
    fn compare(&mut self, native_start_sample: u64, native: &[f32], reference_bytes: &[u8]) {
        let ref_samples = reference_bytes.len() / 4;
        let count = native.len().min(ref_samples);
        for i in 0..count {
            let o = i * 4;
            let reference = f32::from_le_bytes([
                reference_bytes[o],
                reference_bytes[o + 1],
                reference_bytes[o + 2],
                reference_bytes[o + 3],
            ]);
            let d = (native[i] - reference).abs();
            self.compared_samples += 1;
            self.abs_error_sum += d as f64;
            self.sq_error_sum += (d as f64) * (d as f64);
            self.max_abs_error = self.max_abs_error.max(d);
            if self.first_large_error.is_none() && d > 1.0e-4 {
                self.first_large_error = Some(format!(
                    "sample={} native={:.9} ffmpeg={:.9} abs_diff={:.9}",
                    native_start_sample + i as u64,
                    native[i],
                    reference,
                    d
                ));
            }
        }
        if native.len() > ref_samples {
            self.missing_reference_samples += (native.len() - ref_samples) as u64;
        }
    }

    fn mean_abs_error(&self) -> f64 {
        if self.compared_samples == 0 {
            0.0
        } else {
            self.abs_error_sum / self.compared_samples as f64
        }
    }

    fn rms_error(&self) -> f64 {
        if self.compared_samples == 0 {
            0.0
        } else {
            (self.sq_error_sum / self.compared_samples as f64).sqrt()
        }
    }
}

fn decode_audio(opts: &Options) -> Result<()> {
    ensure_wmapro_input(&opts.input_path)?;

    let file = File::open(&opts.input_path)?;
    let reader = BufReader::new(file);
    let mut dec = AsfWmaDecoder::open(reader)?;
    let sample_rate = dec.sample_rate();
    let channels = dec.channels();

    println!(
        "audio decoder: WMA Pro channels={} rate={} duration_ms={:?}",
        channels,
        sample_rate,
        dec.duration_ms()
    );

    let mut raw_out = if opts.dump_audio_f32 {
        let path = opts.output_dir.as_ref().unwrap().join("audio_native.f32le");
        Some(BufWriter::new(File::create(path)?))
    } else {
        None
    };

    let mut wav_out = if opts.dump_audio_wav {
        let path = opts
            .output_dir
            .as_ref()
            .unwrap()
            .join("audio_native_f32.wav");
        let mut file = File::create(path)?;
        write_float_wav_header(&mut file, channels, sample_rate, 0)?;
        Some(file)
    } else {
        None
    };

    let mut ffmpeg = if opts.verify_ffmpeg {
        Some(spawn_ffmpeg_audio(&opts.input_path)?)
    } else {
        None
    };

    let mut native_stats = AudioStats::default();
    let mut reference_stats = AudioStats::default();
    let mut compare = AudioCompareStats::default();
    let mut wav_data_bytes: u64 = 0;
    let mut native_sample_cursor: u64 = 0;

    while let Some(df) = dec.next_frame()? {
        let samples = &df.frame.samples;
        native_stats.update(df.pts_ms, channels, samples);

        if let Some(w) = raw_out.as_mut() {
            write_f32le(w, samples)?;
        }
        if let Some(w) = wav_out.as_mut() {
            write_f32le(w, samples)?;
            wav_data_bytes = wav_data_bytes
                .checked_add((samples.len() as u64) * 4)
                .ok_or_else(|| DecoderError::InvalidData("WAV size overflow".into()))?;
        }

        if let Some(proc) = ffmpeg.as_mut() {
            let mut reference_bytes = vec![0u8; samples.len() * 4];
            let n = read_exact_or_eof(&mut proc.stdout, &mut reference_bytes)?;
            reference_bytes.truncate(n - (n % 4));
            let reference_samples = bytes_to_f32(&reference_bytes);
            reference_stats.update(df.pts_ms, channels, &reference_samples);
            compare.compare(native_sample_cursor, samples, &reference_bytes);
        }
        native_sample_cursor += samples.len() as u64;
    }

    if let Some(w) = raw_out.as_mut() {
        w.flush()?;
    }
    if let Some(mut w) = wav_out {
        if wav_data_bytes > u32::MAX as u64 {
            return Err(DecoderError::Unsupported(
                "diagnostic WAV is larger than classic RIFF/WAVE supports".into(),
            ));
        }
        w.seek(SeekFrom::Start(0))?;
        write_float_wav_header(&mut w, channels, sample_rate, wav_data_bytes as u32)?;
        w.flush()?;
    }

    print_audio_stats("audio native", &native_stats);
    if native_stats.samples != 0 && native_stats.nonzero == 0 {
        println!("audio verdict: NATIVE DECODER OUTPUT IS COMPLETELY SILENT");
    }

    if let Some(mut proc) = ffmpeg {
        // Consume any reference samples left after the native decoder reached EOF.
        let mut tail = [0u8; 64 * 1024];
        let mut reference_extra_samples = 0u64;
        loop {
            let n = proc.stdout.read(&mut tail)?;
            if n == 0 {
                break;
            }
            let n = n - (n % 4);
            let samples = bytes_to_f32(&tail[..n]);
            reference_extra_samples += samples.len() as u64;
            reference_stats.update(0, channels, &samples);
        }
        let status = proc.child.wait()?;
        if !status.success() {
            return Err(DecoderError::InvalidData(format!(
                "FFmpeg audio reference process failed with {status}"
            )));
        }

        print_audio_stats("audio ffmpeg", &reference_stats);
        println!(
            "audio ffmpeg compare: compared_samples={} missing_reference_samples={} extra_reference_samples={} mean_abs_error={:.9} rms_error={:.9} max_abs_error={:.9}",
            compare.compared_samples,
            compare.missing_reference_samples,
            reference_extra_samples,
            compare.mean_abs_error(),
            compare.rms_error(),
            compare.max_abs_error
        );
        if let Some(first) = compare.first_large_error {
            println!("audio first >1e-4 mismatch: {first}");
        }
        if reference_stats.samples != 0 && reference_stats.nonzero != 0 && native_stats.nonzero == 0
        {
            println!(
                "audio verdict: DECODER FAILURE -- FFmpeg is non-silent but native WMA Pro output is silent"
            );
        } else if compare.missing_reference_samples == 0
            && reference_extra_samples == 0
            && compare.max_abs_error <= 1.0e-4
        {
            println!("audio verdict: MATCH within 1e-4/sample against FFmpeg");
        } else {
            println!(
                "audio verdict: MISMATCH with FFmpeg; inspect sample-count/RMS/error statistics above"
            );
        }
    }

    Ok(())
}

fn ensure_wmapro_input(input_path: &str) -> Result<()> {
    let file = File::open(input_path)?;
    let mut reader = BufReader::new(file);
    let asf = AsfFile::open(&mut reader)?;
    let Some(audio) = asf
        .audio_streams
        .iter()
        .find(|a| matches!(a.format_tag, 0x0160..=0x0162))
    else {
        return Err(DecoderError::Unsupported(
            "no supported WMA audio stream found".into(),
        ));
    };
    if audio.format_tag != 0x0162 {
        return Err(DecoderError::Unsupported(format!(
            "audio validation mode is intentionally limited to WMA Pro (0x0162); selected stream is 0x{:04X}",
            audio.format_tag
        )));
    }
    Ok(())
}

struct FfmpegPipe {
    child: Child,
    stdout: ChildStdout,
}

fn spawn_ffmpeg_video(input_path: &str) -> Result<FfmpegPipe> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            input_path,
            "-map",
            "0:v:0",
            "-an",
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
        .map_err(|e| {
            DecoderError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to start ffmpeg for video reference: {e}"),
            ))
        })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        DecoderError::InvalidData("ffmpeg video stdout pipe was not created".into())
    })?;
    Ok(FfmpegPipe { child, stdout })
}

fn spawn_ffmpeg_audio(input_path: &str) -> Result<FfmpegPipe> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            input_path,
            "-map",
            "0:a:0",
            "-vn",
            "-acodec",
            "pcm_f32le",
            "-f",
            "f32le",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            DecoderError::Io(std::io::Error::new(
                e.kind(),
                format!("failed to start ffmpeg for audio reference: {e}"),
            ))
        })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        DecoderError::InvalidData("ffmpeg audio stdout pipe was not created".into())
    })?;
    Ok(FfmpegPipe { child, stdout })
}

fn read_exact_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut off = 0usize;
    while off < buf.len() {
        match reader.read(&mut buf[off..])? {
            0 => break,
            n => off += n,
        }
    }
    Ok(off)
}

fn drain_to_eof<R: Read>(reader: &mut R) -> std::io::Result<u64> {
    let mut total = 0u64;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
    }
    Ok(total)
}

fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn print_audio_stats(label: &str, stats: &AudioStats) {
    let (min, max) = if stats.initialized {
        (stats.min, stats.max)
    } else {
        (0.0, 0.0)
    };
    println!(
        "{label}: chunks={} sample_frames={} samples={} pts={:?}..{:?} min={:.9} max={:.9} peak={:.9} rms={:.9} nonzero={} non_finite={} hash=0x{:016X}",
        stats.chunks,
        stats.sample_frames,
        stats.samples,
        stats.first_pts_ms,
        stats.last_pts_ms,
        min,
        max,
        stats.peak,
        stats.rms(),
        stats.nonzero,
        stats.non_finite,
        stats.hash
    );
}

const FNV64_OFFSET: u64 = 0xcbf29ce484222325;
const FNV64_PRIME: u64 = 0x100000001b3;

fn fnv1a_byte(hash: &mut u64, byte: u8) {
    *hash ^= byte as u64;
    *hash = hash.wrapping_mul(FNV64_PRIME);
}

fn write_f32le<W: Write>(w: &mut W, samples: &[f32]) -> Result<()> {
    for &sample in samples {
        w.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}

fn write_float_wav_header<W: Write>(
    w: &mut W,
    channels: u16,
    sample_rate: u32,
    data_bytes: u32,
) -> Result<()> {
    let block_align = channels
        .checked_mul(4)
        .ok_or_else(|| DecoderError::InvalidData("WAV block-align overflow".into()))?;
    let byte_rate = sample_rate
        .checked_mul(block_align as u32)
        .ok_or_else(|| DecoderError::InvalidData("WAV byte-rate overflow".into()))?;
    let riff_size = 36u32
        .checked_add(data_bytes)
        .ok_or_else(|| DecoderError::InvalidData("WAV RIFF size overflow".into()))?;

    w.write_all(b"RIFF")?;
    w.write_all(&riff_size.to_le_bytes())?;
    w.write_all(b"WAVE")?;
    w.write_all(b"fmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&3u16.to_le_bytes())?; // WAVE_FORMAT_IEEE_FLOAT
    w.write_all(&channels.to_le_bytes())?;
    w.write_all(&sample_rate.to_le_bytes())?;
    w.write_all(&byte_rate.to_le_bytes())?;
    w.write_all(&block_align.to_le_bytes())?;
    w.write_all(&32u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_bytes.to_le_bytes())?;
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
    // Convert YUV420p (BT.601-ish) -> RGB for human inspection only.  The
    // FFmpeg verifier compares the original YUV planes, never this conversion.
    let w = frame.width as usize;
    let h = frame.height as usize;

    let mut rgb = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let yy = frame.y[y * w + x] as i32;
            let uv_idx = (y / 2) * (w / 2) + (x / 2);
            let cb = frame.cb[uv_idx] as i32;
            let cr = frame.cr[uv_idx] as i32;

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
    let mut writer = enc
        .write_header()
        .map_err(|e| DecoderError::InvalidData(format!("PNG header error: {e}")))?;
    writer
        .write_image_data(&rgb)
        .map_err(|e| DecoderError::InvalidData(format!("PNG write error: {e}")))?;
    Ok(())
}
