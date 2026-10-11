//! Desktop harness for the `no_std` core.
//!
//! Boots [`RfvpCore`] on a real game directory through the same host API the console ports use
//! (file system, renderer, audio, clock), with a plain std-backed host. This is how the console
//! code paths (pack streaming, boot, VM, rendering commands) can be exercised without a device.
//!
//! Run with a game directory, for example:
//!
//! ```text
//! RFVP_HARNESS_GAME=/path/to/game RFVP_HARNESS_FONT=/path/to/font.otf \
//!   cargo test -p rfvp --lib --no-default-features --features no_std --release \
//!   no_std_harness -- --nocapture
//! ```
//!
//! Optional: `RFVP_HARNESS_FRAMES` (default 600), `RFVP_HARNESS_NLS` (`sjis`, `gbk`, `utf8`),
//! `RFVP_HARNESS_OUT` (directory to write PPM screenshots to), `RFVP_HARNESS_OUT_W` and
//! `RFVP_HARNESS_OUT_H` (screen size, default 1280x720), `RFVP_HARNESS_START` (`x,y` of the
//! title menu entry to click as fractions of the screen, default `0.144,0.5`, which is "New
//! Game" on the games this was tried on) and `RFVP_HARNESS_SKIP_FROM` (frame from which Ctrl is
//! held down to skip text). `RFVP_HARNESS_NO_CLICKS` turns all clicks off.

extern crate std as real_std;

/// Heap in use and its peak, to size the engine for small consoles (the renderer's textures
/// are counted too: subtract `texture_stats`).
mod heap {
    use core::sync::atomic::{AtomicUsize, Ordering};
    use super::real_std::alloc::{GlobalAlloc, Layout, System};

    pub static CURRENT: AtomicUsize = AtomicUsize::new(0);
    pub static PEAK: AtomicUsize = AtomicUsize::new(0);

    /// Live allocations of at least `BIG` bytes, with where they were made, when
    /// `RFVP_HARNESS_HEAP_SITES` is set.
    pub static BIG_BYTES: AtomicUsize = AtomicUsize::new(32 * 1024);
    pub const BIG: usize = 32 * 1024;
    pub static TRACK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
    super::real_std::thread_local! {
        static IN_TRACK: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    }
    pub static SITES: super::real_std::sync::Mutex<Option<super::real_std::collections::HashMap<usize, (usize, super::real_std::string::String)>>> =
        super::real_std::sync::Mutex::new(None);

    fn note(ptr: *mut u8, size: usize) {
        if size < BIG_BYTES.load(Ordering::Relaxed) || !TRACK.load(Ordering::Relaxed) || IN_TRACK.with(|t| t.replace(true)) {
            return;
        }
        let bt = super::real_std::format!("{}", super::real_std::backtrace::Backtrace::force_capture());
        // Keep the frames in this crate, past the allocator itself.
        let site: super::real_std::vec::Vec<&str> = bt
            .lines()
            .filter(|l| l.contains("rfvp::") && !l.contains("no_std_harness::heap"))
            .take(6)
            .collect();
        let site = site.join(" <- ").replace("             ", "");
        if super::real_std::env::var_os("RFVP_HARNESS_HEAP_PRINT").is_some() {
            super::real_std::eprintln!("alloc {size} at {site}");
        }
        SITES.lock().unwrap().get_or_insert_with(Default::default).insert(ptr as usize, (size, site));
        IN_TRACK.with(|t| t.set(false));
    }

    fn forget(ptr: *mut u8, size: usize) {
        if size < BIG_BYTES.load(Ordering::Relaxed) || !TRACK.load(Ordering::Relaxed) || IN_TRACK.with(|t| t.get()) {
            return;
        }
        IN_TRACK.with(|t| t.set(true));
        if let Some(map) = SITES.lock().unwrap().as_mut() {
            map.remove(&(ptr as usize));
        }
        IN_TRACK.with(|t| t.set(false));
    }

