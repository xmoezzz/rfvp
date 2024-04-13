
use std::mem::size_of;

use crate::script::global::GLOBAL;
use crate::script::parser::Parser;
use crate::script::Variant;
use crate::script::VmSyscall;
use crate::script::opcode::Opcode;

use anyhow::{bail, Result};

static MAX_STACK_SIZE: usize = 0x100;

#[derive(Debug, Clone, Default)]
pub struct StackFrame {
    pub args_count: u16,
    pub locals_count: u16,
}

/// implementation of the virtual machine
/// stack layout:
/// |-----------------|
/// | arg(n)          | <- ...
/// | ...             |
/// | arg(0)          | <- -2 (0xfe in hex)
/// | SavedFrameInfo  | <- -1, includes the base/current pointer and the return address
/// |-----------------|
/// | local(0)        | <- cur_stack_base
///
#[derive(Debug, Clone)]
pub struct Context {
    /// the context id
    id: u64,
    stack: Vec<Variant>,
    cursor: usize,
    /// absolute position of the current stack pointer
    /// start from 0 if the context is just created
    cur_stack_pos: usize,
    /// relative to the base pointer of the current stack frame
    /// start from 0
    cur_stack_base: usize,
    start_addr: u32,
    return_value: Variant,
    state: u32,
    wait_ms: u64,
    should_exit: bool,
    should_break: bool,
}

pub const CONTEXT_STATUS_NONE: u32 = 0;
pub const CONTEXT_STATUS_RUNNING: u32 = 1;
pub const CONTEXT_STATUS_WAIT: u32 = 2;
pub const CONTEXT_STATUS_SLEEP: u32 = 4;
pub const CONTEXT_STATUS_DISSOLVE_WAIT: u32 = 16;

impl Context {
    pub fn new(start_addr: u32) -> Self {
        let mut ctx = Context {
            id: 0,
            stack: vec![Variant::Nil; MAX_STACK_SIZE],
            cursor: start_addr as usize,
            cur_stack_pos: 0,
            cur_stack_base: 0,
            start_addr,
            return_value: Variant::Nil,
            state: CONTEXT_STATUS_NONE,
            wait_ms: 0,
            should_exit: false,
            should_break: false,
        };

        // the initial stack frame
        ctx.push(Variant::SavedStackInfo(
            super::SavedStackInfo { 
                stack_base: 0, 
                stack_pos: 0, 
                return_addr: 0,
                args: 0,
            }
        )).unwrap();

        ctx.cur_stack_base = ctx.cur_stack_pos;
        ctx.cur_stack_pos = 0;

        ctx
    }

    pub fn set_should_break(&mut self, should_break: bool) {
        self.should_break = should_break;
    }

    pub fn should_break(&self) -> bool {
        self.should_break
    }

    fn to_global_offset(&self) -> Result<usize> {
        let base = self.cur_stack_base as isize;
        let base = match base.checked_add(self.cur_stack_pos as isize) {
            Some(base) => base,
            None => bail!("stack pointer out of bounds"),
        };

        if base.is_negative() {
            bail!("stack position is negative");
        }

        Ok(base as usize)
    }

    /// push a value onto the stack and increment the stack pointer
    fn push(&mut self, value: Variant) -> Result<()> {
        let pos = self.to_global_offset();
        if let Ok(pos) = pos {
            if pos >= self.stack.len() {
                bail!("push: stack is unable to grow to the position: {}", pos);
            } else {
                self.stack[pos] = value;
            }
        }

        self.cur_stack_pos += 1;

        Ok(())
    }

    fn pop(&mut self) -> Result<Variant> {
        if self.cur_stack_pos == 0 {
            bail!("no top of the stack")
        }

        let pos = self.to_global_offset();
        let result = if let Ok(mut pos) = pos {
            // be aware of the offset, we should always decrement the position first
            pos -= 1;
            if pos >= self.stack.len() {
                let msg = format!("pop: stack pointer out of bounds: {:x}", self.cursor);
                bail!(msg);
            }
            let r = self.stack[pos].clone();
            self.stack[pos].set_nil();
            r
        }
        else {
            bail!("stack pointer out of bounds");
        };

        self.cur_stack_pos -= 1;
        Ok(result)
    }

