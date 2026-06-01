use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::io::Cursor;
use std::io::{Read, Seek, SeekFrom, Write};
use std::num::NonZeroU32;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::{Parser, ValueEnum};
use encoding_rs::SHIFT_JIS;
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use tempfile::NamedTempFile;
use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisDecoder, VorbisEncoderBuilder};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/BIZUDGothic-Regular.ttf");
const FONT_LICENSE: &str = include_str!("../assets/fonts/OFL.txt");

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    platform: Platform,
}

#[derive(Clone, Copy, Debug, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
enum Platform {
    Psp,
    Ps2,
}

#[derive(Clone, Debug, Serialize)]
struct Profile {
    platform: Platform,
    hcb_path: String,
    hcb_game_mode: u8,
    game_mode_reserved: u8,
    design_width: u32,
    design_height: u32,
    target_width: u32,
    target_height: u32,
    scale_num: u32,
    scale_den: u32,
    viewport_x: u32,
    viewport_y: u32,
    viewport_width: u32,
    viewport_height: u32,
}

#[derive(Clone, Debug)]
enum ResourceSource {
    Loose(PathBuf),
    BinEntry {
        pack_path: PathBuf,
        offset: u64,
        size: u64,
    },
}

#[derive(Clone, Debug)]
struct Resource {
    virtual_path: PathBuf,
    source: ResourceSource,
}

#[derive(Clone, Debug)]
struct BinEntry {
    name: String,
    offset: u64,
    size: u64,
}

