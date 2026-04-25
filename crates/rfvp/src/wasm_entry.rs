use anyhow::Result;
use wasm_bindgen::prelude::*;

use crate::app::App;
use crate::boot::app_config;
use crate::script::parser::{Nls, Parser};
use crate::subsystem::anzu_scene::AnzuScene;
use crate::subsystem::resources::thread_manager::ThreadManager;
use crate::subsystem::resources::vfs::Vfs;
use crate::wasm_app_path::WasmAppPath;

#[wasm_bindgen]
pub async fn start_rfvp_from_directory(
    canvas_id: String,
    nls: String,
    files_json: String,
) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let nls: Nls = nls
        .parse()
        .map_err(|e| JsValue::from_str(&format!("invalid NLS: {e:#}")))?;

    let app_path = WasmAppPath::from_metadata_json(&files_json)
        .map_err(|e| JsValue::from_str(&format!("failed to import selected directory: {e:#}")))?;

    let hcb = app_path
        .first_root_hcb_bytes()
        .map_err(|e| JsValue::from_str(&format!("failed to locate hcb: {e:#}")))?;
    let parser = Parser::from_bytes(hcb, nls)
        .map_err(|e| JsValue::from_str(&format!("failed to parse hcb: {e:#}")))?;

    let vfs = Vfs::from_wasm_app_path(nls, app_path)
        .map_err(|e| JsValue::from_str(&format!("failed to build wasm VFS: {e:#}")))?;

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
