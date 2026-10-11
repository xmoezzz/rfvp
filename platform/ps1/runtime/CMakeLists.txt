# The PlayStation build: the engine (one object, made by ../build_ps1_rust.sh) linked into this
# PSn00bSDK runtime. Driven by ../build_ps1.sh.
cmake_minimum_required(VERSION 3.21)
project(rfvp-ps1 LANGUAGES C ASM)

set(RFVP_RUST_OBJECT "" CACHE FILEPATH "The engine's object (build_ps1_rust.sh)")
# 0x80200000 on a console (2 MiB), 0x80800000 with 8 MiB (development consoles, emulators).
set(RFVP_RAM_TOP "0x80200000" CACHE STRING "End of usable RAM")

psn00bsdk_add_executable(rfvp NOGPREL main.c math.c)
target_sources(rfvp PRIVATE ${RFVP_RUST_OBJECT})
set_source_files_properties(${RFVP_RUST_OBJECT} PROPERTIES EXTERNAL_OBJECT TRUE GENERATED TRUE)
target_compile_definitions(rfvp PRIVATE RAM_TOP=${RFVP_RAM_TOP}u)
target_compile_options(rfvp PRIVATE -O2 -std=gnu17)
