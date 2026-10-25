//! Play a movie's soundtrack through the real audio backend.
//!
//! This exercises the streaming path end to end without the game loop: the WMA
//! track is decoded packet by packet into a `PcmWavStream`, `KiraBackend` pulls
//! from it as it plays, and the samples reach the speakers.
//!
//! `cargo run --release -p rfvp --example play_movie_audio -- <movie.wmv> [seconds]`

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rfvp::rfvp_audio::kira::KiraBackend;
use rfvp::rfvp_audio::pcm_stream::PcmWavStream;
use rfvp::host_api::AudioParams;
use rfvp::rfvp_audio::{AudioManager, AudioStreamSource, Tween};
use rfvp::subsystem::resources::vfs::Vfs;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("usage: play_movie_audio <movie.wmv> [secs]");
    let secs: f32 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(6.0);

    let file = std::fs::File::open(&path)?;
    let mut dec = wmv_decoder::AsfWmaDecoder::open(std::io::BufReader::new(file))?;
    let sample_rate = dec.sample_rate();
    let channels = dec.channels();
    println!("track: {channels} ch {sample_rate} Hz");

    let manager = Arc::new(AudioManager::with_backend(Box::new(KiraBackend::new())));
    if !manager.pcm_stream_supported() {
        anyhow::bail!("the backend does not accept a live PCM stream");
    }

    let (stream, writer) = PcmWavStream::new(sample_rate, channels);
    manager.load_pcm_stream(
        rfvp::host_api::AudioStreamId::MOVIE,
        AudioStreamSource::new(Box::new(stream)),
    )?;

    let stop = Arc::new(AtomicBool::new(false));
    let decode_stop = Arc::clone(&stop);
    let decoder = std::thread::spawn(move || {
        let mut pushed = 0usize;
        while !decode_stop.load(Ordering::Relaxed) {
            match dec.next_frame() {
                Ok(Some(frame)) => {
                    let mut chunk: Vec<i16> = Vec::with_capacity(frame.frame.samples.len());
                    for &s in frame.frame.samples.iter() {
                        let s = s.clamp(-1.0, 1.0);
                        chunk.push(((s * 32767.0).round() as i32).clamp(-32768, 32767) as i16);
                    }
                    pushed += chunk.len();
                    if !writer.push(&chunk) {
                        break;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    println!("decode stopped early: {e}");
                    break;
                }
            }
        }
        writer.finish();
        pushed
    });

    manager.backend().play(
        rfvp::host_api::AudioStreamId::MOVIE,
        AudioParams {
            volume: 1.0,
            pan: 0.0,
            repeat: false,
        },
        Tween::default(),
        &Vfs::default(),
    )?;

    let started = Instant::now();
    while started.elapsed() < Duration::from_secs_f32(secs) {
        std::thread::sleep(Duration::from_millis(100));
    }
    stop.store(true, Ordering::Relaxed);
    manager.stop_stream(rfvp::host_api::AudioStreamId::MOVIE);
    let pushed = decoder.join().unwrap_or(0);
    println!("played {secs}s, decoder pushed {pushed} samples ({:.1} KiB)", pushed as f64 * 2.0 / 1024.0);
    Ok(())
}
