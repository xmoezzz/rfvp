use std::collections::HashMap;

use anyhow::Result;
use js_sys::{Array, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;

use crate::app::App;
use crate::boot::app_config;
use crate::script::parser::{Nls, Parser};
use crate::subsystem::anzu_scene::AnzuScene;
use crate::subsystem::resources::thread_manager::ThreadManager;
use crate::subsystem::resources::vfs::Vfs;

#[wasm_bindgen]
pub async fn start_rfvp_from_directory(
    canvas_id: String,
    nls: String,
    files: Array,
) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let nls: Nls = nls
        .parse()
        .map_err(|e| JsValue::from_str(&format!("invalid NLS: {e:#}")))?;

    let files = js_array_to_file_map(files)
        .map_err(|e| JsValue::from_str(&format!("failed to import selected directory: {e:#}")))?;
    let vfs = Vfs::from_memory_files(nls, files)
        .map_err(|e| JsValue::from_str(&format!("failed to build wasm VFS: {e:#}")))?;

    let hcb = vfs
        .first_hcb_bytes()
        .map_err(|e| JsValue::from_str(&format!("failed to locate hcb: {e:#}")))?;
    let parser = Parser::from_bytes(hcb, nls)
        .map_err(|e| JsValue::from_str(&format!("failed to parse hcb: {e:#}")))?;

    let title = parser.get_title();
    let size = parser.get_screen_size();
    let script_engine = ThreadManager::new();

    App::app_with_config(app_config(&title, size))
        .with_scene::<AnzuScene>()
        .with_script_engine(script_engine)
        .with_window_title(&title)
        .with_window_size(size)
        .with_parser(parser)
        .with_wasm_vfs(vfs)
        .run_web(&canvas_id)
        .await
        .map_err(|e| JsValue::from_str(&format!("rfvp wasm start failed: {e:#}")))
}

fn js_array_to_file_map(files: Array) -> Result<HashMap<String, Vec<u8>>> {
    let mut raw: Vec<(String, Vec<u8>)> = Vec::new();

    for i in 0..files.length() {
        let item = files.get(i);

        let path_value = Reflect::get(&item, &JsValue::from_str("path"))
            .map_err(|_| anyhow::anyhow!("files[{i}].path is inaccessible"))?;
        let path = path_value
            .as_string()
            .ok_or_else(|| anyhow::anyhow!("files[{i}].path is not a string"))?;

        let data_value = Reflect::get(&item, &JsValue::from_str("data"))
            .map_err(|_| anyhow::anyhow!("files[{i}].data is inaccessible"))?;
        let data = Uint8Array::new(&data_value);
        let mut bytes = vec![0u8; data.length() as usize];
        data.copy_to(&mut bytes);

        raw.push((path, bytes));
    }

    let root_prefix = common_selected_root(&raw);
    let mut out = HashMap::new();

    for (path, bytes) in raw {
        let mut normalized = path.replace('\\', "/");
        if let Some(prefix) = root_prefix.as_ref() {
            if let Some(stripped) = normalized.strip_prefix(prefix) {
                normalized = stripped.trim_start_matches('/').to_string();
            }
        }
        normalized = normalized.trim_start_matches("./").trim_start_matches('/').to_string();
        if !normalized.is_empty() {
            out.insert(normalized, bytes);
        }
    }

    Ok(out)
}

fn common_selected_root(files: &[(String, Vec<u8>)]) -> Option<String> {
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
