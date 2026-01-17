use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Parser as ClapParser;

mod decode;
mod cfg;
mod lua;
mod opcode;
mod parser;

use crate::decode::decode_program;
use crate::lua::emit_lua_script;
use crate::parser::{Nls, Parser};

#[derive(ClapParser, Debug)]
#[command(version, about = "HCB bytecode to Lua decompiler")]
struct Args {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long)]
    output: PathBuf,

    #[arg(short, long, default_value = "sjis")]
    lang: Nls,
}


fn output_dir_from_file(output: &Path) -> PathBuf {
    output
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(parent) = args.output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut writer = fs::File::create(&args.output)?;

    let parser = Parser::new(&args.input, args.lang)?;
    let program = decode_program(&parser)?;
    let mut functions = Vec::new();
    for (_, func) in &program.functions {
        functions.push((*func).clone());
    }
    emit_lua_script(&mut writer, &parser, &functions)?;

    let name = parser.get_title();
    let ymal_name = if name.is_empty() {
        "output".to_string() + ".yaml"
    } else {
        name.to_string() + ".yaml"
    };

    let yaml_path = output_dir_from_file(&args.output).join(ymal_name);
    parser.export_yaml(yaml_path)?;
    Ok(())
}
