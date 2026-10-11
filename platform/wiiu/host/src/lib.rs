//! The rfvp engine for the Wii U, as a static library for the wut runtime in
//! `platform/wiiu/runtime` (which owns the process, the GamePad, sound and the screens).

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
