use std::collections::HashMap;
use std::convert::TryFrom;
use anyhow::Result;

use super::{context::Context, global::Global, parser::Parser, VmSyscall};

pub struct Scheduler {
    pub queue: HashMap<u32, Context>,
    pub parser: Parser,
    pub global: Global,
}

enum Opcode {
    Nop = 0,
    InitStack = 1,
    Call = 2,
    Syscall,
    Ret,
    RetV,
    Jmp,
    Jz,
    PushNil,
    PushTrue,
    PushI32,
    PushI16,
    PushI8,
    PushF32,
    PushString,
    PushGlobal,
    PushStack,
    PushGlobalTable,
    PushLocalTable,
    PushTop,
    PushReturn,
    PopGlobal,
    PopStack,
    PopGlobalTable,
    PopLocalTable,
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
}

impl TryFrom<i32> for Opcode {
    type Error = ();

    fn try_from(v: i32) -> core::result::Result<Self, Self::Error> {
        match v {
            x if x == Opcode::Nop as i32 => Ok(Opcode::Nop),
            x if x == Opcode::InitStack as i32 => Ok(Opcode::InitStack),
            x if x == Opcode::Call as i32 => Ok(Opcode::Call),
            x if x == Opcode::Syscall as i32 => Ok(Opcode::Syscall),
            x if x == Opcode::Ret as i32 => Ok(Opcode::Ret),
            x if x == Opcode::RetV as i32 => Ok(Opcode::RetV),
            x if x == Opcode::Jmp as i32 => Ok(Opcode::Jmp),
            x if x == Opcode::Jz as i32 => Ok(Opcode::Jz),
            x if x == Opcode::PushNil as i32 => Ok(Opcode::PushNil),
            x if x == Opcode::PushTrue as i32 => Ok(Opcode::PushTrue),
            x if x == Opcode::PushI32 as i32 => Ok(Opcode::PushI32),
            x if x == Opcode::PushI16 as i32 => Ok(Opcode::PushI16),
            x if x == Opcode::PushI8 as i32 => Ok(Opcode::PushI8),
            x if x == Opcode::PushF32 as i32 => Ok(Opcode::PushF32),
            x if x == Opcode::PushString as i32 => Ok(Opcode::PushString),
            x if x == Opcode::PushGlobal as i32 => Ok(Opcode::PushGlobal),
            x if x == Opcode::PushStack as i32 => Ok(Opcode::PushStack),
            x if x == Opcode::PushGlobalTable as i32 => Ok(Opcode::PushGlobalTable),
            x if x == Opcode::PushLocalTable as i32 => Ok(Opcode::PushLocalTable),
            x if x == Opcode::PushTop as i32 => Ok(Opcode::PushTop),
            x if x == Opcode::PushReturn as i32 => Ok(Opcode::PushReturn),
            x if x == Opcode::PopGlobal as i32 => Ok(Opcode::PopGlobal),
            x if x == Opcode::PopStack as i32 => Ok(Opcode::PopStack),
            x if x == Opcode::PopGlobalTable as i32 => Ok(Opcode::PopGlobalTable),
            x if x == Opcode::PopLocalTable as i32 => Ok(Opcode::PopLocalTable),
            x if x == Opcode::Neg as i32 => Ok(Opcode::Neg),
            x if x == Opcode::Add as i32 => Ok(Opcode::Add),
            x if x == Opcode::Sub as i32 => Ok(Opcode::Sub),
            x if x == Opcode::Mul as i32 => Ok(Opcode::Mul),
            x if x == Opcode::Div as i32 => Ok(Opcode::Div),
            x if x == Opcode::Mod as i32 => Ok(Opcode::Mod),
            x if x == Opcode::BitTest as i32 => Ok(Opcode::BitTest),
            x if x == Opcode::And as i32 => Ok(Opcode::And),
            x if x == Opcode::Or as i32 => Ok(Opcode::Or),
            x if x == Opcode::SetE as i32 => Ok(Opcode::SetE),
            x if x == Opcode::SetNE as i32 => Ok(Opcode::SetNE),
            x if x == Opcode::SetG as i32 => Ok(Opcode::SetG),
            x if x == Opcode::SetLE as i32 => Ok(Opcode::SetLE),
            x if x == Opcode::SetL as i32 => Ok(Opcode::SetL),
            x if x == Opcode::SetGE as i32 => Ok(Opcode::SetGE),
            _ => Err(()),
        }
    }
}

impl Scheduler {
    pub fn new(parser: Parser, global: Global) -> Self {
        Scheduler {
            queue: HashMap::new(),
            parser,
            global,
        }
    }

