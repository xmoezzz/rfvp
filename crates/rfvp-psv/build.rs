use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=c/rfvp_psv_c.h");
    println!("cargo:rerun-if-changed=c/rfvp_psv_c.c");
    println!("cargo:rerun-if-changed=c/rfvp_psv_vitasdk.c");
    println!("cargo:rerun-if-changed=../rfvp/vendor/rfvp_ogg_vorbis.c");
    println!("cargo:rerun-if-changed=../rfvp/vendor/rfvp_ogg_vorbis.h");
    println!("cargo:rerun-if-changed=../rfvp/vendor/stb_vorbis.c");
    println!("cargo:rustc-link-lib=m");

    if env::var_os("CARGO_FEATURE_C_GLUE").is_none() {
        return;
    }

    let vitasdk_backend = env::var_os("CARGO_FEATURE_VITASDK_BACKEND").is_some();
    if vitasdk_backend {
        build_vitasdk_c_glue();
    } else {
        build_host_c_glue();
    }
}

fn build_host_c_glue() {
    cc::Build::new()
        .file("c/rfvp_psv_c.c")
        .file("../rfvp/vendor/rfvp_ogg_vorbis.c")
        .include("c")
        .include("../rfvp/vendor")
        .flag_if_supported("-std=c11")
        .flag_if_supported("-Wall")
        .flag_if_supported("-Wextra")
        .flag_if_supported("-Werror=implicit-function-declaration")
        .compile("rfvp_psv_c_glue");
}

fn build_vitasdk_c_glue() {
    let vitasdk = env::var("VITASDK").unwrap_or_else(|_| {
        panic!("feature `vitasdk-backend` requires VITASDK to point to a VitaSDK installation");
    });
    let vitasdk_root = PathBuf::from(vitasdk);
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is not set"));

    let compiler = vitasdk_root.join("bin/arm-vita-eabi-gcc");
    let archiver = vitasdk_root.join("bin/arm-vita-eabi-ar");
    let ranlib = vitasdk_root.join("bin/arm-vita-eabi-ranlib");
    let include_dir = vitasdk_root.join("arm-vita-eabi/include");
    let lib_dir = vitasdk_root.join("arm-vita-eabi/lib");

    require_tool(&compiler, "arm-vita-eabi-gcc");
    require_tool(&archiver, "arm-vita-eabi-ar");
    require_tool(&ranlib, "arm-vita-eabi-ranlib");

    let c_obj = out_dir.join("rfvp_psv_c.o");
    let vitasdk_obj = out_dir.join("rfvp_psv_vitasdk.o");
    let ogg_obj = out_dir.join("rfvp_ogg_vorbis.o");
    let archive = out_dir.join("librfvp_psv_c_glue.a");

    compile_c(
        &compiler,
        "c/rfvp_psv_c.c",
        &c_obj,
        &[Path::new("c"), include_dir.as_path()],
        &["RFVP_PSV_VITASDK_BACKEND"],
    );
    compile_c(
        &compiler,
        "c/rfvp_psv_vitasdk.c",
        &vitasdk_obj,
        &[Path::new("c"), include_dir.as_path()],
        &["RFVP_PSV_VITASDK_BACKEND"],
    );
    compile_c(
        &compiler,
        "../rfvp/vendor/rfvp_ogg_vorbis.c",
        &ogg_obj,
        &[
            Path::new("c"),
            Path::new("../rfvp/vendor"),
            include_dir.as_path(),
        ],
        &["RFVP_PSV_VITASDK_BACKEND"],
    );

    if archive.exists() {
        std::fs::remove_file(&archive).expect("failed to remove stale C glue archive");
    }

    run(
        Command::new(&archiver)
            .arg("crs")
            .arg(&archive)
            .arg(&c_obj)
            .arg(&vitasdk_obj)
            .arg(&ogg_obj),
        "arm-vita-eabi-ar failed",
    );
    run(
        Command::new(&ranlib).arg(&archive),
        "arm-vita-eabi-ranlib failed",
    );

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=rfvp_psv_c_glue");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=SceDisplay_stub");
    println!("cargo:rustc-link-lib=SceCommonDialog_stub");
    println!("cargo:rustc-link-lib=SceCtrl_stub");
    println!("cargo:rustc-link-lib=SceTouch_stub");
    println!("cargo:rustc-link-lib=SceAudio_stub");
    println!("cargo:rustc-link-lib=SceIofilemgr_stub");
    println!("cargo:rustc-link-lib=SceLibKernel_stub");
}

fn compile_c(compiler: &Path, source: &str, object: &Path, includes: &[&Path], defines: &[&str]) {
    let mut cmd = Command::new(compiler);
    cmd.arg("-c")
        .arg(source)
        .arg("-o")
        .arg(object)
        .arg("-std=c11")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-Werror=implicit-function-declaration");

    for include in includes {
        cmd.arg("-I").arg(include);
    }
    for define in defines {
        cmd.arg(format!("-D{}", define));
    }
    if let Ok(asset_root) = env::var("RFVP_PSV_VITASDK_ASSET_ROOT") {
        cmd.arg(format!("-DRFVP_PSV_VITASDK_ASSET_ROOT=\"{}\"", asset_root));
    }

    run(&mut cmd, "arm-vita-eabi-gcc failed");
}

fn require_tool(path: &Path, name: &str) {
    if !path.exists() {
        panic!("{} was not found at {}", name, path.display());
    }
}

fn run(cmd: &mut Command, message: &str) {
    let status = cmd.status().unwrap_or_else(|err| {
        panic!("{}: failed to start command: {}", message, err);
    });
    if !status.success() {
        panic!("{}: command exited with {}", message, status);
    }
}
