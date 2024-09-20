//! Support for ROM format, which is an archive format used by the game
//!
//! Note that, unlike the original engine, this implementation reads the entire ROM index into memory.
//!
//! This makes the implementation much simpler and file access much faster, but it increases startup time a bit.
//!
//! When using BufReader, the startup time with Umineko's rom is about 300 ms on my machine, so it's not a big deal.

use std::{collections::BTreeMap, io, io::SeekFrom};

use anyhow::{anyhow, bail, Context, Result};
use binrw::{BinRead, BinResult, BinWrite, Endian, NullString};
use itertools::Itertools;
use smartstring::alias::CompactString;

const VERSION: u32 = 0x10001;
const DIRECTORY_OFFSET_MULTIPLIER: u64 = 16;

#[derive(BinRead, BinWrite)]
#[br(magic = b"ROM2", little)]
struct RawHeader {
    pub version: u32,
    pub index_len: u32,
    pub offset_multiplier: u32,
    pub whatever1: u32,
    pub whatever2: u32,
    pub whatever3: u32,
    pub whatever4: u32,
}

#[derive(Copy, Clone)]
pub struct ReadContext {
    pub index_offset: u64,
    pub current_dir_offset: u64,
    pub data_offset_multiplier: u64,
}

impl ReadContext {
    pub fn with_dir_offset(self, dir_offset: u64) -> Self {
        Self {
            current_dir_offset: dir_offset,
            ..self
        }
    }
}

#[derive(BinRead, BinWrite)]
#[br(little)]
struct RawEntry {
    // name offset is from the beginning of the entry
    pub directory_and_name_offset: u32,
    // data offset is from from the beginning of the archive file
    pub data_offset: u32,
    pub data_size: u32,
}

impl BinRead for Entry {
    type Args<'a> = ReadContext;

    fn read_options<R: io::Read + io::Seek>(
        reader: &mut R,
        endian: Endian,
        ctx: Self::Args<'_>,
    ) -> BinResult<Entry> {
        // let entry_pos = reader.stream_position()?;
        let entry: RawEntry = <_>::read_options(reader, endian, ())?;

        let leave_pos = reader.stream_position()?;

        let is_directory = entry.directory_and_name_offset >> 31 != 0;

        reader.seek(SeekFrom::Start(
            ctx.current_dir_offset + (entry.directory_and_name_offset & 0x7fffffff) as u64,
        ))?;
        // NullString does an extra heap alloc =(
        // better write one ourselves it seems
        let name: NullString = <_>::read_options(reader, endian, ())?;
        let name: String = name.try_into().unwrap();
        let name: CompactString = name.into();

        let res = match is_directory {
            true => Entry::Directory {
                name,
                entries_offset: entry.data_offset as u64 * DIRECTORY_OFFSET_MULTIPLIER,
                // data_size: entry.data_size,
            },
            false => Entry::File {
                name,
                data_offset: entry.data_offset as u64 * ctx.data_offset_multiplier,
                data_size: entry.data_size,
            },
        };

        reader.seek(SeekFrom::Start(leave_pos as _))?;

        Ok(res)
    }
}

#[derive(Debug)]
enum Entry {
    Directory {
        name: CompactString,
        // this is offset from the beginning of the archive file
        entries_offset: u64,
        // this is provided, but we don't really use it
        // data_size: u32
    },
    File {
        name: CompactString,
        data_offset: u64,
        data_size: u32,
    },
}

#[derive(Debug)]
pub enum IndexEntry {
    File(IndexFile),
    Directory(IndexDirectory),
}

#[derive(Debug, Copy, Clone)]
pub struct IndexFile {
    data_offset: u64,
    data_size: u32,
}

impl IndexFile {
    pub fn size(&self) -> u32 {
        self.data_size
    }
}

#[derive(Debug)]
pub struct IndexDirectory {
    entries: BTreeMap<CompactString, IndexEntry>,
}

impl IndexDirectory {
    pub fn iter(&self) -> IndexDirectoryIter {
        IndexDirectoryIter {
            inner: self.entries.iter(),
        }
    }
}

pub struct IndexDirectoryIter<'a> {
    inner: std::collections::btree_map::Iter<'a, CompactString, IndexEntry>,
}

impl<'a> Iterator for IndexDirectoryIter<'a> {
    type Item = (&'a str, &'a IndexEntry);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, v)| (k.as_str(), v))
    }
}

#[derive(Debug)]
struct NamedEntry(CompactString, IndexEntry);

impl BinRead for NamedEntry {
    type Args<'a> = ReadContext;

    fn read_options<R: io::Read + io::Seek>(
        reader: &mut R,
        endian: Endian,
        ctx: Self::Args<'_>,
    ) -> BinResult<NamedEntry> {
        let entry: Entry = <_>::read_options(reader, endian, ctx)?;
        let leave_pos = reader.stream_position()?;
        let res = match entry {
            Entry::Directory {
                name,
                entries_offset,
            } => {
                if matches!(name.as_str(), "." | "..") {
                    // do not read contents of these, just return a dummy entry
                    // they will be ignored anyways
                    NamedEntry(
                        name,
                        IndexEntry::Directory(IndexDirectory {
                            entries: BTreeMap::new(),
                        }),
                    )
                } else {
                    reader.seek(SeekFrom::Start(ctx.index_offset + entries_offset))?;
                    let entry = <_>::read_options(reader, endian, ctx)?;

                    NamedEntry(name, IndexEntry::Directory(entry))
                }
            }
            Entry::File {
                name,
                data_offset,
                data_size,
            } => NamedEntry(
                name,
                IndexEntry::File(IndexFile {
                    data_offset,
                    data_size,
                }),
            ),
        };

        reader.seek(SeekFrom::Start(leave_pos))?;
        Ok(res)
    }
}