    pub fn add(&mut self, id: u32, context: Context) {
        self.queue.insert(id, context);
    }

    pub fn remove(&mut self, id: u32) {
        self.queue.remove(&id);
    }

    pub fn get(&self, id: u32) -> Option<&Context> {
        self.queue.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Context> {
        self.queue.get_mut(&id)
    }

    #[inline]
    fn dispatch_opcode(&mut self, syscaller: impl VmSyscall, context: &mut Context) -> Result<()> {
        let opcode = self.parser.read_u8(context.cursor)? as i32;
        match opcode.try_into() {
            Ok(Opcode::Nop) => {
                context.nop()?;
            }
            Ok(Opcode::InitStack) => {
                context.init_stack(&mut self.parser)?;
            }
            Ok(Opcode::Call) => {
                context.call(&mut self.parser)?;
            }
            Ok(Opcode::Syscall) => {
                context.syscall(syscaller,&mut self.parser)?;
            }
            Ok(Opcode::Ret) => {
                context.ret()?;
            }
            Ok(Opcode::RetV) => {
                context.retv()?;
            }
            Ok(Opcode::Jmp) => {
                context.jmp(&mut self.parser)?;
            }
            Ok(Opcode::Jz) => {
                context.jz(&mut self.parser)?;
            }
            Ok(Opcode::PushNil) => {
                context.push_nil()?;
            }
            Ok(Opcode::PushTrue) => {
                context.push_true()?;
            }
            Ok(Opcode::PushI32) => {
                context.push_i32(&mut self.parser)?;
            }
            Ok(Opcode::PushI16) => {
                context.push_i16(&mut self.parser)?;
            }
            Ok(Opcode::PushI8) => {
                context.push_i8(&mut self.parser)?;
            }
            Ok(Opcode::PushF32) => {
                context.push_f32(&mut self.parser)?;
            }
            Ok(Opcode::PushString) => {
                context.push_string(&mut self.parser)?;
            }
            Ok(Opcode::PushGlobal) => {
                context.push_global(&mut self.parser, &mut self.global)?;
            }
            Ok(Opcode::PushStack) => {
                context.push_stack(&mut self.parser)?;
            }
            Ok(Opcode::PushGlobalTable) => {
                context.push_global_table(&mut self.parser, &mut self.global)?;
            }
            Ok(Opcode::PushLocalTable) => {
                context.push_local_table(&mut self.parser)?;
            }
            Ok(Opcode::PushTop) => {
                context.push_top()?;
            }
            Ok(Opcode::PushReturn) => {
                context.push_return_value()?;
            }
            Ok(Opcode::PopGlobal) => {
                context.pop_global(&mut self.parser, &mut self.global)?;
            }
            Ok(Opcode::PopStack) => {
                context.local_copy(&mut self.parser)?;
            }
            Ok(Opcode::PopGlobalTable) => {
                context.pop_global_table(&mut self.parser, &mut self.global)?;
            }
            Ok(Opcode::PopLocalTable) => {
                context.pop_local_table(&mut self.parser)?;
            }
            Ok(Opcode::Neg) => {
                context.neg()?;
            }
            Ok(Opcode::Add) => {
                context.add()?;
            }
            Ok(Opcode::Sub) => {
                context.sub()?;
            }
            Ok(Opcode::Mul) => {
                context.mul()?;
            }
            Ok(Opcode::Div) => {
                context.div()?;
            }
            Ok(Opcode::Mod) => {
                context.modulo()?;
            }
            Ok(Opcode::BitTest) => {
                context.bittest()?;
            }
            Ok(Opcode::And) => {
                context.and()?;
            }
            Ok(Opcode::Or) => {
                context.or()?;
            }
            Ok(Opcode::SetE) => {
                context.sete()?;
            }
            Ok(Opcode::SetNE) => {
                context.setne()?;
            }
            Ok(Opcode::SetG) => {
                context.setg()?;
            }
            Ok(Opcode::SetLE) => {
                context.setle()?;
            }
            Ok(Opcode::SetL) => {
                context.setl()?;
            }
            Ok(Opcode::SetGE) => {
                context.setge()?;
            }
            _ => {
                context.nop()?;
                log::error!("unknown opcode: {}", opcode);
            }
        };

        Ok(())
    }

    pub fn execute(&mut self, _syscall: impl VmSyscall) -> Result<()> {
        loop {
            // in the original implementation, context will less than 32
            // 0 is the main context
            for i in 0..32 {
                let cur_context = self.get_mut(i);
                if let Some(_context) = cur_context {
                    
                }
            }
        }
    }
}