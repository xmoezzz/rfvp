use std::{
    fmt::Debug,
    fs::File,
    io,
    io::BufReader,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock, Weak},
};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use bevy_utils::HashMap;
use derive_more::From;
use pollster::FutureExt;
use rfvp_core::format::rom::RomReader;
use rfvp_tasks::{AsyncComputeTaskPool, IoTaskPool};
use tracing::debug;

pub trait Asset: Send + Sync + Sized + 'static {
    fn load_from_bytes(data: Vec<u8>) -> Result<Self>;
}

struct AssetMap<T: Asset>(HashMap<String, Weak<T>>);

impl<T: Asset> Deref for AssetMap<T> {
    type Target = HashMap<String, Weak<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T: Asset> DerefMut for AssetMap<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct AssetServer<Io: AssetIo> {
    io: Io,
    loaded_assets: RwLock<anymap::Map<dyn core::any::Any + Send + Sync>>,
}

impl<Io: AssetIo> AssetServer<Io> {
    pub fn new(io: Io) -> Self {
        Self {
            io,
            loaded_assets: RwLock::new(anymap::Map::new()),
        }
    }

    pub async fn load<T: Asset, P: AsRef<str>>(&self, path: P) -> Result<Arc<T>> {
        let path = path.as_ref();

        if let Some(loaded) = self.loaded_assets.read().unwrap().get::<AssetMap<T>>() {
            if let Some(asset) = loaded.get(path) {
                if let Some(asset) = asset.upgrade() {
                    debug!("Loaded asset from cache: {}", path);
                    return Ok(asset);
                }
            }
        }

        debug!("Loading asset: {}", path);

        // could not find the asset in the cache, load it
        let data = self
            .io
            .read_file(path)
            .await
            .with_context(|| format!("Reading asset {:?}", path))?;

        let asset = AsyncComputeTaskPool::get()
            .spawn(async move { T::load_from_bytes(data) })
            .await?;
        let asset = Arc::new(asset);

        self.loaded_assets
            .write()
            .unwrap()
            .entry::<AssetMap<T>>()
            .or_insert_with(|| AssetMap(HashMap::default()))
            .insert(path.to_string(), Arc::downgrade(&asset));

        Ok(asset)
    }

    /// Load an asset synchronously. This is useful for assets not requiring much CPU time to load.
    /// Though it might cause lockups if the loading is not blazing fast (tm).
    ///
    /// Ideally I want to get rid of all uses of this function
    pub fn load_sync<T: Asset, P: AsRef<str>>(&self, path: P) -> Result<Arc<T>> {
        self.load(path).block_on()
    }
}

pub type AnyAssetServer = AssetServer<AnyAssetIo>;

impl AnyAssetServer {
    #[allow(unused)]
    pub fn new_dir(root_path: PathBuf) -> Self {
        debug!("Using directory for assets: {:?}", root_path);
        Self::new(AnyAssetIo::new_dir(root_path))
    }

    #[allow(unused)]
    pub fn new_rom(rom_path: impl AsRef<Path>) -> Self {
        debug!("Using ROM for assets: {}", rom_path.as_ref().display());
        Self::new(AnyAssetIo::new_rom(rom_path))
    }
}

#[async_trait]
pub trait AssetIo {
    async fn read_file(&self, path: &str) -> Result<Vec<u8>>;
}

#[derive(Debug)]
pub struct DirAssetIo {
    root_path: PathBuf,
}

impl DirAssetIo {
    pub fn new(root_path: PathBuf) -> Self {
        Self { root_path }
    }
}

#[async_trait]
impl AssetIo for DirAssetIo {
    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let full_path = self.root_path.join(path.trim_start_matches('/'));
        IoTaskPool::get()
            .spawn(async move { std::fs::read(full_path) })
            .await
            .with_context(|| {
                format!(
                    "Reading asset {:?} (root_path = {:?})",
                    path, self.root_path
                )
            })
    }
}

pub struct RomAssetIo<S: io::Read + io::Seek + Send + Sync + 'static> {
    rom: Arc<Mutex<RomReader<S>>>,
    label: Option<String>,
}