    struct Counting;

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let p = unsafe { System.alloc(layout) };
            if !p.is_null() {
                let now = CURRENT.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
                PEAK.fetch_max(now, Ordering::Relaxed);
                note(p, layout.size());
            }
            p
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            forget(ptr, layout.size());
            unsafe { System.dealloc(ptr, layout) };
            CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
        }
    }

    #[global_allocator]
    static COUNTING: Counting = Counting;
}

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use real_std::path::{Path, PathBuf};
use real_std::time::Instant;
use real_std::{env, fs, println, process};

use crate::host_api::soft_raster::SoftRaster;
use crate::host_api::std_fs::{StdDir, StdFile};
use crate::host_api::*;
use crate::script::parser::Nls;
use crate::{RfvpBootConfig, RfvpCore, RfvpCoreConfig};

struct StdFs {
    root: PathBuf,
    font: Option<PathBuf>,
    /// Where saves go, if not next to the game.
    save_root: Option<PathBuf>,
    /// Bytes read through `open`, to show that packs are not loaded whole.
    opened: Vec<String>,
}

impl StdFs {
    fn resolve(&self, path: &str) -> PathBuf {
        let p = Path::new(path.trim_start_matches("./"));
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        }
    }
}

impl RfvpFileSystem for StdFs {
    type File = StdFile;

    fn open(&mut self, path: &str) -> RfvpResult<Self::File> {
        let mut resolved = self.resolve(path);
        if !resolved.exists() && path == "default.ttf" {
            if let Some(font) = &self.font {
                resolved = font.clone();
            }
        }
        self.opened.push(path.to_string());
        fs::File::open(&resolved)
            .map(StdFile)
            .map_err(|_| RfvpError::NotFound)
    }

    fn game_files(&mut self) -> Option<alloc::sync::Arc<dyn RfvpStorage>> {
        Some(alloc::sync::Arc::new(StdDir::new(self.root.clone())))
    }

    fn storage(&mut self) -> Option<alloc::sync::Arc<dyn RfvpStorage>> {
        Some(alloc::sync::Arc::new(StdDir::new(
            self.save_root.clone().unwrap_or_else(|| self.root.clone()),
        )))
    }

    fn write_all(&mut self, path: &str, bytes: &[u8]) -> RfvpResult<()> {
        let resolved = self.resolve(path);
        if let Some(parent) = resolved.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(resolved, bytes).map_err(|_| RfvpError::Io)
    }

    fn metadata(&mut self, path: &str) -> RfvpResult<RfvpFileInfo> {
        let meta = fs::metadata(self.resolve(path)).map_err(|_| RfvpError::NotFound)?;
        Ok(RfvpFileInfo {
            len: meta.len(),
            kind: if meta.is_dir() {
                RfvpFileKind::Directory
            } else {
                RfvpFileKind::File
            },
        })
    }

    fn enumerate_by_extension(
        &mut self,
        root: &str,
        extension_without_dot: &str,
        visitor: &mut dyn FnMut(&str, RfvpFileInfo) -> RfvpResult<()>,
    ) -> RfvpResult<()> {
        let mut stack = Vec::new();
        stack.push(self.resolve(root));
        while let Some(dir) = stack.pop() {
            let Ok(read) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                if meta.is_dir() {
                    stack.push(path);
                } else if path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case(extension_without_dot))
                {
                    let text = path.to_string_lossy().to_string();
                    visitor(&text, RfvpFileInfo::file(meta.len()))?;
                }
            }
        }
        Ok(())
    }
}

/// The shared software rasterizer, plus counters.
struct CountingRaster {
    inner: SoftRaster,
    sprites: u64,
    /// Print each sprite drawn (for comparing with a console's list).
    log_sprites: bool,
    solids: u64,
    additive: u64,
    /// Frames put on the screen.
    presents: u64,
    peak_texture_bytes: usize,
    /// Time spent inside the rasterizer.
    raster_time: real_std::time::Duration,
}

