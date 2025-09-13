use std::{
    fmt,
    io::{Error as IoError, Read, Seek}, time::Instant,
};

use byteorder::{LittleEndian, ReadBytesExt};
use ico::IconDir;
use riff::{Chunk, ChunkId, LIST_ID};
use bitflags::bitflags;
use std::time::Duration;

use winit::window::{CustomCursor, CustomCursorSource};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnimatedCursorMetadata {
    /// The header size in bytes.
    header_size_bytes: u32,
    /// The number of frames in the animation.
    pub frame_count: u32,
    /// The number of steps in the animation. May include duplicate frames.
    /// Equals `frame_count`, if no 'seq '-chunk is present.
    pub step_count: u32,
    /// The frame width in pixels.
    pub width: u32,
    /// The frame height in pixels.
    pub height: u32,
    /// The number of bits/pixel. `color_depth = 2 * bit_count`.
    pub bit_count: u32,
    /// The number of planes.
    pub plane_count: u32,
    /// The number of ticks per frame where a "tick" equals 1/60th of a second.
    ///
    /// To calculate the duration of each frame in milliseconds:
    ///
    /// ```rust
    /// let ticks_per_frame = 10;
    /// let duration = 1000.0 * ticks_per_frame as f32 / 60.0;
    /// assert_eq!(duration, 166.66666666666666); // per frame
    /// ```
    pub ticks_per_frame: u32,
    /// The animation flags.
    pub flags: AnimatedCursorFlags,
}

bitflags! {
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub struct AnimatedCursorFlags: u32 {
        const NONE = 0;
        /// If set, frames are icon or cursor data.
        ///
        /// If not set, frames are raw data.
        const ICON_OR_CURSOR_DATA = 1 << 0;
        /// If set, the 'seq '-chunk is present.
        const HAS_SEQUENCE_CHUNK = 1 << 1;
    }
}


impl AnimatedCursorMetadata {
    #[inline(always)]
    pub fn duration_per_frame(&self) -> Duration {
        Duration::from_secs_f32(self.ticks_per_frame as f32 / 60.0)
    }
}

#[derive(Clone)]
pub struct AnimatedCursor {
    pub metadata: AnimatedCursorMetadata,
    pub frames: Vec<IconDir>,
}

#[derive(Debug)]
pub enum DecodeError {
    IoError(IoError),
    UnsupportedRootChunkId(ChunkId),
    UnsupportedRootType(ChunkId),
    InvalidHeaderFlags,
    MissingHeaderChunk,
    MissingFramesChunk,
    UnsupportedFrameChunkId(ChunkId),
    UnsupportedRawDataFrameType,
}

impl std::error::Error for DecodeError {}

impl From<IoError> for DecodeError {
    fn from(error: IoError) -> Self {
        DecodeError::IoError(error)
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::IoError(e) => write!(f, "IO error: {}", e),
            DecodeError::UnsupportedRootChunkId(id) => {
                write!(f, "unsupported root chunk ID: {:?} (expected 'RIFF')", id)
            }
            DecodeError::UnsupportedRootType(id) => {
                write!(f, "unsupported root type: {:?} (expected 'ACON')", id)
            }
            DecodeError::InvalidHeaderFlags => write!(f, "invalid header flags"),
            DecodeError::MissingHeaderChunk => write!(f, "missing header chunk ('anih')"),
            DecodeError::MissingFramesChunk => write!(f, "missing frames chunk ('fram')"),
            DecodeError::UnsupportedFrameChunkId(id) => {
                write!(f, "unsupported frame chunk ID: {:?} (expected 'icon')", id)
            }
            DecodeError::UnsupportedRawDataFrameType => {
                write!(f, "unsupported raw data frame type")
            }
        }
    }
}

pub struct Decoder<R>
where
    R: Read + Seek,
{
    reader: R,
}

fn read_chunks<T>(iter: &mut riff::Iter<T>) -> std::io::Result<Vec<Chunk>>
where
    T: Read + Seek,
{
    let mut vec: Vec<Chunk> = Vec::new();
    for item in iter {
        match item {
            Ok(chunk) => vec.push(chunk),
            Err(e) => return Err(e),
        }
    }
    Ok(vec)
}

impl<R: Read + Seek> Decoder<R> {
    pub fn new(reader: R) -> Self {
        Decoder { reader }
    }

