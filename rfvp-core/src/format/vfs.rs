use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, Write};
use std::mem::size_of;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::scenario::Nls;

#[derive(Debug, Clone)]
pub struct VfsEntry {
    offset: u64,
    size: u64,
}

#[derive(Debug, Clone)]
pub struct VfsFile {
    entries: HashMap<String, VfsEntry>,
    nls: Nls,
    path: PathBuf,
    folder_name: String,
    dir_path: PathBuf,
}

impl VfsFile {
    pub fn new(path: impl AsRef<Path>, folder_name: &str, nls: Nls) -> Result<Self> {
        let entries = Self::parse(&path, nls.clone())?;

        let vf = Self {
            entries,
            nls,
            path: path.as_ref().to_path_buf(),
            folder_name: folder_name.to_string(),
            dir_path: path.as_ref().parent().unwrap().to_path_buf(),
        };

        Ok(vf)
    }

    fn read_u32le(reader: &mut (impl Read + Seek), offset: u64) -> Result<u32> {
        let mut buffer = [0; 4];
        reader.seek(std::io::SeekFrom::Start(offset))?;
        reader.read_exact(&mut buffer)?;
        Ok(u32::from_le_bytes(buffer))
    }

    /// read c-style strings from buffer
    fn read_filename_table(
        reader: &mut (impl Read + Seek),
        offset: u64,
        size: u64,
        nls: Nls,
    ) -> Result<HashMap<u64, String>> {
        let mut buffer = vec![];
        reader.seek(std::io::SeekFrom::Start(offset))?;
        let mut chunk = reader.take(size);
        chunk.read_to_end(&mut buffer)?;

        let mut results = HashMap::new();
        let mut start = 0;
        for (i, &b) in buffer.iter().enumerate() {
            if b == 0 {
                match nls {
                    Nls::ShiftJIS => {
                        let (s, _, _) = encoding_rs::SHIFT_JIS.decode(&buffer[start..i]);
                        results.insert(start as u64, s.to_string());
                    }
                    Nls::GBK => {
                        let (s, _, _) = encoding_rs::GBK.decode(&buffer[start..i]);
                        results.insert(start as u64, s.to_string());
                    }
                    Nls::UTF8 => {
                        let s = String::from_utf8_lossy(&buffer[start..i]);
                        results.insert(start as u64, s.to_string());
                    }
                };
                start = i + 1;
            }
        }

        Ok(results)
    }

    pub(crate) fn parse(path: impl AsRef<Path>, nls: Nls) -> Result<HashMap<String, VfsEntry>> {
        if !path.as_ref().exists() {
            bail!("File does not exist: {:?}", path.as_ref());
        }

        let mut file = File::open(path.as_ref())?;
        let mut rdr = BufReader::new(&file);
        let mut offset = 0;
        let file_count = Self::read_u32le(&mut rdr, offset)?;
        offset += size_of::<u32>() as u64;

        let filename_table_size = Self::read_u32le(&mut file, offset)?;
        offset += size_of::<u32>() as u64;

        let old_offset = offset;

        // each entry is 12 bytes
        let filename_table_offset = offset + file_count as u64 * 12;
        let filename_table = Self::read_filename_table(
            &mut file,
            filename_table_offset,
            filename_table_size.into(),
            nls,
        )?;

        offset = old_offset;
        file.seek(std::io::SeekFrom::Start(offset))?;
        let mut entries = HashMap::new();
        for _ in 0..file_count {
            let name_offset = Self::read_u32le(&mut file, offset)?;
            offset += size_of::<u32>() as u64;
            let entry_offset = Self::read_u32le(&mut file, offset)?;
            offset += size_of::<u32>() as u64;
            let size = Self::read_u32le(&mut file, offset)?;
            offset += size_of::<u32>() as u64;

            if let Some(name) = filename_table.get(&(name_offset as u64)) {
                entries.insert(
                    name.clone(),
                    VfsEntry {
                        offset: entry_offset as u64,
                        size: size as u64,
                    },
                );
            }
        }

        Ok(entries)
    }