impl CountingRaster {
    fn new(out_width: u32, out_height: u32) -> Self {
        Self {
            inner: SoftRaster::new(out_width, out_height),
            sprites: 0,
            log_sprites: false,
            solids: 0,
            additive: 0,
            presents: 0,
            peak_texture_bytes: 0,
            raster_time: real_std::time::Duration::ZERO,
        }
    }

    fn write_ppm(&self, path: &Path) {
        let (w, h) = self.inner.output_size();
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        for px in self.inner.frame() {
            out.extend_from_slice(&[*px as u8, (*px >> 8) as u8, (*px >> 16) as u8]);
        }
        let _ = fs::write(path, out);
    }
}

impl RfvpRenderer for CountingRaster {
    fn create_texture(
        &mut self,
        id: TextureId,
        desc: TextureDesc,
        pixels: Option<&[u8]>,
    ) -> RfvpResult<()> {
        RfvpRenderer::create_texture(&mut self.inner, id, desc, pixels)?;
        self.peak_texture_bytes = self.peak_texture_bytes.max(self.inner.texture_stats().1);
        Ok(())
    }
    fn update_texture(
        &mut self,
        id: TextureId,
        rect: TextureRect,
        pixels: &[u8],
    ) -> RfvpResult<()> {
        RfvpRenderer::update_texture(&mut self.inner, id, rect, pixels)
    }
    fn destroy_texture(&mut self, id: TextureId) {
        RfvpRenderer::destroy_texture(&mut self.inner, id)
    }
    fn begin_frame(&mut self, width: u32, height: u32, clear: Option<ColorRgba>) -> RfvpResult<()> {
        let started = Instant::now();
        let result = RfvpRenderer::begin_frame(&mut self.inner, width, height, clear);
        self.raster_time += started.elapsed();
        result
    }
    fn draw_sprite(&mut self, command: &DrawSpriteCommand) -> RfvpResult<()> {
        self.sprites += 1;
        if self.log_sprites {
            let v = &command.vertices;
            println!(
                "  tex {} at ({:.0},{:.0})-({:.0},{:.0}) a {:.2} {:?}",
                command.texture.0,
                v[0].position[0],
                v[0].position[1],
                v[3].position[0],
                v[3].position[1],
                v[0].color.a,
                command.blend
            );
        }
        if command.blend == BlendMode::Add {
            self.additive += 1;
        }
        let started = Instant::now();
        let result = RfvpRenderer::draw_sprite(&mut self.inner, command);
        self.raster_time += started.elapsed();
        result
    }
    fn draw_solid(&mut self, command: &DrawSolidCommand) -> RfvpResult<()> {
        self.solids += 1;
        RfvpRenderer::draw_solid(&mut self.inner, command)
    }
    fn end_frame(&mut self) -> RfvpResult<()> {
        RfvpRenderer::end_frame(&mut self.inner)
    }
    fn present(&mut self) -> RfvpResult<()> {
        self.presents += 1;
        RfvpRenderer::present(&mut self.inner)
    }
    fn capture_thumbnail(&mut self, width: u32, height: u32) -> Option<Vec<u8>> {
        RfvpRenderer::capture_thumbnail(&mut self.inner, width, height)
    }
}

#[derive(Default)]
struct CountingAudio {
    loaded: u64,
    played: u64,
    loaded_bytes: u64,
}

