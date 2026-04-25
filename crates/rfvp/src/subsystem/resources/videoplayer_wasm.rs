use std::collections::VecDeque;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};

use crate::platform_time::Instant;
use crate::rfvp_audio::AudioManager;

use na_mpeg2_decoder::{MpegAvEvent, MpegAvPipeline};
use wmv_decoder::AsfWmv2Decoder;

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
const MPEG_CHUNK_SIZE: usize = 64 * 1024;

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

struct WmvPlayback {
    dec: AsfWmv2Decoder<Cursor<Vec<u8>>>,
    stash: VecDeque<WmvRgbaFrame>,
    started_at: Option<Instant>,
    base_pts_ms: Option<u32>,
    last_presented_pts_ms: u32,
    eof: bool,
    screen_w: u32,
    screen_h: u32,
}

impl WmvPlayback {
    fn open_from_bytes(bytes: Vec<u8>, screen_w: u32, screen_h: u32) -> Result<Self> {
        let dec = AsfWmv2Decoder::open(Cursor::new(bytes)).context("wasm WMV AsfWmv2Decoder::open")?;
        Ok(Self {
            dec,
            stash: VecDeque::new(),
            started_at: None,
            base_pts_ms: None,
            last_presented_pts_ms: u32::MAX,
            eof: false,
            screen_w,
            screen_h,
        })
    }

    fn fill_stash(&mut self) -> Result<()> {
        while !self.eof && self.stash.len() < MAX_MOVIE_STASH_FRAMES {
            match self.dec.next_frame().context("wasm WMV next_frame")? {
                Some(frame) => {
                    let mut rgba = Vec::new();
                    yuv420_to_rgba_scaled(&frame.frame, self.screen_w, self.screen_h, &mut rgba);
                    self.stash.push_back(WmvRgbaFrame { pts_ms: frame.pts_ms, rgba });
                }
                None => {
                    self.eof = true;
                    break;
                }
            }
        }
        Ok(())
    }