#[derive(Clone, Debug)]
struct HcbInfo {
    path: PathBuf,
    game_mode: u8,
    game_mode_reserved: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NvsgKind {
    Single24Bit,
    Single32Bit,
    Multi32Bit,
    Single8Bit,
    Single1Bit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OggAction {
    Copy,
    Transcode(u32),
}

#[derive(Clone, Debug)]
struct NvsgHeader {
    width: u16,
    height: u16,
    entry_count: u32,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    rebuild(&cli)
}

fn rebuild(cli: &Cli) -> Result<()> {
    ensure!(
        cli.input.is_dir(),
        "input is not a directory: {}",
        cli.input.display()
    );
    fs::create_dir_all(&cli.output)
        .with_context(|| format!("create output directory {}", cli.output.display()))?;

    let hcb = find_and_parse_hcb(&cli.input)?;
    let profile = make_profile(cli.platform, &hcb)?;
    write_json(&cli.output.join("profile.json"), &profile)?;
    write_rfvp_toml(&cli.output.join("rfvp.toml"), &profile)?;

    let resources = build_resource_table(&cli.input)?;
    resources.values().par_bridge().try_for_each(|resource| {
        process_resource(resource, &cli.output, &profile)
            .with_context(|| format!("process {}", resource.virtual_path.display()))
    })?;

    write_font_assets(&cli.output)?;
    Ok(())
}

fn find_and_parse_hcb(input: &Path) -> Result<HcbInfo> {
    let mut hcbs = Vec::new();
    for entry in fs::read_dir(input).with_context(|| format!("read {}", input.display()))? {
        let entry = entry.with_context(|| format!("read entry in {}", input.display()))?;
        let path = entry.path();
        if path.is_file() && ext_eq(&path, "hcb") {
            hcbs.push(path);
        }
    }

    match hcbs.len() {
        0 => bail!("no root-level .hcb found in {}", input.display()),
        1 => {
            let path = hcbs.remove(0);
            let data = fs::read(&path).with_context(|| format!("read HCB {}", path.display()))?;
            let (game_mode, game_mode_reserved) = parse_hcb_game_mode(&data)
                .with_context(|| format!("parse HCB {}", path.display()))?;
            Ok(HcbInfo {
                path,
                game_mode,
                game_mode_reserved,
            })
        }
        _ => bail!(
            "multiple root-level .hcb files found in {}",
            input.display()
        ),
    }
}

fn parse_hcb_game_mode(data: &[u8]) -> Result<(u8, u8)> {
    let sys_desc = read_u32le_at(data, 0)? as usize;
    let game_mode_off = sys_desc
        .checked_add(8)
        .ok_or_else(|| anyhow!("HCB sys_desc offset overflow"))?;
    let reserved_off = sys_desc
        .checked_add(9)
        .ok_or_else(|| anyhow!("HCB sys_desc offset overflow"))?;
    ensure!(reserved_off < data.len(), "HCB sys_desc is outside file");
    Ok((data[game_mode_off], data[reserved_off]))
}

fn make_profile(platform: Platform, hcb: &HcbInfo) -> Result<Profile> {
    let (design_width, design_height) = design_resolution(hcb.game_mode)?;
    let (target_width, target_height) = match platform {
        Platform::Psp => (480, 272),
        Platform::Ps2 => (640, 448),
    };

    let (scale_num, scale_den) = if target_width as u64 * design_height as u64
        <= target_height as u64 * design_width as u64
    {
        (target_width, design_width)
    } else {
        (target_height, design_height)
    };
    let viewport_width = scale_round(design_width, scale_num, scale_den);
    let viewport_height = scale_round(design_height, scale_num, scale_den);
    let viewport_x = (target_width - viewport_width) / 2;
    let viewport_y = (target_height - viewport_height) / 2;

    Ok(Profile {
        platform,
        hcb_path: hcb.path.display().to_string(),
        hcb_game_mode: hcb.game_mode,
        game_mode_reserved: hcb.game_mode_reserved,
        design_width,
        design_height,
        target_width,
        target_height,
        scale_num,
        scale_den,
        viewport_x,
        viewport_y,
        viewport_width,
        viewport_height,
    })
}

fn design_resolution(game_mode: u8) -> Result<(u32, u32)> {
    match game_mode {
        0 => Ok((640, 480)),
        1 => Ok((800, 600)),
        2 => Ok((1024, 768)),
        3 => Ok((1280, 960)),
        4 => Ok((1600, 1200)),
        5 => Ok((640, 480)),
        6 => Ok((1024, 576)),
        7 => Ok((1024, 640)),
        8 => Ok((1280, 720)),
        9 => Ok((1280, 800)),
        10 => Ok((1440, 810)),
        11 => Ok((1440, 900)),
        12 => Ok((1680, 945)),
        13 => Ok((1680, 1050)),
        14 => Ok((1920, 1080)),
        15 => Ok((1920, 1200)),
        _ => bail!("unknown HCB game_mode: {game_mode}"),
    }
}

fn scale_round(value: u32, scale_num: u32, scale_den: u32) -> u32 {
    let n = value as u64 * scale_num as u64;
    let q = n / scale_den as u64;
    let r = n % scale_den as u64;
    if r * 2 >= scale_den as u64 {
        (q + 1) as u32
    } else {
        q as u32
    }
}

fn write_rfvp_toml(path: &Path, profile: &Profile) -> Result<()> {
    let scale = profile.scale_num as f64 / profile.scale_den as f64;
    write_bytes(path, format!("scale = {scale}\n").as_bytes())
}

fn build_resource_table(input: &Path) -> Result<BTreeMap<PathBuf, Resource>> {
    let mut resources = BTreeMap::new();
    let mut loose_paths = BTreeSet::new();

    for entry in walkdir::WalkDir::new(input) {
        let entry = entry.with_context(|| format!("walk {}", input.display()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(input)
            .with_context(|| format!("strip input prefix from {}", path.display()))?;
        if rel.components().count() == 1 && ext_eq(path, "bin") {
            continue;
        }
        let rel = normalize_relative_path(rel)?;
        loose_paths.insert(rel.clone());
        resources.insert(
            rel.clone(),
            Resource {
                virtual_path: rel,
                source: ResourceSource::Loose(path.to_path_buf()),
            },
        );
    }

    for entry in fs::read_dir(input).with_context(|| format!("read {}", input.display()))? {
        let entry = entry.with_context(|| format!("read entry in {}", input.display()))?;
        let pack_path = entry.path();
        if !pack_path.is_file() || !ext_eq(&pack_path, "bin") {
            continue;
        }
        let stem = pack_path
            .file_stem()
            .and_then(OsStr::to_str)
            .ok_or_else(|| anyhow!("invalid pack file name: {}", pack_path.display()))?;
        for bin_entry in parse_bin_file(&pack_path)? {
            let virtual_path = normalize_relative_path(&Path::new(stem).join(&bin_entry.name))?;
            if loose_paths.contains(&virtual_path) || resources.contains_key(&virtual_path) {
                continue;
            }
            resources.insert(
                virtual_path.clone(),
                Resource {
                    virtual_path,
                    source: ResourceSource::BinEntry {
                        pack_path: pack_path.clone(),
                        offset: bin_entry.offset,
                        size: bin_entry.size,
                    },
                },
            );
        }
    }

    Ok(resources)
}

fn normalize_relative_path(path: &Path) -> Result<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => bail!("unsupported relative path component in {}", path.display()),
        }
    }
    ensure!(!out.as_os_str().is_empty(), "empty relative path");
    Ok(out)
}

fn parse_bin_file(path: &Path) -> Result<Vec<BinEntry>> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .with_context(|| format!("read {}", path.display()))?;
    parse_bin_bytes(&data).with_context(|| format!("parse bin {}", path.display()))
}

