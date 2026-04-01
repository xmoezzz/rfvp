use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::{Result as IoResult, Write};

use crate::cfg::{build_cfg, BlockTerm, FunctionCfg};
use crate::decode::{Function, Instruction, Op};
use crate::lua_opt::BlockEmitter;
use crate::parser::Parser;

fn func_name(addr: u32) -> String {
    format!("f_{:08X}", addr)
}

fn emit_stack_slot_get(args_count: u8, idx: i8) -> String {
    if idx < 0 {
        let abs = (-idx) as u8 - 2;
        if abs <= args_count {
            let a = (args_count - abs) as usize;
            return format!("a{}", a);
        }
        return format!("a_{}", idx);
    }

    let u = idx as u8;
    if u < args_count {
        format!("a{}", u as usize)
    } else {
        let l = (u - args_count) as usize;
        format!("l{}", l)
    }
}

fn emit_stack_slot_set(args_count: u8, idx: i8, rhs: &str) -> String {
    let lhs = emit_stack_slot_get(args_count, idx);
    format!("{} = {}", lhs, rhs)
}

fn emit_global(idx: u16, non_volatile_count: u16, volatile_count: u16) -> String {
    if idx < non_volatile_count {
        return format!("g{}", idx);
    }

    let vbase = non_volatile_count;
    let vlimit = non_volatile_count.saturating_add(volatile_count);
    if idx >= vbase && idx < vlimit {
        return format!("vg{}", idx - vbase);
    }

    format!("G[{}]", idx)
}

fn emit_global_table(idx: u16) -> String {
    format!("GT[{}]", idx)
}

fn emit_local_table(idx: i8) -> String {
    format!("LT[{}]", idx)
}

