#![cfg_attr(feature = "no_std", no_std)]
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

extern crate alloc;
#[cfg(feature = "no_std")]
extern crate self as image;
#[cfg(feature = "no_std")]
extern crate self as std;
#[cfg(feature = "no_std")]
pub use alloc::{format, vec};

#[cfg(feature = "no_std")]
pub trait PixelBytes: Copy {
    const CHANNELS: usize;
    fn from_bytes(bytes: &[u8]) -> Self;
    fn write_bytes(self, bytes: &mut [u8]);
}

#[cfg(feature = "no_std")]
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba<T>(pub [T; 4]);

#[cfg(feature = "no_std")]
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb<T>(pub [T; 3]);

#[cfg(feature = "no_std")]
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LumaA<T>(pub [T; 2]);

#[cfg(feature = "no_std")]
impl PixelBytes for Rgba<u8> {
    const CHANNELS: usize = 4;
    fn from_bytes(bytes: &[u8]) -> Self {
        Self([bytes[0], bytes[1], bytes[2], bytes[3]])
    }
    fn write_bytes(self, bytes: &mut [u8]) {
        bytes[..4].copy_from_slice(&self.0);
    }
}

#[cfg(feature = "no_std")]
impl PixelBytes for Rgb<u8> {
    const CHANNELS: usize = 3;
    fn from_bytes(bytes: &[u8]) -> Self {
        Self([bytes[0], bytes[1], bytes[2]])
    }
    fn write_bytes(self, bytes: &mut [u8]) {
        bytes[..3].copy_from_slice(&self.0);
    }
}

#[cfg(feature = "no_std")]
impl PixelBytes for LumaA<u8> {
    const CHANNELS: usize = 2;
    fn from_bytes(bytes: &[u8]) -> Self {
        Self([bytes[0], bytes[1]])
    }
    fn write_bytes(self, bytes: &mut [u8]) {
        bytes[..2].copy_from_slice(&self.0);
    }
}

#[cfg(feature = "no_std")]
#[derive(Clone, Debug)]
pub struct ImageBuffer<P: PixelBytes, Container = alloc::vec::Vec<u8>> {
    width: u32,
    height: u32,
    data: Container,
    _pixel: core::marker::PhantomData<P>,
}

#[cfg(feature = "no_std")]
pub type RgbaImage = ImageBuffer<Rgba<u8>, alloc::vec::Vec<u8>>;
#[cfg(feature = "no_std")]
pub type GrayAlphaImage = ImageBuffer<LumaA<u8>, alloc::vec::Vec<u8>>;

#[cfg(feature = "no_std")]
impl<P: PixelBytes> ImageBuffer<P, alloc::vec::Vec<u8>> {
    pub fn new(width: u32, height: u32) -> Self {
        let len = width as usize * height as usize * P::CHANNELS;
        Self {
            width,
            height,
            data: alloc::vec![0; len],
            _pixel: core::marker::PhantomData,
        }
    }

    pub fn from_raw(width: u32, height: u32, data: alloc::vec::Vec<u8>) -> Option<Self> {
        if data.len() != width as usize * height as usize * P::CHANNELS {
            return None;
        }
        Some(Self {
            width,
            height,
            data,
            _pixel: core::marker::PhantomData,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn as_raw(&self) -> &alloc::vec::Vec<u8> {
        &self.data
    }

    pub fn as_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    pub fn into_raw(self) -> alloc::vec::Vec<u8> {
        self.data
    }

    pub fn put_pixel(&mut self, x: u32, y: u32, pixel: P) {
        let idx = (y as usize * self.width as usize + x as usize) * P::CHANNELS;
        pixel.write_bytes(&mut self.data[idx..idx + P::CHANNELS]);
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> P {
        let idx = (y as usize * self.width as usize + x as usize) * P::CHANNELS;
        P::from_bytes(&self.data[idx..idx + P::CHANNELS])
    }

    pub fn enumerate_pixels_mut(&mut self) -> EnumeratePixelsMut<'_, P> {
        EnumeratePixelsMut {
            width: self.width,
            index: 0,
            chunks: self.data.chunks_exact_mut(P::CHANNELS),
            _pixel: core::marker::PhantomData,
        }
    }
}

#[cfg(feature = "no_std")]
impl ImageBuffer<Rgba<u8>, alloc::vec::Vec<u8>> {
    pub fn get_pixel_mut(&mut self, x: u32, y: u32) -> &mut Rgba<u8> {
        let idx = (y as usize * self.width as usize + x as usize) * 4;
        let ptr = self.data[idx..idx + 4].as_mut_ptr() as *mut Rgba<u8>;
        unsafe { &mut *ptr }
    }

    pub fn pixels_mut(&mut self) -> RgbaPixelsMut<'_> {
        RgbaPixelsMut {
            chunks: self.data.chunks_exact_mut(4),
        }
    }
}

#[cfg(feature = "no_std")]
pub struct RgbaPixelsMut<'a> {
    chunks: core::slice::ChunksExactMut<'a, u8>,
}

#[cfg(feature = "no_std")]
impl<'a> Iterator for RgbaPixelsMut<'a> {
    type Item = &'a mut Rgba<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.chunks.next()?;
        let ptr = chunk.as_mut_ptr() as *mut Rgba<u8>;
        Some(unsafe { &mut *ptr })
    }
}