    fn top(&mut self) -> Result<Variant> {
        if self.cur_stack_pos == 0 {
            bail!("no top of the stack")
        }
        self.get_local(self.cur_stack_pos as i8)
    }

    fn get_local(&self, offset: i8) -> Result<Variant> {
        let base = self.cur_stack_base as isize;
        let off = match base.checked_add(offset as isize) {
            Some(off) => off,
            None => bail!("stack pointer out of bounds"),
        };
        // off += 1;

        if off < 0 {
            bail!("stack pointer is negative");
        }

        if off > MAX_STACK_SIZE as isize {
            bail!("stack pointer out of bounds");
        }

        let var = self
            .stack
            .get(off as usize)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("stack pointer out of bounds"))?;

        Ok(var)
    }

    fn get_local_mut(&mut self, offset: i8) -> Result<&mut Variant> {
        let base = self.cur_stack_base as isize;
        let mut off = match base.checked_add(offset as isize) {
            Some(off) => off,
            None => bail!("stack pointer out of bounds"),
        };
        // off += 1;

        if off < 0 {
            bail!("stack pointer is negative");
        }

        if off > MAX_STACK_SIZE as isize {
            bail!("stack pointer out of bounds");
        }

        let var = self
            .stack
            .get_mut(off as usize)
            .ok_or_else(|| anyhow::anyhow!("stack pointer out of bounds"))?;

        Ok(var)
    }

    fn set_local(&mut self, offset: i8, value: Variant) -> Result<()> {
        let base = self.cur_stack_base as isize;
        let off = match base.checked_add(offset as isize) {
            Some(off) => off,
            None => bail!("stack pointer out of bounds"),
        };
        // off += 1;

        if off < 0 {
            bail!("stack pointer is negative");
        }

        if off > MAX_STACK_SIZE as isize {
            bail!("stack pointer out of bounds");
        }

        self.stack[off as usize] = value;

        Ok(())
    }

    fn print_stack(&self) {
        log::error!("thread id : {}", self.id);
        log::error!("pc: {:x}", self.cursor);
        if let Ok(offset) = self.to_global_offset() {
            let slice = &self.stack[0..offset + 1];
            log::error!("stack: {:?}", slice);
        }
    }

    /// 0x00 nop instruction
    /// nop, no operation
    pub fn nop(&mut self) -> Result<()> {
        self.cursor += 1;
        Ok(())
    }

    /// 0x01 init stack instruction
    /// initialize the local routine stack, as well as
    /// the post-phase of perforimg call instruction or launching a new routine
    pub fn init_stack(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;

        // how many arguments are passed to the routine
        let args_count = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();
        if args_count < 0 {
            bail!("args count is negative");
        }

        // how many locals are declared in the routine
        let locals_count = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();
        if locals_count < 0 {
            bail!("locals count is negative");
        }

        log::info!("init_stack: args: {} locals: {}", args_count, locals_count);

        let frame = self.get_local_mut(-1)?;
        if let Some(frame) = frame.as_saved_stack_info_mut() {
            frame.args = args_count as usize;
        } else {
            self.print_stack();
            bail!("init_stack: invalid stack frame");
        }

        for _ in 0..locals_count {
            // we must allocate the space for the locals
            self.push(Variant::Nil)?;
        }
        
        Ok(())
    }


    /// 0x02 call instruction
    /// call a routine
    pub fn call(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let addr = parser.read_u32(self.cursor)?;
        self.cursor += size_of::<u32>();
        if !parser.is_code_area(addr) {
            bail!("call: address is not in the code area");
        }

        log::info!("call: {:x}", addr);

        let frame = Variant::SavedStackInfo(
            super::SavedStackInfo { 
                stack_base: self.cur_stack_base, 
                stack_pos: self.cur_stack_pos, 
                return_addr: self.cursor,
                args: 0, // the field will be updated in the init_stack instruction
            }
        );

        self.push(frame)?;

        self.cur_stack_base += self.cur_stack_pos;
        self.cur_stack_pos = 0;
        // update the program counter
        self.cursor = addr as usize;

        Ok(())
    }

    /// 0x03 syscall
    /// call a system call
    pub fn syscall(&mut self, sys: &mut impl VmSyscall, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let id = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();

        if let Some(syscall) = parser.get_syscall(id) {
            let mut args = Vec::new();
            for _ in 0..syscall.args {
                args.push(self.pop()?);
            }

            // reverse the arguments
            args.reverse();

            log::info!("syscall: {} {:?}", &syscall.name, &args);
            let result = sys.do_syscall(syscall.name.as_str(), args)?;
            self.return_value = result;
        } else {
            bail!("syscall not found: {}", id);
        }

        Ok(())
    }

    /// 0x04 ret instruction
    /// return from a routine
    pub fn ret(&mut self) -> Result<()> {
        self.cursor += 1;
        self.return_value = Variant::Nil;
        let frame = self.get_local(-1)?;
        if let Some(frame) = frame.as_saved_stack_info() {
            self.cur_stack_pos = frame.stack_pos;
            self.cur_stack_base = frame.stack_base;
            self.cursor = frame.return_addr;

            // pop the arguments
            for _ in 0..frame.args {
                self.pop()?;
            }
        } else {
            self.print_stack();
            bail!("ret: invalid stack frame: {:?}", &frame);
        }
        Ok(())
    }

    /// 0x05 retv instruction
    /// return from a routine with a value
    pub fn retv(&mut self) -> Result<()> {
        self.cursor += 1;
        self.return_value = self.pop()?;
        let frame = self.get_local(-1)?;
        if let Some(frame) = frame.as_saved_stack_info() {
            self.cur_stack_pos = frame.stack_pos;
            self.cur_stack_base = frame.stack_base;
            self.cursor = frame.return_addr;

            // pop the arguments
            for _ in 0..frame.args {
                self.pop()?;
            }
        } else {
            self.print_stack();
            bail!("retv: invalid stack frame");
        }
        Ok(())
    }

    /// 0x06 jmp instruction
    /// jump to the address
    pub fn jmp(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let addr = parser.read_u32(self.cursor)?;
        self.cursor += size_of::<u32>();
        log::info!("jmp: {:x}", addr);

        self.cursor = addr as usize;
        Ok(())
    }

    /// 0x07 jz instruction
    /// jump to the address if the top of the stack is zero
    pub fn jz(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let addr = parser.read_u32(self.cursor)?;
        self.cursor += size_of::<u32>();

        let top = self.pop()?;
        log::info!("jz: {:?}", &top);

        if !top.canbe_true() {
            self.cursor = addr as usize;
        }
        Ok(())
    }

    /// 0x08 push nil
    /// push a nil value onto the stack
    pub fn push_nil(&mut self) -> Result<()> {
        self.cursor += 1;
        self.push(Variant::Nil)?;

        log::info!("push_nil");
        Ok(())
    }

    /// 0x09 push true
    /// push a true value onto the stack
    pub fn push_true(&mut self) -> Result<()> {
        self.cursor += 1;
        self.push(Variant::True)?;

        log::info!("push_true");
        Ok(())
    }

    /// 0x0A push i32
    /// push an i32 value onto the stack
    pub fn push_i32(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let value = parser.read_i32(self.cursor)?;
        self.cursor += size_of::<i32>();

        log::info!("push_i32: {}", value);

        self.push(Variant::Int(value))?;
        Ok(())
    }

    /// 0x0B push i16
    /// push an i16 value onto the stack
    pub fn push_i16(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let value = parser.read_i16(self.cursor)?;
        self.cursor += size_of::<i16>();

        log::info!("push_i16: {}", value);

        self.push(Variant::Int(value as i32))?;
        Ok(())
    }

    /// 0x0C push i8
    /// push an i8 value onto the stack
    pub fn push_i8(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let value = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();

        log::info!("push_i8: {}", value);

        self.push(Variant::Int(value as i32))?;
        Ok(())
    }

    /// 0x0D push f32
    /// push an f32 value onto the stack
    pub fn push_f32(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let value = parser.read_f32(self.cursor)?;
        self.cursor += size_of::<f32>();

        log::info!("push_f32: {}", value);

        self.push(Variant::Float(value))?;
        Ok(())
    }

    /// 0x0E push string
    /// push a string onto the stack
    pub fn push_string(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let len = parser.read_u8(self.cursor)? as usize;
        self.cursor += size_of::<u8>();

        let s = parser.read_cstring(self.cursor, len)?;
        self.cursor += len;

        log::info!("push_string: {}", &s);

        self.push(Variant::String(s))?;
        Ok(())
    }

    /// 0x0F push global
    /// push a global variable onto the stack
    pub fn push_global(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let key = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();

        log::info!("push_global: {:x}", key);

        if let Some(value) = GLOBAL.lock().unwrap().get(key) {
            self.push(value.clone())?;
            log::info!("global: {:?}", &value);
        } else {
            bail!("global variable not found");
        }
        Ok(())
    }

    /// 0x10 push stack
    /// push a stack variable onto the stack
    pub fn push_stack(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let offset = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();

        let local = self.get_local(offset)?;
        log::info!("push stack: {} {:?}", offset, &local);
        self.push(local)?;

        Ok(())
    }

    /// 0x11 push global table
    /// push a value than stored in the global table by immediate key onto the stack
    /// we assume that if any failure occurs, such as the key not found, 
    /// we will push a nil value onto the stack for compatibility reasons.
    pub fn push_global_table(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let key = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();

        let top = self.pop()?;
        log::info!("push_global_table: {:x} {:?}", key, &top);
        if let Some(table) = GLOBAL.lock().unwrap().get_mut(key) {
            if let Some(table) = table.as_table() {
                if let Some(table_key) = top.as_int() {
                    if let Some(value) = table.get(table_key as u32) {
                        self.push(value.clone())?;
                    } else {
                        self.push(Variant::Nil)?;
                        log::warn!("key not found in the global table");
                    }
                } else {
                    self.push(Variant::Nil)?;
                    log::warn!("top of the stack is not an integer");
                }
            } else {
                // TODO:
                // Should create a new table for the corresponding key?
                self.push(Variant::Nil)?;
                log::warn!("the value in the global table is not a table");
            }
        } else {
            self.push(Variant::Nil)?;
            log::error!("global table not found: {}", key);
        }
        Ok(())
    }

    /// 0x12 push local table
    /// push a value than stored in the local table by key onto the stack
    pub fn push_local_table(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let idx = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();

        let key = self.pop()?.as_int();

        let mut local = self.get_local(idx)?;
        if let Some(table) = local.as_table() {
            if let Some(table_key) = key {
                if let Some(value) = table.get(table_key as u32) {
                    self.push(value.clone())?;
                } else {
                    self.push(Variant::Nil)?;
                    log::warn!("key not found in the local table");
                }
            } else {
                self.push(Variant::Nil)?;
                log::warn!("key is not an integer");
            }
        } else {
            self.push(Variant::Nil)?;
            log::warn!("local is not a table");
        }
        Ok(())
    }

    /// 0x13 push top
    /// push the top of the stack onto the stack
    pub fn push_top(&mut self) -> Result<()> {
        self.cursor += 1;
        let top = self.top()?;
        self.push(top)?;
        Ok(())
    }

    /// 0x14 push return value
    /// push the return value onto the stack
    pub fn push_return_value(&mut self) -> Result<()> {
        self.cursor += 1;
        self.push(self.return_value.clone())?;
        self.return_value.set_nil();
        Ok(())
    }

    /// 0x15 pop global
    /// pop the top of the stack and store it in the global table
    pub fn pop_global(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let key = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();

        let value = self.pop()?;
        GLOBAL.lock().unwrap().set(key, value);
        Ok(())
    }

    /// 0x16 local copy
    /// copy the top of the stack to the local variable
    pub fn local_copy(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let idx = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();

        let value = self.pop()?;
        log::info!("local_copy: {} {:?}", idx, &value);
        self.set_local(idx, value)?;
        Ok(())
    }

    /// 0x17 pop global table
    /// pop the top of the stack and store it in the global table by key
    pub fn pop_global_table(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let key = parser.read_u16(self.cursor)?;
        self.cursor += size_of::<u16>();

        let value = self.pop()?;
        let mkey = self.pop()?;

        if let Some(table) = GLOBAL.lock().unwrap().get_mut(key) {
            // cast to table if it is not
            if !table.is_table() {
                table.cast_table();
            }

            if let Some(table) = table.as_table() {
                if let Some(mkey) = mkey.as_int() {
                    table.insert(mkey as u32, value);
                } else {
                    log::warn!("top of the stack is not an integer");
                }
            } else {
                log::warn!("the value in the global table is not a table");
            }
        } else {
            log::error!("global table not found: {}", key);
        }
        Ok(())
    }

    /// 0x18 pop local table 
    /// pop the top of the stack and store it in the local table by key
    pub fn pop_local_table(&mut self, parser: &mut Parser) -> Result<()> {
        self.cursor += 1;
        let idx = parser.read_i8(self.cursor)?;
        self.cursor += size_of::<i8>();

        let value = self.pop()?;
        let key = self.pop()?.as_int();

        let local = self.get_local_mut(idx)?;
        if !local.is_table() {
            local.cast_table();
        }
        if let Some(table) = local.as_table() {
            if let Some(table_key) = key {
                table.insert(table_key as u32, value);
            } else {
                log::warn!("key is not an integer");
            }
        } else {
            log::warn!("local is not a table");
        }
        Ok(())
    }

    /// 0x19 neg 
    /// negate the top of the stack, only works for integers and floats
    pub fn neg(&mut self) -> Result<()> {
        self.cursor += 1;
        let mut top = self.pop()?;

        log::info!("neg: {:?}", &top);
        top.neg();
        self.push(top)?;

        Ok(())
    }

    /// 0x1A add
    /// add the top two values on the stack
    pub fn add(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let mut a = self.pop()?;

        log::info!("add: {:?} {:?}", &a, &b);
        a.vadd(&b);
        self.push(a)?;
        Ok(())
    }

    /// 0x1B sub
    /// subtract the top two values on the stack
    pub fn sub(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let mut a = self.pop()?;

        log::info!("sub: {:?} {:?}", &a, &b);
        a.vsub(&b);
        self.push(a)?;
        Ok(())
    }

    /// 0x1C mul
    /// multiply the top two values on the stack
    pub fn mul(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let mut a = self.pop()?;

        log::info!("mul: {:?} {:?}", &a, &b);
        a.vmul(&b);
        self.push(a)?;
        Ok(())
    }

    /// 0x1D div
    /// divide the top two values on the stack
    pub fn div(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let mut a = self.pop()?;

        log::info!("div: {:?} {:?}", &a, &b);
        a.vdiv(&b);
        self.push(a)?;
        Ok(())
    }

    /// 0x1E modulo
    /// modulo the top two values on the stack
    pub fn modulo(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let mut a = self.pop()?;

        log::info!("mod: {:?} {:?}", &a, &b);
        a.vmod(&b);
        self.push(a)?;
        Ok(())
    }

    /// 0x1F bittest
    /// test with the top two values on the stack
    pub fn bittest(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let a = self.pop()?;

        log::info!("bittest: {:?} {:?}", &a, &b);
        if let (Some(a), Some(b)) = (a.as_int(), b.as_int()) {
            self.push(Variant::Int(a & (1 << b)))?;
        } else {
            self.push(Variant::Nil)?;
            log::warn!("bittest only works for integers");
        }
        Ok(())
    }

    /// 0x20 and
    /// push true if both the top two values on the stack are none-nil
    pub fn and(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let mut a = self.pop()?;

        log::info!("and: {:?} {:?}", &a, &b);
        a.and(&b);
        self.push(a)?;
        Ok(())
    }

    /// 0x21 or
    /// push true if either of the top two values on the stack is none-nil
    pub fn or(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let mut a = self.pop()?;

        log::info!("or: {:?} {:?}", &a, &b);
        a.or(&b);
        self.push(a)?;
        Ok(())
    }

    /// 0x22 sete
    /// set the top of the stack to true if the top two values on the stack are equal
    pub fn sete(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let mut a = self.pop()?;

        log::info!("sete: {:?} {:?}", &a, &b);
        a.equal(&b);
        self.push(a)?;
        Ok(())
    }

    /// 0x23 setne
    /// set the top of the stack to true if the top two values on the stack are not equal
    pub fn setne(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let mut a = self.pop()?;

        log::info!("setne: {:?} {:?}", &a, &b);
        a.not_equal(&b);
        self.push(a)?;
        Ok(())
    }

    /// 0x24 setg
    /// set the top of the stack to true if the top two values on the stack are greater
    pub fn setg(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let mut a = self.pop()?;

        log::info!("setg: {:?} {:?}", &a, &b);
        a.greater(&b);
        self.push(a)?;
        Ok(())
    }

    /// 0x25 setle
    /// set the top of the stack to true if the top two values on the stack are less or equal
    pub fn setle(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let mut a = self.pop()?;

        log::info!("setle: {:?} {:?}", &a, &b);
        a.less_equal(&b);
        self.push(a)?;
        Ok(())
    }

    /// 0x26 setl
    /// set the top of the stack to true if the top two values on the stack are less
    pub fn setl(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let mut a = self.pop()?;

        log::info!("setl: {:?} {:?}", &a, &b);
        a.less(&b);
        self.push(a)?;
        Ok(())
    }

    /// 0x27 setge
    /// set the top of the stack to true if the top two values on the stack are greater or equal
    pub fn setge(&mut self) -> Result<()> {
        self.cursor += 1;
        let b = self.pop()?;
        let mut a = self.pop()?;

        log::info!("setge: {:?} {:?}", &a, &b);
        a.greater_equal(&b);
        self.push(a)?;
        Ok(())
    }

    /// get the program counter
    pub fn get_pc(&self) -> usize {
        self.cursor
    }

    /// get waiting time for the context in ms
    pub fn get_waiting_time(&self) -> u64 {
        self.wait_ms
    } 

    /// set waiting time for the context in ms
    pub fn set_waiting_time(&mut self, wait_ms: u64) {
        self.wait_ms = wait_ms;
    }

    pub fn get_status(&self) -> u32 {
        self.state
    }

    pub fn set_status(&mut self, state: u32) {
        self.state = state;
    }

    /// is the main context
    pub fn is_main(&self) -> bool {
        self.id == 0
    }
    
    pub fn set_exited(&mut self) {
        self.should_exit = true;
    }

    pub fn should_exit_now(&self) -> bool {
        self.should_exit
    }

    #[inline]
    pub fn dispatch_opcode(&mut self, syscaller: &mut impl VmSyscall, parser: &mut Parser) -> Result<()> {
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
                self.syscall(syscaller, parser)?;
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

}
