use anyhow::{bail, Context, Result};
use rfvp_nls::TextDecoder;
use std::borrow::Cow;


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEntry {
    pub arg_count: u8,
    /// Raw bytes of the syscall name as stored in the script file.
    /// (Encoding is game-dependent; keep bytes to avoid guessing.)
    pub name: Vec<u8>,
}

impl ImportEntry {
    #[inline]
    pub fn name_str<'a>(&'a self, nls: &dyn TextDecoder) -> Cow<'a, str> {
        nls.decode_cstr(&self.name)
    }
}

impl CustomSyscallEntry {
    #[inline]
    pub fn name_str<'a>(&'a self, nls: &dyn TextDecoder) -> Cow<'a, str> {
        nls.decode_cstr(&self.name)
    }
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomSyscallEntry {
    /// Callback address (in the original engine). Kept for completeness; unused in the Rust port.
    pub callback_addr: u32,
    pub arg_count: u8,
    pub name: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct HcbFile {
    /// Entire script file buffer (the VM uses absolute offsets into this buffer).
    pub bytes: Vec<u8>,

    /// Offset of the imported syscall table / metadata block.
    pub sys_tbl_offset: u32,

    /// Entry point PC (absolute offset into `bytes`).
    pub entry_point: u32,

    pub non_volatile_global_count: u16,
    pub volatile_global_count: u16,

    /// Resolution index (0..=15; out-of-range is clamped to 0 by the original loader).
    pub resolution_idx: u8,

    /// One extra byte read after `resolution_idx` in the original loader.
    /// The original symbol name is unclear from the decompiler; keep as raw for now.
    pub dis_flag: u8,

    /// Title bytes. If empty, the original uses a fallback title.
    pub title: Vec<u8>,

    /// Imported syscalls referenced by the bytecode via `syscall` opcode.
    pub imports: Vec<ImportEntry>,

    /// Custom syscalls defined by the script. (Rare; kept for completeness.)
    pub custom_syscalls: Vec<CustomSyscallEntry>,
}

impl HcbFile {
    #[inline]
    pub fn total_global_count(&self) -> usize {
        self.non_volatile_global_count as usize + self.volatile_global_count as usize
    }

    /// Code section is conventionally `[4, sys_tbl_offset)`.
    /// This matches the original loader behavior (it reads the first u32 as `sys_tbl_offset`).
    #[inline]
    pub fn code_range(&self) -> std::ops::Range<usize> {
        4..(self.sys_tbl_offset as usize)
    }

    #[inline]
    pub fn is_code_addr(&self, addr: u32) -> bool {
        let a = addr as usize;
        a >= 4 && a < self.sys_tbl_offset as usize && a < self.bytes.len()
    }

    #[inline]
    pub fn title_str<'a>(&'a self, nls: &dyn TextDecoder) -> Cow<'a, str> {
        nls.decode_cstr(&self.title)
    }
}

fn read_u8(bytes: &[u8], off: &mut usize) -> Result<u8> {
    if *off + 1 > bytes.len() {
        bail!("unexpected EOF while reading u8 at {}", off);
    }
    let v = bytes[*off];
    *off += 1;
    Ok(v)
}

fn read_u16_le(bytes: &[u8], off: &mut usize) -> Result<u16> {
    if *off + 2 > bytes.len() {
        bail!("unexpected EOF while reading u16 at {}", off);
    }
    let v = u16::from_le_bytes([bytes[*off], bytes[*off + 1]]);
    *off += 2;
    Ok(v)
}

fn read_u32_le(bytes: &[u8], off: &mut usize) -> Result<u32> {
    if *off + 4 > bytes.len() {
        bail!("unexpected EOF while reading u32 at {}", off);
    }
    let v = u32::from_le_bytes([bytes[*off], bytes[*off + 1], bytes[*off + 2], bytes[*off + 3]]);
    *off += 4;
    Ok(v)
}

