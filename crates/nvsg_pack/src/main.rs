use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use image::{DynamicImage, GenericImageView, GrayAlphaImage, ImageBuffer, LumaA, Rgba, RgbaImage};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const HZC1_SIGNATURE: [u8; 4] = *b"hzc1";
const NVSG_SIGNATURE: [u8; 4] = *b"NVSG";
const NVSG_HEADER_LEN: u32 = 32;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Pack one or more images into an HZC1+NVSG texture.
    Pack(PackArgs),
    /// Print metadata about an existing HZC1+NVSG texture.
    Inspect(InspectArgs),
    /// Decode an HZC1+NVSG texture back into one or more PNG files.
    Unpack(UnpackArgs),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum NvsgTypeArg {
    Single24,
    Single32,
    Multi32,
    Single8,
    Single1,
}

impl NvsgTypeArg {
    fn code(self) -> u16 {
        match self {
            Self::Single24 => 0,
            Self::Single32 => 1,
            Self::Multi32 => 2,
            Self::Single8 => 3,
            Self::Single1 => 4,
        }
    }

    fn depth(self) -> usize {
        match self {
            Self::Single24 => 3,
            Self::Single32 | Self::Multi32 => 4,
            Self::Single8 | Self::Single1 => 1,
        }
    }

    fn is_multi(self) -> bool {
        matches!(self, Self::Multi32)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum MaskSourceArg {
    Alpha,
    Luma,
    Red,
    Green,
    Blue,
}

#[derive(Args, Debug)]
struct PackArgs {
    /// Output .nvsg file path.
    #[arg(short, long)]
    output: PathBuf,

    /// Target NVSG texture type.
    #[arg(short = 't', long = "type", value_enum)]
    texture_type: NvsgTypeArg,

    /// Input image files. Use multiple files for multi32.
    #[arg(required = true)]
    inputs: Vec<PathBuf>,

    #[arg(long, default_value_t = 0)]
    offset_x: u16,
    #[arg(long, default_value_t = 0)]
    offset_y: u16,
    #[arg(long, default_value_t = 0)]
    u: u16,
    #[arg(long, default_value_t = 0)]
    v: u16,
    #[arg(long, default_value_t = 0)]
    unknown1: u16,
    #[arg(long, default_value_t = 0)]
    unknown3: u32,
    #[arg(long, default_value_t = 0)]
    unknown4: u32,

    /// zlib compression level 0..=9.
    #[arg(long, default_value_t = 6)]
    compression_level: u32,

    /// Source channel for 8-bit / 1-bit textures.
    #[arg(long, value_enum, default_value_t = MaskSourceArg::Alpha)]
    mask_source: MaskSourceArg,

    /// Threshold for single1 textures. Values >= threshold become 1.
    #[arg(long, default_value_t = 128)]
    one_bit_threshold: u8,
}

#[derive(Args, Debug)]
struct InspectArgs {
    input: PathBuf,
}

#[derive(Args, Debug)]
struct UnpackArgs {
    input: PathBuf,

    /// Output path for single-frame textures, or output directory for multi-frame textures.
    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum NvsgType {
    Single24Bit,
    Single32Bit,
    Multi32Bit,
    Single8Bit,
    Single1Bit,
}

impl TryFrom<u16> for NvsgType {
    type Error = anyhow::Error;

    fn try_from(value: u16) -> Result<Self> {
        Ok(match value {
            0 => Self::Single24Bit,
            1 => Self::Single32Bit,
            2 => Self::Multi32Bit,
            3 => Self::Single8Bit,
            4 => Self::Single1Bit,
            _ => bail!("invalid NVSG type: {value}"),
        })
    }
}

impl NvsgType {
    fn depth(self) -> usize {
        match self {
            Self::Single24Bit => 3,
            Self::Single32Bit | Self::Multi32Bit => 4,
            Self::Single8Bit | Self::Single1Bit => 1,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Single24Bit => "single24",
            Self::Single32Bit => "single32",
            Self::Multi32Bit => "multi32",
            Self::Single8Bit => "single8",
            Self::Single1Bit => "single1",
        }
    }
}

#[derive(Debug, Clone)]
struct NvsgHeader {
    unknown1: u16,
    typ: NvsgType,
    width: u16,
    height: u16,
    offset_x: u16,
    offset_y: u16,
    u: u16,
    v: u16,
    entry_count: u32,
    unknown3: u32,
    unknown4: u32,
}

#[derive(Debug, Clone)]
struct NvsgFile {
    header: NvsgHeader,
    original_length: u32,
    compressed_payload: Vec<u8>,
    frames: Vec<Vec<u8>>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Pack(args) => pack_command(args),
        Command::Inspect(args) => inspect_command(args),
        Command::Unpack(args) => unpack_command(args),
    }
}

fn pack_command(args: PackArgs) -> Result<()> {
    validate_pack_args(&args)?;

    let images = load_images(&args.inputs)?;
    let (width, height) = common_dimensions(&images)?;
    let entry_count = if args.texture_type.is_multi() {
        images.len() as u32
    } else {
        1
    };

    let mut payload = Vec::with_capacity(
        width as usize * height as usize * args.texture_type.depth() * entry_count as usize,
    );
    for image in &images {
        encode_frame(&mut payload, image, &args)?;
    }

    let compressed_payload = compress_payload(&payload, args.compression_level)?;
    let bytes = build_file_bytes(&args, width, height, entry_count, &payload, &compressed_payload)?;

    if let Some(parent) = args.output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create output directory {}", parent.display()))?;
        }
    }
    fs::write(&args.output, &bytes)
        .with_context(|| format!("failed to write {}", args.output.display()))?;

