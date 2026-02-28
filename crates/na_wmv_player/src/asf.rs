//! ASF (Advanced Systems Format) container parser.

use std::io::{Read, Seek, SeekFrom};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::error::{DecoderError, Result};

// ─── Known ASF GUIDs ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guid(pub [u8; 16]);

impl Guid {
    pub fn read<R: Read>(r: &mut R) -> Result<Self> {
        let mut buf = [0u8; 16];
        r.read_exact(&mut buf)?;
        Ok(Guid(buf))
    }
}

impl std::fmt::Display for Guid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let b = &self.0;
        write!(
            f,
            "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            b[3], b[2], b[1], b[0], b[5], b[4], b[7], b[6], b[8], b[9], b[10], b[11], b[12],
            b[13], b[14], b[15]
        )
    }
}

// Well-known GUIDs (stored in little-endian GUID format)
pub const GUID_ASF_HEADER: Guid =
    Guid([0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11, 0xA6, 0xD9, 0x00, 0xAA, 0x00, 0x62, 0xCE, 0x6C]);
pub const GUID_ASF_DATA: Guid =
    Guid([0x36, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11, 0xA6, 0xD9, 0x00, 0xAA, 0x00, 0x62, 0xCE, 0x6C]);
pub const GUID_FILE_PROPERTIES: Guid =
    Guid([0xA1, 0xDC, 0xAB, 0x8C, 0x47, 0xA9, 0xCF, 0x11, 0x8E, 0xE4, 0x00, 0xC0, 0x0C, 0x20, 0x53, 0x65]);
pub const GUID_STREAM_PROPERTIES: Guid =
    Guid([0x91, 0x07, 0xDC, 0xB7, 0xB7, 0xA9, 0xCF, 0x11, 0x8E, 0xE6, 0x00, 0xC0, 0x0C, 0x20, 0x53, 0x65]);
pub const GUID_STREAM_TYPE_VIDEO: Guid =
    Guid([0xC0, 0xEF, 0x19, 0xBC, 0x4D, 0x5B, 0xCF, 0x11, 0xA8, 0xFD, 0x00, 0x80, 0x5F, 0x5C, 0x44, 0x2B]);
pub const GUID_STREAM_TYPE_AUDIO: Guid =
    Guid([0x40, 0x9E, 0x69, 0xF8, 0x4D, 0x5B, 0xCF, 0x11, 0xA8, 0xFD, 0x00, 0x80, 0x5F, 0x5C, 0x44, 0x2B]);

// ─── ASF Object Header ───────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ObjectHeader {
    pub guid: Guid,
    pub size: u64,
}

impl ObjectHeader {
    pub fn read<R: Read>(r: &mut R) -> Result<Self> {
        let guid = Guid::read(r)?;
        let size = r.read_u64::<LittleEndian>()?;
        if size < 24 {
            return Err(DecoderError::InvalidData("ASF object size < 24".into()));
        }
        Ok(Self { guid, size })
    }

    pub fn payload_size(&self) -> u64 {
        self.size - 24
    }
}

// ─── Stream Information ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct VideoStreamInfo {
    pub stream_number: u8,
    pub width: u32,
    pub height: u32,
    pub codec_four_cc: [u8; 4],
    pub extra_data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct AudioStreamInfo {
    pub stream_number: u8,
    pub format_tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub bit_rate: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    pub extra_data: Vec<u8>,
    /// ASF audio descrambling (interleaving) parameters (upstream: ds_span/ds_packet_size/ds_chunk_size).
    /// span==0 or 1 means descrambling disabled.
    pub ds_span: u8,
    pub ds_packet_size: u16,
    pub ds_chunk_size: u16,
}

// ─── Output Payload (Complete Media Object) ──────────────────────────────────

#[derive(Debug, Clone)]
pub struct AsfPayload {
    pub stream_number: u8,
    pub object_id: u32,
    pub obj_offset: u32,
    pub obj_size: u32,
    pub pts_ms: u32,
    pub duration_ms: u16,
    pub is_key_frame: bool,
    pub data: Vec<u8>,
}

#[derive(Debug, Default, Clone)]
struct AsfStreamState {
    pkt: Vec<u8>,
    frag_offset_sum: usize,
    pkt_clean: bool,
    seq: u32,
    pts_ms: u32,
    is_key: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct AsfAudioDescramble {
    span: u8,
    packet_size: u16,
    chunk_size: u16,
}

// ─── Top-level ASF File (stateful demuxer) ───────────────────────────────────

pub struct AsfFile {
    pub video_streams: Vec<VideoStreamInfo>,
    pub audio_streams: Vec<AudioStreamInfo>,

