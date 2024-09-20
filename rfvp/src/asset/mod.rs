mod audio;
pub mod bustup;
mod font;
mod locate;
pub mod movie;
pub mod picture;
mod scenario;
mod server;
pub mod texture_archive;

pub mod asset_paths {
    pub const SCENARIO: &str = "/main.snr";
    pub const SYSTEM_FNT: &str = "/system.fnt";
    pub const MSGTEX: &str = "/msgtex.txa";
    pub const NEWRODIN_MEDIUM_FNT: &str = "/newrodin-medium.fnt";
    pub const NEWRODIN_BOLD_FNT: &str = "/newrodin-bold.fnt";
}

pub use locate::locate_assets;
pub use server::{
    AnyAssetIo, AnyAssetServer, Asset, AssetIo, AssetServer, DirAssetIo, LayeredAssetIo, RomAssetIo,
};