    fn next_due_frame(&mut self) -> Result<Option<WmvRgbaFrame>> {
        self.fill_stash()?;
        if self.started_at.is_none() {
            if let Some(front) = self.stash.front() {
                self.started_at = Some(Instant::now());
                self.base_pts_ms = Some(front.pts_ms);
            } else {
                return Ok(None);
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
        Ok(latest_due)
    }

    fn is_finished(&mut self) -> Result<bool> {
        self.fill_stash()?;
        Ok(self.eof && self.stash.is_empty())
    }

    fn stop_and_cleanup(self) {}
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

struct MpegPlayback {
    data: Vec<u8>,
    pos: usize,
    pipe: MpegAvPipeline,
    stash: VecDeque<MpegRgbaFrame>,
    started_at: Option<Instant>,
    base_pts_ms: Option<i64>,
    last_presented_pts_ms: i64,
    eof: bool,
    flushed: bool,
    screen_w: u32,
    screen_h: u32,
}

impl MpegPlayback {
    fn open_from_bytes(bytes: Vec<u8>, screen_w: u32, screen_h: u32) -> Result<Self> {
        Ok(Self {
            data: bytes,
            pos: 0,
            pipe: MpegAvPipeline::new(),
            stash: VecDeque::new(),
            started_at: None,
            base_pts_ms: None,
            last_presented_pts_ms: i64::MIN,
            eof: false,
            flushed: false,
            screen_w,
            screen_h,
        })
    }

    fn push_video_event(&mut self, ev: MpegAvEvent) {
        if let MpegAvEvent::Video(v) = ev {
            let rgba = if v.width == self.screen_w && v.height == self.screen_h {
                v.rgba
            } else {
                let mut out = Vec::new();
                rgba_scale_nearest(&v.rgba, v.width, v.height, self.screen_w, self.screen_h, &mut out);
                out
            };
            self.stash.push_back(MpegRgbaFrame { pts_ms: v.pts_ms, rgba });
        }
    }

    fn fill_stash(&mut self) -> Result<()> {
        while self.stash.len() < MAX_MOVIE_STASH_FRAMES && !self.eof {
            if self.pos >= self.data.len() {
                self.eof = true;
                break;
            }
            let end = self.pos.saturating_add(MPEG_CHUNK_SIZE).min(self.data.len());
            let chunk = self.data[self.pos..end].to_vec();
            self.pos = end;

            let mut events = Vec::new();
            self.pipe.push_with(&chunk, None, |ev| events.push(ev)).context("wasm MPEG push")?;
            for ev in events {
                self.push_video_event(ev);
            }
        }

        if self.eof && !self.flushed {
            self.flushed = true;
            let mut events = Vec::new();
            self.pipe.flush_with(|ev| events.push(ev)).context("wasm MPEG flush")?;
            for ev in events {
                self.push_video_event(ev);
            }
        }
        Ok(())
    }

    fn next_due_frame(&mut self) -> Result<Option<MpegRgbaFrame>> {
        self.fill_stash()?;
        if self.started_at.is_none() {
            if let Some(front) = self.stash.front() {
                self.started_at = Some(Instant::now());
                self.base_pts_ms = Some(front.pts_ms);
            } else {
                return Ok(None);
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
        Ok(latest_due)
    }

    fn is_finished(&mut self) -> Result<bool> {
        self.fill_stash()?;
        Ok(self.eof && self.flushed && self.stash.is_empty())
    }

    fn stop_and_cleanup(self) {}
}

enum Playback {
    Wmv(WmvPlayback),
    Mpeg(MpegPlayback),
}

#[derive(Default)]
pub struct VideoPlayerManager {
    playing: bool,
    modal: bool,
    playback: Option<Playback>,
}

impl std::fmt::Debug for VideoPlayerManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoPlayerManager")
            .field("playing", &self.playing)
            .field("modal", &self.modal)
            .field("loaded", &self.playback.is_some())
            .finish()
    }
}

impl VideoPlayerManager {
    pub fn new() -> Self { Self::default() }
    pub fn is_playing(&self) -> bool { self.playing }
    pub fn is_loaded(&self) -> bool { self.playback.is_some() }
    pub fn is_modal_active(&self) -> bool { self.playing && self.modal }

    pub fn start(
        &mut self,
        movie_path: impl AsRef<Path>,
        mode: MovieMode,
        screen_w: u32,
        screen_h: u32,
        motion: &mut MotionManager,
        audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<()> {
        let _ = (movie_path, mode, screen_w, screen_h, motion, audio_manager);
        Err(anyhow!("wasm movie playback requires start_from_bytes"))
    }

    pub fn start_from_bytes(
        &mut self,
        movie_name: &str,
        bytes: Vec<u8>,
        mode: MovieMode,
        screen_w: u32,
        screen_h: u32,
        motion: &mut MotionManager,
        _audio_manager: Option<Arc<AudioManager>>,
    ) -> Result<()> {
        if self.playing {
            self.stop(motion);
        }

        let ext = Path::new(movie_name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let playback = if ext == "wmv" || ext == "asf" {
            Playback::Wmv(WmvPlayback::open_from_bytes(bytes, screen_w, screen_h)?)
        } else if matches!(ext.as_str(), "mpg" | "mpeg" | "m2v" | "ts" | "ps" | "vob" | "dat") {
            Playback::Mpeg(MpegPlayback::open_from_bytes(bytes, screen_w, screen_h)?)
        } else {
            return Err(anyhow!("unsupported wasm movie format: {movie_name}"));
        };

        if matches!(mode, MovieMode::ModalWithAudio) {
            log::warn!("wasm movie audio is not implemented; playing video only: {movie_name}");
        }

        self.playing = true;
        self.modal = matches!(mode, MovieMode::ModalWithAudio);
        self.playback = Some(playback);
        self.ensure_layer(motion, screen_w as i16, screen_h as i16);
        self.tick(motion)?;
        Ok(())
    }

    pub fn tick(&mut self, motion: &mut MotionManager) -> Result<()> {
        if !self.playing { return Ok(()); }
        let Some(pb) = self.playback.as_mut() else {
            self.playing = false;
            self.modal = false;
            return Ok(());
        };

        match pb {
            Playback::Wmv(wmv) => {
                if let Some(frame) = wmv.next_due_frame()? {
                    if frame.pts_ms != wmv.last_presented_pts_ms {
                        wmv.last_presented_pts_ms = frame.pts_ms;
                        motion.load_texture_from_buff(MOVIE_GRAPH_ID, frame.rgba, wmv.screen_w, wmv.screen_h)?;
                        motion.refresh_prims(MOVIE_GRAPH_ID);
                    }
                }
                if wmv.is_finished()? { self.stop(motion); }
            }
            Playback::Mpeg(mpeg) => {
                if let Some(frame) = mpeg.next_due_frame()? {
                    if frame.pts_ms != mpeg.last_presented_pts_ms {
                        mpeg.last_presented_pts_ms = frame.pts_ms;
                        motion.load_texture_from_buff(MOVIE_GRAPH_ID, frame.rgba, mpeg.screen_w, mpeg.screen_h)?;
                        motion.refresh_prims(MOVIE_GRAPH_ID);
                    }
                }
                if mpeg.is_finished()? { self.stop(motion); }
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
    if v < 0 { 0 } else if v > 255 { 255 } else { v as u8 }
}

fn yuv420_to_rgba_scaled(src: &wmv_decoder::YuvFrame, dst_w: u32, dst_h: u32, out: &mut Vec<u8>) {
    let sw = src.width.max(1);
    let sh = src.height.max(1);
    let dw = dst_w.max(1);
    let dh = dst_h.max(1);
    out.clear();
    out.resize((dw as usize).saturating_mul(dh as usize).saturating_mul(4), 0);

    let uv_w = (sw / 2).max(1);
    let uv_h = (sh / 2).max(1);

    for dy in 0..dh {
        let sy = ((dy as u64 * sh as u64) / dh as u64) as u32;
        let sy = sy.min(sh - 1);
        let suv = (sy / 2).min(uv_h - 1);

        for dx in 0..dw {
            let sx = ((dx as u64 * sw as u64) / dw as u64) as u32;
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
        let sy = ((dy as u64 * sh as u64) / dh as u64) as u32;
        let sy = sy.min(sh - 1);
        for dx in 0..dw {
            let sx = ((dx as u64 * sw as u64) / dw as u64) as u32;
            let sx = sx.min(sw - 1);
            let so = ((sy * sw + sx) as usize) * 4;
            let doff = ((dy * dw + dx) as usize) * 4;
            out[doff..doff + 4].copy_from_slice(&src[so..so + 4]);
        }
    }
}
