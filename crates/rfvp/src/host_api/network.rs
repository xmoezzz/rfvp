//! Optional read-only game directory. Connect and access this on a host worker;
//! a separate RfvpStorage must be injected for writable saves.
use alloc::{boxed::Box,sync::Arc,string::{String,ToString},vec::Vec};
use super::{RfvpFile,RfvpFileInfo,RfvpFileKind,RfvpFileSystem,RfvpStorage,RfvpError,RfvpResult};
#[derive(Clone)]pub struct NetworkStorage(pub game_network::Directory);
pub struct NetworkFile(game_network::File);
impl RfvpFile for NetworkFile {
 fn len(&mut self)->RfvpResult<u64>{Ok(self.0.len())}
 fn read_at(&mut self,offset:u64,buf:&mut[u8])->RfvpResult<usize>{self.0.read_at(offset,buf).map_err(|_|RfvpError::Io)}
}
fn path(p:&str)->String {p.strip_prefix("./").unwrap_or(p).replace('\\',"/")}
impl NetworkStorage {
 pub fn connect(url:&str)->anyhow::Result<Self>{Ok(Self(game_network::Directory::connect(url)?))}
 fn open_file(&self,p:&str)->RfvpResult<NetworkFile>{self.0.open(&path(p)).map(NetworkFile).map_err(|_|RfvpError::NotFound)}
}
impl RfvpStorage for NetworkStorage {
 fn read(&self,p:&str)->RfvpResult<Vec<u8>>{self.open_file(p)?.read_to_vec(256*1024*1024)}
 fn read_prefix(&self,p:&str,n:usize)->RfvpResult<Vec<u8>>{let mut file=self.open_file(p)?;let mut b=alloc::vec![0;n.min(file.len()? as usize)];file.read_exact_at(0,&mut b)?;Ok(b)}
 fn open(&self,p:&str)->RfvpResult<Box<dyn RfvpFile+Send>>{Ok(Box::new(self.open_file(p)?))}
 fn exists(&self,p:&str)->bool{self.0.metadata(&path(p)).is_ok()}
 fn list(&self,p:&str)->RfvpResult<Vec<String>>{self.0.list(&path(p)).map_err(|_|RfvpError::Io)}
 fn write(&self,_:&str,_:&[u8])->RfvpResult<()>{Err(RfvpError::Unsupported)}
 fn remove(&self,_:&str)->RfvpResult<()>{Err(RfvpError::Unsupported)}
}
/// Connect before boot on the session worker. The core streams packs and loose
/// assets through independent read-at handles; the host retains save authority.
pub struct NetworkFileSystem {pub game:NetworkStorage,pub saves:Arc<dyn RfvpStorage>}
impl RfvpFileSystem for NetworkFileSystem {
 type File=NetworkFile;
 fn open(&mut self,p:&str)->RfvpResult<NetworkFile>{self.game.open_file(p)}
 fn storage(&mut self)->Option<Arc<dyn RfvpStorage>>{Some(self.saves.clone())}
 fn game_files(&mut self)->Option<Arc<dyn RfvpStorage>>{Some(Arc::new(self.game.clone()))}
 fn metadata(&mut self,p:&str)->RfvpResult<RfvpFileInfo>{let m=self.game.0.metadata(&path(p)).map_err(|_|RfvpError::NotFound)?;Ok(RfvpFileInfo{len:m.len,kind:if m.directory{RfvpFileKind::Directory}else{RfvpFileKind::File}})}
 fn enumerate_by_extension(&mut self,root:&str,ext:&str,visitor:&mut dyn FnMut(&str,RfvpFileInfo)->RfvpResult<()>)->RfvpResult<()>{let root=if root=="."{"".to_string()}else{path(root)};for name in self.game.0.list(&root).map_err(|_|RfvpError::Io)?{if name.rsplit_once('.').is_some_and(|(_,e)|e.eq_ignore_ascii_case(ext)){let rel=if root.is_empty(){name.clone()}else{alloc::format!("{root}/{name}")};let m=self.metadata(&rel)?;if m.kind==RfvpFileKind::File{visitor(&rel,m)?;}}}Ok(())}
}
