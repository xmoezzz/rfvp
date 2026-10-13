use anyhow::Result;
use clap::{Parser, Subcommand};
use rfvp_converter::{ConversionContext, ConversionRequest, Mode, Platform, Ps1Options};
use std::{
    io::{self, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(version, about = "Prepare games for RFVP console ports")]
struct Cli {
    /// Emit newline-delimited JSON for GUI/process clients.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Show platform formats and installation paths.
    Platforms,
    /// Check optional FFmpeg tools before converting.
    Doctor {
        #[arg(long)]
        platform: Platform,
        #[arg(long)]
        images_only: bool,
        #[arg(long)]
        no_movies: bool,
    },
    /// Convert an original game into a separate platform directory.
    Convert {
        #[arg(long)]
        platform: Platform,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "dpid-balanced")]
        image_filter: String,
        #[arg(long)]
        ui_layout: Option<PathBuf>,
        #[arg(long, conflicts_with = "masks_only")]
        images_only: bool,
        #[arg(long)]
        masks_only: bool,
        #[arg(long)]
        image_pack: Vec<String>,
        #[arg(long, default_value_t = 0)]
        threads: usize,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        no_movies: bool,
        #[arg(long)]
        no_music: bool,
        #[arg(long, default_value_t = 12)]
        font_size: usize,
    },
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Platforms => {
            if cli.json {
                println!("{}", serde_json::to_string(&rfvp_converter::platforms())?);
            } else {
                for info in rfvp_converter::platforms() {
                    println!(
                        "{:?}: {}x{}, {}, {}",
                        info.platform,
                        info.display_width,
                        info.display_height,
                        info.image_policy,
                        info.install_directory
                    );
                }
            }
        }
        Commands::Doctor {
            platform,
            images_only,
            no_movies,
        } => {
            let options = Ps1Options {
                no_movies,
                ..Default::default()
            };
            let dependencies = rfvp_converter::dependencies(
                platform,
                if images_only {
                    Mode::ImagesOnly
                } else {
                    Mode::Full
                },
                &options,
            );
            let ready = dependencies.iter().all(|d| !d.required || d.available);
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({"kind":"doctor","ready":ready,"platform":platform,"dependencies":dependencies})
                );
            } else {
                for d in dependencies {
                    println!(
                        "{}: {} ({})",
                        d.name,
                        if d.available { "available" } else { "missing" },
                        if d.required { "required" } else { "optional" }
                    );
                }
            }
            anyhow::ensure!(ready, "required media tools are missing");
        }
        Commands::Convert {
            platform,
            input,
            output,
            image_filter,
            ui_layout,
            images_only,
            masks_only,
            image_pack,
            threads,
            overwrite,
            no_movies,
            no_music,
            font_size,
        } => {
            let context = if cli.json {
                ConversionContext::new(|event| {
                    let mut stdout = io::stdout().lock();
                    serde_json::to_writer(&mut stdout, event)?;
                    writeln!(stdout)?;
                    Ok(())
                })
            } else {
                ConversionContext::human()
            };
            let request = ConversionRequest {
                platform,
                input,
                output,
                mode: if masks_only {
                    Mode::MasksOnly
                } else if images_only {
                    Mode::ImagesOnly
                } else {
                    Mode::Full
                },
                image_filter,
                ui_layout,
                image_pack,
                threads,
                overwrite,
                ps1: Ps1Options {
                    no_movies,
                    no_music,
                    font_size,
                    ..Default::default()
                },
            };
            let summary = rfvp_converter::convert(&request, &context)?;
            if cli.json {
                println!("{}", serde_json::json!({"kind":"result","summary":summary}));
            } else {
                println!(
                    "Done: {} files, {} bytes in {}",
                    summary.files,
                    summary.bytes,
                    summary.output.display()
                );
            }
        }
    }
    Ok(())
}
fn main() {
    let cli = Cli::parse();
    let json = cli.json;
    if let Err(error) = run(cli) {
        if json {
            println!(
                "{}",
                serde_json::json!({"kind":"error","message":format!("{error:#}")})
            );
        } else {
            eprintln!("Error: {error:#}");
        }
        std::process::exit(1);
    }
}