impl RfvpAudio for CountingAudio {
    fn load_encoded(
        &mut self,
        _id: AudioStreamId,
        _kind: EncodedAudioKind,
        bytes: &[u8],
    ) -> RfvpResult<()> {
        self.loaded += 1;
        self.loaded_bytes += bytes.len() as u64;
        Ok(())
    }
    fn create_stream(&mut self, _id: AudioStreamId, _desc: AudioStreamDesc) -> RfvpResult<()> {
        Ok(())
    }
    fn submit_i16(&mut self, _id: AudioStreamId, _samples: &[i16]) -> RfvpResult<()> {
        Ok(())
    }
    fn submit_f32(&mut self, _id: AudioStreamId, _samples: &[f32]) -> RfvpResult<()> {
        Ok(())
    }
    fn play(
        &mut self,
        _id: AudioStreamId,
        _params: AudioParams,
        _fade_in_ms: u32,
    ) -> RfvpResult<()> {
        self.played += 1;
        Ok(())
    }
    fn stop(&mut self, _id: AudioStreamId, _fade_ms: u32) -> RfvpResult<()> {
        Ok(())
    }
    fn pause(&mut self, _id: AudioStreamId) -> RfvpResult<()> {
        Ok(())
    }
    fn resume(&mut self, _id: AudioStreamId) -> RfvpResult<()> {
        Ok(())
    }
    fn set_params(&mut self, _id: AudioStreamId, _params: AudioParams) -> RfvpResult<()> {
        Ok(())
    }
    fn set_master_volume(&mut self, _volume: f32) -> RfvpResult<()> {
        Ok(())
    }
    fn destroy_stream(&mut self, _id: AudioStreamId) {}
    fn tick(&mut self, _delta_us: u64) -> RfvpResult<()> {
        Ok(())
    }
}

struct FakeClock(u64);

impl RfvpClock for FakeClock {
    fn ticks_us(&mut self) -> u64 {
        self.0
    }
}

struct Host {
    fs: StdFs,
    renderer: CountingRaster,
    audio: CountingAudio,
    clock: FakeClock,
    logs: Vec<String>,
}

impl RfvpHost for Host {
    type FileSystem = StdFs;
    type Renderer = CountingRaster;
    type Audio = CountingAudio;
    type Clock = FakeClock;

    fn fs(&mut self) -> &mut StdFs {
        &mut self.fs
    }
    fn renderer(&mut self) -> &mut CountingRaster {
        &mut self.renderer
    }
    fn audio(&mut self) -> &mut CountingAudio {
        &mut self.audio
    }
    fn clock(&mut self) -> &mut FakeClock {
        &mut self.clock
    }

    fn log(&mut self, level: RfvpLogLevel, message: &str) {
        if matches!(level, RfvpLogLevel::Error | RfvpLogLevel::Warn) {
            println!("[{level:?}] {message}");
        }
        if self.logs.len() < 64 {
            self.logs.push(message.to_string());
        }
    }
}

