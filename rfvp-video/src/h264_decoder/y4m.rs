use std::{fmt, io, num, str};

use futures_lite::{io::BufReader, AsyncBufReadExt, AsyncRead, AsyncReadExt};
use num_integer::Integer;

const FILE_MAGICK: &[u8] = b"YUV4MPEG2 ";
const FRAME_MAGICK: &[u8] = b"FRAME";
const TERMINATOR: u8 = 0x0A;
const FIELD_SEP: u8 = b' ';
const RATIO_SEP: u8 = b':';

/// Both encoding and decoding errors.
#[derive(Debug)]
pub enum Error {
    /// End of the file. Technically not an error, but it's easier to process
    /// that way.
    EndOfFile,
    /// Unknown colorspace (possibly just unimplemented).
    UnknownColorspace,
    /// Error while parsing the file/frame header.
    // TODO(Kagami): Better granularity of parse errors.
    ParseError(ParseError),
    /// Error while reading/writing the file.
    IoError(io::Error),
    /// Out of memory (limits exceeded).
    OutOfMemory,
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            Error::EndOfFile => None,
            Error::UnknownColorspace => None,
            Error::ParseError(ref err) => Some(err),
            Error::IoError(ref err) => Some(err),
            Error::OutOfMemory => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::EndOfFile => write!(f, "End of file"),
            Error::UnknownColorspace => write!(f, "Bad input parameters provided"),
            Error::ParseError(ref err) => err.fmt(f),
            Error::IoError(ref err) => err.fmt(f),
            Error::OutOfMemory => write!(f, "Out of memory (limits exceeded)"),
        }
    }
}

/// Granular ParseError Definiations
pub enum ParseError {
    /// Error reading y4m header
    InvalidY4M,
    /// Error parsing int
    Int,
    /// Error parsing UTF8
    Utf8,
    /// General Parsing Error
    General,
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            ParseError::InvalidY4M => None,
            ParseError::Int => None,
            ParseError::Utf8 => None,
            ParseError::General => None,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::InvalidY4M => write!(f, "Error parsing y4m header"),
            ParseError::Int => write!(f, "Error parsing Int"),
            ParseError::Utf8 => write!(f, "Error parsing UTF8"),
            ParseError::General => write!(f, "General parsing error"),
        }
    }
}

impl fmt::Debug for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::InvalidY4M => write!(f, "Error parsing y4m header"),
            ParseError::Int => write!(f, "Error parsing Int"),
            ParseError::Utf8 => write!(f, "Error parsing UTF8"),
            ParseError::General => write!(f, "General parsing error"),
        }
    }
}

macro_rules! parse_error {
    ($p:expr) => {
        return Err(Error::ParseError($p))
    };
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        match err.kind() {
            io::ErrorKind::UnexpectedEof => Error::EndOfFile,
            _ => Error::IoError(err),
        }
    }
}

impl From<num::ParseIntError> for Error {
    fn from(_: num::ParseIntError) -> Error {
        Error::ParseError(ParseError::Int)
    }
}

impl From<str::Utf8Error> for Error {
    fn from(_: str::Utf8Error) -> Error {
        Error::ParseError(ParseError::Utf8)
    }
}

fn parse_bytes(buf: &[u8]) -> Result<u32, Error> {
    // A bit kludgy but seems like there is no other way.
    Ok(str::from_utf8(buf)?.parse()?)
}

/// Simple ratio structure since stdlib lacks one.
#[derive(Debug, Clone, Copy)]
pub struct Ratio {
    /// Numerator.
    pub num: u32,
    /// Denominator.
    pub den: u32,
}

impl Ratio {
    /// Create a new ratio.
    pub fn new(num: u32, den: u32) -> Ratio {
        Ratio { num, den }
    }

    /// Parse a ratio from a byte slice.
    pub fn parse(value: &[u8]) -> Result<Ratio, Error> {
        let parts: Vec<_> = value.splitn(2, |&b| b == RATIO_SEP).collect();
        if parts.len() != 2 {
            parse_error!(ParseError::General)
        }
        let num = parse_bytes(parts[0])?;
        let den = parse_bytes(parts[1])?;
        Ok(Ratio::new(num, den))
    }
}

