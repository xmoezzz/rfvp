//! Minimal MPEG demux helpers.
//!
//! This module is intentionally small: it only extracts *video elementary stream*
//! payload bytes (and optional PTS) from common MPEG containers.
//!
//! Supported:
//! - Raw ES (start-code byte stream)
//! - MPEG-TS (188-byte packets; video PID auto-sniffed from PES stream_id 0xE0..0xEF)
//! - MPEG-PS (pack/system headers + PES; extracts video PES payload)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamType {
    MpegVideo,
    MpegAudio,
    Unknown,
}

impl Default for StreamType {
    #[inline]
    fn default() -> Self {
        StreamType::Unknown
    }
}

#[derive(Clone, Debug)]
pub struct Packet {
    pub stream_type: StreamType,
    pub pts_90k: Option<i64>,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContainerKind {
    Auto,
    Es,
    MpegTs,
    MpegPs,
}

impl Default for ContainerKind {
    #[inline]
    fn default() -> Self {
        ContainerKind::Auto
    }
}

#[derive(Debug, Default)]
pub struct Demuxer {
    kind: ContainerKind,
    stream_type: StreamType,

    buf: Vec<u8>,

    // TS state
    ts_video_pid: Option<u16>,
    ts_audio_pid: Option<u16>,
}

impl Demuxer {
    /// Create an auto-detecting demuxer.
    pub fn new_auto() -> Self {
        Self { kind: ContainerKind::Auto, stream_type: StreamType::MpegVideo, buf: Vec::new(), ts_video_pid: None, ts_audio_pid: None }
    }

    /// Create a demuxer with explicit container kind.
    pub fn new(kind: StreamType) -> Self {
        Self { kind: ContainerKind::Es, stream_type: kind, buf: Vec::new(), ts_video_pid: None, ts_audio_pid: None }
    }

    /// Feed bytes and return extracted video ES chunks.
    pub fn push(&mut self, data: &[u8], pts_90k: Option<i64>) -> Vec<Packet> {
        let mut out = Vec::new();
        self.push_into(data, pts_90k, &mut out);
        out
    }

    /// Feed bytes and append extracted video ES chunks into `out`.
    ///
    /// This is the preferred API for memory-sensitive callers because it allows
    /// reusing `out` capacity across calls.
    pub fn push_into(&mut self, data: &[u8], pts_90k: Option<i64>, out: &mut Vec<Packet>) {
        self.buf.extend_from_slice(data);

        if self.kind == ContainerKind::Auto {
            self.kind = detect_kind(&self.buf);
        }

        match self.kind {
            ContainerKind::Es => {
                if !self.buf.is_empty() {
                    out.push(Packet { stream_type: self.stream_type, pts_90k, data: std::mem::take(&mut self.buf) });
                }
            }
            ContainerKind::MpegTs => self.push_ts_into(out),
            ContainerKind::MpegPs => self.push_ps_into(out),
            ContainerKind::Auto => {}
        }
    }