#[cfg(feature = "no_std")]
impl<P: PixelBytes> AsMut<[u8]> for ImageBuffer<P, alloc::vec::Vec<u8>> {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

#[cfg(feature = "no_std")]
pub struct EnumeratePixelsMut<'a, P: PixelBytes> {
    width: u32,
    index: u32,
    chunks: core::slice::ChunksExactMut<'a, u8>,
    _pixel: core::marker::PhantomData<P>,
}

#[cfg(feature = "no_std")]
impl<'a, P: PixelBytes> Iterator for EnumeratePixelsMut<'a, P> {
    type Item = (u32, u32, PixelMut<'a, P>);

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.chunks.next()?;
        let value = P::from_bytes(chunk);
        let idx = self.index;
        self.index = self.index.wrapping_add(1);
        Some((
            idx % self.width,
            idx / self.width,
            PixelMut { chunk, value },
        ))
    }
}

#[cfg(feature = "no_std")]
pub struct PixelMut<'a, P: PixelBytes> {
    chunk: &'a mut [u8],
    value: P,
}

#[cfg(feature = "no_std")]
impl<P: PixelBytes> core::ops::Deref for PixelMut<'_, P> {
    type Target = P;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[cfg(feature = "no_std")]
impl<P: PixelBytes> core::ops::DerefMut for PixelMut<'_, P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

#[cfg(feature = "no_std")]
impl<P: PixelBytes> Drop for PixelMut<'_, P> {
    fn drop(&mut self) {
        self.value.write_bytes(self.chunk);
    }
}

#[cfg(feature = "no_std")]
#[derive(Clone, Debug)]
pub enum DynamicImage {
    ImageLumaA8(GrayAlphaImage),
    ImageRgba8(RgbaImage),
}

#[cfg(feature = "no_std")]
impl DynamicImage {
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::ImageLumaA8(img) => (img.width(), img.height()),
            Self::ImageRgba8(img) => (img.width(), img.height()),
        }
    }

    pub fn to_rgba8(&self) -> RgbaImage {
        match self {
            Self::ImageRgba8(img) => img.clone(),
            Self::ImageLumaA8(img) => {
                let mut out = RgbaImage::new(img.width(), img.height());
                for y in 0..img.height() {
                    for x in 0..img.width() {
                        let idx = (y as usize * img.width() as usize + x as usize) * 2;
                        let l = img.as_raw()[idx];
                        let a = img.as_raw()[idx + 1];
                        out.put_pixel(x, y, Rgba([l, l, l, a]));
                    }
                }
                out
            }
        }
    }

    pub fn as_mut_rgba8(&mut self) -> Option<&mut RgbaImage> {
        match self {
            Self::ImageRgba8(img) => Some(img),
            _ => None,
        }
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> Rgba<u8> {
        match self {
            Self::ImageRgba8(img) => img.get_pixel(x, y),
            Self::ImageLumaA8(img) => {
                let p = img.get_pixel(x, y);
                Rgba([p.0[0], p.0[0], p.0[0], p.0[1]])
            }
        }
    }

    pub fn view(&self, x: u32, y: u32, width: u32, height: u32) -> ImageView<'_> {
        ImageView {
            image: self,
            x,
            y,
            width,
            height,
        }
    }
}

#[cfg(feature = "no_std")]
pub struct ImageView<'a> {
    image: &'a DynamicImage,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[cfg(feature = "no_std")]
