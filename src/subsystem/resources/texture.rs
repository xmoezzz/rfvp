use anyhow::{bail, Result};
use byteorder::{LittleEndian, WriteBytesExt};
use flate2::read::ZlibDecoder;
use std::io::Read;
use std::io::Seek;
use std::io::Write;

// NVSGHDR type constants
const NVSGHDR_TYPE_SINGLE_24BIT: u16 = 0;
const NVSGHDR_TYPE_SINGLE_32BIT: u16 = 1;
const NVSGHDR_TYPE_MULTI_32BIT: u16 = 2;
const NVSGHDR_TYPE_SINGLE_8BIT: u16 = 3;
const NVSGHDR_TYPE_SINGLE_1BIT: u16 = 4;

const HZC1_SIGNATURE: [u8; 4] = [b'h', b'z', b'c', b'1'];
const NVSG_SIGNATURE: [u8; 4] = [b'N', b'V', b'S', b'G'];

fn read_u16le(buff: &[u8], offset: usize) -> Result<u16> {
    if buff.len() < offset + 2 {
        bail!("buffer too small for u16");
    }
    Ok(u16::from_le_bytes([buff[offset], buff[offset + 1]]))
}

fn read_u32le(buff: &[u8], offset: usize) -> Result<u32> {
    if buff.len() < offset + 4 {
        bail!("buffer too small for u32");
    }
    Ok(u32::from_le_bytes([
        buff[offset],
        buff[offset + 1],
        buff[offset + 2],
        buff[offset + 3],
    ]))
}

#[repr(C, packed)]
#[derive(Default)]
struct HZC1HDR {
    signature: [u8; 4],
    original_length: u32,
    header_length: u32,
}

#[repr(C, packed)]
#[derive(Default)]
struct NVSGHDR {
    signature: [u8; 4],
    unknown1: u16,
    type_: u16,
    width: u16,
    height: u16,
    offset_x: u16,
    offset_y: u16,
    unknown2: u32,
    entry_count: u32,
    unknown3: u32,
    unknown4: u32,
}

#[repr(C, packed)]
struct BITMAPFILEHEADER {
    bf_type: u16,
    bf_size: u32,
    bf_reserved1: u16,
    bf_reserved2: u16,
    bf_off_bits: u32,
}

#[repr(C, packed)]
struct BITMAPINFOHEADER {
    bi_size: u32,
    bi_width: i32,
    bi_height: i32,
    bi_planes: u16,
    bi_bit_count: u16,
    bi_compression: u32,
    bi_size_image: u32,
    bi_x_pels_per_meter: i32,
    bi_y_pels_per_meter: i32,
    bi_clr_used: u32,
    bi_clr_important: u32,
}

#[derive(Debug)]
pub struct TextureContainer {
    unknown1: u16,
    typ: u16,
    width: u16,
    height: u16,
    offset_x: u16,
    offset_y: u16,
    unknown2: u32,
    entry_count: u32,
    unknown3: u32,
    unknown4: u32,
    slices: Vec<Vec<u8>>,
}

impl TextureContainer {
    pub fn add_slice(&mut self, slice: Vec<u8>) {
        self.slices.push(slice);
    }
}

fn texture_to_bmp(buff: &[u8], width: i32, height: i32, depth_bytes: u16) -> Result<Vec<u8>> {
    let bmf = BITMAPFILEHEADER {
        bf_type: 0x4D42,
        bf_size: (std::mem::size_of::<BITMAPFILEHEADER>() as u32)
            + (std::mem::size_of::<BITMAPINFOHEADER>() as u32)
            + (buff.len() as u32),
        bf_reserved1: 0,
        bf_reserved2: 0,
        bf_off_bits: (std::mem::size_of::<BITMAPFILEHEADER>() as u32)
            + (std::mem::size_of::<BITMAPINFOHEADER>() as u32),
    };

    let bmi = BITMAPINFOHEADER {
        bi_size: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        bi_width: width,
        bi_height: height,
        bi_planes: 1,
        bi_bit_count: depth_bytes * 8,
        bi_compression: 0,
        bi_size_image: 0,
        bi_x_pels_per_meter: 0,
        bi_y_pels_per_meter: 0,
        bi_clr_used: 0,
        bi_clr_important: 0,
    };

    let mut buffer = Vec::with_capacity(bmf.bf_size as usize);
    buffer.write_u16::<LittleEndian>(bmf.bf_type)?;
    buffer.write_u32::<LittleEndian>(bmf.bf_size)?;
    buffer.write_u16::<LittleEndian>(bmf.bf_reserved1)?;
    buffer.write_u16::<LittleEndian>(bmf.bf_reserved2)?;
    buffer.write_u32::<LittleEndian>(bmf.bf_off_bits)?;

    buffer.write_u32::<LittleEndian>(bmi.bi_size)?;
    buffer.write_i32::<LittleEndian>(bmi.bi_width)?;
    buffer.write_i32::<LittleEndian>(bmi.bi_height)?;
    buffer.write_u16::<LittleEndian>(bmi.bi_planes)?;
    buffer.write_u16::<LittleEndian>(bmi.bi_bit_count)?;
    buffer.write_u32::<LittleEndian>(bmi.bi_compression)?;
    buffer.write_u32::<LittleEndian>(bmi.bi_size_image)?;
    buffer.write_i32::<LittleEndian>(bmi.bi_x_pels_per_meter)?;
    buffer.write_i32::<LittleEndian>(bmi.bi_y_pels_per_meter)?;
    buffer.write_u32::<LittleEndian>(bmi.bi_clr_used)?;
    buffer.write_u32::<LittleEndian>(bmi.bi_clr_important)?;

    buffer.write_all(buff)?;

    Ok(buffer)
}

