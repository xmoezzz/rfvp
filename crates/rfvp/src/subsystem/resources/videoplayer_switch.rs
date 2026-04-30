use std::collections::VecDeque;
use std::io::Read;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::{anyhow, Context, Result};
use na_mpeg2_decoder::{MpegAvEvent, MpegAvPipeline};

use crate::platform_time::Instant;
use crate::rfvp_audio::{AudioBus, AudioManager, PlayParams, SoundData, SoundHandle, Tween};

use super::motion_manager::MotionManager;
use super::prim::PrimType;

pub const MOVIE_GRAPH_ID: u16 = 4063;
pub const MOVIE_GROUP_PRIM_ID: i16 = 4095;
pub const MOVIE_SPRT_PRIM_ID: i16 = 4094;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovieMode {
    ModalWithAudio,
    LayerNoAudio,
}

const MAX_MOVIE_STASH_FRAMES: usize = 4;

#[derive(Clone)]
struct WmvRgbaFrame {
    pts_ms: u32,
    rgba: Vec<u8>,
}

impl std::fmt::Debug for WmvRgbaFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WmvRgbaFrame")
            .field("pts_ms", &self.pts_ms)
            .field("rgba_len", &self.rgba.len())
            .finish()
    }
}

#[derive(Debug)]
struct WmvPlayback {
    rx: crossbeam_channel::Receiver<WmvRgbaFrame>,
    stash: VecDeque<WmvRgbaFrame>,
    started_at: Option<Instant>,
    base_pts_ms: Option<u32>,
    last_presented_pts_ms: u32,
    stop_flag: Arc<AtomicBool>,
    screen_w: u32,
    screen_h: u32,
    audio_manager: Option<Arc<AudioManager>>,
    audio_data: Option<SoundData>,
    audio_handle: Option<SoundHandle>,
    audio_started: bool,
}

impl WmvPlayback {
    fn open_from_wmv_path(
        wmv_path: impl AsRef<Path>,
        mode: MovieMode,
        screen_w: u32,
        screen_h: u32,
        audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<Self> {
        let wmv_path = wmv_path.as_ref().to_path_buf();
        let (audio_data, audio_manager) = match mode {
            MovieMode::ModalWithAudio => {
                let am = audio_manager;
                let data = match am.as_ref() {
                    None => None,
                    Some(am) => match decode_wmv_audio_to_wav_bytes(&wmv_path)? {
                        Some(wav_bytes) => Some(am.load_sound(wav_bytes).context("Switch WMV audio load")?),
                        None => None,
                    },
                };
                (data, am)
            }
            MovieMode::LayerNoAudio => (None, None),
        };

        let stop_flag = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::bounded::<WmvRgbaFrame>(2);
        {
            let stop = stop_flag.clone();
            let path = wmv_path.clone();
            std::thread::spawn(move || {
                let f = match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(e) => {
                        log::warn!("Switch WMV decode: open failed: {e:?}");
                        return;
                    }
                };
                let mut dec = match wmv_decoder::AsfWmv2Decoder::open(std::io::BufReader::new(f)) {
                    Ok(d) => d,
                    Err(e) => {
                        log::warn!("Switch WMV decode: open decoder failed: {e:?}");
                        return;
                    }
                };
                while !stop.load(Ordering::Relaxed) {
                    let fr = match dec.next_frame() {
                        Ok(Some(fr)) => fr,
                        Ok(None) => break,
                        Err(e) => {
                            log::warn!("Switch WMV decode: next_frame failed: {e:?}");
                            break;
                        }
                    };
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut rgba = Vec::new();
                    yuv420_to_rgba_scaled(&fr.frame, screen_w, screen_h, &mut rgba);
                    if tx.send(WmvRgbaFrame { pts_ms: fr.pts_ms, rgba }).is_err() {
                        break;
                    }
                }
            });
        }

        Ok(Self {
            rx,
            stash: VecDeque::new(),
            started_at: None,
            base_pts_ms: None,
            last_presented_pts_ms: u32::MAX,
            stop_flag,
            screen_w,
            screen_h,
            audio_manager,
            audio_data,
            audio_handle: None,
            audio_started: false,
        })
    }

