//! The rfvp engine for the Wii, as a static library for the libogc runtime in
//! `platform/wii/runtime` (which owns the GPU through GX, the controllers, ASND and the files).

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