    fn push_ts_into(&mut self, out: &mut Vec<Packet>) {
        const TS_SIZE: usize = 188;

        // Try to resync to TS packet boundary.
        while self.buf.len() >= TS_SIZE {
            if self.buf[0] != 0x47 {
                if let Some(pos) = self.buf.iter().position(|&b| b == 0x47) {
                    self.buf.drain(0..pos);
                } else {
                    self.buf.clear();
                    break;
                }
                if self.buf.len() < TS_SIZE {
                    break;
                }
            }

            // Parse directly from the buffered TS packet without allocating.
            // Important: always consume exactly one TS packet per iteration.
            if self.buf[0] != 0x47 {
                self.buf.drain(0..1);
                continue;
            }

            let pkt = &self.buf[..TS_SIZE];

            let pusi = (pkt[1] & 0x40) != 0;
            let pid: u16 = (((pkt[1] & 0x1F) as u16) << 8) | (pkt[2] as u16);
            let afc = (pkt[3] >> 4) & 0x3;

            let mut idx = 4usize;
            if afc == 2 || afc == 3 {
                if idx >= TS_SIZE {
                    self.buf.drain(0..TS_SIZE);
                    continue;
                }
                let afl = pkt[idx] as usize;
                idx += 1 + afl;
                if idx > TS_SIZE {
                    self.buf.drain(0..TS_SIZE);
                    continue;
                }
            }

            if afc == 0 || afc == 2 {
                self.buf.drain(0..TS_SIZE);
                continue; // no payload
            }
            if idx >= TS_SIZE {
                self.buf.drain(0..TS_SIZE);
                continue;
            }
            let payload = &pkt[idx..TS_SIZE];
            if payload.is_empty() {
                self.buf.drain(0..TS_SIZE);
                continue;
            }

            // Auto-sniff video/audio PID from PES headers.
            if pusi && (self.ts_video_pid.is_none() || self.ts_audio_pid.is_none()) {
                if payload.len() >= 4 && payload[0] == 0 && payload[1] == 0 && payload[2] == 1 {
                    let sid = payload[3];
                    if self.ts_video_pid.is_none() && (0xE0..=0xEF).contains(&sid) {
                        self.ts_video_pid = Some(pid);
                    }
                    if self.ts_audio_pid.is_none() && (0xC0..=0xDF).contains(&sid) {
                        self.ts_audio_pid = Some(pid);
                    }
                }
            }

            let mut st: Option<StreamType> = None;
            if let Some(vpid) = self.ts_video_pid {
                if pid == vpid {
                    st = Some(StreamType::MpegVideo);
                }
            }
            if let Some(apid) = self.ts_audio_pid {
                if pid == apid {
                    st = Some(StreamType::MpegAudio);
                }
            }
            let Some(stream_type) = st else {
                self.buf.drain(0..TS_SIZE);
                continue;
            };

            if pusi {
                // PES start.
                if let Some((pts, off)) = parse_pes_header(payload) {
                    if off <= payload.len() {
                        let es = &payload[off..];
                        if !es.is_empty() {
                            out.push(Packet { stream_type, pts_90k: pts, data: es.to_vec() });
                        }
                    }
                } else {
                    // No PES header; forward payload.
                    out.push(Packet { stream_type, pts_90k: None, data: payload.to_vec() });
                }
            } else {
                // PES continuation: payload is pure ES bytes.
                out.push(Packet { stream_type, pts_90k: None, data: payload.to_vec() });
            }

            // Drop processed TS packet bytes.
            self.buf.drain(0..TS_SIZE);
        }
    }