impl<S: io::Read + io::Seek + Send + Sync + 'static> Debug for RomAssetIo<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RomAssetIo")
            .field(&self.label.as_deref().unwrap_or("unnamed"))
            .finish()
    }
}

impl<S: io::Read + io::Seek + Send + Sync + 'static> RomAssetIo<S> {
    pub fn new(rom: RomReader<S>, label: Option<&str>) -> Self {
        Self {
            rom: Arc::new(Mutex::new(rom)),
            label: label.map(|s| s.to_string()),
        }
    }
}

#[async_trait]
impl<S: io::Read + io::Seek + Send + Sync + 'static> AssetIo for RomAssetIo<S> {
    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let rom = self.rom.clone();
        let path = path.to_string();

        IoTaskPool::get()
            .spawn(async move {
                use io::Read;

                let mut rom = rom.lock().unwrap();
                let file = rom
                    .find_file(&path)
                    .with_context(|| format!("Finding asset {:?}", path))?;
                let mut file = rom
                    .open_file(file)
                    .with_context(|| format!("Opening asset {:?}", path))?;

                let mut data = Vec::new();
                file.read_to_end(&mut data)
                    .with_context(|| format!("Reading asset {:?}", path))?;

                Ok(data)
            })
            .await
    }
}

#[derive(Debug, From)]
pub enum AnyAssetIo {
    Dir(DirAssetIo),
    RomFile(RomAssetIo<BufReader<File>>),
    Layered(LayeredAssetIo),
}

impl AnyAssetIo {
    pub fn new_dir(root_path: PathBuf) -> Self {
        Self::Dir(DirAssetIo::new(root_path))
    }

    pub fn new_rom(rom_path: impl AsRef<Path>) -> Self {
        let rom_path = rom_path.as_ref();
        let rom =
            RomReader::new(BufReader::new(File::open(rom_path).unwrap())).expect("Opening rom");
        Self::RomFile(RomAssetIo::new(
            rom,
            Some(&format!("{}", rom_path.display())),
        ))
    }
}

#[async_trait]
impl AssetIo for AnyAssetIo {
    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        match self {
            Self::Dir(io) => io.read_file(path).await,
            Self::RomFile(io) => io.read_file(path).await,
            Self::Layered(io) => io.read_file(path).await,
        }
    }
}

#[derive(Debug, Default)]
pub struct LayeredAssetIo {
    io: Vec<AnyAssetIo>,
}

impl LayeredAssetIo {
    pub fn new() -> Self {
        Self { io: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.io.is_empty()
    }

    pub fn with(&mut self, io: AnyAssetIo) {
        self.io.push(io);
    }

    pub fn try_with_dir(&mut self, dir_path: impl AsRef<Path>) -> Result<()> {
        let dir_path = dir_path.as_ref();
        let meta = std::fs::metadata(dir_path).with_context(|| {
            format!(
                "Failed to get metadata for {:?}, cannot use as asset directory",
                dir_path
            )
        })?;
        if !meta.is_dir() {
            bail!(
                "{:?} is not a directory, cannot use as asset directory",
                dir_path
            );
        }
        self.with(AnyAssetIo::new_dir(dir_path.to_path_buf()));
        Ok(())
    }

    pub fn try_with_rom(&mut self, rom_path: impl AsRef<Path>) -> Result<()> {
        let rom_path = rom_path.as_ref();
        let meta = std::fs::metadata(rom_path).with_context(|| {
            format!(
                "Failed to get metadata for {:?}, cannot use as asset ROM",
                rom_path
            )
        })?;
        if !meta.is_file() {
            bail!("{:?} is not a file, cannot use as asset ROM", rom_path);
        }
        self.with(AnyAssetIo::new_rom(rom_path));
        Ok(())
    }
}

#[async_trait]
impl AssetIo for LayeredAssetIo {
    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let mut errors = Vec::new();

        for io in &self.io {
            match io.read_file(path).await {
                Ok(data) => return Ok(data),
                Err(err) => errors.push(err),
            }
        }

        Err(anyhow!(
            "Failed to read asset {:?} from all layers: {:?}",
            path,
            errors
        ))
    }
}