impl ImageView<'_> {
    pub fn get_pixel(&self, x: u32, y: u32) -> Rgba<u8> {
        if x >= self.width || y >= self.height {
            return Rgba([0, 0, 0, 0]);
        }
        self.image.get_pixel(self.x + x, self.y + y)
    }
}

#[cfg(feature = "no_std")]
pub trait GenericImageView {
    fn dimensions(&self) -> (u32, u32);
}

#[cfg(feature = "no_std")]
impl GenericImageView for DynamicImage {
    fn dimensions(&self) -> (u32, u32) {
        DynamicImage::dimensions(self)
    }
}

#[cfg(feature = "no_std")]
pub mod boxed {
    pub use alloc::boxed::*;
}

#[cfg(feature = "no_std")]
pub mod collections {
    pub use alloc::collections::{BTreeMap, BTreeSet, LinkedList, VecDeque};
    pub use hashbrown::{HashMap, HashSet};
}

#[cfg(feature = "no_std")]
pub mod io {
    use alloc::string::String;
    use alloc::vec::Vec;
    use core::fmt;

    #[derive(Debug, Clone)]
    pub struct Error {
        message: String,
    }

    impl Error {
        pub fn new(_kind: ErrorKind, message: impl fmt::Display) -> Self {
            Self {
                message: alloc::format!("{}", message),
            }
        }
    }

    impl fmt::Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.message)
        }
    }

    impl core::error::Error for Error {}

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ErrorKind {
        Other,
        InvalidInput,
        Unsupported,
        UnexpectedEof,
    }

    pub type Result<T> = core::result::Result<T, Error>;

    pub trait Read {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

        fn read_to_end(&mut self, out: &mut Vec<u8>) -> Result<usize> {
            let start = out.len();
            let mut buf = [0u8; 4096];
            loop {
                let n = self.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..n]);
            }
            Ok(out.len() - start)
        }

        fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<()> {
            while !buf.is_empty() {
                let n = self.read(buf)?;
                if n == 0 {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "unexpected EOF"));
                }
                let rest = buf.split_at_mut(n).1;
                buf = rest;
            }
            Ok(())
        }
    }

    #[derive(Debug, Clone, Copy)]
    pub enum SeekFrom {
        Start(u64),
        End(i64),
        Current(i64),
    }

    pub trait Seek {
        fn seek(&mut self, pos: SeekFrom) -> Result<u64>;
    }

    pub trait Write {
        fn write(&mut self, buf: &[u8]) -> Result<usize>;

        fn write_all(&mut self, mut buf: &[u8]) -> Result<()> {
            while !buf.is_empty() {
                let n = self.write(buf)?;
                if n == 0 {
                    return Err(Error::new(ErrorKind::UnexpectedEof, "write returned zero"));
                }
                buf = &buf[n..];
            }
            Ok(())
        }
    }

    #[derive(Debug, Clone)]
    pub struct Cursor<T> {
        inner: T,
        pos: u64,
    }

    impl Cursor<Vec<u8>> {
        pub fn new(inner: Vec<u8>) -> Self {
            Self { inner, pos: 0 }
        }

        pub fn into_inner(self) -> Vec<u8> {
            self.inner
        }
    }

    impl Read for Cursor<Vec<u8>> {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
            let pos = self.pos as usize;
            if pos >= self.inner.len() {
                return Ok(0);
            }
            let n = buf.len().min(self.inner.len() - pos);
            buf[..n].copy_from_slice(&self.inner[pos..pos + n]);
            self.pos += n as u64;
            Ok(n)
        }
    }

    impl Seek for Cursor<Vec<u8>> {
        fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
            let len = self.inner.len() as i128;
            let next = match pos {
                SeekFrom::Start(pos) => pos as i128,
                SeekFrom::End(delta) => len + delta as i128,
                SeekFrom::Current(delta) => self.pos as i128 + delta as i128,
            }
            .clamp(0, len) as u64;
            self.pos = next;
            Ok(next)
        }
    }
}

#[cfg(feature = "no_std")]
pub mod path {
    use alloc::string::{String, ToString};
    use core::fmt;

