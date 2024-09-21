//! Support for BUP files, storing the character bustup sprites.
//!
//! The BUP format is re-using the machinery from the picture format, but it has some additions on top.
//!
//! The character sprite is composed of three layers:
//! - the base image, which is the character's body
//! - the expression, which displays the character's facial expression
//! - the mouth, which displays the character's mouth
//!
//! The layers are separate because one base image can have multiple facial expressions layered on top, using storage more efficiently.
//!
//! The mouth is also separate because it is usually animated, storing multiple versions with varying openness.

use std::collections::HashMap;

use anyhow::{bail, Result};
use binrw::{BinRead, BinWrite};
use bitvec::bitbox;
use image::RgbaImage;
use rfvp_tasks::ParallelSlice;

use crate::format::{
    picture::{read_picture_chunk, PictureChunk},
    text::ZeroString,
};

#[derive(BinRead, BinWrite, Debug)]
#[br(little, magic = b"BUP4")]
#[br(assert(version == 4))]
#[bw(assert(*version == 4))]
struct BustupHeader {
    version: u32,
    file_size: u32,
    // origin?
    origin_x: u16,
    origin_y: u16,
    viewport_width: u16,
    viewport_height: u16,
    f_14: u32,
    f_18: u32,
    f_1c: u32,
    f_20: u32,
    f_24: u32,
    f_28: u32,
    f_2c: u32,
    f_30: u32,

    #[brw(align_before = 0x10)]
    base_chunks_count: u32,
    #[br(count = base_chunks_count)]
    base_chunks: Vec<BustupChunkDesc>,

    #[brw(align_before = 0x10)]
    expression_count: u32,
    #[br(count = expression_count)]
    expressions: Vec<BustupExpressionDesc>,
}

impl BustupHeader {
    pub fn iter_additional_chunk_descs(&self) -> impl Iterator<Item = &BustupChunkDesc> {
        self.expressions
            .iter()
            .flat_map(|c| std::iter::once(&c.face).chain(c.mouth_chunks.iter()))
    }
}

#[derive(BinRead, BinWrite, Debug, Clone, Copy, PartialEq)]
struct BustupChunkDesc {
    offset: u32,
    size: u32,
    chunk_id: u32,
}

#[derive(BinRead, BinWrite, Debug)]
#[br(assert(f_4 == 0 && f_8 == 0 && f_c == 0, "Expected f_4, f_8, f_c to be 0"))]
struct BustupExpressionDesc {
    header_length: u32,
    f_4: u32,
    f_8: u32,
    f_c: u32,
    face: BustupChunkDesc,
    mount_chunk_count: u32,

    expression_name: ZeroString,

    #[brw(align_before = 0x4)]
    #[br(count = mount_chunk_count)]
    mouth_chunks: Vec<BustupChunkDesc>,
}

// TODO: do we want to support non-composited bustups, like we do in pictures?
pub struct Bustup {
    pub base_image: RgbaImage,
    pub origin: (u16, u16),
    pub expressions: HashMap<String, BustupExpression>,
}

pub struct BustupExpression {
    pub face_chunk: PictureChunk,
    pub mouth_chunks: Vec<PictureChunk>,
}

fn cleanup_unused_areas(chunk: &mut PictureChunk) {
    let mut bitbox = bitbox![0u32; chunk.data.width() as usize * chunk.data.height() as usize];
    let coord_to_index = |x: u32, y: u32| (y * chunk.data.width() + x) as usize;
    for vertex in chunk
        .opaque_vertices
        .iter()
        .chain(chunk.transparent_vertices.iter())
    {
        let clamp_y = |y: u16| std::cmp::min(y, chunk.data.height() as u16 - 1);
        let clamp_x = |x: u16| std::cmp::min(x, chunk.data.width() as u16 - 1);
        for y in vertex.from_y.saturating_sub(0)..clamp_y(vertex.to_y) {
            for x in vertex.from_x.saturating_sub(0)..clamp_x(vertex.to_x) {
                bitbox.set(coord_to_index(x as u32, y as u32), true);
            }
        }
    }

    for (pixel, mask) in chunk.data.pixels_mut().zip(bitbox) {
        if !mask {
            *pixel = image::Rgba([0, 0, 0, 0]);
        }
    }
}

pub fn read_bustup(source: &[u8]) -> Result<Bustup> {
    let mut source = std::io::Cursor::new(source);
    let source = &mut source;

    let header = BustupHeader::read(source)?;

    if header.file_size != source.get_ref().len() as u32 {
        bail!("File size mismatch");
    }

    let mut base_chunks = HashMap::new();
    for chunk in header.base_chunks.iter() {
        let e = base_chunks.entry(chunk.chunk_id).or_insert(*chunk);
        assert_eq!(
            e, chunk,
            "Two chunks have the same ID, but different contents"
        );
    }
    let base_chunks = base_chunks.into_iter().collect::<Vec<_>>();

    let mut additional_chunks = HashMap::new();
    for chunk in header.iter_additional_chunk_descs() {
        let e = additional_chunks.entry(chunk.chunk_id).or_insert(*chunk);
        assert_eq!(
            e, chunk,
            "Two chunks have the same ID, but different contents"
        );
    }
    let additional_chunks = additional_chunks.into_iter().collect::<Vec<_>>();

    // TODO: actually, collecting all of these is not the most efficient in terms of memory...
    // It might be better to first collect the "base" chunks into the base picture (the same way it's done in picture)
    // and then we can start reading the expressions and their mouths.

    let mut base_image =
        RgbaImage::new(header.viewport_width as u32, header.viewport_height as u32);

    fn par_decode_chunks(
        chunks: Vec<(u32, BustupChunkDesc)>,
        source: &[u8],
    ) -> Vec<Result<(u32, PictureChunk)>> {
        chunks.par_chunk_map(
            rfvp_tasks::AsyncComputeTaskPool::get(),
            1,
            |chunk| -> Result<_> {
                let &[(id, desc)] = chunk else { unreachable!() };
                let data = &source[desc.offset as usize..(desc.offset + desc.size) as usize];
                let mut chunk = read_picture_chunk(data)?;
                cleanup_unused_areas(&mut chunk);
                Ok((id, chunk))
            },
        )
    }

    par_decode_chunks(base_chunks, source.get_ref())
        .into_iter()
        .try_for_each(|res| -> Result<()> {
            let (_, chunk) = res?;

            image::imageops::overlay(
                &mut base_image,
                &chunk.data,
                chunk.offset_x as i64,
                chunk.offset_y as i64,
            );
            Ok(())
        })?;

    let additional_chunks = par_decode_chunks(additional_chunks, source.get_ref())
        .into_iter()
        .collect::<Result<HashMap<_, _>>>()?;

    Ok(Bustup {
        base_image,
        origin: (header.origin_x, header.origin_y),
        expressions: header
            .expressions
            .into_iter()
            .map(|e| {
                let name = e.expression_name.0;

                let expression = BustupExpression {
                    face_chunk: additional_chunks[&e.face.chunk_id].clone(),
                    mouth_chunks: e
                        .mouth_chunks
                        .into_iter()
                        .map(|c| additional_chunks[&c.chunk_id].clone())
                        .collect(),
                };

                (name, expression)
            })
            .collect(),
    })
}
