//! The game as it ships: its script (`.hcb`), its resource packs (`.bin`) and loose files.

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail, ensure};
use encoding_rs::SHIFT_JIS;

pub struct Script {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub design_width: u32,
    pub design_height: u32,
}

/// The one `.hcb` at the top of the game folder.
pub fn find_script(input: &Path) -> Result<Script> {
    let mut found = Vec::new();
    for entry in fs::read_dir(input).with_context(|| format!("read {}", input.display()))? {
        let path = entry?.path();
        if path.is_file() && ext_eq(&path, "hcb") {
            found.push(path);
        }
    }
    let path = match found.len() {
        1 => found.remove(0),
        0 => bail!("no .hcb in {}", input.display()),
        _ => bail!("more than one .hcb in {}", input.display()),
    };
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let sys_desc = read_u32(&bytes, 0)? as usize;
    let game_mode = *bytes
        .get(sys_desc + 8)
        .ok_or_else(|| anyhow!("{}: system description outside the file", path.display()))?;
    let (design_width, design_height) = design_resolution(game_mode)?;
    Ok(Script { path, bytes, design_width, design_height })
}

fn design_resolution(game_mode: u8) -> Result<(u32, u32)> {
    Ok(match game_mode {
        0 | 5 => (640, 480),
        1 => (800, 600),
        2 => (1024, 768),
        3 => (1280, 960),
        4 => (1600, 1200),
        6 => (1024, 576),
        7 => (1024, 640),
        8 => (1280, 720),
        9 => (1280, 800),
        10 => (1440, 810),
        11 => (1440, 900),
        12 => (1680, 945),
        13 => (1680, 1050),
        14 => (1920, 1080),
        15 => (1920, 1200),
        _ => bail!("unknown game mode {game_mode}"),
    })
}

/// Every string the script pushes (`push_string`), decoded from Shift-JIS: the text the font
/// must have.
pub fn script_strings(script: &[u8]) -> Result<Vec<String>> {
    let code_end = read_u32(script, 0)? as usize;
    ensure!(code_end <= script.len(), "script code runs past the file");
    let mut strings = Vec::new();
    let mut pc = 4;
    while pc < code_end {
        let op = script[pc];
        let operand = match op {
            0 | 4 | 5 | 8 | 9 | 19 | 20 => 0,
            1 => 2,
            2 | 6 | 7 | 10 | 13 => 4,
            3 | 11 | 15 | 17 | 21 | 23 => 2,
            12 | 16 | 18 | 22 | 24 => 1,
            14 => {
                let len = *script.get(pc + 1).ok_or_else(|| anyhow!("string past the code"))? as usize;
                let raw = script
                    .get(pc + 2..pc + 2 + len)
                    .ok_or_else(|| anyhow!("string past the code"))?;
                let raw = raw.split(|&b| b == 0).next().unwrap_or(&[]);
                strings.push(SHIFT_JIS.decode(raw).0.into_owned());
                1 + len
            }
            25..=41 => 0,
            _ => bail!("unknown opcode {op:#x} at {pc:#x}"),
        };
        pc += 1 + operand;
    }
    Ok(strings)
}

/// A file of the game: loose, or an entry of a pack.
#[derive(Clone, Debug)]
pub struct Resource {
    pub name: String,
    pub source: Source,
}

#[derive(Clone, Debug)]
pub enum Source {
    Loose(PathBuf),
    Packed { pack: PathBuf, offset: u64, size: u64 },
}

impl Resource {
    pub fn read(&self) -> Result<Vec<u8>> {
        match &self.source {
            Source::Loose(path) => fs::read(path).with_context(|| format!("read {}", path.display())),
            Source::Packed { pack, offset, size } => {
                let mut file = fs::File::open(pack).with_context(|| format!("open {}", pack.display()))?;
                file.seek(SeekFrom::Start(*offset))?;
                let mut bytes = vec![0; *size as usize];
                file.read_exact(&mut bytes)
                    .with_context(|| format!("read {} from {}", self.name, pack.display()))?;
                Ok(bytes)
            }
        }
    }
}

/// A pack (`graph.bin`) and its entries.
pub struct Pack {
    /// The file name without `.bin`, lower case: `graph`, `bgm2`, ...
    pub stem: String,
    pub entries: Vec<Resource>,
}

