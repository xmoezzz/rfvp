//! Time spent in each stage of a frame, for finding what is slow on a console.
//!
//! Off until a host hands over a clock with [`set_clock`]; then [`start`] and [`stop`] add
//! the time between them to a named stage, and [`take`] reads and clears the totals.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

static CLOCK: AtomicUsize = AtomicUsize::new(0);

/// The stages that are measured.
pub const STAGES: [&str; 23] = [
    "text", "alpha", "move", "rotation", "scale", "z", "v3d", "anim", "parts", "snow",
    "dissolve", "tree", "submit", "end_frame", "text_tick", "text_raster", "audio_mix", "vm", "audio_cmds", "save", "syscalls", "vm_snapshot",
    "script_io",
];

/// [`STAGES`] index of reading script pages from the disc (counted in [`SCRIPT_PAGE_READS`]).
pub const SCRIPT_IO: usize = 22;

/// Script pages read since the last [`take`].
pub static SCRIPT_PAGE_READS: AtomicU32 = AtomicU32::new(0);

static TOTALS: [AtomicU32; STAGES.len()] = [const { AtomicU32::new(0) }; STAGES.len()];

/// Start measuring with `clock`, a monotonic microsecond counter.
pub fn set_clock(clock: fn() -> u64) {
    CLOCK.store(clock as usize, Ordering::Relaxed);
}

#[inline]
fn now() -> Option<u64> {
    let clock = CLOCK.load(Ordering::Relaxed);
    if clock == 0 {
        return None;
    }
    // SAFETY: only `set_clock` stores here, and it stores a `fn() -> u64`.
    let clock: fn() -> u64 = unsafe { core::mem::transmute(clock) };
    Some(clock())
}

/// The time to pass to [`stop`]; zero when nothing is measured.
#[inline]
pub fn start() -> u64 {
    now().unwrap_or(0)
}

/// Add the time since `started` to `stage` (an index into [`STAGES`]).
#[inline]
pub fn stop(stage: usize, started: u64) {
    if let Some(now) = now() {
        let us = now.saturating_sub(started).min(u32::MAX as u64) as u32;
        TOTALS[stage].fetch_add(us, Ordering::Relaxed);
    }
}

/// The totals in microseconds since the last call, in the order of [`STAGES`].
pub fn take() -> [u32; STAGES.len()] {
    core::array::from_fn(|stage| TOTALS[stage].swap(0, Ordering::Relaxed))
}

/// The last place the core passed (see [`mark`]), for a host's watchdog when a frame stalls.
static MARK: AtomicU32 = AtomicU32::new(0);

/// Note that the core reached `place` (a number that names a spot in the code).
#[inline]
pub fn mark(place: u32) {
    MARK.store(place, Ordering::Relaxed);
}

/// The last [`mark`].
pub fn last_mark() -> u32 {
    MARK.load(Ordering::Relaxed)
}

/// Script instructions run, counted only while a clock is set (see [`count_op`]).
static OPS: AtomicU32 = AtomicU32::new(0);

/// Count one script instruction.
#[inline]
pub fn count_op() {
    if CLOCK.load(Ordering::Relaxed) != 0 {
        OPS.fetch_add(1, Ordering::Relaxed);
    }
}

/// Script instructions run since the last call.
pub fn take_ops() -> u32 {
    OPS.swap(0, Ordering::Relaxed)
}

/// A cycle counter from the host (the PS2's COP0 Count), for timing single script instructions.
static CYCLES: AtomicUsize = AtomicUsize::new(0);
static OP_CYCLES: [AtomicU32; 256] = [const { AtomicU32::new(0) }; 256];
static OP_COUNTS: [AtomicU32; 256] = [const { AtomicU32::new(0) }; 256];

/// Time each script instruction with `counter`, a fast cycle counter.
pub fn set_cycle_counter(counter: extern "C" fn() -> u32) {
    CYCLES.store(counter as usize, Ordering::Relaxed);
}

#[inline]
pub fn cycles() -> u32 {
    let counter = CYCLES.load(Ordering::Relaxed);
    if counter == 0 {
        return 0;
    }
    // SAFETY: only `set_cycle_counter` stores here.
    let counter: extern "C" fn() -> u32 = unsafe { core::mem::transmute(counter) };
    counter()
}

/// Add the cycles since `started` to script opcode `op`.
#[inline]
pub fn op_cycles(op: u8, started: u32) {
    if CYCLES.load(Ordering::Relaxed) != 0 {
        OP_CYCLES[op as usize].fetch_add(cycles().wrapping_sub(started), Ordering::Relaxed);
        OP_COUNTS[op as usize].fetch_add(1, Ordering::Relaxed);
    }
}

/// The opcodes that took the most cycles since the last call: (opcode, count, cycles).
pub fn take_op_cycles(top: usize) -> alloc::vec::Vec<(u8, u32, u32)> {
    let mut all: alloc::vec::Vec<(u8, u32, u32)> = (0..256)
        .map(|op| {
            (
                op as u8,
                OP_COUNTS[op].swap(0, Ordering::Relaxed),
                OP_CYCLES[op].swap(0, Ordering::Relaxed),
            )
        })
        .filter(|&(_, count, _)| count > 0)
        .collect();
    all.sort_by(|a, b| b.2.cmp(&a.2));
    all.truncate(top);
    all
}
