//! The rfvp engine for the Nintendo 3DS, as a static library for the libctru runtime in
//! `platform/3ds/runtime` (which owns the GPU through citro3d, the buttons, ndsp and the files).

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
