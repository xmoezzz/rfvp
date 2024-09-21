// this is noisy & not well-supported by IDEs
#![allow(clippy::uninlined_format_args)]

extern crate self as shin;

use clap::Parser;

mod asset;
// mod camera;
mod adv;
mod audio;
mod cli;
mod fps_counter;
mod input;
mod layer;
mod render;
mod time;
mod update;
mod window;

fn main() {
    let cli = cli::Cli::parse();

    pollster::block_on(window::run(cli));
}
