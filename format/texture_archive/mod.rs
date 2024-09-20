//! Support for decoding TXA texture archives.

use std::collections::HashMap;

use anyhow::Result;
use binrw::{BinRead, BinWrite};
use image::RgbaImage;
use rfvp_tasks::ParallelSlice;

use crate::format::text::ZeroString;

#[derive(BinRead, BinWrite, Debug)]
#[brw(little, magic = b"TXA4")]
#[br(assert(version == 2))]
struct TxaHeader {
    version: u32,
    file_size: u32,
    use_dict_encoding: u32,
    count: u32,
    max_decompressed_size: u32,
    index_size: u32,

    #[br(count = count)]
    #[brw(align_before = 0x10)]
    index: Vec<TxaIndexEntry>,
}

#[derive(BinRead, BinWrite, Debug)]
struct TxaIndexEntry {
    #[brw(align_before = 0x4)]
    entry_length: u16,
    virtual_index: u16,
    width: u16,
    height: u16,
    data_offset: u32,
    data_compressed_size: u32,
    data_decompressed_size: u32,

    name: ZeroString,
}

pub struct TextureArchive {
    pub textures: Vec<RgbaImage>,
    pub name_to_index: HashMap<String, usize>,
    pub vindex_to_index: HashMap<u16, usize>,
}

impl TextureArchive {
    pub fn get_texture(&self, name: &str) -> Option<&RgbaImage> {
        self.name_to_index.get(name).map(|&i| &self.textures[i])
    }

    pub fn get_texture_by_vindex(&self, vindex: u16) -> Option<&RgbaImage> {
        self.vindex_to_index
            .get(&vindex)
            .map(|&i| &self.textures[i])
    }
}

fn decode_texture(
    data: &[u8],
    index_entry: &TxaIndexEntry,
    use_dict_encoding: bool,
) -> Result<RgbaImage> {
    let mut image = RgbaImage::new(index_entry.width as u32, index_entry.height as u32);
    super::picture::read_texture(
        data,
        index_entry.data_compressed_size as usize,
        &mut image,
        use_dict_encoding,
        true,
    );

    Ok(image)
}

pub fn read_texture_archive(source: &[u8]) -> Result<TextureArchive> {
    let mut source = std::io::Cursor::new(source);
    let source = &mut source;

    let header: TxaHeader = TxaHeader::read(source)?;

    assert_eq!(header.file_size, source.get_ref().len() as u32);

    let textures = header
        .index
        .par_chunk_map(rfvp_tasks::AsyncComputeTaskPool::get(), 1, |chunk| {
            let [v] = chunk else { unreachable!() };
            let size = if v.data_compressed_size != 0 {
                v.data_compressed_size
            } else {
                v.data_decompressed_size
            } as usize;
            decode_texture(
                &source.get_ref()[v.data_offset as usize..][..size],
                v,
                header.use_dict_encoding != 0,
            )
        })
        .into_iter()
        .collect::<Result<Vec<_>>>()?;

    let name_to_index = header
        .index
        .iter()
        .enumerate()
        .map(|(i, v)| (v.name.0.clone(), i))
        .collect();
    let vindex_to_index = header
        .index
        .iter()
        .enumerate()
        .map(|(i, v)| (v.virtual_index, i))
        .collect();

    Ok(TextureArchive {
        textures,
        name_to_index,
        vindex_to_index,
    })
}