    eprintln!(
        "packed {} frame(s), {}x{}, type={}, payload={} bytes, compressed={} bytes -> {}",
        entry_count,
        width,
        height,
        args.texture_type.code(),
        payload.len(),
        compressed_payload.len(),
        args.output.display()
    );
    Ok(())
}

fn inspect_command(args: InspectArgs) -> Result<()> {
    let bytes = fs::read(&args.input)
        .with_context(|| format!("failed to read {}", args.input.display()))?;
    let file = decode_nvsg(&bytes)?;
    println!("path: {}", args.input.display());
    println!("type: {}", file.header.typ.name());
    println!("width: {}", file.header.width);
    println!("height: {}", file.header.height);
    println!("offset_x: {}", file.header.offset_x);
    println!("offset_y: {}", file.header.offset_y);
    println!("u: {}", file.header.u);
    println!("v: {}", file.header.v);
    println!("entry_count: {}", file.header.entry_count);
    println!("unknown1: {}", file.header.unknown1);
    println!("unknown3: {}", file.header.unknown3);
    println!("unknown4: {}", file.header.unknown4);
    println!("original_length: {}", file.original_length);
    println!("compressed_payload_length: {}", file.compressed_payload.len());
    Ok(())
}

fn unpack_command(args: UnpackArgs) -> Result<()> {
    let bytes = fs::read(&args.input)
        .with_context(|| format!("failed to read {}", args.input.display()))?;
    let file = decode_nvsg(&bytes)?;
    let is_multi = matches!(file.header.typ, NvsgType::Multi32Bit);
    if is_multi {
        fs::create_dir_all(&args.output)
            .with_context(|| format!("failed to create {}", args.output.display()))?;
        for (index, frame) in file.frames.iter().enumerate() {
            let image = decode_frame_to_image(&file.header, frame)?;
            let output = args.output.join(format!("frame_{index:04}.png"));
            image.save(&output)
                .with_context(|| format!("failed to write {}", output.display()))?;
        }
    } else {
        if let Some(parent) = args.output.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
        }
        let frame = file.frames.first().ok_or_else(|| anyhow!("decoded file has no frames"))?;
        let image = decode_frame_to_image(&file.header, frame)?;
        image.save(&args.output)
            .with_context(|| format!("failed to write {}", args.output.display()))?;
    }
    Ok(())
}

fn validate_pack_args(args: &PackArgs) -> Result<()> {
    if args.inputs.is_empty() {
        bail!("at least one input image is required");
    }
    if args.compression_level > 9 {
        bail!("compression level must be 0..=9");
    }
    match args.texture_type {
        NvsgTypeArg::Multi32 => {}
        _ if args.inputs.len() > 1 => {
            bail!("multiple input images are only valid for --type multi32");
        }
        _ => {}
    }
    Ok(())
}

fn load_images(paths: &[PathBuf]) -> Result<Vec<DynamicImage>> {
    paths
        .iter()
        .map(|path| image::open(path).with_context(|| format!("failed to read image {}", path.display())))
        .collect()
}

fn common_dimensions(images: &[DynamicImage]) -> Result<(u16, u16)> {
    let first = images.first().ok_or_else(|| anyhow!("no input images"))?;
    let (width, height) = first.dimensions();
    if width == 0 || height == 0 {
        bail!("image dimensions must be non-zero");
    }
    if width > u16::MAX as u32 || height > u16::MAX as u32 {
        bail!("image dimensions exceed NVSG u16 limits: {}x{}", width, height);
    }
    for image in &images[1..] {
        let dims = image.dimensions();
        if dims != (width, height) {
            bail!("all frames must have identical dimensions, got {:?} vs {:?}", dims, (width, height));
        }
    }
    Ok((width as u16, height as u16))
}

