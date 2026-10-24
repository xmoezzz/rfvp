//! A WAV byte stream produced while it is read.
//!
//! A movie's soundtrack is decoded chunk by chunk, so handing the audio backend
//! one whole WAV buffer costs the size of the clip. This wraps a ring buffer
//! behind a `Read + Seek` source that presents a WAV header followed by the PCM
//! the decoder has produced so far: the backend pulls, the decoder pushes, and
//! memory stays at the ring's size no matter how long the movie is.

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Condvar, Mutex};

/// Bytes a reader may get ahead of the decoder before `push` blocks.
const DEFAULT_CAPACITY: usize = 1 << 20; // 1 MiB ≈ 2.7 s of 48 kHz stereo i16

/// The read side: a WAV header followed by the PCM pushed into it.
pub struct PcmWavStream {
    shared: Arc<Shared>,
}

struct Shared {
    state: Mutex<State>,
    /// Signalled when the decoder pushes or finishes.
    data_ready: Condvar,
    /// Signalled when a reader consumed bytes, so the decoder can push again.
    space_free: Condvar,
}

struct State {
    /// PCM bytes not yet consumed, in the order they were pushed.
    ring: VecDeque<u8>,
    /// The decoder will push nothing more.
    finished: bool,
    /// The reader gave up (movie stopped); the writer must not block forever.
    abandoned: bool,
    /// Logical read position inside the stream, header included.
    read_pos: u64,
    header: [u8; HEADER_LEN],
    capacity: usize,
}

const HEADER_LEN: usize = 44;

/// The write side, handed to the decode thread.
pub struct PcmWavWriter {
    shared: Arc<Shared>,
}

impl PcmWavStream {
    /// `sample_rate` and `channels` go into the WAV header.
    ///
    /// The declared data length is deliberately the largest a 32-bit WAV can
    /// express: the real length is not known when the stream starts, and readers
    /// stop at end of stream anyway.
    pub fn new(sample_rate: u32, channels: u16) -> (Self, PcmWavWriter) {
        let mut header = [0u8; HEADER_LEN];
        let byte_rate = sample_rate * channels as u32 * 2;
        let block_align = channels * 2;
        header[0..4].copy_from_slice(b"RIFF");
        header[4..8].copy_from_slice(&u32::MAX.to_le_bytes()); // size patched later
        header[8..12].copy_from_slice(b"WAVE");
        header[12..16].copy_from_slice(b"fmt ");
        header[16..20].copy_from_slice(&16u32.to_le_bytes());
        header[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
        header[22..24].copy_from_slice(&channels.to_le_bytes());
        header[24..28].copy_from_slice(&sample_rate.to_le_bytes());
        header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
        header[32..34].copy_from_slice(&block_align.to_le_bytes());
        header[34..36].copy_from_slice(&16u16.to_le_bytes()); // bits per sample
        header[36..40].copy_from_slice(b"data");
        header[40..44].copy_from_slice(&u32::MAX.to_le_bytes()); // size patched later

        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                ring: VecDeque::with_capacity(DEFAULT_CAPACITY),
                finished: false,
                abandoned: false,
                read_pos: 0,
                header,
                capacity: DEFAULT_CAPACITY,
            }),
            data_ready: Condvar::new(),
            space_free: Condvar::new(),
        });

        (
            Self {
                shared: Arc::clone(&shared),
            },
            PcmWavWriter { shared },
        )
    }

    /// Tell the writer that nobody will read this stream any more.
    pub fn abandon(&self) {
        let mut state = lock(&self.shared.state);
        state.abandoned = true;
        self.shared.space_free.notify_all();
        self.shared.data_ready.notify_all();
    }
}

impl Drop for PcmWavStream {
    /// Dropping the reader is how a stopped movie tells the decoder to stop: the
    /// backend drops the sound, which drops this, which releases a writer that
    /// would otherwise wait on a full ring forever.
    fn drop(&mut self) {
        let mut state = lock(&self.shared.state);
        state.abandoned = true;
        self.shared.space_free.notify_all();
        self.shared.data_ready.notify_all();
    }
}

impl PcmWavWriter {    /// Append interleaved little-endian i16 samples, blocking while the ring is
    /// full so the decoder cannot outrun the reader.
    ///
    /// Returns `false` once the stream was abandoned (the movie stopped), which
    /// is the decode thread's cue to stop.
    pub fn push(&self, samples: &[i16]) -> bool {
        let mut bytes = Vec::with_capacity(samples.len() * 2);
        for s in samples {
            bytes.extend_from_slice(&s.to_le_bytes());
        }

        let mut state = lock(&self.shared.state);
        let mut offset = 0;
        while offset < bytes.len() {
            if state.abandoned {
                return false;
            }
            if state.ring.len() >= state.capacity {
                state = wait(&self.shared.space_free, state);
                continue;
            }
            let room = state.capacity - state.ring.len();
            let take = room.min(bytes.len() - offset);
            state.ring.extend(&bytes[offset..offset + take]);
            offset += take;
            self.shared.data_ready.notify_all();
        }
        true
    }