fn parse_bin_bytes(data: &[u8]) -> Result<Vec<BinEntry>> {
    let file_count = read_u32le_at(data, 0)? as usize;
    let filename_table_size = read_u32le_at(data, 4)? as usize;
    let entries_offset = 8usize;
    let filename_table_offset = entries_offset
        .checked_add(
            file_count
                .checked_mul(12)
                .ok_or_else(|| anyhow!("bin entry table overflow"))?,
        )
        .ok_or_else(|| anyhow!("bin filename table offset overflow"))?;
    let filename_table_end = filename_table_offset
        .checked_add(filename_table_size)
        .ok_or_else(|| anyhow!("bin filename table overflow"))?;
    ensure!(
        filename_table_end <= data.len(),
        "bin filename table outside file"
    );
    let filename_table = &data[filename_table_offset..filename_table_end];

    let mut entries = Vec::with_capacity(file_count);
    for i in 0..file_count {
        let base = entries_offset + i * 12;
        let name_off = read_u32le_at(data, base)? as usize;
        let data_off = read_u32le_at(data, base + 4)? as u64;
        let data_size = read_u32le_at(data, base + 8)? as u64;
        ensure!(
            name_off < filename_table.len(),
            "bin name offset outside filename table"
        );
        let name_end = filename_table[name_off..]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| anyhow!("bin filename is not NUL-terminated"))?
            + name_off;
        let (name, _, had_errors) = SHIFT_JIS.decode(&filename_table[name_off..name_end]);
        ensure!(!had_errors, "bin filename is not valid Shift-JIS/CP932");
        let name = name.into_owned();
        let end = data_off
            .checked_add(data_size)
            .ok_or_else(|| anyhow!("bin data range overflow"))?;
        ensure!(
            end <= data.len() as u64,
            "bin data range outside file for {name}"
        );
        entries.push(BinEntry {
            name,
            offset: data_off,
            size: data_size,
        });
    }
    Ok(entries)
}

fn process_resource(resource: &Resource, output: &Path, profile: &Profile) -> Result<()> {
    let bytes = read_resource_bytes(&resource.source)?;
    let output_path = output.join(&resource.virtual_path);
    if is_nvsg(&bytes) {
        let converted = convert_nvsg(&bytes, profile)?;
        write_bytes(&output_path, &converted)?;
    } else if is_ogg(&bytes) {
        process_ogg(resource, &bytes, &output_path)?;
    } else if is_video_path(&resource.virtual_path) {
        transcode_video(resource, &bytes, &output_path, profile)?;
    } else {
        write_bytes(&output_path, &bytes)?;
    }
    Ok(())
}

fn read_resource_bytes(source: &ResourceSource) -> Result<Vec<u8>> {
    match source {
        ResourceSource::Loose(path) => {
            fs::read(path).with_context(|| format!("read {}", path.display()))
        }
        ResourceSource::BinEntry {
            pack_path,
            offset,
            size,
        } => {
            let mut file = fs::File::open(pack_path)
                .with_context(|| format!("open {}", pack_path.display()))?;
            file.seek(SeekFrom::Start(*offset))
                .with_context(|| format!("seek {}", pack_path.display()))?;
            let mut data = vec![0; *size as usize];
            file.read_exact(&mut data)
                .with_context(|| format!("read entry from {}", pack_path.display()))?;
            Ok(data)
        }
    }
}

fn process_ogg(resource: &Resource, bytes: &[u8], output_path: &Path) -> Result<()> {
    let input = OggProbeInput::new(&resource.source, bytes)?;
    let bitrate = probe_ogg_bitrate(input.path(), bytes.len(), &resource.virtual_path)?;
    match choose_ogg_action(bitrate) {
        OggAction::Copy => write_bytes(output_path, bytes),
        OggAction::Transcode(target_bitrate) => {
            transcode_ogg_vorbis_rs(bytes, output_path, target_bitrate, &resource.virtual_path)
        }
    }
}

enum OggProbeInput<'a> {
    Loose(&'a Path),
    Temp(NamedTempFile),
}

impl<'a> OggProbeInput<'a> {
    fn new(source: &'a ResourceSource, bytes: &[u8]) -> Result<Self> {
        match source {
            ResourceSource::Loose(path) => Ok(Self::Loose(path)),
            ResourceSource::BinEntry { .. } => {
                let mut file = NamedTempFile::new().context("create temporary OGG input file")?;
                file.write_all(bytes)
                    .context("write temporary OGG input file")?;
                file.flush().context("flush temporary OGG input file")?;
                Ok(Self::Temp(file))
            }
        }
    }

    fn path(&self) -> &Path {
        match self {
            Self::Loose(path) => path,
            Self::Temp(file) => file.path(),
        }
    }
}

fn probe_ogg_bitrate(
    input_path: &Path,
    file_size_bytes: usize,
    virtual_path: &Path,
) -> Result<u64> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=bit_rate,duration",
            "-of",
            "json",
        ])
        .arg(input_path)
        .output()
        .with_context(|| format!("run ffprobe for {}", virtual_path.display()))?;
    ensure!(
        output.status.success(),
        "ffprobe failed for {}:\n{}",
        virtual_path.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("parse ffprobe json for {}", virtual_path.display()))?;
    let format = json.get("format").ok_or_else(|| {
        anyhow!(
            "ffprobe output has no format for {}",
            virtual_path.display()
        )
    })?;

    if let Some(bit_rate) = parse_ffprobe_u64(format.get("bit_rate"))? {
        return Ok(bit_rate);
    }

    let duration = parse_ffprobe_f64(format.get("duration"))?.ok_or_else(|| {
        anyhow!(
            "ffprobe returned no duration for {}",
            virtual_path.display()
        )
    })?;
    ensure!(
        duration.is_finite() && duration > 0.0,
        "invalid ffprobe duration for {}: {duration}",
        virtual_path.display()
    );
    Ok(((file_size_bytes as f64 * 8.0) / duration).round() as u64)
}

