use std::fmt::Write as _;
use std::io::{Result as IoResult, Write};

use crate::cfg::{build_cfg, BasicBlock, BlockTerm};
use crate::decode::{Function, Instruction, Op};
use crate::parser::Parser;
use crate::lua_opt::BlockEmitter;

fn func_name(addr: u32) -> String {
    format!("f_{:08X}", addr)
}

fn emit_stack_slot_get(args_count: u8, idx: i8) -> String {
    // This is *frame* stack (args + locals), not the operand stack.
    if idx < 0 {
        let abs = (-idx) as u8 - 2;
        // a_{args_count-abs}
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

fn emit_global(idx: u16) -> String {
	format!("G[{}]", idx)
}

fn emit_global_table(idx: u16) -> String {
	format!("GT[{}]", idx)
}

fn emit_local_table(idx: i8) -> String {
    // idx can be negative in some scripts; keep it stable.
	format!("LT[{}]", idx)
}

fn s(idx: usize) -> String {
	format!("S{}", idx)
}

fn emit_call_args(base: usize, argc: usize) -> String {
    // Caller pushed args in order; call consumes them. The first arg is at base.
    let mut out = String::new();
    for i in 0..argc {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&s(base + i));
    }
    out
}

fn emit_block_header<W: Write>(
    w: &mut W,
    indent: &str,
    block_id: usize,
    start: u32,
    in_depth: usize,
    term: BlockTerm,
    preds: &[usize],
    succs: &[usize],
    is_loop_header: bool,
) -> IoResult<()> {
    let term_s = match term {
        BlockTerm::Fallthrough => "fallthrough",
        BlockTerm::Jmp => "jmp",
        BlockTerm::Jz => "jz",
        BlockTerm::Ret => "ret",
        BlockTerm::RetV => "retv",
    };

    writeln!(
        w,
        "{}-- BB{} @0x{:08X} depth_in={} term={} preds={:?} succs={:?}{}",
        indent,
        block_id,
        start,
        in_depth,
        term_s,
        preds,
        succs,
        if is_loop_header { " loop_header" } else { "" }
    )?;
    Ok(())
}

fn emit_inst<W: Write>(
    w: &mut W,
    indent: &str,
    func: &Function,
    inst: &Instruction,
    sp: &mut usize,
    callee_args: &std::collections::BTreeMap<u32, u8>,
) -> IoResult<()> {
    // The operand stack is represented by locals S0..S{max}. We do *static* stack
    // index rewriting (no runtime push/pop helpers).
    match &inst.op {
        Op::Nop => {}
        Op::InitStack { .. } => {
            // Function prologue marker. No code emission here.
        }

        Op::PushNil => {
            writeln!(w, "{}{} = nil", indent, s(*sp))?;
            *sp += 1;
        }
        Op::PushTrue => {
            writeln!(w, "{}{} = true", indent, s(*sp))?;
            *sp += 1;
        }
        Op::PushI8(v) => {
            writeln!(w, "{}{} = {}", indent, s(*sp), v)?;
            *sp += 1;
        }
        Op::PushI16(v) => {
            writeln!(w, "{}{} = {}", indent, s(*sp), v)?;
            *sp += 1;
        }
        Op::PushI32(v) => {
            writeln!(w, "{}{} = {}", indent, s(*sp), v)?;
            *sp += 1;
        }
        Op::PushF32(v) => {
            // Lua number is double; keep it readable.
            writeln!(w, "{}{} = {}", indent, s(*sp), v)?;
            *sp += 1;
        }
        Op::PushString(s0) => {
            let lit = s0.replace('\\', "\\\\").replace('"', "\\\"");
            writeln!(w, "{}{} = \"{}\"", indent, s(*sp), lit)?;
            *sp += 1;
        }
        Op::PushTop => {
            if *sp == 0 {
                writeln!(w, "{}-- push_top on empty stack", indent)?;
            } else {
                writeln!(w, "{}{} = {}", indent, s(*sp), s(*sp - 1))?;
                *sp += 1;
            }
        }
        Op::PushReturn => {
            writeln!(w, "{}{} = __ret", indent, s(*sp))?;
            *sp += 1;
        }

        Op::PushGlobal(idx) => {
            writeln!(w, "{}{} = {}", indent, s(*sp), emit_global(*idx))?;
            *sp += 1;
        }
        Op::PopGlobal(idx) => {
            if *sp == 0 {
                writeln!(w, "{}-- pop_global with empty stack", indent)?;
            } else {
                *sp -= 1;
                writeln!(w, "{}{} = {}", indent, emit_global(*idx), s(*sp))?;
            }
        }
        Op::PushGlobalTable(idx) => {
            if *sp == 0 {
                writeln!(w, "{}-- push_global_table on empty stack", indent)?;
            } else {
                let k = s(*sp - 1);
                // Direct read: S[top] = GT[idx][S[top]] ; net delta = 0
                writeln!(w, "{}{} = {}[{}]", indent, k, emit_global_table(*idx), k)?;
            }
        }
        Op::PopGlobalTable(idx) => {
            if *sp < 2 {
                writeln!(w, "{}-- pop_global_table on short stack", indent)?;
                *sp = 0;
            } else {
                let v = s(*sp - 1);
                let k = s(*sp - 2);
                // Direct write: GT[idx][key] = value ; net delta = -2
                writeln!(w, "{}{}[{}] = {}", indent, emit_global_table(*idx), k, v)?;
                *sp -= 2;
            }
        }
        Op::PushLocalTable(idx) => {
            if *sp == 0 {
                writeln!(w, "{}-- push_local_table on empty stack", indent)?;
            } else {
                let k = s(*sp - 1);
                writeln!(w, "{}{} = {}[{}]", indent, k, emit_local_table(*idx), k)?;
            }
        }
        Op::PopLocalTable(idx) => {
            if *sp < 2 {
                writeln!(w, "{}-- pop_local_table on short stack", indent)?;
                *sp = 0;
            } else {
                let v = s(*sp - 1);
                let k = s(*sp - 2);
                writeln!(w, "{}{}[{}] = {}", indent, emit_local_table(*idx), k, v)?;
                *sp -= 2;
            }
        }
        Op::PushStack(idx) => {
            let v = emit_stack_slot_get(func.args, *idx);
            writeln!(w, "{}{} = {}", indent, s(*sp), v)?;
            *sp += 1;
        }
        Op::PopStack(idx) => {
            if *sp == 0 {
                writeln!(w, "{}-- pop_stack with empty stack", indent)?;
            } else {
                *sp -= 1;
                writeln!(w, "{}{}", indent, emit_stack_slot_set(func.args, *idx, &s(*sp)))?;
            }
        }

        Op::Neg => {
            if *sp == 0 {
                writeln!(w, "{}-- neg on empty stack", indent)?;
            } else {
                writeln!(w, "{}{} = -{}", indent, s(*sp - 1), s(*sp - 1))?;
            }
        }
        Op::Add => {
            if *sp < 2 {
                writeln!(w, "{}-- add on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = {} + {}", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }
        Op::Sub => {
            if *sp < 2 {
                writeln!(w, "{}-- sub on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = {} - {}", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }
        Op::Mul => {
            if *sp < 2 {
                writeln!(w, "{}-- mul on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = {} * {}", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }
        Op::Div => {
            if *sp < 2 {
                writeln!(w, "{}-- div on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = {} / {}", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }
        Op::Mod => {
            if *sp < 2 {
                writeln!(w, "{}-- mod on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = {} % {}", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }
        Op::BitTest => {
            if *sp < 2 {
                writeln!(w, "{}-- bittest on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = ({} & {}) ~= 0", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }
        Op::And => {
            if *sp < 2 {
                writeln!(w, "{}-- and on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = ({} ~= nil) and ({} ~= nil)", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }
        Op::Or => {
            if *sp < 2 {
                writeln!(w, "{}-- or on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = ({} ~= nil) or ({} ~= nil)", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }
        Op::SetE => {
            if *sp < 2 {
                writeln!(w, "{}-- sete on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = ({} == {})", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }
        Op::SetNE => {
            if *sp < 2 {
                writeln!(w, "{}-- setne on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = ({} ~= {})", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }
        Op::SetG => {
            if *sp < 2 {
                writeln!(w, "{}-- setg on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = ({} > {})", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }
        Op::SetLE => {
            if *sp < 2 {
                writeln!(w, "{}-- setle on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = ({} <= {})", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }
        Op::SetL => {
            if *sp < 2 {
                writeln!(w, "{}-- setl on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = ({} < {})", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }
        Op::SetGE => {
            if *sp < 2 {
                writeln!(w, "{}-- setge on short stack", indent)?;
            } else {
                let a = *sp - 2;
                let b = *sp - 1;
                writeln!(w, "{}{} = ({} >= {})", indent, s(a), s(a), s(b))?;
                *sp -= 1;
            }
        }

        Op::Call { target } => {
            let argc = callee_args.get(target).copied().unwrap_or(0) as usize;
            if *sp < argc {
                writeln!(
                    w,
                    "{}-- call {} with argc={} on short stack",
                    indent,
                    func_name(*target),
                    argc
                )?;
                writeln!(w, "{}__ret = {}()", indent, func_name(*target))?;
                *sp = 0;
            } else {
                let base = *sp - argc;
                let args = emit_call_args(base, argc);
                writeln!(w, "{}__ret = {}({})", indent, func_name(*target), args)?;
                *sp = base;
            }
        }
        Op::Syscall { id, name, args } => {
            let argc = *args as usize;
            if *sp < argc {
                writeln!(
                    w,
                    "{}-- syscall {} (id={}) argc={} on short stack",
                    indent,
                    name,
                    id,
                    argc
                )?;
                writeln!(w, "{}__ret = {}()", indent, name)?;
                *sp = 0;
            } else {
                let base = *sp - argc;
                let args_s = emit_call_args(base, argc);
                writeln!(w, "{}__ret = {}({})", indent, name, args_s)?;
                *sp = base;
            }
        }

        // Control-flow is handled at the basic-block terminator level.
        Op::Jmp { .. } | Op::Jz { .. } | Op::Ret | Op::RetV => {}

        Op::Unknown(opcode) => {
            writeln!(w, "{}-- unknown opcode 0x{:02X}", indent, opcode)?;
        }
    }

    Ok(())
}

fn emit_block_terminator<W: Write>(w: &mut W, indent: &str, b: &BasicBlock, sp: &mut usize) -> IoResult<()> {
    match b.term {
        BlockTerm::Jmp => {
            if let Some(&t) = b.succs.get(0) {
                writeln!(w, "{}__pc = {}", indent, t)?;
            } else {
                writeln!(w, "{}return", indent)?;
            }
        }
        BlockTerm::Jz => {
            if *sp == 0 {
                writeln!(w, "{}-- jz on empty stack", indent)?;
                if let Some(&t) = b.succs.get(0) {
                    writeln!(w, "{}__pc = {}", indent, t)?;
                } else {
                    writeln!(w, "{}return", indent)?;
                }
                return Ok(());
            }

            *sp -= 1;
            let cond = s(*sp);
            let t = b.succs.get(0).copied();
            let f = b.succs.get(1).copied();

            writeln!(w, "{}if {} == 0 then", indent, cond)?;
            match t {
                Some(tid) => writeln!(w, "{}  __pc = {}", indent, tid)?,
                None => writeln!(w, "{}  return", indent)?,
            }
            writeln!(w, "{}else", indent)?;
            match f {
                Some(fid) => writeln!(w, "{}  __pc = {}", indent, fid)?,
                None => writeln!(w, "{}  return", indent)?,
            }
            writeln!(w, "{}end", indent)?;
        }
        BlockTerm::Ret => {
            writeln!(w, "{}return", indent)?;
        }
        BlockTerm::RetV => {
            if *sp == 0 {
                writeln!(w, "{}return nil", indent)?;
            } else {
                *sp -= 1;
                writeln!(w, "{}return {}", indent, s(*sp))?;
            }
        }
        BlockTerm::Fallthrough => {
            if let Some(&t) = b.succs.get(0) {
                writeln!(w, "{}__pc = {}", indent, t)?;
            } else {
                writeln!(w, "{}return", indent)?;
            }
        }
    }

    Ok(())
}

fn emit_function<W: Write>(
    w: &mut W,
    func: &Function,
    callee_args: &std::collections::BTreeMap<u32, u8>,
    is_entry: bool,
) -> IoResult<()> {
    let cfg = build_cfg(func, callee_args);

    // Generate the dispatcher body first, collecting which operand stack slots (S*)
    // are actually needed after expression reconstruction.
    let entry_pc = cfg.blocks.first().map(|b| b.id).unwrap_or(0);
    let mut used_s: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();

    // Frame locals (l*) referenced by PushStack/PopStack are declared; unused ones are omitted.
    let mut used_l: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
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

    let mut body: Vec<u8> = Vec::new();
    writeln!(&mut body, "  local __pc = {}", entry_pc)?;
    writeln!(&mut body, "  while true do")?;

    for (i, b) in cfg.blocks.iter().enumerate() {
        if i == 0 {
            writeln!(&mut body, "    if __pc == {} then", b.id)?;
        } else {
            writeln!(&mut body, "    elseif __pc == {} then", b.id)?;
        }

        let indent_case = "      ";
        emit_block_header(
            &mut body,
            indent_case,
            b.id,
            b.start,
            b.in_depth,
            b.term,
            &b.preds,
            &b.succs,
            b.is_loop_header,
        )?;

        let term_idx = if matches!(b.term, BlockTerm::Jmp | BlockTerm::Jz | BlockTerm::Ret | BlockTerm::RetV) {
            b.inst_indices.clone().last()
        } else {
            None
        };

        let mut be = BlockEmitter::new(indent_case, func.args, callee_args, &mut used_s);
        be.init_stack(b.in_depth);
        for ii in b.inst_indices.clone() {
            if Some(ii) == term_idx {
                continue;
            }
            be.emit_inst(&func.insts[ii]);
        }
        be.emit_terminator(b.term, &b.succs);
        body.extend_from_slice(be.take_output().as_bytes());
    }

    writeln!(&mut body, "    else")?;
    writeln!(&mut body, "      return")?;
    writeln!(&mut body, "    end")?;
    writeln!(&mut body, "  end")?;
    writeln!(&mut body, "end")?;
    writeln!(&mut body)?;

    // Signature
    let mut sig = String::new();
    if is_entry {
        write!(&mut sig, "function entry_point(").ok();
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

    // Locals (only those referenced).
    if !used_l.is_empty() {
        let mut ls = String::new();
        ls.push_str("  local ");
        for (i, lidx) in used_l.iter().enumerate() {
            if i > 0 {
                ls.push_str(", ");
            }
            ls.push_str(&format!("l{}", lidx));
        }
        writeln!(w, "{}", ls)?;
    }

    writeln!(w, "  local __ret = nil")?;

    // Operand stack slots (only those referenced by the optimized body).
    if !used_s.is_empty() {
        let mut ss = String::new();
        ss.push_str("  local ");
        for (i, sidx) in used_s.iter().enumerate() {
            if i > 0 {
                ss.push_str(", ");
            }
            ss.push_str(&s(*sidx));
        }
        writeln!(w, "{}", ss)?;
    }

    w.write_all(&body)?;
    Ok(())
}

pub fn emit_lua_script<W: Write>(w: &mut W, parser: &Parser, functions: &[Function]) -> IoResult<()> {
    // Header
    writeln!(w, "-- Decompiled from HCB bytecode")?;
    writeln!(w, "-- Title: {}", parser.get_title())?;
    writeln!(w, "-- Entry: 0x{:08X}", parser.get_entry_point())?;
    let (sw, sh) = parser.get_screen_size();
    writeln!(w, "-- Screen: {}x{}", sw, sh)?;
    writeln!(w)?;

    // Globals/tables (placeholders for readability; the original runtime is host-provided)
    writeln!(w, "G = {{}}")?;
    // writeln!(w, "GT = GT or {}")?;
    // writeln!(w, "LT = LT or {}")?;
    writeln!(w)?;

    // Callee arg-count map (for CallInst stack consumption)
    let mut callee_args = std::collections::BTreeMap::<u32, u8>::new();
    for f in functions {
        callee_args.insert(f.start_addr, f.args);
    }

    for func in functions {
        if parser.entry_point == func.start_addr {
            emit_function(w, func, &callee_args, true)?;
        } else {
            emit_function(w, func, &callee_args, false)?;
        }
    }

    Ok(())
}
