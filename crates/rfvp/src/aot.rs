//! The PlayStation's script engine: the game script compiled ahead of time to MIPS code
//! (platform/ps1/aot: hcb_aot.py, aot_link.py), loaded from SCRIPT.AOT on demand.
//!
//! It replaces the interpreter there. The compiled code keeps the script's values in C
//! variables and in frames (one per active script function, holding its arguments, locals and
//! what lives across a call or a yield); it calls the helpers below (`aot.h` is their C side)
//! for what is not plain integer work, and the engine's syscalls through `aot_syscall`.
//!
//! A segment (a compiled function, or a part of a large one) runs until it calls a script
//! function, returns, yields or continues in another segment; the driver (`run`) then runs the
//! frame on top of the thread, loading its segment if it is not cached. So a thread is its
//! chain of frames, and any segment not running can be evicted.
//!
//! Values are 64 bits: a tag and a payload, Nil all zero. Strings from the script stay in the
//! HCB file and are values by address; strings made at run time and tables are runtime objects,
//! collected (mark and sweep) between segments, when every live value is in a frame, a global
//! or a thread's return value.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::cell::UnsafeCell;
use core::ptr;

use crate::host_api::RfvpFile;
use crate::script::context::{ContextSnapshotV1, ThreadState};
use crate::script::parser::Parser;
use crate::script::{Table, Variant};
use crate::subsystem::components::syscalls::Syscaller;
use crate::subsystem::resources::thread_manager::ThreadManagerSnapshotV1;
use crate::subsystem::resources::thread_wrapper::ThreadRequest;
use crate::subsystem::world::GameData;

pub type V = u64;

const T_NIL: u32 = 0;
const T_TRUE: u32 = 1;
const T_INT: u32 = 2;
const T_FLOAT: u32 = 3;
const T_CSTR: u32 = 4;
const T_STR: u32 = 5;
const T_TABLE: u32 = 6;

const TRUE: V = mk(T_TRUE, 0);

const fn mk(tag: u32, val: u32) -> V {
    (tag as u64) << 32 | val as u64
}

fn tag(v: V) -> u32 {
    (v >> 32) as u32
}

fn val(v: V) -> u32 {
    v as u32
}

/// `Frame` in aot.h.
#[repr(C)]
pub struct Frame {
    parent: *mut Frame,
    seg: u16,
    nslots: u16,
    state: u32,
    s: [V; 0],
}

impl Frame {
    fn layout(nslots: u16) -> Layout {
        Layout::from_size_align(16 + 8 * nslots as usize, 8).unwrap()
    }

    fn slots(&mut self) -> &mut [V] {
        unsafe { core::slice::from_raw_parts_mut(self.s.as_mut_ptr(), self.nslots as usize) }
    }

    fn new(seg: u16, nslots: u16, parent: *mut Frame) -> *mut Frame {
        // A freed frame of the same size if there is one (calls are frequent), else a new one.
        let pool = unsafe { &mut *FRAME_POOL.0.get() };
        let f = match pool.get_mut(nslots as usize).and_then(|list| list.pop()) {
            Some(f) => {
                unsafe { core::ptr::write_bytes((*f).s.as_mut_ptr(), 0, nslots as usize) };
                f
            }
            None => {
                let f = unsafe { alloc::alloc::alloc_zeroed(Self::layout(nslots)) } as *mut Frame;
                if f.is_null() {
                    alloc::alloc::handle_alloc_error(Self::layout(nslots));
                }
                f
            }
        };
        unsafe {
            (*f).parent = parent;
            (*f).seg = seg;
            (*f).nslots = nslots;
            (*f).state = 0;
        }
        f
    }

    unsafe fn free(f: *mut Frame) {
        let n = (*f).nslots as usize;
        let pool = &mut *FRAME_POOL.0.get();
        if n < POOLED_SLOTS {
            if pool.len() <= n {
                pool.resize_with(n + 1, Vec::new);
            }
            if pool[n].len() < POOLED_FRAMES {
                pool[n].push(f);
                return;
            }
        }
        alloc::alloc::dealloc(f as *mut u8, Self::layout((*f).nslots));
    }
}

/// Freed frames kept for reuse, by slot count (a few of each size: a call and its return
/// usually reuse the same one).
struct FramePool(UnsafeCell<Vec<Vec<*mut Frame>>>);

unsafe impl Sync for FramePool {}

static FRAME_POOL: FramePool = FramePool(UnsafeCell::new(Vec::new()));
const POOLED_SLOTS: usize = 24;
const POOLED_FRAMES: usize = 4;

/// Free a chain of frames.
unsafe fn free_chain(mut f: *mut Frame) {
    while !f.is_null() {
        let parent = (*f).parent;
        Frame::free(f);
        f = parent;
    }
}

// -----------------------------------------------------------------------------------------------
// Runtime objects

enum Obj {
    Str(String),
    Table(Vec<(u32, V)>),
}

struct Heap {
    objs: Vec<(*mut Obj, bool)>,
    since_gc: usize,
}

/// Collect once this many objects were made since the last collection.
const GC_EVERY: usize = 64;

// -----------------------------------------------------------------------------------------------
// The segment cache

/// Size of `Segments::hints`.
const HINTS: usize = 1024;

/// A segment in the cache.
struct Resident {
    seg: u16,
    /// Running (its code is on the native stack): it stays.
    pins: u16,
    used: u32,
    /// Where its record (header, code, relocations, strings) is in the arena.
    offset: usize,
    size: usize,
}