fn parse_ffprobe_u64(value: Option<&Value>) -> Result<Option<u64>> {
    match value {
        Some(Value::String(s)) if !s.is_empty() && s != "N/A" => Ok(Some(s.parse()?)),
        Some(Value::Number(n)) => Ok(n.as_u64()),
        _ => Ok(None),
    }
}

fn parse_ffprobe_f64(value: Option<&Value>) -> Result<Option<f64>> {
    match value {
        Some(Value::String(s)) if !s.is_empty() && s != "N/A" => Ok(Some(s.parse()?)),
        Some(Value::Number(n)) => Ok(n.as_f64()),
        _ => Ok(None),
    }
}

fn choose_ogg_action(bitrate_bps: u64) -> OggAction {
    match bitrate_bps {
        0..=40_000 => OggAction::Copy,
        40_001..=100_000 => OggAction::Transcode(40_000),
        _ => OggAction::Transcode(96_000),
    }
}

fn transcode_ogg_vorbis_rs(
    input_bytes: &[u8],
    output_path: &Path,
    target_bitrate: u32,
    virtual_path: &Path,
) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let encoded = encode_ogg_vorbis_48000(input_bytes, target_bitrate)
        .with_context(|| format!("transcode OGG/Vorbis {}", virtual_path.display()))?;
    write_bytes(output_path, &encoded)
}

fn encode_ogg_vorbis_48000(input_bytes: &[u8], target_bitrate: u32) -> Result<Vec<u8>> {
    let mut decoder =
        VorbisDecoder::new(Cursor::new(input_bytes.to_vec())).context("create Vorbis decoder")?;
    let channels = decoder.channels();
    let source_rate = decoder.sampling_frequency().get();
    let mut samples = vec![Vec::<f32>::new(); channels.get() as usize];

    while let Some(block) = decoder
        .decode_audio_block()
        .context("decode Vorbis audio block")?
    {
        for (dst, src) in samples.iter_mut().zip(block.samples()) {
            dst.extend_from_slice(src);
        }
    }

    let samples = if source_rate == 48_000 {
        samples
    } else {
        resample_planar_linear(&samples, source_rate, 48_000)
    };

    let target_bitrate = NonZeroU32::new(target_bitrate)
        .ok_or_else(|| anyhow!("target Vorbis bitrate must be non-zero"))?;
    let mut output = Vec::new();
    {
        let mut builder =
            VorbisEncoderBuilder::new(NonZeroU32::new(48_000).unwrap(), channels, &mut output)
                .context("create Vorbis encoder builder")?;
        builder.bitrate_management_strategy(VorbisBitrateManagementStrategy::Abr {
            average_bitrate: target_bitrate,
        });
        let mut encoder = builder.build().context("create Vorbis encoder")?;
        for start in (0..samples.first().map_or(0, Vec::len)).step_by(2048) {
            let end = (start + 2048).min(samples[0].len());
            let block: Vec<&[f32]> = samples.iter().map(|channel| &channel[start..end]).collect();
            encoder
                .encode_audio_block(block)
                .context("encode Vorbis audio block")?;
        }
        encoder.finish().context("finish Vorbis encoder")?;
    }
    Ok(output)
}

fn resample_planar_linear(
    samples: &[Vec<f32>],
    source_rate: u32,
    target_rate: u32,
) -> Vec<Vec<f32>> {
    if samples.is_empty() || samples[0].is_empty() {
        return vec![Vec::new(); samples.len()];
    }
    let source_len = samples[0].len();
    let target_len = ((source_len as u64 * target_rate as u64 + source_rate as u64 / 2)
        / source_rate as u64) as usize;
    samples
        .iter()
        .map(|channel| resample_channel_linear(channel, source_rate, target_rate, target_len))
        .collect()
}

fn resample_channel_linear(
    samples: &[f32],
    source_rate: u32,
    target_rate: u32,
    target_len: usize,
) -> Vec<f32> {
    if samples.is_empty() || target_len == 0 {
        return Vec::new();
    }
    if samples.len() == 1 {
        return vec![samples[0]; target_len];
    }
    let mut out = Vec::with_capacity(target_len);
    for i in 0..target_len {
        let numerator = i as u64 * source_rate as u64;
        let base = (numerator / target_rate as u64).min(samples.len() as u64 - 1) as usize;
        let frac = (numerator % target_rate as u64) as f32 / target_rate as f32;
        let next = (base + 1).min(samples.len() - 1);
        out.push(samples[base] * (1.0 - frac) + samples[next] * frac);
    }
    out
}

