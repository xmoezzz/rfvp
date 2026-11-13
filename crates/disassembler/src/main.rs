use anyhow::Result;
use clap::Parser;
use disassembler::Disassembler;
use rfvp::script::parser::Nls;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, required = true)]
    input: PathBuf,
    #[arg(short, long, required = true)]
    output: PathBuf,
    #[arg(short, long, default_value = "sjis")]
    lang: Nls,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut disassembler = Disassembler::new(args.input, args.lang)?;
    disassembler.disassemble()?;
    disassembler.write_insts(args.output)
}
