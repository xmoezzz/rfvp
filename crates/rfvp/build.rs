fn main() {
    println!("cargo:rerun-if-env-changed=VITASDK");
    println!("cargo:rerun-if-changed=vendor/rfvp_psv_pixels.c");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("vita") {
        return;
    }

    // Only our pixel channel converters need C. Texture decompression continues
    // to use miniz_oxide; no additional third-party library is built or linked.
    let sdk = std::path::PathBuf::from(
        std::env::var_os("VITASDK").unwrap_or_else(|| "/usr/local/vitasdk".into()),
    );
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let object = out.join("rfvp_psv_pixels.o");
    let status = std::process::Command::new(sdk.join("bin/arm-vita-eabi-gcc"))
        .args(["-c", "vendor/rfvp_psv_pixels.c", "-o"])
        .arg(&object)
        .args([
            "-std=c11",
            "-O2",
            "-mcpu=cortex-a9",
            "-mfpu=neon",
            "-mfloat-abi=hard",
            "-ffunction-sections",
            "-fdata-sections",
        ])
        .status()
        .expect("could not start Vita C compiler");
    assert!(status.success(), "Vita pixel conversion compilation failed");

    let archive = out.join("librfvp_psv_pixels.a");
    if archive.exists() {
        std::fs::remove_file(&archive).unwrap();
    }
    let status = std::process::Command::new(sdk.join("bin/arm-vita-eabi-ar"))
        .arg("crs")
        .arg(&archive)
        .arg(&object)
        .status()
        .expect("could not start Vita archiver");
    assert!(status.success(), "Vita pixel conversion archive failed");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=rfvp_psv_pixels");
}
