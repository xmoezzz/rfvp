mod asm;
mod compile;
mod ir;
mod lua;
mod meta;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "lua2hcb")]
#[command(about = "Compile a constrained Lua 5.3 subset (decompiler IR style) back to HCB.")]
struct Cli {
    #[arg(long)]
    meta: PathBuf,

    #[arg(long)]
    lua: PathBuf,

    #[arg(short, long)]
    out: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let meta = meta::load_meta(&cli.meta)
        .with_context(|| format!("loading meta: {}", cli.meta.display()))?;

    let program = lua::parse_lua(&cli.lua).with_context(|| "parsing lua source")?;
    if program.functions.is_empty() {
        return Err(anyhow!("no functions found in lua input"));
    }

    let (items, layout) = compile::compile_program(&meta, &program).with_context(|| "compiling")?;

    let mut emit_meta = meta.clone();
    emit_meta.non_volatile_global_count = layout.non_volatile_count;
    emit_meta.volatile_global_count = layout.volatile_count;

    let (code, labels) = asm::assemble(&emit_meta, &items).with_context(|| "assembling")?;

    let entry_point = labels
        .get("fn:main")
        .copied()
        .ok_or_else(|| anyhow!("missing entry function: main"))?;

    let sysdesc = asm::build_sysdesc(&emit_meta, entry_point).with_context(|| "building sysdesc")?;

    let sys_desc_offset: u32 =
        4 + u32::try_from(code.len()).map_err(|_| anyhow!("code too large"))?;

    let mut out = Vec::new();
    out.extend_from_slice(&sys_desc_offset.to_le_bytes());
    out.extend_from_slice(&code);
    out.extend_from_slice(&sysdesc);

    fs::write(&cli.out, out).with_context(|| format!("writing output: {}", cli.out.display()))?;

    Ok(())
}