/// The compiled code in RAM: segments, each as it is needed. SCRIPT.AOT keeps them in pages
/// (what runs together is together on the disc, so the disc's sector cache in main.c often
/// already holds the next one), but the cache holds segments: the code that runs every frame is
/// a few kilobytes spread over many pages.
struct Segments {
    file: Box<dyn RfvpFile>,
    /// Per page: its offset in SCRIPT.AOT and its first segment (one more entry ends the
    /// last page).
    pages: Vec<(u32, u16)>,
    resident: Vec<Resident>,
    /// The segments below this ran in the recorded run SCRIPT.AOT was laid out from: those of a
    /// page tend to run together.
    profiled: u16,
    /// For segment `seg`, likely its index in `resident` (checked before use).
    hints: [u16; HINTS],
    /// One block, taken at boot, so that loading and dropping segments never breaks up the
    /// heap (big pictures need it in one piece).
    arena: *mut u8,
    arena_size: usize,
    tick: u32,
    /// The last segment run, its code and its index in `resident` (a thread mostly returns to
    /// where it was).
    last: (u16, usize, usize),
}

extern "C" {
    /// main.c: the instruction cache, after code was written.
    fn rfvp_ps1_flush_icache();
}

fn le16(b: &[u8], at: usize) -> usize {
    u16::from_le_bytes([b[at], b[at + 1]]) as usize
}

impl Segments {
    /// The code of segment `seg`, loaded if needed.
    fn code(&mut self, seg: u16) -> usize {
        self.tick = self.tick.wrapping_add(1);
        if self.last.0 == seg && self.last.1 != 0 {
            // Still used: keep it from looking old to the eviction.
            if let Some(r) = self.resident.get_mut(self.last.2) {
                r.used = self.tick;
            }
            return self.last.1;
        }
        let tick = self.tick;
        let hint = self.hints[seg as usize % HINTS] as usize;
        let found = if self.resident.get(hint).is_some_and(|r| r.seg == seg) {
            Some(hint)
        } else {
            self.resident.iter().position(|r| r.seg == seg)
        };
        let index = match found {
            Some(k) => k,
            None => {
                let t0 = crate::perf::start();
                let k = self.load(seg);
                crate::perf::stop(crate::perf::SCRIPT_IO, t0);
                k
            }
        };
        self.hints[seg as usize % HINTS] = index as u16;
        let r = &mut self.resident[index];
        r.used = tick;
        let code = self.arena as usize + r.offset + 4;
        self.last = (seg, code, index);
        code
    }

    fn read(&mut self, at: usize, buf: &mut [u8]) {
        // A read can fail while the drive changes from streaming music (main.c retries a few
        // times already): try again before giving up.
        let mut tries = 0;
        while read_exact(&mut *self.file, at as u64, buf).is_err() {
            tries += 1;
            if tries == 8 {
                panic!("SCRIPT.AOT unreadable at {at}");
            }
        }
    }

    /// A free place of `size` bytes in the arena, after evicting the least recently used
    /// segments that are not running as needed.
    fn place(&mut self, size: usize) -> usize {
        loop {
            let mut taken: Vec<(usize, usize)> = self.resident.iter().map(|r| (r.offset, r.size)).collect();
            taken.sort_unstable();
            let mut at = 0;
            for &(offset, used) in taken.iter() {
                if offset - at >= size {
                    return at;
                }
                at = offset + used;
            }
            if self.arena_size - at >= size {
                return at;
            }
            let tick = self.tick;
            let Some((k, _)) = self
                .resident
                .iter()
                .enumerate()
                .filter(|(_, r)| r.pins == 0)
                .max_by_key(|(_, r)| tick.wrapping_sub(r.used))
            else {
                panic!("SCRIPT.AOT: the code cache is full of running code");
            };
            self.resident.swap_remove(k);
            self.last = (0, 0, 0);
        }
    }

    /// Read segment `seg` into the arena; its index in `resident`. Its page is read whole (one
    /// seek), and the page's other segments that ran in the recorded run come with it: they
    /// usually run next (the drive's other reads would have moved it away from them).
    fn load(&mut self, seg: u16) -> usize {
        let page = self.pages.partition_point(|p| p.1 <= seg) - 1;
        let (start, first) = (self.pages[page].0 as usize, self.pages[page].1);
        let size = self.pages[page + 1].0 as usize - start;
        let count = (self.pages[page + 1].1 - first) as usize;
        let mut bytes = alloc::vec![0u8; size];
        self.read(start, &mut bytes);
        let mut index = 0;
        for k in 0..count {
            let s = first + k as u16;
            if s != seg && (s >= self.profiled || self.resident.iter().any(|r| r.seg == s)) {
                continue;
            }
            let at = le16(&bytes, 2 + 2 * k);
            let end = if k + 1 < count { le16(&bytes, 2 + 2 * (k + 1)) } else { size };
            let i = self.install(s, &bytes[at..end]);
            if s == seg {
                index = i;
            }
        }
        unsafe { rfvp_ps1_flush_icache() };
        self.last = (0, 0, 0);
        // `install` may have moved entries (evictions): find it again.
        self.resident.iter().position(|r| r.seg == seg).unwrap_or(index)
    }

    /// Put segment `seg`'s record in the arena, placed where it is; its index in `resident`.
    fn install(&mut self, seg: u16, record: &[u8]) -> usize {
        let size = record.len();
        let offset = self.place(size);
        let buf = unsafe { core::slice::from_raw_parts_mut(self.arena.add(offset), size) };
        buf.copy_from_slice(record);
        // Its references to itself (the entry table) where it now is.
        let code_len = le16(buf, 0);
        let count = le16(buf, 2);
        let code = self.arena as usize + offset + 4;
        for r in 0..count {
            let at = 4 + code_len + 4 * r;
            let entry = le16(buf, at);
            let value = (code + le16(buf, at + 2)) as u32;
            let imm = if entry >> 14 == 0 { value.wrapping_add(0x8000) >> 16 } else { value & 0xffff };
            let off = 4 + (entry & 0x3fff) * 4;
            let word = u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
            buf[off..off + 4].copy_from_slice(&((word & 0xffff_0000) | imm).to_le_bytes());
        }
        self.resident.push(Resident { seg, pins: 0, used: self.tick, offset, size });
        self.resident.len() - 1
    }

