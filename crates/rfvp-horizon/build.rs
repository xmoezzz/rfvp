use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=../rfvp/vendor/rfvp_ogg_vorbis.c");
    println!("cargo:rerun-if-changed=../rfvp/vendor/rfvp_ogg_vorbis.h");
    println!("cargo:rerun-if-changed=../rfvp/vendor/stb_vorbis.c");

    let devkitpro =
        PathBuf::from(env::var("DEVKITPRO").unwrap_or_else(|_| "/opt/devkitpro".into()));
    let compiler = devkitpro.join("devkitA64/bin/aarch64-none-elf-gcc");
    let archiver = devkitpro.join("devkitA64/bin/aarch64-none-elf-ar");
    println!(
        "cargo:rustc-link-search=native={}",
        devkitpro.join("devkitA64/aarch64-none-elf/lib").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        devkitpro
            .join("devkitA64/lib/gcc/aarch64-none-elf/15.2.0")
            .display()
    );
    println!("cargo:rustc-link-lib=m");

    let mut build = cc::Build::new();
    build
        .file("../rfvp/vendor/rfvp_ogg_vorbis.c")
        .include("../rfvp/vendor")
        .include(devkitpro.join("libnx/include"))
        .include(devkitpro.join("devkitA64/aarch64-none-elf/include"))
        .include(devkitpro.join("devkitA64/lib/gcc/aarch64-none-elf/15.2.0/include"))
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
