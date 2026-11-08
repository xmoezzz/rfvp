// Copyright (c) 2026 CrySirror contributors. MPL-2.0.
//! Read-only NFS assets. One transport owner drives bounded asynchronous RPCs;
//! native VMs only see the existing VFS/read-at interfaces.
use super::{Metadata, RemoteStorage};
use crate::{
    path::{Mount as AssetMount, VfsPath},
    service::AssetReader,
    vfs::VfsProvider,
    Error, Result,
};
use anyhow::{ensure, Context};
use futures::StreamExt;
use nfs_rs::{Attr, Mount, NfsError, Time, OPEN_READ};
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};
use tokio::{sync::mpsc as queue, task::JoinSet, time::timeout};

const BLOCK: usize = 256 * 1024;
const BLOCKS: usize = 128;
const MAX_ENTRIES: usize = 65_536;
const MAX_DIRS: usize = 64;
const DEADLINE: Duration = Duration::from_secs(30);
type Job = Box<dyn FnOnce(Arc<dyn Mount>) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

fn network_error(error: NfsError) -> Error {
    if error.is_not_found() {
        Error::NotFound
    } else {
        Error::Io(format!("NFS: {error}"))
    }
}
fn canonical(path: &str) -> Result<()> {
    if !path.is_empty() && VfsPath::new(AssetMount::Asset, path)?.relative() != path {
        return Err(Error::Invalid("noncanonical NFS path"));
    }
    Ok(())
}
fn decoded_path(path: &str) -> anyhow::Result<String> {
    let mut parts = Vec::new();
    for part in path.split('/').filter(|p| !p.is_empty()) {
        let part = percent_encoding::percent_decode_str(part)
            .decode_utf8()
            .context("NFS URL must contain UTF-8 names")?
            .into_owned();
        ensure!(
            !matches!(part.as_str(), "." | "..") && !part.contains(['/', '\\', '\0']),
            "noncanonical NFS URL component"
        );
        parts.push(part);
    }
    Ok(parts.join("/"))
}
/// The optional export separates a server export from the selected game path.
/// Without it, the URL directory itself is mounted. No URL contains a password.
fn address(input: &str) -> anyhow::Result<(String, String, String)> {
    let mut url = url::Url::parse(input).context("invalid NFS URL")?;
    ensure!(
        url.scheme() == "nfs" && url.host_str().is_some(),
        "NFS URL host missing"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none() && url.fragment().is_none(),
        "NFS uses uid/gid options, not URL credentials or fragments"
    );
    let path = decoded_path(url.path())?;
    let mut options = BTreeMap::new();
    for (name, value) in url.query_pairs() {
        ensure!(
            matches!(
                name.as_ref(),
                "export" | "version" | "uid" | "gid" | "mountport" | "noresvport"
            ),
            "unsupported NFS URL option: {name}"
        );
        ensure!(
            options
                .insert(name.into_owned(), value.into_owned())
                .is_none(),
            "duplicate NFS URL option"
        );
    }
    let export = match options.remove("export") {
        Some(p) => {
            ensure!(p.starts_with('/'), "NFS export must be absolute");
            let p = p.trim_matches('/').to_owned();
            canonical(&p).map_err(anyhow::Error::from)?;
            p
        }
        None => path.clone(),
    };
    let relative = if path == export {
        String::new()
    } else if export.is_empty() {
        path.clone()
    } else {
        path.strip_prefix(&format!("{export}/"))
            .context("NFS game path must be beneath its export")?
            .to_owned()
    };
    canonical(&relative).map_err(anyhow::Error::from)?;
    options
        .entry("version".into())
        .or_insert_with(|| "3".into());
    options
        .entry("noresvport".into())
        .or_insert_with(|| "true".into());
    for (option, env) in [("uid", "CRYSIRROR_NFS_UID"), ("gid", "CRYSIRROR_NFS_GID")] {
        if !options.contains_key(option) {
            if let Ok(value) = std::env::var(env) {
                options.insert(option.into(), value);
            }
        }
    }
    for option in ["uid", "gid"] {
        if let Some(value) = options.get(option) {
            value
                .parse::<u32>()
                .with_context(|| format!("invalid NFS {option}"))?;
        }
    }
    ensure!(
        matches!(options["version"].as_str(), "3" | "4.1" | "4.2"),
        "NFS version must be 3, 4.1 or 4.2"
    );
    ensure!(
        matches!(options["noresvport"].as_str(), "true" | "false"),
        "invalid NFS noresvport option"
    );
    if let Some(value) = options.get("mountport") {
        ensure!(
            value.parse::<u16>().context("invalid NFS mountport")? != 0,
            "invalid NFS mountport"
        );
    }
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("invalid NFS path"))?;
        segments.clear();
        segments.extend(export.split('/').filter(|p| !p.is_empty()));
    }
    url.set_query(None);
    let origin = format!("{}/", url.as_str().trim_end_matches('/'));
    url.query_pairs_mut().extend_pairs(options.iter());
    Ok((url.into(), origin, relative))
}