impl fmt::Display for Ratio {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.num, self.den)
    }
}

/// Colorspace (color model/pixel format). Only subset of them is supported.
///
/// From libavformat/yuv4mpegenc.c:
///
/// > yuv4mpeg can only handle yuv444p, yuv422p, yuv420p, yuv411p and gray8
/// pixel formats. And using 'strict -1' also yuv444p9, yuv422p9, yuv420p9,
/// yuv444p10, yuv422p10, yuv420p10, yuv444p12, yuv422p12, yuv420p12,
/// yuv444p14, yuv422p14, yuv420p14, yuv444p16, yuv422p16, yuv420p16, gray9,
/// gray10, gray12 and gray16 pixel formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Colorspace {
    /// Grayscale only, 8-bit.
    Cmono,
    /// Grayscale only, 12-bit.
    Cmono12,
    /// 4:2:0 with coincident chroma planes, 8-bit.
    C420,
    /// 4:2:0 with coincident chroma planes, 10-bit.
    C420p10,
    /// 4:2:0 with coincident chroma planes, 12-bit.
    C420p12,
    /// 4:2:0 with biaxially-displaced chroma planes, 8-bit.
    C420jpeg,
    /// 4:2:0 with vertically-displaced chroma planes, 8-bit.
    C420paldv,
    /// Found in some files. Same as `C420`.
    C420mpeg2,
    /// 4:2:2, 8-bit.
    C422,
    /// 4:2:2, 10-bit.
    C422p10,
    /// 4:2:2, 12-bit.
    C422p12,
    /// 4:4:4, 8-bit.
    C444,
    /// 4:4:4, 10-bit.
    C444p10,
    /// 4:4:4, 12-bit.
    C444p12,
}

impl Colorspace {
    /// Return the bit depth per sample
    #[inline]
    pub fn get_bit_depth(self) -> usize {
        match self {
            Colorspace::Cmono
            | Colorspace::C420
            | Colorspace::C422
            | Colorspace::C444
            | Colorspace::C420jpeg
            | Colorspace::C420paldv
            | Colorspace::C420mpeg2 => 8,
            Colorspace::C420p10 | Colorspace::C422p10 | Colorspace::C444p10 => 10,
            Colorspace::Cmono12
            | Colorspace::C420p12
            | Colorspace::C422p12
            | Colorspace::C444p12 => 12,
        }
    }

    pub fn get_bits_per_sample(self) -> BitsPerSample {
        match self {
            Colorspace::Cmono
            | Colorspace::C420
            | Colorspace::C422
            | Colorspace::C444
            | Colorspace::C420jpeg
            | Colorspace::C420paldv
            | Colorspace::C420mpeg2 => BitsPerSample::B8,
            Colorspace::C420p10 | Colorspace::C422p10 | Colorspace::C444p10 => BitsPerSample::B10,
            Colorspace::Cmono12
            | Colorspace::C420p12
            | Colorspace::C422p12
            | Colorspace::C444p12 => BitsPerSample::B12,
        }
    }
}

fn get_plane_sizes(width: u32, height: u32, colorspace: Colorspace) -> [PlaneSize; 3] {
    let y_plane_size = PlaneSize::new(width, height, colorspace.get_bits_per_sample());

    let c420_chroma_size = PlaneSize::new(
        Integer::div_ceil(&width, &2),
        Integer::div_ceil(&height, &2),
        colorspace.get_bits_per_sample(),
    );
    let c422_chroma_size = PlaneSize::new(
        Integer::div_ceil(&width, &2),
        height,
        colorspace.get_bits_per_sample(),
    );

    let c420_sizes = [y_plane_size, c420_chroma_size, c420_chroma_size];
    let c422_sizes = [y_plane_size, c422_chroma_size, c422_chroma_size];
    let c444_sizes = [y_plane_size, y_plane_size, y_plane_size];

    match colorspace {
        Colorspace::Cmono | Colorspace::Cmono12 => {
            [y_plane_size, PlaneSize::EMPTY, PlaneSize::EMPTY]
        }
        Colorspace::C420
        | Colorspace::C420p10
        | Colorspace::C420p12
        | Colorspace::C420jpeg
        | Colorspace::C420paldv
        | Colorspace::C420mpeg2 => c420_sizes,
        Colorspace::C422 | Colorspace::C422p10 | Colorspace::C422p12 => c422_sizes,
        Colorspace::C444 | Colorspace::C444p10 | Colorspace::C444p12 => c444_sizes,
    }
}

