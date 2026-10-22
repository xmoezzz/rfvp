//! Offset-based translation data. The HCB stays intact: its bytecode, resource
//! names and unlisted strings keep their original encoding and addresses.
use alloc::string::String;
use alloc::vec::Vec;
use anyhow::{anyhow, bail, Result};
use super::parser::{Nls, Parser};

const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct TextPatch {
    bytes: Vec<u8>,
    entries: Vec<(u32, u32, u16)>,
    nls: Nls,
}

impl TextPatch {
    pub fn parse(input: Vec<u8>, nls: Nls, parser: &Parser) -> Result<Self> {
        if input.len() < 4 || input.len() > MAX_BYTES { bail!("invalid text patch size"); }
        let word = |bytes: &[u8], offset: usize| -> Result<u32> {
            Ok(u32::from_le_bytes(bytes.get(offset..offset + 4)
                .ok_or_else(|| anyhow!("truncated text patch"))?.try_into()?))
        };
        let bytes = if word(&input, 0)? == 0 {
            let unpacked = word(&input, 8)? as usize;
            let packed = word(&input, 12)? as usize;
            if unpacked < 4 || unpacked > MAX_BYTES || packed.checked_add(16) != Some(input.len()) {
                bail!("invalid compressed text patch lengths");
            }
            let out = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&input[16..], unpacked)
                .map_err(|_| anyhow!("text patch zlib decode failed"))?;
            if out.len() != unpacked { bail!("text patch output length mismatch"); }
            out
        } else { input };
        let count = word(&bytes, 0)? as usize;
        if count > (bytes.len() - 4) / 6 { bail!("invalid text patch entry count"); }
        let mut entries = Vec::new();
        entries.try_reserve_exact(count).map_err(|_| anyhow!("cannot allocate text patch index"))?;
        let mut pos = 4usize;
        let mut offsets = Vec::new();
        for _ in 0..count {
            let offset = word(&bytes, pos)?;
            let len_bytes = bytes.get(pos + 4..pos + 6).ok_or_else(|| anyhow!("truncated text patch entry"))?;
            let len = u16::from_le_bytes(len_bytes.try_into()?) as usize;
            pos += 6;
            let text = bytes.get(pos..pos + len).ok_or_else(|| anyhow!("truncated translation"))?;
            // Only PushString payloads may be replaced; headers/syscalls stay intact.
            let at = offset as usize;
            if at < 2 || parser.read_u8(at - 2)? != 0x0e { bail!("text patch offset {offset:#x} is not PushString"); }
            let original_len = parser.read_u8(at - 1)? as usize;
            if original_len == 0 || parser.read_u8(at + original_len - 1)? != 0 {
                bail!("invalid original string at text patch offset {offset:#x}");
            }
            offsets.push(offset);
            // Zero length means keep the original string, not erase it.
            if len != 0 {
                if text.last() != Some(&0) || text[..len - 1].contains(&0) { bail!("invalid translation terminator"); }
                if crate::text_codec::decode(nls, &text[..len - 1]).1 { bail!("translation has invalid encoding at {offset:#x}"); }
                entries.push((offset, pos as u32, len as u16));
            }
            pos += len;
        }
        if pos != bytes.len() { bail!("unexpected trailing text patch data"); }
        offsets.sort_unstable();
        if offsets.windows(2).any(|pair| pair[0] == pair[1]) { bail!("duplicate text patch offsets"); }
        entries.sort_unstable_by_key(|entry| entry.0);
        log::info!("text patch loaded entries={} translated={} bytes={} nls={nls:?}", count, entries.len(), bytes.len());
        Ok(Self { bytes, entries, nls })
    }

    pub fn lookup(&self, offset: usize) -> Option<String> {
        let offset = u32::try_from(offset).ok()?;
        let i = self.entries.binary_search_by_key(&offset, |entry| entry.0).ok()?;
        let (_, start, len) = self.entries[i];
        Some(crate::text_codec::decode(self.nls, &self.bytes[start as usize..start as usize + len as usize - 1]).0)
    }
}
