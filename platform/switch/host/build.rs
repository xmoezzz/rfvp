use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=../rfvp/vendor/rfvp_ogg_vorbis.c");
    println!("cargo:rerun-if-changed=../rfvp/vendor/rfvp_ogg_vorbis.h");
    println!("cargo:rerun-if-changed=../rfvp/vendor/stb_vorbis.c");

    let devkitpro =
        PathBuf::from(env::var("DEVKITPRO").unwrap_or_else(|_| "/opt/devkitpro".into()));
    // devkitA64 keeps its GCC's own headers and libraries in a directory named after the GCC
    // version, which changes between releases: use the newest one that is there.
    let gcc_dir = newest_subdir(&devkitpro.join("devkitA64/lib/gcc/aarch64-none-elf"))
        .unwrap_or_else(|| devkitpro.join("devkitA64/lib/gcc/aarch64-none-elf/15.2.0"));
    let compiler = devkitpro.join("devkitA64/bin/aarch64-none-elf-gcc");
    let archiver = devkitpro.join("devkitA64/bin/aarch64-none-elf-ar");
    println!(
        "cargo:rustc-link-search=native={}",
        devkitpro.join("devkitA64/aarch64-none-elf/lib").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        gcc_dir.display()
    );
    println!("cargo:rustc-link-lib=m");

    let mut build = cc::Build::new();
    build
        .file("../../../crates/rfvp/vendor/rfvp_ogg_vorbis.c")
        .include("../../../crates/rfvp/vendor")
        .include(devkitpro.join("libnx/include"))
        .include(devkitpro.join("devkitA64/aarch64-none-elf/include"))
        .include(gcc_dir.join("include"))
        .flag("-std=c11")
        .flag("-Wall")
        .flag("-Wextra")
        .flag("-Wno-unused-function")
        .flag("-Wno-unused-variable")
        .define("NDEBUG", None);

    if compiler.exists() {
        build.compiler(compiler);
    }
    if archiver.exists() {
        build.archiver(archiver);
    }

    build.compile("rfvp_ogg_vorbis");
}

/// The subdirectory of `dir` with the highest version number in its name, if any.
fn newest_subdir(dir: &std::path::Path) -> Option<PathBuf> {
    let version = |name: &str| -> Vec<u32> {
        name.split('.').map(|part| part.parse().unwrap_or(0)).collect()
    };
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .max_by_key(|entry| version(&entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
}