    /// Keep (1) or release (-1) segment `seg` (its code is running).
    fn pin(&mut self, seg: u16, delta: i16) {
        if let Some(r) = self.resident.iter_mut().find(|r| r.seg == seg) {
            r.pins = r.pins.wrapping_add_signed(delta);
        }
    }

    /// The record of segment `seg` if it is in memory.
    fn resident_record(&self, seg: u16) -> Option<usize> {
        let r = self.resident.iter().find(|r| r.seg == seg)?;
        Some(self.arena as usize + r.offset)
    }
}

// -----------------------------------------------------------------------------------------------
// The runtime

struct Runtime {
    segments: Segments,
    /// Script functions a thread can start at: address, segment, frame slots.
    entries: Vec<(u32, u16, u16)>,
    /// Each syscall of the script: its argument count and handler.
    syscalls: Vec<(u8, Option<&'static (dyn Syscaller + Send + Sync)>)>,
    globals: Vec<V>,
    heap: Heap,
    /// Frames of the running thread that were dropped (the thread restarted or ended) while
    /// its segment still runs: freed when it returns.
    graveyard: Vec<*mut Frame>,
    /// While a thread runs.
    game: *mut GameData,
    tm: *mut ThreadManager,
    parser: *const Parser,
}

struct Global(UnsafeCell<Option<Runtime>>);

unsafe impl Sync for Global {}

static RUNTIME: Global = Global(UnsafeCell::new(None));

fn rt() -> &'static mut Runtime {
    unsafe { (*RUNTIME.0.get()).as_mut().expect("AOT runtime not initialized") }
}

/// The C side's view of the runtime (aot.h).
#[no_mangle]
pub static mut aot_g: *mut V = ptr::null_mut();
#[no_mangle]
pub static mut aot_ret: V = 0;
#[no_mangle]
pub static mut aot_yield: i32 = 0;

/// Load SCRIPT.AOT's tables. `cache_bytes` is the space for compiled code.
pub fn init(mut file: Box<dyn RfvpFile>, parser: &Parser, cache_bytes: usize) -> anyhow::Result<()> {
    let mut head = [0u8; 32];
    read_exact(&mut *file, 0, &mut head)?;
    let word = |k: usize| u32::from_le_bytes([head[k], head[k + 1], head[k + 2], head[k + 3]]) as usize;
    if &head[0..4] != b"AOT2" {
        anyhow::bail!("SCRIPT.AOT: not a compiled script");
    }
    let (npages, table_off, thr_off, threads, globals) = (word(8), word(12), word(16), word(20), word(24));
    let profiled = word(28).min(u16::MAX as usize) as u16;
    let mut raw = alloc::vec![0u8; (npages + 1) * 8];
    read_exact(&mut *file, table_off as u64, &mut raw)?;
    let pages: Vec<(u32, u16)> = raw
        .chunks_exact(8)
        .map(|c| (u32::from_le_bytes([c[0], c[1], c[2], c[3]]), u16::from_le_bytes([c[4], c[5]])))
        .collect();
    let mut raw = alloc::vec![0u8; threads * 8];
    read_exact(&mut *file, thr_off as u64, &mut raw)?;
    let entries = raw
        .chunks_exact(8)
        .map(|c| {
            (
                u32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                u16::from_le_bytes([c[4], c[5]]),
                u16::from_le_bytes([c[6], c[7]]),
            )
        })
        .collect();
    let arena_size = cache_bytes & !7;
    let arena = unsafe { alloc::alloc::alloc(Layout::from_size_align(arena_size, 8).unwrap()) };
    if arena.is_null() {
        anyhow::bail!("no room for the script's code cache");
    }
    let all = parser.get_all_syscalls();
    let count_sys = all.keys().max().map_or(0, |&k| k + 1);
    let syscalls = (0..count_sys)
        .map(|id| match all.get(&id) {
            Some(s) => (s.args, crate::subsystem::world::syscall_handler(&s.name)),
            None => (0, None),
        })
        .collect();
    let mut globals = alloc::vec![0 as V; globals];
    unsafe {
        aot_g = globals.as_mut_ptr();
        *RUNTIME.0.get() = Some(Runtime {
            segments: Segments {
                file,
                pages,
                resident: Vec::new(),
                profiled,
                hints: [0; HINTS],
                arena,
                arena_size,
                tick: 0,
                last: (0, 0, 0),
            },
            entries,
            syscalls,
            globals,
            heap: Heap { objs: Vec::new(), since_gc: 0 },
            graveyard: Vec::new(),
            game: ptr::null_mut(),
            tm: ptr::null_mut(),
            parser: ptr::null(),
        });
    }
    // The compiled code reaches these only through SCRIPT.AOT: keep them in the executable.
    core::hint::black_box([
        aot_call as *const () as usize,
        aot_invoke as *const () as usize,
        aot_return as *const () as usize,
        aot_goto as *const () as usize,
        aot_syscall as *const () as usize,
        aot_table_get as *const () as usize,
        aot_table_set as *const () as usize,
        aot_arith as *const () as usize,
        aot_cmp as *const () as usize,
        aot_neg as *const () as usize,
    ]);
    Ok(())
}

fn read_exact(file: &mut dyn RfvpFile, offset: u64, out: &mut [u8]) -> anyhow::Result<()> {
    let mut done = 0;
    while done < out.len() {
        match file.read_at(offset + done as u64, &mut out[done..]) {
            Ok(0) | Err(_) => anyhow::bail!("SCRIPT.AOT: short read"),
            Ok(n) => done += n,
        }
    }
    Ok(())
}