impl BinRead for IndexDirectory {
    type Args<'a> = ReadContext;

    fn read_options<R: io::Read + io::Seek>(
        reader: &mut R,
        endian: Endian,
        ctx: Self::Args<'_>,
    ) -> BinResult<IndexDirectory> {
        let dir_offset = reader.stream_position()?;
        let entry_count: u32 = <_>::read_options(reader, endian, ())?;

        let mut entries = BTreeMap::new();
        for _ in 0..entry_count {
            let entry: NamedEntry =
                <_>::read_options(reader, endian, ctx.with_dir_offset(dir_offset))?;
            if matches!(entry.0.as_str(), "." | "..") {
                continue;
            }
            entries.insert(entry.0, entry.1);
        }

        Ok(IndexDirectory { entries })
    }
}

/// Allows reading files from the archive
///
/// Assumes that the underlying file will not change
pub struct RomReader<S: io::Read + io::Seek> {
    index: IndexDirectory,
    reader: S,
}

impl<S: io::Read + io::Seek> RomReader<S> {
    pub fn new(mut reader: S) -> Result<Self> {
        reader.seek(SeekFrom::Start(0))?;
        let header = RawHeader::read(&mut reader).context("Reading rom header")?;
        if VERSION != header.version {
            bail!("Unknown version: 0x{:08x}", header.version)
        }
        let index_offset = reader.stream_position()?;

        let ctx = ReadContext {
            index_offset,
            current_dir_offset: index_offset,
            data_offset_multiplier: header.offset_multiplier as u64,
        };

        let index = IndexDirectory::read_le_args(&mut reader, ctx)?;

        Ok(Self { index, reader })
    }

    pub fn index(&self) -> &IndexDirectory {
        &self.index
    }

    pub fn find_file(&self, path: &str) -> Result<IndexFile> {
        let path = path
            .strip_prefix('/')
            .ok_or_else(|| anyhow!("Path must start with /"))?;

        let mut entry = &self.index;
        let mut split_path = path.split('/').peekable();
        let mut filename = None;

        while let Some(part) = split_path.next() {
            if split_path.peek().is_none() {
                filename = Some(part);
                break;
            }
            entry = match entry.entries.get(part) {
                Some(IndexEntry::Directory(dir)) => dir,
                Some(IndexEntry::File(_)) => bail!(
                    "Invalid path, found a file when expected a directory: {:?}",
                    path
                ),
                None => bail!("Invalid path, directory not found: {:?}", path),
            }
        }

        let filename = filename.ok_or_else(|| anyhow!("Invalid path, no filename: {:?}", path))?;

        Ok(*match entry.entries.get(filename) {
            Some(IndexEntry::File(file)) => file,
            Some(IndexEntry::Directory(_)) => bail!(
                "Invalid path, found a directory when expected a file: {:?}",
                path
            ),
            None => bail!("Invalid path, file not found: {:?}", path),
        })
    }

    pub fn open_file(&mut self, file: IndexFile) -> Result<RomFileReader<S>> {
        Ok(RomFileReader {
            reader: self,
            file,
            position: 0,
        })
    }

    pub fn traverse(&self) -> impl Iterator<Item = (String, &IndexEntry)> {
        Traverse {
            stack: vec![("", self.index.iter())],
        }
    }
}

pub struct Traverse<'a> {
    stack: Vec<(&'a str, IndexDirectoryIter<'a>)>,
}

impl<'a> Iterator for Traverse<'a> {
    type Item = (String, &'a IndexEntry);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (_, iter) = match self.stack.last_mut() {
                Some(v) => v,
                None => return None,
            };

            match iter.next() {
                Some((entry_name, entry)) => {
                    let name = self
                        .stack
                        .iter()
                        .map(|(p, _)| p)
                        .chain(std::iter::once(&entry_name))
                        .join("/");
                    return match entry {
                        IndexEntry::Directory(dir) => {
                            self.stack.push((entry_name, dir.iter()));
                            Some((name, entry))
                        }
                        IndexEntry::File(_) => Some((name, entry)),
                    };
                }
                None => {
                    self.stack.pop();
                }
            }
        }
    }
}

/// Implements `Read` for `RomReader`
/// Assumes that the underlying file will not change
pub struct RomFileReader<'a, S: io::Read + io::Seek> {
    reader: &'a mut RomReader<S>,
    file: IndexFile,
    position: u64,
}

impl<'a, S: io::Read + io::Seek> io::Read for RomFileReader<'a, S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let to_read =
            std::cmp::min(buf.len() as u64, self.file.data_size as u64 - self.position) as usize;
        self.reader
            .reader
            .seek(SeekFrom::Start(self.file.data_offset + self.position))?;
        let read = self.reader.reader.read(&mut buf[..to_read])?;
        self.position += read as u64;
        Ok(read)
    }
}

fn checked_add_signed(pos: u64, offset: i64) -> Option<u64> {
    if offset >= 0 {
        u64::checked_add(pos, offset as u64)
    } else {
        u64::checked_sub(pos, offset.unsigned_abs())
    }
}

impl<'a, S: io::Read + io::Seek> io::Seek for RomFileReader<'a, S> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(pos) => Some(pos),
            SeekFrom::End(pos) => checked_add_signed(self.file.data_size as u64, pos),
            SeekFrom::Current(pos) => checked_add_signed(self.position, pos),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid seek"))?;
        let new_pos = std::cmp::min(self.file.data_size as u64, new_pos);
        let new_pos = std::cmp::max(0, new_pos);
        self.position = new_pos;
        Ok(new_pos)
    }
}