    /// No more PCM is coming; readers see end of stream once the ring drains.
    pub fn finish(&self) {
        let mut state = lock(&self.shared.state);
        state.finished = true;
        self.shared.data_ready.notify_all();
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn wait<'a, T>(cv: &Condvar, guard: std::sync::MutexGuard<'a, T>) -> std::sync::MutexGuard<'a, T> {
    cv.wait(guard).unwrap_or_else(|e| e.into_inner())
}

impl Read for PcmWavStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut state = lock(&self.shared.state);
        let mut written = 0;

        // The header first, so the reader can identify the format.
        while written < buf.len() && state.read_pos < HEADER_LEN as u64 {
            let at = state.read_pos as usize;
            let take = (HEADER_LEN - at).min(buf.len() - written);
            buf[written..written + take].copy_from_slice(&state.header[at..at + take]);
            written += take;
            state.read_pos += take as u64;
        }

        // Then PCM, waiting for the decoder while it is still producing.
        while written < buf.len() {
            if let Some(byte) = state.ring.pop_front() {
                buf[written] = byte;
                written += 1;
                state.read_pos += 1;
                self.shared.space_free.notify_all();
                continue;
            }
            if state.finished || state.abandoned {
                break;
            }
            state = wait(&self.shared.data_ready, state);
        }

        Ok(written)
    }
}

impl Seek for PcmWavStream {
    /// Only the current position is seekable: the bytes behind it were produced
    /// once and are gone. Readers that want to restart re-open the stream.
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let state = lock(&self.shared.state);
        match pos {
            SeekFrom::Current(0) => Ok(state.read_pos),
            SeekFrom::Start(0) if state.read_pos == 0 => Ok(0),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "a live PCM stream cannot seek",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_all(stream: &mut PcmWavStream, out: &mut Vec<u8>) {
        let mut buf = [0u8; 4096];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(e) => panic!("read failed: {e}"),
            }
        }
    }

    #[test]
    fn header_describes_the_pushed_format() {
        let (mut stream, writer) = PcmWavStream::new(48000, 2);
        writer.push(&[0i16; 4]);
        writer.finish();

        let mut out = Vec::new();
        read_all(&mut stream, &mut out);

        assert_eq!(&out[0..4], b"RIFF");
        assert_eq!(&out[8..12], b"WAVE");
        assert_eq!(&out[12..16], b"fmt ");
        assert_eq!(u16::from_le_bytes([out[20], out[21]]), 1, "PCM");
        assert_eq!(u16::from_le_bytes([out[22], out[23]]), 2, "channels");
        assert_eq!(
            u32::from_le_bytes([out[24], out[25], out[26], out[27]]),
            48000,
            "sample rate"
        );
        assert_eq!(
            u32::from_le_bytes([out[28], out[29], out[30], out[31]]),
            48000 * 2 * 2,
            "byte rate"
        );
        assert_eq!(u16::from_le_bytes([out[34], out[35]]), 16, "bits");
        assert_eq!(&out[36..40], b"data");
    }

    #[test]
    fn pushed_samples_come_out_byte_for_byte() {
        let (mut stream, writer) = PcmWavStream::new(44100, 2);
        let pushed: Vec<i16> = (0..2000).map(|i| (i as i16).wrapping_mul(37)).collect();
        for chunk in pushed.chunks(97) {
            assert!(writer.push(chunk));
        }
        writer.finish();

        let mut out = Vec::new();
        read_all(&mut stream, &mut out);

        assert_eq!(out.len(), HEADER_LEN + pushed.len() * 2);
        let pcm = &out[HEADER_LEN..];
        for (i, s) in pushed.iter().enumerate() {
            let got = i16::from_le_bytes([pcm[i * 2], pcm[i * 2 + 1]]);
            assert_eq!(got, *s, "sample {i}");
        }
    }

    #[test]
    fn read_waits_for_the_decoder_instead_of_returning_eof_early() {
        let (mut stream, writer) = PcmWavStream::new(44100, 2);
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            writer.push(&[7i16; 8]);
            writer.finish();
        });

        let mut out = Vec::new();
        read_all(&mut stream, &mut out);
        handle.join().unwrap();

        assert_eq!(out.len(), HEADER_LEN + 16, "the read waited for the push");
        assert_eq!(i16::from_le_bytes([out[HEADER_LEN], out[HEADER_LEN + 1]]), 7);
    }

    #[test]
    fn abandon_unblocks_a_writer_that_nobody_reads() {
        let (stream, writer) = PcmWavStream::new(44100, 2);
        stream.abandon();
        // More than the ring can hold, with no reader at all: without the
        // abandon this would block forever.
        let big = vec![0i16; DEFAULT_CAPACITY];
        assert!(!writer.push(&big), "push must report the abandonment");
    }

    #[test]
    fn a_live_stream_refuses_to_seek() {
        let (mut stream, writer) = PcmWavStream::new(44100, 2);
        writer.push(&[1i16; 4]);
        writer.finish();
        let mut buf = [0u8; 8];
        stream.read(&mut buf).unwrap();

        assert!(stream.seek(SeekFrom::Start(0)).is_err(), "no rewind");
        assert_eq!(stream.seek(SeekFrom::Current(0)).unwrap(), 8);
    }
}
