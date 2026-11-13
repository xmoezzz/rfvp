//! Merge offset translations through RFVP's existing disassembler and assembler.
use anyhow::{bail, ensure, Context, Result};
use clap::Parser;
use disassembler::Disassembler;
use rfvp::script::parser::Nls;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const PATCH_LIMIT: usize = 16 * 1024 * 1024;
const HCB_LIMIT: u64 = 256 * 1024 * 1024;

#[derive(Parser, Debug)]
#[command(about = "Merge patch.dat into a standard HCB using the existing RFVP assembler")]
struct Args {
    hcb: PathBuf,
    patch: PathBuf,
    #[arg(short, long)]
    output: PathBuf,
    #[arg(long, default_value = "sjis")]
    input_encoding: Nls,
    #[arg(long, default_value = "gbk")]
    patch_encoding: Nls,
    #[arg(long, default_value = "utf8")]
    output_encoding: Nls,
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, size: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(size)
            .context("patch offset overflow")?;
        let bytes = self.bytes.get(self.pos..end).context("truncated patch")?;
        self.pos = end;
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into()?))
    }
}

fn read_patch(path: &Path, offsets: &BTreeSet<u32>, nls: Nls) -> Result<BTreeMap<u32, String>> {
    let mut input = Vec::new();
    File::open(path)?
        .take(PATCH_LIMIT as u64 + 1)
        .read_to_end(&mut input)?;
    ensure!(
        (4..=PATCH_LIMIT).contains(&input.len()),
        "invalid patch size"
    );
    let mut header = Reader {
        bytes: &input,
        pos: 0,
    };
    let bytes = if header.u32()? == 0 {
        header.u32()?; // Reserved word in the original format.
        let unpacked = header.u32()? as usize;
        let packed = header.u32()? as usize;
        ensure!(
            (4..=PATCH_LIMIT).contains(&unpacked) && packed.checked_add(16) == Some(input.len()),
            "invalid compressed patch lengths"
        );
        let decoded =
            miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&input[16..], unpacked)
                .map_err(|error| anyhow::anyhow!("patch zlib decode failed: {error:?}"))?;
        ensure!(
            decoded.len() == unpacked,
            "patch uncompressed length mismatch"
        );
        decoded
    } else {
        input
    };
    let mut reader = Reader {
        bytes: &bytes,
        pos: 0,
    };
    let count = reader.u32()? as usize;
    ensure!(count <= (bytes.len() - 4) / 6, "invalid patch record count");
    let mut seen = BTreeSet::new();
    let mut replacements = BTreeMap::new();
    let encoding = match nls {
        Nls::ShiftJIS => encoding_rs::SHIFT_JIS,
        Nls::GBK => encoding_rs::GBK,
        Nls::UTF8 => encoding_rs::UTF_8,
    };
    for _ in 0..count {
        let offset = reader.u32()?;
        let size = reader.u16()? as usize;
        let raw = reader.take(size)?;
        ensure!(
            offsets.contains(&offset),
            "patch offset {offset:#x} is not an original PushString payload"
        );
        ensure!(seen.insert(offset), "duplicate patch offset {offset:#x}");
        if raw.is_empty() {
            continue; // A zero-length record retains the original string.
        }
        ensure!(
            raw.last() == Some(&0) && !raw[..raw.len() - 1].contains(&0),
            "invalid patch string at {offset:#x}"
        );
        let text = encoding
            .decode_without_bom_handling_and_without_replacement(&raw[..raw.len() - 1])
            .with_context(|| format!("invalid patch encoding at {offset:#x}"))?;
        replacements.insert(offset, text.into_owned());
    }
    ensure!(reader.pos == bytes.len(), "trailing patch bytes");
    Ok(replacements)
}

fn main() -> Result<()> {
    let args = Args::parse();
    match fs::symlink_metadata(&args.output) {
        Ok(_) => bail!("output already exists: {}", args.output.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    ensure!(
        fs::metadata(&args.hcb)?.len() <= HCB_LIMIT,
        "HCB exceeds 256 MiB"
    );
    let mut program = Disassembler::new(&args.hcb, args.input_encoding)?;
    program
        .disassemble()
        .context("disassembling original HCB")?;
    let offsets = program
        .string_literals()
        .map(|(offset, _)| offset)
        .collect();
    let replacements = read_patch(&args.patch, &offsets, args.patch_encoding)?;
    program.replace_strings(&replacements)?;

    // Preserve the original addresses in the project; the existing assembler
    // owns instruction sizing, branch/function/thread relocation and metadata.
    let project = tempfile::tempdir()?;
    program.write_insts(project.path())?;
    let output = assembler::assemble(project.path(), args.output_encoding)
        .context("assembling translated HCB")?;
    ensure!(
        output.len() as u64 <= HCB_LIMIT,
        "output HCB exceeds 256 MiB"
    );
    let parent = args
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&output)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(&args.output)
        .context("publishing new HCB without overwriting existing files")?;
    println!(
        "Wrote {}: {} translated literals, {} bytes",
        args.output.display(),
        replacements.len(),
        output.len()
    );
    Ok(())
}