pub fn is_ready() -> bool {
    unsafe { (*RUNTIME.0.get()).is_some() }
}

// -----------------------------------------------------------------------------------------------
// Values and the engine's Variant

fn new_obj(obj: Obj) -> *mut Obj {
    let heap = &mut rt().heap;
    let p = Box::into_raw(Box::new(obj));
    heap.objs.push((p, false));
    heap.since_gc += 1;
    p
}

fn obj<'a>(v: V) -> &'a mut Obj {
    unsafe { &mut *(val(v) as usize as *mut Obj) }
}

/// The text of a string from the script (`addr` is where it is in the HCB; its length, NUL
/// included, is the byte before).
fn script_string(addr: u32) -> String {
    if let Some(bytes) = short_string(addr) {
        return unsafe { (*rt().parser).decode_string_bytes(bytes) }.unwrap_or_default();
    }
    let parser = unsafe { &*rt().parser };
    let len = parser.read_u8(addr as usize - 1).unwrap_or(0) as usize;
    parser.read_cstring(addr as usize, len).unwrap_or_default()
}

/// A short string a segment of the running thread carries (SCRIPT.AOT), without its NUL: the
/// one running, or one of its callers (a string is often passed down before it is used).
fn short_string(addr: u32) -> Option<&'static [u8]> {
    let rt = rt();
    let tm = rt.tm;
    if tm.is_null() {
        return None;
    }
    let tm = unsafe { &*tm };
    let mut f = tm.contexts[tm.current_id as usize].top;
    while !f.is_null() {
        if let Some(rec) = rt.segments.resident_record(unsafe { (*f).seg }) {
            if let Some(s) = unsafe { record_string(rec, addr) } {
                return Some(s);
            }
        }
        f = unsafe { (*f).parent };
    }
    None
}

unsafe fn record_string(rec: usize, addr: u32) -> Option<&'static [u8]> {
    let head = core::slice::from_raw_parts(rec as *const u8, 4);
    let mut at = rec + 4 + le16(head, 0) + 4 * le16(head, 2);
    let count = *(at as *const u8) as usize | (*(at as *const u8).add(1) as usize) << 8;
    at += 2;
    for _ in 0..count {
        let p = at as *const u8;
        let a = u32::from_le_bytes([*p, *p.add(1), *p.add(2), *p.add(3)]);
        let len = *p.add(4) as usize;
        if a == addr {
            let bytes = core::slice::from_raw_parts(p.add(5), len);
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(len);
            return Some(&bytes[..end]);
        }
        at += 5 + len;
    }
    None
}

pub fn to_variant(v: V) -> Variant {
    match tag(v) {
        T_TRUE => Variant::True,
        T_INT => Variant::Int(val(v) as i32),
        T_FLOAT => Variant::Float(f32::from_bits(val(v))),
        T_CSTR => Variant::ConstString(script_string(val(v)), val(v)),
        T_STR => match obj(v) {
            Obj::Str(s) => Variant::String(s.clone()),
            Obj::Table(_) => Variant::Nil,
        },
        T_TABLE => match obj(v) {
            Obj::Table(entries) => {
                let mut table = Table::new();
                for &(k, e) in entries.iter() {
                    table.insert(k, to_variant(e));
                }
                Variant::Table(table)
            }
            Obj::Str(_) => Variant::Nil,
        },
        _ => Variant::Nil,
    }
}

pub fn from_variant(v: Variant) -> V {
    match v {
        Variant::Nil => 0,
        Variant::True => TRUE,
        Variant::Int(i) => mk(T_INT, i as u32),
        Variant::Float(f) => mk(T_FLOAT, f.to_bits()),
        // Script strings are values by address.
        Variant::ConstString(_, addr) => mk(T_CSTR, addr),
        Variant::String(s) => mk(T_STR, new_obj(Obj::Str(s)) as u32),
        Variant::Table(t) => {
            let entries = t.entries().map(|(k, e)| (k, from_variant(e.clone()))).collect();
            mk(T_TABLE, new_obj(Obj::Table(entries)) as u32)
        }
        // Frame bookkeeping of the interpreter: never a script value.
        _ => 0,
    }
}

// -----------------------------------------------------------------------------------------------
// Helpers of the compiled code (aot.h)

#[no_mangle]
pub extern "C" fn aot_call(f: *mut Frame, pack: u32, state: u32) -> *mut Frame {
    unsafe {
        (*f).state = state;
        let c = Frame::new(pack as u16, (pack >> 16) as u16, f);
        let tm = &mut *rt().tm;
        tm.contexts[tm.current_id as usize].top = c;
        c
    }
}

/// How deep calls run nested on the native stack (each level keeps its caller's page).
const MAX_NESTED: u32 = 12;
static mut NESTED: u32 = 0;

#[no_mangle]
pub extern "C" fn aot_invoke(c: *mut Frame) -> i32 {
    unsafe {
        if NESTED >= MAX_NESTED {
            return 1;
        }
        let caller = (*c).parent;
        let caller_seg = (*caller).seg;
        rt().segments.pin(caller_seg, 1);
        NESTED += 1;
        let tm = rt().tm;
        let tid = (*tm).current_id as usize;
        let result = loop {
            let top = (&(*tm).contexts)[tid].top;
            if top == caller {
                break -1;
            }
            if top.is_null() {
                break 0;
            }
            if step(top) == 0 {
                break 0;
            }
        };
        NESTED -= 1;
        rt().segments.pin(caller_seg, -1);
        result
    }
}

