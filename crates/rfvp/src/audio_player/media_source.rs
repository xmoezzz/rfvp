//! VFS streams as Symphonia media sources, so kira streams songs and sounds from the packs.

use std::io::{Read, Seek, SeekFrom};

use anyhow::Result;

use crate::subsystem::resources::vfs::{Vfs, VfsStream};

pub struct VfsMediaSource {
    stream: VfsStream,
    byte_len: Option<u64>,
}

impl Read for VfsMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Seek for VfsMediaSource {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.stream.seek(pos)
    }
}

impl symphonia_core::io::MediaSource for VfsMediaSource {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        self.byte_len
    }
}

pub fn open(vfs: &Vfs, path: &str) -> Result<VfsMediaSource> {
    let (stream, byte_len) = vfs.open_stream_with_len(path)?;
    Ok(VfsMediaSource { stream, byte_len })
}