fn transcode_video(
    resource: &Resource,
    input_bytes: &[u8],
    output_path: &Path,
    profile: &Profile,
) -> Result<()> {
    ensure!(
        Command::new("ffmpeg").arg("-version").output().is_ok(),
        "ffmpeg was not found in PATH"
    );
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    let mut final_output = output_path.to_path_buf();
    match profile.platform {
        Platform::Psp => final_output.set_extension("mp4"),
        Platform::Ps2 => final_output.set_extension("mpg"),
    };
    let filter = format!(
        "scale={}:{}:flags=area,pad={}:{}:{}:{}:black,setsar=1",
        profile.viewport_width,
        profile.viewport_height,
        profile.target_width,
        profile.target_height,
        profile.viewport_x,
        profile.viewport_y
    );

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y");
    match &resource.source {
        ResourceSource::Loose(path) => {
            cmd.arg("-i").arg(path);
        }
        ResourceSource::BinEntry { .. } => {
            cmd.arg("-i").arg("pipe:0").stdin(Stdio::piped());
        }
    }
    cmd.arg("-vf").arg(filter);
    match profile.platform {
        Platform::Psp => {
            cmd.args([
                "-c:v",
                "libx264",
                "-profile:v",
                "baseline",
                "-pix_fmt",
                "yuv420p",
            ]);
            cmd.args(["-c:a", "aac"]);
        }
        Platform::Ps2 => {
            cmd.args(["-c:v", "mpeg2video", "-c:a", "mp2", "-f", "mpeg"]);
        }
    }
    let mut child = cmd
        .arg(&final_output)
        .spawn()
        .with_context(|| format!("run ffmpeg for {}", resource.virtual_path.display()))?;
    if matches!(resource.source, ResourceSource::BinEntry { .. }) {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("ffmpeg stdin was not available"))?;
        stdin.write_all(input_bytes).with_context(|| {
            format!("write {} to ffmpeg stdin", resource.virtual_path.display())
        })?;
    }
    let status = child
        .wait()
        .with_context(|| format!("wait for ffmpeg on {}", resource.virtual_path.display()))?;
    ensure!(
        status.success(),
        "ffmpeg failed for {}",
        resource.virtual_path.display()
    );
    Ok(())
}

fn is_nvsg(bytes: &[u8]) -> bool {
    bytes.len() >= 16 && &bytes[0..4] == b"hzc1" && &bytes[12..16] == b"NVSG"
}

fn is_ogg(bytes: &[u8]) -> bool {
    bytes.starts_with(b"OggS")
}

fn is_video_path(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "mpg" | "mpeg" | "mp4" | "avi" | "wmv" | "omv"
            )
        })
        .unwrap_or(false)
}

fn convert_nvsg(bytes: &[u8], profile: &Profile) -> Result<Vec<u8>> {
    ensure!(bytes.len() >= 44, "NVSG file too small");
    ensure!(&bytes[0..4] == b"hzc1", "invalid HZC1 signature");
    let original_length = read_u32le_at(bytes, 4)? as usize;
    let header_length = read_u32le_at(bytes, 8)? as usize;
    let data_buff = &bytes[12..];
    ensure!(data_buff.len() >= 32, "NVSG data too small");
    ensure!(&data_buff[0..4] == b"NVSG", "invalid NVSG signature");
    ensure!(
        header_length >= 32,
        "NVSG header_length is shorter than the NVSG header"
    );
    ensure!(
        header_length <= data_buff.len(),
        "NVSG header_length outside file"
    );

    let raw_kind = read_u16le_at(data_buff, 6)?;
    let kind = match raw_kind {
        0 => NvsgKind::Single24Bit,
        1 => NvsgKind::Single32Bit,
        2 => NvsgKind::Multi32Bit,
        3 => NvsgKind::Single8Bit,
        4 => NvsgKind::Single1Bit,
        _ => bail!("unsupported NVSG type: {raw_kind}"),
    };
    let header = NvsgHeader {
        width: read_u16le_at(data_buff, 8)?,
        height: read_u16le_at(data_buff, 10)?,
        entry_count: read_u32le_at(data_buff, 20)?,
    };
    let effective_entry_count = if header.entry_count == 0 {
        1
    } else {
        header.entry_count
    };

    let depth = match kind {
        NvsgKind::Single24Bit => 3,
        NvsgKind::Single32Bit | NvsgKind::Multi32Bit => 4,
        NvsgKind::Single8Bit | NvsgKind::Single1Bit => 1,
    };
    let expected_len =
        header.width as usize * header.height as usize * depth * effective_entry_count as usize;
    ensure!(
        original_length == expected_len,
        "NVSG original_length mismatch: header says {original_length}, expected {expected_len}"
    );

    let compressed = &data_buff[header_length..];
    let mut decoder = ZlibDecoder::new(compressed);
    let mut payload = vec![0; expected_len];
    decoder
        .read_exact(&mut payload)
        .context("decompress NVSG zlib payload")?;

    let new_w = scale_round(header.width as u32, profile.scale_num, profile.scale_den);
    let new_h = scale_round(header.height as u32, profile.scale_num, profile.scale_den);
    ensure!(new_w > 0 && new_h > 0, "scaled NVSG dimensions became zero");
    ensure!(
        new_w <= u16::MAX as u32 && new_h <= u16::MAX as u32,
        "scaled NVSG dimensions exceed u16"
    );

    let resized = match kind {
        NvsgKind::Single24Bit => resize_frames_area(
            &payload,
            header.width as usize,
            header.height as usize,
            new_w as usize,
            new_h as usize,
            3,
            effective_entry_count as usize,
            false,
        ),
        NvsgKind::Single32Bit | NvsgKind::Multi32Bit => resize_frames_area(
            &payload,
            header.width as usize,
            header.height as usize,
            new_w as usize,
            new_h as usize,
            4,
            effective_entry_count as usize,
            false,
        ),
        NvsgKind::Single8Bit => resize_frames_area(
            &payload,
            header.width as usize,
            header.height as usize,
            new_w as usize,
            new_h as usize,
            1,
            effective_entry_count as usize,
            false,
        ),
        NvsgKind::Single1Bit => resize_frames_area(
            &payload,
            header.width as usize,
            header.height as usize,
            new_w as usize,
            new_h as usize,
            1,
            effective_entry_count as usize,
            true,
        ),
    };

    write_hzc1_nvsg(bytes, header_length, new_w as u16, new_h as u16, &resized)
}

