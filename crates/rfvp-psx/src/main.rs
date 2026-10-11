//! rfvp-psx-convert: a game, converted for the PlayStation port (platform/ps1).
//!
//! ```text
//! rfvp-psx-convert --input <game> --output <dir>
//! ```
//!
//! Everything is turned into what the console uses as it is, in parallel:
//!
//! - pictures: CLUT images at the console's resolution (image.rs);
//! - sound effects and voices: SPU-ADPCM, played from sound RAM (audio.rs);
//! - music: XA, interleaved into one file the CD drive streams (xa.rs);
//! - movies: STR (video.rs);
//! - text: a fixed-size bitmap font of the characters the script uses (font.rs).
//!
//! The output folder holds the disc's files with ISO 9660 names and `iso.xml` for mkpsxiso;
//! `FILES.MAP` tells the port which disc file each of the game's names is.

mod adpcm;
mod audio;
mod font;
mod game;
mod image;
mod mdec;
mod quant;
mod video;
mod xa;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use clap::Parser;
use rayon::prelude::*;

const FONT_TTF: &[u8] = include_bytes!("../../rfvp-rebuilder/assets/fonts/BIZUDGothic-Regular.ttf");
const FONT_LICENSE: &str = include_str!("../../rfvp-rebuilder/assets/fonts/OFL.txt");

/// The console's picture.
const SCREEN_W: u32 = 320;
const SCREEN_H: u32 = 240;
/// Music without a loop found is cut to this many seconds.
const MUSIC_CAP_SECONDS: usize = 300;
/// Movies: whole macroblocks.
const MOVIE_W: usize = 320;
const MOVIE_H: usize = 176;

#[derive(Parser, Debug)]
#[command(about = "Convert a game to the PlayStation's own formats")]
struct Cli {
    /// The game folder (the .hcb and the .bin packs).
    #[arg(long)]
    input: PathBuf,
    /// Where the disc's files go.
    #[arg(long)]
    output: PathBuf,
    /// Text size in pixels.
    #[arg(long, default_value_t = 12)]
    font_size: usize,
    /// Music sample rate: 18900 or 37800 (XA's two).
    #[arg(long, default_value_t = 18900)]
    music_rate: u32,
    /// Voice sample rate.
    #[arg(long, default_value_t = 18900)]
    voice_rate: u32,
    /// Sound effect sample rate.
    #[arg(long, default_value_t = 22050)]
    se_rate: u32,
    /// Leave out the movies (faster test builds).
    #[arg(long)]
    no_movies: bool,
    /// Leave out the music.
    #[arg(long)]
    no_music: bool,
    /// Packs of alternate music that play the tracks of another pack of the same names
    /// instead (`bgm2=bgm`): one disc does not hold both.
    #[arg(long, default_value = "bgm2=bgm")]
    music_alias: Vec<String>,
    /// Packs whose pictures are all made 4-bit (16 colors, dithered): prims and UI parts.
    #[arg(long, default_value = "graph")]
    clut4: Vec<String>,
    /// Worker threads (0: one per core).
    #[arg(long, default_value_t = 0)]
    threads: usize,
}

/// What a pack holds, from its name.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Pictures,
    Music,
    Voice,
    Effects,
    Other,
}

