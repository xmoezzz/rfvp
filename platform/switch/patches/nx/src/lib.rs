//! Userland library for Nintendo Switch homebrew (and other potential purposes), written in pure Rust and some assembly bits
//!
//! # Features
//!
//! This library covers a lot of different modules, wrappers, etc. so some of them (essentially those which can be opt-in) are separated as optional features:
//!
//! - `services`: Enables custom client-IPC service implementations, AKA the `nx::service` module
//!
//! - `smc`: Enables secure-monitor support, AKA the `nx::smc` module
//!
//! - `gpu`: Enables graphics support, AKA the `nx::gpu` module (also enables `services`)
//!
//! - `console`: Enables console support, AKA the `nx::console` module (also enables `canvas` and the `font8x8` dependency)
//!
//! - `vty`: Enables virtual tty support, AKA, the `nx::console::vty` module (also enables `console` as well as the dependencies `embedded-term` and `embedded-graphics-core`)
//!
//! - `fs`: Enables support for this library's FS implementation, aka the `nx::fs` module (also enables `services`)
//!
//! - `input`: Enables input support, AKA the `nx::input` module (also enables `services`)
//!
//! - `la`: Enables library applet support, AKA the `nx::la` module (also enables `services`)
//!
//! - `rand`: Enables pseudo-RNG support, AKA the `nx::rand` module (also enables `services`)
//!
//! - `socket` : Enables std-like network support, AKA the `nx::socket` module (also enables `services`)
//!
//! - `applet` : Enables applet service support, AKA the `nx::applet` module (also enables `services`)
//!
//! - `mii` : Enables mii support, AKA the `nx::mii` module (also enables `services`)
//!
//! Note that most of these features/modules are just simplified and easy-to-use wrappers around IPC/raw system features, so not using them doesn't fully block those features (for instance, you could use services using IPC commands more directly without the `services` feature).
//!
//! # Contributing
//!
//! You can always contribute to these libraries, report bugs, etc. at their [repository](https://github.com/aarch64-switch-rs/nx)
//!
//! # Examples
//!
//! Library examples are located at this other [repository](https://github.com/aarch64-switch-rs/examples)

#![no_std]
// needed to implement the APIs for collection types with custom allocators, and doing raw allocations
#![feature(allocator_api)]
// needed to specify weak linkage on some items
#![feature(linkage)]
// needed for the implementation of the threads module
#![feature(get_mut_unchecked)]
// get rid of mangled error handling in applet::initialize
#![feature(try_blocks)]
// used for ergonomics reading UTF16 strings
#![feature(str_from_utf16_endian)]
//#![warn(missing_docs)]
#![macro_use]
use core::arch::global_asm;

// Required assembly bits (those which essentially cannot/shouldn't be inlined)
global_asm!(include_str!("rrt0.s"));
global_asm!(include_str!("mod0.s"));
//global_asm!(include_str!("exception.s"));

extern crate self as nx;

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate static_assertions;

#[macro_use]
pub mod macros;

#[macro_use]
pub mod result;

pub mod rc;

#[macro_use]
pub mod util;

pub mod mem;

pub mod elf;

pub mod exception;

pub mod sync;

pub mod thread;

pub mod hbl;

#[macro_use]
pub mod rrt0;

// We're going to allow this just because EVERYTHING in there is potentially unsafe in some way,
// even if it's not necessarily memory safety.
#[allow(clippy::missing_safety_doc)]
pub mod svc;

#[macro_use]
pub mod ipc;

#[macro_use]
pub mod diag;

#[cfg(feature = "input")]
pub mod input;

pub mod vmem;

pub mod arm;

pub mod wait;

pub mod version;

#[cfg(feature = "applet")]
pub mod applet;
#[cfg(feature = "services")]
pub mod service;

#[cfg(feature = "gpu")]
pub mod gpu;
#[cfg(feature = "smc")]
pub mod smc;

#[cfg(feature = "fs")]
pub mod fs;

#[cfg(feature = "rand")]
pub mod rand;

#[cfg(feature = "la")]
pub mod la;

#[cfg(any(feature = "console", feature = "vty"))]
#[macro_use]
pub mod console;

#[cfg(feature = "socket")]
pub mod socket;

#[cfg(feature = "mii")]
pub mod mii;