    fn maybe_start_audio(&mut self) {
        if self.audio_started {
            return;
        }
        self.audio_started = true;
        let Some(am) = self.audio_manager.as_ref() else {
            return;
        };
        let Some(data) = self.audio_data.as_ref() else {
            return;
        };
        match am.play_sound(data, PlayParams {
            bus: AudioBus::Movie,
            slot: 0,
            repeat: false,
            volume: 1.0,
            pan: 0.5,
            fade_in: Tween::default(),
        }) {
            Ok(handle) => self.audio_handle = Some(handle),
            Err(e) => log::warn!("Switch WMV audio play failed: {e:#}"),
        }
    }

    fn next_due_frame(&mut self) -> Option<WmvRgbaFrame> {
        while self.stash.len() < MAX_MOVIE_STASH_FRAMES {
            match self.rx.try_recv() {
                Ok(f) => self.stash.push_back(f),
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => break,
            }
        }
        if self.started_at.is_none() {
            if let Some(front) = self.stash.front() {
                self.started_at = Some(Instant::now());
                self.base_pts_ms = Some(front.pts_ms);
                self.maybe_start_audio();
            } else {
                return None;
            }
        }
        let elapsed_ms = self.started_at.unwrap().elapsed().as_millis() as u32;
        let target_pts_ms = self.base_pts_ms.unwrap_or(0).saturating_add(elapsed_ms);
        let mut latest_due = None;
        while let Some(front) = self.stash.front() {
            if front.pts_ms <= target_pts_ms {
                latest_due = self.stash.pop_front();
            } else {
                break;
            }
        }
        latest_due
    }

    fn is_finished(&mut self) -> bool {
        if !self.stash.is_empty() {
            return false;
        }
        match self.rx.try_recv() {
            Ok(f) => {
                self.stash.push_back(f);
                false
            }
            Err(crossbeam_channel::TryRecvError::Empty) => false,
            Err(crossbeam_channel::TryRecvError::Disconnected) => true,
        }
    }

    fn stop_and_cleanup(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let (Some(am), Some(mut h)) = (self.audio_manager.as_ref(), self.audio_handle.take()) {
            am.stop_handle(&mut h, Tween::default());
        }
    }
}

#[derive(Clone)]
struct MpegRgbaFrame {
    pts_ms: i64,
    rgba: Vec<u8>,
}

impl std::fmt::Debug for MpegRgbaFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MpegRgbaFrame")
            .field("pts_ms", &self.pts_ms)
            .field("rgba_len", &self.rgba.len())
            .finish()
    }
}

#[derive(Debug)]
struct MpegPlayback {
    rx: crossbeam_channel::Receiver<MpegRgbaFrame>,
    stash: VecDeque<MpegRgbaFrame>,
    started_at: Option<Instant>,
    base_pts_ms: Option<i64>,
    last_presented_pts_ms: i64,
    stop_flag: Arc<AtomicBool>,
    screen_w: u32,
    screen_h: u32,
    audio_manager: Option<Arc<AudioManager>>,
    audio_data: Option<SoundData>,
    audio_handle: Option<SoundHandle>,
    audio_started: bool,
}

