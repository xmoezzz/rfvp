use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::mem::size_of;
use core::str::FromStr;

#[cfg(feature = "old_school")]
use alloc::boxed::Box;
#[cfg(feature = "no_std")]
use alloc::collections::BTreeMap;
#[cfg(not(feature = "old_school"))]
use alloc::sync::Arc;
#[cfg(all(not(feature = "no_std"), target_os = "uefi"))]
use std::collections::hash_map::DefaultHasher;
#[cfg(not(feature = "no_std"))]
use std::collections::HashMap;
#[cfg(not(feature = "no_std"))]
use std::fs::File;
#[cfg(all(not(feature = "no_std"), target_os = "uefi"))]
use std::hash::BuildHasherDefault;
#[cfg(not(feature = "no_std"))]
use std::io::Read;
#[cfg(not(feature = "no_std"))]
use std::path::Path;

use anyhow::Result;

#[cfg(target_os = "uefi")]
macro_rules! uefi_parser_stage {
    ($($arg:tt)*) => {
        log::info!($($arg)*);
    };
}

#[cfg(not(target_os = "uefi"))]
macro_rules! uefi_parser_stage {
    ($($arg:tt)*) => {};
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Nls {
    #[default]
    ShiftJIS = 0,
    GBK = 1,
    UTF8 = 2,
}

impl FromStr for Nls {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "sjis" | "shiftjis" | "shift_jis" | "shift-jis" => Ok(Nls::ShiftJIS),
            "gbk" | "gb2312" | "gb18030" => Ok(Nls::GBK),
            "utf8" | "utf-8" => Ok(Nls::UTF8),
            _ => Err(anyhow::anyhow!(
                "unknown NLS '{}', valid values: sjis, gbk, utf8",
                s
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Syscall {
    /// how many arguments the syscall takes from the stack
    pub args: u8,
    /// name of the syscall
    pub name: String,
}

#[cfg(all(not(feature = "no_std"), target_os = "uefi"))]
pub type SyscallMap = HashMap<usize, Syscall, BuildHasherDefault<DefaultHasher>>;

#[cfg(all(not(feature = "no_std"), not(target_os = "uefi")))]
pub type SyscallMap = HashMap<usize, Syscall>;

#[cfg(feature = "no_std")]
pub type SyscallMap = BTreeMap<usize, Syscall>;

#[cfg_attr(not(feature = "old_school"), derive(Debug, Clone, Default))]
pub struct Parser {
    #[cfg(not(feature = "old_school"))]
    pub buffer: Arc<Vec<u8>>,
    #[cfg(feature = "old_school")]
    source: Box<dyn HcbByteSource>,
    /// The script bytes last fetched from `source`: an instruction and its operands come
    /// from one fetch (on the PlayStation a fetch per byte was a fifth of the frame).
    #[cfg(feature = "old_school")]
    window: core::cell::UnsafeCell<FetchWindow>,
    pub nls: Nls,
    pub sys_desc_offset: u32,
    /// entry point (offset) of the script
    pub entry_point: u32,
    pub non_volatile_global_count: u16,
    pub volatile_global_count: u16,
    // register a script function as syscall, never use?
    pub custom_syscall_count: u16,
    /// Game resolution for the window mode
    game_mode: u8,
    game_mode_reserved: u8,
    game_title: String,
    pub syscall_count: u16,
    pub syscalls: SyscallMap,
}

/// See `Parser::window`.
#[cfg(feature = "old_school")]
struct FetchWindow {
    start: usize,
    len: usize,
    bytes: [u8; 32],
}

// SAFETY: `window` is only touched through `&self` methods of a parser used by one thread at a
// time (the VM's); it holds no pointers.
#[cfg(feature = "old_school")]
unsafe impl Sync for Parser {}

impl Parser {
    /// `N` script bytes from `offset`, from the fetch window, refilling it if needed.
    #[cfg(feature = "old_school")]
    #[inline]
    fn fetch<const N: usize>(&self, offset: usize) -> Result<[u8; N]> {
        // SAFETY: see `FetchWindow`; no reference into the window outlives this call.
        let window = unsafe { &mut *self.window.get() };
        let at = offset.wrapping_sub(window.start);
        if at <= window.len && window.len - at >= N {
            let mut out = [0u8; N];
            out.copy_from_slice(&window.bytes[at..at + N]);
            return Ok(out);
        }
        let len = self.source.len().saturating_sub(offset).min(window.bytes.len());
        if len < N {
            return Err(anyhow::anyhow!("offset out of bounds"));
        }
        self.source.read_bytes(offset, &mut window.bytes[..len])?;
        window.start = offset;
        window.len = len;
        let mut out = [0u8; N];
        out.copy_from_slice(&window.bytes[..N]);
        Ok(out)
    }

    #[cfg(not(feature = "no_std"))]
    pub fn new(path: impl AsRef<Path>, nls: Nls) -> Result<Self> {
        let mut rdr = File::open(path)?;
        let mut buffer = Vec::new();
        rdr.read_to_end(&mut buffer)?;
        Self::from_bytes(buffer, nls)
    }

    pub fn from_bytes(buffer: impl Into<Vec<u8>>, nls: Nls) -> Result<Self> {
        uefi_parser_stage!("[UEFI] Parser::from_bytes entered");
        let buffer = buffer.into();
        uefi_parser_stage!("[UEFI] Parser::from_bytes buffer len={}", buffer.len());
        #[cfg(feature = "old_school")]
        {
            return Self::from_source(Box::new(MemoryHcbSource { bytes: buffer }), nls);
        }

        #[cfg(not(feature = "old_school"))]
        {
            uefi_parser_stage!("[UEFI] Parser::from_bytes before Arc::new");
            let buffer = Arc::new(buffer);
            uefi_parser_stage!("[UEFI] Parser::from_bytes after Arc::new");
            let mut parser = Parser {
                buffer,
                nls,
                sys_desc_offset: 0,
                entry_point: 0,
                non_volatile_global_count: 0,
                volatile_global_count: 0,
                custom_syscall_count: 0,
                game_mode: 0,
                game_mode_reserved: 0,
                game_title: String::new(),
                syscall_count: 0,
                syscalls: SyscallMap::default(),
            };

            uefi_parser_stage!("[UEFI] Parser::from_bytes before parser()");
            parser.parser()?;
            uefi_parser_stage!("[UEFI] Parser::from_bytes after parser()");

            Ok(parser)
        }
    }

    #[cfg(feature = "old_school")]
    pub fn from_paged_file<F: crate::host_api::RfvpFile + 'static>(
        file: F,
        len: usize,
        nls: Nls,
        page_size: usize,
        page_count: usize,
    ) -> Result<Self> {
        if len == 0 || page_size == 0 || page_count == 0 {
            return Err(anyhow::anyhow!("invalid HCB page cache configuration"));
        }
        Self::from_source(
            Box::new(PagedHcbSource::new(file, len, page_size, page_count)),
            nls,
        )
    }

    #[cfg(feature = "old_school")]
    fn from_source(source: Box<dyn HcbByteSource>, nls: Nls) -> Result<Self> {
        let mut parser = Parser {
            source,
            window: core::cell::UnsafeCell::new(FetchWindow { start: 0, len: 0, bytes: [0; 32] }),
            nls,
            sys_desc_offset: 0,
            entry_point: 0,
            non_volatile_global_count: 0,
            volatile_global_count: 0,
            custom_syscall_count: 0,
            game_mode: 0,
            game_mode_reserved: 0,
            game_title: String::new(),
            syscall_count: 0,
            syscalls: SyscallMap::default(),
        };

        parser.parser()?;
        Ok(parser)
    }

    /// safely read a u8 from the buffer
    pub fn read_u8(&self, offset: usize) -> Result<u8> {
        #[cfg(feature = "old_school")]
        {
            return Ok(self.fetch::<1>(offset)?[0]);
        }
        #[cfg(not(feature = "old_school"))]
        {
            if offset >= self.buffer.len() {
                return Err(anyhow::anyhow!("offset out of bounds"));
            }
            Ok(self.buffer[offset])
        }
    }

    /// safely read a little-endian u16 from the buffer
    pub fn read_u16(&self, offset: usize) -> Result<u16> {
        #[cfg(feature = "old_school")]
        {
            return Ok(u16::from_le_bytes(self.fetch::<2>(offset)?));
        }
        #[cfg(not(feature = "old_school"))]
        {
            if offset + 1 >= self.buffer.len() {
                return Err(anyhow::anyhow!("offset out of bounds"));
            }
            Ok(u16::from_le_bytes([
                self.buffer[offset],
                self.buffer[offset + 1],
            ]))
        }
    }

    /// safely read a little-endian u32 from the buffer
    pub fn read_u32(&self, offset: usize) -> Result<u32> {
        #[cfg(feature = "old_school")]
        {
            return Ok(u32::from_le_bytes(self.fetch::<4>(offset)?));
        }
        #[cfg(not(feature = "old_school"))]
        {
            if offset + 3 >= self.buffer.len() {
                return Err(anyhow::anyhow!("offset out of bounds"));
            }
            Ok(u32::from_le_bytes([
                self.buffer[offset],
                self.buffer[offset + 1],
                self.buffer[offset + 2],
                self.buffer[offset + 3],
            ]))
        }
    }

    /// safely read a little-endian i8 from the buffer
    pub fn read_i8(&self, offset: usize) -> Result<i8> {
        #[cfg(feature = "old_school")]
        {
            return Ok(self.read_u8(offset)? as i8);
        }
        #[cfg(not(feature = "old_school"))]
        {
            if offset >= self.buffer.len() {
                return Err(anyhow::anyhow!("offset out of bounds"));
            }
            Ok(self.buffer[offset] as i8)
        }
    }

    /// safely read a little-endian i16 from the buffer
    pub fn read_i16(&self, offset: usize) -> Result<i16> {
        #[cfg(feature = "old_school")]
        {
            return Ok(i16::from_le_bytes(self.fetch::<2>(offset)?));
        }
        #[cfg(not(feature = "old_school"))]
        {
            if offset + 1 >= self.buffer.len() {
                return Err(anyhow::anyhow!("offset out of bounds"));
            }
            Ok(i16::from_le_bytes([
                self.buffer[offset],
                self.buffer[offset + 1],
            ]))
        }
    }

    /// safely read a little-endian i32 from the buffer
    pub fn read_i32(&self, offset: usize) -> Result<i32> {
        #[cfg(feature = "old_school")]
        {
            return Ok(i32::from_le_bytes(self.fetch::<4>(offset)?));
        }
        #[cfg(not(feature = "old_school"))]
        {
            if offset + 3 >= self.buffer.len() {
                return Err(anyhow::anyhow!("offset out of bounds"));
            }
            Ok(i32::from_le_bytes([
                self.buffer[offset],
                self.buffer[offset + 1],
                self.buffer[offset + 2],
                self.buffer[offset + 3],
            ]))
        }
    }

    /// safely read a little-endian f32 from the buffer
    pub fn read_f32(&self, offset: usize) -> Result<f32> {
        #[cfg(feature = "old_school")]
        {
            return Ok(f32::from_le_bytes(self.fetch::<4>(offset)?));
        }
        #[cfg(not(feature = "old_school"))]
        {
            if offset + 3 >= self.buffer.len() {
                return Err(anyhow::anyhow!("offset out of bounds"));
            }
            Ok(f32::from_le_bytes([
                self.buffer[offset],
                self.buffer[offset + 1],
                self.buffer[offset + 2],
                self.buffer[offset + 3],
            ]))
        }
    }

    /// safe read a c-style string from the buffer with string length
    /// (with null terminator)
    /// then convert it to a UTF-8 string due to the NLS
    pub fn read_cstring(&self, offset: usize, len: usize) -> Result<String> {
        #[cfg(feature = "old_school")]
        {
            if offset
                .checked_add(len)
                .is_none_or(|end| end > self.source.len())
            {
                return Err(anyhow::anyhow!("offset out of bounds"));
            }
            let mut string = alloc::vec![0u8; len];
            self.source.read_bytes(offset, &mut string)?;
            if let Some(end) = string.iter().position(|&b| b == 0) {
                string.truncate(end);
            }
            return self.decode_string_bytes(&string);
        }

        #[cfg(not(feature = "old_school"))]
        {
            if offset + len >= self.buffer.len() {
                return Err(anyhow::anyhow!("offset out of bounds"));
            }
            let mut string = Vec::new();
            for i in 0..len {
                if self.buffer[offset + i] == 0 {
                    break;
                }
                string.push(self.buffer[offset + i]);
            }

            if string.ends_with(&[0]) {
                string.pop();
            }

            self.decode_string_bytes(&string)
        }
    }

    pub(crate) fn decode_string_bytes(&self, string: &[u8]) -> Result<String> {
        let (s, errors) = crate::text_codec::decode(self.nls.clone(), string);
        if errors {
            log::error!("failed to decode string as {:?}", self.nls);
        }
        Ok(s)
    }

    fn parser(&mut self) -> Result<()> {
        uefi_parser_stage!("[UEFI] Parser::parser entered");
        let mut off = 0usize;
        self.sys_desc_offset = self.read_u32(off)?;
        uefi_parser_stage!(
            "[UEFI] Parser::parser sys_desc_offset={}",
            self.sys_desc_offset
        );

        off = self.sys_desc_offset as usize;
        self.entry_point = self.read_u32(off)?;
        uefi_parser_stage!("[UEFI] Parser::parser entry_point={}", self.entry_point);
        off += size_of::<u32>();

        self.non_volatile_global_count = self.read_u16(off)?;
        uefi_parser_stage!(
            "[UEFI] Parser::parser non_volatile_global_count={}",
            self.non_volatile_global_count
        );
        off += size_of::<u16>();

        self.volatile_global_count = self.read_u16(off)?;
        uefi_parser_stage!(
            "[UEFI] Parser::parser volatile_global_count={}",
            self.volatile_global_count
        );
        off += size_of::<u16>();

        self.game_mode = self.read_u8(off)? as u8;
        uefi_parser_stage!("[UEFI] Parser::parser game_mode={}", self.game_mode);
        off += size_of::<u8>();

        self.game_mode_reserved = self.read_u8(off)? as u8;
        uefi_parser_stage!(
            "[UEFI] Parser::parser game_mode_reserved={}",
            self.game_mode_reserved
        );
        off += size_of::<u8>();

        let title_len = self.read_u8(off)?;
        uefi_parser_stage!("[UEFI] Parser::parser title_len={}", title_len);
        off += size_of::<u8>();

        uefi_parser_stage!("[UEFI] Parser::parser before game_title");
        self.game_title = self.read_cstring(off, title_len as usize)?;
        uefi_parser_stage!("[UEFI] Parser::parser after game_title");
        off += title_len as usize;

        self.syscall_count = self.read_u16(off)?;
        uefi_parser_stage!("[UEFI] Parser::parser syscall_count={}", self.syscall_count);
        off += size_of::<u16>();

        for i in 0..self.syscall_count {
            let args = self.read_u8(off)?;
            off += size_of::<u8>();

            let name_len = self.read_u8(off)?;
            off += size_of::<u8>();

            let name = self.read_cstring(off, name_len as usize)?;
            off += name_len as usize;

            self.syscalls.insert(i as usize, Syscall { args, name });
        }

        self.custom_syscall_count = self.read_u16(off)?;
        uefi_parser_stage!(
            "[UEFI] Parser::parser custom_syscall_count={}",
            self.custom_syscall_count
        );
        if self.custom_syscall_count > 0 {
            log::warn!("custom syscall count: {}", self.custom_syscall_count);
        }

        Ok(())
    }

    pub fn is_code_area(&self, addr: u32) -> bool {
        addr >= 4 && addr < self.sys_desc_offset
    }

    pub fn get_syscall(&self, id: u16) -> Option<&Syscall> {
        self.syscalls.get(&(id as usize))
    }

    pub fn get_all_syscalls(&self) -> &SyscallMap {
        &self.syscalls
    }

    pub fn get_title(&self) -> String {
        self.game_title.clone()
    }

    pub fn get_non_volatile_global_count(&self) -> u16 {
        self.non_volatile_global_count
    }

    pub fn get_volatile_global_count(&self) -> u16 {
        self.volatile_global_count
    }

    pub fn get_screen_size(&self) -> (u32, u32) {
        match self.game_mode {
            0 => (640, 480),
            1 => (800, 600),
            2 => (1024, 768),
            3 => (1280, 960),
            4 => (1600, 1200),
            5 => (640, 480),
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
            _ => {
                log::error!(
                    "unknown resolution: {}, use 640x480 as defualt",
                    self.game_mode
                );
                (640, 480)
            }
        }
    }

    pub fn get_game_mode(&self) -> u8 {
        self.game_mode
    }

    pub fn get_game_mode_reserved(&self) -> u8 {
        self.game_mode_reserved
    }

    pub fn get_entry_point(&self) -> u32 {
        self.entry_point
    }

    pub fn get_custom_syscall_count(&self) -> u16 {
        self.custom_syscall_count
    }

    // the upper bound of the code area
    pub fn get_sys_desc_offset(&self) -> u32 {
        self.sys_desc_offset
    }
}

#[cfg(feature = "old_school")]
trait HcbByteSource {
    fn len(&self) -> usize;
    fn read_byte(&self, offset: usize) -> Result<u8>;
    fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<()>;

}

#[cfg(feature = "old_school")]
#[derive(Debug)]
struct MemoryHcbSource {
    bytes: Vec<u8>,
}

#[cfg(feature = "old_school")]
impl HcbByteSource for MemoryHcbSource {
    fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<()> {
        let end = offset.checked_add(out.len())
            .ok_or_else(|| anyhow::anyhow!("offset out of bounds"))?;
        let bytes = self.bytes.get(offset..end)
            .ok_or_else(|| anyhow::anyhow!("offset out of bounds"))?;
        out.copy_from_slice(bytes);
        Ok(())
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn read_byte(&self, offset: usize) -> Result<u8> {
        self.bytes
            .get(offset)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("offset out of bounds"))
    }
}

#[cfg(feature = "old_school")]
struct HcbPage {
    index: usize,
    valid_len: usize,
    age: u32,
    data: Vec<u8>,
}

#[cfg(feature = "old_school")]
struct PagedHcbState<F: crate::host_api::RfvpFile> {
    file: F,
    tick: u32,
    pages: Vec<HcbPage>,
    /// For each page of the file, its slot in `pages` plus one (0: not cached): a lookup per
    /// page change instead of a search of every cached page.
    slot_of: Vec<u16>,
}

#[cfg(feature = "old_school")]
struct PagedHcbSource<F: crate::host_api::RfvpFile> {
    len: usize,
    page_shift: u32,
    page_count: usize,
    state: spin::Mutex<PagedHcbState<F>>,
}

#[cfg(feature = "old_school")]
impl<F: crate::host_api::RfvpFile> PagedHcbSource<F> {
    fn new(file: F, len: usize, page_size: usize, page_count: usize) -> Self {
        // Pages are a power of two, so a page and an offset in it are a shift and a mask.
        let page_shift = page_size.next_power_of_two().trailing_zeros();
        let total_pages = (len >> page_shift) + 1;
        Self {
            len,
            page_shift,
            page_count: page_count.clamp(1, u16::MAX as usize - 1),
            state: spin::Mutex::new(PagedHcbState {
                file,
                tick: 0,
                pages: Vec::new(),
                slot_of: alloc::vec![0; total_pages],
            }),
        }
    }

    /// The slot holding page `page_index`, read from the file (in place of the page used
    /// longest ago) if it is not cached.
    fn slot(&self, state: &mut PagedHcbState<F>, page_index: usize) -> Result<usize> {
        state.tick = state.tick.wrapping_add(1);
        let tick = state.tick;
        let cached = state.slot_of[page_index] as usize;
        if cached != 0 {
            state.pages[cached - 1].age = tick;
            return Ok(cached - 1);
        }
        let page_size = 1usize << self.page_shift;
        let slot = if state.pages.len() < self.page_count {
            state.pages.push(HcbPage { index: usize::MAX, valid_len: 0, age: 0, data: alloc::vec![0; page_size] });
            state.pages.len() - 1
        } else {
            let victim = state
                .pages
                .iter()
                .enumerate()
                .max_by_key(|(_, page)| tick.wrapping_sub(page.age))
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            let old = state.pages[victim].index;
            if old != usize::MAX {
                state.slot_of[old] = 0;
            }
            victim
        };
        let PagedHcbState { file, pages, slot_of, .. } = state;
        let page = &mut pages[slot];
        let started = crate::perf::start();
        let read = file
            .read_at((page_index << self.page_shift) as u64, &mut page.data)
            .map_err(|err| anyhow::anyhow!("HCB page read failed: {:?}", err))?;
        crate::perf::stop(crate::perf::SCRIPT_IO, started);
        crate::perf::SCRIPT_PAGE_READS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        if read == 0 {
            page.index = usize::MAX;
            return Err(anyhow::anyhow!("HCB page read returned EOF"));
        }
        page.index = page_index;
        page.valid_len = read;
        page.age = tick;
        slot_of[page_index] = (slot + 1) as u16;
        Ok(slot)
    }
}

#[cfg(feature = "old_school")]
impl<F: crate::host_api::RfvpFile> HcbByteSource for PagedHcbSource<F> {
    fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<()> {
        if offset.checked_add(out.len()).is_none_or(|end| end > self.len) {
            return Err(anyhow::anyhow!("offset out of bounds"));
        }
        let mut state = self.state.lock();
        let mask = (1usize << self.page_shift) - 1;
        let mut done = 0;
        while done < out.len() {
            let at = offset + done;
            let slot = self.slot(&mut state, at >> self.page_shift)?;
            let page = &state.pages[slot];
            let from = at & mask;
            let n = (out.len() - done).min(page.valid_len.saturating_sub(from));
            if n == 0 {
                return Err(anyhow::anyhow!("offset out of bounds"));
            }
            out[done..done + n].copy_from_slice(&page.data[from..from + n]);
            done += n;
        }
        Ok(())
    }

    fn len(&self) -> usize {
        self.len
    }

    fn read_byte(&self, offset: usize) -> Result<u8> {
        let mut byte = [0u8; 1];
        self.read_bytes(offset, &mut byte)?;
        Ok(byte[0])
    }
}

#[cfg(all(test, not(feature = "no_std")))]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parser() {
        let filepath = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testcase/AstralAirFinale.hcb"
        ));

        let parser = Parser::new(filepath, Nls::ShiftJIS).unwrap();
        log::debug!("{:?}", parser);
    }
}