    #[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct PathBuf {
        inner: String,
    }

    pub type Path = PathBuf;

    impl PathBuf {
        pub fn new() -> Self {
            Self {
                inner: String::new(),
            }
        }

        pub fn from(path: &str) -> Self {
            Self {
                inner: path.to_string(),
            }
        }

        pub fn from_string(path: String) -> Self {
            Self { inner: path }
        }

        pub fn push(&mut self, path: impl AsRef<str>) {
            if !self.inner.is_empty() && !self.inner.ends_with('/') {
                self.inner.push('/');
            }
            self.inner.push_str(path.as_ref().trim_matches('/'));
        }

        pub fn join(&self, path: impl AsRef<str>) -> Self {
            let mut out = self.clone();
            out.push(path);
            out
        }

        pub fn display(&self) -> Display<'_> {
            Display(&self.inner)
        }

        pub fn as_os_str(&self) -> &str {
            &self.inner
        }

        pub fn exists(&self) -> bool {
            false
        }

        pub fn is_dir(&self) -> bool {
            false
        }

        pub fn is_file(&self) -> bool {
            false
        }

        pub fn metadata(&self) -> crate::io::Result<crate::fs::Metadata> {
            Err(crate::io::Error::new(
                crate::io::ErrorKind::Unsupported,
                "host path metadata is not available through std::path in no_std",
            ))
        }

        pub fn parent(&self) -> Option<Self> {
            let trimmed = self.inner.trim_end_matches('/');
            let (parent, _) = trimmed.rsplit_once('/')?;
            Some(Self::from(parent))
        }

        pub fn file_name(&self) -> Option<FileName<'_>> {
            let name = self.inner.trim_end_matches('/').rsplit('/').next()?;
            if name.is_empty() {
                None
            } else {
                Some(FileName(name))
            }
        }

        pub fn file_stem(&self) -> Option<FileName<'_>> {
            let name = self.file_name()?.0;
            let stem = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name);
            if stem.is_empty() {
                None
            } else {
                Some(FileName(stem))
            }
        }

        pub fn extension(&self) -> Option<FileName<'_>> {
            let name = self.file_name()?.0;
            let (_, ext) = name.rsplit_once('.')?;
            if ext.is_empty() {
                None
            } else {
                Some(FileName(ext))
            }
        }

        pub fn to_string_lossy(&self) -> String {
            self.inner.clone()
        }

        pub fn get_path(&self) -> &Self {
            self
        }

        pub fn components(&self) -> Components<'_> {
            Components {
                inner: self.inner.split('/'),
            }
        }
    }

    impl AsRef<str> for PathBuf {
        fn as_ref(&self) -> &str {
            &self.inner
        }
    }

    impl AsRef<PathBuf> for PathBuf {
        fn as_ref(&self) -> &PathBuf {
            self
        }
    }

    impl From<&str> for PathBuf {
        fn from(value: &str) -> Self {
            PathBuf::from(value)
        }
    }

    impl From<String> for PathBuf {
        fn from(value: String) -> Self {
            PathBuf::from_string(value)
        }
    }

    pub struct FileName<'a>(&'a str);

    impl<'a> FileName<'a> {
        pub fn to_str(&self) -> Option<&'a str> {
            Some(self.0)
        }

        pub fn to_string_lossy(&self) -> String {
            self.0.to_string()
        }
    }

    impl fmt::Display for FileName<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.0)
        }
    }

    pub struct Display<'a>(&'a str);

    impl fmt::Display for Display<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.0)
        }
    }

    pub enum Component<'a> {
        Normal(&'a str),
        CurDir,
        RootDir,
        Prefix(&'a str),
        ParentDir,
    }

    pub struct Components<'a> {
        inner: core::str::Split<'a, char>,
    }

    impl<'a> Iterator for Components<'a> {
        type Item = Component<'a>;

        fn next(&mut self) -> Option<Self::Item> {
            for part in self.inner.by_ref() {
                if part.is_empty() {
                    continue;
                }
                if part == "." {
                    return Some(Component::CurDir);
                }
                if part == ".." {
                    return Some(Component::ParentDir);
                }
                return Some(Component::Normal(part));
            }
            None
        }
    }
}

#[cfg(feature = "no_std")]
pub mod fs {
    use alloc::vec::Vec;

    use crate::io::{Error, ErrorKind, Result};
    use crate::path::PathBuf;

    pub struct File;
    #[derive(Debug, Clone, Copy)]
    pub struct Metadata {
        len: u64,
        is_file: bool,
    }

