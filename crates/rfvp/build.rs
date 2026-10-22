fn main() {
    println!("cargo:rerun-if-env-changed=VITASDK");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("vita")
        || std::env::var_os("CARGO_FEATURE_VITA_ZLIB").is_none()
    {
        return;
    }
    let sdk = std::env::var_os("VITASDK").unwrap_or_else(|| "/usr/local/vitasdk".into());
    let sdk = std::path::PathBuf::from(sdk);
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let compiler = sdk.join("bin/arm-vita-eabi-gcc");
    let archiver = sdk.join("bin/arm-vita-eabi-ar");
    println!("cargo:rerun-if-changed=vendor/libdeflate");
    println!("cargo:rerun-if-changed=vendor/rfvp_psv_pixels.c");
    // Only the zlib decompression API is used. The upstream library sources are
    // unmodified. Compile with the documented -O2 recommendation, targeting the
    // Vita's Cortex-A9/NEON; no fast-math or resource-specific defines.
    let sources = [
        "vendor/libdeflate/lib/adler32.c",
        "vendor/libdeflate/lib/deflate_decompress.c",
        "vendor/libdeflate/lib/utils.c",
        "vendor/libdeflate/lib/zlib_decompress.c",
        "vendor/libdeflate/lib/arm/cpu_features.c",
        "vendor/rfvp_psv_pixels.c",
    ];
    let mut objects = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let object = out.join(format!("fast_assets_{index}.o"));
        let status = std::process::Command::new(&compiler)
            .args(["-c", source, "-o"]).arg(&object)
            .args(["-std=c11", "-O2", "-mcpu=cortex-a9", "-mfpu=neon", "-mfloat-abi=hard",
                   "-ffunction-sections", "-fdata-sections", "-Ivendor/libdeflate"])
            .status().expect("could not start Vita C compiler");
        assert!(status.success(), "native asset compilation failed: {source}");
        objects.push(object);
    }
    let archive = out.join("librfvp_fast_assets.a");
    if archive.exists() { std::fs::remove_file(&archive).unwrap(); }
    let status = std::process::Command::new(&archiver)
        .arg("crs").arg(&archive).args(&objects).status().expect("could not start Vita archiver");
    assert!(status.success(), "native asset archive failed");
    // Bundle beside the callers in rfvp so the Vita static linker sees both.
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=rfvp_fast_assets");
}