#[cfg(all(test, feature = "old_school"))]
mod paged_operand_tests {
    use super::*;
    use crate::host_api::{RfvpFile, RfvpResult};

    struct File(Vec<u8>);
    impl RfvpFile for File {
        fn len(&mut self) -> RfvpResult<u64> { Ok(self.0.len() as u64) }
        fn read_at(&mut self, offset: u64, out: &mut [u8]) -> RfvpResult<usize> {
            let start = (offset as usize).min(self.0.len());
            let n = out.len().min(self.0.len() - start);
            out[..n].copy_from_slice(&self.0[start..start + n]);
            Ok(n)
        }
    }

    #[test]
    fn operands_cross_pages_survive_eviction_and_reject_overflow() {
        let bytes: Vec<u8> = (0..35).collect();
        let memory = MemoryHcbSource { bytes: bytes.clone() };
        let paged = PagedHcbSource::new(File(bytes), 35, 8, 2);
        for offset in [0, 2, 7, 15, 24, 0, 31, 8, 16] {
            let mut expected = [0; 4];
            let mut actual = [0; 4];
            memory.read_bytes(offset, &mut expected).unwrap();
            paged.read_bytes(offset, &mut actual).unwrap();
            assert_eq!(actual, expected);
            // Repeat to exercise the cached, single-lock path.
            paged.read_bytes(offset, &mut actual).unwrap();
            assert_eq!(actual, expected);
        }
        assert!(paged.read_bytes(32, &mut [0; 4]).is_err());
        assert!(paged.read_bytes(usize::MAX, &mut [0; 4]).is_err());
        assert!(memory.read_bytes(usize::MAX, &mut [0; 4]).is_err());
    }
}
