//! Builds the Ogg Vorbis decoder (stb_vorbis) for the PSP.
//!
//! It is compiled with clang for rustc's own target, `mipsel-sony-psp`, so both agree on the
//! ABI and lld accepts the objects (psp-gcc's are refused). PSPDEV supplies the C headers only:
//! nothing links a C library (see c/rfvp_psp_crt.c).
//!
//! `PSP_CLANG` names the clang to use (default: Homebrew's LLVM, then `clang`); `PSPDEV` the
//! PSPDEV toolchain (default: `~/pspdev`).

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=c/rfvp_psp_crt.c");
    println!("cargo:rerun-if-changed=../../../crates/rfvp/vendor/rfvp_ogg_vorbis.c");
    println!("cargo:rerun-if-changed=../../../crates/rfvp/vendor/stb_vorbis.c");
    println!("cargo:rerun-if-env-changed=PSPDEV");
    println!("cargo:rerun-if-env-changed=PSP_CLANG");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("psp") {
        return;
    }

    let pspdev = env::var_os("PSPDEV")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join("pspdev")))
        .expect("set PSPDEV to the PSPDEV toolchain");
    let include = pspdev.join("psp/include");
    assert!(
        include.join("stdlib.h").exists(),
        "PSPDEV C headers were not found in {}",
        include.display()
    );
    let clang = env::var_os("PSP_CLANG").map(PathBuf::from).unwrap_or_else(|| {
        let brew = PathBuf::from("/opt/homebrew/opt/llvm/bin/clang");
        if brew.exists() {
            brew
        } else {
            PathBuf::from("clang")
        }
    });
    let llvm_ar = clang.with_file_name("llvm-ar");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    let sources = [
        "c/rfvp_psp_crt.c",
        "../../../crates/rfvp/vendor/rfvp_ogg_vorbis.c",
    ];
    let mut objects = Vec::new();
    for source in sources {
        let object = out_dir.join(format!(
            "{}.o",
            PathBuf::from(source).file_stem().unwrap().to_string_lossy()
        ));
        let status = Command::new(&clang)
            .args([
                "--target=mipsel-sony-psp",
                "-mcpu=mips2",
                "-msingle-float",
                "-mno-abicalls",
                // As rustc's PSP target: no `teq` trap after each division.
                "-mllvm",
                "-mno-check-zero-division",
                "-fno-pic",
                "-G0",
                "-c",
                "-O2",
                "-std=gnu11",
                "-ffunction-sections",
                "-fdata-sections",
                "-fno-builtin-malloc",
                "-D__NEWLIB__",
                "-DNDEBUG",
                "-I../../../crates/rfvp/vendor",
                "-Wno-everything",
            ])
            .arg("-idirafter")
            .arg(&include)
            .arg(source)
            .arg("-o")
            .arg(&object)
            .status()
            .expect("failed to run clang");
        assert!(status.success(), "clang failed on {source}");
        // rustc's objects carry no FP ABI mark while these say "single precision", which lld
        // refuses to mix; the code is the same (rustc builds with +single-float), so drop it.
        let status = Command::new(clang.with_file_name("llvm-objcopy"))
            .arg("--remove-section=.MIPS.abiflags")
            .arg("--remove-section=.gnu.attributes")
            .arg(&object)
            .status()
            .expect("failed to run llvm-objcopy");
        assert!(status.success(), "llvm-objcopy failed on {source}");
        objects.push(object);
    }

    let archive = out_dir.join("librfvp_psp_c.a");
    let _ = std::fs::remove_file(&archive);
    let ar = if llvm_ar.exists() { llvm_ar } else { PathBuf::from("llvm-ar") };
    let status = Command::new(&ar)
        .arg("crs")
        .arg(&archive)
        .args(&objects)
        .status()
        .expect("failed to run llvm-ar");
    assert!(status.success(), "llvm-ar failed");
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=rfvp_psp_c");
}