fn resident_kib() -> u64 {
    let out = process::Command::new("ps")
        .args(["-o", "rss=", "-p", &process::id().to_string()])
        .output()
        .ok();
    out.and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// The engine keeps the script's global variables in a process-wide singleton, so only one game
/// can be running in the process at a time: the tests take turns.
static ONE_GAME_AT_A_TIME: real_std::sync::Mutex<()> = real_std::sync::Mutex::new(());

/// A booted game and the host it runs on.
struct Run {
    _turn: real_std::sync::MutexGuard<'static, ()>,
    core: RfvpCore,
    host: Host,
    /// Frames ticked so far.
    frame: u32,
    /// Size of the game's virtual frame.
    width: i32,
    height: i32,
    start_click: (i32, i32),
}

impl Run {
    /// Boot the game named by `RFVP_HARNESS_GAME`, or `None` (so the test is skipped) without it.
    fn start() -> Option<Self> {
        let Ok(game) = env::var("RFVP_HARNESS_GAME") else {
            println!("RFVP_HARNESS_GAME is not set; skipping");
            return None;
        };
        // Collect the engine's log lines, to show with a boot failure.
        let _ = crate::ring_log::install();
        // A test that failed while holding its turn must not stop the others.
        let turn = ONE_GAME_AT_A_TIME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let nls = match env::var("RFVP_HARNESS_NLS").as_deref() {
            Ok("gbk") => Nls::GBK,
            Ok("utf8") => Nls::UTF8,
            _ => Nls::ShiftJIS,
        };
        // The size of the screen being drawn to; the game's virtual frame is scaled to fit.
        let out_w: u32 = env::var("RFVP_HARNESS_OUT_W")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1280);
        let out_h: u32 = env::var("RFVP_HARNESS_OUT_H")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(720);

        // Never write saves into the game directory the harness is pointed at.
        let saves = env::var("RFVP_HARNESS_SAVES")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                // One directory per test: they run side by side in one process.
                let test = real_std::thread::current()
                    .name()
                    .unwrap_or("test")
                    .replace("::", "-");
                env::temp_dir().join(format!("rfvp-harness-saves-{}-{test}", process::id()))
            });
        let _ = fs::remove_dir_all(&saves);

        let mut host = Host {
            fs: StdFs {
                root: PathBuf::from(&game),
                font: env::var("RFVP_HARNESS_FONT").ok().map(PathBuf::from),
                save_root: Some(saves),
                opened: Vec::new(),
            },
            renderer: CountingRaster::new(out_w, out_h),
            audio: CountingAudio::default(),
            clock: FakeClock(0),
            logs: Vec::new(),
        };

        let rss_before = resident_kib();
        let mut core = RfvpCore::new(RfvpCoreConfig::default());
        let boot = RfvpBootConfig {
            asset_root: ".",
            nls,
            ..RfvpBootConfig::default()
        };
        core.boot(&mut host, boot).unwrap_or_else(|err| {
            panic!(
                "boot failed: {err:?} {:?}\nlogs: {:#?}\nengine log:\n{}",
                core.last_error_detail(),
                host.logs,
                crate::ring_log::dump()
            )
        });
        println!(
            "booted: virtual {}x{}, rss {} MiB -> {} MiB, files opened: {:?}",
            core.config().virtual_width,
            core.config().virtual_height,
            rss_before / 1024,
            resident_kib() / 1024,
            host.fs.opened
        );

        let (width, height) = (
            core.config().virtual_width as i32,
            core.config().virtual_height as i32,
        );
        let start_click = env::var("RFVP_HARNESS_START")
            .ok()
            .or(Some("0.144,0.5".into()))
            .and_then(|v| {
                let (fx, fy) = v.split_once(',')?;
                Some((
                    (fx.parse::<f32>().ok()? * width as f32) as i32,
                    (fy.parse::<f32>().ok()? * height as f32) as i32,
                ))
            })
            .unwrap_or((width / 2, height / 2));
        Some(Self {
            _turn: turn,
            core,
            host,
            frame: 0,
            width,
            height,
            start_click,
        })
    }

    fn click(&mut self, x: i32, y: i32) {
        let _ = self.core.push_event(RfvpEvent::PointerMove {
            x,
            y,
            in_screen: true,
        });
        let _ = self.core.push_event(RfvpEvent::PointerDown {
            button: PointerButton::Left,
            x,
            y,
        });
    }

    fn release(&mut self, x: i32, y: i32) {
        let _ = self.core.push_event(RfvpEvent::PointerUp {
            button: PointerButton::Left,
            x,
            y,
        });
    }

    fn set_skip(&mut self, skip: bool) {
        let event = if skip {
            RfvpEvent::KeyDown {
                key: KeyCode::Control,
                repeat: false,
                modifiers: InputModifiers::CONTROL,
            }
        } else {
            RfvpEvent::KeyUp {
                key: KeyCode::Control,
                modifiers: InputModifiers::empty(),
            }
        };
        let _ = self.core.push_event(event);
    }

    /// Tick one frame, clicking the way a player gets through the logos, the title menu and the
    /// text: in the middle of the screen every half second, and on the first title entry at two
    /// fixed frames.
    fn step(&mut self) {
        let frame = self.frame;
        self.host.clock.0 += 16_667;
        let (cx, cy) = if frame == 665 || frame == 725 {
            self.start_click
        } else {
            (self.width / 2, self.height / 2)
        };
        // `RFVP_HARNESS_NO_CLICKS`: no clicks at all, as a console left alone.
        let idle = env::var_os("RFVP_HARNESS_NO_CLICKS").is_some();
        if !idle && (frame % 30 == 5 || frame == 665 || frame == 725) {
            self.click(cx, cy);
        }
        if !idle && (frame % 30 == 8 || frame == 668 || frame == 728) {
            self.release(cx, cy);
        }
        if let Err(err) = self.core.tick(&mut self.host) {
            panic!(
                "tick {frame} failed: {err:?} {:?}",
                self.core.last_error_detail()
            );
        }
        self.frame += 1;
    }

    fn run(&mut self, frames: u32) {
        for _ in 0..frames {
            self.step();
        }
    }

    /// A hash of the script VM's state.
    fn vm_hash(&self) -> u64 {
        let snap = self.core.vm_state().expect("no VM");
        fnv1a_bytes(&crate::subsystem::snapshot_codec::encode(&snap).unwrap())
    }

    /// Bytes the engine keeps for decoded images (the CPU copies it holds besides what the
    /// renderer has), and how many graphs hold one.
    fn cpu_image_bytes(&mut self) -> (usize, usize) {
        let mut bytes = 0;
        let mut count = 0;
        for graph in self.core.game_data_mut().motion_manager.graphs().iter() {
            if graph.get_texture().is_some() {
                bytes += graph.get_width() as usize * graph.get_height() as usize * 4;
                count += 1;
            }
        }
        (bytes, count)
    }

    /// The graphs that are loaded, as (slot, file).
    fn graphs(&mut self) -> Vec<(u16, String)> {
        self.core
            .game_data_mut()
            .motion_manager
            .capture_snapshot_v1()
            .textures
            .into_iter()
            .map(|t| (t.id, t.texture_path))
            .collect()
    }
}