fn kind_of(stem: &str) -> Kind {
    if stem.starts_with("graph") {
        Kind::Pictures
    } else if stem.starts_with("bgm") {
        Kind::Music
    } else if stem.starts_with("voice") {
        Kind::Voice
    } else if stem.starts_with("se") {
        Kind::Effects
    } else {
        Kind::Other
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    ensure!(matches!(cli.music_rate, 18900 | 37800), "--music-rate is 18900 or 37800");
    if cli.threads > 0 {
        rayon::ThreadPoolBuilder::new().num_threads(cli.threads).build_global()?;
    }
    std::fs::create_dir_all(&cli.output)?;
    let started = Instant::now();

    let script = game::find_script(&cli.input)?;
    // Scale the design resolution into the screen, keeping its shape.
    let (num, den) = if SCREEN_W * script.design_height <= SCREEN_H * script.design_width {
        (SCREEN_W, script.design_width)
    } else {
        (SCREEN_H, script.design_height)
    };
    let viewport_h = (script.design_height * num / den) as usize;
    println!(
        "{}: {}x{} -> {}x{} (scale {num}/{den})",
        script.path.display(),
        script.design_width,
        script.design_height,
        script.design_width * num / den,
        viewport_h
    );

    let mut names = DiscNames::default();
    let script_name = script.path.file_name().unwrap().to_string_lossy().to_string();
    let disc = names.add(&script_name);
    game::write_file(&cli.output.join(&disc), &script.bytes)?;
    let scale = num as f64 / den as f64;
    let disc = names.add("rfvp.toml");
    game::write_file(&cli.output.join(&disc), format!("scale = {scale}\n").as_bytes())?;

    // The font.
    let strings = game::script_strings(&script.bytes)?;
    let font = font::build(FONT_TTF, cli.font_size, &strings)?;
    println!("font: {} px, {} bytes ({} strings)", cli.font_size, font.len(), strings.len());
    let disc = names.add("font.psxf");
    game::write_file(&cli.output.join(&disc), &font)?;
    game::write_file(&cli.output.join("FONT_LIC.TXT"), FONT_LICENSE.as_bytes())?;

    // The packs.
    let aliases: Vec<(String, String)> = cli
        .music_alias
        .iter()
        .filter_map(|a| a.split_once('=').map(|(x, y)| (x.to_ascii_lowercase(), y.to_ascii_lowercase())))
        .collect();
    let mut music_tracks: Vec<(String, String, Vec<[u8; adpcm::XA_DATA]>, u32, u32)> = Vec::new();
    let mut aliased: Vec<(String, String, String)> = Vec::new();
    let mut loops_found = 0usize;
    let mut packs_out: Vec<(String, Vec<(String, Vec<u8>)>)> = Vec::new();
    let mut stats: BTreeMap<&str, (usize, usize, f64)> = BTreeMap::new();
    for pack in game::packs(&cli.input)? {
        let kind = kind_of(&pack.stem);
        if kind == Kind::Music {
            if let Some((_, target)) = aliases.iter().find(|(from, _)| from == &pack.stem) {
                for entry in &pack.entries {
                    aliased.push((pack.stem.clone(), entry.name.clone(), target.clone()));
                }
                println!("{}: plays {}'s tracks", pack.stem, target);
                packs_out.push((pack.stem, Vec::new()));
                continue;
            }
        }
        let done = AtomicUsize::new(0);
        let total = pack.entries.len();
        let t = Instant::now();
        let results: Vec<Result<(String, Converted)>> = pack
            .entries
            .par_iter()
            .map(|entry| {
                let bytes = entry.read()?;
                let force4 = cli.clut4.iter().any(|p| p.eq_ignore_ascii_case(&pack.stem));
                let converted = convert_entry(kind, &entry.name, bytes, &cli, num, den, force4)
                    .with_context(|| format!("{}/{}", pack.stem, entry.name))?;
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                if n % 200 == 0 {
                    eprintln!("  {}: {n}/{total}", pack.stem);
                }
                Ok((entry.name.clone(), converted))
            })
            .collect();
        let mut entries = Vec::with_capacity(total);
        let label = match kind {
            Kind::Pictures => "pictures",
            Kind::Music => "music",
            Kind::Voice => "voices",
            Kind::Effects => "sound effects",
            Kind::Other => "other",
        };
        let stat = stats.entry(label).or_default();
        for result in results {
            let (name, converted) = result?;
            match converted {
                Converted::Bytes(b, seconds) => {
                    stat.0 += 1;
                    stat.1 += b.len();
                    stat.2 += seconds;
                    entries.push((name, b));
                }
                Converted::Music(sectors, frames, seconds, loop_sector) => {
                    stat.0 += 1;
                    stat.1 += sectors.len() * xa::RAW_SECTOR;
                    stat.2 += seconds;
                    loops_found += (loop_sector != u32::MAX) as usize;
                    let loop_sector = if loop_sector == u32::MAX { 0 } else { loop_sector };
                    music_tracks.push((pack.stem.clone(), name, sectors, frames, loop_sector));
                }
                Converted::Skipped => {}
            }
        }
        println!("{}: {} entries in {:.1}s", pack.stem, total, t.elapsed().as_secs_f64());
        packs_out.push((pack.stem, entries));
    }

    // Music: one interleaved XA file; the packs get the stubs that point into it.
    if !music_tracks.is_empty() {
        let tracks: Vec<(Vec<[u8; adpcm::XA_DATA]>, u32, u32)> =
            music_tracks.iter().map(|t| (t.2.clone(), t.3, t.4)).collect();
        let (file, refs) = xa::build(1, &tracks, true, cli.music_rate);
        let disc = names.add("music.xa");
        game::write_file(&cli.output.join(&disc), &file)?;
        println!(
            "music: {} tracks ({loops_found} cut at their loop), {} MiB of XA",
            tracks.len(),
            file.len() >> 20
        );
        for ((stem, name, ..), r) in music_tracks.iter().zip(&refs) {
            let pack = packs_out.iter_mut().find(|p| &p.0 == stem).unwrap();
            pack.1.push((name.clone(), audio::xa_stub(r)));
        }
        for (stem, name, target) in &aliased {
            let stub = music_tracks
                .iter()
                .zip(&refs)
                .find(|((s, n, ..), _)| s == target && n == name)
                .map(|(_, r)| audio::xa_stub(r));
            match stub {
                Some(stub) => packs_out.iter_mut().find(|p| &p.0 == stem).unwrap().1.push((name.clone(), stub)),
                None => eprintln!("warning: {stem}/{name} has no {target}/{name} to play"),
            }
        }
    }
    for (stem, mut entries) in packs_out {
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let disc = names.add(&format!("{stem}.bin"));
        game::write_pack(&cli.output.join(&disc), &entries)?;
    }

    // Movies.
    if !cli.no_movies {
        for file in game::loose_files(&cli.input)? {
            let game::Source::Loose(path) = &file.source else { continue };
            if !is_movie(path) {
                continue;
            }
            let t = Instant::now();
            let stream = video::convert(path, MOVIE_W, MOVIE_H, viewport_h)
                .with_context(|| format!("convert {}", path.display()))?;
            let disc = names.add_str(&file.name);
            game::write_file(&cli.output.join(&disc), &stream)?;
            println!(
                "{}: {} MiB of STR in {:.1}s",
                file.name,
                stream.len() >> 20,
                t.elapsed().as_secs_f64()
            );
        }
    }

    names.write(&cli.output.join("FILES.MAP"))?;
    write_iso_xml(&cli.output, &names)?;
    for (label, (count, bytes, seconds)) in &stats {
        if *seconds > 0.0 {
            println!("{label}: {count}, {} MiB, {:.0} min", bytes >> 20, seconds / 60.0);
        } else {
            println!("{label}: {count}, {} MiB", bytes >> 20);
        }
    }
    let total: u64 = walkdir::WalkDir::new(&cli.output)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum();
    println!("disc: {} MiB of files, done in {:.0}s", total >> 20, started.elapsed().as_secs_f64());
    Ok(())
}

enum Converted {
    /// The new pack entry, and the sound's length in seconds (0 for pictures).
    Bytes(Vec<u8>, f64),
    /// XA sectors of a music track, its frames, seconds and loop sector (`u32::MAX`: none
    /// found).
    Music(Vec<[u8; adpcm::XA_DATA]>, u32, f64, u32),
    Skipped,
}

fn convert_entry(kind: Kind, name: &str, bytes: Vec<u8>, cli: &Cli, num: u32, den: u32, force4: bool) -> Result<Converted> {
    if image::is_nvsg(&bytes) {
        return Ok(Converted::Bytes(image::convert(&bytes, num, den, force4)?, 0.0));
    }
    let pcm = if audio::is_ogg(&bytes) {
        audio::decode_ogg(&bytes).with_context(|| format!("decode {name}"))?
    } else if audio::is_wav(&bytes) {
        audio::decode_wav(&bytes).with_context(|| format!("decode {name}"))?
    } else {
        return Ok(Converted::Bytes(bytes, 0.0));
    };
    let seconds = pcm.seconds();
    Ok(match kind {
        Kind::Music if cli.no_music => Converted::Skipped,
        Kind::Music => {
            let [mut l, mut r] = pcm.stereo();
            // Keep the intro and one pass of the loop.
            let found = audio::find_loop(&pcm.mono(), pcm.rate);
            let mut loop_frame = None;
            if let Some((intro, period)) = found {
                l.truncate(intro + period);
                r.truncate(intro + period);
                loop_frame = Some(intro as u64 * cli.music_rate as u64 / pcm.rate as u64);
            } else if l.len() > MUSIC_CAP_SECONDS * pcm.rate as usize {
                // No loop to cut at: keep the first minutes, faded out, and repeat from the
                // start.
                let keep = MUSIC_CAP_SECONDS * pcm.rate as usize;
                let fade = 3 * pcm.rate as usize;
                for ch in [&mut l, &mut r] {
                    ch.truncate(keep);
                    for (i, s) in ch[keep - fade..].iter_mut().enumerate() {
                        *s *= 1.0 - i as f32 / fade as f32;
                    }
                }
            }
            let l = audio::to_i16(&audio::resample(&l, pcm.rate, cli.music_rate));
            let r = audio::to_i16(&audio::resample(&r, pcm.rate, cli.music_rate));
            let frames = l.len() as u32;
            let interleaved: Vec<i16> = l.iter().zip(&r).flat_map(|(&a, &b)| [a, b]).collect();
            let per_sector = adpcm::xa_frames_per_sector(true) as u64;
            let loop_sector = loop_frame.map_or(u32::MAX, |f| (f / per_sector) as u32);
            let kept = frames as f64 / cli.music_rate as f64;
            Converted::Music(adpcm::encode_xa(&interleaved, true), frames, kept.min(seconds), loop_sector)
        }
        Kind::Voice => Converted::Bytes(audio::spu_sound(&pcm, cli.voice_rate), seconds),
        _ => Converted::Bytes(audio::spu_sound(&pcm, cli.se_rate), seconds),
    })
}

fn is_movie(path: &Path) -> bool {
    ["wmv", "mpg", "mpeg", "mp4", "avi", "omv"].iter().any(|e| game::ext_eq(path, e))
}

/// ISO 9660 names for the game's files: 8.3, upper case, unique.
#[derive(Default)]
struct DiscNames {
    /// Game name (lower case) -> disc name, in the order added.
    map: Vec<(String, String)>,
}

impl DiscNames {
    fn add(&mut self, name: &str) -> String {
        let lower = name.to_ascii_lowercase();
        let (stem, ext) = match lower.rsplit_once('.') {
            Some((s, e)) => (s.to_string(), e.to_string()),
            None => (lower.clone(), String::new()),
        };
        self.insert(lower, &stem, &ext)
    }

    /// A movie: its folder joins its name, and it becomes `.STR`.
    fn add_str(&mut self, name: &str) -> String {
        let lower = name.to_ascii_lowercase();
        let stem = lower.rsplit_once('.').map_or(lower.as_str(), |(s, _)| s).replace('/', "");
        self.insert(lower, &stem, "str")
    }

    fn insert(&mut self, lower: String, stem: &str, ext: &str) -> String {
        let clean = |s: &str, n: usize| -> String {
            s.chars()
                .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' })
                .take(n)
                .collect()
        };
        let ext = clean(ext, 3);
        let mut base = clean(stem, 8);
        let mut n = 1;
        while self.map.iter().any(|(_, d)| d == &format!("{base}.{ext}")) {
            let suffix = n.to_string();
            base = format!("{}{suffix}", clean(stem, 8 - suffix.len()));
            n += 1;
        }
        let disc = format!("{base}.{ext}");
        self.map.push((lower, disc.clone()));
        disc
    }

    fn write(&self, path: &Path) -> Result<()> {
        let text: String = self.map.iter().map(|(g, d)| format!("{g} {d}\n")).collect();
        game::write_file(path, text.as_bytes())
    }
}

/// mkpsxiso's project: the boot files, then the small files, then packs, music and movies.
fn write_iso_xml(out: &Path, names: &DiscNames) -> Result<()> {
    let mut files = String::new();
    files.push_str("      <file name=\"SYSTEM.CNF\" type=\"data\" source=\"SYSTEM.CNF\"/>\n");
    files.push_str("      <file name=\"RFVP.EXE\" type=\"data\" source=\"RFVP.EXE\"/>\n");
    files.push_str("      <file name=\"FILES.MAP\" type=\"data\" source=\"FILES.MAP\"/>\n");
    files.push_str("      <file name=\"FONT_LIC.TXT\" type=\"data\" source=\"FONT_LIC.TXT\"/>\n");
    for (_, disc) in &names.map {
        let ty = if disc.ends_with(".XA") || disc.ends_with(".STR") { "mixed" } else { "data" };
        files.push_str(&format!("      <file name=\"{disc}\" type=\"{ty}\" source=\"{disc}\"/>\n"));
    }
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <iso_project image_name=\"rfvp.bin\" cue_sheet=\"rfvp.cue\">\n\
         \x20 <track type=\"data\">\n\
         \x20   <identifiers system=\"PLAYSTATION\" application=\"PLAYSTATION\" volume=\"RFVP\" volume_set=\"RFVP\" publisher=\"RFVP\" data_preparer=\"RFVP-PSX\"/>\n\
         \x20   <directory_tree>\n{files}\x20     <dummy sectors=\"1024\"/>\n\x20   </directory_tree>\n\
         \x20 </track>\n</iso_project>\n"
    );
    game::write_file(&out.join("iso.xml"), xml.as_bytes())?;
    game::write_file(
        &out.join("SYSTEM.CNF"),
        b"BOOT=cdrom:\\RFVP.EXE;1\r\nTCB=4\r\nEVENT=10\r\nSTACK=801FFFF0\r\n",
    )
}
