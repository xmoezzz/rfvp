// Copyright (c) 2026 CrySirror contributors. MPL-2.0.
use super::{Metadata, RemoteStorage};
use crate::tasks::TaskPool;
use crate::{
    path::{Mount, VfsPath},
    service::AssetReader,
    vfs::VfsProvider,
    Error, Result,
};
use ::smb::{
    Client, ClientConfig, FileAccessMask, FileCreateArgs, FileDirectoryInformation, GetLen, ReadAt,
    Resource, UncPath,
};
use anyhow::{ensure, Context};
use percent_encoding::percent_decode_str;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const BLOCK: usize = 256 * 1024;
const BLOCKS: usize = 128; // 32 MiB per connection, not per archive/file.
const MAX_ENTRIES: usize = 262_144;
const MAX_DIRS: usize = 128;

fn network_error(error: ::smb::Error) -> Error {
    match &error {
        ::smb::Error::UnexpectedMessageStatus(status)
        | ::smb::Error::ReceivedErrorMessage(status, _)
            if matches!(*status, 0xc0000034 | 0xc000003a | 0xc000000f) =>
        {
            Error::NotFound
        }
        _ => Error::Io(format!("SMB: {error}")),
    }
}
fn core_error(error: anyhow::Error) -> Error {
    if matches!(error.downcast_ref::<Error>(), Some(Error::NotFound)) {
        Error::NotFound
    } else {
        Error::Io(format!("{error:#}"))
    }
}
fn canonical(relative: &str) -> Result<()> {
    if !relative.is_empty() {
        if VfsPath::new(Mount::Asset, relative)?.relative() != relative {
            return Err(Error::Invalid("noncanonical SMB path"));
        }
    }
    Ok(())
}
fn decoded(value: &str) -> anyhow::Result<String> {
    Ok(percent_decode_str(value)
        .decode_utf8()
        .context("SMB URL must contain UTF-8 names")?
        .into_owned())
}
struct OpenFile(::smb::File);
impl Drop for OpenFile {
    fn drop(&mut self) {
        let _ = self.0.close();
    }
}
fn close_resource(resource: Resource) {
    match resource {
        Resource::File(value) => {
            let _ = value.close();
        }
        Resource::Directory(value) => {
            let _ = value.close();
        }
        Resource::Pipe(value) => {
            let _ = value.close();
        }
    }
}
#[derive(Clone, Copy)]
struct Entry {
    metadata: Metadata,
    reparse: bool,
}
struct Cache {
    stamp: u64,
    blocks: BTreeMap<(u64, u64), (u64, Arc<[u8]>)>,
    directories: BTreeMap<String, (u64, Arc<BTreeMap<String, Entry>>)>,
}
impl Cache {
    fn next(&mut self) -> u64 {
        self.stamp = self.stamp.wrapping_add(1);
        self.stamp
    }
}
struct Connection {
    client: Arc<Client>,
    share: UncPath,
    origin: String,
    cache: Arc<Mutex<Cache>>,
    workers: TaskPool,
    next_reader: AtomicU64,
}
impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.client.close();
    }
}

