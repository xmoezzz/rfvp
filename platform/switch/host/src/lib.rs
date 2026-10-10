//! The rfvp engine for the Nintendo Switch, as a static library for the libnx runtime in
//! `platform/switch/runtime` (which owns the process, input, audio and the display).

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