/// Run the segment of frame `f` (the thread's top) once; its result (0: the thread stopped).
fn step(f: *mut Frame) -> i32 {
    let code = rt().segments.code(unsafe { (*f).seg });
    let segment: extern "C" fn(*mut Frame) -> i32 = unsafe { core::mem::transmute(code) };
    unsafe { aot_yield = 0 };
    crate::perf::count_op();
    let r = segment(f);
    if rt().heap.since_gc >= GC_EVERY {
        collect(unsafe { &*rt().tm });
    }
    r
}

#[no_mangle]
pub extern "C" fn aot_return(f: *mut Frame) -> i32 {
    unsafe {
        let tm = &mut *rt().tm;
        let tid = tm.current_id;
        let parent = (*f).parent;
        Frame::free(f);
        tm.contexts[tid as usize].top = parent;
        if parent.is_null() {
            // The thread's first function returned: the thread ends.
            tm.thread_exit(Some(tid));
            return 0;
        }
        1
    }
}

#[no_mangle]
pub extern "C" fn aot_goto(f: *mut Frame, to: u32) -> i32 {
    unsafe {
        (*f).seg = to as u16;
        (*f).state = to >> 16;
    }
    1
}

/// The most arguments a syscall takes.
const MAX_SYSCALL_ARGS: usize = 24;

#[no_mangle]
pub extern "C" fn aot_syscall(id: u32, args: *const V) -> V {
    let (argc, handler) = rt().syscalls.get(id as usize).copied().unwrap_or((0, None));
    // On the stack, only as many as the syscall takes: no allocation per syscall.
    let argc = (argc as usize).min(MAX_SYSCALL_ARGS);
    let mut buf: [core::mem::MaybeUninit<Variant>; MAX_SYSCALL_ARGS] =
        [const { core::mem::MaybeUninit::uninit() }; MAX_SYSCALL_ARGS];
    for (i, slot) in buf.iter_mut().enumerate().take(argc) {
        slot.write(to_variant(unsafe { *args.add(i) }));
    }
    // SAFETY: the first `argc` are written; they are dropped below.
    let args = unsafe { core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut Variant, argc) };
    let (game, tm) = (rt().game, rt().tm);
    let game = unsafe { &mut *game };
    let t0 = crate::perf::start();
    let result = match handler {
        Some(h) => h.call(game, args).unwrap_or_else(|e| {
            log::error!("syscall {id}: {e:#}");
            Variant::Nil
        }),
        None => Variant::Nil,
    };
    crate::perf::stop(20, t0);
    unsafe { core::ptr::drop_in_place(args as *mut [Variant]) };
    let result = from_variant(result);
    unsafe { (*tm).after_syscall(game) };
    result
}

#[no_mangle]
pub extern "C" fn aot_table_get(table: V, key: V) -> V {
    if tag(table) != T_TABLE || tag(key) != T_INT {
        return 0;
    }
    match obj(table) {
        Obj::Table(entries) => entries.iter().find(|e| e.0 == val(key)).map_or(0, |e| e.1),
        Obj::Str(_) => 0,
    }
}

#[no_mangle]
pub extern "C" fn aot_table_set(slot: *mut V, key: V, value: V) {
    unsafe {
        // As the interpreter: a non-int key clears the variable, and one that is not a table
        // becomes a new, empty one.
        if tag(key) != T_INT {
            *slot = 0;
            return;
        }
        if tag(*slot) != T_TABLE {
            *slot = mk(T_TABLE, new_obj(Obj::Table(Vec::new())) as u32);
        }
        if let Obj::Table(entries) = obj(*slot) {
            match entries.iter_mut().find(|e| e.0 == val(key)) {
                Some(e) => e.1 = value,
                None => entries.push((val(key), value)),
            }
        }
    }
}

const OP_ADD: u32 = 0;
const OP_SUB: u32 = 1;
const OP_MUL: u32 = 2;
const OP_DIV: u32 = 3;
const OP_MOD: u32 = 4;
const OP_BITTEST: u32 = 5;
const OP_AND: u32 = 6;
const OP_OR: u32 = 7;
const OP_EQ: u32 = 8;
const OP_NE: u32 = 9;
const OP_GT: u32 = 10;
const OP_GE: u32 = 11;
const OP_LT: u32 = 12;
const OP_LE: u32 = 13;

/// Arithmetic as the interpreter does it (the engine's own operations, for all but ints).
#[no_mangle]
pub extern "C" fn aot_arith(op: u32, a: V, b: V) -> V {
    if tag(a) == T_INT && tag(b) == T_INT {
        let (x, y) = (val(a) as i32, val(b) as i32);
        let r = match op {
            OP_ADD => Some(x.wrapping_add(y)),
            OP_SUB => Some(x.wrapping_sub(y)),
            OP_MUL => Some(x.wrapping_mul(y)),
            OP_DIV => x.checked_div(y),
            OP_MOD => x.checked_rem(y),
            _ => None,
        };
        return r.map_or(0, |r| mk(T_INT, r as u32));
    }
    // Floats: the engine (vm_add, vm_sub, vm_mul) widens both to f64, keeps the f64 result
    // (and its error, which it then drops) and narrows that to an f32. The same without the
    // double-double error terms, all in software on this CPU.
    let number = |v: V| tag(v) == T_INT || tag(v) == T_FLOAT;
    if op <= OP_MUL && (tag(a) == T_FLOAT || tag(b) == T_FLOAT) && number(a) && number(b) {
        let wide = |v: V, int_as_f32: bool| match tag(v) {
            T_FLOAT => f32::from_bits(val(v)) as f64,
            _ if int_as_f32 => val(v) as i32 as f32 as f64,
            _ => val(v) as i32 as f64,
        };
        // vm_add turns an int right operand into an f32 first.
        let x = wide(a, false);
        let y = wide(b, op == OP_ADD && tag(a) == T_FLOAT);
        let r = match op {
            OP_ADD => x + y,
            OP_SUB => x - y,
            _ => x * y,
        };
        return mk(T_FLOAT, (r as f32).to_bits());
    }
    let (x, y) = (to_variant(a), to_variant(b));
    let r = match op {
        OP_ADD => crate::script::vm_add(x, y),
        OP_SUB => crate::script::vm_sub(x, y),
        OP_MUL => crate::script::vm_mul(x, y),
        OP_DIV => crate::script::vm_div(x, y),
        OP_MOD => crate::script::vm_mod(x, y),
        _ => Variant::Nil,
    };
    from_variant(r)
}