    pub fn decode(&mut self) -> Result<AnimatedCursor, DecodeError> {
        const fn chunk_id(value: &[u8; 4]) -> ChunkId {
            ChunkId { value: *value }
        }

        let chunk = riff::Chunk::read(&mut self.reader, 0)?;

        if chunk.id() != riff::RIFF_ID {
            return Err(DecodeError::UnsupportedRootChunkId(chunk.id()));
        }

        let type_chunk_id = chunk.read_type(&mut self.reader)?;
        if type_chunk_id != chunk_id(b"ACON") {
            return Err(DecodeError::UnsupportedRootType(type_chunk_id));
        }

        let chunks = read_chunks(&mut chunk.iter(&mut self.reader))?;

        let metadata: Result<AnimatedCursorMetadata, DecodeError> = chunks
            .iter()
            .find(|c| c.id() == chunk_id(b"anih"))
            .map(|c| {
                let contents = c.read_contents(&mut self.reader)?;
                let mut cursor = std::io::Cursor::new(contents);

                Ok(AnimatedCursorMetadata {
                    header_size_bytes: cursor.read_u32::<LittleEndian>()?,
                    frame_count: cursor.read_u32::<LittleEndian>()?,
                    step_count: cursor.read_u32::<LittleEndian>()?,
                    width: cursor.read_u32::<LittleEndian>()?,
                    height: cursor.read_u32::<LittleEndian>()?,
                    bit_count: cursor.read_u32::<LittleEndian>()?,
                    plane_count: cursor.read_u32::<LittleEndian>()?,
                    ticks_per_frame: cursor.read_u32::<LittleEndian>()?,
                    flags: AnimatedCursorFlags::from_bits(cursor.read_u32::<LittleEndian>()?)
                        .ok_or(DecodeError::InvalidHeaderFlags)?,
                })
            })
            .ok_or(DecodeError::MissingHeaderChunk)?;

        let metadata = metadata?;

        let frames = chunks
            .iter()
            .find(|c| c.id() == LIST_ID)
            .map(|c| {
                if c.read_type(&mut self.reader)? != chunk_id(b"fram") {
                    return Err(DecodeError::MissingFramesChunk);
                }

                read_chunks(&mut c.iter(&mut self.reader))?
                    .iter()
                    .map(|c| {
                        if c.id() != chunk_id(b"icon") {
                            return Err(DecodeError::UnsupportedFrameChunkId(c.id()));
                        };

                        let contents = c.read_contents(&mut self.reader)?;

                        // TODO: Support raw data frames.
                        if !metadata
                            .flags
                            .contains(AnimatedCursorFlags::ICON_OR_CURSOR_DATA)
                        {
                            return Err(DecodeError::UnsupportedRawDataFrameType);
                        }

                        let icon = IconDir::read(&mut std::io::Cursor::new(contents))?;

                        Ok(icon)
                    })
                    .collect()
            })
            .transpose()?
            .ok_or(DecodeError::MissingFramesChunk)?;

        Ok(AnimatedCursor { metadata, frames })
    }
}


pub fn icondir_to_custom_cursor(frame: &IconDir) -> anyhow::Result<CustomCursorSource> {
    let entry = frame.entries().first().unwrap();
    let image = entry.decode().unwrap();
    let rgba = image.rgba_data().to_vec();
    let width = image.width() as u16;
    let height = image.height() as u16;
    let (hot_x, hot_y) = entry.cursor_hotspot().unwrap_or((width / 2, height / 2));
    Ok(CustomCursor::from_rgba(rgba, width, height, hot_x, hot_y)?)
}

#[derive(Clone)]
pub struct CursorBundle {
    pub animated_cursor: AnimatedCursor,
    pub frames: Vec<CustomCursor>,
    pub current_frame: usize,
    pub last_update: Instant,
}

impl CursorBundle {
    pub fn update(&mut self) -> CustomCursor {
        let now = Instant::now();
        if now.duration_since(self.last_update) >= self.animated_cursor.metadata.duration_per_frame() {
            self.current_frame = (self.current_frame + 1) % self.frames.len();
            self.last_update = now;
        }
        
        self.frames[self.current_frame].clone()
    }

    pub fn reset(&mut self) {
        self.current_frame = 0;
        self.last_update = Instant::now();
    }
}


mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_read_ani() {
        let filepath = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../testcase/cursor1.ani"));
        let file = std::fs::File::open(filepath).unwrap();
        let cursor = Decoder::new(file).decode().unwrap();
        println!("{:#?}", cursor.metadata);
    }
}