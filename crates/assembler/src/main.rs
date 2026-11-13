use anyhow::Result;
use clap::Parser;
use rfvp::script::parser::Nls;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[clap(short, long)]
    project_dir: String,
    #[clap(short, long)]
    output: String,
    #[clap(short, long)]
    nls: Nls,
}

fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();
    assembler::compile(args.project_dir, args.output, args.nls)
}
