use std::env;
use std::fmt;
use std::sync::OnceLock;

/// Trace categories, enabled via environment variables.
///
/// Supported:
/// - RFVP_TRACE="vm,syscall,prim,prim_tree,motion,render" (comma/space separated; "all" enables all)
/// - RFVP_TRACE_VM=1, RFVP_TRACE_SYSCALL=1, RFVP_TRACE_PRIM=1, RFVP_TRACE_PRIM_TREE=1,
///   RFVP_TRACE_MOTION=1, RFVP_TRACE_RENDER=1
///
/// Rate limits / caps:
/// - RFVP_TRACE_PRIM_TREE_EVERY (u64, default 60)
/// - RFVP_TRACE_MOTION_EVERY (u64, default 30)
/// - RFVP_TRACE_PRIM_TREE_MAX_NODES (usize, default 256)
/// - RFVP_TRACE_PRIM_TREE_MAX_DEPTH (usize, default 32)
/// - RFVP_TRACE_MOTION_MAX (usize, default 16)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceKind {
    Vm,
    Syscall,
    Prim,
    PrimTree,
    Motion,
    Render,
}

#[derive(Clone, Copy, Debug)]
struct TraceConfig {
    mask: u32,
    prim_tree_every: u64,
    motion_every: u64,
    prim_tree_max_nodes: usize,
    prim_tree_max_depth: usize,
    motion_max: usize,
}

const M_VM: u32 = 1 << 0;
const M_SYSCALL: u32 = 1 << 1;
const M_PRIM: u32 = 1 << 2;
const M_PRIM_TREE: u32 = 1 << 3;
const M_MOTION: u32 = 1 << 4;
const M_RENDER: u32 = 1 << 5;

fn parse_bool_env(name: &str) -> bool {
    match env::var(name) {
        Ok(v) => {
            let s = v.trim().to_ascii_lowercase();
            !(s.is_empty() || s == "0" || s == "false" || s == "no" || s == "off")
        }
        Err(_) => false,
    }
}

fn parse_u64_env(name: &str, default_v: u64) -> u64 {
    env::var(name).ok().and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(default_v)
}

fn parse_usize_env(name: &str, default_v: usize) -> usize {
    env::var(name).ok().and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(default_v)
}

fn parse_mask_from_trace_list(s: &str) -> u32 {
    let mut mask = 0u32;
    for raw in s.split(|c: char| c == ',' || c == ';' || c.is_whitespace()) {
        let t = raw.trim().to_ascii_lowercase();
        if t.is_empty() {
            continue;
        }
        match t.as_str() {
            "all" => {
                mask |= M_VM | M_SYSCALL | M_PRIM | M_PRIM_TREE | M_MOTION | M_RENDER;
            }
            "vm" => mask |= M_VM,
            "syscall" | "sc" => mask |= M_SYSCALL,
            "prim" => mask |= M_PRIM,
            "prim_tree" | "tree" => mask |= M_PRIM_TREE,
            "motion" => mask |= M_MOTION,
            "render" => mask |= M_RENDER,
            _ => {}
        }
    }
    mask
}

fn build_config() -> TraceConfig {
    let mut mask = 0u32;

    if let Ok(list) = env::var("RFVP_TRACE") {
        mask |= parse_mask_from_trace_list(&list);
    }

    if parse_bool_env("RFVP_TRACE_VM") {
        mask |= M_VM;
    }
    if parse_bool_env("RFVP_TRACE_SYSCALL") {
        mask |= M_SYSCALL;
    }
    if parse_bool_env("RFVP_TRACE_PRIM") {
        mask |= M_PRIM;
    }
    if parse_bool_env("RFVP_TRACE_PRIM_TREE") {
        mask |= M_PRIM_TREE;
    }
    if parse_bool_env("RFVP_TRACE_MOTION") {
        mask |= M_MOTION;
    }
    if parse_bool_env("RFVP_TRACE_RENDER") {
        mask |= M_RENDER;
    }

    TraceConfig {
        mask,
        prim_tree_every: parse_u64_env("RFVP_TRACE_PRIM_TREE_EVERY", 60),
        motion_every: parse_u64_env("RFVP_TRACE_MOTION_EVERY", 30),
        prim_tree_max_nodes: parse_usize_env("RFVP_TRACE_PRIM_TREE_MAX_NODES", 256),
        prim_tree_max_depth: parse_usize_env("RFVP_TRACE_PRIM_TREE_MAX_DEPTH", 32),
        motion_max: parse_usize_env("RFVP_TRACE_MOTION_MAX", 16),
    }
}

fn cfg() -> &'static TraceConfig {
    static CFG: OnceLock<TraceConfig> = OnceLock::new();
    CFG.get_or_init(build_config)
}

pub fn enabled(k: TraceKind) -> bool {
    let c = cfg();
    match k {
        TraceKind::Vm => (c.mask & M_VM) != 0,
        TraceKind::Syscall => (c.mask & M_SYSCALL) != 0,
        TraceKind::Prim => (c.mask & M_PRIM) != 0,
        TraceKind::PrimTree => (c.mask & M_PRIM_TREE) != 0,
        TraceKind::Motion => (c.mask & M_MOTION) != 0,
        TraceKind::Render => (c.mask & M_RENDER) != 0,
    }
}

pub fn prim_tree_every() -> u64 {
    cfg().prim_tree_every
}
pub fn motion_every() -> u64 {
    cfg().motion_every
}
pub fn prim_tree_max_nodes() -> usize {
    cfg().prim_tree_max_nodes
}
pub fn prim_tree_max_depth() -> usize {
    cfg().prim_tree_max_depth
}
pub fn motion_max() -> usize {
    cfg().motion_max
}

pub fn should_dump_prim_tree(frame_no: u64) -> bool {
    enabled(TraceKind::PrimTree) && (prim_tree_every() == 0 || frame_no % prim_tree_every() == 0)
}

pub fn should_dump_motion(frame_no: u64) -> bool {
    enabled(TraceKind::Motion) && (motion_every() == 0 || frame_no % motion_every() == 0)
}

pub fn vm(args: fmt::Arguments) {
    if !enabled(TraceKind::Vm) {
        return;
    }
    log::info!("{}", args);
}

pub fn syscall(args: fmt::Arguments) {
    if !enabled(TraceKind::Syscall) {
        return;
    }
    log::info!("{}", args);
}

pub fn prim_evt(args: fmt::Arguments) {
    if !enabled(TraceKind::Prim) {
        return;
    }
    log::info!("{}", args);
}

pub fn motion(args: fmt::Arguments) {
    if !enabled(TraceKind::Motion) {
        return;
    }
    log::info!("{}", args);
}

pub fn render(args: fmt::Arguments) {
    if !enabled(TraceKind::Render) {
        return;
    }
    log::info!("{}", args);
}

pub fn dump(kind: TraceKind, title: &str, body: &str) {
    if !enabled(kind) {
        return;
    }
    log::info!("=== {} ===\n{}\n=== /{} ===", title, body, title);
}