/// Limits on the resources `Decoder` is allowed to use.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum allowed size of frame buffer, default is 1 GiB.
    pub bytes: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            bytes: 1024 * 1024 * 1024,
        }
    }
}

/// YUV4MPEG2 decoder.
// gstreamer impl does not use YUV4MPEG2, but passes stuff directly in-memory
#[cfg_attr(feature = "gstreamer", allow(unused))]
pub struct Decoder<R: AsyncRead> {
    reader: BufReader<R>,
    params_buf: Vec<u8>,
    frame_buf: Vec<u8>,
    colorspace: Colorspace,
    plane_sizes: [PlaneSize; 3],
}

#[cfg_attr(feature = "gstreamer", allow(unused))]
impl<R: AsyncRead + Unpin> Decoder<R> {
    /// Create a new decoder instance.
    pub async fn new(reader: R) -> Result<Decoder<R>, Error> {
        Decoder::new_with_limits(reader, Limits::default()).await
    }

    /// Create a new decoder instance with custom limits.
    pub async fn new_with_limits(reader: R, limits: Limits) -> Result<Decoder<R>, Error> {
        let mut raw_params = Vec::new();
        let mut buf_reader = BufReader::new(reader);
        let end_params_pos = buf_reader.read_until(TERMINATOR, &mut raw_params).await?;
        if end_params_pos < FILE_MAGICK.len() || !raw_params.starts_with(FILE_MAGICK) {
            parse_error!(ParseError::InvalidY4M)
        }
        let mut width = 0;
        let mut height = 0;
        // Framerate is actually required per spec, but let's be a bit more
        // permissive as per ffmpeg behavior.
        let mut _framerate = Ratio::new(25, 1);
        let mut _pixel_aspect = Ratio::new(1, 1);
        let mut colorspace = None;
        // We shouldn't convert it to string because encoding is unspecified.
        for param in raw_params.split(|&b| b == FIELD_SEP) {
            if param.is_empty() {
                continue;
            }
            let (name, value) = (param[0], &param[1..]);
            // TODO(Kagami): interlacing, comment.
            match name {
                b'W' => width = parse_bytes(value)?,
                b'H' => height = parse_bytes(value)?,
                b'F' => _framerate = Ratio::parse(value)?,
                b'A' => _pixel_aspect = Ratio::parse(value)?,
                b'C' => {
                    colorspace = match value {
                        b"mono" => Some(Colorspace::Cmono),
                        b"mono12" => Some(Colorspace::Cmono12),
                        b"420" => Some(Colorspace::C420),
                        b"420p10" => Some(Colorspace::C420p10),
                        b"420p12" => Some(Colorspace::C420p12),
                        b"422" => Some(Colorspace::C422),
                        b"422p10" => Some(Colorspace::C422p10),
                        b"422p12" => Some(Colorspace::C422p12),
                        b"444" => Some(Colorspace::C444),
                        b"444p10" => Some(Colorspace::C444p10),
                        b"444p12" => Some(Colorspace::C444p12),
                        b"420jpeg" => Some(Colorspace::C420jpeg),
                        b"420paldv" => Some(Colorspace::C420paldv),
                        b"420mpeg2" => Some(Colorspace::C420mpeg2),
                        _ => return Err(Error::UnknownColorspace),
                    }
                }
                _ => {}
            }
        }
        let colorspace = colorspace.unwrap_or(Colorspace::C420);
        if width == 0 || height == 0 {
            parse_error!(ParseError::General)
        }
        let plane_sizes = get_plane_sizes(width, height, colorspace);
        let frame_size = plane_sizes.into_iter().map(|s| s.get_bytes_len()).sum();
        if frame_size > limits.bytes {
            return Err(Error::OutOfMemory);
        }
        let frame_buf = vec![0; frame_size];
        Ok(Decoder {
            reader: buf_reader,
            params_buf: Vec::new(),
            frame_buf,
            colorspace,
            plane_sizes,
        })
    }

