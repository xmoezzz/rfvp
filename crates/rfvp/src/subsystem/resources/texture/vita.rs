//! Vita's exact-size decode path. The other targets keep their existing decoder.
use alloc::{boxed::Box, vec::Vec};
use anyhow::{anyhow, bail, Result};
use miniz_oxide::inflate::{
    core::{decompress, inflate_flags, DecompressorOxide},
    TINFLStatus,
};

pub(super) fn inflate(source: &[u8], output_len: usize) -> Result<Vec<u8>> {
    let mut decoded = Vec::new();
    decoded
        .try_reserve_exact(output_len)
        .map_err(|_| anyhow!("cannot allocate {} decoded bytes", output_len))?;
    decoded.resize(output_len, 0);
    let mut state = Box::<DecompressorOxide>::default();
    let flags = inflate_flags::TINFL_FLAG_PARSE_ZLIB_HEADER
        | inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
    // Keep zlib header and Adler32 verification, including when the output buffer
    // fills exactly. Done requires the complete stream, not merely full output.
    let (status, _, written) = decompress(&mut state, source, &mut decoded, 0, flags);
    if status != TINFLStatus::Done {
        bail!("zlib decompress failed: {:?}", status);
    }
    if written != output_len {
        bail!(
            "zlib output length mismatch: expected {}, got {}",
            output_len,
            written
        );
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::super::{NvsgTexture, TextureType};
    use super::*;

    #[test]
    fn exact_size_decode_matches_existing_decoder() {
        // Include empty, small, and multi-block resources with back references.
        for size in [0, 1, 17, 131_073] {
            let input: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
            let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&input, 6);
            let old = miniz_oxide::inflate::decompress_to_vec_zlib(&compressed).unwrap();
            assert_eq!(inflate(&compressed, input.len()).unwrap(), old);
        }
    }

    #[test]
    fn rejects_wrong_size_truncation_and_bad_checksum() {
        let input = b"a repeated image frame a repeated image frame";
        let mut compressed = miniz_oxide::deflate::compress_to_vec_zlib(input, 6);
        assert!(inflate(&compressed, input.len() - 1).is_err());
        assert!(inflate(&compressed, input.len() + 1).is_err());
        for end in 0..compressed.len() {
            assert!(
                inflate(&compressed[..end], input.len()).is_err(),
                "end={end}"
            );
        }
        *compressed.last_mut().unwrap() ^= 1;
        assert!(inflate(&compressed, input.len()).is_err());
        assert!(inflate(&[0; 16], input.len()).is_err());
    }

    fn frames() -> NvsgTexture {
        let mut texture = NvsgTexture::new("expressions");
        texture.typ = TextureType::Multi32Bit;
        texture.width = 2;
        texture.height = 1;
        texture.entry_count = 2;
        texture.packed_frames = Some((0u8..16).collect());
        texture
    }

    #[test]
    fn packed_frames_borrow_disjoint_regions_and_reject_invalid_indices() {
        let mut texture = frames();
        let allocation = texture.packed_frames.as_ref().unwrap().as_ptr();
        assert_eq!(texture.frame_bytes(0, 4).unwrap().as_ptr(), allocation);
        assert_eq!(
            texture.frame_bytes(1, 4).unwrap(),
            &[8, 9, 10, 11, 12, 13, 14, 15]
        );
        texture.frame_slice_mut(0).unwrap()[0] = 99;
        assert_eq!(texture.frame_bytes(0, 4).unwrap()[0], 99);
        assert_eq!(texture.frame_bytes(1, 4).unwrap()[0], 8);
        assert!(texture.frame_bytes(2, 4).is_err());
        assert!(texture.frame_slice_mut(usize::MAX).is_none());
    }

    #[test]
    fn truncated_packed_frames_fail_without_indexing_past_the_buffer() {
        let mut texture = frames();
        texture.packed_frames.as_mut().unwrap().truncate(15);
        assert!(texture.frame_bytes(0, 4).is_ok());
        assert!(texture.frame_bytes(1, 4).is_err());
        assert!(texture.frame_slice_mut(1).is_none());
        texture.packed_frames = None;
        texture.slices.push(alloc::vec![1; 7]);
        assert!(texture.frame_bytes(0, 4).is_err());
    }
}
