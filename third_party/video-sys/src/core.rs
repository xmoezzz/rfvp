use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender, TrySendError};

use crate::backend::{create_default_h264_decoder, H264Decoder};
use crate::mp4::{EncodedSample, Mp4H264Source};

#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub pts_us: i64,
    pub rgba: Vec<u8>,
}

pub struct VideoCore {
    path: PathBuf,
    src: Mp4H264Source,
    dec: Box<dyn H264Decoder>,

    pending: Option<EncodedSample>,
    stash: VecDeque<VideoFrame>,

    eof: bool,
    flushed: bool,
}

impl VideoCore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let src = Mp4H264Source::open(&path).context("open mp4 source")?;
        let dec = create_default_h264_decoder(&src.config).context("create decoder backend")?;

        Ok(Self {
            path,
            src,
            dec,
            pending: None,
            stash: VecDeque::new(),
            eof: false,
            flushed: false,
        })
    }

    pub fn width(&self) -> u32 {
        self.src.config.width
    }

    pub fn height(&self) -> u32 {
        self.src.config.height
    }

    pub fn is_eof(&self) -> bool {
        self.eof
    }

    pub fn reset(&mut self) -> Result<()> {
        let src = Mp4H264Source::open(&self.path)?;
        let dec = create_default_h264_decoder(&src.config)?;
        self.src = src;
        self.dec = dec;

        self.pending = None;
        self.stash.clear();
        self.eof = false;
        self.flushed = false;
        Ok(())
    }

    /// Feed some compressed samples into the decoder and drain all available decoded frames.
    ///
    /// This is a pure "producer" pump: it does NOT follow wall-clock or playhead.
    /// The renderer/player should decide what to present based on PTS.
    pub fn pump(&mut self) -> Result<()> {
        // Feed a bounded number of samples per pump to avoid monopolizing CPU.
        const FEED_BUDGET: usize = 32;

        if !self.eof {
            for _ in 0..FEED_BUDGET {
                match self.next_sample_cached()? {
                    Some(s) => {
                        self.dec.push(s)?;
                    }
                    None => {
                        self.eof = true;
                        if !self.flushed {
                            self.dec.flush()?;
                            self.flushed = true;
                        }
                        break;
                    }
                }
            }
        }

        // Drain decoder outputs into stash (do NOT drop anything here).
        while let Some(f) = self.dec.try_receive()? {
            self.stash.push_back(VideoFrame {
                width: f.width,
                height: f.height,
                pts_us: f.pts_us,
                rgba: f.rgba,
            });
        }

        Ok(())
    }

    /// Pop the next decoded frame in presentation order (if any).
    pub fn pop_decoded(&mut self) -> Option<VideoFrame> {
        self.stash.pop_front()
    }

    /// Finished means: we reached EOF, flushed, and no pending input/output remains.
    pub fn is_finished(&self) -> bool {
        self.eof && self.flushed && self.pending.is_none() && self.stash.is_empty()
    }

    fn next_sample_cached(&mut self) -> Result<Option<EncodedSample>> {
        if let Some(s) = self.pending.take() {
            return Ok(Some(s));
        }
        self.src.next_sample()
    }
}

#[derive(Debug)]
pub struct VideoStream {
    width: u32,
    height: u32,
    rx: Receiver<VideoFrame>,
    stop: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl VideoStream {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        let src = Mp4H264Source::open(&path).context("open mp4 source for config")?;
        let width = src.config.width;
        let height = src.config.height;
        drop(src);

        // Larger buffer reduces producer/consumer phase mismatch.
        let (tx, rx) = crossbeam_channel::bounded::<VideoFrame>(120);
        let stop = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));

        let stop_t = stop.clone();
        let finished_t = finished.clone();
        let path_t = path.clone();

        let join = thread::spawn(move || {
            let mut core = match VideoCore::open(&path_t) {
                Ok(c) => c,
                Err(e) => {
                    log::error!("VideoCore::open failed in decode thread: {e:?}");
                    finished_t.store(true, Ordering::Relaxed);
                    return;
                }
            };

            let mut pending_out: VecDeque<VideoFrame> = VecDeque::new();

            loop {
                if stop_t.load(Ordering::Relaxed) {
                    break;
                }

                // 1) Flush pending_out to channel first (never drop).
                while let Some(f) = pending_out.pop_front() {
                    match tx.try_send(f) {
                        Ok(()) => {}
                        Err(TrySendError::Full(f)) => {
                            pending_out.push_front(f);
                            break;
                        }
                        Err(TrySendError::Disconnected(_)) => {
                            finished_t.store(true, Ordering::Relaxed);
                            return;
                        }
                    }
                }

                // If channel is full, do not pump more; let consumer catch up.
                if !pending_out.is_empty() {
                    thread::yield_now();
                    continue;
                }

                // 2) Produce: feed decoder and drain decoded frames into core.stash.
                if let Err(e) = core.pump() {
                    log::error!("video decode thread pump error: {e:?}");
                    finished_t.store(true, Ordering::Relaxed);
                    return;
                }

                let mut produced_any = false;
                while let Some(frame) = core.pop_decoded() {
                    produced_any = true;
                    pending_out.push_back(frame);
                }

                // 3) Immediately try to ship freshly produced frames.
                while let Some(f) = pending_out.pop_front() {
                    match tx.try_send(f) {
                        Ok(()) => {}
                        Err(TrySendError::Full(f)) => {
                            pending_out.push_front(f);
                            break;
                        }
                        Err(TrySendError::Disconnected(_)) => {
                            finished_t.store(true, Ordering::Relaxed);
                            return;
                        }
                    }
                }

                if core.is_finished() && pending_out.is_empty() {
                    finished_t.store(true, Ordering::Relaxed);
                    break;
                }

                // Avoid spinning when decoder has no output yet.
                if !produced_any {
                    thread::sleep(Duration::from_millis(1));
                }
            }
        });

        Ok(Self {
            width,
            height,
            rx,
            stop,
            finished,
            join: Some(join),
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    pub fn try_recv_one(&self) -> Option<VideoFrame> {
        match self.rx.try_recv() {
            Ok(f) => Some(f),
            Err(_) => None,
        }
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for VideoStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}