fn encode_frame(payload: &mut Vec<u8>, image: &DynamicImage, args: &PackArgs) -> Result<()> {
    match args.texture_type {
        NvsgTypeArg::Single24 => {
            for pixel in image.to_rgba8().pixels() {
                payload.push(pixel[2]);
                payload.push(pixel[1]);
                payload.push(pixel[0]);
            }
        }
        NvsgTypeArg::Single32 | NvsgTypeArg::Multi32 => {
            for pixel in image.to_rgba8().pixels() {
                let [r, g, b, a] = pixel.0;
                let pr = premul(r, a);
                let pg = premul(g, a);
                let pb = premul(b, a);
                payload.push(pb);
                payload.push(pg);
                payload.push(pr);
                payload.push(a);
            }
        }
        NvsgTypeArg::Single8 => {
            for pixel in image.to_rgba8().pixels() {
                payload.push(mask_value(pixel.0, args.mask_source));
            }
        }
        NvsgTypeArg::Single1 => {
            for pixel in image.to_rgba8().pixels() {
                let value = mask_value(pixel.0, args.mask_source);
                payload.push(if value >= args.one_bit_threshold { 1 } else { 0 });
            }
        }
    }
    Ok(())
}

fn premul(channel: u8, alpha: u8) -> u8 {
    ((channel as u16 * alpha as u16 + 127) / 255) as u8
}

fn mask_value(pixel: [u8; 4], source: MaskSourceArg) -> u8 {
    match source {
        MaskSourceArg::Alpha => pixel[3],
        MaskSourceArg::Luma => {
            let r = pixel[0] as u16;
            let g = pixel[1] as u16;
            let b = pixel[2] as u16;
            ((r * 299 + g * 587 + b * 114 + 500) / 1000) as u8
        }
        MaskSourceArg::Red => pixel[0],
        MaskSourceArg::Green => pixel[1],
        MaskSourceArg::Blue => pixel[2],
    }
}

fn compress_payload(payload: &[u8], level: u32) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(payload)?;
    Ok(encoder.finish()?)
}

fn build_file_bytes(
    args: &PackArgs,
    width: u16,
    height: u16,
    entry_count: u32,
    payload: &[u8],
    compressed_payload: &[u8],
) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(12 + NVSG_HEADER_LEN as usize + compressed_payload.len());
    bytes.extend_from_slice(&HZC1_SIGNATURE);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&NVSG_HEADER_LEN.to_le_bytes());

    bytes.extend_from_slice(&NVSG_SIGNATURE);
    bytes.extend_from_slice(&args.unknown1.to_le_bytes());
    bytes.extend_from_slice(&args.texture_type.code().to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&args.offset_x.to_le_bytes());
    bytes.extend_from_slice(&args.offset_y.to_le_bytes());
    bytes.extend_from_slice(&args.u.to_le_bytes());
    bytes.extend_from_slice(&args.v.to_le_bytes());
    bytes.extend_from_slice(&entry_count.to_le_bytes());
    bytes.extend_from_slice(&args.unknown3.to_le_bytes());
    bytes.extend_from_slice(&args.unknown4.to_le_bytes());

    if bytes.len() != (12 + NVSG_HEADER_LEN as usize) {
        bail!("internal error: unexpected header size {}", bytes.len());
    }

    bytes.extend_from_slice(compressed_payload);
    Ok(bytes)
}