impl MpegPlayback {
    fn open_from_mpeg_path(
        mpeg_path: impl AsRef<Path>,
        mode: MovieMode,
        screen_w: u32,
        screen_h: u32,
        audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<Self> {
        let mpeg_path = mpeg_path.as_ref().to_path_buf();
        let (audio_data, audio_manager) = match mode {
            MovieMode::ModalWithAudio => {
                let am = audio_manager;
                let data = match am.as_ref() {
                    None => None,
                    Some(am) => match decode_mpeg_audio_to_wav_bytes(&mpeg_path)? {
                        Some(wav_bytes) => Some(am.load_sound(wav_bytes).context("Switch MPEG audio load")?),
                        None => None,
                    },
                };
                (data, am)
            }
            MovieMode::LayerNoAudio => (None, None),
        };

        let stop_flag = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::bounded::<MpegRgbaFrame>(2);
        {
            let stop = stop_flag.clone();
            let path = mpeg_path.clone();
            std::thread::spawn(move || {
                let mut f = match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(e) => {
                        log::warn!("Switch MPEG decode: open failed: {e:?}");
                        return;
                    }
                };
                let mut pipe = MpegAvPipeline::new();
                let mut buf = vec![0u8; 64 * 1024];
                while !stop.load(Ordering::Relaxed) {
                    let n = match f.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(e) => {
                            log::warn!("Switch MPEG decode: read failed: {e:?}");
                            break;
                        }
                    };
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let r = pipe.push_with(&buf[..n], None, |ev| {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        if let MpegAvEvent::Video(v) = ev {
                            let rgba = if v.width == screen_w && v.height == screen_h {
                                v.rgba
                            } else {
                                let mut out = Vec::new();
                                rgba_scale_nearest(&v.rgba, v.width, v.height, screen_w, screen_h, &mut out);
                                out
                            };
                            let _ = tx.send(MpegRgbaFrame { pts_ms: v.pts_ms, rgba });
                        }
                    });
                    if let Err(e) = r {
                        log::warn!("Switch MPEG decode: push failed: {e:?}");
                        break;
                    }
                }
                let _ = pipe.flush_with(|ev| {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    if let MpegAvEvent::Video(v) = ev {
                        let rgba = if v.width == screen_w && v.height == screen_h {
                            v.rgba
                        } else {
                            let mut out = Vec::new();
                            rgba_scale_nearest(&v.rgba, v.width, v.height, screen_w, screen_h, &mut out);
                            out
                        };
                        let _ = tx.send(MpegRgbaFrame { pts_ms: v.pts_ms, rgba });
                    }
                });
            });
        }

        Ok(Self {
            rx,
            stash: VecDeque::new(),
            started_at: None,
            base_pts_ms: None,
            last_presented_pts_ms: i64::MIN,
            stop_flag,
            screen_w,
            screen_h,
            audio_manager,
            audio_data,
            audio_handle: None,
            audio_started: false,
        })
    }

    fn maybe_start_audio(&mut self) {
        if self.audio_started {
            return;
        }
        self.audio_started = true;
        let Some(am) = self.audio_manager.as_ref() else {
            return;
        };
        let Some(data) = self.audio_data.as_ref() else {
            return;
        };
        match am.play_sound(data, PlayParams {
            bus: AudioBus::Movie,
            slot: 0,
            repeat: false,
            volume: 1.0,
            pan: 0.5,
            fade_in: Tween::default(),
        }) {
            Ok(handle) => self.audio_handle = Some(handle),
            Err(e) => log::warn!("Switch MPEG audio play failed: {e:#}"),
        }
    }

    fn next_due_frame(&mut self) -> Option<MpegRgbaFrame> {
        while self.stash.len() < MAX_MOVIE_STASH_FRAMES {
            match self.rx.try_recv() {
                Ok(f) => self.stash.push_back(f),
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => break,
            }
        }
        if self.started_at.is_none() {
            if let Some(front) = self.stash.front() {
                self.started_at = Some(Instant::now());
                self.base_pts_ms = Some(front.pts_ms);
                self.maybe_start_audio();
            } else {
                return None;
            }
        }
        let elapsed_ms = self.started_at.unwrap().elapsed().as_millis() as i64;
        let target_pts_ms = self.base_pts_ms.unwrap_or(0).saturating_add(elapsed_ms);
        let mut latest_due = None;
        while let Some(front) = self.stash.front() {
            if front.pts_ms <= target_pts_ms {
                latest_due = self.stash.pop_front();
            } else {
                break;
            }
        }
        latest_due
    }

    fn is_finished(&mut self) -> bool {
        if !self.stash.is_empty() {
            return false;
        }
        match self.rx.try_recv() {
            Ok(f) => {
                self.stash.push_back(f);
                false
            }
            Err(crossbeam_channel::TryRecvError::Empty) => false,
            Err(crossbeam_channel::TryRecvError::Disconnected) => true,
        }
    }

    fn stop_and_cleanup(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let (Some(am), Some(mut h)) = (self.audio_manager.as_ref(), self.audio_handle.take()) {
            am.stop_handle(&mut h, Tween::default());
        }
    }
}

