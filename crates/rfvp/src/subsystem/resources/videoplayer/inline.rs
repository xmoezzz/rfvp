use std::collections::VecDeque;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};

use crate::platform_time::Instant;

use na_mpeg2_decoder::{MpegAvEvent, MpegAvPipeline};
use wmv_decoder::AsfWmv2Decoder;

use super::{MovieFrame, MovieRequest, MovieSource};

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
        let dec =
            AsfWmv2Decoder::open(Cursor::new(bytes)).context("wasm WMV AsfWmv2Decoder::open")?;
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
                    self.stash.push_back(WmvRgbaFrame {
                        pts_ms: frame.pts_ms,
                        rgba,
                    });
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
                rgba_scale_nearest(
                    &v.rgba,
                    v.width,
                    v.height,
                    self.screen_w,
                    self.screen_h,
                    &mut out,
                );
                out
            };
            self.stash.push_back(MpegRgbaFrame {
                pts_ms: v.pts_ms,
                rgba,
            });
        }
    }

    fn fill_stash(&mut self) -> Result<()> {
        while self.stash.len() < MAX_MOVIE_STASH_FRAMES && !self.eof {
            if self.pos >= self.data.len() {
                self.eof = true;
                break;
            }
            let end = self
                .pos
                .saturating_add(MPEG_CHUNK_SIZE)
                .min(self.data.len());
            let chunk = self.data[self.pos..end].to_vec();
            self.pos = end;

            let mut events = Vec::new();
            self.pipe
                .push_with(&chunk, None, |ev| events.push(ev))
                .context("wasm MPEG push")?;
            for ev in events {
                self.push_video_event(ev);
            }
        }

        if self.eof && !self.flushed {
            self.flushed = true;
            let mut events = Vec::new();
            self.pipe
                .flush_with(|ev| events.push(ev))
                .context("wasm MPEG flush")?;
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
    out.resize(
        (dw as usize).saturating_mul(dh as usize).saturating_mul(4),
        0,
    );

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

fn rgba_scale_nearest(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    out: &mut Vec<u8>,
) {
    let sw = src_w.max(1);
    let sh = src_h.max(1);
    let dw = dst_w.max(1);
    let dh = dst_h.max(1);
    out.clear();
    out.resize(
        (dw as usize).saturating_mul(dh as usize).saturating_mul(4),
        0,
    );

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

impl MovieSource for WmvPlayback {
    fn next_frame(&mut self) -> Result<Option<MovieFrame>> {
        let Some(frame) = self.next_due_frame()? else {
            return Ok(None);
        };
        if frame.pts_ms == self.last_presented_pts_ms {
            return Ok(None);
        }
        self.last_presented_pts_ms = frame.pts_ms;
        Ok(Some(MovieFrame {
            rgba: frame.rgba,
            width: self.screen_w,
            height: self.screen_h,
        }))
    }

    fn is_finished(&mut self) -> Result<bool> {
        WmvPlayback::is_finished(self)
    }

    fn stop(self: Box<Self>) {
        self.stop_and_cleanup();
    }
}

impl MovieSource for MpegPlayback {
    fn next_frame(&mut self) -> Result<Option<MovieFrame>> {
        let Some(frame) = self.next_due_frame()? else {
            return Ok(None);
        };
        if frame.pts_ms == self.last_presented_pts_ms {
            return Ok(None);
        }
        self.last_presented_pts_ms = frame.pts_ms;
        Ok(Some(MovieFrame {
            rgba: frame.rgba,
            width: self.screen_w,
            height: self.screen_h,
        }))
    }

    fn is_finished(&mut self) -> Result<bool> {
        MpegPlayback::is_finished(self)
    }

    fn stop(self: Box<Self>) {
        self.stop_and_cleanup();
    }
}

/// Open a movie for the inline decoders, which take the whole file from the VFS.
pub(super) fn open(request: &MovieRequest<'_>) -> Result<Box<dyn MovieSource>> {
    let name = request.name;
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_wmv = ext == "wmv" || ext == "asf";
    let is_mpeg = matches!(
        ext.as_str(),
        "mpg" | "mpeg" | "m2v" | "ts" | "ps" | "vob" | "dat"
    );
    if !is_wmv && !is_mpeg {
        return Err(anyhow!("unsupported movie format for the inline decoders: {name}"));
    }

    let bytes = request
        .vfs
        .read_file(name)
        .with_context(|| format!("read movie {name}"))?;
    if request.audio.is_some() {
        log::warn!("movie audio is not implemented for the inline decoders; playing video only: {name}");
    }
    let (w, h) = (request.screen_w, request.screen_h);
    Ok(if is_wmv {
        Box::new(WmvPlayback::open_from_bytes(bytes, w, h)?)
    } else {
        Box::new(MpegPlayback::open_from_bytes(bytes, w, h)?)
    })
}
