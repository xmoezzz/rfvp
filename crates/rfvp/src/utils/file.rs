use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[cfg(not(target_os = "uefi"))]
use std::{env, path};

pub struct FileReaderError {
    _msg: String,
}

/// This will read a file and return it as a byte vec.
pub fn read_file(path: &Path) -> Result<Vec<u8>, FileReaderError> {
    let mut file = open_file(path)?;
    let mut buffer = Vec::new();
    let read_result = file.read_to_end(&mut buffer);
    match read_result {
        Ok(_) => Ok(buffer),
        Err(e) => Err(FileReaderError {
            _msg: e.to_string(),
        }),
    }
}

pub fn read_file_modification_time(path: &Path) -> Result<SystemTime, FileReaderError> {
    let file = open_file(path)?;
    match file.metadata() {
        Ok(metadata) => Ok(metadata.modified().unwrap()),
        Err(e) => Err(FileReaderError {
            _msg: e.to_string(),
        }),
    }
}

pub fn open_file(path: &Path) -> Result<File, FileReaderError> {
    match File::open(path) {
        Ok(file) => Ok(file),
        Err(e) => Err(FileReaderError {
            _msg: e.to_string(),
        }),
    }
}

#[cfg(not(target_os = "uefi"))]
pub fn set_base_path(path: &str) {
    env::set_var("FVP_BASE_PATH", path);
}

#[cfg(target_os = "uefi")]
pub fn set_base_path(_path: &str) {}

#[cfg(not(target_os = "uefi"))]
pub fn set_hcb_root_path(path: &str) {
    env::set_var("FVP_HCB_ROOT", path);
}

#[cfg(target_os = "uefi")]
pub fn set_hcb_root_path(_path: &str) {}

#[cfg(not(target_os = "uefi"))]
pub fn hcb_root_path() -> Option<PathBuf> {
    env::var("FVP_HCB_ROOT")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

#[cfg(target_os = "uefi")]
pub fn hcb_root_path() -> Option<PathBuf> {
    None
}

/// This will give you the path to the executable (when in build mode) or to the root of the current project.
#[cfg(not(target_os = "uefi"))]
pub fn app_base_path() -> PathBuilder {
    if env::var_os("FVP_TEST").is_some() {
        let mut testcase_path = "testcase".to_string();
        if let Ok(testcase) = env::var("FVP_TESTCASE") {
            testcase_path = testcase;
        }
        if let Some(manifest_dir) = env::var_os("CARGO_MANIFEST_DIR") {
            let mut p = path::PathBuf::from(manifest_dir);
            p.push("../..");
            p.push(testcase_path);
            return PathBuilder { path_buff: p };
        }
    }

    if let Ok(base_path) = env::var("FVP_BASE_PATH") {
        return PathBuilder {
            path_buff: PathBuf::from(base_path),
        };
    }

    match env::current_exe() {
        Ok(mut path) => {
            path.pop();
            PathBuilder { path_buff: path }
        }
        Err(e) => {
            log::error!(
                "Error while creating the app base_path {:?}, will use default.",
                e
            );
            PathBuilder {
                path_buff: Default::default(),
            }
        }
    }
}

/// UEFI hosts do not have process environment variables. The UEFI app mounts
/// game data under the same default project directory it passes to the runtime.
#[cfg(target_os = "uefi")]
pub fn app_base_path() -> PathBuilder {
    PathBuilder {
        path_buff: PathBuf::from(r"\rfvp"),
    }
}

/// The game's files on a desktop: the packs and loose files in [`app_base_path`].
#[cfg(not(any(target_os = "uefi", target_arch = "wasm32")))]
pub fn desktop_vfs(
    nls: crate::script::parser::Nls,
) -> anyhow::Result<crate::subsystem::resources::vfs::Vfs> {
    let dir = crate::host_api::std_fs::StdDir::new(app_base_path().get_path().clone());
    crate::subsystem::resources::vfs::Vfs::from_dir(nls, std::sync::Arc::new(dir))
}

/// Loose `cursor*.ani` files that make up the game's mouse cursors.
///
/// They sit next to the game rather than in a pack, so look in every directory that can be the
/// game directory: the configured base path (`--project-dir` / `FVP_BASE_PATH` / exe dir), the
/// directory of the `.hcb`, the working directory and the executable's directory.
#[cfg(not(any(target_os = "uefi", target_arch = "wasm32")))]
pub fn find_loose_ani_cursors() -> Vec<PathBuf> {
    use std::collections::HashSet;
    use std::fs;

    let mut roots = Vec::new();
    if let Some(root) = hcb_root_path() {
        roots.push(root);
    }
    roots.push(app_base_path().get_path().clone());
    if let Ok(cwd) = env::current_dir() {
        roots.push(cwd);
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            roots.push(parent.to_path_buf());
        }
    }

    let mut seen_roots = HashSet::new();
    let mut seen_files = HashSet::new();
    let mut matches = Vec::new();
    for root in roots {
        if !seen_roots.insert(root.to_string_lossy().to_string()) {
            continue;
        }
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let lower = name.to_ascii_lowercase();
            if !(lower.starts_with("cursor") && lower.ends_with(".ani")) {
                continue;
            }
            if seen_files.insert(path.to_string_lossy().to_string()) {
                matches.push(path);
            }
        }
    }

    matches.sort();
    if matches.is_empty() {
        log::warn!(
            "no loose ANI cursor files found; searched roots: {}",
            seen_roots.into_iter().collect::<Vec<_>>().join(", ")
        );
    } else {
        log::info!("found {} loose ANI cursor file(s)", matches.len());
        for path in &matches {
            log::info!("found ANI cursor: {}", path.display());
        }
    }
    matches
}

/// Browsers and UEFI have no loose cursor files to look for.
#[cfg(any(target_os = "uefi", target_arch = "wasm32"))]
pub fn find_loose_ani_cursors() -> Vec<PathBuf> {
    Vec::new()
}

/// Utils to help to build path and get them as String
pub struct PathBuilder {
    path_buff: PathBuf,
}

impl PathBuilder {
    pub fn join(mut self, path: &str) -> PathBuilder {
        self.path_buff = self.path_buff.join(path);
        self
    }

    pub fn get(self) -> String {
        self.path_buff
            .as_path()
            .to_str()
            .expect("Unable to extract the path from the path builder")
            .to_string()
    }

    pub fn exists(&self) -> bool {
        self.path_buff.exists()
    }

    pub fn get_path(&self) -> &PathBuf {
        &self.path_buff
    }
}