    pub data_offset: u64,
    pub packet_count: u64,

    pub packet_size: u32,     // upstream: s->packet_size = hdr.max_pktsize
    pub min_packet_size: u32, // upstream: hdr.min_pktsize
    pub preroll_ms: u32,      // upstream: hdr.preroll

    is_audio_stream: [bool; 128],
    audio_descramble: [AsfAudioDescramble; 128],

    // Per-stream reassembly state.
    streams: [AsfStreamState; 128],
}

impl AsfFile {
    /// Parse the ASF header and locate the Data section.
    pub fn open<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let hdr = ObjectHeader::read(reader)?;
        if hdr.guid != GUID_ASF_HEADER {
            return Err(DecoderError::InvalidData("Not an ASF file".into()));
        }

        let _num_headers = reader.read_u32::<LittleEndian>()?;
        let _reserved1 = reader.read_u8()?;
        let _reserved2 = reader.read_u8()?;

        let mut video_streams = Vec::new();
        let mut audio_streams = Vec::new();
        let mut is_audio_stream = [false; 128];
        let mut audio_descramble = std::array::from_fn(|_| AsfAudioDescramble::default());

        let mut packet_count = 0u64;
        let mut min_pktsize = 0u32;
        let mut max_pktsize = 0u32;
        let mut preroll_ms = 0u32;

        let header_end = hdr.size;
        let mut pos = 24u64 + 4 + 1 + 1;

        while pos < header_end {
            let obj = ObjectHeader::read(reader)?;
            let obj_end = pos + obj.size;

            if obj.guid == GUID_FILE_PROPERTIES {
                // ASFMainHeader: we follow upstream's `asf_read_file_properties`.
                reader.seek(SeekFrom::Current(16 + 8 + 8))?; // file_id + file_size + create_time
                packet_count = reader.read_u64::<LittleEndian>()?; // data_packets_count
                reader.seek(SeekFrom::Current(8 + 8))?; // play_time + send_time

                // preroll is a QWORD in ASF; upstream uses the low 32 bits.
                preroll_ms = reader.read_u32::<LittleEndian>()?;
                let _preroll_hi_ignored = reader.read_u32::<LittleEndian>()?;

                let _flags = reader.read_u32::<LittleEndian>()?;
                min_pktsize = reader.read_u32::<LittleEndian>()?;
                max_pktsize = reader.read_u32::<LittleEndian>()?;
                let _max_bitrate = reader.read_u32::<LittleEndian>()?;
            } else if obj.guid == GUID_STREAM_PROPERTIES {
                let stream_type = Guid::read(reader)?;
                let _error_correct = Guid::read(reader)?;
                let _time_offset = reader.read_u64::<LittleEndian>()?;
                let type_specific_len = reader.read_u32::<LittleEndian>()? as usize;
                let _err_correct_len = reader.read_u32::<LittleEndian>()?;
                let flags = reader.read_u16::<LittleEndian>()?;
                let stream_number = (flags & 0x7F) as u8;
                let _reserved = reader.read_u32::<LittleEndian>()?;

                if stream_type == GUID_STREAM_TYPE_VIDEO {
                    let _enc_width = reader.read_u32::<LittleEndian>()?;
                    let _enc_height = reader.read_u32::<LittleEndian>()?;
                    reader.read_u8()?;
                    let fmt_data_size = reader.read_u16::<LittleEndian>()? as usize;

                    let _bi_size = reader.read_u32::<LittleEndian>()?;
                    let width = reader.read_u32::<LittleEndian>()?;
                    let height_i = reader.read_i32::<LittleEndian>()?;
                    let height = height_i.unsigned_abs();
                    let _planes = reader.read_u16::<LittleEndian>()?;
                    let _bit_count = reader.read_u16::<LittleEndian>()?;
                    let mut four_cc = [0u8; 4];
                    reader.read_exact(&mut four_cc)?;
                    reader.seek(SeekFrom::Current(20))?;

                    let extra_len = if fmt_data_size > 40 { fmt_data_size - 40 } else { 0 };
                    let mut extra_data = vec![0u8; extra_len];
                    reader.read_exact(&mut extra_data)?;

                    video_streams.push(VideoStreamInfo {
                        stream_number,
                        width,
                        height,
                        codec_four_cc: four_cc,
                        extra_data,
                    });
                } else if stream_type == GUID_STREAM_TYPE_AUDIO {
                    is_audio_stream[stream_number as usize] = true;

                    let format_tag = reader.read_u16::<LittleEndian>()?;
                    let channels = reader.read_u16::<LittleEndian>()?;
                    let sample_rate = reader.read_u32::<LittleEndian>()?;
                    let bit_rate = reader.read_u32::<LittleEndian>()? * 8; // avg bytes/sec -> bps
                    let block_align = reader.read_u16::<LittleEndian>()?;
                    let bits_per_sample = reader.read_u16::<LittleEndian>()?;

                    let (cb_size, base_len) = if type_specific_len >= 18 {
                        (reader.read_u16::<LittleEndian>()? as usize, 18usize)
                    } else {
                        (0usize, 16usize)
                    };

                    let mut extra_data = vec![0u8; cb_size];
                    if cb_size != 0 {
                        reader.read_exact(&mut extra_data)?;
                    }

                    let consumed = base_len + cb_size;
                    let remain = type_specific_len.saturating_sub(consumed);
                    if remain != 0 {
                        reader.seek(SeekFrom::Current(remain as i64))?;
                    }

                    let mut ds_span: u8 = 0;
                    let mut ds_packet_size: u16 = 0;
                    let mut ds_chunk_size: u16 = 0;
                    let pos2 = reader.stream_position()?;
                    if (obj_end as i128) - (pos2 as i128) >= 8 {
                        ds_span = reader.read_u8()?;
                        ds_packet_size = reader.read_u16::<LittleEndian>()?;
                        ds_chunk_size = reader.read_u16::<LittleEndian>()?;
                        let _ds_data_size = reader.read_u16::<LittleEndian>()?;
                        let _ds_silence = reader.read_u8()?;

                        if ds_span > 1 {
                            if ds_chunk_size == 0
                                || (ds_packet_size / ds_chunk_size) <= 1
                                || (ds_packet_size % ds_chunk_size) != 0
                            {
                                ds_span = 0;
                            }
                        }
                    }

                    audio_descramble[stream_number as usize] = AsfAudioDescramble {
                        span: ds_span,
                        packet_size: ds_packet_size,
                        chunk_size: ds_chunk_size,
                    };

                    audio_streams.push(AudioStreamInfo {
                        stream_number,
                        format_tag,
                        channels,
                        sample_rate,
                        bit_rate,
                        block_align,
                        bits_per_sample,
                        extra_data,
                        ds_span,
                        ds_packet_size,
                        ds_chunk_size,
                    });
                } else {
                    reader.seek(SeekFrom::Current(type_specific_len as i64))?;
                }
            }

            pos = obj_end;
            reader.seek(SeekFrom::Start(obj_end))?;
        }