fn decode_nvsg(bytes: &[u8]) -> Result<NvsgFile> {
    if bytes.len() < 12 {
        bail!("file too small for HZC1 header");
    }
    if bytes[0..4] != HZC1_SIGNATURE {
        bail!("invalid HZC1 signature");
    }
    let original_length = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let header_length = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;

    if bytes.len() < 12 + header_length {
        bail!("file too small for NVSG header");
    }
    let header_bytes = &bytes[12..12 + header_length];
    if header_bytes.len() < NVSG_HEADER_LEN as usize {
        bail!("NVSG header shorter than 32 bytes");
    }
    if header_bytes[0..4] != NVSG_SIGNATURE {
        bail!("invalid NVSG signature");
    }

    let typ = NvsgType::try_from(u16::from_le_bytes(header_bytes[6..8].try_into().unwrap()))?;
    let header = NvsgHeader {
        unknown1: u16::from_le_bytes(header_bytes[4..6].try_into().unwrap()),
        typ,
        width: u16::from_le_bytes(header_bytes[8..10].try_into().unwrap()),
        height: u16::from_le_bytes(header_bytes[10..12].try_into().unwrap()),
        offset_x: u16::from_le_bytes(header_bytes[12..14].try_into().unwrap()),
        offset_y: u16::from_le_bytes(header_bytes[14..16].try_into().unwrap()),
        u: u16::from_le_bytes(header_bytes[16..18].try_into().unwrap()),
        v: u16::from_le_bytes(header_bytes[18..20].try_into().unwrap()),
        entry_count: {
            let count = u32::from_le_bytes(header_bytes[20..24].try_into().unwrap());
            if count == 0 { 1 } else { count }
        },
        unknown3: u32::from_le_bytes(header_bytes[24..28].try_into().unwrap()),
        unknown4: u32::from_le_bytes(header_bytes[28..32].try_into().unwrap()),
    };

    let compressed_payload = bytes[12 + header_length..].to_vec();
    let mut decoder = ZlibDecoder::new(compressed_payload.as_slice());
    let mut payload = vec![0; original_length as usize];
    decoder.read_exact(&mut payload)?;

    let frame_len = header.width as usize * header.height as usize * header.typ.depth();
    if frame_len == 0 {
        bail!("invalid zero-sized frame");
    }
    let expected_len = frame_len
        .checked_mul(header.entry_count as usize)
        .ok_or_else(|| anyhow!("payload length overflow"))?;
    if payload.len() < expected_len {
        bail!(
            "decoded payload too short: got {}, need {} for {} frame(s)",
            payload.len(),
            expected_len,
            header.entry_count
        );
    }

    let mut frames = Vec::with_capacity(header.entry_count as usize);
    for index in 0..header.entry_count as usize {
        let start = index * frame_len;
        let end = start + frame_len;
        frames.push(payload[start..end].to_vec());
    }

    Ok(NvsgFile {
        header,
        original_length,
        compressed_payload,
        frames,
    })
}

fn decode_frame_to_image(header: &NvsgHeader, frame: &[u8]) -> Result<DynamicImage> {
    let width = header.width as u32;
    let height = header.height as u32;
    Ok(match header.typ {
        NvsgType::Single24Bit => {
            let mut image = RgbaImage::new(width, height);
            for (idx, pixel) in image.pixels_mut().enumerate() {
                let base = idx * 3;
                let b = frame[base];
                let g = frame[base + 1];
                let r = frame[base + 2];
                *pixel = Rgba([r, g, b, 255]);
            }
            DynamicImage::ImageRgba8(image)
        }
        NvsgType::Single32Bit | NvsgType::Multi32Bit => {
            let mut image = RgbaImage::new(width, height);
            for (idx, pixel) in image.pixels_mut().enumerate() {
                let base = idx * 4;
                let pb = frame[base];
                let pg = frame[base + 1];
                let pr = frame[base + 2];
                let a = frame[base + 3];
                let (r, g, b) = unpremul_rgb(pr, pg, pb, a);
                *pixel = Rgba([r, g, b, a]);
            }
            DynamicImage::ImageRgba8(image)
        }
        NvsgType::Single8Bit => {
            let mut image: GrayAlphaImage = ImageBuffer::new(width, height);
            for (idx, pixel) in image.pixels_mut().enumerate() {
                let a = frame[idx];
                *pixel = LumaA([255, a]);
            }
            DynamicImage::ImageLumaA8(image)
        }
        NvsgType::Single1Bit => {
            let mut image: GrayAlphaImage = ImageBuffer::new(width, height);
            for (idx, pixel) in image.pixels_mut().enumerate() {
                let a = if frame[idx] != 0 { 255 } else { 0 };
                *pixel = LumaA([255, a]);
            }
            DynamicImage::ImageLumaA8(image)
        }
    })
}

fn unpremul_rgb(pr: u8, pg: u8, pb: u8, a: u8) -> (u8, u8, u8) {
    if a == 0 {
        return (0, 0, 0);
    }
    let r = ((pr as u32 * 255 + (a as u32 / 2)) / a as u32).min(255) as u8;
    let g = ((pg as u32 * 255 + (a as u32 / 2)) / a as u32).min(255) as u8;
    let b = ((pb as u32 * 255 + (a as u32 / 2)) / a as u32).min(255) as u8;
    (r, g, b)
}
