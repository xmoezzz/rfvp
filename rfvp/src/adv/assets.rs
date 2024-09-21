use std::sync::Arc;

use anyhow::Result;
use futures::try_join;
use rfvp_core::format::{font::LazyFont, scenario::Scenario};

use crate::{
    asset::{asset_paths, AnyAssetServer},
    layer::MessageboxTextures,
};

// TODO: this can be done with a macro
#[derive(Clone)]
pub struct AdvAssets {
    pub scenario: Arc<Scenario>,
    pub fonts: AdvFonts,
    pub messagebox_textures: Arc<MessageboxTextures>,
}

#[derive(Clone)]
pub struct AdvFonts {
    pub system_font: Arc<LazyFont>,
    pub medium_font: Arc<LazyFont>,
    pub bold_font: Arc<LazyFont>,
}

impl AdvAssets {
    pub async fn load(asset_server: &AnyAssetServer) -> Result<Self> {
        let result = try_join!(
            asset_server.load(asset_paths::SCENARIO),
            AdvFonts::load(asset_server),
            asset_server.load(asset_paths::MSGTEX),
        )?;

        Ok(Self {
            scenario: result.0,
            fonts: result.1,
            messagebox_textures: result.2,
        })
    }

    pub fn find_hcb(game_path: impl AsRef<Path>) -> Result<PathBuf> {
        let mut path = game_path.as_ref().to_path_buf();
        path.push("*.hcb");

        let macthes: Vec<_> = glob::glob(&path.to_string_lossy())?.flatten().collect();

        if macthes.is_empty() {
            anyhow::bail!("No hcb file found in the game directory");
        }

        Ok(macthes[0].to_path_buf())
    }
}

impl AdvFonts {
    pub async fn load(asset_server: &AnyAssetServer) -> Result<Self> {
        let result = try_join!(
            asset_server.load(asset_paths::SYSTEM_FNT),
            asset_server.load(asset_paths::NEWRODIN_MEDIUM_FNT),
            asset_server.load(asset_paths::NEWRODIN_BOLD_FNT),
        )?;

        Ok(Self {
            system_font: result.0,
            medium_font: result.1,
            bold_font: result.2,
        })
    }
}