fn write_hzc1_nvsg(
    original: &[u8],
    header_length: usize,
    width: u16,
    height: u16,
    payload: &[u8],
) -> Result<Vec<u8>> {
    ensure!(
        payload.len() <= u32::MAX as usize,
        "resized NVSG payload exceeds u32"
    );
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(payload)
        .context("compress resized NVSG payload")?;
    let compressed = encoder
        .finish()
        .context("finish resized NVSG zlib stream")?;

    let mut out = Vec::with_capacity(12 + header_length + compressed.len());
    out.extend_from_slice(b"hzc1");
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&(header_length as u32).to_le_bytes());
    out.extend_from_slice(&original[12..12 + header_length]);
    out[12 + 8..12 + 10].copy_from_slice(&width.to_le_bytes());
    out[12 + 10..12 + 12].copy_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&compressed);
    Ok(out)
}

fn resize_frames_area(
    input: &[u8],
    src_w: usize,
    src_h: usize,
    dst_w: usize,
    dst_h: usize,
    channels: usize,
    frames: usize,
    threshold_binary_alpha: bool,
) -> Vec<u8> {
    let src_frame_len = src_w * src_h * channels;
    let dst_frame_len = dst_w * dst_h * channels;
    let mut output = vec![0; dst_frame_len * frames];

    for frame in 0..frames {
        let src = &input[frame * src_frame_len..(frame + 1) * src_frame_len];
        let dst = &mut output[frame * dst_frame_len..(frame + 1) * dst_frame_len];
        resize_area(
            src,
            dst,
            src_w,
            src_h,
            dst_w,
            dst_h,
            channels,
            threshold_binary_alpha,
        );
    }

    output
}

fn resize_area(
    src: &[u8],
    dst: &mut [u8],
    src_w: usize,
    src_h: usize,
    dst_w: usize,
    dst_h: usize,
    channels: usize,
    threshold_binary_alpha: bool,
) {
    for dy in 0..dst_h {
        let y0 = dy * src_h;
        let y1 = (dy + 1) * src_h;
        let sy_start = y0 / dst_h;
        let sy_end = y1.div_ceil(dst_h);
        for dx in 0..dst_w {
            let x0 = dx * src_w;
            let x1 = (dx + 1) * src_w;
            let sx_start = x0 / dst_w;
            let sx_end = x1.div_ceil(dst_w);
            let mut sums = vec![0u128; channels];
            let mut total = 0u128;

            for sy in sy_start..sy_end {
                let py0 = sy * dst_h;
                let py1 = (sy + 1) * dst_h;
                let y_overlap = y1.min(py1) - y0.max(py0);
                if y_overlap == 0 {
                    continue;
                }
                for sx in sx_start..sx_end {
                    let px0 = sx * dst_w;
                    let px1 = (sx + 1) * dst_w;
                    let x_overlap = x1.min(px1) - x0.max(px0);
                    if x_overlap == 0 {
                        continue;
                    }
                    let weight = (x_overlap * y_overlap) as u128;
                    let src_i = (sy * src_w + sx) * channels;
                    total += weight;
                    for c in 0..channels {
                        if threshold_binary_alpha {
                            if src[src_i + c] != 0 {
                                sums[c] += weight;
                            }
                        } else {
                            sums[c] += src[src_i + c] as u128 * weight;
                        }
                    }
                }
            }

            let dst_i = (dy * dst_w + dx) * channels;
            for c in 0..channels {
                dst[dst_i + c] = if threshold_binary_alpha {
                    if sums[c] * 2 >= total { 1 } else { 0 }
                } else {
                    let value = ((sums[c] * 2 + total) / (total * 2)) as u8;
                    value
                };
            }
        }
    }
}