/// Comparisons and the logical operations as the interpreter does them: 1 for True, 0 for Nil.
#[no_mangle]
pub extern "C" fn aot_cmp(op: u32, a: V, b: V) -> i32 {
    match op {
        OP_AND => return (tag(a) != T_NIL && tag(b) != T_NIL) as i32,
        OP_OR => return (tag(a) != T_NIL || tag(b) != T_NIL) as i32,
        OP_BITTEST => {
            return (tag(a) == T_INT && tag(b) == T_INT && val(b) < 32 && (val(a) >> val(b)) & 1 != 0)
                as i32
        }
        _ => {}
    }
    if tag(a) == T_INT && tag(b) == T_INT {
        let (x, y) = (val(a) as i32, val(b) as i32);
        return match op {
            OP_EQ => x == y,
            OP_NE => x != y,
            OP_GT => x > y,
            OP_GE => x >= y,
            OP_LT => x < y,
            OP_LE => x <= y,
            _ => false,
        } as i32;
    }
    // Same tag and payload: equal for everything but floats (NaN) and strings (by content).
    let mut x = to_variant(a);
    let y = to_variant(b);
    match op {
        OP_EQ => x.equal(&y),
        OP_NE => x.not_equal(&y),
        OP_GT => x.greater(&y),
        OP_GE => x.greater_equal(&y),
        OP_LT => x.less(&y),
        _ => x.less_equal(&y),
    }
    (!x.is_nil()) as i32
}

#[no_mangle]
pub extern "C" fn aot_neg(a: V) -> V {
    match tag(a) {
        T_INT => mk(T_INT, (val(a) as i32).wrapping_neg() as u32),
        T_FLOAT => mk(T_FLOAT, (-f32::from_bits(val(a))).to_bits()),
        _ => a,
    }
}

// -----------------------------------------------------------------------------------------------
// Garbage collection

fn mark(v: V, objs: &mut Vec<(*mut Obj, bool)>, work: &mut Vec<V>) {
    work.push(v);
    while let Some(v) = work.pop() {
        if tag(v) != T_STR && tag(v) != T_TABLE {
            continue;
        }
        let p = val(v) as usize as *mut Obj;
        // Objects are few: a search is cheaper than a mark bit in each.
        if let Some(e) = objs.iter_mut().find(|e| e.0 == p) {
            if e.1 {
                continue;
            }
            e.1 = true;
            if let Obj::Table(entries) = unsafe { &*p } {
                work.extend(entries.iter().map(|e| e.1));
            }
        }
    }
}

fn collect(tm: &ThreadManager) {
    let Runtime { heap, globals, .. } = rt();
    heap.since_gc = 0;
    if heap.objs.is_empty() {
        return;
    }
    let mut work = Vec::new();
    for &g in globals.iter() {
        mark(g, &mut heap.objs, &mut work);
    }
    mark(unsafe { aot_ret }, &mut heap.objs, &mut work);
    for t in tm.contexts.iter() {
        mark(t.ret, &mut heap.objs, &mut work);
        let mut f = t.top;
        while !f.is_null() {
            for &v in unsafe { (*f).slots() }.iter() {
                mark(v, &mut heap.objs, &mut work);
            }
            f = unsafe { (*f).parent };
        }
    }
    heap.objs.retain_mut(|e| {
        if e.1 {
            e.1 = false;
            true
        } else {
            drop(unsafe { Box::from_raw(e.0) });
            false
        }
    });
}

// -----------------------------------------------------------------------------------------------
// Threads

/// A script thread: its state, as the interpreter's Context has it, and its frames.
#[derive(Debug)]
pub struct AotThread {
    id: u32,
    status: ThreadState,
    wait_ms: u64,
    sleep_ms: u64,
    should_break: bool,
    start_addr: u32,
    /// The running function's frame (its callers through `parent`); null when not started.
    top: *mut Frame,
    /// The thread's return value while it does not run (`aot_ret` while it does).
    ret: V,
}

