use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::decode::{Instruction, Function, Op};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockTerm {
    Fallthrough,
    Jmp,
    Jz,
    Ret,
    RetV,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: usize,
    pub start: u32,
    pub end: u32, // exclusive (next leader or function end)
    pub inst_indices: std::ops::Range<usize>,
    pub preds: Vec<usize>,
    pub succs: Vec<usize>,
    pub term: BlockTerm,
    pub in_depth: usize,
    pub out_depth: usize,
    pub is_loop_header: bool,
}

#[derive(Debug, Clone)]
pub struct FunctionCfg {
    pub blocks: Vec<BasicBlock>,
    pub addr_to_block: BTreeMap<u32, usize>,
    pub max_depth: usize,
}

fn inst_stack_delta(inst: &Instruction, callee_args: &BTreeMap<u32, u8>) -> i32 {
    match &inst.op {
        Op::Nop | Op::InitStack { .. } | Op::Jmp { .. } | Op::Ret => 0,
        Op::Jz { .. } => -1,
        Op::RetV => -1,
        Op::PushNil
        | Op::PushTrue
	    | Op::PushI8(_)
	    | Op::PushI16(_)
	    | Op::PushI32(_)
	    | Op::PushF32(_)
	    | Op::PushString(_)
	    | Op::PushGlobal(_)
	    | Op::PushStack(_) => 1,
	    | Op::PushGlobalTable(_)
	    | Op::PushLocalTable(_) => 0,
        | Op::PushTop
        | Op::PushReturn => 1,
	    Op::PopGlobal(_) | Op::PopStack(_) => -1,
        | Op::PopGlobalTable(_) | Op::PopLocalTable(_) => -2,
        Op::Neg => 0,
        Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Mod
        | Op::BitTest
        | Op::And
        | Op::Or
        | Op::SetE
        | Op::SetNE
        | Op::SetG
        | Op::SetLE
        | Op::SetL
        | Op::SetGE => -1,
        Op::Call { target } => -(callee_args.get(target).copied().unwrap_or(0) as i32),
        Op::Syscall { args, .. } => -(*args as i32),
	    Op::Unknown(_) => 0,
    }
}

fn block_term(last: Option<&Instruction>) -> BlockTerm {
    match last.map(|i| &i.op) {
        Some(Op::Jmp { .. }) => BlockTerm::Jmp,
        Some(Op::Jz { .. }) => BlockTerm::Jz,
        Some(Op::Ret) => BlockTerm::Ret,
        Some(Op::RetV) => BlockTerm::RetV,
        _ => BlockTerm::Fallthrough,
    }
}

