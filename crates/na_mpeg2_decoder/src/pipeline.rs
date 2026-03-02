use std::sync::Arc;

use crate::demux::{Demuxer, StreamType};
use crate::video::{Decoder, Frame};

use crate::video::Result;

/// High-level convenience wrapper: demux container bytes and decode MPEG-1/2 video frames.
///
/// This type is designed for low-overhead integration:
/// - You can reuse a single pipeline instance across the whole stream.
/// - Use `push_with()`/`flush_with()` to avoid collecting frames into intermediate vectors.
#[derive(Debug, Default)]
pub struct MpegVideoPipeline {
    demux: Demuxer,
    dec: Decoder,
    pkts: Vec<crate::demux::Packet>,
}

impl MpegVideoPipeline {
    #[inline]
    pub fn new() -> Self {
        Self { demux: Demuxer::new_auto(), dec: Decoder::new(), pkts: Vec::new() }
    }

    #[inline]
    pub fn decoder_mut(&mut self) -> &mut Decoder {
        &mut self.dec
    }

    #[inline]
    pub fn demuxer_mut(&mut self) -> &mut Demuxer {
        &mut self.demux
    }

    /// Feed container bytes and invoke `on_frame` for each decoded frame.
    ///
    /// `pts_90k` is optional chunk-level PTS in 90 kHz timebase. When demuxing TS/PS,
    /// packet-level PTS from PES headers takes precedence.
    pub fn push_with<F>(&mut self, data: &[u8], pts_90k: Option<i64>, mut on_frame: F) -> Result<()>
    where
        F: FnMut(Arc<Frame>),
    {
        self.pkts.clear();
        self.demux.push_into(data, pts_90k, &mut self.pkts);
        for pkt in self.pkts.drain(..) {
            if pkt.stream_type != StreamType::MpegVideo {
                continue;
            }
            for f in self.dec.decode_shared(&pkt.data, pkt.pts_90k)? {
                on_frame(f);
            }
        }
        Ok(())
    }

    /// Flush delayed frames and invoke `on_frame` for each of them.
    pub fn flush_with<F>(&mut self, mut on_frame: F) -> Result<()>
    where
        F: FnMut(Arc<Frame>),
    {
        for f in self.dec.flush_shared()? {
            on_frame(f);
        }
        Ok(())
    }
}