/// Parse an `.hcb` script file buffer following the exact layout implied by the decompiled loader.
///
/// Layout (little-endian):
/// - 0x00: u32 sys_tbl_offset
/// - 0x04..sys_tbl_offset: code section (bytecode + embedded string literals)
/// - sys_tbl_offset:
///     - u32 entry_point
///     - i16 non_volatile_global_count
///     - i16 volatile_global_count
///     - u8  resolution_idx
///     - u8  dis_flag   (the loader increments the cursor once here)
///     - u8  title_len
///     - [title_len] title bytes
///     - i16 import_count
///       repeated import_count:
///         - u8 arg_count
///         - u8 name_len
///         - [name_len] name bytes
///     - i16 custom_syscall_count
///       repeated custom_syscall_count:
///         - u32 callback_addr
///         - u8  arg_count
///         - u8  name_len
///         - [name_len] name bytes
pub fn parse_hcb(bytes: &[u8]) -> Result<HcbFile> {
    if bytes.len() < 4 {
        bail!("buffer too small for HCB header: {}", bytes.len());
    }

    let sys_tbl_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if sys_tbl_offset < 4 || sys_tbl_offset > bytes.len() {
        bail!("invalid sys_tbl_offset: {} (len={})", sys_tbl_offset, bytes.len());
    }

    let mut off = sys_tbl_offset;

    let entry_point = read_u32_le(bytes, &mut off).context("read entry_point")?;

    let non_volatile_global_count =
        read_u16_le(bytes, &mut off).context("read non_volatile_global_count")?;
    let volatile_global_count = read_u16_le(bytes, &mut off).context("read volatile_global_count")?;

    let resolution_idx = read_u8(bytes, &mut off).context("read resolution_idx")?;

    // The decompiled loader does `v19 = ++script_pos; title_len = mem[v19];`
    // i.e., it consumes one extra byte here before reading title_len.
    let dis_flag = read_u8(bytes, &mut off).context("read dis_flag")?;

    let title_len = read_u8(bytes, &mut off).context("read title_len")? as usize;
    if off + title_len > bytes.len() {
        bail!(
            "title bytes out of range: off={} len={} total={}",
            off,
            title_len,
            bytes.len()
        );
    }
    let title = bytes[off..off + title_len].to_vec();
    off += title_len;

    let import_count = read_u16_le(bytes, &mut off).context("read import_count")? as usize;
    let mut imports = Vec::with_capacity(import_count);
    for _ in 0..import_count {
        let arg_count = read_u8(bytes, &mut off).context("read import arg_count")?;
        let name_len = read_u8(bytes, &mut off).context("read import name_len")? as usize;
        if off + name_len > bytes.len() {
            bail!(
                "import name out of range: off={} len={} total={}",
                off,
                name_len,
                bytes.len()
            );
        }
        let name = bytes[off..off + name_len].to_vec();
        off += name_len;
        imports.push(ImportEntry { arg_count, name });
    }

    let custom_count =
        read_u16_le(bytes, &mut off).context("read custom_syscall_count")? as usize;
    let mut custom_syscalls = Vec::with_capacity(custom_count);
    for _ in 0..custom_count {
        let callback_addr = read_u32_le(bytes, &mut off).context("read custom callback_addr")?;
        let arg_count = read_u8(bytes, &mut off).context("read custom arg_count")?;
        let name_len = read_u8(bytes, &mut off).context("read custom name_len")? as usize;
        if off + name_len > bytes.len() {
            bail!(
                "custom syscall name out of range: off={} len={} total={}",
                off,
                name_len,
                bytes.len()
            );
        }
        let name = bytes[off..off + name_len].to_vec();
        off += name_len;
        custom_syscalls.push(CustomSyscallEntry {
            callback_addr,
            arg_count,
            name,
        });
    }

    Ok(HcbFile {
        bytes: bytes.to_vec(),
        sys_tbl_offset: sys_tbl_offset as u32,
        entry_point,
        non_volatile_global_count,
        volatile_global_count,
        resolution_idx: if resolution_idx >= 0x10 { 0 } else { resolution_idx },
        dis_flag,
        title,
        imports,
        custom_syscalls,
    })
}
