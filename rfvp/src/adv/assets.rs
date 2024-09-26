use std::sync::Arc;
use std::path::{Path, PathBuf};
use anyhow::Result;
use futures::try_join;
use rfvp_core::format::scenario::Scenario;

use crate::asset::AnyAssetServer;

// TODO: this can be done with a macro
#[derive(Clone)]
pub struct AdvAssets {
    pub scenario: Arc<Scenario>,
}


impl AdvAssets {
    pub async fn load(asset_server: &AnyAssetServer, root: impl AsRef<Path>) -> Result<Self> {
        let hcb = Self::find_hcb(root)?;
        // assume hcb is a valid path
        let hcb = hcb.to_string_lossy();
        let result = try_join!(
            asset_server.load(hcb),
        )?;

        Ok(Self {
            scenario: result.0,
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