fn write_font_assets(output: &Path) -> Result<()> {
    ensure!(!FONT_BYTES.is_empty(), "embedded font asset is empty");
    ensure!(
        !FONT_LICENSE.is_empty(),
        "embedded font license asset is empty"
    );
    let tmap = rfvp_bitmap::build_japanese_bitmap_font_from_ttf(
        FONT_BYTES,
        &rfvp_bitmap::BitmapFontBuildOptions::default(),
    )
    .context("build RFVPTMAP bitmap font")?;
    write_bytes(&output.join("defualt.tmap"), &tmap)?;
    write_bytes(&output.join("FONT_LICENSE.txt"), FONT_LICENSE.as_bytes())?;
    Ok(())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).context("serialize json")?;
    write_bytes(path, &bytes)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
}

fn ext_eq(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|ext| ext.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn read_u16le_at(data: &[u8], offset: usize) -> Result<u16> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| anyhow!("read u16 outside buffer at {offset}"))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32le_at(data: &[u8], offset: usize) -> Result<u32> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| anyhow!("read u32 outside buffer at {offset}"))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hcb_game_mode_parser_reads_sys_desc_fields() {
        let mut data = vec![0u8; 32];
        data[0..4].copy_from_slice(&16u32.to_le_bytes());
        data[16..20].copy_from_slice(&123u32.to_le_bytes());
        data[20..22].copy_from_slice(&1u16.to_le_bytes());
        data[22..24].copy_from_slice(&2u16.to_le_bytes());
        data[24] = 8;
        data[25] = 77;
        let (game_mode, reserved) = parse_hcb_game_mode(&data).unwrap();
        assert_eq!(game_mode, 8);
        assert_eq!(reserved, 77);
    }

    #[test]
    fn scale_round_uses_rational_rounding() {
        let psp = make_profile(
            Platform::Psp,
            &HcbInfo {
                path: PathBuf::from("game.hcb"),
                game_mode: 8,
                game_mode_reserved: 0,
            },
        )
        .unwrap();
        assert_eq!((psp.scale_num, psp.scale_den), (480, 1280));
        assert_eq!(scale_round(1280, psp.scale_num, psp.scale_den), 480);
        assert_eq!(scale_round(720, psp.scale_num, psp.scale_den), 270);

        let ps2 = make_profile(
            Platform::Ps2,
            &HcbInfo {
                path: PathBuf::from("game.hcb"),
                game_mode: 0,
                game_mode_reserved: 0,
            },
        )
        .unwrap();
        assert_eq!((ps2.scale_num, ps2.scale_den), (448, 480));
        assert_eq!(scale_round(480, ps2.scale_num, ps2.scale_den), 448);
    }

    #[test]
    fn bin_parser_reads_one_file() {
        let payload = b"hello";
        let filename = b"foo.nvsg\0";
        let file_count = 1u32;
        let filename_table_size = filename.len() as u32;
        let data_off = 8 + 12 + filename.len() as u32;
        let mut data = Vec::new();
        data.extend_from_slice(&file_count.to_le_bytes());
        data.extend_from_slice(&filename_table_size.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&data_off.to_le_bytes());
        data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        data.extend_from_slice(filename);
        data.extend_from_slice(payload);

        let entries = parse_bin_bytes(&data).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "foo.nvsg");
        assert_eq!(entries[0].offset, data_off as u64);
        assert_eq!(entries[0].size, payload.len() as u64);
    }

    #[test]
    fn bin_parser_decodes_shift_jis_filename() {
        let payload = b"hello";
        let filename = [
            0x83, 0x74, 0x83, 0x40, 0x83, 0x43, 0x83, 0x8b, 0x2e, 0x6e, 0x76, 0x73, 0x67, 0x00,
        ];
        let file_count = 1u32;
        let filename_table_size = filename.len() as u32;
        let data_off = 8 + 12 + filename.len() as u32;
        let mut data = Vec::new();
        data.extend_from_slice(&file_count.to_le_bytes());
        data.extend_from_slice(&filename_table_size.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&data_off.to_le_bytes());
        data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        data.extend_from_slice(&filename);
        data.extend_from_slice(payload);

        let entries = parse_bin_bytes(&data).unwrap();
        assert_eq!(entries[0].name, "ファイル.nvsg");
    }

    #[test]
    fn oggs_magic_detects_ogg_without_extension() {
        assert!(is_ogg(b"OggSexample"));
        assert!(!is_ogg(b"not ogg"));
    }

    #[test]
    fn ogg_bitrate_thresholds_choose_expected_action() {
        assert_eq!(choose_ogg_action(100_001), OggAction::Transcode(96_000));
        assert_eq!(choose_ogg_action(100_000), OggAction::Transcode(40_000));
        assert_eq!(choose_ogg_action(40_001), OggAction::Transcode(40_000));
        assert_eq!(choose_ogg_action(40_000), OggAction::Copy);
    }

    #[test]
    fn ogg_resampler_outputs_48000hz_shape_and_keeps_channels() {
        let samples = vec![vec![0.0, 1.0], vec![1.0, 0.0]];
        let out = resample_planar_linear(&samples, 24_000, 48_000);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].len(), 4);
        assert_eq!(out[1].len(), 4);
        assert_eq!(out[0][0], 0.0);
        assert!(out[0][1] > 0.0 && out[0][1] < 1.0);
    }

    #[test]
    fn font_assets_write_only_tmap_and_license() {
        let dir = tempfile::tempdir().unwrap();
        write_font_assets(dir.path()).unwrap();

        let tmap_path = dir.path().join("defualt.tmap");
        let license_path = dir.path().join("FONT_LICENSE.txt");
        assert!(tmap_path.exists());
        assert!(license_path.exists());
        assert!(!dir.path().join("defualt.tmap.json").exists());

        let tmap = std::fs::read(tmap_path).unwrap();
        let font = rfvp_bitmap::BitmapFont::parse(&tmap).unwrap();
        assert_ne!(font.lookup_glyph_index(0x3042), font.fallback_index());
        assert_ne!(font.lookup_glyph_index(0x30a2), font.fallback_index());
        assert_eq!(font.lookup_glyph_index(0x10ffff), font.fallback_index());
    }

    #[test]
    fn single1bit_resize_preserves_binary_coverage() {
        let input = [1u8, 1, 0, 0];
        let output = resize_frames_area(&input, 2, 2, 1, 1, 1, 1, true);
        assert_eq!(output, vec![1]);

        let input = [1u8, 0, 0, 0];
        let output = resize_frames_area(&input, 2, 2, 1, 1, 1, 1, true);
        assert_eq!(output, vec![0]);
    }

    #[test]
    fn nvsg_conversion_preserves_hzc1_nvsg_format_and_extra_header() {
        let payload = [1u8, 1, 0, 0];
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut input = Vec::new();
        input.extend_from_slice(b"hzc1");
        input.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        input.extend_from_slice(&36u32.to_le_bytes());
        input.extend_from_slice(b"NVSG");
        input.extend_from_slice(&9u16.to_le_bytes());
        input.extend_from_slice(&4u16.to_le_bytes());
        input.extend_from_slice(&2u16.to_le_bytes());
        input.extend_from_slice(&2u16.to_le_bytes());
        input.extend_from_slice(&11u16.to_le_bytes());
        input.extend_from_slice(&12u16.to_le_bytes());
        input.extend_from_slice(&13u16.to_le_bytes());
        input.extend_from_slice(&14u16.to_le_bytes());
        input.extend_from_slice(&0u32.to_le_bytes());
        input.extend_from_slice(&15u32.to_le_bytes());
        input.extend_from_slice(&16u32.to_le_bytes());
        input.extend_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd]);
        input.extend_from_slice(&compressed);

        let profile = Profile {
            platform: Platform::Psp,
            hcb_path: "game.hcb".to_string(),
            hcb_game_mode: 0,
            game_mode_reserved: 0,
            design_width: 640,
            design_height: 480,
            target_width: 320,
            target_height: 240,
            scale_num: 1,
            scale_den: 2,
            viewport_x: 0,
            viewport_y: 0,
            viewport_width: 320,
            viewport_height: 240,
        };

        let output = convert_nvsg(&input, &profile).unwrap();
        assert_eq!(&output[0..4], b"hzc1");
        assert_eq!(read_u32le_at(&output, 4).unwrap(), 1);
        assert_eq!(read_u32le_at(&output, 8).unwrap(), 36);
        assert_eq!(&output[12..16], b"NVSG");
        assert_eq!(read_u16le_at(&output, 18).unwrap(), 4);
        assert_eq!(read_u16le_at(&output, 20).unwrap(), 1);
        assert_eq!(read_u16le_at(&output, 22).unwrap(), 1);
        assert_eq!(read_u16le_at(&output, 24).unwrap(), 11);
        assert_eq!(read_u16le_at(&output, 26).unwrap(), 12);
        assert_eq!(read_u16le_at(&output, 28).unwrap(), 13);
        assert_eq!(read_u16le_at(&output, 30).unwrap(), 14);
        assert_eq!(read_u32le_at(&output, 32).unwrap(), 0);
        assert_eq!(read_u32le_at(&output, 36).unwrap(), 15);
        assert_eq!(read_u32le_at(&output, 40).unwrap(), 16);
        assert_eq!(&output[44..48], &[0xaa, 0xbb, 0xcc, 0xdd]);
        let mut decoder = ZlibDecoder::new(&output[48..]);
        let mut resized = Vec::new();
        decoder.read_to_end(&mut resized).unwrap();
        assert_eq!(resized, vec![1]);
    }
}
