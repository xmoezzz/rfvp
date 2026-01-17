use std::collections::BTreeMap;
use std::mem::size_of;

use anyhow::{bail, Result};

use crate::opcode::Opcode;
use crate::parser::Parser;

#[derive(Debug, Clone)]
pub enum Op {
    Nop,
    InitStack { args: u8, locals: u8 },
    Call { target: u32 },
    Syscall { id: u16, name: String, args: u8 },
    Ret,
    RetV,
    Jmp { target: u32 },
    Jz { target: u32 },
    PushNil,
    PushTrue,
    PushI32(i32),
    PushI16(i16),
    PushI8(i8),
    PushF32(f32),
    PushString(String),
    PushGlobal(u16),
    PushStack(i8),
    PushGlobalTable(u16),
    PushLocalTable(i8),
    PushTop,
    PushReturn,
    PopGlobal(u16),
    PopStack(i8),
    PopGlobalTable(u16),
    PopLocalTable(i8),
    Neg,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitTest,
    And,
    Or,
    SetE,
    SetNE,
    SetG,
    SetLE,
    SetL,
    SetGE,
    Unknown(u8),
}

#[derive(Debug, Clone)]
pub struct Instruction {
    pub addr: u32,
    pub op: Op,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub start_addr: u32,
    pub args: u8,
    pub locals: u8,
    pub insts: Vec<Instruction>,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub functions: BTreeMap<u32, Function>,
}

fn decode_one(parser: &Parser, pc: usize) -> Result<(Instruction, usize)> {
    let addr = pc as u32;
    let opcode_u8 = parser.read_u8(pc)?;

    let mut cur = pc + 1;
    let op = match opcode_u8.try_into() {
        Ok(Opcode::Nop) => Op::Nop,
        Ok(Opcode::InitStack) => {
            let args = parser.read_i8(cur)? as u8;
            cur += size_of::<i8>();
            let locals = parser.read_i8(cur)? as u8;
            cur += size_of::<i8>();
            Op::InitStack { args, locals }
        }
        Ok(Opcode::Call) => {
            let target = parser.read_u32(cur)?;
            cur += size_of::<u32>();
            Op::Call { target }
        }
        Ok(Opcode::Syscall) => {
            let id = parser.read_u16(cur)?;
            cur += size_of::<u16>();

            let (name, args) = match parser.get_syscall(id) {
                Some(s) => (s.name.clone(), s.args),
                None => (format!("syscall_{}", id), 0),
            };
            Op::Syscall { id, name, args }
        }
        Ok(Opcode::Ret) => Op::Ret,
        Ok(Opcode::RetV) => Op::RetV,
        Ok(Opcode::Jmp) => {
            let target = parser.read_u32(cur)?;
            cur += size_of::<u32>();
            Op::Jmp { target }
        }
        Ok(Opcode::Jz) => {
            let target = parser.read_u32(cur)?;
            cur += size_of::<u32>();
            Op::Jz { target }
        }
        Ok(Opcode::PushNil) => Op::PushNil,
        Ok(Opcode::PushTrue) => Op::PushTrue,
        Ok(Opcode::PushI32) => {
            let v = parser.read_i32(cur)?;
            cur += size_of::<i32>();
            Op::PushI32(v)
        }
        Ok(Opcode::PushI16) => {
            let v = parser.read_i16(cur)?;
            cur += size_of::<i16>();
            Op::PushI16(v)
        }
        Ok(Opcode::PushI8) => {
            let v = parser.read_i8(cur)?;
            cur += size_of::<i8>();
            Op::PushI8(v)
        }
        Ok(Opcode::PushF32) => {
            let v = parser.read_f32(cur)?;
            cur += size_of::<f32>();
            Op::PushF32(v)
        }
        Ok(Opcode::PushString) => {
            let len = parser.read_u8(cur)? as usize;
            cur += size_of::<u8>();
            let s = parser.read_cstring(cur, len)?;
            cur += len;
            Op::PushString(s)
        }
        Ok(Opcode::PushGlobal) => {
            let idx = parser.read_u16(cur)?;
            cur += size_of::<u16>();
            Op::PushGlobal(idx)
        }
        Ok(Opcode::PushStack) => {
            let off = parser.read_i8(cur)?;
            cur += size_of::<i8>();
            Op::PushStack(off)
        }
        Ok(Opcode::PushGlobalTable) => {
            let idx = parser.read_u16(cur)?;
            cur += size_of::<u16>();
            Op::PushGlobalTable(idx)
        }
        Ok(Opcode::PushLocalTable) => {
            let idx = parser.read_i8(cur)?;
            cur += size_of::<i8>();
            Op::PushLocalTable(idx)
        }
        Ok(Opcode::PushTop) => Op::PushTop,
        Ok(Opcode::PushReturn) => Op::PushReturn,
        Ok(Opcode::PopGlobal) => {
            let idx = parser.read_u16(cur)?;
            cur += size_of::<u16>();
            Op::PopGlobal(idx)
        }
        Ok(Opcode::PopStack) => {
            let off = parser.read_i8(cur)?;
            cur += size_of::<i8>();
            Op::PopStack(off)
        }
        Ok(Opcode::PopGlobalTable) => {
            let idx = parser.read_u16(cur)?;
            cur += size_of::<u16>();
            Op::PopGlobalTable(idx)
        }
        Ok(Opcode::PopLocalTable) => {
            let idx = parser.read_i8(cur)?;
            cur += size_of::<i8>();
            Op::PopLocalTable(idx)
        }
        Ok(Opcode::Neg) => Op::Neg,
        Ok(Opcode::Add) => Op::Add,
        Ok(Opcode::Sub) => Op::Sub,
        Ok(Opcode::Mul) => Op::Mul,
        Ok(Opcode::Div) => Op::Div,
        Ok(Opcode::Mod) => Op::Mod,
        Ok(Opcode::BitTest) => Op::BitTest,
        Ok(Opcode::And) => Op::And,
        Ok(Opcode::Or) => Op::Or,
        Ok(Opcode::SetE) => Op::SetE,
        Ok(Opcode::SetNE) => Op::SetNE,
        Ok(Opcode::SetG) => Op::SetG,
        Ok(Opcode::SetLE) => Op::SetLE,
        Ok(Opcode::SetL) => Op::SetL,
        Ok(Opcode::SetGE) => Op::SetGE,
        Err(_) => Op::Unknown(opcode_u8),
    };

    Ok((Instruction { addr, op }, cur))
}

