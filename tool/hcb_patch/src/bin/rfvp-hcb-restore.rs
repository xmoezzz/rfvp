//! Offline substitute-character restoration; HCB emission belongs to the existing assembler.
use anyhow::{bail, ensure, Context, Result};
use clap::{Args, Parser, Subcommand};
use disassembler::Disassembler;
use rfvp::script::parser::Nls;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const LIMIT: u64 = 256 * 1024 * 1024;

#[derive(Parser)]
#[command(about = "Extract a translation character table or rebuild an HCB offline")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Extract character pairs from an explicitly selected EXE/DLL or table file.
    Extract {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, requires = "count", value_parser = number)]
        offset: Option<usize>,
        #[arg(long, requires = "offset")]
        count: Option<usize>,
    },
    /// Restore selected string literals, then use the existing HCB assembler.
    Rebuild(Rebuild),
}

#[derive(Args)]
#[command(group(clap::ArgGroup::new("string_selection").required(true).args(["all_strings", "offsets"])))]
struct Rebuild {
    hcb: PathBuf,
    #[arg(long)]
    table: PathBuf,
    #[arg(short, long)]
    output: PathBuf,
    #[arg(long, default_value = "sjis")]
    input_encoding: Nls,
    #[arg(long, default_value = "utf8")]
    output_encoding: Nls,
    /// Explicitly include all string literals, including any resource paths.
    #[arg(long)]
    all_strings: bool,
    /// UTF-8 file of original string payload offsets, one decimal/0xHEX offset per line.
    #[arg(long)]
    offsets: Option<PathBuf>,
}

fn number(text: &str) -> Result<usize, String> {
    let text = text.trim();
    let parsed = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => usize::from_str_radix(hex, 16),
        None => text.parse(),
    };
    parsed.map_err(|error| error.to_string())
}

fn read(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= LIMIT, "input exceeds 256 MiB");
    Ok(bytes)
}

fn publish(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path)
        .context("output must be a new file")?;
    Ok(())
}

fn pairs(bytes: &[u8]) -> Result<BTreeMap<char, char>> {
    ensure!(
        !bytes.is_empty() && bytes.len() % 4 == 0,
        "table must contain UTF-16LE character pairs"
    );
    let mut map = BTreeMap::new();
    for pair in bytes.chunks_exact(4) {
        let from = u16::from_le_bytes([pair[0], pair[1]]) as u32;
        let to = u16::from_le_bytes([pair[2], pair[3]]) as u32;
        ensure!(
            from >= 0x2000 && to >= 0x2000,
            "unexpected character-table code unit"
        );
        let from = char::from_u32(from).context("surrogate key in character table")?;
        let to = char::from_u32(to).context("surrogate value in character table")?;
        ensure!(
            map.insert(from, to).is_none(),
            "duplicate character-table key {from:?}"
        );
    }
    Ok(map)
}

// Pair-run discovery from PR #58, restricted to an explicitly selected input.
// This is a heuristic candidate, never installed in an engine or applied automatically.
fn candidate(bytes: &[u8]) -> Result<&[u8]> {
    let mut best = 0..0;
    let mut start = 0;
    for (index, pair) in bytes.chunks_exact(4).enumerate() {
        let from = u16::from_le_bytes([pair[0], pair[1]]) as u32;
        let to = u16::from_le_bytes([pair[2], pair[3]]) as u32;
        if from < 0x2000
            || to < 0x2000
            || char::from_u32(from).is_none()
            || char::from_u32(to).is_none()
        {
            start = (index + 1) * 4;
        } else {
            let end = (index + 1) * 4;
            if end - start > best.len() {
                best = start..end;
            }
        }
    }
    ensure!(
        best.len() / 4 >= 64,
        "no plausible table; provide --offset and --count from source analysis"
    );
    eprintln!("Candidate at {:#x}, {} pairs; heuristic extraction is not proof of a valid translation table.", best.start, best.len() / 4);
    Ok(&bytes[best])
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Extract {
            input,
            output,
            offset,
            count,
        } => {
            let bytes = read(&input)?;
            let table = if let (Some(offset), Some(count)) = (offset, count) {
                let end = count
                    .checked_mul(4)
                    .and_then(|size| offset.checked_add(size))
                    .context("table range overflow")?;
                bytes
                    .get(offset..end)
                    .context("table range outside input")?
            } else {
                candidate(&bytes)?
            };
            let map = pairs(table)?;
            publish(&output, table)?;
            println!(
                "Wrote {} character pairs to {}",
                map.len(),
                output.display()
            );
        }
        Command::Rebuild(args) => {
            ensure!(
                fs::metadata(&args.hcb)?.len() <= LIMIT,
                "HCB exceeds 256 MiB"
            );
            let map = pairs(&read(&args.table)?)?;
            let mut program = Disassembler::new(&args.hcb, args.input_encoding)?;
            program.disassemble()?;
            let selected = if let Some(path) = args.offsets {
                let text = String::from_utf8(read(&path)?)?;
                let mut offsets = BTreeSet::new();
                for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
                    let value = number(line).map_err(anyhow::Error::msg)?;
                    offsets.insert(u32::try_from(value)?);
                }
                Some(offsets)
            } else {
                ensure!(
                    args.all_strings,
                    "choose --offsets or explicitly select --all-strings"
                );
                None
            };
            let mut replacements = BTreeMap::new();
            let mut found = BTreeSet::new();
            for (offset, text) in program.string_literals() {
                if selected.as_ref().is_some_and(|set| !set.contains(&offset)) {
                    continue;
                }
                found.insert(offset);
                let restored: String = text
                    .chars()
                    .map(|ch| map.get(&ch).copied().unwrap_or(ch))
                    .collect();
                if restored != text {
                    replacements.insert(offset, restored);
                }
            }
            if let Some(selected) = selected {
                if let Some(missing) = selected.difference(&found).next() {
                    bail!("offset {missing:#x} is not an original PushString payload");
                }
            }
            program.replace_strings(&replacements)?;
            let project = tempfile::tempdir()?;
            program.write_insts(project.path())?;
            let bytes = assembler::assemble(project.path(), args.output_encoding)?;
            ensure!(bytes.len() as u64 <= LIMIT, "output HCB exceeds 256 MiB");
            publish(&args.output, &bytes)?;
            println!(
                "Wrote {}: {} restored literals",
                args.output.display(),
                replacements.len()
            );
        }
    }
    Ok(())
}