#[derive(Debug)]
enum Playback {
    Wmv(WmvPlayback),
    Mpeg(MpegPlayback),
}

#[derive(Debug, Default)]
pub struct VideoPlayerManager {
    playing: bool,
    modal: bool,
    playback: Option<Playback>,
}

impl VideoPlayerManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(
        &mut self,
        path: impl AsRef<Path>,
        mode: MovieMode,
        screen_w: u32,
        screen_h: u32,
        motion: &mut MotionManager,
        audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<()> {
        if self.playing {
            self.stop(motion);
        }
        let p = path.as_ref();
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let playback = if ext == "wmv" || ext == "asf" {
            Playback::Wmv(WmvPlayback::open_from_wmv_path(p, mode, screen_w, screen_h, audio_manager)?)
        } else if matches!(ext.as_str(), "mpg" | "mpeg" | "m2v" | "ts" | "ps" | "vob" | "dat") {
            Playback::Mpeg(MpegPlayback::open_from_mpeg_path(p, mode, screen_w, screen_h, audio_manager)?)
        } else {
            return Err(anyhow!(
                "Switch movie backend supports only WMV/ASF and MPEG files, got: {}",
                p.display()
            ));
        };

        self.playing = true;
        self.modal = matches!(mode, MovieMode::ModalWithAudio);
        self.playback = Some(playback);
        self.ensure_layer(motion, screen_w as i16, screen_h as i16);
        self.tick(motion)?;
        Ok(())
    }

    pub fn tick(&mut self, motion: &mut MotionManager) -> Result<()> {
        if !self.playing {
            return Ok(());
        }
        let Some(pb) = self.playback.as_mut() else {
            self.playing = false;
            self.modal = false;
            return Ok(());
        };
        match pb {
            Playback::Wmv(wmv) => {
                if let Some(frame) = wmv.next_due_frame() {
                    if frame.pts_ms != wmv.last_presented_pts_ms {
                        wmv.last_presented_pts_ms = frame.pts_ms;
                        motion.load_texture_from_buff(MOVIE_GRAPH_ID, frame.rgba, wmv.screen_w, wmv.screen_h)?;
                        motion.refresh_prims(MOVIE_GRAPH_ID);
                    }
                }
                if wmv.is_finished() {
                    self.stop(motion);
                }
            }
            Playback::Mpeg(mpeg) => {
                if let Some(frame) = mpeg.next_due_frame() {
                    if frame.pts_ms != mpeg.last_presented_pts_ms {
                        mpeg.last_presented_pts_ms = frame.pts_ms;
                        motion.load_texture_from_buff(MOVIE_GRAPH_ID, frame.rgba, mpeg.screen_w, mpeg.screen_h)?;
                        motion.refresh_prims(MOVIE_GRAPH_ID);
                    }
                }
                if mpeg.is_finished() {
                    self.stop(motion);
                }
            }
        }
        Ok(())
    }

    pub fn stop(&mut self, motion: &mut MotionManager) {
        self.playing = false;
        self.modal = false;
        let pm = &mut motion.prim_manager;
        pm.prim_set_draw(MOVIE_SPRT_PRIM_ID as i32, 0);
        pm.prim_set_draw(MOVIE_GROUP_PRIM_ID as i32, 0);
        pm.unlink_prim(MOVIE_SPRT_PRIM_ID);
        pm.unlink_prim(MOVIE_GROUP_PRIM_ID);
        if let Some(pb) = self.playback.take() {
            match pb {
                Playback::Wmv(wmv) => wmv.stop_and_cleanup(),
                Playback::Mpeg(mpeg) => mpeg.stop_and_cleanup(),
            }
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn is_loaded(&self) -> bool {
        self.playback.is_some()
    }

    pub fn is_modal_active(&self) -> bool {
        self.playing && self.modal
    }

    fn ensure_layer(&self, motion: &mut MotionManager, screen_w: i16, screen_h: i16) {
        let pm = &mut motion.prim_manager;
        let root = 0i32;
        pm.prim_init_with_type(MOVIE_GROUP_PRIM_ID, PrimType::PrimTypeGroup);
        pm.prim_set_pos(MOVIE_GROUP_PRIM_ID as i32, 0, 0);
        pm.prim_set_z(MOVIE_GROUP_PRIM_ID as i32, 32767);
        pm.prim_set_draw(MOVIE_GROUP_PRIM_ID as i32, 1);

        pm.prim_init_with_type(MOVIE_SPRT_PRIM_ID, PrimType::PrimTypeSprt);
        pm.prim_set_texture_id(MOVIE_SPRT_PRIM_ID as i32, -2);
        pm.prim_set_uv(MOVIE_SPRT_PRIM_ID as i32, 0, 0);
        pm.prim_set_pos(MOVIE_SPRT_PRIM_ID as i32, 0, 0);
        pm.prim_set_size(MOVIE_SPRT_PRIM_ID as i32, screen_w as i32, screen_h as i32);
        pm.prim_set_alpha(MOVIE_SPRT_PRIM_ID as i32, 255);
        pm.prim_set_blend(MOVIE_SPRT_PRIM_ID as i32, 0);
        pm.prim_set_z(MOVIE_SPRT_PRIM_ID as i32, 32767);
        pm.prim_set_draw(MOVIE_SPRT_PRIM_ID as i32, 1);

        pm.set_prim_group_in(MOVIE_GROUP_PRIM_ID as i32, MOVIE_SPRT_PRIM_ID as i32);
        pm.set_prim_group_in(root, MOVIE_GROUP_PRIM_ID as i32);
    }
}

#[inline]
fn clamp_u8(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

fn yuv420_to_rgba_scaled(src: &wmv_decoder::YuvFrame, dst_w: u32, dst_h: u32, out: &mut Vec<u8>) {
    let sw = src.width.max(1);
    let sh = src.height.max(1);
    let dw = dst_w.max(1);
    let dh = dst_h.max(1);
    out.clear();
    out.resize((dw as usize).saturating_mul(dh as usize).saturating_mul(4), 0);
    let uv_w = (sw / 2).max(1);
    for dy in 0..dh {
        let sy = (dy as u64 * sh as u64 / dh as u64) as u32;
        let sy = sy.min(sh - 1);
        let suv = (sy / 2).min((sh / 2).saturating_sub(1));
        for dx in 0..dw {
            let sx = (dx as u64 * sw as u64 / dw as u64) as u32;
            let sx = sx.min(sw - 1);
            let sux = (sx / 2).min(uv_w - 1);
            let y = src.y[(sy * sw + sx) as usize] as i32;
            let u = src.cb[(suv * uv_w + sux) as usize] as i32;
            let v = src.cr[(suv * uv_w + sux) as usize] as i32;
            let c = y - 16;
            let d = u - 128;
            let e = v - 128;
            let r = (298 * c + 409 * e + 128) >> 8;
            let g = (298 * c - 100 * d - 208 * e + 128) >> 8;
            let b = (298 * c + 516 * d + 128) >> 8;
            let off = ((dy * dw + dx) as usize) * 4;
            out[off] = clamp_u8(r);
            out[off + 1] = clamp_u8(g);
            out[off + 2] = clamp_u8(b);
            out[off + 3] = 255;
        }
    }
}

fn rgba_scale_nearest(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32, out: &mut Vec<u8>) {
    let sw = src_w.max(1);
    let sh = src_h.max(1);
    let dw = dst_w.max(1);
    let dh = dst_h.max(1);
    out.clear();
    out.resize((dw as usize).saturating_mul(dh as usize).saturating_mul(4), 0);
    for dy in 0..dh {
        let sy = (dy as u64 * sh as u64 / dh as u64) as u32;
        let sy = sy.min(sh - 1);
        for dx in 0..dw {
            let sx = (dx as u64 * sw as u64 / dw as u64) as u32;
            let sx = sx.min(sw - 1);
            let so = ((sy * sw + sx) as usize) * 4;
            let doff = ((dy * dw + dx) as usize) * 4;
            out[doff..doff + 4].copy_from_slice(&src[so..so + 4]);
        }
    }
}

fn decode_mpeg_audio_to_wav_bytes(mpeg_path: impl AsRef<Path>) -> Result<Option<Vec<u8>>> {
    let mut f = match std::fs::File::open(mpeg_path.as_ref()) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("Switch MPEG audio: open failed: {}: {e:?}", mpeg_path.as_ref().display());
            return Ok(None);
        }
    };
    let mut pipe = MpegAvPipeline::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut pcm: Vec<i16> = Vec::new();
    let mut sr: u32 = 0;
    let mut ch: u16 = 0;
    loop {
        let n = match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                log::warn!("Switch MPEG audio: read failed: {e:?}");
                break;
            }
        };
        pipe.push_with(&buf[..n], None, |ev| {
            if let MpegAvEvent::Audio(a) = ev {
                if sr == 0 {
                    sr = a.sample_rate;
                    ch = a.channels;
                }
                if a.sample_rate != sr || a.channels != ch || ch == 0 {
                    return;
                }
                for s in a.samples {
                    let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i32;
                    pcm.push(v.clamp(-32768, 32767) as i16);
                }
            }
        })?;
    }
    if pcm.is_empty() || sr == 0 || ch == 0 {
        return Ok(None);
    }
    Ok(Some(build_wav_pcm16le(&pcm, ch, sr)))
}

