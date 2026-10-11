//! The rfvp engine for the PlayStation, as one object for the PSn00bSDK runtime in
//! `platform/ps1/runtime` (which owns the GPU, the SPU, the CD drive and the pad). It plays
//! games converted by rfvp-psx-convert.

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