    impl Metadata {
        pub fn len(&self) -> u64 {
            self.len
        }

        pub fn is_file(&self) -> bool {
            self.is_file
        }
    }

    impl File {
        pub fn open(_path: impl AsRef<str>) -> Result<Self> {
            Err(Error::new(
                ErrorKind::Unsupported,
                "std::fs::File is not available in no_std; use host_api filesystem",
            ))
        }
    }

    pub fn read(_path: impl AsRef<str>) -> Result<Vec<u8>> {
        Err(Error::new(
            ErrorKind::Unsupported,
            "std::fs::read is not available in no_std; use host_api filesystem",
        ))
    }

    pub fn write(_path: impl AsRef<str>, _bytes: &[u8]) -> Result<()> {
        Err(Error::new(
            ErrorKind::Unsupported,
            "std::fs::write is not available in no_std; use host_api filesystem",
        ))
    }

    pub fn create_dir_all(_path: impl AsRef<str>) -> Result<()> {
        Ok(())
    }

    pub fn create_dir(_path: impl AsRef<str>) -> Result<()> {
        Ok(())
    }

    pub fn remove_file(_path: impl AsRef<str>) -> Result<()> {
        Ok(())
    }

    pub fn copy(_src: impl AsRef<str>, _dst: impl AsRef<str>) -> Result<u64> {
        Err(Error::new(
            ErrorKind::Unsupported,
            "std::fs::copy is not available in no_std; use host_api filesystem",
        ))
    }

    pub fn read_dir(_path: impl AsRef<str>) -> Result<alloc::vec::IntoIter<Result<DirEntry>>> {
        Ok(Vec::new().into_iter())
    }

    pub struct DirEntry;

    impl DirEntry {
        pub fn path(&self) -> PathBuf {
            PathBuf::new()
        }
    }
}

#[cfg(feature = "no_std")]
pub mod env {
    use crate::path::PathBuf;

    pub fn var_os(_key: &str) -> Option<&'static str> {
        None
    }

    pub fn current_dir() -> crate::io::Result<PathBuf> {
        Ok(PathBuf::new())
    }

    pub fn current_exe() -> crate::io::Result<PathBuf> {
        Ok(PathBuf::new())
    }
}

#[cfg(feature = "no_std")]
pub mod fmt {
    pub use core::fmt::*;
}

#[cfg(feature = "no_std")]
pub mod mem {
    pub use core::mem::*;
}

#[cfg(feature = "no_std")]
pub mod ops {
    pub use core::ops::*;
}

#[cfg(feature = "no_std")]
pub mod ptr {
    pub use core::ptr::*;
}

#[cfg(feature = "no_std")]
pub mod sync {
    pub use alloc::sync::{Arc, Weak};
    pub mod atomic {
        pub use core::sync::atomic::*;
    }

    pub struct Mutex<T>(spin::Mutex<T>);

    impl<T> Mutex<T> {
        pub const fn new(value: T) -> Self {
            Self(spin::Mutex::new(value))
        }

        pub fn lock(&self) -> Result<spin::MutexGuard<'_, T>, ()> {
            Ok(self.0.lock())
        }
    }
}

#[cfg(feature = "no_std")]
pub mod cmp {
    pub use core::cmp::*;
}

#[cfg(feature = "no_std")]
pub mod hint {
    pub use core::hint::*;
}

#[cfg(feature = "no_std")]
pub mod string {
    pub use alloc::string::*;
}

#[cfg(all(
    feature = "no_std",
    any(
        feature = "runtime-core-deps",
        feature = "gpu-render",
        feature = "soft-render-core",
        feature = "soft-render",
        feature = "soft-render-desktop",
        feature = "rfvp-os",
        feature = "audio",
        feature = "no-audio",
        feature = "anzu-audio",
        feature = "mp4",
        feature = "native-video",
        feature = "uefi-native-video",
        feature = "wasm",
        feature = "image-formats",
        feature = "logging",
        feature = "zlib-flate2",
        feature = "uefi-zlib",
        feature = "random",
        feature = "uuid-support",
        feature = "bevy-utils",
        feature = "fontdue-compat",
        feature = "cursor-ani",
    )
))]
compile_error!("feature `no_std` is an independent core-library build and must not be combined with runtime/backend features");

pub mod host_api;

#[cfg(feature = "no_std")]
pub mod no_std_core;

