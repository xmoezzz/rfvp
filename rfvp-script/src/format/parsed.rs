use std::sync::Arc;

use anyhow::{anyhow, Context, Result};

use super::ScriptLayout;

#[derive(Clone, Debug, Default)]
pub struct ScriptHeader {
    pub title: Option<String>,
    pub screen_w: Option<u32>,
    pub screen_h: Option<u32>,
    pub globals_count: Option<u32>,
    pub syscalls_count: Option<u32>,
}

/// Parsed script container.
///
/// For now, we only extract:
/// - `bytecode` (a view into the original file bytes)
/// - a small `header` summary from optional layout fields
#[derive(Clone, Debug)]
pub struct ParsedScript {
    pub bytes: Arc<[u8]>,
    pub header: ScriptHeader,
    pub bytecode_off: u32,
    pub bytecode: Arc<[u8]>,
}

impl ParsedScript {
    pub fn parse(bytes: Arc<[u8]>, layout: &ScriptLayout) -> Result<Self> {
        let len = bytes.len() as u32;
        if layout.bytecode_off >= len {
            return Err(anyhow!(
                "bytecode_off=0x{:X} is out of range (file_len=0x{:X})",
                layout.bytecode_off,
                len
            ));
        }

        let bc_end = match layout.bytecode_len {
            Some(n) => layout.bytecode_off.saturating_add(n).min(len),
            None => len,
        };

        let bytecode = bytes[layout.bytecode_off as usize..bc_end as usize].to_vec().into();

        let mut header = ScriptHeader::default();

        if let (Some(off), Some(l)) = (layout.title_off, layout.title_len) {
            if off.saturating_add(l) <= len {
                let raw = &bytes[off as usize..(off + l) as usize];
                // Accept non-null-terminated UTF-8; strip trailing nulls.
                let raw = raw.split(|b| *b == 0).next().unwrap_or(raw);
                header.title = Some(String::from_utf8_lossy(raw).to_string());
            }
        }

        if let Some(off) = layout.screen_w_off {
            if off + 4 <= len {
                header.screen_w = Some(u32::from_le_bytes(bytes[off as usize..off as usize + 4].try_into().unwrap()));
            }
        }
        if let Some(off) = layout.screen_h_off {
            if off + 4 <= len {
                header.screen_h = Some(u32::from_le_bytes(bytes[off as usize..off as usize + 4].try_into().unwrap()));
            }
        }

        header.globals_count = layout.globals_count;
        header.syscalls_count = layout.syscalls_count;

        Ok(Self { bytes, header, bytecode_off: layout.bytecode_off, bytecode })
    }

    pub fn bytecode_bytes(&self) -> &[u8] {
        &self.bytecode
    }

    pub fn file_bytes(&self) -> &[u8] {
        &self.bytes
    }
}