    /// Iterate over frames. End of input is indicated by `Error::EOF`.
    pub async fn read_frame(&mut self) -> Result<Frame, Error> {
        self.params_buf.clear();
        self.reader
            .read_until(TERMINATOR, &mut self.params_buf)
            .await?;

        if self.params_buf.is_empty() {
            return Err(Error::EndOfFile);
        }

        self.params_buf.resize(self.params_buf.len() - 1, 0); // remove the terminator
        if self.params_buf.len() < FRAME_MAGICK.len() || !self.params_buf.starts_with(FRAME_MAGICK)
        {
            parse_error!(ParseError::InvalidY4M)
        }
        // We don't parse frame params currently but user has access to them.
        let start_params_pos = FRAME_MAGICK.len();
        let raw_params = if self.params_buf.len() - start_params_pos > 0 {
            // Check for extra space.
            if dbg!(self.params_buf[start_params_pos]) != FIELD_SEP {
                parse_error!(ParseError::InvalidY4M)
            }
            Some(self.params_buf[start_params_pos + 1..].to_owned())
        } else {
            None
        };
        self.reader.read_exact(&mut self.frame_buf).await?;

        let [y_len, u_len, _] = self.plane_sizes.map(|v| v.get_bytes_len());

        Ok(Frame::new(
            [
                self.frame_buf[0..y_len].to_vec(),
                self.frame_buf[y_len..y_len + u_len].to_vec(),
                self.frame_buf[y_len + u_len..].to_vec(),
            ],
            raw_params,
            FrameSize {
                plane_sizes: self.plane_sizes,
                colorspace: self.colorspace,
            },
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BitsPerSample {
    B8,
    B10,
    B12,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlaneSize {
    pub width: u32,
    pub height: u32,
    pub bits_per_sample: BitsPerSample,
}

impl PlaneSize {
    pub const EMPTY: PlaneSize = PlaneSize {
        width: 0,
        height: 0,
        bits_per_sample: BitsPerSample::B8,
    };

    pub fn new(width: u32, height: u32, bits_per_sample: BitsPerSample) -> PlaneSize {
        PlaneSize {
            width,
            height,
            bits_per_sample,
        }
    }

    pub fn get_bytes_per_sample(&self) -> usize {
        match self.bits_per_sample {
            BitsPerSample::B8 => 1,
            BitsPerSample::B10 => 2,
            BitsPerSample::B12 => 2,
        }
    }

    pub fn get_bytes_len(&self) -> usize {
        self.width as usize * self.height as usize * self.get_bytes_per_sample()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct FrameSize {
    pub colorspace: Colorspace,
    pub plane_sizes: [PlaneSize; 3],
}

/// A single frame.
#[derive(Debug)]
pub struct Frame {
    planes: [Vec<u8>; 3],
    raw_params: Option<Vec<u8>>,
    info: FrameSize,
}

impl Frame {
    /// Create a new frame with optional parameters.
    /// No heap allocations are made.
    pub fn new(planes: [Vec<u8>; 3], raw_params: Option<Vec<u8>>, info: FrameSize) -> Frame {
        Frame {
            planes,
            raw_params,
            info,
        }
    }

    /// Return Y (first) plane.
    #[inline]
    pub fn get_y_plane(&self) -> &[u8] {
        self.planes[0].as_ref()
    }
    /// Return U (second) plane. Empty in case of grayscale.
    #[inline]
    pub fn get_u_plane(&self) -> &[u8] {
        self.planes[1].as_ref()
    }
    /// Return V (third) plane. Empty in case of grayscale.
    #[inline]
    pub fn get_v_plane(&self) -> &[u8] {
        self.planes[2].as_ref()
    }
    /// Return frame raw parameters if any.
    #[inline]
    pub fn get_raw_params(&self) -> Option<&[u8]> {
        self.raw_params.as_ref().map(|v| &v[..])
    }

    #[inline]
    pub fn size(&self) -> &FrameSize {
        &self.info
    }
}