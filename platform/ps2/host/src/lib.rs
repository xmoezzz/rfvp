//! The rfvp engine for the PlayStation 2, as a static library for the ps2sdk runtime in
//! `platform/ps2/runtime` (which owns the GS, the pads, audsrv and the files).

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