    fn push_ps_into(&mut self, out: &mut Vec<Packet>) {

        // Scan for PES start codes; keep the last partial chunk.
        let mut pos = 0usize;
        while let Some((sc_pos, sid)) = find_start_code(&self.buf, pos) {
            if sc_pos + 4 > self.buf.len() {
                break;
            }
            let stream_type = if (0xE0..=0xEF).contains(&sid) {
                StreamType::MpegVideo
            } else if (0xC0..=0xDF).contains(&sid) {
                StreamType::MpegAudio
            } else {
                pos = sc_pos + 4;
                continue;
            };
            if sc_pos + 6 > self.buf.len() {
                break;
            }

            let pes_len = u16::from_be_bytes([self.buf[sc_pos + 4], self.buf[sc_pos + 5]]) as usize;
            let pes_end = if pes_len != 0 {
                sc_pos + 6 + pes_len
            } else {
                // Unbounded PES: end at the next *system-layer* start code.
                // Important: video ES itself contains 00 00 01 xx start codes
                // (e.g., 0x00, 0xB3, 0xB5, 0x01..0xAF). We must not cut on those.
                let mut search = sc_pos + 6;
                let mut end_opt: Option<usize> = None;
                while let Some((next_sc, next_id)) = find_start_code(&self.buf, search) {
                    // System / PES start codes are >= 0xB9 in program streams.
                    if next_id >= 0xB9 {
                        end_opt = Some(next_sc);
                        break;
                    }
                    search = next_sc + 4;
                }
                let Some(end_pos) = end_opt else {
                    break;
                };
                end_pos
            };
            if pes_end > self.buf.len() {
                break;
            }

            let pes = &self.buf[sc_pos..pes_end];
            if let Some((pts, off)) = parse_pes_header(pes) {
                if off < pes.len() {
                    out.push(Packet { stream_type, pts_90k: pts, data: pes[off..].to_vec() });
                }
            } else {
                // Could not parse; forward raw PES bytes after start code.
                out.push(Packet { stream_type, pts_90k: None, data: pes[4..].to_vec() });
            }

            pos = pes_end;
        }

        // Keep tail for next push.
        if pos > 0 {
            self.buf.drain(0..pos);
        }
    }
}

fn detect_kind(buf: &[u8]) -> ContainerKind {
    // TS: sync byte 0x47 with 188-byte periodicity.
    if buf.len() >= 188 * 3 {
        if buf[0] == 0x47 && buf[188] == 0x47 && buf[376] == 0x47 {
            return ContainerKind::MpegTs;
        }
    }
    // PS: pack start code 00 00 01 BA.
    if buf.windows(4).take(4096).any(|w| w == [0x00, 0x00, 0x01, 0xBA]) {
        return ContainerKind::MpegPs;
    }
    ContainerKind::Es
}

fn find_start_code(buf: &[u8], from: usize) -> Option<(usize, u8)> {
    if buf.len() < 4 {
        return None;
    }
    let mut i = from;
    while i + 3 < buf.len() {
        if buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 1 {
            return Some((i, buf[i + 3]));
        }
        i += 1;
    }
    None
}

/// Parse PES header and return (PTS, payload_offset).
/// The returned offset is relative to the provided `buf`.
fn parse_pes_header(buf: &[u8]) -> Option<(Option<i64>, usize)> {
    if buf.len() < 9 {
        return None;
    }
    if !(buf[0] == 0 && buf[1] == 0 && buf[2] == 1) {
        return None;
    }
    let _sid = buf[3];
    // buf[4..6] is PES_packet_length.

    // Prefer MPEG-2 PES header syntax: '10' in buf[6] bits 7..6.
    if (buf[6] & 0xC0) == 0x80 {
        let flags = buf[7];
        let hdr_len = buf[8] as usize;
        let hdr_start = 9usize;
        let payload_off = hdr_start + hdr_len;
        if payload_off > buf.len() {
            return None;
        }
        let pts_dts = (flags >> 6) & 0x3;
        let mut pts: Option<i64> = None;
        if (pts_dts == 2 || pts_dts == 3) && hdr_len >= 5 && hdr_start + 5 <= buf.len() {
            pts = Some(parse_pts_90k(&buf[hdr_start..hdr_start + 5]));
        }
        return Some((pts, payload_off));
    }

    // MPEG-1 PES: skip stuffing and parse optional PTS.
    // Reference: ISO/IEC 11172-1.
    let mut idx = 6usize;
    while idx < buf.len() && buf[idx] == 0xFF {
        idx += 1;
    }
    if idx + 1 < buf.len() && (buf[idx] & 0xC0) == 0x40 {
        idx += 2; // STD_buffer_scale/size
    }
    if idx >= buf.len() {
        return None;
    }
    let mut pts: Option<i64> = None;
    if (buf[idx] & 0xF0) == 0x20 {
        // PTS only
        if idx + 5 <= buf.len() {
            pts = Some(parse_pts_90k(&buf[idx..idx + 5]));
            idx += 5;
        }
    } else if (buf[idx] & 0xF0) == 0x30 {
        // PTS + DTS, ignore DTS
        if idx + 10 <= buf.len() {
            pts = Some(parse_pts_90k(&buf[idx..idx + 5]));
            idx += 10;
        }
    } else if buf[idx] == 0x0F {
        idx += 1; // no pts
    }
    Some((pts, idx))
}

fn parse_pts_90k(p: &[u8]) -> i64 {
    // p must be 5 bytes.
    if p.len() < 5 {
        return 0;
    }
    let pts = (((p[0] & 0x0E) as i64) << 29)
        | ((p[1] as i64) << 22)
        | (((p[2] & 0xFE) as i64) << 14)
        | ((p[3] as i64) << 7)
        | (((p[4] & 0xFE) as i64) >> 1);
    pts
}