pub fn decode_program(parser: &Parser) -> Result<Program> {
    let end = parser.get_sys_desc_offset() as usize;
    let mut pc = 4usize;

    let mut cur_fn: Option<Function> = None;
    let mut functions: BTreeMap<u32, Function> = BTreeMap::new();

    while pc < end {
        let (inst, next_pc) = decode_one(parser, pc)?;

        match &inst.op {
            Op::InitStack { args, locals } => {
                if let Some(f) = cur_fn.take() {
                    functions.insert(f.start_addr, f);
                }
                cur_fn = Some(Function {
                    start_addr: inst.addr,
                    args: *args,
                    locals: *locals,
                    insts: vec![inst],
                });
            }
            _ => {
                if let Some(f) = cur_fn.as_mut() {
                    f.insts.push(inst);
                } else {
                    // Code before the first InitStack should not happen, but keep it as a stub.
                    cur_fn = Some(Function {
                        start_addr: inst.addr,
                        args: 0,
                        locals: 0,
                        insts: vec![inst],
                    });
                }
            }
        }

        if next_pc <= pc {
            bail!("decoder did not advance at pc=0x{:08X}", pc);
        }
        pc = next_pc;
    }

    if let Some(f) = cur_fn.take() {
        functions.insert(f.start_addr, f);
    }

    Ok(Program { functions })
}

pub fn all_syscalls(program: &Program) -> BTreeMap<String, u8> {
    let mut m = BTreeMap::new();
    for f in program.functions.values() {
        for inst in &f.insts {
            if let Op::Syscall { name, args, .. } = &inst.op {
                m.entry(name.clone()).or_insert(*args);
            }
        }
    }
    m
}

pub fn all_call_targets(program: &Program) -> BTreeMap<u32, ()> {
    let mut m = BTreeMap::new();
    for f in program.functions.values() {
        for inst in &f.insts {
            if let Op::Call { target } = &inst.op {
                m.insert(*target, ());
            }
        }
    }
    m
}