/// A read-only SMB2/3 share with bounded concurrent jobs and a shared block cache.
/// No native OS mount, C SMB library, executable launcher or bulk download.
#[derive(Clone)]
pub struct SmbRoot(Arc<Connection>);
impl SmbRoot {
    /// Connect on a host boot worker. Username/password may come from URL userinfo
    /// or GAME_SMB_USER / GAME_SMB_PASSWORD. They are not in origin().
    /// The returned path is relative to the share and can name a directory/file.
    pub fn connect(address: &str) -> anyhow::Result<(Self, String)> {
        let mut url = url::Url::parse(address).context("invalid SMB URL")?;
        ensure!(url.scheme() == "smb", "unsupported network source scheme");
        ensure!(
            url.query().is_none() && url.fragment().is_none(),
            "SMB URL queries/fragments are unsupported"
        );
        let host = url
            .host_str()
            .context("SMB URL host missing")?
            .trim_matches(['[', ']'])
            .to_owned();
        let components = url
            .path_segments()
            .context("SMB URL path missing")?
            .map(decoded)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let components = components
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>();
        let share = components.first().context("SMB URL must name a share")?;
        for part in &components {
            ensure!(
                !matches!(part.as_str(), "." | "..") && !part.contains(['/', '\\', '\0']),
                "noncanonical SMB URL component"
            );
        }
        let relative = components[1..].join("/");
        canonical(&relative).map_err(anyhow::Error::from)?;
        let username = if url.username().is_empty() {
            std::env::var("GAME_SMB_USER").unwrap_or_else(|_| "guest".into())
        } else {
            decoded(url.username())?
        };
        let password = url
            .password()
            .map(decoded)
            .transpose()?
            .unwrap_or_else(|| std::env::var("GAME_SMB_PASSWORD").unwrap_or_default());
        let mut config = ClientConfig::default();
        config.dfs = false; // Keep this capability on the selected server/share.
        config.connection.port = url.port();
        config.connection.timeout = Some(Duration::from_secs(15));
        let client = Arc::new(Client::new(config));
        let unc = UncPath::new(&host)?.with_share(share)?;
        client
            .share_connect(&unc, &username, password)
            .map_err(network_error)?;
        url.set_username("")
            .map_err(|_| anyhow::anyhow!("SMB URL userinfo"))?;
        url.set_password(None)
            .map_err(|_| anyhow::anyhow!("SMB URL userinfo"))?;
        url.set_path(&format!("/{share}/"));
        let origin = url.to_string();
        Ok((
            Self(Arc::new(Connection {
                client,
                share: unc,
                origin,
                cache: Arc::new(Mutex::new(Cache {
                    stamp: 0,
                    blocks: BTreeMap::new(),
                    directories: BTreeMap::new(),
                })),
                workers: TaskPool::new("game-smb", 4, 32)?,
                next_reader: AtomicU64::new(1),
            })),
            relative,
        ))
    }
    fn job<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
    ) -> Result<T> {
        let mut task = self
            .0
            .workers
            .spawn(move |cancel| {
                cancel.check()?;
                work()
            })
            .map_err(core_error)?;
        let started = Instant::now();
        loop {
            match task.try_take().map_err(core_error)? {
                Some(value) => return Ok(value),
                None if started.elapsed() >= Duration::from_secs(30) => {
                    return Err(Error::Io("SMB operation timed out".into()))
                }
                None => std::thread::sleep(Duration::from_millis(1)),
            }
        }
    }
    fn resolve(&self, relative: &str) -> Result<(String, Metadata)> {
        canonical(relative)?;
        let mut actual = String::new();
        let mut metadata = Metadata {
            directory: true,
            len: 0,
        };
        for name in relative.split('/').filter(|part| !part.is_empty()) {
            if !metadata.directory {
                return Err(Error::NotFound);
            }
            let entries = self.entries_exact(&actual)?;
            let (selected, entry) = if let Some(entry) = entries.get_key_value(name) {
                entry
            } else {
                let mut matches = entries
                    .iter()
                    .filter(|(key, _)| key.eq_ignore_ascii_case(name));
                let selected = matches.next().ok_or(Error::NotFound)?;
                if matches.next().is_some() {
                    return Err(Error::Invalid("ambiguous SMB filename"));
                }
                selected
            };
            if entry.reparse {
                return Err(Error::Unsupported("SMB reparse-point assets"));
            }
            if !actual.is_empty() {
                actual.push('/');
            }
            actual.push_str(selected);
            metadata = entry.metadata;
        }
        Ok((actual, metadata))
    }
    fn entries(&self, directory: &str) -> Result<Arc<BTreeMap<String, Entry>>> {
        let (directory, metadata) = self.resolve(directory)?;
        if !metadata.directory {
            return Err(Error::NotFound);
        }
        self.entries_exact(&directory)
    }
    fn entries_exact(&self, directory: &str) -> Result<Arc<BTreeMap<String, Entry>>> {
        canonical(directory)?;
        {
            let mut cache = self
                .0
                .cache
                .lock()
                .map_err(|_| Error::Invalid("SMB cache lock"))?;
            let stamp = cache.next();
            if let Some((last, entries)) = cache.directories.get_mut(directory) {
                *last = stamp;
                return Ok(entries.clone());
            }
        }
        let client = self.0.client.clone();
        let path = self.0.share.clone().with_path(directory);
        let entries = self.job(move || {
            let args =
                FileCreateArgs::make_open_existing(FileAccessMask::new().with_generic_read(true));
            let resource = client.create_file(&path, &args).map_err(network_error)?;
            let dir = match resource {
                Resource::Directory(dir) => dir,
                other => {
                    close_resource(other);
                    anyhow::bail!("SMB listing source is not a directory")
                }
            };
            let result = (|| -> anyhow::Result<_> {
                let mut entries = BTreeMap::new();
                for entry in dir
                    .query::<FileDirectoryInformation>("*")
                    .map_err(network_error)?
                {
                    let entry = entry.map_err(network_error)?;
                    let name = entry.file_name.to_string();
                    if matches!(name.as_str(), "." | "..") {
                        continue;
                    }
                    ensure!(
                        !name.contains(['/', '\\', '\0']) && !name.is_empty(),
                        "invalid SMB directory member"
                    );
                    ensure!(entries.len() < MAX_ENTRIES, "SMB directory entry budget");
                    entries.insert(
                        name,
                        Entry {
                            metadata: Metadata {
                                directory: entry.file_attributes.directory(),
                                len: entry.end_of_file,
                            },
                            reparse: entry.file_attributes.reparse_point(),
                        },
                    );
                }
                Ok(Arc::new(entries))
            })();
            let close = dir.close().map_err(network_error);
            let entries = result?;
            close?;
            Ok(entries)
        })?;
        let mut cache = self
            .0
            .cache
            .lock()
            .map_err(|_| Error::Invalid("SMB cache lock"))?;
        while cache.directories.len() >= MAX_DIRS
            || cache
                .directories
                .values()
                .map(|(_, e)| e.len())
                .sum::<usize>()
                + entries.len()
                > MAX_ENTRIES
        {
            let Some(key) = cache
                .directories
                .iter()
                .min_by_key(|(_, (stamp, _))| *stamp)
                .map(|(p, _)| p.clone())
            else {
                break;
            };
            cache.directories.remove(&key);
        }
        let stamp = cache.next();
        cache
            .directories
            .insert(directory.into(), (stamp, entries.clone()));
        Ok(entries)
    }
}
impl RemoteStorage for SmbRoot {
    fn origin(&self) -> &str {
        &self.0.origin
    }
    fn metadata(&self, relative: &str) -> Result<Metadata> {
        Ok(self.resolve(relative)?.1)
    }
}
impl VfsProvider for SmbRoot {
    fn kind(&self) -> &'static str {
        "smb"
    }
    fn exists(&self, relative: &str) -> bool {
        self.metadata(relative).is_ok()
    }
    fn list_dir(&self, relative: &str) -> Result<Vec<String>> {
        Ok(self.entries(relative)?.keys().cloned().collect())
    }
    fn read(&self, relative: &str) -> Result<Vec<u8>> {
        let reader = self.open(relative)?;
        let len = reader.len()?;
        if len > 64 * 1024 * 1024 {
            return Err(Error::Limit("SMB whole-file read"));
        }
        let mut bytes = vec![0; len as usize];
        let mut done = 0;
        while done < bytes.len() {
            let n = reader.read_at(done as u64, &mut bytes[done..])?;
            if n == 0 {
                return Err(Error::Io("SMB short read".into()));
            }
            done += n;
        }
        Ok(bytes)
    }
    fn open(&self, relative: &str) -> Result<Arc<dyn AssetReader>> {
        let (relative, metadata) = self.resolve(relative)?;
        if metadata.directory {
            return Err(Error::NotFound);
        }
        let client = self.0.client.clone();
        let path = self.0.share.clone().with_path(&relative);
        let (file, len) = self.job(move || {
            let args =
                FileCreateArgs::make_open_existing(FileAccessMask::new().with_generic_read(true));
            let resource = client.create_file(&path, &args).map_err(network_error)?;
            let file = match resource {
                Resource::File(file) => OpenFile(file),
                other => {
                    close_resource(other);
                    anyhow::bail!("SMB asset is not a file")
                }
            };
            let len = file.0.get_len().map_err(network_error)?;
            Ok((Arc::new(file), len))
        })?;
        let generation = self
            .0
            .next_reader
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| Error::Limit("SMB reader identities"))?;
        Ok(Arc::new(Reader {
            file,
            root: self.clone(),
            generation,
            len,
        }))
    }
}
struct Reader {
    file: Arc<OpenFile>,
    root: SmbRoot,
    generation: u64,
    len: u64,
}
impl Drop for Reader {
    fn drop(&mut self) {
        if let Ok(mut cache) = self.root.0.cache.lock() {
            cache
                .blocks
                .retain(|(generation, _), _| *generation != self.generation);
        }
    }
}
impl AssetReader for Reader {
    fn len(&self) -> Result<u64> {
        Ok(self.len)
    }
    fn read_at(&self, offset: u64, out: &mut [u8]) -> Result<usize> {
        if offset >= self.len || out.is_empty() {
            return Ok(0);
        }
        let count = out
            .len()
            .min((self.len - offset).min(usize::MAX as u64) as usize);
        let mut done = 0;
        while done < count {
            let at = offset + done as u64;
            let block_start = at / BLOCK as u64 * BLOCK as u64;
            let key = (self.generation, block_start);
            let cached = {
                let mut cache = self
                    .root
                    .0
                    .cache
                    .lock()
                    .map_err(|_| Error::Invalid("SMB cache lock"))?;
                let stamp = cache.next();
                cache.blocks.get_mut(&key).map(|(last, bytes)| {
                    *last = stamp;
                    bytes.clone()
                })
            };
            let bytes = if let Some(bytes) = cached {
                bytes
            } else {
                let file = self.file.clone();
                let size = (self.len - block_start).min(BLOCK as u64) as usize;
                let bytes: Arc<[u8]> = self.root.job(move || {
                    let mut bytes = vec![0; size];
                    let mut read = 0;
                    while read < size {
                        let n = file
                            .0
                            .read_at(&mut bytes[read..], block_start + read as u64)
                            .map_err(network_error)?;
                        ensure!(
                            n > 0 && n <= size - read,
                            "SMB asset changed or ended during read"
                        );
                        read += n;
                    }
                    Ok(Arc::from(bytes))
                })?;
                let mut cache = self
                    .root
                    .0
                    .cache
                    .lock()
                    .map_err(|_| Error::Invalid("SMB cache lock"))?;
                if cache.blocks.len() >= BLOCKS {
                    if let Some(old) = cache
                        .blocks
                        .iter()
                        .min_by_key(|(_, (stamp, _))| *stamp)
                        .map(|(k, _)| k.clone())
                    {
                        cache.blocks.remove(&old);
                    }
                }
                let stamp = cache.next();
                cache.blocks.insert(key, (stamp, bytes.clone()));
                bytes
            };
            let local = (at - block_start) as usize;
            let n = (bytes.len() - local).min(count - done);
            out[done..done + n].copy_from_slice(&bytes[local..local + n]);
            done += n;
        }
        Ok(done)
    }
}
