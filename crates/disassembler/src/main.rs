use anyhow::{bail, Result};
use clap::Parser as ClapParser;
use serde::{Deserialize, Serialize};
use rfvp::script::inst::nop::NopInst;
use std::mem::size_of;
use std::path::{PathBuf, Path};
use rfvp::script::inst::*;
use rfvp::script::opcode::*;
use rfvp::script::parser::{Nls, Parser};

use std::io::Write;

#[derive(Debug, Serialize, Deserialize)]
pub struct Function {
    address: u32,
    args_count: u8,
    locals_count: u8,
    insts: Vec<Inst>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Inst {
    address: u32,
    mnemonic: String,
    operands: Vec<String>,
}

impl Inst {
    pub fn from_nop(inst: NopInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_init_stack(inst: InitStackInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_arg_count().to_string(), inst.get_local_count().to_string()],
        }
    }

    pub fn from_call(inst: CallInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_target().to_string()],
        }
    }

    pub fn from_syscall(inst: SyscallInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_syscall_name().to_string()],
        }
    }

    pub fn from_ret(inst: RetInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_ret_value(inst: RetValueInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_jmp(inst: JmpInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_target().to_string()],
        }
    }

    pub fn from_jz(inst: JzInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_target().to_string()],
        }
    }

    pub fn from_push_nil(inst: PushNilInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_push_true(inst: PushTrueInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_push_i32(inst: PushI32Inst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_value().to_string()],
        }
    }

    pub fn from_push_i16(inst: PushI16Inst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_value().to_string()],
        }
    }

    pub fn from_push_i8(inst: PushI8Inst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_value().to_string()],
        }
    }

    pub fn from_push_f32(inst: PushF32Inst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_value().to_string()],
        }
    }

    pub fn from_push_string(inst: PushStringInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_value().to_string()],
        }
    }

    pub fn from_push_global(inst: PushGlobalInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_idx().to_string()],
        }
    }

    pub fn from_push_stack(inst: PushStackInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_idx().to_string()],
        }
    }

    pub fn from_push_global_table(inst: PushGlobalTableInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_idx().to_string()],
        }
    }

    pub fn from_push_local_table(inst: PushLocalTableInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_idx().to_string()],
        }
    }

    pub fn from_push_top(inst: PushTopInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_push_return(inst: PushReturnInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }
    
    pub fn from_pop_global(inst: PopGlobalInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_idx().to_string()],
        }
    }

    pub fn from_pop_stack(inst: PopStackInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_idx().to_string()],
        }
    }

    pub fn from_pop_global_table(inst: PopGlobalTableInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_idx().to_string()],
        }
    }

    pub fn from_pop_local_table(inst: PopLocalTableInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: vec![inst.get_idx().to_string()],
        }
    }

    pub fn from_neg(inst: NegInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_add(inst: AddInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_sub(inst: SubInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_mul(inst: MulInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_div(inst: DivInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_mod(inst: ModInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_bittest(inst: BitTestInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_and(inst: AndInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_or(inst: OrInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_sete(inst: SeteInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_setne(inst: SetneInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_setg(inst: SetgInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_setle(inst: SetleInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_setl(inst: SetlInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

    pub fn from_setge(inst: SetgeInst) -> Self {
        Self {
            address: inst.address(),
            mnemonic: inst.opcode().to_string(),
            operands: Vec::new(),
        }
    }

}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyscallEntry {
    id: u32,
    name: String,
    args_count: u8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectConfig {
    entry_point: u32,
    non_volatile_global_count: u16,
    volatile_global_count: u16,
    game_mode: u16,
    game_title: String,
    syscalls: Vec<SyscallEntry>,
    custom_syscall_count: u16,
}

pub struct Disassembler {
    parser: Parser,
    cursor: usize,
    functions: Vec<Function>,
}

impl Disassembler {
    pub fn new(path: impl AsRef<Path>, nls: Nls) -> Result<Self> {
        let parser = Parser::new(path, nls.clone())?;
        Ok(Self {
            parser,
            cursor: 4,
            functions: Vec::new(),
        })
    }

    pub fn get_parser(&self) -> &Parser {
        &self.parser
    }

    pub fn get_pc(&self) -> usize {
        self.cursor
    }

    /// 0x00 nop instruction
    /// nop, no operation
    pub fn nop(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let inst = NopInst::new(addr);
        let inst = Inst::from_nop(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x01 init stack instruction
    /// initialize the local routine stack, as well as
    /// the post-phase of perforimg call instruction or launching a new routine
    pub fn init_stack(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        // how many arguments are passed to the routine
        let args_count = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();

        // how many locals are declared in the routine
        let locals_count = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();

        self.functions.push(Function {
            address: addr,
            args_count: args_count as u8,
            locals_count: locals_count as u8,
            insts: Vec::new(),
        });

        let inst = InitStackInst::new(addr, args_count as u8, locals_count as u8);
        let inst = Inst::from_init_stack(inst);
        self.functions.last_mut().unwrap().insts.push(inst);
        
        Ok(())
    }


    /// 0x02 call instruction
    /// call a routine
    pub fn call(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let target = parser.read_u32(self.cursor)?;
        self.cursor += size_of::<u32>();

        let inst = CallInst::new(addr, target);
        let inst = Inst::from_call(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x03 syscall
    /// call a system call
    pub fn syscall(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let id = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();

        if let Some(syscall) = parser.get_syscall(id) {
            let inst = SyscallInst::new(addr, syscall.name.clone());
            let inst = Inst::from_syscall(inst);
            self.functions.last_mut().unwrap().insts.push(inst);

        } else {
            bail!("syscall not found: {}", id);
        }

        Ok(())
    }

    /// 0x04 ret instruction
    /// return from a routine
    pub fn ret(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = RetInst::new(addr);
        let inst = Inst::from_ret(inst);
        self.functions.last_mut().unwrap().insts.push(inst);
        
        Ok(())
    }

    /// 0x05 retv instruction
    /// return from a routine with a value
    pub fn retv(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = RetValueInst::new(addr);
        let inst = Inst::from_ret_value(inst);
        self.functions.last_mut().unwrap().insts.push(inst);
        
        Ok(())
    }

    /// 0x06 jmp instruction
    /// jump to the address
    pub fn jmp(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let target = parser.read_u32(self.cursor)?;
        self.cursor += size_of::<u32>();

        let inst = JmpInst::new(addr, target);
        let inst = Inst::from_jmp(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x07 jz instruction
    /// jump to the address if the top of the stack is zero
    pub fn jz(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let target = parser.read_u32(self.cursor)?;
        self.cursor += size_of::<u32>();

        let inst = JzInst::new(addr, target);
        let inst = Inst::from_jz(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x08 push nil
    /// push a nil value onto the stack
    pub fn push_nil(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = PushNilInst::new(addr);
        let inst = Inst::from_push_nil(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x09 push true
    /// push a true value onto the stack
    pub fn push_true(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = PushTrueInst::new(addr);
        let inst = Inst::from_push_true(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x0A push i32
    /// push an i32 value onto the stack
    pub fn push_i32(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let value = parser.read_i32(self.cursor)?;
        self.cursor += size_of::<i32>();

        let inst = PushI32Inst::new(addr, value);
        let inst = Inst::from_push_i32(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x0B push i16
    /// push an i16 value onto the stack
    pub fn push_i16(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let value = parser.read_i16(self.cursor)?;
        self.cursor += size_of::<i16>();

        let inst = PushI16Inst::new(addr, value);
        let inst = Inst::from_push_i16(inst);
        self.functions.last_mut().unwrap().insts.push(inst);
        
        Ok(())
    }

    /// 0x0C push i8
    /// push an i8 value onto the stack
    pub fn push_i8(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let value = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();

        let inst = PushI8Inst::new(addr, value);
        let inst = Inst::from_push_i8(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x0D push f32
    /// push an f32 value onto the stack
    pub fn push_f32(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let value = parser.read_f32(self.cursor)?;
        self.cursor += size_of::<f32>();

        let inst = PushF32Inst::new(addr, value);
        let inst = Inst::from_push_f32(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x0E push string
    /// push a string onto the stack
    pub fn push_string(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let len = parser.read_u8(self.cursor)? as usize;
        self.cursor += size_of::<u8>();

        let s = parser.read_cstring(self.cursor, len)?;
        self.cursor += len;

        let inst = PushStringInst::new(addr, s);
        let inst = Inst::from_push_string(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x0F push global
    /// push a global variable onto the stack
    pub fn push_global(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let key = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();

        let inst = PushGlobalInst::new(addr, key as u32);
        let inst = Inst::from_push_global(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x10 push stack
    /// push a stack variable onto the stack
    pub fn push_stack(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let offset = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();

        let inst = PushStackInst::new(addr, offset);
        let inst = Inst::from_push_stack(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x11 push global table
    /// push a value than stored in the global table by immediate key onto the stack
    /// we assume that if any failure occurs, such as the key not found, 
    /// we will push a nil value onto the stack for compatibility reasons.
    pub fn push_global_table(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let key = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();

        let inst = PushGlobalTableInst::new(addr, key as u32);
        let inst = Inst::from_push_global_table(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x12 push local table
    /// push a value than stored in the local table by key onto the stack
    pub fn push_local_table(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let idx = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();

        let inst = PushLocalTableInst::new(addr, idx);
        let inst = Inst::from_push_local_table(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x13 push top
    /// push the top of the stack onto the stack
    pub fn push_top(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = PushTopInst::new(addr);
        let inst = Inst::from_push_top(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x14 push return value
    /// push the return value onto the stack
    pub fn push_return_value(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = PushReturnInst::new(addr);
        let inst = Inst::from_push_return(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x15 pop global
    /// pop the top of the stack and store it in the global table
    pub fn pop_global(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let key = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();

        let inst = PopGlobalInst::new(addr, key as u32);
        let inst = Inst::from_pop_global(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x16 local copy
    /// copy the top of the stack to the local variable
    pub fn local_copy(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let idx = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();

        let inst = PopStackInst::new(addr, idx);
        let inst = Inst::from_pop_stack(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x17 pop global table
    /// pop the top of the stack and store it in the global table by key
    pub fn pop_global_table(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let key = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();

        let inst = PopGlobalTableInst::new(addr, key as u32);
        let inst = Inst::from_pop_global_table(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x18 pop local table 
    /// pop the top of the stack and store it in the local table by key
    pub fn pop_local_table(&mut self, parser: &mut Parser) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;
        let idx = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();

        let inst = PopLocalTableInst::new(addr, idx);
        let inst = Inst::from_pop_local_table(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x19 neg 
    /// negate the top of the stack, only works for integers and floats
    pub fn neg(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = NegInst::new(addr);
        let inst = Inst::from_neg(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x1A add
    /// add the top two values on the stack
    pub fn add(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = AddInst::new(addr);
        let inst = Inst::from_add(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x1B sub
    /// subtract the top two values on the stack
    pub fn sub(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = SubInst::new(addr);
        let inst = Inst::from_sub(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x1C mul
    /// multiply the top two values on the stack
    pub fn mul(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = MulInst::new(addr);
        let inst = Inst::from_mul(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x1D div
    /// divide the top two values on the stack
    pub fn div(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = DivInst::new(addr);
        let inst = Inst::from_div(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x1E modulo
    /// modulo the top two values on the stack
    pub fn modulo(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = ModInst::new(addr);
        let inst = Inst::from_mod(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x1F bittest
    /// test with the top two values on the stack
    pub fn bittest(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = BitTestInst::new(addr);
        let inst = Inst::from_bittest(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x20 and
    /// push true if both the top two values on the stack are none-nil
    pub fn and(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = AndInst::new(addr);
        let inst = Inst::from_and(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x21 or
    /// push true if either of the top two values on the stack is none-nil
    pub fn or(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = OrInst::new(addr);
        let inst = Inst::from_or(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x22 sete
    /// set the top of the stack to true if the top two values on the stack are equal
    pub fn sete(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = SeteInst::new(addr);
        let inst = Inst::from_sete(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x23 setne
    /// set the top of the stack to true if the top two values on the stack are not equal
    pub fn setne(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = SetneInst::new(addr);
        let inst = Inst::from_setne(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x24 setg
    /// set the top of the stack to true if the top two values on the stack are greater
    pub fn setg(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = SetgInst::new(addr);
        let inst = Inst::from_setg(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x25 setle
    /// set the top of the stack to true if the top two values on the stack are less or equal
    pub fn setle(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = SetleInst::new(addr);
        let inst = Inst::from_setle(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x26 setl
    /// set the top of the stack to true if the top two values on the stack are less
    pub fn setl(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = SetlInst::new(addr);
        let inst = Inst::from_setl(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    /// 0x27 setge
    /// set the top of the stack to true if the top two values on the stack are greater or equal
    pub fn setge(&mut self) -> Result<()> {
        let addr = self.get_pc() as u32;
        self.cursor += 1;

        let inst = SetgeInst::new(addr);
        let inst = Inst::from_setge(inst);
        self.functions.last_mut().unwrap().insts.push(inst);

        Ok(())
    }

    fn disassemble_opcode(&mut self, parser: &mut Parser) -> Result<()> {
        let opcode = parser.read_u8(self.get_pc())? as i32;
        
        match opcode.try_into() {
            Ok(Opcode::Nop) => {
                self.nop()?;
            }
            Ok(Opcode::InitStack) => {
                self.init_stack(parser)?;
            }
            Ok(Opcode::Call) => {
                self.call(parser)?;
            }
            Ok(Opcode::Syscall) => {
                self.syscall(parser)?;
            }
            Ok(Opcode::Ret) => {
                self.ret()?;
            }
            Ok(Opcode::RetV) => {
                self.retv()?;
            }
            Ok(Opcode::Jmp) => {
                self.jmp(parser)?;
            }
            Ok(Opcode::Jz) => {
                self.jz(parser)?;
            }
            Ok(Opcode::PushNil) => {
                self.push_nil()?;
            }
            Ok(Opcode::PushTrue) => {
                self.push_true()?;
            }
            Ok(Opcode::PushI32) => {
                self.push_i32(parser)?;
            }
            Ok(Opcode::PushI16) => {
                self.push_i16(parser)?;
            }
            Ok(Opcode::PushI8) => {
                self.push_i8(parser)?;
            }
            Ok(Opcode::PushF32) => {
                self.push_f32(parser)?;
            }
            Ok(Opcode::PushString) => {
                self.push_string(parser)?;
            }
            Ok(Opcode::PushGlobal) => {
                self.push_global(parser)?;
            }
            Ok(Opcode::PushStack) => {
                self.push_stack(parser)?;
            }
            Ok(Opcode::PushGlobalTable) => {
                self.push_global_table(parser)?;
            }
            Ok(Opcode::PushLocalTable) => {
                self.push_local_table(parser)?;
            }
            Ok(Opcode::PushTop) => {
                self.push_top()?;
            }
            Ok(Opcode::PushReturn) => {
                self.push_return_value()?;
            }
            Ok(Opcode::PopGlobal) => {
                self.pop_global(parser)?;
            }
            Ok(Opcode::PopStack) => {
                self.local_copy(parser)?;
            }
            Ok(Opcode::PopGlobalTable) => {
                self.pop_global_table(parser)?;
            }
            Ok(Opcode::PopLocalTable) => {
                self.pop_local_table(parser)?;
            }
            Ok(Opcode::Neg) => {
                self.neg()?;
            }
            Ok(Opcode::Add) => {
                self.add()?;
            }
            Ok(Opcode::Sub) => {
                self.sub()?;
            }
            Ok(Opcode::Mul) => {
                self.mul()?;
            }
            Ok(Opcode::Div) => {
                self.div()?;
            }
            Ok(Opcode::Mod) => {
                self.modulo()?;
            }
            Ok(Opcode::BitTest) => {
                self.bittest()?;
            }
            Ok(Opcode::And) => {
                self.and()?;
            }
            Ok(Opcode::Or) => {
                self.or()?;
            }
            Ok(Opcode::SetE) => {
                self.sete()?;
            }
            Ok(Opcode::SetNE) => {
                self.setne()?;
            }
            Ok(Opcode::SetG) => {
                self.setg()?;
            }
            Ok(Opcode::SetLE) => {
                self.setle()?;
            }
            Ok(Opcode::SetL) => {
                self.setl()?;
            }
            Ok(Opcode::SetGE) => {
                self.setge()?;
            }
            _ => {
                self.nop()?;
                log::error!("unknown opcode: {}", opcode);
            }
        };

        Ok(())
    }

    pub fn disassemble(&mut self) -> Result<()> {
        let mut parser = self.parser.clone();
        while self.get_pc() < parser.get_sys_desc_offset() as usize {
            self.disassemble_opcode(&mut parser)?;
        }

        Ok(())
    }

    pub fn write_insts(&self, path: impl AsRef<Path>) -> Result<()> {
        // create a new directory
        let output = path.as_ref();
        if !output.exists() {
            std::fs::create_dir_all(output)?;
        }

        let disassembly_path = output.join("disassembly.yaml");
        let mut writer = std::fs::File::create(disassembly_path)?;
        serde_yaml::to_writer(&mut writer, &self.functions)?;

        let config = ProjectConfig {
            entry_point: self.get_parser().get_entry_point(),
            non_volatile_global_count: self.get_parser().get_non_volatile_global_count(),
            volatile_global_count: self.get_parser().get_volatile_global_count(),
            game_mode: self.get_parser().get_game_mode(),
            game_title: self.get_parser().get_title(),
            syscalls: self.get_parser().get_all_syscalls().iter().map(|(id, sys)| {
                SyscallEntry {
                    id: *id as u32,
                    name: sys.name.clone(),
                    args_count: sys.args,
                }
            }).collect(),
            custom_syscall_count: self.get_parser().get_custom_syscall_count(),
        };

        let yaml_config = output.join("config.yaml");
        let mut writer = std::fs::File::create(yaml_config)?;
        serde_yaml::to_writer(&mut writer, &config)?;

        let project = FVPProject {
            config_file: PathBuf::from("config.yaml"),
            disassembly_file: PathBuf::from("disassembly.yaml"),
        };

        let toml_project = output.join("project.toml");
        let mut writer = std::fs::File::create(toml_project)?;
        let serialized_string = toml::to_string_pretty(&project)?;
        writer.write_all(serialized_string.as_bytes())?;

        Ok(())
    }
}


#[derive(Debug, Serialize, Deserialize)]
pub struct FVPProject {
    config_file: PathBuf,
    disassembly_file: PathBuf,
}

/// Simple program to greet a person
#[derive(ClapParser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, required = true)]
    input: PathBuf,

    #[arg(short, long, required = true)]
    output: PathBuf,

    #[arg(short, long, default_value = "sjis")]
    lang: Nls,
}



fn main() -> Result<()> {
    let args = Args::parse();
    let mut disassembler = Disassembler::new(args.input, args.lang)?;
    disassembler.disassemble()?;
    disassembler.write_insts(args.output)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disassembler() -> Result<()> {
        let input = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase/Snow.hcb"));
        let output = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testcase/Snow"));
        let mut disassembler = Disassembler::new(input, Nls::ShiftJIS)?;
        disassembler.disassemble()?;
        disassembler.write_insts(output)?;

        Ok(())
    }
}