#[derive(Clone, Copy, PartialEq)]
struct Fingerprint {
    fsid: u64,
    id: u64,
    len: u64,
    mtime: Time,
    ctime: Time,
}
#[derive(Clone, Copy)]
struct Entry {
    metadata: Metadata,
    kind: u32,
    version: Fingerprint,
}
impl Entry {
    fn from_attr(a: &Attr) -> Self {
        Self {
            metadata: Metadata {
                directory: a.type_ == 2,
                len: a.filesize,
            },
            kind: a.type_,
            version: Fingerprint {
                fsid: a.fsid,
                id: a.fileid,
                len: a.filesize,
                mtime: a.mtime,
                ctime: a.ctime,
            },
        }
    }
    fn readable(self) -> Result<Self> {
        if !matches!(self.kind, 1 | 2) {
            return Err(Error::Unsupported(
                "NFS symbolic links and special-file assets",
            ));
        }
        Ok(self)
    }
}
#[derive(Default)]
struct Cache {
    stamp: u64,
    blocks: BTreeMap<(u64, u64), (u64, Arc<[u8]>)>,
    dirs: BTreeMap<String, (u64, Arc<BTreeMap<String, Entry>>)>,
}
impl Cache {
    fn next(&mut self) -> u64 {
        self.stamp = self.stamp.wrapping_add(1);
        self.stamp
    }
}
struct Connection {
    jobs: queue::Sender<Job>,
    faulted: Arc<AtomicBool>,
    origin: String,
    root: Entry,
    cache: Mutex<Cache>,
    next_reader: AtomicU64,
}