#[cfg(feature = "no_std")]
pub use no_std_core::{
    RfvpBootConfig, RfvpCore, RfvpCoreConfig, RfvpCoreRunState, RfvpLoadedGame, RfvpResourceEntry,
    RfvpTickResult,
};

#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
pub mod app;
pub mod audio_player;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
pub mod boot;
#[cfg(not(feature = "no_std"))]
pub mod config;
#[cfg(not(feature = "no_std"))]
pub mod debug_ui;
#[cfg(feature = "no_std")]
pub mod debug_ui {
    pub fn enabled() -> bool {
        false
    }

    pub mod vm_snapshot {
        use crate::subsystem::resources::thread_manager::ThreadManager;

        #[derive(Debug, Default)]
        pub struct VmSnapshot;

        impl VmSnapshot {
            pub fn update_from_thread_manager(&mut self, _thread_manager: &ThreadManager) {}
        }
    }
}
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
pub mod exit_confirm_ui;
pub mod font;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
pub mod legacy_save_load_ui;
pub(crate) mod platform_random;
pub(crate) mod platform_time;
pub mod rendering;
#[cfg(not(feature = "no_std"))]
pub mod rfvp_audio;
#[cfg(feature = "no_std")]
#[path = "rfvp_audio_no_std.rs"]
pub mod rfvp_audio;
#[cfg(all(not(feature = "no_std"), feature = "rfvp-os"))]
pub mod rfvp_os_host;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
pub mod rfvp_render;
pub mod script;
#[cfg(all(not(feature = "no_std"), feature = "soft-render-desktop"))]
pub mod soft_host;
#[cfg(all(
    not(feature = "no_std"),
    any(
        feature = "soft-render-core",
        feature = "soft-render",
        feature = "soft-render-desktop"
    )
))]
pub mod soft_render;
pub mod subsystem;
pub mod trace;
#[cfg(not(feature = "no_std"))]
pub mod utils;
#[cfg(feature = "no_std")]
#[path = "utils_no_std.rs"]
pub mod utils;
pub mod vm_runner;
#[cfg(not(feature = "no_std"))]
pub mod vm_worker;
#[cfg(not(feature = "no_std"))]
pub mod window;

#[cfg(all(not(feature = "no_std"), target_arch = "wasm32", feature = "mp4"))]
compile_error!("rfvp wasm build must use --no-default-features --features wasm");

#[cfg(all(not(feature = "no_std"), target_arch = "wasm32"))]
pub mod wasm_app_path;

#[cfg(all(not(feature = "no_std"), target_arch = "wasm32"))]
pub mod wasm_entry;

#[cfg(all(not(feature = "no_std"), target_os = "ios"))]
mod ios_host;

#[cfg(all(not(feature = "no_std"), target_os = "android"))]
mod android_host;

#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use crate::platform_time::Duration;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use std::ffi::{CStr, CString};
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use std::os::raw::c_char;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use std::ptr::null_mut;

#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use crate::app::App;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use crate::script::parser::Nls;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use crate::subsystem::anzu_scene::AnzuScene;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use crate::subsystem::resources::thread_manager::ThreadManager;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use crate::utils::file::set_base_path;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use anyhow::Result;
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use boot::{app_config, load_script};
#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
use log::LevelFilter;

#[cfg(all(
    not(feature = "no_std"),
    feature = "gpu-render",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
use winit::platform::pump_events::PumpStatus;

#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
fn run_rfvp(game_root: &str, nls: Nls) -> Result<()> {
    set_base_path(game_root);
    let parser = load_script(nls)?;
    let title = parser.get_title();
    let size = parser.get_screen_size();
    let script_engine = ThreadManager::new();

    App::app_with_config(app_config(&title, size))
        .with_scene::<AnzuScene>()
        .with_script_engine(script_engine)
        .with_window_title(&title)
        .with_window_size(size)
        .with_parser(parser)
        .with_vfs(nls)?
        .run();

    Ok(())
}

/// Opaque pump handle for GUI hosts (e.g. SwiftUI launcher) that already own the platform main loop.
#[cfg(all(
    not(feature = "no_std"),
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
#[cfg(feature = "gpu-render")]
pub struct RfvpPumpHandle {
    inst: crate::app::PumpInstance,
}