        let data_obj = ObjectHeader::read(reader)?;
        if data_obj.guid != GUID_ASF_DATA {
            return Err(DecoderError::InvalidData(
                "Expected ASF Data Object after header".into(),
            ));
        }

        // Data Object payload begins with a FileID GUID (16), total packets (8), reserved (2).
        reader.seek(SeekFrom::Current(16 + 8 + 2))?;
        let data_offset = reader.stream_position()?;

        if max_pktsize == 0 {
            return Err(DecoderError::InvalidData("ASF max packet size is 0".into()));
        }

        Ok(Self {
            video_streams,
            audio_streams,
            data_offset,
            packet_count,
            packet_size: max_pktsize,
            min_packet_size: min_pktsize,
            preroll_ms,
            is_audio_stream,
            audio_descramble,
            streams: std::array::from_fn(|_| AsfStreamState::default()),
        })
    }

    fn descramble_audio_if_needed(&self, stream_num: u8, data: Vec<u8>) -> Vec<u8> {
        let ds = self.audio_descramble[stream_num as usize];
        if ds.span <= 1 {
            return data;
        }
        let span = ds.span as usize;
        let packet_size = ds.packet_size as usize;
        let chunk_size = ds.chunk_size as usize;
        if chunk_size == 0 {
            return data;
        }
        if data.len() != packet_size.saturating_mul(span) {
            return data;
        }
        if packet_size % chunk_size != 0 {
            return data;
        }
        let chunks_per_packet = packet_size / chunk_size;
        if chunks_per_packet <= 1 {
            return data;
        }

        // Packet descrambling (upstream asfdec_f.c)
        let mut out = vec![0u8; data.len()];
        let mut offset: usize = 0;
        while offset < data.len() {
            let off = offset / chunk_size;
            let row = off / span;
            let col = off % span;
            let idx = row + col * chunks_per_packet;
            let src = idx * chunk_size;
            if src + chunk_size > data.len() || offset + chunk_size > out.len() {
                return data;
            }
            out[offset..offset + chunk_size].copy_from_slice(&data[src..src + chunk_size]);
            offset += chunk_size;
        }
        out
    }