fn escape_lua_string(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

fn collect_used_frame_locals(func: &Function) -> BTreeSet<usize> {
    let mut used_l = BTreeSet::new();
    for inst in &func.insts {
        match inst.op {
            Op::PushStack(idx) | Op::PopStack(idx) => {
                if idx >= 0 {
                    let u = idx as u8;
                    if u >= func.args {
                        used_l.insert((u - func.args) as usize);
                    }
                }
            }
            _ => {}
        }
    }
    used_l
}

fn scan_runtime_tables(functions: &[Function]) -> (bool, bool, bool) {
    let mut need_g = false;
    let mut need_gt = false;
    let mut need_lt = false;

    for func in functions {
        for inst in &func.insts {
            match inst.op {
                Op::PushGlobalTable(_) | Op::PopGlobalTable(_) => need_gt = true,
                Op::PushLocalTable(_) | Op::PopLocalTable(_) => need_lt = true,
                _ => {}
            }
        }
    }

    (need_g, need_gt, need_lt)
}

fn uses_fallback_global(functions: &[Function], non_volatile_count: u16, volatile_count: u16) -> bool {
    let limit = non_volatile_count.saturating_add(volatile_count);
    for func in functions {
        for inst in &func.insts {
            match inst.op {
                Op::PushGlobal(idx) | Op::PopGlobal(idx) if idx >= limit => return true,
                _ => {}
            }
        }
    }
    false
}

fn emit_name_decls<W: Write>(w: &mut W, keyword: &str, prefix: &str, count: u16) -> IoResult<()> {
    if count == 0 {
        return Ok(());
    }

    let mut start = 0u16;
    while start < count {
        let end = count.min(start + 16);
        write!(w, "{} ", keyword)?;
        for i in start..end {
            if i > start {
                write!(w, ", ")?;
            }
            write!(w, "{}{}", prefix, i)?;
        }
        writeln!(w)?;
        start = end;
    }
    Ok(())
}

fn emit_push_value<W: Write>(w: &mut W, indent: &str, value: &str) -> IoResult<()> {
    writeln!(w, "{}__sp = __sp + 1", indent)?;
    writeln!(w, "{}__stk[__sp] = {}", indent, value)
}

fn emit_binary_reduce<W: Write>(w: &mut W, indent: &str, expr: &str, op_name: &str) -> IoResult<()> {
    writeln!(w, "{}if __sp < 2 then", indent)?;
    writeln!(w, "{}  -- {} on short stack", indent, op_name)?;
    writeln!(w, "{}else", indent)?;
    writeln!(w, "{}  local __rhs = __stk[__sp]", indent)?;
    writeln!(w, "{}  local __lhs = __stk[__sp - 1]", indent)?;
    writeln!(w, "{}  __stk[__sp] = nil", indent)?;
    writeln!(w, "{}  __sp = __sp - 1", indent)?;
    writeln!(w, "{}  __stk[__sp] = {}", indent, expr)?;
    writeln!(w, "{}end", indent)
}

fn emit_runtime_inst<W: Write>(
    w: &mut W,
    indent: &str,
    func: &Function,
    inst: &Instruction,
    callee_args: &std::collections::BTreeMap<u32, u8>,
    non_volatile_count: u16,
    volatile_count: u16,
) -> IoResult<()> {
    match &inst.op {
        Op::Nop | Op::InitStack { .. } => {}
        Op::PushNil => emit_push_value(w, indent, "nil")?,
        Op::PushTrue => emit_push_value(w, indent, "true")?,
        Op::PushI8(v) => emit_push_value(w, indent, &format!("{}", v))?,
        Op::PushI16(v) => emit_push_value(w, indent, &format!("{}", v))?,
        Op::PushI32(v) => emit_push_value(w, indent, &format!("{}", v))?,
        Op::PushF32(v) => emit_push_value(w, indent, &format!("{}", v))?,
        Op::PushString(s0) => emit_push_value(w, indent, &format!("\"{}\"", escape_lua_string(s0)))?,
        Op::PushTop => {
            writeln!(w, "{}if __sp == 0 then", indent)?;
            writeln!(w, "{}  -- push_top on empty stack", indent)?;
            writeln!(w, "{}  __sp = 1", indent)?;
            writeln!(w, "{}  __stk[1] = nil", indent)?;
            writeln!(w, "{}else", indent)?;
            writeln!(w, "{}  __sp = __sp + 1", indent)?;
            writeln!(w, "{}  __stk[__sp] = __stk[__sp - 1]", indent)?;
            writeln!(w, "{}end", indent)?;
        }
        Op::PushReturn => emit_push_value(w, indent, "__ret")?,
        Op::PushGlobal(idx) => emit_push_value(w, indent, &emit_global(*idx, non_volatile_count, volatile_count))?,
        Op::PopGlobal(idx) => {
            writeln!(w, "{}if __sp == 0 then", indent)?;
            writeln!(w, "{}  -- pop_global with empty stack", indent)?;
            writeln!(w, "{}else", indent)?;
            writeln!(w, "{}  {} = __stk[__sp]", indent, emit_global(*idx, non_volatile_count, volatile_count))?;
            writeln!(w, "{}  __stk[__sp] = nil", indent)?;
            writeln!(w, "{}  __sp = __sp - 1", indent)?;
            writeln!(w, "{}end", indent)?;
        }
        Op::PushGlobalTable(idx) => {
            writeln!(w, "{}if __sp == 0 then", indent)?;
            writeln!(w, "{}  -- push_global_table on empty stack", indent)?;
            writeln!(w, "{}else", indent)?;
            writeln!(w, "{}  __stk[__sp] = {}[__stk[__sp]]", indent, emit_global_table(*idx))?;
            writeln!(w, "{}end", indent)?;
        }
        Op::PopGlobalTable(idx) => {
            writeln!(w, "{}if __sp < 2 then", indent)?;
            writeln!(w, "{}  -- pop_global_table on short stack", indent)?;
            writeln!(w, "{}  __sp = 0", indent)?;
            writeln!(w, "{}else", indent)?;
            writeln!(w, "{}  local __value = __stk[__sp]", indent)?;
            writeln!(w, "{}  __stk[__sp] = nil", indent)?;
            writeln!(w, "{}  __sp = __sp - 1", indent)?;
            writeln!(w, "{}  local __key = __stk[__sp]", indent)?;
            writeln!(w, "{}  {}[__key] = __value", indent, emit_global_table(*idx))?;
            writeln!(w, "{}  __stk[__sp] = nil", indent)?;
            writeln!(w, "{}  __sp = __sp - 1", indent)?;
            writeln!(w, "{}end", indent)?;
        }
        Op::PushLocalTable(idx) => {
            writeln!(w, "{}if __sp == 0 then", indent)?;
            writeln!(w, "{}  -- push_local_table on empty stack", indent)?;
            writeln!(w, "{}else", indent)?;
            writeln!(w, "{}  __stk[__sp] = {}[__stk[__sp]]", indent, emit_local_table(*idx))?;
            writeln!(w, "{}end", indent)?;
        }
        Op::PopLocalTable(idx) => {
            writeln!(w, "{}if __sp < 2 then", indent)?;
            writeln!(w, "{}  -- pop_local_table on short stack", indent)?;
            writeln!(w, "{}  __sp = 0", indent)?;
            writeln!(w, "{}else", indent)?;
            writeln!(w, "{}  local __value = __stk[__sp]", indent)?;
            writeln!(w, "{}  __stk[__sp] = nil", indent)?;
            writeln!(w, "{}  __sp = __sp - 1", indent)?;
            writeln!(w, "{}  local __key = __stk[__sp]", indent)?;
            writeln!(w, "{}  {}[__key] = __value", indent, emit_local_table(*idx))?;
            writeln!(w, "{}  __stk[__sp] = nil", indent)?;
            writeln!(w, "{}  __sp = __sp - 1", indent)?;
            writeln!(w, "{}end", indent)?;
        }
        Op::PushStack(idx) => emit_push_value(w, indent, &emit_stack_slot_get(func.args, *idx))?,
        Op::PopStack(idx) => {
            writeln!(w, "{}if __sp == 0 then", indent)?;
            writeln!(w, "{}  -- pop_stack with empty stack", indent)?;
            writeln!(w, "{}else", indent)?;
            writeln!(w, "{}  {}", indent, emit_stack_slot_set(func.args, *idx, "__stk[__sp]"))?;
            writeln!(w, "{}  __stk[__sp] = nil", indent)?;
            writeln!(w, "{}  __sp = __sp - 1", indent)?;
            writeln!(w, "{}end", indent)?;
        }
        Op::Neg => {
            writeln!(w, "{}if __sp == 0 then", indent)?;
            writeln!(w, "{}  -- neg on empty stack", indent)?;
            writeln!(w, "{}else", indent)?;
            writeln!(w, "{}  __stk[__sp] = -__stk[__sp]", indent)?;
            writeln!(w, "{}end", indent)?;
        }
        Op::Add => emit_binary_reduce(w, indent, "__lhs + __rhs", "add")?,
        Op::Sub => emit_binary_reduce(w, indent, "__lhs - __rhs", "sub")?,
        Op::Mul => emit_binary_reduce(w, indent, "__lhs * __rhs", "mul")?,
        Op::Div => emit_binary_reduce(w, indent, "__lhs / __rhs", "div")?,
        Op::Mod => emit_binary_reduce(w, indent, "__lhs % __rhs", "mod")?,
        Op::BitTest => emit_binary_reduce(w, indent, "(__lhs & __rhs) ~= 0", "bittest")?,
        Op::And => emit_binary_reduce(w, indent, "(__lhs ~= nil) and (__rhs ~= nil)", "and")?,
        Op::Or => emit_binary_reduce(w, indent, "(__lhs ~= nil) or (__rhs ~= nil)", "or")?,
        Op::SetE => emit_binary_reduce(w, indent, "(__lhs == __rhs)", "sete")?,
        Op::SetNE => emit_binary_reduce(w, indent, "(__lhs ~= __rhs)", "setne")?,
        Op::SetG => emit_binary_reduce(w, indent, "(__lhs > __rhs)", "setg")?,
        Op::SetGE => emit_binary_reduce(w, indent, "(__lhs >= __rhs)", "setge")?,
        Op::SetL => emit_binary_reduce(w, indent, "(__lhs < __rhs)", "setl")?,
        Op::SetLE => emit_binary_reduce(w, indent, "(__lhs <= __rhs)", "setle")?,
        Op::Call { target } => {
            let argc = callee_args.get(target).copied().unwrap_or(0) as usize;
            writeln!(w, "{}if __sp < {} then", indent, argc)?;
            writeln!(w, "{}  -- call {} with argc={} on short stack", indent, func_name(*target), argc)?;
            writeln!(w, "{}  __ret = {}()", indent, func_name(*target))?;
            writeln!(w, "{}  __sp = 0", indent)?;
            writeln!(w, "{}else", indent)?;
            let base_expr = if argc == 0 { "__sp + 1".to_string() } else { format!("__sp - {} + 1", argc) };
            writeln!(w, "{}  local __base = {}", indent, base_expr)?;
            let mut args_s = String::new();
            for i in 0..argc {
                if i > 0 { args_s.push_str(", "); }
                write!(&mut args_s, "__stk[__base + {}]", i).ok();
            }
            writeln!(w, "{}  __ret = {}({})", indent, func_name(*target), args_s)?;
            writeln!(w, "{}  for __i = __base, __sp do", indent)?;
            writeln!(w, "{}    __stk[__i] = nil", indent)?;
            writeln!(w, "{}  end", indent)?;
            writeln!(w, "{}  __sp = __base - 1", indent)?;
            writeln!(w, "{}end", indent)?;
        }
        Op::Syscall { id, name, args } => {
            let argc = *args as usize;
            writeln!(w, "{}if __sp < {} then", indent, argc)?;
            writeln!(w, "{}  -- syscall {} (id={}) argc={} on short stack", indent, name, id, argc)?;
            writeln!(w, "{}  __ret = {}()", indent, name)?;
            writeln!(w, "{}  __sp = 0", indent)?;
            writeln!(w, "{}else", indent)?;
            let base_expr = if argc == 0 { "__sp + 1".to_string() } else { format!("__sp - {} + 1", argc) };
            writeln!(w, "{}  local __base = {}", indent, base_expr)?;
            let mut args_s = String::new();
            for i in 0..argc {
                if i > 0 { args_s.push_str(", "); }
                write!(&mut args_s, "__stk[__base + {}]", i).ok();
            }
            writeln!(w, "{}  __ret = {}({})", indent, name, args_s)?;
            writeln!(w, "{}  for __i = __base, __sp do", indent)?;
            writeln!(w, "{}    __stk[__i] = nil", indent)?;
            writeln!(w, "{}  end", indent)?;
            writeln!(w, "{}  __sp = __base - 1", indent)?;
            writeln!(w, "{}end", indent)?;
        }
        Op::Jmp { .. } | Op::Jz { .. } | Op::Ret | Op::RetV => {}
        Op::Unknown(opcode) => writeln!(w, "{}-- unknown opcode 0x{:02X}", indent, opcode)?,
    }
    Ok(())
}

fn emit_runtime_terminator<W: Write>(w: &mut W, indent: &str, term: BlockTerm, succs: &[usize]) -> IoResult<()> {
    match term {
        BlockTerm::Jmp | BlockTerm::Fallthrough => {
            if let Some(&t) = succs.get(0) {
                writeln!(w, "{}__pc = {}", indent, t)?;
            } else {
                writeln!(w, "{}return", indent)?;
            }
        }
        BlockTerm::Jz => {
            writeln!(w, "{}if __sp == 0 then", indent)?;
            writeln!(w, "{}  -- jz on empty stack", indent)?;
            if let Some(&t) = succs.get(0) {
                writeln!(w, "{}  __pc = {}", indent, t)?;
            } else {
                writeln!(w, "{}  return", indent)?;
            }
            writeln!(w, "{}else", indent)?;
            writeln!(w, "{}  local __cond = __stk[__sp]", indent)?;
            writeln!(w, "{}  __stk[__sp] = nil", indent)?;
            writeln!(w, "{}  __sp = __sp - 1", indent)?;
            writeln!(w, "{}  if __cond == 0 then", indent)?;
            match succs.get(0).copied() {
                Some(tid) => writeln!(w, "{}    __pc = {}", indent, tid)?,
                None => writeln!(w, "{}    return", indent)?,
            }
            writeln!(w, "{}  else", indent)?;
            match succs.get(1).copied() {
                Some(fid) => writeln!(w, "{}    __pc = {}", indent, fid)?,
                None => writeln!(w, "{}    return", indent)?,
            }
            writeln!(w, "{}  end", indent)?;
            writeln!(w, "{}end", indent)?;
        }
        BlockTerm::Ret => writeln!(w, "{}return", indent)?,
        BlockTerm::RetV => {
            writeln!(w, "{}if __sp == 0 then", indent)?;
            writeln!(w, "{}  return nil", indent)?;
            writeln!(w, "{}else", indent)?;
            writeln!(w, "{}  return __stk[__sp]", indent)?;
            writeln!(w, "{}end", indent)?;
        }
    }
    Ok(())
}

fn is_linear_cfg(cfg: &FunctionCfg) -> bool {
    if cfg.blocks.is_empty() {
        return true;
    }

    for (i, b) in cfg.blocks.iter().enumerate() {
        match b.term {
            BlockTerm::Jz => return false,
            BlockTerm::Ret | BlockTerm::RetV => {
                if i + 1 != cfg.blocks.len() {
                    return false;
                }
            }
            BlockTerm::Jmp | BlockTerm::Fallthrough => {
                if i + 1 == cfg.blocks.len() {
                    if !b.succs.is_empty() {
                        return false;
                    }
                } else if b.succs.as_slice() != [i + 1] {
                    return false;
                }
            }
        }
    }

    true
}

fn can_chain_to(cfg: &FunctionCfg, bid: usize) -> Option<usize> {
    let b = &cfg.blocks[bid];
    if !matches!(b.term, BlockTerm::Jmp | BlockTerm::Fallthrough) {
        return None;
    }
    if b.succs.len() != 1 {
        return None;
    }
    let sid = b.succs[0];
    if sid <= bid {
        return None;
    }
    let succ = &cfg.blocks[sid];
    if succ.preds.as_slice() != [bid] {
        return None;
    }
    Some(sid)
}

fn build_block_chains(cfg: &FunctionCfg) -> Vec<Vec<usize>> {
    let n = cfg.blocks.len();
    let mut is_cont = vec![false; n];
    for bid in 0..n {
        if let Some(sid) = can_chain_to(cfg, bid) {
            is_cont[sid] = true;
        }
    }

    let mut seen = vec![false; n];
    let mut chains = Vec::new();

    for head in 0..n {
        if is_cont[head] || seen[head] {
            continue;
        }
        let mut chain = vec![head];
        seen[head] = true;
        let mut cur = head;
        while let Some(next) = can_chain_to(cfg, cur) {
            if seen[next] {
                break;
            }
            chain.push(next);
            seen[next] = true;
            cur = next;
        }
        chains.push(chain);
    }

    for bid in 0..n {
        if !seen[bid] {
            chains.push(vec![bid]);
        }
    }

    chains.sort_by_key(|chain| chain[0]);
    chains
}

fn emit_optimized_chain(
    out: &mut Vec<u8>,
    func: &Function,
    cfg: &FunctionCfg,
    chain: &[usize],
    callee_args: &std::collections::BTreeMap<u32, u8>,
    used_s: &mut BTreeSet<usize>,
    non_volatile_count: u16,
    volatile_count: u16,
    indent: &str,
) {
    if chain.is_empty() {
        return;
    }

    let first = &cfg.blocks[chain[0]];
    let mut be = BlockEmitter::new(
        indent,
        func.args,
        callee_args,
        used_s,
        non_volatile_count,
        volatile_count,
    );
    be.init_stack(first.in_depth);

    for (pos, &bid) in chain.iter().enumerate() {
        let b = &cfg.blocks[bid];
        let term_idx = if matches!(b.term, BlockTerm::Jmp | BlockTerm::Jz | BlockTerm::Ret | BlockTerm::RetV) {
            b.inst_indices.clone().last()
        } else {
            None
        };

        for ii in b.inst_indices.clone() {
            if Some(ii) == term_idx {
                continue;
            }
            be.emit_inst(&func.insts[ii]);
        }

        if pos + 1 == chain.len() {
            be.emit_terminator(b.term, &b.succs);
        }
    }

    out.extend_from_slice(be.take_output().as_bytes());
}

fn emit_optimized_linear_body<W: Write>(
    w: &mut W,
    func: &Function,
    cfg: &FunctionCfg,
    callee_args: &std::collections::BTreeMap<u32, u8>,
    used_s: &mut BTreeSet<usize>,
    non_volatile_count: u16,
    volatile_count: u16,
) -> IoResult<()> {
    let chain = build_block_chains(cfg);
    let mut body = Vec::new();
    for part in &chain {
        emit_optimized_chain(
            &mut body,
            func,
            cfg,
            part,
            callee_args,
            used_s,
            non_volatile_count,
            volatile_count,
            "  ",
        );
    }
    w.write_all(&body)?;
    writeln!(w, "end")?;
    writeln!(w)?;
    Ok(())
}

fn emit_optimized_dispatcher_body<W: Write>(
    w: &mut W,
    func: &Function,
    cfg: &FunctionCfg,
    callee_args: &std::collections::BTreeMap<u32, u8>,
    used_s: &mut BTreeSet<usize>,
    non_volatile_count: u16,
    volatile_count: u16,
) -> IoResult<()> {
    let entry_pc = cfg.blocks.first().map(|b| b.id).unwrap_or(0);
    let chains = build_block_chains(cfg);
    let mut body: Vec<u8> = Vec::new();

    writeln!(&mut body, "  local __pc = {}", entry_pc)?;
    writeln!(&mut body, "  while true do")?;

    for (i, chain) in chains.iter().enumerate() {
        let head = chain[0];
        if i == 0 {
            writeln!(&mut body, "    if __pc == {} then", head)?;
        } else {
            writeln!(&mut body, "    elseif __pc == {} then", head)?;
        }
        emit_optimized_chain(
            &mut body,
            func,
            cfg,
            chain,
            callee_args,
            used_s,
            non_volatile_count,
            volatile_count,
            "      ",
        );
    }

    writeln!(&mut body, "    else")?;
    writeln!(&mut body, "      return")?;
    writeln!(&mut body, "    end")?;
    writeln!(&mut body, "  end")?;
    writeln!(&mut body, "end")?;
    writeln!(&mut body)?;
    w.write_all(&body)?;
    Ok(())
}

fn emit_runtime_chain<W: Write>(
    w: &mut W,
    func: &Function,
    cfg: &FunctionCfg,
    chain: &[usize],
    callee_args: &std::collections::BTreeMap<u32, u8>,
    non_volatile_count: u16,
    volatile_count: u16,
    indent: &str,
) -> IoResult<()> {
    for (pos, &bid) in chain.iter().enumerate() {
        let b = &cfg.blocks[bid];
        let term_idx = if matches!(b.term, BlockTerm::Jmp | BlockTerm::Jz | BlockTerm::Ret | BlockTerm::RetV) {
            b.inst_indices.clone().last()
        } else {
            None
        };

        for ii in b.inst_indices.clone() {
            if Some(ii) == term_idx {
                continue;
            }
            emit_runtime_inst(
                w,
                indent,
                func,
                &func.insts[ii],
                callee_args,
                non_volatile_count,
                volatile_count,
            )?;
        }

        if pos + 1 == chain.len() {
            emit_runtime_terminator(w, indent, b.term, &b.succs)?;
        }
    }
    Ok(())
}

fn emit_runtime_linear_body<W: Write>(
    w: &mut W,
    func: &Function,
    cfg: &FunctionCfg,
    callee_args: &std::collections::BTreeMap<u32, u8>,
    non_volatile_count: u16,
    volatile_count: u16,
) -> IoResult<()> {
    let chains = build_block_chains(cfg);
    for chain in &chains {
        emit_runtime_chain(
            w,
            func,
            cfg,
            chain,
            callee_args,
            non_volatile_count,
            volatile_count,
            "  ",
        )?;
    }
    writeln!(w, "end")?;
    writeln!(w)?;
    Ok(())
}

fn emit_runtime_dispatcher_body<W: Write>(
    w: &mut W,
    func: &Function,
    cfg: &FunctionCfg,
    callee_args: &std::collections::BTreeMap<u32, u8>,
    non_volatile_count: u16,
    volatile_count: u16,
) -> IoResult<()> {
    let entry_pc = cfg.blocks.first().map(|b| b.id).unwrap_or(0);
    let chains = build_block_chains(cfg);
    writeln!(w, "  local __pc = {}", entry_pc)?;
    writeln!(w, "  while true do")?;

    for (i, chain) in chains.iter().enumerate() {
        let head = chain[0];
        if i == 0 {
            writeln!(w, "    if __pc == {} then", head)?;
        } else {
            writeln!(w, "    elseif __pc == {} then", head)?;
        }
        emit_runtime_chain(
            w,
            func,
            cfg,
            chain,
            callee_args,
            non_volatile_count,
            volatile_count,
            "      ",
        )?;
    }

    writeln!(w, "    else")?;
    writeln!(w, "      return")?;
    writeln!(w, "    end")?;
    writeln!(w, "  end")?;
    writeln!(w, "end")?;
    writeln!(w)?;
    Ok(())
}

fn emit_function<W: Write>(
    w: &mut W,
    func: &Function,
    callee_args: &std::collections::BTreeMap<u32, u8>,
    is_entry: bool,
    non_volatile_count: u16,
    volatile_count: u16,
) -> IoResult<()> {
    let cfg = build_cfg(func, callee_args);
    let used_l = collect_used_frame_locals(func);

    let mut sig = String::new();
    if is_entry {
        write!(&mut sig, "function main(").ok();
    } else {
        write!(&mut sig, "function {}(", func_name(func.start_addr)).ok();
    }
    for i in 0..(func.args as usize) {
        if i > 0 {
            sig.push_str(", ");
        }
        sig.push_str(&format!("a{}", i));
    }
    sig.push(')');
    writeln!(w, "{}", sig)?;

    if !used_l.is_empty() {
        write!(w, "  local ")?;
        for (i, lidx) in used_l.iter().enumerate() {
            if i > 0 {
                write!(w, ", ")?;
            }
            write!(w, "l{}", lidx)?;
        }
        writeln!(w)?;
    }

    writeln!(w, "  local __ret = nil")?;

    if cfg.stack_consistent {
        let mut used_s: BTreeSet<usize> = BTreeSet::new();
        let mut body = Vec::new();
        if is_linear_cfg(&cfg) {
            emit_optimized_linear_body(
                &mut body,
                func,
                &cfg,
                callee_args,
                &mut used_s,
                non_volatile_count,
                volatile_count,
            )?;
        } else {
            emit_optimized_dispatcher_body(
                &mut body,
                func,
                &cfg,
                callee_args,
                &mut used_s,
                non_volatile_count,
                volatile_count,
            )?;
        }

        if !used_s.is_empty() {
            write!(w, "  local ")?;
            for (i, sidx) in used_s.iter().enumerate() {
                if i > 0 {
                    write!(w, ", ")?;
                }
                write!(w, "S{}", sidx)?;
            }
            writeln!(w)?;
        }

        w.write_all(&body)?;
    } else {
        writeln!(w, "  local __stk = {{}}")?;
        writeln!(w, "  local __sp = 0")?;
        if is_linear_cfg(&cfg) {
            emit_runtime_linear_body(
                w,
                func,
                &cfg,
                callee_args,
                non_volatile_count,
                volatile_count,
            )?;
        } else {
            emit_runtime_dispatcher_body(
                w,
                func,
                &cfg,
                callee_args,
                non_volatile_count,
                volatile_count,
            )?;
        }
    }

    Ok(())
}

pub fn emit_lua_script<W: Write>(w: &mut W, parser: &Parser, functions: &[Function]) -> IoResult<()> {
    writeln!(w, "-- Decompiled from HCB bytecode")?;
    writeln!(w, "-- Title: {}", parser.get_title())?;
    let (sw, sh) = parser.get_screen_size();
    writeln!(w, "-- Screen: {}x{}", sw, sh)?;
    writeln!(w)?;

    let non_volatile_count = parser.get_non_volatile_global_count();
    let volatile_count = parser.get_volatile_global_count();
    emit_name_decls(w, "global", "g", non_volatile_count)?;
    emit_name_decls(w, "volatile global", "vg", volatile_count)?;

    let (mut need_g, need_gt, need_lt) = scan_runtime_tables(functions);
    need_g |= uses_fallback_global(functions, non_volatile_count, volatile_count);
    if need_g {
        writeln!(w, "G = G or {{}}")?;
    }
    if need_gt {
        writeln!(w, "GT = GT or {{}}")?;
    }
    if need_lt {
        writeln!(w, "LT = LT or {{}}")?;
    }
    if non_volatile_count > 0 || volatile_count > 0 || need_g || need_gt || need_lt {
        writeln!(w)?;
    }

    let mut callee_args = std::collections::BTreeMap::<u32, u8>::new();
    for f in functions {
        callee_args.insert(f.start_addr, f.args);
    }

    for func in functions {
        emit_function(
            w,
            func,
            &callee_args,
            parser.get_entry_point() == func.start_addr,
            non_volatile_count,
            volatile_count,
        )?;
    }

    Ok(())
}
