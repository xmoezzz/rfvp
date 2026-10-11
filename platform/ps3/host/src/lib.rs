//! The rfvp engine for the PlayStation 3, as a static library for the PSL1GHT runtime in
//! `platform/ps3/runtime` (which owns the video output, the pad, audio and the files).

#![no_std]

extern crate alloc;

mod app;
mod audio;
mod clock;
mod event;
mod ffi;
mod fs;
mod host;
mod render;