/// The packs at the top of the game folder.
pub fn packs(input: &Path) -> Result<Vec<Pack>> {
    let mut packs = Vec::new();
    for entry in fs::read_dir(input)? {
        let path = entry?.path();
        if !path.is_file() || !ext_eq(&path, "bin") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("bad pack name {}", path.display()))?
            .to_ascii_lowercase();
        let entries = read_pack_index(&path)?
            .into_iter()
            .map(|(name, offset, size)| Resource {
                name,
                source: Source::Packed { pack: path.clone(), offset, size },
            })
            .collect();
        packs.push(Pack { stem, entries });
    }
    packs.sort_by(|a, b| a.stem.cmp(&b.stem));
    Ok(packs)
}

/// Loose files the converter handles: those below the top folder (`movie/01.wmv`).
pub fn loose_files(input: &Path) -> Result<Vec<Resource>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(input).min_depth(2) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let rel = entry.path().strip_prefix(input)?;
            let name = rel.to_string_lossy().replace('\\', "/");
            files.push(Resource { name, source: Source::Loose(entry.path().to_path_buf()) });
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

/// A pack's index: `u32 count, u32 name table size`, then per file `u32 name offset, u32 data
/// offset, u32 size`, then the NUL-terminated Shift-JIS names.
fn read_pack_index(path: &Path) -> Result<Vec<(String, u64, u64)>> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut head = [0u8; 8];
    file.read_exact(&mut head)?;
    let count = u32::from_le_bytes(head[0..4].try_into().unwrap()) as usize;
    let names_len = u32::from_le_bytes(head[4..8].try_into().unwrap()) as usize;
    let mut index = vec![0u8; count * 12 + names_len];
    file.read_exact(&mut index)
        .with_context(|| format!("read the index of {}", path.display()))?;
    let names = &index[count * 12..];
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let name_off = read_u32(&index, i * 12)? as usize;
        let offset = read_u32(&index, i * 12 + 4)? as u64;
        let size = read_u32(&index, i * 12 + 8)? as u64;
        let raw = names
            .get(name_off..)
            .and_then(|rest| rest.split(|&b| b == 0).next())
            .ok_or_else(|| anyhow!("{}: name outside the table", path.display()))?;
        entries.push((SHIFT_JIS.decode(raw).0.into_owned(), offset, size));
    }
    Ok(entries)
}

/// Write a pack in the engine's format, entries in the order given.
pub fn write_pack(path: &Path, entries: &[(String, Vec<u8>)]) -> Result<()> {
    let mut names = Vec::new();
    let mut name_offsets = Vec::with_capacity(entries.len());
    for (name, _) in entries {
        name_offsets.push(names.len() as u32);
        let (encoded, _, bad) = SHIFT_JIS.encode(name);
        ensure!(!bad, "{name} has no Shift-JIS form");
        names.extend_from_slice(&encoded);
        names.push(0);
    }
    let header_len = 8 + entries.len() * 12 + names.len();
    let mut out = Vec::with_capacity(header_len + entries.iter().map(|(_, d)| d.len()).sum::<usize>());
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    out.extend_from_slice(&(names.len() as u32).to_le_bytes());
    let mut data_off = header_len as u64;
    for ((_, data), name_off) in entries.iter().zip(&name_offsets) {
        ensure!(data_off + data.len() as u64 <= u32::MAX as u64, "{} is over 4 GiB", path.display());
        out.extend_from_slice(&name_off.to_le_bytes());
        out.extend_from_slice(&(data_off as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        data_off += data.len() as u64;
    }
    out.extend_from_slice(&names);
    for (_, data) in entries {
        out.extend_from_slice(data);
    }
    write_file(path, &out)
}

pub fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
}

pub fn ext_eq(path: &Path, ext: &str) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

pub fn read_u16(data: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        data.get(at..at + 2).ok_or_else(|| anyhow!("read past the end at {at}"))?.try_into()?,
    ))
}

pub fn read_u32(data: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        data.get(at..at + 4).ok_or_else(|| anyhow!("read past the end at {at}"))?.try_into()?,
    ))
}