fn decode_wmv_audio_to_wav_bytes(wmv_path: impl AsRef<Path>) -> Result<Option<Vec<u8>>> {
    let f = match std::fs::File::open(wmv_path.as_ref()) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("Switch WMV audio: open failed: {}: {e:?}", wmv_path.as_ref().display());
            return Ok(None);
        }
    };
    let mut dec = match wmv_decoder::AsfWmaDecoder::open(std::io::BufReader::new(f)) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("Switch WMV audio: decoder open failed: {}: {e:?}", wmv_path.as_ref().display());
            return Ok(None);
        }
    };
    let sr = dec.sample_rate();
    let ch = dec.channels();
    if ch == 0 {
        return Ok(None);
    }
    let mut pcm: Vec<i16> = Vec::new();
    while let Some(fr) = dec.next_frame()? {
        for &s in fr.frame.samples.iter() {
            let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i32;
            pcm.push(v.clamp(-32768, 32767) as i16);
        }
    }
    if pcm.is_empty() {
        return Ok(None);
    }
    Ok(Some(build_wav_pcm16le(&pcm, ch, sr)))
}

fn build_wav_pcm16le(samples: &[i16], channels: u16, sample_rate: u32) -> Vec<u8> {
    let bytes_per_sample = 2u16;
    let block_align = channels.saturating_mul(bytes_per_sample);
    let byte_rate = sample_rate.saturating_mul(block_align as u32);
    let data_size = (samples.len() as u32).saturating_mul(2);
    let riff_size = 36u32.saturating_add(data_size);
    let mut out = Vec::with_capacity((44 + data_size) as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());
    for &s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}
