#![cfg(target_arch = "wasm32")]

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};

use anyhow::{Context, Result};
use js_sys::Uint8Array;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = rfvpReadFileRange)]
    fn rfvp_read_file_range(file_id: u32, offset: f64, len: u32) -> Uint8Array;
}

#[derive(Debug, Clone)]
pub struct WasmFileRef {
    pub id: u32,
    pub size: u64,
}

#[derive(Debug, Clone, Default)]
pub struct WasmAppPath {
    files: BTreeMap<String, WasmFileRef>,
}

#[derive(Debug, Deserialize)]
struct WasmFileMetadata {
    path: String,
    id: u32,
    size: u64,
}

impl WasmAppPath {
    pub fn from_metadata_json(files_json: &str) -> Result<Self> {
        let files: Vec<WasmFileMetadata> = serde_json::from_str(files_json)
            .context("parse wasm selected-file metadata JSON")?;

        let raw: Vec<(String, WasmFileRef)> = files
            .into_iter()
            .map(|file| {
                (
                    file.path,
                    WasmFileRef {
                        id: file.id,
                        size: file.size,
                    },
                )
            })
            .collect();

        let root_prefix = common_selected_root(&raw);
        let mut out = BTreeMap::new();

        for (path, file_ref) in raw {
            let mut normalized = path.replace('\\', "/");
            if let Some(prefix) = root_prefix.as_ref() {
                if let Some(stripped) = normalized.strip_prefix(prefix) {
                    normalized = stripped.trim_start_matches('/').to_string();
                }
            }
            let key = normalize_key(&normalized);
            if !key.is_empty() {
                out.insert(key, file_ref);
            }
        }

        Ok(Self { files: out })
    }

    pub fn first_root_hcb_bytes(&self) -> Result<Vec<u8>> {
        let Some((path, file_ref)) = self
            .files
            .iter()
            .filter(|(path, _)| is_root_file(path) && path.ends_with(".hcb"))
            .next()
        else {
            anyhow::bail!("No root-level .hcb file found in selected browser directory");
        };

        log_wasm(&format!("using wasm hcb: {path} size={}", file_ref.size));
        self.read_all(file_ref)
            .with_context(|| format!("read wasm hcb {path}"))
    }

    pub fn root_bin_files(&self) -> Vec<(String, WasmFileRef)> {
        self.files
            .iter()
            .filter(|(path, _)| is_root_file(path) && path.ends_with(".bin"))
            .map(|(path, file_ref)| (path.clone(), file_ref.clone()))
            .collect()
    }

    pub fn lookup(&self, path: &str) -> Option<&WasmFileRef> {
        let key = normalize_key(path);
        self.files.get(&key)
    }

    pub fn known_root_files_sample(&self) -> Vec<String> {
        self.files
            .keys()
            .filter(|path| is_root_file(path))
            .take(64)
            .cloned()
            .collect()
    }

    fn read_all(&self, file_ref: &WasmFileRef) -> Result<Vec<u8>> {
        if file_ref.size > u32::MAX as u64 {
            anyhow::bail!("wasm read_all too large: {} bytes", file_ref.size);
        }
        wasm_read_range(file_ref.id, 0, file_ref.size as usize)
    }
}

#[derive(Debug, Clone)]
pub struct WasmRangeStream {
    file_id: u32,
    file_size: u64,
    start: u64,
    len: u64,
    pos: u64,
    cache_start: u64,
    cache: Vec<u8>,
}

impl WasmRangeStream {
    const CACHE_CHUNK: usize = 1024 * 1024;

    pub fn new(file_ref: &WasmFileRef, start: u64, len: u64) -> Result<Self> {
        let end = start
            .checked_add(len)
            .ok_or_else(|| anyhow::anyhow!("wasm range overflow"))?;
        if end > file_ref.size {
            anyhow::bail!(
                "wasm range out of bounds: start={} len={} file_size={}",
                start,
                len,
                file_ref.size
            );
        }
        Ok(Self {
            file_id: file_ref.id,
            file_size: file_ref.size,
            start,
            len,
            pos: 0,
            cache_start: u64::MAX,
            cache: Vec::new(),
        })
    }

    fn clamp_pos(&self, p: i128) -> u64 {
        if p <= 0 {
            return 0;
        }
        let p = p as u128;
        if p >= self.len as u128 {
            return self.len;
        }
        p as u64
    }

    fn fill_cache(&mut self) -> std::io::Result<()> {
        let absolute = self.start + self.pos;
        let remaining = self.len - self.pos;
        let to_read = (remaining as usize).min(Self::CACHE_CHUNK);
        let bytes = wasm_read_range(self.file_id, absolute, to_read)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        self.cache_start = self.pos;
        self.cache = bytes;
        Ok(())
    }
}

impl Read for WasmRangeStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.len || buf.is_empty() {
            return Ok(0);
        }

        let mut written = 0usize;
        while written < buf.len() && self.pos < self.len {
            let cache_end = self.cache_start.saturating_add(self.cache.len() as u64);
            if self.cache.is_empty() || self.pos < self.cache_start || self.pos >= cache_end {
                self.fill_cache()?;
                if self.cache.is_empty() {
                    break;
                }
            }

            let cache_offset = (self.pos - self.cache_start) as usize;
            let available = self.cache.len().saturating_sub(cache_offset);
            if available == 0 {
                self.cache.clear();
                continue;
            }

            let remaining_stream = (self.len - self.pos) as usize;
            let to_copy = (buf.len() - written).min(available).min(remaining_stream);
            buf[written..written + to_copy]
                .copy_from_slice(&self.cache[cache_offset..cache_offset + to_copy]);
            self.pos += to_copy as u64;
            written += to_copy;
        }

        Ok(written)
    }
}

impl Seek for WasmRangeStream {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(off) => self.clamp_pos(off as i128),
            SeekFrom::End(delta) => self.clamp_pos(self.len as i128 + delta as i128),
            SeekFrom::Current(delta) => self.clamp_pos(self.pos as i128 + delta as i128),
        };
        self.pos = next;
        Ok(self.pos)
    }
}

pub fn wasm_read_range(file_id: u32, offset: u64, len: usize) -> Result<Vec<u8>> {
    if len > u32::MAX as usize {
        anyhow::bail!("wasm range request too large: {len} bytes");
    }
    let arr = rfvp_read_file_range(file_id, offset as f64, len as u32);
    let actual = arr.length() as usize;
    if actual != len {
        anyhow::bail!(
            "wasm range read length mismatch: requested={} actual={} file_id={} offset={}",
            len,
            actual,
            file_id,
            offset
        );
    }
    let mut out = vec![0u8; actual];
    arr.copy_to(&mut out);
    Ok(out)
}

pub fn normalize_key(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_ascii_lowercase()
}

fn is_root_file(path: &str) -> bool {
    !path.is_empty() && !path.contains('/')
}

fn common_selected_root(files: &[(String, WasmFileRef)]) -> Option<String> {
    let mut first_components = files
        .iter()
        .filter_map(|(path, _)| path.replace('\\', "/").split('/').next().map(str::to_string))
        .filter(|s| !s.is_empty());

    let first = first_components.next()?;
    if first_components.all(|c| c == first) {
        Some(first)
    } else {
        None
    }
}

fn log_wasm(msg: &str) {
    web_sys::console::log_1(&JsValue::from_str(msg));
}
