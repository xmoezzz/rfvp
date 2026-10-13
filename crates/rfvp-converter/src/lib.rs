//! Conversion API for the CLI and future GUI. No console SDK or GUI runtime is needed.
use anyhow::{Context, Result, ensure};
pub use rfvp_converter_api::{ConversionContext, ProgressEvent, ProgressKind};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Ps1,
    Ps2,
    Psp,
    #[value(name = "3ds")]
    #[serde(rename = "3ds")]
    ThreeDs,
    Ps3,
    #[value(alias = "vita")]
    Psv,
    Switch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    #[default]
    Full,
    ImagesOnly,
    MasksOnly,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Ps1Options {
    pub font_size: usize,
    pub music_rate: u32,
    pub voice_rate: u32,
    pub se_rate: u32,
    pub no_movies: bool,
    pub no_music: bool,
    pub music_alias: Vec<String>,
    pub clut4: Vec<String>,
}
impl Default for Ps1Options {
    fn default() -> Self {
        Self {
            font_size: 12,
            music_rate: 18900,
            voice_rate: 18900,
            se_rate: 22050,
            no_movies: false,
            no_music: false,
            music_alias: vec!["bgm2=bgm".into()],
            clut4: vec!["graph".into()],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversionRequest {
    pub platform: Platform,
    pub input: PathBuf,
    pub output: PathBuf,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default = "default_filter")]
    pub image_filter: String,
    #[serde(default)]
    pub ui_layout: Option<PathBuf>,
    #[serde(default)]
    pub image_pack: Vec<String>,
    #[serde(default)]
    pub threads: usize,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub ps1: Ps1Options,
}
fn default_filter() -> String {
    "dpid-balanced".into()
}

#[derive(Clone, Debug, Serialize)]
pub struct PlatformInfo {
    pub platform: Platform,
    pub display_width: u32,
    pub display_height: u32,
    pub layout: &'static str,
    pub image_policy: &'static str,
    pub font: &'static str,
    pub install_directory: &'static str,
}

pub fn platforms() -> Vec<PlatformInfo> {
    use Platform::*;
    vec![
        PlatformInfo {
            platform: Ps1,
            display_width: 320,
            display_height: 240,
            layout: "iso9660-source",
            image_policy: "ps1-clut-and-bayer-mask",
            font: "font.psxf",
            install_directory: "converted disc source",
        },
        PlatformInfo {
            platform: Ps2,
            display_width: 640,
            display_height: 448,
            layout: "loose-resources",
            image_policy: "low-resolution-nvsg",
            font: "defualt.tmap",
            install_directory: "mass:/RFVP or host:game",
        },
        PlatformInfo {
            platform: Psp,
            display_width: 480,
            display_height: 272,
            layout: "loose-resources",
            image_policy: "low-resolution-nvsg",
            font: "defualt.tmap",
            install_directory: "next to EBOOT.PBP",
        },
        PlatformInfo {
            platform: ThreeDs,
            display_width: 400,
            display_height: 240,
            layout: "loose-resources",
            image_policy: "low-resolution-nvsg",
            font: "defualt.tmap",
            install_directory: "sdmc:/3ds/rfvp/game",
        },
        PlatformInfo {
            platform: Ps3,
            display_width: 1280,
            display_height: 720,
            layout: "original-packs",
            image_policy: "preserve-original",
            font: "default.ttf",
            install_directory: "/dev_usb000/rfvp/game",
        },
        PlatformInfo {
            platform: Psv,
            display_width: 960,
            display_height: 544,
            layout: "original-packs",
            image_policy: "preserve-original",
            font: "default.ttf",
            install_directory: "ux0:data/rfvp/game",
        },
        PlatformInfo {
            platform: Switch,
            display_width: 1280,
            display_height: 720,
            layout: "original-packs",
            image_policy: "preserve-original",
            font: "default.ttf",
            install_directory: "sdmc:/switch/rfvp/game",
        },
    ]
}

#[derive(Debug, Serialize)]
pub struct DependencyInfo {
    pub name: &'static str,
    pub required: bool,
    pub available: bool,
}
pub fn dependencies(platform: Platform, mode: Mode, ps1: &Ps1Options) -> Vec<DependencyInfo> {
    let media = mode == Mode::Full;
    let native = matches!(platform, Platform::Ps3 | Platform::Psv | Platform::Switch);
    let required_ffmpeg = media && !native && (platform != Platform::Ps1 || !ps1.no_movies);
    let required_ffprobe = media && !native && platform != Platform::Ps1;
    [("ffmpeg", required_ffmpeg), ("ffprobe", required_ffprobe)]
        .into_iter()
        .map(|(name, required)| {
            let available = rfvp_converter_api::media_command(name)
                .arg("-version")
                .output()
                .is_ok_and(|r| r.status.success());
            DependencyInfo {
                name,
                required,
                available,
            }
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct ConversionSummary {
    pub platform: Platform,
    pub output: PathBuf,
    pub mode: Mode,
    pub layout: &'static str,
    pub image_policy: &'static str,
    pub files: usize,
    pub bytes: u64,
}

pub fn convert(
    request: &ConversionRequest,
    context: &ConversionContext,
) -> Result<ConversionSummary> {
    rfvp_converter_api::validate_paths(&request.input, &request.output)?;
    ensure!(
        request.mode != Mode::MasksOnly || request.platform == Platform::Ps1,
        "--masks-only is PS1-only"
    );
    let native = matches!(
        request.platform,
        Platform::Ps3 | Platform::Psv | Platform::Switch
    );
    ensure!(
        !native || request.ui_layout.is_none(),
        "native platforms preserve image geometry; UI layouts require PS1, PS2, PSP or 3DS"
    );
    ensure!(
        request.image_pack.is_empty() || request.mode != Mode::Full,
        "--image-pack requires --images-only or --masks-only"
    );
    ensure!(
        request.platform == Platform::Ps1 || request.image_pack.len() <= 1,
        "this platform accepts one --image-pack per update"
    );
    ensure!(
        request.platform == Platform::Ps1
            || (!request.ps1.no_movies && !request.ps1.no_music && request.ps1.font_size == 12),
        "--no-movies, --no-music and --font-size are PS1-only options"
    );
    for name in ["conversion-report.json", "profile.json"] {
        let path = request.output.join(name);
        if path.is_file() {
            let existing: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
            if let Some(platform) = existing.get("platform") {
                ensure!(
                    *platform == serde_json::to_value(request.platform)?,
                    "output belongs to a different platform; choose a separate output directory"
                );
            }
        }
    }
    if request.mode == Mode::Full && !request.overwrite && request.output.exists() {
        ensure!(
            fs::read_dir(&request.output)?.next().is_none(),
            "output is not empty; choose a new directory or use --overwrite"
        );
    }
    if request.mode != Mode::Full {
        ensure!(
            request.output.is_dir(),
            "image updates require an existing converted directory"
        );
    }
    for dependency in dependencies(request.platform, request.mode, &request.ps1) {
        ensure!(
            !dependency.required || dependency.available,
            "{} is required for full media conversion; install FFmpeg or use --images-only",
            dependency.name
        );
    }
    let filter = request.image_filter.parse().map_err(anyhow::Error::msg)?;
    if request.platform == Platform::Ps1 {
        let p = &request.ps1;
        rfvp_psx::convert(
            &rfvp_psx::ConversionOptions {
                input: request.input.clone(),
                output: request.output.clone(),
                font_size: p.font_size,
                music_rate: p.music_rate,
                voice_rate: p.voice_rate,
                se_rate: p.se_rate,
                no_movies: p.no_movies,
                no_music: p.no_music,
                music_alias: p.music_alias.clone(),
                clut4: p.clut4.clone(),
                threads: request.threads,
                image_filter: filter,
                ui_layout: request.ui_layout.clone(),
                images_only: request.mode != Mode::Full,
                image_pack: request.image_pack.clone(),
                masks_only: request.mode == Mode::MasksOnly,
            },
            context,
        )?;
    } else {
        let platform = match request.platform {
            Platform::Ps2 => rfvp_rebuilder::Platform::Ps2,
            Platform::Psp => rfvp_rebuilder::Platform::Psp,
            Platform::ThreeDs => rfvp_rebuilder::Platform::ThreeDs,
            Platform::Ps3 => rfvp_rebuilder::Platform::Ps3,
            Platform::Psv => rfvp_rebuilder::Platform::Psv,
            Platform::Switch => rfvp_rebuilder::Platform::Switch,
            Platform::Ps1 => unreachable!(),
        };
        rfvp_rebuilder::convert(
            &rfvp_rebuilder::ConversionOptions {
                input: request.input.clone(),
                output: request.output.clone(),
                platform,
                image_filter: filter,
                ui_layout: request.ui_layout.clone(),
                images_only: request.mode != Mode::Full,
                image_pack: request.image_pack.first().cloned(),
                threads: request.threads,
            },
            context,
        )?;
    }
    let mut files = 0;
    let mut bytes = 0;
    let mut pending = vec![request.output.clone()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if entry.file_name() == "conversion-report.json" {
                continue;
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                files += 1;
                bytes += metadata.len();
            }
        }
    }
    let info = platforms()
        .into_iter()
        .find(|info| info.platform == request.platform)
        .unwrap();
    let summary = ConversionSummary {
        platform: request.platform,
        output: request.output.clone(),
        mode: request.mode,
        layout: info.layout,
        image_policy: info.image_policy,
        files,
        bytes,
    };
    let report = request.output.join("conversion-report.json");
    fs::write(&report, serde_json::to_vec_pretty(&summary)?)
        .with_context(|| format!("write {}", report.display()))?;
    Ok(summary)
}