impl AotThread {
    fn new(id: u32) -> Self {
        Self {
            id,
            status: ThreadState::CONTEXT_STATUS_NONE,
            wait_ms: 0,
            sleep_ms: 0,
            should_break: false,
            start_addr: 0,
            top: ptr::null_mut(),
            ret: 0,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn get_status(&self) -> ThreadState {
        self.status.clone()
    }

    /// Where the thread is: its segment and resume point.
    pub fn get_pc(&self) -> usize {
        if self.top.is_null() {
            return 0;
        }
        unsafe { ((*self.top).seg as usize) << 16 | (*self.top).state as usize }
    }

    pub fn get_waiting_time(&self) -> u64 {
        self.wait_ms
    }

    pub fn get_sleeping_time(&self) -> u64 {
        self.sleep_ms
    }
}

unsafe impl Send for AotThread {}

/// The script threads (the interpreter's ThreadManager, for compiled code).
#[derive(Debug)]
pub struct ThreadManager {
    pub contexts: Vec<AotThread>,
    current_id: u32,
    thread_break: bool,
}

impl Default for ThreadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ThreadManager {
    fn drop(&mut self) {
        for t in self.contexts.iter_mut() {
            unsafe { free_chain(core::mem::replace(&mut t.top, ptr::null_mut())) };
        }
    }
}

impl ThreadManager {
    pub fn new() -> Self {
        Self { contexts: (0..32).map(AotThread::new).collect(), current_id: 0, thread_break: false }
    }

    pub fn get_current_id(&self) -> u32 {
        self.current_id
    }

    pub fn set_current_id(&mut self, id: u32) {
        self.current_id = id;
    }

    pub fn total_contexts(&self) -> usize {
        self.contexts.len()
    }

    pub fn get_context_status(&self, id: u32) -> ThreadState {
        self.contexts[id as usize].status.clone()
    }

    pub fn set_context_status(&mut self, id: u32, status: ThreadState) {
        self.contexts[id as usize].status = status;
    }

    pub fn get_context_waiting_time(&self, id: u32) -> u64 {
        self.contexts[id as usize].wait_ms
    }

    pub fn set_context_waiting_time(&mut self, id: u32, time: u64) {
        self.contexts[id as usize].wait_ms = time;
    }

    pub fn get_context_sleeping_time(&self, id: u32) -> u64 {
        self.contexts[id as usize].sleep_ms
    }

    pub fn set_context_sleeping_time(&mut self, id: u32, time: u64) {
        self.contexts[id as usize].sleep_ms = time;
    }

    pub fn set_context_should_break(&mut self, id: u32, should_break: bool) {
        self.contexts[id as usize].should_break = should_break;
    }

    pub fn get_context_should_break(&self, id: u32) -> bool {
        self.contexts[id as usize].should_break
    }

    pub fn get_should_break(&self) -> bool {
        self.thread_break
    }

    pub fn set_should_break(&mut self, should_break: bool) {
        self.thread_break = should_break;
    }

    /// Replace thread `id` with a new one (None: stopped). The running thread's frames are
    /// freed once its segment returns.
    fn reset(&mut self, id: u32, start: Option<u32>) {
        let t = &mut self.contexts[id as usize];
        let old = core::mem::replace(t, AotThread::new(id));
        if !old.top.is_null() {
            if id == self.current_id && is_ready() && !rt().game.is_null() {
                rt().graveyard.push(old.top);
                unsafe { aot_ret = 0 };
            } else {
                unsafe { free_chain(old.top) };
            }
        }
        let t = &mut self.contexts[id as usize];
        match start {
            Some(addr) => {
                t.start_addr = addr;
                t.status = ThreadState::CONTEXT_STATUS_RUNNING;
                match rt().entries.iter().find(|e| e.0 == addr) {
                    Some(&(_, seg, nslots)) => t.top = Frame::new(seg, nslots, ptr::null_mut()),
                    None => {
                        log::error!("thread_start: no function at {addr}");
                        t.status = ThreadState::CONTEXT_STATUS_NONE;
                    }
                }
            }
            None => {
                t.status = ThreadState::CONTEXT_STATUS_NONE;
                t.should_break = true;
            }
        }
    }

    pub fn thread_start(&mut self, id: u32, addr: u32) {
        if id == 0 {
            self.thread_break = false;
            for i in 0..self.total_contexts() as u32 {
                self.reset(i, None);
            }
        }
        self.reset(id, Some(addr));
    }

    fn current(&mut self) -> &mut AotThread {
        &mut self.contexts[self.current_id as usize]
    }

    pub fn thread_wait(&mut self, time: u32) {
        let t = self.current();
        t.should_break = true;
        t.wait_ms = time as u64;
        t.status = (t.status.clone() | ThreadState::CONTEXT_STATUS_WAIT) & !ThreadState::CONTEXT_STATUS_RUNNING;
    }

    pub fn thread_dissolve_wait(&mut self) {
        let t = self.current();
        t.should_break = true;
        t.status = (t.status.clone() | ThreadState::CONTEXT_STATUS_DISSOLVE_WAIT)
            & !ThreadState::CONTEXT_STATUS_RUNNING;
    }

    pub fn thread_sleep(&mut self, time: u32) {
        let t = self.current();
        t.should_break = true;
        t.sleep_ms = time as u64;
        t.status = (t.status.clone() | ThreadState::CONTEXT_STATUS_SLEEP) & !ThreadState::CONTEXT_STATUS_RUNNING;
    }

    pub fn thread_raise(&mut self, time: u32) {
        for t in self.contexts.iter_mut() {
            if t.status.contains(ThreadState::CONTEXT_STATUS_SLEEP) && t.wait_ms == time as u64 {
                t.status = (t.status.clone() & !ThreadState::CONTEXT_STATUS_SLEEP)
                    | ThreadState::CONTEXT_STATUS_RUNNING;
            }
        }
    }

    pub fn thread_next(&mut self) {
        self.current().should_break = true;
    }

    pub fn thread_exit(&mut self, id: Option<u32>) {
        let id = id.unwrap_or(self.current_id);
        if id == 0 {
            for i in 0..self.total_contexts() as u32 {
                self.reset(i, None);
            }
            self.thread_break = true;
        } else {
            self.reset(id, None);
        }
    }

    pub fn start_main(&mut self, entry_point: u32) {
        self.thread_start(0, entry_point);
    }

    /// The thread requests the syscall just made (as the interpreter's run loop handles them
    /// after each instruction); `aot_yield` tells the compiled code to stop.
    fn after_syscall(&mut self, game: &mut GameData) {
        let tid = self.current_id;
        let mut stop = false;
        while let Some(event) = game.thread_wrapper.pop() {
            match event {
                ThreadRequest::Start(id, addr) => self.thread_start(id, addr),
                ThreadRequest::Wait(time) => {
                    self.thread_wait(time);
                    stop = true;
                }
                ThreadRequest::DissolveWait() => {
                    self.thread_dissolve_wait();
                    stop = true;
                }
                ThreadRequest::Sleep(time) => {
                    self.thread_sleep(time);
                    stop = true;
                }
                ThreadRequest::Raise(time) => {
                    self.thread_raise(time);
                    stop = true;
                }
                ThreadRequest::Next() => {
                    self.thread_next();
                    stop = true;
                }
                ThreadRequest::TextWait(id) => {
                    let st = self.get_context_status(id);
                    self.set_context_status(
                        id,
                        (st | ThreadState::CONTEXT_STATUS_TEXT) & !ThreadState::CONTEXT_STATUS_RUNNING,
                    );
                    stop = true;
                }
                ThreadRequest::TextResume(id) => {
                    let st = self.get_context_status(id);
                    self.set_context_status(
                        id,
                        (st & !ThreadState::CONTEXT_STATUS_TEXT) | ThreadState::CONTEXT_STATUS_RUNNING,
                    );
                }
                ThreadRequest::Exit(id) => {
                    self.thread_exit(id);
                    stop = true;
                }
                ThreadRequest::ShouldBreak() => {
                    self.set_should_break(true);
                    stop = true;
                }
            }
        }
        // A yield ends the thread's turn. A restart of the thread does not (the new one runs
        // on), but the old one's code must stop: its frames are gone.
        if stop {
            self.contexts[tid as usize].should_break = true;
        }
        if stop || !rt().graveyard.is_empty() {
            unsafe { aot_yield = 1 };
        }
    }

    /// Run thread `tid` until it yields or ends (the interpreter's run_one_context).
    pub fn run(&mut self, tid: u32, game: &mut GameData, parser: &Parser) {
        self.current_id = tid;
        game.set_current_thread(tid);
        {
            let rt = rt();
            rt.game = game;
            rt.tm = self;
            rt.parser = parser;
        }
        self.contexts[tid as usize].should_break = false;
        unsafe { aot_ret = self.contexts[tid as usize].ret };
        loop {
            let t = &self.contexts[tid as usize];
            if t.should_break || t.top.is_null() {
                break;
            }
            step(t.top);
            for f in core::mem::take(&mut rt().graveyard) {
                unsafe { free_chain(f) };
            }
        }
        self.contexts[tid as usize].ret = unsafe { aot_ret };
        rt().game = ptr::null_mut();
    }

    pub fn capture_snapshot_v1(&self) -> ThreadManagerSnapshotV1 {
        ThreadManagerSnapshotV1 {
            current_id: self.current_id,
            thread_break: self.thread_break,
            contexts: self.contexts.iter().map(capture_thread).collect(),
        }
    }

    pub fn apply_snapshot_v1(&mut self, snap: &ThreadManagerSnapshotV1) {
        self.current_id = snap.current_id;
        self.thread_break = snap.thread_break;
        for (i, c) in snap.contexts.iter().enumerate().take(self.contexts.len()) {
            self.reset(i as u32, None);
            let t = &mut self.contexts[i];
            t.status = ThreadState::from_bits_truncate(c.state_bits);
            t.wait_ms = c.wait_ms;
            t.sleep_ms = c.sleep_ms;
            t.should_break = c.should_break;
            t.start_addr = c.start_addr;
            t.ret = from_variant(c.return_value.clone());
            t.top = restore_frames(&c.stack);
        }
    }
}

/// A thread as the interpreter's snapshot holds it: its frames, from the first, as three ints
/// (segment, resume point, slot count) and the slots.
fn capture_thread(t: &AotThread) -> ContextSnapshotV1 {
    let mut frames = Vec::new();
    let mut f = t.top;
    while !f.is_null() {
        frames.push(f);
        f = unsafe { (*f).parent };
    }
    let mut stack = Vec::new();
    for &f in frames.iter().rev() {
        let f = unsafe { &mut *f };
        stack.push(Variant::Int(f.seg as i32));
        stack.push(Variant::Int(f.state as i32));
        stack.push(Variant::Int(f.nslots as i32));
        stack.extend(f.slots().iter().map(|&v| to_variant(v)));
    }
    ContextSnapshotV1 {
        id: t.id,
        stack,
        cursor: 0,
        cur_stack_pos: 0,
        cur_stack_base: 0,
        start_addr: t.start_addr,
        return_value: to_variant(t.ret),
        state_bits: t.status.bits(),
        wait_ms: t.wait_ms,
        sleep_ms: t.sleep_ms,
        should_exit: false,
        should_break: t.should_break,
    }
}

fn restore_frames(stack: &[Variant]) -> *mut Frame {
    let mut top: *mut Frame = ptr::null_mut();
    let mut k = 0;
    while k + 3 <= stack.len() {
        let (Some(seg), Some(state), Some(n)) = (stack[k].as_int(), stack[k + 1].as_int(), stack[k + 2].as_int()) else {
            break;
        };
        let f = Frame::new(seg as u16, n as u16, top);
        unsafe { (*f).state = state as u32 };
        k += 3;
        for slot in unsafe { (*f).slots() }.iter_mut() {
            *slot = stack.get(k).cloned().map_or(0, from_variant);
            k += 1;
        }
        top = f;
    }
    top
}

// -----------------------------------------------------------------------------------------------
// Globals (script::global on the PlayStation)

pub fn global(index: usize) -> V {
    rt().globals.get(index).copied().unwrap_or(0)
}

pub fn global_variant(index: usize) -> Variant {
    to_variant(global(index))
}

pub fn set_global_variant(index: usize, value: Variant) {
    let v = from_variant(value);
    if let Some(g) = rt().globals.get_mut(index) {
        *g = v;
    }
}

pub fn global_int(index: usize) -> Option<i32> {
    let v = global(index);
    (tag(v) == T_INT).then_some(val(v) as i32)
}