pub fn build_cfg(func: &Function, callee_args: &BTreeMap<u32, u8>) -> FunctionCfg {
    // Leaders.
    let mut leaders: BTreeSet<u32> = BTreeSet::new();
    if let Some(first) = func.insts.first() {
        leaders.insert(first.addr);
    }

    // Map addr -> inst index.
    let mut addr_to_idx: BTreeMap<u32, usize> = BTreeMap::new();
    for (i, inst) in func.insts.iter().enumerate() {
        addr_to_idx.insert(inst.addr, i);
    }

    for (i, inst) in func.insts.iter().enumerate() {
        match &inst.op {
            Op::Jmp { target } | Op::Jz { target } => {
                leaders.insert(*target);
                if let Some(next) = func.insts.get(i + 1) {
                    leaders.insert(next.addr);
                }
            }
            Op::Ret | Op::RetV => {
                if let Some(next) = func.insts.get(i + 1) {
                    leaders.insert(next.addr);
                }
            }
            _ => {}
        }
    }

    let func_end = func
        .insts
        .last()
        .map(|i| i.addr + 1)
        .unwrap_or(func.start_addr);

    // Build blocks by leader ranges.
    let mut leader_vec: Vec<u32> = leaders.into_iter().collect();
    leader_vec.sort();

    let mut blocks: Vec<BasicBlock> = Vec::new();
    let mut addr_to_block: BTreeMap<u32, usize> = BTreeMap::new();

    for (bid, &start) in leader_vec.iter().enumerate() {
        let end = leader_vec.get(bid + 1).copied().unwrap_or(func_end);
        let si = addr_to_idx.get(&start).copied().unwrap_or(0);
        let ei = addr_to_idx
            .range(end..)
            .next()
            .map(|(_, &idx)| idx)
            .unwrap_or(func.insts.len());

        for (&a, _) in addr_to_idx.range(start..end) {
            addr_to_block.insert(a, bid);
        }

        let last = if ei > 0 { func.insts.get(ei - 1) } else { None };
        blocks.push(BasicBlock {
            id: bid,
            start,
            end,
            inst_indices: si..ei,
            preds: Vec::new(),
            succs: Vec::new(),
            term: block_term(last),
            in_depth: 0,
            out_depth: 0,
            is_loop_header: false,
        });
    }

    // Fill succs.
    for b in &mut blocks {
        let last_idx = b.inst_indices.clone().last();
        let last = last_idx.and_then(|i| func.insts.get(i));
        match last.map(|i| &i.op) {
            Some(Op::Jmp { target }) => {
                if let Some(&tid) = addr_to_block.get(target) {
                    b.succs.push(tid);
                }
            }
            Some(Op::Jz { target }) => {
                if let Some(&tid) = addr_to_block.get(target) {
                    b.succs.push(tid);
                }
                // fallthrough
                let ft = func
                    .insts
                    .get(b.inst_indices.end)
                    .map(|i| i.addr);
                if let Some(ft) = ft {
                    if let Some(&fid) = addr_to_block.get(&ft) {
                        b.succs.push(fid);
                    }
                }
            }
            Some(Op::Ret) | Some(Op::RetV) => {}
            _ => {
                // Fallthrough to next block by address.
                let ft = func
                    .insts
                    .get(b.inst_indices.end)
                    .map(|i| i.addr);
                if let Some(ft) = ft {
                    if let Some(&fid) = addr_to_block.get(&ft) {
                        b.succs.push(fid);
                    }
                }
            }
        }
    }

    // Fill preds.
    for b in 0..blocks.len() {
        let succs = blocks[b].succs.clone();
        for s in succs {
            if let Some(sb) = blocks.get_mut(s) {
                sb.preds.push(b);
            }
        }
    }

    // Compute in/out depths using a simple worklist.
    let mut in_depth: Vec<Option<usize>> = vec![None; blocks.len()];
    if !blocks.is_empty() {
        in_depth[0] = Some(0);
    }
    let mut q: VecDeque<usize> = VecDeque::new();
    if !blocks.is_empty() {
        q.push_back(0);
    }
    let mut max_depth = 0usize;

    while let Some(bid) = q.pop_front() {
        let mut d = in_depth[bid].unwrap_or(0) as i32;
        let b = &blocks[bid];

        for i in b.inst_indices.clone() {
            let inst = &func.insts[i];
            d += inst_stack_delta(inst, callee_args);
            if d < 0 {
                d = 0;
            }
            max_depth = max_depth.max(d as usize);
        }

        let out = d as usize;
        for &sid in &blocks[bid].succs {
            match in_depth[sid] {
                None => {
                    in_depth[sid] = Some(out);
                    q.push_back(sid);
                }
                Some(existing) => {
                    // If mismatched, keep the max to avoid panics; we will still emit
                    // readable code, but the stack-model is approximate.
                    if existing != out {
                        let newd = existing.max(out);
                        if newd != existing {
                            in_depth[sid] = Some(newd);
                            q.push_back(sid);
                        }
                    }
                }
            }
        }
    }

    for b in &mut blocks {
        b.in_depth = in_depth[b.id].unwrap_or(0);
        // Simulate to compute out depth.
        let mut d = b.in_depth as i32;
        for i in b.inst_indices.clone() {
            let inst = &func.insts[i];
            d += inst_stack_delta(inst, callee_args);
            if d < 0 {
                d = 0;
            }
        }
        b.out_depth = d as usize;
    }

    // Loop header classification: any block that is target of a back-edge.
    let mut loop_headers: BTreeSet<usize> = BTreeSet::new();
    for b in &blocks {
        for &s in &b.succs {
            if let (Some(src), Some(dst)) = (blocks.get(b.id), blocks.get(s)) {
                if dst.start < src.start {
                    loop_headers.insert(s);
                }
            }
        }
    }
    for b in &mut blocks {
        b.is_loop_header = loop_headers.contains(&b.id);
    }

    FunctionCfg {
        blocks,
        addr_to_block,
        max_depth,
    }
}
