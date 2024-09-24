use std::path::{Path, PathBuf};

use anyhow::bail;
use tracing::{debug, instrument, trace};

use crate::asset::LayeredAssetIo;

#[instrument]
fn try_assets_directory(path: &Path) -> anyhow::Result<Option<LayeredAssetIo>> {
    debug!("Trying assets directory {:?}...", path);
    if !path.is_dir() {
        debug!("Cannot use {:?} as assets directory, not a directory", path);
        return Ok(None);
    }
    let mut result = LayeredAssetIo::new();
    // try to use "data" directory first and them data.rom file
    if let Ok(patch_dir) = path.join("patch").canonicalize() {
        trace!("Trying patch directory {:?}...", patch_dir);
        match result.try_with_dir(&patch_dir) {
            Ok(_) => trace!("Using patch directory {:?}", patch_dir),
            Err(err) => trace!("Cannot use {:?} as assets directory: {}", path, err),
        }
    }
    if let Ok(patch_rom) = path.join("patch.rom").canonicalize() {
        trace!("Trying patch ROM {:?}...", patch_rom);
        match result.try_with_rom(&patch_rom) {
            Ok(_) => trace!("Using patch ROM {:?}", patch_rom),
            Err(err) => trace!("Cannot use {:?} as assets directory: {}", path, err),
        }
    }
    if let Ok(data_dir) = path.join("data").canonicalize() {
        trace!("Trying data directory {:?}...", data_dir);
        match result.try_with_dir(&data_dir) {
            Ok(_) => trace!("Using data directory {:?}", data_dir),
            Err(err) => trace!("Cannot use {:?} as assets directory: {}", path, err),
        }
    }
    if let Ok(data_rom) = path.join("data.rom").canonicalize() {
        trace!("Trying data ROM {:?}...", data_rom);
        match result.try_with_rom(&data_rom) {
            Ok(_) => trace!("Using data ROM {:?}", data_rom),
            Err(err) => trace!("Cannot use {:?} as assets directory: {}", path, err),
        }
    }

    if result.is_empty() {
        trace!("Cannot use {:?} as assets directory, no data found", path);
        Ok(None)
    } else {
        Ok(Some(result))
    }
}

/// Implements the logic for locating game assets.
///
/// The asset directory is expected to contain a "data" directory or a "data.rom" file.
///
/// It can also contain both, in which case the assets are searched in the "data" directory first, then in the "data.rom" file. This can be useful for translation mods.
///
/// The candidate asset directories are (in order)
/// 1. The directory specified on the command line with the `--assets-dir` option
/// 2. The directory specified in the `rfvp_ASSETS` environment variable
/// 3. The directory "assets" next to the executable
/// 4. The directory "assets" in the current working directory
/// 5. The user's shared data directory (see `dirs::data_dir`, `/home/alice/.local/share/rfvp/assets` / `C:\Users\Alice\AppData\Roaming\rfvp\assets` / `/Users/Alice/Library/Application Support/rfvp/assets`)
///
/// The used asset directory is the first one having a "data" directory or a "data.rom" file.
#[allow(clippy::match_result_ok)]
pub fn locate_assets(cli_assets: Option<&Path>) -> anyhow::Result<LayeredAssetIo> {
    // First, try the assets directory specified on the command line
    // Then, try the assets directory specified in the environment
    // Then, try the assets directory next to the executable
    // Then, try the assets directory in the current working directory
    // Then, try the user's shared data directory

    let mut try_list = Vec::new();

    if let Some(cli_assets) = cli_assets {
        try_list.push(cli_assets.to_path_buf());
    }

    if let Some(env_assets) = std::env::var_os("rfvp_ASSETS") {
        try_list.push(PathBuf::from(env_assets));
    }

    if let Some(exe_assets) = std::env::current_exe()?.parent().map(|p| p.join("assets")) {
        try_list.push(exe_assets);
    }

    if let Some(cwd_assets) = std::env::current_dir()?.join("assets").canonicalize().ok() {
        try_list.push(cwd_assets);
    }

    // |Platform | Value                                    | Example                                  |
    // | ------- | ---------------------------------------- | ---------------------------------------- |
    // | Linux   | `$XDG_DATA_HOME` or `$HOME`/.local/share | /home/alice/.local/share                 |
    // | macOS   | `$HOME`/Library/Application Support      | /Users/Alice/Library/Application Support |
    // | Windows | `{FOLDERID_RoamingAppData}`              | C:\Users\Alice\AppData\Roaming           |
    if let Some(shared_assets) = dirs_next::data_dir().map(|p| p.join("rfvp").join("assets")) {
        try_list.push(shared_assets);
    }

    for path in try_list.iter() {
        if let Some(result) = try_assets_directory(path)? {
            return Ok(result);
        }
    }

    bail!("Failed to locate assets directory, tried: {:#?}", try_list);
}
