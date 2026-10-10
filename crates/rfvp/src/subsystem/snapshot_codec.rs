//! How save states are turned into bytes.
//!
//! The desktop and mobile builds use bincode. The `no_std` builds (consoles) cannot, since
//! bincode needs `std::io`, so they use postcard. The two formats are not compatible, and each
//! gets its own footer magic so a save written by the other kind is recognised and ignored
//! rather than misread.

#[cfg(feature = "no_std")]
use alloc::vec::Vec;
use anyhow::{Context, Result};
#[cfg(not(feature = "no_std"))]
use bincode::Options;
use serde::{de::DeserializeOwned, Serialize};

/// Footer magic of the save-state chunk at the end of a slot file.
#[cfg(not(feature = "no_std"))]
pub const STATE_MAGIC: [u8; 4] = *b"RFVS";
#[cfg(feature = "no_std")]
pub const STATE_MAGIC: [u8; 4] = *b"RFPS";

/// Footer magic of the global save file.
#[cfg(not(feature = "no_std"))]
pub const GLOBAL_MAGIC: [u8; 4] = *b"RFVG";
#[cfg(feature = "no_std")]
pub const GLOBAL_MAGIC: [u8; 4] = *b"RFPG";

/// `u32` payload length followed by the 4-byte magic.
pub const FOOTER_LEN: usize = 8;

#[cfg(not(feature = "no_std"))]
fn bincode_opts() -> impl bincode::Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .with_little_endian()
        .reject_trailing_bytes()
}

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    #[cfg(not(feature = "no_std"))]
    {
        bincode_opts().serialize(value).context("serialize")
    }
    #[cfg(feature = "no_std")]
    {
        postcard::to_allocvec(value).context("serialize")
    }
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    #[cfg(not(feature = "no_std"))]
    {
        bincode_opts().deserialize(bytes).context("deserialize")
    }
    #[cfg(feature = "no_std")]
    {
        postcard::from_bytes(bytes).context("deserialize")
    }
}

/// Append `payload`, its length and `magic` to `out`.
pub fn append_chunk(
    out: &mut Vec<u8>,
    payload: &[u8],
    magic: [u8; 4],
    max_len: usize,
) -> Result<()> {
    if payload.len() > max_len {
        anyhow::bail!(
            "payload too large: {} bytes (max {})",
            payload.len(),
            max_len
        );
    }
    let len: u32 = payload
        .len()
        .try_into()
        .context("payload length overflow")?;
    out.extend_from_slice(payload);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&magic);
    Ok(())
}

/// The payload of the chunk at the end of `bytes`, or `None` if `bytes` does not end in a chunk
/// with this `magic`.
pub fn find_chunk(bytes: &[u8], magic: [u8; 4], max_len: usize) -> Result<Option<&[u8]>> {
    if bytes.len() < FOOTER_LEN || bytes[bytes.len() - 4..] != magic {
        return Ok(None);
    }
    let len_pos = bytes.len() - FOOTER_LEN;
    let len = u32::from_le_bytes([
        bytes[len_pos],
        bytes[len_pos + 1],
        bytes[len_pos + 2],
        bytes[len_pos + 3],
    ]) as usize;
    if len > max_len {
        anyhow::bail!("chunk length {len} exceeds max {max_len}");
    }
    if bytes.len() < FOOTER_LEN + len {
        anyhow::bail!(
            "chunk is truncated: file={} need={} payload_len={len}",
            bytes.len(),
            FOOTER_LEN + len
        );
    }
    let start = bytes.len() - FOOTER_LEN - len;
    Ok(Some(&bytes[start..start + len]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn a_chunk_is_found_again_at_the_end_of_a_file() {
        let mut file = b"header and thumbnail".to_vec();
        append_chunk(&mut file, b"state", STATE_MAGIC, 1024).unwrap();
        assert_eq!(
            find_chunk(&file, STATE_MAGIC, 1024).unwrap(),
            Some(&b"state"[..])
        );
        // Another kind of chunk is not this one.
        assert_eq!(find_chunk(&file, GLOBAL_MAGIC, 1024).unwrap(), None);
        assert_eq!(find_chunk(b"short", STATE_MAGIC, 1024).unwrap(), None);
    }

    #[test]
    fn a_truncated_chunk_is_an_error() {
        let mut file = Vec::new();
        append_chunk(&mut file, b"0123456789", STATE_MAGIC, 1024).unwrap();
        // Cut out the middle of the payload but keep the footer.
        let footer = file[file.len() - FOOTER_LEN..].to_vec();
        let mut cut = file[..3].to_vec();
        cut.extend_from_slice(&footer);
        assert!(find_chunk(&cut, STATE_MAGIC, 1024).is_err());
    }

    #[test]
    fn values_round_trip() {
        let value = (
            7u32,
            String::from("日本語"),
            alloc::vec![1u8, 2, 3],
            Some(1.5f32),
        );
        let bytes = encode(&value).unwrap();
        let back: (u32, String, Vec<u8>, Option<f32>) = decode(&bytes).unwrap();
        assert_eq!(back, value);
    }
}