    #[inline(always)]
    fn read_2bits_from_buf(buf: &[u8], i: &mut usize, code: u8, def: u32) -> Result<u32> {
        match code & 3 {
            0 => Ok(def),
            1 => {
                if *i + 1 > buf.len() {
                    return Err(DecoderError::InvalidData("ASF packet truncated".into()));
                }
                let v = buf[*i] as u32;
                *i += 1;
                Ok(v)
            }
            2 => {
                if *i + 2 > buf.len() {
                    return Err(DecoderError::InvalidData("ASF packet truncated".into()));
                }
                let v = u16::from_le_bytes([buf[*i], buf[*i + 1]]) as u32;
                *i += 2;
                Ok(v)
            }
            3 => {
                if *i + 4 > buf.len() {
                    return Err(DecoderError::InvalidData("ASF packet truncated".into()));
                }
                let v = u32::from_le_bytes([buf[*i], buf[*i + 1], buf[*i + 2], buf[*i + 3]]);
                *i += 4;
                Ok(v)
            }
            _ => unreachable!(),
        }
    }

    /// Read the next ASF packet and return any completed media objects.
    ///
    /// This is stateful and matches upstream's `asf_get_packet` + `asf_parse_packet` assembly.
    pub fn read_packet<R: Read + Seek>(&mut self, reader: &mut R) -> Result<Vec<AsfPayload>> {
        let pkt_size = self.packet_size as usize;
        if pkt_size == 0 {
            return Err(DecoderError::InvalidData("ASF packet size is 0".into()));
        }

        let mut buf = vec![0u8; pkt_size];
        match reader.read_exact(&mut buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Err(DecoderError::EndOfStream),
            Err(e) => return Err(DecoderError::Io(e)),
        }

        const FRAME_HEADER_SIZE: i32 = 6; // upstream's constant

        let mut out: Vec<AsfPayload> = Vec::new();

        // Packet-level state (mirrors ASFContext fields used by upstream).
        let mut i: usize = 0;

        // Error correction header handling (conservative, fixed-size packet mode).
        // Common case: 0x82 0x00 0x00.
        if buf.len() >= 3 && buf[0] == 0x82 && buf[1] == 0 && buf[2] == 0 {
            i = 3;
        } else if (buf[0] & 0x80) != 0 {
            let ec_len = (buf[0] & 0x0F) as usize;
            i = 1 + ec_len;
            if i > buf.len() {
                // Drop this packet.
                return Ok(out);
            }
        }

        if i + 2 > buf.len() {
            return Ok(out);
        }
        let packet_flags = buf[i];
        let packet_property = buf[i + 1];
        i += 2;

        // packet_length / sequence / padsize (upstream DO_2BITS)
        let packet_length = Self::read_2bits_from_buf(&buf, &mut i, packet_flags >> 5, self.packet_size)? as u32;
        let _seq_ignored = Self::read_2bits_from_buf(&buf, &mut i, packet_flags >> 1, 0)?;
        let mut padsize = Self::read_2bits_from_buf(&buf, &mut i, packet_flags >> 3, 0)? as u32;

        if packet_length == 0 || packet_length >= (1u32 << 29) {
            return Ok(out);
        }
        if padsize >= packet_length {
            return Ok(out);
        }

        if i + 6 > buf.len() {
            return Ok(out);
        }
        let packet_timestamp = u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
        i += 4;
        let _duration = u16::from_le_bytes([buf[i], buf[i + 1]]);
        i += 2;

        let (packet_segsizetype, mut packet_segments): (u8, i32) = if (packet_flags & 0x01) != 0 {
            if i >= buf.len() {
                return Ok(out);
            }
            let st = buf[i];
            i += 1;
            (st, (st & 0x3f) as i32)
        } else {
            (0x80u8, 1)
        };

        // Header length so far.
        let header_len = i as u32;
        if header_len > packet_length.saturating_sub(padsize) {
            return Ok(out);
        }

        // upstream: packet_size_left = packet_length - padsize - header_len
        let mut packet_size_left: i32 = (packet_length - padsize - header_len) as i32;

        // upstream: if packet_length < min_pktsize, extend padsize.
        if packet_length < self.min_packet_size {
            padsize = padsize.saturating_add(self.min_packet_size - packet_length);
        }
        let mut packet_padsize: i32 = padsize as i32;

        // Payload-level state.
        let mut packet_time_start: u32 = 0;
        let mut packet_time_delta: u8 = 0;
        let mut packet_multi_size: i32 = 0;

        // The current payload header fields.
        let mut cur_stream_num: u8 = 0;
        let mut packet_seq: u32 = 0;
        let mut packet_frag_offset: u32 = 0;
        let mut packet_replic_size: u32 = 0;
        let mut packet_key_frame: bool = false;
        let mut packet_frag_size: u32 = 0;
        let mut packet_frag_timestamp: u32 = 0;
        let mut packet_obj_size: u32 = 0;

        // Parse payloads within this packet.
        loop {
            if packet_size_left < FRAME_HEADER_SIZE || (packet_segments < 1 && packet_time_start == 0) {
                // End-of-packet; ignore remaining + padding.
                let _ = packet_padsize;
                break;
            }

            if packet_time_start == 0 {
                // asf_read_frame_header
                if i >= buf.len() {
                    break;
                }
                let num = buf[i];
                i += 1;
                packet_size_left -= 1;

                packet_segments -= 1;
                packet_key_frame = (num & 0x80) != 0;
                cur_stream_num = num & 0x7f;

                // packet_seq / frag_offset / replic_size are variable length depending on packet_property
                let mut before = i;
                packet_seq = Self::read_2bits_from_buf(&buf, &mut i, packet_property >> 4, 0)?;
                packet_size_left -= (i - before) as i32;

                before = i;
                packet_frag_offset = Self::read_2bits_from_buf(&buf, &mut i, packet_property >> 2, 0)?;
                packet_size_left -= (i - before) as i32;

                before = i;
                packet_replic_size = Self::read_2bits_from_buf(&buf, &mut i, packet_property, 0)?;
                packet_size_left -= (i - before) as i32;

                // rsize: how many bytes were consumed by this frame header (excluding the initial 'num' already counted)
                // We mirror upstream's checks by comparing against packet_size_left.
                if (packet_replic_size as i32) > packet_size_left {
                    // Drop remainder of this packet.
                    break;
                }

                packet_obj_size = 0;

                if packet_replic_size >= 8 {
                    if i + 8 > buf.len() {
                        break;
                    }
                    packet_obj_size = u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
                    i += 4;
                    packet_frag_timestamp = u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
                    i += 4;
                    packet_size_left -= 8;

                    let skip = (packet_replic_size - 8) as usize;
                    if i + skip > buf.len() {
                        break;
                    }
                    i += skip;
                    packet_size_left -= skip as i32;
                } else if packet_replic_size == 1 {
                    // multipacket - frag_offset is beginning timestamp
                    packet_time_start = packet_frag_offset;
                    packet_frag_offset = 0;
                    packet_frag_timestamp = packet_timestamp;

                    if i >= buf.len() {
                        break;
                    }
                    packet_time_delta = buf[i];
                    i += 1;
                    packet_size_left -= 1;
                } else if packet_replic_size != 0 {
                    // upstream treats this as invalid.
                    break;
                }

                // frag_size
                if (packet_flags & 0x01) != 0 {
                    let before = i;
                    packet_frag_size = Self::read_2bits_from_buf(&buf, &mut i, packet_segsizetype >> 6, 0)?;
                    let consumed = (i - before) as i32;
                    packet_size_left -= consumed;

                    if packet_frag_size == 0 {
                        break;
                    }

                    // upstream: allow the fragment to eat padding.
                    if packet_frag_size as i32 > packet_size_left {
                        if packet_frag_size as i32 > packet_size_left + packet_padsize {
                            break;
                        }
                        let diff = packet_frag_size as i32 - packet_size_left;
                        packet_size_left += diff;
                        packet_padsize -= diff;
                    }
                } else {
                    // Single payload: rest of packet (excluding this header) is the fragment.
                    packet_frag_size = packet_size_left as u32;
                }

                if packet_replic_size == 1 {
                    packet_multi_size = packet_frag_size as i32;
                    if packet_multi_size > packet_size_left {
                        break;
                    }
                }
            }

            // Multipacket: each sub-payload begins with a 1-byte size.
            if packet_replic_size == 1 {
                packet_frag_timestamp = packet_time_start;
                packet_time_start = packet_time_start.wrapping_add(packet_time_delta as u32);

                if i >= buf.len() {
                    break;
                }
                let sz = buf[i] as u32;
                i += 1;
                packet_size_left -= 1;
                packet_multi_size -= 1;

                packet_obj_size = sz;
                packet_frag_size = sz;
                packet_frag_offset = 0;

                if packet_multi_size < packet_obj_size as i32 {
                    // Drop remaining bytes in the multipacket.
                    let drop = packet_multi_size.max(0) as usize;
                    if i + drop > buf.len() {
                        break;
                    }
                    i += drop;
                    packet_size_left -= drop as i32;
                    packet_time_start = 0;
                    packet_multi_size = 0;
                    continue;
                }

                packet_multi_size -= packet_obj_size as i32;

                // Audio is treated as keyframe by upstream.
                packet_key_frame = true;
            }

            let frag_size = packet_frag_size as usize;
            if frag_size == 0 {
                break;
            }
            if packet_size_left < frag_size as i32 {
                break;
            }
            if i + frag_size > buf.len() {
                break;
            }

            // Copy fragment bytes.
            let data = &buf[i..i + frag_size];
            i += frag_size;
            packet_size_left -= frag_size as i32;

            // For non-multipacket payloads, force reading a new frame header next.
            if packet_replic_size != 1 {
                packet_time_start = 0;
            }

            let pts_ms = packet_frag_timestamp.saturating_sub(self.preroll_ms);

            if packet_obj_size == 0 {
                // Unknown object size: emit as-is.
                out.push(AsfPayload {
                    stream_number: cur_stream_num,
                    object_id: packet_seq,
                    obj_offset: 0,
                    obj_size: data.len() as u32,
                    pts_ms,
                    duration_ms: 0,
                    is_key_frame: packet_key_frame,
                    data: data.to_vec(),
                });
                continue;
            }

            // Per-stream reassembly (upstream's ASFStream::pkt/frag_offset logic).
            let st = &mut self.streams[cur_stream_num as usize];

            if st.frag_offset_sum == 0 && packet_frag_offset != 0 {
                // upstream: skip unexpected non-zero fragment offset when no in-flight object.
                continue;
            }

            let obj_size = packet_obj_size as usize;
            let frag_off = packet_frag_offset as usize;

            let need_new = st.pkt.len() != obj_size || st.frag_offset_sum + frag_size > st.pkt.len();
            if need_new {
                st.pkt.clear();
                st.pkt.resize(obj_size, 0);
                st.frag_offset_sum = 0;
                st.pkt_clean = false;
                st.seq = packet_seq;
                st.pts_ms = pts_ms;
                st.is_key = packet_key_frame || self.is_audio_stream[cur_stream_num as usize];
            }

            if frag_off >= st.pkt.len() || frag_size > st.pkt.len().saturating_sub(frag_off) {
                continue;
            }

            if frag_off != st.frag_offset_sum && !st.pkt_clean {
                // upstream: zero-fill remainder once if offsets jump.
                for b in &mut st.pkt[st.frag_offset_sum..] {
                    *b = 0;
                }
                st.pkt_clean = true;
            }

            st.pkt[frag_off..frag_off + frag_size].copy_from_slice(data);
            st.frag_offset_sum += frag_size;

            if st.frag_offset_sum == st.pkt.len() {
                // IMPORTANT: end the mutable borrow of self.streams[] before calling any &self method.
                // We must cache fields from `st` into locals and reset `st` state first.
                let seq = st.seq;
                let pts_ms_full = st.pts_ms;
                let is_key_full = st.is_key;

                let mut full = std::mem::take(&mut st.pkt);
                st.frag_offset_sum = 0;
                st.pkt_clean = false;

                // `st` is no longer used after this point, so the mutable borrow ends here.
                if self.is_audio_stream[cur_stream_num as usize] {
                    full = self.descramble_audio_if_needed(cur_stream_num, full);
                }

                out.push(AsfPayload {
                    stream_number: cur_stream_num,
                    object_id: seq,
                    obj_offset: 0,
                    obj_size: full.len() as u32,
                    pts_ms: pts_ms_full,
                    duration_ms: 0,
                    is_key_frame: is_key_full,
                    data: full,
                });
            }
        }

        Ok(out)
    }
}