fn read_texture(buff: &[u8], output_raw: bool) -> Result<TextureContainer> {
    if buff.len() < 4 || buff[..4] != HZC1_SIGNATURE {
        bail!("Invalid HZC1 header");
    }

    if buff.len() < std::mem::size_of::<HZC1HDR>() {
        bail!("buffer too small for HZC1 header");
    }

    let mut hzc1hdr = HZC1HDR::default();
    hzc1hdr.signature = [buff[0], buff[1], buff[2], buff[3]];
    hzc1hdr.original_length = read_u32le(buff, 4)?;
    hzc1hdr.header_length = read_u32le(buff, 8)?;

    let data_len = buff.len() - std::mem::size_of::<HZC1HDR>();
    let data_buff = &buff[std::mem::size_of::<HZC1HDR>()..];

    if data_len < std::mem::size_of::<NVSGHDR>() {
        bail!("buffer too small for NVSG header");
    }

    let mut nvsghdr = NVSGHDR::default();
    nvsghdr.signature = [data_buff[0], data_buff[1], data_buff[2], data_buff[3]];
    nvsghdr.unknown1 = read_u16le(data_buff, 4)?;
    nvsghdr.type_ = read_u16le(data_buff, 6)?;
    nvsghdr.width = read_u16le(data_buff, 8)?;
    nvsghdr.height = read_u16le(data_buff, 10)?;
    nvsghdr.offset_x = read_u16le(data_buff, 12)?;
    nvsghdr.offset_y = read_u16le(data_buff, 14)?;
    nvsghdr.unknown2 = read_u32le(data_buff, 16)?;
    nvsghdr.entry_count = read_u32le(data_buff, 20)?;
    nvsghdr.unknown3 = read_u32le(data_buff, 24)?;
    nvsghdr.unknown4 = read_u32le(data_buff, 28)?;

    if nvsghdr.signature != NVSG_SIGNATURE {
        bail!("Invalid NVSG header: {:?}", &nvsghdr.signature);
    }

    if nvsghdr.entry_count == 0 {
        nvsghdr.entry_count = 1;
    }

    let data_buff = &data_buff[hzc1hdr.header_length as usize..];
    let mut container = TextureContainer {
        unknown1: nvsghdr.unknown1,
        typ: nvsghdr.type_,
        width: nvsghdr.width,
        height: nvsghdr.height,
        offset_x: nvsghdr.offset_x,
        offset_y: nvsghdr.offset_y,
        unknown2: nvsghdr.unknown2,
        entry_count: nvsghdr.entry_count,
        unknown3: nvsghdr.unknown3,
        unknown4: nvsghdr.unknown4,
        slices: vec![],
    };

    let depth = match nvsghdr.type_ {
        NVSGHDR_TYPE_SINGLE_24BIT => 3,
        NVSGHDR_TYPE_SINGLE_32BIT | NVSGHDR_TYPE_MULTI_32BIT => 4,
        NVSGHDR_TYPE_SINGLE_8BIT | NVSGHDR_TYPE_SINGLE_1BIT => 1,
        _ => bail!("Invalid NVSG type: {}", container.typ),
    };

    let out_len = hzc1hdr.original_length as usize;
    let mut out_buff = vec![0; out_len];
    let mut decoder = ZlibDecoder::new(data_buff);
    decoder.read_exact(&mut out_buff)?;

    if nvsghdr.type_ == NVSGHDR_TYPE_SINGLE_1BIT {
        for byte in &mut out_buff {
            if *byte == 1 {
                *byte = 0xFF;
            }
        }
    }

    // let reader = Cursor::new(&out_buff);
    let frame_len = nvsghdr.width as u64 * nvsghdr.height as u64 * depth;

    for i in 0..nvsghdr.entry_count as u64 {
        let frame =
            out_buff.get(i as usize * frame_len as usize..(i as usize + 1) * frame_len as usize);

        if let Some(frame) = frame {
            let slice = if output_raw { 
                texture_to_bmp(
                    frame,
                    nvsghdr.width as i32,
                    0 - (nvsghdr.height as i32),
                    depth as u16,
                )?
            } else {
                frame.to_vec()
            };
            container.add_slice(slice);
        }
    }

    Ok(container)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_read_texture() {
        let filepath = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase/BGS016b"));
        let mut file = std::fs::File::open(filepath).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();
        let container = read_texture(&buffer, false).unwrap();
        assert!(!container.slices.is_empty());

        let slice = &container.slices[0];
        let output = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase/BGS016b.bmp"));
        let mut file = std::fs::File::create(output).unwrap();
        file.write_all(slice).unwrap();
    }

    #[test]
    fn test_read_texture_2() {
        let filepath = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase/BGS016a_parts"));
        let mut file = std::fs::File::open(filepath).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();
        let container = read_texture(&buffer, false).unwrap();
        assert!(!container.slices.is_empty());

        let output = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase/BGS016a_parts.dir"));
        if !output.exists() {
            std::fs::create_dir(output).unwrap();
        }

        for (i, slice) in container.slices.iter().enumerate() {
            let output = output.join(format!("BGS016b_parts_{}.bmp", i));
            let mut file = std::fs::File::create(output).unwrap();
            file.write_all(slice).unwrap();
        }
    }
}