fn fnv1a_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[test]
fn boots_and_runs_a_real_game() {
    let heap_sites: Option<u32> = env::var("RFVP_HARNESS_HEAP_SITES").ok().and_then(|v| v.parse().ok());
    if heap_sites.is_some() {
        heap::TRACK.store(true, core::sync::atomic::Ordering::Relaxed);
        if let Some(min) = env::var("RFVP_HARNESS_HEAP_MIN").ok().and_then(|v| v.parse().ok()) {
            heap::BIG_BYTES.store(min, core::sync::atomic::Ordering::Relaxed);
        }
    }
    let Some(mut run) = Run::start() else {
        return;
    };
    let frames: u32 = env::var("RFVP_HARNESS_FRAMES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600);
    let skip_from: Option<u32> = env::var("RFVP_HARNESS_SKIP_FROM")
        .ok()
        .and_then(|v| v.parse().ok());
    let out_dir = env::var("RFVP_HARNESS_OUT").ok().map(PathBuf::from);
    // `RFVP_HARNESS_SPRITES`: a frame whose sprites are printed.
    let sprites_at: Option<u32> = env::var("RFVP_HARNESS_SPRITES")
        .ok()
        .and_then(|v| v.parse().ok());
    let (out_w, out_h) = run.host.renderer.inner.output_size();

    let mut peak_rss = resident_kib();
    let mut frame_time = real_std::time::Duration::ZERO;
    let mut worst = real_std::time::Duration::ZERO;
    for frame in 0..frames {
        if skip_from == Some(frame) {
            run.set_skip(true);
        }
        run.host.renderer.log_sprites = sprites_at == Some(frame);
        let started = Instant::now();
        run.step();
        if sprites_at == Some(frame) {
            println!("{}", run.core.debug_threads().trim_end());
        }
        let took = started.elapsed();
        if heap_sites == Some(frame) {
            // The big live allocations, largest first.
            let map = heap::SITES.lock().unwrap().clone().unwrap_or_default();
            // Summed by where they were made (the first two frames of the site).
            let mut by_site: std::collections::HashMap<String, (usize, usize)> = Default::default();
            for (size, site) in map.into_values() {
                let key: String = site.split(" <- ").take(2).collect::<Vec<_>>().join(" <- ");
                let e = by_site.entry(key).or_default();
                e.0 += size;
                e.1 += 1;
            }
            let mut sites: Vec<(String, (usize, usize))> = by_site.into_iter().collect();
            sites.sort_by(|a, b| b.1.0.cmp(&a.1.0));
            let total: usize = sites.iter().map(|s| s.1.0).sum();
            println!(
                "live allocations of {} bytes or more at frame {frame}: {} KiB",
                heap::BIG_BYTES.load(core::sync::atomic::Ordering::Relaxed),
                total / 1024
            );
            for (site, (size, count)) in sites.iter().take(40) {
                println!("  {:6} KiB in {count:5}  {site}", size / 1024);
            }
        }
        frame_time += took;
        worst = worst.max(took);
        if frame % 60 == 0 {
            peak_rss = peak_rss.max(resident_kib());
            let (cpu_bytes, cpu_images) = run.cpu_image_bytes();
            let host = &run.host;
            let textures = host.renderer.inner.texture_stats().1;
            println!(
                "frame {frame}: heap {} KiB (peak {} KiB) without textures, rss {} MiB, textures {} ({} MiB), CPU images {} ({} MiB), sprites {}, additive {}, solids {}, audio loaded {} played {}",
                heap::CURRENT.load(core::sync::atomic::Ordering::Relaxed).saturating_sub(textures) / 1024,
                heap::PEAK.load(core::sync::atomic::Ordering::Relaxed) / 1024,
                resident_kib() / 1024,
                host.renderer.inner.texture_stats().0,
                host.renderer.inner.texture_stats().1 / (1024 * 1024),
                cpu_images,
                cpu_bytes / (1024 * 1024),
                host.renderer.sprites,
                host.renderer.additive,
                host.renderer.solids,
                host.audio.loaded,
                host.audio.played,
            );
            if let Some(dir) = &out_dir {
                if frame % 300 == 0 {
                    let _ = fs::create_dir_all(dir);
                    host.renderer
                        .write_ppm(&dir.join(format!("frame_{frame:05}.ppm")));
                }
            }
        }
    }
    println!(
        "done: {frames} frames at {out_w}x{out_h}, peak rss {} MiB, peak texture memory {} MiB, \
         avg {:.2} ms/frame ({:.2} ms of it in the rasterizer), worst {:.2} ms",
        peak_rss / 1024,
        run.host.renderer.peak_texture_bytes / (1024 * 1024),
        frame_time.as_secs_f64() * 1000.0 / frames as f64,
        run.host.renderer.raster_time.as_secs_f64() * 1000.0 / frames as f64,
        worst.as_secs_f64() * 1000.0,
    );
    assert!(run.host.renderer.sprites > 0, "nothing was drawn");
    // Every tick draws the frame once and puts it on the screen once: a host that never
    // presented would show a black screen, and one that drew twice would waste half its time.
    assert_eq!(
        run.host.renderer.presents, frames as u64,
        "the frame must be presented exactly once per tick"
    );
}

/// Save in the middle of the story, play on, load: the script and the scene must be back where
/// they were saved, and the slot must be on disk the way a real game would leave it.
#[test]
fn saves_and_loads_a_slot() {
    let Some(mut run) = Run::start() else {
        return;
    };
    // Into the story, skipping text.
    run.run(1500);
    run.set_skip(true);
    run.run(1500);
    run.set_skip(false);
    run.run(30);

    let vm_at_save = run.vm_hash();
    let graphs_at_save = run.graphs();
    assert!(!graphs_at_save.is_empty());

    // What `SaveCreate(3, 7)` followed by `SaveWrite(7)` ask of the engine.
    {
        let saves = run.core.game_data_mut();
        saves
            .save_manager
            .set_current_title(String::from("harness save"));
        saves
            .save_manager
            .set_current_scene_title(String::from("scene title"));
        saves.save_manager.request_prepare_local_savedata();
        saves.save_manager.set_savedata_requested(true);
        saves.save_manager.set_current_save_slot(7);
    }
    run.step();
    run.run(2);

    let storage = run.host.fs.storage().expect("no storage");
    let file = storage
        .read("save/rfvp_s007.bin")
        .expect("the slot file was not written");
    println!("slot file is {} bytes", file.len());
    let item =
        crate::subsystem::resources::save_manager::SaveItem::load_from_mem(&file, Nls::ShiftJIS)
            .unwrap();
    assert_eq!(item.title, "harness save");
    assert_eq!(item.scene_title, "scene title");
    // The game may have set its own thumbnail size with SaveThumbSize.
    let (thumb_w, thumb_h) = {
        let saves = &run.core.game_data_mut().save_manager;
        (saves.get_thumb_width(), saves.get_thumb_height())
    };
    println!("thumbnail is {thumb_w}x{thumb_h}");
    let thumb = crate::subsystem::resources::save_manager::SaveItem::read_thumb_texture_from_mem(
        &file, thumb_w, thumb_h,
    )
    .unwrap();
    assert!(
        thumb.chunks_exact(4).any(|p| p[..3] != [0, 0, 0]),
        "the thumbnail is blank"
    );
    assert!(
        crate::subsystem::save_state::try_decode_state_chunk_v1(&file)
            .unwrap()
            .is_some(),
        "no engine state in the slot file"
    );
    assert!(run.core.game_data_mut().save_manager.test_save_slot(7));

    // Play on, further into the story.
    run.set_skip(true);
    run.run(1200);
    run.set_skip(false);
    run.run(30);
    assert_ne!(run.vm_hash(), vm_at_save, "the story did not move on");

    // Load it back.
    run.core.game_data_mut().save_manager.request_load(7);
    run.step();
    assert_eq!(
        run.vm_hash(),
        vm_at_save,
        "the script is not where it was saved"
    );
    assert_eq!(run.graphs(), graphs_at_save, "the scene was not restored");

    // The restored game keeps running.
    run.run(120);
}

/// The global save (read-text marks, flags) is written during play and restored by the next
/// boot.
#[test]
fn the_global_save_is_written_and_read_back() {
    let Some(mut run) = Run::start() else {
        return;
    };
    run.run(1500);
    run.set_skip(true);
    run.run(900);
    // Leaving the game writes the global save.
    let _ = run.core.push_event(RfvpEvent::Quit);
    run.step();
    let storage = run.host.fs.storage().expect("no storage");
    let global = storage
        .read("save/rfvp_global.bin")
        .expect("the global save was not written");
    println!("global save is {} bytes", global.len());
    assert!(
        crate::subsystem::global_savedata::try_decode_global_savedata_v1(&global)
            .unwrap()
            .is_some()
    );

    // A fresh start of the game on the same storage restores what was read.
    let read_marks = |core: &mut RfvpCore| -> u32 {
        core.game_data_mut()
            .motion_manager
            .text_manager
            .readed_text
            .iter()
            .map(|word| word.count_ones())
            .sum()
    };
    let before = read_marks(&mut run.core);
    println!("{before} lines marked as read");
    assert!(before > 0, "playing through text marked nothing as read");

    let mut second = RfvpCore::new(RfvpCoreConfig::default());
    let boot = RfvpBootConfig {
        asset_root: ".",
        ..RfvpBootConfig::default()
    };
    second
        .boot(&mut run.host, boot)
        .expect("second boot failed");
    assert_eq!(
        read_marks(&mut second),
        before,
        "the read-text marks were not restored"
    );
}