/// Create a pump-driven instance. Returns NULL on error.
#[cfg(all(
    not(feature = "no_std"),
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
#[cfg(feature = "gpu-render")]
#[no_mangle]
pub unsafe extern "C" fn rfvp_pump_create(
    game_root_utf8: *const c_char,
    nls_utf8: *const c_char,
) -> *mut RfvpPumpHandle {
    if game_root_utf8.is_null() || nls_utf8.is_null() {
        return null_mut();
    }

    let game_root = match CStr::from_ptr(game_root_utf8).to_str() {
        Ok(s) if !s.is_empty() => s.to_string(),
        _ => return null_mut(),
    };

    let nls_str = match CStr::from_ptr(nls_utf8).to_str() {
        Ok(s) if !s.is_empty() => s.to_string(),
        _ => return null_mut(),
    };

    let nls: Nls = match nls_str.parse() {
        Ok(v) => v,
        Err(e) => {
            log::error!("rfvp_pump_create: invalid NLS '{nls_str}': {e:?}");
            return null_mut();
        }
    };

    // Build the app but do not enter the blocking run loop.
    set_base_path(&game_root);
    let parser = match load_script(nls) {
        Ok(p) => p,
        Err(e) => {
            log::error!("rfvp_pump_create: failed to load script: {e:?}");
            return null_mut();
        }
    };
    let title = parser.get_title();
    let size = parser.get_screen_size();
    let script_engine = ThreadManager::new();

    let builder = match App::app_with_config(app_config(&title, size))
        .with_scene::<AnzuScene>()
        .with_script_engine(script_engine)
        .with_window_title(&title)
        .with_window_size(size)
        .with_parser(parser)
        .with_vfs(nls)
    {
        Ok(b) => b,
        Err(e) => {
            log::error!("rfvp_pump_create: failed to build AppBuilder: {e:?}");
            return null_mut();
        }
    };

    let inst = match builder.build_pump() {
        Ok(i) => i,
        Err(e) => {
            log::error!("rfvp_pump_create: build_pump failed: {e:?}");
            return null_mut();
        }
    };

    Box::into_raw(Box::new(RfvpPumpHandle { inst }))
}

/// Pump events for up to `timeout_ms` milliseconds.
///
/// Return values:
/// - 0: continue running
/// - 1: app requested exit
/// - 2: invalid handle
#[cfg(all(
    not(feature = "no_std"),
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
#[cfg(feature = "gpu-render")]
#[no_mangle]
pub unsafe extern "C" fn rfvp_pump_step(handle: *mut RfvpPumpHandle, timeout_ms: u32) -> i32 {
    if handle.is_null() {
        return 2;
    }
    let h = &mut *handle;
    match h
        .inst
        .pump(Duration::from_millis(std::cmp::max(timeout_ms as u64, 1)))
    {
        PumpStatus::Continue => 0,
        _ => 1,
    }
}

/// Destroy a pump-driven instance created by `rfvp_pump_create`.
#[cfg(all(
    not(feature = "no_std"),
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
#[cfg(feature = "gpu-render")]
#[no_mangle]
pub unsafe extern "C" fn rfvp_pump_destroy(handle: *mut RfvpPumpHandle) {
    if handle.is_null() {
        return;
    }
    drop(Box::from_raw(handle));
}

#[cfg(all(not(feature = "no_std"), feature = "gpu-render"))]
#[no_mangle]
pub unsafe extern "C" fn rfvp_run_entry(
    game_root_utf8: *const c_char,
    nls_utf8: *const c_char,
) -> i32 {
    if game_root_utf8.is_null() || nls_utf8.is_null() {
        return 2;
    }

    let game_root = match CStr::from_ptr(game_root_utf8).to_str() {
        Ok(s) => s.to_string(),
        _ => {
            return 3;
        }
    };

    let nls_str = match CStr::from_ptr(nls_utf8).to_str() {
        Ok(s) if !s.is_empty() => s.to_lowercase(),
        _ => {
            return 4;
        }
    };

    let nls = match nls_str.as_str() {
        "shiftjis" | "sjis" => Nls::ShiftJIS,
        "utf8" | "utf-8" => Nls::UTF8,
        "gbk" | "gb2312" => Nls::GBK,
        _ => {
            return 5;
        }
    };

    match run_rfvp(&game_root, nls) {
        Ok(_) => 0,
        Err(e) => {
            log::error!("Error running RFVP: {:?}", e);
            1
        }
    }
}
