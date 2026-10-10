#![cfg(target_arch = "wasm32")]

use std::collections::BTreeMap;

use anyhow::{Context, Result};

use crate::host_api::{RfvpError, RfvpFile, RfvpResult, RfvpStorage};
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
        let files: Vec<WasmFileMetadata> =
            serde_json::from_str(files_json).context("parse wasm selected-file metadata JSON")?;

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

    pub fn lookup(&self, path: &str) -> Option<&WasmFileRef> {
        let key = normalize_key(path);
        self.files.get(&key)
    }

    fn read_all(&self, file_ref: &WasmFileRef) -> Result<Vec<u8>> {
        if file_ref.size > u32::MAX as u64 {
            anyhow::bail!("wasm read_all too large: {} bytes", file_ref.size);
        }
        wasm_read_range(file_ref.id, 0, file_ref.size as usize)
    }
}

/// A selected file, read through the page's range requests.
struct WasmFile(WasmFileRef);

impl RfvpFile for WasmFile {
    fn len(&mut self) -> RfvpResult<u64> {
        Ok(self.0.size)
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> RfvpResult<usize> {
        if offset >= self.0.size {
            return Ok(0);
        }
        let n = buf.len().min((self.0.size - offset) as usize);
        let bytes = wasm_read_range(self.0.id, offset, n).map_err(|_| RfvpError::Io)?;
        buf[..n].copy_from_slice(&bytes);
        Ok(n)
    }
}

/// The selected game directory, read-only. Names are matched without regard to case.
impl RfvpStorage for WasmAppPath {
    fn read(&self, path: &str) -> RfvpResult<Vec<u8>> {
        let file_ref = self.lookup(path).ok_or(RfvpError::NotFound)?;
        self.read_all(file_ref).map_err(|_| RfvpError::Io)
    }

    fn write(&self, _path: &str, _bytes: &[u8]) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn exists(&self, path: &str) -> bool {
        self.lookup(path).is_some()
    }

    fn remove(&self, _path: &str) -> RfvpResult<()> {
        Err(RfvpError::Unsupported)
    }

    fn list(&self, dir: &str) -> RfvpResult<Vec<String>> {
        let dir = normalize_key(dir);
        Ok(self
            .files
            .keys()
            .filter_map(|path| {
                let name = if dir.is_empty() {
                    path.as_str()
                } else {
                    path.strip_prefix(dir.as_str())?.strip_prefix('/')?
                };
                is_root_file(name).then(|| name.to_string())
            })
            .collect())
    }

    fn open(&self, path: &str) -> RfvpResult<Box<dyn RfvpFile + Send>> {
        let file_ref = self.lookup(path).ok_or(RfvpError::NotFound)?;
        Ok(Box::new(WasmFile(file_ref.clone())))
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
        .filter_map(|(path, _)| {
            path.replace('\\', "/")
                .split('/')
                .next()
                .map(str::to_string)
        })
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