#[derive(Clone)]
pub struct NfsRoot(Arc<Connection>);
impl NfsRoot {
    /// Called on the host's boot worker. No OS mount or elevated helper.
    pub fn connect(input: &str) -> anyhow::Result<(Self, String)> {
        let (url, origin, relative) = address(input)?;
        let (jobs, mut incoming) = queue::channel::<Job>(32);
        let (ready, wait) = mpsc::sync_channel(1);
        let faulted = Arc::new(AtomicBool::new(false));
        let failed = faulted.clone();
        std::thread::Builder::new().name("game-nfs".into()).spawn(move||{
            let runtime=match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(r)=>r,Err(e)=>{let _=ready.send(Err(Error::Io(format!("NFS runtime: {e}"))));return;}
            };
            runtime.block_on(async move {
                let connected=timeout(Duration::from_secs(15),async {
                    let mount:Arc<dyn Mount>=Arc::from(nfs_rs::parse_url_and_mount(&url).await.map_err(network_error)?);
                    let attr=mount.getattr_path("").await.map_err(network_error)?;
                    let root=Entry::from_attr(&attr).readable()?;
                    if !root.metadata.directory {return Err(Error::Invalid("NFS mount is not a directory"));}
                    Ok((mount,root))
                }).await.unwrap_or_else(|_|Err(Error::Io("NFS connection timed out".into())));
                let (mount,root)=match connected {Ok(v)=>v,Err(e)=>{let _=ready.send(Err(e));return;}};
                if ready.send(Ok(root)).is_err() {let _=timeout(Duration::from_secs(5),mount.umount()).await;return;}
                let mut active=JoinSet::new();
                loop {
                    if failed.load(Ordering::Acquire) {active.abort_all();break;}
                    if active.len()>=4 {let _=active.join_next().await;continue;}
                    tokio::select! {
                        request=incoming.recv()=>match request {Some(job)=>{active.spawn(job(mount.clone()));},None=>break},
                        _=active.join_next(),if !active.is_empty()=>{},
                    }
                }
                incoming.close();
                while active.join_next().await.is_some() {}
                let _=timeout(Duration::from_secs(5),mount.umount()).await;
            });
        }).context("start NFS transport owner")?;
        let root = wait
            .recv_timeout(Duration::from_secs(17))
            .context("NFS connection worker unavailable")??;
        Ok((
            Self(Arc::new(Connection {
                jobs,
                faulted,
                origin,
                root,
                cache: Mutex::new(Cache::default()),
                next_reader: AtomicU64::new(1),
            })),
            relative,
        ))
    }
    fn job<T: Send + 'static, F: Future<Output = Result<T>> + Send + 'static>(
        &self,
        work: impl FnOnce(Arc<dyn Mount>) -> F + Send + 'static,
    ) -> Result<T> {
        if self.0.faulted.load(Ordering::Acquire) {
            return Err(Error::Io("NFS connection faulted; reopen the game".into()));
        }
        let (reply, wait) = mpsc::sync_channel(1);
        let faulted = self.0.faulted.clone();
        let deadline = tokio::time::Instant::now() + DEADLINE;
        self.0
            .jobs
            .try_send(Box::new(move |mount| {
                Box::pin(async move {
                    let result = match tokio::time::timeout_at(deadline, work(mount)).await {
                        Ok(result) => result,
                        Err(_) => {
                            faulted.store(true, Ordering::Release);
                            Err(Error::Io(
                                "NFS operation timed out; connection retired".into(),
                            ))
                        }
                    };
                    let _ = reply.send(result);
                })
            }))
            .map_err(|e| match e {
                queue::error::TrySendError::Full(_) => Error::Limit("NFS pending operations"),
                queue::error::TrySendError::Closed(_) => Error::Io("NFS transport closed".into()),
            })?;
        // Four active RPC jobs and a bounded queue, with no polling thread per read.
        wait.recv_timeout(DEADLINE + Duration::from_secs(2))
            .map_err(|_| Error::Io("NFS operation owner unavailable".into()))?
    }
    fn entries_exact(&self, path: &str) -> Result<Arc<BTreeMap<String, Entry>>> {
        canonical(path)?;
        {
            let mut cache = self
                .0
                .cache
                .lock()
                .map_err(|_| Error::Invalid("NFS cache lock"))?;
            let stamp = cache.next();
            if let Some((last, entries)) = cache.dirs.get_mut(path) {
                *last = stamp;
                return Ok(entries.clone());
            }
        }
        let directory = path.to_owned();
        let entries = self.job(move |mount| async move {
            let dir = mount.lookup_path(&directory).await.map_err(network_error)?;
            let mut stream = mount.readdirplus(dir.fh).await;
            let mut entries = BTreeMap::new();
            while let Some(entry) = stream.next().await {
                let entry = entry.map_err(network_error)?;
                let name = entry.file_name;
                if matches!(name.as_str(), "." | "..") {
                    continue;
                }
                canonical(&name)?;
                if name.is_empty() || name.contains('/') {
                    return Err(Error::Invalid("invalid NFS directory member"));
                }
                if entries.len() >= MAX_ENTRIES {
                    return Err(Error::Limit("NFS directory entries"));
                }
                let attr = match entry.attr {
                    Some(a) => a,
                    None => mount.getattr(entry.handle).await.map_err(network_error)?,
                };
                entries.insert(name, Entry::from_attr(&attr));
            }
            Ok(Arc::new(entries))
        })?;
        let mut cache = self
            .0
            .cache
            .lock()
            .map_err(|_| Error::Invalid("NFS cache lock"))?;
        while cache.dirs.len() >= MAX_DIRS
            || cache.dirs.values().map(|(_, e)| e.len()).sum::<usize>() + entries.len()
                > MAX_ENTRIES
        {
            let Some(key) = cache
                .dirs
                .iter()
                .min_by_key(|(_, v)| v.0)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            cache.dirs.remove(&key);
        }
        let stamp = cache.next();
        cache.dirs.insert(path.to_owned(), (stamp, entries.clone()));
        Ok(entries)
    }
    fn resolve(&self, path: &str) -> Result<(String, Entry)> {
        canonical(path)?;
        let mut actual = String::new();
        let mut selected = self.0.root;
        for part in path.split('/').filter(|p| !p.is_empty()) {
            if !selected.metadata.directory {
                return Err(Error::NotFound);
            }
            let entries = self.entries_exact(&actual)?;
            let (name, entry) = if let Some(pair) = entries.get_key_value(part) {
                pair
            } else {
                let mut candidates = entries
                    .iter()
                    .filter(|(name, _)| name.eq_ignore_ascii_case(part));
                let found = candidates.next().ok_or(Error::NotFound)?;
                if candidates.next().is_some() {
                    return Err(Error::Invalid("ambiguous NFS filename"));
                }
                found
            };
            selected = entry.readable()?;
            if !actual.is_empty() {
                actual.push('/');
            }
            actual.push_str(name);
        }
        Ok((actual, selected))
    }
}
impl RemoteStorage for NfsRoot {
    fn origin(&self) -> &str {
        &self.0.origin
    }
    fn metadata(&self, path: &str) -> Result<Metadata> {
        Ok(self.resolve(path)?.1.metadata)
    }
}
impl VfsProvider for NfsRoot {
    fn kind(&self) -> &'static str {
        "nfs"
    }
    fn exists(&self, path: &str) -> bool {
        self.metadata(path).is_ok()
    }
    fn list_dir(&self, path: &str) -> Result<Vec<String>> {
        let (path, entry) = self.resolve(path)?;
        if !entry.metadata.directory {
            return Err(Error::NotFound);
        }
        Ok(self.entries_exact(&path)?.keys().cloned().collect())
    }
    fn open(&self, path: &str) -> Result<Arc<dyn AssetReader>> {
        let (path, entry) = self.resolve(path)?;
        if entry.kind != 1 {
            return Err(Error::NotFound);
        }
        // Query current identity rather than adopting stale directory metadata.
        let query = path.clone();
        let current = self.job(move |mount| async move {
            Ok(
                Entry::from_attr(&mount.getattr_path(&query).await.map_err(network_error)?)
                    .readable()?,
            )
        })?;
        if current.kind != 1 {
            return Err(Error::NotFound);
        }
        Ok(Arc::new(Reader {
            root: self.clone(),
            path,
            version: current.version,
            generation: self.0.next_reader.fetch_add(1, Ordering::Relaxed),
        }))
    }
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        let reader = self.open(path)?;
        let len = reader.len()?;
        if len > 64 * 1024 * 1024 {
            return Err(Error::Limit("NFS whole-file read; use random access"));
        }
        let mut bytes = vec![0; len as usize];
        let mut done = 0;
        while done < bytes.len() {
            let n = reader.read_at(done as u64, &mut bytes[done..])?;
            if n == 0 {
                return Err(Error::Io("NFS short read".into()));
            }
            done += n;
        }
        Ok(bytes)
    }
}
struct Reader {
    root: NfsRoot,
    path: String,
    version: Fingerprint,
    generation: u64,
}
impl Drop for Reader {
    fn drop(&mut self) {
        if let Ok(mut cache) = self.root.0.cache.lock() {
            cache.blocks.retain(|(g, _), _| *g != self.generation);
        }
    }
}
impl AssetReader for Reader {
    fn len(&self) -> Result<u64> {
        Ok(self.version.len)
    }
    fn read_at(&self, offset: u64, out: &mut [u8]) -> Result<usize> {
        let count = out.len().min(
            self.version
                .len
                .saturating_sub(offset)
                .min(usize::MAX as u64) as usize,
        );
        let mut done = 0;
        while done < count {
            let at = offset + done as u64;
            let start = at / BLOCK as u64 * BLOCK as u64;
            let key = (self.generation, start);
            let cached = {
                let mut cache = self
                    .root
                    .0
                    .cache
                    .lock()
                    .map_err(|_| Error::Invalid("NFS cache lock"))?;
                let stamp = cache.next();
                cache.blocks.get_mut(&key).map(|(last, data)| {
                    *last = stamp;
                    data.clone()
                })
            };
            let bytes = if let Some(data) = cached {
                data
            } else {
                let path = self.path.clone();
                let version = self.version;
                let size = (version.len - start).min(BLOCK as u64) as usize;
                let data = self.root.job(move |mount| async move {
                    let file = mount
                        .open_path_stateful(&path, OPEN_READ)
                        .await
                        .map_err(network_error)?;
                    let result = async {
                        let before = mount
                            .getattr(file.object.fh.clone())
                            .await
                            .map_err(network_error)?;
                        if Entry::from_attr(&before).version != version {
                            return Err(Error::Io("NFS asset changed during playback".into()));
                        }
                        let mut bytes = Vec::with_capacity(size);
                        while bytes.len() < size {
                            let chunk = mount
                                .read(
                                    file.object.fh.clone(),
                                    start + bytes.len() as u64,
                                    (size - bytes.len()).min(64 * 1024) as u32,
                                )
                                .await
                                .map_err(network_error)?;
                            if chunk.is_empty() || chunk.len() > size - bytes.len() {
                                return Err(Error::Io(
                                    "NFS asset ended or changed during read".into(),
                                ));
                            }
                            bytes.extend_from_slice(&chunk);
                        }
                        let after = mount
                            .getattr(file.object.fh.clone())
                            .await
                            .map_err(network_error)?;
                        if Entry::from_attr(&after).version != version {
                            return Err(Error::Io("NFS asset changed during read".into()));
                        }
                        Ok(Arc::<[u8]>::from(bytes))
                    }
                    .await;
                    let close = mount.close_stateful(file).await.map_err(network_error);
                    let bytes = result?;
                    close?;
                    Ok(bytes)
                })?;
                let mut cache = self
                    .root
                    .0
                    .cache
                    .lock()
                    .map_err(|_| Error::Invalid("NFS cache lock"))?;
                if cache.blocks.len() >= BLOCKS {
                    if let Some(old) = cache
                        .blocks
                        .iter()
                        .min_by_key(|(_, v)| v.0)
                        .map(|(k, _)| *k)
                    {
                        cache.blocks.remove(&old);
                    }
                }
                let stamp = cache.next();
                cache.blocks.insert(key, (stamp, data.clone()));
                data
            };
            let local = (at - start) as usize;
            let n = (bytes.len() - local).min(count - done);
            out[done..done + n].copy_from_slice(&bytes[local..local + n]);
            done += n;
        }
        Ok(done)
    }
}