    #[allow(dead_code)]
    pub(crate) fn extract_all(&self, output_dir: impl AsRef<Path>) -> Result<()> {
        println!("Extracting {} entries", self.entries.len());
        for (name, entry) in &self.entries {
            let mut buffer = vec![0; entry.size as usize];
            let mut file = File::open(&self.path)?;

            file.seek(std::io::SeekFrom::Start(entry.offset))?;
            file.read_exact(&mut buffer)?;

            let output_path = output_dir.as_ref().join(name);
            let mut output_file = File::create(output_path)?;
            output_file.write_all(&buffer)?;
        }

        Ok(())
    }

    /// we assume that modern systems have enough memory to load the whole file into memory
    pub fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        let path = self.dir_path.join(&self.folder_name).join(name);
        if path.exists() {
            let mut file = File::open(path)?;
            let mut buffer = vec![];
            file.read_to_end(&mut buffer)?;
            return Ok(buffer);
        }

        let entry = self
            .entries
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("File not found in VFS: {}", name))?;

        let mut buffer = vec![0; entry.size as usize];
        let mut file = File::open(&self.path)?;

        file.seek(std::io::SeekFrom::Start(entry.offset))?;
        file.read_exact(&mut buffer)?;

        Ok(buffer)
    }
}

#[derive(Debug, Default)]
pub struct Vfs {
    files: HashMap<String, VfsFile>,
    nls: Nls,
    base_path: PathBuf,
}

impl Vfs {
    pub fn new(nls: Nls, base_path: impl AsRef<Path>) -> Result<Self> {
        let path = base_path.as_ref();
        let mut path = path.to_path_buf();
        path.push("*.bin");

        let macthes: Vec<_> = glob::glob(&path.to_string_lossy())?.flatten().collect();

        let mut files = HashMap::new();
        for path in &macthes {
            if let Some(file_name) = path.file_name() {
                let file_name = file_name.to_string_lossy();
                if let Some(folder_name) = file_name.split('.').next() {
                    if let Ok(vfs) = VfsFile::new(path, folder_name, nls.clone()) {
                        log::info!("VFS file found: {}", folder_name);
                        files.insert(folder_name.to_string(), vfs);
                    } else {
                        log::error!("Failed to load VFS file: {}", folder_name);
                    }
                }
            }
        }

        let vfs = Self {
            files,
            nls,
            base_path: base_path.as_ref().to_path_buf(),
        };

        Ok(vfs)
    }

    fn read_vfs_file(&self, folder_name: &str, name: &str) -> Result<Vec<u8>> {
        let vfs = self
            .files
            .get(folder_name)
            .ok_or_else(|| anyhow::anyhow!("VFS not found: {}", folder_name))?;

        vfs.read_file(name)
    }

    fn has_hash_vfs(&self, folder_name: &str) -> bool {
        self.files.contains_key(folder_name)
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        if let Some((folder_name, name)) = path.split_once('/') {
            if self.has_hash_vfs(folder_name) {
                return self.read_vfs_file(folder_name, name);
            }
        }

        // otherwise, we assume the file is present in the filesystem
        let path = self.base_path.join(path);
        let content =
            std::fs::read(path.clone()).context(format!("unable to load : {:?}", path))?;
        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_vfs_file() {
        let filepath = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase/se_sys.bin"));

        let vfs = VfsFile::new(filepath, "se_sys", Nls::ShiftJIS).unwrap();
        let buf = vfs.read_file("001").unwrap();
        if buf.is_empty() {
            panic!("Buffer is empty");
        }
    }

    // #[test]
    // fn test_vfs_file2() {
    //     let filepath = Path::new("/Users/xmoe/Downloads/WhiteEternity/graph.bin");

    //     let vfs = VfsFile::new(filepath, "graph", Nls::ShiftJIS).unwrap();
    //     vfs.extract_all("/Users/xmoe/Downloads/graph").unwrap();
    // }

    #[test]
    fn test_vfs() {
        let vfs = Vfs::new(Nls::ShiftJIS, ".").unwrap();
        let buf = vfs.read_file("se_sys/001").unwrap();
        if buf.is_empty() {
            panic!("Buffer is empty");
        }
    }
}
